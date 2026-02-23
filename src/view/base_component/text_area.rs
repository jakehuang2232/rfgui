use crate::ui::MouseButton as UiMouseButton;
use crate::view::frame_graph::FrameGraph;
use crate::view::render_pass::{DrawRectPass, TextPass};
use crate::{ColorLike, HexColor};
use glyphon::cosmic_text::{Affinity, Cursor, Motion};
use glyphon::{Attrs, Buffer as GlyphBuffer, Family, FontSystem, Metrics, Shaping, Wrap};
use std::cell::RefCell;
use std::time::{Duration, Instant};

use crate::ui::Binding;

use super::{
    BoxModelSnapshot, Element, ElementTrait, EventTarget, LayoutConstraints, LayoutPlacement,
    Layoutable, Position, Renderable, Size, UiBuildContext,
};

pub struct TextArea {
    element: Element,
    position: Position,
    size: Size,
    layout_position: Position,
    layout_size: Size,
    should_render: bool,
    content: String,
    color: Box<dyn ColorLike>,
    placeholder_color: Box<dyn ColorLike>,
    font_families: Vec<String>,
    font_size: f32,
    line_height: f32,
    opacity: f32,
    auto_width: bool,
    auto_height: bool,
    multiline: bool,
    placeholder: String,
    read_only: bool,
    text_binding: Option<Binding<String>>,
    max_length: Option<usize>,
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
    glyph_cache_font_families: Vec<String>,
    measure_revision: u64,
    cached_measure_line_widths: Option<(u64, Vec<f32>)>,
    caret_blink_started_at: Instant,
}

thread_local! {
    static SHARED_TEXT_AREA_FONT_SYSTEM: RefCell<FontSystem> = RefCell::new(FontSystem::new());
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
            should_render: true,
            content: String::new(),
            color: Box::new(HexColor::new("#111111")),
            placeholder_color: Box::new(HexColor::new("#7d8596")),
            font_families: Vec::new(),
            font_size: 16.0,
            line_height: 1.25,
            opacity: 1.0,
            auto_width: false,
            auto_height: false,
            multiline: true,
            placeholder: String::new(),
            read_only: false,
            text_binding: None,
            max_length: None,
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
            glyph_cache_font_families: Vec::new(),
            measure_revision: 0,
            cached_measure_line_widths: None,
            caret_blink_started_at: Instant::now(),
        };
        text_area.set_text(content);
        text_area
    }

    fn mark_measure_dirty(&mut self) {
        self.measure_revision = self.measure_revision.wrapping_add(1);
        let widths = if self.content.is_empty() {
            vec![0.0]
        } else {
            self.content
                .lines()
                .map(|line| estimate_line_width_px(line, self.font_size))
                .collect()
        };
        self.cached_measure_line_widths = Some((self.measure_revision, widths));
    }

    fn ensure_measure_line_widths(&mut self) -> &[f32] {
        let rebuild = self
            .cached_measure_line_widths
            .as_ref()
            .map(|(revision, _)| *revision != self.measure_revision)
            .unwrap_or(true);
        if rebuild {
            let widths = if self.content.is_empty() {
                vec![0.0]
            } else {
                self.content
                    .lines()
                    .map(|line| estimate_line_width_px(line, self.font_size))
                    .collect()
            };
            self.cached_measure_line_widths = Some((self.measure_revision, widths));
        }
        self.cached_measure_line_widths
            .as_ref()
            .map(|(_, widths)| widths.as_slice())
            .unwrap_or(&[])
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
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        self.size = Size { width, height };
        self.element.set_size(width, height);
        self.auto_width = false;
        self.auto_height = false;
        self.invalidate_glyph_layout();
    }

    pub fn set_width(&mut self, width: f32) {
        self.size.width = width;
        self.element.set_width(width);
        self.auto_width = false;
        self.invalidate_glyph_layout();
    }

    pub fn set_height(&mut self, height: f32) {
        self.size.height = height;
        self.element.set_height(height);
        self.auto_height = false;
        self.cached_ime_cursor_rect = None;
    }

    pub fn set_text(&mut self, content: impl Into<String>) {
        let mut next = normalize_multiline(content.into(), self.multiline);
        if let Some(max_length) = self.max_length {
            next = truncate_to_chars(&next, max_length);
        }
        let changed = self.content != next;
        self.content = next;
        if changed {
            self.mark_measure_dirty();
        }
        self.clear_vertical_goal();
        self.reset_caret_blink();
        self.invalidate_glyph_layout();
        self.clamp_cursor();
        self.clamp_scroll();
        self.sync_bound_text();
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
            }
        }
    }

    pub fn set_color<T: ColorLike + 'static>(&mut self, color: T) {
        self.color = Box::new(color);
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
    }

    pub fn set_auto_width(&mut self, auto: bool) {
        self.auto_width = auto;
    }

    pub fn set_auto_height(&mut self, auto: bool) {
        self.auto_height = auto;
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
        }
    }

    fn line_height_px(&self) -> f32 {
        (self.font_size * self.line_height.max(0.1)).max(1.0)
    }

    fn clear_preedit(&mut self) {
        self.ime_preedit.clear();
        self.ime_preedit_cursor = None;
        self.invalidate_glyph_layout();
    }

    fn clear_vertical_goal(&mut self) {
        self.vertical_cursor_x_opt = None;
    }

    fn set_preedit(&mut self, text: String, cursor: Option<(usize, usize)>) {
        self.ime_preedit = normalize_multiline(text, self.multiline);
        self.ime_preedit_cursor = cursor;
        self.invalidate_glyph_layout();
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
        let composed = self.composed_text();
        self.ensure_glyph_layout(composed.as_str());
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

    fn render_payload(&self) -> (String, [f32; 4]) {
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

    fn caret_screen_position(&mut self) -> Option<(f32, f32)> {
        if !self.is_focused {
            return None;
        }
        let composed = self.composed_text();
        let caret_byte = self.caret_byte_in_composed(composed.as_str())?;
        self.ensure_glyph_layout(composed.as_str());
        let (cursor_line, cursor_index) = line_and_index_from_byte(composed.as_str(), caret_byte);
        for affinity in [Affinity::Before, Affinity::After] {
            let cursor = Cursor::new_with_affinity(cursor_line, cursor_index, affinity);
            let Some(layout_cursor) =
                Self::with_shared_font_system(|font_system| {
                    self.glyph_buffer.layout_cursor(font_system, cursor)
                })
            else {
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
        self.ensure_glyph_layout(composed);

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
        self.screen_rects_for_char_range(composed, start_char, end_char)
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
        let line_rects = self.screen_rects_for_char_range(composed, start_char, end_char);
        line_rects
            .into_iter()
            .filter_map(|(position, size)| {
                if size[0] <= 0.01 || size[1] <= 0.01 {
                    return None;
                }
                Some((
                    [position[0], position[1] + size[1] - 1.0],
                    [size[0], 1.0],
                ))
            })
            .collect()
    }

    fn ensure_glyph_layout(&mut self, text: &str) {
        let width = self.effective_width();
        let font_size = self.font_size.max(1.0);
        let line_height_px = (self.font_size * self.line_height.max(0.8)).max(1.0);
        let needs_rebuild = !self.glyph_layout_valid
            || self.glyph_cache_text != text
            || (self.glyph_cache_width - width).abs() > 0.01
            || (self.glyph_cache_font_size - font_size).abs() > 0.01
            || (self.glyph_cache_line_height_px - line_height_px).abs() > 0.01
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
                Some(glyphon::cosmic_text::Align::Left),
            );
            self.glyph_buffer.shape_until_scroll(font_system, false);
        });

        self.glyph_layout_valid = true;
        self.glyph_cache_text = text.to_string();
        self.glyph_cache_width = width;
        self.glyph_cache_font_size = font_size;
        self.glyph_cache_line_height_px = line_height_px;
        self.glyph_cache_font_families = self.font_families.clone();
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

    fn caret_byte_in_composed(&self, composed: &str) -> Option<usize> {
        let base = byte_index_at_char(&self.content, self.cursor_char);
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
        if !self.multiline {
            return false;
        }

        let text = self.content.clone();
        let caret_byte = byte_index_at_char(text.as_str(), self.cursor_char);
        let (line, index) = line_and_index_from_byte(text.as_str(), caret_byte);
        self.ensure_glyph_layout(text.as_str());
        let mut layout_cursor_opt = None;
        for affinity in [Affinity::Before, Affinity::After] {
            let cursor = Cursor::new_with_affinity(line, index, affinity);
            let Some(layout_cursor) =
                Self::with_shared_font_system(|font_system| {
                    self.glyph_buffer.layout_cursor(font_system, cursor)
                })
            else {
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

fn estimate_line_width_px(line: &str, font_size: f32) -> f32 {
    line.chars()
        .map(|ch| estimate_char_width_px(ch, font_size))
        .sum()
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

fn caret_x_in_layout_run(
    cursor_index: usize,
    run: &glyphon::cosmic_text::LayoutRun<'_>,
) -> Option<f32> {
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
) -> Option<glyphon::cosmic_text::LayoutRun<'a>> {
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
        self.set_cursor_from_local_position(event.mouse.local_x, event.mouse.local_y);
        if event.mouse.modifiers.shift {
            let anchor = self.selection_anchor_char.unwrap_or(previous);
            self.selection_anchor_char = Some(anchor);
            self.selection_focus_char = Some(self.cursor_char);
        } else {
            self.selection_anchor_char = Some(self.cursor_char);
            self.selection_focus_char = Some(self.cursor_char);
        }
        self.mouse_selecting = event.mouse.button == Some(UiMouseButton::Left);
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
            self.set_cursor_from_local_position(event.mouse.local_x, event.mouse.local_y);
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
        let handled = self.handle_key_down(event, control);
        if handled {
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
        if self.insert_text(event.text.as_str()) {
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
        self.set_preedit(event.text.clone(), event.cursor);
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
        self.element.dispatch_focus(event, control);
    }

    fn dispatch_blur(
        &mut self,
        event: &mut crate::ui::BlurEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.is_focused = false;
        self.mouse_selecting = false;
        self.cached_ime_cursor_rect = None;
        self.glyph_layout_valid = false;
        self.clear_preedit();
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
        }
        changed || max > 0.0
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
        (next - self.scroll_y).abs() > 0.001 || max > 0.0
    }

    fn get_scroll_offset(&self) -> (f32, f32) {
        (0.0, self.scroll_y)
    }

    fn set_scroll_offset(&mut self, offset: (f32, f32)) {
        self.scroll_y = offset.1;
        self.clamp_scroll();
        self.cached_ime_cursor_rect = None;
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
        self.size.width = width;
        self.element.set_width(width);
        self.invalidate_glyph_layout();
    }

    fn set_layout_height(&mut self, height: f32) {
        self.size.height = height;
        self.element.set_height(height);
        self.cached_ime_cursor_rect = None;
    }

    fn set_layout_offset(&mut self, x: f32, y: f32) {
        self.position = Position { x, y };
        self.element.set_position(x, y);
    }

    fn measure(&mut self, constraints: LayoutConstraints) {
        if !self.auto_width && !self.auto_height {
            return;
        }

        let line_px = self.line_height_px();
        let line_widths = self.ensure_measure_line_widths().to_vec();
        let line_count = line_widths.len().max(1);

        if self.auto_width {
            let intrinsic_width = line_widths
                .iter()
                .copied()
                .fold(0.0_f32, f32::max)
                .max(1.0);
            let available = constraints.max_width.max(1.0);
            self.size.width = intrinsic_width.min(available);
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
                        .map(|line_width| ((*line_width) / effective_width).ceil().max(1.0) as usize)
                        .sum::<usize>()
                };
                wrapped_lines.max(line_count)
            } else {
                1
            };

            self.size.height = (line_px * resolved_lines as f32).max(1.0);
            self.element.set_height(self.size.height);
        }
    }

    fn place(&mut self, placement: LayoutPlacement) {
        let prev_layout_width = self.layout_size.width;
        let available_width = placement.available_width.max(0.0);
        let available_height = placement.available_height.max(0.0);
        let max_width = (available_width - self.position.x.max(0.0)).max(0.0);
        let max_height = (available_height - self.position.y.max(0.0)).max(0.0);
        self.layout_size = Size {
            width: self.size.width.max(0.0).min(max_width),
            height: self.size.height.max(0.0).min(max_height),
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
        self.clamp_scroll();
    }
}

impl Renderable for TextArea {
    fn build(&mut self, graph: &mut FrameGraph, ctx: &mut UiBuildContext) {
        if !self.should_render {
            return;
        }

        let opacity = self.opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return;
        }

        let (content, color) = self.render_payload();

        let clip = Some([
            self.layout_position.x.floor().max(0.0) as u32,
            self.layout_position.y.floor().max(0.0) as u32,
            self.layout_size.width.ceil().max(0.0) as u32,
            self.layout_size.height.ceil().max(0.0) as u32,
        ]);

        if !content.is_empty() {
            for (position, size) in self.selection_screen_rects(content.as_str()) {
                let mut selection_pass =
                    DrawRectPass::new(position, size, [0.28, 0.52, 0.94, 0.35], opacity);
                selection_pass.set_scissor_rect(clip);
                ctx.push_pass(graph, selection_pass);
            }
            let ime_underline_rects = self.ime_preedit_underline_rects(content.as_str());

            let mut pass = TextPass::new(
                content,
                self.layout_position.x,
                self.layout_position.y - self.scroll_y,
                self.layout_size.width,
                self.layout_size.height.max(self.content_height()),
                color,
                opacity,
                self.font_size,
                self.line_height,
                self.font_families.clone(),
            );
            pass.set_scissor_rect(clip);
            ctx.push_pass(graph, pass);

            for (position, size) in ime_underline_rects {
                let mut underline_pass =
                    DrawRectPass::new(position, size, self.color.to_rgba_f32(), opacity);
                underline_pass.set_scissor_rect(clip);
                ctx.push_pass(graph, underline_pass);
            }
        }

        if let Some((caret_x, caret_y)) = self.caret_screen_position() {
            self.cached_ime_cursor_rect = Some((caret_x, caret_y, 1.0, self.line_height_px()));
            if !self.should_draw_caret() {
                return;
            }
            let mut caret_pass = DrawRectPass::new(
                [caret_x, caret_y],
                [1.0, self.line_height_px()],
                self.color.to_rgba_f32(),
                opacity,
            );
            caret_pass.set_scissor_rect(clip);
            ctx.push_pass(graph, caret_pass);
        } else {
            self.cached_ime_cursor_rect = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TextArea;
    use crate::view::base_component::{LayoutPlacement, Layoutable};

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
            percent_base_width: Some(20.0),
            percent_base_height: Some(100.0),
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
}
