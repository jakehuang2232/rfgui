use super::*;

impl Viewport {
    fn dispatch_viewport_mouse_move_listeners(&mut self, event: &mut MouseMoveEvent) -> bool {
        if self.viewport_mouse_move_listeners.is_empty() {
            return false;
        }
        let mut handled = false;
        for listener in &mut self.viewport_mouse_move_listeners {
            listener.call(event);
            handled = true;
            if event.meta.propagation_stopped() {
                break;
            }
        }
        handled
    }

    fn dispatch_viewport_mouse_up_listeners(&mut self, event: &mut MouseUpEvent) -> bool {
        if self.viewport_mouse_up_listeners.is_empty() {
            return false;
        }
        let mut handled = false;
        let mut remove_ids = Vec::new();
        for listener in &mut self.viewport_mouse_up_listeners {
            match listener {
                ViewportMouseUpListener::Persistent(handler) => {
                    handler.call(event);
                    handled = true;
                }
                ViewportMouseUpListener::Until(handler) => {
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
            self.viewport_mouse_up_listeners
                .retain(|listener| !remove_ids.contains(&listener.id()));
        }
        handled
    }

    pub fn dispatch_mouse_down_event(&mut self, button: MouseButton) -> bool {
        let Some((x, y)) = self.mouse_position_viewport() else {
            return false;
        };
        self.input_state.pending_click = None;
        let focus_before = self.focused_node_id();
        let buttons = self.current_ui_mouse_buttons();
        let meta = EventMeta::new(0);
        let mut event = MouseDownEvent {
            meta: meta.clone(),
            mouse: MouseEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                current_target_width: 0.0,
                current_target_height: 0.0,
                button: Some(to_ui_mouse_button(button)),
                buttons,
                modifiers: self.current_key_modifiers(),
            },
            viewport: meta.viewport(),
        };
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            if let Some((root_idx, _)) =
                crate::view::base_component::hit_test_roots(&roots, x, y)
            {
                if let Some(root) = roots.get_mut(root_idx) {
                    if crate::view::base_component::dispatch_mouse_down_from_hit_test(
                        root.as_mut(),
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                    }
                }
            }
        }
        self.scene.ui_roots = roots;
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
                self.scene.ui_roots.iter().rev().any(|root| {
                    crate::view::base_component::subtree_contains_node(
                        root.as_ref(),
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

    pub fn dispatch_mouse_up_event(&mut self, button: MouseButton) -> bool {
        let Some((x, y)) = self.mouse_position_viewport() else {
            self.input_state.pointer_capture_node_id = None;
            let changed = Self::cancel_pointer_interactions(&mut self.scene.ui_roots);
            if changed {
                self.request_redraw();
            }
            return false;
        };
        let buttons = self.current_ui_mouse_buttons();
        let meta = EventMeta::new(0);
        let mut event = MouseUpEvent {
            meta: meta.clone(),
            mouse: MouseEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                current_target_width: 0.0,
                current_target_height: 0.0,
                button: Some(to_ui_mouse_button(button)),
                buttons,
                modifiers: self.current_key_modifiers(),
            },
            viewport: meta.viewport(),
        };
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            if let Some(target_id) = control.viewport.pointer_capture_node_id() {
                for root in roots.iter_mut().rev() {
                    if crate::view::base_component::dispatch_mouse_up_to_target(
                        root.as_mut(),
                        target_id,
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                        break;
                    }
                }
                control.viewport.set_pointer_capture_node_id(None);
            } else if let Some((root_idx, _)) =
                crate::view::base_component::hit_test_roots(&roots, x, y)
            {
                if let Some(root) = roots.get_mut(root_idx) {
                    if crate::view::base_component::dispatch_mouse_up_from_hit_test(
                        root.as_mut(),
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                    }
                }
            }
        }
        let listener_handled = self.dispatch_viewport_mouse_up_listeners(&mut event);
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.ui_roots = roots;
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        if handled || listener_handled {
            self.request_redraw();
        }
        handled || listener_handled
    }

    pub fn dispatch_mouse_move_event(&mut self) -> bool {
        let Some((x, y)) = self.mouse_position_viewport() else {
            return false;
        };
        let redraw_requested_before = self.redraw_requested;
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let hit_result = crate::view::base_component::hit_test_roots(&roots, x, y);
        let hover_target = hit_result.map(|(_, id)| id);
        let (hover_changed, hover_event_dispatched) = Self::sync_hover_target(
            &mut roots,
            &mut self.input_state.hovered_node_id,
            hover_target,
        );
        let buttons = self.current_ui_mouse_buttons();
        let meta = EventMeta::new(0);
        let mut event = MouseMoveEvent {
            meta: meta.clone(),
            mouse: MouseEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                current_target_width: 0.0,
                current_target_height: 0.0,
                button: None,
                buttons,
                modifiers: self.current_key_modifiers(),
            },
            viewport: meta.viewport(),
        };
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            if let Some(target_id) = control.viewport.pointer_capture_node_id() {
                for root in roots.iter_mut().rev() {
                    if crate::view::base_component::dispatch_mouse_move_to_target(
                        root.as_mut(),
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
            } else if let Some((root_idx, _)) = hit_result {
                if let Some(root) = roots.get_mut(root_idx) {
                    if crate::view::base_component::dispatch_mouse_move_from_hit_test(
                        root.as_mut(),
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                    }
                }
            }
        }
        let listener_handled = self.dispatch_viewport_mouse_move_listeners(&mut event);
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.ui_roots = roots;
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        let redraw_requested_during_event = !redraw_requested_before && self.redraw_requested;
        if hover_changed || hover_event_dispatched || redraw_requested_during_event {
            self.request_redraw();
        }
        handled || hover_changed || hover_event_dispatched || listener_handled
    }

    pub fn dispatch_click_event(&mut self, button: MouseButton) -> bool {
        let Some((x, y)) = self.mouse_position_viewport() else {
            return false;
        };
        let Some(pending_click) = self.input_state.pending_click.take() else {
            return false;
        };
        if pending_click.button != button {
            return false;
        }
        let buttons = self.current_ui_mouse_buttons();
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let hit_target = crate::view::base_component::hit_test_roots(&roots, x, y)
            .map(|(_, id)| id);
        let is_valid_click = is_valid_click_candidate(pending_click, button, hit_target, x, y);
        if !is_valid_click {
            self.scene.ui_roots = roots;
            return false;
        }
        let mut event = ClickEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                current_target_width: 0.0,
                current_target_height: 0.0,
                button: Some(to_ui_mouse_button(button)),
                buttons,
                modifiers: self.current_key_modifiers(),
            },
        };
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::dispatch_click_to_target(
                    root.as_mut(),
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
        self.scene.ui_roots = roots;
        self.apply_viewport_listener_actions(pending_actions);
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_mouse_wheel_event(&mut self, delta_x: f32, delta_y: f32) -> bool {
        let Some((x, y)) = self.mouse_position_viewport() else {
            return false;
        };
        let mut pending_scroll_track: Option<(TrackTarget, (f32, f32), (f32, f32))> = None;
        let Some((root_index, target_id)) =
            Self::find_scroll_handler_at_pointer(&self.scene.ui_roots, x, y, delta_x, delta_y)
        else {
            return false;
        };
        if let Some(root) = self.scene.ui_roots.get_mut(root_index) {
            let Some(from) =
                crate::view::base_component::get_scroll_offset_by_id(root.as_ref(), target_id)
            else {
                return false;
            };
            let _ = crate::view::base_component::dispatch_scroll_to_target(
                root.as_mut(),
                target_id,
                delta_x,
                delta_y,
            );
            let Some(to) =
                crate::view::base_component::get_scroll_offset_by_id(root.as_ref(), target_id)
            else {
                return false;
            };
            let _ =
                crate::view::base_component::set_scroll_offset_by_id(root.as_mut(), target_id, from);

            if (to.0 - from.0).abs() > 0.001 || (to.1 - from.1).abs() > 0.001 {
                pending_scroll_track = Some((target_id, from, to));
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
        roots: &[Box<dyn crate::view::base_component::ElementTrait>],
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
    ) -> Option<(usize, u64)> {
        let hit_target = crate::view::base_component::hit_test_roots(roots, x, y)
            .map(|(_, id)| id)?;
        let mut best_match: Option<(usize, u64, usize)> = None;

        for (root_index, root) in roots.iter().enumerate() {
            let Some(target_path) =
                crate::view::base_component::get_node_ancestry_ids(root.as_ref(), hit_target)
            else {
                continue;
            };
            let Some(handler_id) = crate::view::base_component::find_scroll_handler_from_target(
                root.as_ref(),
                hit_target,
                delta_x,
                delta_y,
            ) else {
                continue;
            };
            let Some(handler_path) =
                crate::view::base_component::get_node_ancestry_ids(root.as_ref(), handler_id)
            else {
                continue;
            };
            let ancestor_distance = target_path.len().saturating_sub(handler_path.len());
            match best_match {
                Some((_, _, best_distance)) if ancestor_distance >= best_distance => {}
                _ => best_match = Some((root_index, handler_id, ancestor_distance)),
            }
        }

        best_match.map(|(root_index, handler_id, _)| (root_index, handler_id))
    }

    pub fn dispatch_key_down_event(&mut self, key: String, code: String, repeat: bool) -> bool {
        let Some(target_id) = self.focused_node_id() else {
            return false;
        };
        let mut event = KeyDownEvent {
            meta: EventMeta::new(target_id),
            key: KeyEventData {
                key,
                code,
                repeat,
                modifiers: self.current_key_modifiers(),
            },
        };
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::dispatch_key_down_bubble(
                    root.as_mut(),
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
        self.scene.ui_roots = roots;
        self.apply_viewport_listener_actions(pending_actions);
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_key_up_event(&mut self, key: String, code: String, repeat: bool) -> bool {
        let Some(target_id) = self.focused_node_id() else {
            return false;
        };
        let mut event = KeyUpEvent {
            meta: EventMeta::new(target_id),
            key: KeyEventData {
                key,
                code,
                repeat,
                modifiers: self.current_key_modifiers(),
            },
        };
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::dispatch_key_up_bubble(
                    root.as_mut(),
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
        self.scene.ui_roots = roots;
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
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::dispatch_text_input_bubble(
                    root.as_mut(),
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
        self.scene.ui_roots = roots;
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
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::dispatch_ime_preedit_bubble(
                    root.as_mut(),
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
        self.scene.ui_roots = roots;
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_focus_event(&mut self, target_id: u64) -> bool {
        let mut event = FocusEvent {
            meta: EventMeta::new(target_id),
        };
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::dispatch_focus_bubble(
                    root.as_mut(),
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
        self.scene.ui_roots = roots;
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_blur_event(&mut self, target_id: u64) -> bool {
        let mut event = BlurEvent {
            meta: EventMeta::new(target_id),
        };
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::dispatch_blur_bubble(
                    root.as_mut(),
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
        self.scene.ui_roots = roots;
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        if handled {
            self.request_redraw();
        }
        handled
    }

    /// Dispatch a platform-neutral mouse event.
    ///
    /// Canonical entry point for backends (winit, web, headless). Internally
    /// forwards to the legacy primitive-argument `dispatch_mouse_*` methods;
    /// those remain public for now so component tests and existing callers
    /// keep working. New backend code should only ever see this method.
    pub fn dispatch_platform_mouse_event(&mut self, event: &PlatformMouseEvent) -> bool {
        match event.kind {
            PlatformMouseEventKind::Down(button) => {
                self.dispatch_mouse_down_event(mouse_button_from_platform(button))
            }
            PlatformMouseEventKind::Up(button) => {
                self.dispatch_mouse_up_event(mouse_button_from_platform(button))
            }
            PlatformMouseEventKind::Move { x, y } => {
                self.set_mouse_position_viewport(x, y);
                self.dispatch_mouse_move_event()
            }
            PlatformMouseEventKind::Click(button) => {
                self.dispatch_click_event(mouse_button_from_platform(button))
            }
        }
    }

    pub fn dispatch_platform_wheel_event(&mut self, event: &PlatformWheelEvent) -> bool {
        self.dispatch_mouse_wheel_event(event.delta_x, event.delta_y)
    }

    pub fn dispatch_platform_key_event(&mut self, event: &PlatformKeyEvent) -> bool {
        if event.pressed {
            self.dispatch_key_down_event(event.key.clone(), event.code.clone(), event.repeat)
        } else {
            self.dispatch_key_up_event(event.key.clone(), event.code.clone(), event.repeat)
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

    fn current_ui_mouse_buttons(&self) -> UiMouseButtons {
        UiMouseButtons {
            left: self.is_mouse_button_pressed(MouseButton::Left),
            right: self.is_mouse_button_pressed(MouseButton::Right),
            middle: self.is_mouse_button_pressed(MouseButton::Middle),
            back: self.is_mouse_button_pressed(MouseButton::Back),
            forward: self.is_mouse_button_pressed(MouseButton::Forward),
        }
    }

    pub fn focused_ime_cursor_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let target_id = self.focused_node_id()?;
        for root in self.scene.ui_roots.iter().rev() {
            if let Some(rect) =
                crate::view::base_component::get_ime_cursor_rect_by_id(root.as_ref(), target_id)
            {
                return Some(rect);
            }
        }
        None
    }

    fn current_key_modifiers(&self) -> KeyModifiers {
        KeyModifiers {
            alt: self.is_key_pressed("Named(Alt)")
                || self.is_key_pressed("Named(AltGraph)")
                || self.is_key_pressed("Code(AltLeft)")
                || self.is_key_pressed("Code(AltRight)"),
            ctrl: self.is_key_pressed("Named(Control)")
                || self.is_key_pressed("Code(ControlLeft)")
                || self.is_key_pressed("Code(ControlRight)"),
            shift: self.is_key_pressed("Named(Shift)")
                || self.is_key_pressed("Code(ShiftLeft)")
                || self.is_key_pressed("Code(ShiftRight)"),
            meta: self.is_key_pressed("Named(Super)")
                || self.is_key_pressed("Named(Meta)")
                || self.is_key_pressed("Code(SuperLeft)")
                || self.is_key_pressed("Code(SuperRight)")
                || self.is_key_pressed("Code(MetaLeft)")
                || self.is_key_pressed("Code(MetaRight)"),
        }
    }

    pub(super) fn sync_focus_dispatch(&mut self) {
        if self.scene.ui_roots.is_empty() {
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
                let _ = self.dispatch_blur_event(prev_id);
            }
            if let Some(next_id) = desired {
                let _ = self.dispatch_focus_event(next_id);
            }
        }
    }

    pub(super) fn resolve_cursor(&self) -> Cursor {
        if let Some(cursor) = self.cursor_override {
            return cursor;
        }
        let Some(target_id) = self.input_state.hovered_node_id else {
            return Cursor::Default;
        };
        for root in self.scene.ui_roots.iter().rev() {
            if let Some(cursor) =
                crate::view::base_component::get_cursor_by_id(root.as_ref(), target_id)
            {
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

/// Convert a platform-neutral mouse button into the viewport-internal
/// `MouseButton` enum. Kept as a free function (rather than `From`) so the
/// viewport owns the mapping without leaking its internal type into the
/// platform crate.
fn mouse_button_from_platform(button: PlatformMouseButton) -> MouseButton {
    match button {
        PlatformMouseButton::Left => MouseButton::Left,
        PlatformMouseButton::Right => MouseButton::Right,
        PlatformMouseButton::Middle => MouseButton::Middle,
        PlatformMouseButton::Back => MouseButton::Back,
        PlatformMouseButton::Forward => MouseButton::Forward,
        PlatformMouseButton::Other(code) => MouseButton::Other(code),
    }
}

impl Viewport {
    pub fn has_viewport_mouse_listeners(&self) -> bool {
        !self.viewport_mouse_move_listeners.is_empty()
            || !self.viewport_mouse_up_listeners.is_empty()
    }

    fn apply_viewport_listener_actions(&mut self, actions: Vec<ViewportListenerAction>) {
        let mut selection_changed = false;
        for action in actions {
            match action {
                ViewportListenerAction::AddMouseMoveListener(handler) => {
                    self.viewport_mouse_move_listeners.push(handler);
                }
                ViewportListenerAction::AddMouseUpListener(handler) => {
                    self.viewport_mouse_up_listeners
                        .push(ViewportMouseUpListener::Persistent(handler));
                }
                ViewportListenerAction::AddMouseUpListenerUntil(handler) => {
                    self.viewport_mouse_up_listeners
                        .push(ViewportMouseUpListener::Until(handler));
                }
                ViewportListenerAction::SetFocus(node_id) => {
                    self.set_focused_node_id(node_id);
                }
                ViewportListenerAction::SetCursor(cursor) => {
                    self.set_cursor(cursor);
                }
                ViewportListenerAction::SelectTextRangeAll(target_id) => {
                    for root in self.scene.ui_roots.iter_mut().rev() {
                        if crate::view::base_component::select_all_text_by_id(root.as_mut(), target_id) {
                            selection_changed = true;
                            break;
                        }
                    }
                }
                ViewportListenerAction::SelectTextRange {
                    target_id,
                    start,
                    end,
                } => {
                    for root in self.scene.ui_roots.iter_mut().rev() {
                        if crate::view::base_component::select_text_range_by_id(
                            root.as_mut(),
                            target_id,
                            start,
                            end,
                        ) {
                            selection_changed = true;
                            break;
                        }
                    }
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
        self.viewport_mouse_move_listeners
            .retain(|listener| listener.id() != handle.0);
        self.viewport_mouse_up_listeners
            .retain(|listener| listener.id() != handle.0);
    }

    pub fn set_selects(&mut self, selects: Vec<u64>) {
        self.input_state.selects = selects;
    }

    pub fn selects(&self) -> &[u64] {
        &self.input_state.selects
    }

    pub fn set_mouse_position_viewport(&mut self, x: f32, y: f32) {
        self.input_state.mouse_position_viewport = Some((x, y));
    }

    pub fn clear_mouse_position_viewport(&mut self) {
        self.input_state.mouse_position_viewport = None;
        self.input_state.pointer_capture_node_id = None;
        let (hover_changed, hover_event_dispatched) = Self::sync_hover_target(
            &mut self.scene.ui_roots,
            &mut self.input_state.hovered_node_id,
            None,
        );
        let pointer_changed = Self::cancel_pointer_interactions(&mut self.scene.ui_roots);
        if hover_changed || hover_event_dispatched || pointer_changed {
            self.request_redraw();
        }
    }

    pub fn mouse_position_viewport(&self) -> Option<(f32, f32)> {
        self.input_state.mouse_position_viewport
    }

    pub fn set_mouse_button_pressed(&mut self, button: MouseButton, pressed: bool) {
        if pressed {
            self.input_state.pressed_mouse_buttons.insert(button);
        } else {
            self.input_state.pressed_mouse_buttons.remove(&button);
        }
    }

    pub fn is_mouse_button_pressed(&self, button: MouseButton) -> bool {
        self.input_state.pressed_mouse_buttons.contains(&button)
    }

    pub fn pressed_mouse_buttons(&self) -> impl Iterator<Item = MouseButton> + '_ {
        self.input_state.pressed_mouse_buttons.iter().copied()
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
        self.viewport_mouse_move_listeners.clear();
        self.viewport_mouse_up_listeners.clear();
        self.dispatched_focus_node_id = None;
        let (hover_changed, hover_event_dispatched) = Self::sync_hover_target(
            &mut self.scene.ui_roots,
            &mut self.input_state.hovered_node_id,
            None,
        );
        let pointer_changed = Self::cancel_pointer_interactions(&mut self.scene.ui_roots);
        if hover_changed || hover_event_dispatched || pointer_changed {
            self.request_redraw();
        }
    }

}
