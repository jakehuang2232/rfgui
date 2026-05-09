//! Inline-fragment plan builder.

use std::sync::Arc;

use cosmic_text::{Buffer as GlyphBuffer, Hinting, Wrap};

use super::Text;
use super::cache::{
    FirstLineLayoutCacheEntry, FirstLineLayoutCacheKey, InlinePlanCacheKey, MeasuredTextLayout,
    WrappedSuffixCacheKey, quantize_milli,
};
use super::measure_text_layout;
use super::profile::{record_text_measure_profile, text_measure_profile_enabled};
use crate::style::TextWrap;
use crate::time::Instant;

#[derive(Clone, Debug, Default)]
pub(in crate::view::base_component) struct InlineTextFragment {
    pub(super) content: String,
    pub(super) width: f32,
    pub(super) height: f32,
    /// Distance from fragment top to typography baseline. Sourced from
    /// cosmic-text `LayoutRun.line_y - line_top` (already includes
    /// line-height leading/2). See `docs/design/inline-baseline.md` D1/D4.
    pub(super) baseline: f32,
    pub(super) position: Option<super::super::Position>,
    pub(super) layout_buffer: Option<Arc<GlyphBuffer>>,
}

#[derive(Clone, Debug, Default)]
pub(in crate::view::base_component) struct InlineTextPlan {
    pub(super) runs: Vec<InlineTextFragment>,
    pub(super) max_width: f32,
    pub(super) max_height: f32,
}

impl Text {
    pub(super) fn first_wrapped_fragment(
        &mut self,
        first_width: f32,
    ) -> Option<FirstLineLayoutCacheEntry> {
        let started_at = text_measure_profile_enabled().then(Instant::now);
        let cache_key = FirstLineLayoutCacheKey {
            first_width_milli: quantize_milli(first_width.max(1.0)),
        };
        if let Some(cached) = self.first_line_fragment_cache.get(&cache_key).cloned() {
            if let Some(started_at) = started_at {
                let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
                record_text_measure_profile(|profile| {
                    profile.first_wrapped_fragment_calls += 1;
                    profile.first_wrapped_fragment_cache_hits += 1;
                    profile.first_wrapped_fragment_ms += elapsed_ms;
                });
            }
            return Some(cached);
        }

        let shaped = self.get_global_shaped_text();
        let shape_line = shaped.shape_line.as_ref()?;
        let layout_line = shape_line
            .layout(
                self.font_size,
                Some(first_width.max(1.0)),
                Wrap::WordOrGlyph,
                Some(self.align),
                None,
                Hinting::Disabled,
            )
            .into_iter()
            .next()?;
        let content = if let (Some(first), Some(last)) =
            (layout_line.glyphs.first(), layout_line.glyphs.last())
        {
            self.content[first.start..last.end].to_string()
        } else {
            String::new()
        };
        let consumed_bytes = layout_line
            .glyphs
            .last()
            .map(|glyph| glyph.end)
            .unwrap_or(0);
        let width = layout_line
            .glyphs
            .iter()
            .fold(layout_line.w, |current, glyph| {
                current.max(glyph.x + glyph.w.max(0.0))
            })
            .max(1.0);
        let fragment_buffer = measure_text_layout(
            content.as_str(),
            Some(width),
            false,
            self.font_size,
            self.line_height,
            self.font_weight,
            self.align,
            self.font_families.as_slice(),
        )
        .buffer;

        // Mirror cosmic-text's LayoutRunIter::next centering formula
        // (`buffer.rs`: `centering_offset + max_ascent`) so the
        // first-line wrapped fragment baseline matches the
        // `LayoutRun.line_y - line_top` value used elsewhere.
        let effective_line_height = layout_line
            .line_height_opt
            .unwrap_or(self.font_size * self.line_height.max(0.8))
            .max(1.0);
        let glyph_height = layout_line.max_ascent + layout_line.max_descent;
        let leading = (effective_line_height - glyph_height).max(0.0);
        let baseline = (layout_line.max_ascent + leading / 2.0).max(0.0);
        let entry = FirstLineLayoutCacheEntry {
            consumed_bytes,
            fragment: InlineTextFragment {
                content,
                width,
                height: effective_line_height,
                baseline,
                position: None,
                layout_buffer: Some(fragment_buffer),
            },
        };
        self.first_line_fragment_cache
            .insert(cache_key, entry.clone());
        if let Some(started_at) = started_at {
            let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
            record_text_measure_profile(|profile| {
                profile.first_wrapped_fragment_calls += 1;
                profile.first_wrapped_fragment_ms += elapsed_ms;
            });
        }
        Some(entry)
    }

    pub(super) fn wrapped_suffix_fragments(
        &mut self,
        suffix_start: usize,
        full_width: f32,
    ) -> Vec<InlineTextFragment> {
        let started_at = text_measure_profile_enabled().then(Instant::now);
        let cache_key = WrappedSuffixCacheKey {
            suffix_start,
            full_width_milli: quantize_milli(full_width.max(1.0)),
        };
        if let Some(cached) = self.wrapped_suffix_cache.get(&cache_key).cloned() {
            if let Some(started_at) = started_at {
                let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
                record_text_measure_profile(|profile| {
                    profile.wrapped_suffix_fragments_calls += 1;
                    profile.wrapped_suffix_fragments_cache_hits += 1;
                    profile.wrapped_suffix_fragments_ms += elapsed_ms;
                });
            }
            return cached;
        }

        let mut fragments = Vec::new();
        if suffix_start < self.content.len() {
            let remaining_content = &self.content[suffix_start..];
            let remaining_layout = measure_text_layout(
                remaining_content,
                Some(full_width.max(1.0)),
                true,
                self.font_size,
                self.line_height,
                self.font_weight,
                self.align,
                self.font_families.as_slice(),
            );
            fragments = self.fragments_from_measured_layout(&remaining_layout);
        }

        self.wrapped_suffix_cache
            .insert(cache_key, fragments.clone());
        if let Some(started_at) = started_at {
            let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
            record_text_measure_profile(|profile| {
                profile.wrapped_suffix_fragments_calls += 1;
                profile.wrapped_suffix_fragments_ms += elapsed_ms;
            });
        }
        fragments
    }

    pub(super) fn collect_wrapped_inline_fragments(
        &mut self,
        first_width: f32,
        full_width: f32,
    ) -> Vec<InlineTextFragment> {
        let started_at = text_measure_profile_enabled().then(Instant::now);
        let first_width = first_width.max(1.0);
        let full_width = full_width.max(1.0);
        let first_key = FirstLineLayoutCacheKey {
            first_width_milli: quantize_milli(first_width),
        };
        let first_cache_hit = self.first_line_fragment_cache.contains_key(&first_key);
        let mut fragments = Vec::new();

        if (first_width - full_width).abs() <= 0.01 {
            let full_layout = measure_text_layout(
                self.content.as_str(),
                Some(full_width),
                true,
                self.font_size,
                self.line_height,
                self.font_weight,
                self.align,
                self.font_families.as_slice(),
            );
            fragments = self.fragments_from_measured_layout(&full_layout);
            if let Some(started_at) = started_at {
                let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
                record_text_measure_profile(|profile| {
                    profile.collect_wrapped_inline_fragments_calls += 1;
                    profile.collect_wrapped_inline_fragments_ms += elapsed_ms;
                });
            }
            return fragments;
        }

        let Some(first_line) = self.first_wrapped_fragment(first_width) else {
            if let Some(started_at) = started_at {
                let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
                record_text_measure_profile(|profile| {
                    profile.collect_wrapped_inline_fragments_calls += 1;
                    profile.collect_wrapped_inline_fragments_ms += elapsed_ms;
                });
            }
            return fragments;
        };

        fragments.push(first_line.fragment.clone());
        let suffix_key = WrappedSuffixCacheKey {
            suffix_start: first_line.consumed_bytes,
            full_width_milli: quantize_milli(full_width),
        };
        let suffix_cache_hit = self.wrapped_suffix_cache.contains_key(&suffix_key);
        fragments.extend(self.wrapped_suffix_fragments(first_line.consumed_bytes, full_width));
        let cache_hit = first_cache_hit
            && (suffix_cache_hit || first_line.consumed_bytes >= self.content.len());

        if let Some(started_at) = started_at {
            let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
            record_text_measure_profile(|profile| {
                profile.collect_wrapped_inline_fragments_calls += 1;
                if cache_hit {
                    profile.collect_wrapped_inline_fragments_cache_hits += 1;
                }
                profile.collect_wrapped_inline_fragments_ms += elapsed_ms;
            });
        }
        fragments
    }

    pub(super) fn fragments_from_measured_layout(
        &self,
        measured: &MeasuredTextLayout,
    ) -> Vec<InlineTextFragment> {
        measured
            .buffer
            .layout_runs()
            .map(|run| {
                let content =
                    if let (Some(first), Some(last)) = (run.glyphs.first(), run.glyphs.last()) {
                        run.text[first.start..last.end].to_string()
                    } else {
                        String::new()
                    };
                let width = run
                    .glyphs
                    .iter()
                    .fold(run.line_w, |current, glyph| {
                        current.max(glyph.x + glyph.w.max(0.0))
                    })
                    .max(1.0);
                let fragment_buffer = measure_text_layout(
                    content.as_str(),
                    Some(width),
                    false,
                    self.font_size,
                    self.line_height,
                    self.font_weight,
                    self.align,
                    self.font_families.as_slice(),
                )
                .buffer;

                InlineTextFragment {
                    content,
                    width,
                    height: run.line_height.max(1.0),
                    baseline: (run.line_y - run.line_top).max(0.0),
                    position: None,
                    layout_buffer: Some(fragment_buffer),
                }
            })
            .collect()
    }

    pub(super) fn build_inline_plan(
        &mut self,
        first_width: f32,
        full_width: f32,
    ) -> InlineTextPlan {
        let cache_key = InlinePlanCacheKey {
            first_width_milli: quantize_milli(first_width.max(1.0)),
            full_width_milli: quantize_milli(full_width.max(1.0)),
            text_wrap: match self.text_wrap {
                TextWrap::NoWrap => 0,
                TextWrap::Wrap => 1,
            },
        };
        if let Some(cached) = self.inline_plan_cache.get(&cache_key).cloned() {
            return cached;
        }

        let runs = if self.text_wrap == TextWrap::NoWrap {
            let layout = measure_text_layout(
                self.content.as_str(),
                None,
                false,
                self.font_size,
                self.line_height,
                self.font_weight,
                self.align,
                self.font_families.as_slice(),
            );
            // Single-fragment NoWrap: derive baseline from buffer's
            // first LayoutRun (cosmic-text already places line_y with
            // line-height leading/2 distributed).
            let baseline = layout
                .buffer
                .layout_runs()
                .next()
                .map(|run| (run.line_y - run.line_top).max(0.0))
                .unwrap_or(0.0);
            vec![InlineTextFragment {
                content: self.content.clone(),
                width: layout.width,
                height: layout.height,
                baseline,
                position: None,
                layout_buffer: Some(layout.buffer),
            }]
        } else {
            self.collect_wrapped_inline_fragments(first_width.max(1.0), full_width.max(1.0))
        };
        let (max_width, max_height) = runs.iter().fold((0.0_f32, 0.0_f32), |(w, h), item| {
            (w.max(item.width), h.max(item.height))
        });
        let plan = InlineTextPlan {
            runs,
            max_width,
            max_height,
        };
        self.inline_plan_cache.insert(cache_key, plan.clone());
        plan
    }
}
