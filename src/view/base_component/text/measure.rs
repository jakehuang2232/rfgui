//! Text measure pipeline: shape / relayout / cache helpers and the
//! `measure_text_size` convenience used by tests.

use std::sync::Arc;

use super::Text;
use super::cache::{
    MEASURE_TEXT_CACHE, MeasuredTextLayout, TextLayoutCacheKey, make_measure_cache_key,
    quantize_milli,
};
use super::profile::{record_text_measure_profile, text_measure_profile_enabled};
use crate::time::Instant;
use crate::view::base_component::DirtyFlags;
use crate::view::text_layout::{TextLayoutAlignment, build_text_layout};

impl Text {
    pub(super) fn relayout_from_base(
        &mut self,
        width: Option<f32>,
        allow_wrap: bool,
    ) -> MeasuredTextLayout {
        let started_at = text_measure_profile_enabled().then(Instant::now);
        let cache_key = TextLayoutCacheKey {
            width_milli: width.map(quantize_milli).unwrap_or(-1),
            allow_wrap,
        };
        if let Some(cached) = self.layout_cache.get(&cache_key).cloned() {
            if let Some(started_at) = started_at {
                let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
                record_text_measure_profile(|profile| {
                    profile.relayout_from_base_calls += 1;
                    profile.relayout_from_base_cache_hits += 1;
                    profile.relayout_from_base_ms += elapsed_ms;
                });
            }
            return cached;
        }

        let text_layout = build_text_layout(
            self.content.as_str(),
            width,
            allow_wrap,
            self.font_size,
            self.line_height,
            self.font_weight,
            self.align,
            self.font_families.as_slice(),
        );
        let (measured_width, measured_height) = text_layout.layout.measure_size();
        let measured = MeasuredTextLayout {
            text_layout: Arc::new(text_layout.layout),
            width: measured_width,
            height: measured_height,
        };
        self.layout_cache.insert(cache_key, measured.clone());
        if let Some(started_at) = started_at {
            let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
            record_text_measure_profile(|profile| {
                profile.relayout_from_base_calls += 1;
                profile.relayout_from_base_ms += elapsed_ms;
            });
        }
        measured
    }

    pub(super) fn clear_layout_caches(&mut self) {
        self.cached_intrinsic_layout = None;
        self.cached_height_for_width = None;
        self.layout_cache.clear();
        self.inline_plan_cache.clear();
        self.text_layout = None;
        self.inline_plan = None;
    }

    pub(super) fn mark_measure_dirty(&mut self) {
        self.measure_revision = self.measure_revision.wrapping_add(1);
        self.clear_layout_caches();
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
    }
}

pub(in crate::view::base_component) fn measure_text_layout(
    content: &str,
    max_width: Option<f32>,
    allow_wrap: bool,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    align: TextLayoutAlignment,
    font_families: &[String],
) -> MeasuredTextLayout {
    let started_at = text_measure_profile_enabled().then(Instant::now);
    let cache_key = make_measure_cache_key(
        content,
        max_width,
        font_size,
        line_height,
        font_weight,
        align,
        font_families,
    );
    if let Some(cached) = MEASURE_TEXT_CACHE.with(|cache| cache.borrow_mut().get_cloned(&cache_key))
    {
        if let Some(started_at) = started_at {
            let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
            record_text_measure_profile(|profile| {
                profile.measure_text_layout_calls += 1;
                profile.measure_text_layout_cache_hits += 1;
                profile.measure_text_layout_ms += elapsed_ms;
            });
        }
        return cached;
    }

    let text_layout = build_text_layout(
        content,
        max_width,
        allow_wrap,
        font_size,
        line_height,
        font_weight,
        align,
        font_families,
    );
    let (width, height) = text_layout.layout.measure_size();
    let measured = MeasuredTextLayout {
        text_layout: Arc::new(text_layout.layout),
        width,
        height,
    };
    MEASURE_TEXT_CACHE.with(|cache| {
        cache.borrow_mut().insert(cache_key, measured.clone());
    });
    if let Some(started_at) = started_at {
        let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
        record_text_measure_profile(|profile| {
            profile.measure_text_layout_calls += 1;
            profile.measure_text_layout_ms += elapsed_ms;
        });
    }
    measured
}

#[cfg(test)]
pub(crate) fn measure_text_size(
    content: &str,
    max_width: Option<f32>,
    allow_wrap: bool,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    align: TextLayoutAlignment,
    font_families: &[String],
) -> (f32, f32) {
    let measured = measure_text_layout(
        content,
        max_width,
        allow_wrap,
        font_size,
        line_height,
        font_weight,
        align,
        font_families,
    );
    (measured.width, measured.height)
}
