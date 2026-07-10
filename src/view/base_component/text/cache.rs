//! Text cache types: LRU + measure / shape cache keys.

use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::sync::Arc;

use crate::view::inline_formatting_context::{InlineFormattingContext, InlineIfcAlignment};

const TEXT_LAYOUT_CACHE_MAX_ENTRIES: usize = 4;

#[derive(Clone)]
pub(in crate::view::base_component) struct MeasuredTextIfc {
    pub(in crate::view::base_component) context: Arc<InlineFormattingContext>,
    pub(in crate::view::base_component) width: f32,
    pub(in crate::view::base_component) height: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::view::base_component) struct TextLayoutCacheKey {
    pub(super) width_milli: i32,
    pub(super) allow_wrap: bool,
}

/// Tiny per-node MRU. A Text only benefits from its intrinsic layout and a
/// handful of recent constrained widths; retaining every animated/resized
/// width pins the corresponding shaped contexts indefinitely.
#[derive(Default)]
pub(in crate::view::base_component) struct TextLayoutCache {
    entries: Vec<(TextLayoutCacheKey, MeasuredTextIfc)>,
}

impl TextLayoutCache {
    pub(super) fn get_cloned(&mut self, key: &TextLayoutCacheKey) -> Option<MeasuredTextIfc> {
        let index = self
            .entries
            .iter()
            .position(|(candidate, _)| candidate == key)?;
        let entry = self.entries.remove(index);
        let value = entry.1.clone();
        self.entries.push(entry);
        Some(value)
    }

    pub(super) fn insert(&mut self, key: TextLayoutCacheKey, value: MeasuredTextIfc) {
        if let Some(index) = self
            .entries
            .iter()
            .position(|(candidate, _)| candidate == &key)
        {
            self.entries.remove(index);
        } else if self.entries.len() == TEXT_LAYOUT_CACHE_MAX_ENTRIES {
            self.entries.remove(0);
        }
        self.entries.push((key, value));
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}

struct MeasureTextCacheEntry {
    key: TextMeasureCacheKey,
    value: MeasuredTextIfc,
    access_generation: u64,
    estimated_bytes: usize,
}

/// Shared shaped-text cache, bucketed by a borrowed-key fingerprint. Cache
/// hits compare the owned key for collision safety without allocating a
/// temporary String/Vec, while eviction observes both entry and byte budgets.
pub(super) struct MeasureTextCache {
    buckets: FxHashMap<u64, Vec<MeasureTextCacheEntry>>,
    generation: u64,
    len: usize,
    estimated_bytes: usize,
}

const MEASURE_CACHE_MAX_ENTRIES: usize = 4096;
const MEASURE_CACHE_MAX_ESTIMATED_BYTES: usize = 64 * 1024 * 1024;

impl MeasureTextCache {
    pub(super) fn new() -> Self {
        Self {
            buckets: FxHashMap::default(),
            generation: 0,
            len: 0,
            estimated_bytes: 0,
        }
    }

    pub(super) fn get_cloned(
        &mut self,
        lookup: &TextMeasureCacheLookup<'_>,
    ) -> Option<MeasuredTextIfc> {
        let bucket = self.buckets.get_mut(&lookup.fingerprint())?;
        let entry = bucket.iter_mut().find(|entry| entry.key.matches(lookup))?;
        self.generation = self.generation.wrapping_add(1);
        entry.access_generation = self.generation;
        Some(entry.value.clone())
    }

    pub(super) fn insert(
        &mut self,
        key: TextMeasureCacheKey,
        value: MeasuredTextIfc,
        estimated_bytes: usize,
    ) {
        self.generation = self.generation.wrapping_add(1);
        let fingerprint = key.fingerprint();
        let bucket = self.buckets.entry(fingerprint).or_default();
        if let Some(entry) = bucket.iter_mut().find(|entry| entry.key == key) {
            self.estimated_bytes = self
                .estimated_bytes
                .saturating_sub(entry.estimated_bytes)
                .saturating_add(estimated_bytes);
            *entry = MeasureTextCacheEntry {
                key,
                value,
                access_generation: self.generation,
                estimated_bytes,
            };
        } else {
            bucket.push(MeasureTextCacheEntry {
                key,
                value,
                access_generation: self.generation,
                estimated_bytes,
            });
            self.len += 1;
            self.estimated_bytes = self.estimated_bytes.saturating_add(estimated_bytes);
        }
        self.evict_if_needed();
    }

    fn evict_if_needed(&mut self) {
        if self.len <= MEASURE_CACHE_MAX_ENTRIES
            && self.estimated_bytes <= MEASURE_CACHE_MAX_ESTIMATED_BYTES
        {
            return;
        }

        let target_len = MEASURE_CACHE_MAX_ENTRIES * 3 / 4;
        let target_bytes = MEASURE_CACHE_MAX_ESTIMATED_BYTES * 3 / 4;
        let mut oldest = self
            .buckets
            .values()
            .flat_map(|bucket| {
                bucket
                    .iter()
                    .map(|entry| (entry.access_generation, entry.estimated_bytes))
            })
            .collect::<Vec<_>>();
        oldest.sort_unstable_by_key(|(generation, _)| *generation);

        let mut remaining_len = self.len;
        let mut remaining_bytes = self.estimated_bytes;
        let mut cutoff = None;
        for (generation, bytes) in oldest {
            if remaining_len <= target_len && remaining_bytes <= target_bytes {
                break;
            }
            remaining_len = remaining_len.saturating_sub(1);
            remaining_bytes = remaining_bytes.saturating_sub(bytes);
            cutoff = Some(generation);
        }
        let Some(cutoff) = cutoff else {
            return;
        };
        self.buckets.retain(|_, bucket| {
            bucket.retain(|entry| entry.access_generation > cutoff);
            !bucket.is_empty()
        });
        self.len = remaining_len;
        self.estimated_bytes = remaining_bytes;
    }
}

thread_local! {
    pub(super) static MEASURE_TEXT_CACHE: RefCell<MeasureTextCache> =
        RefCell::new(MeasureTextCache::new());
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct TextMeasureCacheKey {
    pub(super) content: String,
    pub(super) max_width_milli: i32,
    pub(super) allow_wrap: bool,
    pub(super) font_size_milli: i32,
    pub(super) line_height_milli: i32,
    pub(super) font_weight: u16,
    pub(super) align: InlineIfcAlignment,
    pub(super) font_families: Vec<String>,
}

pub(super) struct TextMeasureCacheLookup<'a> {
    content: &'a str,
    max_width_milli: i32,
    allow_wrap: bool,
    font_size_milli: i32,
    line_height_milli: i32,
    font_weight: u16,
    align: InlineIfcAlignment,
    font_families: &'a [String],
}

impl TextMeasureCacheLookup<'_> {
    fn fingerprint(&self) -> u64 {
        measure_cache_fingerprint(
            self.content,
            self.max_width_milli,
            self.allow_wrap,
            self.font_size_milli,
            self.line_height_milli,
            self.font_weight,
            self.align,
            self.font_families,
        )
    }

    pub(super) fn to_owned_key(&self) -> TextMeasureCacheKey {
        TextMeasureCacheKey {
            content: self.content.to_owned(),
            max_width_milli: self.max_width_milli,
            allow_wrap: self.allow_wrap,
            font_size_milli: self.font_size_milli,
            line_height_milli: self.line_height_milli,
            font_weight: self.font_weight,
            align: self.align,
            font_families: self.font_families.to_vec(),
        }
    }

    pub(super) fn estimated_entry_bytes(&self) -> usize {
        let family_bytes = self
            .font_families
            .iter()
            .map(|family| family.len())
            .sum::<usize>();
        // Parley layout storage is implementation-private. Glyph-heavy text
        // dominates it, so use a deliberately conservative per-input-byte
        // estimate and a floor for short labels.
        4096usize
            .saturating_add(self.content.len().saturating_mul(128))
            .saturating_add(family_bytes)
    }
}

impl TextMeasureCacheKey {
    fn matches(&self, lookup: &TextMeasureCacheLookup<'_>) -> bool {
        self.content == lookup.content
            && self.max_width_milli == lookup.max_width_milli
            && self.allow_wrap == lookup.allow_wrap
            && self.font_size_milli == lookup.font_size_milli
            && self.line_height_milli == lookup.line_height_milli
            && self.font_weight == lookup.font_weight
            && self.align == lookup.align
            && self.font_families == lookup.font_families
    }

    fn fingerprint(&self) -> u64 {
        measure_cache_fingerprint(
            &self.content,
            self.max_width_milli,
            self.allow_wrap,
            self.font_size_milli,
            self.line_height_milli,
            self.font_weight,
            self.align,
            &self.font_families,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn measure_cache_fingerprint(
    content: &str,
    max_width_milli: i32,
    allow_wrap: bool,
    font_size_milli: i32,
    line_height_milli: i32,
    font_weight: u16,
    align: InlineIfcAlignment,
    font_families: &[String],
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = rustc_hash::FxHasher::default();
    content.hash(&mut hasher);
    max_width_milli.hash(&mut hasher);
    allow_wrap.hash(&mut hasher);
    font_size_milli.hash(&mut hasher);
    line_height_milli.hash(&mut hasher);
    font_weight.hash(&mut hasher);
    align.hash(&mut hasher);
    font_families.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn quantize_milli(value: f32) -> i32 {
    (value * 1000.0).round() as i32
}

pub(super) fn make_measure_cache_lookup<'a>(
    content: &'a str,
    max_width: Option<f32>,
    allow_wrap: bool,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    align: InlineIfcAlignment,
    font_families: &'a [String],
) -> TextMeasureCacheLookup<'a> {
    TextMeasureCacheLookup {
        content,
        max_width_milli: max_width.map(quantize_milli).unwrap_or(-1),
        allow_wrap,
        font_size_milli: quantize_milli(font_size),
        line_height_milli: quantize_milli(line_height),
        font_weight,
        align,
        font_families,
    }
}
