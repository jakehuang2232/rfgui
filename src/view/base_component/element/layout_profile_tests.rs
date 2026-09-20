use super::*;

#[test]
fn disabled_scope_does_not_enter_the_timing_stack_or_update_profile() {
    let _enabled = enable_layout_profile_scoped(false);
    reset_layout_place_profile();
    let scope = layout_profile_scope(LayoutPlaceTiming::AxisSolve);
    LAYOUT_PLACE_TIMING_STACK.with(|s| assert!(s.borrow().is_empty()));
    with_layout_place_profile(|p| p.axis_solve_calls += 1);
    drop(scope);
    let p = take_layout_place_profile();
    assert_eq!(p.axis_solve_calls, 0);
    assert_eq!(p.axis_solve_ms, 0.0);
}

#[test]
fn nested_timer_unwind_restores_stack_and_prior_enable_state() {
    let _outer_enabled = enable_layout_profile_scoped(true);
    reset_layout_place_profile();
    {
        let _disabled = enable_layout_profile_scoped(false);
        assert!(!layout_place_profile_enabled());
    }
    assert!(layout_place_profile_enabled());
    let result = std::panic::catch_unwind(|| {
        let _enabled = enable_layout_profile_scoped(true);
        let _outer = layout_profile_scope(LayoutPlaceTiming::MeasureBody);
        let _inner = layout_profile_scope(LayoutPlaceTiming::InlineIfcCandidate);
        LAYOUT_PLACE_TIMING_STACK.with(|s| assert_eq!(s.borrow().len(), 2));
        panic!("abort diagnostics");
    });
    assert!(result.is_err());
    assert!(layout_place_profile_enabled());
    LAYOUT_PLACE_TIMING_STACK.with(|s| assert!(s.borrow().is_empty()));
    let p = take_layout_place_profile();
    assert!(p.measure_body_ms.is_finite() && p.measure_body_ms >= 0.0);
    assert!(p.inline_ifc_candidate_ms.is_finite() && p.inline_ifc_candidate_ms >= 0.0);
}
