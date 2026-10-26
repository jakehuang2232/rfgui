impl EventTarget for Element {
    fn dispatch_pointer_down(
        &mut self,
        event: &mut PointerDownEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
    ) {
        if self.handle_scrollbar_pointer_down(event, control, self_key) {
            event.meta.suppress_focus_change();
            event.meta.stop_propagation();
            return;
        }
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.pointer_down {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_pointer_up(
        &mut self,
        event: &mut PointerUpEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
    ) {
        if self.handle_scrollbar_pointer_up(event, control, self_key) {
            event.meta.stop_propagation();
            return;
        }
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.pointer_up {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_pointer_move(
        &mut self,
        event: &mut PointerMoveEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if self.handle_scrollbar_pointer_move(event, control) {
            event.meta.stop_propagation();
            return;
        }
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.pointer_move {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_click(
        &mut self,
        event: &mut ClickEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if self.is_scrollbar_hit(event.pointer.local_x, event.pointer.local_y) {
            event.meta.stop_propagation();
            return;
        }
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.click {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_wheel(
        &mut self,
        event: &mut crate::ui::WheelEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.wheel {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_context_menu(
        &mut self,
        event: &mut crate::ui::ContextMenuEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if self.is_scrollbar_hit(event.pointer.local_x, event.pointer.local_y) {
            event.meta.stop_propagation();
            return;
        }
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.context_menu {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_key_down(
        &mut self,
        event: &mut KeyDownEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.key_down {
                crate::ui::run_callback("an event handler", || handler(event, _control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_key_up(
        &mut self,
        event: &mut KeyUpEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.key_up {
                crate::ui::run_callback("an event handler", || handler(event, _control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_focus(
        &mut self,
        event: &mut FocusEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.focus {
                crate::ui::run_callback("an event handler", || handler(event, _control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_blur(
        &mut self,
        event: &mut BlurEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.blur {
                crate::ui::run_callback("an event handler", || handler(event, _control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_ime_commit(
        &mut self,
        event: &mut crate::ui::ImeCommitEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.ime_commit {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_ime_enabled(
        &mut self,
        event: &mut crate::ui::ImeEnabledEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.ime_enabled {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_ime_disabled(
        &mut self,
        event: &mut crate::ui::ImeDisabledEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.ime_disabled {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_drag_start(
        &mut self,
        event: &mut crate::ui::DragStartEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.drag_start {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_drag_over(
        &mut self,
        event: &mut crate::ui::DragOverEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.drag_over {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_drag_leave(
        &mut self,
        event: &mut crate::ui::DragLeaveEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.drag_leave {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_drop(
        &mut self,
        event: &mut crate::ui::DropEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.drop {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_drag_end(
        &mut self,
        event: &mut crate::ui::DragEndEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.drag_end {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_copy(
        &mut self,
        event: &mut crate::ui::CopyEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.copy {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_cut(
        &mut self,
        event: &mut crate::ui::CutEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.cut {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_paste(
        &mut self,
        event: &mut crate::ui::PasteEvent,
        control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.paste {
                crate::ui::run_callback("an event handler", || handler(event, control));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn cancel_pointer_interaction(&mut self) -> bool {
        let canceled = self.scrollbar_drag.take().is_some();
        if canceled {
            self.note_scrollbar_interaction();
        }
        canceled
    }

    fn hover_update_needed(&self, hovered: bool) -> bool {
        self.is_hovered != hovered
    }

    fn set_hovered(&mut self, hovered: bool) -> bool {
        if self.is_hovered == hovered {
            return false;
        }
        self.is_hovered = hovered;
        // Hover is an animation boundary only for actual overflowing scrollbars.
        // Leave starts their hold/fade epoch at the next viewport frame sample.
        if self.has_scrollbar() {
            self.note_scrollbar_interaction();
        }
        self.recompute_style();
        true
    }

    fn dispatch_pointer_enter(
        &mut self,
        event: &mut PointerEnterEvent,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.pointer_enter {
                crate::ui::run_callback("an event handler", || handler(event));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn dispatch_pointer_leave(
        &mut self,
        event: &mut PointerLeaveEvent,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.pointer_leave {
                crate::ui::run_callback("an event handler", || handler(event));
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn scroll_by(&mut self, dx: f32, dy: f32) -> bool {
        let can_scroll = !matches!(self.scroll_direction, ScrollDirection::None);
        if !can_scroll {
            return false;
        }
        let (max_scroll_x, max_scroll_y) = self.scroll_limits();
        let mut next_x = self.requested_scroll_offset.x;
        let mut next_y = self.requested_scroll_offset.y;
        match self.scroll_direction {
            ScrollDirection::Horizontal => {
                next_x = (next_x + dx).clamp(0.0, max_scroll_x);
            }
            ScrollDirection::Vertical => {
                next_y = (next_y + dy).clamp(0.0, max_scroll_y);
            }
            ScrollDirection::Both => {
                next_x = (next_x + dx).clamp(0.0, max_scroll_x);
                next_y = (next_y + dy).clamp(0.0, max_scroll_y);
            }
            ScrollDirection::None => {}
        }
        let changed = !approx_eq(next_x, self.requested_scroll_offset.x)
            || !approx_eq(next_y, self.requested_scroll_offset.y);
        if changed {
            self.note_scrollbar_interaction();
        }
        if self.scroll_to(Position {
            x: next_x,
            y: next_y,
        }) {
            self.mark_scroll_dirty();
        }
        changed
    }

    fn can_scroll_by(&self, dx: f32, dy: f32) -> bool {
        let can_scroll = !matches!(self.scroll_direction, ScrollDirection::None);
        if !can_scroll {
            return false;
        }
        let (max_scroll_x, max_scroll_y) = self.scroll_limits();
        let mut next_x = self.requested_scroll_offset.x;
        let mut next_y = self.requested_scroll_offset.y;
        match self.scroll_direction {
            ScrollDirection::Horizontal => {
                next_x = (next_x + dx).clamp(0.0, max_scroll_x);
            }
            ScrollDirection::Vertical => {
                next_y = (next_y + dy).clamp(0.0, max_scroll_y);
            }
            ScrollDirection::Both => {
                next_x = (next_x + dx).clamp(0.0, max_scroll_x);
                next_y = (next_y + dy).clamp(0.0, max_scroll_y);
            }
            ScrollDirection::None => {}
        }
        !approx_eq(next_x, self.requested_scroll_offset.x)
            || !approx_eq(next_y, self.requested_scroll_offset.y)
    }

    fn get_scroll_offset(&self) -> (f32, f32) {
        (
            self.requested_scroll_offset.x,
            self.requested_scroll_offset.y,
        )
    }

    fn set_scroll_offset(&mut self, offset: (f32, f32)) {
        if self.scroll_to(Position {
            x: offset.0,
            y: offset.1,
        }) {
            self.mark_scroll_dirty();
        }
    }

    fn cursor(&self) -> Cursor {
        self.computed_style.cursor
    }

    fn animation_frame_request(
        &self,
        now: Instant,
    ) -> crate::view::base_component::AnimationFrameRequest {
        use crate::view::base_component::AnimationFrameRequest;
        if self.scrollbar_interaction_pending {
            return AnimationFrameRequest::NextFrame;
        }
        if self.is_hovered || self.scrollbar_drag.is_some() {
            return AnimationFrameRequest::None;
        }
        match self.last_scrollbar_interaction {
            Some(last) if now < last + SCROLLBAR_HOLD => {
                AnimationFrameRequest::At(last + SCROLLBAR_HOLD)
            }
            Some(_) => AnimationFrameRequest::NextFrame,
            None => AnimationFrameRequest::None,
        }
    }

    fn take_style_transition_requests(&mut self) -> Vec<StyleTrackRequest> {
        self.transition_requests
            .as_mut()
            .map_or_else(Vec::new, |r| std::mem::take(&mut r.style))
    }

    fn take_animation_requests(&mut self) -> Vec<crate::transition::AnimationRequest> {
        self.transition_requests
            .as_mut()
            .map_or_else(Vec::new, |r| std::mem::take(&mut r.animation))
    }

    fn take_layout_transition_requests(&mut self) -> Vec<LayoutTrackRequest> {
        self.transition_requests
            .as_mut()
            .map_or_else(Vec::new, |r| std::mem::take(&mut r.layout))
    }

    fn take_visual_transition_requests(&mut self) -> Vec<VisualTrackRequest> {
        self.transition_requests
            .as_mut()
            .map_or_else(Vec::new, |r| std::mem::take(&mut r.visual))
    }
}
