//! Closure-backed platform helpers usable on any host.
//!
//! These are intentionally free of `winit`, `web_sys`, and every other
//! platform crate. A host wires up its own window/redraw plumbing and
//! passes a closure in; the helpers adapt the closure to the platform
//! trait shape so the rfgui runtime can drive it.

use super::{CursorSink, RedrawRequester};
use crate::Cursor;
use std::sync::Arc;

/// Cursor sink backed by a user-supplied closure. Host is responsible for
/// actually applying the cursor to the window (`winit::Window::set_cursor`,
/// canvas CSS, native API, …).
pub struct CallbackCursorSink {
    apply: Box<dyn FnMut(Cursor) + Send>,
}

impl CallbackCursorSink {
    pub fn new<F>(apply: F) -> Self
    where
        F: FnMut(Cursor) + Send + 'static,
    {
        Self {
            apply: Box::new(apply),
        }
    }
}

impl CursorSink for CallbackCursorSink {
    fn set_cursor(&mut self, cursor: Cursor) {
        (self.apply)(cursor);
    }
}

/// Redraw requester backed by a user-supplied closure. Typical hosts call
/// `window.request_redraw()` or schedule a `requestAnimationFrame`.
///
/// Wrapped in `Arc` so the host can clone it freely and stash it wherever
/// it needs to be (inside `Rc<RefCell<_>>`, another thread, …). The closure
/// itself must be `Fn` (not `FnMut`) for the same reason.
#[derive(Clone)]
pub struct CallbackRedrawRequester {
    schedule: Arc<dyn Fn() + Send + Sync>,
}

impl CallbackRedrawRequester {
    pub fn new<F>(schedule: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        Self {
            schedule: Arc::new(schedule),
        }
    }
}

impl RedrawRequester for CallbackRedrawRequester {
    fn request_redraw(&self) {
        (self.schedule)();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn callback_cursor_forwards() {
        let seen = Arc::new(Mutex::new(None));
        let seen_clone = seen.clone();
        let mut sink = CallbackCursorSink::new(move |c| {
            *seen_clone.lock().unwrap() = Some(c);
        });
        sink.set_cursor(Cursor::Pointer);
        assert_eq!(*seen.lock().unwrap(), Some(Cursor::Pointer));
    }

    #[test]
    fn callback_redraw_fires() {
        let count = Arc::new(Mutex::new(0u32));
        let count_clone = count.clone();
        let r = CallbackRedrawRequester::new(move || {
            *count_clone.lock().unwrap() += 1;
        });
        r.request_redraw();
        r.request_redraw();
        assert_eq!(*count.lock().unwrap(), 2);
    }
}
