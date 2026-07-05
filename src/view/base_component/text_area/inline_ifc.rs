use std::cell::Ref;
use std::ops::Range;

use crate::ui::Rect;
use crate::view::base_component::{
    DirtyFlags, LayoutConstraints, LayoutPlacement, Position, Size, baseline_cross_offset,
};
use crate::view::inline_formatting_context::{
    InlineFormattingContext, InlineIfcAtomicBoxPlacementPackage, InlineIfcAtomicMeasureConstraints,
    InlineIfcCacheKey, InlineIfcCaretAffinity, InlineIfcCaretGeometry, InlineIfcInput,
    InlineIfcItem, InlineIfcLayoutOptions, InlineIfcMeasuredAtomicBox, InlineIfcPaintRect,
    InlineIfcSize, InlineIfcSourceId, InlineIfcStyle,
};
use crate::view::inline_text_pass_adapter::inline_ifc_paint_input_to_text_pass_staging_input;
use crate::view::layout::{FlexLayoutInfo, LayoutState};
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::render_pass::text_pass::TextPassPreparedStagingInput;

use super::run::{TextAreaLineBreak, TextAreaTextRun};
use super::{TextArea, TextAreaProjectionSegment};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextAreaUnifiedIfcSourceKind {
    TextRun,
    LineBreak,
    ProjectionAtomicBox,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaUnifiedIfcSourceSegment {
    pub(crate) child_key: NodeKey,
    pub(crate) source: InlineIfcSourceId,
    pub(crate) kind: TextAreaUnifiedIfcSourceKind,
    pub(crate) char_range: Range<usize>,
    pub(crate) backing_byte_range: Range<usize>,
    pub(crate) preedit_backing_byte_range: Option<Range<usize>>,
    pub(crate) preedit_caret_backing_byte: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaUnifiedIfcCaretGeometry {
    pub(crate) char_index: usize,
    pub(crate) x: f32,
    pub(crate) y_top: f32,
    pub(crate) height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaUnifiedIfcCaretStop {
    pub(crate) char_index: usize,
    pub(crate) affinity: super::caret_map::CaretAffinity,
    pub(crate) x: f32,
    pub(crate) y_top: f32,
    pub(crate) height: f32,
    pub(crate) is_line_head: bool,
    pub(crate) is_line_tail: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaUnifiedIfcCaretLine {
    pub(crate) y_top: f32,
    pub(crate) y_bottom: f32,
    pub(crate) stops: Vec<TextAreaUnifiedIfcCaretStop>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaUnifiedIfcProjectionOverlaySource {
    pub(crate) child_key: NodeKey,
    pub(crate) source: InlineIfcSourceId,
    pub(crate) char_range: Range<usize>,
    pub(crate) backing_byte_range: Range<usize>,
    pub(crate) atomic_rect: InlineIfcPaintRect,
}

pub(crate) struct TextAreaUnifiedIfcRootPackage {
    pub(crate) input: InlineIfcInput,
    pub(crate) ifc: InlineFormattingContext,
    pub(crate) source_segments: Vec<TextAreaUnifiedIfcSourceSegment>,
    pub(crate) atomic_sources: Vec<InlineIfcSourceId>,
    pub(crate) width_constraint: Option<f32>,
    pub(crate) allow_wrap: bool,
    pub(crate) vertical_align: crate::style::VerticalAlign,
    /// child_key → index into `source_segments`; per-child queries run
    /// once per segment (per editor line), so a linear find would make
    /// the placement pass O(n²).
    segment_index_by_child: std::collections::HashMap<NodeKey, usize>,
    /// Memoized caret lines: building them walks every segment's caret
    /// stops (O(lines)), and per-segment callers (child_layout_rects for
    /// every line break) made that O(lines²) per measure/place.
    visual_caret_lines_cache: std::cell::OnceCell<Vec<TextAreaUnifiedIfcCaretLine>>,
}

#[derive(Default)]
pub(crate) struct TextAreaUnifiedIfcRootCache {
    entry: Option<TextAreaUnifiedIfcRootCacheEntry>,
    #[cfg(test)]
    build_count: usize,
}

struct TextAreaUnifiedIfcRootCacheEntry {
    key: TextAreaUnifiedIfcRootCacheKey,
    package: TextAreaUnifiedIfcRootPackage,
    /// Cheap validity state: revision + the input scalars a fast check
    /// can compare without rebuilding the full source (which clones
    /// every run's text and hashes the whole backing string).
    source_revision: u64,
    children_snapshot: Vec<NodeKey>,
    style_probe: InlineIfcStyle,
}

#[derive(Clone, Debug, PartialEq)]
struct TextAreaUnifiedIfcRootCacheKey {
    ifc: InlineIfcCacheKey,
    source_segments: Vec<TextAreaUnifiedIfcSourceSegment>,
    atomic_sources: Vec<InlineIfcSourceId>,
    ime_preedit: String,
    ime_preedit_cursor: Option<(usize, usize)>,
    vertical_align: crate::style::VerticalAlign,
    allow_wrap: bool,
}

struct TextAreaUnifiedIfcRootSource {
    key: TextAreaUnifiedIfcRootCacheKey,
    input: InlineIfcInput,
    layout_options: InlineIfcLayoutOptions,
    source_segments: Vec<TextAreaUnifiedIfcSourceSegment>,
    atomic_sources: Vec<InlineIfcSourceId>,
    width_constraint: Option<f32>,
    allow_wrap: bool,
    vertical_align: crate::style::VerticalAlign,
}

impl TextAreaUnifiedIfcRootSource {
    fn into_package(self) -> TextAreaUnifiedIfcRootPackage {
        let ifc =
            InlineFormattingContext::build_with_options(self.input.clone(), self.layout_options);
        let segment_index_by_child = self
            .source_segments
            .iter()
            .enumerate()
            .map(|(index, segment)| (segment.child_key, index))
            .collect();
        TextAreaUnifiedIfcRootPackage {
            input: self.input,
            ifc,
            source_segments: self.source_segments,
            atomic_sources: self.atomic_sources,
            width_constraint: self.width_constraint,
            allow_wrap: self.allow_wrap,
            vertical_align: self.vertical_align,
            segment_index_by_child,
            visual_caret_lines_cache: std::cell::OnceCell::new(),
        }
    }
}

impl TextAreaUnifiedIfcRootPackage {
    pub(crate) fn projection_segment_count(&self) -> usize {
        self.source_segments
            .iter()
            .filter(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox)
            .count()
    }

    pub(crate) fn text_run_count(&self) -> usize {
        self.source_segments
            .iter()
            .filter(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::TextRun)
            .count()
    }

    pub(crate) fn has_projection_atomic_boxes(&self) -> bool {
        self.projection_segment_count() > 0 && !self.atomic_sources.is_empty()
    }

    pub(crate) fn source_for_child(
        &self,
        child_key: NodeKey,
    ) -> Option<&TextAreaUnifiedIfcSourceSegment> {
        self.segment_index_by_child
            .get(&child_key)
            .and_then(|&index| self.source_segments.get(index))
    }

    pub(crate) fn atomic_package_for_child(
        &self,
        child_key: NodeKey,
    ) -> Option<InlineIfcAtomicBoxPlacementPackage> {
        let source = self.source_for_child(child_key)?;
        if source.kind != TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox {
            return None;
        }
        Some(self.aligned_atomic_box_placement_package(source.source))
    }

    pub(crate) fn projection_overlay_source_for_child(
        &self,
        child_key: NodeKey,
    ) -> Option<TextAreaUnifiedIfcProjectionOverlaySource> {
        let segment = self.source_for_child(child_key)?;
        if segment.kind != TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox {
            return None;
        }
        let atomic_package = self.aligned_atomic_box_placement_package(segment.source);
        let atomic = atomic_package.placements.first()?;
        Some(TextAreaUnifiedIfcProjectionOverlaySource {
            child_key,
            source: segment.source,
            char_range: segment.char_range.clone(),
            backing_byte_range: segment.backing_byte_range.clone(),
            atomic_rect: atomic.rect,
        })
    }

    pub(crate) fn projection_overlay_sources(
        &self,
    ) -> Vec<TextAreaUnifiedIfcProjectionOverlaySource> {
        self.source_segments
            .iter()
            .filter_map(|segment| self.projection_overlay_source_for_child(segment.child_key))
            .collect()
    }

    pub(crate) fn content_rect(&self) -> Option<InlineIfcPaintRect> {
        let snapshot = self.ifc.text_layout_snapshot_ref();
        let top_offset = self.content_top_offset();
        let mut rect: Option<InlineIfcPaintRect> = None;
        for line in &snapshot.lines {
            let line_rect = InlineIfcPaintRect {
                x: line.x,
                y: line.y - top_offset,
                width: line.width,
                height: line.height,
            };
            rect = Some(merge_rect(rect, line_rect));
        }
        for source in &self.atomic_sources {
            for placement in self
                .aligned_atomic_box_placement_package(*source)
                .placements
            {
                rect = Some(merge_rect(rect, placement.rect));
            }
        }
        rect
    }

    pub(crate) fn visual_line_rects(&self) -> Vec<Rect> {
        let top_offset = self.content_top_offset();
        self.ifc
            .text_layout_snapshot_ref()
            .lines
            .iter()
            .map(|line| Rect {
                x: line.x,
                y: line.y - top_offset,
                width: line.width.max(0.0),
                height: line.height.max(1.0),
            })
            .collect()
    }

    pub(crate) fn child_layout_rects(&self, child_key: NodeKey) -> Vec<Rect> {
        let Some(segment) = self.source_for_child(child_key) else {
            return Vec::new();
        };
        match segment.kind {
            TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox => self
                .aligned_atomic_box_placement_package(segment.source)
                .placements
                .into_iter()
                .map(|placement| Rect {
                    x: placement.rect.x,
                    y: placement.rect.y,
                    width: placement.rect.width.max(0.0),
                    height: placement.rect.height.max(1.0),
                })
                .collect(),
            TextAreaUnifiedIfcSourceKind::TextRun => {
                let top_offset = self.content_top_offset();
                let mut rects = if self.vertical_align == crate::style::VerticalAlign::Baseline {
                    self.ifc
                        .source_line_rects(segment.source)
                        .into_iter()
                        .map(|rect| Rect {
                            x: rect.x,
                            y: rect.y - top_offset,
                            width: rect.width.max(0.0),
                            height: rect.height.max(1.0),
                        })
                        .collect::<Vec<_>>()
                } else {
                    self.ifc
                        .source_text_line_rects(segment.source)
                        .into_iter()
                        .map(|(line_index, rect)| Rect {
                            x: rect.x,
                            y: rect.y + self.text_vertical_align_delta(line_index) - top_offset,
                            width: rect.width.max(0.0),
                            height: rect.height.max(1.0),
                        })
                        .collect::<Vec<_>>()
                };
                if rects.is_empty() {
                    rects = self.caret_line_rects_for_segment(segment);
                }
                if segment.preedit_backing_byte_range.is_some() {
                    self.expand_tall_text_rects(rects)
                } else {
                    rects
                }
            }
            TextAreaUnifiedIfcSourceKind::LineBreak => self.caret_line_rects_for_segment(segment),
        }
    }

    pub(crate) fn content_size(&self) -> Size {
        let mut rect: Option<Rect> = None;
        let mut content_width = 0.0_f32;
        for segment in &self.source_segments {
            for child_rect in self.child_layout_rects(segment.child_key) {
                content_width = content_width.max(child_rect.x + child_rect.width.max(0.0));
            }
        }
        for line in self.visual_line_rects() {
            rect = Some(
                merge_ui_rects(
                    [rect, Some(Rect { width: 0.0, ..line })]
                        .into_iter()
                        .flatten(),
                )
                .unwrap(),
            );
        }
        for line in self.visual_caret_lines_ref() {
            rect = Some(
                merge_ui_rects(
                    [
                        rect,
                        Some(Rect {
                            x: 0.0,
                            y: line.y_top,
                            width: 0.0,
                            height: (line.y_bottom - line.y_top).max(1.0),
                        }),
                    ]
                    .into_iter()
                    .flatten(),
                )
                .unwrap(),
            );
        }
        for source in &self.atomic_sources {
            for placement in self
                .aligned_atomic_box_placement_package(*source)
                .placements
            {
                rect = Some(
                    merge_ui_rects(
                        [
                            rect,
                            Some(Rect {
                                x: placement.rect.x,
                                y: placement.rect.y,
                                width: placement.rect.width.max(0.0),
                                height: placement.rect.height.max(1.0),
                            }),
                        ]
                        .into_iter()
                        .flatten(),
                    )
                    .unwrap(),
                );
            }
        }
        let logical_line_height = self.text_line_height()
            * (self
                .source_segments
                .iter()
                .filter(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::LineBreak)
                .count()
                + 1) as f32;
        rect.map(|rect| Size {
            width: content_width.max(0.0),
            height: (rect.y + rect.height).max(logical_line_height).max(0.0),
        })
        .unwrap_or(Size {
            width: content_width.max(0.0),
            height: logical_line_height.max(self.text_line_height()),
        })
    }

    pub(crate) fn flex_info_for_children(&self, _children: &[NodeKey]) -> FlexLayoutInfo {
        let rects = self.visual_line_rects();
        let mut lines = Vec::new();
        let mut line_main_sum = Vec::new();
        let mut line_cross_max = Vec::new();
        for rect in rects {
            let height = rect.height.max(1.0);
            lines.push(Vec::new());
            line_main_sum.push(rect.width.max(0.0));
            line_cross_max.push(height);
        }
        FlexLayoutInfo {
            lines,
            total_main: line_main_sum.iter().copied().fold(0.0_f32, f32::max),
            total_cross: line_cross_max.iter().sum(),
            line_main_sum,
            line_cross_max,
        }
    }

    pub(crate) fn child_fragment_rects(&self, child_key: NodeKey) -> Vec<Rect> {
        self.child_layout_rects(child_key)
    }

    pub(crate) fn text_pass_staging_input(
        &self,
        origin: [f32; 2],
        opacity: f32,
        fragment_index: u32,
        scale_factor: f32,
    ) -> TextPassPreparedStagingInput {
        let paint_input = self.ifc.text_pass_paint_input_ref();
        let mut staging_input = inline_ifc_paint_input_to_text_pass_staging_input(
            &paint_input,
            origin,
            opacity,
            fragment_index,
            scale_factor,
        );
        let top_offset = self.content_top_offset();
        // The prepared pass positions glyphs from `paint.local_pos` plus the
        // fragment origin; `final_paint_pos` only feeds probes. Shift both so
        // the painted glyphs match the aligned geometry.
        for (staged, glyph) in staging_input
            .glyphs
            .iter_mut()
            .zip(paint_input.glyphs.iter())
        {
            let delta = self.text_vertical_align_delta(glyph.line_index) - top_offset;
            staged.paint.local_pos[1] += delta;
            staged.final_paint_pos[1] += delta;
        }
        staging_input
    }

    pub(crate) fn selection_rects_for_char_range(&self, range: Range<usize>) -> Vec<Rect> {
        if range.start >= range.end {
            return Vec::new();
        }

        let paint_input = self.ifc.text_pass_paint_input_ref();
        let staging_input = self.text_pass_staging_input([0.0, 0.0], 1.0, 0, 1.0);
        let top_offset = self.content_top_offset();
        let mut out = Vec::new();
        for segment in self
            .source_segments
            .iter()
            .filter(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::TextRun)
        {
            // Vertical band per line comes from the same geometry the run
            // fragments paint at (line box for Baseline, aligned text box
            // otherwise) — glyph paint y is the baseline, so deriving the
            // band from it drew selections ~an ascent below the text.
            let text_line_rects = if self.vertical_align == crate::style::VerticalAlign::Baseline {
                Vec::new()
            } else {
                self.ifc.source_text_line_rects(segment.source)
            };
            let start = range.start.max(segment.char_range.start);
            let end = range.end.min(segment.char_range.end);
            if start >= end {
                continue;
            }
            let backing_start = segment.backing_byte_range.start
                + byte_offset_for_char_count(
                    self.ifc.backing_text(),
                    segment.backing_byte_range.clone(),
                    start - segment.char_range.start,
                );
            let backing_end = segment.backing_byte_range.start
                + byte_offset_for_char_count(
                    self.ifc.backing_text(),
                    segment.backing_byte_range.clone(),
                    end - segment.char_range.start,
                );

            for line in &paint_input.lines {
                let mut left: Option<f32> = None;
                let mut right: Option<f32> = None;
                for (glyph, staged) in paint_input
                    .glyphs
                    .iter()
                    .zip(staging_input.glyphs.iter())
                    .filter(|(glyph, _)| {
                        glyph.line_index == line.line_index
                            && glyph.source == segment.source
                            && glyph.cluster_range.start < backing_end
                            && glyph.cluster_range.end > backing_start
                    })
                {
                    let x = staged.final_paint_pos[0];
                    left = Some(left.map_or(x, |current| current.min(x)));
                    right = Some(
                        right.map_or(x + glyph.advance, |current| current.max(x + glyph.advance)),
                    );
                }
                let (Some(left), Some(right)) = (left, right) else {
                    continue;
                };
                if right <= left {
                    continue;
                }
                let band = if self.vertical_align == crate::style::VerticalAlign::Baseline {
                    Some((line.y - top_offset, line.height.max(1.0)))
                } else {
                    text_line_rects
                        .iter()
                        .find(|(line_index, _)| *line_index == line.line_index)
                        .map(|(line_index, rect)| {
                            (
                                rect.y + self.text_vertical_align_delta(*line_index) - top_offset,
                                rect.height.max(1.0),
                            )
                        })
                };
                let Some((band_y, band_height)) = band else {
                    continue;
                };
                out.push(Rect {
                    x: left,
                    y: band_y,
                    width: right - left,
                    height: band_height,
                });
            }
        }
        out
    }

    pub(crate) fn preedit_underline_rects(&self) -> Vec<Rect> {
        self.source_segments
            .iter()
            .filter_map(|segment| segment.preedit_backing_byte_range.clone())
            .flat_map(|range| self.underline_rects_for_backing_byte_range(range))
            .collect()
    }

    pub(crate) fn preedit_caret_geometry_for_char(
        &self,
        char_index: usize,
    ) -> Option<TextAreaUnifiedIfcCaretGeometry> {
        let segment = self
            .source_segments
            .iter()
            .find(|segment| self.preedit_caret_char_for_segment(segment) == Some(char_index))?;
        let byte_index = segment.preedit_caret_backing_byte?;
        let mut geometry = self
            .ifc
            .caret_geometry_for_byte(byte_index, InlineIfcCaretAffinity::Downstream)?;
        // Parley's line attribution keys off the cursor y, which misfires
        // when adjacent line bands overlap (line gap < line height) — e.g.
        // a preedit at a wrapped hard-newline tail. Repair from the byte
        // range like the committed-caret stops do.
        if let Some(line_index) =
            self.line_index_for_backing_byte(byte_index, InlineIfcCaretAffinity::Downstream)
        {
            geometry.line_index = line_index;
        }
        Some(self.root_caret_geometry_from_ifc(char_index, &geometry))
    }

    pub(crate) fn caret_geometry_for_char(
        &self,
        char_index: usize,
        affinity: super::caret_map::CaretAffinity,
    ) -> Option<TextAreaUnifiedIfcCaretGeometry> {
        if let Some(geometry) = self.preedit_caret_geometry_for_char(char_index) {
            return Some(geometry);
        }
        let lines = self.visual_caret_lines_ref();
        let mut found: Option<&TextAreaUnifiedIfcCaretStop> = None;
        for line in lines {
            if let Some(stop) = line.stops.iter().find(|stop| stop.char_index == char_index) {
                match affinity {
                    super::caret_map::CaretAffinity::Upstream => {
                        if found.is_none() {
                            found = Some(stop);
                        }
                    }
                    super::caret_map::CaretAffinity::Downstream => {
                        found = Some(stop);
                    }
                }
            }
        }
        found.map(|stop| TextAreaUnifiedIfcCaretGeometry {
            char_index: stop.char_index,
            x: stop.x,
            y_top: stop.y_top,
            height: stop.height,
        })
    }

    pub(crate) fn visual_caret_lines(&self) -> Vec<TextAreaUnifiedIfcCaretLine> {
        self.visual_caret_lines_ref().to_vec()
    }

    pub(crate) fn visual_caret_lines_ref(&self) -> &[TextAreaUnifiedIfcCaretLine] {
        self.visual_caret_lines_cache.get_or_init(|| {
            let mut lines = Vec::new();
            for segment in self.source_segments.iter().filter(|segment| {
                matches!(
                    segment.kind,
                    TextAreaUnifiedIfcSourceKind::TextRun | TextAreaUnifiedIfcSourceKind::LineBreak
                )
            }) {
                self.push_text_segment_committed_caret_stops(segment, &mut lines);
            }
            for segment in self
                .source_segments
                .iter()
                .filter(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox)
            {
                self.push_projection_atomic_caret_stops(segment, &mut lines);
            }
            normalize_root_caret_lines(&mut lines)
        })
    }

    fn push_text_segment_committed_caret_stops(
        &self,
        segment: &TextAreaUnifiedIfcSourceSegment,
        lines: &mut Vec<IndexedTextAreaUnifiedIfcCaretLine>,
    ) {
        let span = segment
            .char_range
            .end
            .saturating_sub(segment.char_range.start);
        for local_char in 0..=span {
            let byte_index = self.committed_local_char_to_backing_byte(segment, local_char);
            for affinity in [
                InlineIfcCaretAffinity::Downstream,
                InlineIfcCaretAffinity::Upstream,
            ] {
                let Some(geometry) = self.ifc.caret_geometry_for_byte(byte_index, affinity) else {
                    continue;
                };
                let line_index = self
                    .line_index_for_backing_byte(byte_index, affinity)
                    .unwrap_or(geometry.line_index);
                push_root_caret_stop(lines, line_index, {
                    let (y_top, height) = self
                        .caret_line_top_height(line_index)
                        .unwrap_or((geometry.y, geometry.height));
                    TextAreaUnifiedIfcCaretStop {
                        char_index: segment.char_range.start + local_char,
                        affinity: caret_affinity_from_ifc(affinity),
                        x: geometry.x,
                        y_top,
                        height,
                        is_line_head: false,
                        is_line_tail: false,
                    }
                });
            }
        }
    }

    fn caret_line_rects_for_segment(&self, segment: &TextAreaUnifiedIfcSourceSegment) -> Vec<Rect> {
        self.visual_caret_lines_ref()
            .iter()
            .filter_map(|line| {
                let stops = line
                    .stops
                    .iter()
                    .filter(|stop| {
                        segment.char_range.start <= stop.char_index
                            && stop.char_index <= segment.char_range.end
                    })
                    .collect::<Vec<_>>();
                if stops.is_empty() {
                    return None;
                }
                let left = stops
                    .iter()
                    .map(|stop| stop.x)
                    .fold(f32::INFINITY, f32::min);
                let right = stops
                    .iter()
                    .map(|stop| stop.x)
                    .fold(f32::NEG_INFINITY, f32::max);
                Some(Rect {
                    x: if left.is_finite() { left } else { 0.0 },
                    y: line.y_top,
                    width: if right.is_finite() && left.is_finite() {
                        (right - left).max(0.0)
                    } else {
                        0.0
                    },
                    height: (line.y_bottom - line.y_top).max(1.0),
                })
            })
            .collect()
    }

    fn line_index_for_backing_byte(
        &self,
        byte_index: usize,
        affinity: InlineIfcCaretAffinity,
    ) -> Option<usize> {
        let snapshot = self.ifc.text_layout_snapshot_ref();
        match affinity {
            InlineIfcCaretAffinity::Downstream => snapshot
                .lines
                .iter()
                .find(|line| line.range.start == byte_index)
                .or_else(|| {
                    snapshot
                        .lines
                        .iter()
                        .find(|line| line.range.start <= byte_index && byte_index < line.range.end)
                })
                .or_else(|| {
                    snapshot
                        .lines
                        .iter()
                        .rev()
                        .find(|line| line.range.end == byte_index)
                })
                .map(|line| line.line_index),
            InlineIfcCaretAffinity::Upstream => {
                snapshot
                    .lines
                    .iter()
                    .rev()
                    .find(|line| line.range.end == byte_index)
                    .or_else(|| {
                        snapshot.lines.iter().rev().find(|line| {
                            line.range.start < byte_index && byte_index <= line.range.end
                        })
                    })
                    .or_else(|| {
                        snapshot
                            .lines
                            .iter()
                            .find(|line| line.range.start == byte_index)
                    })
                    .map(|line| line.line_index)
            }
        }
    }

    fn caret_line_top_height(&self, line_index: usize) -> Option<(f32, f32)> {
        let snapshot = self.ifc.text_layout_snapshot_ref();
        let line = snapshot.lines.get(line_index)?;
        Some((
            line.y + self.text_vertical_align_delta(line_index) - self.content_top_offset(),
            line.height.max(1.0),
        ))
    }

    fn committed_local_char_to_backing_byte(
        &self,
        segment: &TextAreaUnifiedIfcSourceSegment,
        local_char: usize,
    ) -> usize {
        let span = segment
            .char_range
            .end
            .saturating_sub(segment.char_range.start);
        let local_char = local_char.min(span);
        let Some(preedit_range) = segment.preedit_backing_byte_range.as_ref() else {
            return segment.backing_byte_range.start
                + byte_offset_for_char_count(
                    self.ifc.backing_text(),
                    segment.backing_byte_range.clone(),
                    local_char,
                );
        };
        let before_preedit =
            self.backing_char_count(segment.backing_byte_range.start..preedit_range.start);
        if local_char <= before_preedit {
            return segment.backing_byte_range.start
                + byte_offset_for_char_count(
                    self.ifc.backing_text(),
                    segment.backing_byte_range.start..preedit_range.start,
                    local_char,
                );
        }
        preedit_range.end
            + byte_offset_for_char_count(
                self.ifc.backing_text(),
                preedit_range.end..segment.backing_byte_range.end,
                local_char - before_preedit,
            )
    }

    fn root_caret_geometry_from_ifc(
        &self,
        char_index: usize,
        geometry: &InlineIfcCaretGeometry,
    ) -> TextAreaUnifiedIfcCaretGeometry {
        let (y_top, height) = self
            .caret_line_top_height(geometry.line_index)
            .unwrap_or((geometry.y, geometry.height));
        TextAreaUnifiedIfcCaretGeometry {
            char_index,
            x: geometry.x,
            y_top,
            height,
        }
    }

    fn push_projection_atomic_caret_stops(
        &self,
        segment: &TextAreaUnifiedIfcSourceSegment,
        lines: &mut Vec<IndexedTextAreaUnifiedIfcCaretLine>,
    ) {
        let atomic_package = self.aligned_atomic_box_placement_package(segment.source);
        let Some(atomic) = atomic_package.placements.first() else {
            return;
        };
        let span = segment
            .char_range
            .end
            .saturating_sub(segment.char_range.start);
        let stop_count = span + 1;
        if stop_count == 0 {
            return;
        }
        for local in 0..=span {
            let fraction = if span == 0 {
                0.0
            } else {
                (local as f32 / span as f32).clamp(0.0, 1.0)
            };
            let affinity = if local == 0 {
                super::caret_map::CaretAffinity::Downstream
            } else {
                super::caret_map::CaretAffinity::Upstream
            };
            push_root_caret_stop(
                lines,
                atomic.line_index,
                TextAreaUnifiedIfcCaretStop {
                    char_index: segment.char_range.start + local,
                    affinity,
                    x: atomic.rect.x + atomic.rect.width * fraction,
                    y_top: atomic.rect.y,
                    height: atomic.rect.height.max(1.0),
                    is_line_head: local == 0,
                    is_line_tail: local == span,
                },
            );
        }
    }

    fn preedit_caret_char_for_segment(
        &self,
        segment: &TextAreaUnifiedIfcSourceSegment,
    ) -> Option<usize> {
        let preedit_range = segment.preedit_backing_byte_range.clone()?;
        let before = self.backing_char_count(segment.backing_byte_range.start..preedit_range.start);
        Some(segment.char_range.start + before)
    }

    fn root_char_for_backing_byte(
        &self,
        segment: &TextAreaUnifiedIfcSourceSegment,
        byte_index: usize,
    ) -> usize {
        let byte_index = byte_index.clamp(
            segment.backing_byte_range.start,
            segment.backing_byte_range.end,
        );
        if let Some(preedit_range) = segment.preedit_backing_byte_range.as_ref() {
            if byte_index <= preedit_range.start {
                return segment.char_range.start
                    + self.backing_char_count(segment.backing_byte_range.start..byte_index);
            }
            let before =
                self.backing_char_count(segment.backing_byte_range.start..preedit_range.start);
            if byte_index <= preedit_range.end {
                return segment.char_range.start + before;
            }
            let after = self.backing_char_count(preedit_range.end..byte_index);
            return (segment.char_range.start + before + after).min(segment.char_range.end);
        }
        (segment.char_range.start
            + self.backing_char_count(segment.backing_byte_range.start..byte_index))
        .min(segment.char_range.end)
    }

    fn backing_char_count(&self, range: Range<usize>) -> usize {
        self.ifc.backing_text()[range].chars().count()
    }

    fn underline_rects_for_backing_byte_range(&self, range: Range<usize>) -> Vec<Rect> {
        if range.start >= range.end {
            return Vec::new();
        }

        let paint_input = self.ifc.text_pass_paint_input_ref();
        let staging_input = self.text_pass_staging_input([0.0, 0.0], 1.0, 0, 1.0);
        let text_height = self.text_line_height();
        let top_offset = self.content_top_offset();
        let mut out = Vec::new();
        for line in &paint_input.lines {
            let mut left: Option<f32> = None;
            let mut right: Option<f32> = None;
            for (glyph, staged) in paint_input
                .glyphs
                .iter()
                .zip(staging_input.glyphs.iter())
                .filter(|(glyph, _)| {
                    glyph.line_index == line.line_index
                        && glyph.cluster_range.start < range.end
                        && glyph.cluster_range.end > range.start
                })
            {
                let x = staged.final_paint_pos[0];
                left = Some(left.map_or(x, |current| current.min(x)));
                right =
                    Some(right.map_or(x + glyph.advance, |current| current.max(x + glyph.advance)));
            }
            let (Some(left), Some(right)) = (left, right) else {
                continue;
            };
            if right <= left {
                continue;
            }
            let line_top = line.y + self.text_vertical_align_delta(line.line_index) - top_offset;
            out.push(Rect {
                x: left,
                y: line_top + text_height.max(1.0) - 1.0,
                width: (right - left).max(1.0),
                height: 1.0,
            });
        }
        out
    }

    /// Parley lets a line's `block_min_coord` go negative when an atomic
    /// box is taller than the surrounding text ascent. TextArea content
    /// must start at the TextArea origin, so every package consumer
    /// shifts by this offset to pin the top-most line at `y == 0`.
    fn content_top_offset(&self) -> f32 {
        self.ifc
            .text_layout_snapshot_ref()
            .lines
            .iter()
            .map(|line| line.y)
            .fold(0.0f32, f32::min)
    }

    fn aligned_atomic_box_placement_package(
        &self,
        source: InlineIfcSourceId,
    ) -> InlineIfcAtomicBoxPlacementPackage {
        let mut package = self.ifc.atomic_box_placement_package(source);
        let snapshot = self.ifc.text_layout_snapshot_ref();
        let top_offset = self.content_top_offset();
        for placement in &mut package.placements {
            let Some(line) = snapshot.lines.get(placement.line_index) else {
                continue;
            };
            let item_height = placement.rect.height.max(0.0);
            let align_offset = baseline_cross_offset(
                line.baseline,
                line.height,
                item_height,
                item_height,
                self.vertical_align,
            );
            placement.rect.y = line.y + align_offset - top_offset;
        }
        package
    }

    fn text_vertical_align_delta(&self, line_index: usize) -> f32 {
        let snapshot = self.ifc.text_layout_snapshot_ref();
        let Some(line) = snapshot.lines.get(line_index) else {
            return 0.0;
        };
        let style = &self.input.default_style;
        let text_height = (style.font_size.max(1.0) * style.line_height.max(0.8)).max(1.0);
        let font_size = style.font_size.max(1.0);
        let leading = (text_height - font_size).max(0.0);
        let text_baseline = (font_size * 0.8779297 + leading / 2.0).max(0.0);
        let baseline_offset = baseline_cross_offset(
            line.baseline,
            line.height,
            text_baseline,
            text_height,
            crate::style::VerticalAlign::Baseline,
        );
        let aligned_offset = baseline_cross_offset(
            line.baseline,
            line.height,
            text_baseline,
            text_height,
            self.vertical_align,
        );
        aligned_offset - baseline_offset
    }

    fn text_line_height(&self) -> f32 {
        let style = &self.input.default_style;
        (style.font_size.max(1.0) * style.line_height.max(0.8)).max(1.0)
    }

    fn inline_baseline_for_line_height(&self, height: f32) -> f32 {
        let style = &self.input.default_style;
        let font_size = style.font_size.max(1.0);
        let leading = (height.max(font_size) - font_size).max(0.0);
        (font_size * 0.8779297 + leading / 2.0).max(0.0)
    }

    fn line_rects_for_backing_range(&self, range: Range<usize>) -> Vec<Rect> {
        let snapshot = self.ifc.text_layout_snapshot_ref();
        let top_offset = self.content_top_offset();
        snapshot
            .lines
            .iter()
            .filter(|line| {
                let overlaps = line.range.start < range.end && range.start < line.range.end;
                let boundary = range.start == range.end
                    && line.range.start <= range.start
                    && range.start <= line.range.end;
                overlaps || boundary
            })
            .map(|line| Rect {
                x: line.x,
                y: line.y - top_offset,
                width: line.width,
                height: line.height,
            })
            .collect()
    }

    fn expand_tall_text_rects(&self, rects: Vec<Rect>) -> Vec<Rect> {
        let line_height = self.text_line_height();
        if line_height <= 0.0 {
            return rects;
        }
        let mut out = Vec::new();
        for rect in rects {
            if rect.height <= line_height * 1.5 {
                out.push(rect);
                continue;
            }
            let count = (rect.height / line_height).ceil().max(1.0) as usize;
            for idx in 0..count {
                let y = rect.y + line_height * idx as f32;
                let remaining = (rect.y + rect.height) - y;
                out.push(Rect {
                    x: rect.x,
                    y,
                    width: rect.width,
                    height: remaining.min(line_height).max(1.0),
                });
            }
        }
        out
    }
}

fn byte_offset_for_char_count(backing_text: &str, range: Range<usize>, char_count: usize) -> usize {
    let slice = &backing_text[range.clone()];
    slice
        .char_indices()
        .nth(char_count)
        .map(|(offset, _)| offset)
        .unwrap_or(range.end - range.start)
}

impl TextArea {
    #[allow(dead_code)]
    pub(crate) fn unified_inline_ifc_root_package(
        &self,
        arena: &NodeArena,
    ) -> Option<TextAreaUnifiedIfcRootPackage> {
        self.build_unified_inline_ifc_root_source(arena)
            .map(TextAreaUnifiedIfcRootSource::into_package)
    }

    pub(crate) fn bump_unified_ifc_source_revision(&self) {
        self.unified_ifc_source_revision
            .set(self.unified_ifc_source_revision.get().wrapping_add(1));
    }

    /// O(1)-ish check that the cached package still describes the current
    /// inputs: content/structure changes bump the source revision, and
    /// everything else that feeds `build_unified_inline_ifc_root_source`
    /// is compared directly. Rebuilding the source per call cloned every
    /// run's text and hashed the whole backing string — per frame, per
    /// query — which dominated drag frames on editor-sized content.
    fn unified_ifc_cache_entry_is_current(&self, entry: &TextAreaUnifiedIfcRootCacheEntry) -> bool {
        entry.source_revision == self.unified_ifc_source_revision.get()
            && entry.children_snapshot == self.children
            && entry.key.vertical_align == self.vertical_align
            && entry.key.allow_wrap == self.auto_wrap
            && entry.key.ime_preedit == self.ime_preedit
            && entry.key.ime_preedit_cursor == self.ime_preedit_cursor
            && entry.package.width_constraint == self.current_unified_ifc_width_constraint()
            && entry.style_probe == self.current_unified_ifc_style()
    }

    fn current_unified_ifc_style(&self) -> InlineIfcStyle {
        InlineIfcStyle {
            font_size: self.font_size,
            line_height: self.line_height,
            font_weight: self.font_weight,
            brush: self.color.to_rgba_u8(),
            font_families: self.font_families.clone(),
        }
    }

    fn current_unified_ifc_width_constraint(&self) -> Option<f32> {
        if self.auto_wrap {
            let width = if self.viewport_size.width > 0.0 {
                self.viewport_size.width
            } else {
                self.layout_state.layout_size.width
            }
            .max(1.0);
            Some(width)
        } else {
            None
        }
    }

    /// True when the cached unified package is valid for the current
    /// inputs without rebuilding the source (see
    /// `unified_ifc_cache_entry_is_current`).
    pub(crate) fn unified_ifc_package_cache_is_current(&self) -> bool {
        self.unified_inline_ifc_root_cache
            .borrow()
            .entry
            .as_ref()
            .is_some_and(|entry| self.unified_ifc_cache_entry_is_current(entry))
    }

    fn cached_unified_inline_ifc_root_package(
        &self,
        arena: &NodeArena,
    ) -> Option<Ref<'_, TextAreaUnifiedIfcRootPackage>> {
        let is_current = self
            .unified_inline_ifc_root_cache
            .borrow()
            .entry
            .as_ref()
            .is_some_and(|entry| self.unified_ifc_cache_entry_is_current(entry));
        if !is_current {
            let source = self.build_unified_inline_ifc_root_source(arena)?;
            let needs_update = self
                .unified_inline_ifc_root_cache
                .borrow()
                .entry
                .as_ref()
                .is_none_or(|entry| entry.key != source.key);
            let mut cache = self.unified_inline_ifc_root_cache.borrow_mut();
            if needs_update {
                let style_probe = source.input.default_style.clone();
                cache.entry = Some(TextAreaUnifiedIfcRootCacheEntry {
                    key: source.key.clone(),
                    source_revision: self.unified_ifc_source_revision.get(),
                    children_snapshot: self.children.clone(),
                    style_probe,
                    package: source.into_package(),
                });
                #[cfg(test)]
                {
                    cache.build_count += 1;
                }
            } else if let Some(entry) = cache.entry.as_mut() {
                // Same shaped inputs reached through a changed scalar
                // (e.g. width restored): refresh the validity state so
                // the fast check passes again.
                entry.source_revision = self.unified_ifc_source_revision.get();
                entry.children_snapshot = self.children.clone();
                entry.style_probe = source.input.default_style.clone();
            }
        }

        Ref::filter_map(self.unified_inline_ifc_root_cache.borrow(), |cache| {
            cache.entry.as_ref().map(|entry| &entry.package)
        })
        .ok()
    }

    fn build_unified_inline_ifc_root_source(
        &self,
        arena: &NodeArena,
    ) -> Option<TextAreaUnifiedIfcRootSource> {
        if self.children.is_empty() {
            return None;
        }

        let style = InlineIfcStyle {
            font_size: self.font_size,
            line_height: self.line_height,
            font_weight: self.font_weight,
            brush: self.color.to_rgba_u8(),
            font_families: self.font_families.clone(),
        };
        let width_constraint = if self.auto_wrap {
            let width = if self.viewport_size.width > 0.0 {
                self.viewport_size.width
            } else {
                self.layout_state.layout_size.width
            }
            .max(1.0);
            Some(width)
        } else {
            None
        };

        let mut items = Vec::new();
        let mut source_segments = Vec::new();
        let mut atomic_sources = Vec::new();
        let mut backing_byte_cursor = 0usize;

        for &child_key in &self.children {
            let child_item = arena
                .with_element_taken_ref(child_key, |child, _| {
                    let source = InlineIfcSourceId(child.stable_id());
                    if let Some(run) = child.as_any().downcast_ref::<TextAreaTextRun>() {
                        let start = backing_byte_cursor;
                        let effective_text = run.effective_text();
                        let preedit_backing_byte_range =
                            run.effective_preedit_backing_byte_range(start);
                        let preedit_caret_backing_byte =
                            run.effective_preedit_caret_backing_byte(start);
                        backing_byte_cursor += effective_text.len();
                        let end = backing_byte_cursor;
                        source_segments.push(TextAreaUnifiedIfcSourceSegment {
                            child_key,
                            source,
                            kind: TextAreaUnifiedIfcSourceKind::TextRun,
                            char_range: run.char_range.clone(),
                            backing_byte_range: start..end,
                            preedit_backing_byte_range,
                            preedit_caret_backing_byte,
                        });
                        return Some(InlineIfcItem::TextSpan {
                            source,
                            text: effective_text,
                            style: Some(style.clone()),
                        });
                    }

                    if let Some(line_break) = child.as_any().downcast_ref::<TextAreaLineBreak>() {
                        let start = backing_byte_cursor;
                        backing_byte_cursor += "\n".len();
                        let end = backing_byte_cursor;
                        source_segments.push(TextAreaUnifiedIfcSourceSegment {
                            child_key,
                            source,
                            kind: TextAreaUnifiedIfcSourceKind::LineBreak,
                            char_range: line_break.char_range.clone(),
                            backing_byte_range: start..end,
                            preedit_backing_byte_range: None,
                            preedit_caret_backing_byte: None,
                        });
                        return Some(InlineIfcItem::TextSpan {
                            source,
                            text: "\n".to_string(),
                            style: Some(style.clone()),
                        });
                    }

                    if let Some(projection) =
                        child.as_any().downcast_ref::<TextAreaProjectionSegment>()
                    {
                        let snapshot = child.box_model_snapshot();
                        let measurement = InlineIfcMeasuredAtomicBox::new(
                            InlineIfcSize::new(snapshot.width, snapshot.height),
                            InlineIfcAtomicMeasureConstraints::new(width_constraint),
                        );
                        source_segments.push(TextAreaUnifiedIfcSourceSegment {
                            child_key,
                            source,
                            kind: TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox,
                            char_range: projection.char_range(),
                            backing_byte_range: backing_byte_cursor..backing_byte_cursor,
                            preedit_backing_byte_range: None,
                            preedit_caret_backing_byte: None,
                        });
                        atomic_sources.push(source);
                        return Some(InlineIfcItem::AtomicInlineBox {
                            source,
                            measurement,
                        });
                    }

                    None
                })
                .flatten();

            if let Some(item) = child_item {
                items.push(item);
            }
        }

        if items.is_empty() {
            return None;
        }

        let mut input = InlineIfcInput::new(items);
        input.default_style = style;
        if let Some(width_constraint) = width_constraint {
            input = input.with_max_width(width_constraint);
        }
        let layout_options = InlineIfcLayoutOptions::new(width_constraint, self.auto_wrap);
        let key = TextAreaUnifiedIfcRootCacheKey {
            ifc: input.cache_key_with_layout_options(layout_options),
            source_segments: source_segments.clone(),
            atomic_sources: atomic_sources.clone(),
            ime_preedit: self.ime_preedit.clone(),
            ime_preedit_cursor: self.ime_preedit_cursor,
            vertical_align: self.vertical_align,
            allow_wrap: self.auto_wrap,
        };
        Some(TextAreaUnifiedIfcRootSource {
            key,
            input,
            layout_options,
            source_segments,
            atomic_sources,
            width_constraint,
            allow_wrap: self.auto_wrap,
            vertical_align: self.vertical_align,
        })
    }

    pub(crate) fn measure_unified_inline_ifc_atomic_children(
        &self,
        constraints: LayoutConstraints,
        arena: &mut NodeArena,
    ) {
        let child_keys = self.children.clone();
        let mut any_resized = false;
        for child_key in child_keys {
            arena.with_element_taken(child_key, |child, arena| {
                if child
                    .as_any()
                    .downcast_ref::<TextAreaProjectionSegment>()
                    .is_none()
                {
                    return;
                }
                let before = child.box_model_snapshot();
                child.measure(
                    LayoutConstraints {
                        max_width: constraints.max_width.max(0.0),
                        max_height: constraints.max_height.max(0.0),
                        viewport_width: constraints.viewport_width,
                        viewport_height: constraints.viewport_height,
                        percent_base_width: constraints.percent_base_width,
                        percent_base_height: constraints.percent_base_height,
                    },
                    arena,
                );
                let after = child.box_model_snapshot();
                if before.width != after.width || before.height != after.height {
                    any_resized = true;
                }
            });
        }
        if any_resized {
            // An atomic box's measurement feeds the unified IFC source;
            // invalidate the revision fast path so the package rebuilds.
            self.bump_unified_ifc_source_revision();
        }
    }

    pub(crate) fn measure_generated_text_children_for_fallbacks(
        &self,
        constraints: LayoutConstraints,
        arena: &mut NodeArena,
    ) {
        let width = constraints.max_width.max(0.0);
        let child_keys = self.children.clone();
        for child_key in child_keys {
            arena.with_element_taken(child_key, |child, _arena| {
                if child.as_any().downcast_ref::<TextAreaTextRun>().is_none()
                    && child.as_any().downcast_ref::<TextAreaLineBreak>().is_none()
                {
                    return;
                }
                // The unified pass is these children's measure: clear
                // their local LAYOUT dirt like Element::measure would,
                // or the TextArea's subtree aggregate stays LAYOUT-dirty
                // forever and the clean measure fast path never engages.
                if let Some(run) = child.as_any_mut().downcast_mut::<TextAreaTextRun>() {
                    run.dirty_flags = run.dirty_flags.without(DirtyFlags::LAYOUT);
                } else if let Some(line_break) =
                    child.as_any_mut().downcast_mut::<TextAreaLineBreak>()
                {
                    line_break.dirty_flags = line_break.dirty_flags.without(DirtyFlags::LAYOUT);
                }
                if let Some(run) = child.as_any_mut().downcast_mut::<TextAreaTextRun>() {
                    run.measure_generated_text_child(
                        width,
                        constraints.viewport_width,
                        constraints.viewport_height,
                        constraints.percent_base_width,
                        constraints.percent_base_height,
                    );
                } else if let Some(line_break) =
                    child.as_any_mut().downcast_mut::<TextAreaLineBreak>()
                {
                    line_break.measure_generated_text_child();
                }
            });
        }
    }

    pub(crate) fn apply_unified_inline_ifc_child_placements(
        &self,
        arena: &mut NodeArena,
        placement: LayoutPlacement,
    ) -> bool {
        let Some(package) = self.cached_unified_inline_ifc_root_package(arena) else {
            return false;
        };

        let origin_x = self.layout_state.layout_position.x - self.scroll_x;
        let origin_y = self.layout_state.layout_position.y - self.scroll_y;

        // Same shaped package applied at the same origin: every child's
        // installed geometry is already correct. A pure move only shifts
        // absolute coordinates, so delta-shift text runs and line breaks
        // in place instead of recomputing per-child fragment rects.
        // Projection atomic boxes still go through child.place so their
        // own placement gate decides.
        let revision = self.unified_ifc_source_revision.get();
        if let Some((last_x, last_y, last_revision)) = self.last_unified_apply.get() {
            if last_revision == revision {
                let dx = origin_x - last_x;
                let dy = origin_y - last_y;
                let has_atomic = package.source_segments.iter().any(|segment| {
                    segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox
                });
                if dx == 0.0 && dy == 0.0 && !has_atomic {
                    return true;
                }
                let atomic_rects: Vec<(NodeKey, Rect)> = if has_atomic {
                    package
                        .source_segments
                        .iter()
                        .filter(|segment| {
                            segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox
                        })
                        .filter_map(|segment| {
                            package
                                .child_fragment_rects(segment.child_key)
                                .first()
                                .map(|rect| (segment.child_key, *rect))
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                let shift_keys: Vec<(NodeKey, TextAreaUnifiedIfcSourceKind)> = package
                    .source_segments
                    .iter()
                    .filter(|segment| {
                        segment.kind != TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox
                    })
                    .map(|segment| (segment.child_key, segment.kind))
                    .collect();
                drop(package);
                if dx != 0.0 || dy != 0.0 {
                    for (child_key, kind) in shift_keys {
                        arena.with_element_taken(child_key, |child, _arena| match kind {
                            TextAreaUnifiedIfcSourceKind::TextRun => {
                                if let Some(run) =
                                    child.as_any_mut().downcast_mut::<TextAreaTextRun>()
                                {
                                    shift_layout_state_and_fragments(
                                        &mut run.layout_state,
                                        &mut run.inline_paint_fragments,
                                        dx,
                                        dy,
                                    );
                                }
                            }
                            TextAreaUnifiedIfcSourceKind::LineBreak => {
                                if let Some(line_break) =
                                    child.as_any_mut().downcast_mut::<TextAreaLineBreak>()
                                {
                                    let mut fragments = Vec::new();
                                    shift_layout_state_and_fragments(
                                        &mut line_break.layout_state,
                                        &mut fragments,
                                        dx,
                                        dy,
                                    );
                                    for fragment in line_break.caret_fragments.iter_mut().flatten()
                                    {
                                        fragment.x += dx;
                                        fragment.y += dy;
                                    }
                                }
                            }
                            TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox => {}
                        });
                    }
                }
                for (child_key, rect) in atomic_rects {
                    arena.with_element_taken(child_key, |child, arena| {
                        child.set_layout_offset(rect.x, rect.y);
                        child.place(
                            LayoutPlacement {
                                parent_x: origin_x,
                                parent_y: origin_y,
                                visual_offset_x: 0.0,
                                visual_offset_y: 0.0,
                                available_width: rect.width.max(1.0),
                                available_height: rect.height.max(1.0),
                                viewport_width: placement.viewport_width,
                                viewport_height: placement.viewport_height,
                                percent_base_width: placement.percent_base_width,
                                percent_base_height: placement.percent_base_height,
                            },
                            arena,
                        );
                    });
                }
                self.last_unified_apply
                    .set(Some((origin_x, origin_y, revision)));
                return true;
            }
        }
        let mut applied = false;
        for segment in &package.source_segments {
            let rects = package.child_fragment_rects(segment.child_key);
            if rects.is_empty() {
                continue;
            }
            arena.with_element_taken(segment.child_key, |child, arena| match segment.kind {
                TextAreaUnifiedIfcSourceKind::TextRun => {
                    if let Some(run) = child.as_any_mut().downcast_mut::<TextAreaTextRun>() {
                        apply_root_rects_to_layout_state(
                            &mut run.layout_state,
                            &mut run.inline_paint_fragments,
                            origin_x,
                            origin_y,
                            &rects,
                            true,
                        );
                        run.dirty_flags = run.dirty_flags.without(
                            DirtyFlags::PLACE
                                .union(DirtyFlags::BOX_MODEL)
                                .union(DirtyFlags::HIT_TEST),
                        );
                    }
                }
                TextAreaUnifiedIfcSourceKind::LineBreak => {
                    if let Some(line_break) = child.as_any_mut().downcast_mut::<TextAreaLineBreak>()
                    {
                        let mut paint_fragments = Vec::new();
                        apply_root_rects_to_layout_state(
                            &mut line_break.layout_state,
                            &mut paint_fragments,
                            origin_x,
                            origin_y,
                            &rects,
                            false,
                        );
                        line_break.caret_fragments = [
                            paint_fragments.first().copied(),
                            paint_fragments.last().copied(),
                        ];
                        line_break.dirty_flags = line_break.dirty_flags.without(
                            DirtyFlags::PLACE
                                .union(DirtyFlags::BOX_MODEL)
                                .union(DirtyFlags::HIT_TEST),
                        );
                    }
                }
                TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox => {
                    let rect = rects[0];
                    child.set_layout_offset(rect.x, rect.y);
                    child.place(
                        LayoutPlacement {
                            parent_x: origin_x,
                            parent_y: origin_y,
                            visual_offset_x: 0.0,
                            visual_offset_y: 0.0,
                            available_width: rect.width.max(1.0),
                            available_height: rect.height.max(1.0),
                            viewport_width: placement.viewport_width,
                            viewport_height: placement.viewport_height,
                            percent_base_width: placement.percent_base_width,
                            percent_base_height: placement.percent_base_height,
                        },
                        arena,
                    );
                }
            });
            applied = true;
        }
        if applied {
            self.last_unified_apply
                .set(Some((origin_x, origin_y, revision)));
        }
        applied
    }

    pub(crate) fn unified_inline_ifc_render_package(
        &self,
        arena: &NodeArena,
    ) -> Option<Ref<'_, TextAreaUnifiedIfcRootPackage>> {
        self.cached_unified_inline_ifc_root_package(arena)
    }

    #[cfg(test)]
    pub(crate) fn unified_inline_ifc_root_cache_build_count(&self) -> usize {
        self.unified_inline_ifc_root_cache.borrow().build_count
    }
}

/// In-place delta shift for a run/line-break whose owning TextArea moved
/// without reshaping: mirrors what `apply_root_rects_to_layout_state`
/// would produce at the new origin.
fn shift_layout_state_and_fragments(
    layout_state: &mut LayoutState,
    paint_fragments: &mut [Rect],
    dx: f32,
    dy: f32,
) {
    for fragment in paint_fragments.iter_mut() {
        fragment.x += dx;
        fragment.y += dy;
    }
    layout_state.layout_position.x += dx;
    layout_state.layout_position.y += dy;
    layout_state.layout_inner_position = layout_state.layout_position;
    layout_state.layout_flow_position = layout_state.layout_position;
    layout_state.layout_flow_inner_position = layout_state.layout_position;
}

fn apply_root_rects_to_layout_state(
    layout_state: &mut LayoutState,
    paint_fragments: &mut Vec<Rect>,
    origin_x: f32,
    origin_y: f32,
    rects: &[Rect],
    should_render: bool,
) {
    paint_fragments.clear();
    for rect in rects {
        paint_fragments.push(Rect {
            x: origin_x + rect.x,
            y: origin_y + rect.y,
            width: rect.width.max(0.0),
            height: rect.height.max(0.0),
        });
    }
    let Some(bounds) = merge_ui_rects(paint_fragments.iter().copied()) else {
        return;
    };
    layout_state.layout_position = Position {
        x: bounds.x,
        y: bounds.y,
    };
    layout_state.layout_size = Size {
        width: bounds.width.max(0.0),
        height: bounds.height.max(0.0),
    };
    layout_state.layout_inner_position = layout_state.layout_position;
    layout_state.layout_inner_size = layout_state.layout_size;
    layout_state.layout_flow_position = layout_state.layout_position;
    layout_state.layout_flow_inner_position = layout_state.layout_inner_position;
    layout_state.content_size = layout_state.layout_size;
    layout_state.should_render = should_render
        && layout_state.layout_size.width > 0.0
        && layout_state.layout_size.height > 0.0;
}

fn merge_ui_rects(rects: impl IntoIterator<Item = Rect>) -> Option<Rect> {
    let mut out: Option<Rect> = None;
    for rect in rects {
        let left = rect.x;
        let top = rect.y;
        let right = rect.x + rect.width.max(0.0);
        let bottom = rect.y + rect.height.max(0.0);
        out = Some(match out {
            None => Rect {
                x: left,
                y: top,
                width: (right - left).max(0.0),
                height: (bottom - top).max(0.0),
            },
            Some(current) => {
                let current_right = current.x + current.width.max(0.0);
                let current_bottom = current.y + current.height.max(0.0);
                let next_left = current.x.min(left);
                let next_top = current.y.min(top);
                let next_right = current_right.max(right);
                let next_bottom = current_bottom.max(bottom);
                Rect {
                    x: next_left,
                    y: next_top,
                    width: (next_right - next_left).max(0.0),
                    height: (next_bottom - next_top).max(0.0),
                }
            }
        });
    }
    out
}

fn merge_rect(current: Option<InlineIfcPaintRect>, next: InlineIfcPaintRect) -> InlineIfcPaintRect {
    let Some(current) = current else {
        return next;
    };
    let left = current.x.min(next.x);
    let top = current.y.min(next.y);
    let right = (current.x + current.width.max(0.0)).max(next.x + next.width.max(0.0));
    let bottom = (current.y + current.height.max(0.0)).max(next.y + next.height.max(0.0));
    InlineIfcPaintRect {
        x: left,
        y: top,
        width: (right - left).max(0.0),
        height: (bottom - top).max(0.0),
    }
}

struct IndexedTextAreaUnifiedIfcCaretLine {
    line_index: usize,
    line: TextAreaUnifiedIfcCaretLine,
}

fn push_root_caret_stop(
    lines: &mut Vec<IndexedTextAreaUnifiedIfcCaretLine>,
    line_index: usize,
    stop: TextAreaUnifiedIfcCaretStop,
) {
    if let Some(line) = lines.iter_mut().find(|line| line.line_index == line_index) {
        line.line.y_top = line.line.y_top.min(stop.y_top);
        line.line.y_bottom = line.line.y_bottom.max(stop.y_top + stop.height);
        line.line.stops.push(stop);
        return;
    }
    lines.push(IndexedTextAreaUnifiedIfcCaretLine {
        line_index,
        line: TextAreaUnifiedIfcCaretLine {
            y_top: stop.y_top,
            y_bottom: stop.y_top + stop.height,
            stops: vec![stop],
        },
    });
}

fn normalize_root_caret_lines(
    lines: &mut Vec<IndexedTextAreaUnifiedIfcCaretLine>,
) -> Vec<TextAreaUnifiedIfcCaretLine> {
    lines.sort_by(|a, b| {
        a.line
            .y_top
            .partial_cmp(&b.line.y_top)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.line_index.cmp(&b.line_index))
    });
    let mut out = Vec::with_capacity(lines.len());
    for indexed in lines.drain(..) {
        let mut line = indexed.line;
        line.stops
            .sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        let mut deduped: Vec<TextAreaUnifiedIfcCaretStop> = Vec::with_capacity(line.stops.len());
        for stop in line.stops.drain(..) {
            if let Some(existing) = deduped
                .iter_mut()
                .find(|existing| existing.char_index == stop.char_index)
            {
                if stop.x > existing.x
                    || stop.affinity == super::caret_map::CaretAffinity::Downstream
                {
                    *existing = stop;
                }
                continue;
            }
            deduped.push(stop);
        }
        for stop in &mut deduped {
            stop.y_top = line.y_top;
            stop.height = (line.y_bottom - line.y_top).max(1.0);
        }
        line.stops = deduped;
        if !line.stops.is_empty() {
            out.push(line);
        }
    }
    out
}

fn caret_affinity_from_ifc(affinity: InlineIfcCaretAffinity) -> super::caret_map::CaretAffinity {
    match affinity {
        InlineIfcCaretAffinity::Downstream => super::caret_map::CaretAffinity::Downstream,
        InlineIfcCaretAffinity::Upstream => super::caret_map::CaretAffinity::Upstream,
    }
}

#[cfg(test)]
mod tests {
    use super::super::run::InlinePreedit;
    use super::*;
    use crate::view::base_component::{ElementTrait, Size};
    use crate::view::renderer_adapter::ElementDescriptor;
    use crate::view::test_support::{commit_descriptor, new_test_arena};

    fn text_area_with_run(
        text: &str,
        width: f32,
    ) -> (
        NodeArena,
        crate::view::node_arena::NodeKey,
        crate::view::node_arena::NodeKey,
    ) {
        let mut arena = new_test_arena();
        let mut text_area = TextArea::new();
        text_area.content = text.to_string();
        text_area.auto_wrap = true;
        text_area.viewport_size = Size {
            width,
            height: 120.0,
        };
        text_area.layout_state.layout_size = Size {
            width,
            height: 24.0,
        };
        let root = commit_descriptor(
            &mut arena,
            None,
            ElementDescriptor {
                element: Box::new(text_area) as Box<dyn ElementTrait>,
                children: vec![ElementDescriptor::leaf(Box::new(TextAreaTextRun::new(
                    text.to_string(),
                    0..text.chars().count(),
                ))
                    as Box<dyn ElementTrait>)],
                side_slots: Vec::new(),
            },
        );
        arena.push_root(root);
        let run = arena.children_of(root)[0];
        (arena, root, run)
    }

    fn touch_unified_package(arena: &NodeArena, root: crate::view::node_arena::NodeKey) -> usize {
        arena
            .with_element_taken_ref(root, |el, arena| {
                let text_area = el
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root");
                let package = text_area
                    .cached_unified_inline_ifc_root_package(arena)
                    .expect("unified package");
                assert_eq!(package.text_run_count(), 1);
                drop(package);
                text_area.unified_inline_ifc_root_cache_build_count()
            })
            .expect("TextArea root")
    }

    #[test]
    fn text_area_unified_ifc_root_cache_reuses_package_for_repeated_queries() {
        let (arena, root, _) = text_area_with_run("hello", 120.0);

        let build_count = arena
            .with_element_taken_ref(root, |el, arena| {
                let text_area = el
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root");
                let first = text_area
                    .cached_unified_inline_ifc_root_package(arena)
                    .expect("first package");
                assert_eq!(first.text_run_count(), 1);
                drop(first);

                let second = text_area
                    .cached_unified_inline_ifc_root_package(arena)
                    .expect("second package");
                assert_eq!(second.text_run_count(), 1);
                drop(second);

                text_area.unified_inline_ifc_root_cache_build_count()
            })
            .expect("TextArea root");

        assert_eq!(build_count, 1, "same key should reuse the cached package");
    }

    #[test]
    fn text_area_unified_ifc_root_cache_invalidates_on_content_style_preedit_and_width() {
        let (arena, root, run) = text_area_with_run("hello", 120.0);

        assert_eq!(touch_unified_package(&arena, root), 1);
        assert_eq!(touch_unified_package(&arena, root), 1);

        arena
            .with_element_taken_ref(run, |el, _| {
                el.as_any_mut()
                    .downcast_mut::<TextAreaTextRun>()
                    .expect("TextAreaTextRun")
                    .set_text("hello!".to_string(), 0..6);
            })
            .expect("TextAreaTextRun");
        // Every production run-text mutation flows through a TextArea
        // choke point (edits via mark_content_dirty, projection in-place
        // updates) that bumps the source revision; mirror that here.
        arena
            .with_element_taken_ref(root, |el, _| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .bump_unified_ifc_source_revision();
            })
            .expect("TextArea root");
        assert_eq!(touch_unified_package(&arena, root), 2);

        arena
            .with_element_taken_ref(root, |el, _| {
                el.as_any_mut()
                    .downcast_mut::<TextArea>()
                    .expect("TextArea root")
                    .font_size = 18.0;
            })
            .expect("TextArea root");
        assert_eq!(touch_unified_package(&arena, root), 3);

        arena
            .with_element_taken_ref(root, |el, _| {
                let text_area = el
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .expect("TextArea root");
                text_area.ime_preedit = "中".to_string();
                text_area.ime_preedit_cursor = Some((0, 1));
            })
            .expect("TextArea root");
        assert_eq!(touch_unified_package(&arena, root), 4);

        arena
            .with_element_taken_ref(root, |el, _| {
                let text_area = el
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .expect("TextArea root");
                text_area.viewport_size.width = 160.0;
                text_area.layout_state.layout_size.width = 160.0;
            })
            .expect("TextArea root");
        assert_eq!(touch_unified_package(&arena, root), 5);
    }

    #[test]
    fn text_area_unified_ifc_root_package_uses_effective_text_and_preedit_range() {
        let (arena, root, run) = text_area_with_run("hello", 120.0);
        arena
            .with_element_taken_ref(run, |el, _| {
                el.as_any_mut()
                    .downcast_mut::<TextAreaTextRun>()
                    .expect("TextAreaTextRun")
                    .set_inline_preedit(Some(InlinePreedit {
                        insert_at_local: 2,
                        preedit_text: "中".to_string(),
                        preedit_cursor: Some((0, "中".len())),
                    }));
            })
            .expect("TextAreaTextRun");

        arena
            .with_element_taken_ref(root, |el, arena| {
                let text_area = el
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root");
                let package = text_area
                    .unified_inline_ifc_root_package(arena)
                    .expect("unified package");
                assert_eq!(package.ifc.backing_text(), "he中llo");
                let segment = package
                    .source_segments
                    .iter()
                    .find(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::TextRun)
                    .expect("text run segment");
                assert_eq!(segment.preedit_backing_byte_range, Some(2.."he中".len()));
                assert!(
                    !package.preedit_underline_rects().is_empty(),
                    "root package should expose underline rects for the spliced preedit"
                );
            })
            .expect("TextArea root");
    }
}
