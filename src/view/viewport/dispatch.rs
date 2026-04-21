use super::*;

impl Viewport {
    fn dispatch_viewport_pointer_move_listeners(&mut self, event: &mut PointerMoveEvent) -> bool {
        if self.viewport_pointer_move_listeners.is_empty() {
            return false;
        }
        let mut handled = false;
        for listener in &mut self.viewport_pointer_move_listeners {
            listener.call(event);
            handled = true;
            if event.meta.propagation_stopped() {
                break;
            }
        }
        handled
    }

    fn dispatch_viewport_pointer_up_listeners(&mut self, event: &mut PointerUpEvent) -> bool {
        if self.viewport_pointer_up_listeners.is_empty() {
            return false;
        }
        let mut handled = false;
        let mut remove_ids = Vec::new();
        for listener in &mut self.viewport_pointer_up_listeners {
            match listener {
                ViewportPointerUpListener::Persistent(handler) => {
                    handler.call(event);
                    handled = true;
                }
                ViewportPointerUpListener::Until(handler) => {
                    handled = true;
                    if handler.call(event) {
                        remove_ids.push(handler.id());
                    }
                }
            }
            if event.meta.propagation_stopped() {
                break;
            }
        }
        if !remove_ids.is_empty() {
            self.viewport_pointer_up_listeners
                .retain(|listener| !remove_ids.contains(&listener.id()));
        }
        handled
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_pointer_down_from_hit_test(
                    &mut arena,
                    root_key,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        self.scene.node_arena = arena;
        if handled {
            self.input_state.pending_click = Some(PendingClick {
                button,
                target_id: event.meta.target_id(),
                viewport_x: x,
                viewport_y: y,
            });
        }
        if let Some(capture_target_id) = event.meta.pointer_capture_target_id() {
            self.input_state.pointer_capture_node_id = Some(capture_target_id);
        }
        self.apply_viewport_listener_actions(event.meta.take_viewport_listener_actions());
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
            let mut arena = std::mem::take(&mut self.scene.node_arena);
            let root_keys = self.scene.ui_root_keys.clone();
            let changed = Self::cancel_pointer_interactions(&mut arena, &root_keys);
            self.scene.node_arena = arena;
            if changed {
                self.request_redraw();
            }
            return false;
        };
        // Drag-active: close the drag gesture instead of running the
        // normal pointer_up path.
        if self.input_state.drag_state.is_some() {
            let _ = button;
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            if let Some(target_id) = control.viewport.pointer_capture_node_id() {
                for &root_key in root_keys.iter().rev() {
                    if crate::view::base_component::dispatch_pointer_up_to_target(
                        &mut arena,
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
                for &root_key in root_keys.iter().rev() {
                    if crate::view::base_component::dispatch_pointer_up_from_hit_test(
                        &mut arena,
                        root_key,
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                        break;
                    }
                }
            }
        }
        let listener_handled = self.dispatch_viewport_pointer_up_listeners(&mut event);
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        if handled || listener_handled {
            self.request_redraw();
        }
        handled || listener_handled
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let hover_target = root_keys
            .iter()
            .rev()
            .find_map(|&root_key| crate::view::base_component::hit_test(&arena, root_key, x, y));
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
            &mut arena,
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
            let mut control = ViewportControl::new(self);
            if let Some(target_id) = control.viewport.pointer_capture_node_id() {
                for &root_key in root_keys.iter().rev() {
                    if crate::view::base_component::dispatch_pointer_move_to_target(
                        &mut arena,
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
                for &root_key in root_keys.iter().rev() {
                    if crate::view::base_component::dispatch_pointer_move_from_hit_test(
                        &mut arena,
                        root_key,
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                        break;
                    }
                }
            }
        }
        let listener_handled = self.dispatch_viewport_pointer_move_listeners(&mut event);
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        let redraw_requested_during_event = !redraw_requested_before && self.redraw_requested;
        if hover_changed || hover_event_dispatched || redraw_requested_during_event {
            self.request_redraw();
        }
        handled || hover_changed || hover_event_dispatched || listener_handled
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let hit_target = root_keys
            .iter()
            .rev()
            .find_map(|&root_key| crate::view::base_component::hit_test(&arena, root_key, x, y));
        let is_valid_click = is_valid_click_candidate(pending_click, button, hit_target, x, y);
        if !is_valid_click {
            self.scene.node_arena = arena;
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
                let mut control = ViewportControl::new(self);
                for &root_key in root_keys.iter().rev() {
                    if crate::view::base_component::dispatch_context_menu_to_target(
                        &mut arena,
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
            event.meta.take_viewport_listener_actions()
        } else {
            let mut event = ClickEvent {
                meta: EventMeta::new(NodeId::default()),
                pointer,
                click_count,
            };
            {
                let mut control = ViewportControl::new(self);
                for &root_key in root_keys.iter().rev() {
                    if crate::view::base_component::dispatch_click_to_target(
                        &mut arena,
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
            event.meta.take_viewport_listener_actions()
        };
        self.scene.node_arena = arena;
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
        let mut wheel_arena = std::mem::take(&mut self.scene.node_arena);
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
        let mut wheel_user_handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in wheel_root_keys.iter().rev() {
                if crate::view::base_component::dispatch_wheel_from_hit_test(
                    &mut wheel_arena,
                    root_key,
                    &mut wheel_event,
                    &mut control,
                ) {
                    wheel_user_handled = true;
                    break;
                }
            }
        }
        let wheel_actions = wheel_event.meta.take_viewport_listener_actions();
        self.scene.node_arena = wheel_arena;
        self.apply_viewport_listener_actions(wheel_actions);
        if wheel_event.meta.default_prevented() {
            if wheel_user_handled {
                self.request_redraw();
            }
            return wheel_user_handled;
        }
        let mut pending_scroll_track: Option<(TrackTarget, (f32, f32), (f32, f32))> = None;
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let Some((root_index, target_key)) =
            Self::find_scroll_handler_at_pointer(&arena, &root_keys, x, y, delta_x, delta_y)
        else {
            self.scene.node_arena = arena;
            return false;
        };
        // Cross-frame scroll track keys are u64 stable_ids; resolve once.
        let target_stable_id = arena
            .get(target_key)
            .map(|n| n.element.stable_id())
            .unwrap_or(0);
        if let Some(&root_key) = root_keys.get(root_index) {
            if let Some(from) = crate::view::base_component::get_scroll_offset_by_id(
                &arena, root_key, target_stable_id,
            ) {
                let _ = crate::view::base_component::dispatch_scroll_to_target(
                    &mut arena,
                    root_key,
                    target_key,
                    delta_x,
                    delta_y,
                );
                if let Some(to) = crate::view::base_component::get_scroll_offset_by_id(
                    &arena,
                    root_key,
                    target_stable_id,
                ) {
                    let _ = crate::view::base_component::set_scroll_offset_by_id(
                        &mut arena,
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
        self.scene.node_arena = arena;
        let mut handled = false;
        if let Some((target_id, from, to)) = pending_scroll_track {
            let transition_spec = self.transitions.scroll_transition;
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            if (to.0 - from.0).abs() > 0.001 {
                let _ = self.transitions.scroll_transition_plugin.start_scroll_track(
                    &mut host,
                    target_id,
                    ScrollAxis::X,
                    from.0,
                    to.0,
                    transition_spec,
                );
            }
            if (to.1 - from.1).abs() > 0.001 {
                let _ = self.transitions.scroll_transition_plugin.start_scroll_track(
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
        root_keys: &[crate::view::node_arena::NodeKey],
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
    ) -> Option<(usize, crate::view::node_arena::NodeKey)> {
        let hit_target = root_keys
            .iter()
            .rev()
            .find_map(|&root_key| crate::view::base_component::hit_test(arena, root_key, x, y))?;

        // Walk up from hit_target via arena.parent_of, stopping at the first
        // ancestor that reports `can_scroll_by`. Determine which root it sits
        // under by walking further up to the root.
        let handler_key = crate::view::base_component::find_scroll_handler_from_target(
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_key_down_bubble(
                    &mut arena,
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
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_key_up_bubble(
                    &mut arena,
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
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_text_input_bubble(
                    &mut arena,
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
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_ime_preedit_bubble(
                    &mut arena,
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
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_ime_commit_bubble(
                    &mut arena,
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
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_ime_enabled_bubble(
                    &mut arena,
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
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_ime_disabled_bubble(
                    &mut arena,
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
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_copy_bubble(
                    &mut arena,
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
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_cut_bubble(
                    &mut arena,
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
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_paste_bubble(
                    &mut arena,
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
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_drag_start_bubble(
                    &mut arena,
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
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_drag_over_bubble(
                    &mut arena,
                    root_key,
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    break;
                }
            }
        }
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
        self.apply_viewport_listener_actions(pending_actions);
        event.drop_effect
    }

    /// Fire [`crate::ui::DragLeaveEvent`] at `target_id`. Non-bubbling.
    fn dispatch_drag_leave_event(&mut self, target_id: NodeId) -> bool {
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let handled = {
            let mut control = ViewportControl::new(self);
            crate::view::base_component::dispatch_drag_leave_to_key(
                &mut arena,
                target_id,
                &mut control,
            )
        };
        self.scene.node_arena = arena;
        handled
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_drop_bubble(
                    &mut arena,
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
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_drag_end_bubble(
                    &mut arena,
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
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
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
        let target = root_keys
            .iter()
            .rev()
            .find_map(|&root_key| {
                crate::view::base_component::hit_test(&arena_view, root_key, x, y)
            });
        self.scene.node_arena = arena_view;

        let prev_target = self
            .input_state
            .drag_state
            .as_ref()
            .and_then(|s| s.last_over_target);
        let data = self
            .input_state
            .drag_state
            .as_ref()
            .map(|s| s.data.clone());
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
        let effect = state.last_drop_effect;
        if let (Some(target), Some(effect)) = (state.last_over_target, effect) {
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
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_focus_bubble(
                    &mut arena,
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
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
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
        eprintln!(
            "[dispatch] blur target_id={:?} related={:?}",
            target_id, related
        );
        let reason = self.input_state.pending_focus_reason;
        let mut meta = EventMeta::new(target_id);
        meta.set_related_target(related.map(crate::ui::EventTarget::bare));
        meta.set_source(crate::ui::EventSource::Synthetic);
        let mut event = BlurEvent { meta, reason };
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for &root_key in root_keys.iter().rev() {
                if crate::view::base_component::dispatch_blur_bubble(
                    &mut arena,
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
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        eprintln!("[dispatch] blur handled={}", handled);
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
            PlatformPointerEventKind::Down(button) => {
                self.dispatch_pointer_down_event(button)
            }
            PlatformPointerEventKind::Up(button) => {
                self.dispatch_pointer_up_event(button)
            }
            PlatformPointerEventKind::Move { x, y } => {
                self.set_pointer_position_viewport(x, y);
                self.dispatch_pointer_move_event()
            }
            PlatformPointerEventKind::Click(button) => {
                self.dispatch_click_event(button)
            }
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
        let Some(stable_id) = self
            .scene
            .node_arena
            .get(target_key)
            .map(|n| n.element.stable_id())
        else {
            return Cursor::Default;
        };
        for &root_key in self.scene.ui_root_keys.iter().rev() {
            if let Some(cursor) = crate::view::base_component::get_cursor_by_id(
                &self.scene.node_arena,
                root_key,
                stable_id,
            ) {
                return cursor;
            }
        }
        Cursor::Default
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
        !self.viewport_pointer_move_listeners.is_empty()
            || !self.viewport_pointer_up_listeners.is_empty()
    }

    fn apply_viewport_listener_actions(&mut self, actions: Vec<EventCommand>) {
        let mut selection_changed = false;
        for action in actions {
            match action {
                EventCommand::AddPointerMoveListener(handler) => {
                    self.viewport_pointer_move_listeners.push(handler);
                }
                EventCommand::AddPointerUpListener(handler) => {
                    self.viewport_pointer_up_listeners
                        .push(ViewportPointerUpListener::Persistent(handler));
                }
                EventCommand::AddPointerUpListenerUntil(handler) => {
                    self.viewport_pointer_up_listeners
                        .push(ViewportPointerUpListener::Until(handler));
                }
                EventCommand::SetFocus(node_id) => {
                    self.set_focused_node_id(node_id);
                }
                EventCommand::SetCursor(cursor) => {
                    self.set_cursor(cursor);
                }
                EventCommand::SelectTextRangeAll(target_id) => {
                    let mut arena = std::mem::take(&mut self.scene.node_arena);
                    let root_keys = self.scene.ui_root_keys.clone();
                    let stable_id = arena
                        .get(target_id)
                        .map(|n| n.element.stable_id())
                        .unwrap_or(0);
                    for &root_key in root_keys.iter().rev() {
                        if crate::view::base_component::select_all_text_by_id(
                            &mut arena,
                            root_key,
                            stable_id,
                        ) {
                            selection_changed = true;
                            break;
                        }
                    }
                    self.scene.node_arena = arena;
                }
                EventCommand::SelectTextRange {
                    target_id,
                    start,
                    end,
                } => {
                    let mut arena = std::mem::take(&mut self.scene.node_arena);
                    let root_keys = self.scene.ui_root_keys.clone();
                    let stable_id = arena
                        .get(target_id)
                        .map(|n| n.element.stable_id())
                        .unwrap_or(0);
                    for &root_key in root_keys.iter().rev() {
                        if crate::view::base_component::select_text_range_by_id(
                            &mut arena,
                            root_key,
                            stable_id,
                            start,
                            end,
                        ) {
                            selection_changed = true;
                            break;
                        }
                    }
                    self.scene.node_arena = arena;
                }
                EventCommand::RemoveListener(handle) => {
                    self.remove_viewport_listener(handle);
                }
                EventCommand::RequestRedraw => {
                    self.request_redraw();
                }
                EventCommand::WriteClipboard(text) => {
                    self.set_clipboard_text(text);
                }
                EventCommand::ScrollIntoView { target_id, options } => {
                    let mut arena = std::mem::take(&mut self.scene.node_arena);
                    let root_keys = self.scene.ui_root_keys.clone();
                    let scrolled = crate::view::base_component::scroll_into_view_impl(
                        &mut arena,
                        &root_keys,
                        target_id,
                        options,
                    );
                    self.scene.node_arena = arena;
                    if scrolled {
                        self.request_redraw();
                    }
                }
                EventCommand::KeyboardCapture(node_id) => {
                    self.input_state.keyboard_capture_node_id = node_id;
                }
                EventCommand::Window(command) => {
                    self.pending_platform_requests
                        .window_commands
                        .push(command);
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
                    self.pending_platform_requests
                        .pending_drags
                        .push(crate::platform::PendingDrag {
                            source_id,
                            payload,
                            effect_allowed,
                        });
                    // Fire DragStart synchronously so the handler can
                    // veto (future: prevent_default clears drag_state).
                    let pointer = synthetic_pointer_data(
                        self.input_state.pointer_position_viewport.unwrap_or((0.0, 0.0)),
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

    fn remove_viewport_listener(&mut self, handle: ViewportListenerHandle) {
        self.viewport_pointer_move_listeners
            .retain(|listener| listener.id() != handle.0);
        self.viewport_pointer_up_listeners
            .retain(|listener| listener.id() != handle.0);
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
        let last_pos = self.input_state.pointer_position_viewport.unwrap_or((0.0, 0.0));
        self.input_state.pointer_position_viewport = None;
        self.input_state.pointer_capture_node_id = None;
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let pointer_data = synthetic_pointer_data(
            last_pos,
            self.current_key_modifiers(),
            self.current_ui_pointer_buttons(),
        );
        let (hover_changed, hover_event_dispatched) = Self::sync_hover_target(
            &mut arena,
            &root_keys,
            &mut self.input_state.hovered_node_id,
            None,
            pointer_data,
        );
        let pointer_changed = Self::cancel_pointer_interactions(&mut arena, &root_keys);
        self.scene.node_arena = arena;
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
        self.viewport_pointer_move_listeners.clear();
        self.viewport_pointer_up_listeners.clear();
        self.dispatched_focus_node_id = None;
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let pointer_data = synthetic_pointer_data(
            self.input_state.pointer_position_viewport.unwrap_or((0.0, 0.0)),
            self.current_key_modifiers(),
            self.current_ui_pointer_buttons(),
        );
        let (hover_changed, hover_event_dispatched) = Self::sync_hover_target(
            &mut arena,
            &root_keys,
            &mut self.input_state.hovered_node_id,
            None,
            pointer_data,
        );
        let pointer_changed = Self::cancel_pointer_interactions(&mut arena, &root_keys);
        self.scene.node_arena = arena;
        if hover_changed || hover_event_dispatched || pointer_changed {
            self.request_redraw();
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
