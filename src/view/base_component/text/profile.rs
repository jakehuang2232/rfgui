//! Text measure-time profiling counters.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

thread_local! {
    static TEXT_MEASURE_PROFILE: RefCell<TextMeasureProfile> =
        RefCell::new(TextMeasureProfile::default());
}

static TEXT_MEASURE_PROFILE_ENABLED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TextMeasureProfile {
    pub measure_inline_calls: usize,
    pub measure_inline_ms: f64,
    pub ensure_shaped_base_buffer_calls: usize,
    pub ensure_shaped_base_buffer_cache_hits: usize,
    pub ensure_shaped_base_buffer_ms: f64,
    pub relayout_from_base_calls: usize,
    pub relayout_from_base_cache_hits: usize,
    pub relayout_from_base_ms: f64,
    pub collect_wrapped_inline_fragments_calls: usize,
    pub collect_wrapped_inline_fragments_cache_hits: usize,
    pub collect_wrapped_inline_fragments_ms: f64,
    pub first_wrapped_fragment_calls: usize,
    pub first_wrapped_fragment_cache_hits: usize,
    pub first_wrapped_fragment_ms: f64,
    pub wrapped_suffix_fragments_calls: usize,
    pub wrapped_suffix_fragments_cache_hits: usize,
    pub wrapped_suffix_fragments_ms: f64,
    pub trimmed_suffix_shape_line_calls: usize,
    pub trimmed_suffix_shape_line_cache_hits: usize,
    pub trimmed_suffix_shape_line_ms: f64,
    pub measure_text_layout_calls: usize,
    pub measure_text_layout_cache_hits: usize,
    pub measure_text_layout_ms: f64,
}

pub(crate) fn set_text_measure_profile_enabled(enabled: bool) {
    TEXT_MEASURE_PROFILE_ENABLED.store(enabled, Ordering::Relaxed);
}

pub(crate) fn reset_text_measure_profile() {
    TEXT_MEASURE_PROFILE.with(|profile| {
        *profile.borrow_mut() = TextMeasureProfile::default();
    });
}

pub(crate) fn take_text_measure_profile() -> TextMeasureProfile {
    TEXT_MEASURE_PROFILE.with(|profile| std::mem::take(&mut *profile.borrow_mut()))
}

pub(super) fn text_measure_profile_enabled() -> bool {
    TEXT_MEASURE_PROFILE_ENABLED.load(Ordering::Relaxed)
}

pub(super) fn record_text_measure_profile(update: impl FnOnce(&mut TextMeasureProfile)) {
    if !text_measure_profile_enabled() {
        return;
    }
    TEXT_MEASURE_PROFILE.with(|profile| update(&mut profile.borrow_mut()));
}
