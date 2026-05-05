//! Edit primitives — insert / delete / normalize / max_length / binding sync.
//!
//! All ops mutate `content` + `cursor_char` + selection at the **root** char
//! index level (decision A9). They mark `children_dirty` so the next frame
//! reconciles the run subtree, and `dirty_flags |= ALL` so layout reshapes.
//! Direction-preserving selection (decision: v2 fixes v1's anchor/focus
//! min/max bug) is honored — none of the ops collapse the anchor/focus pair
//! before clearing.

use crate::ui::{EventMeta, TextChangeEvent};
use crate::view::base_component::DirtyFlags;
use crate::view::node_arena::NodeKey;

use super::TextArea;

impl TextArea {
    /// Snapshot the currently-selected range as `(start, end)` *sorted*.
    /// Direction is preserved on the underlying `selection_anchor_char` /
    /// `selection_focus_char` fields — callers that need direction read
    /// those directly. Returns `None` when no selection or zero-length.
    pub(super) fn selection_range_chars(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor_char?;
        let focus = self.selection_focus_char?;
        if anchor == focus {
            return None;
        }
        Some((anchor.min(focus), anchor.max(focus)))
    }

    pub(super) fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_range_chars()?;
        let start_byte = byte_index_at_char(&self.content, start);
        let end_byte = byte_index_at_char(&self.content, end);
        Some(self.content[start_byte..end_byte].to_string())
    }

    /// Delete the active selection range. Returns true if anything was
    /// deleted. Cursor lands at `start`; selection cleared.
    pub(super) fn delete_selected_text(&mut self) -> bool {
        let Some((start, end)) = self.selection_range_chars() else {
            return false;
        };
        let start_byte = byte_index_at_char(&self.content, start);
        let end_byte = byte_index_at_char(&self.content, end);
        self.content.replace_range(start_byte..end_byte, "");
        self.cursor_char = start;
        self.clear_selection();
        self.mark_content_dirty();
        self.reset_caret_blink();
        self.clear_vertical_goal();
        self.mark_caret_scroll_pending();
        self.sync_bound_text();
        true
    }

    /// Insert `text` at the cursor. Honors `multiline` (collapses `\n`s to
    /// space when single-line) and `max_length`. Replaces any active
    /// selection first.
    pub(super) fn insert_text(&mut self, text: &str) -> bool {
        let deleted = self.delete_selected_text();
        if text.is_empty() {
            return deleted;
        }
        let normalized = normalize_multiline(text, self.multiline);
        if normalized.is_empty() {
            return deleted;
        }
        let allowed = self.can_insert_chars();
        if allowed == 0 {
            return deleted;
        }
        let incoming = truncate_to_chars(&normalized, allowed);
        if incoming.is_empty() {
            return deleted;
        }

        let insert_at_byte = byte_index_at_char(&self.content, self.cursor_char);
        self.content.insert_str(insert_at_byte, &incoming);
        self.cursor_char += incoming.chars().count();
        self.mark_content_dirty();
        self.reset_caret_blink();
        self.clear_vertical_goal();
        self.mark_caret_scroll_pending();
        self.sync_bound_text();
        true
    }

    pub(super) fn delete_backspace(&mut self) -> bool {
        if self.delete_selected_text() {
            return true;
        }
        if self.cursor_char == 0 {
            return false;
        }
        let end_byte = byte_index_at_char(&self.content, self.cursor_char);
        let start_byte = byte_index_at_char(&self.content, self.cursor_char - 1);
        self.content.replace_range(start_byte..end_byte, "");
        self.cursor_char -= 1;
        self.mark_content_dirty();
        self.reset_caret_blink();
        self.clear_vertical_goal();
        self.mark_caret_scroll_pending();
        self.sync_bound_text();
        true
    }

    pub(super) fn delete_forward(&mut self) -> bool {
        if self.delete_selected_text() {
            return true;
        }
        let len = self.content.chars().count();
        if self.cursor_char >= len {
            return false;
        }
        let start_byte = byte_index_at_char(&self.content, self.cursor_char);
        let end_byte = byte_index_at_char(&self.content, self.cursor_char + 1);
        self.content.replace_range(start_byte..end_byte, "");
        self.mark_content_dirty();
        self.reset_caret_blink();
        self.clear_vertical_goal();
        self.mark_caret_scroll_pending();
        self.sync_bound_text();
        true
    }

    /// Word-granular backspace (Alt+Backspace on macOS, Ctrl+Backspace on
    /// Win/Linux). Falls through to selection-delete when a range is
    /// active, mirroring `delete_backspace`.
    pub(super) fn delete_prev_word(&mut self) -> bool {
        if self.delete_selected_text() {
            return true;
        }
        if self.cursor_char == 0 {
            return false;
        }
        let target = self.prev_word_boundary_at(self.cursor_char);
        if target >= self.cursor_char {
            return false;
        }
        let start_byte = byte_index_at_char(&self.content, target);
        let end_byte = byte_index_at_char(&self.content, self.cursor_char);
        self.content.replace_range(start_byte..end_byte, "");
        self.cursor_char = target;
        self.mark_content_dirty();
        self.reset_caret_blink();
        self.clear_vertical_goal();
        self.mark_caret_scroll_pending();
        self.sync_bound_text();
        true
    }

    /// Word-granular forward delete (Alt+Delete on macOS, Ctrl+Delete on
    /// Win/Linux).
    pub(super) fn delete_next_word(&mut self) -> bool {
        if self.delete_selected_text() {
            return true;
        }
        let len = self.content.chars().count();
        if self.cursor_char >= len {
            return false;
        }
        let target = self.next_word_boundary_at(self.cursor_char);
        if target <= self.cursor_char {
            return false;
        }
        let start_byte = byte_index_at_char(&self.content, self.cursor_char);
        let end_byte = byte_index_at_char(&self.content, target);
        self.content.replace_range(start_byte..end_byte, "");
        self.mark_content_dirty();
        self.reset_caret_blink();
        self.clear_vertical_goal();
        self.mark_caret_scroll_pending();
        self.sync_bound_text();
        true
    }

    /// Apply an externally-set content (e.g. the bound `Binding<String>`
    /// changed elsewhere). Cursor / selection clamped, ime preedit cleared.
    /// Skips work if the value already matches to avoid spurious dirty.
    pub(super) fn set_content_from_external(&mut self, value: String) -> bool {
        if self.content == value {
            return false;
        }
        let normalized = normalize_multiline(&value, self.multiline);
        let normalized = match self.max_length {
            Some(limit) => truncate_to_chars(&normalized, limit),
            None => normalized,
        };
        if self.content == normalized {
            return false;
        }
        self.content = normalized;
        let len = self.content.chars().count();
        self.cursor_char = self.cursor_char.min(len);
        self.clear_selection();
        self.ime_preedit.clear();
        self.ime_preedit_cursor = None;
        self.mark_content_dirty();
        self.reset_caret_blink();
        self.clear_vertical_goal();
        self.mark_caret_scroll_pending();
        true
    }

    /// Push the current `content` into the bound `Binding<String>` if any.
    /// Skip if values already match to avoid Binding-set churn cycles.
    pub(super) fn sync_bound_text(&self) {
        let Some(binding) = self.text_binding.as_ref() else {
            return;
        };
        if binding.get() != self.content {
            binding.set(self.content.clone());
        }
    }

    pub(super) fn notify_change_handlers(&self) {
        if self.on_change_handlers.is_empty() {
            return;
        }
        let mut event = TextChangeEvent {
            meta: EventMeta::new(NodeKey::default()),
            value: self.content.clone(),
        };
        for handler in &self.on_change_handlers {
            handler.call(&mut event);
        }
    }

    pub(super) fn clear_selection(&mut self) {
        self.selection_anchor_char = None;
        self.selection_focus_char = None;
    }

    pub(super) fn clear_vertical_goal(&mut self) {
        self.vertical_cursor_x = None;
        // Caret affinity is a sticky bit owned by the cursor position;
        // every horizontal / arbitrary cursor move that already clears
        // sticky-x should also reset affinity to Downstream. Cmd+Right
        // (and similar wrap-tail navigations) reapply Upstream *after*
        // the move so the override survives this reset.
        self.cursor_affinity =
            crate::view::base_component::text_area::caret_map::CaretAffinity::Downstream;
    }

    pub(super) fn mark_caret_scroll_pending(&mut self) {
        self.pending_caret_scroll = true;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::PLACE);
    }

    pub(super) fn reset_caret_blink(&mut self) {
        self.caret_blink_started_at = crate::time::Instant::now();
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::PAINT);
    }

    /// Mark that content / range mapping changed → run subtree must rebuild
    /// + layout must reshape. The walker / layout entry point picks up
    /// `children_dirty` next tick (see `layout.rs`); shape invalidation
    /// happens automatically inside `TextAreaTextRun::set_text`.
    pub(super) fn mark_content_dirty(&mut self) {
        self.children_dirty = true;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
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
}

pub(super) fn normalize_multiline(text: &str, multiline: bool) -> String {
    if multiline {
        text.to_string()
    } else {
        text.replace('\n', " ")
    }
}

pub(super) fn truncate_to_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

pub(super) fn byte_index_at_char(value: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }
    value
        .char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(value.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ta(text: &str, cursor: usize) -> TextArea {
        let mut t = TextArea::new();
        t.content = text.to_string();
        t.cursor_char = cursor;
        t
    }

    #[test]
    fn delete_prev_word_strips_back_to_word_start() {
        let mut t = ta("foo bar baz", 11);
        assert!(t.delete_prev_word());
        assert_eq!(t.content, "foo bar ");
        assert_eq!(t.cursor_char, 8);
    }

    #[test]
    fn delete_prev_word_eats_trailing_whitespace_then_word() {
        let mut t = ta("foo bar  ", 9);
        assert!(t.delete_prev_word());
        assert_eq!(t.content, "foo ");
        assert_eq!(t.cursor_char, 4);
    }

    #[test]
    fn delete_prev_word_at_start_is_noop() {
        let mut t = ta("foo", 0);
        assert!(!t.delete_prev_word());
        assert_eq!(t.content, "foo");
    }

    #[test]
    fn delete_next_word_strips_to_word_end() {
        let mut t = ta("foo bar baz", 0);
        assert!(t.delete_next_word());
        assert_eq!(t.content, " bar baz");
        assert_eq!(t.cursor_char, 0);
    }

    #[test]
    fn delete_next_word_eats_leading_whitespace_then_word() {
        let mut t = ta("  foo bar", 0);
        assert!(t.delete_next_word());
        assert_eq!(t.content, " bar");
        assert_eq!(t.cursor_char, 0);
    }

    #[test]
    fn delete_next_word_at_end_is_noop() {
        let mut t = ta("foo", 3);
        assert!(!t.delete_next_word());
        assert_eq!(t.content, "foo");
    }

    #[test]
    fn delete_prev_word_with_selection_falls_through_to_selection_delete() {
        let mut t = ta("hello world", 11);
        t.select_range(2, 8);
        assert!(t.delete_prev_word());
        assert_eq!(t.content, "herld");
        assert_eq!(t.cursor_char, 2);
    }
}
