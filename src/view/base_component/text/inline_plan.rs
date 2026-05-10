//! Inline-fragment plan builder.

use std::sync::Arc;

use super::Text;
use super::cache::{InlinePlanCacheKey, quantize_milli};
use super::measure::measure_text_layout;
use super::profile::{record_text_measure_profile, text_measure_profile_enabled};
use crate::style::TextWrap;
use crate::time::Instant;
use crate::view::text_layout::{
    TextLayout, TextLayoutLineFragment, build_text_layout_with_line_widths,
};

#[derive(Clone, Default)]
pub(in crate::view::base_component) struct InlineTextFragment {
    pub(super) content: String,
    pub(super) width: f32,
    pub(super) height: f32,
    /// Distance from fragment top to typography baseline. See
    /// `docs/design/inline-baseline.md` D1/D4.
    pub(super) baseline: f32,
    pub(super) position: Option<super::super::Position>,
    pub(super) text_layout: Option<Arc<TextLayout>>,
}

#[derive(Clone, Default)]
pub(in crate::view::base_component) struct InlineTextPlan {
    pub(super) runs: Vec<InlineTextFragment>,
    pub(super) max_width: f32,
    pub(super) max_height: f32,
}

impl Text {
    pub(super) fn collect_wrapped_inline_fragments(
        &mut self,
        first_width: f32,
        full_width: f32,
    ) -> Vec<InlineTextFragment> {
        let started_at = text_measure_profile_enabled().then(Instant::now);
        let first_width = first_width.max(1.0);
        let full_width = full_width.max(1.0);
        let effective_first_width =
            self.first_line_width_for_word_boundary(first_width, full_width);
        let (layout_first_width, layout_full_width) =
            self.parley_inline_wrap_widths(effective_first_width, full_width);
        let layout = build_text_layout_with_line_widths(
            self.content.as_str(),
            layout_first_width,
            layout_full_width,
            self.font_size,
            self.line_height,
            self.font_weight,
            self.align,
            self.font_families.as_slice(),
        );
        let fragments = self.fragments_from_text_layout_lines(
            layout.layout.inline_line_fragments(self.content.as_str()),
            effective_first_width,
            full_width,
        );

        if let Some(started_at) = started_at {
            let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
            record_text_measure_profile(|profile| {
                profile.collect_wrapped_inline_fragments_calls += 1;
                profile.collect_wrapped_inline_fragments_ms += elapsed_ms;
            });
        }
        fragments
    }

    fn first_line_width_for_word_boundary(&mut self, first_width: f32, full_width: f32) -> f32 {
        if first_width >= full_width - 0.01 || self.content.starts_with(char::is_whitespace) {
            return first_width;
        }
        let Some(first_token) = self.content.split_whitespace().next() else {
            return first_width;
        };
        let measured = measure_text_layout(
            first_token,
            None,
            false,
            self.font_size,
            self.line_height,
            self.font_weight,
            self.align,
            self.font_families.as_slice(),
        );
        if measured.width > first_width + 0.01 && measured.width <= full_width + 0.01 {
            full_width
        } else {
            first_width
        }
    }

    fn parley_inline_wrap_widths(&self, first_width: f32, full_width: f32) -> (f32, f32) {
        let unwrapped = measure_text_layout(
            self.content.as_str(),
            None,
            false,
            self.font_size,
            self.line_height,
            self.font_weight,
            self.align,
            self.font_families.as_slice(),
        );
        let legacy_width = unwrapped.width.max(1.0);
        let parley_width = unwrapped
            .text_layout
            .inline_line_fragments(self.content.as_str())
            .first()
            .map(|line| line.width.max(1.0))
            .unwrap_or(legacy_width);
        let scale = (parley_width / legacy_width).clamp(1.0, 1.25);
        (first_width * scale, full_width * scale)
    }

    fn fragments_from_text_layout_lines(
        &self,
        lines: Vec<TextLayoutLineFragment>,
        first_width: f32,
        full_width: f32,
    ) -> Vec<InlineTextFragment> {
        lines
            .into_iter()
            .enumerate()
            .map(|(idx, line)| {
                let max_width = if idx == 0 { first_width } else { full_width };
                let fragment_layout = measure_text_layout(
                    line.content.as_str(),
                    Some(line.width.min(max_width.max(1.0))),
                    false,
                    self.font_size,
                    self.line_height,
                    self.font_weight,
                    self.align,
                    self.font_families.as_slice(),
                );
                let width = fragment_layout.width.min(max_width.max(1.0));
                InlineTextFragment {
                    content: line.content,
                    width,
                    height: line.height,
                    baseline: line.baseline,
                    position: None,
                    text_layout: Some(fragment_layout.text_layout),
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
            let baseline = layout.first_inline_baseline();
            vec![InlineTextFragment {
                content: self.content.clone(),
                width: layout.width,
                height: layout.height,
                baseline,
                position: None,
                text_layout: Some(layout.text_layout),
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
