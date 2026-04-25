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
                handler(event, control);
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
                handler(event, control);
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
                handler(event, control);
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
                handler(event, control);
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
                handler(event, control);
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
                handler(event, control);
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
                handler(event, _control);
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
                handler(event, _control);
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
                handler(event, _control);
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
                handler(event, _control);
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
                handler(event, control);
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
                handler(event, control);
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
                handler(event, control);
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
                handler(event, control);
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
                handler(event, control);
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
                handler(event, control);
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
                handler(event, control);
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
                handler(event, control);
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
                handler(event, control);
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
                handler(event, control);
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
                handler(event, control);
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn cancel_pointer_interaction(&mut self) -> bool {
        self.scrollbar_drag.take().is_some()
    }

    fn set_hovered(&mut self, hovered: bool) -> bool {
        if self.is_hovered == hovered {
            return false;
        }
        self.is_hovered = hovered;
        if hovered {
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
                handler(event);
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
                handler(event);
                if event.meta.immediate_propagation_stopped() { break; }
            }
        }
    }

    fn scroll_by(&mut self, dx: f32, dy: f32) -> bool {
        let can_scroll = !matches!(self.scroll_direction, ScrollDirection::None);
        if !can_scroll {
            return false;
        }
        let max_scroll_x = (self.layout_state.content_size.width - self.layout_state.layout_inner_size.width).max(0.0);
        let max_scroll_y = (self.layout_state.content_size.height - self.layout_state.layout_inner_size.height).max(0.0);
        let mut next_x = self.scroll_offset.x;
        let mut next_y = self.scroll_offset.y;
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
        let changed =
            !approx_eq(next_x, self.scroll_offset.x) || !approx_eq(next_y, self.scroll_offset.y);
        self.scroll_offset.x = next_x;
        self.scroll_offset.y = next_y;
        if changed {
            self.note_scrollbar_interaction();
            self.mark_place_dirty();
        }
        changed
    }

    fn can_scroll_by(&self, dx: f32, dy: f32) -> bool {
        let can_scroll = !matches!(self.scroll_direction, ScrollDirection::None);
        if !can_scroll {
            return false;
        }
        let max_scroll_x = (self.layout_state.content_size.width - self.layout_state.layout_inner_size.width).max(0.0);
        let max_scroll_y = (self.layout_state.content_size.height - self.layout_state.layout_inner_size.height).max(0.0);
        let mut next_x = self.scroll_offset.x;
        let mut next_y = self.scroll_offset.y;
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
        !approx_eq(next_x, self.scroll_offset.x) || !approx_eq(next_y, self.scroll_offset.y)
    }

    fn get_scroll_offset(&self) -> (f32, f32) {
        (self.scroll_offset.x, self.scroll_offset.y)
    }

    fn set_scroll_offset(&mut self, offset: (f32, f32)) {
        let changed = !approx_eq(self.scroll_offset.x, offset.0)
            || !approx_eq(self.scroll_offset.y, offset.1);
        self.scroll_offset.x = offset.0;
        self.scroll_offset.y = offset.1;
        if changed {
            self.mark_place_dirty();
        }
    }

    fn cursor(&self) -> Cursor {
        self.computed_style.cursor
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
