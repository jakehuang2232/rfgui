use super::*;
use crate::view::base_component::{
    BoxModelSnapshot, DirtyPassMask, Element, ElementTrait, hit_test,
};

impl Viewport {
    pub(super) fn hit_test_pointer_target(
        arena: &crate::view::node_arena::NodeArena,
        popup_stack: &crate::view::popup_stack::PopupStack,
        root_keys: &[crate::view::node_arena::NodeKey],
        x: f32,
        y: f32,
    ) -> Option<(
        crate::view::node_arena::NodeKey,
        crate::view::node_arena::NodeKey,
    )> {
        crate::view::base_component::hit_test_pointer_target(arena, popup_stack, root_keys, x, y)
            .map(|target| (target.root_key, target.target_key))
    }

    #[doc(hidden)]
    pub fn dispatch_pointer_down_event(&mut self, button: PointerButton) -> bool {
        let Some((x, y)) = self.pointer_position_viewport() else {
            return false;
        };
        self.input_state.pending_click = None;
        let focus_before = self.focused_node_id();
        let buttons = self.current_ui_pointer_buttons();
        let meta = EventMeta::new(NodeId::default());
        let mut event = PointerDownEvent {
            meta: meta.clone(),
            pointer: PointerEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(button),
                buttons,
                modifiers: self.current_key_modifiers(),
                pointer_id: 0,
                pointer_type: PointerType::Mouse,
                pressure: 0.5,
                timestamp: crate::time::Instant::now(),
            },
            viewport: meta.viewport(),
        };
        let root_keys = self.scene.ui_root_keys.clone();
        let hit_target = Self::hit_test_pointer_target(
            &self.scene.node_arena,
            &self.scene.popup_stack,
            &root_keys,
            x,
            y,
        );
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            if let Some((root_key, target_key)) = hit_target {
                if crate::view::viewport::dispatch::dispatch_pointer_down_to_target(
                    &arena,
                    root_key,
                    target_key,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        if handled {
            // Promote the popup that absorbed the click to the top of the
            // stack so subsequent renders + hit-tests treat it as topmost.
            let target_id = event.meta.target_id();
            if let Some(sid) = crate::view::viewport::dispatch::nearest_viewport_clip_ancestor_id(
                &self.scene.node_arena,
                target_id,
            ) {
                self.scene.popup_stack.promote(sid);
            }
            self.input_state.pending_click = Some(PendingClick {
                button,
                target_id,
                viewport_x: x,
                viewport_y: y,
            });
        }
        if let Some(capture_target_id) = event.meta.pointer_capture_target_id() {
            self.input_state.pointer_capture_node_id = Some(capture_target_id);
        }
        self.apply_viewport_listener_actions(event.meta.take_viewport_listener_actions());
        crate::ui::dispatch_viewport_pointer_down_hook(crate::ui::ViewportPointerDownEvent {
            meta: event.meta.snapshot(),
            pointer: event.pointer,
        });
        self.sync_focus_dispatch();
        if handled {
            let clicked_target = event.meta.target_id();
            let keep_focus_requested = event.meta.focus_change_suppressed();
            let focus_after = self.focused_node_id();
            let focus_changed_by_handler = focus_after != focus_before;
            let clicked_within_focused_subtree = focus_before.is_some_and(|focus_id| {
                let arena = &self.scene.node_arena;
                self.scene.ui_root_keys.iter().rev().any(|&root_key| {
                    crate::view::base_component::subtree_contains_node(
                        arena,
                        root_key,
                        focus_id,
                        clicked_target,
                    )
                })
            });
            if !focus_changed_by_handler {
                if keep_focus_requested || clicked_within_focused_subtree {
                    // Keep existing focus during controlled interactions or subtree clicks.
                } else if Some(clicked_target) != focus_before {
                    self.input_state.pending_focus_reason = crate::ui::FocusReason::Pointer;
                    self.set_focused_node_id(Some(clicked_target));
                    self.sync_focus_dispatch();
                    self.input_state.pending_focus_reason = crate::ui::FocusReason::Programmatic;
                }
            }
            self.request_redraw();
        } else if self.focused_node_id().is_some() {
            self.input_state.pending_focus_reason = crate::ui::FocusReason::Pointer;
            self.set_focused_node_id(None);
            self.sync_focus_dispatch();
            self.input_state.pending_focus_reason = crate::ui::FocusReason::Programmatic;
            self.request_redraw();
        }
        handled
    }

    #[doc(hidden)]
    pub fn dispatch_pointer_up_event(&mut self, button: PointerButton) -> bool {
        let Some((x, y)) = self.pointer_position_viewport() else {
            self.input_state.pointer_capture_node_id = None;
            let root_keys = self.scene.ui_root_keys.clone();
            let changed = Self::cancel_pointer_interactions(&self.scene.node_arena, &root_keys);
            if changed {
                self.request_redraw();
            }
            return false;
        };
        // Drag-active: close the drag gesture instead of running the
        // normal pointer_up path.
        if self.input_state.drag_state.is_some() {
            let _ = button;
            self.input_state.pending_click = None;
            return self.handle_drag_up(x, y);
        }
        let buttons = self.current_ui_pointer_buttons();
        let meta = EventMeta::new(NodeId::default());
        let mut event = PointerUpEvent {
            meta: meta.clone(),
            pointer: PointerEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(button),
                buttons,
                modifiers: self.current_key_modifiers(),
                pointer_id: 0,
                pointer_type: PointerType::Mouse,
                pressure: 0.5,
                timestamp: crate::time::Instant::now(),
            },
            viewport: meta.viewport(),
        };
        let root_keys = self.scene.ui_root_keys.clone();
        let hit_target = Self::hit_test_pointer_target(
            &self.scene.node_arena,
            &self.scene.popup_stack,
            &root_keys,
            x,
            y,
        );
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            if let Some(target_id) = control.viewport.pointer_capture_node_id() {
                for &root_key in root_keys.iter().rev() {
                    if crate::view::viewport::dispatch::dispatch_pointer_up_to_target(
                        &arena,
                        root_key,
                        target_id,
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                        break;
                    }
                }
                control.viewport.set_pointer_capture_node_id(None);
            } else {
                if let Some((root_key, target_key)) = hit_target {
                    if crate::view::viewport::dispatch::dispatch_pointer_up_to_target(
                        &arena,
                        root_key,
                        target_key,
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                    }
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        crate::ui::dispatch_viewport_pointer_up_hook(crate::ui::ViewportPointerUpEvent {
            meta: event.meta.snapshot(),
            pointer: event.pointer,
        });
        self.sync_focus_dispatch();
        if handled {
            self.request_redraw();
        }
        handled
    }

    #[doc(hidden)]
    pub fn dispatch_pointer_move_event(&mut self) -> bool {
        let Some((x, y)) = self.pointer_position_viewport() else {
            return false;
        };
        // Drag-active: route the move through DragOver / DragLeave
        // instead of the normal hover+move path.
        if self.input_state.drag_state.is_some() {
            return self.handle_drag_move(x, y);
        }
        let redraw_requested_before = self.redraw_requested;
        let root_keys = self.scene.ui_root_keys.clone();
        let hit_target = Self::hit_test_pointer_target(
            &self.scene.node_arena,
            &self.scene.popup_stack,
            &root_keys,
            x,
            y,
        );
        let hover_target = hit_target.map(|(_, t)| t);
        let buttons = self.current_ui_pointer_buttons();
        let pointer_data = PointerEventData {
            viewport_x: x,
            viewport_y: y,
            local_x: 0.0,
            local_y: 0.0,
            button: None,
            buttons,
            modifiers: self.current_key_modifiers(),
            pointer_id: 0,
            pointer_type: PointerType::Mouse,
            pressure: 0.0,
            timestamp: crate::time::Instant::now(),
        };
        let (hover_changed, hover_event_dispatched) = Self::sync_hover_target(
            &self.scene.node_arena,
            &root_keys,
            &mut self.input_state.hovered_node_id,
            hover_target,
            pointer_data,
        );
        let meta = EventMeta::new(NodeId::default());
        let mut event = PointerMoveEvent {
            meta: meta.clone(),
            pointer: pointer_data,
            viewport: meta.viewport(),
        };
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            if let Some(target_id) = control.viewport.pointer_capture_node_id() {
                for &root_key in root_keys.iter().rev() {
                    if crate::view::viewport::dispatch::dispatch_pointer_move_to_target(
                        &arena,
                        root_key,
                        target_id,
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                        break;
                    }
                }
                if !handled {
                    control.viewport.set_pointer_capture_node_id(None);
                }
            } else {
                if let Some((root_key, target_key)) = hit_target {
                    if crate::view::viewport::dispatch::dispatch_pointer_move_to_target(
                        &arena,
                        root_key,
                        target_key,
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                    }
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        crate::ui::dispatch_viewport_pointer_move_hook(crate::ui::ViewportPointerMoveEvent {
            meta: event.meta.snapshot(),
            pointer: event.pointer,
        });
        self.sync_focus_dispatch();
        let redraw_requested_during_event = !redraw_requested_before && self.redraw_requested;
        if hover_changed || hover_event_dispatched || redraw_requested_during_event {
            self.request_redraw();
        }
        handled || hover_changed || hover_event_dispatched
    }

    #[doc(hidden)]
    pub fn dispatch_click_event(&mut self, button: PointerButton) -> bool {
        let Some((x, y)) = self.pointer_position_viewport() else {
            return false;
        };
        let Some(pending_click) = self.input_state.pending_click.take() else {
            return false;
        };
        if pending_click.button != button {
            return false;
        }
        let buttons = self.current_ui_pointer_buttons();
        let root_keys = self.scene.ui_root_keys.clone();
        let hit_target = Self::hit_test_pointer_target(
            &self.scene.node_arena,
            &self.scene.popup_stack,
            &root_keys,
            x,
            y,
        )
        .map(|(_, t)| t);
        let is_valid_click = is_valid_click_candidate(pending_click, button, hit_target, x, y);
        if !is_valid_click {
            return false;
        }
        let now = crate::time::Instant::now();
        // `click_count` only tracks the left-button primary click stream.
        // Right-button becomes `contextmenu`, which carries no count; other
        // buttons (middle/back/forward) each count independently.
        let click_count = crate::view::viewport::input::compute_click_count(
            self.input_state.last_click,
            button,
            pending_click.target_id,
            x,
            y,
            now,
        );
        if !matches!(button, PointerButton::Right) {
            self.input_state.last_click = Some(crate::view::viewport::input::LastClick {
                button,
                target_id: pending_click.target_id,
                viewport_x: x,
                viewport_y: y,
                timestamp: now,
                count: click_count,
            });
        }
        let pointer = PointerEventData {
            viewport_x: x,
            viewport_y: y,
            local_x: 0.0,
            local_y: 0.0,
            button: Some(button),
            buttons,
            modifiers: self.current_key_modifiers(),
            pointer_id: 0,
            pointer_type: PointerType::Mouse,
            pressure: 0.0,
            timestamp: now,
        };
        // Right-button clicks surface as `ContextMenuEvent` (matching DOM
        // `contextmenu`) rather than a plain click. Left/middle/back/forward
        // keep firing `ClickEvent`.
        let is_context_menu = matches!(button, PointerButton::Right);
        let mut handled = false;
        let pending_actions = if is_context_menu {
            let mut event = crate::ui::ContextMenuEvent {
                meta: EventMeta::new(NodeId::default()),
                pointer,
            };
            {
                event.meta.attach_dispatch_ctx(&*self);
                let (arena, mut control) = self.borrow_for_dispatch();
                for &root_key in root_keys.iter().rev() {
                    if crate::view::viewport::dispatch::dispatch_context_menu_to_target(
                        &arena,
                        root_key,
                        pending_click.target_id,
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                        break;
                    }
                }
            }
            event.meta.detach_dispatch_ctx();
            event.meta.take_viewport_listener_actions()
        } else {
            let mut event = ClickEvent {
                meta: EventMeta::new(NodeId::default()),
                pointer,
                click_count,
            };
            {
                event.meta.attach_dispatch_ctx(&*self);
                let (arena, mut control) = self.borrow_for_dispatch();
                for &root_key in root_keys.iter().rev() {
                    if crate::view::viewport::dispatch::dispatch_click_to_target(
                        &arena,
                        root_key,
                        pending_click.target_id,
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                        break;
                    }
                }
            }
            event.meta.detach_dispatch_ctx();
            event.meta.take_viewport_listener_actions()
        };
        self.apply_viewport_listener_actions(pending_actions);
        if handled {
            self.request_redraw();
        }
        handled
    }

    #[doc(hidden)]
    pub fn dispatch_pointer_wheel_event(&mut self, delta_x: f32, delta_y: f32) -> bool {
        self.dispatch_pointer_wheel_event_full(
            delta_x,
            delta_y,
            crate::platform::input::WheelDeltaMode::Pixel,
            crate::platform::input::WheelPhase::Changed,
        )
    }

    pub fn dispatch_pointer_wheel_event_full(
        &mut self,
        delta_x: f32,
        delta_y: f32,
        delta_mode: crate::platform::input::WheelDeltaMode,
        phase: crate::platform::input::WheelPhase,
    ) -> bool {
        let Some((x, y)) = self.pointer_position_viewport() else {
            return false;
        };
        // Surface the event to user handlers first. A handler that calls
        // `meta.prevent_default()` suppresses the built-in scroll routing
        // — lets apps trap ctrl+wheel as zoom, implement custom scroll
        // containers, etc.
        let modifiers = self.current_key_modifiers();
        let now = crate::time::Instant::now();
        let wheel_root_keys = self.scene.ui_root_keys.clone();
        let mut wheel_event = crate::ui::WheelEvent {
            meta: EventMeta::new(NodeId::default()),
            viewport_x: x,
            viewport_y: y,
            local_x: 0.0,
            local_y: 0.0,
            delta_x,
            delta_y,
            delta_mode,
            phase,
            modifiers,
            timestamp: now,
        };
        let wheel_hit = Self::hit_test_pointer_target(
            &self.scene.node_arena,
            &self.scene.popup_stack,
            &wheel_root_keys,
            x,
            y,
        );
        let mut wheel_user_handled = false;
        {
            wheel_event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            if let Some((root_key, target_key)) = wheel_hit {
                if crate::view::viewport::dispatch::dispatch_wheel_to_target(
                    arena,
                    root_key,
                    target_key,
                    &mut wheel_event,
                    &mut control,
                ) {
                    wheel_user_handled = true;
                }
            }
        }
        wheel_event.meta.detach_dispatch_ctx();
        let wheel_actions = wheel_event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(wheel_actions);
        if wheel_event.meta.default_prevented() {
            if wheel_user_handled {
                self.request_redraw();
            }
            return wheel_user_handled;
        }
        let mut pending_scroll_track: Option<(TrackTarget, (f32, f32), (f32, f32))> = None;
        let root_keys = self.scene.ui_root_keys.clone();
        let Some((root_index, target_key)) = Self::find_scroll_handler_at_pointer(
            &self.scene.node_arena,
            &self.scene.popup_stack,
            &root_keys,
            x,
            y,
            delta_x,
            delta_y,
        ) else {
            return false;
        };
        // Cross-frame scroll track keys are u64 stable_ids; resolve once.
        let target_stable_id = self
            .scene
            .node_arena
            .get(target_key)
            .map(|n| n.element.stable_id())
            .unwrap_or(0);
        if let Some(&root_key) = root_keys.get(root_index) {
            if let Some(from) = crate::view::viewport::dispatch::get_scroll_offset_by_id(
                &self.scene.node_arena,
                root_key,
                target_stable_id,
            ) {
                let _ = crate::view::viewport::dispatch::dispatch_scroll_to_target(
                    &self.scene.node_arena,
                    root_key,
                    target_key,
                    delta_x,
                    delta_y,
                );
                if let Some(to) = crate::view::viewport::dispatch::get_scroll_offset_by_id(
                    &self.scene.node_arena,
                    root_key,
                    target_stable_id,
                ) {
                    let _ = crate::view::viewport::dispatch::set_scroll_offset_by_id(
                        &self.scene.node_arena,
                        root_key,
                        target_stable_id,
                        from,
                    );

                    if (to.0 - from.0).abs() > 0.001 || (to.1 - from.1).abs() > 0.001 {
                        pending_scroll_track = Some((target_stable_id, from, to));
                    }
                }
            }
        }
        let mut handled = false;
        if let Some((target_id, from, to)) = pending_scroll_track {
            let transition_spec = self.transitions.scroll_transition;
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            if (to.0 - from.0).abs() > 0.001 {
                let _ = self
                    .transitions
                    .scroll_transition_plugin
                    .start_scroll_track(
                        &mut host,
                        target_id,
                        ScrollAxis::X,
                        from.0,
                        to.0,
                        transition_spec,
                    );
            }
            if (to.1 - from.1).abs() > 0.001 {
                let _ = self
                    .transitions
                    .scroll_transition_plugin
                    .start_scroll_track(
                        &mut host,
                        target_id,
                        ScrollAxis::Y,
                        from.1,
                        to.1,
                        transition_spec,
                    );
            }
            handled = true;
        }
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub(super) fn find_scroll_handler_at_pointer(
        arena: &crate::view::node_arena::NodeArena,
        popup_stack: &crate::view::popup_stack::PopupStack,
        root_keys: &[crate::view::node_arena::NodeKey],
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
    ) -> Option<(usize, crate::view::node_arena::NodeKey)> {
        let (_, hit_target) = Self::hit_test_pointer_target(arena, popup_stack, root_keys, x, y)?;

        // Walk up from hit_target via arena.parent_of, stopping at the first
        // ancestor that reports `can_scroll_by`. Determine which root it sits
        // under by walking further up to the root.
        let handler_key = crate::view::viewport::dispatch::find_scroll_handler_from_target(
            arena, hit_target, hit_target, delta_x, delta_y,
        )?;
        // Find the root that contains handler_key.
        let mut cur = Some(handler_key);
        let mut root_of_handler: Option<crate::view::node_arena::NodeKey> = None;
        while let Some(k) = cur {
            if root_keys.iter().any(|&r| r == k) {
                root_of_handler = Some(k);
                break;
            }
            cur = arena.parent_of(k);
        }
        let root_of_handler = root_of_handler?;
        let root_index = root_keys.iter().position(|&r| r == root_of_handler)?;
        Some((root_index, handler_key))
    }

    #[doc(hidden)]
    pub fn dispatch_key_down_event(&mut self, data: KeyEventData) -> bool {
        let Some(target_id) = self.keyboard_dispatch_target() else {
            return false;
        };
        let mut event = KeyDownEvent {
            meta: EventMeta::new(target_id),
            key: data,
        };
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            for &root_key in root_keys.iter().rev() {
                if crate::view::viewport::dispatch::dispatch_key_down_bubble(
                    &arena,
                    root_key,
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        if handled {
            self.request_redraw();
        }
        handled
    }

    #[doc(hidden)]
    pub fn dispatch_key_up_event(&mut self, data: KeyEventData) -> bool {
        let Some(target_id) = self.keyboard_dispatch_target() else {
            return false;
        };
        let mut event = KeyUpEvent {
            meta: EventMeta::new(target_id),
            key: data,
        };
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            for &root_key in root_keys.iter().rev() {
                if crate::view::viewport::dispatch::dispatch_key_up_bubble(
                    &arena,
                    root_key,
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        if handled {
            self.request_redraw();
        }
        handled
    }

    #[doc(hidden)]
    pub fn dispatch_text_input_event(&mut self, text: String) -> bool {
        self.dispatch_text_input_event_full(text, crate::ui::InputType::Typing, false)
    }

    pub fn dispatch_text_input_event_full(
        &mut self,
        text: String,
        input_type: crate::ui::InputType,
        is_composing: bool,
    ) -> bool {
        if text.is_empty() {
            return false;
        }
        let Some(target_id) = self.keyboard_dispatch_target() else {
            return false;
        };
        let mut event = TextInputEvent {
            meta: EventMeta::new(target_id),
            text,
            input_type,
            is_composing,
        };
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            for &root_key in root_keys.iter().rev() {
                if crate::view::viewport::dispatch::dispatch_text_input_bubble(
                    &arena,
                    root_key,
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        if handled {
            self.request_redraw();
        }
        handled
    }

    #[doc(hidden)]
    pub fn dispatch_ime_preedit_event(
        &mut self,
        text: String,
        cursor: Option<(usize, usize)>,
    ) -> bool {
        self.dispatch_ime_preedit_event_full(text, cursor, None, Vec::new())
    }

    pub fn dispatch_ime_preedit_event_full(
        &mut self,
        text: String,
        cursor: Option<(usize, usize)>,
        selection: Option<(usize, usize)>,
        attributes: Vec<crate::ui::PreeditAttribute>,
    ) -> bool {
        let Some(target_id) = self.keyboard_dispatch_target() else {
            return false;
        };
        let mut event = ImePreeditEvent {
            meta: EventMeta::new(target_id),
            text,
            cursor,
            selection,
            attributes,
        };
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            for &root_key in root_keys.iter().rev() {
                if crate::view::viewport::dispatch::dispatch_ime_preedit_bubble(
                    &arena,
                    root_key,
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        if handled {
            self.request_redraw();
        }
        handled
    }

    /// Fire an [`ImeCommitEvent`] at the focused node. The IME composition
    /// has already closed by the time this fires; `text` is the committed
    /// final string. Observers can react independently of the regular
    /// [`TextInputEvent`] path (which carries the same text with
    /// `input_type = ImeCommit`).
    pub fn dispatch_ime_commit_event(&mut self, text: String) -> bool {
        let Some(target_id) = self.keyboard_dispatch_target() else {
            return false;
        };
        let mut event = crate::ui::ImeCommitEvent {
            meta: EventMeta::new(target_id),
            text,
        };
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            for &root_key in root_keys.iter().rev() {
                if crate::view::viewport::dispatch::dispatch_ime_commit_bubble(
                    &arena,
                    root_key,
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        if handled {
            self.request_redraw();
        }
        handled
    }

    /// Fire an [`ImeEnabledEvent`] at the focused node. Runners call this
    /// when a composition window opens.
    pub fn dispatch_ime_enabled_event(&mut self) -> bool {
        let Some(target_id) = self.keyboard_dispatch_target() else {
            return false;
        };
        let mut event = crate::ui::ImeEnabledEvent {
            meta: EventMeta::new(target_id),
        };
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            for &root_key in root_keys.iter().rev() {
                if crate::view::viewport::dispatch::dispatch_ime_enabled_bubble(
                    &arena,
                    root_key,
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        if handled {
            self.request_redraw();
        }
        handled
    }

    /// Fire an [`ImeDisabledEvent`] at the focused node. Runners call
    /// this when a composition window closes (either committed or
    /// cancelled).
    pub fn dispatch_ime_disabled_event(&mut self) -> bool {
        let Some(target_id) = self.keyboard_dispatch_target() else {
            return false;
        };
        let mut event = crate::ui::ImeDisabledEvent {
            meta: EventMeta::new(target_id),
        };
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            for &root_key in root_keys.iter().rev() {
                if crate::view::viewport::dispatch::dispatch_ime_disabled_bubble(
                    &arena,
                    root_key,
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        if handled {
            self.request_redraw();
        }
        handled
    }

    /// Fire a [`CopyEvent`] at the focused node. Handlers fill `data`
    /// with the text they want on the clipboard; if nothing gets filled
    /// and no handler calls `prevent_default`, the viewport performs
    /// no default copy (future: copy the current text selection).
    pub fn dispatch_copy_event(&mut self) -> bool {
        let Some(target_id) = self.keyboard_dispatch_target() else {
            return false;
        };
        let data = crate::ui::DataTransfer::new();
        let mut event = crate::ui::CopyEvent {
            meta: EventMeta::new(target_id),
            data: data.clone(),
        };
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            for &root_key in root_keys.iter().rev() {
                if crate::view::viewport::dispatch::dispatch_copy_bubble(
                    &arena,
                    root_key,
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        // Default action: when a handler ran and did not `prevent_default`,
        // take the first text item from `data` and queue it to the host
        // clipboard. Future: fall back to "copy current selection" when
        // `data` is empty.
        if handled && !event.meta.default_prevented() {
            if let Some(text) = data.text() {
                self.set_clipboard_text(text);
            }
        }
        if handled {
            self.request_redraw();
        }
        handled
    }

    /// Fire a [`CutEvent`] at the focused node. Handlers are expected to
    /// fill `data` with the text *and* delete the selected region. The
    /// viewport queues the text to the clipboard just like copy.
    pub fn dispatch_cut_event(&mut self) -> bool {
        let Some(target_id) = self.keyboard_dispatch_target() else {
            return false;
        };
        let data = crate::ui::DataTransfer::new();
        let mut event = crate::ui::CutEvent {
            meta: EventMeta::new(target_id),
            data: data.clone(),
        };
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            for &root_key in root_keys.iter().rev() {
                if crate::view::viewport::dispatch::dispatch_cut_bubble(
                    &arena,
                    root_key,
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        if handled && !event.meta.default_prevented() {
            if let Some(text) = data.text() {
                self.set_clipboard_text(text);
            }
        }
        if handled {
            self.request_redraw();
        }
        handled
    }

    /// Fire a [`PasteEvent`] at the focused node, carrying `text` read
    /// from the OS clipboard by the runner. Handlers call
    /// `event.data.text()` to read.
    pub fn dispatch_paste_event(&mut self, text: String) -> bool {
        let Some(target_id) = self.keyboard_dispatch_target() else {
            return false;
        };
        let mut data = crate::ui::DataTransfer::new();
        data.set_text(text);
        let mut event = crate::ui::PasteEvent {
            meta: EventMeta::new(target_id),
            data,
        };
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            for &root_key in root_keys.iter().rev() {
                if crate::view::viewport::dispatch::dispatch_paste_bubble(
                    &arena,
                    root_key,
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        if handled {
            self.request_redraw();
        }
        handled
    }

    // ---------------------------------------------------------------
    // Drag & drop dispatch entry points
    // ---------------------------------------------------------------

    /// Fire [`crate::ui::DragStartEvent`] at `source_id`. Called by the
    /// engine immediately after a `StartDrag` command is applied.
    fn dispatch_drag_start_event(
        &mut self,
        source_id: NodeId,
        pointer: PointerEventData,
        data: crate::ui::DataTransfer,
    ) -> bool {
        let mut event = crate::ui::DragStartEvent {
            meta: EventMeta::new(source_id),
            pointer,
            data,
        };
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            for &root_key in root_keys.iter().rev() {
                if crate::view::viewport::dispatch::dispatch_drag_start_bubble(
                    &arena,
                    root_key,
                    source_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        handled
    }

    /// Fire [`crate::ui::DragOverEvent`] at `target_id`. Returns the
    /// `drop_effect` the handler chose (or `None` if refused).
    fn dispatch_drag_over_event(
        &mut self,
        target_id: NodeId,
        pointer: PointerEventData,
        data: crate::ui::DataTransfer,
    ) -> Option<crate::ui::DragEffect> {
        let mut event = crate::ui::DragOverEvent {
            meta: EventMeta::new(target_id),
            pointer,
            data,
            drop_effect: None,
        };
        let root_keys = self.scene.ui_root_keys.clone();
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            for &root_key in root_keys.iter().rev() {
                if crate::view::viewport::dispatch::dispatch_drag_over_bubble(
                    &arena,
                    root_key,
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    break;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        event.drop_effect
    }

    /// Fire [`crate::ui::DragLeaveEvent`] at `target_id`. Non-bubbling.
    fn dispatch_drag_leave_event(&mut self, target_id: NodeId) -> bool {
        let (arena, mut control) = self.borrow_for_dispatch();
        crate::view::viewport::dispatch::dispatch_drag_leave_to_key(&arena, target_id, &mut control)
    }

    /// Fire [`crate::ui::DropEvent`] at `target_id` with the resolved
    /// `effect` from the prior DragOver.
    fn dispatch_drop_event(
        &mut self,
        target_id: NodeId,
        pointer: PointerEventData,
        data: crate::ui::DataTransfer,
        effect: crate::ui::DragEffect,
    ) -> bool {
        let mut event = crate::ui::DropEvent {
            meta: EventMeta::new(target_id),
            pointer,
            data,
            effect,
        };
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            for &root_key in root_keys.iter().rev() {
                if crate::view::viewport::dispatch::dispatch_drop_bubble(
                    &arena,
                    root_key,
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        handled
    }

    /// Fire [`crate::ui::DragEndEvent`] at `source_id`.
    fn dispatch_drag_end_event(
        &mut self,
        source_id: NodeId,
        pointer: PointerEventData,
        effect: Option<crate::ui::DragEffect>,
    ) -> bool {
        let mut event = crate::ui::DragEndEvent {
            meta: EventMeta::new(source_id),
            pointer,
            effect,
        };
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            for &root_key in root_keys.iter().rev() {
                if crate::view::viewport::dispatch::dispatch_drag_end_bubble(
                    &arena,
                    root_key,
                    source_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        handled
    }

    /// True while a drag gesture is active.
    pub fn is_dragging(&self) -> bool {
        self.input_state.drag_state.is_some()
    }

    /// Handle a pointer_move while a drag gesture is active. Replaces
    /// the regular hover + move dispatch: hit-tests for a drop target,
    /// fires `DragLeave` on the previous target if it changed, then
    /// `DragOver` on the new one. Caches the handler's `drop_effect`
    /// back into the [`DragState`] so the eventual `Drop` knows which
    /// effect to carry.
    fn handle_drag_move(&mut self, x: f32, y: f32) -> bool {
        let arena_view = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let target =
            Self::hit_test_pointer_target(&arena_view, &self.scene.popup_stack, &root_keys, x, y)
                .map(|(_, t)| t);
        self.scene.node_arena = arena_view;

        let prev_target = self
            .input_state
            .drag_state
            .as_ref()
            .and_then(|s| s.last_over_target);
        let data = self.input_state.drag_state.as_ref().map(|s| s.data.clone());
        let Some(data) = data else {
            return false;
        };
        let pointer = synthetic_pointer_data(
            (x, y),
            self.current_key_modifiers(),
            self.current_ui_pointer_buttons(),
        );

        // Target changed → emit DragLeave on the previous one first.
        if prev_target != target {
            if let Some(prev) = prev_target {
                let _ = self.dispatch_drag_leave_event(prev);
            }
        }

        let mut drop_effect = None;
        if let Some(tgt) = target {
            drop_effect = self.dispatch_drag_over_event(tgt, pointer, data);
        }

        if let Some(state) = self.input_state.drag_state.as_mut() {
            state.last_over_target = target;
            state.last_drop_effect = drop_effect;
        }
        self.request_redraw();
        true
    }

    /// Handle pointer_up while a drag gesture is active. Fires `Drop`
    /// on the last DragOver target (if it accepted the drop), then
    /// `DragEnd` on the source, then clears the drag state.
    fn handle_drag_up(&mut self, x: f32, y: f32) -> bool {
        let Some(state) = self.input_state.drag_state.take() else {
            return false;
        };
        let pointer = synthetic_pointer_data(
            (x, y),
            self.current_key_modifiers(),
            self.current_ui_pointer_buttons(),
        );
        let current_target = {
            let arena_view = std::mem::take(&mut self.scene.node_arena);
            let root_keys = self.scene.ui_root_keys.clone();
            let target = Self::hit_test_pointer_target(
                &arena_view,
                &self.scene.popup_stack,
                &root_keys,
                x,
                y,
            )
            .map(|(_, t)| t);
            self.scene.node_arena = arena_view;
            target
        };
        let drop_target = current_target.or(state.last_over_target);
        let effect = if let Some(target) = current_target {
            self.dispatch_drag_over_event(target, pointer, state.data.clone())
        } else {
            state.last_drop_effect
        };
        if let (Some(target), Some(effect)) = (drop_target, effect) {
            let _ = self.dispatch_drop_event(target, pointer, state.data.clone(), effect);
        }
        let _ = self.dispatch_drag_end_event(state.source_id, pointer, effect);
        self.request_redraw();
        true
    }

    #[doc(hidden)]
    pub fn dispatch_focus_event(&mut self, target_id: NodeId) -> bool {
        self.dispatch_focus_event_with_related(target_id, None)
    }

    pub(super) fn dispatch_focus_event_with_related(
        &mut self,
        target_id: NodeId,
        related: Option<NodeId>,
    ) -> bool {
        let reason = self.input_state.pending_focus_reason;
        let mut meta = EventMeta::new(target_id);
        meta.set_related_target(related.map(crate::ui::EventTarget::bare));
        meta.set_source(crate::ui::EventSource::Synthetic);
        let mut event = FocusEvent { meta, reason };
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            for &root_key in root_keys.iter().rev() {
                if crate::view::viewport::dispatch::dispatch_focus_bubble(
                    &arena,
                    root_key,
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        if handled {
            self.request_redraw();
        }
        handled
    }

    #[doc(hidden)]
    pub fn dispatch_blur_event(&mut self, target_id: NodeId) -> bool {
        self.dispatch_blur_event_with_related(target_id, None)
    }

    pub(super) fn dispatch_blur_event_with_related(
        &mut self,
        target_id: NodeId,
        related: Option<NodeId>,
    ) -> bool {
        let reason = self.input_state.pending_focus_reason;
        let mut meta = EventMeta::new(target_id);
        meta.set_related_target(related.map(crate::ui::EventTarget::bare));
        meta.set_source(crate::ui::EventSource::Synthetic);
        let mut event = BlurEvent { meta, reason };
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            event.meta.attach_dispatch_ctx(&*self);
            let (arena, mut control) = self.borrow_for_dispatch();
            for &root_key in root_keys.iter().rev() {
                if crate::view::viewport::dispatch::dispatch_blur_bubble(
                    &arena,
                    root_key,
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        event.meta.detach_dispatch_ctx();
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        if handled {
            self.request_redraw();
        }
        handled
    }

    /// Dispatch a platform-neutral pointer event.
    ///
    /// Canonical entry point for backends (winit, web, headless). Internally
    /// forwards to the legacy primitive-argument `dispatch_pointer_*` methods;
    /// those remain public for now so component tests and existing callers
    /// keep working. New backend code should only ever see this method.
    pub fn dispatch_platform_pointer_event(&mut self, event: &PlatformPointerEvent) -> bool {
        match event.kind {
            PlatformPointerEventKind::Down(button) => self.dispatch_pointer_down_event(button),
            PlatformPointerEventKind::Up(button) => self.dispatch_pointer_up_event(button),
            PlatformPointerEventKind::Move { x, y } => {
                self.set_pointer_position_viewport(x, y);
                self.dispatch_pointer_move_event()
            }
            PlatformPointerEventKind::Click(button) => self.dispatch_click_event(button),
        }
    }

    pub fn dispatch_platform_wheel_event(&mut self, event: &PlatformWheelEvent) -> bool {
        self.dispatch_pointer_wheel_event_full(
            event.delta_x,
            event.delta_y,
            event.delta_mode,
            event.phase,
        )
    }

    pub fn dispatch_platform_key_event(&mut self, event: &PlatformKeyEvent) -> bool {
        let data = KeyEventData {
            key: event.key,
            characters: event.characters.clone(),
            modifiers: event.modifiers,
            repeat: event.repeat,
            is_composing: event.is_composing,
            location: crate::ui::KeyLocation::from_key(event.key),
            timestamp: event.timestamp,
        };
        if event.pressed {
            self.dispatch_key_down_event(data)
        } else {
            self.dispatch_key_up_event(data)
        }
    }

    pub fn dispatch_platform_text_input(&mut self, event: &PlatformTextInput) -> bool {
        self.dispatch_text_input_event_full(
            event.text.clone(),
            ui_input_type_from_platform(event.input_type),
            event.is_composing,
        )
    }

    pub fn dispatch_platform_ime_preedit(&mut self, event: &PlatformImePreedit) -> bool {
        let cursor = match (event.cursor_start, event.cursor_end) {
            (Some(start), Some(end)) => Some((start, end)),
            _ => None,
        };
        let selection = match (event.selection_start, event.selection_end) {
            (Some(start), Some(end)) => Some((start, end)),
            _ => None,
        };
        let attributes = event
            .attributes
            .iter()
            .map(|a| crate::ui::PreeditAttribute {
                start: a.start,
                end: a.end,
                style: match a.style {
                    crate::platform::input::PlatformPreeditStyle::Underline => {
                        crate::ui::PreeditStyle::Underline
                    }
                    crate::platform::input::PlatformPreeditStyle::DottedUnderline => {
                        crate::ui::PreeditStyle::DottedUnderline
                    }
                    crate::platform::input::PlatformPreeditStyle::Highlight => {
                        crate::ui::PreeditStyle::Highlight
                    }
                },
            })
            .collect();
        self.dispatch_ime_preedit_event_full(event.text.clone(), cursor, selection, attributes)
    }

    fn current_ui_pointer_buttons(&self) -> UiPointerButtons {
        UiPointerButtons {
            left: self.is_pointer_button_pressed(PointerButton::Left),
            right: self.is_pointer_button_pressed(PointerButton::Right),
            middle: self.is_pointer_button_pressed(PointerButton::Middle),
            back: self.is_pointer_button_pressed(PointerButton::Back),
            forward: self.is_pointer_button_pressed(PointerButton::Forward),
        }
    }

    pub fn focused_ime_cursor_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let target_key = self.keyboard_dispatch_target()?;
        let stable_id = self
            .scene
            .node_arena
            .get(target_key)
            .map(|n| n.element.stable_id())?;
        for &root_key in self.scene.ui_root_keys.iter().rev() {
            if let Some(rect) = crate::view::base_component::get_ime_cursor_rect_by_id(
                &self.scene.node_arena,
                root_key,
                stable_id,
            ) {
                return Some(rect);
            }
        }
        None
    }

    fn current_key_modifiers(&self) -> Modifiers {
        self.input_state.modifiers
    }

    /// Update the live modifier state. Backends call this on
    /// `WindowEvent::ModifiersChanged` (winit) or `keydown`/`keyup` (web)
    /// so pointer and key events can be tagged with the current set.
    pub fn set_modifiers(&mut self, modifiers: Modifiers) {
        self.input_state.modifiers = modifiers;
    }

    pub fn modifiers(&self) -> Modifiers {
        self.input_state.modifiers
    }

    pub(super) fn sync_focus_dispatch(&mut self) {
        if self.scene.ui_root_keys.is_empty() {
            return;
        }

        loop {
            let desired = self.input_state.focused_node_id;
            let dispatched = self.dispatched_focus_node_id;
            if desired == dispatched {
                break;
            }

            // Mark the in-flight target first so reentrant redraws triggered
            // by focus/blur handlers do not redispatch the same focus change.
            self.dispatched_focus_node_id = desired;

            if let Some(prev_id) = dispatched {
                // Blur's related_target = where focus is going next.
                let _ = self.dispatch_blur_event_with_related(prev_id, desired);
            }
            if let Some(next_id) = desired {
                // Focus's related_target = where focus came from.
                let _ = self.dispatch_focus_event_with_related(next_id, dispatched);
            }
        }
    }

    pub(super) fn resolve_cursor(&self) -> Cursor {
        if let Some(cursor) = self.cursor_override {
            return cursor;
        }
        let Some(target_key) = self.input_state.hovered_node_id else {
            return Cursor::Default;
        };
        self.scene
            .node_arena
            .get(target_key)
            .map(|node| node.element.cursor())
            .unwrap_or(Cursor::Default)
    }

    /// Record the currently-desired cursor into the pending platform
    /// request queue. Deduped against the last value recorded — the backend
    /// only sees changes.
    pub(super) fn notify_cursor_handler(&mut self) {
        let cursor = self.resolve_cursor();
        if self.last_recorded_cursor == Some(cursor) {
            return;
        }
        self.last_recorded_cursor = Some(cursor);
        self.pending_platform_requests.cursor = Some(cursor);
    }
}

impl Viewport {
    pub fn has_viewport_pointer_listeners(&self) -> bool {
        crate::ui::has_viewport_pointer_hooks()
            && !self.input_state.pressed_pointer_buttons.is_empty()
    }

    fn apply_viewport_listener_actions(&mut self, actions: Vec<EventCommand>) {
        let mut selection_changed = false;
        for action in actions {
            match action {
                EventCommand::SetFocus(node_id) => {
                    self.set_focused_node_id(node_id);
                }
                EventCommand::SetCursor(cursor) => {
                    self.set_cursor(cursor);
                }
                EventCommand::SelectTextRangeAll(target_id) => {
                    let root_keys = self.scene.ui_root_keys.clone();
                    let stable_id = self
                        .scene
                        .node_arena
                        .get(target_id)
                        .map(|n| n.element.stable_id())
                        .unwrap_or(0);
                    for &root_key in root_keys.iter().rev() {
                        if crate::view::base_component::select_all_text_by_id(
                            &self.scene.node_arena,
                            root_key,
                            stable_id,
                        ) {
                            selection_changed = true;
                            break;
                        }
                    }
                }
                EventCommand::SelectTextRange {
                    target_id,
                    start,
                    end,
                } => {
                    let root_keys = self.scene.ui_root_keys.clone();
                    let stable_id = self
                        .scene
                        .node_arena
                        .get(target_id)
                        .map(|n| n.element.stable_id())
                        .unwrap_or(0);
                    for &root_key in root_keys.iter().rev() {
                        if crate::view::base_component::select_text_range_by_id(
                            &self.scene.node_arena,
                            root_key,
                            stable_id,
                            start,
                            end,
                        ) {
                            selection_changed = true;
                            break;
                        }
                    }
                }
                EventCommand::RequestRedraw => {
                    self.request_redraw();
                }
                EventCommand::WriteClipboard(text) => {
                    self.set_clipboard_text(text);
                }
                EventCommand::ScrollIntoView { target_id, options } => {
                    let root_keys = self.scene.ui_root_keys.clone();
                    let scrolled = crate::view::viewport::dispatch::scroll_into_view_impl(
                        &self.scene.node_arena,
                        &root_keys,
                        target_id,
                        options,
                    );
                    if scrolled {
                        self.request_redraw();
                    }
                }
                EventCommand::KeyboardCapture(node_id) => {
                    self.input_state.keyboard_capture_node_id = node_id;
                }
                EventCommand::Window(command) => {
                    self.pending_platform_requests.window_commands.push(command);
                }
                EventCommand::Ime(command) => {
                    self.pending_platform_requests.ime_commands.push(command);
                }
                EventCommand::StartDrag {
                    source_id,
                    payload,
                    effect_allowed,
                } => {
                    // Fill a shared DataTransfer the DragStart handler can
                    // still mutate, then latch it into `drag_state` so
                    // subsequent DragOver / Drop see the same object.
                    let mut data = crate::ui::DataTransfer::with_items(payload.clone());
                    data.set_effect_allowed(effect_allowed);
                    self.input_state.drag_state = Some(crate::view::viewport::DragState {
                        source_id,
                        data: data.clone(),
                        effect_allowed,
                        last_over_target: None,
                        last_drop_effect: None,
                    });
                    // Tell the runner an OS-level drag should start (no-op
                    // on backends without a native drag bridge).
                    self.pending_platform_requests.pending_drags.push(
                        crate::platform::PendingDrag {
                            source_id,
                            payload,
                            effect_allowed,
                        },
                    );
                    // Fire DragStart synchronously so the handler can
                    // veto (future: prevent_default clears drag_state).
                    let pointer = synthetic_pointer_data(
                        self.input_state
                            .pointer_position_viewport
                            .unwrap_or((0.0, 0.0)),
                        self.current_key_modifiers(),
                        self.current_ui_pointer_buttons(),
                    );
                    let _ = self.dispatch_drag_start_event(source_id, pointer, data);
                }
                EventCommand::RequestPaste => {
                    self.pending_platform_requests.request_paste = true;
                }
            }
        }
        if selection_changed {
            self.request_redraw();
        }
    }

    pub fn set_selects(&mut self, selects: Vec<u64>) {
        self.input_state.selects = selects;
    }

    pub fn selects(&self) -> &[u64] {
        &self.input_state.selects
    }

    pub fn set_pointer_position_viewport(&mut self, x: f32, y: f32) {
        self.input_state.pointer_position_viewport = Some((x, y));
    }

    pub fn clear_pointer_position_viewport(&mut self) {
        let last_pos = self
            .input_state
            .pointer_position_viewport
            .unwrap_or((0.0, 0.0));
        self.input_state.pointer_position_viewport = None;
        self.input_state.pointer_capture_node_id = None;
        let root_keys = self.scene.ui_root_keys.clone();
        let pointer_data = synthetic_pointer_data(
            last_pos,
            self.current_key_modifiers(),
            self.current_ui_pointer_buttons(),
        );
        let (hover_changed, hover_event_dispatched) = Self::sync_hover_target(
            &self.scene.node_arena,
            &root_keys,
            &mut self.input_state.hovered_node_id,
            None,
            pointer_data,
        );
        let pointer_changed = Self::cancel_pointer_interactions(&self.scene.node_arena, &root_keys);
        if hover_changed || hover_event_dispatched || pointer_changed {
            self.request_redraw();
        }
    }

    pub fn pointer_position_viewport(&self) -> Option<(f32, f32)> {
        self.input_state.pointer_position_viewport
    }

    pub fn set_pointer_button_pressed(&mut self, button: PointerButton, pressed: bool) {
        if pressed {
            self.input_state.pressed_pointer_buttons.insert(button);
        } else {
            self.input_state.pressed_pointer_buttons.remove(&button);
        }
    }

    pub fn is_pointer_button_pressed(&self, button: PointerButton) -> bool {
        self.input_state.pressed_pointer_buttons.contains(&button)
    }

    pub fn pressed_pointer_buttons(&self) -> impl Iterator<Item = PointerButton> + '_ {
        self.input_state.pressed_pointer_buttons.iter().copied()
    }

    pub fn set_key_pressed(&mut self, key: impl Into<String>, pressed: bool) {
        let key = key.into();
        if pressed {
            self.input_state.pressed_keys.insert(key);
        } else {
            self.input_state.pressed_keys.remove(&key);
        }
    }

    pub fn is_key_pressed(&self, key: &str) -> bool {
        self.input_state.pressed_keys.contains(key)
    }

    pub fn pressed_keys(&self) -> impl Iterator<Item = &str> {
        self.input_state.pressed_keys.iter().map(String::as_str)
    }

    pub fn clear_input_state(&mut self) {
        self.set_focused_node_id(None);
        self.sync_focus_dispatch();
        let previous_hovered_node_id = self.input_state.hovered_node_id;
        self.input_state = InputState::default();
        self.input_state.hovered_node_id = previous_hovered_node_id;
        self.dispatched_focus_node_id = None;
        let root_keys = self.scene.ui_root_keys.clone();
        let pointer_data = synthetic_pointer_data(
            self.input_state
                .pointer_position_viewport
                .unwrap_or((0.0, 0.0)),
            self.current_key_modifiers(),
            self.current_ui_pointer_buttons(),
        );
        let (hover_changed, hover_event_dispatched) = Self::sync_hover_target(
            &self.scene.node_arena,
            &root_keys,
            &mut self.input_state.hovered_node_id,
            None,
            pointer_data,
        );
        let pointer_changed = Self::cancel_pointer_interactions(&self.scene.node_arena, &root_keys);
        if hover_changed || hover_event_dispatched || pointer_changed {
            self.request_redraw();
        }
    }

    /// Re-run hover hit-test at current pointer position without a real
    /// PointerMove. Used after layout-affecting changes (scroll, layout
    /// transitions) move elements under a stationary pointer so that
    /// PointerEnter/PointerLeave still fire.
    pub(super) fn resync_pointer_hover(&mut self) -> bool {
        if self.input_state.drag_state.is_some() {
            return false;
        }
        if self.input_state.pointer_capture_node_id.is_some() {
            return false;
        }
        let Some((x, y)) = self.pointer_position_viewport() else {
            return false;
        };
        let root_keys = self.scene.ui_root_keys.clone();
        let hover_target = Self::hit_test_pointer_target(
            &self.scene.node_arena,
            &self.scene.popup_stack,
            &root_keys,
            x,
            y,
        )
        .map(|(_, t)| t);
        if hover_target == self.input_state.hovered_node_id {
            return false;
        }
        let pointer_data = synthetic_pointer_data(
            (x, y),
            self.current_key_modifiers(),
            self.current_ui_pointer_buttons(),
        );
        let (hover_changed, hover_event_dispatched) = Self::sync_hover_target(
            &self.scene.node_arena,
            &root_keys,
            &mut self.input_state.hovered_node_id,
            hover_target,
            pointer_data,
        );
        if hover_changed || hover_event_dispatched {
            self.request_redraw();
            true
        } else {
            false
        }
    }
}

fn ui_input_type_from_platform(
    input_type: crate::platform::input::PlatformInputType,
) -> crate::ui::InputType {
    match input_type {
        crate::platform::input::PlatformInputType::Typing => crate::ui::InputType::Typing,
        crate::platform::input::PlatformInputType::Paste => crate::ui::InputType::Paste,
        crate::platform::input::PlatformInputType::Drop => crate::ui::InputType::Drop,
        crate::platform::input::PlatformInputType::ImeCommit => crate::ui::InputType::ImeCommit,
        crate::platform::input::PlatformInputType::Programmatic => {
            crate::ui::InputType::Programmatic
        }
    }
}

/// Build a minimal [`PointerEventData`] for synthetic hover-transition
/// dispatches (e.g. cleanup after focus loss) when no live platform event
/// is available. Uses `pointer_id = 0`, `pointer_type = Mouse`, zero pressure.
fn synthetic_pointer_data(
    pos: (f32, f32),
    modifiers: crate::platform::input::Modifiers,
    buttons: crate::ui::PointerButtons,
) -> crate::ui::PointerEventData {
    crate::ui::PointerEventData {
        viewport_x: pos.0,
        viewport_y: pos.1,
        local_x: 0.0,
        local_y: 0.0,
        button: None,
        buttons,
        modifiers,
        pointer_id: 0,
        pointer_type: crate::platform::input::PointerType::Mouse,
        pressure: 0.0,
        timestamp: crate::time::Instant::now(),
    }
}

/// Build the `target → root` path (DOM `composedPath` order) for the given
/// target key, by walking the arena parent chain from `target_key` upward.
///
/// Stops at the root (parent=None) or when `root_key` is reached (inclusive).
/// Ordering matches DOM `composedPath()`: target first, root last. Elements
/// are emitted as `NodeId` (alias for `NodeKey`) — the event layer uses
/// NodeKey end-to-end now.
fn composed_path_for_target(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
) -> Vec<crate::ui::NodeId> {
    let mut path = Vec::new();
    let mut current = Some(target_key);
    while let Some(k) = current {
        path.push(k);
        if k == root_key {
            return path;
        }
        current = arena.parent_of(k);
    }
    // target_key not reachable from root_key — return empty per contract.
    Vec::new()
}

pub fn dispatch_pointer_down_from_hit_test(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    event: &mut PointerDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_key) = hit_test(
        arena,
        root_key,
        event.pointer.viewport_x,
        event.pointer.viewport_y,
    ) else {
        return false;
    };
    event.meta.set_target_id(target_key);
    event
        .meta
        .set_path(composed_path_for_target(arena, root_key, target_key));
    dispatch_pointer_down_bubble(arena, target_key, event, control)
}

pub(crate) fn dispatch_pointer_down_to_target(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut PointerDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    event.meta.set_target_id(target_key);
    event
        .meta
        .set_path(composed_path_for_target(arena, root_key, target_key));
    dispatch_pointer_down_bubble(arena, target_key, event, control)
}

/// Walk the parent chain from `target` upward and return the `stable_id`
/// of the nearest viewport-clip absolute ancestor (or `target` itself if
/// it qualifies). Used by dispatch to promote the popup that absorbed
/// the pointer event to the top of the popup stack.
pub fn nearest_viewport_clip_ancestor_id(
    arena: &crate::view::node_arena::NodeArena,
    target: crate::view::node_arena::NodeKey,
) -> Option<u64> {
    let mut current = Some(target);
    while let Some(k) = current {
        let (sid_opt, parent) = {
            let node = arena.get(k)?;
            let parent = node.parent;
            let sid = node
                .element
                .as_any()
                .downcast_ref::<Element>()
                .filter(|el| el.should_append_to_root_viewport_render())
                .map(|el| el.stable_id())
                .filter(|s| *s != 0);
            (sid, parent)
        };
        if let Some(sid) = sid_opt {
            return Some(sid);
        }
        current = parent;
    }
    None
}

pub fn dispatch_pointer_up_from_hit_test(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    event: &mut PointerUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_key) = hit_test(
        arena,
        root_key,
        event.pointer.viewport_x,
        event.pointer.viewport_y,
    ) else {
        return false;
    };
    event.meta.set_target_id(target_key);
    event
        .meta
        .set_path(composed_path_for_target(arena, root_key, target_key));
    dispatch_pointer_up_bubble(arena, target_key, event, control)
}

pub(crate) fn dispatch_pointer_up_to_target(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut PointerUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    event.meta.set_target_id(target_key);
    event
        .meta
        .set_path(composed_path_for_target(arena, root_key, target_key));
    dispatch_pointer_up_bubble(arena, target_key, event, control)
}

pub fn dispatch_pointer_move_from_hit_test(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    event: &mut PointerMoveEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_key) = hit_test(
        arena,
        root_key,
        event.pointer.viewport_x,
        event.pointer.viewport_y,
    ) else {
        return false;
    };
    event.meta.set_target_id(target_key);
    event
        .meta
        .set_path(composed_path_for_target(arena, root_key, target_key));
    dispatch_pointer_move_bubble(arena, target_key, event, control)
}

pub(crate) fn dispatch_pointer_move_to_target(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut PointerMoveEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    event.meta.set_target_id(target_key);
    event
        .meta
        .set_path(composed_path_for_target(arena, root_key, target_key));
    dispatch_pointer_move_bubble(arena, target_key, event, control)
}

pub fn dispatch_click_from_hit_test(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    event: &mut ClickEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_key) = hit_test(
        arena,
        root_key,
        event.pointer.viewport_x,
        event.pointer.viewport_y,
    ) else {
        return false;
    };
    event.meta.set_target_id(target_key);
    event
        .meta
        .set_path(composed_path_for_target(arena, root_key, target_key));
    dispatch_click_bubble(arena, target_key, event, control)
}

pub(crate) fn dispatch_click_to_target(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut ClickEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    event.meta.set_target_id(target_key);
    event
        .meta
        .set_path(composed_path_for_target(arena, root_key, target_key));
    dispatch_click_bubble(arena, target_key, event, control)
}

pub(crate) fn dispatch_context_menu_to_target(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut crate::ui::ContextMenuEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    event.meta.set_target_id(target_key);
    event
        .meta
        .set_path(composed_path_for_target(arena, root_key, target_key));
    dispatch_context_menu_bubble(arena, target_key, event, control)
}

pub fn dispatch_scroll_from_hit_test(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    viewport_x: f32,
    viewport_y: f32,
    delta_x: f32,
    delta_y: f32,
) -> bool {
    let Some(target_key) = hit_test(arena, root_key, viewport_x, viewport_y) else {
        return false;
    };
    dispatch_scroll_bubble(arena, target_key, delta_x, delta_y)
}

pub(crate) fn find_scroll_handler_from_target(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    delta_x: f32,
    delta_y: f32,
) -> Option<crate::view::node_arena::NodeKey> {
    find_scroll_handler_bubble(arena, target_key, delta_x, delta_y)
}

pub(crate) fn dispatch_scroll_to_target(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    delta_x: f32,
    delta_y: f32,
) -> bool {
    dispatch_scroll_bubble(arena, target_key, delta_x, delta_y)
}

/// Scroll `target_key` into view inside its nearest scrollable ancestor.
/// Returns `true` when a scrollable ancestor was found and a non-zero
/// delta applied. Currently implements DOM `ScrollAlignment::Nearest`
/// for both axes regardless of `options.block` / `options.inline` —
/// Start / Center / End variants are recognised but fall back to
/// Nearest until a future pass computes precise alignment deltas.
pub(crate) fn scroll_into_view_impl(
    arena: &crate::view::node_arena::NodeArena,
    _root_keys: &[crate::view::node_arena::NodeKey],
    target_key: crate::view::node_arena::NodeKey,
    options: crate::ui::ScrollIntoViewOptions,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    let Some(target_rect) = arena.get(target_key).map(|n| {
        let snapshot = n.element.box_model_snapshot();
        crate::ui::Rect::new(snapshot.x, snapshot.y, snapshot.width, snapshot.height)
    }) else {
        return false;
    };
    scroll_rect_into_view_from(arena, target_key, target_rect, options, true, false)
}

pub(crate) fn scroll_rect_into_view_from(
    arena: &crate::view::node_arena::NodeArena,
    target_key: crate::view::node_arena::NodeKey,
    target_rect: crate::ui::Rect,
    options: crate::ui::ScrollIntoViewOptions,
    include_target: bool,
    reveal_all_ancestors: bool,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }

    let mut current = if include_target {
        Some(target_key)
    } else {
        arena.parent_of(target_key)
    };
    let mut rect = target_rect;
    let mut scrolled = false;
    let _ = options; // Start/Center/End: future work.

    while let Some(scroller_key) = current {
        current = arena.parent_of(scroller_key);
        let Some(scroller_rect) = arena.get(scroller_key).map(|n| {
            let snapshot = n.element.box_model_snapshot();
            crate::ui::Rect::new(snapshot.x, snapshot.y, snapshot.width, snapshot.height)
        }) else {
            continue;
        };

        let (dx, dy) = nearest_scroll_delta(rect, scroller_rect);
        if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
            continue;
        }

        let changed = arena
            .mutate_element_ref_with_invalidation(scroller_key, |element, cx| {
                let before = element.get_scroll_offset();
                let handled = element.scroll_by(dx, dy);
                let after = element.get_scroll_offset();
                let actual_dx = after.0 - before.0;
                let actual_dy = after.1 - before.1;
                if handled {
                    rect.x -= actual_dx;
                    rect.y -= actual_dy;
                }
                if handled && before != after {
                    cx.invalidate(DirtyPassMask::RUNTIME);
                }
                handled
            })
            .unwrap_or(false);
        scrolled |= changed;
        if changed && !reveal_all_ancestors {
            break;
        }
    }

    scrolled
}

fn nearest_scroll_delta(
    target_rect: crate::ui::Rect,
    scroller_rect: crate::ui::Rect,
) -> (f32, f32) {
    let mut dy = 0.0;
    if target_rect.y < scroller_rect.y {
        dy = target_rect.y - scroller_rect.y;
    } else if target_rect.y + target_rect.height > scroller_rect.y + scroller_rect.height {
        dy = (target_rect.y + target_rect.height) - (scroller_rect.y + scroller_rect.height);
    }
    let mut dx = 0.0;
    if target_rect.x < scroller_rect.x {
        dx = target_rect.x - scroller_rect.x;
    } else if target_rect.x + target_rect.width > scroller_rect.x + scroller_rect.width {
        dx = (target_rect.x + target_rect.width) - (scroller_rect.x + scroller_rect.width);
    }
    (dx, dy)
}

pub fn get_scroll_offset_by_id(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    stable_id: u64,
) -> Option<(f32, f32)> {
    let node = arena.get(root_key)?;
    if node.element.stable_id() == stable_id {
        return Some(node.element.get_scroll_offset());
    }
    let children: Vec<_> = node.children.clone();
    drop(node);
    for child_key in children {
        if let Some(offset) = get_scroll_offset_by_id(arena, child_key, stable_id) {
            return Some(offset);
        }
    }
    None
}

pub fn set_scroll_offset_by_id(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    stable_id: u64,
    offset: (f32, f32),
) -> bool {
    fn walk(
        arena: &crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
        stable_id: u64,
        offset: (f32, f32),
    ) -> bool {
        let Some(node) = arena.get(key) else {
            return false;
        };
        if node.element.stable_id() == stable_id {
            drop(node);
            return arena
                .mutate_element_ref_with_invalidation(key, |element, cx| {
                    let before = element.get_scroll_offset();
                    element.set_scroll_offset(offset);
                    if before != offset {
                        cx.invalidate(DirtyPassMask::RUNTIME);
                    }
                    true
                })
                .unwrap_or(false);
        }
        let children = node.children.clone();
        drop(node);
        for child_key in children {
            if walk(arena, child_key, stable_id, offset) {
                return true;
            }
        }
        false
    }

    walk(arena, root_key, stable_id, offset)
}

pub(crate) fn dispatch_key_down_bubble(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut KeyDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    dispatch_key_down_impl(arena, target_key, event, control)
}

pub(crate) fn dispatch_key_up_bubble(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut KeyUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    dispatch_key_up_impl(arena, target_key, event, control)
}

pub(crate) fn dispatch_text_input_bubble(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut TextInputEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    dispatch_text_input_impl(arena, target_key, event, control)
}

pub(crate) fn dispatch_ime_preedit_bubble(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut ImePreeditEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    dispatch_ime_preedit_impl(arena, target_key, event, control)
}

pub(crate) fn dispatch_focus_bubble(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut FocusEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    dispatch_focus_impl(arena, target_key, event, control)
}

pub(crate) fn dispatch_blur_bubble(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut BlurEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    dispatch_blur_impl(arena, target_key, event, control)
}

pub(super) fn local_point_for_node(
    node: &dyn ElementTrait,
    snapshot: &BoxModelSnapshot,
    viewport_x: f32,
    viewport_y: f32,
) -> (f32, f32) {
    let (paint_x, paint_y) = node
        .as_any()
        .downcast_ref::<Element>()
        .and_then(|element| element.map_viewport_to_paint_space(viewport_x, viewport_y))
        .unwrap_or((viewport_x, viewport_y));
    (paint_x - snapshot.x, paint_y - snapshot.y)
}

/// Bubble a pointer-down event from `target_key` up the arena parent chain.
/// Dispatches to target first, then each ancestor (via `arena.parent_of`)
/// until the root is reached or `stop_propagation` is called.
fn dispatch_pointer_down_bubble(
    arena: &crate::view::node_arena::NodeArena,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut PointerDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let mut current = Some(target_key);
    let mut dispatched = false;
    let mut at_target = true;
    while let Some(key) = current {
        if event.meta.propagation_stopped() {
            break;
        }
        event.meta.set_phase(if at_target {
            crate::ui::EventPhase::AtTarget
        } else {
            crate::ui::EventPhase::Bubbling
        });
        let next = arena.parent_of(key);
        let did = arena
            .mutate_element_ref_with_invalidation(key, |element, cx| {
                let node_id = element.stable_id();
                let snapshot = element.box_model_snapshot();
                let (local_x, local_y) = local_point_for_node(
                    element.as_ref(),
                    &snapshot,
                    event.pointer.viewport_x,
                    event.pointer.viewport_y,
                );
                event.pointer.local_x = local_x;
                event.pointer.local_y = local_y;
                let ct = crate::ui::EventTarget::snapshot(
                    key,
                    crate::ui::Rect::new(snapshot.x, snapshot.y, snapshot.width, snapshot.height),
                    crate::ui::Rect::new(0.0, 0.0, snapshot.width, snapshot.height),
                );
                event.meta.set_current_target(ct);
                let _ = node_id;
                element.dispatch_pointer_down(event, control, cx.arena(), key);
                cx.invalidate(element.local_dirty_flags());
                true
            })
            .unwrap_or(false);
        dispatched |= did;
        if at_target && !event.meta.bubbles() {
            break;
        }
        at_target = false;
        current = next;
    }
    event.meta.set_phase(crate::ui::EventPhase::None);
    dispatched
}

fn dispatch_pointer_up_bubble(
    arena: &crate::view::node_arena::NodeArena,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut PointerUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let mut current = Some(target_key);
    let mut dispatched = false;
    let mut at_target = true;
    while let Some(key) = current {
        if event.meta.propagation_stopped() {
            break;
        }
        event.meta.set_phase(if at_target {
            crate::ui::EventPhase::AtTarget
        } else {
            crate::ui::EventPhase::Bubbling
        });
        let next = arena.parent_of(key);
        let did = arena
            .mutate_element_ref_with_invalidation(key, |element, cx| {
                let snapshot = element.box_model_snapshot();
                let (local_x, local_y) = local_point_for_node(
                    element.as_ref(),
                    &snapshot,
                    event.pointer.viewport_x,
                    event.pointer.viewport_y,
                );
                event.pointer.local_x = local_x;
                event.pointer.local_y = local_y;
                let ct = crate::ui::EventTarget::snapshot(
                    key,
                    crate::ui::Rect::new(snapshot.x, snapshot.y, snapshot.width, snapshot.height),
                    crate::ui::Rect::new(0.0, 0.0, snapshot.width, snapshot.height),
                );
                event.meta.set_current_target(ct);
                element.dispatch_pointer_up(event, control, cx.arena(), key);
                cx.invalidate(element.local_dirty_flags());
                true
            })
            .unwrap_or(false);
        dispatched |= did;
        if at_target && !event.meta.bubbles() {
            break;
        }
        at_target = false;
        current = next;
    }
    event.meta.set_phase(crate::ui::EventPhase::None);
    dispatched
}

fn dispatch_pointer_move_bubble(
    arena: &crate::view::node_arena::NodeArena,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut PointerMoveEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let mut current = Some(target_key);
    let mut dispatched = false;
    let mut at_target = true;
    while let Some(key) = current {
        if event.meta.propagation_stopped() {
            break;
        }
        event.meta.set_phase(if at_target {
            crate::ui::EventPhase::AtTarget
        } else {
            crate::ui::EventPhase::Bubbling
        });
        let next = arena.parent_of(key);
        let did = arena
            .mutate_element_ref_with_invalidation(key, |element, cx| {
                let snapshot = element.box_model_snapshot();
                let (local_x, local_y) = local_point_for_node(
                    element.as_ref(),
                    &snapshot,
                    event.pointer.viewport_x,
                    event.pointer.viewport_y,
                );
                event.pointer.local_x = local_x;
                event.pointer.local_y = local_y;
                let ct = crate::ui::EventTarget::snapshot(
                    key,
                    crate::ui::Rect::new(snapshot.x, snapshot.y, snapshot.width, snapshot.height),
                    crate::ui::Rect::new(0.0, 0.0, snapshot.width, snapshot.height),
                );
                event.meta.set_current_target(ct);
                element.dispatch_pointer_move(event, control, cx.arena(), key);
                cx.invalidate(element.local_dirty_flags());
                true
            })
            .unwrap_or(false);
        dispatched |= did;
        if at_target && !event.meta.bubbles() {
            break;
        }
        at_target = false;
        current = next;
    }
    event.meta.set_phase(crate::ui::EventPhase::None);
    dispatched
}

fn dispatch_wheel_bubble(
    arena: &crate::view::node_arena::NodeArena,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut crate::ui::WheelEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let mut current = Some(target_key);
    let mut dispatched = false;
    let mut at_target = true;
    while let Some(key) = current {
        if event.meta.propagation_stopped() {
            break;
        }
        event.meta.set_phase(if at_target {
            crate::ui::EventPhase::AtTarget
        } else {
            crate::ui::EventPhase::Bubbling
        });
        let next = arena.parent_of(key);
        let did = arena
            .mutate_element_ref_with_invalidation(key, |element, cx| {
                let snapshot = element.box_model_snapshot();
                let (local_x, local_y) = local_point_for_node(
                    element.as_ref(),
                    &snapshot,
                    event.viewport_x,
                    event.viewport_y,
                );
                event.local_x = local_x;
                event.local_y = local_y;
                let ct = crate::ui::EventTarget::snapshot(
                    key,
                    crate::ui::Rect::new(snapshot.x, snapshot.y, snapshot.width, snapshot.height),
                    crate::ui::Rect::new(0.0, 0.0, snapshot.width, snapshot.height),
                );
                event.meta.set_current_target(ct);
                element.dispatch_wheel(event, control, cx.arena(), key);
                cx.invalidate(element.local_dirty_flags());
                true
            })
            .unwrap_or(false);
        dispatched |= did;
        if at_target && !event.meta.bubbles() {
            break;
        }
        at_target = false;
        current = next;
    }
    event.meta.set_phase(crate::ui::EventPhase::None);
    dispatched
}

#[allow(dead_code)]
pub(crate) fn dispatch_wheel_from_hit_test(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    event: &mut crate::ui::WheelEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_key) = hit_test(arena, root_key, event.viewport_x, event.viewport_y) else {
        return false;
    };
    event.meta.set_target_id(target_key);
    event
        .meta
        .set_path(composed_path_for_target(arena, root_key, target_key));
    dispatch_wheel_bubble(arena, target_key, event, control)
}

pub(crate) fn dispatch_wheel_to_target(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut crate::ui::WheelEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    event.meta.set_target_id(target_key);
    event
        .meta
        .set_path(composed_path_for_target(arena, root_key, target_key));
    dispatch_wheel_bubble(arena, target_key, event, control)
}

fn dispatch_context_menu_bubble(
    arena: &crate::view::node_arena::NodeArena,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut crate::ui::ContextMenuEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let mut current = Some(target_key);
    let mut dispatched = false;
    let mut at_target = true;
    while let Some(key) = current {
        if event.meta.propagation_stopped() {
            break;
        }
        event.meta.set_phase(if at_target {
            crate::ui::EventPhase::AtTarget
        } else {
            crate::ui::EventPhase::Bubbling
        });
        let next = arena.parent_of(key);
        let did = arena
            .mutate_element_ref_with_invalidation(key, |element, cx| {
                let snapshot = element.box_model_snapshot();
                let (local_x, local_y) = local_point_for_node(
                    element.as_ref(),
                    &snapshot,
                    event.pointer.viewport_x,
                    event.pointer.viewport_y,
                );
                event.pointer.local_x = local_x;
                event.pointer.local_y = local_y;
                let ct = crate::ui::EventTarget::snapshot(
                    key,
                    crate::ui::Rect::new(snapshot.x, snapshot.y, snapshot.width, snapshot.height),
                    crate::ui::Rect::new(0.0, 0.0, snapshot.width, snapshot.height),
                );
                event.meta.set_current_target(ct);
                element.dispatch_context_menu(event, control, cx.arena(), key);
                cx.invalidate(element.local_dirty_flags());
                true
            })
            .unwrap_or(false);
        dispatched |= did;
        if at_target && !event.meta.bubbles() {
            break;
        }
        at_target = false;
        current = next;
    }
    event.meta.set_phase(crate::ui::EventPhase::None);
    dispatched
}

fn dispatch_click_bubble(
    arena: &crate::view::node_arena::NodeArena,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut ClickEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let mut current = Some(target_key);
    let mut dispatched = false;
    let mut at_target = true;
    while let Some(key) = current {
        if event.meta.propagation_stopped() {
            break;
        }
        event.meta.set_phase(if at_target {
            crate::ui::EventPhase::AtTarget
        } else {
            crate::ui::EventPhase::Bubbling
        });
        let next = arena.parent_of(key);
        let did = arena
            .mutate_element_ref_with_invalidation(key, |element, cx| {
                let snapshot = element.box_model_snapshot();
                let (local_x, local_y) = local_point_for_node(
                    element.as_ref(),
                    &snapshot,
                    event.pointer.viewport_x,
                    event.pointer.viewport_y,
                );
                event.pointer.local_x = local_x;
                event.pointer.local_y = local_y;
                let ct = crate::ui::EventTarget::snapshot(
                    key,
                    crate::ui::Rect::new(snapshot.x, snapshot.y, snapshot.width, snapshot.height),
                    crate::ui::Rect::new(0.0, 0.0, snapshot.width, snapshot.height),
                );
                event.meta.set_current_target(ct);
                element.dispatch_click(event, control, cx.arena(), key);
                cx.invalidate(element.local_dirty_flags());
                true
            })
            .unwrap_or(false);
        dispatched |= did;
        if at_target && !event.meta.bubbles() {
            break;
        }
        at_target = false;
        current = next;
    }
    event.meta.set_phase(crate::ui::EventPhase::None);
    dispatched
}

/// Bubble a scroll event from `target_key` upward, letting the deepest
/// ancestor that can scroll consume the delta.
fn dispatch_scroll_bubble(
    arena: &crate::view::node_arena::NodeArena,
    target_key: crate::view::node_arena::NodeKey,
    dx: f32,
    dy: f32,
) -> bool {
    let mut current = Some(target_key);
    while let Some(key) = current {
        let next = arena.parent_of(key);
        let handled = arena
            .mutate_element_ref_with_invalidation(key, |element, cx| {
                let before = element.get_scroll_offset();
                let handled = element.scroll_by(dx, dy);
                if handled && before != element.get_scroll_offset() {
                    cx.invalidate(DirtyPassMask::RUNTIME);
                }
                handled
            })
            .unwrap_or(false);
        if handled {
            return true;
        }
        current = next;
    }
    false
}

fn find_scroll_handler_bubble(
    arena: &crate::view::node_arena::NodeArena,
    target_key: crate::view::node_arena::NodeKey,
    dx: f32,
    dy: f32,
) -> Option<crate::view::node_arena::NodeKey> {
    let mut current = Some(target_key);
    while let Some(key) = current {
        let can = arena
            .get(key)
            .is_some_and(|n| n.element.can_scroll_by(dx, dy));
        if can {
            return Some(key);
        }
        current = arena.parent_of(key);
    }
    None
}

fn dispatch_key_down_impl(
    arena: &crate::view::node_arena::NodeArena,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut KeyDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let mut current = Some(target_key);
    let mut dispatched = false;
    let mut at_target = true;
    while let Some(key) = current {
        if event.meta.propagation_stopped() {
            break;
        }
        event.meta.set_phase(if at_target {
            crate::ui::EventPhase::AtTarget
        } else {
            crate::ui::EventPhase::Bubbling
        });
        let next = arena.parent_of(key);
        let did = arena
            .mutate_element_ref_with_invalidation(key, |element, cx| {
                event.meta.set_current_target_id(key);
                element.dispatch_key_down(event, control, cx.arena(), key);
                cx.invalidate(element.local_dirty_flags());
                true
            })
            .unwrap_or(false);
        dispatched |= did;
        if at_target && !event.meta.bubbles() {
            break;
        }
        at_target = false;
        current = next;
    }
    event.meta.set_phase(crate::ui::EventPhase::None);
    dispatched
}

fn dispatch_key_up_impl(
    arena: &crate::view::node_arena::NodeArena,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut KeyUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let mut current = Some(target_key);
    let mut dispatched = false;
    let mut at_target = true;
    while let Some(key) = current {
        if event.meta.propagation_stopped() {
            break;
        }
        event.meta.set_phase(if at_target {
            crate::ui::EventPhase::AtTarget
        } else {
            crate::ui::EventPhase::Bubbling
        });
        let next = arena.parent_of(key);
        let did = arena
            .mutate_element_ref_with_invalidation(key, |element, cx| {
                event.meta.set_current_target_id(key);
                element.dispatch_key_up(event, control, cx.arena(), key);
                cx.invalidate(element.local_dirty_flags());
                true
            })
            .unwrap_or(false);
        dispatched |= did;
        if at_target && !event.meta.bubbles() {
            break;
        }
        at_target = false;
        current = next;
    }
    event.meta.set_phase(crate::ui::EventPhase::None);
    dispatched
}

fn dispatch_focus_impl(
    arena: &crate::view::node_arena::NodeArena,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut FocusEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let mut current = Some(target_key);
    let mut dispatched = false;
    let mut at_target = true;
    while let Some(key) = current {
        if event.meta.propagation_stopped() {
            break;
        }
        event.meta.set_phase(if at_target {
            crate::ui::EventPhase::AtTarget
        } else {
            crate::ui::EventPhase::Bubbling
        });
        let next = arena.parent_of(key);
        let did = arena
            .mutate_element_ref_with_invalidation(key, |element, cx| {
                event.meta.set_current_target_id(key);
                element.dispatch_focus(event, control, cx.arena(), key);
                cx.invalidate(element.local_dirty_flags());
                true
            })
            .unwrap_or(false);
        dispatched |= did;
        if at_target && !event.meta.bubbles() {
            break;
        }
        at_target = false;
        current = next;
    }
    event.meta.set_phase(crate::ui::EventPhase::None);
    dispatched
}

fn dispatch_text_input_impl(
    arena: &crate::view::node_arena::NodeArena,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut TextInputEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let mut current = Some(target_key);
    let mut dispatched = false;
    let mut at_target = true;
    while let Some(key) = current {
        if event.meta.propagation_stopped() {
            break;
        }
        event.meta.set_phase(if at_target {
            crate::ui::EventPhase::AtTarget
        } else {
            crate::ui::EventPhase::Bubbling
        });
        let next = arena.parent_of(key);
        let did = arena
            .mutate_element_ref_with_invalidation(key, |element, cx| {
                event.meta.set_current_target_id(key);
                element.dispatch_text_input(event, control, cx.arena(), key);
                cx.invalidate(element.local_dirty_flags());
                true
            })
            .unwrap_or(false);
        dispatched |= did;
        if at_target && !event.meta.bubbles() {
            break;
        }
        at_target = false;
        current = next;
    }
    event.meta.set_phase(crate::ui::EventPhase::None);
    dispatched
}

fn dispatch_ime_preedit_impl(
    arena: &crate::view::node_arena::NodeArena,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut ImePreeditEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let mut current = Some(target_key);
    let mut dispatched = false;
    let mut at_target = true;
    while let Some(key) = current {
        if event.meta.propagation_stopped() {
            break;
        }
        event.meta.set_phase(if at_target {
            crate::ui::EventPhase::AtTarget
        } else {
            crate::ui::EventPhase::Bubbling
        });
        let next = arena.parent_of(key);
        let did = arena
            .mutate_element_ref_with_invalidation(key, |element, cx| {
                event.meta.set_current_target_id(key);
                element.dispatch_ime_preedit(event, control, cx.arena(), key);
                cx.invalidate(element.local_dirty_flags());
                true
            })
            .unwrap_or(false);
        dispatched |= did;
        if at_target && !event.meta.bubbles() {
            break;
        }
        at_target = false;
        current = next;
    }
    event.meta.set_phase(crate::ui::EventPhase::None);
    dispatched
}

fn dispatch_blur_impl(
    arena: &crate::view::node_arena::NodeArena,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut BlurEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let mut current = Some(target_key);
    let mut dispatched = false;
    let mut at_target = true;
    while let Some(key) = current {
        if event.meta.propagation_stopped() {
            break;
        }
        event.meta.set_phase(if at_target {
            crate::ui::EventPhase::AtTarget
        } else {
            crate::ui::EventPhase::Bubbling
        });
        let next = arena.parent_of(key);
        let did = arena
            .mutate_element_ref_with_invalidation(key, |element, cx| {
                event.meta.set_current_target_id(key);
                element.dispatch_blur(event, control, cx.arena(), key);
                cx.invalidate(element.local_dirty_flags());
                true
            })
            .unwrap_or(false);
        dispatched |= did;
        if at_target && !event.meta.bubbles() {
            break;
        }
        at_target = false;
        current = next;
    }
    event.meta.set_phase(crate::ui::EventPhase::None);
    dispatched
}

macro_rules! define_focused_target_bubble {
    ($impl_fn:ident, $event_ty:ty, $dispatch_method:ident) => {
        fn $impl_fn(
            arena: &crate::view::node_arena::NodeArena,
            target_key: crate::view::node_arena::NodeKey,
            event: &mut $event_ty,
            control: &mut ViewportControl<'_>,
        ) -> bool {
            let mut current = Some(target_key);
            let mut dispatched = false;
            let mut at_target = true;
            while let Some(key) = current {
                if event.meta.propagation_stopped() {
                    break;
                }
                event.meta.set_phase(if at_target {
                    crate::ui::EventPhase::AtTarget
                } else {
                    crate::ui::EventPhase::Bubbling
                });
                let next = arena.parent_of(key);
                let did = arena
                    .mutate_element_ref_with_invalidation(key, |element, cx| {
                        event.meta.set_current_target_id(key);
                        element.$dispatch_method(event, control, cx.arena(), key);
                        cx.invalidate(element.local_dirty_flags());
                        true
                    })
                    .unwrap_or(false);
                dispatched |= did;
                if at_target && !event.meta.bubbles() {
                    break;
                }
                at_target = false;
                current = next;
            }
            event.meta.set_phase(crate::ui::EventPhase::None);
            dispatched
        }
    };
}

define_focused_target_bubble!(
    dispatch_ime_commit_impl,
    crate::ui::ImeCommitEvent,
    dispatch_ime_commit
);
define_focused_target_bubble!(
    dispatch_ime_enabled_impl,
    crate::ui::ImeEnabledEvent,
    dispatch_ime_enabled
);
define_focused_target_bubble!(
    dispatch_ime_disabled_impl,
    crate::ui::ImeDisabledEvent,
    dispatch_ime_disabled
);
define_focused_target_bubble!(dispatch_copy_impl, crate::ui::CopyEvent, dispatch_copy);
define_focused_target_bubble!(dispatch_cut_impl, crate::ui::CutEvent, dispatch_cut);
define_focused_target_bubble!(dispatch_paste_impl, crate::ui::PasteEvent, dispatch_paste);

macro_rules! define_pointer_target_bubble {
    ($impl_fn:ident, $event_ty:ty, $dispatch_method:ident) => {
        fn $impl_fn(
            arena: &crate::view::node_arena::NodeArena,
            target_key: crate::view::node_arena::NodeKey,
            event: &mut $event_ty,
            control: &mut ViewportControl<'_>,
        ) -> bool {
            let mut current = Some(target_key);
            let mut dispatched = false;
            let mut at_target = true;
            while let Some(key) = current {
                if event.meta.propagation_stopped() {
                    break;
                }
                event.meta.set_phase(if at_target {
                    crate::ui::EventPhase::AtTarget
                } else {
                    crate::ui::EventPhase::Bubbling
                });
                let next = arena.parent_of(key);
                let did = arena
                    .mutate_element_ref_with_invalidation(key, |element, cx| {
                        let snapshot = element.box_model_snapshot();
                        let (local_x, local_y) = local_point_for_node(
                            element.as_ref(),
                            &snapshot,
                            event.pointer.viewport_x,
                            event.pointer.viewport_y,
                        );
                        event.pointer.local_x = local_x;
                        event.pointer.local_y = local_y;
                        let ct = crate::ui::EventTarget::snapshot(
                            key,
                            crate::ui::Rect::new(
                                snapshot.x,
                                snapshot.y,
                                snapshot.width,
                                snapshot.height,
                            ),
                            crate::ui::Rect::new(0.0, 0.0, snapshot.width, snapshot.height),
                        );
                        event.meta.set_current_target(ct);
                        element.$dispatch_method(event, control, cx.arena(), key);
                        cx.invalidate(element.local_dirty_flags());
                        true
                    })
                    .unwrap_or(false);
                dispatched |= did;
                if at_target && !event.meta.bubbles() {
                    break;
                }
                at_target = false;
                current = next;
            }
            event.meta.set_phase(crate::ui::EventPhase::None);
            dispatched
        }
    };
}

pub(crate) fn dispatch_ime_commit_bubble(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut crate::ui::ImeCommitEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    dispatch_ime_commit_impl(arena, target_key, event, control)
}

pub(crate) fn dispatch_ime_enabled_bubble(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut crate::ui::ImeEnabledEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    dispatch_ime_enabled_impl(arena, target_key, event, control)
}

pub(crate) fn dispatch_ime_disabled_bubble(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut crate::ui::ImeDisabledEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    dispatch_ime_disabled_impl(arena, target_key, event, control)
}

pub(crate) fn dispatch_copy_bubble(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut crate::ui::CopyEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    dispatch_copy_impl(arena, target_key, event, control)
}

pub(crate) fn dispatch_cut_bubble(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut crate::ui::CutEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    dispatch_cut_impl(arena, target_key, event, control)
}

pub(crate) fn dispatch_paste_bubble(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut crate::ui::PasteEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    dispatch_paste_impl(arena, target_key, event, control)
}

// ---------------------------------------------------------------------
// Drag & drop bubble dispatchers
// ---------------------------------------------------------------------

define_pointer_target_bubble!(
    dispatch_drag_start_impl,
    crate::ui::DragStartEvent,
    dispatch_drag_start
);
define_pointer_target_bubble!(
    dispatch_drag_over_impl,
    crate::ui::DragOverEvent,
    dispatch_drag_over
);
define_pointer_target_bubble!(dispatch_drop_impl, crate::ui::DropEvent, dispatch_drop);
define_pointer_target_bubble!(
    dispatch_drag_end_impl,
    crate::ui::DragEndEvent,
    dispatch_drag_end
);

pub(crate) fn dispatch_drag_start_bubble(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut crate::ui::DragStartEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    dispatch_drag_start_impl(arena, target_key, event, control)
}

pub(crate) fn dispatch_drag_over_bubble(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut crate::ui::DragOverEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    dispatch_drag_over_impl(arena, target_key, event, control)
}

pub(crate) fn dispatch_drop_bubble(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut crate::ui::DropEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    dispatch_drop_impl(arena, target_key, event, control)
}

pub(crate) fn dispatch_drag_end_bubble(
    arena: &crate::view::node_arena::NodeArena,
    _root_key: crate::view::node_arena::NodeKey,
    target_key: crate::view::node_arena::NodeKey,
    event: &mut crate::ui::DragEndEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    if !arena.contains_key(target_key) {
        return false;
    }
    dispatch_drag_end_impl(arena, target_key, event, control)
}

/// Fire [`DragLeaveEvent`] at a specific node. Non-bubbling (no-bubble
/// counterpart of `DragOver`), so no ancestor walk — matches
/// `PointerLeaveEvent` shape.
pub(crate) fn dispatch_drag_leave_to_key(
    arena: &crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    control: &mut ViewportControl<'_>,
) -> bool {
    arena
        .mutate_element_ref_with_invalidation(key, |element, cx| {
            let snapshot = element.box_model_snapshot();
            let target = crate::ui::EventTarget::snapshot(
                key,
                crate::ui::Rect::new(snapshot.x, snapshot.y, snapshot.width, snapshot.height),
                crate::ui::Rect::new(0.0, 0.0, snapshot.width, snapshot.height),
            );
            let mut meta = crate::ui::EventMeta::with_target(target);
            meta.set_bubbles(false);
            meta.set_source(crate::ui::EventSource::Synthetic);
            let mut event = crate::ui::DragLeaveEvent { meta };
            element.dispatch_drag_leave(&mut event, control, cx.arena(), key);
            cx.invalidate(element.local_dirty_flags());
            true
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::style::Color;
    use crate::style::{Length, ParsedValue, Position, PropertyId, ScrollDirection, Style};
    use crate::ui::{
        ClickEvent, DataTransfer, DragEffect, DragOverEvent, EventMeta, Modifiers, NodeId,
        PointerButton, PointerButtons, PointerDownEvent, PointerEventData,
    };
    use crate::view::base_component::{Element, EventTarget, LayoutConstraints, LayoutPlacement};
    use crate::view::test_support::{
        commit_child, commit_element, measure_and_place, new_test_arena,
    };
    use crate::view::{Viewport, ViewportControl};
    use std::cell::Cell;
    use std::rc::Rc;

    fn constraints(w: f32, h: f32) -> LayoutConstraints {
        LayoutConstraints {
            max_width: w,
            max_height: h,
            viewport_width: w,
            percent_base_width: Some(w),
            percent_base_height: Some(h),
            viewport_height: h,
        }
    }

    fn placement(w: f32, h: f32) -> LayoutPlacement {
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: w,
            available_height: h,
            viewport_width: w,
            percent_base_width: Some(w),
            percent_base_height: Some(h),
            viewport_height: h,
        }
    }

    #[test]
    fn drag_over_bubble_recomputes_local_pointer_for_current_target() {
        let observed_local = Rc::new(Cell::new(None::<(i32, i32)>));

        let root = Element::new(0.0, 0.0, 200.0, 120.0);
        let mut child = Element::new(0.0, 0.0, 100.0, 40.0);
        let child_observed = observed_local.clone();
        child.on_drag_over(move |event, _control| {
            child_observed.set(Some((
                event.pointer.local_x.round() as i32,
                event.pointer.local_y.round() as i32,
            )));
            event.accept(DragEffect::Move);
        });
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(20.0))
                    .top(Length::px(30.0)),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let child_key = commit_child(&mut arena, root_key, Box::new(child));

        measure_and_place(
            &mut arena,
            root_key,
            constraints(200.0, 120.0),
            placement(200.0, 120.0),
        );

        let mut viewport = Viewport::new();
        let mut control = ViewportControl::new(&mut viewport);
        let mut event = DragOverEvent {
            meta: EventMeta::new(child_key),
            pointer: PointerEventData {
                viewport_x: 25.0,
                viewport_y: 45.0,
                local_x: 0.0,
                local_y: 0.0,
                button: None,
                buttons: PointerButtons::default(),
                modifiers: Modifiers::default(),
                pointer_id: 0,
                pointer_type: crate::platform::input::PointerType::Mouse,
                pressure: 0.0,
                timestamp: crate::time::Instant::now(),
            },
            data: DataTransfer::new(),
            drop_effect: None,
        };

        assert!(dispatch_drag_over_bubble(
            &arena,
            root_key,
            child_key,
            &mut event,
            &mut control,
        ));
        assert_eq!(observed_local.get(), Some((5, 15)));
        assert_eq!(event.drop_effect, Some(DragEffect::Move));
    }

    #[test]
    fn click_on_scrollbar_does_not_reach_click_handlers() {
        let mut root = Element::new(0.0, 0.0, 120.0, 120.0);
        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#101010")),
        );
        root_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        root.apply_style(root_style);

        let child_clicked = Rc::new(Cell::new(false));
        let mut child = Element::new(0.0, 0.0, 120.0, 360.0);
        child.set_background_color_value(Color::rgb(255, 0, 0));
        let child_clicked_flag = child_clicked.clone();
        child.on_click(move |_, _| child_clicked_flag.set(true));

        let root_clicked = Rc::new(Cell::new(false));
        let root_clicked_flag = root_clicked.clone();
        root.on_click(move |_, _| root_clicked_flag.set(true));

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let _child_key = commit_child(&mut arena, root_key, Box::new(child));

        measure_and_place(
            &mut arena,
            root_key,
            constraints(120.0, 120.0),
            placement(120.0, 120.0),
        );
        arena.with_element_taken(root_key, |el, _a| {
            if let Some(e) = el.as_any_mut().downcast_mut::<Element>() {
                let _ = e.set_hovered(true);
            }
        });

        let mut viewport = Viewport::new();
        let mut control = ViewportControl::new(&mut viewport);
        let mut click = ClickEvent {
            meta: EventMeta::new(NodeId::default()),
            pointer: PointerEventData {
                viewport_x: 115.0,
                viewport_y: 60.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(PointerButton::Left),
                buttons: PointerButtons::default(),
                modifiers: Modifiers::default(),
                pointer_id: 0,
                pointer_type: crate::platform::input::PointerType::Mouse,
                pressure: 0.0,
                timestamp: crate::time::Instant::now(),
            },
            click_count: 1,
        };

        let handled = dispatch_click_from_hit_test(&mut arena, root_key, &mut click, &mut control);
        assert!(handled);
        assert!(!child_clicked.get());
        assert!(!root_clicked.get());
    }

    #[test]
    fn mouse_down_on_scrollbar_requests_focus_keep() {
        let mut root = Element::new(0.0, 0.0, 120.0, 120.0);
        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#101010")),
        );
        root_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        root.apply_style(root_style);
        let mut child = Element::new(0.0, 0.0, 120.0, 360.0);
        child.set_background_color_value(Color::rgb(255, 0, 0));

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let _child_key = commit_child(&mut arena, root_key, Box::new(child));

        measure_and_place(
            &mut arena,
            root_key,
            constraints(120.0, 120.0),
            placement(120.0, 120.0),
        );
        arena.with_element_taken(root_key, |el, _a| {
            if let Some(e) = el.as_any_mut().downcast_mut::<Element>() {
                let _ = e.set_hovered(true);
            }
        });

        let mut viewport = Viewport::new();
        let meta = EventMeta::new(NodeId::default());
        let mut control = ViewportControl::new(&mut viewport);
        let mut down = PointerDownEvent {
            meta: meta.clone(),
            pointer: PointerEventData {
                viewport_x: 115.0,
                viewport_y: 60.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(PointerButton::Left),
                buttons: PointerButtons::default(),
                modifiers: Modifiers::default(),
                pointer_id: 0,
                pointer_type: crate::platform::input::PointerType::Mouse,
                pressure: 0.0,
                timestamp: crate::time::Instant::now(),
            },
            viewport: meta.viewport(),
        };

        let handled =
            dispatch_pointer_down_from_hit_test(&mut arena, root_key, &mut down, &mut control);
        assert!(handled);
        assert!(down.meta.focus_change_suppressed());
    }
}
