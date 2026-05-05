//! `EventTarget` impl for Text. Text is a typography leaf — no event handlers,
//! no scroll, no animator. Authors who need pointer/keyboard handlers wrap
//! `<Element>` outside the `<Text>`.

use crate::style::Cursor;
use crate::view::base_component::EventTarget;

use super::Text;

impl EventTarget for Text {
    fn cursor(&self) -> Cursor {
        self.cursor
    }
    // All other EventTarget methods (dispatch_pointer_*, dispatch_key_*,
    // dispatch_focus, dispatch_blur, dispatch_wheel, dispatch_click,
    // dispatch_context_menu, plus get_scroll_offset / set_scroll_offset /
    // ime_cursor_rect / wants_animation_frame / take_*_requests) take the
    // trait default. Defaults are no-ops / `None` / empty Vec — exactly what
    // a leaf Text needs.
}
