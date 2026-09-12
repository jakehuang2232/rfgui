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
