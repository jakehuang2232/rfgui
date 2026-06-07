use std::ops::Range;

use crate::ui::Rect;
use crate::view::base_component::{LayoutPlacement, inline_cross_offset};
use crate::view::inline_formatting_context::{
    InlineFormattingContext, InlineIfcAtomicBoxPlacementPackage, InlineIfcAtomicMeasureConstraints,
    InlineIfcInput, InlineIfcItem, InlineIfcMeasuredAtomicBox, InlineIfcPaintRect, InlineIfcSize,
    InlineIfcSourceId, InlineIfcStyle,
};
use crate::view::inline_text_pass_adapter::inline_ifc_paint_input_to_text_pass_staging_input;
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
        self.source_segments
            .iter()
            .find(|segment| segment.child_key == child_key)
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
        let snapshot = self.ifc.text_layout_snapshot();
        let top_offset = self.content_top_offset();
        let mut rect: Option<InlineIfcPaintRect> = None;
        for line in snapshot.lines {
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

    pub(crate) fn text_pass_staging_input(
        &self,
        origin: [f32; 2],
        opacity: f32,
        fragment_index: u32,
        scale_factor: f32,
    ) -> TextPassPreparedStagingInput {
        let paint_input = self.ifc.text_pass_paint_input();
        let mut staging_input = inline_ifc_paint_input_to_text_pass_staging_input(
            &paint_input,
            origin,
            opacity,
            fragment_index,
            scale_factor,
        );
        let top_offset = self.content_top_offset();
        for (staged, glyph) in staging_input
            .glyphs
            .iter_mut()
            .zip(paint_input.glyphs.iter())
        {
            staged.final_paint_pos[1] +=
                self.text_vertical_align_delta(glyph.line_index) - top_offset;
        }
        staging_input
    }

    pub(crate) fn selection_rects_for_char_range(&self, range: Range<usize>) -> Vec<Rect> {
        if range.start >= range.end {
            return Vec::new();
        }

        let paint_input = self.ifc.text_pass_paint_input();
        let staging_input = self.text_pass_staging_input([0.0, 0.0], 1.0, 0, 1.0);
        let text_height = self.text_line_height();
        let mut out = Vec::new();
        for segment in self
            .source_segments
            .iter()
            .filter(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::TextRun)
        {
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
                let mut top: Option<f32> = None;
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
                    let y = staged.final_paint_pos[1];
                    left = Some(left.map_or(x, |current| current.min(x)));
                    right = Some(
                        right.map_or(x + glyph.advance, |current| current.max(x + glyph.advance)),
                    );
                    top = Some(top.map_or(y, |current| current.min(y)));
                }
                let (Some(left), Some(right), Some(top)) = (left, right, top) else {
                    continue;
                };
                if right <= left {
                    continue;
                }
                out.push(Rect {
                    x: left,
                    y: top,
                    width: right - left,
                    height: text_height,
                });
            }
        }
        out
    }

    /// Parley lets a line's `block_min_coord` go negative when an atomic
    /// box is taller than the surrounding text ascent. TextArea content
    /// must start at the TextArea origin, so every package consumer
    /// shifts by this offset to pin the top-most line at `y == 0`.
    fn content_top_offset(&self) -> f32 {
        self.ifc
            .text_layout_snapshot()
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
        let snapshot = self.ifc.text_layout_snapshot();
        let top_offset = self.content_top_offset();
        for placement in &mut package.placements {
            let Some(line) = snapshot.lines.get(placement.line_index) else {
                continue;
            };
            let item_height = placement.rect.height.max(0.0);
            let align_offset = inline_cross_offset(
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
        let snapshot = self.ifc.text_layout_snapshot();
        let Some(line) = snapshot.lines.get(line_index) else {
            return 0.0;
        };
        let style = &self.input.default_style;
        let text_height = (style.font_size.max(1.0) * style.line_height.max(0.8)).max(1.0);
        let font_size = style.font_size.max(1.0);
        let leading = (text_height - font_size).max(0.0);
        let text_baseline = (font_size * 0.8779297 + leading / 2.0).max(0.0);
        let baseline_offset = inline_cross_offset(
            line.baseline,
            line.height,
            text_baseline,
            text_height,
            crate::style::VerticalAlign::Baseline,
        );
        let aligned_offset = inline_cross_offset(
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
            let width = self
                .viewport_size
                .width
                .max(self.layout_state.layout_size.width)
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
                        backing_byte_cursor += run.text.len();
                        let end = backing_byte_cursor;
                        source_segments.push(TextAreaUnifiedIfcSourceSegment {
                            child_key,
                            source,
                            kind: TextAreaUnifiedIfcSourceKind::TextRun,
                            char_range: run.char_range.clone(),
                            backing_byte_range: start..end,
                        });
                        return Some(InlineIfcItem::TextSpan {
                            source,
                            text: run.text.clone(),
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
        let ifc = InlineFormattingContext::build(input.clone());
        Some(TextAreaUnifiedIfcRootPackage {
            input,
            ifc,
            source_segments,
            atomic_sources,
            width_constraint,
            allow_wrap: self.auto_wrap,
            vertical_align: self.vertical_align,
        })
    }

    pub(crate) fn apply_unified_inline_ifc_projection_placements(
        &self,
        arena: &mut NodeArena,
        placement: LayoutPlacement,
    ) -> bool {
        let Some(package) = self.unified_inline_ifc_root_package(arena) else {
            return false;
        };
        if !package.has_projection_atomic_boxes() {
            return false;
        }

        let origin_x = self.layout_state.layout_position.x - self.scroll_x;
        let origin_y = self.layout_state.layout_position.y - self.scroll_y;
        let mut applied = false;
        for segment in package
            .source_segments
            .iter()
            .filter(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox)
        {
            let atomic_package = package.aligned_atomic_box_placement_package(segment.source);
            let Some(atomic) = atomic_package.placements.first() else {
                continue;
            };
            arena.with_element_taken(segment.child_key, |child, arena| {
                child.set_layout_offset(atomic.rect.x, atomic.rect.y);
                child.place(
                    LayoutPlacement {
                        parent_x: origin_x,
                        parent_y: origin_y,
                        visual_offset_x: 0.0,
                        visual_offset_y: 0.0,
                        available_width: atomic.rect.width.max(1.0),
                        available_height: atomic.rect.height.max(1.0),
                        viewport_width: placement.viewport_width,
                        viewport_height: placement.viewport_height,
                        percent_base_width: placement.percent_base_width,
                        percent_base_height: placement.percent_base_height,
                    },
                    arena,
                );
            });
            applied = true;
        }
        applied
    }

    pub(crate) fn unified_inline_ifc_render_package(
        &self,
        arena: &NodeArena,
    ) -> Option<TextAreaUnifiedIfcRootPackage> {
        let package = self.unified_inline_ifc_root_package(arena)?;
        package.has_projection_atomic_boxes().then_some(package)
    }
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
