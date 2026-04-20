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
                button: Some(to_ui_pointer_button(button)),
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
            let keep_focus_requested = event.meta.keep_focus_requested();
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
                    self.set_focused_node_id(Some(clicked_target));
                    self.sync_focus_dispatch();
                }
            }
            self.request_redraw();
        } else if self.focused_node_id().is_some() {
            self.set_focused_node_id(None);
            self.sync_focus_dispatch();
            self.request_redraw();
        }
        handled
    }

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
        let buttons = self.current_ui_pointer_buttons();
        let meta = EventMeta::new(NodeId::default());
        let mut event = PointerUpEvent {
            meta: meta.clone(),
            pointer: PointerEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(to_ui_pointer_button(button)),
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

    pub fn dispatch_pointer_move_event(&mut self) -> bool {
        let Some((x, y)) = self.pointer_position_viewport() else {
            return false;
        };
        let redraw_requested_before = self.redraw_requested;
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let hover_target = root_keys
            .iter()
            .rev()
            .find_map(|&root_key| crate::view::base_component::hit_test(&arena, root_key, x, y));
        let (hover_changed, hover_event_dispatched) = Self::sync_hover_target(
            &mut arena,
            &root_keys,
            &mut self.input_state.hovered_node_id,
            hover_target,
        );
        let buttons = self.current_ui_pointer_buttons();
        let meta = EventMeta::new(NodeId::default());
        let mut event = PointerMoveEvent {
            meta: meta.clone(),
            pointer: PointerEventData {
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
            },
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
        let mut event = ClickEvent {
            meta: EventMeta::new(NodeId::default()),
            pointer: PointerEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(to_ui_pointer_button(button)),
                buttons,
                modifiers: self.current_key_modifiers(),
                pointer_id: 0,
                pointer_type: PointerType::Mouse,
                pressure: 0.0,
                timestamp: crate::time::Instant::now(),
            },
        };
        let mut handled = false;
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
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.node_arena = arena;
        self.apply_viewport_listener_actions(pending_actions);
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_pointer_wheel_event(&mut self, delta_x: f32, delta_y: f32) -> bool {
        let Some((x, y)) = self.pointer_position_viewport() else {
            return false;
        };
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

    pub fn dispatch_key_down_event(&mut self, data: KeyEventData) -> bool {
        let Some(target_id) = self.focused_node_id() else {
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

    pub fn dispatch_key_up_event(&mut self, data: KeyEventData) -> bool {
        let Some(target_id) = self.focused_node_id() else {
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

    pub fn dispatch_text_input_event(&mut self, text: String) -> bool {
        if text.is_empty() {
            return false;
        }
        let Some(target_id) = self.focused_node_id() else {
            return false;
        };
        let mut event = TextInputEvent {
            meta: EventMeta::new(target_id),
            text,
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

    pub fn dispatch_ime_preedit_event(
        &mut self,
        text: String,
        cursor: Option<(usize, usize)>,
    ) -> bool {
        let Some(target_id) = self.focused_node_id() else {
            return false;
        };
        let mut event = ImePreeditEvent {
            meta: EventMeta::new(target_id),
            text,
            cursor,
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

    pub fn dispatch_focus_event(&mut self, target_id: NodeId) -> bool {
        self.dispatch_focus_event_with_related(target_id, None)
    }

    pub(super) fn dispatch_focus_event_with_related(
        &mut self,
        target_id: NodeId,
        related: Option<NodeId>,
    ) -> bool {
        let mut meta = EventMeta::new(target_id);
        meta.set_related_target(related.map(crate::ui::EventTarget::bare));
        let mut event = FocusEvent { meta };
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

    pub fn dispatch_blur_event(&mut self, target_id: NodeId) -> bool {
        self.dispatch_blur_event_with_related(target_id, None)
    }

    pub(super) fn dispatch_blur_event_with_related(
        &mut self,
        target_id: NodeId,
        related: Option<NodeId>,
    ) -> bool {
        let mut meta = EventMeta::new(target_id);
        meta.set_related_target(related.map(crate::ui::EventTarget::bare));
        let mut event = BlurEvent { meta };
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
                self.dispatch_pointer_down_event(pointer_button_from_platform(button))
            }
            PlatformPointerEventKind::Up(button) => {
                self.dispatch_pointer_up_event(pointer_button_from_platform(button))
            }
            PlatformPointerEventKind::Move { x, y } => {
                self.set_pointer_position_viewport(x, y);
                self.dispatch_pointer_move_event()
            }
            PlatformPointerEventKind::Click(button) => {
                self.dispatch_click_event(pointer_button_from_platform(button))
            }
        }
    }

    pub fn dispatch_platform_wheel_event(&mut self, event: &PlatformWheelEvent) -> bool {
        self.dispatch_pointer_wheel_event(event.delta_x, event.delta_y)
    }

    pub fn dispatch_platform_key_event(&mut self, event: &PlatformKeyEvent) -> bool {
        let data = KeyEventData {
            key: event.key,
            characters: event.characters.clone(),
            modifiers: event.modifiers,
            repeat: event.repeat,
            is_composing: event.is_composing,
            timestamp: event.timestamp,
        };
        if event.pressed {
            self.dispatch_key_down_event(data)
        } else {
            self.dispatch_key_up_event(data)
        }
    }

    pub fn dispatch_platform_text_input(&mut self, event: &PlatformTextInput) -> bool {
        self.dispatch_text_input_event(event.text.clone())
    }

    pub fn dispatch_platform_ime_preedit(&mut self, event: &PlatformImePreedit) -> bool {
        let cursor = match (event.cursor_start, event.cursor_end) {
            (Some(start), Some(end)) => Some((start, end)),
            _ => None,
        };
        self.dispatch_ime_preedit_event(event.text.clone(), cursor)
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
        let target_key = self.focused_node_id()?;
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

/// Convert a platform-neutral pointer button into the viewport-internal
/// `PointerButton` enum. Kept as a free function (rather than `From`) so the
/// viewport owns the mapping without leaking its internal type into the
/// platform crate.
fn pointer_button_from_platform(button: PlatformPointerButton) -> PointerButton {
    match button {
        PlatformPointerButton::Left => PointerButton::Left,
        PlatformPointerButton::Right => PointerButton::Right,
        PlatformPointerButton::Middle => PointerButton::Middle,
        PlatformPointerButton::Back => PointerButton::Back,
        PlatformPointerButton::Forward => PointerButton::Forward,
        PlatformPointerButton::Other(code) => PointerButton::Other(code),
    }
}

impl Viewport {
    pub fn has_viewport_pointer_listeners(&self) -> bool {
        !self.viewport_pointer_move_listeners.is_empty()
            || !self.viewport_pointer_up_listeners.is_empty()
    }

    fn apply_viewport_listener_actions(&mut self, actions: Vec<ViewportListenerAction>) {
        let mut selection_changed = false;
        for action in actions {
            match action {
                ViewportListenerAction::AddPointerMoveListener(handler) => {
                    self.viewport_pointer_move_listeners.push(handler);
                }
                ViewportListenerAction::AddPointerUpListener(handler) => {
                    self.viewport_pointer_up_listeners
                        .push(ViewportPointerUpListener::Persistent(handler));
                }
                ViewportListenerAction::AddPointerUpListenerUntil(handler) => {
                    self.viewport_pointer_up_listeners
                        .push(ViewportPointerUpListener::Until(handler));
                }
                ViewportListenerAction::SetFocus(node_id) => {
                    self.set_focused_node_id(node_id);
                }
                ViewportListenerAction::SetCursor(cursor) => {
                    self.set_cursor(cursor);
                }
                ViewportListenerAction::SelectTextRangeAll(target_id) => {
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
                ViewportListenerAction::SelectTextRange {
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
                ViewportListenerAction::RemoveListener(handle) => {
                    self.remove_viewport_listener(handle);
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
        self.input_state.pointer_position_viewport = None;
        self.input_state.pointer_capture_node_id = None;
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let (hover_changed, hover_event_dispatched) = Self::sync_hover_target(
            &mut arena,
            &root_keys,
            &mut self.input_state.hovered_node_id,
            None,
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
        let (hover_changed, hover_event_dispatched) = Self::sync_hover_target(
            &mut arena,
            &root_keys,
            &mut self.input_state.hovered_node_id,
            None,
        );
        let pointer_changed = Self::cancel_pointer_interactions(&mut arena, &root_keys);
        self.scene.node_arena = arena;
        if hover_changed || hover_event_dispatched || pointer_changed {
            self.request_redraw();
        }
    }

}
