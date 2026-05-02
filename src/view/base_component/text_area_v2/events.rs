//! `EventTarget` impls for `TextArea2`.
//!
//! Decision A6: TextArea2 returns `true` from all 5 `block_*_child_event`
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
    BlurEvent, EventMeta, FocusEvent, ImeCommitEvent, ImeDisabledEvent, ImeEnabledEvent,
    ImePreeditEvent, InputType, KeyDownEvent, PointerButton, PointerDownEvent, PointerMoveEvent,
    PointerUpEvent, TextAreaFocusEvent, TextInputEvent,
};
use crate::view::base_component::EventTarget;
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::viewport::ViewportControl;

use super::TextArea2;

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

fn set_platform_ime_cursor_rect(text_area: &TextArea2, meta: &EventMeta, arena: &NodeArena) {
    if !text_area.is_focused {
        return;
    }
    let Some((x, y, height)) = text_area.caret_screen_position(arena) else {
        return;
    };
    let mut vp = meta.viewport();
    vp.ime_command(ImeCommand::SetCursorRect(
        x,
        y,
        1.0,
        height.max(1.0),
    ));
}

impl EventTarget for TextArea2 {
    fn cursor(&self) -> crate::style::Cursor {
        crate::style::Cursor::Text
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
        let had_preedit =
            !self.ime_preedit.is_empty() || self.ime_preedit_cursor.is_some();
        let committed = had_preedit && self.commit_preedit();
        let target_char = self.cursor_char_at_screen(
            arena,
            event.pointer.viewport_x,
            event.pointer.viewport_y,
        );
        if event.pointer.modifiers.shift() {
            self.extend_selection_to(target_char);
            self.pointer_selecting = !had_preedit;
        } else {
            self.start_pointer_selection(target_char);
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
        let target_char = self.cursor_char_at_screen(
            arena,
            event.pointer.viewport_x,
            event.pointer.viewport_y,
        );
        self.update_pointer_selection(target_char);
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
        let mut handled = true;
        let prev_content = self.content.clone();

        match key {
            Key::ArrowLeft => {
                if !shift && self.selection_range_chars().is_some() {
                    let (start, _) = self.selection_range_chars().unwrap();
                    self.move_cursor_to(start);
                } else if shift {
                    self.extend_selection_left();
                } else {
                    self.move_cursor_left();
                }
            }
            Key::ArrowRight => {
                if !shift && self.selection_range_chars().is_some() {
                    let (_, end) = self.selection_range_chars().unwrap();
                    self.move_cursor_to(end);
                } else if shift {
                    self.extend_selection_right();
                } else {
                    self.move_cursor_right();
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
                self.delete_backspace();
            }
            Key::Delete if !self.read_only => {
                self.delete_forward();
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

        if self.content != prev_content {
            self.notify_change_handlers();
        }
        if handled {
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
        let prev = self.content.clone();
        if self.insert_text(event.text.as_str()) {
            if self.content != prev {
                self.notify_change_handlers();
            }
            if had_preedit {
                self.route_preedit_to_runs(arena);
            }
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
        let inserted =
            !event.text.is_empty() && self.insert_text(event.text.as_str());
        if inserted {
            self.notify_change_handlers();
        }
        self.route_preedit_to_runs(arena);
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
        // the IME session belonging to this TextArea2.
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
}
