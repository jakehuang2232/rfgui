//! Per-thread observations of executed recording work, never production telemetry.
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
