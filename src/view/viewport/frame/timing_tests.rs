use super::*;

#[test]
fn consecutive_frame_checkpoints_include_work_between_calls() {
    let start = crate::time::Instant::now();
    let mut clock = FramePhaseClock::new(start);
    // Supplied boundaries model work and caller bookkeeping, with no sleeps
    // or scheduling assumptions. Each end must also be the following start.
    let phases: Vec<_> = [85, 507, 1007, 1607, 8310, 11955, 12380, 12548, 17697]
        .into_iter()
        .map(|us| clock.checkpoint_at(start + Duration::from_micros(us)))
        .collect();
    for (actual, expected) in phases
        .iter()
        .zip([0.085, 0.422, 0.5, 0.6, 6.703, 3.645, 0.425, 0.168, 5.149])
    {
        assert!((actual - expected).abs() < 1e-9);
    }
    assert!((clock.total_ms() - 17.697).abs() < 1e-9);
    assert!((phases.iter().sum::<f64>() - clock.total_ms()).abs() < 1e-9);
}

#[test]
fn frame_total_ends_at_the_final_boundary_without_an_extra_clock_sample() {
    let start = crate::time::Instant::now();
    let mut clock = FramePhaseClock::new(start);
    assert_eq!(clock.total_ms(), 0.0);
    assert_eq!(clock.checkpoint_at(start + Duration::from_millis(3)), 3.0);
    assert_eq!(clock.total_ms(), 3.0);
    // Reading total is observational: it must not advance the next boundary.
    assert_eq!(clock.total_ms(), 3.0);
    assert_eq!(clock.checkpoint_at(start + Duration::from_millis(5)), 2.0);
    assert_eq!(clock.total_ms(), 5.0);
}
