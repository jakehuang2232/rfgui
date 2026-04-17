use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::sync::Arc;

use crate::view::font_system::with_shared_font_system;
use crate::view::frame_graph::FrameGraph;
use crate::view::render_pass::TextPass;
use crate::view::render_pass::text_pass::{TextInput, TextOutput};
use crate::view::render_pass::text_pass::{TextPassFragment, TextPassParams};
use crate::view::text_layout::{build_text_buffer, measure_buffer_size};
use crate::{ColorLike, Cursor, HexColor, Style, TextAlign, TextWrap};
use cosmic_text::{Align, Buffer as GlyphBuffer, Hinting, ShapeLine, Wrap};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use super::{
    BoxModelSnapshot, BuildState, Element, ElementTrait, EventTarget, InlineMeasureContext,
    InlineNodeSize, InlinePlacement, Layoutable, Position, Renderable, Size, UiBuildContext,
};
use crate::view::promotion::PromotionNodeInfo;

#[derive(Clone, Debug, Default)]
struct InlineTextFragment {
    content: String,
    width: f32,
    height: f32,
    position: Option<Position>,
    layout_buffer: Option<Arc<GlyphBuffer>>,
}

#[derive(Clone, Debug, Default)]
struct InlineTextPlan {
    runs: Vec<InlineTextFragment>,
    max_width: f32,
    max_height: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct InlinePlanCacheKey {
    first_width_milli: i32,
    full_width_milli: i32,
    text_wrap: u8,
}

#[derive(Clone)]
struct MeasuredTextLayout {
    buffer: Arc<GlyphBuffer>,
    width: f32,
    height: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TextShapeCacheKey {
    content: String,
    font_size_milli: i32,
    line_height_milli: i32,
    font_weight: u16,
    align: u8,
    font_families: Vec<String>,
}

#[derive(Clone)]
struct GlobalShapedText {
    buffer: GlyphBuffer,
    shape_line: Option<ShapeLine>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TextLayoutCacheKey {
    width_milli: i32,
    allow_wrap: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FirstLineLayoutCacheKey {
    first_width_milli: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct WrappedSuffixCacheKey {
    suffix_start: usize,
    full_width_milli: i32,
}

#[derive(Clone, Debug)]
struct FirstLineLayoutCacheEntry {
    consumed_bytes: usize,
    fragment: InlineTextFragment,
}

#[derive(Clone)]
struct TextMeasurementSnapshot {
    signature: u64,
    measure_revision: u64,
    cached_intrinsic_layout: Option<(u64, MeasuredTextLayout)>,
    cached_height_for_width: Option<(u64, f32, f32)>,
    layout_cache: FxHashMap<TextLayoutCacheKey, MeasuredTextLayout>,
    inline_plan_cache: FxHashMap<InlinePlanCacheKey, InlineTextPlan>,
    first_line_fragment_cache: FxHashMap<FirstLineLayoutCacheKey, FirstLineLayoutCacheEntry>,
    wrapped_suffix_cache: FxHashMap<WrappedSuffixCacheKey, Vec<InlineTextFragment>>,
    layout_buffer: Option<Arc<GlyphBuffer>>,
    inline_plan: Option<InlineTextPlan>,
    last_inline_measure_context: Option<InlineMeasureContext>,
    dirty_flags: super::DirtyFlags,
    size: Size,
    render_size: Size,
    layout_size: Size,
    layout_override_width: Option<f32>,
    layout_override_height: Option<f32>,
    allow_wrap: bool,
}

pub struct Text {
    element: Element,
    position: Position,
    size: Size,
    render_size: Size,
    layout_position: Position,
    layout_size: Size,
    layout_override_width: Option<f32>,
    layout_override_height: Option<f32>,
    should_render: bool,
    content: String,
    color: Box<dyn ColorLike>,
    font_families: Vec<String>,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    align: Align,
    opacity: f32,
    auto_width: bool,
    auto_height: bool,
    text_wrap: TextWrap,
    allow_wrap: bool,
    measure_revision: u64,
    cached_intrinsic_layout: Option<(u64, MeasuredTextLayout)>,
    cached_height_for_width: Option<(u64, f32, f32)>,
    layout_cache: FxHashMap<TextLayoutCacheKey, MeasuredTextLayout>,
    inline_plan_cache: FxHashMap<InlinePlanCacheKey, InlineTextPlan>,
    first_line_fragment_cache: FxHashMap<FirstLineLayoutCacheKey, FirstLineLayoutCacheEntry>,
    wrapped_suffix_cache: FxHashMap<WrappedSuffixCacheKey, Vec<InlineTextFragment>>,
    layout_buffer: Option<Arc<GlyphBuffer>>,
    inline_plan: Option<InlineTextPlan>,
    last_inline_measure_context: Option<InlineMeasureContext>,
    dirty_flags: super::DirtyFlags,
    last_layout_placement: Option<crate::view::base_component::LayoutPlacement>,
}

/// LRU cache with generation-based eviction (à la Skia SkStrikeCache).
///
/// Each entry tracks an `access_gen` bumped on every hit.  When the cache
/// exceeds `MAX_ENTRIES`, the coldest 25 % of entries are evicted in one
/// batch — matching Skia's "at least `fTotalMemoryUsed >> 2`" policy.
struct LruCache<K: Eq + std::hash::Hash + Clone, V> {
    map: FxHashMap<K, (V, u64)>, // value + access generation
    generation: u64,
}

const LRU_CACHE_MAX_ENTRIES: usize = 4096;

impl<K: Eq + std::hash::Hash + Clone, V> LruCache<K, V> {

    fn new() -> Self {
        Self {
            map: FxHashMap::default(),
            generation: 0,
        }
    }

    fn get(&mut self, key: &K) -> Option<&V> {
        self.generation += 1;
        let current_gen = self.generation;
        self.map.get_mut(key).map(|(v, g)| {
            *g = current_gen;
            &*v
        })
    }

    fn get_cloned(&mut self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        self.get(key).cloned()
    }

    fn insert(&mut self, key: K, value: V) {
        self.generation += 1;
        self.map.insert(key, (value, self.generation));
        self.evict_if_needed();
    }

    /// Evict coldest 25 % when over capacity (Skia-style batch eviction).
    fn evict_if_needed(&mut self) {
        if self.map.len() <= LRU_CACHE_MAX_ENTRIES {
            return;
        }
        let evict_count = self.map.len() / 4; // 25 %
        let mut gens: Vec<u64> = self.map.values().map(|(_, g)| *g).collect();
        gens.sort_unstable();
        let cutoff = gens.get(evict_count).copied().unwrap_or(0);
        self.map.retain(|_, (_, g)| *g > cutoff);
    }

}

thread_local! {
    static MEASURE_TEXT_CACHE: RefCell<LruCache<TextMeasureCacheKey, MeasuredTextLayout>> =
        RefCell::new(LruCache::new());
    static SHAPED_TEXT_CACHE: RefCell<LruCache<TextShapeCacheKey, Arc<GlobalShapedText>>> =
        RefCell::new(LruCache::new());
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

fn text_measure_profile_enabled() -> bool {
    TEXT_MEASURE_PROFILE_ENABLED.load(Ordering::Relaxed)
}

fn record_text_measure_profile(update: impl FnOnce(&mut TextMeasureProfile)) {
    if !text_measure_profile_enabled() {
        return;
    }
    TEXT_MEASURE_PROFILE.with(|profile| update(&mut profile.borrow_mut()));
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TextMeasureCacheKey {
    content: String,
    max_width_milli: i32,
    font_size_milli: i32,
    line_height_milli: i32,
    font_weight: u16,
    align: u8,
    font_families: Vec<String>,
}

fn quantize_milli(value: f32) -> i32 {
    (value * 1000.0).round() as i32
}

fn make_measure_cache_key(
    content: &str,
    max_width: Option<f32>,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    align: Align,
    font_families: &[String],
) -> TextMeasureCacheKey {
    TextMeasureCacheKey {
        content: content.to_string(),
        max_width_milli: max_width.map(quantize_milli).unwrap_or(-1),
        font_size_milli: quantize_milli(font_size),
        line_height_milli: quantize_milli(line_height),
        font_weight,
        align: match align {
            Align::Left => 0,
            Align::Center => 1,
            Align::Right => 2,
            Align::Justified => 3,
            Align::End => 4,
        },
        font_families: font_families.to_vec(),
    }
}

fn make_shape_cache_key(
    content: &str,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    align: Align,
    font_families: &[String],
) -> TextShapeCacheKey {
    TextShapeCacheKey {
        content: content.to_string(),
        font_size_milli: quantize_milli(font_size),
        line_height_milli: quantize_milli(line_height),
        font_weight,
        align: match align {
            Align::Left => 0,
            Align::Center => 1,
            Align::Right => 2,
            Align::Justified => 3,
            Align::End => 4,
        },
        font_families: font_families.to_vec(),
    }
}

fn shape_text_global(
    content: &str,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    align: Align,
    font_families: &[String],
) -> (bool, Arc<GlobalShapedText>) {
    let key = make_shape_cache_key(
        content,
        font_size,
        line_height,
        font_weight,
        align,
        font_families,
    );
    SHAPED_TEXT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(cached) = cache.get_cloned(&key) {
            return (true, cached);
        }
        let shaped = with_shared_font_system(|font_system| {
            let mut buffer = build_text_buffer(
                font_system,
                content,
                None,
                None,
                false,
                font_size,
                line_height,
                font_weight,
                align,
                font_families,
            );
            let shape_line = buffer.line_shape(font_system, 0).cloned();
            Arc::new(GlobalShapedText { buffer, shape_line })
        });
        cache.insert(key, Arc::clone(&shaped));
        (false, shaped)
    })
}

impl Text {
    fn get_global_shaped_text(&self) -> Arc<GlobalShapedText> {
        let started_at = text_measure_profile_enabled().then(Instant::now);
        let (cache_hit, shaped) = shape_text_global(
            self.content.as_str(),
            self.font_size,
            self.line_height,
            self.font_weight,
            self.align,
            self.font_families.as_slice(),
        );
        if let Some(started_at) = started_at {
            let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
            record_text_measure_profile(|profile| {
                profile.ensure_shaped_base_buffer_calls += 1;
                if cache_hit {
                    profile.ensure_shaped_base_buffer_cache_hits += 1;
                }
                profile.ensure_shaped_base_buffer_ms += elapsed_ms;
            });
        }
        shaped
    }

    fn relayout_from_base(&mut self, width: Option<f32>, allow_wrap: bool) -> MeasuredTextLayout {
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

        let mut buffer = self.get_global_shaped_text().buffer.clone();
        with_shared_font_system(|font_system| {
            buffer.set_wrap(
                font_system,
                if allow_wrap {
                    Wrap::WordOrGlyph
                } else {
                    Wrap::None
                },
            );
            buffer.set_size(font_system, width.map(|value| value.max(1.0)), None);
        });
        let (measured_width, measured_height) = measure_buffer_size(&buffer);
        let measured = MeasuredTextLayout {
            buffer: Arc::new(buffer),
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

    fn first_wrapped_fragment(&mut self, first_width: f32) -> Option<FirstLineLayoutCacheEntry> {
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

        let entry = FirstLineLayoutCacheEntry {
            consumed_bytes,
            fragment: InlineTextFragment {
                content,
                width,
                height: layout_line
                    .line_height_opt
                    .unwrap_or(self.font_size * self.line_height.max(0.8))
                    .max(1.0),
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

    fn wrapped_suffix_fragments(
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

    fn collect_wrapped_inline_fragments(
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

    fn fragments_from_measured_layout(
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
                    position: None,
                    layout_buffer: Some(fragment_buffer),
                }
            })
            .collect()
    }

    fn build_inline_plan(&mut self, first_width: f32, full_width: f32) -> InlineTextPlan {
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
            vec![InlineTextFragment {
                content: self.content.clone(),
                width: layout.width,
                height: layout.height,
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

    pub fn from_content(content: impl Into<String>) -> Self {
        let mut text = Self::new(0.0, 0.0, 10_000.0, 10_000.0, content);
        text.set_auto_width(true);
        text.set_auto_height(true);
        text
    }

    pub fn from_content_with_id(id: u64, content: impl Into<String>) -> Self {
        let mut text = Self::new_with_id(id, 0.0, 0.0, 10_000.0, 10_000.0, content);
        text.set_auto_width(true);
        text.set_auto_height(true);
        text
    }

    pub fn new(x: f32, y: f32, width: f32, height: f32, content: impl Into<String>) -> Self {
        Self::new_with_id(0, x, y, width, height, content)
    }

    pub fn new_with_id(
        id: u64,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        content: impl Into<String>,
    ) -> Self {
        Self {
            element: Element::new_with_id(id, x, y, width, height),
            position: Position { x, y },
            size: Size { width, height },
            render_size: Size { width, height },
            layout_position: Position { x, y },
            layout_size: Size {
                width: width.max(0.0),
                height: height.max(0.0),
            },
            layout_override_width: None,
            layout_override_height: None,
            should_render: true,
            content: content.into(),
            color: Box::new(HexColor::new("#111111")),
            font_families: Vec::new(),
            font_size: 16.0,
            line_height: 1.25,
            font_weight: 400,
            align: Align::Left,
            opacity: 1.0,
            auto_width: false,
            auto_height: false,
            text_wrap: TextWrap::Wrap,
            allow_wrap: true,
            measure_revision: 0,
            cached_intrinsic_layout: None,
            cached_height_for_width: None,
            layout_cache: FxHashMap::default(),
            inline_plan_cache: FxHashMap::default(),
            first_line_fragment_cache: FxHashMap::default(),
            wrapped_suffix_cache: FxHashMap::default(),
            layout_buffer: None,
            inline_plan: None,
            last_inline_measure_context: None,
            dirty_flags: super::DirtyFlags::ALL,
            last_layout_placement: None,
        }
    }

    fn measurement_signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.content.hash(&mut hasher);
        self.font_families.hash(&mut hasher);
        self.font_size.to_bits().hash(&mut hasher);
        self.line_height.to_bits().hash(&mut hasher);
        self.font_weight.hash(&mut hasher);
        std::mem::discriminant(&self.align).hash(&mut hasher);
        std::mem::discriminant(&self.text_wrap).hash(&mut hasher);
        self.auto_width.hash(&mut hasher);
        self.auto_height.hash(&mut hasher);
        hasher.finish()
    }

    fn clear_layout_caches(&mut self) {
        self.cached_intrinsic_layout = None;
        self.cached_height_for_width = None;
        self.layout_cache.clear();
        self.inline_plan_cache.clear();
        self.first_line_fragment_cache.clear();
        self.wrapped_suffix_cache.clear();
        self.layout_buffer = None;
        self.inline_plan = None;
    }

    fn mark_measure_dirty(&mut self) {
        self.measure_revision = self.measure_revision.wrapping_add(1);
        self.clear_layout_caches();
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::ALL);
    }

    pub fn set_position(&mut self, x: f32, y: f32) {
        self.position = Position { x, y };
        self.element.set_position(x, y);
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::RUNTIME);
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        self.size = Size { width, height };
        self.render_size = Size {
            width: width.max(0.0),
            height: height.max(0.0),
        };
        self.element.set_size(width, height);
        self.layout_override_width = None;
        self.layout_override_height = None;
        self.auto_width = false;
        self.auto_height = false;
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::ALL);
    }

    pub fn set_width(&mut self, width: f32) {
        self.size.width = width;
        self.render_size.width = width.max(0.0);
        self.element.set_width(width);
        self.layout_override_width = None;
        self.auto_width = false;
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::ALL);
    }

    pub fn set_height(&mut self, height: f32) {
        self.size.height = height;
        self.render_size.height = height.max(0.0);
        self.element.set_height(height);
        self.layout_override_height = None;
        self.auto_height = false;
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::ALL);
    }

    pub fn set_text(&mut self, content: impl Into<String>) {
        let next = content.into();
        if self.content != next {
            self.content = next;
            self.mark_measure_dirty();
        }
    }

    pub fn set_color<T: ColorLike + 'static>(&mut self, color: T) {
        self.color = Box::new(color);
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::PAINT);
    }

    pub fn set_font(&mut self, font_family: impl Into<String>) {
        let raw = font_family.into();
        let families: Vec<String> = raw
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        if self.font_families != families {
            self.font_families = families;
            self.mark_measure_dirty();
        }
    }

    pub fn set_fonts<I, S>(&mut self, font_families: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let next: Vec<String> = font_families
            .into_iter()
            .map(Into::into)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if self.font_families != next {
            self.font_families = next;
            self.mark_measure_dirty();
        }
    }

    pub fn set_font_size(&mut self, font_size: f32) {
        if (self.font_size - font_size).abs() > f32::EPSILON {
            self.font_size = font_size;
            self.mark_measure_dirty();
        }
    }

    pub fn set_line_height(&mut self, line_height: f32) {
        if (self.line_height - line_height).abs() > f32::EPSILON {
            self.line_height = line_height;
            self.mark_measure_dirty();
        }
    }

    pub fn set_font_weight(&mut self, font_weight: u16) {
        let clamped = font_weight.clamp(100, 900);
        if self.font_weight != clamped {
            self.font_weight = clamped;
            self.mark_measure_dirty();
        }
    }

    pub fn set_align(&mut self, align: Align) {
        if std::mem::discriminant(&self.align) != std::mem::discriminant(&align) {
            self.align = align;
            self.mark_measure_dirty();
        }
    }

    pub fn set_text_align(&mut self, align: TextAlign) {
        self.set_align(match align {
            TextAlign::Left => Align::Left,
            TextAlign::Center => Align::Center,
            TextAlign::Right => Align::Right,
        });
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity;
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::PAINT);
    }

    pub fn set_text_wrap(&mut self, text_wrap: TextWrap) {
        if self.text_wrap != text_wrap {
            self.text_wrap = text_wrap;
            self.clear_layout_caches();
            self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::ALL);
        }
    }

    pub fn set_auto_width(&mut self, auto: bool) {
        if self.auto_width != auto {
            self.auto_width = auto;
            self.clear_layout_caches();
            self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::LAYOUT);
        }
    }

    pub fn set_auto_height(&mut self, auto: bool) {
        if self.auto_height != auto {
            self.auto_height = auto;
            self.clear_layout_caches();
            self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::LAYOUT);
        }
    }

    pub fn set_cursor(&mut self, cursor: Cursor) {
        let mut style = Style::new();
        style.set_cursor(cursor);
        self.element.apply_style(style);
    }

    #[cfg(test)]
    pub(crate) fn inline_fragment_positions(&self) -> Vec<(String, Position)> {
        self.inline_plan
            .as_ref()
            .map(|plan| plan.runs.as_slice())
            .unwrap_or(&[])
            .iter()
            .filter_map(|fragment| {
                fragment
                    .position
                    .map(|position| (fragment.content.clone(), position))
            })
            .collect()
    }
}

fn measure_text_layout(
    content: &str,
    max_width: Option<f32>,
    allow_wrap: bool,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    align: Align,
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
    if let Some(cached) = MEASURE_TEXT_CACHE.with(|cache| cache.borrow_mut().get_cloned(&cache_key)) {
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

    let (_, shaped) = shape_text_global(
        content,
        font_size,
        line_height,
        font_weight,
        align,
        font_families,
    );
    with_shared_font_system(|font_system| {
        let mut buffer = shaped.buffer.clone();
        buffer.set_wrap(
            font_system,
            if allow_wrap {
                Wrap::WordOrGlyph
            } else {
                Wrap::None
            },
        );
        buffer.set_size(font_system, max_width.map(|w| w.max(1.0)), None);
        buffer.shape_until_scroll(font_system, false);
        let (width, height) = measure_buffer_size(&buffer);
        let measured = MeasuredTextLayout {
            buffer: Arc::new(buffer),
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
    })
}

#[cfg(test)]
fn measure_text_size(
    content: &str,
    max_width: Option<f32>,
    allow_wrap: bool,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    align: Align,
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

impl ElementTrait for Text {
    fn id(&self) -> u64 {
        self.element.id()
    }

    fn parent_id(&self) -> Option<u64> {
        self.element.parent_id()
    }

    fn set_parent_id(&mut self, parent_id: Option<u64>) {
        self.element.set_parent_id(parent_id);
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.element.id(),
            parent_id: self.element.parent_id(),
            x: self.layout_position.x,
            y: self.layout_position.y,
            width: self.layout_size.width,
            height: self.layout_size.height,
            border_radius: 0.0,
            should_render: self.should_render,
        }
    }

    fn children(&self) -> Option<&[Box<dyn ElementTrait>]> {
        None
    }

    fn children_mut(&mut self) -> Option<&mut [Box<dyn ElementTrait>]> {
        None
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn snapshot_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(TextMeasurementSnapshot {
            signature: self.measurement_signature(),
            measure_revision: self.measure_revision,
            cached_intrinsic_layout: self.cached_intrinsic_layout.clone(),
            cached_height_for_width: self.cached_height_for_width,
            layout_cache: self.layout_cache.clone(),
            inline_plan_cache: self.inline_plan_cache.clone(),
            first_line_fragment_cache: self.first_line_fragment_cache.clone(),
            wrapped_suffix_cache: self.wrapped_suffix_cache.clone(),
            layout_buffer: self.layout_buffer.clone(),
            inline_plan: self.inline_plan.clone(),
            last_inline_measure_context: self.last_inline_measure_context,
            dirty_flags: self.dirty_flags,
            size: self.size,
            render_size: self.render_size,
            layout_size: self.layout_size,
            layout_override_width: self.layout_override_width,
            layout_override_height: self.layout_override_height,
            allow_wrap: self.allow_wrap,
        }))
    }

    fn restore_state(&mut self, snapshot: &dyn std::any::Any) -> bool {
        let Some(snapshot) = snapshot.downcast_ref::<TextMeasurementSnapshot>() else {
            return false;
        };
        if snapshot.signature != self.measurement_signature() {
            return false;
        }
        self.measure_revision = snapshot.measure_revision;
        self.cached_intrinsic_layout = snapshot.cached_intrinsic_layout.clone();
        self.cached_height_for_width = snapshot.cached_height_for_width;
        self.layout_cache = snapshot.layout_cache.clone();
        self.inline_plan_cache = snapshot.inline_plan_cache.clone();
        self.first_line_fragment_cache = snapshot.first_line_fragment_cache.clone();
        self.wrapped_suffix_cache = snapshot.wrapped_suffix_cache.clone();
        self.layout_buffer = snapshot.layout_buffer.clone();
        self.inline_plan = snapshot.inline_plan.clone();
        self.last_inline_measure_context = snapshot.last_inline_measure_context;
        self.dirty_flags = snapshot.dirty_flags;
        self.size = snapshot.size;
        self.render_size = snapshot.render_size;
        self.layout_size = snapshot.layout_size;
        self.layout_override_width = snapshot.layout_override_width;
        self.layout_override_height = snapshot.layout_override_height;
        self.allow_wrap = snapshot.allow_wrap;
        self.element.set_size(self.size.width, self.size.height);
        true
    }

    fn promotion_node_info(&self) -> PromotionNodeInfo {
        PromotionNodeInfo {
            estimated_pass_count: 1,
            opacity: self.opacity,
            ..Default::default()
        }
    }

    fn promotion_self_signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.should_render.hash(&mut hasher);
        self.layout_position.x.to_bits().hash(&mut hasher);
        self.layout_position.y.to_bits().hash(&mut hasher);
        self.content.hash(&mut hasher);
        self.color.to_rgba_u8().hash(&mut hasher);
        self.font_families.hash(&mut hasher);
        self.font_size.to_bits().hash(&mut hasher);
        self.line_height.to_bits().hash(&mut hasher);
        self.font_weight.hash(&mut hasher);
        std::mem::discriminant(&self.align).hash(&mut hasher);
        self.allow_wrap.hash(&mut hasher);
        self.layout_size.width.max(0.0).to_bits().hash(&mut hasher);
        self.layout_size.height.max(0.0).to_bits().hash(&mut hasher);
        let inline_runs = self
            .inline_plan
            .as_ref()
            .map(|plan| plan.runs.as_slice())
            .unwrap_or(&[]);
        for fragment in inline_runs {
            fragment.content.hash(&mut hasher);
            fragment.width.to_bits().hash(&mut hasher);
            fragment.height.to_bits().hash(&mut hasher);
            if let Some(position) = fragment.position {
                position.x.to_bits().hash(&mut hasher);
                position.y.to_bits().hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    fn local_dirty_flags(&self) -> super::DirtyFlags {
        self.dirty_flags
    }

    fn clear_local_dirty_flags(&mut self, flags: super::DirtyFlags) {
        self.dirty_flags = self.dirty_flags.without(flags);
    }
}

impl EventTarget for Text {
    fn dispatch_mouse_down(
        &mut self,
        event: &mut crate::ui::MouseDownEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_mouse_down(event, control);
    }

    fn dispatch_mouse_up(
        &mut self,
        event: &mut crate::ui::MouseUpEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_mouse_up(event, control);
    }

    fn dispatch_mouse_move(
        &mut self,
        event: &mut crate::ui::MouseMoveEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_mouse_move(event, control);
    }

    fn dispatch_click(
        &mut self,
        event: &mut crate::ui::ClickEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_click(event, control);
    }

    fn dispatch_key_down(
        &mut self,
        event: &mut crate::ui::KeyDownEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_key_down(event, control);
    }

    fn dispatch_key_up(
        &mut self,
        event: &mut crate::ui::KeyUpEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_key_up(event, control);
    }

    fn dispatch_focus(
        &mut self,
        event: &mut crate::ui::FocusEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_focus(event, control);
    }

    fn dispatch_blur(
        &mut self,
        event: &mut crate::ui::BlurEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_blur(event, control);
    }

    fn cursor(&self) -> crate::Cursor {
        self.element.cursor()
    }
}

impl Layoutable for Text {
    fn measured_size(&self) -> (f32, f32) {
        (self.size.width, self.size.height)
    }

    fn set_layout_width(&mut self, width: f32) {
        self.layout_override_width = Some(width.max(0.0));
    }

    fn set_layout_height(&mut self, height: f32) {
        self.layout_override_height = Some(height.max(0.0));
    }

    fn flex_props(&self) -> crate::view::base_component::FlexProps {
        let (measured_w, measured_h) = self.measured_size();
        let base = self.element.flex_props();
        crate::view::base_component::FlexProps {
            width: if self.auto_width { crate::SizeValue::Auto } else { base.width },
            height: if self.auto_height { crate::SizeValue::Auto } else { base.height },
            allows_cross_stretch_when_row: self.auto_height,
            allows_cross_stretch_when_col: self.auto_width,
            intrinsic_width: Some(measured_w),
            intrinsic_height: Some(measured_h),
            intrinsic_feeds_auto_min: true,
            intrinsic_feeds_auto_base: false,
            ..base
        }
    }

    fn inline_relative_position(&self) -> (f32, f32) {
        (self.position.x, self.position.y)
    }

    fn set_layout_offset(&mut self, x: f32, y: f32) {
        self.position = Position { x, y };
        self.element.set_position(x, y);
        self.dirty_flags = self.dirty_flags.union(super::DirtyFlags::RUNTIME);
    }

    fn measure_inline(&mut self, context: InlineMeasureContext) {
        if !self.dirty_flags.intersects(super::DirtyFlags::LAYOUT)
            && self.last_inline_measure_context == Some(context)
        {
            return;
        }
        let started_at = text_measure_profile_enabled().then(Instant::now);
        self.inline_plan = None;
        self.last_inline_measure_context = Some(context);

        if self.content.is_empty() {
            self.size = Size {
                width: 0.0,
                height: 0.0,
            };
            self.render_size = self.size;
            self.element.set_size(0.0, 0.0);
            self.dirty_flags = self.dirty_flags.without(super::DirtyFlags::LAYOUT).union(
                super::DirtyFlags::PLACE
                    .union(super::DirtyFlags::BOX_MODEL)
                    .union(super::DirtyFlags::HIT_TEST)
                    .union(super::DirtyFlags::PAINT),
            );
            if let Some(started_at) = started_at {
                let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
                record_text_measure_profile(|profile| {
                    profile.measure_inline_calls += 1;
                    profile.measure_inline_ms += elapsed_ms;
                });
            }
            return;
        }

        let plan = self.build_inline_plan(
            context.first_available_width.max(1.0),
            context.full_available_width.max(1.0),
        );
        self.inline_plan = Some(plan.clone());
        self.size = Size {
            width: plan.max_width.max(0.0),
            height: plan.max_height.max(0.0),
        };
        self.render_size = Size {
            width: plan.max_width.max(0.0),
            height: plan.max_height.max(0.0),
        };
        self.layout_buffer = None;
        self.element.set_size(self.size.width, self.size.height);
        self.dirty_flags = self.dirty_flags.without(super::DirtyFlags::LAYOUT).union(
            super::DirtyFlags::PLACE
                .union(super::DirtyFlags::BOX_MODEL)
                .union(super::DirtyFlags::HIT_TEST)
                .union(super::DirtyFlags::PAINT),
        );
        if let Some(started_at) = started_at {
            let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
            record_text_measure_profile(|profile| {
                profile.measure_inline_calls += 1;
                profile.measure_inline_ms += elapsed_ms;
            });
        }
    }

    fn get_inline_nodes_size(&self) -> Vec<InlineNodeSize> {
        self.inline_plan
            .as_ref()
            .map(|plan| plan.runs.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|fragment| InlineNodeSize {
                width: fragment.width,
                height: fragment.height,
            })
            .collect()
    }

    fn place_inline(&mut self, placement: InlinePlacement) {
        let Some(plan) = self.inline_plan.as_mut() else {
            return;
        };
        if placement.node_index == 0 {
            for fragment in &mut plan.runs {
                fragment.position = None;
            }
            self.layout_position = Position {
                x: placement.x,
                y: placement.y,
            };
            self.layout_size = Size {
                width: 0.0,
                height: 0.0,
            };
            self.should_render = false;
        }

        let Some(fragment) = plan.runs.get_mut(placement.node_index) else {
            return;
        };
        fragment.position = Some(Position {
            x: placement.x,
            y: placement.y,
        });

        let left = placement.x;
        let top = placement.y;
        let right = placement.x + fragment.width.max(0.0);
        let bottom = placement.y + fragment.height.max(0.0);
        if self.should_render {
            let current_right = self.layout_position.x + self.layout_size.width;
            let current_bottom = self.layout_position.y + self.layout_size.height;
            self.layout_position.x = self.layout_position.x.min(left);
            self.layout_position.y = self.layout_position.y.min(top);
            self.layout_size.width = current_right.max(right) - self.layout_position.x;
            self.layout_size.height = current_bottom.max(bottom) - self.layout_position.y;
        } else {
            self.layout_position = Position { x: left, y: top };
            self.layout_size = Size {
                width: (right - left).max(0.0),
                height: (bottom - top).max(0.0),
            };
        }
        self.should_render = self.layout_size.width > 0.0 && self.layout_size.height > 0.0;
        self.dirty_flags = self.dirty_flags.without(
            super::DirtyFlags::PLACE
                .union(super::DirtyFlags::BOX_MODEL)
                .union(super::DirtyFlags::HIT_TEST),
        );
    }

    fn measure(&mut self, constraints: crate::view::base_component::LayoutConstraints) {
        self.inline_plan = None;
        self.last_inline_measure_context = None;
        self.layout_override_width = None;
        self.layout_override_height = None;
        let parent_width_is_constrained = constraints.percent_base_width.is_some();
        let next_allow_wrap = self.text_wrap == TextWrap::Wrap && parent_width_is_constrained;
        if self.allow_wrap != next_allow_wrap {
            self.allow_wrap = next_allow_wrap;
        }
        self.layout_buffer = None;

        if !self.auto_width && !self.auto_height {
            self.layout_buffer = Some(
                self.relayout_from_base(Some(self.size.width.max(1.0)), self.allow_wrap)
                    .buffer,
            );
            self.dirty_flags = self.dirty_flags.without(super::DirtyFlags::LAYOUT);
            return;
        }
        let mut intrinsic_layout: Option<MeasuredTextLayout> = None;
        if self.auto_width {
            let next_intrinsic_layout = match self.cached_intrinsic_layout.as_ref() {
                Some((revision, layout)) if *revision == self.measure_revision => layout.clone(),
                _ => {
                    let layout = self.relayout_from_base(None, false);
                    self.cached_intrinsic_layout = Some((self.measure_revision, layout.clone()));
                    layout
                }
            };
            let intrinsic_width = next_intrinsic_layout.width;
            intrinsic_layout = Some(next_intrinsic_layout);
            let available = if parent_width_is_constrained {
                constraints.max_width.max(1.0)
            } else {
                f32::INFINITY
            };
            self.size.width = intrinsic_width.min(available).max(0.0);
            self.render_size.width = intrinsic_width.min(available).max(0.0);
            self.element.set_width(self.size.width);
        }
        if self.auto_height {
            let effective_width = if self.auto_width {
                self.render_size.width.max(1.0)
            } else {
                self.size.width.min(constraints.max_width.max(1.0)).max(1.0)
            };
            if let Some(layout) = intrinsic_layout.as_ref()
                && !self.allow_wrap
                && (effective_width - layout.width.max(1.0)).abs() <= 0.01
            {
                self.cached_height_for_width =
                    Some((self.measure_revision, effective_width, layout.height));
                self.size.height = layout.height.max(1.0);
                self.render_size.height = layout.height.max(1.0);
                self.element.set_height(self.size.height);
                self.layout_buffer = Some(layout.buffer.clone());
            } else {
                let buffer = self
                    .relayout_from_base(Some(effective_width), self.allow_wrap)
                    .buffer;
                let (_, measured_height) = measure_buffer_size(&buffer);
                self.cached_height_for_width =
                    Some((self.measure_revision, effective_width, measured_height));
                self.size.height = measured_height.max(1.0);
                self.render_size.height = measured_height.max(1.0);
                self.element.set_height(self.size.height);
                self.layout_buffer = Some(buffer);
            }
        } else {
            let final_width = if self.auto_width {
                self.render_size.width.max(1.0)
            } else {
                self.size.width.max(1.0)
            };
            if let Some(layout) = intrinsic_layout
                && !self.allow_wrap
                && (final_width - layout.width.max(1.0)).abs() <= 0.01
            {
                self.layout_buffer = Some(layout.buffer);
            } else {
                self.layout_buffer = Some(
                    self.relayout_from_base(Some(final_width), self.allow_wrap)
                        .buffer,
                );
            }
        }
        self.dirty_flags = self.dirty_flags.without(super::DirtyFlags::LAYOUT);
    }

    fn place(&mut self, placement: crate::view::base_component::LayoutPlacement) {
        if !self.dirty_flags.intersects(
            super::DirtyFlags::PLACE
                .union(super::DirtyFlags::BOX_MODEL)
                .union(super::DirtyFlags::HIT_TEST),
        ) && self.last_layout_placement == Some(placement)
        {
            return;
        }
        let available_width = placement.available_width.max(0.0);
        let available_height = placement.available_height.max(0.0);
        let max_width = (available_width - self.position.x.max(0.0)).max(0.0);
        let max_height = (available_height - self.position.y.max(0.0)).max(0.0);
        let layout_width = self.layout_override_width.unwrap_or(self.size.width);
        let layout_height = self.layout_override_height.unwrap_or(self.size.height);
        self.layout_size = Size {
            width: layout_width.max(0.0).min(max_width),
            height: layout_height.max(0.0).min(max_height),
        };
        self.layout_position = Position {
            x: placement.parent_x + self.position.x + placement.visual_offset_x,
            y: placement.parent_y + self.position.y + placement.visual_offset_y,
        };

        let parent_left = placement.parent_x + placement.visual_offset_x;
        let parent_top = placement.parent_y + placement.visual_offset_y;
        let parent_right = parent_left + available_width;
        let parent_bottom = parent_top + available_height;
        let self_left = self.layout_position.x;
        let self_top = self.layout_position.y;
        let self_right = self.layout_position.x + self.layout_size.width;
        let self_bottom = self.layout_position.y + self.layout_size.height;
        self.should_render = self.layout_size.width > 0.0
            && self.layout_size.height > 0.0
            && self_right > parent_left
            && self_left < parent_right
            && self_bottom > parent_top
            && self_top < parent_bottom;
        self.last_layout_placement = Some(placement);
        self.dirty_flags = self.dirty_flags.without(
            super::DirtyFlags::PLACE
                .union(super::DirtyFlags::BOX_MODEL)
                .union(super::DirtyFlags::HIT_TEST),
        );
    }
}

impl Renderable for Text {
    fn build(&mut self, graph: &mut FrameGraph, mut ctx: UiBuildContext) -> BuildState {
        if !self.should_render || self.content.is_empty() {
            return ctx.into_state();
        }

        let opacity = if ctx.is_node_promoted(self.id()) {
            1.0
        } else {
            self.opacity.clamp(0.0, 1.0)
        };
        if opacity <= 0.0 {
            return ctx.into_state();
        }

        let Some(input_target) = ctx.current_target() else {
            return ctx.into_state();
        };
        let inline_runs = self
            .inline_plan
            .as_ref()
            .map(|plan| plan.runs.as_slice())
            .unwrap_or(&[]);
        let inline_fragment_indices = inline_runs
            .iter()
            .enumerate()
            .filter(|(_, fragment)| fragment.position.is_some() && !fragment.content.is_empty())
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let has_inline_fragments = !inline_fragment_indices.is_empty();
        let fragments = if !has_inline_fragments {
            let layout_buffer = self
                .layout_buffer
                .as_ref()
                .expect("text layout buffer should be prepared during layout")
                .clone();
            vec![TextPassFragment {
                content: self.content.clone(),
                x: self.layout_position.x,
                y: self.layout_position.y,
                width: self.render_size.width.max(self.layout_size.width),
                height: self.render_size.height.max(self.layout_size.height),
                color: self.color.to_rgba_f32(),
                opacity,
                layout_buffer: Some(layout_buffer),
            }]
        } else {
            inline_fragment_indices
                .into_iter()
                .filter_map(|index| {
                    let fragment = inline_runs.get(index)?;
                    let position = fragment.position?;
                    let content = fragment.content.clone();
                    let width = fragment.width;
                    let height = fragment.height;
                    let layout_buffer = fragment.layout_buffer.clone()?;
                    Some(TextPassFragment {
                        content,
                        x: position.x,
                        y: position.y,
                        width: width.max(1.0),
                        height: height.max(1.0),
                        color: self.color.to_rgba_f32(),
                        opacity,
                        layout_buffer: Some(layout_buffer),
                    })
                })
                .collect::<Vec<_>>()
        };
        let pass = TextPass::new(
            TextPassParams {
                fragments,
                font_size: self.font_size,
                line_height: self.line_height,
                font_weight: self.font_weight,
                font_families: self.font_families.clone(),
                align: self.align,
                allow_wrap: !has_inline_fragments && self.allow_wrap,
                scissor_rect: None,
                stencil_clip_id: None,
            },
            TextInput {
                pass_context: ctx.graphics_pass_context(),
            },
            TextOutput {
                render_target: input_target,
                ..Default::default()
            },
        );
        graph.add_graphics_pass(pass);
        ctx.set_current_target(input_target);
        ctx.into_state()
    }
}

#[cfg(test)]
mod tests {
    use super::{ElementTrait, Layoutable, Text, measure_text_size};
    use crate::view::base_component::{
        DirtyFlags, InlineMeasureContext, LayoutConstraints, LayoutPlacement,
    };
    use crate::{Length, TextWrap};
    use cosmic_text::Align;

    #[test]
    fn layout_clamps_to_parent_available_area() {
        let mut text = Text::new(0.0, 0.0, 10_000.0, 10_000.0, "demo");
        text.set_position(8.0, 4.0);
        text.measure(LayoutConstraints {
            max_width: 240.0,
            max_height: 140.0,
            viewport_width: 240.0,
            percent_base_width: Some(240.0),
            percent_base_height: Some(140.0),
            viewport_height: 140.0,
        });
        text.place(LayoutPlacement {
            parent_x: 40.0,
            parent_y: 40.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 240.0,
            available_height: 140.0,
            viewport_width: 240.0,
            percent_base_width: Some(240.0),
            percent_base_height: Some(140.0),
            viewport_height: 140.0,
        });

        let snapshot = text.box_model_snapshot();
        assert_eq!(snapshot.x, 48.0);
        assert_eq!(snapshot.y, 44.0);
        assert_eq!(snapshot.width, 232.0);
        assert_eq!(snapshot.height, 136.0);
    }

    #[test]
    fn text_wraps_when_parent_width_is_constrained() {
        let mut text = Text::from_content("123456789012345678901234567890");
        text.set_width(60.0);
        text.set_auto_height(true);
        text.measure(LayoutConstraints {
            max_width: 60.0,
            max_height: 200.0,
            viewport_width: 60.0,
            percent_base_width: Some(60.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        });
        text.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 60.0,
            available_height: 200.0,
            viewport_width: 60.0,
            percent_base_width: Some(60.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        });

        let snapshot = text.box_model_snapshot();
        assert_eq!(snapshot.width, 60.0);
        assert!(snapshot.height > 20.0);
    }

    #[test]
    fn text_wrap_can_be_disabled_via_text_wrap_style() {
        let mut text = Text::from_content("123456789012345678901234567890");
        text.set_width(60.0);
        text.set_auto_height(true);
        text.set_text_wrap(TextWrap::NoWrap);
        text.measure(LayoutConstraints {
            max_width: 60.0,
            max_height: 200.0,
            viewport_width: 60.0,
            percent_base_width: Some(60.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        });
        text.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 60.0,
            available_height: 200.0,
            viewport_width: 60.0,
            percent_base_width: Some(60.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        });

        let snapshot = text.box_model_snapshot();
        assert_eq!(snapshot.width, 60.0);
        assert!(snapshot.height <= 20.0);
    }

    #[test]
    fn percent_width_uses_layout_override_without_mutating_measured_width() {
        let mut text = Text::from_content("123");
        text.element.set_width(10.0);
        text.set_width(10.0);
        text.element.apply_style({
            let mut style = crate::Style::new();
            style.insert(
                crate::style::PropertyId::Width,
                crate::style::ParsedValue::Length(Length::percent(100.0)),
            );
            style
        });

        text.measure(LayoutConstraints {
            max_width: 200.0,
            max_height: 40.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(40.0),
            viewport_height: 40.0,
        });
        assert_eq!(text.measured_size().0, 10.0);

        text.set_layout_width(80.0);
        text.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 40.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(40.0),
            viewport_height: 40.0,
        });
        assert_eq!(text.box_model_snapshot().width, 80.0);

        text.measure(LayoutConstraints {
            max_width: 200.0,
            max_height: 40.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(40.0),
            viewport_height: 40.0,
        });
        assert_eq!(text.measured_size().0, 10.0);
    }

    #[test]
    fn auto_width_for_cjk_text_is_not_underestimated() {
        let mut text = Text::from_content("This is a Chinese text segment");
        text.measure(LayoutConstraints {
            max_width: 300.0,
            max_height: 200.0,
            viewport_width: 300.0,
            percent_base_width: Some(300.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        });
        text.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 300.0,
            available_height: 200.0,
            viewport_width: 300.0,
            percent_base_width: Some(300.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        });
        let snapshot = text.box_model_snapshot();
        assert!(snapshot.width >= 80.0);
    }

    #[test]
    fn auto_width_with_space_includes_following_word() {
        let mut single = Text::from_content("Click");
        single.measure(LayoutConstraints {
            max_width: 400.0,
            max_height: 200.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        });
        single.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 200.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        });

        let mut spaced = Text::from_content("Click Me");
        spaced.measure(LayoutConstraints {
            max_width: 400.0,
            max_height: 200.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        });
        spaced.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 200.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        });

        let a = single.box_model_snapshot().width;
        let b = spaced.box_model_snapshot().width;
        assert!(
            b > a,
            "expected \"Click Me\" width > \"Click\", got {b} <= {a}"
        );
    }

    #[test]
    fn text_does_not_wrap_when_parent_width_is_unresolved() {
        let mut text = Text::from_content("Click Me Click Me");
        text.set_auto_width(true);
        text.set_auto_height(true);
        text.measure(LayoutConstraints {
            max_width: 60.0,
            max_height: 200.0,
            viewport_width: 60.0,
            percent_base_width: None,
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        });
        text.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 200.0,
            viewport_width: 400.0,
            percent_base_width: None,
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        });

        let snapshot = text.box_model_snapshot();
        assert!(snapshot.width > 60.0);
        assert!(snapshot.height <= 24.0);
    }

    #[test]
    fn text_reflows_when_parent_width_changes() {
        let mut text =
            Text::from_content("This is a long sentence that should wrap to multiple lines.");
        text.set_auto_width(true);
        text.set_auto_height(true);

        text.measure(LayoutConstraints {
            max_width: 220.0,
            max_height: 300.0,
            viewport_width: 220.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        text.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 220.0,
            available_height: 300.0,
            viewport_width: 220.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        let h_wide = text.box_model_snapshot().height;

        text.measure(LayoutConstraints {
            max_width: 90.0,
            max_height: 300.0,
            viewport_width: 90.0,
            percent_base_width: Some(90.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        text.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 90.0,
            available_height: 300.0,
            viewport_width: 90.0,
            percent_base_width: Some(90.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        let h_narrow = text.box_model_snapshot().height;

        assert!(
            h_narrow > h_wide,
            "expected text to reflow when parent width shrinks: narrow={h_narrow}, wide={h_wide}"
        );
    }

    #[test]
    fn inline_measure_clears_layout_dirty() {
        let mut text = Text::from_content("inline text");
        text.measure_inline(InlineMeasureContext {
            first_available_width: 200.0,
            full_available_width: 200.0,
            viewport_width: 200.0,
            viewport_height: 120.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(120.0),
        });

        assert!(!text.local_dirty_flags().intersects(DirtyFlags::LAYOUT));
    }

    #[test]
    fn auto_measured_text_size_preserves_fractional_precision() {
        let mut text = Text::from_content("rounded measurement");
        text.measure(LayoutConstraints {
            max_width: 300.0,
            max_height: 200.0,
            viewport_width: 300.0,
            percent_base_width: Some(300.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        });

        let (width, height) = text.measured_size();
        assert!(width.fract() > 0.0 || height.fract() > 0.0);
    }

    #[test]
    fn auto_width_uses_precise_text_width_before_final_pixel_rounding() {
        let content = "Option 4";
        let (precise_width, precise_height) =
            measure_text_size(content, None, false, 16.0, 1.25, 400, Align::Left, &[]);
        assert!(precise_width.fract() > 0.0);

        let mut text = Text::from_content(content);
        text.measure(LayoutConstraints {
            max_width: precise_width.ceil(),
            max_height: 200.0,
            viewport_width: precise_width.ceil(),
            percent_base_width: Some(precise_width.ceil()),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        });

        let (measured_width, measured_height) = text.measured_size();
        assert!(
            (measured_width - precise_width).abs() < 0.01,
            "expected precise width {precise_width}, got {measured_width}"
        );
        assert!(
            (measured_height - precise_height).abs() < 0.01,
            "expected single-line height {precise_height}, got {measured_height}"
        );
    }

    #[test]
    fn inline_measure_does_not_split_word_when_available_width_matches_precise_measurement() {
        let content = "Reset";
        let (precise_width, _) =
            measure_text_size(content, None, false, 16.0, 1.25, 400, Align::Left, &[]);
        let mut text = Text::from_content(content);
        text.measure_inline(InlineMeasureContext {
            first_available_width: precise_width,
            full_available_width: precise_width,
            viewport_width: 400.0,
            viewport_height: 200.0,
            percent_base_width: Some(precise_width),
            percent_base_height: Some(200.0),
        });

        let nodes = text.get_inline_nodes_size();
        assert_eq!(
            nodes.len(),
            1,
            "word should stay on one line when precise width fits"
        );
    }

    #[test]
    fn inline_wrap_uses_one_fragment_per_wrapped_line() {
        let content = "alpha beta gamma delta";
        let available_width = 64.0;
        let mut text = Text::from_content(content);
        text.measure_inline(InlineMeasureContext {
            first_available_width: available_width,
            full_available_width: available_width,
            viewport_width: 400.0,
            viewport_height: 200.0,
            percent_base_width: Some(available_width),
            percent_base_height: Some(200.0),
        });

        let nodes = text.get_inline_nodes_size();
        assert!(
            nodes.len() > 1,
            "expected wrapped text to produce multiple line fragments"
        );
        assert!(
            nodes
                .iter()
                .all(|node| node.width <= available_width + 0.01)
        );
    }

    #[test]
    fn inline_wrap_uses_first_available_width_for_first_fragment() {
        let content = "alpha beta gamma";
        let mut text = Text::from_content(content);
        text.measure_inline(InlineMeasureContext {
            first_available_width: 48.0,
            full_available_width: 160.0,
            viewport_width: 200.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        });

        let nodes = text.get_inline_nodes_size();
        assert!(
            nodes.len() >= 2,
            "expected first-line constraint to force wrapping"
        );
        assert!(nodes[0].width <= 48.01);
    }

    #[test]
    fn auto_height_uses_precise_auto_width_to_avoid_spurious_wrap_height() {
        let content = "Start";
        let (precise_width, precise_height) =
            measure_text_size(content, None, false, 16.0, 1.25, 400, Align::Left, &[]);
        let mut text = Text::from_content(content);
        text.measure(LayoutConstraints {
            max_width: precise_width.ceil(),
            max_height: 200.0,
            viewport_width: 400.0,
            percent_base_width: Some(precise_width.ceil()),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        });

        assert!((text.measured_size().1 - precise_height).abs() < 0.01);
    }

    #[test]
    fn placed_text_box_preserves_fractional_layout_coordinates() {
        let mut text = Text::new(1.4, 2.6, 10.4, 20.6, "demo");
        text.place(LayoutPlacement {
            parent_x: 3.2,
            parent_y: 4.7,
            visual_offset_x: 0.3,
            visual_offset_y: -0.2,
            available_width: 100.0,
            available_height: 100.0,
            viewport_width: 100.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(100.0),
            viewport_height: 100.0,
        });

        let snapshot = text.box_model_snapshot();
        assert!((snapshot.x - 4.9).abs() < 0.01);
        assert!((snapshot.y - 7.1).abs() < 0.01);
        assert!((snapshot.width - 10.4).abs() < 0.01);
        assert!((snapshot.height - 20.6).abs() < 0.01);
    }
}
