use super::*;

#[test]
fn nested_costs_are_exclusive_and_repeated_scopes_accumulate() {
    let mut profile = Profile::default();
    profile.stack.push(Duration::ZERO);
    for _ in 0..2 {
        profile.stack.push(Duration::ZERO);
        profile.complete_scope("child", Duration::from_millis(2));
    }
    profile.complete_scope("parent", Duration::from_millis(10));
    assert_eq!(profile.phases["child"], (Duration::from_millis(4), 2));
    assert_eq!(profile.phases["parent"], (Duration::from_millis(6), 1));
    assert_eq!(
        profile.phases.values().map(|p| p.0).sum::<Duration>(),
        Duration::from_millis(10)
    );
}

#[test]
fn disabled_scope_does_not_start_a_clock_or_record_costs() {
    PROFILE.with(|p| *p.borrow_mut() = Profile::default());
    let scope = scope("disabled");
    assert!(scope.0.is_none());
    drop(scope);
    count("disabled", 1);
    PROFILE.with(|p| {
        assert!(p.borrow().stack.is_empty());
        assert!(p.borrow().phases.is_empty());
    });
}
