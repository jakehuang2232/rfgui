use super::*;
use crate::ui::{Binding, batch_state_updates, flush_state_updates};

#[test]
fn queue_profile_counts_actual_actions_and_final_changes() {
    let a = Binding::new(0);
    let b = Binding::new(0);
    let (_, p) = profile_ui_work(|| {
        batch_state_updates(|| {
            a.set(1);
            a.update(|n| *n += 1);
            b.set(1);
            b.set(0);
        })
    });
    assert_eq!(
        (p.state_targets, p.state_actions, p.changed_targets),
        (2, 4, 1)
    );
    assert_eq!(a.snapshot().get(), 2);
    assert!(p.state_flush_ms.is_finite());
    assert_eq!(snapshot().state_actions, 0);
}

#[test]
fn recursive_phase_does_not_double_count_inclusive_time() {
    let _capture = capture(true);
    let outer = scope(Phase::Unwrap);
    let inner = scope(Phase::Unwrap);
    assert!(outer.start.is_some());
    assert!(inner.start.is_none());
    drop(inner);
    assert_eq!(snapshot().unwrap_ms, 0.0);
    drop(outer);
    ACTIVE.with(|a| assert_eq!(a.borrow().as_ref().unwrap().depth, [0; 7]));
}

#[test]
fn unwind_releases_capture_and_nested_capture_keeps_outer_counts() {
    let result = std::panic::catch_unwind(|| {
        profile_ui_work(|| {
            let _scope = scope(Phase::Unwrap);
            panic!("profile unwind");
        })
    });
    assert!(result.is_err());
    ACTIVE.with(|a| assert!(a.borrow().is_none()));
    let (_, p) = profile_ui_work(|| {
        count(|p| p.component_renders += 1);
        let (_, nested) = profile_ui_work(|| count(|p| p.component_renders += 1));
        assert_eq!(nested.component_renders, 2);
        count(|p| p.component_renders += 1);
    });
    assert_eq!(p.component_renders, 3);
    flush_state_updates();
}
