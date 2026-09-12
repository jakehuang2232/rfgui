use super::*;

#[test]
fn register_appends_top() {
    let mut s = PopupStack::new();
    s.register(1);
    s.register(2);
    s.register(3);
    assert_eq!(s.as_slice(), &[1, 2, 3]);
}

#[test]
fn register_dedup() {
    let mut s = PopupStack::new();
    s.register(1);
    s.register(2);
    s.register(1);
    assert_eq!(s.as_slice(), &[1, 2]);
}

#[test]
fn promote_moves_to_top() {
    let mut s = PopupStack::new();
    s.register(1);
    s.register(2);
    s.register(3);
    s.promote(1);
    assert_eq!(s.as_slice(), &[2, 3, 1]);
}

#[test]
fn promote_already_top_is_noop() {
    let mut s = PopupStack::new();
    s.register(1);
    s.register(2);
    s.promote(2);
    assert_eq!(s.as_slice(), &[1, 2]);
}

#[test]
fn promote_missing_inserts_at_top() {
    let mut s = PopupStack::new();
    s.register(1);
    s.promote(2);
    assert_eq!(s.as_slice(), &[1, 2]);
}

#[test]
fn iter_top_down_is_reverse() {
    let mut s = PopupStack::new();
    s.register(1);
    s.register(2);
    s.register(3);
    let collected: Vec<u64> = s.iter_top_down().collect();
    assert_eq!(collected, vec![3, 2, 1]);
}

#[test]
fn zero_id_ignored() {
    let mut s = PopupStack::new();
    s.register(0);
    s.promote(0);
    assert!(s.is_empty());
}
