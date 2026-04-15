impl EventTarget for Element {
    fn dispatch_mouse_down(
        &mut self,
        event: &mut MouseDownEvent,
        control: &mut ViewportControl<'_>,
    ) {
        if self.handle_scrollbar_mouse_down(event, control) {
            event.meta.keep_focus();
            event.meta.stop_propagation();
            return;
        }
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.mouse_down {
                handler(event, control);
            }
        }
    }

    fn dispatch_mouse_up(&mut self, event: &mut MouseUpEvent, control: &mut ViewportControl<'_>) {
        if self.handle_scrollbar_mouse_up(event, control) {
            event.meta.stop_propagation();
            return;
        }
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.mouse_up {
                handler(event, control);
            }
        }
    }

    fn dispatch_mouse_move(
        &mut self,
        event: &mut MouseMoveEvent,
        control: &mut ViewportControl<'_>,
    ) {
        if self.handle_scrollbar_mouse_move(event, control) {
            event.meta.stop_propagation();
            return;
        }
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.mouse_move {
                handler(event, control);
            }
        }
    }

    fn dispatch_click(&mut self, event: &mut ClickEvent, control: &mut ViewportControl<'_>) {
        if self.is_scrollbar_hit(event.mouse.local_x, event.mouse.local_y) {
            event.meta.stop_propagation();
            return;
        }
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.click {
                handler(event, control);
            }
        }
    }

    fn dispatch_key_down(&mut self, event: &mut KeyDownEvent, _control: &mut ViewportControl<'_>) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.key_down {
                handler(event, _control);
            }
        }
    }

    fn dispatch_key_up(&mut self, event: &mut KeyUpEvent, _control: &mut ViewportControl<'_>) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.key_up {
                handler(event, _control);
            }
        }
    }

    fn dispatch_focus(&mut self, event: &mut FocusEvent, _control: &mut ViewportControl<'_>) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.focus {
                handler(event, _control);
            }
        }
    }

    fn dispatch_blur(&mut self, event: &mut BlurEvent, _control: &mut ViewportControl<'_>) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.blur {
                handler(event, _control);
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

    fn dispatch_mouse_enter(&mut self, event: &mut MouseEnterEvent) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.mouse_enter {
                handler(event);
            }
        }
    }

    fn dispatch_mouse_leave(&mut self, event: &mut MouseLeaveEvent) {
        if let Some(h) = &mut self.event_handlers {
            for handler in &mut h.mouse_leave {
                handler(event);
            }
        }
    }

    fn scroll_by(&mut self, dx: f32, dy: f32) -> bool {
        let can_scroll = !matches!(self.scroll_direction, ScrollDirection::None);
        if !can_scroll {
            return false;
        }
        let max_scroll_x = (self.content_size.width - self.layout_inner_size.width).max(0.0);
        let max_scroll_y = (self.content_size.height - self.layout_inner_size.height).max(0.0);
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
        let max_scroll_x = (self.content_size.width - self.layout_inner_size.width).max(0.0);
        let max_scroll_y = (self.content_size.height - self.layout_inner_size.height).max(0.0);
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
