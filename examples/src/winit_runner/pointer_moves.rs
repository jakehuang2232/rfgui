use super::*;

impl Runner {
    pub(super) fn queue_pointer_move(&mut self, x: f32, y: f32, notify_app: bool) {
        self.pending_pointer_move = Some((x, y, notify_app));
        // Queue a host redraw even if the final move will not change pixels.
        // Admission runs after the pending move has reached the engine.
        if let Some(viewport) = self.viewport.as_mut() {
            viewport.request_redraw();
        }
    }

    pub(super) fn flush_pointer_move_before(&mut self, event: &WindowEvent) {
        // Every non-move boundary preserves ordering, including Down, Up,
        // Wheel, Key, IME, modifier changes and leaving/resizing the window.
        if !matches!(event, WindowEvent::CursorMoved { .. }) {
            self.flush_pointer_move();
        }
    }

    pub(super) fn flush_pointer_move(&mut self) {
        let Some((x, y, notify_app)) = self.pending_pointer_move.take() else {
            return;
        };
        let Some(viewport) = self.viewport.as_mut() else {
            return;
        };
        let event = PlatformPointerEvent {
            kind: PlatformPointerEventKind::Move { x, y },
            pointer_id: 0,
            pointer_type: PointerType::Mouse,
            pressure: 0.,
        };
        if notify_app {
            viewport.dispatch_app_event(
                &AppEvent::Pointer(event),
                PlatformServices {
                    clipboard: self.clipboard.as_mut(),
                    cursor: &mut self.cursor,
                    redraw: &self.redraw,
                },
            );
        }
        let _ = viewport.dispatch_platform_pointer_event(&event);
    }
}

#[cfg(test)]
mod tests;
