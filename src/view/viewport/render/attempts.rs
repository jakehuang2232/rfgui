//! Per-thread observations of executed selector work, never production telemetry.
use std::cell::RefCell;
use std::collections::BTreeMap;
thread_local! {
    static COUNTS: RefCell<Option<BTreeMap<&'static str, usize>>> = const { RefCell::new(None) };
}
pub(super) fn record(kind: &'static str) {
    COUNTS.with(|counts| {
        if let Some(counts) = counts.borrow_mut().as_mut() {
            *counts.entry(kind).or_default() += 1;
        }
    });
}
pub(super) fn observe<T>(run: impl FnOnce() -> T) -> (T, BTreeMap<&'static str, usize>) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            COUNTS.with(|counts| counts.replace(None));
        }
    }
    COUNTS.with(|counts| assert!(counts.replace(Some(BTreeMap::new())).is_none()));
    let _reset = Reset;
    let value = run();
    let counts = COUNTS.with(|counts| counts.take().unwrap());
    (value, counts)
}
