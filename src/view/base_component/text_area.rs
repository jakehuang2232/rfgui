use crate::time::{Duration, Instant};
use crate::ui::MouseButton as UiMouseButton;
use crate::view::font_system::create_font_system;
use crate::view::frame_graph::FrameGraph;
use crate::view::render_pass::draw_rect_pass::{DrawRectInput, DrawRectOutput, RectPassParams};
use crate::view::render_pass::text_pass::{
    TextInput, TextOutput, TextPassFragment, TextPassParams,
};
use crate::view::render_pass::{DrawRectPass, TextPass};
use crate::view::rsx_to_elements_scoped_with_context;
use crate::view::text_layout::{build_text_buffer, measure_buffer_size};
use crate::{
    ColorLike, Cursor as UiCursor, FontFamily, FontSize, HexColor, IntoColor, Layout, Length,
    ParsedValue, PropertyId, Style,
};
use cosmic_text::{
    Affinity, Align, Attrs, Buffer as GlyphBuffer, Cursor, Family, FontSystem, Metrics, Motion,
    Shaping, Wrap,
};
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::{Bound, Range, RangeBounds};

use crate::ui::Binding;
use crate::ui::PropValue;
use crate::ui::RsxNode;
use crate::view::promotion::PromotionNodeInfo;

use super::{
    BoxModelSnapshot, BuildState, Element, ElementTrait, EventTarget, LayoutConstraints,
    LayoutPlacement, Layoutable, Position, Renderable, Size, UiBuildContext, round_layout_value,
};

#[derive(Clone, Debug, PartialEq)]
pub struct TextAreaRenderProjection {
    pub range: Range<usize>,
    pub node: RsxNode,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextAreaRenderString {
    content: String,
    projections: Vec<TextAreaRenderProjection>,
}

impl TextAreaRenderString {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            projections: Vec::new(),
        }
    }

    pub fn content(&self) -> &str {
        self.content.as_str()
    }

    pub fn projections(&self) -> &[TextAreaRenderProjection] {
        self.projections.as_slice()
    }

    pub fn range<R, F>(&mut self, range: R, render: F)
    where
        R: RangeBounds<usize>,
        F: FnOnce(RsxNode) -> RsxNode,
    {
        let Some(range) = clamp_char_range(self.content.as_str(), range) else {
            return;
        };
        let start_byte = byte_index_at_char(self.content.as_str(), range.start);
        let end_byte = byte_index_at_char(self.content.as_str(), range.end);
        let text_area_node = RsxNode::tagged(
            "TextArea",
            crate::ui::RsxTagDescriptor::of::<crate::view::TextArea>(),
        )
        .with_prop("content", self.content[start_byte..end_byte].to_string())
        .with_prop("multiline", false)
        .with_prop("source_text_start", range.start as i64)
        .with_prop("source_text_end", range.end as i64);
        self.projections.push(TextAreaRenderProjection {
            range,
            node: render(text_area_node),
        });
    }
}

#[derive(Clone)]
enum TextAreaRenderFragmentKind {
    Text(String),
    Preedit(String),
    Projection(usize),
}

#[derive(Clone)]
struct TextAreaRenderFragment {
    source_range: Range<usize>,
    kind: TextAreaRenderFragmentKind,
    content_x: f32,
    content_y: f32,
    width: f32,
    height: f32,
    layout_buffer: Option<GlyphBuffer>,
}

struct TextAreaProjectionNode {
    range: Range<usize>,
    node: Box<dyn ElementTrait>,
}

pub struct TextArea {
    element: Element,
    position: Position,
    size: Size,
    layout_position: Position,
    layout_size: Size,
    layout_override_width: Option<f32>,
    layout_override_height: Option<f32>,
    should_render: bool,
    content: String,
    color: Box<dyn ColorLike>,
    selection_background_color: Box<dyn ColorLike>,
    placeholder_color: Box<dyn ColorLike>,
    font_families: Vec<String>,
    font_size: f32,
    line_height: f32,
    opacity: f32,
    style_width: Option<Length>,
    style_height: Option<Length>,
    auto_width: bool,
    auto_height: bool,
    multiline: bool,
    placeholder: String,
    read_only: bool,
    text_binding: Option<Binding<String>>,
    max_length: Option<usize>,
    on_focus_handlers: Vec<crate::ui::TextAreaFocusHandlerProp>,
    on_change_handlers: Vec<crate::ui::TextChangeHandlerProp>,
    on_render_handler: Option<crate::ui::TextAreaRenderHandlerProp>,
    render_nodes: Vec<TextAreaProjectionNode>,
    render_fragments: Vec<TextAreaRenderFragment>,
    render_content_height: f32,
    cursor_char: usize,
    selection_anchor_char: Option<usize>,
    selection_focus_char: Option<usize>,
    mouse_selecting: bool,
    is_focused: bool,
    scroll_y: f32,
    ime_preedit: String,
    ime_preedit_cursor: Option<(usize, usize)>,
    cached_ime_cursor_rect: Option<(f32, f32, f32, f32)>,
    vertical_cursor_x_opt: Option<i32>,
    glyph_buffer: GlyphBuffer,
    glyph_layout_valid: bool,
    glyph_cache_text: String,
    glyph_cache_width: f32,
    glyph_cache_font_size: f32,
    glyph_cache_line_height_px: f32,
    glyph_cache_scale_factor: f32,
    glyph_cache_font_families: Vec<String>,
    measure_revision: u64,
    cached_measure_line_widths: Option<(u64, Vec<f32>)>,
    caret_blink_started_at: Instant,
    dirty_flags: super::DirtyFlags,
    last_layout_placement: Option<LayoutPlacement>,
    source_text_range: Option<Range<usize>>,
}

thread_local! {
    static SHARED_TEXT_AREA_FONT_SYSTEM: RefCell<FontSystem> = RefCell::new(create_font_system());
}

#[derive(Clone, Debug)]
struct VisualLine {
    start_char: usize,
    end_char: usize,
}

#[derive(Clone, Copy, Debug)]
struct TextAreaCursorSnapshot {
    cursor_char: usize,
    selection_anchor_char: Option<usize>,
    selection_focus_char: Option<usize>,
    scroll_y: f32,
    is_focused: bool,
}

impl TextArea {
    fn with_shared_font_system<R>(f: impl FnOnce(&mut FontSystem) -> R) -> R {
        SHARED_TEXT_AREA_FONT_SYSTEM.with(|slot| f(&mut slot.borrow_mut()))
    }

    pub fn from_content(content: impl Into<String>) -> Self {
        let mut text_area = Self::new(0.0, 0.0, 10_000.0, 10_000.0, content);
        text_area.set_auto_width(true);
        text_area.set_auto_height(true);
        text_area
    }

    pub fn from_content_with_id(id: u64, content: impl Into<String>) -> Self {
        let mut text_area = Self::new_with_id(id, 0.0, 0.0, 10_000.0, 10_000.0, content);
        text_area.set_auto_width(true);
        text_area.set_auto_height(true);
        text_area
    }

    pub fn new(x: f32, y: f32, width: f32, height: f32, content: impl Into<String>) -> Self {
        Self::new_with_id(0, x, y, width, height, content)
    }

    pub fn new_with_id(
        id: u64,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        content: impl Into<String>,
    ) -> Self {
        let initial_font_size = 16.0_f32;
        let initial_line_height_ratio = 1.25_f32;
        let initial_line_height_px =
            (initial_font_size * initial_line_height_ratio.max(0.8)).max(1.0);
        let glyph_buffer = Self::with_shared_font_system(|font_system| {
            let mut glyph_buffer = GlyphBuffer::new(
                font_system,
                Metrics::new(initial_font_size.max(1.0), initial_line_height_px),
            );
            glyph_buffer.set_wrap(font_system, Wrap::WordOrGlyph);
            glyph_buffer.set_size(font_system, Some(width.max(1.0)), None);
            glyph_buffer
        });

        let mut text_area = Self {
            element: Element::new_with_id(id, x, y, width, height),
            position: Position { x, y },
            size: Size { width, height },
            layout_position: Position { x, y },
            layout_size: Size {
                width: width.max(0.0),
                height: height.max(0.0),
            },
            layout_override_width: None,
            layout_override_height: None,
            should_render: true,
            content: String::new(),
            color: Box::new(HexColor::new("#111111")),
            selection_background_color: Box::new(crate::Color::rgba(71, 133, 240, 89)),
            placeholder_color: Box::new(HexColor::new("#7d8596")),
            font_families: Vec::new(),
            font_size: 16.0,
            line_height: 1.25,
            opacity: 1.0,
            style_width: None,
            style_height: None,
            auto_width: false,
            auto_height: false,
            multiline: true,
            placeholder: String::new(),
            read_only: false,
            text_binding: None,
            max_length: None,
            on_focus_handlers: Vec::new(),
            on_change_handlers: Vec::new(),
            on_render_handler: None,
            render_nodes: Vec::new(),
            render_fragments: Vec::new(),
            render_content_height: 0.0,
            cursor_char: 0,
            selection_anchor_char: None,
            selection_focus_char: None,
            mouse_selecting: false,
            is_focused: false,
            scroll_y: 0.0,
            ime_preedit: String::new(),
            ime_preedit_cursor: None,
            cached_ime_cursor_rect: None,
            vertical_cursor_x_opt: None,
            glyph_buffer,
            glyph_layout_valid: false,
            glyph_cache_text: String::new(),
            glyph_cache_width: width.max(1.0),
            glyph_cache_font_size: initial_font_size,
            glyph_cache_line_height_px: initial_line_height_px,
            glyph_cache_scale_factor: 1.0,
            glyph_cache_font_families: Vec::new(),
            measure_revision: 0,
            cached_measure_line_widths: None,
            caret_blink_started_at: Instant::now(),
            dirty_flags: super::DirtyFlags::ALL,
            last_layout_placement: None,
            source_text_range: None,
        };
        text_area.set_text(content);
        text_area
    }

    fn mark_measure_dirty(&mut self) {
        self.measure_revision = self.measure_revision.wrapping_add(1);
        let widths = self.measure_content_line_widths();
        self.cached_measure_line_widths = Some((self.measure_revision, widths));
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::ALL);
    }

    fn ensure_measure_line_widths(&mut self) -> &[f32] {
        let rebuild = self
            .cached_measure_line_widths
            .as_ref()
            .map(|(revision, _)| *revision != self.measure_revision)
            .unwrap_or(true);
        if rebuild {
            let widths = self.measure_content_line_widths();
            self.cached_measure_line_widths = Some((self.measure_revision, widths));
        }
        self.cached_measure_line_widths
            .as_ref()
            .map(|(_, widths)| widths.as_slice())
            .unwrap_or(&[])
    }

    fn measure_content_line_widths(&self) -> Vec<f32> {
        let composed = self.composed_text();
        if composed.is_empty() {
            return vec![0.0];
        }

        composed
            .lines()
            .map(|line| {
                Self::measure_render_text_run_with_style(
                    line,
                    self.font_size,
                    self.line_height,
                    &self.font_families,
                )
                .0
            })
            .collect()
    }

    fn reset_caret_blink(&mut self) {
        self.caret_blink_started_at = Instant::now();
    }

    fn should_draw_caret(&self) -> bool {
        if !self.is_focused {
            return false;
        }
        const CARET_BLINK_PERIOD: Duration = Duration::from_millis(1060);
        const CARET_BLINK_VISIBLE: Duration = Duration::from_millis(530);
        let period_ms = CARET_BLINK_PERIOD.as_millis();
        let visible_ms = CARET_BLINK_VISIBLE.as_millis();
        let elapsed_ms = self.caret_blink_started_at.elapsed().as_millis();
        (elapsed_ms % period_ms) < visible_ms
    }

    fn invalidate_glyph_layout(&mut self) {
        self.glyph_layout_valid = false;
        self.cached_ime_cursor_rect = None;
    }

    pub fn set_position(&mut self, x: f32, y: f32) {
        self.position = Position { x, y };
        self.element.set_position(x, y);
        self.cached_ime_cursor_rect = None;
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::RUNTIME);
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        self.size = Size { width, height };
        self.element.set_size(width, height);
        self.layout_override_width = None;
        self.layout_override_height = None;
        self.auto_width = false;
        self.auto_height = false;
        self.invalidate_glyph_layout();
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::ALL);
    }

    pub fn set_width(&mut self, width: f32) {
        self.size.width = width;
        self.element.set_width(width);
        self.layout_override_width = None;
        self.auto_width = false;
        self.invalidate_glyph_layout();
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::ALL);
    }

    pub fn set_height(&mut self, height: f32) {
        self.size.height = height;
        self.element.set_height(height);
        self.layout_override_height = None;
        self.auto_height = false;
        self.cached_ime_cursor_rect = None;
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::ALL);
    }

    pub fn set_text(&mut self, content: impl Into<String>) {
        let mut next = normalize_multiline(content.into(), self.multiline);
        if let Some(max_length) = self.max_length {
            next = truncate_to_chars(&next, max_length);
        }
        if self.content == next {
            return;
        }
        self.content = next;
        self.mark_measure_dirty();
        self.rebuild_render_nodes();
        self.clear_vertical_goal();
        self.reset_caret_blink();
        self.invalidate_glyph_layout();
        self.clamp_cursor();
        self.clamp_scroll();
        self.sync_bound_text();
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::ALL);
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<String>) {
        self.placeholder = placeholder.into();
    }

    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    fn cursor_snapshot(&self) -> TextAreaCursorSnapshot {
        TextAreaCursorSnapshot {
            cursor_char: self.cursor_char,
            selection_anchor_char: self.selection_anchor_char,
            selection_focus_char: self.selection_focus_char,
            scroll_y: self.scroll_y,
            is_focused: self.is_focused,
        }
    }

    fn apply_cursor_snapshot(&mut self, snapshot: TextAreaCursorSnapshot) {
        self.cursor_char = snapshot.cursor_char.min(self.content.chars().count());
        self.selection_anchor_char = snapshot
            .selection_anchor_char
            .map(|idx| idx.min(self.content.chars().count()));
        self.selection_focus_char = snapshot
            .selection_focus_char
            .map(|idx| idx.min(self.content.chars().count()));
        self.scroll_y = snapshot.scroll_y.clamp(0.0, self.max_scroll_y());
        self.is_focused = snapshot.is_focused;
        self.cached_ime_cursor_rect = None;
        self.reset_caret_blink();
    }

    pub fn bind_text(&mut self, binding: Binding<String>) {
        self.text_binding = Some(binding);
        self.sync_bound_text();
    }

    pub fn set_max_length(&mut self, max_length: Option<usize>) {
        self.max_length = max_length;
        if let Some(limit) = max_length {
            let prev = self.content.clone();
            self.content = truncate_to_chars(&self.content, limit);
            self.invalidate_glyph_layout();
            self.clamp_cursor();
            self.clamp_scroll();
            if self.content != prev {
                self.mark_measure_dirty();
                self.sync_bound_text();
                self.rebuild_render_nodes();
            }
        }
    }

    pub fn on_change<F>(&mut self, handler: F)
    where
        F: FnMut(&mut crate::ui::TextChangeEvent) + 'static,
    {
        self.on_change_handlers
            .push(crate::ui::TextChangeHandlerProp::new(handler));
    }

    pub fn on_focus<F>(&mut self, handler: F)
    where
        F: FnMut(&mut crate::ui::TextAreaFocusEvent) + 'static,
    {
        self.on_focus_handlers
            .push(crate::ui::TextAreaFocusHandlerProp::new(handler));
    }

    pub fn on_render<F>(&mut self, handler: F)
    where
        F: FnMut(&mut TextAreaRenderString) + 'static,
    {
        self.on_render_handler = Some(crate::ui::TextAreaRenderHandlerProp::new(handler));
        self.rebuild_render_nodes();
    }

    pub fn set_render_projection_nodes(
        &mut self,
        projections: Vec<(Range<usize>, Box<dyn ElementTrait>)>,
    ) {
        self.on_render_handler = None;
        self.render_nodes = projections
            .into_iter()
            .map(|(range, node)| TextAreaProjectionNode { range, node })
            .collect();
        self.rebuild_render_fragments_from_ranges(
            self.render_nodes
                .iter()
                .map(|projection| projection.range.clone())
                .collect(),
        );
    }

    pub fn source_text_range(&self) -> Option<Range<usize>> {
        self.source_text_range.clone()
    }

    pub fn set_source_text_range(&mut self, range: Option<Range<usize>>) {
        self.source_text_range = range;
    }

    pub fn on_blur<F>(&mut self, handler: F)
    where
        F: FnMut(&mut crate::ui::BlurEvent, &mut crate::view::viewport::ViewportControl<'_>)
            + 'static,
    {
        self.element.on_blur(handler);
    }

    fn notify_change_handlers(&mut self) {
        if self.on_change_handlers.is_empty() {
            return;
        }
        let mut event = crate::ui::TextChangeEvent {
            meta: crate::ui::EventMeta::new(self.id()),
            value: self.content.clone(),
        };
        for handler in &self.on_change_handlers {
            handler.call(&mut event);
        }
    }

    pub fn select_all(&mut self) {
        self.select_range(0, self.content.chars().count());
    }

    pub fn select_range(&mut self, start: usize, end: usize) {
        let len = self.content.chars().count();
        let start = start.min(len);
        let end = end.min(len);
        self.selection_anchor_char = Some(start);
        self.selection_focus_char = Some(end);
        self.cursor_char = end;
        if start == end {
            self.clear_selection();
        }
        self.reset_caret_blink();
        self.cached_ime_cursor_rect = None;
        self.clear_vertical_goal();
        self.ensure_cursor_visible();
    }

    pub fn set_color<T: ColorLike + 'static>(&mut self, color: T) {
        self.color = Box::new(color);
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::PAINT);
    }

    pub fn color_rgba_f32(&self) -> [f32; 4] {
        self.color.to_rgba_f32()
    }

    pub fn set_selection_background_color<T: ColorLike + 'static>(&mut self, color: T) {
        self.selection_background_color = Box::new(color);
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::PAINT);
    }

    pub fn selection_background_rgba_f32(&self) -> [f32; 4] {
        self.selection_background_color.to_rgba_f32()
    }

    pub fn set_font(&mut self, font_family: impl Into<String>) {
        let raw = font_family.into();
        let families: Vec<String> = raw
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        if self.font_families != families {
            self.font_families = families;
            self.mark_measure_dirty();
            self.invalidate_glyph_layout();
        }
    }

    pub fn set_fonts<I, S>(&mut self, font_families: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let next: Vec<String> = font_families
            .into_iter()
            .map(Into::into)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if self.font_families != next {
            self.font_families = next;
            self.mark_measure_dirty();
            self.invalidate_glyph_layout();
        }
    }

    pub fn set_font_size(&mut self, font_size: f32) {
        if (self.font_size - font_size).abs() > f32::EPSILON {
            self.font_size = font_size;
            self.mark_measure_dirty();
            self.invalidate_glyph_layout();
        }
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity;
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::PAINT);
    }

    pub fn set_style_width(&mut self, width: Option<Length>) {
        self.style_width = width;
    }

    pub fn set_style_height(&mut self, height: Option<Length>) {
        self.style_height = height;
    }

    pub fn set_auto_width(&mut self, auto: bool) {
        self.auto_width = auto;
    }

    pub fn set_auto_height(&mut self, auto: bool) {
        self.auto_height = auto;
    }

    pub fn set_cursor(&mut self, cursor: UiCursor) {
        let mut style = Style::new();
        style.set_cursor(cursor);
        self.element.apply_style(style);
    }

    fn sync_size_from_style(
        &mut self,
        percent_base_width: Option<f32>,
        percent_base_height: Option<f32>,
        viewport_width: f32,
        viewport_height: f32,
    ) {
        if let Some(width) = self.style_width {
            if let Some(resolved) =
                width.resolve_with_base(percent_base_width, viewport_width, viewport_height)
            {
                self.size.width = round_layout_value(resolved.max(0.0));
                self.element.set_width(self.size.width);
                self.auto_width = false;
            } else {
                self.auto_width = true;
            }
        } else {
            self.auto_width = true;
        }

        if let Some(height) = self.style_height {
            if let Some(resolved) =
                height.resolve_with_base(percent_base_height, viewport_width, viewport_height)
            {
                self.size.height = round_layout_value(resolved.max(0.0));
                self.element.set_height(self.size.height);
                self.auto_height = false;
            } else {
                self.auto_height = true;
            }
        } else {
            self.auto_height = true;
        }
    }

    pub fn set_multiline(&mut self, multiline: bool) {
        let prev = self.content.clone();
        let prev_multiline = self.multiline;
        self.multiline = multiline;
        self.content = normalize_multiline(self.content.clone(), self.multiline);
        self.clear_vertical_goal();
        self.invalidate_glyph_layout();
        self.clamp_cursor();
        self.clamp_scroll();
        if self.content != prev || self.multiline != prev_multiline {
            self.mark_measure_dirty();
        }
        if self.content != prev {
            self.sync_bound_text();
            self.rebuild_render_nodes();
        }
    }

    fn line_height_px(&self) -> f32 {
        (self.font_size * self.line_height.max(0.1)).max(1.0)
    }

    fn clear_preedit(&mut self) {
        let had_preedit = !self.ime_preedit.is_empty();
        if !had_preedit && self.ime_preedit_cursor.is_none() {
            return;
        }
        self.ime_preedit.clear();
        self.ime_preedit_cursor = None;
        self.invalidate_glyph_layout();
        self.cached_ime_cursor_rect = None;
        self.mark_measure_dirty();
        if had_preedit && !self.render_nodes.is_empty() {
            self.rebuild_render_fragments_from_ranges(
                self.render_nodes
                    .iter()
                    .map(|projection| projection.range.clone())
                    .collect(),
            );
        }
    }

    fn clear_vertical_goal(&mut self) {
        self.vertical_cursor_x_opt = None;
    }

    fn set_preedit(&mut self, text: String, cursor: Option<(usize, usize)>) {
        let next = normalize_multiline(text, self.multiline);
        if self.ime_preedit == next && self.ime_preedit_cursor == cursor {
            return;
        }
        self.ime_preedit = next;
        self.ime_preedit_cursor = cursor;
        self.invalidate_glyph_layout();
        self.cached_ime_cursor_rect = None;
        self.mark_measure_dirty();
        if !self.render_nodes.is_empty() {
            self.rebuild_render_fragments_from_ranges(
                self.render_nodes
                    .iter()
                    .map(|projection| projection.range.clone())
                    .collect(),
            );
        }
    }

    fn effective_width(&self) -> f32 {
        let width = if self.layout_size.width > 0.0 {
            self.layout_size.width
        } else {
            self.size.width
        };
        width.max(1.0)
    }

    fn effective_height(&self) -> f32 {
        let height = if self.layout_size.height > 0.0 {
            self.layout_size.height
        } else {
            self.size.height
        };
        height.max(1.0)
    }

    fn clamp_cursor(&mut self) {
        let len = self.content.chars().count();
        self.cursor_char = self.cursor_char.min(len);
        self.selection_anchor_char = self.selection_anchor_char.map(|idx| idx.min(len));
        self.selection_focus_char = self.selection_focus_char.map(|idx| idx.min(len));
        self.cached_ime_cursor_rect = None;
    }

    fn clear_selection(&mut self) {
        self.selection_anchor_char = None;
        self.selection_focus_char = None;
    }

    fn selection_range_chars(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor_char?;
        let focus = self.selection_focus_char.unwrap_or(anchor);
        if anchor == focus {
            return None;
        }
        Some((anchor.min(focus), anchor.max(focus)))
    }

    fn delete_selected_text(&mut self) -> bool {
        let Some((start, end)) = self.selection_range_chars() else {
            return false;
        };
        let start_byte = byte_index_at_char(&self.content, start);
        let end_byte = byte_index_at_char(&self.content, end);
        self.content.replace_range(start_byte..end_byte, "");
        self.mark_measure_dirty();
        self.rebuild_render_nodes();
        self.cursor_char = start;
        self.clear_selection();
        self.reset_caret_blink();
        self.invalidate_glyph_layout();
        self.clear_vertical_goal();
        self.sync_bound_text();
        true
    }

    fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_range_chars()?;
        let start_byte = byte_index_at_char(&self.content, start);
        let end_byte = byte_index_at_char(&self.content, end);
        Some(self.content[start_byte..end_byte].to_string())
    }

    fn can_insert_chars(&self) -> usize {
        match self.max_length {
            Some(limit) => {
                let current = self.content.chars().count();
                limit.saturating_sub(current)
            }
            None => usize::MAX,
        }
    }

    fn insert_text(&mut self, text: &str) -> bool {
        if self.delete_selected_text() && text.is_empty() {
            return true;
        }
        if text.is_empty() {
            return false;
        }
        let text = normalize_multiline(text.to_string(), self.multiline);
        if text.is_empty() {
            return false;
        }

        let allowed = self.can_insert_chars();
        if allowed == 0 {
            return false;
        }
        let incoming = truncate_to_chars(&text, allowed);
        if incoming.is_empty() {
            return false;
        }

        let insert_at = byte_index_at_char(&self.content, self.cursor_char);
        self.content.insert_str(insert_at, &incoming);
        self.mark_measure_dirty();
        self.rebuild_render_nodes();
        self.cursor_char += incoming.chars().count();
        self.reset_caret_blink();
        self.invalidate_glyph_layout();
        self.clear_vertical_goal();
        self.sync_bound_text();
        true
    }

    fn delete_backspace(&mut self) -> bool {
        if self.delete_selected_text() {
            return true;
        }
        if self.cursor_char == 0 {
            return false;
        }
        let end = byte_index_at_char(&self.content, self.cursor_char);
        let start = byte_index_at_char(&self.content, self.cursor_char - 1);
        self.content.replace_range(start..end, "");
        self.mark_measure_dirty();
        self.rebuild_render_nodes();
        self.cursor_char -= 1;
        self.reset_caret_blink();
        self.invalidate_glyph_layout();
        self.clear_vertical_goal();
        self.sync_bound_text();
        true
    }

    fn delete_forward(&mut self) -> bool {
        if self.delete_selected_text() {
            return true;
        }
        let len = self.content.chars().count();
        if self.cursor_char >= len {
            return false;
        }
        let start = byte_index_at_char(&self.content, self.cursor_char);
        let end = byte_index_at_char(&self.content, self.cursor_char + 1);
        self.content.replace_range(start..end, "");
        self.mark_measure_dirty();
        self.rebuild_render_nodes();
        self.reset_caret_blink();
        self.invalidate_glyph_layout();
        self.clear_vertical_goal();
        self.sync_bound_text();
        true
    }

    fn sync_bound_text(&self) {
        let Some(binding) = self.text_binding.as_ref() else {
            return;
        };
        if binding.get() != self.content {
            binding.set(self.content.clone());
        }
    }

    fn move_cursor_left(&mut self) -> bool {
        if self.cursor_char == 0 {
            return false;
        }
        self.cursor_char -= 1;
        self.reset_caret_blink();
        self.cached_ime_cursor_rect = None;
        self.clear_vertical_goal();
        true
    }

    fn move_cursor_right(&mut self) -> bool {
        let len = self.content.chars().count();
        if self.cursor_char >= len {
            return false;
        }
        self.cursor_char += 1;
        self.reset_caret_blink();
        self.cached_ime_cursor_rect = None;
        self.clear_vertical_goal();
        true
    }

    fn visual_lines(&self) -> Vec<VisualLine> {
        let max_width = self.effective_width();
        let chars: Vec<char> = self.content.chars().collect();
        if chars.is_empty() {
            return vec![VisualLine {
                start_char: 0,
                end_char: 0,
            }];
        }
        if !self.multiline {
            return vec![VisualLine {
                start_char: 0,
                end_char: chars.len(),
            }];
        }

        let mut lines = Vec::new();
        let mut paragraph_start = 0usize;
        for idx in 0..=chars.len() {
            let at_end = idx == chars.len();
            let is_newline = !at_end && chars[idx] == '\n';
            if !at_end && !is_newline {
                continue;
            }
            Self::push_wrapped_lines(
                &mut lines,
                &chars,
                paragraph_start,
                idx,
                max_width,
                self.font_size,
            );
            paragraph_start = idx + 1;
        }

        if lines.is_empty() {
            lines.push(VisualLine {
                start_char: 0,
                end_char: 0,
            });
        }

        lines
    }

    fn push_wrapped_lines(
        out: &mut Vec<VisualLine>,
        chars: &[char],
        start: usize,
        end: usize,
        max_width: f32,
        font_size: f32,
    ) {
        if start >= end {
            out.push(VisualLine {
                start_char: start,
                end_char: start,
            });
            return;
        }

        let mut line_start = start;
        let mut width = 0.0_f32;
        for idx in start..end {
            let char_width = estimate_char_width_px(chars[idx], font_size);
            if idx > line_start && width + char_width > max_width {
                out.push(VisualLine {
                    start_char: line_start,
                    end_char: idx,
                });
                line_start = idx;
                width = 0.0;
            }
            width += char_width;
        }

        out.push(VisualLine {
            start_char: line_start,
            end_char: end,
        });
    }

    fn resolve_cursor_line(&self, lines: &[VisualLine]) -> usize {
        for (idx, line) in lines.iter().enumerate() {
            let is_last = idx + 1 == lines.len();
            let in_empty_line =
                line.start_char == line.end_char && self.cursor_char == line.start_char;
            let in_regular_line =
                self.cursor_char >= line.start_char && self.cursor_char < line.end_char;
            let at_last_line_end = is_last && self.cursor_char == line.end_char;
            if in_empty_line || in_regular_line || at_last_line_end {
                return idx;
            }
        }
        lines.len().saturating_sub(1)
    }

    fn set_cursor_from_local_position(&mut self, local_x: f32, local_y: f32) {
        if self.uses_projection_rendering() {
            let hit_x = local_x.max(0.0);
            let hit_y = (local_y + self.scroll_y).max(0.0);
            for fragment in &self.render_fragments {
                let left = fragment.content_x;
                let top = fragment.content_y;
                let right = left + fragment.width;
                let bottom = top + fragment.height;
                if hit_y < top || hit_y > bottom || hit_x < left || hit_x > right {
                    continue;
                }
                self.cursor_char = if hit_x - left <= fragment.width * 0.5 {
                    fragment.source_range.start
                } else {
                    fragment.source_range.end
                };
                self.reset_caret_blink();
                self.cached_ime_cursor_rect = None;
                self.clear_vertical_goal();
                self.ensure_cursor_visible();
                return;
            }
            self.cursor_char = self.content.chars().count();
            self.reset_caret_blink();
            self.cached_ime_cursor_rect = None;
            self.clear_vertical_goal();
            self.ensure_cursor_visible();
            return;
        }
        let composed = self.composed_text();
        let scale = self.glyph_cache_scale_factor.max(0.0001);
        self.ensure_glyph_layout(composed.as_str(), scale);
        let hit_x = local_x.max(0.0) * scale;
        let hit_y = (local_y + self.scroll_y).max(0.0) * scale;
        if let Some(cursor) = self.glyph_buffer.hit(hit_x, hit_y) {
            let composed_char = self.cursor_char_from_line_index_for_text(
                composed.as_str(),
                cursor.line,
                cursor.index,
            );
            self.cursor_char = self.cursor_char_from_composed(composed_char);
        } else {
            self.cursor_char = self.content.chars().count();
        }
        self.reset_caret_blink();
        self.cached_ime_cursor_rect = None;
        self.clear_vertical_goal();
        self.ensure_cursor_visible();
    }

    fn update_shift_selection_after_move(&mut self, previous_cursor: usize, shift: bool) {
        if !shift {
            self.clear_selection();
            return;
        }
        let anchor = self.selection_anchor_char.unwrap_or(previous_cursor);
        self.selection_anchor_char = Some(anchor);
        self.selection_focus_char = Some(self.cursor_char);
        if self.selection_anchor_char == self.selection_focus_char {
            self.clear_selection();
        }
    }

    fn ensure_cursor_visible(&mut self) {
        let lines = self.visual_lines();
        let line_index = self.resolve_cursor_line(&lines);
        let line_height = self.line_height_px();
        let line_top = (line_index as f32) * line_height;
        let line_bottom = line_top + line_height;
        let view_height = self.effective_height();

        if line_top < self.scroll_y {
            self.scroll_y = line_top;
        } else if line_bottom > self.scroll_y + view_height {
            self.scroll_y = (line_bottom - view_height).max(0.0);
        }

        self.clamp_scroll();
    }

    fn content_height(&self) -> f32 {
        if self.uses_projection_rendering() {
            return self.render_content_height.max(self.line_height_px());
        }
        if let Some((_, widths)) = self.cached_measure_line_widths.as_ref() {
            let effective_width = self.effective_width().max(1.0);
            let line_count = widths.len().max(1);
            let resolved_lines = if self.multiline {
                let wrapped_lines = widths
                    .iter()
                    .map(|line_width| ((*line_width) / effective_width).ceil().max(1.0) as usize)
                    .sum::<usize>();
                wrapped_lines.max(line_count)
            } else {
                1
            };
            return self.line_height_px() * resolved_lines as f32;
        }
        let lines = self.visual_lines();
        self.line_height_px() * lines.len() as f32
    }

    fn max_scroll_y(&self) -> f32 {
        (self.content_height() - self.effective_height()).max(0.0)
    }

    fn clamp_scroll(&mut self) {
        self.scroll_y = self.scroll_y.clamp(0.0, self.max_scroll_y());
        self.cached_ime_cursor_rect = None;
    }

    fn has_projection_rendering(&self) -> bool {
        (!self.render_nodes.is_empty() || self.on_render_handler.is_some())
            && !self.render_fragments.is_empty()
    }

    fn uses_projection_rendering(&self) -> bool {
        self.has_projection_rendering()
    }

    fn preedit_is_inside_projection_range(&self, ranges: &[Range<usize>]) -> bool {
        if self.ime_preedit.is_empty() {
            return false;
        }
        let cursor_char = self.cursor_char.min(self.content.chars().count());
        ranges
            .iter()
            .any(|range| cursor_char >= range.start && cursor_char <= range.end)
    }

    fn render_payload(&self) -> (String, [f32; 4]) {
        if self.has_projection_rendering() {
            return (String::new(), self.color.to_rgba_f32());
        }
        let composed = self.composed_text();
        if composed.is_empty() {
            if !self.placeholder.is_empty() {
                return (
                    self.placeholder.clone(),
                    self.placeholder_color.to_rgba_f32(),
                );
            }
            return (String::new(), self.color.to_rgba_f32());
        }

        (composed, self.color.to_rgba_f32())
    }

    fn fragment_cursor_offset_px(
        &self,
        fragment: &TextAreaRenderFragment,
        cursor_char: usize,
    ) -> f32 {
        let start = fragment.source_range.start;
        let end = fragment.source_range.end;
        if start >= end || fragment.width <= 0.0 {
            return 0.0;
        }

        if let Some(buffer) = fragment.layout_buffer.as_ref() {
            let mut buffer = buffer.clone();
            let local_char = cursor_char.clamp(start, end).saturating_sub(start);
            let text = match &fragment.kind {
                TextAreaRenderFragmentKind::Text(text)
                | TextAreaRenderFragmentKind::Preedit(text) => text.as_str(),
                TextAreaRenderFragmentKind::Projection(_) => "",
            };
            let caret_byte = byte_index_at_char(text, local_char.min(text.chars().count()));
            let (cursor_line, cursor_index) = line_and_index_from_byte(text, caret_byte);
            for affinity in [Affinity::Before, Affinity::After] {
                let cursor = Cursor::new_with_affinity(cursor_line, cursor_index, affinity);
                let Some(layout_cursor) = Self::with_shared_font_system(|font_system| {
                    buffer.layout_cursor(font_system, cursor)
                }) else {
                    continue;
                };
                if layout_cursor.line != cursor_line {
                    continue;
                }
                if let Some(run) = find_layout_run_by_line_layout(
                    &buffer,
                    layout_cursor.line,
                    layout_cursor.layout,
                ) && let Some(x) = caret_x_in_layout_run(cursor_index, &run)
                {
                    let scale = self.glyph_cache_scale_factor.max(0.0001);
                    return x / scale;
                }
            }
        }

        let clamped = cursor_char.clamp(start, end);
        let chars: Vec<char> = self.content.chars().collect();
        let mut offset = 0.0_f32;
        for index in start..clamped.min(chars.len()) {
            offset += estimate_char_width_px(chars[index], self.font_size);
        }

        let fragment_text_width = (start..end.min(chars.len()))
            .map(|index| estimate_char_width_px(chars[index], self.font_size))
            .sum::<f32>()
            .max(0.0001);
        let scale = if fragment.width > 0.0 {
            fragment.width / fragment_text_width
        } else {
            1.0
        };
        (offset * scale).clamp(0.0, fragment.width)
    }

    fn fragment_cursor_screen_x(
        &self,
        fragment: &TextAreaRenderFragment,
        cursor_char: usize,
    ) -> f32 {
        self.layout_position.x
            + fragment.content_x
            + self.fragment_cursor_offset_px(fragment, cursor_char)
    }

    fn preedit_fragment_caret_screen_position(
        &self,
        fragment: &TextAreaRenderFragment,
        text: &str,
    ) -> Option<(f32, f32)> {
        let caret_chars = match self.ime_preedit_cursor {
            Some((_, end)) => text[..clamp_utf8_boundary(text, end)].chars().count(),
            None => text.chars().count(),
        };
        let total_chars = text.chars().count().max(1);
        let offset = if total_chars == 0 {
            0.0
        } else {
            fragment.width * (caret_chars as f32 / total_chars as f32)
        };
        Some((
            self.layout_position.x + fragment.content_x + offset.clamp(0.0, fragment.width),
            self.layout_position.y + fragment.content_y - self.scroll_y,
        ))
    }

    fn projection_fragment_caret_screen_position(
        &mut self,
        node_index: usize,
        fragment: &TextAreaRenderFragment,
        cursor_char: usize,
    ) -> Option<(f32, f32)> {
        let node = self.render_nodes.get_mut(node_index)?;
        let nested = find_first_text_area_mut(node.node.as_mut())?;
        let local_char = cursor_char
            .clamp(fragment.source_range.start, fragment.source_range.end)
            .saturating_sub(fragment.source_range.start)
            .min(nested.content.chars().count());
        nested.caret_screen_position_for_char(local_char, false)
    }

    fn place_projection_fragments(&mut self, viewport_width: f32, viewport_height: f32) {
        for fragment in &self.render_fragments {
            let TextAreaRenderFragmentKind::Projection(index) = fragment.kind else {
                continue;
            };
            let Some(node) = self.render_nodes.get_mut(index) else {
                continue;
            };
            let screen_x = self.layout_position.x + fragment.content_x;
            let screen_y = self.layout_position.y + fragment.content_y - self.scroll_y;
            node.node.place(LayoutPlacement {
                parent_x: screen_x,
                parent_y: screen_y,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: fragment.width.max(1.0),
                available_height: fragment.height.max(1.0),
                viewport_width: viewport_width.max(1.0),
                viewport_height: viewport_height.max(1.0),
                percent_base_width: Some(fragment.width.max(1.0)),
                percent_base_height: Some(fragment.height.max(1.0)),
            });
        }
    }

    fn projection_fragment_cursor_char_from_viewport_position(
        &mut self,
        node_index: usize,
        fragment: &TextAreaRenderFragment,
        viewport_x: f32,
        viewport_y: f32,
    ) -> Option<usize> {
        let node = self.render_nodes.get_mut(node_index)?;
        let nested = find_first_text_area_mut(node.node.as_mut())?;
        let local_x = viewport_x - nested.layout_position.x;
        let local_y = viewport_y - nested.layout_position.y;
        nested.set_cursor_from_local_position(local_x, local_y);
        let local_char = nested.cursor_char.min(nested.content.chars().count());
        Some(
            fragment.source_range.start
                + local_char.min(
                    fragment
                        .source_range
                        .end
                        .saturating_sub(fragment.source_range.start),
                ),
        )
    }

    fn caret_screen_position_for_char(
        &mut self,
        cursor_char: usize,
        require_focus: bool,
    ) -> Option<(f32, f32)> {
        if require_focus && !self.is_focused {
            return None;
        }
        if self.uses_projection_rendering() {
            for fragment_index in 0..self.render_fragments.len() {
                let fragment = self.render_fragments[fragment_index].clone();
                if let TextAreaRenderFragmentKind::Preedit(text) = &fragment.kind
                    && cursor_char == fragment.source_range.start
                {
                    return self.preedit_fragment_caret_screen_position(&fragment, text.as_str());
                }
                if cursor_char <= fragment.source_range.start {
                    return Some((
                        self.layout_position.x + fragment.content_x,
                        self.layout_position.y + fragment.content_y - self.scroll_y,
                    ));
                }
                if cursor_char <= fragment.source_range.end {
                    if let TextAreaRenderFragmentKind::Projection(index) = fragment.kind.clone()
                        && let Some(position) = self.projection_fragment_caret_screen_position(
                            index,
                            &fragment,
                            cursor_char,
                        )
                    {
                        return Some(position);
                    }
                    return Some((
                        self.fragment_cursor_screen_x(&fragment, cursor_char),
                        self.layout_position.y + fragment.content_y - self.scroll_y,
                    ));
                }
            }
            if let Some(last) = self.render_fragments.last() {
                return Some((
                    self.layout_position.x + last.content_x + last.width,
                    self.layout_position.y + last.content_y - self.scroll_y,
                ));
            }
        }
        let composed = self.composed_text();
        let caret_byte = self.caret_byte_in_composed_for_char(composed.as_str(), cursor_char)?;
        let scale = self.glyph_cache_scale_factor.max(0.0001);
        self.ensure_glyph_layout(composed.as_str(), scale);
        let (cursor_line, cursor_index) = line_and_index_from_byte(composed.as_str(), caret_byte);
        for affinity in [Affinity::Before, Affinity::After] {
            let cursor = Cursor::new_with_affinity(cursor_line, cursor_index, affinity);
            let Some(layout_cursor) = Self::with_shared_font_system(|font_system| {
                self.glyph_buffer.layout_cursor(font_system, cursor)
            }) else {
                continue;
            };
            if layout_cursor.line != cursor_line {
                continue;
            }
            if let Some(run) = find_layout_run_by_line_layout(
                &self.glyph_buffer,
                layout_cursor.line,
                layout_cursor.layout,
            ) {
                if let Some(x) = caret_x_in_layout_run(cursor_index, &run) {
                    return Some((
                        self.layout_position.x + x / scale,
                        self.layout_position.y + run.line_top / scale - self.scroll_y,
                    ));
                }
            }
        }

        let fallback_y =
            fallback_line_top_for_cursor_line(&self.glyph_buffer, cursor_line).unwrap_or(0.0);
        Some((
            self.layout_position.x,
            self.layout_position.y + fallback_y / scale - self.scroll_y,
        ))
    }

    fn caret_screen_position(&mut self) -> Option<(f32, f32)> {
        self.caret_screen_position_for_char(self.cursor_char, true)
    }

    fn screen_rects_for_char_range(
        &mut self,
        composed: &str,
        start_char: usize,
        end_char: usize,
    ) -> Vec<([f32; 2], [f32; 2])> {
        if composed.is_empty() {
            return Vec::new();
        }

        let start_byte = byte_index_at_char(composed, start_char);
        let end_byte = byte_index_at_char(composed, end_char);
        if start_byte >= end_byte {
            return Vec::new();
        }

        let (start_line, start_index) = line_and_index_from_byte(composed, start_byte);
        let (end_line, end_index) = line_and_index_from_byte(composed, end_byte);
        let line_lengths = line_lengths_bytes(composed);
        let scale = self.glyph_cache_scale_factor.max(0.0001);
        self.ensure_glyph_layout(composed, scale);

        let mut rects = Vec::new();
        for run in self.glyph_buffer.layout_runs() {
            if run.line_i < start_line || run.line_i > end_line {
                continue;
            }
            let line_len = *line_lengths.get(run.line_i).unwrap_or(&0);
            let local_start = if run.line_i == start_line {
                start_index
            } else {
                0
            };
            let local_end = if run.line_i == end_line {
                end_index
            } else {
                line_len
            };
            if local_end <= local_start {
                continue;
            }
            let Some(x0) = caret_x_in_layout_run(local_start.min(line_len), &run) else {
                continue;
            };
            let Some(x1) = caret_x_in_layout_run(local_end.min(line_len), &run) else {
                continue;
            };
            let left = x0.min(x1);
            let width = (x1 - x0).abs();
            if width <= 0.01 {
                continue;
            }
            rects.push((
                [
                    self.layout_position.x + left / scale,
                    self.layout_position.y + run.line_top / scale - self.scroll_y,
                ],
                [width / scale, (run.line_height / scale).max(1.0)],
            ));
        }
        rects
    }

    fn selection_screen_rects(&mut self, composed: &str) -> Vec<([f32; 2], [f32; 2])> {
        let Some((start_char, end_char)) = self.selection_range_chars() else {
            return Vec::new();
        };
        if self.uses_projection_rendering() {
            let mut rects = Vec::new();
            for fragment_index in 0..self.render_fragments.len() {
                let fragment = self.render_fragments[fragment_index].clone();
                if fragment.source_range.end <= start_char
                    || fragment.source_range.start >= end_char
                {
                    continue;
                }
                let overlap_start = start_char.max(fragment.source_range.start);
                let overlap_end = end_char.min(fragment.source_range.end);
                if matches!(fragment.kind, TextAreaRenderFragmentKind::Projection(_)) {
                    continue;
                }
                let left = self.fragment_cursor_screen_x(&fragment, overlap_start);
                let right = self.fragment_cursor_screen_x(&fragment, overlap_end);
                rects.push((
                    [
                        left,
                        self.layout_position.y + fragment.content_y - self.scroll_y,
                    ],
                    [(right - left).abs().max(1.0), fragment.height.max(1.0)],
                ));
            }
            return rects;
        }
        self.screen_rects_for_char_range(composed, start_char, end_char)
    }

    fn rebuild_render_nodes(&mut self) {
        let Some(handler) = self.on_render_handler.clone() else {
            self.rebuild_render_fragments_from_ranges(
                self.render_nodes
                    .iter()
                    .map(|projection| projection.range.clone())
                    .collect(),
            );
            return;
        };

        let mut render_string = TextAreaRenderString::new(self.content.clone());
        handler.call(&mut render_string);

        let projections = normalize_text_area_render_projections(
            self.content.as_str(),
            render_string.projections(),
        );
        let mut next_nodes = Vec::with_capacity(projections.len());
        let mut next_fragments = Vec::new();

        let mut cursor = 0_usize;
        for (index, projection) in projections.iter().enumerate() {
            let mut root = match self.build_projection_root(index, &projection.node) {
                Ok(root) => root,
                Err(_error) => continue,
            };
            root.set_parent_id(Some(self.id()));
            apply_text_source_range(root.as_mut(), projection.range.clone());
            next_nodes.push(root);
        }
        for (projection_index, projection) in projections.iter().enumerate() {
            if cursor < projection.range.start {
                append_plain_render_fragments(
                    &mut next_fragments,
                    self.content.as_str(),
                    cursor,
                    projection.range.start,
                );
            }
            next_fragments.push(TextAreaRenderFragment {
                source_range: projection.range.clone(),
                kind: TextAreaRenderFragmentKind::Projection(projection_index),
                content_x: 0.0,
                content_y: 0.0,
                width: 0.0,
                height: 0.0,
                layout_buffer: None,
            });
            cursor = projection.range.end;
        }
        if cursor < self.content.chars().count() {
            append_plain_render_fragments(
                &mut next_fragments,
                self.content.as_str(),
                cursor,
                self.content.chars().count(),
            );
        }

        self.render_nodes = next_nodes
            .into_iter()
            .zip(projections.iter())
            .map(|(node, projection)| TextAreaProjectionNode {
                range: projection.range.clone(),
                node,
            })
            .collect();
        self.render_fragments = next_fragments;
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::ALL);
    }

    fn rebuild_render_fragments_from_ranges(&mut self, mut projection_ranges: Vec<Range<usize>>) {
        let mut next_fragments = Vec::new();
        projection_ranges.sort_by_key(|range| range.start);
        let preedit_cursor = self.cursor_char.min(self.content.chars().count());
        let render_preedit_as_fragment = !self.ime_preedit.is_empty()
            && !self.preedit_is_inside_projection_range(&projection_ranges);
        let mut preedit_inserted = false;
        let mut cursor = 0_usize;
        for projection in projection_ranges {
            if render_preedit_as_fragment && !preedit_inserted && preedit_cursor <= projection.start
            {
                if cursor < preedit_cursor {
                    append_plain_render_fragments(
                        &mut next_fragments,
                        self.content.as_str(),
                        cursor,
                        preedit_cursor,
                    );
                }
                append_preedit_render_fragment(
                    &mut next_fragments,
                    preedit_cursor,
                    self.ime_preedit.as_str(),
                );
                cursor = preedit_cursor;
                preedit_inserted = true;
            }
            if cursor < projection.start {
                append_plain_render_fragments(
                    &mut next_fragments,
                    self.content.as_str(),
                    cursor,
                    projection.start,
                );
            }
            if let Some(node_index) = self
                .render_nodes
                .iter()
                .position(|candidate| candidate.range == projection)
            {
                next_fragments.push(TextAreaRenderFragment {
                    source_range: projection.clone(),
                    kind: TextAreaRenderFragmentKind::Projection(node_index),
                    content_x: 0.0,
                    content_y: 0.0,
                    width: 0.0,
                    height: 0.0,
                    layout_buffer: None,
                });
            }
            cursor = projection.end;
        }
        if render_preedit_as_fragment && !preedit_inserted {
            if cursor < preedit_cursor {
                append_plain_render_fragments(
                    &mut next_fragments,
                    self.content.as_str(),
                    cursor,
                    preedit_cursor,
                );
            }
            append_preedit_render_fragment(
                &mut next_fragments,
                preedit_cursor,
                self.ime_preedit.as_str(),
            );
            cursor = preedit_cursor;
        }
        if cursor < self.content.chars().count() {
            append_plain_render_fragments(
                &mut next_fragments,
                self.content.as_str(),
                cursor,
                self.content.chars().count(),
            );
        }
        self.render_fragments = next_fragments;
        self.render_content_height = 0.0;
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::ALL);
    }

    fn projection_inherited_style(&self) -> Style {
        let mut style = Style::new();
        if !self.font_families.is_empty() {
            style.insert(
                PropertyId::FontFamily,
                ParsedValue::FontFamily(FontFamily::new(self.font_families.clone())),
            );
        }
        style.insert(
            PropertyId::FontSize,
            ParsedValue::FontSize(FontSize::px(self.font_size)),
        );
        style.insert(
            PropertyId::Color,
            ParsedValue::Color(self.color.clone().into_color().into()),
        );
        style
    }

    fn build_projection_root(
        &self,
        index: usize,
        node: &RsxNode,
    ) -> Result<Box<dyn ElementTrait>, String> {
        let scope = [self.id(), 0x5445_5854, index as u64];
        let inherited_style = self.projection_inherited_style();
        let mut children =
            rsx_to_elements_scoped_with_context(node, &scope, &inherited_style, 0.0, 0.0)?;
        self.wrap_projection_children(index, &mut children)
    }

    fn wrap_projection_children(
        &self,
        index: usize,
        children: &mut Vec<Box<dyn ElementTrait>>,
    ) -> Result<Box<dyn ElementTrait>, String> {
        if children.is_empty() {
            return Err("projection produced no elements".to_string());
        }
        if children.len() == 1 {
            return Ok(children.remove(0));
        }

        let wrapper_id = self
            .id()
            .wrapping_mul(1_000_003)
            .wrapping_add(index as u64 + 1);
        let mut wrapper = Element::new_with_id(wrapper_id, 0.0, 0.0, 0.0, 0.0);
        wrapper.set_intrinsic_size_as_percent_base(false);

        let mut style = Style::new();
        style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().row().no_wrap().into()),
        );
        style.insert(PropertyId::Width, ParsedValue::Auto);
        style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper.apply_style(style);

        for child in children.drain(..) {
            wrapper.add_child(child);
        }
        Ok(Box::new(wrapper))
    }

    fn sync_projection_preedit_state(&mut self) {
        let cursor_char = self.cursor_char.min(self.content.chars().count());
        for projection in &mut self.render_nodes {
            let Some(nested) = find_first_text_area_mut(projection.node.as_mut()) else {
                continue;
            };
            let local_cursor = cursor_char
                .saturating_sub(projection.range.start)
                .min(nested.content.chars().count());
            nested.cursor_char = local_cursor;
            if !self.ime_preedit.is_empty()
                && cursor_char >= projection.range.start
                && cursor_char <= projection.range.end
            {
                nested.set_preedit(self.ime_preedit.clone(), self.ime_preedit_cursor);
            } else {
                nested.clear_preedit();
            }
        }
    }

    fn sync_projection_selection_state(&mut self) {
        let selection = self.selection_range_chars();
        let selection_color = self.selection_background_color.to_rgba_f32();
        for projection in &mut self.render_nodes {
            let Some(nested) = find_first_text_area_mut(projection.node.as_mut()) else {
                continue;
            };
            nested.set_selection_background_color(crate::Color::rgba(
                (selection_color[0] * 255.0).round().clamp(0.0, 255.0) as u8,
                (selection_color[1] * 255.0).round().clamp(0.0, 255.0) as u8,
                (selection_color[2] * 255.0).round().clamp(0.0, 255.0) as u8,
                (selection_color[3] * 255.0).round().clamp(0.0, 255.0) as u8,
            ));
            let Some((start, end)) = selection else {
                nested.clear_selection();
                continue;
            };
            let overlap_start = start.max(projection.range.start);
            let overlap_end = end.min(projection.range.end);
            if overlap_start >= overlap_end {
                nested.clear_selection();
                continue;
            }
            nested.selection_anchor_char =
                Some(overlap_start.saturating_sub(projection.range.start));
            nested.selection_focus_char = Some(overlap_end.saturating_sub(projection.range.start));
        }
    }

    fn layout_render_fragments(&mut self, viewport_width: f32, viewport_height: f32) {
        if self.render_fragments.is_empty() {
            self.render_content_height = 0.0;
            return;
        }
        self.sync_projection_preedit_state();
        self.sync_projection_selection_state();
        let available_width = self.effective_width().max(1.0);
        let line_height_px = self.line_height_px();
        let effective_height = self.effective_height().max(line_height_px);
        let font_size = self.font_size;
        let line_height_ratio = self.line_height;
        let font_families = self.font_families.clone();
        let scale = self.glyph_cache_scale_factor.max(0.0001);
        let mut cursor_x = 0.0_f32;
        let mut cursor_y = 0.0_f32;
        let mut line_height = line_height_px;

        for fragment in &mut self.render_fragments {
            let (fragment_width, fragment_height) = match &fragment.kind {
                TextAreaRenderFragmentKind::Text(text)
                | TextAreaRenderFragmentKind::Preedit(text) => {
                    let buffer = Self::build_render_text_buffer_with_style(
                        text.as_str(),
                        font_size,
                        line_height_ratio,
                        &font_families,
                        scale,
                    );
                    let measured = measure_buffer_size(&buffer);
                    fragment.layout_buffer = Some(buffer);
                    (measured.0 / scale, measured.1 / scale)
                }
                TextAreaRenderFragmentKind::Projection(index) => {
                    fragment.layout_buffer = None;
                    let Some(node) = self.render_nodes.get_mut(*index) else {
                        continue;
                    };
                    node.node.measure(LayoutConstraints {
                        max_width: available_width,
                        max_height: effective_height,
                        viewport_width,
                        viewport_height,
                        percent_base_width: Some(available_width),
                        percent_base_height: Some(effective_height),
                    });
                    let (measured_width, measured_height) = node.node.measured_size();
                    (measured_width.max(1.0), measured_height.max(line_height_px))
                }
            };

            if self.multiline && cursor_x > 0.0 && cursor_x + fragment_width > available_width {
                cursor_x = 0.0;
                cursor_y += line_height;
                line_height = line_height_px;
            }

            fragment.content_x = round_layout_value(cursor_x);
            fragment.content_y = round_layout_value(cursor_y);
            fragment.width = round_layout_value(fragment_width.max(1.0));
            fragment.height = round_layout_value(fragment_height.max(line_height_px));
            cursor_x += fragment_width;
            line_height = line_height.max(fragment_height);
        }

        self.render_content_height = (cursor_y + line_height).max(line_height_px);
    }

    fn set_cursor_from_projection_position(&mut self, viewport_x: f32, viewport_y: f32) -> bool {
        for fragment_index in 0..self.render_fragments.len() {
            let fragment = self.render_fragments[fragment_index].clone();
            let left = self.layout_position.x + fragment.content_x;
            let top = self.layout_position.y + fragment.content_y - self.scroll_y;
            let right = left + fragment.width;
            let bottom = top + fragment.height;
            if viewport_x < left || viewport_x > right || viewport_y < top || viewport_y > bottom {
                continue;
            }
            if let TextAreaRenderFragmentKind::Projection(index) = fragment.kind
                && let Some(cursor_char) = self
                    .projection_fragment_cursor_char_from_viewport_position(
                        index, &fragment, viewport_x, viewport_y,
                    )
            {
                self.cursor_char = cursor_char;
                self.cached_ime_cursor_rect = None;
                self.reset_caret_blink();
                self.clear_vertical_goal();
                return true;
            }
            let mut best_char = fragment.source_range.start;
            let mut best_distance = f32::INFINITY;
            for candidate in fragment.source_range.start..=fragment.source_range.end {
                let caret_x = self.fragment_cursor_screen_x(&fragment, candidate);
                let distance = (viewport_x - caret_x).abs();
                if distance < best_distance {
                    best_distance = distance;
                    best_char = candidate;
                }
            }
            self.cursor_char = best_char;
            self.cached_ime_cursor_rect = None;
            self.reset_caret_blink();
            self.clear_vertical_goal();
            return true;
        }
        false
    }

    fn ime_preedit_range_chars(&self) -> Option<(usize, usize)> {
        if self.ime_preedit.is_empty() {
            return None;
        }
        let start = self.cursor_char;
        let end = start + self.ime_preedit.chars().count();
        Some((start, end))
    }

    fn ime_preedit_underline_rects(&mut self, composed: &str) -> Vec<([f32; 2], [f32; 2])> {
        let Some((start_char, end_char)) = self.ime_preedit_range_chars() else {
            return Vec::new();
        };
        if self.uses_projection_rendering() {
            let mut rects = Vec::new();
            for fragment_index in 0..self.render_fragments.len() {
                let fragment = self.render_fragments[fragment_index].clone();
                match fragment.kind {
                    TextAreaRenderFragmentKind::Preedit(_) => {
                        if fragment.width > 0.01 && fragment.height > 0.01 {
                            rects.push((
                                [
                                    self.layout_position.x + fragment.content_x,
                                    self.layout_position.y + fragment.content_y - self.scroll_y
                                        + fragment.height
                                        - 1.0,
                                ],
                                [fragment.width, 1.0],
                            ));
                        }
                    }
                    TextAreaRenderFragmentKind::Projection(index) => {
                        if start_char < fragment.source_range.start
                            || start_char > fragment.source_range.end
                        {
                            continue;
                        }
                        let Some(node) = self.render_nodes.get_mut(index) else {
                            continue;
                        };
                        let Some(nested) = find_first_text_area_mut(node.node.as_mut()) else {
                            continue;
                        };
                        let nested_composed = nested.composed_text();
                        rects.extend(nested.ime_preedit_underline_rects(nested_composed.as_str()));
                    }
                    TextAreaRenderFragmentKind::Text(_) => {}
                }
            }
            return rects;
        }
        let line_rects = self.screen_rects_for_char_range(composed, start_char, end_char);
        line_rects
            .into_iter()
            .filter_map(|(position, size)| {
                if size[0] <= 0.01 || size[1] <= 0.01 {
                    return None;
                }
                Some(([position[0], position[1] + size[1] - 1.0], [size[0], 1.0]))
            })
            .collect()
    }

    fn ensure_glyph_layout(&mut self, text: &str, scale_factor: f32) {
        let scale = scale_factor.max(0.0001);
        let width = self.effective_width() * scale;
        let font_size = self.font_size.max(1.0) * scale;
        let line_height_px = (self.font_size * self.line_height.max(0.8)).max(1.0) * scale;
        let needs_rebuild = !self.glyph_layout_valid
            || self.glyph_cache_text != text
            || (self.glyph_cache_width - width).abs() > 0.01
            || (self.glyph_cache_font_size - font_size).abs() > 0.01
            || (self.glyph_cache_line_height_px - line_height_px).abs() > 0.01
            || (self.glyph_cache_scale_factor - scale).abs() > 0.0001
            || self.glyph_cache_font_families != self.font_families;
        if !needs_rebuild {
            return;
        }

        Self::with_shared_font_system(|font_system| {
            self.glyph_buffer =
                GlyphBuffer::new(font_system, Metrics::new(font_size, line_height_px));
            self.glyph_buffer.set_wrap(font_system, Wrap::WordOrGlyph);
            self.glyph_buffer.set_size(font_system, Some(width), None);
            let attrs = if let Some(first) = self.font_families.first() {
                Attrs::new().family(Family::Name(first.as_str()))
            } else {
                Attrs::new()
            };
            self.glyph_buffer.set_text(
                font_system,
                text,
                &attrs,
                Shaping::Advanced,
                Some(cosmic_text::Align::Left),
            );
            self.glyph_buffer.shape_until_scroll(font_system, false);
        });

        self.glyph_layout_valid = true;
        self.glyph_cache_text = text.to_string();
        self.glyph_cache_width = width;
        self.glyph_cache_font_size = font_size;
        self.glyph_cache_line_height_px = line_height_px;
        self.glyph_cache_scale_factor = scale;
        self.glyph_cache_font_families = self.font_families.clone();
    }

    fn measure_render_text_run_with_style(
        text: &str,
        font_size: f32,
        line_height: f32,
        font_families: &[String],
    ) -> (f32, f32) {
        Self::with_shared_font_system(|font_system| {
            let buffer = build_text_buffer(
                font_system,
                text,
                None,
                None,
                false,
                font_size,
                line_height,
                400,
                Align::Left,
                font_families,
            );
            measure_buffer_size(&buffer)
        })
    }

    fn build_render_text_buffer_with_style(
        text: &str,
        font_size: f32,
        line_height: f32,
        font_families: &[String],
        scale: f32,
    ) -> GlyphBuffer {
        Self::with_shared_font_system(|font_system| {
            build_text_buffer(
                font_system,
                text,
                None,
                None,
                false,
                font_size * scale,
                line_height,
                400,
                Align::Left,
                font_families,
            )
        })
    }

    fn composed_text(&self) -> String {
        if self.ime_preedit.is_empty() {
            return self.content.clone();
        }
        let insert_at = byte_index_at_char(&self.content, self.cursor_char);
        let mut out = String::with_capacity(self.content.len() + self.ime_preedit.len());
        out.push_str(&self.content[..insert_at]);
        out.push_str(&self.ime_preedit);
        out.push_str(&self.content[insert_at..]);
        out
    }

    fn cursor_char_from_line_index_for_text(&self, text: &str, line: usize, index: usize) -> usize {
        let mut byte_offset = 0usize;
        for (current_line, segment) in text.split('\n').enumerate() {
            let segment_bytes = segment.len();
            if current_line == line {
                let target_byte = byte_offset + index.min(segment_bytes);
                return text[..target_byte].chars().count();
            }

            byte_offset += segment_bytes;
            if byte_offset < text.len() {
                byte_offset += 1;
            }
        }

        text.chars().count()
    }

    fn cursor_char_from_composed(&self, composed_char: usize) -> usize {
        if self.ime_preedit.is_empty() {
            return composed_char.min(self.content.chars().count());
        }
        let insert_char = self.cursor_char;
        let preedit_chars = self.ime_preedit.chars().count();
        if composed_char <= insert_char {
            return composed_char;
        }
        if composed_char >= insert_char + preedit_chars {
            return composed_char
                .saturating_sub(preedit_chars)
                .min(self.content.chars().count());
        }
        insert_char
    }

    fn caret_byte_in_composed_for_char(&self, composed: &str, cursor_char: usize) -> Option<usize> {
        let base = byte_index_at_char(&self.content, cursor_char.min(self.content.chars().count()));
        if self.ime_preedit.is_empty() {
            return Some(base.min(composed.len()));
        }
        let preedit = self.ime_preedit.as_str();
        let caret_in_preedit = match self.ime_preedit_cursor {
            Some((_, end)) => clamp_utf8_boundary(preedit, end),
            None => return None,
        };
        Some((base + caret_in_preedit).min(composed.len()))
    }

    fn handle_key_down(
        &mut self,
        event: &crate::ui::KeyDownEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) -> bool {
        let key = event.key.key.as_str();
        let code = event.key.code.as_str();
        let modifiers = event.key.modifiers;
        let shift = modifiers.shift;
        let shortcut = modifiers.ctrl || modifiers.meta;

        if key_matches(key, code, "ArrowLeft") {
            if !shift {
                if let Some((start, _)) = self.selection_range_chars() {
                    self.cursor_char = start;
                    self.clear_selection();
                    return true;
                }
            }
            let previous = self.cursor_char;
            let moved = self.move_cursor_left();
            if moved {
                self.update_shift_selection_after_move(previous, shift);
            }
            return moved;
        }
        if key_matches(key, code, "ArrowRight") {
            if !shift {
                if let Some((_, end)) = self.selection_range_chars() {
                    self.cursor_char = end;
                    self.clear_selection();
                    return true;
                }
            }
            let previous = self.cursor_char;
            let moved = self.move_cursor_right();
            if moved {
                self.update_shift_selection_after_move(previous, shift);
            }
            return moved;
        }
        if key_matches(key, code, "ArrowUp") {
            let previous = self.cursor_char;
            let moved = self.move_cursor_vertical(Motion::Up);
            if moved {
                self.update_shift_selection_after_move(previous, shift);
            }
            return moved;
        }
        if key_matches(key, code, "ArrowDown") {
            let previous = self.cursor_char;
            let moved = self.move_cursor_vertical(Motion::Down);
            if moved {
                self.update_shift_selection_after_move(previous, shift);
            }
            return moved;
        }
        self.clear_vertical_goal();
        if key_matches(key, code, "Home") {
            if self.cursor_char == 0 {
                return false;
            }
            let previous = self.cursor_char;
            self.cursor_char = 0;
            self.update_shift_selection_after_move(previous, shift);
            return true;
        }
        if key_matches(key, code, "End") {
            let end = self.content.chars().count();
            if self.cursor_char == end {
                return false;
            }
            let previous = self.cursor_char;
            self.cursor_char = end;
            self.update_shift_selection_after_move(previous, shift);
            return true;
        }
        if shortcut && key.eq_ignore_ascii_case("a") {
            let end = self.content.chars().count();
            self.selection_anchor_char = Some(0);
            self.selection_focus_char = Some(end);
            self.cursor_char = end;
            self.reset_caret_blink();
            self.clear_vertical_goal();
            return true;
        }
        if shortcut && key.eq_ignore_ascii_case("c") {
            if let Some(selected) = self.selected_text() {
                control.set_clipboard_text(selected);
                return true;
            }
            return false;
        }
        if shortcut && key.eq_ignore_ascii_case("x") {
            if self.read_only {
                return false;
            }
            if let Some(selected) = self.selected_text() {
                control.set_clipboard_text(selected);
                return self.delete_selected_text();
            }
            return false;
        }
        if shortcut && key.eq_ignore_ascii_case("v") {
            if self.read_only {
                return false;
            }
            let Some(text) = control.clipboard_text() else {
                return false;
            };
            return self.insert_text(text.as_str());
        }

        if self.read_only {
            return false;
        }

        if key_matches(key, code, "Backspace") {
            return self.delete_backspace();
        }
        if key_matches(key, code, "Delete") {
            return self.delete_forward();
        }
        if key_matches(key, code, "Enter") {
            if self.multiline {
                return self.insert_text("\n");
            }
            return false;
        }
        if key_matches(key, code, "Tab") {
            return self.insert_text("    ");
        }

        if shortcut || modifiers.alt {
            return false;
        }

        false
    }

    fn move_cursor_vertical(&mut self, motion: Motion) -> bool {
        if self.uses_projection_rendering() {
            return false;
        }
        if !self.multiline {
            return false;
        }

        let text = self.content.clone();
        let caret_byte = byte_index_at_char(text.as_str(), self.cursor_char);
        let (line, index) = line_and_index_from_byte(text.as_str(), caret_byte);
        let scale = self.glyph_cache_scale_factor.max(0.0001);
        self.ensure_glyph_layout(text.as_str(), scale);
        let mut layout_cursor_opt = None;
        for affinity in [Affinity::Before, Affinity::After] {
            let cursor = Cursor::new_with_affinity(line, index, affinity);
            let Some(layout_cursor) = Self::with_shared_font_system(|font_system| {
                self.glyph_buffer.layout_cursor(font_system, cursor)
            }) else {
                continue;
            };
            if layout_cursor.line == line {
                layout_cursor_opt = Some(layout_cursor);
                break;
            }
        }
        let Some(layout_cursor) = layout_cursor_opt else {
            return false;
        };

        let runs = collect_run_positions(&self.glyph_buffer);
        let Some(current_run_idx) = runs.iter().position(|run| {
            run.line_i == layout_cursor.line && run.layout_i == layout_cursor.layout
        }) else {
            return false;
        };

        let target_run_idx = match motion {
            Motion::Up => current_run_idx.checked_sub(1),
            Motion::Down => current_run_idx
                .checked_add(1)
                .filter(|&idx| idx < runs.len()),
            _ => None,
        };
        let Some(target_run_idx) = target_run_idx else {
            return false;
        };

        let current_local_x = find_layout_run_by_line_layout(
            &self.glyph_buffer,
            layout_cursor.line,
            layout_cursor.layout,
        )
        .and_then(|run| caret_x_in_layout_run(index, &run))
        .unwrap_or(0.0);
        let desired_x = self
            .vertical_cursor_x_opt
            .map(|x| x as f32)
            .unwrap_or(current_local_x);

        let target_run = runs[target_run_idx];
        let target_y = target_run.line_top + target_run.line_height * 0.5;
        let Some(next_cursor) = self.glyph_buffer.hit(desired_x.max(0.0), target_y) else {
            return false;
        };

        let next_char = self.cursor_char_from_line_index_for_text(
            text.as_str(),
            next_cursor.line,
            next_cursor.index,
        );
        if next_char == self.cursor_char {
            self.vertical_cursor_x_opt = Some(desired_x.round() as i32);
            return false;
        }
        self.cursor_char = next_char;
        self.reset_caret_blink();
        self.cached_ime_cursor_rect = None;
        self.vertical_cursor_x_opt = Some(desired_x.round() as i32);
        true
    }
}

fn key_matches(key: &str, code: &str, token: &str) -> bool {
    key.eq_ignore_ascii_case(token)
        || key == format!("Named({token})")
        || code == format!("Code({token})")
}

fn normalize_multiline(content: String, multiline: bool) -> String {
    if multiline {
        content
    } else {
        content.replace('\n', " ")
    }
}

fn truncate_to_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn byte_index_at_char(value: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }
    value
        .char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(value.len())
}

fn clamp_utf8_boundary(value: &str, mut byte_index: usize) -> usize {
    byte_index = byte_index.min(value.len());
    while byte_index > 0 && !value.is_char_boundary(byte_index) {
        byte_index -= 1;
    }
    byte_index
}

fn line_and_index_from_byte(value: &str, target: usize) -> (usize, usize) {
    let target = clamp_utf8_boundary(value, target);
    let mut line = 0usize;
    let mut line_start = 0usize;

    for (idx, ch) in value.char_indices() {
        if idx == target {
            return (line, idx - line_start);
        }
        if ch == '\n' {
            let next = idx + ch.len_utf8();
            if target == next {
                return (line + 1, 0);
            }
            line += 1;
            line_start = next;
        }
    }

    (line, target.saturating_sub(line_start))
}

fn estimate_char_width_px(ch: char, font_size: f32) -> f32 {
    if ch == '\t' {
        return font_size * 2.0;
    }
    if ch.is_whitespace() {
        return font_size * 0.33;
    }
    if ch.is_ascii() {
        return font_size * 0.56;
    }
    font_size * 1.0
}

fn line_lengths_bytes(value: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for segment in value.split('\n') {
        out.push(segment.len());
    }
    if out.is_empty() {
        out.push(0);
    }
    out
}

fn caret_x_in_layout_run(cursor_index: usize, run: &cosmic_text::LayoutRun<'_>) -> Option<f32> {
    let mut found_glyph = None;
    let mut offset = 0.0_f32;

    for (glyph_i, glyph) in run.glyphs.iter().enumerate() {
        if cursor_index == glyph.start {
            found_glyph = Some(glyph_i);
            offset = 0.0;
            break;
        }
        if cursor_index > glyph.start && cursor_index < glyph.end {
            found_glyph = Some(glyph_i);
            let span = (glyph.end - glyph.start).max(1) as f32;
            let rel = (cursor_index - glyph.start) as f32;
            offset = glyph.w * (rel / span);
            break;
        }
    }

    let x = match found_glyph {
        Some(glyph_i) => run.glyphs.get(glyph_i).map_or(0.0, |glyph| {
            if glyph.level.is_rtl() {
                glyph.x + glyph.w - offset
            } else {
                glyph.x + offset
            }
        }),
        None => match run.glyphs.last() {
            Some(glyph) if cursor_index == glyph.end => {
                if glyph.level.is_rtl() {
                    glyph.x
                } else {
                    glyph.x + glyph.w
                }
            }
            Some(glyph) => {
                if glyph.level.is_rtl() {
                    glyph.x
                } else {
                    glyph.x + glyph.w
                }
            }
            None => 0.0,
        },
    };

    if run.glyphs.is_empty() {
        return Some(0.0);
    }

    if let Some(first) = run.glyphs.first() {
        if cursor_index < first.start {
            return Some(first.x);
        }
    }

    Some(x)
}

fn find_layout_run_by_line_layout<'a>(
    buffer: &'a GlyphBuffer,
    target_line: usize,
    target_layout: usize,
) -> Option<cosmic_text::LayoutRun<'a>> {
    let mut current_layout_for_line = 0usize;
    let mut previous_line = None::<usize>;
    for run in buffer.layout_runs() {
        if previous_line != Some(run.line_i) {
            previous_line = Some(run.line_i);
            current_layout_for_line = 0;
        }
        if run.line_i == target_line && current_layout_for_line == target_layout {
            return Some(run);
        }
        current_layout_for_line += 1;
    }
    None
}

#[derive(Clone, Copy)]
struct RunPosition {
    line_i: usize,
    layout_i: usize,
    line_top: f32,
    line_height: f32,
}

fn collect_run_positions(buffer: &GlyphBuffer) -> Vec<RunPosition> {
    let mut out = Vec::new();
    let mut current_layout_for_line = 0usize;
    let mut previous_line = None::<usize>;
    for run in buffer.layout_runs() {
        if previous_line != Some(run.line_i) {
            previous_line = Some(run.line_i);
            current_layout_for_line = 0;
        }
        out.push(RunPosition {
            line_i: run.line_i,
            layout_i: current_layout_for_line,
            line_top: run.line_top,
            line_height: run.line_height,
        });
        current_layout_for_line += 1;
    }
    out
}

fn fallback_line_top_for_cursor_line(buffer: &GlyphBuffer, target_line: usize) -> Option<f32> {
    let mut first_for_target = None;
    let mut last_before = None;
    for run in buffer.layout_runs() {
        if run.line_i == target_line {
            first_for_target = Some(run.line_top);
            break;
        }
        if run.line_i < target_line {
            last_before = Some((run.line_i, run.line_top, run.line_height));
        }
    }

    if first_for_target.is_some() {
        return first_for_target;
    }

    let (line_i, line_top, line_height) = last_before?;
    let line_gap = target_line.saturating_sub(line_i) as f32;
    Some(line_top + line_height * line_gap)
}

impl ElementTrait for TextArea {
    fn id(&self) -> u64 {
        self.element.id()
    }

    fn parent_id(&self) -> Option<u64> {
        self.element.parent_id()
    }

    fn set_parent_id(&mut self, parent_id: Option<u64>) {
        self.element.set_parent_id(parent_id);
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.element.id(),
            parent_id: self.element.parent_id(),
            x: self.layout_position.x,
            y: self.layout_position.y,
            width: self.layout_size.width,
            height: self.layout_size.height,
            border_radius: 0.0,
            should_render: self.should_render,
        }
    }

    fn children(&self) -> Option<&[Box<dyn ElementTrait>]> {
        None
    }

    fn children_mut(&mut self) -> Option<&mut [Box<dyn ElementTrait>]> {
        None
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn snapshot_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(self.cursor_snapshot()))
    }

    fn restore_state(&mut self, snapshot: &dyn std::any::Any) -> bool {
        let Some(snapshot) = snapshot.downcast_ref::<TextAreaCursorSnapshot>() else {
            return false;
        };
        self.apply_cursor_snapshot(*snapshot);
        true
    }

    fn promotion_node_info(&self) -> PromotionNodeInfo {
        PromotionNodeInfo {
            estimated_pass_count: 3,
            opacity: self.opacity,
            ..Default::default()
        }
    }

    fn promotion_self_signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.should_render.hash(&mut hasher);
        self.layout_position.x.to_bits().hash(&mut hasher);
        self.layout_position.y.to_bits().hash(&mut hasher);
        self.content.hash(&mut hasher);
        self.placeholder.hash(&mut hasher);
        self.color.to_rgba_u8().hash(&mut hasher);
        self.selection_background_color
            .to_rgba_u8()
            .hash(&mut hasher);
        self.placeholder_color.to_rgba_u8().hash(&mut hasher);
        self.font_families.hash(&mut hasher);
        self.font_size.to_bits().hash(&mut hasher);
        self.line_height.to_bits().hash(&mut hasher);
        self.multiline.hash(&mut hasher);
        self.read_only.hash(&mut hasher);
        self.layout_size.width.max(0.0).to_bits().hash(&mut hasher);
        self.layout_size.height.max(0.0).to_bits().hash(&mut hasher);
        self.scroll_y.to_bits().hash(&mut hasher);
        self.cursor_char.hash(&mut hasher);
        self.selection_anchor_char.hash(&mut hasher);
        self.selection_focus_char.hash(&mut hasher);
        self.ime_preedit.hash(&mut hasher);
        self.ime_preedit_cursor.hash(&mut hasher);
        self.should_draw_caret().hash(&mut hasher);
        self.render_fragments.len().hash(&mut hasher);
        hasher.finish()
    }

    fn local_dirty_flags(&self) -> super::DirtyFlags {
        self.dirty_flags
    }

    fn clear_local_dirty_flags(&mut self, flags: super::DirtyFlags) {
        self.dirty_flags = self.dirty_flags.without(flags);
    }
}

impl EventTarget for TextArea {
    fn dispatch_mouse_down(
        &mut self,
        event: &mut crate::ui::MouseDownEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        control.set_focus(Some(self.id()));
        self.is_focused = true;
        self.clear_preedit();
        self.reset_caret_blink();
        let previous = self.cursor_char;
        if !self.set_cursor_from_projection_position(event.mouse.viewport_x, event.mouse.viewport_y)
        {
            self.set_cursor_from_local_position(event.mouse.local_x, event.mouse.local_y);
        }
        if event.mouse.modifiers.shift {
            let anchor = self.selection_anchor_char.unwrap_or(previous);
            self.selection_anchor_char = Some(anchor);
            self.selection_focus_char = Some(self.cursor_char);
        } else {
            self.selection_anchor_char = Some(self.cursor_char);
            self.selection_focus_char = Some(self.cursor_char);
        }
        self.mouse_selecting = event.mouse.button == Some(UiMouseButton::Left);
        if self.mouse_selecting {
            control.set_pointer_capture(self.id());
        }
        self.element.dispatch_mouse_down(event, control);
        event.meta.stop_propagation();
        control.request_redraw();
    }

    fn dispatch_mouse_up(
        &mut self,
        event: &mut crate::ui::MouseUpEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        if event.mouse.button == Some(UiMouseButton::Left) {
            self.mouse_selecting = false;
            control.release_pointer_capture(self.id());
            if self.selection_anchor_char == self.selection_focus_char {
                self.clear_selection();
            }
            control.request_redraw();
        }
        self.element.dispatch_mouse_up(event, control);
    }

    fn dispatch_mouse_move(
        &mut self,
        event: &mut crate::ui::MouseMoveEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        if self.mouse_selecting && event.mouse.buttons.left {
            if !self
                .set_cursor_from_projection_position(event.mouse.viewport_x, event.mouse.viewport_y)
            {
                self.set_cursor_from_local_position(event.mouse.local_x, event.mouse.local_y);
            }
            if self.selection_anchor_char.is_none() {
                self.selection_anchor_char = Some(self.cursor_char);
            }
            self.selection_focus_char = Some(self.cursor_char);
            event.meta.stop_propagation();
            control.request_redraw();
        }
        self.element.dispatch_mouse_move(event, control);
    }

    fn dispatch_click(
        &mut self,
        event: &mut crate::ui::ClickEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_click(event, control);
    }

    fn cancel_pointer_interaction(&mut self) -> bool {
        let was_selecting = self.mouse_selecting;
        self.mouse_selecting = false;
        was_selecting || self.element.cancel_pointer_interaction()
    }

    fn cursor(&self) -> crate::Cursor {
        crate::Cursor::Text
    }

    fn dispatch_key_down(
        &mut self,
        event: &mut crate::ui::KeyDownEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        // Keep keydown handling in IME while composing to avoid mutating committed text.
        if !self.ime_preedit.is_empty() {
            self.element.dispatch_key_down(event, control);
            return;
        }
        let previous_content = self.content.clone();
        let handled = self.handle_key_down(event, control);
        if handled {
            if self.content != previous_content {
                self.notify_change_handlers();
            }
            self.ensure_cursor_visible();
            event.meta.stop_propagation();
            control.request_redraw();
        }
        self.element.dispatch_key_down(event, control);
    }

    fn dispatch_key_up(
        &mut self,
        event: &mut crate::ui::KeyUpEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_key_up(event, control);
    }

    fn dispatch_text_input(
        &mut self,
        event: &mut crate::ui::TextInputEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        if self.read_only || event.text.is_empty() {
            return;
        }
        self.clear_preedit();
        let previous_content = self.content.clone();
        if self.insert_text(event.text.as_str()) {
            if self.content != previous_content {
                self.notify_change_handlers();
            }
            self.ensure_cursor_visible();
            event.meta.stop_propagation();
            control.request_redraw();
        }
    }

    fn dispatch_ime_preedit(
        &mut self,
        event: &mut crate::ui::ImePreeditEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        if self.read_only {
            return;
        }
        if event.text.is_empty() {
            self.clear_preedit();
        } else {
            self.set_preedit(event.text.clone(), event.cursor);
        }
        self.ensure_cursor_visible();
        event.meta.stop_propagation();
        control.request_redraw();
    }

    fn dispatch_focus(
        &mut self,
        event: &mut crate::ui::FocusEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.is_focused = true;
        self.reset_caret_blink();
        if !self.on_focus_handlers.is_empty() {
            let mut focus_event = crate::ui::TextAreaFocusEvent {
                meta: event.meta.clone(),
                target: event.meta.text_selection_target(self.id()),
            };
            for handler in &self.on_focus_handlers {
                handler.call(&mut focus_event);
            }
        }
        self.element.dispatch_focus(event, control);
    }

    fn dispatch_blur(
        &mut self,
        event: &mut crate::ui::BlurEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        let had_selection = self.selection_range_chars().is_some();
        self.is_focused = false;
        self.mouse_selecting = false;
        self.clear_selection();
        self.cached_ime_cursor_rect = None;
        self.glyph_layout_valid = false;
        self.clear_preedit();
        if had_selection {
            control.request_redraw();
        }
        self.element.dispatch_blur(event, control);
    }

    fn scroll_by(&mut self, _dx: f32, dy: f32) -> bool {
        if !self.multiline {
            return false;
        }
        let max = self.max_scroll_y();
        if max <= 0.0 {
            return false;
        }
        let next = (self.scroll_y + dy).clamp(0.0, max);
        let changed = (next - self.scroll_y).abs() > 0.001;
        self.scroll_y = next;
        if changed {
            self.cached_ime_cursor_rect = None;
            self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::PAINT);
        }
        changed
    }

    fn can_scroll_by(&self, _dx: f32, dy: f32) -> bool {
        if !self.multiline {
            return false;
        }
        let max = self.max_scroll_y();
        if max <= 0.0 {
            return false;
        }
        let next = (self.scroll_y + dy).clamp(0.0, max);
        (next - self.scroll_y).abs() > 0.001
    }

    fn get_scroll_offset(&self) -> (f32, f32) {
        (0.0, self.scroll_y)
    }

    fn set_scroll_offset(&mut self, offset: (f32, f32)) {
        let changed = (self.scroll_y - offset.1).abs() > 0.001;
        self.scroll_y = offset.1;
        self.clamp_scroll();
        self.cached_ime_cursor_rect = None;
        if changed {
            self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::PAINT);
        }
    }

    fn ime_cursor_rect(&self) -> Option<(f32, f32, f32, f32)> {
        self.cached_ime_cursor_rect
    }

    fn wants_animation_frame(&self) -> bool {
        self.is_focused
    }
}

impl Layoutable for TextArea {
    fn measured_size(&self) -> (f32, f32) {
        (self.size.width, self.size.height)
    }

    fn set_layout_width(&mut self, width: f32) {
        self.layout_override_width = Some(width.max(0.0));
    }

    fn set_layout_height(&mut self, height: f32) {
        self.layout_override_height = Some(height.max(0.0));
        self.cached_ime_cursor_rect = None;
    }

    fn allows_cross_stretch(&self, is_row: bool) -> bool {
        if is_row {
            self.auto_height
        } else {
            self.auto_width
        }
    }

    fn flex_grow(&self) -> f32 {
        self.element.flex_grow()
    }

    fn flex_shrink(&self) -> f32 {
        self.element.flex_shrink()
    }

    fn flex_basis(&self) -> crate::SizeValue {
        self.element.flex_basis()
    }

    fn flex_main_size(&self, is_row: bool) -> crate::SizeValue {
        if (is_row && self.auto_width) || (!is_row && self.auto_height) {
            crate::SizeValue::Auto
        } else {
            <Element as Layoutable>::flex_main_size(&self.element, is_row)
        }
    }

    fn flex_has_explicit_min_main_size(&self, is_row: bool) -> bool {
        <Element as Layoutable>::flex_has_explicit_min_main_size(&self.element, is_row)
    }

    fn flex_auto_min_main_size(&self, is_row: bool) -> Option<f32> {
        if self.flex_has_explicit_min_main_size(is_row)
            || self.flex_main_size(is_row) != crate::SizeValue::Auto
        {
            return None;
        }
        let (measured_w, measured_h) = self.measured_size();
        Some(if is_row { measured_w } else { measured_h }.max(0.0))
    }

    fn flex_min_main_size(&self, is_row: bool) -> crate::SizeValue {
        <Element as Layoutable>::flex_min_main_size(&self.element, is_row)
    }

    fn flex_max_main_size(&self, is_row: bool) -> crate::SizeValue {
        <Element as Layoutable>::flex_max_main_size(&self.element, is_row)
    }

    fn set_layout_offset(&mut self, x: f32, y: f32) {
        self.position = Position { x, y };
        self.element.set_position(x, y);
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::RUNTIME);
    }

    fn measure(&mut self, constraints: LayoutConstraints) {
        self.layout_override_width = None;
        self.layout_override_height = None;
        self.sync_size_from_style(
            constraints.percent_base_width,
            constraints.percent_base_height,
            constraints.viewport_width,
            constraints.viewport_height,
        );

        if !self.auto_width && !self.auto_height {
            self.dirty_flags = self.dirty_flags.without(super::DirtyFlags::LAYOUT);
            return;
        }

        let line_px = self.line_height_px();
        let line_widths = self.ensure_measure_line_widths().to_vec();
        let line_count = line_widths.len().max(1);

        if self.auto_width {
            let intrinsic_width = line_widths.iter().copied().fold(0.0_f32, f32::max).max(1.0);
            let available = constraints.max_width.max(1.0);
            self.size.width = round_layout_value(intrinsic_width.min(available));
            self.element.set_width(self.size.width);
        }

        if self.auto_height {
            let effective_width = if self.auto_width {
                self.size.width.max(1.0)
            } else {
                self.size.width.min(constraints.max_width.max(1.0)).max(1.0)
            };

            let resolved_lines = if self.multiline {
                let wrapped_lines = if line_widths.is_empty() {
                    1
                } else {
                    line_widths
                        .iter()
                        .map(|line_width| {
                            ((*line_width) / effective_width).ceil().max(1.0) as usize
                        })
                        .sum::<usize>()
                };
                wrapped_lines.max(line_count)
            } else {
                1
            };

            self.size.height = round_layout_value((line_px * resolved_lines as f32).max(1.0));
            self.element.set_height(self.size.height);
        }
        self.dirty_flags = self.dirty_flags.without(super::DirtyFlags::LAYOUT);
    }

    fn place(&mut self, placement: LayoutPlacement) {
        if !self.dirty_flags.intersects(
            super::DirtyFlags::PLACE
                .union(super::DirtyFlags::BOX_MODEL)
                .union(super::DirtyFlags::HIT_TEST),
        ) && self.last_layout_placement == Some(placement)
        {
            return;
        }
        self.sync_size_from_style(
            placement.percent_base_width,
            placement.percent_base_height,
            placement.viewport_width,
            placement.viewport_height,
        );

        let prev_layout_width = self.layout_size.width;
        let available_width = placement.available_width.max(0.0);
        let available_height = placement.available_height.max(0.0);
        let max_width = (available_width - self.position.x.max(0.0)).max(0.0);
        let max_height = (available_height - self.position.y.max(0.0)).max(0.0);
        let layout_width = self.layout_override_width.unwrap_or(self.size.width);
        let layout_height = self.layout_override_height.unwrap_or(self.size.height);
        self.layout_size = Size {
            width: round_layout_value(layout_width.max(0.0).min(max_width)),
            height: round_layout_value(layout_height.max(0.0).min(max_height)),
        };
        self.layout_position = Position {
            x: round_layout_value(placement.parent_x + self.position.x + placement.visual_offset_x),
            y: round_layout_value(placement.parent_y + self.position.y + placement.visual_offset_y),
        };

        let parent_left = placement.parent_x + placement.visual_offset_x;
        let parent_top = placement.parent_y + placement.visual_offset_y;
        let parent_right = parent_left + available_width;
        let parent_bottom = parent_top + available_height;
        let self_left = self.layout_position.x;
        let self_top = self.layout_position.y;
        let self_right = self.layout_position.x + self.layout_size.width;
        let self_bottom = self.layout_position.y + self.layout_size.height;
        self.should_render = self.layout_size.width > 0.0
            && self.layout_size.height > 0.0
            && self_right > parent_left
            && self_left < parent_right
            && self_bottom > parent_top
            && self_top < parent_bottom;

        if (prev_layout_width - self.layout_size.width).abs() > 0.01 {
            self.invalidate_glyph_layout();
        }
        if !self.render_nodes.is_empty() || self.on_render_handler.is_some() {
            self.layout_render_fragments(placement.viewport_width, placement.viewport_height);
        }
        self.clamp_scroll();
        self.last_layout_placement = Some(placement);
        self.dirty_flags = self.dirty_flags.without(
            super::DirtyFlags::PLACE
                .union(super::DirtyFlags::BOX_MODEL)
                .union(super::DirtyFlags::HIT_TEST),
        );
    }
}

impl Renderable for TextArea {
    fn build(&mut self, graph: &mut FrameGraph, mut ctx: UiBuildContext) -> BuildState {
        if !self.should_render {
            return ctx.into_state();
        }

        let opacity = if ctx.is_node_promoted(self.id()) {
            1.0
        } else {
            self.opacity.clamp(0.0, 1.0)
        };
        if opacity <= 0.0 {
            return ctx.into_state();
        }

        let (content, color) = self.render_payload();

        let clip = Some([
            self.layout_position.x.floor().max(0.0) as u32,
            self.layout_position.y.floor().max(0.0) as u32,
            self.layout_size.width.ceil().max(0.0) as u32,
            self.layout_size.height.ceil().max(0.0) as u32,
        ]);

        if self.uses_projection_rendering() {
            self.place_projection_fragments(
                self.layout_size.width.max(1.0),
                self.layout_size.height.max(1.0),
            );
            let content = self.content.clone();
            for (position, size) in self.selection_screen_rects(content.as_str()) {
                let fill_color = self.selection_background_color.to_rgba_f32();
                let mut selection_pass = DrawRectPass::new(
                    RectPassParams {
                        position,
                        size,
                        fill_color,
                        opacity,
                        ..Default::default()
                    },
                    DrawRectInput::default(),
                    DrawRectOutput::default(),
                );
                selection_pass.set_scissor_rect(clip);
                push_draw_rect_pass_explicit(graph, &mut ctx, selection_pass);
            }

            for fragment in &mut self.render_fragments {
                let screen_x = round_layout_value(self.layout_position.x + fragment.content_x);
                let screen_y =
                    round_layout_value(self.layout_position.y + fragment.content_y - self.scroll_y);
                match &fragment.kind {
                    TextAreaRenderFragmentKind::Text(text)
                    | TextAreaRenderFragmentKind::Preedit(text) => {
                        push_text_pass_explicit(
                            graph,
                            &mut ctx,
                            TextPassParams::single_fragment(
                                crate::view::render_pass::text_pass::TextPassFragment {
                                    content: text.clone(),
                                    x: screen_x,
                                    y: screen_y,
                                    width: round_layout_value(fragment.width.max(1.0)),
                                    height: round_layout_value(fragment.height.max(1.0)),
                                    color: self.color.to_rgba_f32(),
                                    opacity,
                                    layout_buffer: None,
                                },
                                self.font_size,
                                self.line_height,
                                400,
                                self.font_families.clone(),
                                Align::Left,
                                false,
                                clip,
                                None,
                            ),
                        );
                    }
                    TextAreaRenderFragmentKind::Projection(index) => {
                        let Some(node) = self.render_nodes.get_mut(*index) else {
                            continue;
                        };
                        let viewport = ctx.viewport();
                        let next_state = node.node.build(graph, ctx);
                        ctx = UiBuildContext::from_parts(viewport, next_state);
                    }
                }
            }

            let ime_underline_rects = self.ime_preedit_underline_rects(content.as_str());
            for (position, size) in ime_underline_rects {
                let mut underline_pass = DrawRectPass::new(
                    RectPassParams {
                        position,
                        size,
                        fill_color: self.color.to_rgba_f32(),
                        opacity,
                        ..Default::default()
                    },
                    DrawRectInput::default(),
                    DrawRectOutput::default(),
                );
                underline_pass.set_scissor_rect(clip);
                push_draw_rect_pass_explicit(graph, &mut ctx, underline_pass);
            }

            if let Some((caret_x, caret_y)) = self.caret_screen_position() {
                self.cached_ime_cursor_rect = Some((caret_x, caret_y, 1.0, self.line_height_px()));
                if self.should_draw_caret() {
                    let mut caret_pass = DrawRectPass::new(
                        RectPassParams {
                            position: [caret_x, caret_y],
                            size: [1.0, self.line_height_px()],
                            fill_color: self.color.to_rgba_f32(),
                            opacity,
                            ..Default::default()
                        },
                        DrawRectInput::default(),
                        DrawRectOutput::default(),
                    );
                    caret_pass.set_scissor_rect(clip);
                    push_draw_rect_pass_explicit(graph, &mut ctx, caret_pass);
                }
            } else {
                self.cached_ime_cursor_rect = None;
            }
            return ctx.into_state();
        }

        if !content.is_empty() {
            let scale = ctx.viewport().scale_factor();
            self.ensure_glyph_layout(content.as_str(), scale);
            for (position, size) in self.selection_screen_rects(content.as_str()) {
                let fill_color = self.selection_background_color.to_rgba_f32();
                let mut selection_pass = DrawRectPass::new(
                    RectPassParams {
                        position,
                        size,
                        fill_color,
                        opacity,
                        ..Default::default()
                    },
                    DrawRectInput::default(),
                    DrawRectOutput::default(),
                );
                selection_pass.set_scissor_rect(clip);
                push_draw_rect_pass_explicit(graph, &mut ctx, selection_pass);
            }
            let ime_underline_rects = self.ime_preedit_underline_rects(content.as_str());

            push_text_pass_explicit(
                graph,
                &mut ctx,
                TextPassParams::single_fragment(
                    TextPassFragment {
                        content,
                        x: round_layout_value(self.layout_position.x),
                        y: round_layout_value(self.layout_position.y - self.scroll_y),
                        width: round_layout_value(self.layout_size.width),
                        height: round_layout_value(
                            self.layout_size.height.max(self.content_height()),
                        ),
                        color,
                        opacity,
                        layout_buffer: Some(self.glyph_buffer.clone()),
                    },
                    self.font_size,
                    self.line_height,
                    400,
                    self.font_families.clone(),
                    Align::Left,
                    self.multiline,
                    clip,
                    None,
                ),
            );

            for (position, size) in ime_underline_rects {
                let mut underline_pass = DrawRectPass::new(
                    RectPassParams {
                        position,
                        size,
                        fill_color: self.color.to_rgba_f32(),
                        opacity,
                        ..Default::default()
                    },
                    DrawRectInput::default(),
                    DrawRectOutput::default(),
                );
                underline_pass.set_scissor_rect(clip);
                push_draw_rect_pass_explicit(graph, &mut ctx, underline_pass);
            }
        }

        if let Some((caret_x, caret_y)) = self.caret_screen_position() {
            self.cached_ime_cursor_rect = Some((caret_x, caret_y, 1.0, self.line_height_px()));
            if !self.should_draw_caret() {
                for node in &mut self.render_nodes {
                    let viewport = ctx.viewport();
                    let next_state = node.node.build(graph, ctx);
                    ctx = UiBuildContext::from_parts(viewport, next_state);
                }
                return ctx.into_state();
            }
            let mut caret_pass = DrawRectPass::new(
                RectPassParams {
                    position: [caret_x, caret_y],
                    size: [1.0, self.line_height_px()],
                    fill_color: self.color.to_rgba_f32(),
                    opacity,
                    ..Default::default()
                },
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            caret_pass.set_scissor_rect(clip);
            push_draw_rect_pass_explicit(graph, &mut ctx, caret_pass);
        } else {
            self.cached_ime_cursor_rect = None;
        }
        for node in &mut self.render_nodes {
            let viewport = ctx.viewport();
            let next_state = node.node.build(graph, ctx);
            ctx = UiBuildContext::from_parts(viewport, next_state);
        }
        ctx.into_state()
    }
}

fn clamp_char_range<R>(content: &str, range: R) -> Option<Range<usize>>
where
    R: RangeBounds<usize>,
{
    let len = content.chars().count();
    let start = match range.start_bound() {
        Bound::Included(value) => *value,
        Bound::Excluded(value) => value.saturating_add(1),
        Bound::Unbounded => 0,
    }
    .min(len);
    let end = match range.end_bound() {
        Bound::Included(value) => value.saturating_add(1),
        Bound::Excluded(value) => *value,
        Bound::Unbounded => len,
    }
    .min(len);
    if start >= end {
        return None;
    }
    Some(start..end)
}

fn append_plain_render_fragments(
    out: &mut Vec<TextAreaRenderFragment>,
    content: &str,
    start: usize,
    end: usize,
) {
    let chars: Vec<char> = content.chars().collect();
    let end = end.min(chars.len());
    let start = start.min(end);
    if start >= end {
        return;
    }
    out.push(TextAreaRenderFragment {
        source_range: start..end,
        kind: TextAreaRenderFragmentKind::Text(chars[start..end].iter().collect::<String>()),
        content_x: 0.0,
        content_y: 0.0,
        width: 0.0,
        height: 0.0,
        layout_buffer: None,
    });
}

fn append_preedit_render_fragment(
    out: &mut Vec<TextAreaRenderFragment>,
    cursor_char: usize,
    preedit: &str,
) {
    if preedit.is_empty() {
        return;
    }
    out.push(TextAreaRenderFragment {
        source_range: cursor_char..cursor_char,
        kind: TextAreaRenderFragmentKind::Preedit(preedit.to_string()),
        content_x: 0.0,
        content_y: 0.0,
        width: 0.0,
        height: 0.0,
        layout_buffer: None,
    });
}

fn normalize_text_area_render_projections(
    content: &str,
    projections: &[TextAreaRenderProjection],
) -> Vec<TextAreaRenderProjection> {
    let mut sorted = projections.to_vec();
    sorted.sort_by_key(|projection| projection.range.start);

    let mut normalized: Vec<TextAreaRenderProjection> = Vec::new();
    for projection in sorted {
        let mut next = Vec::new();
        for existing in normalized {
            next.extend(subtract_projection_overlap(
                content,
                existing,
                &projection.range,
            ));
        }
        next.push(slice_text_area_render_projection(
            content,
            &projection,
            projection.range.clone(),
        ));
        normalized = next;
    }
    normalized.sort_by_key(|projection| projection.range.start);
    normalized
}

fn subtract_projection_overlap(
    content: &str,
    projection: TextAreaRenderProjection,
    covering_range: &Range<usize>,
) -> Vec<TextAreaRenderProjection> {
    if covering_range.end <= projection.range.start || covering_range.start >= projection.range.end
    {
        return vec![projection];
    }

    let mut fragments = Vec::new();
    if projection.range.start < covering_range.start {
        fragments.push(slice_text_area_render_projection(
            content,
            &projection,
            projection.range.start..covering_range.start.min(projection.range.end),
        ));
    }
    if projection.range.end > covering_range.end {
        fragments.push(slice_text_area_render_projection(
            content,
            &projection,
            covering_range.end.max(projection.range.start)..projection.range.end,
        ));
    }
    fragments
}

fn slice_text_area_render_projection(
    content: &str,
    projection: &TextAreaRenderProjection,
    range: Range<usize>,
) -> TextAreaRenderProjection {
    let mut node = projection.node.clone();
    update_projection_rsx_node_range(&mut node, content, range.clone());
    TextAreaRenderProjection { range, node }
}

fn update_projection_rsx_node_range(node: &mut RsxNode, content: &str, range: Range<usize>) {
    if let RsxNode::Element(element) = node {
        if element.tag == "TextArea" {
            let start_byte = byte_index_at_char(content, range.start);
            let end_byte = byte_index_at_char(content, range.end);
            set_rsx_element_prop(
                element,
                "content",
                PropValue::String(content[start_byte..end_byte].to_string()),
            );
            set_rsx_element_prop(
                element,
                "source_text_start",
                PropValue::I64(range.start as i64),
            );
            set_rsx_element_prop(element, "source_text_end", PropValue::I64(range.end as i64));
        }
    }

    if let Some(children) = node.children_mut() {
        for child in children {
            update_projection_rsx_node_range(child, content, range.clone());
        }
    }
}

fn set_rsx_element_prop(element: &mut crate::ui::RsxElementNode, key: &str, value: PropValue) {
    if let Some((_, prop_value)) = element
        .props
        .iter_mut()
        .rev()
        .find(|(prop_key, _)| prop_key == key)
    {
        *prop_value = value;
        return;
    }
    element.props.push((key.to_string(), value));
}

fn find_first_text_area_mut(node: &mut dyn ElementTrait) -> Option<&mut TextArea> {
    if node.as_any().is::<TextArea>() {
        return node.as_any_mut().downcast_mut::<TextArea>();
    }
    let children = node.children_mut()?;
    for child in children {
        if let Some(text_area) = find_first_text_area_mut(child.as_mut()) {
            return Some(text_area);
        }
    }
    None
}

fn apply_text_source_range(node: &mut dyn ElementTrait, range: Range<usize>) {
    if let Some(text_area) = node.as_any_mut().downcast_mut::<TextArea>() {
        text_area.set_source_text_range(Some(range.clone()));
    }
    if let Some(children) = node.children_mut() {
        for child in children {
            apply_text_source_range(child.as_mut(), range.clone());
        }
    }
}

fn push_draw_rect_pass_explicit(
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
    mut pass: DrawRectPass,
) {
    let Some(input_target) = ctx.current_target() else {
        return;
    };
    pass.set_input(
        input_target
            .handle()
            .map(crate::view::render_pass::draw_rect_pass::RenderTargetIn::with_handle)
            .unwrap_or_default(),
    );
    pass.draw_rect_input_mut().pass_context = ctx.graphics_pass_context();
    pass.set_output(input_target);
    graph.add_graphics_pass(pass);
    ctx.set_current_target(input_target);
}

fn push_text_pass_explicit(
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
    params: TextPassParams,
) {
    let Some(input_target) = ctx.current_target() else {
        return;
    };
    let pass = TextPass::new(
        params,
        TextInput {
            pass_context: ctx.graphics_pass_context(),
        },
        TextOutput {
            render_target: input_target,
            ..Default::default()
        },
    );
    graph.add_graphics_pass(pass);
    ctx.set_current_target(input_target);
}

#[cfg(test)]
mod tests {
    use super::{TextArea, TextAreaRenderFragmentKind};
    use crate::ColorLike;
    use crate::Length;
    use crate::ui::{
        Binding, BlurEvent, EventMeta, FocusEvent, MouseButton, MouseButtons, MouseDownEvent,
        MouseEventData, TextInputEvent, ViewportListenerAction, rsx,
    };
    use crate::view::base_component::{
        DirtyFlags, Element, ElementTrait, EventTarget, LayoutConstraints, LayoutPlacement,
        Layoutable, dispatch_mouse_down_from_hit_test, select_all_text_by_id,
        select_text_range_by_id,
    };
    use crate::view::renderer_adapter::rsx_to_elements;
    use crate::view::{Viewport, ViewportControl};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn multiline_false_normalizes_newline() {
        let mut area = TextArea::from_content("a\nb");
        area.set_multiline(false);
        area.set_text("x\ny");
        assert_eq!(area.content, "x y");
    }

    #[test]
    fn max_length_limits_inserted_content() {
        let mut area = TextArea::from_content("");
        area.set_max_length(Some(5));
        assert!(area.insert_text("123456"));
        assert_eq!(area.content, "12345");
    }

    #[test]
    fn on_render_rebuilds_projection_only_when_content_changes() {
        let calls = Rc::new(RefCell::new(0usize));
        let calls_for_render = calls.clone();
        let mut area = TextArea::from_content("hello");
        area.on_render(move |render| {
            *calls_for_render.borrow_mut() += 1;
            render.range(1..4, |text_area_node| {
                crate::ui::rsx! {
                    <crate::view::Element>
                        {text_area_node}
                    </crate::view::Element>
                }
            });
        });

        assert_eq!(*calls.borrow(), 1);
        area.set_text("hello");
        assert_eq!(*calls.borrow(), 1);
        area.set_text("world");
        assert_eq!(*calls.borrow(), 2);

        assert_eq!(area.render_nodes.len(), 1);
        let wrapper = area.render_nodes[0]
            .node
            .as_any()
            .downcast_ref::<Element>()
            .expect("projection root element");
        let nested = wrapper.children().expect("wrapper children")[0]
            .as_any()
            .downcast_ref::<TextArea>()
            .expect("nested projection text area");
        assert_eq!(nested.source_text_range(), Some(1..4));
    }

    #[test]
    fn on_render_fragment_projection_wraps_multiple_siblings() {
        let mut area = TextArea::from_content("{{x}}");
        area.on_render(|render| {
            render.range(0..5, |text_area_node| {
                crate::ui::rsx! {
                    <crate::view::Text>abc</crate::view::Text>
                    {text_area_node}
                }
            });
        });

        assert_eq!(area.render_nodes.len(), 1);
        let wrapper = area.render_nodes[0]
            .node
            .as_any()
            .downcast_ref::<Element>()
            .expect("projection wrapper element");
        let children = wrapper.children().expect("wrapper children");
        assert_eq!(children.len(), 2);
        assert!(children.iter().any(|child| child.as_any().is::<TextArea>()));
    }

    #[test]
    fn on_render_single_text_area_projection_inherits_text_style() {
        let mut area = TextArea::from_content("{{x}}");
        area.set_font_size(13.0);
        area.set_color(crate::Color::hex("#aabbcc"));
        area.on_render(|render| {
            render.range(0..5, |text_area_node| text_area_node);
        });

        assert_eq!(area.render_nodes.len(), 1);
        let nested = area.render_nodes[0]
            .node
            .as_any()
            .downcast_ref::<TextArea>()
            .expect("nested projection text area");
        assert!((nested.font_size - 13.0).abs() < 0.01);
        assert_eq!(
            nested.color.to_rgba_f32(),
            crate::Color::hex("#aabbcc").to_rgba_f32()
        );
    }

    #[test]
    fn on_render_overlapping_ranges_are_sorted_and_later_ranges_override_earlier_ones() {
        let mut area = TextArea::from_content("abcdefghij");
        area.on_render(|render| {
            render.range(2..8, |text_area_node| {
                crate::ui::rsx! {
                    <crate::view::Element>
                        {text_area_node}
                    </crate::view::Element>
                }
            });
            render.range(4..6, |text_area_node| {
                crate::ui::rsx! {
                    <crate::view::Element>
                        {text_area_node}
                    </crate::view::Element>
                }
            });
        });

        let ranges = area
            .render_nodes
            .iter()
            .map(|projection| projection.range.clone())
            .collect::<Vec<_>>();
        assert_eq!(ranges, vec![2..4, 4..6, 6..8]);

        let nested_contents = area
            .render_nodes
            .iter()
            .map(|projection| {
                projection
                    .node
                    .as_any()
                    .downcast_ref::<Element>()
                    .expect("projection wrapper")
                    .children()
                    .expect("wrapper children")[0]
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .expect("nested projection text area")
                    .content
                    .clone()
            })
            .collect::<Vec<_>>();
        assert_eq!(nested_contents, vec!["cd", "ef", "gh"]);
    }

    #[test]
    fn rsx_on_render_with_binding_populates_projection_nodes() {
        let binding = Binding::new("{{API_HOST}}/user".to_string());
        let tree = rsx! {
            <crate::view::TextArea
                binding={binding}
                multiline={false}
                on_render={move |render: &mut crate::view::base_component::TextAreaRenderString| {
                    render.range(0..12, |text_area_node| {
                        rsx! {
                            <crate::view::Element>
                                {text_area_node}
                            </crate::view::Element>
                        }
                    });
                }}
            />
        };

        let mut roots = rsx_to_elements(&tree).expect("convert rsx");
        let area = roots[0]
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("host textarea");
        assert_eq!(area.render_nodes.len(), 1);
        assert!(matches!(
            area.render_fragments[0].kind,
            TextAreaRenderFragmentKind::Projection(_)
        ));
    }

    #[test]
    fn ime_preedit_outside_projection_becomes_render_fragment() {
        let mut area = TextArea::from_content("ab {{x}}");
        area.on_render(|render| {
            render.range(3..8, |text_area_node| text_area_node);
        });
        area.cursor_char = 1;
        area.set_preedit("中文".to_string(), None);

        assert!(area.uses_projection_rendering());
        assert!(area.render_fragments.iter().any(|fragment| matches!(
            &fragment.kind,
            TextAreaRenderFragmentKind::Preedit(text) if text == "中文"
        )));
    }

    #[test]
    fn ime_preedit_inside_projection_syncs_to_nested_text_area() {
        let mut area = TextArea::from_content("{{x}}");
        area.on_render(|render| {
            render.range(0..5, |text_area_node| text_area_node);
        });
        area.cursor_char = 2;
        area.set_preedit("中文".to_string(), None);
        area.layout_render_fragments(240.0, 64.0);

        let nested = area.render_nodes[0]
            .node
            .as_any()
            .downcast_ref::<TextArea>()
            .expect("nested projection text area");
        assert_eq!(nested.ime_preedit, "中文");
        assert_eq!(nested.cursor_char, 2);
    }

    #[test]
    fn clearing_preedit_removes_preedit_fragments() {
        let mut area = TextArea::from_content("ab {{x}}");
        area.on_render(|render| {
            render.range(3..8, |text_area_node| text_area_node);
        });
        area.cursor_char = 1;
        area.set_preedit("中文".to_string(), None);
        assert!(area.render_fragments.iter().any(|fragment| matches!(
            &fragment.kind,
            TextAreaRenderFragmentKind::Preedit(text) if text == "中文"
        )));

        area.clear_preedit();
        assert!(
            !area
                .render_fragments
                .iter()
                .any(|fragment| matches!(fragment.kind, TextAreaRenderFragmentKind::Preedit(_)))
        );
    }

    #[test]
    fn no_op_preedit_updates_do_not_mark_layout_dirty() {
        let mut area = TextArea::from_content("abc");
        area.clear_local_dirty_flags(DirtyFlags::ALL);

        area.clear_preedit();
        assert!(!area.local_dirty_flags().intersects(DirtyFlags::LAYOUT));

        area.set_preedit("中文".to_string(), Some((1, 1)));
        area.clear_local_dirty_flags(DirtyFlags::ALL);
        area.set_preedit("中文".to_string(), Some((1, 1)));
        assert!(!area.local_dirty_flags().intersects(DirtyFlags::LAYOUT));
    }

    #[test]
    fn mouse_down_on_projection_child_maps_back_to_source_range() {
        let mut area = TextArea::from_content("hello");
        area.on_render(|render| {
            render.range(1..4, |text_area_node| {
                crate::ui::rsx! {
                    <crate::view::Element style={{ width: Length::percent(100.0), height: Length::percent(100.0) }}>
                        {text_area_node}
                    </crate::view::Element>
                }
            });
        });
        area.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 240.0,
            available_height: 64.0,
            viewport_width: 240.0,
            viewport_height: 64.0,
            percent_base_width: Some(240.0),
            percent_base_height: Some(64.0),
        });

        let fragment = area
            .render_fragments
            .iter()
            .find(|fragment| matches!(fragment.kind, TextAreaRenderFragmentKind::Projection(_)))
            .expect("projection fragment");
        let click_x = area.layout_position.x + fragment.content_x + fragment.width * 0.8;
        let click_y = area.layout_position.y + fragment.content_y + fragment.height * 0.5;

        let mut viewport = Viewport::new();
        let mut control = ViewportControl::new(&mut viewport);
        let meta = EventMeta::new(0);
        let viewport_api = meta.viewport();
        let mut event = MouseDownEvent {
            meta,
            mouse: MouseEventData {
                viewport_x: click_x,
                viewport_y: click_y,
                local_x: 0.0,
                local_y: 0.0,
                current_target_width: 0.0,
                current_target_height: 0.0,
                button: Some(MouseButton::Left),
                buttons: MouseButtons::default(),
                modifiers: Default::default(),
            },
            viewport: viewport_api,
        };

        assert!(dispatch_mouse_down_from_hit_test(
            &mut area,
            &mut event,
            &mut control
        ));
        assert!((2..=4).contains(&area.cursor_char));
    }

    #[test]
    fn projection_click_uses_nested_text_area_hit_testing() {
        let mut area = TextArea::from_content("{{API_HOST}}");
        area.on_render(|render| {
            render.range(0..12, |text_area_node| {
                crate::ui::rsx! {
                    <crate::view::Element>
                        {text_area_node}
                    </crate::view::Element>
                }
            });
        });
        area.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 320.0,
            available_height: 64.0,
            viewport_width: 320.0,
            viewport_height: 64.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(64.0),
        });

        let fragment = area
            .render_fragments
            .iter()
            .find(|fragment| matches!(fragment.kind, TextAreaRenderFragmentKind::Projection(_)))
            .expect("projection fragment");
        let click_x = area.layout_position.x + fragment.content_x + fragment.width * 0.45;
        let click_y = area.layout_position.y + fragment.content_y + fragment.height * 0.5;

        assert!(area.set_cursor_from_projection_position(click_x, click_y));
        assert!(area.cursor_char > 0);
        assert!(area.cursor_char < 12);
    }

    #[test]
    fn cursor_at_wrapped_line_start_maps_to_next_line() {
        let mut area = TextArea::from_content("abcd");
        area.set_width(20.0);
        area.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 20.0,
            available_height: 100.0,
            viewport_width: 20.0,
            percent_base_width: Some(20.0),
            percent_base_height: Some(100.0),
            viewport_height: 100.0,
        });

        area.cursor_char = 2;
        let lines = area.visual_lines();
        assert!(lines.len() >= 2);
        assert_eq!(area.resolve_cursor_line(&lines), 1);
    }

    #[test]
    fn delete_selected_text_removes_range() {
        let mut area = TextArea::from_content("hello world");
        area.selection_anchor_char = Some(0);
        area.selection_focus_char = Some(5);
        assert!(area.delete_selected_text());
        assert_eq!(area.content, " world");
        assert_eq!(area.cursor_char, 0);
        assert!(area.selection_range_chars().is_none());
    }

    #[test]
    fn insert_replaces_selected_text() {
        let mut area = TextArea::from_content("hello world");
        area.selection_anchor_char = Some(6);
        area.selection_focus_char = Some(11);
        assert!(area.insert_text("rust"));
        assert_eq!(area.content, "hello rust");
        assert_eq!(area.cursor_char, 10);
    }

    #[test]
    fn ime_preedit_range_tracks_inserted_segment() {
        let mut area = TextArea::from_content("hello");
        area.cursor_char = 2;
        area.set_preedit("中文".to_string(), None);
        assert_eq!(area.ime_preedit_range_chars(), Some((2, 4)));
    }

    #[test]
    fn blur_clears_text_selection() {
        let mut area = TextArea::from_content("hello world");
        area.selection_anchor_char = Some(0);
        area.selection_focus_char = Some(5);
        area.is_focused = true;

        let mut viewport = Viewport::new();
        let mut control = ViewportControl::new(&mut viewport);
        let mut blur = BlurEvent {
            meta: EventMeta::new(0),
        };
        EventTarget::dispatch_blur(&mut area, &mut blur, &mut control);

        assert!(area.selection_range_chars().is_none());
        assert!(!area.is_focused);
    }

    #[test]
    fn text_input_dispatch_emits_on_change_with_latest_value() {
        let changes = Rc::new(RefCell::new(Vec::new()));
        let changes_for_handler = changes.clone();
        let mut area = TextArea::from_content("he");
        area.cursor_char = 2;
        area.on_change(move |event| {
            changes_for_handler.borrow_mut().push(event.value.clone());
        });

        let mut viewport = Viewport::new();
        let mut control = ViewportControl::new(&mut viewport);
        let mut event = TextInputEvent {
            meta: EventMeta::new(area.id()),
            text: "llo".to_string(),
        };
        EventTarget::dispatch_text_input(&mut area, &mut event, &mut control);

        assert_eq!(area.content, "hello");
        assert_eq!(*changes.borrow(), vec!["hello".to_string()]);
    }

    #[test]
    fn focus_event_target_can_request_select_all() {
        let mut area = TextArea::from_content("hello");
        area.on_focus(|event| {
            event.target.select_all();
        });

        let mut viewport = Viewport::new();
        let mut control = ViewportControl::new(&mut viewport);
        let mut event = FocusEvent {
            meta: EventMeta::new(area.id()),
        };
        EventTarget::dispatch_focus(&mut area, &mut event, &mut control);

        let actions = event.meta.take_viewport_listener_actions();
        assert!(matches!(
            actions.as_slice(),
            [ViewportListenerAction::SelectTextRangeAll(target_id)] if *target_id == area.id()
        ));

        let area_id = area.id();
        assert!(select_all_text_by_id(&mut area, area_id));
        assert_eq!(area.selection_range_chars(), Some((0, 5)));
    }

    #[test]
    fn select_range_clamps_to_character_bounds() {
        let mut area = TextArea::from_content("hello");
        let area_id = area.id();
        assert!(select_text_range_by_id(&mut area, area_id, 1, 99));
        assert_eq!(area.selection_range_chars(), Some((1, 5)));
        assert_eq!(area.cursor_char, 5);
    }

    #[test]
    fn mouse_selection_requests_pointer_capture_until_mouse_up() {
        let mut area = TextArea::from_content("hello world");
        area.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 80.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(80.0),
            viewport_height: 80.0,
        });

        let mut viewport = Viewport::new();
        {
            let mut control = ViewportControl::new(&mut viewport);
            let down_meta = EventMeta::new(area.id());
            let mut down = crate::ui::MouseDownEvent {
                viewport: down_meta.viewport(),
                meta: down_meta,
                mouse: crate::ui::MouseEventData {
                    viewport_x: 4.0,
                    viewport_y: 4.0,
                    local_x: 4.0,
                    local_y: 4.0,
                    current_target_width: 0.0,
                    current_target_height: 0.0,
                    button: Some(crate::ui::MouseButton::Left),
                    buttons: crate::ui::MouseButtons {
                        left: true,
                        right: false,
                        middle: false,
                        back: false,
                        forward: false,
                    },
                    modifiers: crate::ui::KeyModifiers::default(),
                },
            };

            EventTarget::dispatch_mouse_down(&mut area, &mut down, &mut control);
        }
        assert_eq!(viewport.pointer_capture_node_id(), Some(area.id()));
        assert!(area.mouse_selecting);

        {
            let mut control = ViewportControl::new(&mut viewport);
            let up_meta = EventMeta::new(area.id());
            let mut up = crate::ui::MouseUpEvent {
                viewport: up_meta.viewport(),
                meta: up_meta,
                mouse: crate::ui::MouseEventData {
                    viewport_x: 4.0,
                    viewport_y: 4.0,
                    local_x: 4.0,
                    local_y: 4.0,
                    current_target_width: 0.0,
                    current_target_height: 0.0,
                    button: Some(crate::ui::MouseButton::Left),
                    buttons: crate::ui::MouseButtons {
                        left: false,
                        right: false,
                        middle: false,
                        back: false,
                        forward: false,
                    },
                    modifiers: crate::ui::KeyModifiers::default(),
                },
            };

            EventTarget::dispatch_mouse_up(&mut area, &mut up, &mut control);
        }
        assert_eq!(viewport.pointer_capture_node_id(), None);
        assert!(!area.mouse_selecting);
    }

    #[test]
    fn cancel_pointer_interaction_stops_mouse_selection() {
        let mut area = TextArea::from_content("hello");
        area.mouse_selecting = true;

        assert!(EventTarget::cancel_pointer_interaction(&mut area));
        assert!(!area.mouse_selecting);
    }

    #[test]
    fn percent_width_uses_layout_override_without_mutating_measured_width() {
        let mut area = TextArea::from_content("123");
        area.set_style_width(Some(Length::percent(100.0)));
        area.set_multiline(false);

        area.measure(LayoutConstraints {
            max_width: 200.0,
            max_height: 40.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(40.0),
            viewport_height: 40.0,
        });
        assert_eq!(area.measured_size().0, 200.0);

        area.set_layout_width(80.0);
        area.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 40.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(40.0),
            viewport_height: 40.0,
        });
        assert_eq!(area.box_model_snapshot().width, 80.0);

        area.measure(LayoutConstraints {
            max_width: 200.0,
            max_height: 40.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(40.0),
            viewport_height: 40.0,
        });
        assert_eq!(area.measured_size().0, 200.0);
    }

    #[test]
    fn glyph_layout_scales_with_hidpi_factor() {
        let mut area = TextArea::from_content("abc");
        area.set_font_size(16.0);
        area.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 100.0,
            available_height: 40.0,
            viewport_width: 100.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(40.0),
            viewport_height: 40.0,
        });

        area.ensure_glyph_layout("abc", 1.0);
        let metrics_1x = area.glyph_buffer.metrics();
        let width_1x = area.glyph_cache_width;

        area.ensure_glyph_layout("abc", 2.0);
        let metrics_2x = area.glyph_buffer.metrics();
        let width_2x = area.glyph_cache_width;

        assert!((metrics_1x.font_size - 16.0).abs() < 0.01);
        assert!((metrics_2x.font_size - 32.0).abs() < 0.01);
        assert!((width_1x - 100.0).abs() < 0.01);
        assert!((width_2x - 200.0).abs() < 0.01);
    }

    #[test]
    fn auto_width_uses_glyph_measurement() {
        let mut area = TextArea::from_content("{{API_HOST}}");
        area.set_multiline(false);
        area.set_font_size(16.0);
        area.measure(LayoutConstraints {
            max_width: 500.0,
            max_height: 40.0,
            viewport_width: 500.0,
            percent_base_width: Some(500.0),
            percent_base_height: Some(40.0),
            viewport_height: 40.0,
        });

        let expected_width = TextArea::measure_render_text_run_with_style(
            "{{API_HOST}}",
            16.0,
            area.line_height,
            &area.font_families,
        )
        .0
        .round();
        assert_eq!(area.measured_size().0, expected_width);
    }
}
