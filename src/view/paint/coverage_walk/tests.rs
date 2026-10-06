use super::*;

#[test]
fn walks_have_distinct_epochs_and_restore_the_enclosing_one() {
    assert_eq!(active(), None);
    let outer = begin();
    let first = active().expect("inside a walk");
    {
        let _inner = begin();
        let nested = active().expect("inside a nested walk");
        assert_ne!(nested, first);
    }
    assert_eq!(active(), Some(first));
    drop(outer);
    assert_eq!(active(), None);
    let _again = begin();
    assert_ne!(active(), Some(first), "a later walk never reuses an epoch");
}
