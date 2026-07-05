// P1-P7 staged IFC scaffold. Most primitives are intentionally crate-visible
// before every formal call site is switched, so example builds should not be
// dominated by dead-code warnings while the rollout remains gated.
#![allow(dead_code)]

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;

use parley::{
    Affinity, Alignment as ParleyAlignment, AlignmentOptions, Cursor as ParleyCursor, FontData,
    FontFamily, FontFamilyName, FontWeight, InlineBox, InlineBoxKind, Layout as ParleyLayout,
    LineHeight, OverflowWrap, PositionedLayoutItem, StyleProperty, TextWrapMode,
};

use crate::view::font_system::with_shared_parley_context;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InlineIfcSourceId(pub(crate) u64);

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcInput {
    pub(crate) items: Vec<InlineIfcItem>,
    pub(crate) default_style: InlineIfcStyle,
    pub(crate) max_width: Option<f32>,
}

impl InlineIfcInput {
    pub(crate) fn new(items: Vec<InlineIfcItem>) -> Self {
        Self {
            items,
            default_style: InlineIfcStyle::default(),
            max_width: None,
        }
    }

    pub(crate) fn with_max_width(mut self, max_width: f32) -> Self {
        self.max_width = Some(max_width.max(1.0));
        self
    }

    pub(crate) fn cache_key(&self) -> InlineIfcCacheKey {
        self.cache_key_with_layout_options(InlineIfcLayoutOptions::from_input(self))
    }

    pub(crate) fn cache_key_with_layout_options(
        &self,
        layout_options: InlineIfcLayoutOptions,
    ) -> InlineIfcCacheKey {
        let content = InlineIfcContentKey {
            items: content_key_items_for(&self.items, &self.default_style),
        };
        let paint = InlineIfcPaintKey {
            items: paint_key_items_for(&self.items, &self.default_style),
        };
        InlineIfcCacheKey {
            content,
            layout: InlineIfcLayoutKey::from_options(layout_options),
            paint,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) enum InlineIfcAlignment {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InlineIfcLayoutOptions {
    pub(crate) max_width: Option<f32>,
    pub(crate) allow_wrap: bool,
    pub(crate) align: InlineIfcAlignment,
}

impl InlineIfcLayoutOptions {
    pub(crate) fn new(max_width: Option<f32>, allow_wrap: bool) -> Self {
        Self {
            max_width: if allow_wrap {
                max_width.map(|value| value.max(1.0))
            } else {
                None
            },
            allow_wrap,
            align: InlineIfcAlignment::Left,
        }
    }

    pub(crate) fn with_align(mut self, align: InlineIfcAlignment) -> Self {
        self.align = align;
        self
    }

    fn from_input(input: &InlineIfcInput) -> Self {
        Self::new(input.max_width, true)
    }
}

impl Default for InlineIfcLayoutOptions {
    fn default() -> Self {
        Self {
            max_width: None,
            allow_wrap: true,
            align: InlineIfcAlignment::Left,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum InlineIfcItem {
    TextSpan {
        source: InlineIfcSourceId,
        text: String,
        style: Option<InlineIfcStyle>,
    },
    Span {
        source: InlineIfcSourceId,
        style: Option<InlineIfcStyle>,
        children: Vec<InlineIfcItem>,
    },
    AtomicInlineBox {
        source: InlineIfcSourceId,
        measurement: InlineIfcMeasuredAtomicBox,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InlineIfcSize {
    pub(crate) width: f32,
    pub(crate) height: f32,
}

impl InlineIfcSize {
    pub(crate) fn new(width: f32, height: f32) -> Self {
        Self {
            width: width.max(0.0),
            height: height.max(0.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InlineIfcPercentBase {
    pub(crate) width: Option<f32>,
    pub(crate) height: Option<f32>,
}

impl InlineIfcPercentBase {
    pub(crate) fn new(width: Option<f32>, height: Option<f32>) -> Self {
        Self {
            width: width.map(|value| value.max(0.0)),
            height: height.map(|value| value.max(0.0)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InlineIfcIntrinsicSize {
    pub(crate) min_content_width: f32,
    pub(crate) max_content_width: f32,
    pub(crate) preferred_width: Option<f32>,
    pub(crate) preferred_height: Option<f32>,
}

impl InlineIfcIntrinsicSize {
    pub(crate) fn new(
        min_content_width: f32,
        max_content_width: f32,
        preferred_width: Option<f32>,
        preferred_height: Option<f32>,
    ) -> Self {
        Self {
            min_content_width: min_content_width.max(0.0),
            max_content_width: max_content_width.max(0.0),
            preferred_width: preferred_width.map(|value| value.max(0.0)),
            preferred_height: preferred_height.map(|value| value.max(0.0)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InlineIfcAtomicSizingRules {
    pub(crate) min_width: Option<f32>,
    pub(crate) max_width: Option<f32>,
    pub(crate) min_height: Option<f32>,
    pub(crate) max_height: Option<f32>,
    pub(crate) intrinsic_size: Option<InlineIfcIntrinsicSize>,
}

impl InlineIfcAtomicSizingRules {
    pub(crate) fn none() -> Self {
        Self {
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            intrinsic_size: None,
        }
    }
}

impl Default for InlineIfcAtomicSizingRules {
    fn default() -> Self {
        Self::none()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InlineIfcAtomicMeasureConstraints {
    pub(crate) max_width: Option<f32>,
    pub(crate) available_height: Option<f32>,
    pub(crate) viewport: Option<InlineIfcSize>,
    pub(crate) percent_base: InlineIfcPercentBase,
    pub(crate) sizing: InlineIfcAtomicSizingRules,
}

impl InlineIfcAtomicMeasureConstraints {
    pub(crate) fn new(max_width: Option<f32>) -> Self {
        Self {
            max_width: max_width.map(|value| value.max(0.0)),
            available_height: None,
            viewport: None,
            percent_base: InlineIfcPercentBase::new(None, None),
            sizing: InlineIfcAtomicSizingRules::none(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcMeasuredAtomicBox {
    pub(crate) constraints: InlineIfcAtomicMeasureConstraints,
    pub(crate) measured_size: InlineIfcSize,
}

impl InlineIfcMeasuredAtomicBox {
    pub(crate) fn new(
        measured_size: InlineIfcSize,
        constraints: InlineIfcAtomicMeasureConstraints,
    ) -> Self {
        Self {
            constraints,
            measured_size,
        }
    }

    fn measurement_width_bits(&self) -> u32 {
        f32_cache_bits(self.measured_size.width)
    }

    fn measurement_height_bits(&self) -> u32 {
        f32_cache_bits(self.measured_size.height)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcStyle {
    pub(crate) font_size: f32,
    pub(crate) line_height: f32,
    pub(crate) font_weight: u16,
    pub(crate) brush: [u8; 4],
    pub(crate) font_families: Vec<String>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InlineIfcStyleKey {
    pub(crate) font_size_bits: u32,
    pub(crate) line_height_bits: u32,
    pub(crate) font_weight: u16,
    pub(crate) font_families: Vec<String>,
}

impl InlineIfcStyleKey {
    fn from_style(style: &InlineIfcStyle) -> Self {
        Self {
            font_size_bits: style.font_size.max(1.0).to_bits(),
            line_height_bits: style.line_height.max(0.1).to_bits(),
            font_weight: style.font_weight,
            font_families: style.font_families.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InlineIfcPaintStyleKey {
    pub(crate) brush: [u8; 4],
}

impl InlineIfcPaintStyleKey {
    fn from_style(style: &InlineIfcStyle) -> Self {
        Self { brush: style.brush }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InlineIfcContentKey {
    pub(crate) items: Vec<InlineIfcContentKeyItem>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum InlineIfcContentKeyItem {
    Text {
        source: InlineIfcSourceId,
        text: String,
        shape_style: InlineIfcStyleKey,
    },
    Span {
        source: InlineIfcSourceId,
        shape_style: InlineIfcStyleKey,
        children: Vec<InlineIfcContentKeyItem>,
    },
    AtomicInlineBox {
        source: InlineIfcSourceId,
        shape_key: InlineIfcAtomicBoxShapeKey,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InlineIfcLayoutKey {
    pub(crate) max_width_bits: Option<u32>,
    pub(crate) allow_wrap: bool,
    pub(crate) align: InlineIfcAlignment,
}

impl InlineIfcLayoutKey {
    fn from_options(options: InlineIfcLayoutOptions) -> Self {
        Self {
            max_width_bits: options.max_width.map(f32::to_bits),
            allow_wrap: options.allow_wrap,
            align: options.align,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InlineIfcPaintKey {
    pub(crate) items: Vec<InlineIfcPaintKeyItem>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum InlineIfcPaintKeyItem {
    Text {
        source: InlineIfcSourceId,
        paint_style: InlineIfcPaintStyleKey,
    },
    Span {
        source: InlineIfcSourceId,
        paint_style: InlineIfcPaintStyleKey,
        children: Vec<InlineIfcPaintKeyItem>,
    },
    AtomicInlineBox {
        source: InlineIfcSourceId,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InlineIfcAtomicBoxShapeKey {
    pub(crate) measured_width_bits: u32,
    pub(crate) measured_height_bits: u32,
    pub(crate) constraints: InlineIfcAtomicConstraintsShapeKey,
}

impl InlineIfcAtomicBoxShapeKey {
    fn from_measurement(measurement: &InlineIfcMeasuredAtomicBox) -> Self {
        Self {
            measured_width_bits: measurement.measurement_width_bits(),
            measured_height_bits: measurement.measurement_height_bits(),
            constraints: InlineIfcAtomicConstraintsShapeKey::from_constraints(
                &measurement.constraints,
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InlineIfcAtomicConstraintsShapeKey {
    pub(crate) max_width_bits: Option<u32>,
    pub(crate) available_height_bits: Option<u32>,
    pub(crate) viewport_width_bits: Option<u32>,
    pub(crate) viewport_height_bits: Option<u32>,
    pub(crate) percent_base_width_bits: Option<u32>,
    pub(crate) percent_base_height_bits: Option<u32>,
    pub(crate) sizing: InlineIfcAtomicSizingShapeKey,
}

impl InlineIfcAtomicConstraintsShapeKey {
    fn from_constraints(constraints: &InlineIfcAtomicMeasureConstraints) -> Self {
        Self {
            max_width_bits: constraints.max_width.map(f32_cache_bits),
            available_height_bits: constraints.available_height.map(f32_cache_bits),
            viewport_width_bits: constraints.viewport.map(|size| f32_cache_bits(size.width)),
            viewport_height_bits: constraints.viewport.map(|size| f32_cache_bits(size.height)),
            percent_base_width_bits: constraints.percent_base.width.map(f32_cache_bits),
            percent_base_height_bits: constraints.percent_base.height.map(f32_cache_bits),
            sizing: InlineIfcAtomicSizingShapeKey::from_sizing(&constraints.sizing),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InlineIfcAtomicSizingShapeKey {
    pub(crate) min_width_bits: Option<u32>,
    pub(crate) max_width_bits: Option<u32>,
    pub(crate) min_height_bits: Option<u32>,
    pub(crate) max_height_bits: Option<u32>,
    pub(crate) intrinsic_size: Option<InlineIfcIntrinsicSizeShapeKey>,
}

impl InlineIfcAtomicSizingShapeKey {
    fn from_sizing(sizing: &InlineIfcAtomicSizingRules) -> Self {
        Self {
            min_width_bits: sizing.min_width.map(f32_cache_bits),
            max_width_bits: sizing.max_width.map(f32_cache_bits),
            min_height_bits: sizing.min_height.map(f32_cache_bits),
            max_height_bits: sizing.max_height.map(f32_cache_bits),
            intrinsic_size: sizing
                .intrinsic_size
                .map(InlineIfcIntrinsicSizeShapeKey::from_intrinsic_size),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InlineIfcIntrinsicSizeShapeKey {
    pub(crate) min_content_width_bits: u32,
    pub(crate) max_content_width_bits: u32,
    pub(crate) preferred_width_bits: Option<u32>,
    pub(crate) preferred_height_bits: Option<u32>,
}

impl InlineIfcIntrinsicSizeShapeKey {
    fn from_intrinsic_size(size: InlineIfcIntrinsicSize) -> Self {
        Self {
            min_content_width_bits: f32_cache_bits(size.min_content_width),
            max_content_width_bits: f32_cache_bits(size.max_content_width),
            preferred_width_bits: size.preferred_width.map(f32_cache_bits),
            preferred_height_bits: size.preferred_height.map(f32_cache_bits),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InlineIfcCacheKey {
    pub(crate) content: InlineIfcContentKey,
    pub(crate) layout: InlineIfcLayoutKey,
    pub(crate) paint: InlineIfcPaintKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InlineIfcInvalidation {
    Reuse,
    RepaintOnly,
    Reshape,
}

impl InlineIfcCacheKey {
    pub(crate) fn invalidation_from(&self, previous: &Self) -> InlineIfcInvalidation {
        if self.content != previous.content || self.layout != previous.layout {
            return InlineIfcInvalidation::Reshape;
        }
        if self.paint != previous.paint {
            return InlineIfcInvalidation::RepaintOnly;
        }
        InlineIfcInvalidation::Reuse
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InlineIfcShapeCacheKey {
    pub(crate) content: InlineIfcContentKey,
    pub(crate) layout: InlineIfcLayoutKey,
}

impl InlineIfcShapeCacheKey {
    fn from_cache_key(cache_key: &InlineIfcCacheKey) -> Self {
        Self {
            content: cache_key.content.clone(),
            layout: cache_key.layout,
        }
    }
}

pub(crate) enum InlineIfcCacheLookup<'a> {
    Reuse(&'a InlineIfcCachedEntry),
    RepaintOnly(&'a InlineIfcCachedEntry),
    Miss { invalidation: InlineIfcInvalidation },
}

impl<'a> InlineIfcCacheLookup<'a> {
    pub(crate) fn cached_entry(&self) -> Option<&'a InlineIfcCachedEntry> {
        match self {
            Self::Reuse(entry) | Self::RepaintOnly(entry) => Some(entry),
            Self::Miss { .. } => None,
        }
    }
}

pub(crate) struct InlineIfcCachedEntry {
    context: InlineFormattingContext,
    shape_key: InlineIfcShapeCacheKey,
}

impl InlineIfcCachedEntry {
    pub(crate) fn context(&self) -> &InlineFormattingContext {
        &self.context
    }

    pub(crate) fn cache_key(&self) -> &InlineIfcCacheKey {
        self.context.cache_key()
    }

    pub(crate) fn shape_key(&self) -> &InlineIfcShapeCacheKey {
        &self.shape_key
    }
}

pub(crate) struct InlineIfcCacheUpdate<'a> {
    pub(crate) invalidation: InlineIfcInvalidation,
    pub(crate) entry: &'a InlineIfcCachedEntry,
    pub(crate) rebuilt: bool,
}

#[derive(Default)]
pub(crate) struct InlineIfcCache {
    entries: HashMap<InlineIfcShapeCacheKey, InlineIfcCachedEntry>,
    // Access-generation LRU bookkeeping: shaped contexts are heavy
    // (parley layout + memoized glyph/snapshot vectors), and resize drags
    // mint a new shape key per width — without eviction the map grows
    // without bound for the lifetime of the owning element.
    access_generation: u64,
    access_by_key: HashMap<InlineIfcShapeCacheKey, u64>,
}

/// Retained shaped contexts per cache. Two would cover current+pending;
/// a little headroom keeps width jitter (resize back-and-forth) warm.
const INLINE_IFC_CACHE_MAX_ENTRIES: usize = 4;

impl InlineIfcCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn touch(&mut self, shape_key: &InlineIfcShapeCacheKey) {
        self.access_generation += 1;
        self.access_by_key
            .insert(shape_key.clone(), self.access_generation);
    }

    fn evict_to_capacity(&mut self, keep: &InlineIfcShapeCacheKey) {
        while self.entries.len() > INLINE_IFC_CACHE_MAX_ENTRIES {
            let Some(coldest) = self
                .entries
                .keys()
                .filter(|key| *key != keep)
                .min_by_key(|key| self.access_by_key.get(*key).copied().unwrap_or(0))
                .cloned()
            else {
                return;
            };
            self.entries.remove(&coldest);
            self.access_by_key.remove(&coldest);
        }
    }

    pub(crate) fn lookup_input(&self, input: &InlineIfcInput) -> InlineIfcCacheLookup<'_> {
        let cache_key = input.cache_key();
        self.lookup_key(&cache_key)
    }

    pub(crate) fn lookup_key(&self, cache_key: &InlineIfcCacheKey) -> InlineIfcCacheLookup<'_> {
        let shape_key = InlineIfcShapeCacheKey::from_cache_key(cache_key);
        let Some(entry) = self.entries.get(&shape_key) else {
            return InlineIfcCacheLookup::Miss {
                invalidation: InlineIfcInvalidation::Reshape,
            };
        };

        match cache_key.invalidation_from(entry.cache_key()) {
            InlineIfcInvalidation::Reuse => InlineIfcCacheLookup::Reuse(entry),
            InlineIfcInvalidation::RepaintOnly => InlineIfcCacheLookup::RepaintOnly(entry),
            InlineIfcInvalidation::Reshape => InlineIfcCacheLookup::Miss {
                invalidation: InlineIfcInvalidation::Reshape,
            },
        }
    }

    pub(crate) fn put(&mut self, input: InlineIfcInput) -> &InlineIfcCachedEntry {
        let context = InlineFormattingContext::build(input);
        let shape_key = InlineIfcShapeCacheKey::from_cache_key(context.cache_key());
        self.entries.insert(
            shape_key.clone(),
            InlineIfcCachedEntry {
                context,
                shape_key: shape_key.clone(),
            },
        );
        self.touch(&shape_key);
        self.evict_to_capacity(&shape_key);
        self.entries
            .get(&shape_key)
            .expect("inserted IFC cache entry should be available")
    }

    pub(crate) fn update(&mut self, input: InlineIfcInput) -> InlineIfcCacheUpdate<'_> {
        let layout_options = InlineIfcLayoutOptions::from_input(&input);
        self.update_with_options(input, layout_options)
    }

    pub(crate) fn update_with_options(
        &mut self,
        input: InlineIfcInput,
        layout_options: InlineIfcLayoutOptions,
    ) -> InlineIfcCacheUpdate<'_> {
        let cache_key = input.cache_key_with_layout_options(layout_options);
        let shape_key = InlineIfcShapeCacheKey::from_cache_key(&cache_key);
        let invalidation = self
            .entries
            .get(&shape_key)
            .map(|entry| cache_key.invalidation_from(entry.cache_key()))
            .unwrap_or(InlineIfcInvalidation::Reshape);

        if invalidation == InlineIfcInvalidation::Reuse {
            self.touch(&shape_key);
            let entry = self
                .entries
                .get(&shape_key)
                .expect("reused IFC cache entry should be available");
            return InlineIfcCacheUpdate {
                invalidation,
                entry,
                rebuilt: false,
            };
        }

        let context = InlineFormattingContext::build_with_options(input, layout_options);
        let shape_key = InlineIfcShapeCacheKey::from_cache_key(context.cache_key());
        self.entries.insert(
            shape_key.clone(),
            InlineIfcCachedEntry {
                context,
                shape_key: shape_key.clone(),
            },
        );
        self.touch(&shape_key);
        self.evict_to_capacity(&shape_key);
        let entry = self
            .entries
            .get(&shape_key)
            .expect("updated IFC cache entry should be available");

        // RepaintOnly keeps the shape cache boundary stable, but this transition
        // API still rebuilds the full context until paint-only storage is split.
        InlineIfcCacheUpdate {
            invalidation,
            entry,
            rebuilt: true,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

impl Default for InlineIfcStyle {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            line_height: 1.2,
            font_weight: 400,
            brush: [0, 0, 0, 255],
            font_families: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InlineIfcSourceKind {
    Text,
    Span,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcSourceRange {
    pub(crate) source: InlineIfcSourceId,
    pub(crate) kind: InlineIfcSourceKind,
    pub(crate) range: Range<usize>,
    pub(crate) depth: usize,
    pub(crate) style: Option<InlineIfcStyle>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcStyleRange {
    pub(crate) range: Range<usize>,
    pub(crate) style: InlineIfcStyle,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcBoxMapping {
    pub(crate) id: u64,
    pub(crate) source: InlineIfcSourceId,
    pub(crate) insertion_byte: usize,
    pub(crate) measurement: InlineIfcMeasuredAtomicBox,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcLineFragment {
    pub(crate) line_index: usize,
    pub(crate) source: InlineIfcSourceId,
    pub(crate) range: Range<usize>,
    pub(crate) x0: f32,
    pub(crate) x1: f32,
    pub(crate) y0: f32,
    pub(crate) y1: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcDecorationFragment {
    pub(crate) line_index: usize,
    pub(crate) source: InlineIfcSourceId,
    pub(crate) range: Range<usize>,
    pub(crate) style: Option<InlineIfcStyle>,
    pub(crate) x0: f32,
    pub(crate) x1: f32,
    pub(crate) y0: f32,
    pub(crate) y1: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcGlyphItem {
    pub(crate) line_index: usize,
    pub(crate) run_index: usize,
    pub(crate) source: InlineIfcSourceId,
    pub(crate) cluster_range: Range<usize>,
    pub(crate) glyph_id: u32,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) advance: f32,
    pub(crate) font_size: f32,
    pub(crate) font_data: Option<FontData>,
    pub(crate) font_data_id: u64,
    pub(crate) font_index: u32,
    pub(crate) normalized_coords_hash: u64,
    pub(crate) style: InlineIfcStyle,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcGlyphGroup {
    pub(crate) line_index: usize,
    pub(crate) source: InlineIfcSourceId,
    pub(crate) range: Range<usize>,
    pub(crate) style: InlineIfcStyle,
    pub(crate) glyphs: Vec<InlineIfcGlyphItem>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InlineIfcTextPaintBatchKey {
    pub(crate) brush: [u8; 4],
    pub(crate) font_data_id: u64,
    pub(crate) font_index: u32,
    pub(crate) normalized_coords_hash: u64,
    pub(crate) font_size_bits: u32,
    pub(crate) font_weight: u16,
}

impl InlineIfcTextPaintBatchKey {
    fn from_glyph(glyph: &InlineIfcGlyphItem) -> Self {
        Self {
            brush: glyph.style.brush,
            font_data_id: glyph.font_data_id,
            font_index: glyph.font_index,
            normalized_coords_hash: glyph.normalized_coords_hash,
            font_size_bits: glyph.font_size.to_bits(),
            font_weight: glyph.style.font_weight,
        }
    }

    #[cfg(test)]
    fn font_size(self) -> f32 {
        f32::from_bits(self.font_size_bits)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcPaintGlyph {
    pub(crate) line_index: usize,
    pub(crate) source: InlineIfcSourceId,
    pub(crate) cluster_range: Range<usize>,
    pub(crate) glyph_id: u32,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) advance: f32,
    pub(crate) font_size: f32,
    pub(crate) font_data: Option<FontData>,
    pub(crate) font_data_id: u64,
    pub(crate) font_index: u32,
    pub(crate) normalized_coords_hash: u64,
    pub(crate) style: InlineIfcStyle,
    pub(crate) batch_key: InlineIfcTextPaintBatchKey,
}

impl From<InlineIfcGlyphItem> for InlineIfcPaintGlyph {
    fn from(glyph: InlineIfcGlyphItem) -> Self {
        let batch_key = InlineIfcTextPaintBatchKey::from_glyph(&glyph);
        Self {
            line_index: glyph.line_index,
            source: glyph.source,
            cluster_range: glyph.cluster_range,
            glyph_id: glyph.glyph_id,
            x: glyph.x,
            y: glyph.y,
            advance: glyph.advance,
            font_size: glyph.font_size,
            font_data: glyph.font_data,
            font_data_id: glyph.font_data_id,
            font_index: glyph.font_index,
            normalized_coords_hash: glyph.normalized_coords_hash,
            style: glyph.style,
            batch_key,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcTextPaintRun {
    pub(crate) line_index: usize,
    pub(crate) source: InlineIfcSourceId,
    pub(crate) range: Range<usize>,
    pub(crate) style: InlineIfcStyle,
    pub(crate) batch_key: InlineIfcTextPaintBatchKey,
    pub(crate) glyphs: Vec<InlineIfcPaintGlyph>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InlineIfcPaintRect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

impl InlineIfcPaintRect {
    pub(crate) fn right(&self) -> f32 {
        self.x + self.width
    }

    pub(crate) fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcDecorationPaintFragment {
    pub(crate) line_index: usize,
    pub(crate) source: InlineIfcSourceId,
    pub(crate) range: Range<usize>,
    pub(crate) rect: InlineIfcPaintRect,
    pub(crate) style: Option<InlineIfcStyle>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct InlineIfcDecorationBoxInsets {
    pub(crate) left: f32,
    pub(crate) right: f32,
    pub(crate) top: f32,
    pub(crate) bottom: f32,
}

impl InlineIfcDecorationBoxInsets {
    pub(crate) fn new(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self {
            left: left.max(0.0),
            right: right.max(0.0),
            top: top.max(0.0),
            bottom: bottom.max(0.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcElementDecorationPaintFragment {
    pub(crate) line_index: usize,
    pub(crate) source: InlineIfcSourceId,
    pub(crate) range: Range<usize>,
    pub(crate) rect: InlineIfcPaintRect,
    pub(crate) is_first_for_source: bool,
    pub(crate) is_last_for_source: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InlineIfcDrawRectMetadata {
    pub(crate) position: [f32; 2],
    pub(crate) size: [f32; 2],
    pub(crate) fill_color: [f32; 4],
    pub(crate) opacity: f32,
    pub(crate) border_widths: [f32; 4],
    pub(crate) border_color: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InlineIfcElementDecorationDrawRectStyle {
    pub(crate) style_key: InlineIfcPaintStyleKey,
    pub(crate) fill_color: [f32; 4],
    pub(crate) opacity: f32,
    pub(crate) border_widths: [f32; 4],
    pub(crate) border_color: [f32; 4],
}

impl InlineIfcElementDecorationDrawRectStyle {
    pub(crate) fn new(
        style_key: InlineIfcPaintStyleKey,
        fill_color: [f32; 4],
        opacity: f32,
        border_widths: [f32; 4],
        border_color: [f32; 4],
    ) -> Self {
        Self {
            style_key,
            fill_color,
            opacity: opacity.clamp(0.0, 1.0),
            border_widths: border_widths.map(|width| width.max(0.0)),
            border_color,
        }
    }

    pub(crate) fn from_fill_style(style: &InlineIfcStyle) -> Self {
        Self::new(
            InlineIfcPaintStyleKey::from_style(style),
            brush_to_text_color(style.brush),
            1.0,
            [0.0; 4],
            [0.0; 4],
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcElementDecorationDrawRect {
    pub(crate) line_index: usize,
    pub(crate) source: InlineIfcSourceId,
    pub(crate) range: Range<usize>,
    pub(crate) rect: InlineIfcPaintRect,
    pub(crate) style_key: InlineIfcPaintStyleKey,
    pub(crate) slice_insets: InlineIfcDecorationBoxInsets,
    pub(crate) is_first_for_source: bool,
    pub(crate) is_last_for_source: bool,
    pub(crate) metadata: InlineIfcDrawRectMetadata,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcElementDecorationDrawRectPackage {
    pub(crate) source: InlineIfcSourceId,
    pub(crate) style_key: InlineIfcPaintStyleKey,
    pub(crate) slice_insets: InlineIfcDecorationBoxInsets,
    pub(crate) fragments: Vec<InlineIfcElementDecorationDrawRect>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcAtomicBoxPlacement {
    pub(crate) id: u64,
    pub(crate) source: InlineIfcSourceId,
    pub(crate) insertion_byte: usize,
    pub(crate) line_index: usize,
    pub(crate) rect: InlineIfcPaintRect,
    pub(crate) measurement: InlineIfcMeasuredAtomicBox,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcAtomicBoxPlacementPackage {
    pub(crate) source: InlineIfcSourceId,
    pub(crate) placements: Vec<InlineIfcAtomicBoxPlacement>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InlineIfcElementDecorationPackageSource {
    pub(crate) source: InlineIfcSourceId,
    pub(crate) slice_insets: InlineIfcDecorationBoxInsets,
    pub(crate) draw_rect_style: InlineIfcElementDecorationDrawRectStyle,
}

impl InlineIfcElementDecorationPackageSource {
    pub(crate) fn new(
        source: InlineIfcSourceId,
        slice_insets: InlineIfcDecorationBoxInsets,
        draw_rect_style: InlineIfcElementDecorationDrawRectStyle,
    ) -> Self {
        Self {
            source,
            slice_insets,
            draw_rect_style,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct InlineIfcElementPackageDistributionInput {
    pub(crate) decoration_sources: Vec<InlineIfcElementDecorationPackageSource>,
    pub(crate) atomic_sources: Vec<InlineIfcSourceId>,
}

impl InlineIfcElementPackageDistributionInput {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_decoration_source(
        mut self,
        source: InlineIfcElementDecorationPackageSource,
    ) -> Self {
        self.decoration_sources.push(source);
        self
    }

    pub(crate) fn with_atomic_source(mut self, source: InlineIfcSourceId) -> Self {
        self.atomic_sources.push(source);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcElementRootSource {
    pub(crate) input: InlineIfcInput,
    pub(crate) layout_options: InlineIfcLayoutOptions,
    pub(crate) package_distribution: InlineIfcElementPackageDistributionInput,
}

impl InlineIfcElementRootSource {
    pub(crate) fn new(input: InlineIfcInput) -> Self {
        let layout_options = InlineIfcLayoutOptions::from_input(&input);
        Self {
            input,
            layout_options,
            package_distribution: InlineIfcElementPackageDistributionInput::new(),
        }
    }

    pub(crate) fn cache_key(&self) -> InlineIfcCacheKey {
        self.input
            .cache_key_with_layout_options(self.layout_options)
    }

    pub(crate) fn with_package_distribution(
        mut self,
        package_distribution: InlineIfcElementPackageDistributionInput,
    ) -> Self {
        self.package_distribution = package_distribution;
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct InlineIfcElementRootSourceBuilder {
    items: Vec<InlineIfcItem>,
    max_width: Option<f32>,
    allow_wrap: bool,
    package_distribution: InlineIfcElementPackageDistributionInput,
}

impl InlineIfcElementRootSourceBuilder {
    pub(crate) fn new() -> Self {
        Self {
            items: Vec::new(),
            max_width: None,
            allow_wrap: true,
            package_distribution: InlineIfcElementPackageDistributionInput::new(),
        }
    }

    pub(crate) fn with_max_width(mut self, max_width: f32) -> Self {
        self.max_width = Some(max_width.max(1.0));
        self
    }

    pub(crate) fn with_allow_wrap(mut self, allow_wrap: bool) -> Self {
        self.allow_wrap = allow_wrap;
        self
    }

    pub(crate) fn push_item(&mut self, item: InlineIfcItem) -> &mut Self {
        self.items.push(item);
        self
    }

    pub(crate) fn add_decoration_source(
        &mut self,
        source: InlineIfcElementDecorationPackageSource,
    ) -> &mut Self {
        self.package_distribution.decoration_sources.push(source);
        self
    }

    pub(crate) fn add_atomic_source(&mut self, source: InlineIfcSourceId) -> &mut Self {
        self.package_distribution.atomic_sources.push(source);
        self
    }

    pub(crate) fn build(self) -> InlineIfcElementRootSource {
        let mut input = InlineIfcInput::new(self.items);
        if let Some(max_width) = self.max_width {
            input = input.with_max_width(max_width);
        }
        let layout_options = InlineIfcLayoutOptions::new(input.max_width, self.allow_wrap);
        InlineIfcElementRootSource {
            input,
            layout_options,
            package_distribution: self.package_distribution,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcElementRootCandidate {
    pub(crate) cache_key: InlineIfcCacheKey,
    pub(crate) invalidation: InlineIfcInvalidation,
    pub(crate) rebuilt: bool,
    pub(crate) package_distributor: InlineIfcElementPackageDistributor,
}

impl InlineIfcElementRootCandidate {
    pub(crate) fn package(
        &self,
        source: InlineIfcSourceId,
    ) -> Option<&InlineIfcDistributedElementPackages> {
        self.package_distributor.package(source)
    }

    pub(crate) fn decoration_package(
        &self,
        source: InlineIfcSourceId,
    ) -> Option<&InlineIfcElementDecorationDrawRectPackage> {
        self.package_distributor.decoration_package(source)
    }

    pub(crate) fn atomic_package(
        &self,
        source: InlineIfcSourceId,
    ) -> Option<&InlineIfcAtomicBoxPlacementPackage> {
        self.package_distributor.atomic_package(source)
    }
}

#[derive(Default)]
pub(crate) struct InlineIfcElementRootCandidateCache {
    cache: InlineIfcCache,
}

impl InlineIfcElementRootCandidateCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn update(
        &mut self,
        source: &InlineIfcElementRootSource,
    ) -> InlineIfcElementRootCandidate {
        let update = self
            .cache
            .update_with_options(source.input.clone(), source.layout_options);
        let cache_key = update.entry.cache_key().clone();
        let package_distributor = update
            .entry
            .context()
            .element_package_distributor(source.package_distribution.clone());
        InlineIfcElementRootCandidate {
            cache_key,
            invalidation: update.invalidation,
            rebuilt: update.rebuilt,
            package_distributor,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.cache.len()
    }

    /// Borrow the shaped context for a candidate produced by [`Self::update`].
    /// Returns `None` if the entry was evicted or never built.
    pub(crate) fn context_for(
        &self,
        cache_key: &InlineIfcCacheKey,
    ) -> Option<&InlineFormattingContext> {
        match self.cache.lookup_key(cache_key) {
            InlineIfcCacheLookup::Reuse(entry) | InlineIfcCacheLookup::RepaintOnly(entry) => {
                Some(entry.context())
            }
            InlineIfcCacheLookup::Miss { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcDistributedElementPackages {
    pub(crate) source: InlineIfcSourceId,
    pub(crate) decoration_draw_rect: Option<InlineIfcElementDecorationDrawRectPackage>,
    pub(crate) atomic_placement: Option<InlineIfcAtomicBoxPlacementPackage>,
}

impl InlineIfcDistributedElementPackages {
    fn new(source: InlineIfcSourceId) -> Self {
        Self {
            source,
            decoration_draw_rect: None,
            atomic_placement: None,
        }
    }

    fn is_empty(&self) -> bool {
        self.decoration_draw_rect
            .as_ref()
            .is_none_or(|package| package.fragments.is_empty())
            && self
                .atomic_placement
                .as_ref()
                .is_none_or(|package| package.placements.is_empty())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct InlineIfcElementPackageDistributor {
    packages_by_source: HashMap<InlineIfcSourceId, InlineIfcDistributedElementPackages>,
}

impl InlineIfcElementPackageDistributor {
    pub(crate) fn package(
        &self,
        source: InlineIfcSourceId,
    ) -> Option<&InlineIfcDistributedElementPackages> {
        self.packages_by_source.get(&source)
    }

    pub(crate) fn decoration_package(
        &self,
        source: InlineIfcSourceId,
    ) -> Option<&InlineIfcElementDecorationDrawRectPackage> {
        self.package(source)
            .and_then(|package| package.decoration_draw_rect.as_ref())
    }

    pub(crate) fn atomic_package(
        &self,
        source: InlineIfcSourceId,
    ) -> Option<&InlineIfcAtomicBoxPlacementPackage> {
        self.package(source)
            .and_then(|package| package.atomic_placement.as_ref())
    }

    pub(crate) fn packages(&self) -> impl Iterator<Item = &InlineIfcDistributedElementPackages> {
        self.packages_by_source.values()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcTextPaintOutput {
    pub(crate) glyphs: Vec<InlineIfcPaintGlyph>,
    pub(crate) runs: Vec<InlineIfcTextPaintRun>,
    pub(crate) decorations: Vec<InlineIfcDecorationPaintFragment>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcTextGlyph {
    pub(crate) source: InlineIfcSourceId,
    pub(crate) cluster_range: Range<usize>,
    pub(crate) glyph_id: u32,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) advance: f32,
    pub(crate) font_size: f32,
    pub(crate) font_data: Option<FontData>,
    pub(crate) font_data_id: u64,
    pub(crate) font_index: u32,
    pub(crate) normalized_coords_hash: u64,
    pub(crate) style: InlineIfcStyle,
    pub(crate) batch_key: InlineIfcTextPaintBatchKey,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcTextLine {
    pub(crate) line_index: usize,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) baseline: f32,
    pub(crate) range: Range<usize>,
    pub(crate) glyphs: Vec<InlineIfcTextGlyph>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcTextLayoutSnapshot {
    pub(crate) lines: Vec<InlineIfcTextLine>,
    pub(crate) inline_boxes: Vec<InlineIfcInlineBoxPlacement>,
    pub(crate) decorations: Vec<InlineIfcDecorationPaintFragment>,
}

impl InlineIfcTextLayoutSnapshot {
    pub(crate) fn text_pass_paint_input(&self) -> InlineIfcTextPassPaintInput {
        let lines = self
            .lines
            .iter()
            .map(|line| InlineIfcTextPassLineInput {
                line_index: line.line_index,
                x: line.x,
                y: line.y,
                width: line.width,
                height: line.height,
                baseline: line.baseline,
                range: line.range.clone(),
            })
            .collect::<Vec<_>>();

        let mut glyphs = Vec::new();
        for line in &self.lines {
            let baseline_y = line.y + line.baseline;
            for glyph in &line.glyphs {
                glyphs.push(InlineIfcTextPassGlyphInput {
                    line_index: line.line_index,
                    source: glyph.source,
                    cluster_range: glyph.cluster_range.clone(),
                    glyph_id: glyph.glyph_id,
                    // Parley positioned glyphs are absolute in layout space
                    // (alignment shift included); line.x must not be added
                    // again or aligned lines paint double-shifted.
                    x: glyph.x,
                    baseline_y,
                    glyph_x: glyph.x - line.x,
                    glyph_y: glyph.y - baseline_y,
                    advance: glyph.advance,
                    font_size: glyph.font_size,
                    font_data: glyph.font_data.clone(),
                    font_data_id: glyph.font_data_id,
                    font_index: glyph.font_index,
                    normalized_coords_hash: glyph.normalized_coords_hash,
                    style: glyph.style.clone(),
                    batch_key: glyph.batch_key,
                    color: brush_to_text_color(glyph.batch_key.brush),
                });
            }
        }

        let batches = text_pass_batches_from_glyphs(&glyphs);
        InlineIfcTextPassPaintInput {
            lines,
            glyphs,
            batches,
            decorations: self.decorations.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcTextPassPaintInput {
    pub(crate) lines: Vec<InlineIfcTextPassLineInput>,
    pub(crate) glyphs: Vec<InlineIfcTextPassGlyphInput>,
    pub(crate) batches: Vec<InlineIfcTextPassBatchInput>,
    pub(crate) decorations: Vec<InlineIfcDecorationPaintFragment>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcTextPassLineInput {
    pub(crate) line_index: usize,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) baseline: f32,
    pub(crate) range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcTextPassGlyphInput {
    pub(crate) line_index: usize,
    pub(crate) source: InlineIfcSourceId,
    pub(crate) cluster_range: Range<usize>,
    pub(crate) glyph_id: u32,
    pub(crate) x: f32,
    pub(crate) baseline_y: f32,
    pub(crate) glyph_x: f32,
    pub(crate) glyph_y: f32,
    pub(crate) advance: f32,
    pub(crate) font_size: f32,
    pub(crate) font_data: Option<FontData>,
    pub(crate) font_data_id: u64,
    pub(crate) font_index: u32,
    pub(crate) normalized_coords_hash: u64,
    pub(crate) style: InlineIfcStyle,
    pub(crate) batch_key: InlineIfcTextPaintBatchKey,
    pub(crate) color: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcTextPassBatchInput {
    pub(crate) batch_key: InlineIfcTextPaintBatchKey,
    pub(crate) color: [f32; 4],
    pub(crate) font_data_id: u64,
    pub(crate) font_index: u32,
    pub(crate) normalized_coords_hash: u64,
    pub(crate) font_size: f32,
    pub(crate) font_weight: u16,
    pub(crate) glyph_indices: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcInlineBoxPlacement {
    pub(crate) id: u64,
    pub(crate) source: InlineIfcSourceId,
    pub(crate) line_index: usize,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcHitTestResult {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) target: InlineIfcHitTarget,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum InlineIfcHitTarget {
    Text {
        source: InlineIfcSourceId,
        byte_index: usize,
        line_index: usize,
        style: Option<InlineIfcStyle>,
    },
    InlineBox {
        source: InlineIfcSourceId,
        id: u64,
        line_index: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum InlineIfcCaretAffinity {
    Downstream,
    Upstream,
}

impl InlineIfcCaretAffinity {
    fn to_parley(self) -> Affinity {
        match self {
            Self::Downstream => Affinity::Downstream,
            Self::Upstream => Affinity::Upstream,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcCaretGeometry {
    pub(crate) source: InlineIfcSourceId,
    pub(crate) byte_index: usize,
    pub(crate) affinity: InlineIfcCaretAffinity,
    pub(crate) line_index: usize,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) height: f32,
    pub(crate) style: Option<InlineIfcStyle>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcCaretStop {
    pub(crate) source: InlineIfcSourceId,
    pub(crate) byte_index: usize,
    pub(crate) affinity: InlineIfcCaretAffinity,
    pub(crate) line_index: usize,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) height: f32,
    pub(crate) style: Option<InlineIfcStyle>,
    pub(crate) is_line_head: bool,
    pub(crate) is_line_tail: bool,
    pub(crate) is_soft_wrap_boundary: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIfcSelectionRect {
    pub(crate) line_index: usize,
    pub(crate) source: InlineIfcSourceId,
    pub(crate) range: Range<usize>,
    pub(crate) rect: InlineIfcPaintRect,
    pub(crate) style: Option<InlineIfcStyle>,
}

pub(crate) struct InlineFormattingContext {
    backing_text: String,
    layout: ParleyLayout<[u8; 4]>,
    source_ranges: Vec<InlineIfcSourceRange>,
    style_ranges: Vec<InlineIfcStyleRange>,
    inline_boxes: Vec<InlineIfcBoxMapping>,
    cache_key: InlineIfcCacheKey,
    // Memoized derivations: walking the parley layout (cluster/source
    // resolution, font lookups) is the dominant per-frame cost for inline
    // roots. The context is immutable once built and cached, so the glyph
    // list and the snapshot are computed lazily once and reused.
    glyph_items_cache: std::cell::OnceCell<Vec<InlineIfcGlyphItem>>,
    snapshot_cache: std::cell::OnceCell<InlineIfcTextLayoutSnapshot>,
    // Visual caret stops run two parley cursor queries per glyph, and the
    // geometry builder asks for them once per text source. Memoize so the
    // whole-layout stop set is built once and filtered per source.
    caret_stops_cache: std::cell::OnceCell<Vec<InlineIfcCaretStop>>,
    // The text-pass paint payload is consumed every paint frame by the
    // unified glyph passes; memoize so steady-state repaints borrow
    // instead of re-materializing per-glyph vectors.
    paint_input_cache: std::cell::OnceCell<InlineIfcTextPassPaintInput>,
}

impl InlineFormattingContext {
    pub(crate) fn build(input: InlineIfcInput) -> Self {
        let layout_options = InlineIfcLayoutOptions::from_input(&input);
        Self::build_with_options(input, layout_options)
    }

    pub(crate) fn build_with_options(
        input: InlineIfcInput,
        layout_options: InlineIfcLayoutOptions,
    ) -> Self {
        let cache_key = input.cache_key_with_layout_options(layout_options);
        let mut builder = InlineIfcBuilder::new();
        builder.push_items(&input.items, &input.default_style, 0);

        let layout = build_parley_layout(
            &builder.backing_text,
            &input.default_style,
            &builder.style_ranges,
            &builder.inline_boxes,
            layout_options,
        );

        Self {
            backing_text: builder.backing_text,
            layout,
            source_ranges: builder.source_ranges,
            style_ranges: builder.style_ranges,
            inline_boxes: builder.inline_boxes,
            cache_key,
            glyph_items_cache: std::cell::OnceCell::new(),
            snapshot_cache: std::cell::OnceCell::new(),
            caret_stops_cache: std::cell::OnceCell::new(),
            paint_input_cache: std::cell::OnceCell::new(),
        }
    }

    pub(crate) fn backing_text(&self) -> &str {
        &self.backing_text
    }

    pub(crate) fn source_ranges(&self) -> &[InlineIfcSourceRange] {
        &self.source_ranges
    }

    pub(crate) fn style_ranges(&self) -> &[InlineIfcStyleRange] {
        &self.style_ranges
    }

    pub(crate) fn inline_boxes(&self) -> &[InlineIfcBoxMapping] {
        &self.inline_boxes
    }

    pub(crate) fn cache_key(&self) -> &InlineIfcCacheKey {
        &self.cache_key
    }

    pub(crate) fn source_for_byte(&self, byte_index: usize) -> Option<InlineIfcSourceId> {
        self.source_ranges
            .iter()
            .filter(|range| range.range.start <= byte_index && byte_index < range.range.end)
            .max_by_key(|range| range.depth)
            .map(|range| range.source)
    }

    pub(crate) fn style_at_byte(&self, byte_index: usize) -> Option<&InlineIfcStyle> {
        self.style_ranges
            .iter()
            .rev()
            .find(|range| range.range.start <= byte_index && byte_index < range.range.end)
            .map(|range| &range.style)
    }

    pub(crate) fn source_for_inline_box(&self, id: u64) -> Option<InlineIfcSourceId> {
        self.inline_boxes
            .iter()
            .find(|inline_box| inline_box.id == id)
            .map(|inline_box| inline_box.source)
    }

    pub(crate) fn line_fragments(&self) -> Vec<InlineIfcLineFragment> {
        self.decoration_fragments()
            .into_iter()
            .map(|fragment| InlineIfcLineFragment {
                line_index: fragment.line_index,
                source: fragment.source,
                range: fragment.range,
                x0: fragment.x0,
                x1: fragment.x1,
                y0: fragment.y0,
                y1: fragment.y1,
            })
            .collect()
    }

    pub(crate) fn decoration_fragments(&self) -> Vec<InlineIfcDecorationFragment> {
        let mut fragments = Vec::new();
        for (line_index, line) in self.layout.lines().enumerate() {
            let line_range = line.text_range();
            let metrics = line.metrics();
            for source in self
                .source_ranges
                .iter()
                .filter(|source| source.kind == InlineIfcSourceKind::Span)
            {
                let start = source.range.start.max(line_range.start);
                let end = source.range.end.min(line_range.end);
                if start >= end {
                    continue;
                }

                let start_cursor =
                    ParleyCursor::from_byte_index(&self.layout, start, Affinity::Downstream)
                        .geometry(&self.layout, 0.0);
                let end_cursor =
                    ParleyCursor::from_byte_index(&self.layout, end, Affinity::Upstream)
                        .geometry(&self.layout, 0.0);
                fragments.push(InlineIfcDecorationFragment {
                    line_index,
                    source: source.source,
                    range: start..end,
                    style: source
                        .style
                        .clone()
                        .or_else(|| self.style_at_byte(start).cloned()),
                    x0: (start_cursor.x0 as f32).min(end_cursor.x0 as f32),
                    x1: (start_cursor.x0 as f32).max(end_cursor.x0 as f32),
                    y0: metrics.block_min_coord,
                    y1: metrics.block_min_coord + metrics.line_height,
                });
            }
        }
        fragments
    }

    pub(crate) fn glyph_items(&self) -> Vec<InlineIfcGlyphItem> {
        self.glyph_items_ref().to_vec()
    }

    /// Memoized glyph list. Built once per shaped context; every caller
    /// after the first borrows instead of re-walking the parley layout.
    pub(crate) fn glyph_items_ref(&self) -> &[InlineIfcGlyphItem] {
        self.glyph_items_cache
            .get_or_init(|| self.compute_glyph_items())
    }

    fn compute_glyph_items(&self) -> Vec<InlineIfcGlyphItem> {
        let mut output = Vec::new();
        for (line_index, line) in self.layout.lines().enumerate() {
            let mut consumed_glyphs_by_run = HashMap::<usize, usize>::new();
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };

                let run = glyph_run.run();
                let font = run.font().clone();
                let normalized_coords = run.normalized_coords();
                let normalized_coords_hash = if normalized_coords.is_empty() {
                    0
                } else {
                    hash_inline_ifc_value(&normalized_coords)
                };
                let run_index = run.index();
                let glyph_count = glyph_run.glyphs().count();
                let start_glyph = *consumed_glyphs_by_run.get(&run_index).unwrap_or(&0);
                consumed_glyphs_by_run.insert(run_index, start_glyph + glyph_count);
                let cluster_ranges = run
                    .visual_clusters()
                    .flat_map(|cluster| {
                        let range = cluster.text_range();
                        cluster.glyphs().map(move |_| range.clone())
                    })
                    .skip(start_glyph)
                    .take(glyph_count);

                for (glyph, cluster_range) in glyph_run.positioned_glyphs().zip(cluster_ranges) {
                    let Some(source) = self.source_for_byte(cluster_range.start) else {
                        continue;
                    };
                    let Some(style) = self.style_at_byte(cluster_range.start) else {
                        continue;
                    };
                    output.push(InlineIfcGlyphItem {
                        line_index,
                        run_index,
                        source,
                        cluster_range,
                        glyph_id: glyph.id,
                        x: glyph.x,
                        y: glyph.y,
                        advance: glyph.advance,
                        font_size: run.font_size(),
                        font_data: Some(font.clone()),
                        font_data_id: font.data.id(),
                        font_index: font.index,
                        normalized_coords_hash,
                        style: style.clone(),
                    });
                }
            }
        }
        output
    }

    pub(crate) fn glyph_groups(&self) -> Vec<InlineIfcGlyphGroup> {
        let mut groups = Vec::<InlineIfcGlyphGroup>::new();
        for glyph in self.glyph_items_ref().iter().cloned() {
            if let Some(group) = groups.last_mut() {
                if group.line_index == glyph.line_index
                    && group.source == glyph.source
                    && group.style == glyph.style
                    && group.range.end == glyph.cluster_range.start
                {
                    group.range.end = glyph.cluster_range.end;
                    group.glyphs.push(glyph);
                    continue;
                }
            }

            groups.push(InlineIfcGlyphGroup {
                line_index: glyph.line_index,
                source: glyph.source,
                range: glyph.cluster_range.clone(),
                style: glyph.style.clone(),
                glyphs: vec![glyph],
            });
        }
        groups
    }

    pub(crate) fn text_paint_output(&self) -> InlineIfcTextPaintOutput {
        let glyphs = self.text_paint_glyphs();
        let runs = text_paint_runs_from_glyphs(glyphs.clone());
        let decorations = self.decoration_paint_fragments();
        InlineIfcTextPaintOutput {
            glyphs,
            runs,
            decorations,
        }
    }

    pub(crate) fn text_layout_snapshot(&self) -> InlineIfcTextLayoutSnapshot {
        self.text_layout_snapshot_ref().clone()
    }

    /// Memoized layout snapshot. Built once per shaped context.
    pub(crate) fn text_layout_snapshot_ref(&self) -> &InlineIfcTextLayoutSnapshot {
        self.snapshot_cache
            .get_or_init(|| self.compute_text_layout_snapshot())
    }

    fn compute_text_layout_snapshot(&self) -> InlineIfcTextLayoutSnapshot {
        let paint_glyphs = self.text_paint_glyphs();
        let inline_boxes = self.inline_box_placements();
        let decorations = self.decoration_paint_fragments();
        let mut lines = Vec::new();

        for (line_index, line) in self.layout.lines().enumerate() {
            let metrics = line.metrics();
            let range = line.text_range();
            let glyphs = paint_glyphs
                .iter()
                .filter(|glyph| glyph.line_index == line_index)
                .map(|glyph| InlineIfcTextGlyph {
                    source: glyph.source,
                    cluster_range: glyph.cluster_range.clone(),
                    glyph_id: glyph.glyph_id,
                    x: glyph.x,
                    y: glyph.y,
                    advance: glyph.advance,
                    font_size: glyph.font_size,
                    font_data: glyph.font_data.clone(),
                    font_data_id: glyph.font_data_id,
                    font_index: glyph.font_index,
                    normalized_coords_hash: glyph.normalized_coords_hash,
                    style: glyph.style.clone(),
                    batch_key: glyph.batch_key,
                })
                .collect();

            lines.push(InlineIfcTextLine {
                line_index,
                x: metrics.offset + metrics.inline_min_coord,
                y: metrics.block_min_coord,
                width: (metrics.inline_max_coord - metrics.inline_min_coord).max(0.0),
                height: metrics.line_height,
                baseline: (metrics.baseline - metrics.block_min_coord).max(0.0),
                range,
                glyphs,
            });
        }

        InlineIfcTextLayoutSnapshot {
            lines,
            inline_boxes,
            decorations,
        }
    }

    pub(crate) fn text_pass_paint_input(&self) -> InlineIfcTextPassPaintInput {
        self.text_pass_paint_input_ref().clone()
    }

    /// Memoized paint payload. Built once per shaped context; every paint
    /// frame after the first borrows instead of rebuilding.
    pub(crate) fn text_pass_paint_input_ref(&self) -> &InlineIfcTextPassPaintInput {
        self.paint_input_cache
            .get_or_init(|| self.text_layout_snapshot_ref().text_pass_paint_input())
    }

    /// Measured content size matching the legacy text engine's formula:
    /// max glyph right edge across lines by max line bottom, floored at
    /// 1.0 per axis; (1.0, 1.0) when nothing shaped.
    pub(crate) fn measure_content_size(&self) -> (f32, f32) {
        let snapshot = self.text_layout_snapshot_ref();
        let mut max_width = 0.0f32;
        let mut max_bottom = 0.0f32;
        let mut line_count = 0usize;
        for line in &snapshot.lines {
            line_count += 1;
            let glyph_right = line
                .glyphs
                .iter()
                .map(|glyph| glyph.x + glyph.advance.max(0.0))
                .fold(0.0f32, f32::max);
            max_width = max_width.max(glyph_right);
            max_bottom = max_bottom.max(line.y + line.height);
        }
        if line_count == 0 {
            return (1.0, 1.0);
        }
        (max_width.max(1.0), max_bottom.max(1.0))
    }

    pub(crate) fn text_paint_glyphs(&self) -> Vec<InlineIfcPaintGlyph> {
        self.glyph_items_ref()
            .iter()
            .cloned()
            .map(InlineIfcPaintGlyph::from)
            .collect()
    }

    pub(crate) fn text_paint_runs(&self) -> Vec<InlineIfcTextPaintRun> {
        text_paint_runs_from_glyphs(self.text_paint_glyphs())
    }

    pub(crate) fn decoration_paint_fragments(&self) -> Vec<InlineIfcDecorationPaintFragment> {
        self.decoration_fragments()
            .into_iter()
            .map(|fragment| InlineIfcDecorationPaintFragment {
                line_index: fragment.line_index,
                source: fragment.source,
                range: fragment.range,
                rect: InlineIfcPaintRect {
                    x: fragment.x0,
                    y: fragment.y0,
                    width: (fragment.x1 - fragment.x0).max(0.0),
                    height: (fragment.y1 - fragment.y0).max(0.0),
                },
                style: fragment.style,
            })
            .collect()
    }

    pub(crate) fn element_decoration_paint_fragments(
        &self,
        source: InlineIfcSourceId,
        insets: InlineIfcDecorationBoxInsets,
    ) -> Vec<InlineIfcElementDecorationPaintFragment> {
        element_decoration_paint_fragments_for_source(
            self.decoration_paint_fragments()
                .into_iter()
                .filter(|fragment| fragment.source == source),
            source,
            insets,
        )
    }

    pub(crate) fn element_decoration_draw_rect_package(
        &self,
        source: InlineIfcSourceId,
        insets: InlineIfcDecorationBoxInsets,
        style: InlineIfcElementDecorationDrawRectStyle,
    ) -> InlineIfcElementDecorationDrawRectPackage {
        element_decoration_draw_rect_package_for_source(
            self.decoration_paint_fragments()
                .into_iter()
                .filter(|fragment| fragment.source == source),
            source,
            insets,
            style,
        )
    }

    pub(crate) fn inline_box_placements(&self) -> Vec<InlineIfcInlineBoxPlacement> {
        let mut placements = Vec::new();
        for (line_index, line) in self.layout.lines().enumerate() {
            for item in line.items() {
                if let PositionedLayoutItem::InlineBox(inline_box) = item {
                    if let Some(mapping) = self
                        .inline_boxes
                        .iter()
                        .find(|mapping| mapping.id == inline_box.id)
                    {
                        placements.push(InlineIfcInlineBoxPlacement {
                            id: inline_box.id,
                            source: mapping.source,
                            line_index,
                            x: inline_box.x,
                            y: inline_box.y,
                            width: inline_box.width,
                            height: inline_box.height,
                        });
                    }
                }
            }
        }
        placements
    }

    pub(crate) fn atomic_box_placement_package(
        &self,
        source: InlineIfcSourceId,
    ) -> InlineIfcAtomicBoxPlacementPackage {
        let mut placements = Vec::new();
        for (line_index, line) in self.layout.lines().enumerate() {
            for item in line.items() {
                let PositionedLayoutItem::InlineBox(inline_box) = item else {
                    continue;
                };
                let Some(mapping) = self
                    .inline_boxes
                    .iter()
                    .find(|mapping| mapping.id == inline_box.id && mapping.source == source)
                else {
                    continue;
                };
                placements.push(InlineIfcAtomicBoxPlacement {
                    id: inline_box.id,
                    source: mapping.source,
                    insertion_byte: mapping.insertion_byte,
                    line_index,
                    rect: InlineIfcPaintRect {
                        x: inline_box.x,
                        y: inline_box.y,
                        width: inline_box.width,
                        height: inline_box.height,
                    },
                    measurement: mapping.measurement.clone(),
                });
            }
        }
        InlineIfcAtomicBoxPlacementPackage { source, placements }
    }

    /// Per-line bounding rects of the glyph runs contributed by `source`,
    /// in IFC content coordinates. Used as the hit-test/selection geometry
    /// for text owned by a unified inline IFC root.
    pub(crate) fn source_line_rects(&self, source: InlineIfcSourceId) -> Vec<InlineIfcPaintRect> {
        let snapshot = self.text_layout_snapshot_ref();
        let mut rects = Vec::new();
        for line in &snapshot.lines {
            let mut left: Option<f32> = None;
            let mut right: Option<f32> = None;
            for glyph in line.glyphs.iter().filter(|glyph| glyph.source == source) {
                let start = glyph.x;
                left = Some(left.map_or(start, |current| current.min(start)));
                right = Some(right.map_or(start + glyph.advance, |current| {
                    current.max(start + glyph.advance)
                }));
            }
            let (Some(left), Some(right)) = (left, right) else {
                continue;
            };
            if right <= left {
                continue;
            }
            rects.push(InlineIfcPaintRect {
                x: left,
                y: line.y,
                width: right - left,
                height: line.height,
            });
        }
        rects
    }

    /// Per-line rects of `source`'s text where `y`/`height` track the text
    /// box (parley run ascent/descent above and below the baseline) rather
    /// than the full line box, so a Text node owned by a unified root
    /// reports the position its glyphs actually paint at. Returns
    /// `(line_index, rect)` pairs in IFC content coordinates.
    pub(crate) fn source_text_line_rects(
        &self,
        source: InlineIfcSourceId,
    ) -> Vec<(usize, InlineIfcPaintRect)> {
        // Map run index → (ascent, descent) so we can size the text box
        // off the actual font metrics parley used for each run.
        let mut run_metrics: HashMap<usize, (f32, f32)> = HashMap::new();
        for line in self.layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let metrics = run.metrics();
                run_metrics.insert(run.index(), (metrics.ascent, metrics.descent));
            }
        }

        // glyph_items carries the per-glyph source (innermost, nesting-aware)
        // and run index, plus the baseline-relative line geometry.
        let glyphs = self.glyph_items_ref();
        // Precompute per-line baseline and inline offset so the glyph loop
        // is O(glyphs), not O(glyphs × lines).
        let line_baselines: Vec<f32> = self
            .layout
            .lines()
            .map(|line| line.metrics().baseline)
            .collect();
        let mut by_line: std::collections::BTreeMap<usize, (f32, f32, f32, f32)> =
            std::collections::BTreeMap::new();
        for glyph in glyphs.iter().filter(|glyph| glyph.source == source) {
            // Positioned glyph x is absolute (alignment offset included).
            let left = glyph.x;
            let right = left + glyph.advance;
            let (ascent, descent) = run_metrics
                .get(&glyph.run_index)
                .copied()
                .unwrap_or((glyph.font_size * 0.88, glyph.font_size * 0.2));
            let entry = by_line
                .entry(glyph.line_index)
                .or_insert((f32::MAX, f32::MIN, 0.0, 0.0));
            entry.0 = entry.0.min(left);
            entry.1 = entry.1.max(right);
            entry.2 = entry.2.max(ascent);
            entry.3 = entry.3.max(descent);
        }

        by_line
            .into_iter()
            .filter_map(|(line_index, (left, right, ascent, descent))| {
                if right <= left {
                    return None;
                }
                let baseline = line_baselines.get(line_index).copied()?;
                Some((
                    line_index,
                    InlineIfcPaintRect {
                        x: left,
                        y: baseline - ascent,
                        width: right - left,
                        height: (ascent + descent).max(1.0),
                    },
                ))
            })
            .collect()
    }

    pub(crate) fn text_top_for_line_range(
        &self,
        line_index: usize,
        range: &Range<usize>,
    ) -> Option<f32> {
        let mut run_metrics: HashMap<usize, (f32, f32)> = HashMap::new();
        for line in self.layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let metrics = run.metrics();
                run_metrics.insert(run.index(), (metrics.ascent, metrics.descent));
            }
        }

        let baseline = self
            .layout
            .lines()
            .nth(line_index)
            .map(|line| line.metrics().baseline)?;
        let ascent = self
            .glyph_items_ref()
            .iter()
            .filter(|glyph| {
                glyph.line_index == line_index
                    && glyph.cluster_range.start < range.end
                    && glyph.cluster_range.end > range.start
            })
            .filter_map(|glyph| run_metrics.get(&glyph.run_index).map(|(ascent, _)| *ascent))
            .fold(None, |current: Option<f32>, ascent| {
                Some(current.map_or(ascent, |current| current.max(ascent)))
            })?;

        Some(baseline - ascent)
    }

    pub(crate) fn element_package_distributor(
        &self,
        input: InlineIfcElementPackageDistributionInput,
    ) -> InlineIfcElementPackageDistributor {
        let decoration_fragments = self.decoration_paint_fragments();
        let mut packages_by_source =
            HashMap::<InlineIfcSourceId, InlineIfcDistributedElementPackages>::new();

        for source in input.decoration_sources {
            let package = element_decoration_draw_rect_package_for_source(
                decoration_fragments
                    .iter()
                    .filter(|fragment| fragment.source == source.source)
                    .cloned(),
                source.source,
                source.slice_insets,
                source.draw_rect_style,
            );
            if package.fragments.is_empty() {
                continue;
            }
            packages_by_source
                .entry(source.source)
                .or_insert_with(|| InlineIfcDistributedElementPackages::new(source.source))
                .decoration_draw_rect = Some(package);
        }

        for source in input.atomic_sources {
            let package = self.atomic_box_placement_package(source);
            if package.placements.is_empty() {
                continue;
            }
            packages_by_source
                .entry(source)
                .or_insert_with(|| InlineIfcDistributedElementPackages::new(source))
                .atomic_placement = Some(package);
        }

        packages_by_source.retain(|_, package| !package.is_empty());
        InlineIfcElementPackageDistributor { packages_by_source }
    }

    pub(crate) fn hit_test_point(&self, x: f32, y: f32) -> Option<InlineIfcHitTestResult> {
        for placement in self.inline_box_placements() {
            if point_in_rect(
                x,
                y,
                placement.x,
                placement.y,
                placement.width,
                placement.height,
            ) {
                return Some(InlineIfcHitTestResult {
                    x,
                    y,
                    target: InlineIfcHitTarget::InlineBox {
                        source: placement.source,
                        id: placement.id,
                        line_index: placement.line_index,
                    },
                });
            }
        }

        let byte_index = clamp_utf8_boundary(
            &self.backing_text,
            ParleyCursor::from_point(&self.layout, x, y).index(),
        );
        let caret = self.caret_geometry_for_byte(byte_index, InlineIfcCaretAffinity::Downstream)?;
        Some(InlineIfcHitTestResult {
            x,
            y,
            target: InlineIfcHitTarget::Text {
                source: caret.source,
                byte_index,
                line_index: caret.line_index,
                style: caret.style,
            },
        })
    }

    pub(crate) fn caret_geometry_for_byte(
        &self,
        byte_index: usize,
        affinity: InlineIfcCaretAffinity,
    ) -> Option<InlineIfcCaretGeometry> {
        let byte_index = clamp_utf8_boundary(&self.backing_text, byte_index);
        let cursor = ParleyCursor::from_byte_index(&self.layout, byte_index, affinity.to_parley());
        let rect = cursor.geometry(&self.layout, 0.0);
        let line_index = self.line_index_for_cursor_y(rect.y0 as f32)?;
        let source = self.source_for_caret_byte(byte_index)?;
        Some(InlineIfcCaretGeometry {
            source,
            byte_index,
            affinity,
            line_index,
            x: rect.x0 as f32,
            y: rect.y0 as f32,
            height: (rect.y1 - rect.y0).max(1.0) as f32,
            style: self.style_for_caret_byte(byte_index).cloned(),
        })
    }

    pub(crate) fn visual_caret_stops(&self) -> Vec<InlineIfcCaretStop> {
        self.visual_caret_stops_ref().to_vec()
    }

    /// Memoized whole-layout caret stops (built once per shaped context).
    pub(crate) fn visual_caret_stops_ref(&self) -> &[InlineIfcCaretStop] {
        self.caret_stops_cache
            .get_or_init(|| self.compute_visual_caret_stops())
    }

    fn compute_visual_caret_stops(&self) -> Vec<InlineIfcCaretStop> {
        let mut stops = Vec::<InlineIfcCaretStop>::new();
        let glyphs = self.glyph_items_ref();
        let inline_box_placements = self.inline_box_placements();
        let line_text_ranges = self
            .layout
            .lines()
            .map(|line| line.text_range())
            .collect::<Vec<_>>();
        for (line_index, line) in self.layout.lines().enumerate() {
            let line_range = line.text_range();
            if line_range.is_empty() {
                self.push_empty_line_visual_caret_stops(&mut stops, line_index, line_range.start);
                for placement in inline_box_placements
                    .iter()
                    .filter(|placement| placement.line_index == line_index)
                {
                    self.push_inline_box_visual_caret_stop(&mut stops, placement, true, false);
                    self.push_inline_box_visual_caret_stop(&mut stops, placement, false, true);
                }
                continue;
            }
            let is_soft_wrap = line_text_ranges
                .get(line_index + 1)
                .is_some_and(|next_range| next_range.start == line_range.end);
            self.push_visual_caret_stop(
                &mut stops,
                line_index,
                line_range.start,
                InlineIfcCaretAffinity::Downstream,
                true,
                false,
                false,
            );

            for glyph in glyphs.iter().filter(|glyph| glyph.line_index == line_index) {
                self.push_visual_caret_stop(
                    &mut stops,
                    line_index,
                    glyph.cluster_range.start,
                    InlineIfcCaretAffinity::Downstream,
                    glyph.cluster_range.start == line_range.start,
                    false,
                    false,
                );
                self.push_visual_caret_stop(
                    &mut stops,
                    line_index,
                    glyph.cluster_range.end,
                    InlineIfcCaretAffinity::Upstream,
                    false,
                    glyph.cluster_range.end == line_range.end,
                    false,
                );
            }

            self.push_visual_caret_stop(
                &mut stops,
                line_index,
                line_range.end,
                InlineIfcCaretAffinity::Upstream,
                false,
                true,
                is_soft_wrap,
            );
            if is_soft_wrap {
                self.push_visual_caret_stop(
                    &mut stops,
                    line_index + 1,
                    line_range.end,
                    InlineIfcCaretAffinity::Downstream,
                    true,
                    false,
                    true,
                );
                if line_text_ranges
                    .get(line_index + 1)
                    .is_some_and(|next_range| next_range.is_empty())
                {
                    self.push_empty_line_visual_caret_stops(
                        &mut stops,
                        line_index + 1,
                        line_range.end,
                    );
                }
            }
        }
        let line_count = line_text_ranges.len();
        for line_index in 0..line_count {
            if stops
                .iter()
                .any(|stop| stop.line_index == line_index && stop.is_line_tail)
            {
                continue;
            }
            if let Some(head) = stops
                .iter()
                .find(|stop| stop.line_index == line_index && stop.is_line_head)
                .cloned()
            {
                stops.push(InlineIfcCaretStop {
                    affinity: InlineIfcCaretAffinity::Upstream,
                    is_line_head: false,
                    is_line_tail: true,
                    is_soft_wrap_boundary: false,
                    ..head
                });
            }
        }
        stops
    }

    pub(crate) fn selection_rects_for_global_range(
        &self,
        range: Range<usize>,
    ) -> Vec<InlineIfcSelectionRect> {
        let start = clamp_utf8_boundary(&self.backing_text, range.start.min(range.end));
        let end = clamp_utf8_boundary(&self.backing_text, range.start.max(range.end));
        if start == end {
            return Vec::new();
        }
        self.selection_rects_for_range_and_source(start..end, None)
    }

    pub(crate) fn selection_rects_for_source_range(
        &self,
        source: InlineIfcSourceId,
        range: Range<usize>,
    ) -> Vec<InlineIfcSelectionRect> {
        let start = clamp_utf8_boundary(&self.backing_text, range.start.min(range.end));
        let end = clamp_utf8_boundary(&self.backing_text, range.start.max(range.end));
        if start == end {
            return Vec::new();
        }
        self.selection_rects_for_range_and_source(start..end, Some(source))
    }

    fn selection_rects_for_range_and_source(
        &self,
        range: Range<usize>,
        source_filter: Option<InlineIfcSourceId>,
    ) -> Vec<InlineIfcSelectionRect> {
        let mut rects = Vec::new();
        for (line_index, line) in self.layout.lines().enumerate() {
            let line_range = line.text_range();
            let metrics = line.metrics();
            let line_start = range.start.max(line_range.start);
            let line_end = range.end.min(line_range.end);
            if line_start >= line_end {
                continue;
            }

            for source_range in self.source_ranges.iter().filter(|source_range| {
                source_range.kind == InlineIfcSourceKind::Text
                    && source_filter.is_none_or(|source| source == source_range.source)
            }) {
                let start = line_start.max(source_range.range.start);
                let end = line_end.min(source_range.range.end);
                if start >= end {
                    continue;
                }

                let start_cursor =
                    ParleyCursor::from_byte_index(&self.layout, start, Affinity::Downstream)
                        .geometry(&self.layout, 0.0);
                let end_cursor =
                    ParleyCursor::from_byte_index(&self.layout, end, Affinity::Upstream)
                        .geometry(&self.layout, 0.0);
                let left = (start_cursor.x0 as f32).min(end_cursor.x0 as f32);
                let right = (start_cursor.x0 as f32).max(end_cursor.x0 as f32);
                rects.push(InlineIfcSelectionRect {
                    line_index,
                    source: source_range.source,
                    range: start..end,
                    rect: InlineIfcPaintRect {
                        x: left,
                        y: metrics.block_min_coord,
                        width: (right - left).max(1.0),
                        height: metrics.line_height.max(1.0),
                    },
                    style: self.style_at_byte(start).cloned(),
                });
            }
        }
        rects
    }

    fn push_visual_caret_stop(
        &self,
        stops: &mut Vec<InlineIfcCaretStop>,
        expected_line_index: usize,
        byte_index: usize,
        affinity: InlineIfcCaretAffinity,
        is_line_head: bool,
        is_line_tail: bool,
        is_soft_wrap_boundary: bool,
    ) {
        let Some(caret) = self.caret_geometry_for_byte(byte_index, affinity) else {
            return;
        };
        let line_index = if is_soft_wrap_boundary {
            expected_line_index
        } else {
            caret.line_index
        };
        if !is_soft_wrap_boundary && line_index != expected_line_index {
            return;
        }
        if stops.iter().any(|stop| {
            stop.byte_index == caret.byte_index
                && stop.affinity == affinity
                && stop.line_index == line_index
                && stop.is_line_head == is_line_head
                && stop.is_line_tail == is_line_tail
                && stop.is_soft_wrap_boundary == is_soft_wrap_boundary
        }) {
            return;
        }
        stops.push(InlineIfcCaretStop {
            source: caret.source,
            byte_index: caret.byte_index,
            affinity,
            line_index,
            x: caret.x,
            y: caret.y,
            height: caret.height,
            style: caret.style,
            is_line_head,
            is_line_tail,
            is_soft_wrap_boundary,
        });
    }

    fn push_empty_line_visual_caret_stops(
        &self,
        stops: &mut Vec<InlineIfcCaretStop>,
        line_index: usize,
        byte_index: usize,
    ) {
        let source = self.source_for_caret_byte(byte_index);
        let style = self.style_for_caret_byte(byte_index).cloned();
        let Some(source) = source else {
            return;
        };
        let Some(line) = self.layout.lines().nth(line_index) else {
            return;
        };
        let metrics = line.metrics();
        for (affinity, is_line_head, is_line_tail, x) in [
            (
                InlineIfcCaretAffinity::Downstream,
                true,
                false,
                metrics.inline_min_coord,
            ),
            (
                InlineIfcCaretAffinity::Upstream,
                false,
                true,
                metrics.inline_max_coord,
            ),
        ] {
            if stops.iter().any(|stop| {
                stop.byte_index == byte_index
                    && stop.affinity == affinity
                    && stop.line_index == line_index
                    && stop.is_line_head == is_line_head
                    && stop.is_line_tail == is_line_tail
            }) {
                continue;
            }
            stops.push(InlineIfcCaretStop {
                source,
                byte_index,
                affinity,
                line_index,
                x,
                y: metrics.block_min_coord,
                height: metrics.line_height.max(1.0),
                style: style.clone(),
                is_line_head,
                is_line_tail,
                is_soft_wrap_boundary: false,
            });
        }
    }

    fn push_inline_box_visual_caret_stop(
        &self,
        stops: &mut Vec<InlineIfcCaretStop>,
        placement: &InlineIfcInlineBoxPlacement,
        is_line_head: bool,
        is_line_tail: bool,
    ) {
        let Some(mapping) = self
            .inline_boxes
            .iter()
            .find(|mapping| mapping.id == placement.id)
        else {
            return;
        };
        let affinity = if is_line_head {
            InlineIfcCaretAffinity::Downstream
        } else {
            InlineIfcCaretAffinity::Upstream
        };
        if stops.iter().any(|stop| {
            stop.byte_index == mapping.insertion_byte
                && stop.affinity == affinity
                && stop.line_index == placement.line_index
                && stop.is_line_head == is_line_head
                && stop.is_line_tail == is_line_tail
        }) {
            return;
        }
        stops.push(InlineIfcCaretStop {
            source: mapping.source,
            byte_index: mapping.insertion_byte,
            affinity,
            line_index: placement.line_index,
            x: if is_line_head {
                placement.x
            } else {
                placement.x + placement.width
            },
            y: placement.y,
            height: placement.height.max(1.0),
            style: self.style_for_caret_byte(mapping.insertion_byte).cloned(),
            is_line_head,
            is_line_tail,
            is_soft_wrap_boundary: false,
        });
    }

    fn source_for_caret_byte(&self, byte_index: usize) -> Option<InlineIfcSourceId> {
        self.source_ranges
            .iter()
            .filter(|range| range.kind == InlineIfcSourceKind::Text)
            .filter(|range| {
                range.range.start <= byte_index
                    && (byte_index < range.range.end
                        || byte_index == range.range.end && range.range.start < range.range.end)
            })
            .max_by_key(|range| range.depth)
            .map(|range| range.source)
    }

    fn style_for_caret_byte(&self, byte_index: usize) -> Option<&InlineIfcStyle> {
        self.style_at_byte(byte_index).or_else(|| {
            byte_index
                .checked_sub(1)
                .and_then(|previous| self.style_at_byte(previous))
        })
    }

    fn line_index_for_cursor_y(&self, y: f32) -> Option<usize> {
        self.layout
            .lines()
            .enumerate()
            .find_map(|(line_index, line)| {
                let metrics = line.metrics();
                let y0 = metrics.block_min_coord;
                let y1 = metrics.block_min_coord + metrics.line_height;
                (y0 <= y && y <= y1).then_some(line_index)
            })
    }
}

fn point_in_rect(x: f32, y: f32, rect_x: f32, rect_y: f32, width: f32, height: f32) -> bool {
    rect_x <= x && x <= rect_x + width && rect_y <= y && y <= rect_y + height
}

fn clamp_utf8_boundary(value: &str, mut byte_index: usize) -> usize {
    byte_index = byte_index.min(value.len());
    while byte_index > 0 && !value.is_char_boundary(byte_index) {
        byte_index -= 1;
    }
    byte_index
}

fn f32_cache_bits(value: f32) -> u32 {
    if value == 0.0 {
        0.0f32.to_bits()
    } else {
        value.to_bits()
    }
}

fn hash_inline_ifc_value<T: std::hash::Hash + ?Sized>(value: &T) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(value, &mut hasher);
    hasher.finish()
}

pub(crate) fn element_decoration_paint_fragments_for_source<I>(
    fragments: I,
    source: InlineIfcSourceId,
    insets: InlineIfcDecorationBoxInsets,
) -> Vec<InlineIfcElementDecorationPaintFragment>
where
    I: IntoIterator<Item = InlineIfcDecorationPaintFragment>,
{
    let mut fragments = fragments
        .into_iter()
        .filter(|fragment| fragment.source == source)
        .collect::<Vec<_>>();
    fragments.sort_by(|a, b| {
        a.line_index
            .cmp(&b.line_index)
            .then(a.range.start.cmp(&b.range.start))
            .then(a.range.end.cmp(&b.range.end))
    });

    let last_index = fragments.len().saturating_sub(1);
    fragments
        .into_iter()
        .enumerate()
        .map(|(index, fragment)| {
            let is_first_for_source = index == 0;
            let is_last_for_source = index == last_index;
            let left = if is_first_for_source {
                insets.left
            } else {
                0.0
            };
            let right = if is_last_for_source {
                insets.right
            } else {
                0.0
            };
            InlineIfcElementDecorationPaintFragment {
                line_index: fragment.line_index,
                source: fragment.source,
                range: fragment.range,
                rect: InlineIfcPaintRect {
                    x: fragment.rect.x,
                    y: fragment.rect.y - insets.top,
                    width: fragment.rect.width + left + right,
                    height: fragment.rect.height + insets.top + insets.bottom,
                },
                is_first_for_source,
                is_last_for_source,
            }
        })
        .collect()
}

pub(crate) fn element_decoration_draw_rect_package_for_source<I>(
    fragments: I,
    source: InlineIfcSourceId,
    insets: InlineIfcDecorationBoxInsets,
    style: InlineIfcElementDecorationDrawRectStyle,
) -> InlineIfcElementDecorationDrawRectPackage
where
    I: IntoIterator<Item = InlineIfcDecorationPaintFragment>,
{
    let fragments = element_decoration_paint_fragments_for_source(fragments, source, insets)
        .into_iter()
        .map(|fragment| {
            let rect = fragment.rect;
            InlineIfcElementDecorationDrawRect {
                line_index: fragment.line_index,
                source: fragment.source,
                range: fragment.range,
                rect,
                style_key: style.style_key,
                slice_insets: insets,
                is_first_for_source: fragment.is_first_for_source,
                is_last_for_source: fragment.is_last_for_source,
                metadata: InlineIfcDrawRectMetadata {
                    position: [rect.x, rect.y],
                    size: [rect.width, rect.height],
                    fill_color: style.fill_color,
                    opacity: style.opacity,
                    border_widths: style.border_widths,
                    border_color: style.border_color,
                },
            }
        })
        .collect();

    InlineIfcElementDecorationDrawRectPackage {
        source,
        style_key: style.style_key,
        slice_insets: insets,
        fragments,
    }
}

fn content_key_items_for(
    items: &[InlineIfcItem],
    inherited_style: &InlineIfcStyle,
) -> Vec<InlineIfcContentKeyItem> {
    items
        .iter()
        .map(|item| content_key_item_for(item, inherited_style))
        .collect()
}

fn content_key_item_for(
    item: &InlineIfcItem,
    inherited_style: &InlineIfcStyle,
) -> InlineIfcContentKeyItem {
    match item {
        InlineIfcItem::TextSpan {
            source,
            text,
            style,
        } => {
            let resolved_style = style.as_ref().unwrap_or(inherited_style);
            InlineIfcContentKeyItem::Text {
                source: *source,
                text: text.clone(),
                shape_style: InlineIfcStyleKey::from_style(resolved_style),
            }
        }
        InlineIfcItem::Span {
            source,
            style,
            children,
        } => {
            let resolved_style = style.as_ref().unwrap_or(inherited_style);
            InlineIfcContentKeyItem::Span {
                source: *source,
                shape_style: InlineIfcStyleKey::from_style(resolved_style),
                children: content_key_items_for(children, resolved_style),
            }
        }
        InlineIfcItem::AtomicInlineBox {
            source,
            measurement,
        } => InlineIfcContentKeyItem::AtomicInlineBox {
            source: *source,
            shape_key: InlineIfcAtomicBoxShapeKey::from_measurement(measurement),
        },
    }
}

fn paint_key_items_for(
    items: &[InlineIfcItem],
    inherited_style: &InlineIfcStyle,
) -> Vec<InlineIfcPaintKeyItem> {
    items
        .iter()
        .map(|item| paint_key_item_for(item, inherited_style))
        .collect()
}

fn paint_key_item_for(
    item: &InlineIfcItem,
    inherited_style: &InlineIfcStyle,
) -> InlineIfcPaintKeyItem {
    match item {
        InlineIfcItem::TextSpan { source, style, .. } => {
            let resolved_style = style.as_ref().unwrap_or(inherited_style);
            InlineIfcPaintKeyItem::Text {
                source: *source,
                paint_style: InlineIfcPaintStyleKey::from_style(resolved_style),
            }
        }
        InlineIfcItem::Span {
            source,
            style,
            children,
        } => {
            let resolved_style = style.as_ref().unwrap_or(inherited_style);
            InlineIfcPaintKeyItem::Span {
                source: *source,
                paint_style: InlineIfcPaintStyleKey::from_style(resolved_style),
                children: paint_key_items_for(children, resolved_style),
            }
        }
        InlineIfcItem::AtomicInlineBox { source, .. } => {
            InlineIfcPaintKeyItem::AtomicInlineBox { source: *source }
        }
    }
}

fn text_paint_runs_from_glyphs(glyphs: Vec<InlineIfcPaintGlyph>) -> Vec<InlineIfcTextPaintRun> {
    let mut runs = Vec::<InlineIfcTextPaintRun>::new();
    for glyph in glyphs {
        if let Some(run) = runs.last_mut() {
            if run.line_index == glyph.line_index
                && run.source == glyph.source
                && run.batch_key == glyph.batch_key
                && run.style == glyph.style
                && run.range.end == glyph.cluster_range.start
            {
                run.range.end = glyph.cluster_range.end;
                run.glyphs.push(glyph);
                continue;
            }
        }

        runs.push(InlineIfcTextPaintRun {
            line_index: glyph.line_index,
            source: glyph.source,
            range: glyph.cluster_range.clone(),
            style: glyph.style.clone(),
            batch_key: glyph.batch_key,
            glyphs: vec![glyph],
        });
    }
    runs
}

fn text_pass_batches_from_glyphs(
    glyphs: &[InlineIfcTextPassGlyphInput],
) -> Vec<InlineIfcTextPassBatchInput> {
    let mut batches = Vec::<InlineIfcTextPassBatchInput>::new();
    for (glyph_index, glyph) in glyphs.iter().enumerate() {
        if let Some(batch) = batches
            .iter_mut()
            .find(|batch| batch.batch_key == glyph.batch_key)
        {
            batch.glyph_indices.push(glyph_index);
            continue;
        }

        batches.push(InlineIfcTextPassBatchInput {
            batch_key: glyph.batch_key,
            color: glyph.color,
            font_data_id: glyph.font_data_id,
            font_index: glyph.font_index,
            normalized_coords_hash: glyph.normalized_coords_hash,
            font_size: glyph.font_size,
            font_weight: glyph.style.font_weight,
            glyph_indices: vec![glyph_index],
        });
    }
    batches
}

fn brush_to_text_color(brush: [u8; 4]) -> [f32; 4] {
    [
        brush[0] as f32 / 255.0,
        brush[1] as f32 / 255.0,
        brush[2] as f32 / 255.0,
        brush[3] as f32 / 255.0,
    ]
}

struct InlineIfcBuilder {
    backing_text: String,
    source_ranges: Vec<InlineIfcSourceRange>,
    style_ranges: Vec<InlineIfcStyleRange>,
    inline_boxes: Vec<InlineIfcBoxMapping>,
    next_inline_box_id: u64,
}

impl InlineIfcBuilder {
    fn new() -> Self {
        Self {
            backing_text: String::new(),
            source_ranges: Vec::new(),
            style_ranges: Vec::new(),
            inline_boxes: Vec::new(),
            next_inline_box_id: 0,
        }
    }

    fn push_items(
        &mut self,
        items: &[InlineIfcItem],
        inherited_style: &InlineIfcStyle,
        depth: usize,
    ) {
        for item in items {
            self.push_item(item, inherited_style, depth);
        }
    }

    fn push_item(&mut self, item: &InlineIfcItem, inherited_style: &InlineIfcStyle, depth: usize) {
        match item {
            InlineIfcItem::TextSpan {
                source,
                text,
                style,
            } => {
                let start = self.backing_text.len();
                self.backing_text.push_str(text);
                let end = self.backing_text.len();
                if start == end {
                    return;
                }
                let resolved_style = style.clone().unwrap_or_else(|| inherited_style.clone());
                self.source_ranges.push(InlineIfcSourceRange {
                    source: *source,
                    kind: InlineIfcSourceKind::Text,
                    range: start..end,
                    depth,
                    style: Some(resolved_style.clone()),
                });
                self.style_ranges.push(InlineIfcStyleRange {
                    range: start..end,
                    style: resolved_style,
                });
            }
            InlineIfcItem::Span {
                source,
                style,
                children,
            } => {
                let resolved_style = style.clone().unwrap_or_else(|| inherited_style.clone());
                let start = self.backing_text.len();
                self.push_items(children, &resolved_style, depth + 1);
                let end = self.backing_text.len();
                if start < end {
                    self.source_ranges.push(InlineIfcSourceRange {
                        source: *source,
                        kind: InlineIfcSourceKind::Span,
                        range: start..end,
                        depth,
                        style: Some(resolved_style),
                    });
                }
            }
            InlineIfcItem::AtomicInlineBox {
                source,
                measurement,
            } => {
                let id = self.next_inline_box_id;
                self.next_inline_box_id += 1;
                self.inline_boxes.push(InlineIfcBoxMapping {
                    id,
                    source: *source,
                    insertion_byte: self.backing_text.len(),
                    measurement: measurement.clone(),
                });
            }
        }
    }
}

fn build_parley_layout(
    backing_text: &str,
    default_style: &InlineIfcStyle,
    style_ranges: &[InlineIfcStyleRange],
    inline_boxes: &[InlineIfcBoxMapping],
    layout_options: InlineIfcLayoutOptions,
) -> ParleyLayout<[u8; 4]> {
    with_shared_parley_context(|ctx| {
        let mut builder = ctx
            .layout
            .ranged_builder(&mut ctx.font, backing_text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(default_style.font_size.max(1.0)));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            default_style.line_height.max(0.1),
        )));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(
            default_style.font_weight as f32,
        )));
        builder.push_default(StyleProperty::TextWrapMode(if layout_options.allow_wrap {
            TextWrapMode::Wrap
        } else {
            TextWrapMode::NoWrap
        }));
        if layout_options.allow_wrap {
            builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::BreakWord));
        }
        builder.push_default(StyleProperty::FontFamily(parley_font_family(
            &default_style.font_families,
        )));
        builder.push_default(StyleProperty::Brush(default_style.brush));

        for style_range in style_ranges {
            let range = style_range.range.clone();
            if range.is_empty() {
                continue;
            }
            builder.push(
                StyleProperty::FontSize(style_range.style.font_size.max(1.0)),
                range.clone(),
            );
            builder.push(
                StyleProperty::LineHeight(LineHeight::FontSizeRelative(
                    style_range.style.line_height.max(0.1),
                )),
                range.clone(),
            );
            builder.push(
                StyleProperty::FontWeight(FontWeight::new(style_range.style.font_weight as f32)),
                range.clone(),
            );
            builder.push(
                StyleProperty::FontFamily(parley_font_family(&style_range.style.font_families)),
                range.clone(),
            );
            builder.push(StyleProperty::Brush(style_range.style.brush), range);
        }

        for inline_box in inline_boxes {
            builder.push_inline_box(InlineBox {
                id: inline_box.id,
                kind: InlineBoxKind::InFlow,
                index: inline_box.insertion_byte,
                width: inline_box.measurement.measured_size.width,
                height: inline_box.measurement.measured_size.height,
            });
        }

        let mut layout = builder.build(backing_text);
        // Legacy-compatible slack: measured content that fits within a
        // couple of float-error pixels of the constraint must not wrap.
        layout.break_all_lines(
            layout_options
                .max_width
                .map(|width| width + INLINE_IFC_WRAP_EPSILON),
        );
        layout.align(
            to_parley_alignment(layout_options.align),
            AlignmentOptions::default(),
        );
        layout
    })
}

const INLINE_IFC_WRAP_EPSILON: f32 = 2.0;

fn to_parley_alignment(align: InlineIfcAlignment) -> ParleyAlignment {
    match align {
        InlineIfcAlignment::Left => ParleyAlignment::Left,
        InlineIfcAlignment::Center => ParleyAlignment::Center,
        InlineIfcAlignment::Right => ParleyAlignment::Right,
    }
}

fn parley_font_family(font_families: &[String]) -> FontFamily<'_> {
    if font_families.is_empty() {
        return FontFamily::from("sans-serif");
    }

    let names = font_families
        .iter()
        .map(|family| {
            FontFamilyName::parse(family.as_str())
                .unwrap_or_else(|| FontFamilyName::named(family.as_str()))
        })
        .collect::<Vec<_>>();
    FontFamily::List(Cow::Owned(names))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: InlineIfcSourceId = InlineIfcSourceId(1);
    const OUTER: InlineIfcSourceId = InlineIfcSourceId(2);
    const INNER: InlineIfcSourceId = InlineIfcSourceId(3);
    const BOX_NODE: InlineIfcSourceId = InlineIfcSourceId(4);
    const SECOND_BOX_NODE: InlineIfcSourceId = InlineIfcSourceId(5);

    fn style(brush: [u8; 4], weight: u16) -> InlineIfcStyle {
        InlineIfcStyle {
            brush,
            font_weight: weight,
            ..InlineIfcStyle::default()
        }
    }

    fn style_with_size(brush: [u8; 4], weight: u16, font_size: f32) -> InlineIfcStyle {
        InlineIfcStyle {
            brush,
            font_weight: weight,
            font_size,
            ..InlineIfcStyle::default()
        }
    }

    fn style_with_metrics(
        brush: [u8; 4],
        weight: u16,
        font_size: f32,
        line_height: f32,
    ) -> InlineIfcStyle {
        InlineIfcStyle {
            brush,
            font_weight: weight,
            font_size,
            line_height,
            ..InlineIfcStyle::default()
        }
    }

    fn measured_box(width: f32, height: f32) -> InlineIfcMeasuredAtomicBox {
        InlineIfcMeasuredAtomicBox::new(
            InlineIfcSize::new(width, height),
            InlineIfcAtomicMeasureConstraints::new(Some(180.0)),
        )
    }

    fn fixture(max_width: f32) -> InlineFormattingContext {
        let input = InlineIfcInput::new(vec![InlineIfcItem::Span {
            source: ROOT,
            style: Some(style([1, 1, 1, 255], 400)),
            children: vec![
                InlineIfcItem::TextSpan {
                    source: ROOT,
                    text: "plain ".to_string(),
                    style: None,
                },
                InlineIfcItem::Span {
                    source: OUTER,
                    style: Some(style([2, 2, 2, 255], 400)),
                    children: vec![
                        InlineIfcItem::TextSpan {
                            source: OUTER,
                            text: "outer ".to_string(),
                            style: None,
                        },
                        InlineIfcItem::Span {
                            source: INNER,
                            style: Some(style([3, 3, 3, 255], 700)),
                            children: vec![InlineIfcItem::TextSpan {
                                source: INNER,
                                text: "strong".to_string(),
                                style: None,
                            }],
                        },
                        InlineIfcItem::TextSpan {
                            source: OUTER,
                            text: " tail wraps after ".to_string(),
                            style: None,
                        },
                        InlineIfcItem::AtomicInlineBox {
                            source: BOX_NODE,
                            measurement: measured_box(28.0, 18.0),
                        },
                        InlineIfcItem::TextSpan {
                            source: OUTER,
                            text: " box".to_string(),
                            style: None,
                        },
                    ],
                },
            ],
        }])
        .with_max_width(max_width);
        InlineFormattingContext::build(input)
    }

    fn cache_fixture_input() -> InlineIfcInput {
        InlineIfcInput::new(vec![InlineIfcItem::Span {
            source: ROOT,
            style: Some(style_with_metrics([1, 1, 1, 255], 400, 14.0, 1.2)),
            children: vec![
                InlineIfcItem::TextSpan {
                    source: OUTER,
                    text: "cache me ".to_string(),
                    style: Some(style_with_metrics([2, 2, 2, 255], 400, 14.0, 1.2)),
                },
                InlineIfcItem::AtomicInlineBox {
                    source: BOX_NODE,
                    measurement: measured_box(24.0, 12.0),
                },
            ],
        }])
        .with_max_width(180.0)
    }

    fn cache_invalidation(
        previous: &InlineIfcInput,
        next: &InlineIfcInput,
    ) -> InlineIfcInvalidation {
        next.cache_key().invalidation_from(&previous.cache_key())
    }

    #[test]
    fn ifc_builder_flattens_nested_styled_text_into_one_backing_string() {
        let ifc = fixture(180.0);

        assert_eq!(
            ifc.backing_text(),
            "plain outer strong tail wraps after  box"
        );
        let strong = ifc.backing_text().find("strong").unwrap();
        assert_eq!(ifc.source_for_byte(strong + 1), Some(INNER));
        assert_eq!(
            ifc.style_at_byte(strong + 1).map(|style| style.brush),
            Some([3, 3, 3, 255])
        );
        assert_eq!(
            ifc.style_at_byte(strong + 1).map(|style| style.font_weight),
            Some(700)
        );
        assert!(
            ifc.style_ranges()
                .iter()
                .any(|range| range.range.contains(&(strong + 1))
                    && range.style.brush == [3, 3, 3, 255]),
            "style ranges should preserve source-byte style lookup"
        );
    }

    #[test]
    fn cache_key_is_stable_for_identical_input() {
        let input = cache_fixture_input();
        let same_input = cache_fixture_input();
        let key = input.cache_key();
        let same_key = same_input.cache_key();
        let ifc = InlineFormattingContext::build(input.clone());

        assert_eq!(key, same_key);
        assert_eq!(ifc.cache_key(), &key);
        assert_eq!(
            cache_invalidation(&input, &same_input),
            InlineIfcInvalidation::Reuse
        );
    }

    fn plain_text_input(text: &str) -> InlineIfcInput {
        InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
            source: ROOT,
            text: text.to_string(),
            style: Some(style([1, 1, 1, 255], 400)),
        }])
    }

    #[test]
    fn wrap_epsilon_keeps_snug_content_on_one_line() {
        let text = "epsilon slack";
        let unconstrained = InlineFormattingContext::build_with_options(
            plain_text_input(text),
            InlineIfcLayoutOptions::new(None, false),
        );
        let intrinsic_width = unconstrained
            .text_pass_paint_input()
            .glyphs
            .iter()
            .map(|glyph| glyph.x + glyph.advance)
            .fold(0.0f32, f32::max);
        assert!(intrinsic_width > 10.0, "fixture should shape glyphs");
        let line_count = |max_width: f32| {
            InlineFormattingContext::build_with_options(
                plain_text_input(text),
                InlineIfcLayoutOptions::new(Some(max_width), true),
            )
            .text_layout_snapshot()
            .lines
            .len()
        };

        assert_eq!(
            line_count(intrinsic_width - 1.0),
            1,
            "content within the 2px slack must not wrap"
        );
        assert!(
            line_count(intrinsic_width - 3.0) > 1,
            "content beyond the slack must wrap"
        );
    }

    #[test]
    fn layout_align_shifts_lines_within_the_constraint() {
        let text = "align me";
        let max_width = 220.0;
        let glyph_extent = |align: InlineIfcAlignment| {
            let glyphs = InlineFormattingContext::build_with_options(
                plain_text_input(text),
                InlineIfcLayoutOptions::new(Some(max_width), true).with_align(align),
            )
            .text_pass_paint_input()
            .glyphs;
            let left = glyphs.iter().map(|glyph| glyph.x).fold(f32::MAX, f32::min);
            let right = glyphs
                .iter()
                .map(|glyph| glyph.x + glyph.advance)
                .fold(0.0f32, f32::max);
            (left, right)
        };

        let (left_l, _) = glyph_extent(InlineIfcAlignment::Left);
        let (left_c, _) = glyph_extent(InlineIfcAlignment::Center);
        let (left_r, right_r) = glyph_extent(InlineIfcAlignment::Right);
        assert!(left_l.abs() <= 1.0, "left-aligned line starts at zero");
        assert!(left_c > left_l + 1.0, "center must shift right of left");
        assert!(left_r > left_c + 1.0, "right must shift right of center");
        assert!(
            ((left_c - left_l) - (left_r - left_c)).abs() <= 1.0,
            "center should sit halfway between left and right"
        );
        assert!(
            right_r <= max_width + INLINE_IFC_WRAP_EPSILON + 0.01,
            "right-aligned glyphs must stay inside the constraint, right={right_r}"
        );
    }

    /// Safety net for dropping the legacy 240-char cluster-break guard
    /// (`parley_safe_text`): long real-world content must still shape
    /// without hanging or producing degenerate output.
    #[test]
    fn shaping_survives_long_content_without_chunk_guard() {
        let long_ascii = "a".repeat(100_000);
        let ifc = InlineFormattingContext::build_with_options(
            plain_text_input(&long_ascii),
            InlineIfcLayoutOptions::new(Some(400.0), true),
        );
        let lines = ifc.text_layout_snapshot().lines.len();
        assert!(
            lines > 100,
            "100k ascii should wrap into many lines: {lines}"
        );

        let long_cjk = "\u{4E2D}".repeat(10_000);
        let ifc = InlineFormattingContext::build_with_options(
            plain_text_input(&long_cjk),
            InlineIfcLayoutOptions::new(Some(400.0), true),
        );
        let lines = ifc.text_layout_snapshot().lines.len();
        assert!(lines > 100, "10k CJK should wrap into many lines: {lines}");
    }

    /// Parley (through 0.11) counts a shaping cluster's chars in a `u8`
    /// (`map_len` in `shape::fill_cluster_in_place`), so a single grapheme
    /// segment with thousands of combining marks used to panic with "attempt
    /// to add with overflow". The `icu_segmenter` shim now splits oversized
    /// grapheme segments at the boundary level (byte offsets untouched, see
    /// `vendor/icu_segmenter_rfgui_shim`), replacing the legacy
    /// zero-width-space insertion (`parley_safe_text`).
    #[test]
    fn shaping_survives_overlong_combining_cluster() {
        let combining = format!("a{}", "\u{0301}".repeat(2_000));
        let ifc = InlineFormattingContext::build_with_options(
            plain_text_input(&combining),
            InlineIfcLayoutOptions::new(Some(400.0), true),
        );
        assert!(!ifc.text_layout_snapshot().lines.is_empty());
    }

    #[test]
    fn cache_evicts_coldest_entries_beyond_capacity() {
        let mut cache = InlineIfcCache::new();
        for width in [100.0_f32, 120.0, 140.0, 160.0, 180.0, 200.0] {
            let update = cache.update_with_options(
                plain_text_input("evict me"),
                InlineIfcLayoutOptions::new(Some(width), true),
            );
            assert!(update.rebuilt, "distinct widths must reshape");
        }
        assert!(
            cache.len() <= INLINE_IFC_CACHE_MAX_ENTRIES,
            "cache must stay bounded, len={}",
            cache.len()
        );
        // The most recent shape must survive eviction.
        let latest_key = plain_text_input("evict me")
            .cache_key_with_layout_options(InlineIfcLayoutOptions::new(Some(200.0), true));
        assert!(
            !matches!(
                cache.lookup_key(&latest_key),
                InlineIfcCacheLookup::Miss { .. }
            ),
            "latest entry must be retained"
        );
    }

    #[test]
    fn layout_cache_key_distinguishes_alignment() {
        let input = plain_text_input("align key");
        let left =
            input.cache_key_with_layout_options(InlineIfcLayoutOptions::new(Some(100.0), true));
        let center = input.cache_key_with_layout_options(
            InlineIfcLayoutOptions::new(Some(100.0), true).with_align(InlineIfcAlignment::Center),
        );
        assert_ne!(left, center, "alignment must participate in the shape key");
    }

    #[test]
    fn cache_invalidation_reshapes_when_text_content_changes() {
        let previous = cache_fixture_input();
        let mut next = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut next.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::TextSpan { text, .. } = &mut children[0] else {
            panic!("expected text span");
        };
        text.push_str("again");

        assert_eq!(
            cache_invalidation(&previous, &next),
            InlineIfcInvalidation::Reshape
        );
    }

    #[test]
    fn cache_invalidation_reshapes_when_text_shape_style_or_width_changes() {
        let previous = cache_fixture_input();

        let mut font_size_changed = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut font_size_changed.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
            panic!("expected text span");
        };
        style.as_mut().unwrap().font_size = 18.0;
        assert_eq!(
            cache_invalidation(&previous, &font_size_changed),
            InlineIfcInvalidation::Reshape
        );

        let mut font_weight_changed = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut font_weight_changed.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
            panic!("expected text span");
        };
        style.as_mut().unwrap().font_weight = 700;
        assert_eq!(
            cache_invalidation(&previous, &font_weight_changed),
            InlineIfcInvalidation::Reshape
        );

        let mut line_height_changed = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut line_height_changed.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
            panic!("expected text span");
        };
        style.as_mut().unwrap().line_height = 1.6;
        assert_eq!(
            cache_invalidation(&previous, &line_height_changed),
            InlineIfcInvalidation::Reshape
        );

        let width_changed = cache_fixture_input().with_max_width(96.0);
        assert_eq!(
            cache_invalidation(&previous, &width_changed),
            InlineIfcInvalidation::Reshape
        );
    }

    #[test]
    fn cache_invalidation_reshapes_when_atomic_layout_inputs_change() {
        let previous = cache_fixture_input();

        let mut measured_size_changed = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut measured_size_changed.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::AtomicInlineBox { measurement, .. } = &mut children[1] else {
            panic!("expected atomic box");
        };
        measurement.measured_size = InlineIfcSize::new(36.0, 12.0);
        assert_eq!(
            cache_invalidation(&previous, &measured_size_changed),
            InlineIfcInvalidation::Reshape
        );

        let mut constraints_changed = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut constraints_changed.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::AtomicInlineBox { measurement, .. } = &mut children[1] else {
            panic!("expected atomic box");
        };
        measurement.constraints.max_width = Some(128.0);
        assert_eq!(
            cache_invalidation(&previous, &constraints_changed),
            InlineIfcInvalidation::Reshape
        );
    }

    #[test]
    fn cache_invalidation_treats_brush_change_as_repaint_only() {
        let previous = cache_fixture_input();
        let mut next = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut next.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
            panic!("expected text span");
        };
        style.as_mut().unwrap().brush = [200, 10, 10, 255];

        assert_eq!(
            cache_invalidation(&previous, &next),
            InlineIfcInvalidation::RepaintOnly
        );
    }

    #[test]
    fn cache_lookup_reuses_same_input() {
        let mut cache = InlineIfcCache::new();
        let input = cache_fixture_input();
        let expected_key = input.cache_key();

        let inserted = cache.put(input.clone());
        assert_eq!(inserted.cache_key(), &expected_key);
        assert_eq!(cache.len(), 1);

        let InlineIfcCacheLookup::Reuse(entry) = cache.lookup_input(&input) else {
            panic!("same input should reuse cached IFC entry");
        };
        assert_eq!(entry.cache_key(), &expected_key);
        assert_eq!(entry.context().backing_text(), "cache me ");
    }

    #[test]
    fn cache_lookup_treats_brush_only_change_as_repaint_only() {
        let mut cache = InlineIfcCache::new();
        let previous = cache_fixture_input();
        let previous_key = previous.cache_key();
        cache.put(previous);

        let mut next = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut next.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
            panic!("expected text span");
        };
        style.as_mut().unwrap().brush = [200, 10, 10, 255];
        let next_key = next.cache_key();

        assert_eq!(previous_key.content, next_key.content);
        assert_eq!(previous_key.layout, next_key.layout);
        assert_ne!(previous_key.paint, next_key.paint);

        let InlineIfcCacheLookup::RepaintOnly(entry) = cache.lookup_input(&next) else {
            panic!("brush-only change should keep shape cache reusable");
        };
        assert_eq!(entry.shape_key().content, next_key.content);
        assert_eq!(entry.shape_key().layout, next_key.layout);
        assert_eq!(entry.cache_key(), &previous_key);
    }

    #[test]
    fn cache_lookup_misses_when_shape_inputs_change() {
        let mut cache = InlineIfcCache::new();
        let previous = cache_fixture_input();
        cache.put(previous);

        let mut text_changed = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut text_changed.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::TextSpan { text, .. } = &mut children[0] else {
            panic!("expected text span");
        };
        text.push_str("again");
        assert_reshape_miss(&cache, &text_changed);

        let mut font_size_changed = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut font_size_changed.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
            panic!("expected text span");
        };
        style.as_mut().unwrap().font_size = 18.0;
        assert_reshape_miss(&cache, &font_size_changed);

        let width_changed = cache_fixture_input().with_max_width(96.0);
        assert_reshape_miss(&cache, &width_changed);

        let mut measured_size_changed = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut measured_size_changed.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::AtomicInlineBox { measurement, .. } = &mut children[1] else {
            panic!("expected atomic box");
        };
        measurement.measured_size = InlineIfcSize::new(36.0, 12.0);
        assert_reshape_miss(&cache, &measured_size_changed);
    }

    #[test]
    fn cache_put_updates_entry_key_for_same_shape() {
        let mut cache = InlineIfcCache::new();
        let previous = cache_fixture_input();
        cache.put(previous);

        let mut next = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut next.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
            panic!("expected text span");
        };
        style.as_mut().unwrap().brush = [10, 200, 10, 255];
        let next_key = next.cache_key();

        let updated = cache.put(next.clone());
        assert_eq!(updated.cache_key(), &next_key);
        assert_eq!(cache.len(), 1);

        let InlineIfcCacheLookup::Reuse(entry) = cache.lookup_input(&next) else {
            panic!("updated paint key should reuse on the next identical lookup");
        };
        assert_eq!(entry.cache_key(), &next_key);
    }

    #[test]
    fn cache_update_reuses_same_input_without_rebuild() {
        let mut cache = InlineIfcCache::new();
        let input = cache_fixture_input();
        let expected_key = input.cache_key();
        cache.put(input.clone());

        {
            let update = cache.update(input);
            assert_eq!(update.invalidation, InlineIfcInvalidation::Reuse);
            assert!(!update.rebuilt);
            assert_eq!(update.entry.cache_key(), &expected_key);
            assert_eq!(update.entry.context().backing_text(), "cache me ");
        }
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn cache_update_repaints_brush_only_change_and_refreshes_entry_key() {
        let mut cache = InlineIfcCache::new();
        let previous = cache_fixture_input();
        let previous_key = previous.cache_key();
        cache.put(previous);

        let mut next = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut next.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
            panic!("expected text span");
        };
        style.as_mut().unwrap().brush = [240, 20, 20, 255];
        let next_key = next.cache_key();

        assert_eq!(previous_key.content, next_key.content);
        assert_eq!(previous_key.layout, next_key.layout);
        assert_ne!(previous_key.paint, next_key.paint);

        {
            let update = cache.update(next.clone());
            assert_eq!(update.invalidation, InlineIfcInvalidation::RepaintOnly);
            assert!(update.rebuilt);
            assert_eq!(update.entry.shape_key().content, next_key.content);
            assert_eq!(update.entry.shape_key().layout, next_key.layout);
            assert_eq!(update.entry.cache_key(), &next_key);
        }
        assert_eq!(cache.len(), 1);

        let InlineIfcCacheLookup::Reuse(entry) = cache.lookup_input(&next) else {
            panic!("repaint update should refresh entry paint key");
        };
        assert_eq!(entry.cache_key(), &next_key);
    }

    #[test]
    fn cache_update_reshapes_when_shape_inputs_change() {
        let mut cache = InlineIfcCache::new();
        cache.put(cache_fixture_input());

        let mut text_changed = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut text_changed.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::TextSpan { text, .. } = &mut children[0] else {
            panic!("expected text span");
        };
        text.push_str("again");
        assert_cache_update_reshape(&mut cache, text_changed);

        let mut font_size_changed = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut font_size_changed.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
            panic!("expected text span");
        };
        style.as_mut().unwrap().font_size = 18.0;
        assert_cache_update_reshape(&mut cache, font_size_changed);

        let width_changed = cache_fixture_input().with_max_width(96.0);
        assert_cache_update_reshape(&mut cache, width_changed);

        let mut measured_size_changed = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut measured_size_changed.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::AtomicInlineBox { measurement, .. } = &mut children[1] else {
            panic!("expected atomic box");
        };
        measurement.measured_size = InlineIfcSize::new(36.0, 12.0);
        assert_cache_update_reshape(&mut cache, measured_size_changed);
    }

    #[test]
    fn cache_update_reuses_after_reshape_builds_new_entry() {
        let mut cache = InlineIfcCache::new();
        cache.put(cache_fixture_input());

        let mut next = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut next.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::TextSpan { text, .. } = &mut children[0] else {
            panic!("expected text span");
        };
        text.push_str("again");
        let next_key = next.cache_key();

        {
            let update = cache.update(next.clone());
            assert_eq!(update.invalidation, InlineIfcInvalidation::Reshape);
            assert!(update.rebuilt);
            assert_eq!(update.entry.cache_key(), &next_key);
        }
        assert_eq!(cache.len(), 2);

        {
            let update = cache.update(next);
            assert_eq!(update.invalidation, InlineIfcInvalidation::Reuse);
            assert!(!update.rebuilt);
            assert_eq!(update.entry.cache_key(), &next_key);
        }
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn cache_update_api_stays_within_ifc_boundary() {
        let mut cache = InlineIfcCache::new();
        let input = cache_fixture_input();
        let update = cache.update(input);

        assert_eq!(update.invalidation, InlineIfcInvalidation::Reshape);
        assert!(update.rebuilt);
        assert_eq!(update.entry.context().backing_text(), "cache me ");
        assert_eq!(update.entry.context().inline_boxes().len(), 1);
        assert!(!update.entry.context().text_paint_runs().is_empty());
        assert!(
            !update
                .entry
                .context()
                .decoration_paint_fragments()
                .is_empty()
        );
    }

    #[test]
    fn cache_api_stores_ifc_context_without_render_dependencies() {
        let mut cache = InlineIfcCache::new();
        let input = cache_fixture_input();
        let entry = cache.put(input);

        assert_eq!(entry.context().backing_text(), "cache me ");
        assert_eq!(entry.context().inline_boxes().len(), 1);
        assert!(!entry.context().text_paint_runs().is_empty());
        assert!(!entry.context().decoration_paint_fragments().is_empty());
    }

    fn assert_reshape_miss(cache: &InlineIfcCache, input: &InlineIfcInput) {
        let InlineIfcCacheLookup::Miss { invalidation } = cache.lookup_input(input) else {
            panic!("shape input change should miss the IFC cache");
        };
        assert_eq!(invalidation, InlineIfcInvalidation::Reshape);
    }

    fn assert_cache_update_reshape(cache: &mut InlineIfcCache, input: InlineIfcInput) {
        let expected_key = input.cache_key();
        let update = cache.update(input);
        assert_eq!(update.invalidation, InlineIfcInvalidation::Reshape);
        assert!(update.rebuilt);
        assert_eq!(update.entry.cache_key(), &expected_key);
    }

    fn text_layout_snapshot_shape(
        snapshot: &InlineIfcTextLayoutSnapshot,
    ) -> Vec<(
        usize,
        u32,
        u32,
        u32,
        u32,
        u32,
        Range<usize>,
        Vec<(
            InlineIfcSourceId,
            Range<usize>,
            u32,
            u32,
            u32,
            u32,
            u32,
            u64,
            u32,
            u64,
        )>,
    )> {
        snapshot
            .lines
            .iter()
            .map(|line| {
                (
                    line.line_index,
                    f32_cache_bits(line.x),
                    f32_cache_bits(line.y),
                    f32_cache_bits(line.width),
                    f32_cache_bits(line.height),
                    f32_cache_bits(line.baseline),
                    line.range.clone(),
                    line.glyphs
                        .iter()
                        .map(|glyph| {
                            (
                                glyph.source,
                                glyph.cluster_range.clone(),
                                glyph.glyph_id,
                                f32_cache_bits(glyph.x),
                                f32_cache_bits(glyph.y),
                                f32_cache_bits(glyph.advance),
                                f32_cache_bits(glyph.font_size),
                                glyph.font_data_id,
                                glyph.font_index,
                                glyph.normalized_coords_hash,
                            )
                        })
                        .collect(),
                )
            })
            .collect()
    }

    fn assert_font_render_handle(font_data: &Option<FontData>, font_data_id: u64, font_index: u32) {
        let font_data = font_data
            .as_ref()
            .expect("IFC glyph payload should carry a renderable FontData handle");
        assert_eq!(font_data.data.id(), font_data_id);
        assert_eq!(font_data.index, font_index);
    }

    #[test]
    fn ifc_builder_keeps_source_ranges_for_text_and_spans() {
        let ifc = fixture(180.0);
        let outer = ifc
            .source_ranges()
            .iter()
            .find(|range| range.source == OUTER && range.kind == InlineIfcSourceKind::Span)
            .expect("outer span should have a source range");
        let inner = ifc
            .source_ranges()
            .iter()
            .find(|range| range.source == INNER && range.kind == InlineIfcSourceKind::Span)
            .expect("inner span should have a source range");

        assert!(outer.range.start < inner.range.start);
        assert!(inner.range.end <= outer.range.end);
        assert_eq!(ifc.source_for_byte(inner.range.start), Some(INNER));
    }

    #[test]
    fn ifc_builder_maps_inline_boxes_back_to_source_nodes() {
        let ifc = fixture(180.0);
        let inline_box = ifc.inline_boxes().first().expect("expected inline box");

        assert_eq!(inline_box.source, BOX_NODE);
        assert_eq!(ifc.source_for_inline_box(inline_box.id), Some(BOX_NODE));
        assert!((inline_box.measurement.measured_size.width - 28.0).abs() < 0.01);
        assert!((inline_box.measurement.measured_size.height - 18.0).abs() < 0.01);

        let placements = ifc.inline_box_placements();
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].source, BOX_NODE);
        assert!((placements[0].width - 28.0).abs() < 0.01);
        assert!((placements[0].height - 18.0).abs() < 0.01);
    }

    #[test]
    fn ifc_builder_rebuilds_line_fragments_for_span_decoration() {
        let ifc = fixture(92.0);
        let fragments = ifc.line_fragments();
        let outer_fragments = fragments
            .iter()
            .filter(|fragment| fragment.source == OUTER)
            .collect::<Vec<_>>();

        assert!(
            outer_fragments.len() >= 2,
            "narrow layout should split a span into drawable per-line fragments: {outer_fragments:?}",
        );
        assert!(
            outer_fragments
                .iter()
                .all(|fragment| fragment.x1 >= fragment.x0 && fragment.y1 > fragment.y0),
            "fragments should expose drawable rects: {outer_fragments:?}",
        );
    }

    #[test]
    fn ifc_builder_keeps_style_lookup_independent_from_line_boundaries() {
        let ifc = fixture(180.0);
        let first_line_sources = ifc
            .line_fragments()
            .into_iter()
            .filter(|fragment| fragment.line_index == 0)
            .map(|fragment| fragment.source)
            .collect::<Vec<_>>();

        assert!(first_line_sources.contains(&ROOT));
        assert!(first_line_sources.contains(&OUTER));
        assert!(first_line_sources.contains(&INNER));
        assert_eq!(
            ifc.style_at_byte(ifc.backing_text().find("outer").unwrap())
                .map(|style| style.brush),
            Some([2, 2, 2, 255])
        );
    }

    #[test]
    fn glyph_output_groups_nested_span_styles_by_source_byte_ranges() {
        let ifc = fixture(240.0);
        let groups = ifc.glyph_groups();
        let outer_start = ifc.backing_text().find("outer").unwrap();
        let strong_start = ifc.backing_text().find("strong").unwrap();

        let outer_group = groups
            .iter()
            .find(|group| group.range.contains(&outer_start))
            .expect("outer text should produce a glyph group");
        let strong_group = groups
            .iter()
            .find(|group| group.range.contains(&strong_start))
            .expect("nested strong text should produce a glyph group");

        assert_eq!(outer_group.source, OUTER);
        assert_eq!(outer_group.style.brush, [2, 2, 2, 255]);
        assert_eq!(outer_group.style.font_weight, 400);
        assert_eq!(strong_group.source, INNER);
        assert_eq!(strong_group.style.brush, [3, 3, 3, 255]);
        assert_eq!(strong_group.style.font_weight, 700);
        assert!(
            strong_group
                .glyphs
                .iter()
                .all(|glyph| glyph.style == strong_group.style
                    && glyph.font_size > 0.0
                    && glyph.advance >= 0.0),
            "glyph items should carry resolved style and font identity: {strong_group:?}"
        );
    }

    #[test]
    fn glyph_output_does_not_depend_on_parley_item_boundaries_for_style_lookup() {
        let input = InlineIfcInput::new(vec![
            InlineIfcItem::TextSpan {
                source: ROOT,
                text: "aa ".to_string(),
                style: Some(style([10, 0, 0, 255], 400)),
            },
            InlineIfcItem::TextSpan {
                source: OUTER,
                text: "bb ".to_string(),
                style: Some(style([0, 10, 0, 255], 700)),
            },
            InlineIfcItem::TextSpan {
                source: INNER,
                text: "cc".to_string(),
                style: Some(style([0, 0, 10, 255], 400)),
            },
        ])
        .with_max_width(300.0);
        let ifc = InlineFormattingContext::build(input);
        let line_zero_groups = ifc
            .glyph_groups()
            .into_iter()
            .filter(|group| group.line_index == 0)
            .collect::<Vec<_>>();

        assert!(
            line_zero_groups
                .iter()
                .any(|group| group.source == ROOT && group.style.brush == [10, 0, 0, 255]),
            "first style should be recovered from IFC style ranges: {line_zero_groups:?}"
        );
        assert!(
            line_zero_groups
                .iter()
                .any(|group| group.source == OUTER && group.style.font_weight == 700),
            "middle style should be recovered from IFC style ranges: {line_zero_groups:?}"
        );
        assert!(
            line_zero_groups
                .iter()
                .any(|group| group.source == INNER && group.style.brush == [0, 0, 10, 255]),
            "last style should be recovered from IFC style ranges: {line_zero_groups:?}"
        );
    }

    #[test]
    fn decoration_fragments_describe_multiline_span_rects() {
        let ifc = fixture(72.0);
        let fragments = ifc
            .decoration_fragments()
            .into_iter()
            .filter(|fragment| fragment.source == OUTER)
            .collect::<Vec<_>>();

        assert!(
            fragments.len() >= 2,
            "narrow layout should split a decorated span across lines: {fragments:?}"
        );
        assert!(
            fragments.iter().all(|fragment| {
                fragment.x1 >= fragment.x0
                    && fragment.y1 > fragment.y0
                    && !fragment.range.is_empty()
                    && fragment.style.is_some()
            }),
            "decoration fragments should have drawable rects and source-byte style: {fragments:?}"
        );
        assert!(
            fragments
                .windows(2)
                .any(|pair| pair[0].line_index != pair[1].line_index),
            "span fragments should preserve line identity: {fragments:?}"
        );
    }

    #[test]
    fn atomic_inline_box_placement_and_glyph_output_do_not_share_sources() {
        let ifc = fixture(180.0);
        let glyphs = ifc.glyph_items();
        let placements = ifc.inline_box_placements();

        assert!(!glyphs.is_empty(), "text should still produce glyph output");
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].source, BOX_NODE);
        assert!(
            glyphs.iter().all(|glyph| glyph.source != BOX_NODE),
            "atomic boxes should not leak into glyph output: {glyphs:?}"
        );
    }

    #[test]
    fn glyph_snapshot_and_text_pass_payloads_carry_font_render_handle() {
        let ifc = fixture(180.0);
        let glyph_item = ifc
            .glyph_items()
            .into_iter()
            .next()
            .expect("fixture should produce IFC glyph items");
        assert_font_render_handle(
            &glyph_item.font_data,
            glyph_item.font_data_id,
            glyph_item.font_index,
        );
        assert_eq!(
            glyph_item.normalized_coords_hash, 0,
            "default fixture fonts should still carry the normalized coords hash field"
        );

        let snapshot = ifc.text_layout_snapshot();
        let snapshot_glyph = snapshot
            .lines
            .iter()
            .flat_map(|line| line.glyphs.iter())
            .find(|glyph| {
                glyph.glyph_id == glyph_item.glyph_id
                    && glyph.cluster_range == glyph_item.cluster_range
            })
            .expect("snapshot should preserve the IFC glyph");
        assert_font_render_handle(
            &snapshot_glyph.font_data,
            snapshot_glyph.font_data_id,
            snapshot_glyph.font_index,
        );
        assert_eq!(snapshot_glyph.font_data_id, glyph_item.font_data_id);
        assert_eq!(snapshot_glyph.font_index, glyph_item.font_index);
        assert_eq!(
            snapshot_glyph.normalized_coords_hash,
            glyph_item.normalized_coords_hash
        );
        assert_eq!(
            snapshot_glyph.batch_key.normalized_coords_hash,
            snapshot_glyph.normalized_coords_hash
        );

        let adapter = snapshot.text_pass_paint_input();
        let adapter_glyph = adapter
            .glyphs
            .iter()
            .find(|glyph| {
                glyph.glyph_id == snapshot_glyph.glyph_id
                    && glyph.cluster_range == snapshot_glyph.cluster_range
            })
            .expect("text-pass adapter should preserve the snapshot glyph");
        assert_font_render_handle(
            &adapter_glyph.font_data,
            adapter_glyph.font_data_id,
            adapter_glyph.font_index,
        );
        assert_eq!(adapter_glyph.font_data_id, snapshot_glyph.font_data_id);
        assert_eq!(adapter_glyph.font_index, snapshot_glyph.font_index);
        assert_eq!(
            adapter_glyph.normalized_coords_hash,
            snapshot_glyph.normalized_coords_hash
        );
        assert_eq!(
            adapter_glyph.batch_key.normalized_coords_hash,
            adapter_glyph.normalized_coords_hash
        );
    }

    #[test]
    fn text_paint_payload_preserves_nested_style_and_font_identity() {
        let ifc = fixture(240.0);
        let output = ifc.text_paint_output();
        let outer_start = ifc.backing_text().find("outer").unwrap();
        let strong_start = ifc.backing_text().find("strong").unwrap();

        let outer_run = output
            .runs
            .iter()
            .find(|run| run.range.contains(&outer_start))
            .expect("outer text should produce a text paint run");
        let strong_run = output
            .runs
            .iter()
            .find(|run| run.range.contains(&strong_start))
            .expect("nested strong text should produce a text paint run");

        assert_eq!(outer_run.source, OUTER);
        assert_eq!(outer_run.style.brush, [2, 2, 2, 255]);
        assert_eq!(outer_run.style.font_weight, 400);
        assert_eq!(strong_run.source, INNER);
        assert_eq!(strong_run.style.brush, [3, 3, 3, 255]);
        assert_eq!(strong_run.style.font_weight, 700);
        assert_ne!(outer_run.batch_key, strong_run.batch_key);
        assert!(outer_run.glyphs.iter().all(|glyph| {
            glyph.batch_key == outer_run.batch_key
                && glyph.font_data_id == outer_run.batch_key.font_data_id
                && glyph.font_index == outer_run.batch_key.font_index
                && glyph.normalized_coords_hash == outer_run.batch_key.normalized_coords_hash
                && glyph.font_data.is_some()
                && (glyph.font_size - outer_run.batch_key.font_size()).abs() < 0.01
        }));
        assert!(strong_run.glyphs.iter().all(|glyph| {
            glyph.batch_key == strong_run.batch_key
                && glyph.font_data_id == strong_run.batch_key.font_data_id
                && glyph.font_index == strong_run.batch_key.font_index
                && glyph.normalized_coords_hash == strong_run.batch_key.normalized_coords_hash
                && glyph.font_data.is_some()
                && (glyph.font_size - strong_run.batch_key.font_size()).abs() < 0.01
        }));
    }

    #[test]
    fn text_paint_payload_keeps_atomic_box_sources_out_of_glyphs() {
        let ifc = fixture(180.0);
        let output = ifc.text_paint_output();
        let placements = ifc.inline_box_placements();

        assert!(
            !output.glyphs.is_empty(),
            "text should produce paint glyphs"
        );
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].source, BOX_NODE);
        assert!(
            output.glyphs.iter().all(|glyph| glyph.source != BOX_NODE),
            "paint glyphs should not inherit atomic inline box source ids: {output:?}"
        );
        assert!(
            output.runs.iter().all(|run| run.source != BOX_NODE),
            "paint runs should not inherit atomic inline box source ids: {output:?}"
        );
    }

    #[test]
    fn decoration_paint_payload_preserves_multiline_rects_and_style() {
        let ifc = fixture(72.0);
        let fragments = ifc
            .decoration_paint_fragments()
            .into_iter()
            .filter(|fragment| fragment.source == OUTER)
            .collect::<Vec<_>>();

        assert!(
            fragments.len() >= 2,
            "narrow layout should produce multiline decoration paint fragments: {fragments:?}"
        );
        assert!(
            fragments.iter().all(|fragment| {
                fragment.rect.width >= 0.0
                    && fragment.rect.height > 0.0
                    && !fragment.range.is_empty()
                    && fragment.style.is_some()
            }),
            "decoration paint fragments should preserve drawable rects and resolved style: {fragments:?}"
        );
        assert!(
            fragments
                .windows(2)
                .any(|pair| pair[0].line_index != pair[1].line_index),
            "decoration paint fragments should preserve line identity: {fragments:?}"
        );
    }

    #[test]
    fn element_decoration_payload_expands_span_rects_with_slice_insets() {
        let ifc = fixture(72.0);
        let raw_fragments = ifc
            .decoration_paint_fragments()
            .into_iter()
            .filter(|fragment| fragment.source == OUTER)
            .collect::<Vec<_>>();
        let expanded = ifc.element_decoration_paint_fragments(
            OUTER,
            InlineIfcDecorationBoxInsets::new(8.0, 6.0, 4.0, 2.0),
        );

        assert!(
            raw_fragments.len() >= 2,
            "fixture should split OUTER decoration across lines: {raw_fragments:?}"
        );
        assert_eq!(expanded.len(), raw_fragments.len());

        let first = expanded.first().expect("first expanded fragment");
        let first_raw = raw_fragments.first().expect("first raw fragment");
        assert!(first.is_first_for_source);
        assert!(!first.is_last_for_source);
        assert!((first.rect.x - first_raw.rect.x).abs() < 0.01);
        assert!((first.rect.y - (first_raw.rect.y - 4.0)).abs() < 0.01);
        assert!((first.rect.width - (first_raw.rect.width + 8.0)).abs() < 0.01);
        assert!((first.rect.height - (first_raw.rect.height + 6.0)).abs() < 0.01);

        let last = expanded.last().expect("last expanded fragment");
        let last_raw = raw_fragments.last().expect("last raw fragment");
        assert!(!last.is_first_for_source);
        assert!(last.is_last_for_source);
        assert!((last.rect.x - last_raw.rect.x).abs() < 0.01);
        assert!((last.rect.right() - (last_raw.rect.right() + 6.0)).abs() < 0.01);
        assert!((last.rect.bottom() - (last_raw.rect.bottom() + 2.0)).abs() < 0.01);
    }

    #[test]
    fn element_decoration_draw_rect_package_preserves_source_style_and_slice_metadata() {
        let ifc = fixture(72.0);
        let outer_style = style([2, 2, 2, 255], 400);
        let draw_style = InlineIfcElementDecorationDrawRectStyle::new(
            InlineIfcPaintStyleKey::from_style(&outer_style),
            [0.1, 0.2, 0.3, 1.0],
            0.5,
            [1.0, 2.0, 3.0, 4.0],
            [0.8, 0.7, 0.6, 1.0],
        );
        let insets = InlineIfcDecorationBoxInsets::new(8.0, 6.0, 4.0, 2.0);

        let package = ifc.element_decoration_draw_rect_package(OUTER, insets, draw_style);

        assert_eq!(package.source, OUTER);
        assert_eq!(package.style_key, draw_style.style_key);
        assert_eq!(package.slice_insets, insets);
        assert!(
            package.fragments.len() >= 2,
            "wrapped outer span should produce multiple draw rect fragments: {package:?}"
        );
        for fragment in &package.fragments {
            assert_eq!(fragment.source, OUTER);
            assert_eq!(fragment.style_key, draw_style.style_key);
            assert_eq!(fragment.slice_insets, insets);
            assert_eq!(
                fragment.metadata.position,
                [fragment.rect.x, fragment.rect.y]
            );
            assert_eq!(
                fragment.metadata.size,
                [fragment.rect.width, fragment.rect.height]
            );
            assert_eq!(fragment.metadata.fill_color, draw_style.fill_color);
            assert_eq!(fragment.metadata.opacity, draw_style.opacity);
            assert_eq!(fragment.metadata.border_widths, draw_style.border_widths);
            assert_eq!(fragment.metadata.border_color, draw_style.border_color);
        }
        assert!(
            package
                .fragments
                .first()
                .is_some_and(|fragment| fragment.is_first_for_source)
        );
        assert!(
            package
                .fragments
                .last()
                .is_some_and(|fragment| fragment.is_last_for_source)
        );
    }

    #[test]
    fn element_package_distributor_splits_nested_atomic_and_missing_sources() {
        let ifc = fixture(72.0);
        let outer_draw_style = InlineIfcElementDecorationDrawRectStyle::new(
            InlineIfcPaintStyleKey::from_style(&style([2, 2, 2, 255], 400)),
            [0.2, 0.3, 0.4, 1.0],
            0.8,
            [1.0, 1.0, 1.0, 1.0],
            [0.1, 0.1, 0.1, 1.0],
        );
        let inner_draw_style = InlineIfcElementDecorationDrawRectStyle::new(
            InlineIfcPaintStyleKey::from_style(&style([3, 3, 3, 255], 700)),
            [0.7, 0.2, 0.2, 1.0],
            0.7,
            [2.0, 2.0, 2.0, 2.0],
            [0.6, 0.0, 0.0, 1.0],
        );
        let missing = InlineIfcSourceId(999);
        let distributor = ifc.element_package_distributor(
            InlineIfcElementPackageDistributionInput::new()
                .with_decoration_source(InlineIfcElementDecorationPackageSource::new(
                    OUTER,
                    InlineIfcDecorationBoxInsets::new(4.0, 5.0, 1.0, 2.0),
                    outer_draw_style,
                ))
                .with_decoration_source(InlineIfcElementDecorationPackageSource::new(
                    INNER,
                    InlineIfcDecorationBoxInsets::new(1.0, 1.0, 1.0, 1.0),
                    inner_draw_style,
                ))
                .with_decoration_source(InlineIfcElementDecorationPackageSource::new(
                    missing,
                    InlineIfcDecorationBoxInsets::new(9.0, 9.0, 9.0, 9.0),
                    outer_draw_style,
                ))
                .with_atomic_source(BOX_NODE)
                .with_atomic_source(missing),
        );

        let outer = distributor
            .decoration_package(OUTER)
            .expect("outer span should receive decoration package");
        let inner = distributor
            .decoration_package(INNER)
            .expect("inner span should receive decoration package");
        let atomic = distributor
            .atomic_package(BOX_NODE)
            .expect("atomic source should receive placement package");

        assert!(outer.fragments.len() >= 2);
        assert!(!inner.fragments.is_empty());
        assert!(outer.fragments.iter().all(|fragment| {
            fragment.source == OUTER
                && fragment.style_key == outer_draw_style.style_key
                && fragment.metadata.fill_color == outer_draw_style.fill_color
                && fragment.metadata.border_widths == outer_draw_style.border_widths
        }));
        assert!(inner.fragments.iter().all(|fragment| {
            fragment.source == INNER
                && fragment.style_key == inner_draw_style.style_key
                && fragment.metadata.fill_color == inner_draw_style.fill_color
                && fragment.metadata.border_widths == inner_draw_style.border_widths
        }));
        assert_eq!(atomic.source, BOX_NODE);
        assert_eq!(atomic.placements.len(), 1);
        assert_eq!(atomic.placements[0].source, BOX_NODE);
        assert!(
            distributor.decoration_package(BOX_NODE).is_none(),
            "atomic source must not be synthesized into decoration package"
        );
        assert!(distributor.package(missing).is_none());
        assert_eq!(distributor.packages().count(), 3);
    }

    #[test]
    fn element_package_distributor_keeps_multiple_sibling_sources_separate() {
        let sibling = InlineIfcSourceId(6);
        let ifc = InlineFormattingContext::build(
            InlineIfcInput::new(vec![
                InlineIfcItem::Span {
                    source: OUTER,
                    style: Some(style_with_metrics([10, 20, 30, 255], 400, 15.0, 1.25)),
                    children: vec![InlineIfcItem::TextSpan {
                        source: OUTER,
                        text: "first inline sibling wraps ".to_string(),
                        style: None,
                    }],
                },
                InlineIfcItem::Span {
                    source: sibling,
                    style: Some(style_with_metrics([40, 50, 60, 255], 700, 15.0, 1.25)),
                    children: vec![InlineIfcItem::TextSpan {
                        source: sibling,
                        text: "second inline sibling wraps too".to_string(),
                        style: None,
                    }],
                },
            ])
            .with_max_width(96.0),
        );
        let outer_style = InlineIfcElementDecorationDrawRectStyle::new(
            InlineIfcPaintStyleKey::from_style(&style_with_metrics(
                [10, 20, 30, 255],
                400,
                15.0,
                1.25,
            )),
            [0.1, 0.2, 0.3, 1.0],
            0.9,
            [1.0, 2.0, 3.0, 4.0],
            [0.0, 0.0, 0.0, 1.0],
        );
        let sibling_style = InlineIfcElementDecorationDrawRectStyle::new(
            InlineIfcPaintStyleKey::from_style(&style_with_metrics(
                [40, 50, 60, 255],
                700,
                15.0,
                1.25,
            )),
            [0.4, 0.5, 0.6, 1.0],
            0.85,
            [4.0, 3.0, 2.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
        );

        let distributor = ifc.element_package_distributor(
            InlineIfcElementPackageDistributionInput::new()
                .with_decoration_source(InlineIfcElementDecorationPackageSource::new(
                    OUTER,
                    InlineIfcDecorationBoxInsets::new(1.0, 2.0, 3.0, 4.0),
                    outer_style,
                ))
                .with_decoration_source(InlineIfcElementDecorationPackageSource::new(
                    sibling,
                    InlineIfcDecorationBoxInsets::new(4.0, 3.0, 2.0, 1.0),
                    sibling_style,
                )),
        );
        let outer = distributor
            .decoration_package(OUTER)
            .expect("outer sibling package");
        let sibling_package = distributor
            .decoration_package(sibling)
            .expect("second sibling package");

        assert!(outer.fragments.len() >= 2);
        assert!(sibling_package.fragments.len() >= 2);
        assert!(outer.fragments.iter().all(|fragment| {
            fragment.source == OUTER
                && fragment.style_key == outer_style.style_key
                && fragment.metadata.fill_color == outer_style.fill_color
        }));
        assert!(sibling_package.fragments.iter().all(|fragment| {
            fragment.source == sibling
                && fragment.style_key == sibling_style.style_key
                && fragment.metadata.fill_color == sibling_style.fill_color
        }));
        assert_ne!(outer.style_key, sibling_package.style_key);
        assert_eq!(distributor.atomic_package(OUTER), None);
        assert_eq!(distributor.packages().count(), 2);
    }

    #[test]
    fn nested_span_decoration_keeps_source_style_identity_separate() {
        let ifc = fixture(180.0);
        let outer = ifc
            .decoration_paint_fragments()
            .into_iter()
            .find(|fragment| fragment.source == OUTER && fragment.range.start < fragment.range.end)
            .expect("outer decoration should exist");
        let inner = ifc
            .decoration_paint_fragments()
            .into_iter()
            .find(|fragment| fragment.source == INNER)
            .expect("inner decoration should exist");

        assert_eq!(
            outer.style.as_ref().map(|style| style.brush),
            Some([2, 2, 2, 255])
        );
        assert_eq!(
            inner.style.as_ref().map(|style| style.brush),
            Some([3, 3, 3, 255])
        );
        assert_eq!(
            outer.style.as_ref().map(InlineIfcPaintStyleKey::from_style),
            Some(InlineIfcPaintStyleKey::from_style(&style(
                [2, 2, 2, 255],
                400
            )))
        );
        assert_eq!(
            inner.style.as_ref().map(InlineIfcPaintStyleKey::from_style),
            Some(InlineIfcPaintStyleKey::from_style(&style(
                [3, 3, 3, 255],
                700
            )))
        );
    }

    #[test]
    fn atomic_inline_box_mixed_text_stays_out_of_text_and_decoration_payloads() {
        let ifc = fixture(120.0);
        let snapshot = ifc.text_layout_snapshot();
        let package = ifc.atomic_box_placement_package(BOX_NODE);

        assert!(
            snapshot
                .inline_boxes
                .iter()
                .any(|placement| placement.source == BOX_NODE),
            "atomic inline box should have a placement in the mixed IFC snapshot: {snapshot:?}"
        );
        assert!(
            snapshot
                .lines
                .iter()
                .flat_map(|line| &line.glyphs)
                .all(|glyph| glyph.source != BOX_NODE),
            "atomic inline box must not enter text glyph payload: {snapshot:?}"
        );
        assert!(
            snapshot
                .decorations
                .iter()
                .all(|fragment| fragment.source != BOX_NODE),
            "atomic inline box must not enter span decoration payload: {snapshot:?}"
        );
        assert!(
            snapshot.lines.iter().any(|line| !line.glyphs.is_empty())
                && !snapshot.inline_boxes.is_empty(),
            "mixed text and atomic box should coexist in the same IFC snapshot: {snapshot:?}"
        );
        assert_eq!(package.source, BOX_NODE);
        assert_eq!(
            package.placements.len(),
            1,
            "atomic placement package should expose one placement for this fixture: {package:?}"
        );
        let placement = package.placements.first().expect("atomic placement");
        assert_eq!(placement.source, BOX_NODE);
        assert_eq!(ifc.source_for_inline_box(placement.id), Some(BOX_NODE));
        assert_eq!(
            placement.insertion_byte,
            ifc.inline_boxes()[0].insertion_byte
        );
        assert!((placement.rect.width - 28.0).abs() < 0.01);
        assert!((placement.rect.height - 18.0).abs() < 0.01);
        assert_eq!(
            placement.measurement.measured_size,
            InlineIfcSize::new(28.0, 18.0)
        );
    }

    #[test]
    fn text_paint_batch_key_distinguishes_brush_and_font_identity() {
        let input = InlineIfcInput::new(vec![
            InlineIfcItem::TextSpan {
                source: ROOT,
                text: "red ".to_string(),
                style: Some(style_with_size([220, 0, 0, 255], 400, 14.0)),
            },
            InlineIfcItem::TextSpan {
                source: OUTER,
                text: "green ".to_string(),
                style: Some(style_with_size([0, 160, 0, 255], 400, 14.0)),
            },
            InlineIfcItem::TextSpan {
                source: INNER,
                text: "large".to_string(),
                style: Some(style_with_size([0, 160, 0, 255], 700, 20.0)),
            },
        ])
        .with_max_width(300.0);
        let ifc = InlineFormattingContext::build(input);
        let runs = ifc.text_paint_runs();
        let red_run = runs
            .iter()
            .find(|run| run.source == ROOT)
            .expect("red run should exist");
        let green_run = runs
            .iter()
            .find(|run| run.source == OUTER)
            .expect("green run should exist");
        let large_run = runs
            .iter()
            .find(|run| run.source == INNER)
            .expect("large run should exist");

        assert_ne!(
            red_run.batch_key, green_run.batch_key,
            "batch keys must separate different brushes"
        );
        assert_ne!(
            green_run.batch_key, large_run.batch_key,
            "batch keys must separate different font paint identities"
        );
        assert_eq!(red_run.batch_key.brush, [220, 0, 0, 255]);
        assert_eq!(green_run.batch_key.brush, [0, 160, 0, 255]);
        assert_eq!(large_run.batch_key.brush, [0, 160, 0, 255]);
        assert!((large_run.batch_key.font_size() - 20.0).abs() < 0.01);
        assert_eq!(large_run.batch_key.font_weight, 700);
    }

    #[test]
    fn text_layout_snapshot_preserves_single_line_nested_span_payload() {
        let input = InlineIfcInput::new(vec![InlineIfcItem::Span {
            source: ROOT,
            style: Some(style([1, 1, 1, 255], 400)),
            children: vec![
                InlineIfcItem::TextSpan {
                    source: ROOT,
                    text: "alpha ".to_string(),
                    style: None,
                },
                InlineIfcItem::Span {
                    source: INNER,
                    style: Some(style([9, 9, 9, 255], 700)),
                    children: vec![InlineIfcItem::TextSpan {
                        source: INNER,
                        text: "bold".to_string(),
                        style: None,
                    }],
                },
            ],
        }])
        .with_max_width(400.0);
        let ifc = InlineFormattingContext::build(input);
        let snapshot = ifc.text_layout_snapshot();
        let bold_start = ifc.backing_text().find("bold").unwrap();

        assert_eq!(snapshot.lines.len(), 1);
        let line = &snapshot.lines[0];
        assert_eq!(line.line_index, 0);
        assert_eq!(line.range, 0..ifc.backing_text().len());
        assert!(line.width > 0.0);
        assert!(line.height > 0.0);
        assert!(line.baseline > 0.0);
        assert!(
            line.glyphs.iter().any(|glyph| {
                glyph.source == INNER
                    && glyph.cluster_range.contains(&bold_start)
                    && glyph.style.brush == [9, 9, 9, 255]
                    && glyph.style.font_weight == 700
                    && glyph.batch_key.brush == [9, 9, 9, 255]
                    && glyph.font_data_id == glyph.batch_key.font_data_id
                    && glyph.font_index == glyph.batch_key.font_index
                    && glyph.normalized_coords_hash == glyph.batch_key.normalized_coords_hash
                    && glyph.font_data.is_some()
            }),
            "snapshot glyphs should preserve source/style/font identity: {snapshot:?}"
        );
    }

    #[test]
    fn text_layout_snapshot_exposes_wrapped_line_ranges() {
        let ifc = fixture(72.0);
        let snapshot = ifc.text_layout_snapshot();

        assert!(
            snapshot.lines.len() >= 2,
            "narrow IFC layout should produce multiple snapshot lines: {snapshot:?}"
        );
        assert!(snapshot.lines.iter().all(|line| line.height > 0.0));
        assert!(snapshot.lines.windows(2).all(|pair| {
            pair[0].line_index + 1 == pair[1].line_index && pair[0].range.end <= pair[1].range.end
        }));
        assert!(snapshot.lines.iter().all(|line| {
            line.glyphs.iter().all(|glyph| {
                line.range.start <= glyph.cluster_range.start
                    && glyph.cluster_range.end <= line.range.end
                    && glyph.source != BOX_NODE
            })
        }));
    }

    #[test]
    fn text_layout_snapshot_keeps_atomic_boxes_out_of_glyph_lines() {
        let ifc = fixture(180.0);
        let snapshot = ifc.text_layout_snapshot();

        assert_eq!(snapshot.inline_boxes.len(), 1);
        assert_eq!(snapshot.inline_boxes[0].source, BOX_NODE);
        assert!(
            snapshot
                .lines
                .iter()
                .flat_map(|line| line.glyphs.iter())
                .all(|glyph| glyph.source != BOX_NODE)
        );
        assert!(
            snapshot.lines.iter().any(|line| !line.glyphs.is_empty()),
            "text glyph payload should still coexist with inline box placements: {snapshot:?}"
        );
    }

    #[test]
    fn text_layout_snapshot_updates_paint_for_brush_only_cache_update() {
        let mut cache = InlineIfcCache::new();
        let previous = cache_fixture_input();
        cache.put(previous);
        let previous_snapshot = cache
            .lookup_input(&cache_fixture_input())
            .cached_entry()
            .expect("same input should be cached")
            .context()
            .text_layout_snapshot();
        let previous_shape = text_layout_snapshot_shape(&previous_snapshot);
        let previous_handles = previous_snapshot
            .lines
            .iter()
            .flat_map(|line| line.glyphs.iter())
            .map(|glyph| {
                assert_font_render_handle(&glyph.font_data, glyph.font_data_id, glyph.font_index);
                (
                    glyph.cluster_range.clone(),
                    glyph
                        .font_data
                        .as_ref()
                        .expect("previous glyph should carry FontData")
                        .data
                        .id(),
                    glyph.font_index,
                    glyph.normalized_coords_hash,
                )
            })
            .collect::<Vec<_>>();

        let mut next = cache_fixture_input();
        let InlineIfcItem::Span { children, .. } = &mut next.items[0] else {
            panic!("expected root span");
        };
        let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
            panic!("expected text span");
        };
        style.as_mut().unwrap().brush = [240, 20, 20, 255];

        let next_snapshot = {
            let update = cache.update(next);
            assert_eq!(update.invalidation, InlineIfcInvalidation::RepaintOnly);
            update.entry.context().text_layout_snapshot()
        };

        assert_eq!(
            previous_shape,
            text_layout_snapshot_shape(&next_snapshot),
            "brush-only updates should keep line/glyph positioning shape stable"
        );
        assert!(
            next_snapshot
                .lines
                .iter()
                .flat_map(|line| line.glyphs.iter())
                .all(|glyph| glyph.style.brush == [240, 20, 20, 255]
                    && glyph.batch_key.brush == [240, 20, 20, 255])
        );
        let next_handles = next_snapshot
            .lines
            .iter()
            .flat_map(|line| line.glyphs.iter())
            .map(|glyph| {
                assert_font_render_handle(&glyph.font_data, glyph.font_data_id, glyph.font_index);
                (
                    glyph.cluster_range.clone(),
                    glyph
                        .font_data
                        .as_ref()
                        .expect("next glyph should carry FontData")
                        .data
                        .id(),
                    glyph.font_index,
                    glyph.normalized_coords_hash,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            previous_handles, next_handles,
            "brush-only cache updates must not mutate font render handles or variation hashes"
        );
    }

    #[test]
    fn text_pass_adapter_preserves_snapshot_glyph_payload() {
        let ifc = fixture(180.0);
        let snapshot = ifc.text_layout_snapshot();
        let adapter = snapshot.text_pass_paint_input();
        let line = snapshot
            .lines
            .iter()
            .find(|line| !line.glyphs.is_empty())
            .expect("fixture should produce text glyphs");
        let glyph = line.glyphs.first().expect("line should have a glyph");
        let adapted = adapter
            .glyphs
            .iter()
            .find(|adapted| {
                adapted.line_index == line.line_index
                    && adapted.glyph_id == glyph.glyph_id
                    && adapted.cluster_range == glyph.cluster_range
            })
            .expect("adapter should expose the snapshot glyph");
        let adapted_line = adapter
            .lines
            .iter()
            .find(|adapted_line| adapted_line.line_index == line.line_index)
            .expect("adapter should expose the snapshot line");

        assert_eq!(adapted_line.x, line.x);
        assert_eq!(adapted_line.y, line.y);
        assert_eq!(adapted_line.width, line.width);
        assert_eq!(adapted_line.height, line.height);
        assert_eq!(adapted_line.baseline, line.baseline);
        assert_eq!(adapted_line.range, line.range);
        assert_eq!(adapted.source, glyph.source);
        assert_eq!(adapted.style, glyph.style);
        assert_eq!(adapted.batch_key, glyph.batch_key);
        assert_font_render_handle(&adapted.font_data, adapted.font_data_id, adapted.font_index);
        assert_font_render_handle(&glyph.font_data, glyph.font_data_id, glyph.font_index);
        assert_eq!(adapted.font_data_id, glyph.font_data_id);
        assert_eq!(adapted.font_index, glyph.font_index);
        assert_eq!(adapted.normalized_coords_hash, glyph.normalized_coords_hash);
        assert_eq!(
            adapted.batch_key.normalized_coords_hash,
            glyph.normalized_coords_hash
        );
        assert_eq!(adapted.font_size, glyph.font_size);
        assert_eq!(adapted.x, glyph.x);
        assert_eq!(adapted.baseline_y, line.y + line.baseline);
        assert_eq!(adapted.glyph_x, glyph.x - line.x);
        assert_eq!(adapted.glyph_y, glyph.y - (line.y + line.baseline));
        assert_eq!(adapted.advance, glyph.advance);
        assert_eq!(adapted.color, brush_to_text_color(glyph.batch_key.brush));
    }

    #[test]
    fn text_pass_adapter_batches_by_color_and_font_identity() {
        let input = InlineIfcInput::new(vec![
            InlineIfcItem::TextSpan {
                source: ROOT,
                text: "red ".to_string(),
                style: Some(style_with_size([255, 0, 0, 255], 400, 14.0)),
            },
            InlineIfcItem::TextSpan {
                source: OUTER,
                text: "green ".to_string(),
                style: Some(style_with_size([0, 180, 0, 255], 400, 14.0)),
            },
            InlineIfcItem::TextSpan {
                source: INNER,
                text: "large".to_string(),
                style: Some(style_with_size([0, 180, 0, 255], 700, 20.0)),
            },
        ])
        .with_max_width(400.0);
        let adapter = InlineFormattingContext::build(input).text_pass_paint_input();

        assert!(
            adapter.batches.len() >= 3,
            "brush, font size, and font weight changes should split adapter batches: {adapter:?}",
        );
        for batch in &adapter.batches {
            assert!(!batch.glyph_indices.is_empty());
            assert_eq!(batch.color, brush_to_text_color(batch.batch_key.brush));
            assert_eq!(batch.font_data_id, batch.batch_key.font_data_id);
            assert_eq!(batch.font_index, batch.batch_key.font_index);
            assert_eq!(
                batch.normalized_coords_hash,
                batch.batch_key.normalized_coords_hash
            );
            assert!((batch.font_size - batch.batch_key.font_size()).abs() < 0.01);
            assert_eq!(batch.font_weight, batch.batch_key.font_weight);
            for glyph_index in &batch.glyph_indices {
                let glyph = &adapter.glyphs[*glyph_index];
                assert_eq!(glyph.batch_key, batch.batch_key);
                assert_eq!(glyph.color, batch.color);
                assert_eq!(glyph.font_data_id, batch.font_data_id);
                assert_eq!(glyph.font_index, batch.font_index);
                assert_eq!(glyph.normalized_coords_hash, batch.normalized_coords_hash);
                assert_font_render_handle(&glyph.font_data, glyph.font_data_id, glyph.font_index);
            }
        }
    }

    #[test]
    fn text_pass_adapter_keeps_atomic_inline_boxes_out_of_glyphs() {
        let ifc = fixture(180.0);
        let snapshot = ifc.text_layout_snapshot();
        let adapter = snapshot.text_pass_paint_input();

        assert_eq!(snapshot.inline_boxes.len(), 1);
        assert_eq!(snapshot.inline_boxes[0].source, BOX_NODE);
        assert!(adapter.glyphs.iter().all(|glyph| glyph.source != BOX_NODE));
        assert!(adapter.batches.iter().all(|batch| {
            batch
                .glyph_indices
                .iter()
                .all(|glyph_index| adapter.glyphs[*glyph_index].source != BOX_NODE)
        }));
    }

    #[test]
    fn text_pass_adapter_is_available_only_through_explicit_snapshot_conversion() {
        let ifc = fixture(180.0);
        let snapshot = ifc.text_layout_snapshot();

        assert!(
            !snapshot.lines.is_empty(),
            "snapshot construction should not require the text pass adapter"
        );

        let from_snapshot = snapshot.text_pass_paint_input();
        let from_context = ifc.text_pass_paint_input();
        assert_eq!(from_snapshot, from_context);
        assert!(!from_context.glyphs.is_empty());
        assert!(!from_context.batches.is_empty());
    }

    #[test]
    fn atomic_inline_box_uses_measured_size_for_parley_placement() {
        let input = InlineIfcInput::new(vec![
            InlineIfcItem::TextSpan {
                source: ROOT,
                text: "before ".to_string(),
                style: None,
            },
            InlineIfcItem::AtomicInlineBox {
                source: BOX_NODE,
                measurement: measured_box(42.0, 21.0),
            },
            InlineIfcItem::TextSpan {
                source: ROOT,
                text: " after".to_string(),
                style: None,
            },
        ])
        .with_max_width(240.0);
        let ifc = InlineFormattingContext::build(input);

        let placement = ifc
            .inline_box_placements()
            .into_iter()
            .find(|placement| placement.source == BOX_NODE)
            .expect("measured atomic box should have a placement");

        assert!((placement.width - 42.0).abs() < 0.01);
        assert!((placement.height - 21.0).abs() < 0.01);
        assert_eq!(ifc.source_for_inline_box(placement.id), Some(BOX_NODE));
    }

    #[test]
    fn atomic_inline_box_remains_whole_when_remaining_line_width_is_too_small() {
        let input = InlineIfcInput::new(vec![
            InlineIfcItem::TextSpan {
                source: ROOT,
                text: "prefix ".to_string(),
                style: None,
            },
            InlineIfcItem::AtomicInlineBox {
                source: BOX_NODE,
                measurement: measured_box(80.0, 16.0),
            },
            InlineIfcItem::TextSpan {
                source: ROOT,
                text: " suffix".to_string(),
                style: None,
            },
        ])
        .with_max_width(64.0);
        let ifc = InlineFormattingContext::build(input);
        let placements = ifc.inline_box_placements();

        assert_eq!(
            placements.len(),
            1,
            "atomic inline boxes should produce one positioned box, not text/glyph fragments"
        );
        assert_eq!(placements[0].source, BOX_NODE);
        assert!((placements[0].width - 80.0).abs() < 0.01);
        assert!((placements[0].height - 16.0).abs() < 0.01);
    }

    #[test]
    fn atomic_measure_constraints_preserve_future_element_measure_inputs() {
        let constraints = InlineIfcAtomicMeasureConstraints {
            max_width: Some(144.0),
            available_height: Some(96.0),
            viewport: Some(InlineIfcSize::new(320.0, 240.0)),
            percent_base: InlineIfcPercentBase::new(Some(180.0), Some(72.0)),
            sizing: InlineIfcAtomicSizingRules {
                min_width: Some(24.0),
                max_width: Some(128.0),
                min_height: Some(12.0),
                max_height: Some(64.0),
                intrinsic_size: Some(InlineIfcIntrinsicSize::new(
                    18.0,
                    160.0,
                    Some(80.0),
                    Some(32.0),
                )),
            },
        };
        let input = InlineIfcInput::new(vec![InlineIfcItem::AtomicInlineBox {
            source: BOX_NODE,
            measurement: InlineIfcMeasuredAtomicBox::new(
                InlineIfcSize::new(64.0, 20.0),
                constraints,
            ),
        }])
        .with_max_width(180.0);
        let ifc = InlineFormattingContext::build(input);
        let mapping = ifc.inline_boxes().first().expect("expected inline box");

        assert_eq!(mapping.measurement.constraints.max_width, Some(144.0));
        assert_eq!(mapping.measurement.constraints.available_height, Some(96.0));
        assert_eq!(
            mapping.measurement.constraints.viewport,
            Some(InlineIfcSize::new(320.0, 240.0))
        );
        assert_eq!(
            mapping.measurement.constraints.percent_base,
            InlineIfcPercentBase::new(Some(180.0), Some(72.0))
        );
        assert_eq!(
            mapping
                .measurement
                .constraints
                .sizing
                .intrinsic_size
                .map(|size| size.max_content_width),
            Some(160.0)
        );
    }

    #[test]
    fn multiple_atomic_inline_boxes_keep_distinct_sources_and_measurements() {
        let input = InlineIfcInput::new(vec![
            InlineIfcItem::AtomicInlineBox {
                source: BOX_NODE,
                measurement: measured_box(20.0, 10.0),
            },
            InlineIfcItem::TextSpan {
                source: ROOT,
                text: " gap ".to_string(),
                style: None,
            },
            InlineIfcItem::AtomicInlineBox {
                source: SECOND_BOX_NODE,
                measurement: measured_box(36.0, 12.0),
            },
        ])
        .with_max_width(160.0);
        let ifc = InlineFormattingContext::build(input);
        let placements = ifc.inline_box_placements();

        assert_eq!(placements.len(), 2);
        assert_ne!(placements[0].id, placements[1].id);
        assert_eq!(ifc.source_for_inline_box(placements[0].id), Some(BOX_NODE));
        assert_eq!(
            ifc.source_for_inline_box(placements[1].id),
            Some(SECOND_BOX_NODE)
        );
        assert!((placements[0].width - 20.0).abs() < 0.01);
        assert!((placements[1].width - 36.0).abs() < 0.01);

        let first_package = ifc.atomic_box_placement_package(BOX_NODE);
        let second_package = ifc.atomic_box_placement_package(SECOND_BOX_NODE);
        assert_eq!(first_package.source, BOX_NODE);
        assert_eq!(second_package.source, SECOND_BOX_NODE);
        assert_eq!(first_package.placements.len(), 1);
        assert_eq!(second_package.placements.len(), 1);
        assert_eq!(first_package.placements[0].source, BOX_NODE);
        assert_eq!(second_package.placements[0].source, SECOND_BOX_NODE);
        assert_ne!(
            first_package.placements[0].id,
            second_package.placements[0].id
        );
        assert!((first_package.placements[0].rect.width - 20.0).abs() < 0.01);
        assert!((second_package.placements[0].rect.width - 36.0).abs() < 0.01);
    }

    #[test]
    fn hit_test_point_on_nested_text_returns_deepest_source_and_byte() {
        let ifc = fixture(240.0);
        let strong_start = ifc.backing_text().find("strong").unwrap();
        let caret = ifc
            .caret_geometry_for_byte(strong_start + 1, InlineIfcCaretAffinity::Downstream)
            .expect("nested strong byte should have caret geometry");

        let hit = ifc
            .hit_test_point(caret.x, caret.y)
            .expect("point near nested text should hit text");

        let InlineIfcHitTarget::Text {
            source,
            byte_index,
            line_index,
            style,
        } = hit.target
        else {
            panic!("expected text hit target: {hit:?}");
        };
        assert_eq!(source, INNER);
        assert!(byte_index >= strong_start);
        assert!(byte_index <= strong_start + "strong".len());
        assert_eq!(line_index, caret.line_index);
        assert_eq!(style.map(|style| style.font_weight), Some(700));
    }

    #[test]
    fn hit_test_point_on_atomic_inline_box_returns_inline_box_source() {
        let ifc = fixture(240.0);
        let placement = ifc
            .inline_box_placements()
            .into_iter()
            .find(|placement| placement.source == BOX_NODE)
            .expect("atomic box should have a placement");

        let hit = ifc
            .hit_test_point(
                placement.x + placement.width / 2.0,
                placement.y + placement.height / 2.0,
            )
            .expect("point inside atomic box should hit inline box");

        assert_eq!(
            hit.target,
            InlineIfcHitTarget::InlineBox {
                source: BOX_NODE,
                id: placement.id,
                line_index: placement.line_index,
            }
        );
    }

    #[test]
    fn caret_geometry_for_nested_byte_preserves_source_and_finite_rect() {
        let ifc = fixture(240.0);
        let strong_start = ifc.backing_text().find("strong").unwrap();
        let caret = ifc
            .caret_geometry_for_byte(strong_start + 2, InlineIfcCaretAffinity::Downstream)
            .expect("nested byte should have caret geometry");

        assert_eq!(caret.source, INNER);
        assert_eq!(caret.byte_index, strong_start + 2);
        assert_eq!(caret.affinity, InlineIfcCaretAffinity::Downstream);
        assert!(caret.x.is_finite());
        assert!(caret.y.is_finite());
        assert!(caret.height.is_finite() && caret.height > 0.0);
        assert_eq!(caret.style.map(|style| style.brush), Some([3, 3, 3, 255]));
    }

    #[test]
    fn selection_across_line_wrap_returns_text_rects_with_source_and_style() {
        let ifc = fixture(72.0);
        let outer_range = ifc
            .source_ranges()
            .iter()
            .find(|range| range.source == OUTER && range.kind == InlineIfcSourceKind::Span)
            .expect("outer span should have a range")
            .range
            .clone();

        let rects = ifc.selection_rects_for_global_range(outer_range);

        assert!(
            rects.len() >= 2,
            "wrapped selection should emit multiple per-line rects: {rects:?}"
        );
        assert!(
            rects
                .windows(2)
                .any(|pair| pair[0].line_index != pair[1].line_index),
            "selection rects should preserve visual line identity: {rects:?}"
        );
        assert!(
            rects.iter().all(|rect| {
                (rect.source == OUTER || rect.source == INNER)
                    && rect.rect.width > 0.0
                    && rect.rect.height > 0.0
                    && rect.style.is_some()
            }),
            "selection rects should keep deepest text source and style: {rects:?}"
        );
    }

    #[test]
    fn source_filtered_selection_keeps_only_matching_text_source() {
        let ifc = fixture(72.0);
        let outer_range = ifc
            .source_ranges()
            .iter()
            .find(|range| range.source == OUTER && range.kind == InlineIfcSourceKind::Span)
            .expect("outer span should have a range")
            .range
            .clone();

        let rects = ifc.selection_rects_for_source_range(INNER, outer_range);

        assert!(!rects.is_empty(), "inner selection rects should exist");
        assert!(
            rects.iter().all(|rect| rect.source == INNER),
            "source-filtered selection should only return the requested source: {rects:?}"
        );
        assert!(
            rects
                .iter()
                .all(|rect| rect.style.as_ref().map(|style| style.font_weight) == Some(700)),
            "source-filtered selection should keep resolved style: {rects:?}"
        );
    }

    #[test]
    fn soft_wrap_boundary_exposes_distinct_upstream_and_downstream_caret_slots() {
        let ifc = fixture(72.0);
        let stops = ifc.visual_caret_stops();
        let soft_tail = stops
            .iter()
            .find(|stop| {
                stop.is_soft_wrap_boundary
                    && stop.is_line_tail
                    && stop.affinity == InlineIfcCaretAffinity::Upstream
            })
            .expect("wrapped layout should expose an upstream soft-wrap tail stop");
        let soft_head = stops
            .iter()
            .find(|stop| {
                stop.is_soft_wrap_boundary
                    && stop.is_line_head
                    && stop.affinity == InlineIfcCaretAffinity::Downstream
                    && stop.byte_index == soft_tail.byte_index
            })
            .expect("same byte should expose a downstream soft-wrap head stop");

        assert_eq!(soft_tail.byte_index, soft_head.byte_index);
        assert_ne!(
            soft_tail.line_index, soft_head.line_index,
            "same insertion byte at a soft wrap should keep both visual slots: {stops:?}"
        );
        assert!(soft_tail.style.is_some());
        assert!(soft_head.style.is_some());
    }

    #[test]
    fn visual_caret_stops_include_line_heads_and_tails_for_navigation_maps() {
        let ifc = fixture(72.0);
        let stops = ifc.visual_caret_stops();
        let line_count = ifc.layout.lines().count();

        for line_index in 0..line_count {
            assert!(
                stops
                    .iter()
                    .any(|stop| stop.line_index == line_index && stop.is_line_head),
                "line {line_index} should have a visual head caret stop: {stops:?}"
            );
            assert!(
                stops
                    .iter()
                    .any(|stop| stop.line_index == line_index && stop.is_line_tail),
                "line {line_index} should have a visual tail caret stop: {stops:?}"
            );
        }
        assert!(
            stops.iter().all(|stop| {
                stop.x.is_finite()
                    && stop.y.is_finite()
                    && stop.height > 0.0
                    && stop.style.is_some()
            }),
            "visual caret stops should carry finite geometry and resolved style: {stops:?}"
        );
    }

    #[test]
    fn utf8_and_combining_mark_selection_clamps_to_char_boundaries() {
        let text = "aé e\u{301} 中z";
        let input = InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
            source: ROOT,
            text: text.to_string(),
            style: Some(style([12, 34, 56, 255], 400)),
        }])
        .with_max_width(240.0);
        let ifc = InlineFormattingContext::build(input);
        let accent_start = ifc.backing_text().find('é').unwrap();
        let cjk_start = ifc.backing_text().find('中').unwrap();

        let rects = ifc.selection_rects_for_global_range((accent_start + 1)..(cjk_start + 2));

        assert!(
            !rects.is_empty(),
            "UTF-8 selection should not panic or disappear"
        );
        assert!(
            rects.iter().all(|rect| {
                ifc.backing_text().is_char_boundary(rect.range.start)
                    && ifc.backing_text().is_char_boundary(rect.range.end)
                    && rect.rect.width > 0.0
                    && rect.rect.height > 0.0
            }),
            "selection rect ranges should be clamped to UTF-8 char boundaries: {rects:?}"
        );
        assert_eq!(
            rects.first().map(|rect| rect.range.start),
            Some(accent_start)
        );
        assert_eq!(rects.last().map(|rect| rect.range.end), Some(cjk_start));
    }

    #[test]
    fn nested_span_boundary_selection_splits_by_source_and_style() {
        let ifc = fixture(240.0);
        let outer_start = ifc.backing_text().find("outer").unwrap();
        let strong_end = ifc.backing_text().find("strong").unwrap() + "strong".len();

        let rects = ifc.selection_rects_for_global_range(outer_start..strong_end);

        assert!(
            rects.iter().any(|rect| rect.source == OUTER
                && rect.style.as_ref().map(|style| style.brush) == Some([2, 2, 2, 255])),
            "selection should keep the outer text source/style: {rects:?}"
        );
        assert!(
            rects.iter().any(|rect| rect.source == INNER
                && rect.style.as_ref().map(|style| style.font_weight) == Some(700)),
            "selection should split at nested span source/style boundary: {rects:?}"
        );
    }

    #[test]
    fn selection_near_atomic_inline_box_does_not_select_atomic_source() {
        let ifc = fixture(240.0);
        let selection_start = ifc.backing_text().find("after").unwrap();
        let selection_end = ifc.backing_text().find("box").unwrap() + "box".len();

        let rects = ifc.selection_rects_for_global_range(selection_start..selection_end);

        assert!(!rects.is_empty(), "text around atomic box should select");
        assert!(
            rects.iter().all(|rect| rect.source != BOX_NODE),
            "text selection primitives should not include atomic boxes implicitly: {rects:?}"
        );
        assert!(
            ifc.inline_box_placements()
                .iter()
                .any(|placement| placement.source == BOX_NODE),
            "atomic box remains available for explicit hit-test/selection handling"
        );
    }
}
