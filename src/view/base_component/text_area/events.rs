//! `EventTarget` impls for `TextArea`.
//!
//! Decision A6: TextArea returns `true` from all 5 `block_*_child_event`
//! methods so keyboard / IME / focus events never reach descendants —
//! editor-level state is the single source of truth. Pointer events are
//! *not* blocked: projection-internal widgets remain interactive.
//!
//! Vertical cursor movement (Up / Down with sticky-x) is deferred — it
//! needs a wrap-aware glyph query and currently returns `false` so the
//! event bubbles per usual EventTarget contract. P3 follow-up.

#![allow(unused_variables)]

use crate::platform::ImeCommand;
use crate::ui::{
    BlurEvent, CopyEvent, CutEvent, EventMeta, FocusEvent, ImeCommitEvent, ImeDisabledEvent,
    ImeEnabledEvent, ImePreeditEvent, InputType, KeyDownEvent, PasteEvent, PointerButton,
    PointerDownEvent, PointerMoveEvent, PointerUpEvent, TextAreaFocusEvent, TextInputEvent,
};
use crate::view::base_component::EventTarget;
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::viewport::ViewportControl;

use super::TextArea;
use super::caret_map::{CaretAffinity, CaretNavigationMap, VerticalDirection};

impl TextArea {
    fn caret_position_for(
        &mut self,
        arena: &NodeArena,
        char_index: usize,
        affinity: CaretAffinity,
    ) -> Option<(f32, f32, f32)> {
        let saved_char = self.cursor_char;
        let saved_affinity = self.cursor_affinity;
        self.cursor_char = self.clamp_char(char_index);
        self.cursor_affinity = affinity;
        let position = self.caret_screen_position(arena);
        self.cursor_char = saved_char;
        self.cursor_affinity = saved_affinity;
        position
    }

    fn caret_y_for(
        &mut self,
        arena: &NodeArena,
        char_index: usize,
        affinity: CaretAffinity,
    ) -> Option<f32> {
        self.caret_position_for(arena, char_index, affinity)
            .map(|(_, y, _)| y)
    }

    fn affinity_nearest_y(
        &mut self,
        arena: &NodeArena,
        char_index: usize,
        reference_y: f32,
    ) -> CaretAffinity {
        let up = self.caret_y_for(arena, char_index, CaretAffinity::Upstream);
        let down = self.caret_y_for(arena, char_index, CaretAffinity::Downstream);
        match (up, down) {
            (Some(up), Some(down)) if (up - down).abs() > 0.5 => {
                if (up - reference_y).abs() <= (down - reference_y).abs() {
                    CaretAffinity::Upstream
                } else {
                    CaretAffinity::Downstream
                }
            }
            _ => CaretAffinity::Downstream,
        }
    }

    fn affinity_matching_y(
        &mut self,
        arena: &NodeArena,
        char_index: usize,
        reference_y: f32,
    ) -> Option<CaretAffinity> {
        let up = self.caret_y_for(arena, char_index, CaretAffinity::Upstream);
        let down = self.caret_y_for(arena, char_index, CaretAffinity::Downstream);
        match (up, down) {
            (Some(up), Some(down)) => {
                let up_matches = (up - reference_y).abs() <= 0.5;
                let down_matches = (down - reference_y).abs() <= 0.5;
                match (up_matches, down_matches) {
                    (true, false) => Some(CaretAffinity::Upstream),
                    (false, true) | (true, true) => Some(CaretAffinity::Downstream),
                    (false, false) => None,
                }
            }
            _ => None,
        }
    }

    fn flip_horizontal_affinity_if_needed(&mut self, arena: &NodeArena, right: bool) -> bool {
        let up = self.caret_y_for(arena, self.cursor_char, CaretAffinity::Upstream);
        let down = self.caret_y_for(arena, self.cursor_char, CaretAffinity::Downstream);
        let Some((up, down)) = up.zip(down) else {
            return false;
        };
        if (up - down).abs() <= 0.5 {
            return false;
        }

        let next_affinity = match (right, self.cursor_affinity) {
            (true, CaretAffinity::Upstream) => Some(CaretAffinity::Downstream),
            (false, CaretAffinity::Downstream) => Some(CaretAffinity::Upstream),
            _ => None,
        };
        let Some(next_affinity) = next_affinity else {
            return false;
        };

        self.cursor_affinity = next_affinity;
        self.vertical_cursor_x = None;
        self.clear_selection();
        self.reset_caret_blink();
        self.mark_caret_scroll_pending();
        true
    }

    pub(super) fn handle_horizontal_arrow(&mut self, arena: &NodeArena, right: bool) -> bool {
        if right && self.flip_horizontal_affinity_if_needed(arena, true) {
            return true;
        }

        let len = self.content_char_len();
        let Some((reference_x, reference_y, _)) = self.caret_screen_position(arena) else {
            return false;
        };

        if !right && self.flip_horizontal_affinity_if_needed(arena, false) {
            return true;
        }

        let mut target = self.cursor_char;
        loop {
            target = if right {
                if target >= len {
                    return false;
                }
                target + 1
            } else {
                if target == 0 {
                    return false;
                }
                target - 1
            };

            let mut target_affinity = self
                .affinity_matching_y(arena, target, reference_y)
                .unwrap_or_else(|| self.affinity_nearest_y(arena, target, reference_y));
            if right
                && target_affinity == CaretAffinity::Upstream
                && self
                    .content
                    .chars()
                    .nth(target.saturating_sub(1))
                    .is_some_and(|ch| ch == '\n')
                && self
                    .caret_position_for(arena, target, CaretAffinity::Downstream)
                    .is_some()
            {
                target_affinity = CaretAffinity::Downstream;
            }
            let Some((target_x, target_y, _)) =
                self.caret_position_for(arena, target, target_affinity)
            else {
                continue;
            };

            if (target_x - reference_x).abs() <= 0.5 && (target_y - reference_y).abs() <= 0.5 {
                let alternate_affinity = match (right, target_affinity) {
                    (true, CaretAffinity::Upstream) => Some(CaretAffinity::Downstream),
                    (false, CaretAffinity::Downstream) => Some(CaretAffinity::Upstream),
                    _ => None,
                };
                if let Some(alternate_affinity) = alternate_affinity
                    && let Some((alternate_x, alternate_y, _)) =
                        self.caret_position_for(arena, target, alternate_affinity)
                    && ((alternate_x - reference_x).abs() > 0.5
                        || (alternate_y - reference_y).abs() > 0.5)
                {
                    self.move_cursor_to(target);
                    self.cursor_affinity = alternate_affinity;
                    self.mark_caret_scroll_pending();
                    return true;
                }
                continue;
            }

            self.move_cursor_to(target);
            self.cursor_affinity = target_affinity;
            self.mark_caret_scroll_pending();
            return true;
        }
    }

    /// Resolve Up / Down vertical caret movement against the live caret
    /// navigation map. Returns `Some((target_char, target_affinity,
    /// sticky_x))` when a target visual line exists, `None` when caret
    /// is already on the edge line.
    fn vertical_arrow_target(
        &self,
        arena: &NodeArena,
        direction: VerticalDirection,
    ) -> Option<(usize, CaretAffinity, f32)> {
        let map = CaretNavigationMap::build(self, arena);
        if map.is_empty() {
            return None;
        }
        let affinity = self.cursor_affinity;
        let sticky_content_x = self.vertical_cursor_x.or_else(|| {
            map.caret_stop_for_char(self.cursor_char, affinity)
                .map(|s| s.x + self.scroll_x)
        })?;
        let sticky_screen_x = sticky_content_x - self.scroll_x;
        let target = map.vertical_target_with_affinity(
            self.cursor_char,
            affinity,
            sticky_screen_x,
            direction,
        )?;
        Some((target.char_index, target.affinity, sticky_content_x))
    }

    /// Apply Up / Down (and Shift+Up / Shift+Down) using the caret
    /// navigation map. Sticky-x is preserved across consecutive vertical
    /// presses; horizontal arrows / clicks / edits clear it via the
    /// existing `clear_vertical_goal` calls in cursor mutators.
    fn handle_vertical_arrow(
        &mut self,
        arena: &NodeArena,
        direction: VerticalDirection,
        shift: bool,
    ) -> bool {
        let Some((target, target_affinity, sticky_x)) =
            self.vertical_arrow_target(arena, direction)
        else {
            // No target: collapse selection at the current edge so plain
            // Up at the start (or Down at the end) still feels responsive.
            if !shift && self.selection_range_chars().is_some() {
                self.clear_selection();
                self.reset_caret_blink();
            }
            return true;
        };
        if shift {
            self.extend_selection_to(target);
        } else {
            self.move_cursor_to(target);
        }
        self.cursor_affinity = target_affinity;
        // Restore sticky_x — both `move_cursor_to` and `extend_selection_to`
        // clear it via `clear_vertical_goal`, but consecutive vertical
        // presses must keep walking the same column.
        self.vertical_cursor_x = Some(sticky_x);
        self.mark_caret_scroll_pending();
        true
    }

    /// macOS Cmd+Left / Cmd+Right target: head / tail of the **visual**
    /// line owning the caret (wrap-aware). Falls back to the
    /// paragraph-based mutators on `state.rs` when the navigation map
    /// can't resolve a line for the cursor (e.g. during an empty-content
    /// build).
    fn handle_visual_line_jump(&mut self, arena: &NodeArena, end: bool, shift: bool) -> bool {
        let map = CaretNavigationMap::build(self, arena);
        let affinity = self.cursor_affinity;
        let target = if end {
            map.visual_line_end_for_char(self.cursor_char, affinity)
        } else {
            map.visual_line_home_for_char(self.cursor_char, affinity)
        };
        match target {
            Some(idx) => {
                if shift {
                    self.extend_selection_to(idx);
                } else {
                    self.move_cursor_to(idx);
                }
                // Cmd+Right at a wrap boundary should *stick* to the
                // upper line's tail; without Upstream the very next
                // render snaps the caret to the lower line's head and
                // visually swallows the jump. Cmd+Left needs no flip
                // (line head sits at x=0 on its own line).
                if end {
                    self.cursor_affinity = CaretAffinity::Upstream;
                    self.mark_caret_scroll_pending();
                }
            }
            None => match (end, shift) {
                (true, true) => self.extend_selection_line_end(),
                (true, false) => self.move_cursor_line_end(),
                (false, true) => self.extend_selection_line_home(),
                (false, false) => self.move_cursor_line_home(),
            },
        }
        true
    }
}

#[cfg(test)]
mod tests;

/// Tell the platform whether this widget wants OS-level IME composition.
/// Mirrors egui/Firefox pattern: toggle enable on focus/blur transitions,
/// don't try to "cancel" composition mid-flight (winit 0.30 has no
/// reliable cancel API; toggling Disable+Enable in the same tick can
/// coalesce on some backends). Composition naturally ends when the IME
/// target loses focus.
fn set_platform_ime(meta: &EventMeta, enabled: bool) {
    let mut vp = meta.viewport();
    vp.ime_command(if enabled {
        ImeCommand::Enable
    } else {
        ImeCommand::Disable
    });
}

fn set_platform_ime_cursor_rect(text_area: &TextArea, meta: &EventMeta, arena: &NodeArena) {
    if !text_area.is_focused {
        return;
    }
    let Some((x, y, height)) = text_area.caret_screen_position(arena) else {
        return;
    };
    let mut vp = meta.viewport();
    vp.ime_command(ImeCommand::SetCursorRect(x, y, 1.0, height.max(1.0)));
}

impl EventTarget for TextArea {
    fn cursor(&self) -> crate::style::Cursor {
        self.cursor
    }

    fn wants_animation_frame(&self) -> bool {
        self.is_focused && self.layout_state.should_render
    }

    fn block_key_down_child_event(&self) -> bool {
        true
    }
    fn block_key_up_child_event(&self) -> bool {
        true
    }
    fn block_text_input_child_event(&self) -> bool {
        true
    }
    fn block_ime_preedit_child_event(&self) -> bool {
        true
    }
    fn block_focus_child_event(&self) -> bool {
        true
    }

    fn dispatch_pointer_down(
        &mut self,
        event: &mut PointerDownEvent,
        control: &mut ViewportControl<'_>,
        arena: &NodeArena,
        self_key: NodeKey,
    ) {
        if event.pointer.button != Some(PointerButton::Left) {
            return;
        }
        control.set_focus(Some(self_key));
        self.set_focused(true);
        // Every click commits any in-flight preedit first, then places
        // caret in the *post-commit* content. The hit-test runs against
        // the Run's still-shaped (effective_text) buffer — that buffer
        // already includes the preedit chars, so the resulting char
        // index matches the new content. Suppress drag-select on the
        // commit-tap (matches v1).
        let had_preedit = !self.ime_preedit.is_empty() || self.ime_preedit_cursor.is_some();
        let committed = had_preedit && self.commit_preedit();
        let target =
            self.cursor_target_at_screen(arena, event.pointer.viewport_x, event.pointer.viewport_y);
        if event.pointer.modifiers.shift() {
            self.extend_selection_to(target.char_index);
            self.cursor_affinity = target.affinity;
            self.pointer_selecting = !had_preedit;
        } else {
            self.start_pointer_selection_with_affinity(target.char_index, target.affinity);
            if had_preedit {
                self.pointer_selecting = false;
            }
        }
        if self.pointer_selecting {
            control.set_pointer_capture(self_key);
        }
        if had_preedit {
            self.route_preedit_to_runs(arena);
            // Force OS IME engine to drop the in-flight composition
            // (focus stays here, so we re-enable immediately). Disable's
            // macOS path runs `discardMarkedText` against the input
            // context — without it the candidate / preedit reappears on
            // the next compose session even though our internal state is
            // clean. See `set_platform_ime` doc.
            set_platform_ime(&event.meta, false);
            set_platform_ime(&event.meta, true);
            if committed {
                self.notify_change_handlers();
            }
        }
        self.mark_caret_scroll_pending();
        self.scroll_caret_into_view(arena);
        set_platform_ime_cursor_rect(self, &event.meta, arena);
        event.meta.stop_propagation();
        control.request_redraw();
    }

    fn dispatch_pointer_move(
        &mut self,
        event: &mut PointerMoveEvent,
        control: &mut ViewportControl<'_>,
        arena: &NodeArena,
        self_key: NodeKey,
    ) {
        if !self.pointer_selecting {
            return;
        }
        let target =
            self.cursor_target_at_screen(arena, event.pointer.viewport_x, event.pointer.viewport_y);
        self.update_pointer_selection_with_affinity(target.char_index, target.affinity);
        self.scroll_caret_into_view(arena);
        set_platform_ime_cursor_rect(self, &event.meta, arena);
        event.meta.stop_propagation();
        control.request_redraw();
    }

    fn dispatch_pointer_up(
        &mut self,
        event: &mut PointerUpEvent,
        control: &mut ViewportControl<'_>,
        arena: &NodeArena,
        self_key: NodeKey,
    ) {
        if event.pointer.button == Some(PointerButton::Left) {
            self.end_pointer_selection();
            control.release_pointer_capture(self_key);
            control.request_redraw();
        }
    }

    fn dispatch_focus(
        &mut self,
        event: &mut FocusEvent,
        control: &mut ViewportControl<'_>,
        arena: &NodeArena,
        self_key: NodeKey,
    ) {
        self.set_focused(true);
        self.mark_caret_scroll_pending();
        self.scroll_caret_into_view(arena);
        set_platform_ime(&event.meta, true);
        set_platform_ime_cursor_rect(self, &event.meta, arena);
        if !self.on_focus_handlers.is_empty() {
            let mut focus_event = TextAreaFocusEvent {
                meta: event.meta.clone(),
                target: event.meta.text_selection_target(self_key),
            };
            for handler in &self.on_focus_handlers {
                handler.call(&mut focus_event);
            }
        }
        control.request_redraw();
    }

    fn dispatch_blur(
        &mut self,
        event: &mut BlurEvent,
        control: &mut ViewportControl<'_>,
        arena: &NodeArena,
        _self_key: NodeKey,
    ) {
        // Blur commits any in-flight preedit (v2 divergence from v1,
        // which dropped it) so the user's composing text isn't lost when
        // focus leaves. Selection clears + drag-select ends as in v1;
        // `set_focused(false)` already resets `pointer_selecting`.
        self.set_focused(false);
        self.clear_selection();
        if self.commit_preedit() {
            self.notify_change_handlers();
        }
        self.route_preedit_to_runs(arena);
        set_platform_ime(&event.meta, false);
        for handler in &self.on_blur_handlers {
            handler.call(event);
        }
        control.request_redraw();
    }

    fn dispatch_key_down(
        &mut self,
        event: &mut KeyDownEvent,
        control: &mut ViewportControl<'_>,
        arena: &NodeArena,
        _self_key: NodeKey,
    ) {
        // Decision A7 / P4.1: keep keydown inert while the IME is composing.
        // Otherwise Enter / Backspace / arrows would mutate committed text
        // before the platform either commits or cancels the preedit.
        if !self.ime_preedit.is_empty() {
            return;
        }
        use crate::platform::input::Key;
        let key = event.key.key;
        let modifiers = event.key.modifiers;
        let shift = modifiers.shift();
        let shortcut = modifiers.ctrl() || modifiers.meta();
        // Word-grain modifier: Alt on macOS, Ctrl on Win/Linux. Ctrl on
        // macOS is reserved for system gestures (Mission Control / Spaces),
        // so don't mix it in there.
        let word = modifiers.alt() || (cfg!(not(target_os = "macos")) && modifiers.ctrl());
        // macOS Cmd+Arrow jumps to line / document edges (TextEdit / Safari).
        // Win/Linux equivalents are Home / End / Ctrl+Home / Ctrl+End and
        // route through the dedicated `Key::Home` / `Key::End` branches.
        let line_jump = cfg!(target_os = "macos") && modifiers.meta();
        let mut handled = true;
        let content_revision = self.unified_ifc_source_revision.get();

        match key {
            Key::ArrowLeft => {
                if line_jump {
                    self.handle_visual_line_jump(arena, false, shift);
                } else if shift && word {
                    self.extend_selection_word_left();
                } else if shift {
                    self.extend_selection_left();
                } else if word {
                    self.move_cursor_word_left();
                } else {
                    self.handle_horizontal_arrow(arena, false);
                }
            }
            Key::ArrowRight => {
                if line_jump {
                    self.handle_visual_line_jump(arena, true, shift);
                } else if !shift && !word && self.selection_range_chars().is_some() {
                    let (_, end) = self.selection_range_chars().unwrap();
                    self.move_cursor_to(end);
                } else if shift && word {
                    self.extend_selection_word_right();
                } else if shift {
                    self.extend_selection_right();
                } else if word {
                    self.move_cursor_word_right();
                } else {
                    self.handle_horizontal_arrow(arena, true);
                }
            }
            Key::ArrowUp => {
                if line_jump {
                    if shift {
                        self.extend_selection_to(0);
                    } else {
                        self.move_cursor_text_home();
                    }
                } else {
                    handled = self.handle_vertical_arrow(arena, VerticalDirection::Up, shift);
                }
            }
            Key::ArrowDown => {
                if line_jump {
                    let len = self.content_char_len();
                    if shift {
                        self.extend_selection_to(len);
                    } else {
                        self.move_cursor_text_end();
                    }
                } else {
                    handled = self.handle_vertical_arrow(arena, VerticalDirection::Down, shift);
                }
            }
            Key::Home => {
                if shortcut {
                    if shift {
                        self.extend_selection_to(0);
                    } else {
                        self.move_cursor_text_home();
                    }
                } else if shift {
                    self.extend_selection_line_home();
                } else {
                    self.move_cursor_line_home();
                }
            }
            Key::End => {
                if shortcut {
                    let len = self.content_char_len();
                    if shift {
                        self.extend_selection_to(len);
                    } else {
                        self.move_cursor_text_end();
                    }
                } else if shift {
                    self.extend_selection_line_end();
                } else {
                    self.move_cursor_line_end();
                }
            }
            Key::Backspace if !self.read_only => {
                if word {
                    self.delete_prev_word();
                } else {
                    self.delete_backspace();
                }
            }
            Key::Delete if !self.read_only => {
                if word {
                    self.delete_next_word();
                } else {
                    self.delete_forward();
                }
            }
            Key::Enter | Key::NumberPadEnter if !self.read_only && self.multiline => {
                self.insert_text("\n");
            }
            Key::Tab if !self.read_only => {
                self.insert_text("    ");
            }
            Key::KeyA if shortcut => {
                self.select_all();
            }
            // Vertical motion + clipboard punted to a follow-up pass.
            _ => {
                handled = false;
            }
        }

        if self.unified_ifc_source_revision.get() != content_revision {
            self.notify_change_handlers();
        }
        if handled {
            self.mark_caret_scroll_pending();
            self.scroll_caret_into_view(arena);
            set_platform_ime_cursor_rect(self, &event.meta, arena);
            event.meta.stop_propagation();
            control.request_redraw();
        }
    }

    fn dispatch_text_input(
        &mut self,
        event: &mut TextInputEvent,
        control: &mut ViewportControl<'_>,
        arena: &NodeArena,
        _self_key: NodeKey,
    ) {
        if self.read_only {
            return;
        }
        if event.text.is_empty() {
            return;
        }
        // IME commits travel via both `ImeCommitEvent` *and* this event
        // (with `input_type=ImeCommit`). `dispatch_ime_commit` owns the
        // commit insert + preedit clear; skip this path so the text isn't
        // inserted twice.
        if matches!(event.input_type, InputType::ImeCommit) {
            event.meta.stop_propagation();
            return;
        }
        let had_preedit = self.clear_preedit();
        if self.insert_text(event.text.as_str()) {
            self.notify_change_handlers();
            if had_preedit {
                self.route_preedit_to_runs(arena);
            }
            self.scroll_caret_into_view(arena);
            set_platform_ime_cursor_rect(self, &event.meta, arena);
            event.meta.stop_propagation();
            control.request_redraw();
        }
    }

    fn dispatch_ime_preedit(
        &mut self,
        event: &mut ImePreeditEvent,
        control: &mut ViewportControl<'_>,
        arena: &NodeArena,
        _self_key: NodeKey,
    ) {
        if self.read_only {
            return;
        }
        if event.text.is_empty() {
            self.clear_preedit();
        } else {
            self.set_preedit(event.text.clone(), event.cursor);
        }
        self.route_preedit_to_runs(arena);
        self.scroll_caret_into_view(arena);
        set_platform_ime_cursor_rect(self, &event.meta, arena);
        event.meta.stop_propagation();
        control.request_redraw();
    }

    fn dispatch_ime_commit(
        &mut self,
        event: &mut ImeCommitEvent,
        control: &mut ViewportControl<'_>,
        arena: &NodeArena,
        _self_key: NodeKey,
    ) {
        // Owns the commit pipeline: clear preedit state, insert committed
        // text, push preedit clearance to the target Run. The sibling
        // `TextInputEvent` (input_type=ImeCommit) is short-circuited in
        // `dispatch_text_input` so the text doesn't insert twice. Doing
        // the work here (rather than only in TextInputEvent) keeps the
        // commit atomic for callers that observe via this lifecycle event.
        if self.read_only {
            event.meta.stop_propagation();
            return;
        }
        self.clear_preedit();
        let inserted = !event.text.is_empty() && self.insert_text(event.text.as_str());
        if inserted {
            self.notify_change_handlers();
        }
        self.route_preedit_to_runs(arena);
        self.scroll_caret_into_view(arena);
        set_platform_ime_cursor_rect(self, &event.meta, arena);
        event.meta.stop_propagation();
        control.request_redraw();
    }

    fn dispatch_ime_enabled(
        &mut self,
        event: &mut ImeEnabledEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &NodeArena,
        _self_key: NodeKey,
    ) {
        // Lifecycle observability hook: nothing to mutate at start of
        // composition. Stop propagation so ancestors can't double-handle
        // the IME session belonging to this TextArea.
        set_platform_ime_cursor_rect(self, &event.meta, _arena);
        event.meta.stop_propagation();
    }

    fn dispatch_ime_disabled(
        &mut self,
        event: &mut ImeDisabledEvent,
        control: &mut ViewportControl<'_>,
        arena: &NodeArena,
        _self_key: NodeKey,
    ) {
        // Composition ended without a separate commit (cancel / focus
        // change / disable). Drop any half-built preedit so the next
        // session starts clean.
        if self.clear_preedit() {
            self.route_preedit_to_runs(arena);
            control.request_redraw();
        }
        event.meta.stop_propagation();
    }

    fn dispatch_copy(
        &mut self,
        event: &mut CopyEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &NodeArena,
        _self_key: NodeKey,
    ) {
        if let Some(text) = self.selected_text() {
            event.data.set_text(text);
        }
        event.meta.stop_propagation();
    }

    fn dispatch_cut(
        &mut self,
        event: &mut CutEvent,
        control: &mut ViewportControl<'_>,
        _arena: &NodeArena,
        _self_key: NodeKey,
    ) {
        let Some(text) = self.selected_text() else {
            event.meta.stop_propagation();
            return;
        };
        event.data.set_text(text);
        if !self.read_only {
            if self.delete_selected_text() {
                self.notify_change_handlers();
            }
            control.request_redraw();
        }
        event.meta.stop_propagation();
    }

    fn dispatch_paste(
        &mut self,
        event: &mut PasteEvent,
        control: &mut ViewportControl<'_>,
        arena: &NodeArena,
        _self_key: NodeKey,
    ) {
        if self.read_only {
            event.meta.stop_propagation();
            return;
        }
        let Some(text) = event.data.text() else {
            event.meta.stop_propagation();
            return;
        };
        if text.is_empty() {
            event.meta.stop_propagation();
            return;
        }
        if self.insert_text(&text) {
            self.notify_change_handlers();
            self.scroll_caret_into_view(arena);
            set_platform_ime_cursor_rect(self, &event.meta, arena);
            control.request_redraw();
        }
        event.meta.stop_propagation();
    }
}
