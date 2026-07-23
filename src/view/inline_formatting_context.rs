// P1-P7 staged IFC scaffold. Most primitives are intentionally crate-visible
// before every formal call site is switched, so example builds should not be
// dominated by dead-code warnings while the rollout remains gated.
#![allow(dead_code)]

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use parley::{
    Affinity, Alignment as ParleyAlignment, AlignmentOptions, Cursor as ParleyCursor, FontData,
    FontFamily, FontFamilyName, FontWeight, InlineBox, InlineBoxKind, Layout as ParleyLayout,
    LineHeight, OverflowWrap, PositionedLayoutItem, StyleProperty, TextWrapMode,
};

use crate::style::srgb_to_linear;
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
        /// Horizontal border+padding of the span's box (left, right).
        /// Reserved in the line as zero-height spacer inline boxes so
        /// following content does not overlap the decoration box.
        edge_insets: [f32; 2],
    },
    AtomicInlineBox {
        source: InlineIfcSourceId,
        measurement: InlineIfcMeasuredAtomicBox,
    },
    /// Horizontal spacing between adjacent children of a `Layout::Inline`
    /// container. It participates in line advance/wrapping without
    /// contributing to the line box height or producing paint geometry.
    GapSpacer {
        source: InlineIfcSourceId,
        width: f32,
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
    pub(crate) font_families: Arc<[String]>,
    pub(crate) vertical_align: crate::style::VerticalAlign,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InlineIfcStyleKey {
    pub(crate) font_size_bits: u32,
    pub(crate) line_height_bits: u32,
    pub(crate) font_weight: u16,
    pub(crate) font_families: Arc<[String]>,
    pub(crate) vertical_align: crate::style::VerticalAlign,
}

impl InlineIfcStyleKey {
    fn from_style(style: &InlineIfcStyle) -> Self {
        Self {
            font_size_bits: style.font_size.max(1.0).to_bits(),
            line_height_bits: style.line_height.max(0.1).to_bits(),
            font_weight: style.font_weight,
            font_families: style.font_families.clone(),
            vertical_align: style.vertical_align,
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
        edge_insets_bits: [u32; 2],
    },
    AtomicInlineBox {
        source: InlineIfcSourceId,
        shape_key: InlineIfcAtomicBoxShapeKey,
    },
    GapSpacer {
        source: InlineIfcSourceId,
        width_bits: u32,
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
    GapSpacer {
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
    access_generation: u64,
}

impl InlineIfcCachedEntry {
    pub(crate) fn context(&self) -> &InlineFormattingContext {
        &self.context
    }

    pub(crate) fn cache_key(&self) -> &InlineIfcCacheKey {
        self.context.cache_key()
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
        if let Some(entry) = self.entries.get_mut(shape_key) {
            entry.access_generation = self.access_generation;
        }
    }

    fn evict_to_capacity(&mut self, keep: &InlineIfcShapeCacheKey) {
        while self.entries.len() > INLINE_IFC_CACHE_MAX_ENTRIES {
            let Some(coldest) = self
                .entries
                .iter()
                .filter(|(key, _)| *key != keep)
                .min_by_key(|(_, entry)| entry.access_generation)
                .map(|(key, _)| key.clone())
            else {
                return;
            };
            self.entries.remove(&coldest);
        }
    }

    fn slim_cold_entries(&mut self, keep: &InlineIfcShapeCacheKey) {
        for (key, entry) in &mut self.entries {
            if key != keep {
                entry.context.clear_derived_caches();
            }
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
                access_generation: 0,
            },
        );
        self.touch(&shape_key);
        self.evict_to_capacity(&shape_key);
        self.slim_cold_entries(&shape_key);
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
            self.slim_cold_entries(&shape_key);
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
                access_generation: 0,
            },
        );
        self.touch(&shape_key);
        self.evict_to_capacity(&shape_key);
        self.slim_cold_entries(&shape_key);
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
            font_families: Arc::default(),
            vertical_align: crate::style::VerticalAlign::Baseline,
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
    pub(crate) role: InlineIfcInlineBoxRole,
}

/// What an inline box in the parley layout stands for: a real atomic
/// child, or a zero-height spacer reserving horizontal advance in the line.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum InlineIfcInlineBoxRole {
    Atomic,
    SpanEdgeSpacer,
    GapSpacer,
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
    pub(crate) border_colors: [[f32; 4]; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InlineIfcElementDecorationDrawRectStyle {
    pub(crate) style_key: InlineIfcPaintStyleKey,
    pub(crate) fill_color: [f32; 4],
    pub(crate) opacity: f32,
    pub(crate) border_widths: [f32; 4],
    pub(crate) border_colors: [[f32; 4]; 4],
}

impl InlineIfcElementDecorationDrawRectStyle {
    pub(crate) fn new(
        style_key: InlineIfcPaintStyleKey,
        fill_color: [f32; 4],
        opacity: f32,
        border_widths: [f32; 4],
        border_color: [f32; 4],
    ) -> Self {
        Self::new_with_side_colors(
            style_key,
            fill_color,
            opacity,
            border_widths,
            [border_color; 4],
        )
    }

    pub(crate) fn new_with_side_colors(
        style_key: InlineIfcPaintStyleKey,
        fill_color: [f32; 4],
        opacity: f32,
        border_widths: [f32; 4],
        border_colors: [[f32; 4]; 4],
    ) -> Self {
        Self {
            style_key,
            fill_color,
            opacity: opacity.clamp(0.0, 1.0),
            border_widths: border_widths.map(|width| width.max(0.0)),
            border_colors,
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

    #[cfg(test)]
    pub(crate) fn damage_atomic_package_cardinality_for_test(
        &mut self,
        cache_key: &InlineIfcCacheKey,
        source: InlineIfcSourceId,
        duplicate: bool,
    ) {
        let shape_key = InlineIfcShapeCacheKey::from_cache_key(cache_key);
        let context = &mut self
            .cache
            .entries
            .get_mut(&shape_key)
            .expect("test fixture must retain the current IFC context")
            .context;
        if duplicate {
            let replacement = context
                .inline_boxes
                .iter_mut()
                .find(|mapping| {
                    mapping.role == InlineIfcInlineBoxRole::Atomic && mapping.source != source
                })
                .expect("duplicate-package fixture requires a second atomic mapping");
            replacement.source = source;
        } else {
            context
                .inline_boxes
                .retain(|mapping| mapping.source != source);
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
    pub(crate) role: InlineIfcInlineBoxRole,
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
    /// Inline boxes nested inside each fragmentable span. This preserves
    /// decoration geometry for spans whose content is atomic-only and has
    /// no backing-text byte range.
    span_inline_box_ids: HashMap<InlineIfcSourceId, Vec<u64>>,
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
    // The full text-pass paint payload is memoized once. Inline-root install
    // plans retain their source-filtered payloads, so the context does not
    // also keep a duplicate per-source copy.
    paint_input_cache: std::cell::OnceCell<InlineIfcTextPassPaintInput>,
    /// Per-source per-line rect maps, built in one glyph pass. Queries
    /// like `source_line_rects` used to rescan every glyph per source,
    /// which made per-segment callers (TextArea child placement) O(n²).
    source_line_rects_cache:
        std::cell::OnceCell<HashMap<InlineIfcSourceId, Vec<InlineIfcPaintRect>>>,
    source_text_line_rects_cache:
        std::cell::OnceCell<HashMap<InlineIfcSourceId, Vec<(usize, InlineIfcPaintRect)>>>,
}

impl InlineFormattingContext {
    fn clear_derived_caches(&mut self) {
        self.glyph_items_cache.take();
        self.snapshot_cache.take();
        self.caret_stops_cache.take();
        self.paint_input_cache.take();
        self.source_line_rects_cache.take();
        self.source_text_line_rects_cache.take();
    }

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
            span_inline_box_ids: builder.span_inline_box_ids,
            cache_key,
            glyph_items_cache: std::cell::OnceCell::new(),
            snapshot_cache: std::cell::OnceCell::new(),
            caret_stops_cache: std::cell::OnceCell::new(),
            paint_input_cache: std::cell::OnceCell::new(),
            source_line_rects_cache: std::cell::OnceCell::new(),
            source_text_line_rects_cache: std::cell::OnceCell::new(),
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
        let glyphs = self.glyph_items_ref();
        let inline_boxes = self.inline_box_placements();
        let mut run_metrics = HashMap::<usize, (f32, f32)>::new();
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
                let mut horizontal_extent: Option<(f32, f32)> = None;
                let mut vertical_extent: Option<(f32, f32)> = None;
                let mut include = |left: f32, right: f32| {
                    horizontal_extent = Some(match horizontal_extent {
                        Some((current_left, current_right)) => {
                            (current_left.min(left), current_right.max(right))
                        }
                        None => (left, right),
                    });
                };
                let mut include_vertical = |top: f32, bottom: f32| {
                    vertical_extent = Some(match vertical_extent {
                        Some((current_top, current_bottom)) => {
                            (current_top.min(top), current_bottom.max(bottom))
                        }
                        None => (top, bottom),
                    });
                };

                if start < end {
                    for glyph in glyphs.iter().filter(|glyph| {
                        glyph.line_index == line_index
                            && glyph.cluster_range.start < source.range.end
                            && glyph.cluster_range.end > source.range.start
                    }) {
                        include(glyph.x, glyph.x + glyph.advance.max(0.0));
                        let (ascent, descent) = run_metrics
                            .get(&glyph.run_index)
                            .copied()
                            .unwrap_or((glyph.font_size * 0.88, glyph.font_size * 0.2));
                        let item_top = metrics.baseline - ascent;
                        let item_height = (ascent + descent).max(0.0);
                        let vertical_offset = inline_vertical_align_offset(
                            glyph.style.vertical_align,
                            metrics.block_min_coord,
                            metrics.line_height,
                            item_top,
                            item_height,
                        );
                        include_vertical(
                            item_top + vertical_offset,
                            item_top + vertical_offset + item_height,
                        );
                    }
                }

                if let Some(ids) = self.span_inline_box_ids.get(&source.source) {
                    for inline_box in inline_boxes.iter().filter(|inline_box| {
                        inline_box.line_index == line_index && ids.contains(&inline_box.id)
                    }) {
                        include(inline_box.x, inline_box.x + inline_box.width.max(0.0));
                        include_vertical(inline_box.y, inline_box.y + inline_box.height.max(0.0));
                    }
                }

                let Some((x0, x1)) = horizontal_extent else {
                    continue;
                };
                let (y0, y1) = vertical_extent.unwrap_or((
                    metrics.block_min_coord,
                    metrics.block_min_coord + metrics.line_height,
                ));
                fragments.push(InlineIfcDecorationFragment {
                    line_index,
                    source: source.source,
                    range: start..end.max(start),
                    style: source.style.clone().or_else(|| {
                        (start < end)
                            .then(|| self.style_at_byte(start))
                            .flatten()
                            .cloned()
                    }),
                    x0,
                    x1,
                    y0,
                    y1,
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
        let line_ranges = self
            .layout
            .lines()
            .map(|line| line.text_range())
            .collect::<Vec<_>>();
        for (line_index, line) in self.layout.lines().enumerate() {
            let trailing_wrap_whitespace_start = line_ranges
                .get(line_index)
                .cloned()
                .filter(|_| self.is_soft_wrap_after(&line_ranges, line_index))
                .and_then(|range| self.trailing_whitespace_start(range));
            let mut consumed_glyphs_by_run = HashMap::<usize, usize>::new();
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };

                let run = glyph_run.run();
                let run_metrics = run.metrics();
                let line_metrics = line.metrics();
                let item_top = line_metrics.baseline - run_metrics.ascent;
                let item_height = (run_metrics.ascent + run_metrics.descent).max(0.0);
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
                    if trailing_wrap_whitespace_start
                        .is_some_and(|start| cluster_range.start >= start)
                    {
                        continue;
                    }
                    let Some(source) = self.source_for_byte(cluster_range.start) else {
                        continue;
                    };
                    let Some(style) = self.style_at_byte(cluster_range.start) else {
                        continue;
                    };
                    let vertical_offset = inline_vertical_align_offset(
                        style.vertical_align,
                        line_metrics.block_min_coord,
                        line_metrics.line_height,
                        item_top,
                        item_height,
                    );
                    output.push(InlineIfcGlyphItem {
                        line_index,
                        run_index,
                        source,
                        cluster_range,
                        glyph_id: glyph.id,
                        x: glyph.x,
                        y: glyph.y + vertical_offset,
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
        let glyph_items = self.glyph_items_ref();
        let inline_boxes = self
            .inline_box_placements()
            .into_iter()
            .filter(|placement| placement.role == InlineIfcInlineBoxRole::Atomic)
            .collect::<Vec<_>>();
        let decorations = self.decoration_paint_fragments();
        let mut lines = Vec::new();

        for (line_index, line) in self.layout.lines().enumerate() {
            let metrics = line.metrics();
            let range = line.text_range();
            let glyphs = glyph_items
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
                    batch_key: InlineIfcTextPaintBatchKey::from_glyph(glyph),
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

    pub(crate) fn text_pass_paint_input_for_source(
        &self,
        source: InlineIfcSourceId,
    ) -> Arc<InlineIfcTextPassPaintInput> {
        let input = self.text_pass_paint_input_ref();
        let glyphs = input
            .glyphs
            .iter()
            .filter(|glyph| glyph.source == source)
            .cloned()
            .collect::<Vec<_>>();
        let line_indices = glyphs
            .iter()
            .map(|glyph| glyph.line_index)
            .collect::<std::collections::HashSet<_>>();
        Arc::new(InlineIfcTextPassPaintInput {
            lines: input
                .lines
                .iter()
                .filter(|line| line_indices.contains(&line.line_index))
                .cloned()
                .collect(),
            batches: text_pass_batches_from_glyphs(&glyphs),
            glyphs,
            decorations: Vec::new(),
        })
    }

    /// Memoized paint payload. Built once per shaped context; every paint
    /// frame after the first borrows instead of rebuilding.
    pub(crate) fn text_pass_paint_input_ref(&self) -> &InlineIfcTextPassPaintInput {
        self.paint_input_cache
            .get_or_init(|| self.text_layout_snapshot_ref().text_pass_paint_input())
    }

    /// Materialize the retained text-pass payload during layout/frame
    /// preparation. Artifact recording must only borrow the result through
    /// [`Self::prepared_text_pass_paint_input_ref`].
    pub(crate) fn prepare_text_pass_paint_input(&self) {
        let _ = self.text_pass_paint_input_ref();
    }

    /// Side-effect-free access used by retained paint recording.
    pub(crate) fn prepared_text_pass_paint_input_ref(
        &self,
    ) -> Option<&InlineIfcTextPassPaintInput> {
        self.paint_input_cache.get()
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
                            role: mapping.role,
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
                let Some(mapping) = self.inline_boxes.iter().find(|mapping| {
                    mapping.id == inline_box.id
                        && mapping.source == source
                        && mapping.role == InlineIfcInlineBoxRole::Atomic
                }) else {
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

    #[cfg(test)]
    pub(crate) fn tamper_atomic_measurement_for_test(
        &mut self,
        source: InlineIfcSourceId,
        constraint: bool,
    ) {
        let mapping = self
            .inline_boxes
            .iter_mut()
            .find(|mapping| {
                mapping.role == InlineIfcInlineBoxRole::Atomic && mapping.source == source
            })
            .expect("test fixture must retain the atomic source mapping");
        if constraint {
            mapping.measurement.constraints.max_width = Some(999.0);
        } else {
            mapping.measurement.measured_size.width += 1.0;
        }
    }

    #[cfg(test)]
    pub(crate) fn tamper_atomic_insertion_byte_for_test(&mut self, source: InlineIfcSourceId) {
        let mapping = self
            .inline_boxes
            .iter_mut()
            .find(|mapping| {
                mapping.role == InlineIfcInlineBoxRole::Atomic && mapping.source == source
            })
            .expect("test fixture must retain the atomic source mapping");
        mapping.insertion_byte = mapping.insertion_byte.saturating_add(1);
    }

    /// Per-line bounding rects of the glyph runs contributed by `source`,
    /// in IFC content coordinates. Used as the hit-test/selection geometry
    /// for text owned by a unified inline IFC root.
    pub(crate) fn source_line_rects(&self, source: InlineIfcSourceId) -> Vec<InlineIfcPaintRect> {
        self.source_line_rects_map()
            .get(&source)
            .cloned()
            .unwrap_or_default()
    }

    /// One glyph pass building every source's per-line rects at once.
    /// Shaped state is immutable, so per-segment callers (one per
    /// TextArea run) amortize to O(1) lookups instead of rescanning
    /// every glyph per source.
    fn source_line_rects_map(&self) -> &HashMap<InlineIfcSourceId, Vec<InlineIfcPaintRect>> {
        self.source_line_rects_cache.get_or_init(|| {
            let snapshot = self.text_layout_snapshot_ref();
            let mut map: HashMap<InlineIfcSourceId, Vec<InlineIfcPaintRect>> = HashMap::new();
            let mut extents: HashMap<InlineIfcSourceId, (f32, f32)> = HashMap::new();
            for line in &snapshot.lines {
                extents.clear();
                for glyph in line.glyphs.iter() {
                    let start = glyph.x;
                    let end = start + glyph.advance;
                    let entry = extents.entry(glyph.source).or_insert((start, end));
                    entry.0 = entry.0.min(start);
                    entry.1 = entry.1.max(end);
                }
                for (source, (left, right)) in extents.drain() {
                    if right <= left {
                        continue;
                    }
                    map.entry(source).or_default().push(InlineIfcPaintRect {
                        x: left,
                        y: line.y,
                        width: right - left,
                        height: line.height,
                    });
                }
            }
            map
        })
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
        self.source_text_line_rects_map()
            .get(&source)
            .cloned()
            .unwrap_or_default()
    }

    /// One glyph pass building every source's per-line text-box rects at
    /// once; see `source_line_rects_map` for why.
    fn source_text_line_rects_map(
        &self,
    ) -> &HashMap<InlineIfcSourceId, Vec<(usize, InlineIfcPaintRect)>> {
        self.source_text_line_rects_cache.get_or_init(|| {
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

            // glyph_items carries the per-glyph source (innermost,
            // nesting-aware) and run index, plus the baseline-relative
            // line geometry.
            let glyphs = self.glyph_items_ref();
            let line_metrics: Vec<(f32, f32, f32)> = self
                .layout
                .lines()
                .map(|line| {
                    let metrics = line.metrics();
                    (
                        metrics.block_min_coord,
                        metrics.line_height,
                        metrics.baseline,
                    )
                })
                .collect();
            let mut by_source: HashMap<
                InlineIfcSourceId,
                std::collections::BTreeMap<usize, (f32, f32, f32, f32)>,
            > = HashMap::new();
            for glyph in glyphs.iter() {
                // Positioned glyph x is absolute (alignment offset included).
                let left = glyph.x;
                let right = left + glyph.advance;
                let (ascent, descent) = run_metrics
                    .get(&glyph.run_index)
                    .copied()
                    .unwrap_or((glyph.font_size * 0.88, glyph.font_size * 0.2));
                let entry = by_source
                    .entry(glyph.source)
                    .or_default()
                    .entry(glyph.line_index)
                    .or_insert((f32::MAX, f32::MIN, f32::MAX, f32::MIN));
                entry.0 = entry.0.min(left);
                entry.1 = entry.1.max(right);
                let (line_top, line_height, baseline) = line_metrics
                    .get(glyph.line_index)
                    .copied()
                    .unwrap_or((0.0, ascent + descent, ascent));
                let item_top = baseline - ascent;
                let item_height = (ascent + descent).max(0.0);
                let vertical_offset = inline_vertical_align_offset(
                    glyph.style.vertical_align,
                    line_top,
                    line_height,
                    item_top,
                    item_height,
                );
                entry.2 = entry.2.min(item_top + vertical_offset);
                entry.3 = entry.3.max(item_top + vertical_offset + item_height);
            }

            by_source
                .into_iter()
                .map(|(source, by_line)| {
                    let rects = by_line
                        .into_iter()
                        .filter_map(|(line_index, (left, right, top, bottom))| {
                            if right <= left || bottom <= top {
                                return None;
                            }
                            Some((
                                line_index,
                                InlineIfcPaintRect {
                                    x: left,
                                    y: top,
                                    width: right - left,
                                    height: (bottom - top).max(1.0),
                                },
                            ))
                        })
                        .collect();
                    (source, rects)
                })
                .collect()
        })
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
        for placement in self
            .inline_box_placements()
            .into_iter()
            .filter(|placement| placement.role == InlineIfcInlineBoxRole::Atomic)
        {
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
        let style = self.style_for_caret_byte(byte_index).cloned();
        let (y, height) = self
            .source_text_line_rects(source)
            .into_iter()
            .find(|(index, _)| *index == line_index)
            .map(|(_, rect)| (rect.y, rect.height))
            .unwrap_or((rect.y0 as f32, (rect.y1 - rect.y0).max(1.0) as f32));
        Some(InlineIfcCaretGeometry {
            source,
            byte_index,
            affinity,
            line_index,
            x: rect.x0 as f32,
            y,
            height,
            style,
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
        let inline_box_placements = self
            .inline_box_placements()
            .into_iter()
            .filter(|placement| placement.role == InlineIfcInlineBoxRole::Atomic)
            .collect::<Vec<_>>();
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
            let is_soft_wrap = self.is_soft_wrap_after(&line_text_ranges, line_index);
            // Parley consumes whitespace at a word-wrap boundary: it has
            // no glyph and must not create a second, invisible caret stop.
            // Keep the upper-line tail before that whitespace; the lower
            // line starts after it at `line_range.end`.
            let trailing_wrap_whitespace_start = is_soft_wrap
                .then(|| self.trailing_whitespace_start(line_range.clone()))
                .flatten();
            let mandatory_line_break_start = self.mandatory_line_break_start(&line_range);
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
                trailing_wrap_whitespace_start
                    .or(mandatory_line_break_start)
                    .unwrap_or(line_range.end),
                InlineIfcCaretAffinity::Upstream,
                false,
                true,
                trailing_wrap_whitespace_start.is_none() && is_soft_wrap,
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

    /// Returns the byte position before whitespace that Parley consumed at
    /// the end of a soft-wrapped visual line. The source text retains those
    /// characters for editing; they simply have no visual/caret position.
    fn trailing_whitespace_start(&self, range: Range<usize>) -> Option<usize> {
        let text = self.backing_text.get(range.clone())?;
        let visible_len = text.trim_end_matches(char::is_whitespace).len();
        (visible_len < text.len()).then_some(range.start + visible_len)
    }

    fn is_soft_wrap_after(&self, line_ranges: &[Range<usize>], line_index: usize) -> bool {
        let Some(range) = line_ranges.get(line_index) else {
            return false;
        };
        let Some(next) = line_ranges.get(line_index + 1) else {
            return false;
        };
        next.start == range.end && self.mandatory_line_break_start(range).is_none()
    }

    fn mandatory_line_break_start(&self, range: &Range<usize>) -> Option<usize> {
        let text = self.backing_text.get(range.clone())?;
        let (mut offset, ch) = text.char_indices().next_back()?;
        if !is_mandatory_line_break(ch) {
            return None;
        }
        if ch == '\n' && text[..offset].ends_with('\r') {
            offset -= '\r'.len_utf8();
        }
        Some(range.start + offset)
    }

    /// Byte ranges of whitespace consumed at automatic word-wrap points.
    /// The text remains in the backing string, but has neither paint glyphs
    /// nor an intermediate caret position.
    pub(crate) fn soft_wrap_trailing_whitespace_ranges(&self) -> Vec<Range<usize>> {
        let line_ranges = self
            .layout
            .lines()
            .map(|line| line.text_range())
            .collect::<Vec<_>>();
        line_ranges
            .iter()
            .enumerate()
            .filter_map(|(line_index, range)| {
                self.is_soft_wrap_after(&line_ranges, line_index)
                    .then(|| self.trailing_whitespace_start(range.clone()))
                    .flatten()
                    .map(|start| start..range.end)
            })
            .collect()
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
                let text_rect = self
                    .source_text_line_rects(source_range.source)
                    .into_iter()
                    .find(|(index, _)| *index == line_index)
                    .map(|(_, rect)| rect);
                rects.push(InlineIfcSelectionRect {
                    line_index,
                    source: source_range.source,
                    range: start..end,
                    rect: InlineIfcPaintRect {
                        x: left,
                        y: text_rect
                            .map(|rect| rect.y)
                            .unwrap_or(metrics.block_min_coord),
                        width: (right - left).max(1.0),
                        height: text_rect
                            .map(|rect| rect.height)
                            .unwrap_or(metrics.line_height)
                            .max(1.0),
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

fn is_mandatory_line_break(ch: char) -> bool {
    matches!(
        ch,
        '\n' | '\r' | '\u{000B}' | '\u{000C}' | '\u{0085}' | '\u{2028}' | '\u{2029}'
    )
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
                    x: fragment.rect.x - left,
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
                    border_colors: style.border_colors,
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
            edge_insets,
        } => {
            let resolved_style = style.as_ref().unwrap_or(inherited_style);
            InlineIfcContentKeyItem::Span {
                source: *source,
                shape_style: InlineIfcStyleKey::from_style(resolved_style),
                children: content_key_items_for(children, resolved_style),
                edge_insets_bits: [edge_insets[0].to_bits(), edge_insets[1].to_bits()],
            }
        }
        InlineIfcItem::AtomicInlineBox {
            source,
            measurement,
        } => InlineIfcContentKeyItem::AtomicInlineBox {
            source: *source,
            shape_key: InlineIfcAtomicBoxShapeKey::from_measurement(measurement),
        },
        InlineIfcItem::GapSpacer { source, width } => InlineIfcContentKeyItem::GapSpacer {
            source: *source,
            width_bits: width.max(0.0).to_bits(),
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
            ..
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
        InlineIfcItem::GapSpacer { source, .. } => {
            InlineIfcPaintKeyItem::GapSpacer { source: *source }
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
            font_weight: glyph.batch_key.font_weight,
            glyph_indices: vec![glyph_index],
        });
    }
    batches
}

fn brush_to_text_color(brush: [u8; 4]) -> [f32; 4] {
    // IFC brushes are packed sRGBA, while every render pipeline consumes
    // linear RGB. Platform-specific surface handling belongs to Viewport's
    // `surface_target_format`: an sRGB view performs linear→sRGB on store
    // (including WebGPU's sRGB view over non-sRGB canvas storage). Feeding
    // packed sRGB values directly here double-encodes text only, so its color
    // no longer matches rect/background passes.
    [
        srgb_to_linear(brush[0]),
        srgb_to_linear(brush[1]),
        srgb_to_linear(brush[2]),
        brush[3] as f32 / 255.0,
    ]
}

fn inline_vertical_align_offset(
    vertical_align: crate::style::VerticalAlign,
    line_top: f32,
    line_height: f32,
    item_top: f32,
    item_height: f32,
) -> f32 {
    match vertical_align {
        crate::style::VerticalAlign::Baseline => 0.0,
        crate::style::VerticalAlign::Top => line_top - item_top,
        crate::style::VerticalAlign::Bottom => line_top + line_height - (item_top + item_height),
        crate::style::VerticalAlign::Middle => {
            line_top + line_height * 0.5 - (item_top + item_height * 0.5)
        }
    }
}

struct InlineIfcBuilder {
    backing_text: String,
    source_ranges: Vec<InlineIfcSourceRange>,
    style_ranges: Vec<InlineIfcStyleRange>,
    inline_boxes: Vec<InlineIfcBoxMapping>,
    span_inline_box_ids: HashMap<InlineIfcSourceId, Vec<u64>>,
    next_inline_box_id: u64,
}

impl InlineIfcBuilder {
    fn new() -> Self {
        Self {
            backing_text: String::new(),
            source_ranges: Vec::new(),
            style_ranges: Vec::new(),
            inline_boxes: Vec::new(),
            span_inline_box_ids: HashMap::new(),
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
                edge_insets,
            } => {
                let resolved_style = style.clone().unwrap_or_else(|| inherited_style.clone());
                if edge_insets[0] > 0.0 {
                    self.push_edge_spacer(*source, edge_insets[0]);
                }
                let start = self.backing_text.len();
                let first_descendant_box = self.inline_boxes.len();
                self.push_items(children, &resolved_style, depth + 1);
                let end = self.backing_text.len();
                let descendant_box_ids = self.inline_boxes[first_descendant_box..]
                    .iter()
                    .map(|inline_box| inline_box.id)
                    .collect::<Vec<_>>();
                if start < end || !descendant_box_ids.is_empty() {
                    self.source_ranges.push(InlineIfcSourceRange {
                        source: *source,
                        kind: InlineIfcSourceKind::Span,
                        range: start..end,
                        depth,
                        style: Some(resolved_style),
                    });
                }
                if !descendant_box_ids.is_empty() {
                    self.span_inline_box_ids.insert(*source, descendant_box_ids);
                }
                if edge_insets[1] > 0.0 {
                    self.push_edge_spacer(*source, edge_insets[1]);
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
                    role: InlineIfcInlineBoxRole::Atomic,
                });
            }
            InlineIfcItem::GapSpacer { source, width } => {
                self.push_gap_spacer(*source, *width);
            }
        }
    }

    /// Reserve a span's horizontal border+padding as a zero-height inline
    /// box: it participates in line advance and wrapping but leaves the
    /// line height untouched (matching CSS inline-box semantics).
    fn push_edge_spacer(&mut self, source: InlineIfcSourceId, width: f32) {
        let id = self.next_inline_box_id;
        self.next_inline_box_id += 1;
        self.inline_boxes.push(InlineIfcBoxMapping {
            id,
            source,
            insertion_byte: self.backing_text.len(),
            measurement: InlineIfcMeasuredAtomicBox::new(
                InlineIfcSize::new(width.max(0.0), 0.0),
                InlineIfcAtomicMeasureConstraints::new(None),
            ),
            role: InlineIfcInlineBoxRole::SpanEdgeSpacer,
        });
    }

    fn push_gap_spacer(&mut self, source: InlineIfcSourceId, width: f32) {
        let id = self.next_inline_box_id;
        self.next_inline_box_id += 1;
        self.inline_boxes.push(InlineIfcBoxMapping {
            id,
            source,
            insertion_byte: self.backing_text.len(),
            measurement: InlineIfcMeasuredAtomicBox::new(
                InlineIfcSize::new(width.max(0.0), 0.0),
                InlineIfcAtomicMeasureConstraints::new(None),
            ),
            role: InlineIfcInlineBoxRole::GapSpacer,
        });
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
mod tests;
