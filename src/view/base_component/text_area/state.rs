//! Cursor / selection setters. Decision A9: char index lives at the root
//! TextArea; children carry no cursor or selection state. Decision (v2):
//! `selection_anchor_char` / `selection_focus_char` preserve direction —
//! callers that need the sorted range use `selection_range_chars`.
//!
//! Vertical cursor movement (Up/Down with sticky-x) and visual-line Home/End
//! need glyph metrics from the Run subtree and live in `events.rs` where
//! arena access is in scope. State.rs stays glyph-free.

use crate::platform::word_segmenter::{next_word_boundary, prev_word_boundary, word_segmenter};
use crate::view::base_component::DirtyFlags;

use super::TextArea;
use super::caret_map::CaretAffinity;

impl TextArea {
    pub(super) fn content_char_len(&self) -> usize {
        self.content.chars().count()
    }

    pub(super) fn clamp_char(&self, char_index: usize) -> usize {
        char_index.min(self.content_char_len())
    }

    /// Move cursor to `char_index`, clearing any selection.
    pub(super) fn move_cursor_to(&mut self, char_index: usize) {
        self.cursor_char = self.clamp_char(char_index);
        self.clear_selection();
        self.reset_caret_blink();
        self.clear_vertical_goal();
        self.mark_caret_scroll_pending();
    }

    /// Move cursor to start of current paragraph (preceding `\n` boundary
    /// or 0). Visual-line aware Home is a P3.3 concern (needs glyph info).
    pub(super) fn move_cursor_line_home(&mut self) {
        let chars: Vec<char> = self.content.chars().collect();
        let mut idx = self.cursor_char.min(chars.len());
        while idx > 0 && chars[idx - 1] != '\n' {
            idx -= 1;
        }
        self.cursor_char = idx;
        self.clear_selection();
        self.reset_caret_blink();
        self.clear_vertical_goal();
        self.mark_caret_scroll_pending();
    }

    pub(super) fn move_cursor_line_end(&mut self) {
        let chars: Vec<char> = self.content.chars().collect();
        let mut idx = self.cursor_char.min(chars.len());
        while idx < chars.len() && chars[idx] != '\n' {
            idx += 1;
        }
        self.cursor_char = idx;
        self.clear_selection();
        self.reset_caret_blink();
        self.clear_vertical_goal();
        self.mark_caret_scroll_pending();
    }

    pub(super) fn move_cursor_text_home(&mut self) {
        self.cursor_char = 0;
        self.clear_selection();
        self.reset_caret_blink();
        self.clear_vertical_goal();
        self.mark_caret_scroll_pending();
    }

    pub(super) fn move_cursor_text_end(&mut self) {
        self.cursor_char = self.content_char_len();
        self.clear_selection();
        self.reset_caret_blink();
        self.clear_vertical_goal();
        self.mark_caret_scroll_pending();
    }

    /// Shift+arrow / drag-extend semantics. Anchor is set once (at the
    /// cursor position before the first extend), focus tracks subsequent
    /// updates. Cursor follows focus. **Direction preserved.**
    pub(super) fn extend_selection_to(&mut self, focus_char: usize) {
        let focus = self.clamp_char(focus_char);
        if self.selection_anchor_char.is_none() {
            self.selection_anchor_char = Some(self.cursor_char);
        }
        self.selection_focus_char = Some(focus);
        self.cursor_char = focus;
        self.reset_caret_blink();
        self.clear_vertical_goal();
        self.mark_caret_scroll_pending();
    }

    pub(super) fn extend_selection_left(&mut self) -> bool {
        if self.cursor_char == 0 {
            return false;
        }
        self.extend_selection_to(self.cursor_char - 1);
        true
    }

    pub(super) fn extend_selection_right(&mut self) -> bool {
        if self.cursor_char >= self.content_char_len() {
            return false;
        }
        self.extend_selection_to(self.cursor_char + 1);
        true
    }

    /// macOS Option+Left target: nearest word START strictly before
    /// `cursor_char`. Routes through the process-wide
    /// `rfgui-segmenter` system segmenter; when no OS-native segmenter
    /// is available for the target, `rfgui-segmenter` falls back to its
    /// Unicode rule based implementation.
    pub(super) fn prev_word_boundary_at(&self, from: usize) -> usize {
        prev_word_boundary(&self.content, word_segmenter(), from)
    }

    pub(super) fn next_word_boundary_at(&self, from: usize) -> usize {
        next_word_boundary(&self.content, word_segmenter(), from)
    }

    pub(super) fn move_cursor_word_left(&mut self) -> bool {
        let target = self.prev_word_boundary_at(self.cursor_char);
        if target == self.cursor_char {
            return false;
        }
        self.move_cursor_to(target);
        true
    }

    pub(super) fn move_cursor_word_right(&mut self) -> bool {
        let target = self.next_word_boundary_at(self.cursor_char);
        if target == self.cursor_char {
            return false;
        }
        self.move_cursor_to(target);
        true
    }

    pub(super) fn extend_selection_word_left(&mut self) -> bool {
        let target = self.prev_word_boundary_at(self.cursor_char);
        if target == self.cursor_char {
            return false;
        }
        self.extend_selection_to(target);
        true
    }

    pub(super) fn extend_selection_word_right(&mut self) -> bool {
        let target = self.next_word_boundary_at(self.cursor_char);
        if target == self.cursor_char {
            return false;
        }
        self.extend_selection_to(target);
        true
    }

    pub(super) fn extend_selection_line_home(&mut self) {
        let chars: Vec<char> = self.content.chars().collect();
        let mut idx = self.cursor_char.min(chars.len());
        while idx > 0 && chars[idx - 1] != '\n' {
            idx -= 1;
        }
        self.extend_selection_to(idx);
    }

    pub(super) fn extend_selection_line_end(&mut self) {
        let chars: Vec<char> = self.content.chars().collect();
        let mut idx = self.cursor_char.min(chars.len());
        while idx < chars.len() && chars[idx] != '\n' {
            idx += 1;
        }
        self.extend_selection_to(idx);
    }

    /// Pointer-drag selection start: collapse selection at `at`, mark
    /// pointer_selecting. The pointer-move handler then calls
    /// `update_pointer_selection` to extend.
    pub(super) fn start_pointer_selection_with_affinity(
        &mut self,
        at: usize,
        affinity: CaretAffinity,
    ) {
        let at = self.clamp_char(at);
        self.cursor_char = at;
        self.selection_anchor_char = Some(at);
        self.selection_focus_char = Some(at);
        self.pointer_selecting = true;
        self.reset_caret_blink();
        self.clear_vertical_goal();
        self.cursor_affinity = affinity;
        self.mark_caret_scroll_pending();
    }

    pub(super) fn update_pointer_selection_with_affinity(
        &mut self,
        focus: usize,
        affinity: CaretAffinity,
    ) {
        if !self.pointer_selecting {
            return;
        }
        let focus = self.clamp_char(focus);
        self.selection_focus_char = Some(focus);
        self.cursor_char = focus;
        self.reset_caret_blink();
        self.clear_vertical_goal();
        self.cursor_affinity = affinity;
        self.mark_caret_scroll_pending();
    }

    pub(super) fn end_pointer_selection(&mut self) {
        self.pointer_selecting = false;
        // Collapse zero-length selection so subsequent caret-only edits
        // don't trip the delete-selection branch.
        if let (Some(anchor), Some(focus)) = (self.selection_anchor_char, self.selection_focus_char)
            && anchor == focus
        {
            self.clear_selection();
        }
    }

    pub fn select_all(&mut self) {
        let len = self.content_char_len();
        self.select_range(0, len);
    }

    pub fn select_range(&mut self, anchor: usize, focus: usize) {
        let len = self.content_char_len();
        let anchor = anchor.min(len);
        let focus = focus.min(len);
        self.selection_anchor_char = Some(anchor);
        self.selection_focus_char = Some(focus);
        self.cursor_char = focus;
        self.reset_caret_blink();
        self.clear_vertical_goal();
        self.mark_caret_scroll_pending();
    }

    /// Replace the active IME preedit text (decision A7). Returns `true`
    /// if anything actually changed. `\n` is collapsed to space when
    /// `multiline=false` to mirror v1 behaviour. Routing the preedit to
    /// the target child Run lives in `projection::route_preedit_to_runs`
    /// since it needs arena access; callers in `events.rs` invoke both.
    pub(super) fn set_preedit(&mut self, text: String, cursor: Option<(usize, usize)>) -> bool {
        let normalized = if self.multiline {
            text
        } else {
            text.replace('\n', " ")
        };
        if self.ime_preedit == normalized && self.ime_preedit_cursor == cursor {
            return false;
        }
        self.children_dirty = true;
        self.ime_preedit = normalized;
        self.ime_preedit_cursor = cursor;
        self.bump_unified_ifc_source_revision();
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
        self.reset_caret_blink();
        self.mark_caret_scroll_pending();
        true
    }

    /// Commit the active preedit by inserting it into `content` and
    /// clearing preedit state. Mirrors v1's
    /// `commit_preedit_preserving_render_fragments`. Returns `true` if
    /// any text was actually inserted.
    pub(super) fn commit_preedit(&mut self) -> bool {
        if self.ime_preedit.is_empty() {
            self.clear_preedit();
            return false;
        }
        let text = std::mem::take(&mut self.ime_preedit);
        self.ime_preedit_cursor = None;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
        self.insert_text(&text)
    }

    pub(super) fn clear_preedit(&mut self) -> bool {
        if self.ime_preedit.is_empty() && self.ime_preedit_cursor.is_none() {
            return false;
        }
        self.children_dirty = true;
        self.ime_preedit.clear();
        self.ime_preedit_cursor = None;
        self.bump_unified_ifc_source_revision();
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
        true
    }

    pub(super) fn set_focused(&mut self, focused: bool) -> bool {
        if self.is_focused == focused {
            return false;
        }
        self.is_focused = focused;
        self.reset_caret_blink();
        if !focused {
            self.pointer_selecting = false;
            // Editor blur: keep selection visible? v1 keeps it. Match v1.
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::caret_map::CaretAffinity;
    use super::*;

    #[test]
    fn pointer_drag_update_resets_stale_affinity() {
        let mut text_area = TextArea::new();
        text_area.content = "abcdef".to_string();
        text_area.cursor_affinity = CaretAffinity::Upstream;
        text_area.vertical_cursor_x = Some(42.0);
        text_area.pointer_selecting = true;
        text_area.selection_anchor_char = Some(0);
        text_area.selection_focus_char = Some(0);

        text_area.update_pointer_selection_with_affinity(3, CaretAffinity::Downstream);

        assert_eq!(text_area.cursor_char, 3);
        assert_eq!(text_area.selection_focus_char, Some(3));
        assert_eq!(text_area.cursor_affinity, CaretAffinity::Downstream);
        assert_eq!(text_area.vertical_cursor_x, None);
    }
}
