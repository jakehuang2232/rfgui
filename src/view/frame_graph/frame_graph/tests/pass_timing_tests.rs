use super::*;

#[test]
fn disabled_pass_timing_collector_keeps_storage_empty() {
    let mut collector = PassTimingCollector::new(false);
    collector.record("pass", collector.start());

    assert!(collector.timings.is_empty());
    assert!(collector.counts.is_empty());
    assert!(collector.first_seen_order.is_empty());
    assert!(collector.finish().is_empty());
}

#[test]
fn enabled_pass_timing_collector_preserves_first_seen_order_and_counts() {
    let mut collector = PassTimingCollector::new(true);
    collector.record("first", collector.start());
    collector.record("second", collector.start());
    collector.record("first", collector.start());

    let rows = collector.finish();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "first");
    assert_eq!(rows[0].2, 2);
    assert_eq!(rows[1].0, "second");
    assert_eq!(rows[1].2, 1);
}
