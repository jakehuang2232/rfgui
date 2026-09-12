use super::*;

#[test]
fn arboard_constructor_does_not_panic() {
    // May return None on headless CI; either outcome is fine — we just
    // care that the call itself is sound.
    let _ = ArboardClipboard::new();
}
