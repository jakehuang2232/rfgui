use super::*;

#[test]
fn null_clipboard_roundtrip() {
    let mut cb = NullClipboard::default();
    assert_eq!(cb.get(), None);
    cb.set("abc");
    assert_eq!(cb.get().as_deref(), Some("abc"));
}

#[test]
fn null_cursor_is_noop() {
    let mut c = NullCursorSink;
    c.set_cursor(Cursor::Pointer);
}

#[test]
fn null_redraw_is_noop() {
    let r = NullRedrawRequester;
    r.request_redraw();
}

#[test]
fn headless_bundle_defaults() {
    let b = HeadlessBackend::default();
    assert_eq!(b.clipboard.buf, None);
}
