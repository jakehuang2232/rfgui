//! Text cache types: LRU + measure / shape cache keys.

use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::sync::Arc;

use crate::view::inline_formatting_context::{InlineFormattingContext, InlineIfcAlignment};

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

/// LRU cache with generation-based eviction (à la Skia SkStrikeCache).
///
/// Each entry tracks an `access_gen` bumped on every hit.  When the cache
/// exceeds `MAX_ENTRIES`, the coldest 25 % of entries are evicted in one
/// batch — matching Skia's "at least `fTotalMemoryUsed >> 2`" policy.
pub(super) struct LruCache<K: Eq + std::hash::Hash + Clone, V> {
    map: FxHashMap<K, (V, u64)>, // value + access generation
    generation: u64,
}

pub(super) const LRU_CACHE_MAX_ENTRIES: usize = 4096;

impl<K: Eq + std::hash::Hash + Clone, V> LruCache<K, V> {
    pub(super) fn new() -> Self {
        Self {
            map: FxHashMap::default(),
            generation: 0,
        }
    }

    pub(super) fn get(&mut self, key: &K) -> Option<&V> {
        self.generation += 1;
        let current_gen = self.generation;
        self.map.get_mut(key).map(|(v, g)| {
            *g = current_gen;
            &*v
        })
    }

    pub(super) fn get_cloned(&mut self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        self.get(key).cloned()
    }

    pub(super) fn insert(&mut self, key: K, value: V) {
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
    pub(super) static MEASURE_TEXT_CACHE: RefCell<LruCache<TextMeasureCacheKey, MeasuredTextIfc>> =
        RefCell::new(LruCache::new());
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

pub(super) fn quantize_milli(value: f32) -> i32 {
    (value * 1000.0).round() as i32
}

pub(super) fn make_measure_cache_key(
    content: &str,
    max_width: Option<f32>,
    allow_wrap: bool,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    align: InlineIfcAlignment,
    font_families: &[String],
) -> TextMeasureCacheKey {
    TextMeasureCacheKey {
        content: content.to_string(),
        max_width_milli: max_width.map(quantize_milli).unwrap_or(-1),
        allow_wrap,
        font_size_milli: quantize_milli(font_size),
        line_height_milli: quantize_milli(line_height),
        font_weight,
        align,
        font_families: font_families.to_vec(),
    }
}
