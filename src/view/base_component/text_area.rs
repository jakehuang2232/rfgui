use crate::time::{Duration, Instant};
use crate::ui::PointerButton as UiPointerButton;
use crate::view::font_system::create_font_system;
use crate::view::frame_graph::FrameGraph;
use crate::view::render_pass::draw_rect_pass::{DrawRectInput, DrawRectOutput, RectPassParams};
use crate::view::render_pass::text_pass::{
    TextInput, TextOutput, TextPassFragment, TextPassParams,
};
use crate::view::render_pass::{DrawRectPass, TextPass};
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
use std::sync::Arc;

use crate::ui::Binding;
use crate::ui::PropValue;
use crate::ui::RsxNode;
use crate::view::promotion::PromotionNodeInfo;

use super::{
    BoxModelSnapshot, BuildState, Element, ElementTrait, EventTarget, LayoutConstraints,
    LayoutPlacement, Layoutable, Position, Renderable, Size, UiBuildContext,
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
}

struct TextAreaProjectionNode {
    range: Range<usize>,
    /// Key of the projection subtree root in the arena. Parented to the
    /// owning `TextArea` but detached (not in the TextArea's own
    /// `Node.children` list) — same pattern as Image loading/error slots.
    node: crate::view::node_arena::NodeKey,
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
    /// When `on_render` is set, the projection subtree needs to be rebuilt
    /// from the handler's RSX output; that rebuild requires `&mut
    /// NodeArena` which most setters don't carry. Flag here and rebuild
    /// during `place` / `build` when the arena is in scope.
    projection_tree_dirty: bool,
    render_fragments: Vec<TextAreaRenderFragment>,
    render_content_height: f32,
    cursor_char: usize,
    selection_anchor_char: Option<usize>,
    selection_focus_char: Option<usize>,
    pointer_selecting: bool,
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
    // 軌 A #7: per-prop explicit-tracking flags. See Text for the
    // cascade semantics. TextArea's setter surface is narrower
    // (no font_weight / text_wrap), so only 4 flags.
    font_family_explicit: bool,
    font_size_explicit: bool,
    color_explicit: bool,
    cursor_explicit: bool,
}

thread_local! {
    static SHARED_TEXT_AREA_FONT_SYSTEM: RefCell<FontSystem> = RefCell::new(create_font_system());
    static GLYPH_BUFFER_POOL: RefCell<Vec<GlyphBuffer>> = RefCell::new(Vec::new());
}

const GLYPH_BUFFER_POOL_MAX: usize = 8;

/// Take a `GlyphBuffer` from the thread-local pool, or create a new one.
fn take_pooled_glyph_buffer(font_system: &mut FontSystem, font_size: f32, line_height: f32) -> GlyphBuffer {
    let mut buffer = GLYPH_BUFFER_POOL.with(|pool| pool.borrow_mut().pop())
        .unwrap_or_else(|| GlyphBuffer::new(font_system, Metrics::new(font_size, font_size * line_height)));
    buffer.set_metrics(font_system, Metrics::new(font_size, font_size * line_height));
    buffer
}

/// Return a `GlyphBuffer` to the pool for reuse.
fn return_pooled_glyph_buffer(buffer: GlyphBuffer) {
    GLYPH_BUFFER_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < GLYPH_BUFFER_POOL_MAX {
            pool.push(buffer);
        }
        // else: drop the buffer, pool is full
    });
}

#[derive(Clone, Debug)]
struct VisualLine {
    start_char: usize,
    end_char: usize,
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
            projection_tree_dirty: false,
            render_fragments: Vec::new(),
            render_content_height: 0.0,
            cursor_char: 0,
            selection_anchor_char: None,
            selection_focus_char: None,
            pointer_selecting: false,
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
            font_family_explicit: false,
            font_size_explicit: false,
            color_explicit: false,
            cursor_explicit: false,
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
        self.mark_projection_tree_dirty();
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
                self.mark_projection_tree_dirty();
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
        self.mark_projection_tree_dirty();
    }

    /// Track 1 #10: incremental replace of the on_change handler list
    /// with a single prop-driven handler. Wipes existing entries so
    /// user-authored `on_change={...}` is not duplicated across
    /// renders (reconciler emits one Update per prop change).
    pub(crate) fn replace_on_change_handler(
        &mut self,
        handler: crate::ui::TextChangeHandlerProp,
    ) {
        self.on_change_handlers.clear();
        self.on_change_handlers.push(handler);
    }

    pub(crate) fn replace_on_focus_handler(
        &mut self,
        handler: crate::ui::TextAreaFocusHandlerProp,
    ) {
        self.on_focus_handlers.clear();
        self.on_focus_handlers.push(handler);
    }

    pub(crate) fn replace_on_render_handler(
        &mut self,
        handler: crate::ui::TextAreaRenderHandlerProp,
    ) {
        self.on_render_handler = Some(handler);
        self.mark_projection_tree_dirty();
    }

    pub(crate) fn replace_on_blur_handler(
        &mut self,
        handler: crate::ui::BlurHandlerProp,
    ) {
        // Forward to inner Element's blur handler list replacement.
        self.element.replace_on_blur_handler(handler);
    }

    fn mark_projection_tree_dirty(&mut self) {
        self.projection_tree_dirty = true;
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::ALL);
    }

    /// Arena-aware wrapper: if a rebuild was scheduled by a setter, do it
    /// now (needs the arena to commit new projection subtrees). Also keeps
    /// `render_fragments` in sync with the current ranges.
    pub(crate) fn rebuild_projection_tree_if_dirty(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        if !self.projection_tree_dirty {
            return;
        }
        self.projection_tree_dirty = false;
        self.rebuild_render_nodes(arena);
    }

    pub fn set_render_projection_nodes(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
        projections: Vec<(Range<usize>, crate::view::node_arena::NodeKey)>,
    ) {
        self.on_render_handler = None;
        // Remove any previously-held projection subtrees so we don't leak
        // detached nodes when the adapter installs a fresh set.
        for prev in self.render_nodes.drain(..) {
            arena.remove_subtree(prev.node);
        }
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
            // Synthetic change events emitted from setters do not have
            // arena context; null NodeKey signals "no target" on the event
            // metadata. Handlers must use the provided `value` field.
            meta: crate::ui::EventMeta::new(crate::view::node_arena::NodeKey::default()),
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
        self.color_explicit = true;
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
        self.font_family_explicit = true;
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
        self.font_family_explicit = true;
    }

    pub fn set_font_size(&mut self, font_size: f32) {
        if (self.font_size - font_size).abs() > f32::EPSILON {
            self.font_size = font_size;
            self.mark_measure_dirty();
            self.invalidate_glyph_layout();
        }
        self.font_size_explicit = true;
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
        self.cursor_explicit = true;
    }

    /// Track 1 #10 (正規): incremental replay of cold-path `style`
    /// fan-out on `<TextArea>`. Mirrors the style-handling block in
    /// `convert_text_area_element` (width/height/color/selection
    /// background) and resets explicit flags so removed declarations
    /// re-pick the ancestor cascade.
    pub(crate) fn apply_style_incremental(
        &mut self,
        style: Option<&Style>,
        inherited: &crate::view::renderer_adapter::InheritedTextStyle,
    ) {
        use crate::style::{ParsedValue, PropertyId};

        // Track 1 #10: do NOT pre-reset explicit flags (mirror the
        // `Text::apply_style_incremental` fix). Values sourced from
        // non-style props must survive a style-prop re-apply.

        let mut style_width: Option<Length> = None;
        let mut style_height: Option<Length> = None;
        if let Some(style) = style {
            if let Some(value) = style.get(PropertyId::Width) {
                style_width = match value {
                    ParsedValue::Length(l) => Some(*l),
                    _ => None,
                };
            }
            if let Some(value) = style.get(PropertyId::Height) {
                style_height = match value {
                    ParsedValue::Length(l) => Some(*l),
                    _ => None,
                };
            }
            if let Some(ParsedValue::Color(color)) = style.get(PropertyId::Color) {
                self.set_color(color.clone());
            }
            if let Some(selection) = style.selection()
                && let Some(background) = selection.background_color()
            {
                self.set_selection_background_color(background.clone());
            }
        }

        self.apply_inherited(inherited);
        self.set_style_width(style_width);
        self.set_style_height(style_height);
    }

    /// 軌 A #7: apply ancestor-derived inherited cascade to props the
    /// author didn't set explicitly. Returns `true` if anything
    /// changed. See `Text::apply_inherited` for the full contract.
    pub(crate) fn apply_inherited(
        &mut self,
        inherited: &crate::view::renderer_adapter::InheritedTextStyle,
    ) -> bool {
        let mut changed = false;
        if !self.font_family_explicit && !inherited.font_families.is_empty()
            && self.font_families != inherited.font_families
        {
            self.font_families = inherited.font_families.clone();
            self.mark_measure_dirty();
            self.invalidate_glyph_layout();
            changed = true;
        }
        if !self.font_size_explicit && let Some(fs) = inherited.font_size
            && (self.font_size - fs).abs() > f32::EPSILON
        {
            self.font_size = fs;
            self.mark_measure_dirty();
            self.invalidate_glyph_layout();
            changed = true;
        }
        if !self.color_explicit && let Some(color) = &inherited.color {
            self.color = Box::new(color.clone());
            self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::PAINT);
            changed = true;
        }
        if !self.cursor_explicit && let Some(cursor) = inherited.cursor {
            let mut style = Style::new();
            style.set_cursor(cursor);
            self.element.apply_style(style);
            changed = true;
        }
        changed
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
                self.size.width = resolved.max(0.0);
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
                self.size.height = resolved.max(0.0);
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
            self.mark_projection_tree_dirty();
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
        self.mark_projection_tree_dirty();
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
        self.mark_projection_tree_dirty();
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
        self.mark_projection_tree_dirty();
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
        self.mark_projection_tree_dirty();
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
        let hit_x = local_x.max(0.0);
        let hit_y = (local_y + self.scroll_y).max(0.0);
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

        let text = match &fragment.kind {
            TextAreaRenderFragmentKind::Text(text)
            | TextAreaRenderFragmentKind::Preedit(text) => text.as_str(),
            TextAreaRenderFragmentKind::Projection(_) => "",
        };
        if !text.is_empty() {
            let font_size = self.font_size;
            let line_height = self.line_height;
            let font_families = &self.font_families;
            let local_char = cursor_char.clamp(start, end).saturating_sub(start);
            let caret_byte = byte_index_at_char(text, local_char.min(text.chars().count()));
            let (cursor_line, cursor_index) = line_and_index_from_byte(text, caret_byte);
            let result = Self::with_pooled_text_buffer(
                text,
                font_size,
                line_height,
                font_families,
                |buffer, font_system| {
                    for affinity in [Affinity::Before, Affinity::After] {
                        let cursor =
                            Cursor::new_with_affinity(cursor_line, cursor_index, affinity);
                        let Some(layout_cursor) =
                            buffer.layout_cursor(font_system, cursor)
                        else {
                            continue;
                        };
                        if layout_cursor.line != cursor_line {
                            continue;
                        }
                        if let Some(run) = find_layout_run_by_line_layout(
                            buffer,
                            layout_cursor.line,
                            layout_cursor.layout,
                        ) && let Some(x) = caret_x_in_layout_run(cursor_index, &run)
                        {
                            return Some(x);
                        }
                    }
                    None
                },
            );
            if let Some(x) = result {
                return x;
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
        arena: &mut crate::view::node_arena::NodeArena,
        node_index: usize,
        fragment: &TextAreaRenderFragment,
        cursor_char: usize,
    ) -> Option<(f32, f32)> {
        let key = self.render_nodes.get(node_index)?.node;
        let fragment_start = fragment.source_range.start;
        let fragment_end = fragment.source_range.end;
        // Walk the arena subtree to find the first nested TextArea, then
        // compute its caret position. Use with_element_taken so we can
        // recurse arena children without holding a borrow across the call.
        find_first_text_area_with_arena(arena, key, |nested, nested_arena| {
            let local_char = cursor_char
                .clamp(fragment_start, fragment_end)
                .saturating_sub(fragment_start)
                .min(nested.content.chars().count());
            nested.caret_screen_position_for_char(nested_arena, local_char, false)
        })
        .flatten()
    }

    fn place_projection_fragments(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        // Clone fragment list out first so we can freely re-borrow arena.
        let fragments = self.render_fragments.clone();
        for fragment in &fragments {
            let TextAreaRenderFragmentKind::Projection(index) = fragment.kind else {
                continue;
            };
            let Some(node) = self.render_nodes.get(index) else {
                continue;
            };
            let key = node.node;
            let screen_x = self.layout_position.x + fragment.content_x;
            let screen_y = self.layout_position.y + fragment.content_y - self.scroll_y;
            let placement = LayoutPlacement {
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
            };
            arena.with_element_taken(key, |element, arena_ref| {
                element.place(placement, arena_ref);
            });
        }
    }

    fn projection_fragment_cursor_char_from_viewport_position(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
        node_index: usize,
        fragment: &TextAreaRenderFragment,
        viewport_x: f32,
        viewport_y: f32,
    ) -> Option<usize> {
        let key = self.render_nodes.get(node_index)?.node;
        let fragment_start = fragment.source_range.start;
        let fragment_end = fragment.source_range.end;
        find_first_text_area_with_arena(arena, key, |nested, _nested_arena| {
            let local_x = viewport_x - nested.layout_position.x;
            let local_y = viewport_y - nested.layout_position.y;
            nested.set_cursor_from_local_position(local_x, local_y);
            let local_char = nested.cursor_char.min(nested.content.chars().count());
            fragment_start + local_char.min(fragment_end.saturating_sub(fragment_start))
        })
    }

    fn caret_screen_position_for_char(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
        cursor_char: usize,
        require_focus: bool,
    ) -> Option<(f32, f32)> {
        if require_focus && !self.is_focused {
            return None;
        }
        if self.uses_projection_rendering() {
            if !self.ime_preedit.is_empty() {
                for fragment in &self.render_fragments {
                    if let TextAreaRenderFragmentKind::Preedit(text) = &fragment.kind
                        && cursor_char == fragment.source_range.start
                    {
                        return self
                            .preedit_fragment_caret_screen_position(fragment, text.as_str());
                    }
                }
            }
            for fragment_index in 0..self.render_fragments.len() {
                let fragment = self.render_fragments[fragment_index].clone();
                if cursor_char <= fragment.source_range.start {
                    return Some((
                        self.layout_position.x + fragment.content_x,
                        self.layout_position.y + fragment.content_y - self.scroll_y,
                    ));
                }
                if cursor_char <= fragment.source_range.end {
                    if let TextAreaRenderFragmentKind::Projection(index) = fragment.kind.clone()
                        && let Some(position) = self.projection_fragment_caret_screen_position(
                            arena,
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
                        self.layout_position.x + x,
                        self.layout_position.y + run.line_top - self.scroll_y,
                    ));
                }
            }
        }

        let fallback_y =
            fallback_line_top_for_cursor_line(&self.glyph_buffer, cursor_line).unwrap_or(0.0);
        Some((
            self.layout_position.x,
            self.layout_position.y + fallback_y - self.scroll_y,
        ))
    }

    fn caret_screen_position(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
    ) -> Option<(f32, f32)> {
        self.caret_screen_position_for_char(arena, self.cursor_char, true)
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
                    self.layout_position.x + left,
                    self.layout_position.y + run.line_top - self.scroll_y,
                ],
                [width, run.line_height.max(1.0)],
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

    fn rebuild_render_nodes(&mut self, arena: &mut crate::view::node_arena::NodeArena) {
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

        // Drop any previously-held projection subtrees so we don't leak
        // detached nodes across rebuilds.
        for prev in std::mem::take(&mut self.render_nodes) {
            arena.remove_subtree(prev.node);
        }

        // Parent key for newly-committed subtrees is `None`: we don't
        // know our own NodeKey here (TextArea stores only the legacy u64
        // id). The descriptor-side post-commit in the adapter parents
        // subtrees to the TextArea key at build time. For handler-driven
        // rebuilds we commit as arena-only detached roots (parent=None);
        // they still live inside the arena and get cleaned up by the
        // drain above on the next rebuild.
        let parent_key: Option<crate::view::node_arena::NodeKey> = None;

        let mut next_nodes: Vec<TextAreaProjectionNode> = Vec::with_capacity(projections.len());
        let mut next_fragments = Vec::new();

        for (index, projection) in projections.iter().enumerate() {
            let Ok(desc) = self.build_projection_root_desc(index, &projection.node) else {
                continue;
            };
            let key = crate::view::renderer_adapter::commit_descriptor_tree(
                arena,
                parent_key,
                desc,
            );
            apply_text_source_range(arena, key, projection.range.clone());
            next_nodes.push(TextAreaProjectionNode {
                range: projection.range.clone(),
                node: key,
            });
        }

        let mut cursor = 0_usize;
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

        self.render_nodes = next_nodes;
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

    fn build_projection_root_desc(
        &self,
        index: usize,
        node: &RsxNode,
    ) -> Result<crate::view::renderer_adapter::ElementDescriptor, String> {
        let scope = [self.stable_id(), 0x5445_5854, index as u64];
        let inherited_style = self.projection_inherited_style();
        let mut children = crate::view::renderer_adapter::rsx_to_descriptors_scoped_with_context(
            node,
            &scope,
            &inherited_style,
            0.0,
            0.0,
        )?;
        self.wrap_projection_children_desc(index, &mut children)
    }

    fn wrap_projection_children_desc(
        &self,
        index: usize,
        children: &mut Vec<crate::view::renderer_adapter::ElementDescriptor>,
    ) -> Result<crate::view::renderer_adapter::ElementDescriptor, String> {
        if children.is_empty() {
            return Err("projection produced no elements".to_string());
        }
        if children.len() == 1 {
            return Ok(children.remove(0));
        }

        let wrapper_id = self
            .stable_id()
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

        Ok(crate::view::renderer_adapter::ElementDescriptor {
            element: Box::new(wrapper) as Box<dyn ElementTrait>,
            children: std::mem::take(children),
            post_commit: None,
        })
    }

    fn sync_projection_preedit_state(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let cursor_char = self.cursor_char.min(self.content.chars().count());
        let ime_preedit = self.ime_preedit.clone();
        let ime_preedit_cursor = self.ime_preedit_cursor;
        let ranges: Vec<(crate::view::node_arena::NodeKey, Range<usize>)> = self
            .render_nodes
            .iter()
            .map(|p| (p.node, p.range.clone()))
            .collect();
        for (key, range) in ranges {
            let ime = ime_preedit.clone();
            find_first_text_area_with_arena(arena, key, |nested, _nested_arena| {
                let local_cursor = cursor_char
                    .saturating_sub(range.start)
                    .min(nested.content.chars().count());
                nested.cursor_char = local_cursor;
                if !ime.is_empty()
                    && cursor_char >= range.start
                    && cursor_char <= range.end
                {
                    nested.set_preedit(ime.clone(), ime_preedit_cursor);
                } else {
                    nested.clear_preedit();
                }
            });
        }
    }

    fn sync_projection_selection_state(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let selection = self.selection_range_chars();
        let selection_color = self.selection_background_color.to_rgba_f32();
        let ranges: Vec<(crate::view::node_arena::NodeKey, Range<usize>)> = self
            .render_nodes
            .iter()
            .map(|p| (p.node, p.range.clone()))
            .collect();
        for (key, range) in ranges {
            find_first_text_area_with_arena(arena, key, |nested, _nested_arena| {
                nested.set_selection_background_color(crate::Color::rgba(
                    (selection_color[0] * 255.0).round().clamp(0.0, 255.0) as u8,
                    (selection_color[1] * 255.0).round().clamp(0.0, 255.0) as u8,
                    (selection_color[2] * 255.0).round().clamp(0.0, 255.0) as u8,
                    (selection_color[3] * 255.0).round().clamp(0.0, 255.0) as u8,
                ));
                let Some((start, end)) = selection else {
                    nested.clear_selection();
                    return;
                };
                let overlap_start = start.max(range.start);
                let overlap_end = end.min(range.end);
                if overlap_start >= overlap_end {
                    nested.clear_selection();
                    return;
                }
                nested.selection_anchor_char =
                    Some(overlap_start.saturating_sub(range.start));
                nested.selection_focus_char =
                    Some(overlap_end.saturating_sub(range.start));
            });
        }
    }

    fn layout_render_fragments(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        if self.render_fragments.is_empty() {
            self.render_content_height = 0.0;
            return;
        }
        self.sync_projection_preedit_state(arena);
        self.sync_projection_selection_state(arena);
        let available_width = self.effective_width().max(1.0);
        let line_height_px = self.line_height_px();
        let effective_height = self.effective_height().max(line_height_px);
        let font_size = self.font_size;
        let line_height_ratio = self.line_height;
        let font_families = self.font_families.clone();
        let mut cursor_x = 0.0_f32;
        let mut cursor_y = 0.0_f32;
        let mut line_height = line_height_px;

        for fragment in &mut self.render_fragments {
            let (fragment_width, fragment_height) = match &fragment.kind {
                TextAreaRenderFragmentKind::Text(text)
                | TextAreaRenderFragmentKind::Preedit(text) => {
                    let measured = Self::measure_text_with_pool(
                        text.as_str(),
                        font_size,
                        line_height_ratio,
                        &font_families,
                    );
                    (measured.0, measured.1)
                }
                TextAreaRenderFragmentKind::Projection(index) => {
                    let Some(node) = self.render_nodes.get(*index) else {
                        continue;
                    };
                    let key = node.node;
                    let constraints = LayoutConstraints {
                        max_width: available_width,
                        max_height: effective_height,
                        viewport_width,
                        viewport_height,
                        percent_base_width: Some(available_width),
                        percent_base_height: Some(effective_height),
                    };
                    let measured = arena
                        .with_element_taken(key, |element, arena_ref| {
                            element.measure(constraints, arena_ref);
                            element.measured_size()
                        })
                        .unwrap_or((0.0, 0.0));
                    (measured.0.max(1.0), measured.1.max(line_height_px))
                }
            };

            if self.multiline && cursor_x > 0.0 && cursor_x + fragment_width > available_width {
                cursor_x = 0.0;
                cursor_y += line_height;
                line_height = line_height_px;
            }

            fragment.content_x = cursor_x;
            fragment.content_y = cursor_y;
            fragment.width = fragment_width.max(1.0);
            fragment.height = fragment_height.max(line_height_px);
            cursor_x += fragment_width;
            line_height = line_height.max(fragment_height);
        }

        self.render_content_height = (cursor_y + line_height).max(line_height_px);
    }

    fn set_cursor_from_projection_position(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
        viewport_x: f32,
        viewport_y: f32,
    ) -> bool {
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
                        arena, index, &fragment, viewport_x, viewport_y,
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

    fn ime_preedit_underline_rects(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
        composed: &str,
    ) -> Vec<([f32; 2], [f32; 2])> {
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
                        let Some(node) = self.render_nodes.get(index) else {
                            continue;
                        };
                        let key = node.node;
                        let nested_rects = find_first_text_area_with_arena(
                            arena,
                            key,
                            |nested_self, nested_arena| {
                                let nested_composed = nested_self.composed_text();
                                nested_self.ime_preedit_underline_rects(
                                    nested_arena,
                                    nested_composed.as_str(),
                                )
                            },
                        );
                        if let Some(nested_rects) = nested_rects {
                            rects.extend(nested_rects);
                        }
                    }
                    TextAreaRenderFragmentKind::Text(_) => {}
                }
            }
            let _ = end_char;
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
        let width = self.effective_width();
        let font_size = self.font_size.max(1.0);
        let line_height_px = (self.font_size * self.line_height.max(0.8)).max(1.0);
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
                Some(Align::Left),
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

    /// Measure text dimensions using a pooled GlyphBuffer (no allocation if pool non-empty).
    fn measure_text_with_pool(
        text: &str,
        font_size: f32,
        line_height: f32,
        font_families: &[String],
    ) -> (f32, f32) {
        Self::with_shared_font_system(|font_system| {
            let mut buffer = take_pooled_glyph_buffer(font_system, font_size, line_height);
            let content = if text.is_empty() { " " } else { text };
            let attrs = if let Some(first) = font_families.first() {
                Attrs::new().family(Family::Name(first.as_str()))
            } else {
                Attrs::new()
            };
            buffer.set_wrap(font_system, Wrap::WordOrGlyph);
            buffer.set_size(font_system, None, None);
            buffer.set_text(font_system, content, &attrs, Shaping::Advanced, Some(Align::Left));
            buffer.shape_until_scroll(font_system, false);
            let measured = measure_buffer_size(&buffer);
            return_pooled_glyph_buffer(buffer);
            measured
        })
    }

    /// Build a temporary GlyphBuffer from pool for hit testing, returning it after use.
    fn with_pooled_text_buffer<R>(
        text: &str,
        font_size: f32,
        line_height: f32,
        font_families: &[String],
        f: impl FnOnce(&mut GlyphBuffer, &mut FontSystem) -> R,
    ) -> R {
        Self::with_shared_font_system(|font_system| {
            let mut buffer = take_pooled_glyph_buffer(font_system, font_size, line_height);
            let content = if text.is_empty() { " " } else { text };
            let attrs = if let Some(first) = font_families.first() {
                Attrs::new().family(Family::Name(first.as_str()))
            } else {
                Attrs::new()
            };
            buffer.set_wrap(font_system, Wrap::WordOrGlyph);
            buffer.set_size(font_system, None, None);
            buffer.set_text(font_system, content, &attrs, Shaping::Advanced, Some(Align::Left));
            buffer.shape_until_scroll(font_system, false);
            let result = f(&mut buffer, font_system);
            return_pooled_glyph_buffer(buffer);
            result
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
        use crate::platform::input::Key;
        let key = event.key.key;
        let modifiers = event.key.modifiers;
        let shift = modifiers.shift();
        let shortcut = modifiers.ctrl() || modifiers.meta();

        if key == Key::ArrowLeft {
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
        if key == Key::ArrowRight {
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
        if key == Key::ArrowUp {
            let previous = self.cursor_char;
            let moved = self.move_cursor_vertical(Motion::Up);
            if moved {
                self.update_shift_selection_after_move(previous, shift);
            }
            return moved;
        }
        if key == Key::ArrowDown {
            let previous = self.cursor_char;
            let moved = self.move_cursor_vertical(Motion::Down);
            if moved {
                self.update_shift_selection_after_move(previous, shift);
            }
            return moved;
        }
        self.clear_vertical_goal();
        if key == Key::Home {
            if self.cursor_char == 0 {
                return false;
            }
            let previous = self.cursor_char;
            self.cursor_char = 0;
            self.update_shift_selection_after_move(previous, shift);
            return true;
        }
        if key == Key::End {
            let end = self.content.chars().count();
            if self.cursor_char == end {
                return false;
            }
            let previous = self.cursor_char;
            self.cursor_char = end;
            self.update_shift_selection_after_move(previous, shift);
            return true;
        }
        if shortcut && key == Key::KeyA {
            let end = self.content.chars().count();
            self.selection_anchor_char = Some(0);
            self.selection_focus_char = Some(end);
            self.cursor_char = end;
            self.reset_caret_blink();
            self.clear_vertical_goal();
            return true;
        }
        if shortcut && key == Key::KeyC {
            if let Some(selected) = self.selected_text() {
                control.set_clipboard_text(selected);
                return true;
            }
            return false;
        }
        if shortcut && key == Key::KeyX {
            if self.read_only {
                return false;
            }
            if let Some(selected) = self.selected_text() {
                control.set_clipboard_text(selected);
                return self.delete_selected_text();
            }
            return false;
        }
        if shortcut && key == Key::KeyV {
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

        if key == Key::Backspace {
            return self.delete_backspace();
        }
        if key == Key::Delete {
            return self.delete_forward();
        }
        if key == Key::Enter || key == Key::NumberPadEnter {
            if self.multiline {
                return self.insert_text("\n");
            }
            return false;
        }
        if key == Key::Tab {
            return self.insert_text("    ");
        }

        if shortcut || modifiers.alt() {
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

fn find_layout_run_by_line_layout(
    buffer: &'_ GlyphBuffer,
    target_line: usize,
    target_layout: usize,
) -> Option<cosmic_text::LayoutRun<'_>> {
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
    fn stable_id(&self) -> u64 {
        self.element.stable_id()
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.element.stable_id(),
            parent_id: self.element.parent_id(),
            x: self.layout_position.x,
            y: self.layout_position.y,
            width: self.layout_size.width,
            height: self.layout_size.height,
            border_radius: 0.0,
            should_render: self.should_render,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    // Phase B: snapshot_state / restore_state removed (see ElementTrait def).

    fn promotion_node_info(&self) -> PromotionNodeInfo {
        PromotionNodeInfo {
            estimated_pass_count: 3,
            opacity: self.opacity,
            ..Default::default()
        }
    }

    fn has_active_animator(&self) -> bool {
        self.element.has_active_animator()
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
    fn dispatch_pointer_down(
        &mut self,
        event: &mut crate::ui::PointerDownEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
        arena: &mut crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
    ) {
        control.set_focus(Some(self_key));
        self.is_focused = true;
        self.clear_preedit();
        self.reset_caret_blink();
        let previous = self.cursor_char;
        if !self.set_cursor_from_projection_position(
            arena,
            event.pointer.viewport_x,
            event.pointer.viewport_y,
        ) {
            self.set_cursor_from_local_position(event.pointer.local_x, event.pointer.local_y);
        }
        if event.pointer.modifiers.shift() {
            let anchor = self.selection_anchor_char.unwrap_or(previous);
            self.selection_anchor_char = Some(anchor);
            self.selection_focus_char = Some(self.cursor_char);
        } else {
            self.selection_anchor_char = Some(self.cursor_char);
            self.selection_focus_char = Some(self.cursor_char);
        }
        self.pointer_selecting = event.pointer.button == Some(UiPointerButton::Left);
        if self.pointer_selecting {
            control.set_pointer_capture(self_key);
        }
        self.element.dispatch_pointer_down(event, control, arena, self_key);
        event.meta.stop_propagation();
        control.request_redraw();
    }

    fn dispatch_pointer_up(
        &mut self,
        event: &mut crate::ui::PointerUpEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
        arena: &mut crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
    ) {
        if event.pointer.button == Some(UiPointerButton::Left) {
            self.pointer_selecting = false;
            control.release_pointer_capture(self_key);
            if self.selection_anchor_char == self.selection_focus_char {
                self.clear_selection();
            }
            control.request_redraw();
        }
        self.element.dispatch_pointer_up(event, control, arena, self_key);
    }

    fn dispatch_pointer_move(
        &mut self,
        event: &mut crate::ui::PointerMoveEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
        arena: &mut crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
    ) {
        if self.pointer_selecting && event.pointer.buttons.left {
            if !self.set_cursor_from_projection_position(
                arena,
                event.pointer.viewport_x,
                event.pointer.viewport_y,
            ) {
                self.set_cursor_from_local_position(event.pointer.local_x, event.pointer.local_y);
            }
            if self.selection_anchor_char.is_none() {
                self.selection_anchor_char = Some(self.cursor_char);
            }
            self.selection_focus_char = Some(self.cursor_char);
            event.meta.stop_propagation();
            control.request_redraw();
        }
        self.element.dispatch_pointer_move(event, control, arena, self_key);
    }

    fn dispatch_click(
        &mut self,
        event: &mut crate::ui::ClickEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
        arena: &mut crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
    ) {
        self.element.dispatch_click(event, control, arena, self_key);
    }

    fn dispatch_key_down(
        &mut self,
        event: &mut crate::ui::KeyDownEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
        arena: &mut crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
    ) {
        // Keep keydown handling in IME while composing to avoid mutating committed text.
        if !self.ime_preedit.is_empty() {
            self.element.dispatch_key_down(event, control, arena, self_key);
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
        self.element.dispatch_key_down(event, control, arena, self_key);
    }

    fn dispatch_key_up(
        &mut self,
        event: &mut crate::ui::KeyUpEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
        arena: &mut crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
    ) {
        self.element.dispatch_key_up(event, control, arena, self_key);
    }

    fn dispatch_text_input(
        &mut self,
        event: &mut crate::ui::TextInputEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
        _arena: &mut crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
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
        _arena: &mut crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
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
        arena: &mut crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
    ) {
        self.is_focused = true;
        self.reset_caret_blink();
        if !self.on_focus_handlers.is_empty() {
            let mut focus_event = crate::ui::TextAreaFocusEvent {
                meta: event.meta.clone(),
                target: event.meta.text_selection_target(self_key),
            };
            for handler in &self.on_focus_handlers {
                handler.call(&mut focus_event);
            }
        }
        self.element.dispatch_focus(event, control, arena, self_key);
    }

    fn dispatch_blur(
        &mut self,
        event: &mut crate::ui::BlurEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
        arena: &mut crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
    ) {
        let had_selection = self.selection_range_chars().is_some();
        self.is_focused = false;
        self.pointer_selecting = false;
        self.clear_selection();
        self.cached_ime_cursor_rect = None;
        self.glyph_layout_valid = false;
        self.clear_preedit();
        if had_selection {
            control.request_redraw();
        }
        self.element.dispatch_blur(event, control, arena, self_key);
    }

    fn cancel_pointer_interaction(&mut self) -> bool {
        let was_selecting = self.pointer_selecting;
        self.pointer_selecting = false;
        was_selecting || self.element.cancel_pointer_interaction()
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

    fn cursor(&self) -> crate::Cursor {
        crate::Cursor::Text
    }

    fn wants_animation_frame(&self) -> bool {
        self.is_focused
    }
}

impl Layoutable for TextArea {
    fn measure(
        &mut self,
        constraints: LayoutConstraints,
        _arena: &mut crate::view::node_arena::NodeArena,
    ) {
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
            self.size.width = intrinsic_width.min(available).max(0.0);
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

            self.size.height = (line_px * resolved_lines as f32).max(1.0);
            self.element.set_height(self.size.height);
        }
        self.dirty_flags = self.dirty_flags.without(super::DirtyFlags::LAYOUT);
    }

    fn place(
        &mut self,
        placement: LayoutPlacement,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
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
            width: layout_width.max(0.0).min(max_width),
            height: layout_height.max(0.0).min(max_height),
        };
        self.layout_position = Position {
            x: placement.parent_x + self.position.x + placement.visual_offset_x,
            y: placement.parent_y + self.position.y + placement.visual_offset_y,
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
        self.rebuild_projection_tree_if_dirty(arena);
        if !self.render_nodes.is_empty() || self.on_render_handler.is_some() {
            self.layout_render_fragments(
                placement.viewport_width,
                placement.viewport_height,
                arena,
            );
        }
        self.clamp_scroll();
        self.last_layout_placement = Some(placement);
        self.dirty_flags = self.dirty_flags.without(
            super::DirtyFlags::PLACE
                .union(super::DirtyFlags::BOX_MODEL)
                .union(super::DirtyFlags::HIT_TEST),
        );
    }

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

    fn flex_props(&self) -> crate::view::base_component::FlexProps {
        let (measured_w, measured_h) = self.measured_size();
        let base = self.element.flex_props();
        crate::view::base_component::FlexProps {
            width: if self.auto_width { crate::SizeValue::Auto } else { base.width },
            height: if self.auto_height { crate::SizeValue::Auto } else { base.height },
            allows_cross_stretch_when_row: self.auto_height,
            allows_cross_stretch_when_col: self.auto_width,
            intrinsic_width: Some(measured_w),
            intrinsic_height: Some(measured_h),
            intrinsic_feeds_auto_min: true,
            intrinsic_feeds_auto_base: false,
            ..base
        }
    }

    fn set_layout_offset(&mut self, x: f32, y: f32) {
        self.position = Position { x, y };
        self.element.set_position(x, y);
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::RUNTIME);
    }
}

impl Renderable for TextArea {
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        if !self.should_render {
            return ctx.into_state();
        }

        let opacity = if ctx.is_node_promoted(self.stable_id()) {
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
                arena,
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
                let screen_x = self.layout_position.x + fragment.content_x;
                let screen_y = self.layout_position.y + fragment.content_y - self.scroll_y;
                match &fragment.kind {
                    TextAreaRenderFragmentKind::Text(text)
                    | TextAreaRenderFragmentKind::Preedit(text) => {
                        push_text_pass_explicit(
                            graph,
                            &mut ctx,
                            TextPassParams::single_fragment(
                                TextPassFragment {
                                    content: text.clone(),
                                    x: screen_x,
                                    y: screen_y,
                                    width: fragment.width.max(1.0),
                                    height: fragment.height.max(1.0),
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
                        let Some(node) = self.render_nodes.get(*index) else {
                            continue;
                        };
                        let key = node.node;
                        let viewport = ctx.viewport();
                        // Move the full ctx into the closure; the arena
                        // side runs build() and returns the resulting
                        // BuildState which we re-wrap. If the slot is
                        // missing, preserve the current ctx state as a
                        // best-effort fallback.
                        let state_before = ctx.state_clone();
                        let moved_ctx = std::mem::replace(
                            &mut ctx,
                            UiBuildContext::from_parts(viewport.clone(), state_before.clone()),
                        );
                        let next_state_opt = arena.with_element_taken(key, |element, arena_ref| {
                            element.build(graph, arena_ref, moved_ctx)
                        });
                        let next_state = next_state_opt.unwrap_or(state_before);
                        ctx = UiBuildContext::from_parts(viewport, next_state);
                    }
                }
            }

            let ime_underline_rects = self.ime_preedit_underline_rects(arena, content.as_str());
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

            if let Some((caret_x, caret_y)) = self.caret_screen_position(arena) {
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
            let ime_underline_rects = self.ime_preedit_underline_rects(arena, content.as_str());

            push_text_pass_explicit(
                graph,
                &mut ctx,
                TextPassParams::single_fragment(
                    TextPassFragment {
                        content,
                        x: self.layout_position.x,
                        y: self.layout_position.y - self.scroll_y,
                        width: self.layout_size.width,
                        height: self.layout_size.height.max(self.content_height()),
                        color,
                        opacity,
                        layout_buffer: Some(Arc::new(self.glyph_buffer.clone())),
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

        if let Some((caret_x, caret_y)) = self.caret_screen_position(arena) {
            self.cached_ime_cursor_rect = Some((caret_x, caret_y, 1.0, self.line_height_px()));
            if !self.should_draw_caret() {
                let keys: Vec<crate::view::node_arena::NodeKey> =
                    self.render_nodes.iter().map(|p| p.node).collect();
                for key in keys {
                    let viewport = ctx.viewport();
                    let state_before = ctx.state_clone();
                    let moved_ctx = std::mem::replace(
                        &mut ctx,
                        UiBuildContext::from_parts(viewport.clone(), state_before.clone()),
                    );
                    let next_state_opt = arena.with_element_taken(key, |element, arena_ref| {
                        element.build(graph, arena_ref, moved_ctx)
                    });
                    let next_state = next_state_opt.unwrap_or(state_before);
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
        let keys: Vec<crate::view::node_arena::NodeKey> =
            self.render_nodes.iter().map(|p| p.node).collect();
        for key in keys {
            let viewport = ctx.viewport();
            let state_before = ctx.state_clone();
            let moved_ctx = std::mem::replace(
                &mut ctx,
                UiBuildContext::from_parts(viewport.clone(), state_before.clone()),
            );
            let next_state_opt = arena.with_element_taken(key, |element, arena_ref| {
                element.build(graph, arena_ref, moved_ctx)
            });
            let next_state = next_state_opt.unwrap_or(state_before);
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
            let element = std::rc::Rc::make_mut(element);
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

fn set_rsx_element_prop(element: &mut crate::ui::RsxElementNode, key: &'static str, value: PropValue) {
    let props = std::rc::Rc::make_mut(&mut element.props);
    if let Some((_, prop_value)) = props
        .iter_mut()
        .rev()
        .find(|(prop_key, _)| *prop_key == key)
    {
        *prop_value = value;
        return;
    }
    props.push((key, value));
}

/// Walk the arena subtree rooted at `key`, find the first `TextArea`,
/// run `f` against it, and return the result. Uses `with_element_taken`
/// for the match so the closure has exclusive `&mut TextArea` plus an
/// unaliased `&mut NodeArena` for any inner lookups. Follows the
/// FP-style recursion rule: clone `children_of` before iterating so no
/// `Ref` is held across the recursive call.
fn find_first_text_area_with_arena<'a, R: 'a>(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    f: impl FnOnce(&mut TextArea, &mut crate::view::node_arena::NodeArena) -> R + 'a,
) -> Option<R> {
    // Box the FnOnce into an Option so the recursive helper can
    // `.take()` it exactly once at the first match.
    let mut slot: Option<
        Box<dyn FnOnce(&mut TextArea, &mut crate::view::node_arena::NodeArena) -> R + 'a>,
    > = Some(Box::new(f));
    recurse_find_first_text_area(arena, key, &mut slot)
}

fn recurse_find_first_text_area<'a, R>(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    slot: &mut Option<
        Box<dyn FnOnce(&mut TextArea, &mut crate::view::node_arena::NodeArena) -> R + 'a>,
    >,
) -> Option<R> {
    let is_match = arena
        .get(key)
        .map(|node| node.element.as_any().is::<TextArea>())
        .unwrap_or(false);
    if is_match {
        let Some(callback) = slot.take() else {
            return None;
        };
        return arena
            .with_element_taken(key, |element, arena_ref| {
                element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .map(|ta| callback(ta, arena_ref))
            })
            .flatten();
    }
    // Clone the child list before iterating — never hold a borrow across
    // the recursive call (session 3 FP rule).
    let children = arena.children_of(key);
    for child in children {
        if let Some(result) = recurse_find_first_text_area(arena, child, slot) {
            return Some(result);
        }
        if slot.is_none() {
            return None;
        }
    }
    None
}

/// Pub wrapper for the renderer adapter: walk an arena subtree under
/// `key` and stamp `range` onto every nested TextArea as
/// `source_text_range`.
pub fn apply_source_range_to_subtree(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    range: Range<usize>,
) {
    apply_text_source_range(arena, key, range);
}

/// Walk the arena subtree rooted at `key` and apply `source_text_range`
/// to every `TextArea` found. FP-style: children cloned before
/// recursion; mutation goes through `with_element_taken`.
fn apply_text_source_range(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    range: Range<usize>,
) {
    let _ = arena.with_element_taken(key, |element, _arena_ref| {
        if let Some(text_area) = element.as_any_mut().downcast_mut::<TextArea>() {
            text_area.set_source_text_range(Some(range.clone()));
        }
    });
    // Recurse using arena Node.children as the source of truth.
    let children = arena.children_of(key);
    for child in children {
        apply_text_source_range(arena, child, range.clone());
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
    use crate::ui::{Binding, BlurEvent, EventMeta, FocusEvent, Modifiers, NodeId, PointerButton, PointerButtons, PointerDownEvent, PointerEventData, TextInputEvent, EventCommand, rsx};
    use crate::view::base_component::{
        DirtyFlags, Element, ElementTrait, EventTarget, LayoutConstraints, LayoutPlacement,
        Layoutable, dispatch_pointer_down_from_hit_test, select_all_text_by_id,
        select_text_range_by_id,
    };
    use crate::view::{Viewport, ViewportControl};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Instant;
    use crate::platform::PointerType;

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

    // Arena-aware helper: drive the projection-tree rebuild that setters
    // schedule lazily. Used by the projection subtree tests below.
    fn flush_projection_tree(area: &mut TextArea) -> crate::view::node_arena::NodeArena {
        let mut arena = crate::view::node_arena::NodeArena::new();
        area.rebuild_projection_tree_if_dirty(&mut arena);
        arena
    }

    #[test]
    fn on_render_rebuilds_projection_only_when_content_changes() {
        let calls = Rc::new(RefCell::new(0usize));
        let calls_for_render = calls.clone();
        let mut area = TextArea::from_content("hello");
        area.on_render(move |render| {
            *calls_for_render.borrow_mut() += 1;
            render.range(1..4, |text_area_node| {
                rsx! {
                    <crate::view::Element>
                        {text_area_node}
                    </crate::view::Element>
                }
            });
        });

        let mut arena = flush_projection_tree(&mut area);
        assert_eq!(*calls.borrow(), 1);
        area.set_text("hello");
        area.rebuild_projection_tree_if_dirty(&mut arena);
        assert_eq!(*calls.borrow(), 1);
        area.set_text("world");
        area.rebuild_projection_tree_if_dirty(&mut arena);
        assert_eq!(*calls.borrow(), 2);

        assert_eq!(area.render_nodes.len(), 1);
        let projection_key = area.render_nodes[0].node;
        // The rsx `<Element>{text_area_node}</Element>` yields a single
        // top-level Element which `wrap_projection_children_desc` keeps
        // as-is (single-child unwrap). The TextArea with the source range
        // is the nested child.
        let wrapper_guard = arena.get(projection_key).expect("projection root");
        let child_keys = wrapper_guard.children.clone();
        drop(wrapper_guard);
        assert_eq!(child_keys.len(), 1);
        let nested_guard = arena.get(child_keys[0]).expect("nested text area");
        let source_range = nested_guard
            .element
            .as_any()
            .downcast_ref::<TextArea>()
            .expect("nested projection text area")
            .source_text_range();
        assert_eq!(source_range, Some(1..4));
    }

    #[test]
    fn on_render_fragment_projection_wraps_multiple_siblings() {
        let mut area = TextArea::from_content("{{x}}");
        area.on_render(|render| {
            render.range(0..5, |text_area_node| {
                rsx! {
                    <crate::view::Text>abc</crate::view::Text>
                    {text_area_node}
                }
            });
        });

        let arena = flush_projection_tree(&mut area);
        assert_eq!(area.render_nodes.len(), 1);
        let projection_key = area.render_nodes[0].node;
        let wrapper = arena.get(projection_key).expect("projection wrapper");
        assert!(wrapper.element.as_any().is::<Element>());
        let child_keys = wrapper.children.clone();
        drop(wrapper);
        assert_eq!(child_keys.len(), 2);
        assert!(child_keys.iter().any(|k| arena
            .get(*k)
            .map(|n| n.element.as_any().is::<TextArea>())
            .unwrap_or(false)));
    }

    #[test]
    fn on_render_single_text_area_projection_inherits_text_style() {
        let mut area = TextArea::from_content("{{x}}");
        area.set_font_size(13.0);
        area.set_color(crate::Color::hex("#aabbcc"));
        area.on_render(|render| {
            render.range(0..5, |text_area_node| text_area_node);
        });

        let arena = flush_projection_tree(&mut area);
        assert_eq!(area.render_nodes.len(), 1);
        let projection_key = area.render_nodes[0].node;
        let nested_guard = arena.get(projection_key).expect("projection root");
        let nested = nested_guard
            .element
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
                rsx! {
                    <crate::view::Element>
                        {text_area_node}
                    </crate::view::Element>
                }
            });
            render.range(4..6, |text_area_node| {
                rsx! {
                    <crate::view::Element>
                        {text_area_node}
                    </crate::view::Element>
                }
            });
        });

        let arena = flush_projection_tree(&mut area);
        let ranges = area
            .render_nodes
            .iter()
            .map(|projection| projection.range.clone())
            .collect::<Vec<_>>();
        assert_eq!(ranges, vec![2..4, 4..6, 6..8]);

        let nested_contents: Vec<String> = area
            .render_nodes
            .iter()
            .map(|projection| {
                let wrapper_guard = arena.get(projection.node).expect("projection wrapper");
                let child_keys = wrapper_guard.children.clone();
                drop(wrapper_guard);
                let nested_key = child_keys[0];
                let nested_guard = arena.get(nested_key).expect("nested node");
                nested_guard
                    .element
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .expect("nested projection text area")
                    .content
                    .clone()
            })
            .collect();
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

        // Arena-era path: legacy `rsx_to_elements` drops projection
        // subtrees (see `convert_text_area_element`). The descriptor
        // pipeline is what populates `render_nodes` now.
        use crate::style::Style;
        use crate::view::renderer_adapter::{commit_descriptor_tree, rsx_to_descriptors_with_context};
        let (descs, errors) = rsx_to_descriptors_with_context(&tree, &Style::new(), 0.0, 0.0);
        assert!(errors.is_empty(), "rsx conversion errors: {:?}", errors);
        assert_eq!(descs.len(), 1);
        let mut arena = crate::view::node_arena::NodeArena::new();
        let mut descs = descs;
        let key = commit_descriptor_tree(&mut arena, None, descs.remove(0));
        let node = arena.get(key).unwrap();
        let area = node.element.as_any().downcast_ref::<TextArea>().expect("host textarea");
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
        let _arena = flush_projection_tree(&mut area);
        area.set_preedit("中文".to_string(), None);

        assert!(area.uses_projection_rendering());
        assert!(area.render_fragments.iter().any(|fragment| matches!(
            &fragment.kind,
            TextAreaRenderFragmentKind::Preedit(text) if text == "中文"
        )));
    }

    #[test]
    fn ime_preedit_outside_projection_updates_caret_position() {
        let mut area = TextArea::from_content("ab {{x}}");
        area.on_render(|render| {
            render.range(3..8, |text_area_node| text_area_node);
        });
        area.is_focused = true;
        area.cursor_char = 1;
        let mut arena = crate::view::node_arena::NodeArena::new();
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
        }, &mut arena);

        area.set_preedit("中文".to_string(), Some((0, 0)));
        area.layout_render_fragments(240.0, 64.0, &mut arena);
        let caret_at_start = area
            .caret_screen_position(&mut arena)
            .expect("caret position at preedit start");

        area.set_preedit("中文".to_string(), Some(("中文".len(), "中文".len())));
        area.layout_render_fragments(240.0, 64.0, &mut arena);
        let caret_at_end = area
            .caret_screen_position(&mut arena)
            .expect("caret position at preedit end");

        assert!(caret_at_end.0 > caret_at_start.0);
    }

    #[test]
    fn ime_preedit_inside_projection_syncs_to_nested_text_area() {
        let mut area = TextArea::from_content("{{x}}");
        area.on_render(|render| {
            render.range(0..5, |text_area_node| text_area_node);
        });
        area.cursor_char = 2;
        area.set_preedit("中文".to_string(), None);
        let mut arena = flush_projection_tree(&mut area);
        area.layout_render_fragments(240.0, 64.0, &mut arena);

        let nested_key = area.render_nodes[0].node;
        let nested_guard = arena.get(nested_key).expect("nested projection node");
        let nested = nested_guard
            .element
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
        let _arena = flush_projection_tree(&mut area);
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
        use crate::view::test_support::{commit_element, new_test_arena};
        let mut area = TextArea::from_content("hello");
        area.on_render(|render| {
            render.range(1..4, |text_area_node| {
                rsx! {
                    <crate::view::Element style={{ width: Length::percent(100.0), height: Length::percent(100.0) }}>
                        {text_area_node}
                    </crate::view::Element>
                }
            });
        });
        let mut arena = new_test_arena();
        let area_key = commit_element(&mut arena, Box::new(area));

        arena.with_element_taken(area_key, |el, a| {
            el.place(LayoutPlacement {
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
            }, a);
        });

        let (click_x, click_y) = {
            let node = arena.get(area_key).unwrap();
            let area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
            let fragment = area
                .render_fragments
                .iter()
                .find(|fragment| matches!(fragment.kind, TextAreaRenderFragmentKind::Projection(_)))
                .expect("projection fragment");
            (
                area.layout_position.x + fragment.content_x + fragment.width * 0.8,
                area.layout_position.y + fragment.content_y + fragment.height * 0.5,
            )
        };

        let mut viewport = Viewport::new();
        let mut control = ViewportControl::new(&mut viewport);
        let meta = EventMeta::new(NodeId::default());
        let viewport_api = meta.viewport();
        let mut event = PointerDownEvent {
            meta,
            pointer: PointerEventData {
                viewport_x: click_x,
                viewport_y: click_y,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(PointerButton::Left),
                buttons: PointerButtons::default(),
                modifiers: Default::default(),
                pointer_id: 0,
                pointer_type: PointerType::Mouse,
                pressure: 0.0,
                timestamp: Instant::now(),
            },
            viewport: viewport_api,
        };

        assert!(dispatch_pointer_down_from_hit_test(
            &mut arena,
            area_key,
            &mut event,
            &mut control
        ));
        let node = arena.get(area_key).unwrap();
        let area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
        assert!((2..=4).contains(&area.cursor_char));
    }

    #[test]
    fn projection_click_uses_nested_text_area_hit_testing() {
        let mut area = TextArea::from_content("{{API_HOST}}");
        area.on_render(|render| {
            render.range(0..12, |text_area_node| {
                rsx! {
                    <crate::view::Element>
                        {text_area_node}
                    </crate::view::Element>
                }
            });
        });
        let mut arena = crate::view::node_arena::NodeArena::new();
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
        }, &mut arena);

        let fragment = area
            .render_fragments
            .iter()
            .find(|fragment| matches!(fragment.kind, TextAreaRenderFragmentKind::Projection(_)))
            .expect("projection fragment");
        let click_x = area.layout_position.x + fragment.content_x + fragment.width * 0.45;
        let click_y = area.layout_position.y + fragment.content_y + fragment.height * 0.5;

        assert!(area.set_cursor_from_projection_position(&mut arena, click_x, click_y));
        assert!(area.cursor_char > 0);
        assert!(area.cursor_char < 12);
    }

    #[test]
    fn cursor_at_wrapped_line_start_maps_to_next_line() {
        let mut area = TextArea::from_content("abcd");
        area.set_width(20.0);
        let mut arena = crate::view::node_arena::NodeArena::new();
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
        }, &mut arena);

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
            meta: EventMeta::new(NodeId::default()),
            reason: crate::ui::FocusReason::Programmatic,
        };
        let mut scratch_arena = crate::view::node_arena::NodeArena::new();
        EventTarget::dispatch_blur(
            &mut area,
            &mut blur,
            &mut control,
            &mut scratch_arena,
            crate::view::node_arena::NodeKey::default(),
        );

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
            meta: EventMeta::new(crate::view::node_arena::NodeKey::default()),
            text: "llo".to_string(),
            input_type: crate::ui::InputType::Typing,
            is_composing: false,
        };
        let mut scratch_arena = crate::view::node_arena::NodeArena::new();
        EventTarget::dispatch_text_input(
            &mut area,
            &mut event,
            &mut control,
            &mut scratch_arena,
            crate::view::node_arena::NodeKey::default(),
        );

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
            meta: EventMeta::new(crate::view::node_arena::NodeKey::default()),
            reason: crate::ui::FocusReason::Programmatic,
        };
        let mut scratch_arena = crate::view::node_arena::NodeArena::new();
        EventTarget::dispatch_focus(
            &mut area,
            &mut event,
            &mut control,
            &mut scratch_arena,
            crate::view::node_arena::NodeKey::default(),
        );

        let actions = event.meta.take_viewport_listener_actions();
        // Test passed NodeKey::default() as self_key, so the emitted action
        // carries the null key. We only assert the action *shape* here.
        assert!(matches!(
            actions.as_slice(),
            [EventCommand::SelectTextRangeAll(_)]
        ));

        let area_id = area.stable_id();
        use crate::view::test_support::{commit_element, new_test_arena};
        let mut arena = new_test_arena();
        let key = commit_element(&mut arena, Box::new(area));
        assert!(select_all_text_by_id(&mut arena, key, area_id));
        let node = arena.get(key).unwrap();
        let area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
        assert_eq!(area.selection_range_chars(), Some((0, 5)));
    }

    #[test]
    fn select_range_clamps_to_character_bounds() {
        let area = TextArea::from_content("hello");
        let area_id = area.stable_id();
        use crate::view::test_support::{commit_element, new_test_arena};
        let mut arena = new_test_arena();
        let key = commit_element(&mut arena, Box::new(area));
        assert!(select_text_range_by_id(&mut arena, key, area_id, 1, 99));
        let node = arena.get(key).unwrap();
        let area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
        assert_eq!(area.selection_range_chars(), Some((1, 5)));
        assert_eq!(area.cursor_char, 5);
    }

    #[test]
    fn mouse_selection_requests_pointer_capture_until_mouse_up() {
        let mut area = TextArea::from_content("hello world");
        let mut arena = crate::view::node_arena::NodeArena::new();
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
        }, &mut arena);

        let mut viewport = Viewport::new();
        {
            let mut control = ViewportControl::new(&mut viewport);
            let down_meta = EventMeta::new(crate::view::node_arena::NodeKey::default());
            let mut down = PointerDownEvent {
                viewport: down_meta.viewport(),
                meta: down_meta,
                pointer: PointerEventData {
                    viewport_x: 4.0,
                    viewport_y: 4.0,
                    local_x: 4.0,
                    local_y: 4.0,
                    button: Some(PointerButton::Left),
                    buttons: PointerButtons {
                        left: true,
                        right: false,
                        middle: false,
                        back: false,
                        forward: false,
                    },
                    modifiers: Modifiers::default(),
                pointer_id: 0,
                pointer_type: PointerType::Mouse,
                pressure: 0.0,
                timestamp: Instant::now(),
                },
            };

            EventTarget::dispatch_pointer_down(
                &mut area,
                &mut down,
                &mut control,
                &mut arena,
                crate::view::node_arena::NodeKey::default(),
            );
        }
        // self_key passed to dispatch was NodeKey::default(); capture therefore
        // reflects the null key. Just assert some capture was set.
        assert_eq!(
            viewport.pointer_capture_node_id(),
            Some(crate::view::node_arena::NodeKey::default())
        );
        assert!(area.pointer_selecting);

        {
            let mut control = ViewportControl::new(&mut viewport);
            let up_meta = EventMeta::new(crate::view::node_arena::NodeKey::default());
            let mut up = crate::ui::PointerUpEvent {
                viewport: up_meta.viewport(),
                meta: up_meta,
                pointer: PointerEventData {
                    viewport_x: 4.0,
                    viewport_y: 4.0,
                    local_x: 4.0,
                    local_y: 4.0,
                    button: Some(PointerButton::Left),
                    buttons: PointerButtons {
                        left: false,
                        right: false,
                        middle: false,
                        back: false,
                        forward: false,
                    },
                    modifiers: Modifiers::default(),
                pointer_id: 0,
                pointer_type: PointerType::Mouse,
                pressure: 0.0,
                timestamp: Instant::now(),
                },
            };

            EventTarget::dispatch_pointer_up(
                &mut area,
                &mut up,
                &mut control,
                &mut arena,
                crate::view::node_arena::NodeKey::default(),
            );
        }
        assert_eq!(viewport.pointer_capture_node_id(), None);
        assert!(!area.pointer_selecting);
    }

    #[test]
    fn cancel_pointer_interaction_stops_mouse_selection() {
        let mut area = TextArea::from_content("hello");
        area.pointer_selecting = true;

        assert!(EventTarget::cancel_pointer_interaction(&mut area));
        assert!(!area.pointer_selecting);
    }

    #[test]
    fn percent_width_uses_layout_override_without_mutating_measured_width() {
        let mut area = TextArea::from_content("123");
        area.set_style_width(Some(Length::percent(100.0)));
        area.set_multiline(false);
        let mut arena = crate::view::node_arena::NodeArena::new();

        area.measure(LayoutConstraints {
            max_width: 200.0,
            max_height: 40.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(40.0),
            viewport_height: 40.0,
        }, &mut arena);
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
        }, &mut arena);
        assert_eq!(area.box_model_snapshot().width, 80.0);

        area.measure(LayoutConstraints {
            max_width: 200.0,
            max_height: 40.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(40.0),
            viewport_height: 40.0,
        }, &mut arena);
        assert_eq!(area.measured_size().0, 200.0);
    }

    #[test]
    fn glyph_layout_keeps_logical_font_size_under_hidpi() {
        let mut area = TextArea::from_content("abc");
        area.set_font_size(16.0);
        let mut arena = crate::view::node_arena::NodeArena::new();
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
        }, &mut arena);

        area.ensure_glyph_layout("abc", 1.0);
        let metrics_1x = area.glyph_buffer.metrics();
        let width_1x = area.glyph_cache_width;

        area.ensure_glyph_layout("abc", 2.0);
        let metrics_2x = area.glyph_buffer.metrics();
        let width_2x = area.glyph_cache_width;

        assert!((metrics_1x.font_size - 16.0).abs() < 0.01);
        assert!((metrics_2x.font_size - 16.0).abs() < 0.01);
        assert!((width_1x - 100.0).abs() < 0.01);
        assert!((width_2x - 100.0).abs() < 0.01);
    }

    #[test]
    fn auto_width_uses_glyph_measurement() {
        let mut area = TextArea::from_content("{{API_HOST}}");
        area.set_multiline(false);
        area.set_font_size(16.0);
        let mut arena = crate::view::node_arena::NodeArena::new();
        area.measure(LayoutConstraints {
            max_width: 500.0,
            max_height: 40.0,
            viewport_width: 500.0,
            percent_base_width: Some(500.0),
            percent_base_height: Some(40.0),
            viewport_height: 40.0,
        }, &mut arena);

        let expected_width = TextArea::measure_render_text_run_with_style(
            "{{API_HOST}}",
            16.0,
            area.line_height,
            &area.font_families,
        )
        .0;
        assert!((area.measured_size().0 - expected_width).abs() < 0.01);
    }

    #[test]
    fn text_area_layout_preserves_fractional_metrics() {
        let mut area = TextArea::from_content("hello");
        area.set_style_width(Some(Length::px(100.5)));
        area.set_style_height(Some(Length::px(40.5)));
        let mut arena = crate::view::node_arena::NodeArena::new();

        area.measure(LayoutConstraints {
            max_width: 200.0,
            max_height: 100.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(100.0),
            viewport_height: 100.0,
        }, &mut arena);
        area.place(LayoutPlacement {
            parent_x: 4.1,
            parent_y: 5.3,
            visual_offset_x: 0.2,
            visual_offset_y: -0.1,
            available_width: 200.0,
            available_height: 100.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(100.0),
            viewport_height: 100.0,
        }, &mut arena);

        let snapshot = area.box_model_snapshot();
        assert!((snapshot.x - 4.3).abs() < 0.01);
        assert!((snapshot.y - 5.2).abs() < 0.01);
        assert!((snapshot.width - 100.5).abs() < 0.01);
        assert!((snapshot.height - 40.5).abs() < 0.01);
    }
}
