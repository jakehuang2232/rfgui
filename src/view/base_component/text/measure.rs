//! Text measure pipeline: shape / relayout / cache helpers and the
//! `measure_text_size` convenience used by tests.

use std::sync::Arc;

use super::Text;
use super::cache::{
    MEASURE_TEXT_CACHE, MeasuredTextIfc, TextLayoutCacheKey, make_measure_cache_lookup,
    quantize_milli,
};
use super::profile::{record_text_measure_profile, text_measure_profile_enabled};
use crate::time::Instant;
use crate::view::base_component::DirtyFlags;
use crate::view::inline_formatting_context::{
    InlineFormattingContext, InlineIfcAlignment, InlineIfcInput, InlineIfcItem,
    InlineIfcLayoutOptions, InlineIfcSourceId, InlineIfcStyle,
};

/// Source id for the single text span a standalone Text node shapes into
/// its own inline formatting context.
pub(super) const TEXT_SELF_SOURCE: InlineIfcSourceId = InlineIfcSourceId(1);

/// Shape one text run into a standalone inline formatting context.
///
/// The brush is constant: glyph color is overridden at bridge time so
/// color changes never reshape (and never miss the measure caches).
fn shape_text_context(
    content: &str,
    max_width: Option<f32>,
    allow_wrap: bool,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    align: InlineIfcAlignment,
    font_families: &[String],
) -> InlineFormattingContext {
    // Preserve the legacy empty-content behavior: shape a single space so
    // an empty Text still measures one line high.
    let shaped_content = if content.is_empty() { " " } else { content };
    let input = InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
        source: TEXT_SELF_SOURCE,
        text: shaped_content.to_string(),
        style: Some(InlineIfcStyle {
            font_size,
            line_height,
            font_weight,
            brush: [0, 0, 0, 255],
            font_families: Arc::from(font_families),
            vertical_align: crate::style::VerticalAlign::Baseline,
        }),
    }]);
    let options = InlineIfcLayoutOptions::new(max_width, allow_wrap).with_align(align);
    InlineFormattingContext::build_with_options(input, options)
}

impl Text {
    pub(super) fn relayout_from_base(
        &mut self,
        width: Option<f32>,
        allow_wrap: bool,
    ) -> MeasuredTextIfc {
        let started_at = text_measure_profile_enabled().then(Instant::now);
        let cache_key = TextLayoutCacheKey {
            width_milli: width.map(quantize_milli).unwrap_or(-1),
            allow_wrap,
        };
        if let Some(cached) = self.layout_cache.get_cloned(&cache_key) {
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

        let measured = measure_text_layout(
            self.content.as_str(),
            width,
            allow_wrap,
            self.font_size,
            self.line_height,
            self.font_weight,
            self.align,
            self.font_families.as_slice(),
        );
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
        self.layout_cache.clear();
        self.shaped_context = None;
    }

    pub(super) fn mark_measure_dirty(&mut self) {
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
    align: InlineIfcAlignment,
    font_families: &[String],
) -> MeasuredTextIfc {
    let started_at = text_measure_profile_enabled().then(Instant::now);
    // Alignment needs a width constraint to have any effect (parley aligns
    // within the break width). Normalize so align changes never invalidate
    // unconstrained (intrinsic) shapings.
    let align = if max_width.is_none() || !allow_wrap {
        InlineIfcAlignment::Left
    } else {
        align
    };
    let cache_lookup = make_measure_cache_lookup(
        content,
        max_width,
        allow_wrap,
        font_size,
        line_height,
        font_weight,
        align,
        font_families,
    );
    if let Some(cached) =
        MEASURE_TEXT_CACHE.with(|cache| cache.borrow_mut().get_cloned(&cache_lookup))
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

    let context = Arc::new(shape_text_context(
        content,
        max_width,
        allow_wrap,
        font_size,
        line_height,
        font_weight,
        align,
        font_families,
    ));
    let (width, height) = context.measure_content_size();
    let measured = MeasuredTextIfc {
        context,
        width,
        height,
    };
    let cache_key = cache_lookup.to_owned_key();
    let estimated_bytes = cache_lookup.estimated_entry_bytes();
    MEASURE_TEXT_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .insert(cache_key, measured.clone(), estimated_bytes);
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
    align: InlineIfcAlignment,
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
