//! `TextAreaTextRun` — internal plain-text segment child of `TextArea`.
//!
//! P2.1: shapes its segment via the shared text layout adapter, exposes inline measure/place,
//! and emits a single `TextPassFragment` per visual run during paint. Wrap
//! happens inside the text layout engine (controlled by the cascaded
//! `auto_wrap` flag), but wrapped visual lines are exposed back to the parent
//! inline solver as individual fragments so the next sibling receives a
//! `first_available_width` derived from the real last visual line.
//!
//! See `docs/design/textarea-v2.md` (Phase 2) for the role of this
//! component within the v2 inline pipeline.

#![allow(dead_code)]

use std::ops::Range;
use std::sync::Arc;

use crate::style::{ColorLike, Cursor};
use crate::ui::Rect;
use crate::view::base_component::text::measure_text_layout;
use crate::view::base_component::{
    BoxModelSnapshot, BuildState, DirtyFlags, ElementTrait, EventTarget, InlineMeasureContext,
    InlineNodeSize, InlinePlacement, LayoutConstraints, LayoutPlacement, Layoutable, Position,
    Renderable, Size, UiBuildContext,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::inline_formatting_context::InlineIfcStyle;
use crate::view::inline_text_pass_adapter::{
    InlineTextPassBridgePackage, InlineTextPassPreparedInput, TextReadOnlyIfcBridgeInput,
    build_inline_text_pass_prepared_input, build_text_read_only_ifc_bridge_package_from_input,
    inline_prepared_input_to_text_pass_staging_input,
};
use crate::view::layout::LayoutState;
use crate::view::node_arena::NodeKey;
use crate::view::render_pass::TextPass;
use crate::view::render_pass::text_pass::{
    TextInput, TextOutput, TextPassFragment, TextPassParams, TextPassPreparedFragment,
    TextPassPreparedParams, TextPassPreparedStagingInput, TextPreparedInputPass,
};
use crate::view::text_layout::{TextLayout, TextLayoutAlignment};

use super::super::next_ui_node_id;
use super::edit::byte_index_at_char;

/// Legacy in-run IME preedit splice used by projection/context paths.
/// Plain TextArea preedit is represented as a transient sibling Run.
#[derive(Clone, Debug, PartialEq)]
pub struct InlinePreedit {
    pub insert_at_local: usize,
    pub preedit_text: String,
    pub preedit_cursor: Option<(usize, usize)>,
}

pub(crate) struct TextAreaTextRun {
    pub(crate) text: String,
    pub(crate) char_range: Range<usize>,
    pub(crate) is_placeholder: bool,
    pub(crate) is_preedit_run: bool,
    pub(crate) preedit_cursor: Option<(usize, usize)>,
    /// `text` is the visible content of one paragraph. Hard newline
    /// characters are represented by a sibling [`TextAreaLineBreak`], not
    /// by flags on the text run.
    // style cascaded from owning TextArea
    pub(crate) font_families: Vec<String>,
    pub(crate) font_size: f32,
    pub(crate) line_height: f32,
    pub(crate) font_weight: u16,
    pub(crate) color: crate::style::Color,
    pub(crate) cursor: Cursor,
    pub(crate) auto_wrap: bool,
    pub(crate) vertical_align: crate::style::VerticalAlign,

    // Legacy IME splice path. Plain TextArea preedit uses `is_preedit_run`.
    pub(crate) inline_preedit: Option<InlinePreedit>,

    // text layout state
    text_layout: Option<Arc<TextLayout>>,
    last_inline_measure_context: Option<InlineMeasureContext>,

    // layout output
    pub(crate) layout_state: LayoutState,
    pub(crate) inline_paint_fragments: Vec<Rect>,
    pub(crate) dirty_flags: DirtyFlags,
    #[cfg(test)]
    inline_ifc_force_missing_prepared_candidate: bool,

    // identity
    pub(crate) node_id: u64,
    pub(crate) parent_id: Option<u64>,
    pub(crate) children: Vec<NodeKey>,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaTextRunInlineIfcRenderFallback {
    LegacyTextPass,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaTextRunInlineIfcReadinessMetadata {
    pub(crate) editable_text_area_run: bool,
    pub(crate) projection_ifc_path_ready: bool,
    pub(crate) ime_ifc_path_ready: bool,
    pub(crate) caret_affinity_ifc_path_ready: bool,
    pub(crate) scroll_follow_ifc_path_ready: bool,
    pub(crate) has_inline_preedit: bool,
    pub(crate) is_preedit_run: bool,
    pub(crate) preedit_cursor: Option<(usize, usize)>,
    pub(crate) projection_diagnostic: TextAreaTextRunInlineIfcProjectionDiagnostic,
    pub(crate) caret_affinity_diagnostic: TextAreaTextRunInlineIfcCaretAffinityDiagnostic,
    pub(crate) scroll_follow_diagnostic: TextAreaTextRunInlineIfcScrollFollowDiagnostic,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaTextRunInlineIfcProjectionDiagnostic {
    pub(crate) char_range: Range<usize>,
    pub(crate) char_span: usize,
    pub(crate) effective_content_len: usize,
    pub(crate) has_inline_preedit: bool,
    pub(crate) is_preedit_run: bool,
    pub(crate) projection_segment_count: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcCaretAffinityStopSnapshot {
    pub(crate) run_index: usize,
    pub(crate) visual_line_index: usize,
    pub(crate) stop_index: usize,
    pub(crate) local_char: usize,
    pub(crate) local_x: f32,
    pub(crate) local_y_top: f32,
    pub(crate) height: f32,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaTextRunInlineIfcCaretAffinityDiagnostic {
    pub(crate) visual_line_count: usize,
    pub(crate) caret_stop_count: usize,
    pub(crate) multi_stop_line_count: usize,
    pub(crate) caret_stop_snapshots: Vec<TextAreaEditableIfcCaretAffinityStopSnapshot>,
    pub(crate) has_preedit_cursor: bool,
    pub(crate) preedit_cursor: Option<(usize, usize)>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaTextRunInlineIfcScrollFollowDiagnostic {
    pub(crate) char_range: Range<usize>,
    pub(crate) char_span: usize,
    pub(crate) layout_size: [f32; 2],
    pub(crate) effective_content_len: usize,
    pub(crate) visual_line_count: usize,
    pub(crate) caret_stop_count: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaTextRunInlineIfcDiagnostic {
    pub(crate) char_range: Range<usize>,
    pub(crate) content_len: usize,
    pub(crate) effective_content_len: usize,
    pub(crate) layout_size: [f32; 2],
    pub(crate) bridge_glyph_count: usize,
    pub(crate) prepared_glyph_count: usize,
    pub(crate) staging_glyph_count: usize,
    pub(crate) batch_count: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaTextRunInlineIfcPreparedGlyphMetadata {
    pub(crate) glyph_index: usize,
    pub(crate) batch_index: Option<usize>,
    pub(crate) final_paint_pos: [f32; 2],
    pub(crate) local_pos: [f32; 2],
    pub(crate) font_data_id: u64,
    pub(crate) font_index: u32,
    pub(crate) font_size: f32,
    pub(crate) normalized_coords_hash: u64,
    pub(crate) has_raster_key: bool,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaTextRunInlineIfcPreparedCandidate {
    pub(crate) char_range: Range<usize>,
    pub(crate) fallback: TextAreaTextRunInlineIfcRenderFallback,
    pub(crate) origin: [f32; 2],
    pub(crate) layout_size: [f32; 2],
    pub(crate) width_constraint: Option<f32>,
    pub(crate) allow_wrap: bool,
    pub(crate) opacity: f32,
    pub(crate) fragment_index: u32,
    pub(crate) scale_factor: f32,
    pub(crate) glyph_count: usize,
    pub(crate) prepared_glyph_count: usize,
    pub(crate) staging_glyph_count: usize,
    pub(crate) batch_count: usize,
    pub(crate) glyph_metadata: Vec<TextAreaTextRunInlineIfcPreparedGlyphMetadata>,
}

#[allow(dead_code)]
impl TextAreaTextRunInlineIfcPreparedCandidate {
    fn from_prepared_payload(
        char_range: Range<usize>,
        bridge_input: &TextReadOnlyIfcBridgeInput,
        bridge_package: &InlineTextPassBridgePackage,
        prepared_input: &InlineTextPassPreparedInput,
        text_pass_staging_input: &TextPassPreparedStagingInput,
    ) -> Self {
        Self {
            char_range,
            fallback: TextAreaTextRunInlineIfcRenderFallback::LegacyTextPass,
            origin: bridge_input.origin,
            layout_size: bridge_input.layout_size,
            width_constraint: bridge_input.width_constraint,
            allow_wrap: bridge_input.allow_wrap,
            opacity: bridge_input.opacity,
            fragment_index: bridge_input.fragment_index,
            scale_factor: prepared_input.scale_factor,
            glyph_count: bridge_package.glyphs.len(),
            prepared_glyph_count: prepared_input.glyphs.len(),
            staging_glyph_count: text_pass_staging_input.glyphs.len(),
            batch_count: prepared_input.batches.len(),
            glyph_metadata: prepared_input
                .glyphs
                .iter()
                .map(|glyph| TextAreaTextRunInlineIfcPreparedGlyphMetadata {
                    glyph_index: glyph.glyph_index,
                    batch_index: glyph.batch_index,
                    final_paint_pos: glyph.final_paint_pos,
                    local_pos: glyph.paint.local_pos,
                    font_data_id: glyph.raster.font_data_id,
                    font_index: glyph.raster.font_index,
                    font_size: glyph.raster.font_size,
                    normalized_coords_hash: glyph.raster.normalized_coords_hash,
                    has_raster_key: glyph.raster_key.is_some(),
                })
                .collect(),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaTextRunInlineIfcStagingPayload {
    pub(crate) char_range: Range<usize>,
    pub(crate) render_enabled: bool,
    pub(crate) fallback: TextAreaTextRunInlineIfcRenderFallback,
    pub(crate) readiness: TextAreaTextRunInlineIfcReadinessMetadata,
    pub(crate) bridge_input: TextReadOnlyIfcBridgeInput,
    pub(crate) bridge_package: InlineTextPassBridgePackage,
    pub(crate) prepared_input: InlineTextPassPreparedInput,
    pub(crate) prepared_candidate: TextAreaTextRunInlineIfcPreparedCandidate,
    pub(crate) text_pass_staging_input: TextPassPreparedStagingInput,
    pub(crate) diagnostic: TextAreaTextRunInlineIfcDiagnostic,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaInlineIfcEvaluationPreflightState {
    Blocked,
    ReadyForDiagnosticEvaluation,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaInlineIfcEvaluationPreflightBlockedReason {
    MissingRunPayload,
    ProjectionPathUnwired,
    ImePathUnwired,
    CaretAffinityPathUnwired,
    ScrollFollowPathUnwired,
    LegacyFallbackMissing,
    ReadOnlyTextPathSeparationMissing,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaInlineIfcEvaluationRunInput {
    pub(crate) char_range: Range<usize>,
    pub(crate) has_inline_preedit: bool,
    pub(crate) is_preedit_run: bool,
    pub(crate) preedit_cursor: Option<(usize, usize)>,
    pub(crate) projection_diagnostic: TextAreaTextRunInlineIfcProjectionDiagnostic,
    pub(crate) caret_affinity_diagnostic: TextAreaTextRunInlineIfcCaretAffinityDiagnostic,
    pub(crate) scroll_follow_diagnostic: TextAreaTextRunInlineIfcScrollFollowDiagnostic,
    pub(crate) diagnostic: TextAreaTextRunInlineIfcDiagnostic,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaInlineIfcEvaluationInput {
    pub(crate) run_inputs: Vec<TextAreaInlineIfcEvaluationRunInput>,
    pub(crate) projection_ifc_path_ready: bool,
    pub(crate) ime_ifc_path_ready: bool,
    pub(crate) caret_affinity_ifc_path_ready: bool,
    pub(crate) scroll_follow_ifc_path_ready: bool,
    pub(crate) legacy_fallback_confirmed: bool,
    pub(crate) read_only_text_path_separated: bool,
}

#[allow(dead_code)]
impl TextAreaInlineIfcEvaluationInput {
    pub(crate) fn from_staging_payloads(
        payloads: Vec<TextAreaTextRunInlineIfcStagingPayload>,
    ) -> Self {
        let mut projection_ifc_path_ready = !payloads.is_empty();
        let mut ime_ifc_path_ready = !payloads.is_empty();
        let mut caret_affinity_ifc_path_ready = !payloads.is_empty();
        let mut scroll_follow_ifc_path_ready = !payloads.is_empty();
        let mut legacy_fallback_confirmed = !payloads.is_empty();
        let run_inputs = payloads
            .into_iter()
            .map(|payload| {
                projection_ifc_path_ready &= payload.readiness.projection_ifc_path_ready;
                ime_ifc_path_ready &= payload.readiness.ime_ifc_path_ready;
                caret_affinity_ifc_path_ready &= payload.readiness.caret_affinity_ifc_path_ready;
                scroll_follow_ifc_path_ready &= payload.readiness.scroll_follow_ifc_path_ready;
                legacy_fallback_confirmed &= !payload.render_enabled
                    && payload.fallback == TextAreaTextRunInlineIfcRenderFallback::LegacyTextPass;
                TextAreaInlineIfcEvaluationRunInput {
                    char_range: payload.char_range,
                    has_inline_preedit: payload.readiness.has_inline_preedit,
                    is_preedit_run: payload.readiness.is_preedit_run,
                    preedit_cursor: payload.readiness.preedit_cursor,
                    projection_diagnostic: payload.readiness.projection_diagnostic.clone(),
                    caret_affinity_diagnostic: payload.readiness.caret_affinity_diagnostic.clone(),
                    scroll_follow_diagnostic: payload.readiness.scroll_follow_diagnostic.clone(),
                    diagnostic: payload.diagnostic,
                }
            })
            .collect();

        Self {
            run_inputs,
            projection_ifc_path_ready,
            ime_ifc_path_ready,
            caret_affinity_ifc_path_ready,
            scroll_follow_ifc_path_ready,
            legacy_fallback_confirmed,
            read_only_text_path_separated: true,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaInlineIfcMetadataBridgeStatus {
    Observed,
    ObservedNoPreedit,
    ObservedCaretStops,
    ObservedNoAffinitySlots,
    ObservedNoProjectionSegments,
    ObservedNoScrollSource,
    Unwired,
}

#[allow(dead_code)]
impl TextAreaInlineIfcMetadataBridgeStatus {
    pub(crate) fn has_observed_metadata_source(self) -> bool {
        !matches!(self, TextAreaInlineIfcMetadataBridgeStatus::Unwired)
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaInlineIfcMetadataBridgePreflightState {
    Blocked,
    ReadyForDiagnosticEvaluation,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaInlineIfcMetadataBridgeBlockedReason {
    MissingRunMetadata,
    ProjectionMetadataUnwired,
    ImeMetadataUnwired,
    CaretAffinityMetadataUnwired,
    ScrollFollowMetadataUnwired,
    LegacyFallbackUnconfirmed,
    ReadOnlyTextPathSeparationUnconfirmed,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaInlineIfcRunMetadataBridgeDiagnostic {
    pub(crate) char_range: Range<usize>,
    pub(crate) has_inline_preedit: bool,
    pub(crate) is_preedit_run: bool,
    pub(crate) preedit_cursor: Option<(usize, usize)>,
    pub(crate) projection_diagnostic: TextAreaTextRunInlineIfcProjectionDiagnostic,
    pub(crate) caret_affinity_diagnostic: TextAreaTextRunInlineIfcCaretAffinityDiagnostic,
    pub(crate) scroll_follow_diagnostic: TextAreaTextRunInlineIfcScrollFollowDiagnostic,
    pub(crate) diagnostic: TextAreaTextRunInlineIfcDiagnostic,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaInlineIfcImeMetadataBridgeDiagnostic {
    pub(crate) run_count: usize,
    pub(crate) has_inline_preedit: bool,
    pub(crate) has_preedit_run: bool,
    pub(crate) preedit_cursor_count: usize,
    pub(crate) effective_content_len: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaInlineIfcCaretAffinityMetadataBridgeDiagnostic {
    pub(crate) run_count: usize,
    pub(crate) visual_line_count: usize,
    pub(crate) caret_stop_count: usize,
    pub(crate) multi_stop_line_count: usize,
    pub(crate) preedit_cursor_count: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaInlineIfcProjectionMetadataBridgeDiagnostic {
    pub(crate) run_count: usize,
    pub(crate) char_range_count: usize,
    pub(crate) char_span: usize,
    pub(crate) effective_content_len: usize,
    pub(crate) inline_preedit_run_count: usize,
    pub(crate) preedit_run_count: usize,
    pub(crate) projection_segment_count: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaInlineIfcScrollFollowMetadataBridgeDiagnostic {
    pub(crate) run_count: usize,
    pub(crate) layout_size_count: usize,
    pub(crate) char_span: usize,
    pub(crate) effective_content_len: usize,
    pub(crate) visual_line_count: usize,
    pub(crate) caret_stop_count: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaInlineIfcMetadataBridgeInput {
    pub(crate) run_metadata: Vec<TextAreaInlineIfcRunMetadataBridgeDiagnostic>,
    pub(crate) projection_metadata_diagnostic:
        Option<TextAreaInlineIfcProjectionMetadataBridgeDiagnostic>,
    pub(crate) ime_metadata_diagnostic: Option<TextAreaInlineIfcImeMetadataBridgeDiagnostic>,
    pub(crate) caret_affinity_metadata_diagnostic:
        Option<TextAreaInlineIfcCaretAffinityMetadataBridgeDiagnostic>,
    pub(crate) scroll_follow_metadata_diagnostic:
        Option<TextAreaInlineIfcScrollFollowMetadataBridgeDiagnostic>,
    pub(crate) projection_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) ime_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) caret_affinity_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) scroll_follow_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) legacy_fallback_confirmed: bool,
    pub(crate) read_only_text_path_separated: bool,
}

#[allow(dead_code)]
impl TextAreaInlineIfcMetadataBridgeInput {
    pub(crate) fn from_evaluation_input(input: TextAreaInlineIfcEvaluationInput) -> Self {
        let run_metadata: Vec<_> = input
            .run_inputs
            .into_iter()
            .map(|run_input| TextAreaInlineIfcRunMetadataBridgeDiagnostic {
                char_range: run_input.char_range,
                has_inline_preedit: run_input.has_inline_preedit,
                is_preedit_run: run_input.is_preedit_run,
                preedit_cursor: run_input.preedit_cursor,
                projection_diagnostic: run_input.projection_diagnostic,
                caret_affinity_diagnostic: run_input.caret_affinity_diagnostic,
                scroll_follow_diagnostic: run_input.scroll_follow_diagnostic,
                diagnostic: run_input.diagnostic,
            })
            .collect();
        let projection_metadata_diagnostic = (!run_metadata.is_empty()).then(|| {
            TextAreaInlineIfcProjectionMetadataBridgeDiagnostic {
                run_count: run_metadata.len(),
                char_range_count: run_metadata
                    .iter()
                    .filter(|metadata| {
                        metadata.projection_diagnostic.char_range.start
                            <= metadata.projection_diagnostic.char_range.end
                    })
                    .count(),
                char_span: run_metadata
                    .iter()
                    .map(|metadata| metadata.projection_diagnostic.char_span)
                    .sum(),
                effective_content_len: run_metadata
                    .iter()
                    .map(|metadata| metadata.projection_diagnostic.effective_content_len)
                    .sum(),
                inline_preedit_run_count: run_metadata
                    .iter()
                    .filter(|metadata| metadata.projection_diagnostic.has_inline_preedit)
                    .count(),
                preedit_run_count: run_metadata
                    .iter()
                    .filter(|metadata| metadata.projection_diagnostic.is_preedit_run)
                    .count(),
                projection_segment_count: run_metadata
                    .iter()
                    .map(|metadata| metadata.projection_diagnostic.projection_segment_count)
                    .sum(),
            }
        });
        let ime_metadata_diagnostic =
            (!run_metadata.is_empty()).then(|| TextAreaInlineIfcImeMetadataBridgeDiagnostic {
                run_count: run_metadata.len(),
                has_inline_preedit: run_metadata
                    .iter()
                    .any(|metadata| metadata.has_inline_preedit),
                has_preedit_run: run_metadata.iter().any(|metadata| metadata.is_preedit_run),
                preedit_cursor_count: run_metadata
                    .iter()
                    .filter(|metadata| metadata.preedit_cursor.is_some())
                    .count(),
                effective_content_len: run_metadata
                    .iter()
                    .map(|metadata| metadata.diagnostic.effective_content_len)
                    .sum(),
            });
        let caret_affinity_metadata_diagnostic = (!run_metadata.is_empty()).then(|| {
            TextAreaInlineIfcCaretAffinityMetadataBridgeDiagnostic {
                run_count: run_metadata.len(),
                visual_line_count: run_metadata
                    .iter()
                    .map(|metadata| metadata.caret_affinity_diagnostic.visual_line_count)
                    .sum(),
                caret_stop_count: run_metadata
                    .iter()
                    .map(|metadata| metadata.caret_affinity_diagnostic.caret_stop_count)
                    .sum(),
                multi_stop_line_count: run_metadata
                    .iter()
                    .map(|metadata| metadata.caret_affinity_diagnostic.multi_stop_line_count)
                    .sum(),
                preedit_cursor_count: run_metadata
                    .iter()
                    .filter(|metadata| metadata.caret_affinity_diagnostic.has_preedit_cursor)
                    .count(),
            }
        });
        let scroll_follow_metadata_diagnostic = (!run_metadata.is_empty()).then(|| {
            TextAreaInlineIfcScrollFollowMetadataBridgeDiagnostic {
                run_count: run_metadata.len(),
                layout_size_count: run_metadata
                    .iter()
                    .filter(|metadata| {
                        metadata.scroll_follow_diagnostic.layout_size[0] > 0.0
                            && metadata.scroll_follow_diagnostic.layout_size[1] > 0.0
                    })
                    .count(),
                char_span: run_metadata
                    .iter()
                    .map(|metadata| metadata.scroll_follow_diagnostic.char_span)
                    .sum(),
                effective_content_len: run_metadata
                    .iter()
                    .map(|metadata| metadata.scroll_follow_diagnostic.effective_content_len)
                    .sum(),
                visual_line_count: run_metadata
                    .iter()
                    .map(|metadata| metadata.scroll_follow_diagnostic.visual_line_count)
                    .sum(),
                caret_stop_count: run_metadata
                    .iter()
                    .map(|metadata| metadata.scroll_follow_diagnostic.caret_stop_count)
                    .sum(),
            }
        });
        let ime_metadata_status = match ime_metadata_diagnostic.as_ref() {
            Some(diagnostic) if diagnostic.has_inline_preedit || diagnostic.has_preedit_run => {
                TextAreaInlineIfcMetadataBridgeStatus::Observed
            }
            Some(_) => TextAreaInlineIfcMetadataBridgeStatus::ObservedNoPreedit,
            None => TextAreaInlineIfcMetadataBridgeStatus::Unwired,
        };
        let caret_affinity_metadata_status = match caret_affinity_metadata_diagnostic.as_ref() {
            Some(diagnostic) if diagnostic.caret_stop_count > 0 => {
                TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
            }
            Some(_) => TextAreaInlineIfcMetadataBridgeStatus::ObservedNoAffinitySlots,
            None => TextAreaInlineIfcMetadataBridgeStatus::Unwired,
        };
        let projection_metadata_status = match projection_metadata_diagnostic.as_ref() {
            Some(diagnostic) if diagnostic.projection_segment_count > 0 => {
                TextAreaInlineIfcMetadataBridgeStatus::Observed
            }
            Some(_) => TextAreaInlineIfcMetadataBridgeStatus::ObservedNoProjectionSegments,
            None => TextAreaInlineIfcMetadataBridgeStatus::Unwired,
        };
        let scroll_follow_metadata_status = match scroll_follow_metadata_diagnostic.as_ref() {
            Some(diagnostic)
                if diagnostic.layout_size_count > 0
                    && diagnostic.visual_line_count > 0
                    && diagnostic.caret_stop_count > 0 =>
            {
                TextAreaInlineIfcMetadataBridgeStatus::Observed
            }
            Some(_) => TextAreaInlineIfcMetadataBridgeStatus::ObservedNoScrollSource,
            None => TextAreaInlineIfcMetadataBridgeStatus::Unwired,
        };

        Self {
            run_metadata,
            projection_metadata_diagnostic,
            ime_metadata_diagnostic,
            caret_affinity_metadata_diagnostic,
            scroll_follow_metadata_diagnostic,
            projection_metadata_status,
            ime_metadata_status,
            caret_affinity_metadata_status,
            scroll_follow_metadata_status,
            legacy_fallback_confirmed: input.legacy_fallback_confirmed,
            read_only_text_path_separated: input.read_only_text_path_separated,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaInlineIfcMetadataBridgePreflight {
    state: TextAreaInlineIfcMetadataBridgePreflightState,
    blocked_reasons: Vec<TextAreaInlineIfcMetadataBridgeBlockedReason>,
    bridge_input: TextAreaInlineIfcMetadataBridgeInput,
    render_enabled: bool,
    layout_enabled: bool,
}

#[allow(dead_code)]
impl TextAreaInlineIfcMetadataBridgePreflight {
    pub(crate) fn evaluate(input: TextAreaInlineIfcMetadataBridgeInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if input.run_metadata.is_empty() {
            blocked_reasons.push(TextAreaInlineIfcMetadataBridgeBlockedReason::MissingRunMetadata);
        }
        if input.projection_metadata_status == TextAreaInlineIfcMetadataBridgeStatus::Unwired {
            blocked_reasons
                .push(TextAreaInlineIfcMetadataBridgeBlockedReason::ProjectionMetadataUnwired);
        }
        if input.ime_metadata_status == TextAreaInlineIfcMetadataBridgeStatus::Unwired {
            blocked_reasons.push(TextAreaInlineIfcMetadataBridgeBlockedReason::ImeMetadataUnwired);
        }
        if input.caret_affinity_metadata_status == TextAreaInlineIfcMetadataBridgeStatus::Unwired {
            blocked_reasons
                .push(TextAreaInlineIfcMetadataBridgeBlockedReason::CaretAffinityMetadataUnwired);
        }
        if input.scroll_follow_metadata_status == TextAreaInlineIfcMetadataBridgeStatus::Unwired {
            blocked_reasons
                .push(TextAreaInlineIfcMetadataBridgeBlockedReason::ScrollFollowMetadataUnwired);
        }
        if !input.legacy_fallback_confirmed {
            blocked_reasons
                .push(TextAreaInlineIfcMetadataBridgeBlockedReason::LegacyFallbackUnconfirmed);
        }
        if !input.read_only_text_path_separated {
            blocked_reasons.push(
                TextAreaInlineIfcMetadataBridgeBlockedReason::ReadOnlyTextPathSeparationUnconfirmed,
            );
        }

        let state = if blocked_reasons.is_empty() {
            TextAreaInlineIfcMetadataBridgePreflightState::ReadyForDiagnosticEvaluation
        } else {
            TextAreaInlineIfcMetadataBridgePreflightState::Blocked
        };

        Self {
            state,
            blocked_reasons,
            bridge_input: input,
            render_enabled: false,
            layout_enabled: false,
        }
    }

    pub(crate) fn state(&self) -> TextAreaInlineIfcMetadataBridgePreflightState {
        self.state
    }

    pub(crate) fn blocked_reasons(&self) -> &[TextAreaInlineIfcMetadataBridgeBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn bridge_input(&self) -> &TextAreaInlineIfcMetadataBridgeInput {
        &self.bridge_input
    }

    pub(crate) fn render_enabled(&self) -> bool {
        self.render_enabled
    }

    pub(crate) fn layout_enabled(&self) -> bool {
        self.layout_enabled
    }

    pub(crate) fn allows_text_area_editable_behavior_path_switch(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcMetadataObservationReadiness {
    Incomplete,
    Ready,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcReadinessAuditState {
    Blocked,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcBehaviorPathBlockedReason {
    ProjectionPathUnwired,
    ImePathUnwired,
    CaretAffinityPathUnwired,
    ScrollFollowPathUnwired,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcBehaviorPathRecommendation {
    KeepLegacyEditableBehaviorPath,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcReadinessAuditInput {
    pub(crate) bridge_preflight_state: TextAreaInlineIfcMetadataBridgePreflightState,
    pub(crate) bridge_blocked_reasons: Vec<TextAreaInlineIfcMetadataBridgeBlockedReason>,
    pub(crate) run_metadata_count: usize,
    pub(crate) projection_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) ime_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) caret_affinity_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) scroll_follow_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) projection_metadata_diagnostic_present: bool,
    pub(crate) ime_metadata_diagnostic_present: bool,
    pub(crate) caret_affinity_metadata_diagnostic_present: bool,
    pub(crate) scroll_follow_metadata_diagnostic_present: bool,
    pub(crate) legacy_fallback_confirmed: bool,
    pub(crate) read_only_text_path_separated: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcReadinessAuditInput {
    pub(crate) fn from_metadata_bridge_preflight(
        preflight: &TextAreaInlineIfcMetadataBridgePreflight,
    ) -> Self {
        let bridge_input = preflight.bridge_input();
        Self {
            bridge_preflight_state: preflight.state(),
            bridge_blocked_reasons: preflight.blocked_reasons().to_vec(),
            run_metadata_count: bridge_input.run_metadata.len(),
            projection_metadata_status: bridge_input.projection_metadata_status,
            ime_metadata_status: bridge_input.ime_metadata_status,
            caret_affinity_metadata_status: bridge_input.caret_affinity_metadata_status,
            scroll_follow_metadata_status: bridge_input.scroll_follow_metadata_status,
            projection_metadata_diagnostic_present: bridge_input
                .projection_metadata_diagnostic
                .is_some(),
            ime_metadata_diagnostic_present: bridge_input.ime_metadata_diagnostic.is_some(),
            caret_affinity_metadata_diagnostic_present: bridge_input
                .caret_affinity_metadata_diagnostic
                .is_some(),
            scroll_follow_metadata_diagnostic_present: bridge_input
                .scroll_follow_metadata_diagnostic
                .is_some(),
            legacy_fallback_confirmed: bridge_input.legacy_fallback_confirmed,
            read_only_text_path_separated: bridge_input.read_only_text_path_separated,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcReadinessAudit {
    state: TextAreaEditableIfcReadinessAuditState,
    metadata_observation_readiness: TextAreaEditableIfcMetadataObservationReadiness,
    metadata_blocked_reasons: Vec<TextAreaInlineIfcMetadataBridgeBlockedReason>,
    behavior_path_blocked_reasons: Vec<TextAreaEditableIfcBehaviorPathBlockedReason>,
    input: TextAreaEditableIfcReadinessAuditInput,
    recommendation: TextAreaEditableIfcBehaviorPathRecommendation,
    render_enabled: bool,
    layout_enabled: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcReadinessAudit {
    pub(crate) fn evaluate(input: TextAreaEditableIfcReadinessAuditInput) -> Self {
        let metadata_observation_readiness = if input
            .projection_metadata_status
            .has_observed_metadata_source()
            && input.ime_metadata_status.has_observed_metadata_source()
            && input
                .caret_affinity_metadata_status
                .has_observed_metadata_source()
            && input
                .scroll_follow_metadata_status
                .has_observed_metadata_source()
        {
            TextAreaEditableIfcMetadataObservationReadiness::Ready
        } else {
            TextAreaEditableIfcMetadataObservationReadiness::Incomplete
        };

        Self {
            state: TextAreaEditableIfcReadinessAuditState::Blocked,
            metadata_observation_readiness,
            metadata_blocked_reasons: input.bridge_blocked_reasons.clone(),
            behavior_path_blocked_reasons: vec![
                TextAreaEditableIfcBehaviorPathBlockedReason::ProjectionPathUnwired,
                TextAreaEditableIfcBehaviorPathBlockedReason::ImePathUnwired,
                TextAreaEditableIfcBehaviorPathBlockedReason::CaretAffinityPathUnwired,
                TextAreaEditableIfcBehaviorPathBlockedReason::ScrollFollowPathUnwired,
            ],
            input,
            recommendation:
                TextAreaEditableIfcBehaviorPathRecommendation::KeepLegacyEditableBehaviorPath,
            render_enabled: false,
            layout_enabled: false,
        }
    }

    pub(crate) fn state(&self) -> TextAreaEditableIfcReadinessAuditState {
        self.state
    }

    pub(crate) fn metadata_observation_readiness(
        &self,
    ) -> TextAreaEditableIfcMetadataObservationReadiness {
        self.metadata_observation_readiness
    }

    pub(crate) fn metadata_blocked_reasons(
        &self,
    ) -> &[TextAreaInlineIfcMetadataBridgeBlockedReason] {
        &self.metadata_blocked_reasons
    }

    pub(crate) fn behavior_path_blocked_reasons(
        &self,
    ) -> &[TextAreaEditableIfcBehaviorPathBlockedReason] {
        &self.behavior_path_blocked_reasons
    }

    pub(crate) fn input(&self) -> &TextAreaEditableIfcReadinessAuditInput {
        &self.input
    }

    pub(crate) fn recommendation(&self) -> TextAreaEditableIfcBehaviorPathRecommendation {
        self.recommendation
    }

    pub(crate) fn render_enabled(&self) -> bool {
        self.render_enabled
    }

    pub(crate) fn layout_enabled(&self) -> bool {
        self.layout_enabled
    }

    pub(crate) fn allows_text_area_editable_behavior_path_switch(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcImeBehaviorPathPrewireState {
    Blocked,
    DiagnosticPrewired,
    ObservedNoPreedit,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcImeBehaviorPathPrewireBlockedReason {
    MissingRunMetadata,
    ImeMetadataUnwired,
    MissingImeMetadataDiagnostic,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcImeBehaviorPathPrewireDiagnostic {
    pub(crate) ime_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) run_count: usize,
    pub(crate) has_inline_preedit: bool,
    pub(crate) has_preedit_run: bool,
    pub(crate) preedit_cursor_count: usize,
    pub(crate) preedit_cursors: Vec<(usize, usize)>,
    pub(crate) effective_content_len: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcImeBehaviorPathPrewireInput {
    pub(crate) readiness_audit_state: TextAreaEditableIfcReadinessAuditState,
    pub(crate) readiness_behavior_blocked_reasons:
        Vec<TextAreaEditableIfcBehaviorPathBlockedReason>,
    pub(crate) run_metadata_count: usize,
    pub(crate) ime_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) ime_metadata_diagnostic: Option<TextAreaEditableIfcImeBehaviorPathPrewireDiagnostic>,
}

#[allow(dead_code)]
impl TextAreaEditableIfcImeBehaviorPathPrewireInput {
    pub(crate) fn from_readiness_audit_and_metadata_bridge_preflight(
        audit: &TextAreaEditableIfcReadinessAudit,
        preflight: &TextAreaInlineIfcMetadataBridgePreflight,
    ) -> Self {
        let bridge_input = preflight.bridge_input();
        let ime_metadata_diagnostic =
            bridge_input
                .ime_metadata_diagnostic
                .as_ref()
                .map(
                    |diagnostic| TextAreaEditableIfcImeBehaviorPathPrewireDiagnostic {
                        ime_metadata_status: bridge_input.ime_metadata_status,
                        run_count: diagnostic.run_count,
                        has_inline_preedit: diagnostic.has_inline_preedit,
                        has_preedit_run: diagnostic.has_preedit_run,
                        preedit_cursor_count: diagnostic.preedit_cursor_count,
                        preedit_cursors: bridge_input
                            .run_metadata
                            .iter()
                            .filter_map(|metadata| metadata.preedit_cursor)
                            .collect(),
                        effective_content_len: diagnostic.effective_content_len,
                    },
                );

        Self {
            readiness_audit_state: audit.state(),
            readiness_behavior_blocked_reasons: audit.behavior_path_blocked_reasons().to_vec(),
            run_metadata_count: bridge_input.run_metadata.len(),
            ime_metadata_status: bridge_input.ime_metadata_status,
            ime_metadata_diagnostic,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcImeBehaviorPathPrewire {
    state: TextAreaEditableIfcImeBehaviorPathPrewireState,
    blocked_reasons: Vec<TextAreaEditableIfcImeBehaviorPathPrewireBlockedReason>,
    input: TextAreaEditableIfcImeBehaviorPathPrewireInput,
    diagnostic: Option<TextAreaEditableIfcImeBehaviorPathPrewireDiagnostic>,
    diagnostic_prewired: bool,
    ime_behavior_path_ready: bool,
    render_enabled: bool,
    layout_enabled: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcImeBehaviorPathPrewire {
    pub(crate) fn evaluate(input: TextAreaEditableIfcImeBehaviorPathPrewireInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if input.run_metadata_count == 0 {
            blocked_reasons
                .push(TextAreaEditableIfcImeBehaviorPathPrewireBlockedReason::MissingRunMetadata);
        }
        if input.ime_metadata_status == TextAreaInlineIfcMetadataBridgeStatus::Unwired {
            blocked_reasons
                .push(TextAreaEditableIfcImeBehaviorPathPrewireBlockedReason::ImeMetadataUnwired);
        }
        if input.ime_metadata_diagnostic.is_none() {
            blocked_reasons.push(
                TextAreaEditableIfcImeBehaviorPathPrewireBlockedReason::
                    MissingImeMetadataDiagnostic,
            );
        }

        let diagnostic = input.ime_metadata_diagnostic.clone();
        let state = if !blocked_reasons.is_empty() {
            TextAreaEditableIfcImeBehaviorPathPrewireState::Blocked
        } else if input.ime_metadata_status
            == TextAreaInlineIfcMetadataBridgeStatus::ObservedNoPreedit
        {
            TextAreaEditableIfcImeBehaviorPathPrewireState::ObservedNoPreedit
        } else {
            TextAreaEditableIfcImeBehaviorPathPrewireState::DiagnosticPrewired
        };

        Self {
            state,
            blocked_reasons,
            diagnostic_prewired: matches!(
                state,
                TextAreaEditableIfcImeBehaviorPathPrewireState::DiagnosticPrewired
                    | TextAreaEditableIfcImeBehaviorPathPrewireState::ObservedNoPreedit
            ),
            input,
            diagnostic,
            ime_behavior_path_ready: false,
            render_enabled: false,
            layout_enabled: false,
        }
    }

    pub(crate) fn state(&self) -> TextAreaEditableIfcImeBehaviorPathPrewireState {
        self.state
    }

    pub(crate) fn blocked_reasons(
        &self,
    ) -> &[TextAreaEditableIfcImeBehaviorPathPrewireBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn input(&self) -> &TextAreaEditableIfcImeBehaviorPathPrewireInput {
        &self.input
    }

    pub(crate) fn diagnostic(
        &self,
    ) -> Option<&TextAreaEditableIfcImeBehaviorPathPrewireDiagnostic> {
        self.diagnostic.as_ref()
    }

    pub(crate) fn diagnostic_prewired(&self) -> bool {
        self.diagnostic_prewired
    }

    pub(crate) fn ime_behavior_path_ready(&self) -> bool {
        self.ime_behavior_path_ready
    }

    pub(crate) fn render_enabled(&self) -> bool {
        self.render_enabled
    }

    pub(crate) fn layout_enabled(&self) -> bool {
        self.layout_enabled
    }

    pub(crate) fn allows_text_area_editable_behavior_path_switch(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcCaretAffinityMetadataSourceState {
    Blocked,
    MetadataObserved,
    ObservedNoAffinitySlots,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcCaretAffinityMetadataSourceBlockedReason {
    MissingRunMetadata,
    CaretAffinityMetadataUnwired,
    MissingCaretAffinityMetadataDiagnostic,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcCaretAffinityMetadataSourceDiagnostic {
    pub(crate) caret_affinity_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) run_count: usize,
    pub(crate) visual_line_count: usize,
    pub(crate) caret_stop_count: usize,
    pub(crate) multi_stop_line_count: usize,
    pub(crate) preedit_cursor_count: usize,
    pub(crate) per_run_caret_diagnostics: Vec<TextAreaTextRunInlineIfcCaretAffinityDiagnostic>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcCaretAffinityMetadataSourceInput {
    pub(crate) readiness_audit_state: TextAreaEditableIfcReadinessAuditState,
    pub(crate) readiness_behavior_blocked_reasons:
        Vec<TextAreaEditableIfcBehaviorPathBlockedReason>,
    pub(crate) run_metadata_count: usize,
    pub(crate) caret_affinity_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) caret_affinity_metadata_diagnostic:
        Option<TextAreaEditableIfcCaretAffinityMetadataSourceDiagnostic>,
}

#[allow(dead_code)]
impl TextAreaEditableIfcCaretAffinityMetadataSourceInput {
    pub(crate) fn from_readiness_audit_and_metadata_bridge_preflight(
        audit: &TextAreaEditableIfcReadinessAudit,
        preflight: &TextAreaInlineIfcMetadataBridgePreflight,
    ) -> Self {
        let bridge_input = preflight.bridge_input();
        let caret_affinity_metadata_diagnostic = bridge_input
            .caret_affinity_metadata_diagnostic
            .as_ref()
            .map(
                |diagnostic| TextAreaEditableIfcCaretAffinityMetadataSourceDiagnostic {
                    caret_affinity_metadata_status: bridge_input.caret_affinity_metadata_status,
                    run_count: diagnostic.run_count,
                    visual_line_count: diagnostic.visual_line_count,
                    caret_stop_count: diagnostic.caret_stop_count,
                    multi_stop_line_count: diagnostic.multi_stop_line_count,
                    preedit_cursor_count: diagnostic.preedit_cursor_count,
                    per_run_caret_diagnostics: bridge_input
                        .run_metadata
                        .iter()
                        .map(|metadata| metadata.caret_affinity_diagnostic.clone())
                        .collect(),
                },
            );

        Self {
            readiness_audit_state: audit.state(),
            readiness_behavior_blocked_reasons: audit.behavior_path_blocked_reasons().to_vec(),
            run_metadata_count: bridge_input.run_metadata.len(),
            caret_affinity_metadata_status: bridge_input.caret_affinity_metadata_status,
            caret_affinity_metadata_diagnostic,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcCaretAffinityMetadataSource {
    state: TextAreaEditableIfcCaretAffinityMetadataSourceState,
    blocked_reasons: Vec<TextAreaEditableIfcCaretAffinityMetadataSourceBlockedReason>,
    input: TextAreaEditableIfcCaretAffinityMetadataSourceInput,
    diagnostic: Option<TextAreaEditableIfcCaretAffinityMetadataSourceDiagnostic>,
    metadata_observed: bool,
    caret_affinity_behavior_path_ready: bool,
    render_enabled: bool,
    layout_enabled: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcCaretAffinityMetadataSource {
    pub(crate) fn evaluate(input: TextAreaEditableIfcCaretAffinityMetadataSourceInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if input.run_metadata_count == 0 {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityMetadataSourceBlockedReason::MissingRunMetadata,
            );
        }
        if input.caret_affinity_metadata_status == TextAreaInlineIfcMetadataBridgeStatus::Unwired {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityMetadataSourceBlockedReason::
                    CaretAffinityMetadataUnwired,
            );
        }
        if input.caret_affinity_metadata_diagnostic.is_none() {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityMetadataSourceBlockedReason::
                    MissingCaretAffinityMetadataDiagnostic,
            );
        }

        let diagnostic = input.caret_affinity_metadata_diagnostic.clone();
        let state = if !blocked_reasons.is_empty() {
            TextAreaEditableIfcCaretAffinityMetadataSourceState::Blocked
        } else if input.caret_affinity_metadata_status
            == TextAreaInlineIfcMetadataBridgeStatus::ObservedNoAffinitySlots
        {
            TextAreaEditableIfcCaretAffinityMetadataSourceState::ObservedNoAffinitySlots
        } else {
            TextAreaEditableIfcCaretAffinityMetadataSourceState::MetadataObserved
        };

        Self {
            state,
            blocked_reasons,
            metadata_observed: matches!(
                state,
                TextAreaEditableIfcCaretAffinityMetadataSourceState::MetadataObserved
                    | TextAreaEditableIfcCaretAffinityMetadataSourceState::ObservedNoAffinitySlots
            ),
            input,
            diagnostic,
            caret_affinity_behavior_path_ready: false,
            render_enabled: false,
            layout_enabled: false,
        }
    }

    pub(crate) fn state(&self) -> TextAreaEditableIfcCaretAffinityMetadataSourceState {
        self.state
    }

    pub(crate) fn blocked_reasons(
        &self,
    ) -> &[TextAreaEditableIfcCaretAffinityMetadataSourceBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn input(&self) -> &TextAreaEditableIfcCaretAffinityMetadataSourceInput {
        &self.input
    }

    pub(crate) fn diagnostic(
        &self,
    ) -> Option<&TextAreaEditableIfcCaretAffinityMetadataSourceDiagnostic> {
        self.diagnostic.as_ref()
    }

    pub(crate) fn metadata_observed(&self) -> bool {
        self.metadata_observed
    }

    pub(crate) fn caret_affinity_behavior_path_ready(&self) -> bool {
        self.caret_affinity_behavior_path_ready
    }

    pub(crate) fn render_enabled(&self) -> bool {
        self.render_enabled
    }

    pub(crate) fn layout_enabled(&self) -> bool {
        self.layout_enabled
    }

    pub(crate) fn allows_text_area_editable_behavior_path_switch(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcProjectionBehaviorPathPrewireState {
    Blocked,
    DiagnosticPrewired,
    ObservedNoProjectionSegments,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcProjectionBehaviorPathPrewireBlockedReason {
    MissingRunMetadata,
    ProjectionMetadataUnwired,
    MissingProjectionMetadataDiagnostic,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcProjectionBehaviorPathPrewireDiagnostic {
    pub(crate) projection_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) run_count: usize,
    pub(crate) char_range_count: usize,
    pub(crate) char_span: usize,
    pub(crate) effective_content_len: usize,
    pub(crate) inline_preedit_run_count: usize,
    pub(crate) preedit_run_count: usize,
    pub(crate) projection_segment_count: usize,
    pub(crate) per_run_projection_diagnostics: Vec<TextAreaTextRunInlineIfcProjectionDiagnostic>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcProjectionBehaviorPathPrewireInput {
    pub(crate) readiness_audit_state: TextAreaEditableIfcReadinessAuditState,
    pub(crate) readiness_behavior_blocked_reasons:
        Vec<TextAreaEditableIfcBehaviorPathBlockedReason>,
    pub(crate) run_metadata_count: usize,
    pub(crate) projection_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) projection_metadata_diagnostic:
        Option<TextAreaEditableIfcProjectionBehaviorPathPrewireDiagnostic>,
}

#[allow(dead_code)]
impl TextAreaEditableIfcProjectionBehaviorPathPrewireInput {
    pub(crate) fn from_readiness_audit_and_metadata_bridge_preflight(
        audit: &TextAreaEditableIfcReadinessAudit,
        preflight: &TextAreaInlineIfcMetadataBridgePreflight,
    ) -> Self {
        let bridge_input = preflight.bridge_input();
        let projection_metadata_diagnostic = bridge_input
            .projection_metadata_diagnostic
            .as_ref()
            .map(
                |diagnostic| TextAreaEditableIfcProjectionBehaviorPathPrewireDiagnostic {
                    projection_metadata_status: bridge_input.projection_metadata_status,
                    run_count: diagnostic.run_count,
                    char_range_count: diagnostic.char_range_count,
                    char_span: diagnostic.char_span,
                    effective_content_len: diagnostic.effective_content_len,
                    inline_preedit_run_count: diagnostic.inline_preedit_run_count,
                    preedit_run_count: diagnostic.preedit_run_count,
                    projection_segment_count: diagnostic.projection_segment_count,
                    per_run_projection_diagnostics: bridge_input
                        .run_metadata
                        .iter()
                        .map(|metadata| metadata.projection_diagnostic.clone())
                        .collect(),
                },
            );

        Self {
            readiness_audit_state: audit.state(),
            readiness_behavior_blocked_reasons: audit.behavior_path_blocked_reasons().to_vec(),
            run_metadata_count: bridge_input.run_metadata.len(),
            projection_metadata_status: bridge_input.projection_metadata_status,
            projection_metadata_diagnostic,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcProjectionBehaviorPathPrewire {
    state: TextAreaEditableIfcProjectionBehaviorPathPrewireState,
    blocked_reasons: Vec<TextAreaEditableIfcProjectionBehaviorPathPrewireBlockedReason>,
    input: TextAreaEditableIfcProjectionBehaviorPathPrewireInput,
    diagnostic: Option<TextAreaEditableIfcProjectionBehaviorPathPrewireDiagnostic>,
    diagnostic_prewired: bool,
    projection_behavior_path_ready: bool,
    render_enabled: bool,
    layout_enabled: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcProjectionBehaviorPathPrewire {
    pub(crate) fn evaluate(input: TextAreaEditableIfcProjectionBehaviorPathPrewireInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if input.run_metadata_count == 0 {
            blocked_reasons.push(
                TextAreaEditableIfcProjectionBehaviorPathPrewireBlockedReason::MissingRunMetadata,
            );
        }
        if input.projection_metadata_status == TextAreaInlineIfcMetadataBridgeStatus::Unwired {
            blocked_reasons.push(
                TextAreaEditableIfcProjectionBehaviorPathPrewireBlockedReason::
                    ProjectionMetadataUnwired,
            );
        }
        if input.projection_metadata_diagnostic.is_none() {
            blocked_reasons.push(
                TextAreaEditableIfcProjectionBehaviorPathPrewireBlockedReason::
                    MissingProjectionMetadataDiagnostic,
            );
        }

        let diagnostic = input.projection_metadata_diagnostic.clone();
        let state = if !blocked_reasons.is_empty() {
            TextAreaEditableIfcProjectionBehaviorPathPrewireState::Blocked
        } else if input.projection_metadata_status
            == TextAreaInlineIfcMetadataBridgeStatus::ObservedNoProjectionSegments
        {
            TextAreaEditableIfcProjectionBehaviorPathPrewireState::ObservedNoProjectionSegments
        } else {
            TextAreaEditableIfcProjectionBehaviorPathPrewireState::DiagnosticPrewired
        };

        Self {
            state,
            blocked_reasons,
            diagnostic_prewired: matches!(
                state,
                TextAreaEditableIfcProjectionBehaviorPathPrewireState::DiagnosticPrewired
                    | TextAreaEditableIfcProjectionBehaviorPathPrewireState::
                        ObservedNoProjectionSegments
            ),
            input,
            diagnostic,
            projection_behavior_path_ready: false,
            render_enabled: false,
            layout_enabled: false,
        }
    }

    pub(crate) fn state(&self) -> TextAreaEditableIfcProjectionBehaviorPathPrewireState {
        self.state
    }

    pub(crate) fn blocked_reasons(
        &self,
    ) -> &[TextAreaEditableIfcProjectionBehaviorPathPrewireBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn input(&self) -> &TextAreaEditableIfcProjectionBehaviorPathPrewireInput {
        &self.input
    }

    pub(crate) fn diagnostic(
        &self,
    ) -> Option<&TextAreaEditableIfcProjectionBehaviorPathPrewireDiagnostic> {
        self.diagnostic.as_ref()
    }

    pub(crate) fn diagnostic_prewired(&self) -> bool {
        self.diagnostic_prewired
    }

    pub(crate) fn projection_behavior_path_ready(&self) -> bool {
        self.projection_behavior_path_ready
    }

    pub(crate) fn render_enabled(&self) -> bool {
        self.render_enabled
    }

    pub(crate) fn layout_enabled(&self) -> bool {
        self.layout_enabled
    }

    pub(crate) fn allows_text_area_editable_behavior_path_switch(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcScrollFollowBehaviorPathPrewireState {
    Blocked,
    DiagnosticPrewired,
    ObservedNoScrollSource,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcScrollFollowBehaviorPathPrewireBlockedReason {
    MissingRunMetadata,
    ScrollFollowMetadataUnwired,
    MissingScrollFollowMetadataDiagnostic,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcScrollFollowBehaviorPathPrewireDiagnostic {
    pub(crate) scroll_follow_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) run_count: usize,
    pub(crate) layout_size_count: usize,
    pub(crate) char_span: usize,
    pub(crate) effective_content_len: usize,
    pub(crate) visual_line_count: usize,
    pub(crate) caret_stop_count: usize,
    pub(crate) per_run_scroll_follow_diagnostics:
        Vec<TextAreaTextRunInlineIfcScrollFollowDiagnostic>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput {
    pub(crate) readiness_audit_state: TextAreaEditableIfcReadinessAuditState,
    pub(crate) readiness_behavior_blocked_reasons:
        Vec<TextAreaEditableIfcBehaviorPathBlockedReason>,
    pub(crate) run_metadata_count: usize,
    pub(crate) scroll_follow_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) scroll_follow_metadata_diagnostic:
        Option<TextAreaEditableIfcScrollFollowBehaviorPathPrewireDiagnostic>,
}

#[allow(dead_code)]
impl TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput {
    pub(crate) fn from_readiness_audit_and_metadata_bridge_preflight(
        audit: &TextAreaEditableIfcReadinessAudit,
        preflight: &TextAreaInlineIfcMetadataBridgePreflight,
    ) -> Self {
        let bridge_input = preflight.bridge_input();
        let scroll_follow_metadata_diagnostic = bridge_input
            .scroll_follow_metadata_diagnostic
            .as_ref()
            .map(
                |diagnostic| TextAreaEditableIfcScrollFollowBehaviorPathPrewireDiagnostic {
                    scroll_follow_metadata_status: bridge_input.scroll_follow_metadata_status,
                    run_count: diagnostic.run_count,
                    layout_size_count: diagnostic.layout_size_count,
                    char_span: diagnostic.char_span,
                    effective_content_len: diagnostic.effective_content_len,
                    visual_line_count: diagnostic.visual_line_count,
                    caret_stop_count: diagnostic.caret_stop_count,
                    per_run_scroll_follow_diagnostics: bridge_input
                        .run_metadata
                        .iter()
                        .map(|metadata| metadata.scroll_follow_diagnostic.clone())
                        .collect(),
                },
            );

        Self {
            readiness_audit_state: audit.state(),
            readiness_behavior_blocked_reasons: audit.behavior_path_blocked_reasons().to_vec(),
            run_metadata_count: bridge_input.run_metadata.len(),
            scroll_follow_metadata_status: bridge_input.scroll_follow_metadata_status,
            scroll_follow_metadata_diagnostic,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcScrollFollowBehaviorPathPrewire {
    state: TextAreaEditableIfcScrollFollowBehaviorPathPrewireState,
    blocked_reasons: Vec<TextAreaEditableIfcScrollFollowBehaviorPathPrewireBlockedReason>,
    input: TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput,
    diagnostic: Option<TextAreaEditableIfcScrollFollowBehaviorPathPrewireDiagnostic>,
    diagnostic_prewired: bool,
    scroll_follow_behavior_path_ready: bool,
    render_enabled: bool,
    layout_enabled: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcScrollFollowBehaviorPathPrewire {
    pub(crate) fn evaluate(input: TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if input.run_metadata_count == 0 {
            blocked_reasons.push(
                TextAreaEditableIfcScrollFollowBehaviorPathPrewireBlockedReason::MissingRunMetadata,
            );
        }
        if input.scroll_follow_metadata_status == TextAreaInlineIfcMetadataBridgeStatus::Unwired {
            blocked_reasons.push(
                TextAreaEditableIfcScrollFollowBehaviorPathPrewireBlockedReason::
                    ScrollFollowMetadataUnwired,
            );
        }
        if input.scroll_follow_metadata_diagnostic.is_none() {
            blocked_reasons.push(
                TextAreaEditableIfcScrollFollowBehaviorPathPrewireBlockedReason::
                    MissingScrollFollowMetadataDiagnostic,
            );
        }

        let diagnostic = input.scroll_follow_metadata_diagnostic.clone();
        let state = if !blocked_reasons.is_empty() {
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::Blocked
        } else if input.scroll_follow_metadata_status
            == TextAreaInlineIfcMetadataBridgeStatus::ObservedNoScrollSource
        {
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::ObservedNoScrollSource
        } else {
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::DiagnosticPrewired
        };

        Self {
            state,
            blocked_reasons,
            diagnostic_prewired: matches!(
                state,
                TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::DiagnosticPrewired
                    | TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::
                        ObservedNoScrollSource
            ),
            input,
            diagnostic,
            scroll_follow_behavior_path_ready: false,
            render_enabled: false,
            layout_enabled: false,
        }
    }

    pub(crate) fn state(&self) -> TextAreaEditableIfcScrollFollowBehaviorPathPrewireState {
        self.state
    }

    pub(crate) fn blocked_reasons(
        &self,
    ) -> &[TextAreaEditableIfcScrollFollowBehaviorPathPrewireBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn input(&self) -> &TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput {
        &self.input
    }

    pub(crate) fn diagnostic(
        &self,
    ) -> Option<&TextAreaEditableIfcScrollFollowBehaviorPathPrewireDiagnostic> {
        self.diagnostic.as_ref()
    }

    pub(crate) fn diagnostic_prewired(&self) -> bool {
        self.diagnostic_prewired
    }

    pub(crate) fn scroll_follow_behavior_path_ready(&self) -> bool {
        self.scroll_follow_behavior_path_ready
    }

    pub(crate) fn render_enabled(&self) -> bool {
        self.render_enabled
    }

    pub(crate) fn layout_enabled(&self) -> bool {
        self.layout_enabled
    }

    pub(crate) fn allows_text_area_editable_behavior_path_switch(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcBehaviorPathStatusState {
    Blocked,
    ReadyForStatusObservation,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcBehaviorPathStatusBlockedReason {
    MetadataBridgeBlocked,
    MetadataObservationIncomplete,
    ImePrewireBlocked,
    ImeDiagnosticNotObserved,
    CaretAffinityMetadataSourceBlocked,
    CaretAffinityMetadataNotObserved,
    ProjectionPrewireBlocked,
    ProjectionDiagnosticNotObserved,
    ScrollFollowPrewireBlocked,
    ScrollFollowDiagnosticNotObserved,
    StatusObservationOnly,
    BehaviorPathsStillNotReady,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcBehaviorPathStatusRecommendation {
    KeepLegacyEditableBehaviorPath,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcBehaviorPathStatusReport {
    pub(crate) metadata_bridge_state: TextAreaInlineIfcMetadataBridgePreflightState,
    pub(crate) metadata_bridge_blocked_reasons: Vec<TextAreaInlineIfcMetadataBridgeBlockedReason>,
    pub(crate) projection_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) ime_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) caret_affinity_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) scroll_follow_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) audit_state: TextAreaEditableIfcReadinessAuditState,
    pub(crate) audit_recommendation: TextAreaEditableIfcBehaviorPathRecommendation,
    pub(crate) ime_prewire_state: TextAreaEditableIfcImeBehaviorPathPrewireState,
    pub(crate) caret_affinity_metadata_source_state:
        TextAreaEditableIfcCaretAffinityMetadataSourceState,
    pub(crate) projection_prewire_state: TextAreaEditableIfcProjectionBehaviorPathPrewireState,
    pub(crate) scroll_follow_prewire_state: TextAreaEditableIfcScrollFollowBehaviorPathPrewireState,
    pub(crate) ime_diagnostic_prewired: bool,
    pub(crate) caret_affinity_metadata_observed: bool,
    pub(crate) projection_diagnostic_prewired: bool,
    pub(crate) scroll_follow_diagnostic_prewired: bool,
    pub(crate) ime_behavior_path_ready: bool,
    pub(crate) caret_affinity_behavior_path_ready: bool,
    pub(crate) projection_behavior_path_ready: bool,
    pub(crate) scroll_follow_behavior_path_ready: bool,
    pub(crate) prepared_render_default_observed: bool,
    pub(crate) render_enabled: bool,
    pub(crate) layout_enabled: bool,
    pub(crate) allows_text_area_editable_behavior_path_switch: bool,
    pub(crate) behavior_path_blocked_reasons: Vec<TextAreaEditableIfcBehaviorPathBlockedReason>,
    pub(crate) recommendation: TextAreaEditableIfcBehaviorPathStatusRecommendation,
}

#[allow(dead_code)]
impl TextAreaEditableIfcBehaviorPathStatusReport {
    pub(crate) fn behavior_paths_still_not_ready(&self) -> bool {
        !self.ime_behavior_path_ready
            || !self.caret_affinity_behavior_path_ready
            || !self.projection_behavior_path_ready
            || !self.scroll_follow_behavior_path_ready
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcBehaviorPathStatusInput {
    pub(crate) metadata_bridge_state: TextAreaInlineIfcMetadataBridgePreflightState,
    pub(crate) metadata_bridge_blocked_reasons: Vec<TextAreaInlineIfcMetadataBridgeBlockedReason>,
    pub(crate) projection_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) ime_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) caret_affinity_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) scroll_follow_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) audit_state: TextAreaEditableIfcReadinessAuditState,
    pub(crate) audit_metadata_observation_readiness:
        TextAreaEditableIfcMetadataObservationReadiness,
    pub(crate) audit_recommendation: TextAreaEditableIfcBehaviorPathRecommendation,
    pub(crate) behavior_path_blocked_reasons: Vec<TextAreaEditableIfcBehaviorPathBlockedReason>,
    pub(crate) ime_prewire_state: TextAreaEditableIfcImeBehaviorPathPrewireState,
    pub(crate) ime_prewire_blocked_reasons:
        Vec<TextAreaEditableIfcImeBehaviorPathPrewireBlockedReason>,
    pub(crate) ime_diagnostic_prewired: bool,
    pub(crate) ime_behavior_path_ready: bool,
    pub(crate) caret_affinity_metadata_source_state:
        TextAreaEditableIfcCaretAffinityMetadataSourceState,
    pub(crate) caret_affinity_metadata_source_blocked_reasons:
        Vec<TextAreaEditableIfcCaretAffinityMetadataSourceBlockedReason>,
    pub(crate) caret_affinity_metadata_observed: bool,
    pub(crate) caret_affinity_behavior_path_ready: bool,
    pub(crate) projection_prewire_state: TextAreaEditableIfcProjectionBehaviorPathPrewireState,
    pub(crate) projection_prewire_blocked_reasons:
        Vec<TextAreaEditableIfcProjectionBehaviorPathPrewireBlockedReason>,
    pub(crate) projection_diagnostic_prewired: bool,
    pub(crate) projection_behavior_path_ready: bool,
    pub(crate) scroll_follow_prewire_state: TextAreaEditableIfcScrollFollowBehaviorPathPrewireState,
    pub(crate) scroll_follow_prewire_blocked_reasons:
        Vec<TextAreaEditableIfcScrollFollowBehaviorPathPrewireBlockedReason>,
    pub(crate) scroll_follow_diagnostic_prewired: bool,
    pub(crate) scroll_follow_behavior_path_ready: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcBehaviorPathStatusInput {
    pub(crate) fn from_bridge_audit_and_behavior_prewires(
        preflight: &TextAreaInlineIfcMetadataBridgePreflight,
        audit: &TextAreaEditableIfcReadinessAudit,
        ime_prewire: &TextAreaEditableIfcImeBehaviorPathPrewire,
        caret_affinity_metadata_source: &TextAreaEditableIfcCaretAffinityMetadataSource,
        projection_prewire: &TextAreaEditableIfcProjectionBehaviorPathPrewire,
        scroll_follow_prewire: &TextAreaEditableIfcScrollFollowBehaviorPathPrewire,
    ) -> Self {
        let bridge_input = preflight.bridge_input();
        Self {
            metadata_bridge_state: preflight.state(),
            metadata_bridge_blocked_reasons: preflight.blocked_reasons().to_vec(),
            projection_metadata_status: bridge_input.projection_metadata_status,
            ime_metadata_status: bridge_input.ime_metadata_status,
            caret_affinity_metadata_status: bridge_input.caret_affinity_metadata_status,
            scroll_follow_metadata_status: bridge_input.scroll_follow_metadata_status,
            audit_state: audit.state(),
            audit_metadata_observation_readiness: audit.metadata_observation_readiness(),
            audit_recommendation: audit.recommendation(),
            behavior_path_blocked_reasons: audit.behavior_path_blocked_reasons().to_vec(),
            ime_prewire_state: ime_prewire.state(),
            ime_prewire_blocked_reasons: ime_prewire.blocked_reasons().to_vec(),
            ime_diagnostic_prewired: ime_prewire.diagnostic_prewired(),
            ime_behavior_path_ready: ime_prewire.ime_behavior_path_ready(),
            caret_affinity_metadata_source_state: caret_affinity_metadata_source.state(),
            caret_affinity_metadata_source_blocked_reasons: caret_affinity_metadata_source
                .blocked_reasons()
                .to_vec(),
            caret_affinity_metadata_observed: caret_affinity_metadata_source.metadata_observed(),
            caret_affinity_behavior_path_ready: caret_affinity_metadata_source
                .caret_affinity_behavior_path_ready(),
            projection_prewire_state: projection_prewire.state(),
            projection_prewire_blocked_reasons: projection_prewire.blocked_reasons().to_vec(),
            projection_diagnostic_prewired: projection_prewire.diagnostic_prewired(),
            projection_behavior_path_ready: projection_prewire.projection_behavior_path_ready(),
            scroll_follow_prewire_state: scroll_follow_prewire.state(),
            scroll_follow_prewire_blocked_reasons: scroll_follow_prewire.blocked_reasons().to_vec(),
            scroll_follow_diagnostic_prewired: scroll_follow_prewire.diagnostic_prewired(),
            scroll_follow_behavior_path_ready: scroll_follow_prewire
                .scroll_follow_behavior_path_ready(),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcBehaviorPathStatus {
    state: TextAreaEditableIfcBehaviorPathStatusState,
    blocked_reasons: Vec<TextAreaEditableIfcBehaviorPathStatusBlockedReason>,
    input: TextAreaEditableIfcBehaviorPathStatusInput,
    report: TextAreaEditableIfcBehaviorPathStatusReport,
    recommendation: TextAreaEditableIfcBehaviorPathStatusRecommendation,
    render_enabled: bool,
    layout_enabled: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcBehaviorPathStatus {
    pub(crate) fn evaluate(input: TextAreaEditableIfcBehaviorPathStatusInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if input.metadata_bridge_state
            != TextAreaInlineIfcMetadataBridgePreflightState::ReadyForDiagnosticEvaluation
            || !input.metadata_bridge_blocked_reasons.is_empty()
        {
            blocked_reasons
                .push(TextAreaEditableIfcBehaviorPathStatusBlockedReason::MetadataBridgeBlocked);
        }
        if input.audit_metadata_observation_readiness
            != TextAreaEditableIfcMetadataObservationReadiness::Ready
        {
            blocked_reasons.push(
                TextAreaEditableIfcBehaviorPathStatusBlockedReason::MetadataObservationIncomplete,
            );
        }
        if input.ime_prewire_state == TextAreaEditableIfcImeBehaviorPathPrewireState::Blocked
            || !input.ime_prewire_blocked_reasons.is_empty()
        {
            blocked_reasons
                .push(TextAreaEditableIfcBehaviorPathStatusBlockedReason::ImePrewireBlocked);
        }
        if !input.ime_diagnostic_prewired {
            blocked_reasons
                .push(TextAreaEditableIfcBehaviorPathStatusBlockedReason::ImeDiagnosticNotObserved);
        }
        if input.caret_affinity_metadata_source_state
            == TextAreaEditableIfcCaretAffinityMetadataSourceState::Blocked
            || !input
                .caret_affinity_metadata_source_blocked_reasons
                .is_empty()
        {
            blocked_reasons.push(
                TextAreaEditableIfcBehaviorPathStatusBlockedReason::CaretAffinityMetadataSourceBlocked,
            );
        }
        if !input.caret_affinity_metadata_observed {
            blocked_reasons.push(
                TextAreaEditableIfcBehaviorPathStatusBlockedReason::CaretAffinityMetadataNotObserved,
            );
        }
        if input.projection_prewire_state
            == TextAreaEditableIfcProjectionBehaviorPathPrewireState::Blocked
            || !input.projection_prewire_blocked_reasons.is_empty()
        {
            blocked_reasons
                .push(TextAreaEditableIfcBehaviorPathStatusBlockedReason::ProjectionPrewireBlocked);
        }
        if !input.projection_diagnostic_prewired {
            blocked_reasons.push(
                TextAreaEditableIfcBehaviorPathStatusBlockedReason::ProjectionDiagnosticNotObserved,
            );
        }
        if input.scroll_follow_prewire_state
            == TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::Blocked
            || !input.scroll_follow_prewire_blocked_reasons.is_empty()
        {
            blocked_reasons.push(
                TextAreaEditableIfcBehaviorPathStatusBlockedReason::ScrollFollowPrewireBlocked,
            );
        }
        if !input.scroll_follow_diagnostic_prewired {
            blocked_reasons.push(
                TextAreaEditableIfcBehaviorPathStatusBlockedReason::ScrollFollowDiagnosticNotObserved,
            );
        }

        let state = if blocked_reasons.is_empty() {
            TextAreaEditableIfcBehaviorPathStatusState::ReadyForStatusObservation
        } else {
            TextAreaEditableIfcBehaviorPathStatusState::Blocked
        };
        let recommendation =
            TextAreaEditableIfcBehaviorPathStatusRecommendation::KeepLegacyEditableBehaviorPath;
        let report = TextAreaEditableIfcBehaviorPathStatusReport {
            metadata_bridge_state: input.metadata_bridge_state,
            metadata_bridge_blocked_reasons: input.metadata_bridge_blocked_reasons.clone(),
            projection_metadata_status: input.projection_metadata_status,
            ime_metadata_status: input.ime_metadata_status,
            caret_affinity_metadata_status: input.caret_affinity_metadata_status,
            scroll_follow_metadata_status: input.scroll_follow_metadata_status,
            audit_state: input.audit_state,
            audit_recommendation: input.audit_recommendation,
            ime_prewire_state: input.ime_prewire_state,
            caret_affinity_metadata_source_state: input.caret_affinity_metadata_source_state,
            projection_prewire_state: input.projection_prewire_state,
            scroll_follow_prewire_state: input.scroll_follow_prewire_state,
            ime_diagnostic_prewired: input.ime_diagnostic_prewired,
            caret_affinity_metadata_observed: input.caret_affinity_metadata_observed,
            projection_diagnostic_prewired: input.projection_diagnostic_prewired,
            scroll_follow_diagnostic_prewired: input.scroll_follow_diagnostic_prewired,
            ime_behavior_path_ready: input.ime_behavior_path_ready,
            caret_affinity_behavior_path_ready: input.caret_affinity_behavior_path_ready,
            projection_behavior_path_ready: input.projection_behavior_path_ready,
            scroll_follow_behavior_path_ready: input.scroll_follow_behavior_path_ready,
            prepared_render_default_observed: true,
            render_enabled: false,
            layout_enabled: false,
            allows_text_area_editable_behavior_path_switch: false,
            behavior_path_blocked_reasons: input.behavior_path_blocked_reasons.clone(),
            recommendation,
        };

        Self {
            state,
            blocked_reasons,
            input,
            report,
            recommendation,
            render_enabled: false,
            layout_enabled: false,
        }
    }

    pub(crate) fn state(&self) -> TextAreaEditableIfcBehaviorPathStatusState {
        self.state
    }

    pub(crate) fn blocked_reasons(&self) -> &[TextAreaEditableIfcBehaviorPathStatusBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn input(&self) -> &TextAreaEditableIfcBehaviorPathStatusInput {
        &self.input
    }

    pub(crate) fn report(&self) -> &TextAreaEditableIfcBehaviorPathStatusReport {
        &self.report
    }

    pub(crate) fn behavior_path_switch_blocked_reasons(
        &self,
    ) -> Vec<TextAreaEditableIfcBehaviorPathStatusBlockedReason> {
        let mut reasons = Vec::new();
        if self.state == TextAreaEditableIfcBehaviorPathStatusState::ReadyForStatusObservation {
            reasons.push(TextAreaEditableIfcBehaviorPathStatusBlockedReason::StatusObservationOnly);
        }
        if self.report.behavior_paths_still_not_ready() {
            reasons.push(
                TextAreaEditableIfcBehaviorPathStatusBlockedReason::BehaviorPathsStillNotReady,
            );
        }
        reasons
    }

    pub(crate) fn recommendation(&self) -> TextAreaEditableIfcBehaviorPathStatusRecommendation {
        self.recommendation
    }

    pub(crate) fn render_enabled(&self) -> bool {
        self.render_enabled
    }

    pub(crate) fn layout_enabled(&self) -> bool {
        self.layout_enabled
    }

    pub(crate) fn allows_text_area_editable_behavior_path_switch(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcBehaviorPathReadinessState {
    BlockedNotReady,
    CaretAffinityCandidateObserved,
    Ready,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcBehaviorPathReadinessCandidate {
    None,
    CaretAffinityCandidateObserved,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcBehaviorPathReadinessBlockedReason {
    BehaviorPathStatusBlocked,
    BehaviorPathsStillNotReady,
    ImeBehaviorPathNotReady,
    ProjectionBehaviorPathNotReady,
    ScrollFollowBehaviorPathNotReady,
    CaretAffinityBehaviorPathNotReady,
    CaretAffinityMetadataSourceNotObserved,
    CaretAffinityMetadataHasNoCandidateSlots,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcBehaviorPathReadinessReport {
    pub(crate) behavior_path_status_state: TextAreaEditableIfcBehaviorPathStatusState,
    pub(crate) behavior_path_status_recommendation:
        TextAreaEditableIfcBehaviorPathStatusRecommendation,
    pub(crate) blocked_reasons: Vec<TextAreaEditableIfcBehaviorPathReadinessBlockedReason>,
    pub(crate) ime_behavior_path_ready: bool,
    pub(crate) caret_affinity_behavior_path_ready: bool,
    pub(crate) projection_behavior_path_ready: bool,
    pub(crate) scroll_follow_behavior_path_ready: bool,
    pub(crate) ime_readiness_state: TextAreaEditableIfcBehaviorPathReadinessState,
    pub(crate) caret_affinity_readiness_state: TextAreaEditableIfcBehaviorPathReadinessState,
    pub(crate) projection_readiness_state: TextAreaEditableIfcBehaviorPathReadinessState,
    pub(crate) scroll_follow_readiness_state: TextAreaEditableIfcBehaviorPathReadinessState,
    pub(crate) caret_affinity_candidate: TextAreaEditableIfcBehaviorPathReadinessCandidate,
    pub(crate) caret_affinity_metadata_source_state:
        TextAreaEditableIfcCaretAffinityMetadataSourceState,
    pub(crate) caret_affinity_metadata_observed: bool,
    pub(crate) caret_affinity_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) caret_affinity_visual_line_count: usize,
    pub(crate) caret_affinity_caret_stop_count: usize,
    pub(crate) caret_affinity_multi_stop_line_count: usize,
    pub(crate) recommendation: TextAreaEditableIfcBehaviorPathStatusRecommendation,
}

#[allow(dead_code)]
impl TextAreaEditableIfcBehaviorPathStatus {
    pub(crate) fn readiness_report(
        &self,
        caret_affinity_metadata_source: &TextAreaEditableIfcCaretAffinityMetadataSource,
    ) -> TextAreaEditableIfcBehaviorPathReadinessReport {
        let report = self.report();
        let caret_diagnostic = caret_affinity_metadata_source.diagnostic();
        let caret_affinity_metadata_source_state = caret_affinity_metadata_source.state();
        let caret_affinity_metadata_observed = caret_affinity_metadata_source.metadata_observed();
        let caret_affinity_metadata_status = caret_diagnostic
            .map(|diagnostic| diagnostic.caret_affinity_metadata_status)
            .unwrap_or(TextAreaInlineIfcMetadataBridgeStatus::Unwired);
        let caret_affinity_visual_line_count = caret_diagnostic
            .map(|diagnostic| diagnostic.visual_line_count)
            .unwrap_or(0);
        let caret_affinity_caret_stop_count = caret_diagnostic
            .map(|diagnostic| diagnostic.caret_stop_count)
            .unwrap_or(0);
        let caret_affinity_multi_stop_line_count = caret_diagnostic
            .map(|diagnostic| diagnostic.multi_stop_line_count)
            .unwrap_or(0);
        let mut blocked_reasons = Vec::new();
        if self.state == TextAreaEditableIfcBehaviorPathStatusState::Blocked {
            blocked_reasons.push(
                TextAreaEditableIfcBehaviorPathReadinessBlockedReason::BehaviorPathStatusBlocked,
            );
        }
        if report.behavior_paths_still_not_ready() {
            blocked_reasons.push(
                TextAreaEditableIfcBehaviorPathReadinessBlockedReason::BehaviorPathsStillNotReady,
            );
        }
        if !report.ime_behavior_path_ready {
            blocked_reasons.push(
                TextAreaEditableIfcBehaviorPathReadinessBlockedReason::ImeBehaviorPathNotReady,
            );
        }
        if !report.projection_behavior_path_ready {
            blocked_reasons.push(
                TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                    ProjectionBehaviorPathNotReady,
            );
        }
        if !report.scroll_follow_behavior_path_ready {
            blocked_reasons.push(
                TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                    ScrollFollowBehaviorPathNotReady,
            );
        }
        if !report.caret_affinity_behavior_path_ready {
            blocked_reasons.push(
                TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                    CaretAffinityBehaviorPathNotReady,
            );
        }

        let caret_candidate_observed = caret_affinity_metadata_source_state
            == TextAreaEditableIfcCaretAffinityMetadataSourceState::MetadataObserved
            && caret_affinity_metadata_observed
            && caret_affinity_metadata_status
                == TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
            && caret_affinity_caret_stop_count > 0;
        if !caret_candidate_observed {
            if !caret_affinity_metadata_observed {
                blocked_reasons.push(
                    TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                        CaretAffinityMetadataSourceNotObserved,
                );
            }
            if caret_affinity_caret_stop_count == 0 {
                blocked_reasons.push(
                    TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                        CaretAffinityMetadataHasNoCandidateSlots,
                );
            }
        }

        let caret_affinity_readiness_state = if report.caret_affinity_behavior_path_ready {
            TextAreaEditableIfcBehaviorPathReadinessState::Ready
        } else if caret_candidate_observed {
            TextAreaEditableIfcBehaviorPathReadinessState::CaretAffinityCandidateObserved
        } else {
            TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
        };
        TextAreaEditableIfcBehaviorPathReadinessReport {
            behavior_path_status_state: self.state,
            behavior_path_status_recommendation: self.recommendation,
            blocked_reasons,
            ime_behavior_path_ready: report.ime_behavior_path_ready,
            caret_affinity_behavior_path_ready: report.caret_affinity_behavior_path_ready,
            projection_behavior_path_ready: report.projection_behavior_path_ready,
            scroll_follow_behavior_path_ready: report.scroll_follow_behavior_path_ready,
            ime_readiness_state: if report.ime_behavior_path_ready {
                TextAreaEditableIfcBehaviorPathReadinessState::Ready
            } else {
                TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
            },
            caret_affinity_readiness_state,
            projection_readiness_state: if report.projection_behavior_path_ready {
                TextAreaEditableIfcBehaviorPathReadinessState::Ready
            } else {
                TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
            },
            scroll_follow_readiness_state: if report.scroll_follow_behavior_path_ready {
                TextAreaEditableIfcBehaviorPathReadinessState::Ready
            } else {
                TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
            },
            caret_affinity_candidate: if caret_candidate_observed {
                TextAreaEditableIfcBehaviorPathReadinessCandidate::CaretAffinityCandidateObserved
            } else {
                TextAreaEditableIfcBehaviorPathReadinessCandidate::None
            },
            caret_affinity_metadata_source_state,
            caret_affinity_metadata_observed,
            caret_affinity_metadata_status,
            caret_affinity_visual_line_count,
            caret_affinity_caret_stop_count,
            caret_affinity_multi_stop_line_count,
            recommendation:
                TextAreaEditableIfcBehaviorPathStatusRecommendation::KeepLegacyEditableBehaviorPath,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcCaretAffinityEquivalenceAuditState {
    Blocked,
    ReadyForObservationOnly,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate {
    None,
    EquivalentCandidateObserved,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason {
    BehaviorPathReadinessBlocked,
    CaretAffinityCandidateNotObserved,
    CaretAffinityMetadataSourceMissing,
    CaretAffinityMetadataSourceNotObserved,
    CaretAffinityMetadataStatusNotObservedCaretStops,
    MissingCaretStops,
    MissingAffinitySlots,
    CaretMetadataShapeMismatch,
    PreeditCursorMetadataIncomplete,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcCaretAffinityEquivalenceAuditRecommendation {
    KeepLegacyEditableBehaviorPath,
    ObservationOnlyNoOp,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcCaretAffinityEquivalenceAuditDiagnostic {
    pub(crate) readiness_blocked_reasons:
        Vec<TextAreaEditableIfcBehaviorPathReadinessBlockedReason>,
    pub(crate) caret_affinity_readiness_state: TextAreaEditableIfcBehaviorPathReadinessState,
    pub(crate) caret_affinity_candidate: TextAreaEditableIfcBehaviorPathReadinessCandidate,
    pub(crate) caret_affinity_metadata_source_state:
        TextAreaEditableIfcCaretAffinityMetadataSourceState,
    pub(crate) caret_affinity_metadata_observed: bool,
    pub(crate) caret_affinity_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) visual_line_count: usize,
    pub(crate) caret_stop_count: usize,
    pub(crate) multi_stop_line_count: usize,
    pub(crate) preedit_cursor_count: usize,
    pub(crate) per_run_caret_diagnostics: Vec<TextAreaTextRunInlineIfcCaretAffinityDiagnostic>,
    pub(crate) equivalent_candidate: TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate,
    pub(crate) ime_readiness_state: TextAreaEditableIfcBehaviorPathReadinessState,
    pub(crate) projection_readiness_state: TextAreaEditableIfcBehaviorPathReadinessState,
    pub(crate) scroll_follow_readiness_state: TextAreaEditableIfcBehaviorPathReadinessState,
    pub(crate) ime_behavior_path_ready: bool,
    pub(crate) caret_affinity_behavior_path_ready: bool,
    pub(crate) projection_behavior_path_ready: bool,
    pub(crate) scroll_follow_behavior_path_ready: bool,
    pub(crate) render_enabled: bool,
    pub(crate) layout_enabled: bool,
    pub(crate) allows_text_area_editable_behavior_path_switch: bool,
    pub(crate) recommendation: TextAreaEditableIfcCaretAffinityEquivalenceAuditRecommendation,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcCaretAffinityEquivalenceAuditInput {
    pub(crate) readiness_blocked_reasons:
        Vec<TextAreaEditableIfcBehaviorPathReadinessBlockedReason>,
    pub(crate) caret_affinity_readiness_state: TextAreaEditableIfcBehaviorPathReadinessState,
    pub(crate) caret_affinity_candidate: TextAreaEditableIfcBehaviorPathReadinessCandidate,
    pub(crate) caret_affinity_metadata_source_state:
        TextAreaEditableIfcCaretAffinityMetadataSourceState,
    pub(crate) caret_affinity_metadata_observed: bool,
    pub(crate) caret_affinity_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) visual_line_count: usize,
    pub(crate) caret_stop_count: usize,
    pub(crate) multi_stop_line_count: usize,
    pub(crate) preedit_cursor_count: usize,
    pub(crate) per_run_caret_diagnostics: Vec<TextAreaTextRunInlineIfcCaretAffinityDiagnostic>,
    pub(crate) ime_readiness_state: TextAreaEditableIfcBehaviorPathReadinessState,
    pub(crate) projection_readiness_state: TextAreaEditableIfcBehaviorPathReadinessState,
    pub(crate) scroll_follow_readiness_state: TextAreaEditableIfcBehaviorPathReadinessState,
    pub(crate) ime_behavior_path_ready: bool,
    pub(crate) caret_affinity_behavior_path_ready: bool,
    pub(crate) projection_behavior_path_ready: bool,
    pub(crate) scroll_follow_behavior_path_ready: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcCaretAffinityEquivalenceAuditInput {
    pub(crate) fn from_behavior_status_and_caret_affinity_metadata_source(
        behavior_status: &TextAreaEditableIfcBehaviorPathStatus,
        caret_affinity_metadata_source: &TextAreaEditableIfcCaretAffinityMetadataSource,
    ) -> Self {
        let readiness = behavior_status.readiness_report(caret_affinity_metadata_source);
        let metadata_source_diagnostic = caret_affinity_metadata_source.diagnostic();
        Self {
            readiness_blocked_reasons: readiness.blocked_reasons,
            caret_affinity_readiness_state: readiness.caret_affinity_readiness_state,
            caret_affinity_candidate: readiness.caret_affinity_candidate,
            caret_affinity_metadata_source_state: caret_affinity_metadata_source.state(),
            caret_affinity_metadata_observed: caret_affinity_metadata_source.metadata_observed(),
            caret_affinity_metadata_status: metadata_source_diagnostic
                .map(|diagnostic| diagnostic.caret_affinity_metadata_status)
                .unwrap_or(TextAreaInlineIfcMetadataBridgeStatus::Unwired),
            visual_line_count: metadata_source_diagnostic
                .map(|diagnostic| diagnostic.visual_line_count)
                .unwrap_or(0),
            caret_stop_count: metadata_source_diagnostic
                .map(|diagnostic| diagnostic.caret_stop_count)
                .unwrap_or(0),
            multi_stop_line_count: metadata_source_diagnostic
                .map(|diagnostic| diagnostic.multi_stop_line_count)
                .unwrap_or(0),
            preedit_cursor_count: metadata_source_diagnostic
                .map(|diagnostic| diagnostic.preedit_cursor_count)
                .unwrap_or(0),
            per_run_caret_diagnostics: metadata_source_diagnostic
                .map(|diagnostic| diagnostic.per_run_caret_diagnostics.clone())
                .unwrap_or_default(),
            ime_readiness_state: readiness.ime_readiness_state,
            projection_readiness_state: readiness.projection_readiness_state,
            scroll_follow_readiness_state: readiness.scroll_follow_readiness_state,
            ime_behavior_path_ready: readiness.ime_behavior_path_ready,
            caret_affinity_behavior_path_ready: readiness.caret_affinity_behavior_path_ready,
            projection_behavior_path_ready: readiness.projection_behavior_path_ready,
            scroll_follow_behavior_path_ready: readiness.scroll_follow_behavior_path_ready,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcCaretAffinityEquivalenceAudit {
    state: TextAreaEditableIfcCaretAffinityEquivalenceAuditState,
    blocked_reasons: Vec<TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason>,
    input: TextAreaEditableIfcCaretAffinityEquivalenceAuditInput,
    diagnostic: TextAreaEditableIfcCaretAffinityEquivalenceAuditDiagnostic,
    recommendation: TextAreaEditableIfcCaretAffinityEquivalenceAuditRecommendation,
    render_enabled: bool,
    layout_enabled: bool,
    caret_affinity_behavior_path_ready: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcCaretAffinityEquivalenceAudit {
    pub(crate) fn evaluate(input: TextAreaEditableIfcCaretAffinityEquivalenceAuditInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if input.readiness_blocked_reasons.is_empty() {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::
                    BehaviorPathReadinessBlocked,
            );
        }
        if input.caret_affinity_candidate
            != TextAreaEditableIfcBehaviorPathReadinessCandidate::CaretAffinityCandidateObserved
            || input.caret_affinity_readiness_state
                != TextAreaEditableIfcBehaviorPathReadinessState::CaretAffinityCandidateObserved
        {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::
                    CaretAffinityCandidateNotObserved,
            );
        }
        if input.per_run_caret_diagnostics.is_empty() {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::
                    CaretAffinityMetadataSourceMissing,
            );
        }
        if input.caret_affinity_metadata_source_state
            != TextAreaEditableIfcCaretAffinityMetadataSourceState::MetadataObserved
            || !input.caret_affinity_metadata_observed
        {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::
                    CaretAffinityMetadataSourceNotObserved,
            );
        }
        if input.caret_affinity_metadata_status
            != TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
        {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::
                    CaretAffinityMetadataStatusNotObservedCaretStops,
            );
        }
        if input.visual_line_count == 0 || input.caret_stop_count == 0 {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::MissingCaretStops,
            );
        }
        if input.multi_stop_line_count == 0 {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::MissingAffinitySlots,
            );
        }

        let per_run_visual_line_count = input
            .per_run_caret_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.visual_line_count)
            .sum::<usize>();
        let per_run_caret_stop_count = input
            .per_run_caret_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.caret_stop_count)
            .sum::<usize>();
        let per_run_caret_stop_snapshot_count = input
            .per_run_caret_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.caret_stop_snapshots.len())
            .sum::<usize>();
        let per_run_multi_stop_line_count = input
            .per_run_caret_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.multi_stop_line_count)
            .sum::<usize>();
        if per_run_visual_line_count != input.visual_line_count
            || per_run_caret_stop_count != input.caret_stop_count
            || per_run_caret_stop_snapshot_count != input.caret_stop_count
            || per_run_multi_stop_line_count != input.multi_stop_line_count
        {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::
                    CaretMetadataShapeMismatch,
            );
        }

        let per_run_preedit_cursor_count = input
            .per_run_caret_diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.has_preedit_cursor && diagnostic.preedit_cursor.is_some()
            })
            .count();
        if per_run_preedit_cursor_count != input.preedit_cursor_count {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::
                    PreeditCursorMetadataIncomplete,
            );
        }

        let state = if blocked_reasons.is_empty() {
            TextAreaEditableIfcCaretAffinityEquivalenceAuditState::ReadyForObservationOnly
        } else {
            TextAreaEditableIfcCaretAffinityEquivalenceAuditState::Blocked
        };
        let recommendation = if state
            == TextAreaEditableIfcCaretAffinityEquivalenceAuditState::ReadyForObservationOnly
        {
            TextAreaEditableIfcCaretAffinityEquivalenceAuditRecommendation::ObservationOnlyNoOp
        } else {
            TextAreaEditableIfcCaretAffinityEquivalenceAuditRecommendation::
                    KeepLegacyEditableBehaviorPath
        };
        let equivalent_candidate = if state
            == TextAreaEditableIfcCaretAffinityEquivalenceAuditState::ReadyForObservationOnly
        {
            TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate::EquivalentCandidateObserved
        } else {
            TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate::None
        };
        let diagnostic = TextAreaEditableIfcCaretAffinityEquivalenceAuditDiagnostic {
            readiness_blocked_reasons: input.readiness_blocked_reasons.clone(),
            caret_affinity_readiness_state: input.caret_affinity_readiness_state,
            caret_affinity_candidate: input.caret_affinity_candidate,
            caret_affinity_metadata_source_state: input.caret_affinity_metadata_source_state,
            caret_affinity_metadata_observed: input.caret_affinity_metadata_observed,
            caret_affinity_metadata_status: input.caret_affinity_metadata_status,
            visual_line_count: input.visual_line_count,
            caret_stop_count: input.caret_stop_count,
            multi_stop_line_count: input.multi_stop_line_count,
            preedit_cursor_count: input.preedit_cursor_count,
            per_run_caret_diagnostics: input.per_run_caret_diagnostics.clone(),
            equivalent_candidate,
            ime_readiness_state: input.ime_readiness_state,
            projection_readiness_state: input.projection_readiness_state,
            scroll_follow_readiness_state: input.scroll_follow_readiness_state,
            ime_behavior_path_ready: input.ime_behavior_path_ready,
            caret_affinity_behavior_path_ready: false,
            projection_behavior_path_ready: input.projection_behavior_path_ready,
            scroll_follow_behavior_path_ready: input.scroll_follow_behavior_path_ready,
            render_enabled: false,
            layout_enabled: false,
            allows_text_area_editable_behavior_path_switch: false,
            recommendation,
        };

        Self {
            state,
            blocked_reasons,
            input,
            diagnostic,
            recommendation,
            render_enabled: false,
            layout_enabled: false,
            caret_affinity_behavior_path_ready: false,
        }
    }

    pub(crate) fn state(&self) -> TextAreaEditableIfcCaretAffinityEquivalenceAuditState {
        self.state
    }

    pub(crate) fn blocked_reasons(
        &self,
    ) -> &[TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn input(&self) -> &TextAreaEditableIfcCaretAffinityEquivalenceAuditInput {
        &self.input
    }

    pub(crate) fn diagnostic(&self) -> &TextAreaEditableIfcCaretAffinityEquivalenceAuditDiagnostic {
        &self.diagnostic
    }

    pub(crate) fn recommendation(
        &self,
    ) -> TextAreaEditableIfcCaretAffinityEquivalenceAuditRecommendation {
        self.recommendation
    }

    pub(crate) fn caret_affinity_behavior_path_ready(&self) -> bool {
        self.caret_affinity_behavior_path_ready
    }

    pub(crate) fn render_enabled(&self) -> bool {
        self.render_enabled
    }

    pub(crate) fn layout_enabled(&self) -> bool {
        self.layout_enabled
    }

    pub(crate) fn allows_text_area_editable_behavior_path_switch(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcCaretAffinityBehaviorInput {
    pub(crate) equivalent_candidate_observed: bool,
    pub(crate) caret_affinity_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) visual_line_count: usize,
    pub(crate) caret_stop_count: usize,
    pub(crate) multi_stop_line_count: usize,
    pub(crate) preedit_cursor_count: usize,
    pub(crate) per_run_caret_diagnostics: Vec<TextAreaTextRunInlineIfcCaretAffinityDiagnostic>,
}

#[allow(dead_code)]
impl TextAreaEditableIfcCaretAffinityBehaviorInput {
    pub(crate) fn from_equivalence_audit(
        audit: &TextAreaEditableIfcCaretAffinityEquivalenceAudit,
    ) -> Self {
        let diagnostic = audit.diagnostic();
        Self {
            equivalent_candidate_observed: audit.state()
                == TextAreaEditableIfcCaretAffinityEquivalenceAuditState::ReadyForObservationOnly
                && diagnostic.equivalent_candidate
                    == TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate::
                        EquivalentCandidateObserved,
            caret_affinity_metadata_status: diagnostic.caret_affinity_metadata_status,
            visual_line_count: diagnostic.visual_line_count,
            caret_stop_count: diagnostic.caret_stop_count,
            multi_stop_line_count: diagnostic.multi_stop_line_count,
            preedit_cursor_count: diagnostic.preedit_cursor_count,
            per_run_caret_diagnostics: diagnostic.per_run_caret_diagnostics.clone(),
        }
    }

    pub(crate) fn caret_affinity_behavior_path_ready(&self) -> bool {
        false
    }

    pub(crate) fn render_enabled(&self) -> bool {
        false
    }

    pub(crate) fn layout_enabled(&self) -> bool {
        false
    }

    pub(crate) fn allows_text_area_editable_behavior_path_switch(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcCaretAffinityBehaviorEvaluationState {
    Blocked,
    InputObserved,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason {
    EquivalentCandidateMissing,
    CaretAffinityMetadataStatusNotObservedCaretStops,
    MissingCaretStops,
    MissingAffinitySlots,
    CaretMetadataShapeMismatch,
    PreeditCursorMetadataIncomplete,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcCaretAffinityBehaviorEvaluationDiagnostic {
    pub(crate) equivalent_candidate_observed: bool,
    pub(crate) caret_affinity_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) visual_line_count: usize,
    pub(crate) caret_stop_count: usize,
    pub(crate) multi_stop_line_count: usize,
    pub(crate) preedit_cursor_count: usize,
    pub(crate) per_run_caret_diagnostics: Vec<TextAreaTextRunInlineIfcCaretAffinityDiagnostic>,
    pub(crate) input_observed: bool,
    pub(crate) caret_affinity_behavior_path_ready: bool,
    pub(crate) render_enabled: bool,
    pub(crate) layout_enabled: bool,
    pub(crate) allows_text_area_editable_behavior_path_switch: bool,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcCaretAffinityBehaviorEvaluation {
    state: TextAreaEditableIfcCaretAffinityBehaviorEvaluationState,
    blocked_reasons: Vec<TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason>,
    input: TextAreaEditableIfcCaretAffinityBehaviorInput,
    diagnostic: TextAreaEditableIfcCaretAffinityBehaviorEvaluationDiagnostic,
    caret_affinity_behavior_path_ready: bool,
    render_enabled: bool,
    layout_enabled: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcCaretAffinityBehaviorEvaluation {
    pub(crate) fn evaluate(input: TextAreaEditableIfcCaretAffinityBehaviorInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if !input.equivalent_candidate_observed {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::
                    EquivalentCandidateMissing,
            );
        }
        if input.caret_affinity_metadata_status
            != TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
        {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::
                    CaretAffinityMetadataStatusNotObservedCaretStops,
            );
        }
        if input.visual_line_count == 0 || input.caret_stop_count == 0 {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::MissingCaretStops,
            );
        }
        if input.multi_stop_line_count == 0 {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::
                    MissingAffinitySlots,
            );
        }

        let per_run_visual_line_count = input
            .per_run_caret_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.visual_line_count)
            .sum::<usize>();
        let per_run_caret_stop_count = input
            .per_run_caret_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.caret_stop_count)
            .sum::<usize>();
        let per_run_caret_stop_snapshot_count = input
            .per_run_caret_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.caret_stop_snapshots.len())
            .sum::<usize>();
        let per_run_multi_stop_line_count = input
            .per_run_caret_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.multi_stop_line_count)
            .sum::<usize>();
        if per_run_visual_line_count != input.visual_line_count
            || per_run_caret_stop_count != input.caret_stop_count
            || per_run_caret_stop_snapshot_count != input.caret_stop_count
            || per_run_multi_stop_line_count != input.multi_stop_line_count
        {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::
                    CaretMetadataShapeMismatch,
            );
        }

        let per_run_preedit_cursor_count = input
            .per_run_caret_diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.has_preedit_cursor && diagnostic.preedit_cursor.is_some()
            })
            .count();
        if per_run_preedit_cursor_count != input.preedit_cursor_count {
            blocked_reasons.push(
                TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::
                    PreeditCursorMetadataIncomplete,
            );
        }

        let state = if blocked_reasons.is_empty() {
            TextAreaEditableIfcCaretAffinityBehaviorEvaluationState::InputObserved
        } else {
            TextAreaEditableIfcCaretAffinityBehaviorEvaluationState::Blocked
        };
        let diagnostic = TextAreaEditableIfcCaretAffinityBehaviorEvaluationDiagnostic {
            equivalent_candidate_observed: input.equivalent_candidate_observed,
            caret_affinity_metadata_status: input.caret_affinity_metadata_status,
            visual_line_count: input.visual_line_count,
            caret_stop_count: input.caret_stop_count,
            multi_stop_line_count: input.multi_stop_line_count,
            preedit_cursor_count: input.preedit_cursor_count,
            per_run_caret_diagnostics: input.per_run_caret_diagnostics.clone(),
            input_observed: state
                == TextAreaEditableIfcCaretAffinityBehaviorEvaluationState::InputObserved,
            caret_affinity_behavior_path_ready: false,
            render_enabled: false,
            layout_enabled: false,
            allows_text_area_editable_behavior_path_switch: false,
        };

        Self {
            state,
            blocked_reasons,
            input,
            diagnostic,
            caret_affinity_behavior_path_ready: false,
            render_enabled: false,
            layout_enabled: false,
        }
    }

    pub(crate) fn state(&self) -> TextAreaEditableIfcCaretAffinityBehaviorEvaluationState {
        self.state
    }

    pub(crate) fn blocked_reasons(
        &self,
    ) -> &[TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn input(&self) -> &TextAreaEditableIfcCaretAffinityBehaviorInput {
        &self.input
    }

    pub(crate) fn diagnostic(
        &self,
    ) -> &TextAreaEditableIfcCaretAffinityBehaviorEvaluationDiagnostic {
        &self.diagnostic
    }

    pub(crate) fn caret_affinity_behavior_path_ready(&self) -> bool {
        self.caret_affinity_behavior_path_ready
    }

    pub(crate) fn render_enabled(&self) -> bool {
        self.render_enabled
    }

    pub(crate) fn layout_enabled(&self) -> bool {
        self.layout_enabled
    }

    pub(crate) fn allows_text_area_editable_behavior_path_switch(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcCaretAffinityReadOnlyLookupState {
    Blocked,
    ReadOnlyLookupObserved,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcCaretAffinityReadOnlyLookupBlockedReason {
    BehaviorEvaluationNotObserved,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcCaretAffinityReadOnlyLookup {
    pub(crate) visual_line_count: usize,
    pub(crate) caret_stop_count: usize,
    pub(crate) multi_stop_line_count: usize,
    pub(crate) preedit_cursor_count: usize,
    pub(crate) per_run_caret_diagnostics: Vec<TextAreaTextRunInlineIfcCaretAffinityDiagnostic>,
    pub(crate) caret_stop_snapshots: Vec<TextAreaEditableIfcCaretAffinityStopSnapshot>,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct TextAreaEditableIfcCaretAffinityReadOnlyBehaviorHelper<'a> {
    lookup: &'a TextAreaEditableIfcCaretAffinityReadOnlyLookup,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextAreaEditableIfcCaretAffinityPlacementNavigationSummary {
    pub(crate) visual_line_count: usize,
    pub(crate) caret_stop_count: usize,
    pub(crate) multi_stop_line_count: usize,
    pub(crate) per_run_visual_line_counts: Vec<usize>,
    pub(crate) per_run_caret_stop_counts: Vec<usize>,
    pub(crate) per_run_multi_stop_line_counts: Vec<usize>,
    pub(crate) has_affinity_slots: bool,
    pub(crate) preedit_cursor_count: usize,
    pub(crate) preedit_cursors: Vec<Option<(usize, usize)>>,
    pub(crate) caret_stop_snapshot_count: usize,
    pub(crate) run_local_char_indices_available: bool,
    pub(crate) run_local_geometry_available: bool,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcCaretAffinityStopGeometrySummary {
    pub(crate) run_index: usize,
    pub(crate) visual_line_index: usize,
    pub(crate) stop_index: usize,
    pub(crate) local_char: usize,
    pub(crate) local_x: f32,
    pub(crate) local_y_top: f32,
    pub(crate) height: f32,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcCaretAffinityLocalCharCandidate {
    pub(crate) run_index: usize,
    pub(crate) local_char: usize,
    pub(crate) affinity: super::caret_map::CaretAffinity,
    pub(crate) visual_line_index: usize,
    pub(crate) stop_index: usize,
    pub(crate) local_x: f32,
    pub(crate) local_y_top: f32,
    pub(crate) height: f32,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcCaretAffinityPlacementCandidate {
    pub(crate) run_index: usize,
    pub(crate) local_char: usize,
    pub(crate) affinity: super::caret_map::CaretAffinity,
    pub(crate) local_x: f32,
    pub(crate) local_y_top: f32,
    pub(crate) height: f32,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct TextAreaEditableIfcCaretAffinityPlacementReadOnlyAdapter<'a> {
    lookup: &'a TextAreaEditableIfcCaretAffinityReadOnlyLookup,
}

#[allow(dead_code)]
fn collect_text_area_editable_ifc_caret_affinity_stop_snapshots(
    diagnostics: &[TextAreaTextRunInlineIfcCaretAffinityDiagnostic],
) -> Vec<TextAreaEditableIfcCaretAffinityStopSnapshot> {
    diagnostics
        .iter()
        .enumerate()
        .flat_map(|(run_index, diagnostic)| {
            diagnostic
                .caret_stop_snapshots
                .iter()
                .cloned()
                .map(move |mut snapshot| {
                    snapshot.run_index = run_index;
                    snapshot
                })
        })
        .collect()
}

#[allow(dead_code)]
impl TextAreaEditableIfcCaretAffinityReadOnlyLookup {
    pub(crate) fn behavior_helper(
        &self,
    ) -> TextAreaEditableIfcCaretAffinityReadOnlyBehaviorHelper<'_> {
        TextAreaEditableIfcCaretAffinityReadOnlyBehaviorHelper { lookup: self }
    }
}

#[allow(dead_code)]
impl TextAreaEditableIfcCaretAffinityReadOnlyBehaviorHelper<'_> {
    pub(crate) fn line_summary(&self) -> (usize, usize) {
        (
            self.lookup.visual_line_count,
            self.lookup.multi_stop_line_count,
        )
    }

    pub(crate) fn stop_summary(&self) -> (usize, usize) {
        (
            self.lookup.caret_stop_count,
            self.lookup.multi_stop_line_count,
        )
    }

    pub(crate) fn preedit_cursor_metadata(&self) -> (usize, Vec<Option<(usize, usize)>>) {
        (
            self.lookup.preedit_cursor_count,
            self.lookup
                .per_run_caret_diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.has_preedit_cursor)
                .map(|diagnostic| diagnostic.preedit_cursor)
                .collect(),
        )
    }

    pub(crate) fn per_run_caret_diagnostics(
        &self,
    ) -> &[TextAreaTextRunInlineIfcCaretAffinityDiagnostic] {
        &self.lookup.per_run_caret_diagnostics
    }

    pub(crate) fn caret_stop_snapshots(&self) -> &[TextAreaEditableIfcCaretAffinityStopSnapshot] {
        &self.lookup.caret_stop_snapshots
    }

    pub(crate) fn placement_read_only_adapter(
        &self,
    ) -> TextAreaEditableIfcCaretAffinityPlacementReadOnlyAdapter<'_> {
        TextAreaEditableIfcCaretAffinityPlacementReadOnlyAdapter {
            lookup: self.lookup,
        }
    }

    pub(crate) fn stop_geometry_summary(
        &self,
        run_index: usize,
        visual_line_index: usize,
        stop_index: usize,
    ) -> Option<TextAreaEditableIfcCaretAffinityStopGeometrySummary> {
        self.lookup
            .caret_stop_snapshots
            .iter()
            .find(|snapshot| {
                snapshot.run_index == run_index
                    && snapshot.visual_line_index == visual_line_index
                    && snapshot.stop_index == stop_index
            })
            .map(
                |snapshot| TextAreaEditableIfcCaretAffinityStopGeometrySummary {
                    run_index: snapshot.run_index,
                    visual_line_index: snapshot.visual_line_index,
                    stop_index: snapshot.stop_index,
                    local_char: snapshot.local_char,
                    local_x: snapshot.local_x,
                    local_y_top: snapshot.local_y_top,
                    height: snapshot.height,
                },
            )
    }

    pub(crate) fn local_char_candidates(
        &self,
        run_index: usize,
        local_char: usize,
    ) -> Vec<TextAreaEditableIfcCaretAffinityLocalCharCandidate> {
        let mut snapshots = self
            .lookup
            .caret_stop_snapshots
            .iter()
            .filter(|snapshot| snapshot.run_index == run_index && snapshot.local_char == local_char)
            .collect::<Vec<_>>();
        snapshots.sort_by_key(|snapshot| (snapshot.visual_line_index, snapshot.stop_index));

        snapshots
            .iter()
            .enumerate()
            .map(|(candidate_index, snapshot)| {
                let affinity = if snapshots.len() > 1 && candidate_index == 0 {
                    super::caret_map::CaretAffinity::Upstream
                } else {
                    super::caret_map::CaretAffinity::Downstream
                };
                TextAreaEditableIfcCaretAffinityLocalCharCandidate {
                    run_index: snapshot.run_index,
                    local_char: snapshot.local_char,
                    affinity,
                    visual_line_index: snapshot.visual_line_index,
                    stop_index: snapshot.stop_index,
                    local_x: snapshot.local_x,
                    local_y_top: snapshot.local_y_top,
                    height: snapshot.height,
                }
            })
            .collect()
    }

    pub(crate) fn local_char_candidate_with_affinity(
        &self,
        run_index: usize,
        local_char: usize,
        affinity: super::caret_map::CaretAffinity,
    ) -> Option<TextAreaEditableIfcCaretAffinityLocalCharCandidate> {
        self.local_char_candidates(run_index, local_char)
            .into_iter()
            .find(|candidate| candidate.affinity == affinity)
    }

    pub(crate) fn placement_navigation_summary(
        &self,
    ) -> TextAreaEditableIfcCaretAffinityPlacementNavigationSummary {
        let has_complete_stop_snapshot =
            self.lookup.caret_stop_snapshots.len() == self.lookup.caret_stop_count;
        TextAreaEditableIfcCaretAffinityPlacementNavigationSummary {
            visual_line_count: self.lookup.visual_line_count,
            caret_stop_count: self.lookup.caret_stop_count,
            multi_stop_line_count: self.lookup.multi_stop_line_count,
            per_run_visual_line_counts: self
                .lookup
                .per_run_caret_diagnostics
                .iter()
                .map(|diagnostic| diagnostic.visual_line_count)
                .collect(),
            per_run_caret_stop_counts: self
                .lookup
                .per_run_caret_diagnostics
                .iter()
                .map(|diagnostic| diagnostic.caret_stop_count)
                .collect(),
            per_run_multi_stop_line_counts: self
                .lookup
                .per_run_caret_diagnostics
                .iter()
                .map(|diagnostic| diagnostic.multi_stop_line_count)
                .collect(),
            has_affinity_slots: self.lookup.multi_stop_line_count > 0,
            preedit_cursor_count: self.lookup.preedit_cursor_count,
            preedit_cursors: self
                .lookup
                .per_run_caret_diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.has_preedit_cursor)
                .map(|diagnostic| diagnostic.preedit_cursor)
                .collect(),
            caret_stop_snapshot_count: self.lookup.caret_stop_snapshots.len(),
            run_local_char_indices_available: has_complete_stop_snapshot,
            run_local_geometry_available: has_complete_stop_snapshot,
        }
    }

    pub(crate) fn caret_affinity_behavior_path_ready(&self) -> bool {
        false
    }

    pub(crate) fn render_enabled(&self) -> bool {
        false
    }

    pub(crate) fn layout_enabled(&self) -> bool {
        false
    }

    pub(crate) fn allows_text_area_editable_behavior_path_switch(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
impl TextAreaEditableIfcCaretAffinityPlacementReadOnlyAdapter<'_> {
    pub(crate) fn local_char_candidate_with_affinity(
        &self,
        run_index: usize,
        local_char: usize,
        affinity: super::caret_map::CaretAffinity,
    ) -> Option<TextAreaEditableIfcCaretAffinityPlacementCandidate> {
        self.lookup
            .behavior_helper()
            .local_char_candidate_with_affinity(run_index, local_char, affinity)
            .map(
                |candidate| TextAreaEditableIfcCaretAffinityPlacementCandidate {
                    run_index: candidate.run_index,
                    local_char: candidate.local_char,
                    affinity: candidate.affinity,
                    local_x: candidate.local_x,
                    local_y_top: candidate.local_y_top,
                    height: candidate.height,
                },
            )
    }

    pub(crate) fn local_char_to_run_local_position_with_affinity(
        &self,
        run_index: usize,
        local_char: usize,
        affinity: super::caret_map::CaretAffinity,
    ) -> Option<(f32, f32, f32)> {
        self.local_char_candidate_with_affinity(run_index, local_char, affinity)
            .map(|candidate| (candidate.local_x, candidate.local_y_top, candidate.height))
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcCaretAffinityReadOnlyLookupAdapter {
    state: TextAreaEditableIfcCaretAffinityReadOnlyLookupState,
    blocked_reasons: Vec<TextAreaEditableIfcCaretAffinityReadOnlyLookupBlockedReason>,
    lookup: Option<TextAreaEditableIfcCaretAffinityReadOnlyLookup>,
    render_enabled: bool,
    layout_enabled: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcCaretAffinityReadOnlyLookupAdapter {
    pub(crate) fn from_behavior_evaluation(
        evaluation: &TextAreaEditableIfcCaretAffinityBehaviorEvaluation,
    ) -> Self {
        let observed = evaluation.state()
            == TextAreaEditableIfcCaretAffinityBehaviorEvaluationState::InputObserved
            && evaluation.diagnostic().input_observed;
        let (state, blocked_reasons, lookup) = if observed {
            let diagnostic = evaluation.diagnostic();
            let caret_stop_snapshots = collect_text_area_editable_ifc_caret_affinity_stop_snapshots(
                &diagnostic.per_run_caret_diagnostics,
            );
            (
                TextAreaEditableIfcCaretAffinityReadOnlyLookupState::ReadOnlyLookupObserved,
                Vec::new(),
                Some(TextAreaEditableIfcCaretAffinityReadOnlyLookup {
                    visual_line_count: diagnostic.visual_line_count,
                    caret_stop_count: diagnostic.caret_stop_count,
                    multi_stop_line_count: diagnostic.multi_stop_line_count,
                    preedit_cursor_count: diagnostic.preedit_cursor_count,
                    per_run_caret_diagnostics: diagnostic.per_run_caret_diagnostics.clone(),
                    caret_stop_snapshots,
                }),
            )
        } else {
            (
                TextAreaEditableIfcCaretAffinityReadOnlyLookupState::Blocked,
                vec![
                    TextAreaEditableIfcCaretAffinityReadOnlyLookupBlockedReason::
                        BehaviorEvaluationNotObserved,
                ],
                None,
            )
        };

        Self {
            state,
            blocked_reasons,
            lookup,
            render_enabled: false,
            layout_enabled: false,
        }
    }

    pub(crate) fn state(&self) -> TextAreaEditableIfcCaretAffinityReadOnlyLookupState {
        self.state
    }

    pub(crate) fn blocked_reasons(
        &self,
    ) -> &[TextAreaEditableIfcCaretAffinityReadOnlyLookupBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn lookup(&self) -> Option<&TextAreaEditableIfcCaretAffinityReadOnlyLookup> {
        self.lookup.as_ref()
    }

    pub(crate) fn behavior_helper(
        &self,
    ) -> Option<TextAreaEditableIfcCaretAffinityReadOnlyBehaviorHelper<'_>> {
        self.lookup
            .as_ref()
            .map(TextAreaEditableIfcCaretAffinityReadOnlyLookup::behavior_helper)
    }

    pub(crate) fn placement_read_only_adapter(
        &self,
    ) -> Option<TextAreaEditableIfcCaretAffinityPlacementReadOnlyAdapter<'_>> {
        self.lookup
            .as_ref()
            .map(|lookup| TextAreaEditableIfcCaretAffinityPlacementReadOnlyAdapter { lookup })
    }

    pub(crate) fn caret_affinity_behavior_path_ready(&self) -> bool {
        false
    }

    pub(crate) fn render_enabled(&self) -> bool {
        self.render_enabled
    }

    pub(crate) fn layout_enabled(&self) -> bool {
        self.layout_enabled
    }

    pub(crate) fn allows_text_area_editable_behavior_path_switch(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcBehaviorEquivalenceAuditState {
    Blocked,
    ReadyForObservationOnly,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcBehaviorEquivalenceAuditCandidate {
    None,
    EquivalentCandidateObserved,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcBehaviorEquivalenceAuditRecommendation {
    KeepLegacyEditableBehaviorPath,
    ObservationOnlyNoOp,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason {
    BehaviorPathReadinessMissing,
    ProjectionPrewireMissing,
    ProjectionMetadataSourceNotObserved,
    ProjectionMetadataStatusNotObserved,
    MissingProjectionMetadataDiagnostic,
    MissingProjectionSegments,
    ProjectionMetadataShapeMismatch,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcProjectionEquivalenceAuditDiagnostic {
    pub(crate) readiness_blocked_reasons:
        Vec<TextAreaEditableIfcBehaviorPathReadinessBlockedReason>,
    pub(crate) projection_prewire_state: TextAreaEditableIfcProjectionBehaviorPathPrewireState,
    pub(crate) projection_diagnostic_prewired: bool,
    pub(crate) projection_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) run_count: usize,
    pub(crate) char_range_count: usize,
    pub(crate) char_span: usize,
    pub(crate) effective_content_len: usize,
    pub(crate) inline_preedit_run_count: usize,
    pub(crate) preedit_run_count: usize,
    pub(crate) projection_segment_count: usize,
    pub(crate) per_run_projection_diagnostics: Vec<TextAreaTextRunInlineIfcProjectionDiagnostic>,
    pub(crate) equivalent_candidate: TextAreaEditableIfcBehaviorEquivalenceAuditCandidate,
    pub(crate) projection_behavior_path_ready: bool,
    pub(crate) render_enabled: bool,
    pub(crate) layout_enabled: bool,
    pub(crate) allows_text_area_editable_behavior_path_switch: bool,
    pub(crate) recommendation: TextAreaEditableIfcBehaviorEquivalenceAuditRecommendation,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcProjectionEquivalenceAuditInput {
    pub(crate) readiness_blocked_reasons:
        Vec<TextAreaEditableIfcBehaviorPathReadinessBlockedReason>,
    pub(crate) projection_prewire_state: TextAreaEditableIfcProjectionBehaviorPathPrewireState,
    pub(crate) projection_diagnostic_prewired: bool,
    pub(crate) projection_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) projection_metadata_diagnostic:
        Option<TextAreaEditableIfcProjectionBehaviorPathPrewireDiagnostic>,
    pub(crate) projection_behavior_path_ready: bool,
    pub(crate) prewire_render_enabled: bool,
    pub(crate) prewire_layout_enabled: bool,
    pub(crate) prewire_allows_text_area_editable_behavior_path_switch: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcProjectionEquivalenceAuditInput {
    pub(crate) fn from_behavior_status_and_projection_prewire(
        behavior_status: &TextAreaEditableIfcBehaviorPathStatus,
        projection_prewire: &TextAreaEditableIfcProjectionBehaviorPathPrewire,
    ) -> Self {
        Self {
            readiness_blocked_reasons: behavior_status
                .behavior_path_switch_blocked_reasons()
                .into_iter()
                .map(|reason| match reason {
                    TextAreaEditableIfcBehaviorPathStatusBlockedReason::StatusObservationOnly => {
                        TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                            BehaviorPathStatusBlocked
                    }
                    TextAreaEditableIfcBehaviorPathStatusBlockedReason::BehaviorPathsStillNotReady => {
                        TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                            BehaviorPathsStillNotReady
                    }
                    _ => TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                        BehaviorPathStatusBlocked,
                })
                .collect(),
            projection_prewire_state: projection_prewire.state(),
            projection_diagnostic_prewired: projection_prewire.diagnostic_prewired(),
            projection_metadata_status: projection_prewire
                .diagnostic()
                .map(|diagnostic| diagnostic.projection_metadata_status)
                .unwrap_or(TextAreaInlineIfcMetadataBridgeStatus::Unwired),
            projection_metadata_diagnostic: projection_prewire.diagnostic().cloned(),
            projection_behavior_path_ready: projection_prewire.projection_behavior_path_ready(),
            prewire_render_enabled: projection_prewire.render_enabled(),
            prewire_layout_enabled: projection_prewire.layout_enabled(),
            prewire_allows_text_area_editable_behavior_path_switch: projection_prewire
                .allows_text_area_editable_behavior_path_switch(),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcProjectionEquivalenceAudit {
    state: TextAreaEditableIfcBehaviorEquivalenceAuditState,
    blocked_reasons: Vec<TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason>,
    input: TextAreaEditableIfcProjectionEquivalenceAuditInput,
    diagnostic: TextAreaEditableIfcProjectionEquivalenceAuditDiagnostic,
    recommendation: TextAreaEditableIfcBehaviorEquivalenceAuditRecommendation,
    render_enabled: bool,
    layout_enabled: bool,
    projection_behavior_path_ready: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcProjectionEquivalenceAudit {
    pub(crate) fn evaluate(input: TextAreaEditableIfcProjectionEquivalenceAuditInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if input.readiness_blocked_reasons.is_empty() {
            blocked_reasons.push(
                TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
                    BehaviorPathReadinessMissing,
            );
        }
        if input.projection_metadata_diagnostic.is_none() {
            blocked_reasons.push(
                TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
                    MissingProjectionMetadataDiagnostic,
            );
        }
        if input.projection_prewire_state
            != TextAreaEditableIfcProjectionBehaviorPathPrewireState::DiagnosticPrewired
        {
            blocked_reasons.push(
                TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
                    ProjectionMetadataSourceNotObserved,
            );
        }
        if !input.projection_diagnostic_prewired {
            blocked_reasons.push(
                TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
                    ProjectionPrewireMissing,
            );
        }
        if input.projection_metadata_status != TextAreaInlineIfcMetadataBridgeStatus::Observed {
            blocked_reasons.push(
                TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
                    ProjectionMetadataStatusNotObserved,
            );
        }

        let diagnostic_source = input.projection_metadata_diagnostic.clone();
        let diagnostic_values = diagnostic_source.unwrap_or(
            TextAreaEditableIfcProjectionBehaviorPathPrewireDiagnostic {
                projection_metadata_status: input.projection_metadata_status,
                run_count: 0,
                char_range_count: 0,
                char_span: 0,
                effective_content_len: 0,
                inline_preedit_run_count: 0,
                preedit_run_count: 0,
                projection_segment_count: 0,
                per_run_projection_diagnostics: Vec::new(),
            },
        );
        if diagnostic_values.projection_segment_count == 0 {
            blocked_reasons.push(
                TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
                    MissingProjectionSegments,
            );
        }
        let per_run_char_range_count = diagnostic_values
            .per_run_projection_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.char_range.start <= diagnostic.char_range.end)
            .count();
        let per_run_char_span = diagnostic_values
            .per_run_projection_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.char_span)
            .sum::<usize>();
        let per_run_effective_content_len = diagnostic_values
            .per_run_projection_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.effective_content_len)
            .sum::<usize>();
        let per_run_inline_preedit_count = diagnostic_values
            .per_run_projection_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.has_inline_preedit)
            .count();
        let per_run_preedit_count = diagnostic_values
            .per_run_projection_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.is_preedit_run)
            .count();
        let per_run_projection_segment_count = diagnostic_values
            .per_run_projection_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.projection_segment_count)
            .sum::<usize>();
        if diagnostic_values.run_count != diagnostic_values.per_run_projection_diagnostics.len()
            || diagnostic_values.char_range_count != per_run_char_range_count
            || diagnostic_values.char_span != per_run_char_span
            || diagnostic_values.effective_content_len != per_run_effective_content_len
            || diagnostic_values.inline_preedit_run_count != per_run_inline_preedit_count
            || diagnostic_values.preedit_run_count != per_run_preedit_count
            || diagnostic_values.projection_segment_count != per_run_projection_segment_count
        {
            blocked_reasons.push(
                TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
                    ProjectionMetadataShapeMismatch,
            );
        }

        let state = if blocked_reasons.is_empty() {
            TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly
        } else {
            TextAreaEditableIfcBehaviorEquivalenceAuditState::Blocked
        };
        let recommendation =
            if state == TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly {
                TextAreaEditableIfcBehaviorEquivalenceAuditRecommendation::ObservationOnlyNoOp
            } else {
                TextAreaEditableIfcBehaviorEquivalenceAuditRecommendation::
                    KeepLegacyEditableBehaviorPath
            };
        let equivalent_candidate =
            if state == TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly {
                TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::EquivalentCandidateObserved
            } else {
                TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::None
            };
        let diagnostic = TextAreaEditableIfcProjectionEquivalenceAuditDiagnostic {
            readiness_blocked_reasons: input.readiness_blocked_reasons.clone(),
            projection_prewire_state: input.projection_prewire_state,
            projection_diagnostic_prewired: input.projection_diagnostic_prewired,
            projection_metadata_status: input.projection_metadata_status,
            run_count: diagnostic_values.run_count,
            char_range_count: diagnostic_values.char_range_count,
            char_span: diagnostic_values.char_span,
            effective_content_len: diagnostic_values.effective_content_len,
            inline_preedit_run_count: diagnostic_values.inline_preedit_run_count,
            preedit_run_count: diagnostic_values.preedit_run_count,
            projection_segment_count: diagnostic_values.projection_segment_count,
            per_run_projection_diagnostics: diagnostic_values.per_run_projection_diagnostics,
            equivalent_candidate,
            projection_behavior_path_ready: false,
            render_enabled: false,
            layout_enabled: false,
            allows_text_area_editable_behavior_path_switch: false,
            recommendation,
        };

        Self {
            state,
            blocked_reasons,
            input,
            diagnostic,
            recommendation,
            render_enabled: false,
            layout_enabled: false,
            projection_behavior_path_ready: false,
        }
    }

    pub(crate) fn state(&self) -> TextAreaEditableIfcBehaviorEquivalenceAuditState {
        self.state
    }

    pub(crate) fn blocked_reasons(
        &self,
    ) -> &[TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn input(&self) -> &TextAreaEditableIfcProjectionEquivalenceAuditInput {
        &self.input
    }

    pub(crate) fn diagnostic(&self) -> &TextAreaEditableIfcProjectionEquivalenceAuditDiagnostic {
        &self.diagnostic
    }

    pub(crate) fn recommendation(
        &self,
    ) -> TextAreaEditableIfcBehaviorEquivalenceAuditRecommendation {
        self.recommendation
    }

    pub(crate) fn projection_behavior_path_ready(&self) -> bool {
        self.projection_behavior_path_ready
    }

    pub(crate) fn render_enabled(&self) -> bool {
        self.render_enabled
    }

    pub(crate) fn layout_enabled(&self) -> bool {
        self.layout_enabled
    }

    pub(crate) fn allows_text_area_editable_behavior_path_switch(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason {
    BehaviorPathReadinessMissing,
    ScrollFollowPrewireMissing,
    ScrollFollowMetadataSourceNotObserved,
    ScrollFollowMetadataStatusNotObserved,
    MissingScrollFollowMetadataDiagnostic,
    MissingScrollSource,
    ScrollFollowMetadataShapeMismatch,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcScrollFollowEquivalenceAuditDiagnostic {
    pub(crate) readiness_blocked_reasons:
        Vec<TextAreaEditableIfcBehaviorPathReadinessBlockedReason>,
    pub(crate) scroll_follow_prewire_state: TextAreaEditableIfcScrollFollowBehaviorPathPrewireState,
    pub(crate) scroll_follow_diagnostic_prewired: bool,
    pub(crate) scroll_follow_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) run_count: usize,
    pub(crate) layout_size_count: usize,
    pub(crate) char_span: usize,
    pub(crate) effective_content_len: usize,
    pub(crate) visual_line_count: usize,
    pub(crate) caret_stop_count: usize,
    pub(crate) per_run_scroll_follow_diagnostics:
        Vec<TextAreaTextRunInlineIfcScrollFollowDiagnostic>,
    pub(crate) equivalent_candidate: TextAreaEditableIfcBehaviorEquivalenceAuditCandidate,
    pub(crate) scroll_follow_behavior_path_ready: bool,
    pub(crate) render_enabled: bool,
    pub(crate) layout_enabled: bool,
    pub(crate) allows_text_area_editable_behavior_path_switch: bool,
    pub(crate) recommendation: TextAreaEditableIfcBehaviorEquivalenceAuditRecommendation,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcScrollFollowEquivalenceAuditInput {
    pub(crate) readiness_blocked_reasons:
        Vec<TextAreaEditableIfcBehaviorPathReadinessBlockedReason>,
    pub(crate) scroll_follow_prewire_state: TextAreaEditableIfcScrollFollowBehaviorPathPrewireState,
    pub(crate) scroll_follow_diagnostic_prewired: bool,
    pub(crate) scroll_follow_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) scroll_follow_metadata_diagnostic:
        Option<TextAreaEditableIfcScrollFollowBehaviorPathPrewireDiagnostic>,
    pub(crate) scroll_follow_behavior_path_ready: bool,
    pub(crate) prewire_render_enabled: bool,
    pub(crate) prewire_layout_enabled: bool,
    pub(crate) prewire_allows_text_area_editable_behavior_path_switch: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcScrollFollowEquivalenceAuditInput {
    pub(crate) fn from_behavior_status_and_scroll_follow_prewire(
        behavior_status: &TextAreaEditableIfcBehaviorPathStatus,
        scroll_follow_prewire: &TextAreaEditableIfcScrollFollowBehaviorPathPrewire,
    ) -> Self {
        Self {
            readiness_blocked_reasons: behavior_status
                .behavior_path_switch_blocked_reasons()
                .into_iter()
                .map(|reason| match reason {
                    TextAreaEditableIfcBehaviorPathStatusBlockedReason::StatusObservationOnly => {
                        TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                            BehaviorPathStatusBlocked
                    }
                    TextAreaEditableIfcBehaviorPathStatusBlockedReason::BehaviorPathsStillNotReady => {
                        TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                            BehaviorPathsStillNotReady
                    }
                    _ => TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                        BehaviorPathStatusBlocked,
                })
                .collect(),
            scroll_follow_prewire_state: scroll_follow_prewire.state(),
            scroll_follow_diagnostic_prewired: scroll_follow_prewire.diagnostic_prewired(),
            scroll_follow_metadata_status: scroll_follow_prewire
                .diagnostic()
                .map(|diagnostic| diagnostic.scroll_follow_metadata_status)
                .unwrap_or(TextAreaInlineIfcMetadataBridgeStatus::Unwired),
            scroll_follow_metadata_diagnostic: scroll_follow_prewire.diagnostic().cloned(),
            scroll_follow_behavior_path_ready: scroll_follow_prewire
                .scroll_follow_behavior_path_ready(),
            prewire_render_enabled: scroll_follow_prewire.render_enabled(),
            prewire_layout_enabled: scroll_follow_prewire.layout_enabled(),
            prewire_allows_text_area_editable_behavior_path_switch: scroll_follow_prewire
                .allows_text_area_editable_behavior_path_switch(),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcScrollFollowEquivalenceAudit {
    state: TextAreaEditableIfcBehaviorEquivalenceAuditState,
    blocked_reasons: Vec<TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason>,
    input: TextAreaEditableIfcScrollFollowEquivalenceAuditInput,
    diagnostic: TextAreaEditableIfcScrollFollowEquivalenceAuditDiagnostic,
    recommendation: TextAreaEditableIfcBehaviorEquivalenceAuditRecommendation,
    render_enabled: bool,
    layout_enabled: bool,
    scroll_follow_behavior_path_ready: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcScrollFollowEquivalenceAudit {
    pub(crate) fn evaluate(input: TextAreaEditableIfcScrollFollowEquivalenceAuditInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if input.readiness_blocked_reasons.is_empty() {
            blocked_reasons.push(
                TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason::
                    BehaviorPathReadinessMissing,
            );
        }
        if input.scroll_follow_metadata_diagnostic.is_none() {
            blocked_reasons.push(
                TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason::
                    MissingScrollFollowMetadataDiagnostic,
            );
        }
        if input.scroll_follow_prewire_state
            != TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::DiagnosticPrewired
        {
            blocked_reasons.push(
                TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason::
                    ScrollFollowMetadataSourceNotObserved,
            );
        }
        if !input.scroll_follow_diagnostic_prewired {
            blocked_reasons.push(
                TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason::
                    ScrollFollowPrewireMissing,
            );
        }
        if input.scroll_follow_metadata_status != TextAreaInlineIfcMetadataBridgeStatus::Observed {
            blocked_reasons.push(
                TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason::
                    ScrollFollowMetadataStatusNotObserved,
            );
        }

        let diagnostic_values = input.scroll_follow_metadata_diagnostic.clone().unwrap_or(
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireDiagnostic {
                scroll_follow_metadata_status: input.scroll_follow_metadata_status,
                run_count: 0,
                layout_size_count: 0,
                char_span: 0,
                effective_content_len: 0,
                visual_line_count: 0,
                caret_stop_count: 0,
                per_run_scroll_follow_diagnostics: Vec::new(),
            },
        );
        if diagnostic_values.layout_size_count == 0
            || diagnostic_values.visual_line_count == 0
            || diagnostic_values.caret_stop_count == 0
        {
            blocked_reasons.push(
                TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason::MissingScrollSource,
            );
        }
        let per_run_layout_size_count = diagnostic_values
            .per_run_scroll_follow_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.layout_size[0] > 0.0 && diagnostic.layout_size[1] > 0.0)
            .count();
        let per_run_char_span = diagnostic_values
            .per_run_scroll_follow_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.char_span)
            .sum::<usize>();
        let per_run_effective_content_len = diagnostic_values
            .per_run_scroll_follow_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.effective_content_len)
            .sum::<usize>();
        let per_run_visual_line_count = diagnostic_values
            .per_run_scroll_follow_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.visual_line_count)
            .sum::<usize>();
        let per_run_caret_stop_count = diagnostic_values
            .per_run_scroll_follow_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.caret_stop_count)
            .sum::<usize>();
        if diagnostic_values.run_count != diagnostic_values.per_run_scroll_follow_diagnostics.len()
            || diagnostic_values.layout_size_count != per_run_layout_size_count
            || diagnostic_values.char_span != per_run_char_span
            || diagnostic_values.effective_content_len != per_run_effective_content_len
            || diagnostic_values.visual_line_count != per_run_visual_line_count
            || diagnostic_values.caret_stop_count != per_run_caret_stop_count
        {
            blocked_reasons.push(
                TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason::
                    ScrollFollowMetadataShapeMismatch,
            );
        }

        let state = if blocked_reasons.is_empty() {
            TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly
        } else {
            TextAreaEditableIfcBehaviorEquivalenceAuditState::Blocked
        };
        let recommendation =
            if state == TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly {
                TextAreaEditableIfcBehaviorEquivalenceAuditRecommendation::ObservationOnlyNoOp
            } else {
                TextAreaEditableIfcBehaviorEquivalenceAuditRecommendation::
                    KeepLegacyEditableBehaviorPath
            };
        let equivalent_candidate =
            if state == TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly {
                TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::EquivalentCandidateObserved
            } else {
                TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::None
            };
        let diagnostic = TextAreaEditableIfcScrollFollowEquivalenceAuditDiagnostic {
            readiness_blocked_reasons: input.readiness_blocked_reasons.clone(),
            scroll_follow_prewire_state: input.scroll_follow_prewire_state,
            scroll_follow_diagnostic_prewired: input.scroll_follow_diagnostic_prewired,
            scroll_follow_metadata_status: input.scroll_follow_metadata_status,
            run_count: diagnostic_values.run_count,
            layout_size_count: diagnostic_values.layout_size_count,
            char_span: diagnostic_values.char_span,
            effective_content_len: diagnostic_values.effective_content_len,
            visual_line_count: diagnostic_values.visual_line_count,
            caret_stop_count: diagnostic_values.caret_stop_count,
            per_run_scroll_follow_diagnostics: diagnostic_values.per_run_scroll_follow_diagnostics,
            equivalent_candidate,
            scroll_follow_behavior_path_ready: false,
            render_enabled: false,
            layout_enabled: false,
            allows_text_area_editable_behavior_path_switch: false,
            recommendation,
        };

        Self {
            state,
            blocked_reasons,
            input,
            diagnostic,
            recommendation,
            render_enabled: false,
            layout_enabled: false,
            scroll_follow_behavior_path_ready: false,
        }
    }

    pub(crate) fn state(&self) -> TextAreaEditableIfcBehaviorEquivalenceAuditState {
        self.state
    }

    pub(crate) fn blocked_reasons(
        &self,
    ) -> &[TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn input(&self) -> &TextAreaEditableIfcScrollFollowEquivalenceAuditInput {
        &self.input
    }

    pub(crate) fn diagnostic(&self) -> &TextAreaEditableIfcScrollFollowEquivalenceAuditDiagnostic {
        &self.diagnostic
    }

    pub(crate) fn recommendation(
        &self,
    ) -> TextAreaEditableIfcBehaviorEquivalenceAuditRecommendation {
        self.recommendation
    }

    pub(crate) fn scroll_follow_behavior_path_ready(&self) -> bool {
        self.scroll_follow_behavior_path_ready
    }

    pub(crate) fn render_enabled(&self) -> bool {
        self.render_enabled
    }

    pub(crate) fn layout_enabled(&self) -> bool {
        self.layout_enabled
    }

    pub(crate) fn allows_text_area_editable_behavior_path_switch(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEditableIfcImeEquivalenceAuditBlockedReason {
    BehaviorPathReadinessMissing,
    ImePrewireMissing,
    ImeMetadataSourceNotObserved,
    ImeMetadataStatusNotObserved,
    MissingImeMetadataDiagnostic,
    NoPreeditCase,
    PreeditMetadataShapeMismatch,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcImeEquivalenceAuditDiagnostic {
    pub(crate) readiness_blocked_reasons:
        Vec<TextAreaEditableIfcBehaviorPathReadinessBlockedReason>,
    pub(crate) ime_prewire_state: TextAreaEditableIfcImeBehaviorPathPrewireState,
    pub(crate) ime_diagnostic_prewired: bool,
    pub(crate) ime_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) run_count: usize,
    pub(crate) has_inline_preedit: bool,
    pub(crate) has_preedit_run: bool,
    pub(crate) preedit_cursor_count: usize,
    pub(crate) preedit_cursors: Vec<(usize, usize)>,
    pub(crate) effective_content_len: usize,
    pub(crate) equivalent_candidate: TextAreaEditableIfcBehaviorEquivalenceAuditCandidate,
    pub(crate) ime_behavior_path_ready: bool,
    pub(crate) render_enabled: bool,
    pub(crate) layout_enabled: bool,
    pub(crate) allows_text_area_editable_behavior_path_switch: bool,
    pub(crate) recommendation: TextAreaEditableIfcBehaviorEquivalenceAuditRecommendation,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcImeEquivalenceAuditInput {
    pub(crate) readiness_blocked_reasons:
        Vec<TextAreaEditableIfcBehaviorPathReadinessBlockedReason>,
    pub(crate) ime_prewire_state: TextAreaEditableIfcImeBehaviorPathPrewireState,
    pub(crate) ime_diagnostic_prewired: bool,
    pub(crate) ime_metadata_status: TextAreaInlineIfcMetadataBridgeStatus,
    pub(crate) ime_metadata_diagnostic: Option<TextAreaEditableIfcImeBehaviorPathPrewireDiagnostic>,
    pub(crate) ime_behavior_path_ready: bool,
    pub(crate) prewire_render_enabled: bool,
    pub(crate) prewire_layout_enabled: bool,
    pub(crate) prewire_allows_text_area_editable_behavior_path_switch: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcImeEquivalenceAuditInput {
    pub(crate) fn from_behavior_status_and_ime_prewire(
        behavior_status: &TextAreaEditableIfcBehaviorPathStatus,
        ime_prewire: &TextAreaEditableIfcImeBehaviorPathPrewire,
    ) -> Self {
        Self {
            readiness_blocked_reasons: behavior_status
                .behavior_path_switch_blocked_reasons()
                .into_iter()
                .map(|reason| match reason {
                    TextAreaEditableIfcBehaviorPathStatusBlockedReason::StatusObservationOnly => {
                        TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                            BehaviorPathStatusBlocked
                    }
                    TextAreaEditableIfcBehaviorPathStatusBlockedReason::BehaviorPathsStillNotReady => {
                        TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                            BehaviorPathsStillNotReady
                    }
                    _ => TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                        BehaviorPathStatusBlocked,
                })
                .collect(),
            ime_prewire_state: ime_prewire.state(),
            ime_diagnostic_prewired: ime_prewire.diagnostic_prewired(),
            ime_metadata_status: ime_prewire
                .diagnostic()
                .map(|diagnostic| diagnostic.ime_metadata_status)
                .unwrap_or(TextAreaInlineIfcMetadataBridgeStatus::Unwired),
            ime_metadata_diagnostic: ime_prewire.diagnostic().cloned(),
            ime_behavior_path_ready: ime_prewire.ime_behavior_path_ready(),
            prewire_render_enabled: ime_prewire.render_enabled(),
            prewire_layout_enabled: ime_prewire.layout_enabled(),
            prewire_allows_text_area_editable_behavior_path_switch: ime_prewire
                .allows_text_area_editable_behavior_path_switch(),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaEditableIfcImeEquivalenceAudit {
    state: TextAreaEditableIfcBehaviorEquivalenceAuditState,
    blocked_reasons: Vec<TextAreaEditableIfcImeEquivalenceAuditBlockedReason>,
    input: TextAreaEditableIfcImeEquivalenceAuditInput,
    diagnostic: TextAreaEditableIfcImeEquivalenceAuditDiagnostic,
    recommendation: TextAreaEditableIfcBehaviorEquivalenceAuditRecommendation,
    render_enabled: bool,
    layout_enabled: bool,
    ime_behavior_path_ready: bool,
}

#[allow(dead_code)]
impl TextAreaEditableIfcImeEquivalenceAudit {
    pub(crate) fn evaluate(input: TextAreaEditableIfcImeEquivalenceAuditInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if input.readiness_blocked_reasons.is_empty() {
            blocked_reasons.push(
                TextAreaEditableIfcImeEquivalenceAuditBlockedReason::BehaviorPathReadinessMissing,
            );
        }
        if input.ime_metadata_diagnostic.is_none() {
            blocked_reasons.push(
                TextAreaEditableIfcImeEquivalenceAuditBlockedReason::MissingImeMetadataDiagnostic,
            );
        }
        if input.ime_prewire_state
            != TextAreaEditableIfcImeBehaviorPathPrewireState::DiagnosticPrewired
        {
            blocked_reasons.push(
                TextAreaEditableIfcImeEquivalenceAuditBlockedReason::ImeMetadataSourceNotObserved,
            );
        }
        if !input.ime_diagnostic_prewired {
            blocked_reasons
                .push(TextAreaEditableIfcImeEquivalenceAuditBlockedReason::ImePrewireMissing);
        }
        if input.ime_metadata_status != TextAreaInlineIfcMetadataBridgeStatus::Observed {
            blocked_reasons.push(
                TextAreaEditableIfcImeEquivalenceAuditBlockedReason::ImeMetadataStatusNotObserved,
            );
        }

        let diagnostic_values = input.ime_metadata_diagnostic.clone().unwrap_or(
            TextAreaEditableIfcImeBehaviorPathPrewireDiagnostic {
                ime_metadata_status: input.ime_metadata_status,
                run_count: 0,
                has_inline_preedit: false,
                has_preedit_run: false,
                preedit_cursor_count: 0,
                preedit_cursors: Vec::new(),
                effective_content_len: 0,
            },
        );
        if !diagnostic_values.has_inline_preedit && !diagnostic_values.has_preedit_run {
            blocked_reasons
                .push(TextAreaEditableIfcImeEquivalenceAuditBlockedReason::NoPreeditCase);
        }
        if diagnostic_values.preedit_cursor_count != diagnostic_values.preedit_cursors.len() {
            blocked_reasons.push(
                TextAreaEditableIfcImeEquivalenceAuditBlockedReason::PreeditMetadataShapeMismatch,
            );
        }

        let state = if blocked_reasons.is_empty() {
            TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly
        } else {
            TextAreaEditableIfcBehaviorEquivalenceAuditState::Blocked
        };
        let recommendation =
            if state == TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly {
                TextAreaEditableIfcBehaviorEquivalenceAuditRecommendation::ObservationOnlyNoOp
            } else {
                TextAreaEditableIfcBehaviorEquivalenceAuditRecommendation::
                    KeepLegacyEditableBehaviorPath
            };
        let equivalent_candidate =
            if state == TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly {
                TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::EquivalentCandidateObserved
            } else {
                TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::None
            };
        let diagnostic = TextAreaEditableIfcImeEquivalenceAuditDiagnostic {
            readiness_blocked_reasons: input.readiness_blocked_reasons.clone(),
            ime_prewire_state: input.ime_prewire_state,
            ime_diagnostic_prewired: input.ime_diagnostic_prewired,
            ime_metadata_status: input.ime_metadata_status,
            run_count: diagnostic_values.run_count,
            has_inline_preedit: diagnostic_values.has_inline_preedit,
            has_preedit_run: diagnostic_values.has_preedit_run,
            preedit_cursor_count: diagnostic_values.preedit_cursor_count,
            preedit_cursors: diagnostic_values.preedit_cursors,
            effective_content_len: diagnostic_values.effective_content_len,
            equivalent_candidate,
            ime_behavior_path_ready: false,
            render_enabled: false,
            layout_enabled: false,
            allows_text_area_editable_behavior_path_switch: false,
            recommendation,
        };

        Self {
            state,
            blocked_reasons,
            input,
            diagnostic,
            recommendation,
            render_enabled: false,
            layout_enabled: false,
            ime_behavior_path_ready: false,
        }
    }

    pub(crate) fn state(&self) -> TextAreaEditableIfcBehaviorEquivalenceAuditState {
        self.state
    }

    pub(crate) fn blocked_reasons(&self) -> &[TextAreaEditableIfcImeEquivalenceAuditBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn input(&self) -> &TextAreaEditableIfcImeEquivalenceAuditInput {
        &self.input
    }

    pub(crate) fn diagnostic(&self) -> &TextAreaEditableIfcImeEquivalenceAuditDiagnostic {
        &self.diagnostic
    }

    pub(crate) fn recommendation(
        &self,
    ) -> TextAreaEditableIfcBehaviorEquivalenceAuditRecommendation {
        self.recommendation
    }

    pub(crate) fn ime_behavior_path_ready(&self) -> bool {
        self.ime_behavior_path_ready
    }

    pub(crate) fn render_enabled(&self) -> bool {
        self.render_enabled
    }

    pub(crate) fn layout_enabled(&self) -> bool {
        self.layout_enabled
    }

    pub(crate) fn allows_text_area_editable_behavior_path_switch(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaInlineIfcEvaluationPreflight {
    state: TextAreaInlineIfcEvaluationPreflightState,
    blocked_reasons: Vec<TextAreaInlineIfcEvaluationPreflightBlockedReason>,
    run_inputs: Vec<TextAreaInlineIfcEvaluationRunInput>,
    render_enabled: bool,
    layout_enabled: bool,
}

#[allow(dead_code)]
impl TextAreaInlineIfcEvaluationPreflight {
    pub(crate) fn evaluate(input: TextAreaInlineIfcEvaluationInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if input.run_inputs.is_empty() {
            blocked_reasons
                .push(TextAreaInlineIfcEvaluationPreflightBlockedReason::MissingRunPayload);
        }
        if !input.projection_ifc_path_ready {
            blocked_reasons
                .push(TextAreaInlineIfcEvaluationPreflightBlockedReason::ProjectionPathUnwired);
        }
        if !input.ime_ifc_path_ready {
            blocked_reasons.push(TextAreaInlineIfcEvaluationPreflightBlockedReason::ImePathUnwired);
        }
        if !input.caret_affinity_ifc_path_ready {
            blocked_reasons
                .push(TextAreaInlineIfcEvaluationPreflightBlockedReason::CaretAffinityPathUnwired);
        }
        if !input.scroll_follow_ifc_path_ready {
            blocked_reasons
                .push(TextAreaInlineIfcEvaluationPreflightBlockedReason::ScrollFollowPathUnwired);
        }
        if !input.legacy_fallback_confirmed {
            blocked_reasons
                .push(TextAreaInlineIfcEvaluationPreflightBlockedReason::LegacyFallbackMissing);
        }
        if !input.read_only_text_path_separated {
            blocked_reasons.push(
                TextAreaInlineIfcEvaluationPreflightBlockedReason::
                    ReadOnlyTextPathSeparationMissing,
            );
        }

        let state = if blocked_reasons.is_empty() {
            TextAreaInlineIfcEvaluationPreflightState::ReadyForDiagnosticEvaluation
        } else {
            TextAreaInlineIfcEvaluationPreflightState::Blocked
        };
        Self {
            state,
            blocked_reasons,
            run_inputs: input.run_inputs,
            render_enabled: false,
            layout_enabled: false,
        }
    }

    pub(crate) fn state(&self) -> TextAreaInlineIfcEvaluationPreflightState {
        self.state
    }

    pub(crate) fn blocked_reasons(&self) -> &[TextAreaInlineIfcEvaluationPreflightBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn run_inputs(&self) -> &[TextAreaInlineIfcEvaluationRunInput] {
        &self.run_inputs
    }

    pub(crate) fn render_enabled(&self) -> bool {
        self.render_enabled
    }

    pub(crate) fn layout_enabled(&self) -> bool {
        self.layout_enabled
    }

    pub(crate) fn allows_text_area_editable_behavior_path_switch(&self) -> bool {
        false
    }
}

impl TextAreaTextRun {
    pub(crate) fn new(text: String, char_range: Range<usize>) -> Self {
        Self {
            text,
            char_range,
            is_placeholder: false,
            is_preedit_run: false,
            preedit_cursor: None,
            font_families: Vec::new(),
            font_size: 14.0,
            line_height: 1.25,
            font_weight: 400,
            color: crate::style::Color::rgba(17, 17, 17, 255),
            cursor: Cursor::Text,
            auto_wrap: true,
            vertical_align: crate::style::VerticalAlign::Baseline,
            inline_preedit: None,
            text_layout: None,
            last_inline_measure_context: None,
            layout_state: LayoutState::new(0.0, 0.0, 0.0, 0.0),
            inline_paint_fragments: Vec::new(),
            dirty_flags: DirtyFlags::ALL,
            #[cfg(test)]
            inline_ifc_force_missing_prepared_candidate: false,
            node_id: next_ui_node_id(),
            parent_id: None,
            children: Vec::new(),
        }
    }

    pub fn char_range(&self) -> Range<usize> {
        self.char_range.clone()
    }

    #[cfg(test)]
    fn force_missing_inline_ifc_prepared_candidate_for_test(&mut self) {
        self.inline_ifc_force_missing_prepared_candidate = true;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::PAINT);
    }

    pub fn set_inline_preedit(&mut self, preedit: Option<InlinePreedit>) {
        if self.inline_preedit == preedit {
            return;
        }
        self.inline_preedit = preedit;
        self.invalidate_text_layout();
    }

    pub(crate) fn set_preedit_run(&mut self, is_preedit_run: bool, cursor: Option<(usize, usize)>) {
        if self.is_preedit_run == is_preedit_run && self.preedit_cursor == cursor {
            return;
        }
        self.is_preedit_run = is_preedit_run;
        self.preedit_cursor = cursor;
        self.invalidate_text_layout();
    }

    pub(crate) fn is_preedit_run(&self) -> bool {
        self.is_preedit_run
    }

    pub(crate) fn set_text(&mut self, text: String, char_range: Range<usize>) {
        if self.text == text && self.char_range == char_range {
            return;
        }
        self.text = text;
        self.char_range = char_range;
        self.invalidate_text_layout();
    }

    /// Cascade-style cascaded set: owner TextArea calls this after edit/
    /// content-rebuild so the run picks up the up-to-date inherited values.
    pub(crate) fn cascade_style(
        &mut self,
        font_families: Vec<String>,
        font_size: f32,
        line_height: f32,
        vertical_align: crate::style::VerticalAlign,
        font_weight: u16,
        color: crate::style::Color,
        cursor: Cursor,
        auto_wrap: bool,
    ) {
        let layout_changed = self.font_families != font_families
            || self.font_size != font_size
            || self.line_height != line_height
            || self.vertical_align != vertical_align
            || self.font_weight != font_weight
            || self.color != color
            || self.auto_wrap != auto_wrap;
        self.font_families = font_families;
        self.font_size = font_size;
        self.line_height = line_height;
        self.vertical_align = vertical_align;
        self.font_weight = font_weight;
        self.color = color;
        self.cursor = cursor;
        self.auto_wrap = auto_wrap;
        if layout_changed {
            self.invalidate_text_layout();
        }
    }

    fn invalidate_text_layout(&mut self) {
        self.text_layout = None;
        self.last_inline_measure_context = None;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
    }

    /// Effective text including any spliced IME preedit segment.
    fn effective_text(&self) -> String {
        match &self.inline_preedit {
            None => self.text.clone(),
            Some(preedit) => {
                let local_byte = byte_index_at_char(&self.text, preedit.insert_at_local);
                let mut out = String::with_capacity(self.text.len() + preedit.preedit_text.len());
                out.push_str(&self.text[..local_byte]);
                out.push_str(&preedit.preedit_text);
                out.push_str(&self.text[local_byte..]);
                out
            }
        }
    }

    fn build_run_text_layout(&self, max_width: Option<f32>) -> Arc<TextLayout> {
        measure_text_layout(
            &self.effective_text(),
            max_width,
            self.auto_wrap,
            self.font_size,
            self.line_height,
            self.font_weight,
            TextLayoutAlignment::Left,
            &self.font_families,
        )
        .text_layout
    }

    #[allow(dead_code)]
    pub(crate) fn inline_ifc_staging_payload(
        &self,
        origin: [f32; 2],
        fragment_index: u32,
        opacity: f32,
    ) -> Option<TextAreaTextRunInlineIfcStagingPayload> {
        if self.text_layout.is_none() || opacity <= 0.0 {
            return None;
        }
        let effective_text = self.effective_text();
        if effective_text.is_empty() {
            return None;
        }

        let layout_width = self.layout_state.layout_size.width.max(1.0);
        let layout_height = self.layout_state.layout_size.height.max(1.0);
        let mut bridge_input = TextReadOnlyIfcBridgeInput::new(
            effective_text.clone(),
            InlineIfcStyle {
                font_size: self.font_size,
                line_height: self.line_height,
                font_weight: self.font_weight,
                brush: self.color.to_rgba_u8(),
                font_families: self.font_families.clone(),
            },
            opacity,
            fragment_index,
        )
        .with_text_color(self.color.to_rgba_f32());
        bridge_input.origin = origin;
        bridge_input.layout_size = [layout_width, layout_height];
        bridge_input.width_constraint = if self.auto_wrap {
            Some(
                self.last_inline_measure_context
                    .map(|context| context.full_available_width.max(1.0))
                    .unwrap_or(layout_width),
            )
        } else {
            None
        };
        bridge_input.allow_wrap = self.auto_wrap;

        let bridge_package = build_text_read_only_ifc_bridge_package_from_input(&bridge_input);
        let prepared_input =
            build_inline_text_pass_prepared_input(&bridge_input, &bridge_package, 1.0);
        let text_pass_staging_input =
            inline_prepared_input_to_text_pass_staging_input(&prepared_input);
        let prepared_candidate = TextAreaTextRunInlineIfcPreparedCandidate::from_prepared_payload(
            self.char_range.clone(),
            &bridge_input,
            &bridge_package,
            &prepared_input,
            &text_pass_staging_input,
        );
        let seed_caret_lines = self.seed_caret_stops_for_ifc_snapshot();
        let caret_stop_snapshots = seed_caret_lines
            .iter()
            .enumerate()
            .flat_map(|(visual_line_index, line)| {
                line.stops
                    .iter()
                    .enumerate()
                    .map(
                        move |(stop_index, stop)| TextAreaEditableIfcCaretAffinityStopSnapshot {
                            run_index: 0,
                            visual_line_index,
                            stop_index,
                            local_char: stop.local_char,
                            local_x: stop.local_x,
                            local_y_top: stop.local_y_top,
                            height: stop.height,
                        },
                    )
            })
            .collect();
        let caret_affinity_diagnostic = TextAreaTextRunInlineIfcCaretAffinityDiagnostic {
            visual_line_count: seed_caret_lines.len(),
            caret_stop_count: seed_caret_lines.iter().map(|line| line.stops.len()).sum(),
            multi_stop_line_count: seed_caret_lines
                .iter()
                .filter(|line| line.stops.len() > 1)
                .count(),
            caret_stop_snapshots,
            has_preedit_cursor: self.preedit_cursor.is_some(),
            preedit_cursor: self.preedit_cursor,
        };
        let projection_diagnostic = TextAreaTextRunInlineIfcProjectionDiagnostic {
            char_range: self.char_range.clone(),
            char_span: self.char_range.end.saturating_sub(self.char_range.start),
            effective_content_len: effective_text.chars().count(),
            has_inline_preedit: self.inline_preedit.is_some(),
            is_preedit_run: self.is_preedit_run,
            projection_segment_count: 0,
        };
        let scroll_follow_diagnostic = TextAreaTextRunInlineIfcScrollFollowDiagnostic {
            char_range: self.char_range.clone(),
            char_span: self.char_range.end.saturating_sub(self.char_range.start),
            layout_size: [layout_width, layout_height],
            effective_content_len: effective_text.chars().count(),
            visual_line_count: caret_affinity_diagnostic.visual_line_count,
            caret_stop_count: caret_affinity_diagnostic.caret_stop_count,
        };
        let diagnostic = TextAreaTextRunInlineIfcDiagnostic {
            char_range: self.char_range.clone(),
            content_len: self.text.chars().count(),
            effective_content_len: effective_text.chars().count(),
            layout_size: [layout_width, layout_height],
            bridge_glyph_count: bridge_package.glyphs.len(),
            prepared_glyph_count: prepared_input.glyphs.len(),
            staging_glyph_count: text_pass_staging_input.glyphs.len(),
            batch_count: prepared_input.batches.len(),
        };

        Some(TextAreaTextRunInlineIfcStagingPayload {
            char_range: self.char_range.clone(),
            render_enabled: false,
            fallback: TextAreaTextRunInlineIfcRenderFallback::LegacyTextPass,
            readiness: TextAreaTextRunInlineIfcReadinessMetadata {
                editable_text_area_run: true,
                projection_ifc_path_ready: false,
                ime_ifc_path_ready: false,
                caret_affinity_ifc_path_ready: false,
                scroll_follow_ifc_path_ready: false,
                has_inline_preedit: self.inline_preedit.is_some(),
                is_preedit_run: self.is_preedit_run,
                preedit_cursor: self.preedit_cursor,
                projection_diagnostic,
                caret_affinity_diagnostic,
                scroll_follow_diagnostic,
            },
            bridge_input,
            bridge_package,
            prepared_input,
            prepared_candidate,
            text_pass_staging_input,
            diagnostic,
        })
    }

    /// `local_char` here is in the run's *own* char index
    /// (0..self.text.chars().count()). Returns `(x, y_top, line_height)`
    /// in run-local coordinates.
    ///
    /// The plain-text `local_char` is translated into the matching byte
    /// inside `effective_text` (which includes any spliced preedit text)
    /// before asking the adapter for caret geometry.
    pub fn local_char_to_screen_position(&self, local_char: usize) -> Option<(f32, f32, f32)> {
        self.local_char_to_screen_position_with_affinity(
            local_char,
            super::caret_map::CaretAffinity::Downstream,
        )
    }

    /// Like [`Self::local_char_to_screen_position`] but biases the
    /// soft-wrap boundary based on `affinity`. `Upstream` returns the
    /// upper line's tail position when `local_char` lands at the wrap
    /// point; `Downstream` returns the lower line's head (current
    /// pre-affinity behaviour).
    pub fn local_char_to_screen_position_with_affinity(
        &self,
        local_char: usize,
        affinity: super::caret_map::CaretAffinity,
    ) -> Option<(f32, f32, f32)> {
        if self.text_layout.is_some() {
            return self.caret_affinity_placement_position_from_ifc(local_char, affinity);
        }
        Some(self.empty_line_caret_position())
    }

    #[allow(dead_code)]
    pub(crate) fn caret_affinity_placement_position_from_ifc(
        &self,
        local_char: usize,
        affinity: super::caret_map::CaretAffinity,
    ) -> Option<(f32, f32, f32)> {
        let adapter = self.caret_affinity_read_only_lookup_adapter_from_ifc()?;
        let ifc_position = adapter
            .placement_read_only_adapter()?
            .local_char_to_run_local_position_with_affinity(0, local_char, affinity)?;
        Some(ifc_position)
    }

    fn caret_affinity_read_only_lookup_adapter_from_ifc(
        &self,
    ) -> Option<TextAreaEditableIfcCaretAffinityReadOnlyLookupAdapter> {
        let payload = self.inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)?;
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let readiness_audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&readiness_audit, &preflight),
        );
        let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
            TextAreaEditableIfcCaretAffinityMetadataSourceInput::
                from_readiness_audit_and_metadata_bridge_preflight(&readiness_audit, &preflight),
        );
        let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&readiness_audit, &preflight),
        );
        let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&readiness_audit, &preflight),
        );
        let behavior_status = TextAreaEditableIfcBehaviorPathStatus::evaluate(
            TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
                &preflight,
                &readiness_audit,
                &ime_prewire,
                &caret_metadata_source,
                &projection_prewire,
                &scroll_prewire,
            ),
        );
        let audit = TextAreaEditableIfcCaretAffinityEquivalenceAudit::evaluate(
            TextAreaEditableIfcCaretAffinityEquivalenceAuditInput::
                from_behavior_status_and_caret_affinity_metadata_source(
                    &behavior_status,
                    &caret_metadata_source,
                ),
        );
        let behavior_input =
            TextAreaEditableIfcCaretAffinityBehaviorInput::from_equivalence_audit(&audit);
        let evaluation =
            TextAreaEditableIfcCaretAffinityBehaviorEvaluation::evaluate(behavior_input);
        Some(
            TextAreaEditableIfcCaretAffinityReadOnlyLookupAdapter::from_behavior_evaluation(
                &evaluation,
            ),
        )
    }

    /// Caret position when the IME preedit is open inside this Run. Honors
    /// the preedit's own caret (`preedit_cursor`) so the visible caret sits
    /// inside the composing text rather than at the splice point — mirrors
    /// v1's `preedit_fragment_caret_screen_position`.
    pub fn preedit_caret_local_position(&self) -> Option<(f32, f32, f32)> {
        if self.is_preedit_run {
            let caret_byte = match self.preedit_cursor {
                Some((_, end)) => clamp_utf8_boundary(&self.text, end),
                None => self.text.len(),
            };
            if let Some(layout) = self.text_layout.as_ref() {
                let geom = layout.cursor_geometry(caret_byte, false);
                return Some((geom.x, geom.y, geom.height));
            }
            return Some(self.empty_line_caret_position());
        }
        let preedit = self.inline_preedit.as_ref()?;
        let pre_byte = byte_index_at_char(&self.text, preedit.insert_at_local);
        let caret_byte_in_preedit = match preedit.preedit_cursor {
            Some((_, end)) => clamp_utf8_boundary(&preedit.preedit_text, end),
            None => preedit.preedit_text.len(),
        };
        let target_byte = pre_byte + caret_byte_in_preedit;
        if let Some(layout) = self.text_layout.as_ref() {
            // TODO: Fold IME composing into `CaretNavigationMap` once the
            // map can represent transient preedit positions separately from
            // committed document char indices. For Phase 5A this special
            // path stays, but its geometry is adapter-backed.
            let geom = layout.cursor_geometry(target_byte, false);
            return Some((geom.x, geom.y, geom.height));
        }
        Some(self.empty_line_caret_position())
    }

    fn plain_local_char_to_effective_byte(&self, local_char: usize, effective: &str) -> usize {
        let char_in_effective = match &self.inline_preedit {
            Some(preedit) if local_char > preedit.insert_at_local => {
                local_char + preedit.preedit_text.chars().count()
            }
            _ => local_char,
        };
        byte_index_at_char(effective, char_in_effective)
    }

    fn empty_line_caret_position(&self) -> (f32, f32, f32) {
        let line_h = self.font_size.max(1.0) * self.line_height.max(0.8);
        (0.0, 0.0, line_h)
    }

    fn fallback_first_baseline(&self) -> f32 {
        let font_size = self.font_size.max(1.0);
        let line_height = font_size * self.line_height.max(0.8);
        let approx_ascent = font_size * 0.8779297;
        let leading = (line_height - font_size).max(0.0);
        (approx_ascent + leading / 2.0).max(0.0)
    }

    fn inline_line_nodes(&self) -> Vec<InlineNodeSize> {
        let Some(layout) = self.text_layout.as_ref() else {
            let nodes = vec![InlineNodeSize {
                width: self.layout_state.layout_size.width,
                height: self.layout_state.layout_size.height,
                baseline: self.fallback_first_baseline(),
                vertical_align: self.vertical_align,
                force_break_after: false,
            }];
            return nodes;
        };
        let effective = self.effective_text();
        let mut nodes: Vec<InlineNodeSize> = layout
            .inline_line_fragments(&effective)
            .into_iter()
            .map(|line| InlineNodeSize {
                width: line.width,
                height: line.height,
                baseline: line.baseline,
                vertical_align: self.vertical_align,
                force_break_after: false,
            })
            .collect();
        if nodes.is_empty() {
            nodes.push(InlineNodeSize {
                width: 0.0,
                height: self.font_size.max(1.0) * self.line_height.max(0.8),
                baseline: self.fallback_first_baseline(),
                vertical_align: self.vertical_align,
                force_break_after: false,
            });
        }
        let last = nodes.len().saturating_sub(1);
        for (idx, node) in nodes.iter_mut().enumerate() {
            node.force_break_after = idx < last;
        }
        nodes
    }

    fn inline_text_pass_fragments(
        &self,
        opacity: f32,
        paint_offset: [f32; 2],
    ) -> Vec<TextPassFragment> {
        let Some(layout) = self.text_layout.as_ref() else {
            return Vec::new();
        };
        let effective = self.effective_text();
        let line_fragments = layout.inline_line_fragments(&effective);
        if line_fragments.len() <= 1 || line_fragments.len() != self.inline_paint_fragments.len() {
            return Vec::new();
        }
        line_fragments
            .into_iter()
            .zip(self.inline_paint_fragments.iter())
            .filter_map(|(line, rect)| {
                if line.content.is_empty() {
                    return None;
                }
                let x = rect.x + paint_offset[0];
                let y = rect.y + paint_offset[1];
                let fragment_layout = measure_text_layout(
                    line.content.as_str(),
                    Some(line.width.max(1.0)),
                    false,
                    self.font_size,
                    self.line_height,
                    self.font_weight,
                    TextLayoutAlignment::Left,
                    self.font_families.as_slice(),
                );
                Some(TextPassFragment {
                    content: line.content,
                    x,
                    y,
                    width: rect.width.max(line.width).max(1.0),
                    height: rect.height.max(line.height).max(1.0),
                    color: self.color.to_rgba_f32(),
                    opacity,
                    text_layout: Some(fragment_layout.text_layout),
                })
            })
            .collect()
    }

    fn prepared_render_payload(
        &self,
        origin: [f32; 2],
        fragment_index: u32,
        opacity: f32,
    ) -> Option<TextAreaTextRunInlineIfcStagingPayload> {
        #[cfg(test)]
        if self.inline_ifc_force_missing_prepared_candidate {
            return None;
        }
        let payload = self.inline_ifc_staging_payload(origin, fragment_index, opacity)?;
        let candidate = &payload.prepared_candidate;
        if candidate.fallback != TextAreaTextRunInlineIfcRenderFallback::LegacyTextPass
            || candidate.glyph_count == 0
            || candidate.prepared_glyph_count == 0
            || candidate.staging_glyph_count == 0
            || candidate.prepared_glyph_count != candidate.staging_glyph_count
            || candidate.glyph_metadata.len() != candidate.prepared_glyph_count
            || payload.text_pass_staging_input.glyphs.len() != candidate.staging_glyph_count
            || candidate.batch_count == 0
            || candidate.opacity <= 0.0
            || candidate.layout_size[0] <= 0.0
            || candidate.layout_size[1] <= 0.0
        {
            return None;
        }
        Some(payload)
    }

    #[cfg(test)]
    pub(crate) fn inline_fragment_positions(&self) -> Vec<(String, Rect)> {
        let Some(layout) = self.text_layout.as_ref() else {
            return Vec::new();
        };
        layout
            .inline_line_fragments(&self.effective_text())
            .into_iter()
            .zip(self.inline_paint_fragments.iter().copied())
            .map(|(line, rect)| (line.content, rect))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn inline_text_pass_fragment_positions(&self) -> Vec<(String, Rect)> {
        self.inline_text_pass_fragment_positions_with_offset([0.0, 0.0])
    }

    #[cfg(test)]
    pub(crate) fn inline_text_pass_fragment_positions_with_offset(
        &self,
        paint_offset: [f32; 2],
    ) -> Vec<(String, Rect)> {
        let fragments = self.inline_text_pass_fragments(1.0, paint_offset);
        if fragments.is_empty() && self.text_layout.is_some() && !self.effective_text().is_empty() {
            return vec![(
                self.effective_text(),
                Rect {
                    x: self.layout_state.layout_position.x + paint_offset[0],
                    y: self.layout_state.layout_position.y + paint_offset[1],
                    width: self.layout_state.layout_size.width.max(1.0),
                    height: self.layout_state.layout_size.height.max(1.0),
                },
            )];
        }
        fragments
            .into_iter()
            .map(|fragment| {
                (
                    fragment.content,
                    Rect {
                        x: fragment.x,
                        y: fragment.y,
                        width: fragment.width,
                        height: fragment.height,
                    },
                )
            })
            .collect()
    }

    /// Hit-test: run-local (x, y) → char index in `effective_text`
    /// (i.e. the spliced text the adapter laid out). When
    /// no preedit is active this matches `self.text`; with preedit, the
    /// returned index counts preedit chars too. Callers in commit-tap
    /// flows commit the preedit first, after which `self.content` matches
    /// the effective text for this Run, so the index maps directly to
    /// the post-commit content char index.
    ///
    pub fn screen_position_to_local_char(&self, x: f32, y: f32) -> Option<usize> {
        if self.is_preedit_run {
            return Some(0);
        }
        if self.text_layout.is_some() {
            if self.effective_text().is_empty() {
                return Some(0);
            }
            return self.screen_position_to_local_char_from_ifc(x, y);
        }
        None
    }

    fn screen_position_to_local_char_from_ifc(&self, x: f32, y: f32) -> Option<usize> {
        let adapter = self.caret_affinity_read_only_lookup_adapter_from_ifc()?;
        let helper = adapter.behavior_helper()?;
        let summary = helper.placement_navigation_summary();
        let visual_line_count = summary.per_run_visual_line_counts.first().copied()?;
        if visual_line_count == 0 {
            return None;
        }
        let snapshots = helper
            .caret_stop_snapshots()
            .iter()
            .filter(|snapshot| snapshot.run_index == 0)
            .collect::<Vec<_>>();
        if snapshots.len()
            != summary
                .per_run_caret_stop_counts
                .first()
                .copied()
                .unwrap_or(0)
        {
            return None;
        }

        let mut lines = (0..visual_line_count)
            .map(|visual_line_index| {
                let mut line_snapshots = snapshots
                    .iter()
                    .copied()
                    .filter(|snapshot| snapshot.visual_line_index == visual_line_index)
                    .collect::<Vec<_>>();
                line_snapshots.sort_by_key(|snapshot| snapshot.stop_index);
                line_snapshots
            })
            .collect::<Vec<_>>();
        if lines.iter().any(Vec::is_empty) {
            return None;
        }

        let line_index = lines
            .iter()
            .enumerate()
            .find_map(|(line_index, line_snapshots)| {
                let line_top = line_snapshots
                    .iter()
                    .map(|snapshot| snapshot.local_y_top)
                    .fold(f32::INFINITY, f32::min);
                let line_bottom = line_snapshots
                    .iter()
                    .map(|snapshot| snapshot.local_y_top + snapshot.height)
                    .fold(line_top, f32::max);
                let is_last = line_index + 1 == lines.len();
                if y >= line_top && (y < line_bottom || is_last && y <= line_bottom) {
                    Some(line_index)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                let first_top = lines
                    .first()
                    .and_then(|line| line.first())
                    .map(|snapshot| snapshot.local_y_top)
                    .unwrap_or(0.0);
                if y < first_top {
                    0
                } else {
                    lines.len().saturating_sub(1)
                }
            });
        let line_snapshots = lines.swap_remove(line_index);
        let first = line_snapshots.first()?;
        if x <= first.local_x {
            return Some(first.local_char);
        }
        for pair in line_snapshots.windows(2) {
            let left = pair[0];
            let right = pair[1];
            let midpoint = left.local_x + (right.local_x - left.local_x) / 2.0;
            if x < midpoint {
                return Some(left.local_char);
            }
        }
        line_snapshots.last().map(|snapshot| snapshot.local_char)
    }

    /// Run-local selection range → visual rects (one per visual line covered).
    pub fn local_selection_rects(&self, local_start: usize, local_end: usize) -> Vec<Rect> {
        let start_char = local_start.min(local_end);
        let end_char = local_start.max(local_end);
        if start_char == end_char {
            return Vec::new();
        }
        if let Some(layout) = self.text_layout.as_ref() {
            let line_fragments = layout.inline_line_fragments(&self.text);
            if line_fragments.len() > 1 && line_fragments.len() == self.inline_paint_fragments.len()
            {
                let origin = self.layout_state.layout_position;
                let mut out = Vec::new();
                let mut consumed_chars = 0_usize;
                for (line, fragment_rect) in line_fragments
                    .into_iter()
                    .zip(self.inline_paint_fragments.iter())
                {
                    let frag_chars = line.content.chars().count();
                    let frag_start = consumed_chars;
                    let frag_end = consumed_chars + frag_chars;
                    consumed_chars = frag_end;
                    if frag_end <= start_char || frag_start >= end_char {
                        continue;
                    }
                    let fragment_start = start_char.saturating_sub(frag_start);
                    let fragment_end = end_char.saturating_sub(frag_start).min(frag_chars);
                    let fragment_layout = measure_text_layout(
                        line.content.as_str(),
                        Some(line.width.max(1.0)),
                        false,
                        self.font_size,
                        self.line_height,
                        self.font_weight,
                        TextLayoutAlignment::Left,
                        self.font_families.as_slice(),
                    );
                    for rect in fragment_layout.text_layout.selection_rects(
                        line.content.as_str(),
                        fragment_start,
                        fragment_end,
                    ) {
                        out.push(Rect {
                            x: fragment_rect.x - origin.x + rect.x,
                            y: fragment_rect.y - origin.y + rect.y,
                            width: rect.width,
                            height: rect.height,
                        });
                    }
                }
                return out;
            }
            return layout.selection_rects(self.text.as_str(), start_char, end_char);
        }
        Vec::new()
    }

    /// Underline rects (run-local coords) covering the active IME preedit
    /// segment. One rect per visual line the preedit spans. Empty when no
    /// preedit is active or layout is stale. 1-px-tall stripes pinned to
    /// the visual-line baseline — matches v1's
    /// `ime_preedit_underline_rects` look.
    pub fn preedit_underline_rects(&self) -> Vec<Rect> {
        if self.is_preedit_run {
            return self
                .local_selection_rects(0, self.text.chars().count())
                .into_iter()
                .map(|rect| Rect {
                    x: rect.x,
                    y: rect.y + rect.height.max(1.0) - 1.0,
                    width: rect.width.max(1.0),
                    height: 1.0,
                })
                .collect();
        }
        let Some(preedit) = self.inline_preedit.as_ref() else {
            return Vec::new();
        };
        if preedit.preedit_text.is_empty() {
            return Vec::new();
        }
        let effective = self.effective_text();
        if let Some(layout) = self.text_layout.as_ref() {
            return layout
                .selection_rects(
                    &effective,
                    preedit.insert_at_local,
                    preedit.insert_at_local + preedit.preedit_text.chars().count(),
                )
                .into_iter()
                .map(|rect| Rect {
                    x: rect.x,
                    y: rect.y + rect.height.max(1.0) - 1.0,
                    width: rect.width.max(1.0),
                    height: 1.0,
                })
                .collect();
        }
        Vec::new()
    }

    /// Number of visual (post-wrap) lines in the current layout. Useful for
    /// vertical caret movement and sticky-x bookkeeping.
    pub fn visual_line_count(&self) -> usize {
        if let Some(layout) = self.text_layout.as_ref() {
            return layout.lines().len().max(1);
        }
        1
    }

    /// Run-local caret stops grouped by visual line. Each line carries the
    /// stops needed by the TextArea-level `CaretNavigationMap` builder so it
    /// can drive vertical Up/Down navigation, caret rendering, and pointer
    /// hit-test from a single source of truth (see
    /// `docs/design/textarea-caret-navigation.md`).
    ///
    /// Coordinates returned here are **run-local**: the map builder adds
    /// `layout_position` to translate to screen space. Char indices are
    /// **run-local** too (`0..self.text.chars().count()`); the builder adds
    /// `char_range.start` for the root content index.
    ///
    /// Laid-out lines come from the read-only IFC caret affinity snapshot.
    /// Empty paragraphs (created by `\n\n` or a trailing `\n`) get a
    /// synthesized line so caret navigation can land on the blank line.
    pub fn caret_stops(&self) -> Vec<RunCaretLine> {
        if self.text_layout.is_some() {
            if self.effective_text().is_empty() {
                return self.empty_line_caret_stops();
            }
            return self.caret_stops_from_ifc().unwrap_or_default();
        }

        self.empty_line_caret_stops()
    }

    fn caret_stops_from_ifc(&self) -> Option<Vec<RunCaretLine>> {
        let adapter = self.caret_affinity_read_only_lookup_adapter_from_ifc()?;
        let helper = adapter.behavior_helper()?;
        let summary = helper.placement_navigation_summary();
        let visual_line_count = summary.per_run_visual_line_counts.first().copied()?;
        if visual_line_count == 0 {
            return Some(Vec::new());
        }
        let snapshots = helper
            .caret_stop_snapshots()
            .iter()
            .filter(|snapshot| snapshot.run_index == 0)
            .collect::<Vec<_>>();
        if snapshots.len()
            != summary
                .per_run_caret_stop_counts
                .first()
                .copied()
                .unwrap_or(0)
        {
            return None;
        }

        (0..visual_line_count)
            .map(|visual_line_index| {
                let line_snapshots = snapshots
                    .iter()
                    .copied()
                    .filter(|snapshot| snapshot.visual_line_index == visual_line_index)
                    .collect::<Vec<_>>();
                let first = line_snapshots.first()?;
                let local_y_top = first.local_y_top;
                let local_y_bottom = line_snapshots
                    .iter()
                    .map(|snapshot| snapshot.local_y_top + snapshot.height)
                    .fold(local_y_top, f32::max);
                Some(RunCaretLine {
                    local_y_top,
                    local_y_bottom,
                    stops: line_snapshots
                        .into_iter()
                        .map(|snapshot| RunCaretStop {
                            local_char: snapshot.local_char,
                            local_x: snapshot.local_x,
                            local_y_top: snapshot.local_y_top,
                            height: snapshot.height,
                        })
                        .collect(),
                })
            })
            .collect()
    }

    fn seed_caret_stops_for_ifc_snapshot(&self) -> Vec<RunCaretLine> {
        if let Some(layout) = self.text_layout.as_ref() {
            let effective = self.effective_text();
            return layout
                .visual_caret_lines(&effective)
                .into_iter()
                .map(|line| RunCaretLine {
                    local_y_top: line.y_top,
                    local_y_bottom: line.y_bottom,
                    stops: line
                        .stops
                        .into_iter()
                        .map(|stop| RunCaretStop {
                            local_char: self.effective_char_to_plain_local_char(stop.char_index),
                            local_x: stop.x,
                            local_y_top: line.y_top,
                            height: stop.height,
                        })
                        .collect(),
                })
                .collect();
        }
        self.empty_line_caret_stops()
    }

    fn empty_line_caret_stops(&self) -> Vec<RunCaretLine> {
        let line_height = self.font_size.max(1.0) * self.line_height.max(0.8);
        vec![RunCaretLine {
            local_y_top: 0.0,
            local_y_bottom: line_height,
            stops: vec![RunCaretStop {
                local_char: 0,
                local_x: 0.0,
                local_y_top: 0.0,
                height: line_height,
            }],
        }]
    }

    fn effective_char_to_plain_local_char(&self, effective_char: usize) -> usize {
        if self.is_preedit_run {
            return 0;
        }
        match &self.inline_preedit {
            Some(preedit) => {
                let preedit_len = preedit.preedit_text.chars().count();
                let insert_at = preedit.insert_at_local;
                if effective_char <= insert_at {
                    effective_char
                } else if effective_char <= insert_at + preedit_len {
                    insert_at
                } else {
                    effective_char - preedit_len
                }
            }
            None => effective_char,
        }
    }
}

/// Run-local caret stop produced by [`TextAreaTextRun::caret_stops`].
#[derive(Clone, Debug)]
pub struct RunCaretStop {
    pub local_char: usize,
    pub local_x: f32,
    pub local_y_top: f32,
    pub height: f32,
}

/// One visual line worth of caret stops in run-local coordinates.
#[derive(Clone, Debug)]
pub struct RunCaretLine {
    pub local_y_top: f32,
    pub local_y_bottom: f32,
    pub stops: Vec<RunCaretStop>,
}

pub(crate) struct TextAreaLineBreak {
    pub(crate) char_range: Range<usize>,
    pub(crate) font_size: f32,
    pub(crate) line_height: f32,
    pub(crate) vertical_align: crate::style::VerticalAlign,
    pub(crate) caret_fragments: [Option<Rect>; 2],
    pub(crate) layout_state: LayoutState,
    pub(crate) dirty_flags: DirtyFlags,
    pub(crate) node_id: u64,
    pub(crate) parent_id: Option<u64>,
    pub(crate) children: Vec<NodeKey>,
}

impl TextAreaLineBreak {
    pub(crate) fn new(char_range: Range<usize>) -> Self {
        Self {
            char_range,
            font_size: 14.0,
            line_height: 1.25,
            vertical_align: crate::style::VerticalAlign::Baseline,
            caret_fragments: [None, None],
            layout_state: LayoutState::new(0.0, 0.0, 0.0, 0.0),
            dirty_flags: DirtyFlags::ALL,
            node_id: next_ui_node_id(),
            parent_id: None,
            children: Vec::new(),
        }
    }

    pub(crate) fn set_char_range(&mut self, char_range: Range<usize>) {
        if self.char_range == char_range {
            return;
        }
        self.char_range = char_range;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::LAYOUT);
    }

    pub(crate) fn cascade_style(
        &mut self,
        font_size: f32,
        line_height: f32,
        vertical_align: crate::style::VerticalAlign,
    ) {
        if self.font_size == font_size
            && self.line_height == line_height
            && self.vertical_align == vertical_align
        {
            return;
        }
        self.font_size = font_size;
        self.line_height = line_height;
        self.vertical_align = vertical_align;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::LAYOUT);
    }

    fn line_height_px(&self) -> f32 {
        self.font_size.max(1.0) * self.line_height.max(0.8)
    }

    fn baseline(&self) -> f32 {
        let font_size = self.font_size.max(1.0);
        let line_height = self.line_height_px();
        let approx_ascent = font_size * 0.8779297;
        let leading = (line_height - font_size).max(0.0);
        (approx_ascent + leading / 2.0).max(0.0)
    }

    pub(crate) fn caret_stops(&self) -> Vec<RunCaretLine> {
        let line_height = self.line_height_px();
        self.caret_fragments
            .iter()
            .enumerate()
            .filter_map(|(idx, rect)| {
                let rect = rect.as_ref()?;
                let local_x = rect.x - self.layout_state.layout_position.x;
                let local_y_top = rect.y - self.layout_state.layout_position.y;
                let stops = if idx == 0 {
                    vec![
                        RunCaretStop {
                            local_char: 0,
                            local_x,
                            local_y_top,
                            height: line_height,
                        },
                        RunCaretStop {
                            local_char: 1,
                            local_x,
                            local_y_top,
                            height: line_height,
                        },
                    ]
                } else {
                    vec![RunCaretStop {
                        local_char: 1,
                        local_x,
                        local_y_top,
                        height: line_height,
                    }]
                };
                Some(RunCaretLine {
                    local_y_top,
                    local_y_bottom: local_y_top + line_height,
                    stops,
                })
            })
            .collect()
    }
}

impl Layoutable for TextAreaTextRun {
    fn measure_inline(
        &mut self,
        context: InlineMeasureContext,
        _arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let layout_clean = !self.dirty_flags.intersects(DirtyFlags::LAYOUT);
        if layout_clean
            && self.last_inline_measure_context == Some(context)
            && self.text_layout.is_some()
        {
            return;
        }

        // Shape budget = container's full inner width, not the line's
        // *remaining* width. Otherwise a Run that can't fit in the
        // remaining slot would shape narrow and produce ugly wraps; with
        // `full_available_width` the text engine wraps at the same width as if
        // the Run started on a fresh line, and the inline solver places
        // the Run on the next line if needed.
        let max_width = if self.auto_wrap {
            Some(context.full_available_width.max(1.0))
        } else {
            None
        };
        let (width, height) = if self.text.is_empty() && self.inline_preedit.is_none() {
            // Empty paragraph: skip shaping (which would substitute a
            // space and report a visible glyph width). The Run still claims
            // a `line_height`-tall slot so the inline solver gives it a
            // proper blank line. Floor at 0.8 to match every other line-
            // height path (`line_height_px`, `empty_line_caret_position`,
            // the shaped path's `build_text_layout`) so a blank paragraph
            // and a shaped one report the same height.
            self.text_layout = None;
            (0.0_f32, self.font_size.max(1.0) * self.line_height.max(0.8))
        } else {
            let layout = self.build_run_text_layout(max_width);
            let (w, h) = layout.measure_size();
            self.text_layout = Some(layout);
            (w, h)
        };
        self.last_inline_measure_context = Some(context);

        self.layout_state.layout_size = Size {
            width: width.max(0.0),
            height: height.max(0.0),
        };
        self.layout_state.content_size = self.layout_state.layout_size;
        self.dirty_flags = self
            .dirty_flags
            .without(DirtyFlags::LAYOUT)
            .union(DirtyFlags::PLACE)
            .union(DirtyFlags::BOX_MODEL)
            .union(DirtyFlags::HIT_TEST)
            .union(DirtyFlags::PAINT);
    }

    fn measure(
        &mut self,
        constraints: LayoutConstraints,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        // Block-level measurement falls back to inline layout with the
        // available width as the wrap budget.
        self.measure_inline(
            InlineMeasureContext {
                first_available_width: constraints.max_width,
                full_available_width: constraints.max_width,
                available_height: 1_000_000.0,
                viewport_width: constraints.viewport_width,
                viewport_height: constraints.viewport_height,
                percent_base_width: constraints.percent_base_width,
                percent_base_height: constraints.percent_base_height,
            },
            arena,
        );
    }

    fn place(
        &mut self,
        placement: LayoutPlacement,
        _arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let x = placement.parent_x + placement.visual_offset_x;
        let y = placement.parent_y + placement.visual_offset_y;
        self.layout_state.layout_position = Position { x, y };
        self.layout_state.layout_inner_position = Position { x, y };
        self.layout_state.layout_inner_size = self.layout_state.layout_size;
        self.layout_state.layout_flow_position = self.layout_state.layout_position;
        self.layout_state.layout_flow_inner_position = self.layout_state.layout_inner_position;
        self.inline_paint_fragments.clear();
        self.inline_paint_fragments.push(Rect {
            x,
            y,
            width: self.layout_state.layout_size.width,
            height: self.layout_state.layout_size.height,
        });
        self.dirty_flags = self.dirty_flags.without(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
        );
    }

    fn place_inline(
        &mut self,
        placement: InlinePlacement,
        _arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let fragments = self.inline_line_nodes();
        let Some(fragment) = fragments.get(placement.node_index).copied() else {
            return;
        };
        let x = placement.x;
        let y = placement.y;
        if placement.node_index == 0 {
            self.inline_paint_fragments.clear();
            self.layout_state.layout_position = Position { x, y };
            self.layout_state.layout_size = Size {
                width: 0.0,
                height: 0.0,
            };
            self.layout_state.should_render = false;
        }
        let left = x;
        let top = y;
        let right = x + fragment.width.max(0.0);
        let bottom = y + fragment.height.max(0.0);
        if self.layout_state.should_render {
            let current_right =
                self.layout_state.layout_position.x + self.layout_state.layout_size.width;
            let current_bottom =
                self.layout_state.layout_position.y + self.layout_state.layout_size.height;
            self.layout_state.layout_position.x = self.layout_state.layout_position.x.min(left);
            self.layout_state.layout_position.y = self.layout_state.layout_position.y.min(top);
            self.layout_state.layout_size.width =
                current_right.max(right) - self.layout_state.layout_position.x;
            self.layout_state.layout_size.height =
                current_bottom.max(bottom) - self.layout_state.layout_position.y;
        } else {
            self.layout_state.layout_position = Position { x: left, y: top };
            self.layout_state.layout_size = Size {
                width: (right - left).max(0.0),
                height: (bottom - top).max(0.0),
            };
        }
        self.layout_state.should_render =
            self.layout_state.layout_size.width > 0.0 && self.layout_state.layout_size.height > 0.0;
        self.layout_state.layout_inner_position = Position { x, y };
        self.layout_state.layout_inner_size = self.layout_state.layout_size;
        self.layout_state.layout_flow_position = self.layout_state.layout_position;
        self.layout_state.layout_flow_inner_position = self.layout_state.layout_inner_position;
        self.inline_paint_fragments.push(Rect {
            x,
            y,
            width: fragment.width.max(0.0),
            height: fragment.height.max(0.0),
        });
        self.dirty_flags = self.dirty_flags.without(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
        );
    }

    fn measured_size(&self) -> (f32, f32) {
        (
            self.layout_state.layout_size.width,
            self.layout_state.layout_size.height,
        )
    }

    fn set_layout_width(&mut self, width: f32) {
        self.layout_state.layout_size.width = width.max(0.0);
    }

    fn set_layout_height(&mut self, height: f32) {
        self.layout_state.layout_size.height = height.max(0.0);
    }

    fn get_inline_nodes_size(
        &self,
        _arena: &crate::view::node_arena::NodeArena,
    ) -> Vec<InlineNodeSize> {
        self.inline_line_nodes()
    }

    fn inline_relative_position(&self) -> (f32, f32) {
        (
            self.layout_state.layout_position.x,
            self.layout_state.layout_position.y,
        )
    }
}

impl Layoutable for TextAreaLineBreak {
    fn measure_inline(
        &mut self,
        _context: InlineMeasureContext,
        _arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let line_height = self.line_height_px();
        self.layout_state.layout_size = Size {
            width: 0.0,
            height: line_height,
        };
        self.layout_state.content_size = self.layout_state.layout_size;
        self.dirty_flags = self
            .dirty_flags
            .without(DirtyFlags::LAYOUT)
            .union(DirtyFlags::PLACE)
            .union(DirtyFlags::BOX_MODEL)
            .union(DirtyFlags::HIT_TEST)
            .union(DirtyFlags::PAINT);
    }

    fn measure(
        &mut self,
        constraints: LayoutConstraints,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        self.measure_inline(
            InlineMeasureContext {
                first_available_width: constraints.max_width,
                full_available_width: constraints.max_width,
                available_height: constraints.max_height,
                viewport_width: constraints.viewport_width,
                viewport_height: constraints.viewport_height,
                percent_base_width: constraints.percent_base_width,
                percent_base_height: constraints.percent_base_height,
            },
            arena,
        );
    }

    fn place(
        &mut self,
        placement: LayoutPlacement,
        _arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let x = placement.parent_x + placement.visual_offset_x;
        let y = placement.parent_y + placement.visual_offset_y;
        self.layout_state.layout_position = Position { x, y };
        self.layout_state.layout_inner_position = Position { x, y };
        self.layout_state.layout_flow_position = Position { x, y };
        self.layout_state.layout_flow_inner_position = Position { x, y };
        self.layout_state.should_render = false;
        self.dirty_flags = self.dirty_flags.without(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
        );
    }

    fn place_inline(
        &mut self,
        placement: InlinePlacement,
        _arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let line_height = self.line_height_px();
        let rect = Rect {
            x: placement.x,
            y: placement.y,
            width: 0.0,
            height: line_height,
        };
        if placement.node_index == 0 {
            self.caret_fragments = [None, None];
            self.layout_state.layout_position = Position {
                x: placement.x,
                y: placement.y,
            };
            self.layout_state.layout_size = Size {
                width: 0.0,
                height: line_height,
            };
            self.layout_state.should_render = false;
        }
        if let Some(slot) = self.caret_fragments.get_mut(placement.node_index) {
            *slot = Some(rect);
        }
        let left = self.layout_state.layout_position.x.min(rect.x);
        let top = self.layout_state.layout_position.y.min(rect.y);
        let right =
            (self.layout_state.layout_position.x + self.layout_state.layout_size.width).max(rect.x);
        let bottom = (self.layout_state.layout_position.y + self.layout_state.layout_size.height)
            .max(rect.y + rect.height);
        self.layout_state.layout_position = Position { x: left, y: top };
        self.layout_state.layout_size = Size {
            width: (right - left).max(0.0),
            height: (bottom - top).max(0.0),
        };
        self.layout_state.layout_inner_position = self.layout_state.layout_position;
        self.layout_state.layout_inner_size = self.layout_state.layout_size;
        self.layout_state.layout_flow_position = self.layout_state.layout_position;
        self.layout_state.layout_flow_inner_position = self.layout_state.layout_inner_position;
        self.dirty_flags = self.dirty_flags.without(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
        );
    }

    fn measured_size(&self) -> (f32, f32) {
        (0.0, self.line_height_px())
    }

    fn set_layout_width(&mut self, width: f32) {
        self.layout_state.layout_size.width = width.max(0.0);
    }

    fn set_layout_height(&mut self, height: f32) {
        self.layout_state.layout_size.height = height.max(0.0);
    }

    fn get_inline_nodes_size(
        &self,
        _arena: &crate::view::node_arena::NodeArena,
    ) -> Vec<InlineNodeSize> {
        let line_height = self.line_height_px();
        vec![
            InlineNodeSize {
                width: 0.0,
                height: line_height,
                baseline: self.baseline(),
                vertical_align: self.vertical_align,
                force_break_after: true,
            },
            InlineNodeSize {
                width: 0.0,
                height: line_height,
                baseline: self.baseline(),
                vertical_align: self.vertical_align,
                force_break_after: false,
            },
        ]
    }
}

impl Renderable for TextAreaTextRun {
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        _arena: &mut crate::view::node_arena::NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        if self.text.is_empty() && self.inline_preedit.is_none() {
            return ctx.into_state();
        }
        if self.text_layout.is_none() {
            return ctx.into_state();
        }
        let Some(input_target) = ctx.current_target() else {
            return ctx.into_state();
        };
        let [x, y] = ctx.paint_point(
            self.layout_state.layout_position.x,
            self.layout_state.layout_position.y,
        );
        if let Some(payload) = self.prepared_render_payload([x, y], 0, 1.0) {
            let pass = TextPreparedInputPass::new(
                TextPassPreparedParams {
                    staging_input: payload.text_pass_staging_input,
                    fragments: vec![TextPassPreparedFragment {
                        origin: payload.prepared_candidate.origin,
                        size: payload.prepared_candidate.layout_size,
                    }],
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
            self.dirty_flags = self.dirty_flags.without(DirtyFlags::PAINT);
            return ctx.into_state();
        }
        let fragments = {
            let inline_fragments = self.inline_text_pass_fragments(1.0, ctx.paint_offset());
            if inline_fragments.is_empty() {
                vec![TextPassFragment {
                    content: self.effective_text(),
                    x,
                    y,
                    width: self.layout_state.layout_size.width.max(1.0),
                    height: self.layout_state.layout_size.height.max(1.0),
                    color: self.color.to_rgba_f32(),
                    opacity: 1.0,
                    text_layout: self.text_layout.clone(),
                }]
            } else {
                inline_fragments
            }
        };
        let pass = TextPass::new(
            TextPassParams {
                fragments,
                font_size: self.font_size,
                line_height: self.line_height,
                font_weight: self.font_weight,
                font_families: self.font_families.clone(),
                allow_wrap: false,
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
        self.dirty_flags = self.dirty_flags.without(DirtyFlags::PAINT);
        ctx.into_state()
    }
}

impl Renderable for TextAreaLineBreak {
    fn build(
        &mut self,
        _graph: &mut FrameGraph,
        _arena: &mut crate::view::node_arena::NodeArena,
        ctx: UiBuildContext,
    ) -> BuildState {
        self.dirty_flags = self.dirty_flags.without(DirtyFlags::PAINT);
        ctx.into_state()
    }
}

impl EventTarget for TextAreaTextRun {
    fn cursor(&self) -> Cursor {
        self.cursor
    }
}

impl EventTarget for TextAreaLineBreak {}

impl ElementTrait for TextAreaTextRun {
    fn stable_id(&self) -> u64 {
        self.node_id
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.node_id,
            parent_id: self.parent_id,
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self.layout_state.layout_size.width,
            height: self.layout_state.layout_size.height,
            border_radius: 0.0,
            should_render: self.layout_state.should_render,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn children(&self) -> &[NodeKey] {
        &self.children
    }

    fn parent_id(&self) -> Option<u64> {
        self.parent_id
    }

    fn set_parent_id(&mut self, parent_id: Option<u64>) {
        self.parent_id = parent_id;
    }

    fn local_dirty_flags(&self) -> DirtyFlags {
        self.dirty_flags
    }

    fn clear_local_dirty_flags(&mut self, flags: DirtyFlags) {
        self.dirty_flags = self.dirty_flags.without(flags);
    }

    fn promotion_node_info(&self) -> crate::view::promotion::PromotionNodeInfo {
        crate::view::promotion::PromotionNodeInfo {
            estimated_pass_count: 1,
            opacity: 1.0,
            ..Default::default()
        }
    }

    /// Hash everything that affects the rendered glyph fragment so a
    /// promoted ancestor's `base_signature` dirties on edit / style /
    /// preedit / layout changes. Default `0` would let the ancestor reuse
    /// a stale layer texture.
    fn promotion_self_signature(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.layout_state.should_render.hash(&mut hasher);
        self.layout_state
            .layout_position
            .x
            .to_bits()
            .hash(&mut hasher);
        self.layout_state
            .layout_position
            .y
            .to_bits()
            .hash(&mut hasher);
        self.layout_state
            .layout_size
            .width
            .max(0.0)
            .to_bits()
            .hash(&mut hasher);
        self.layout_state
            .layout_size
            .height
            .max(0.0)
            .to_bits()
            .hash(&mut hasher);
        self.text.hash(&mut hasher);
        self.char_range.start.hash(&mut hasher);
        self.char_range.end.hash(&mut hasher);
        self.is_placeholder.hash(&mut hasher);
        self.font_families.hash(&mut hasher);
        self.font_size.to_bits().hash(&mut hasher);
        self.line_height.to_bits().hash(&mut hasher);
        self.font_weight.hash(&mut hasher);
        self.color.to_rgba_u8().hash(&mut hasher);
        self.auto_wrap.hash(&mut hasher);
        if let Some(preedit) = &self.inline_preedit {
            preedit.insert_at_local.hash(&mut hasher);
            preedit.preedit_text.hash(&mut hasher);
            preedit.preedit_cursor.hash(&mut hasher);
        } else {
            u64::MAX.hash(&mut hasher);
        }
        self.is_preedit_run.hash(&mut hasher);
        self.preedit_cursor.hash(&mut hasher);
        hasher.finish()
    }
}

impl ElementTrait for TextAreaLineBreak {
    fn stable_id(&self) -> u64 {
        self.node_id
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.node_id,
            parent_id: self.parent_id,
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self.layout_state.layout_size.width,
            height: self.layout_state.layout_size.height,
            border_radius: 0.0,
            should_render: false,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn children(&self) -> &[NodeKey] {
        &self.children
    }

    fn parent_id(&self) -> Option<u64> {
        self.parent_id
    }

    fn set_parent_id(&mut self, parent_id: Option<u64>) {
        self.parent_id = parent_id;
    }

    fn local_dirty_flags(&self) -> DirtyFlags {
        self.dirty_flags
    }

    fn clear_local_dirty_flags(&mut self, flags: DirtyFlags) {
        self.dirty_flags = self.dirty_flags.without(flags);
    }

    fn promotion_self_signature(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.char_range.start.hash(&mut hasher);
        self.char_range.end.hash(&mut hasher);
        self.font_size.to_bits().hash(&mut hasher);
        self.line_height.to_bits().hash(&mut hasher);
        self.layout_state
            .layout_position
            .x
            .to_bits()
            .hash(&mut hasher);
        self.layout_state
            .layout_position
            .y
            .to_bits()
            .hash(&mut hasher);
        hasher.finish()
    }
}

/// Round `byte_index` down to the nearest valid UTF-8 char boundary in
/// `value`. Caller protection for IME preedit cursor offsets that may
/// land on a continuation byte.
fn clamp_utf8_boundary(value: &str, mut byte_index: usize) -> usize {
    byte_index = byte_index.min(value.len());
    while byte_index > 0 && !value.is_char_boundary(byte_index) {
        byte_index -= 1;
    }
    byte_index
}

#[cfg(test)]
mod read_only_ifc_rollout_tests {
    use super::*;
    use crate::view::base_component::text::{Text, TextReadOnlyIfcStagingMode};
    use crate::view::base_component::{
        LayoutConstraints, LayoutPlacement, Layoutable, Renderable, UiBuildContext,
    };
    use crate::view::frame_graph::FrameGraph;
    use crate::view::node_arena::NodeArena;

    #[test]
    fn text_read_only_ifc_staging_stays_separate_from_text_area_prepared_default() {
        let mut run =
            TextAreaTextRun::new("text area run uses prepared default".to_string(), 0..35);
        let mut arena = NodeArena::new();
        run.measure(
            LayoutConstraints {
                max_width: 180.0,
                max_height: 120.0,
                viewport_width: 180.0,
                viewport_height: 120.0,
                percent_base_width: Some(180.0),
                percent_base_height: Some(120.0),
            },
            &mut arena,
        );
        run.place(
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 180.0,
                available_height: 120.0,
                viewport_width: 180.0,
                viewport_height: 120.0,
                percent_base_width: Some(180.0),
                percent_base_height: Some(120.0),
            },
            &mut arena,
        );

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(180, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        run.build(&mut graph, &mut arena, ctx);
        let pass_names = graph
            .pass_descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name.to_string())
            .collect::<Vec<_>>();

        assert_eq!(pass_names.len(), 1);
        assert!(
            pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
            "TextAreaTextRun should use the prepared default independently from read-only Text staging, got {}",
            pass_names[0]
        );
    }

    fn place_run_for_inline_ifc_staging_test(
        run: &mut TextAreaTextRun,
        arena: &mut NodeArena,
        width: f32,
    ) {
        run.measure(
            LayoutConstraints {
                max_width: width,
                max_height: 120.0,
                viewport_width: width,
                viewport_height: 120.0,
                percent_base_width: Some(width),
                percent_base_height: Some(120.0),
            },
            arena,
        );
        run.place(
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: width,
                available_height: 120.0,
                viewport_width: width,
                viewport_height: 120.0,
                percent_base_width: Some(width),
                percent_base_height: Some(120.0),
            },
            arena,
        );
    }

    fn build_text_area_run_pass_names(
        run: &mut TextAreaTextRun,
        arena: &mut NodeArena,
        width: u32,
        height: u32,
    ) -> Vec<String> {
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(width, height, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        run.build(&mut graph, arena, ctx);
        graph
            .pass_descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name.to_string())
            .collect::<Vec<_>>()
    }

    fn text_area_inline_ifc_ready_behavior_path_status() -> TextAreaEditableIfcBehaviorPathStatus {
        let mut run = TextAreaTextRun::new(
            "behavior path status observes readiness inputs".to_string(),
            0..63,
        );
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 18,
            preedit_text: "入力".to_string(),
            preedit_cursor: Some((1, 2)),
        }));
        run.set_preedit_run(true, Some((1, 3)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let mut payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose behavior path status metadata");
        payload
            .readiness
            .projection_diagnostic
            .projection_segment_count = 1;
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
            TextAreaEditableIfcCaretAffinityMetadataSourceInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        TextAreaEditableIfcBehaviorPathStatus::evaluate(
            TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
                &preflight,
                &audit,
                &ime_prewire,
                &caret_metadata_source,
                &projection_prewire,
                &scroll_prewire,
            ),
        )
    }

    fn text_area_inline_ifc_behavior_status_and_caret_metadata_source_for_readiness() -> (
        TextAreaEditableIfcBehaviorPathStatus,
        TextAreaEditableIfcCaretAffinityMetadataSource,
    ) {
        let mut run = TextAreaTextRun::new(
            "behavior readiness observes caret affinity candidate metadata".to_string(),
            0..63,
        );
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 12,
            preedit_text: "入力".to_string(),
            preedit_cursor: Some((1, 2)),
        }));
        run.set_preedit_run(true, Some((1, 3)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let mut payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose behavior readiness metadata");
        payload
            .readiness
            .projection_diagnostic
            .projection_segment_count = 1;
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
            TextAreaEditableIfcCaretAffinityMetadataSourceInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let behavior_status = TextAreaEditableIfcBehaviorPathStatus::evaluate(
            TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
                &preflight,
                &audit,
                &ime_prewire,
                &caret_metadata_source,
                &projection_prewire,
                &scroll_prewire,
            ),
        );

        (behavior_status, caret_metadata_source)
    }

    #[test]
    fn text_area_inline_ifc_staging_payload_captures_diagnostic_without_render_enablement() {
        let mut run = TextAreaTextRun::new(
            "editable text area run diagnostic payload wraps".to_string(),
            4..52,
        );
        run.cascade_style(
            vec!["Arial".to_string()],
            16.0,
            1.3,
            crate::style::VerticalAlign::Baseline,
            500,
            crate::style::Color::rgba(33, 44, 55, 255),
            Cursor::Text,
            true,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 92.0);

        let payload = run
            .inline_ifc_staging_payload([7.0, 11.0], 9, 0.75)
            .expect("laid out TextAreaTextRun should expose P7 staging diagnostic payload");

        assert_eq!(payload.char_range, 4..52);
        assert!(!payload.render_enabled);
        assert_eq!(
            payload.fallback,
            TextAreaTextRunInlineIfcRenderFallback::LegacyTextPass
        );
        assert_eq!(payload.bridge_input.content, run.effective_text());
        assert_eq!(payload.bridge_input.origin, [7.0, 11.0]);
        assert_eq!(payload.bridge_input.fragment_index, 9);
        assert_eq!(payload.bridge_input.opacity, 0.75);
        assert_eq!(payload.bridge_input.width_constraint, Some(92.0));
        assert!(payload.bridge_input.allow_wrap);
        assert!(payload.diagnostic.bridge_glyph_count > 0);
        assert_eq!(
            payload.diagnostic.prepared_glyph_count,
            payload.prepared_input.glyphs.len()
        );
        assert_eq!(
            payload.diagnostic.staging_glyph_count,
            payload.text_pass_staging_input.glyphs.len()
        );
        assert_eq!(
            payload.diagnostic.batch_count,
            payload.prepared_input.batches.len()
        );
        assert_eq!(
            payload.text_pass_staging_input.glyphs.len(),
            payload.prepared_input.glyphs.len()
        );
        assert_eq!(payload.prepared_candidate.char_range, 4..52);
        assert_eq!(
            payload.prepared_candidate.fallback,
            TextAreaTextRunInlineIfcRenderFallback::LegacyTextPass
        );
        assert_eq!(payload.prepared_candidate.origin, [7.0, 11.0]);
        assert_eq!(
            payload.prepared_candidate.layout_size,
            payload.diagnostic.layout_size
        );
        assert_eq!(payload.prepared_candidate.width_constraint, Some(92.0));
        assert!(payload.prepared_candidate.allow_wrap);
        assert_eq!(payload.prepared_candidate.opacity, 0.75);
        assert_eq!(payload.prepared_candidate.fragment_index, 9);
        assert_eq!(
            payload.prepared_candidate.glyph_count,
            payload.bridge_package.glyphs.len()
        );
        assert_eq!(
            payload.prepared_candidate.prepared_glyph_count,
            payload.prepared_input.glyphs.len()
        );
        assert_eq!(
            payload.prepared_candidate.staging_glyph_count,
            payload.text_pass_staging_input.glyphs.len()
        );
        assert_eq!(
            payload.prepared_candidate.batch_count,
            payload.prepared_input.batches.len()
        );
        let first_candidate_glyph = payload
            .prepared_candidate
            .glyph_metadata
            .first()
            .expect("prepared candidate should expose glyph metadata");
        let first_prepared_glyph = payload
            .prepared_input
            .glyphs
            .first()
            .expect("prepared input should expose glyph metadata");
        assert_eq!(
            first_candidate_glyph.glyph_index,
            first_prepared_glyph.glyph_index
        );
        assert_eq!(
            first_candidate_glyph.batch_index,
            first_prepared_glyph.batch_index
        );
        assert_eq!(
            first_candidate_glyph.final_paint_pos,
            first_prepared_glyph.final_paint_pos
        );
        assert_eq!(
            first_candidate_glyph.local_pos,
            first_prepared_glyph.paint.local_pos
        );
        assert_eq!(
            first_candidate_glyph.font_data_id,
            first_prepared_glyph.raster.font_data_id
        );
        assert_eq!(
            first_candidate_glyph.font_index,
            first_prepared_glyph.raster.font_index
        );
        assert_eq!(
            first_candidate_glyph.font_size,
            first_prepared_glyph.raster.font_size
        );
        assert_eq!(
            first_candidate_glyph.normalized_coords_hash,
            first_prepared_glyph.raster.normalized_coords_hash
        );
        assert!(first_candidate_glyph.has_raster_key);
        assert!(payload.prepared_input.glyphs.iter().all(|glyph| {
            payload
                .text_pass_staging_input
                .glyphs
                .iter()
                .any(|staging| staging.final_paint_pos == glyph.final_paint_pos)
        }));
        assert!(payload.readiness.editable_text_area_run);
        assert!(!payload.readiness.projection_ifc_path_ready);
        assert!(!payload.readiness.ime_ifc_path_ready);
        assert!(!payload.readiness.caret_affinity_ifc_path_ready);
        assert!(!payload.readiness.scroll_follow_ifc_path_ready);
    }

    #[test]
    fn text_area_inline_ifc_staging_payload_preserves_preedit_boundary_metadata() {
        let mut run = TextAreaTextRun::new("hello world".to_string(), 10..21);
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 5,
            preedit_text: " IME".to_string(),
            preedit_cursor: Some((1, 3)),
        }));
        run.set_preedit_run(true, Some((0, 2)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);

        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 2, 1.0)
            .expect("preedit TextAreaTextRun should still expose diagnostic payload");

        assert_eq!(payload.char_range, 10..21);
        assert_eq!(
            payload.diagnostic.content_len,
            "hello world".chars().count()
        );
        assert_eq!(
            payload.diagnostic.effective_content_len,
            "hello IME world".chars().count()
        );
        assert_eq!(payload.bridge_input.content, "hello IME world");
        assert!(payload.readiness.has_inline_preedit);
        assert!(payload.readiness.is_preedit_run);
        assert_eq!(payload.readiness.preedit_cursor, Some((0, 2)));
        assert_eq!(
            payload.readiness.caret_affinity_diagnostic.preedit_cursor,
            Some((0, 2))
        );
        assert!(
            payload
                .readiness
                .caret_affinity_diagnostic
                .caret_stop_snapshots
                .iter()
                .all(|snapshot| snapshot.local_char == 0),
            "preedit-run caret snapshot seeding keeps root insertion at the API boundary"
        );
        assert!(!payload.render_enabled);
    }

    #[test]
    fn text_area_inline_ifc_default_prepared_render_uses_valid_candidate() {
        let mut run = TextAreaTextRun::new(
            "text area default renders prepared candidate".to_string(),
            0..44,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose prepared candidate");
        assert!(payload.prepared_candidate.prepared_glyph_count > 0);
        assert_eq!(
            payload.prepared_candidate.fallback,
            TextAreaTextRunInlineIfcRenderFallback::LegacyTextPass
        );

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        run.build(&mut graph, &mut arena, ctx);
        let pass_names = graph
            .pass_descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name.to_string())
            .collect::<Vec<_>>();

        assert_eq!(pass_names.len(), 1);
        assert!(
            pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
            "TextAreaTextRun default should emit TextPreparedInputPass for a valid candidate, got {}",
            pass_names[0]
        );
    }

    #[test]
    fn text_area_inline_ifc_default_render_mode_emits_prepared_input_pass() {
        let mut run = TextAreaTextRun::new(
            "default prepared render pass uses TextArea prepared candidate".to_string(),
            0..61,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose prepared candidate");
        assert!(payload.prepared_candidate.prepared_glyph_count > 0);
        assert_eq!(
            payload.prepared_candidate.fallback,
            TextAreaTextRunInlineIfcRenderFallback::LegacyTextPass
        );

        let pass_names = build_text_area_run_pass_names(&mut run, &mut arena, 180, 120);

        assert_eq!(pass_names.len(), 1);
        assert!(
            pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
            "default TextArea prepared render should emit TextPreparedInputPass, got {}",
            pass_names[0]
        );
    }

    #[test]
    fn text_area_inline_ifc_default_prepared_render_falls_back_when_candidate_is_missing() {
        let mut run = TextAreaTextRun::new(
            "default prepared render falls back when candidate is missing".to_string(),
            0..62,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose a candidate shape before the test hook hides it");
        assert!(payload.prepared_candidate.prepared_glyph_count > 0);

        run.force_missing_inline_ifc_prepared_candidate_for_test();
        let pass_names = build_text_area_run_pass_names(&mut run, &mut arena, 180, 120);

        assert_eq!(pass_names.len(), 1);
        assert!(
            pass_names[0].ends_with("render_pass::text_pass::TextPass"),
            "default prepared render with missing candidate must fallback legacy TextPass, got {}",
            pass_names[0]
        );
    }

    #[test]
    fn text_area_inline_ifc_default_prepared_render_does_not_synthesize_candidate_for_empty_or_unlaid_out_runs()
     {
        let unlaid_out = TextAreaTextRun::new("unlaid out".to_string(), 0..9);
        assert!(
            unlaid_out
                .prepared_render_payload([0.0, 0.0], 0, 1.0)
                .is_none()
        );

        let mut empty = TextAreaTextRun::new(String::new(), 0..0);
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut empty, &mut arena, 180.0);
        assert!(empty.prepared_render_payload([0.0, 0.0], 0, 1.0).is_none());
        assert!(build_text_area_run_pass_names(&mut empty, &mut arena, 180, 120).is_empty());
    }

    #[test]
    fn text_area_inline_ifc_default_prepared_render_rejects_invalid_payload() {
        let mut run = TextAreaTextRun::new(
            "default prepared render rejects invalid opacity".to_string(),
            0..48,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);

        assert!(run.prepared_render_payload([0.0, 0.0], 0, 0.0).is_none());
    }

    #[test]
    fn text_area_inline_ifc_staging_payload_stays_separate_from_read_only_text_prepared_path() {
        let mut run = TextAreaTextRun::new("text area adapter stays diagnostic".to_string(), 0..34);
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let run_payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 3, 1.0)
            .expect("TextAreaTextRun should expose diagnostic payload");

        let mut text = Text::new(0.0, 0.0, 180.0, 120.0, "read-only text uses prepared path");
        text.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::RenderPreparedInput);
        let text_staging = text
            .text_read_only_ifc_prepared_staging_input([0.0, 0.0], 3)
            .expect("read-only Text should expose render-enabled prepared staging input");

        assert!(!run_payload.render_enabled);
        assert_eq!(
            run_payload.text_pass_staging_input.glyphs.len(),
            run_payload.prepared_input.glyphs.len()
        );
        assert!(!text_staging.glyphs.is_empty());
        assert_ne!(
            run_payload.bridge_input.content, "read-only text uses prepared path",
            "TextArea adapter payload must remain independent from read-only Text prepared path"
        );
    }

    #[test]
    fn text_area_inline_ifc_staging_payload_is_probe_only_for_empty_or_unlaid_out_runs() {
        let run = TextAreaTextRun::new("unlaid out".to_string(), 0..9);
        assert!(
            run.inline_ifc_staging_payload([0.0, 0.0], 0, 1.0).is_none(),
            "adapter should not synthesize IFC payload before TextArea run layout exists"
        );

        let mut empty = TextAreaTextRun::new(String::new(), 0..0);
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut empty, &mut arena, 180.0);
        assert!(
            empty
                .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
                .is_none(),
            "empty TextAreaTextRun should stay legacy/no-op"
        );
    }

    #[test]
    fn text_area_inline_ifc_evaluation_input_aggregates_staging_payload_diagnostic() {
        let mut run = TextAreaTextRun::new("hello world".to_string(), 10..21);
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 5,
            preedit_text: " IME".to_string(),
            preedit_cursor: Some((1, 3)),
        }));
        run.set_preedit_run(true, Some((0, 2)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([4.0, 6.0], 5, 1.0)
            .expect("laid out TextAreaTextRun should provide staging payload");

        let input = TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload.clone()]);
        assert_eq!(input.run_inputs.len(), 1);
        assert_eq!(input.run_inputs[0].char_range, 10..21);
        assert!(input.run_inputs[0].has_inline_preedit);
        assert!(input.run_inputs[0].is_preedit_run);
        assert_eq!(input.run_inputs[0].preedit_cursor, Some((0, 2)));
        assert_eq!(
            input.run_inputs[0].diagnostic.effective_content_len,
            "hello IME world".chars().count()
        );
        assert_eq!(
            input.run_inputs[0].diagnostic.staging_glyph_count,
            payload.text_pass_staging_input.glyphs.len()
        );
        assert!(input.legacy_fallback_confirmed);
        assert!(input.read_only_text_path_separated);

        let preflight = TextAreaInlineIfcEvaluationPreflight::evaluate(input);
        assert_eq!(
            preflight.state(),
            TextAreaInlineIfcEvaluationPreflightState::Blocked
        );
        assert_eq!(preflight.run_inputs().len(), 1);
        assert!(!preflight.render_enabled());
        assert!(!preflight.layout_enabled());
        assert!(
            !preflight.allows_text_area_editable_behavior_path_switch(),
            "evaluation input is diagnostic-only and must not authorize TextArea rollout"
        );
    }

    #[test]
    fn text_area_inline_ifc_evaluation_preflight_blocks_current_metadata_paths() {
        let mut run = TextAreaTextRun::new(
            "editable run evaluation metadata remains unwired".to_string(),
            0..48,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should provide staging payload");
        let preflight = TextAreaInlineIfcEvaluationPreflight::evaluate(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );

        assert_eq!(
            preflight.state(),
            TextAreaInlineIfcEvaluationPreflightState::Blocked
        );
        for reason in [
            TextAreaInlineIfcEvaluationPreflightBlockedReason::ProjectionPathUnwired,
            TextAreaInlineIfcEvaluationPreflightBlockedReason::ImePathUnwired,
            TextAreaInlineIfcEvaluationPreflightBlockedReason::CaretAffinityPathUnwired,
            TextAreaInlineIfcEvaluationPreflightBlockedReason::ScrollFollowPathUnwired,
        ] {
            assert!(
                preflight.blocked_reasons().contains(&reason),
                "current P7 TextArea evaluation preflight must block on {reason:?}"
            );
        }
        assert!(
            !preflight
                .blocked_reasons()
                .contains(&TextAreaInlineIfcEvaluationPreflightBlockedReason::MissingRunPayload),
            "laid out run payload should satisfy the run payload requirement"
        );
        assert!(
            !preflight.blocked_reasons().contains(
                &TextAreaInlineIfcEvaluationPreflightBlockedReason::LegacyFallbackMissing
            ),
            "TextArea staging payload confirms correctness fallback to legacy TextPass"
        );
        assert!(
            !preflight.blocked_reasons().contains(
                &TextAreaInlineIfcEvaluationPreflightBlockedReason::
                    ReadOnlyTextPathSeparationMissing
            ),
            "TextArea staging payload remains separated from read-only Text prepared path"
        );
    }

    #[test]
    fn text_area_inline_ifc_evaluation_preflight_blocks_missing_or_unlaid_out_run_payload() {
        let run = TextAreaTextRun::new("unlaid out evaluation".to_string(), 0..21);
        let payloads = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .into_iter()
            .collect::<Vec<_>>();
        let preflight = TextAreaInlineIfcEvaluationPreflight::evaluate(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(payloads),
        );

        assert_eq!(
            preflight.state(),
            TextAreaInlineIfcEvaluationPreflightState::Blocked
        );
        assert!(
            preflight
                .blocked_reasons()
                .contains(&TextAreaInlineIfcEvaluationPreflightBlockedReason::MissingRunPayload)
        );
        assert!(
            preflight.blocked_reasons().contains(
                &TextAreaInlineIfcEvaluationPreflightBlockedReason::LegacyFallbackMissing
            )
        );
        assert!(preflight.run_inputs().is_empty());
        assert!(!preflight.render_enabled());
        assert!(!preflight.layout_enabled());
    }

    #[test]
    fn text_area_inline_ifc_evaluation_preflight_requires_read_only_path_separation() {
        let mut run =
            TextAreaTextRun::new("text area evaluation stays separated".to_string(), 0..36);
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should provide staging payload");
        let mut input = TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]);
        input.read_only_text_path_separated = false;

        let preflight = TextAreaInlineIfcEvaluationPreflight::evaluate(input);

        assert_eq!(
            preflight.state(),
            TextAreaInlineIfcEvaluationPreflightState::Blocked
        );
        assert!(preflight.blocked_reasons().contains(
            &TextAreaInlineIfcEvaluationPreflightBlockedReason::ReadOnlyTextPathSeparationMissing
        ));
        assert!(!preflight.render_enabled());
        assert!(
            !preflight.allows_text_area_editable_behavior_path_switch(),
            "missing read-only Text separation blocks evaluation and still never authorizes rollout"
        );
    }

    #[test]
    fn text_area_inline_ifc_metadata_bridge_builds_from_evaluation_input() {
        let mut run = TextAreaTextRun::new("bridge keeps run metadata".to_string(), 20..45);
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 6,
            preedit_text: " IME".to_string(),
            preedit_cursor: Some((1, 2)),
        }));
        run.set_preedit_run(true, Some((0, 1)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([2.0, 3.0], 4, 1.0)
            .expect("laid out run should expose staging payload for metadata bridge");
        let input = TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload.clone()]);

        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(input);

        assert_eq!(bridge.run_metadata.len(), 1);
        assert_eq!(bridge.run_metadata[0].char_range, 20..45);
        assert!(bridge.run_metadata[0].has_inline_preedit);
        assert!(bridge.run_metadata[0].is_preedit_run);
        assert_eq!(bridge.run_metadata[0].preedit_cursor, Some((0, 1)));
        assert_eq!(
            bridge.run_metadata[0].diagnostic.staging_glyph_count,
            payload.text_pass_staging_input.glyphs.len()
        );
        let ime_diagnostic = bridge
            .ime_metadata_diagnostic
            .as_ref()
            .expect("laid out run metadata should expose IME bridge diagnostic");
        assert_eq!(ime_diagnostic.run_count, 1);
        assert!(ime_diagnostic.has_inline_preedit);
        assert!(ime_diagnostic.has_preedit_run);
        assert_eq!(ime_diagnostic.preedit_cursor_count, 1);
        assert_eq!(
            ime_diagnostic.effective_content_len,
            "bridge IME keeps run metadata".chars().count()
        );
        assert_eq!(
            bridge.projection_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedNoProjectionSegments
        );
        let projection_diagnostic = bridge
            .projection_metadata_diagnostic
            .as_ref()
            .expect("laid out run metadata should expose projection bridge diagnostic");
        assert_eq!(projection_diagnostic.run_count, 1);
        assert_eq!(projection_diagnostic.char_range_count, 1);
        assert_eq!(projection_diagnostic.char_span, 25);
        assert_eq!(projection_diagnostic.projection_segment_count, 0);
        assert_eq!(projection_diagnostic.inline_preedit_run_count, 1);
        assert_eq!(projection_diagnostic.preedit_run_count, 1);
        assert_eq!(
            bridge.ime_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::Observed
        );
        assert_eq!(
            bridge.caret_affinity_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
        );
        let caret_diagnostic = bridge
            .caret_affinity_metadata_diagnostic
            .as_ref()
            .expect("laid out run metadata should expose caret affinity bridge diagnostic");
        assert_eq!(caret_diagnostic.run_count, 1);
        assert!(caret_diagnostic.visual_line_count >= 1);
        assert!(caret_diagnostic.caret_stop_count >= 1);
        assert_eq!(caret_diagnostic.preedit_cursor_count, 1);
        assert_eq!(
            bridge.scroll_follow_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::Observed
        );
        let scroll_diagnostic = bridge
            .scroll_follow_metadata_diagnostic
            .as_ref()
            .expect("laid out run metadata should expose scroll-follow bridge diagnostic");
        assert_eq!(scroll_diagnostic.run_count, 1);
        assert_eq!(scroll_diagnostic.layout_size_count, 1);
        assert_eq!(scroll_diagnostic.char_span, 25);
        assert!(scroll_diagnostic.visual_line_count >= 1);
        assert!(scroll_diagnostic.caret_stop_count >= 1);
        assert!(bridge.legacy_fallback_confirmed);
        assert!(bridge.read_only_text_path_separated);
    }

    #[test]
    fn text_area_inline_ifc_metadata_bridge_observes_ime_preedit_from_run_payload() {
        let mut run = TextAreaTextRun::new("hello world".to_string(), 10..21);
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 5,
            preedit_text: " IME".to_string(),
            preedit_cursor: Some((1, 3)),
        }));
        run.set_preedit_run(true, Some((0, 2)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([1.0, 2.0], 7, 1.0)
            .expect("preedit run should expose metadata bridge payload");

        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

        assert_eq!(
            preflight.bridge_input().ime_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::Observed
        );
        let ime_diagnostic = preflight
            .bridge_input()
            .ime_metadata_diagnostic
            .as_ref()
            .expect("IME metadata observation should preserve aggregate diagnostic");
        assert!(ime_diagnostic.has_inline_preedit);
        assert!(ime_diagnostic.has_preedit_run);
        assert_eq!(ime_diagnostic.preedit_cursor_count, 1);
        assert_eq!(
            ime_diagnostic.effective_content_len,
            "hello IME world".chars().count()
        );
        assert_eq!(preflight.bridge_input().run_metadata[0].char_range, 10..21);
        assert!(preflight.bridge_input().run_metadata[0].has_inline_preedit);
        assert!(preflight.bridge_input().run_metadata[0].is_preedit_run);
        assert_eq!(
            preflight.bridge_input().run_metadata[0].preedit_cursor,
            Some((0, 2))
        );
        assert_eq!(
            preflight.state(),
            TextAreaInlineIfcMetadataBridgePreflightState::ReadyForDiagnosticEvaluation
        );
        assert!(
            !preflight
                .blocked_reasons()
                .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ImeMetadataUnwired)
        );
        assert!(
            !preflight
                .blocked_reasons()
                .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ProjectionMetadataUnwired),
            "laid out run projection metadata should now be observed diagnostically"
        );
        assert!(
            !preflight.blocked_reasons().contains(
                &TextAreaInlineIfcMetadataBridgeBlockedReason::ScrollFollowMetadataUnwired
            ),
            "laid out run scroll-follow metadata should now be observed diagnostically"
        );
        assert!(
            !preflight.blocked_reasons().contains(
                &TextAreaInlineIfcMetadataBridgeBlockedReason::CaretAffinityMetadataUnwired
            ),
            "laid out run caret metadata should now be observed diagnostically"
        );
        assert!(!preflight.render_enabled());
        assert!(!preflight.layout_enabled());
        assert!(!preflight.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_metadata_bridge_observes_no_preedit_without_synthesizing_one() {
        let mut run = TextAreaTextRun::new("plain editable metadata".to_string(), 0..23);
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("plain laid out run should expose metadata bridge payload");

        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

        assert_eq!(
            preflight.bridge_input().ime_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedNoPreedit
        );
        let ime_diagnostic = preflight
            .bridge_input()
            .ime_metadata_diagnostic
            .as_ref()
            .expect("plain run should still expose a no-preedit IME observation");
        assert!(!ime_diagnostic.has_inline_preedit);
        assert!(!ime_diagnostic.has_preedit_run);
        assert_eq!(ime_diagnostic.preedit_cursor_count, 0);
        assert_eq!(
            ime_diagnostic.effective_content_len,
            "plain editable metadata".chars().count()
        );
        assert!(!preflight.bridge_input().run_metadata[0].has_inline_preedit);
        assert!(!preflight.bridge_input().run_metadata[0].is_preedit_run);
        assert_eq!(
            preflight.bridge_input().run_metadata[0].preedit_cursor,
            None
        );
        assert_eq!(
            preflight.state(),
            TextAreaInlineIfcMetadataBridgePreflightState::ReadyForDiagnosticEvaluation
        );
        assert!(
            !preflight
                .blocked_reasons()
                .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ImeMetadataUnwired)
        );
        assert!(
            !preflight
                .blocked_reasons()
                .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ProjectionMetadataUnwired),
            "plain laid out run projection metadata should now be observed diagnostically"
        );
        assert!(
            !preflight.blocked_reasons().contains(
                &TextAreaInlineIfcMetadataBridgeBlockedReason::ScrollFollowMetadataUnwired
            ),
            "no-preedit run scroll-follow metadata should now be observed diagnostically"
        );
        assert!(
            !preflight.blocked_reasons().contains(
                &TextAreaInlineIfcMetadataBridgeBlockedReason::CaretAffinityMetadataUnwired
            ),
            "plain laid out run caret metadata should now be observed diagnostically"
        );
        assert!(!preflight.render_enabled());
        assert!(!preflight.layout_enabled());
        assert!(!preflight.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_metadata_bridge_observes_projection_metadata_from_laid_out_run() {
        let mut run = TextAreaTextRun::new(
            "projection metadata source is a plain laid out run".to_string(),
            40..92,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 140.0);
        let payload = run
            .inline_ifc_staging_payload([5.0, 6.0], 9, 1.0)
            .expect("laid out run should expose projection metadata bridge payload");

        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

        assert_eq!(
            preflight.bridge_input().projection_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedNoProjectionSegments
        );
        let projection_diagnostic = preflight
            .bridge_input()
            .projection_metadata_diagnostic
            .as_ref()
            .expect("projection metadata observation should preserve aggregate diagnostic");
        assert_eq!(projection_diagnostic.run_count, 1);
        assert_eq!(projection_diagnostic.char_range_count, 1);
        assert_eq!(projection_diagnostic.char_span, 52);
        assert_eq!(
            projection_diagnostic.effective_content_len,
            "projection metadata source is a plain laid out run"
                .chars()
                .count()
        );
        assert_eq!(projection_diagnostic.projection_segment_count, 0);
        assert!(
            !preflight
                .blocked_reasons()
                .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ProjectionMetadataUnwired),
            "projection metadata source should be observed even when no projection segments exist"
        );
        assert!(
            !preflight.blocked_reasons().contains(
                &TextAreaInlineIfcMetadataBridgeBlockedReason::ScrollFollowMetadataUnwired
            ),
            "projection metadata observation should preserve scroll-follow diagnostic metadata"
        );
        assert!(!preflight.render_enabled());
        assert!(!preflight.layout_enabled());
        assert!(!preflight.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_metadata_bridge_observes_caret_affinity_stops_from_laid_out_run() {
        let mut run = TextAreaTextRun::new(
            "caret affinity metadata observes laid out stops".to_string(),
            30..77,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 120.0);
        let payload = run
            .inline_ifc_staging_payload([3.0, 4.0], 2, 1.0)
            .expect("laid out run should expose caret metadata bridge payload");

        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

        assert_eq!(
            preflight.bridge_input().caret_affinity_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
        );
        let caret_diagnostic = preflight
            .bridge_input()
            .caret_affinity_metadata_diagnostic
            .as_ref()
            .expect("caret metadata observation should preserve aggregate diagnostic");
        assert_eq!(caret_diagnostic.run_count, 1);
        assert!(caret_diagnostic.visual_line_count >= 1);
        assert!(caret_diagnostic.caret_stop_count >= 1);
        assert!(
            caret_diagnostic.multi_stop_line_count >= 1,
            "non-empty laid out text should expose at least one visual line with caret stops"
        );
        assert_eq!(caret_diagnostic.preedit_cursor_count, 0);
        assert!(
            !preflight.blocked_reasons().contains(
                &TextAreaInlineIfcMetadataBridgeBlockedReason::CaretAffinityMetadataUnwired
            ),
            "caret stops are now observed as diagnostic metadata"
        );
        assert!(
            !preflight
                .blocked_reasons()
                .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ProjectionMetadataUnwired),
            "caret/projection metadata observation should no longer report projection unwired"
        );
        assert!(
            !preflight.blocked_reasons().contains(
                &TextAreaInlineIfcMetadataBridgeBlockedReason::ScrollFollowMetadataUnwired
            ),
            "caret/projection metadata observation should preserve scroll-follow diagnostic metadata"
        );
        assert!(!preflight.render_enabled());
        assert!(!preflight.layout_enabled());
        assert!(!preflight.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_metadata_bridge_keeps_preedit_cursor_in_caret_metadata() {
        let mut run = TextAreaTextRun::new("preedit cursor stays observable".to_string(), 5..35);
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 7,
            preedit_text: "編輯".to_string(),
            preedit_cursor: Some((3, 6)),
        }));
        run.set_preedit_run(true, Some((1, 2)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 3, 1.0)
            .expect("preedit run should expose caret metadata bridge payload");

        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

        assert_eq!(
            preflight.bridge_input().ime_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::Observed
        );
        assert_eq!(
            preflight.bridge_input().caret_affinity_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
        );
        assert_eq!(
            preflight.bridge_input().run_metadata[0].preedit_cursor,
            Some((1, 2))
        );
        assert_eq!(
            preflight.bridge_input().run_metadata[0]
                .caret_affinity_diagnostic
                .preedit_cursor,
            Some((1, 2))
        );
        assert_eq!(
            preflight
                .bridge_input()
                .caret_affinity_metadata_diagnostic
                .as_ref()
                .expect("preedit run should preserve caret metadata diagnostic")
                .preedit_cursor_count,
            1
        );
        assert_eq!(
            preflight.state(),
            TextAreaInlineIfcMetadataBridgePreflightState::ReadyForDiagnosticEvaluation
        );
        assert!(!preflight.render_enabled());
        assert!(!preflight.layout_enabled());
        assert!(!preflight.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_metadata_bridge_does_not_fake_affinity_slots_when_missing() {
        let mut run = TextAreaTextRun::new("caret metadata can be absent".to_string(), 0..28);
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
        let mut payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose metadata bridge payload");
        payload
            .readiness
            .caret_affinity_diagnostic
            .visual_line_count = 0;
        payload.readiness.caret_affinity_diagnostic.caret_stop_count = 0;
        payload
            .readiness
            .caret_affinity_diagnostic
            .multi_stop_line_count = 0;

        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

        assert_eq!(
            preflight.bridge_input().caret_affinity_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedNoAffinitySlots
        );
        let caret_diagnostic = preflight
            .bridge_input()
            .caret_affinity_metadata_diagnostic
            .as_ref()
            .expect("missing slots should still be an explicit diagnostic observation");
        assert_eq!(caret_diagnostic.caret_stop_count, 0);
        assert_eq!(caret_diagnostic.visual_line_count, 0);
        assert_eq!(
            preflight.state(),
            TextAreaInlineIfcMetadataBridgePreflightState::ReadyForDiagnosticEvaluation
        );
        assert!(
            !preflight.blocked_reasons().contains(
                &TextAreaInlineIfcMetadataBridgeBlockedReason::CaretAffinityMetadataUnwired
            ),
            "missing slots should not be mislabeled as an unwired metadata source"
        );
        assert!(
            !preflight
                .blocked_reasons()
                .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ProjectionMetadataUnwired),
            "missing caret slots must not hide observed projection metadata"
        );
        assert!(
            !preflight.blocked_reasons().contains(
                &TextAreaInlineIfcMetadataBridgeBlockedReason::ScrollFollowMetadataUnwired
            )
        );
        assert!(!preflight.render_enabled());
        assert!(!preflight.layout_enabled());
        assert!(!preflight.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_metadata_bridge_observes_scroll_follow_metadata_from_laid_out_run() {
        let mut run = TextAreaTextRun::new(
            "metadata bridge current state remains diagnostic only".to_string(),
            0..53,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose metadata bridge payload");
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );

        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

        assert_eq!(
            preflight.state(),
            TextAreaInlineIfcMetadataBridgePreflightState::ReadyForDiagnosticEvaluation
        );
        assert_eq!(
            preflight.bridge_input().scroll_follow_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::Observed
        );
        let scroll_diagnostic = preflight
            .bridge_input()
            .scroll_follow_metadata_diagnostic
            .as_ref()
            .expect("laid out run should expose scroll-follow aggregate diagnostic");
        assert_eq!(scroll_diagnostic.run_count, 1);
        assert_eq!(scroll_diagnostic.layout_size_count, 1);
        assert_eq!(scroll_diagnostic.char_span, 53);
        assert_eq!(
            scroll_diagnostic.effective_content_len,
            "metadata bridge current state remains diagnostic only"
                .chars()
                .count()
        );
        assert!(scroll_diagnostic.visual_line_count >= 1);
        assert!(scroll_diagnostic.caret_stop_count >= 1);
        assert!(
            !preflight
                .blocked_reasons()
                .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ProjectionMetadataUnwired),
            "laid out run projection metadata should be observed without marking projection behavior ready"
        );
        assert!(
            !preflight.blocked_reasons().contains(
                &TextAreaInlineIfcMetadataBridgeBlockedReason::ScrollFollowMetadataUnwired
            ),
            "scroll-follow metadata source should be observed without marking behavior ready"
        );
        assert!(
            !preflight.blocked_reasons().contains(
                &TextAreaInlineIfcMetadataBridgeBlockedReason::CaretAffinityMetadataUnwired
            ),
            "laid out run caret metadata should be observed without marking caret behavior ready"
        );
        assert!(
            !preflight
                .blocked_reasons()
                .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::MissingRunMetadata),
            "laid out run should satisfy metadata bridge run diagnostics"
        );
        assert!(
            !preflight
                .blocked_reasons()
                .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ImeMetadataUnwired),
            "laid out run metadata now observes the IME/preedit bridge path"
        );
        assert!(
            !preflight
                .blocked_reasons()
                .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::LegacyFallbackUnconfirmed),
            "TextArea bridge confirms correctness fallback to legacy TextPass"
        );
        assert!(
            !preflight.blocked_reasons().contains(
                &TextAreaInlineIfcMetadataBridgeBlockedReason::
                    ReadOnlyTextPathSeparationUnconfirmed
            ),
            "TextArea bridge remains separated from read-only Text prepared path"
        );
        assert!(!preflight.render_enabled());
        assert!(!preflight.layout_enabled());
        assert!(!preflight.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_metadata_bridge_does_not_fake_scroll_follow_source_when_missing() {
        let mut run = TextAreaTextRun::new("scroll metadata can be absent".to_string(), 0..29);
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
        let mut payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose metadata bridge payload");
        payload.readiness.scroll_follow_diagnostic.layout_size = [0.0, 0.0];
        payload.readiness.scroll_follow_diagnostic.visual_line_count = 0;
        payload.readiness.scroll_follow_diagnostic.caret_stop_count = 0;

        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

        assert_eq!(
            preflight.bridge_input().scroll_follow_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedNoScrollSource
        );
        let scroll_diagnostic = preflight
            .bridge_input()
            .scroll_follow_metadata_diagnostic
            .as_ref()
            .expect("missing scroll source should still be an explicit diagnostic observation");
        assert_eq!(scroll_diagnostic.layout_size_count, 0);
        assert_eq!(scroll_diagnostic.visual_line_count, 0);
        assert_eq!(scroll_diagnostic.caret_stop_count, 0);
        assert!(
            !preflight.blocked_reasons().contains(
                &TextAreaInlineIfcMetadataBridgeBlockedReason::ScrollFollowMetadataUnwired
            ),
            "missing scroll source should not be mislabeled as an unwired metadata source"
        );
        assert!(!preflight.render_enabled());
        assert!(!preflight.layout_enabled());
        assert!(!preflight.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_metadata_bridge_observed_placeholders_still_do_not_enable_rollout() {
        let mut run = TextAreaTextRun::new(
            "observed placeholders are still rollout blocked".to_string(),
            0..47,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose metadata bridge payload");
        let mut bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        bridge.projection_metadata_status = TextAreaInlineIfcMetadataBridgeStatus::Observed;
        bridge.ime_metadata_status = TextAreaInlineIfcMetadataBridgeStatus::Observed;
        bridge.caret_affinity_metadata_status = TextAreaInlineIfcMetadataBridgeStatus::Observed;
        bridge.scroll_follow_metadata_status = TextAreaInlineIfcMetadataBridgeStatus::Observed;

        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

        assert_eq!(
            preflight.state(),
            TextAreaInlineIfcMetadataBridgePreflightState::ReadyForDiagnosticEvaluation
        );
        assert!(preflight.blocked_reasons().is_empty());
        assert_eq!(preflight.bridge_input().run_metadata.len(), 1);
        assert!(
            !preflight.render_enabled(),
            "metadata bridge observation must not enable TextArea prepared rendering"
        );
        assert!(
            !preflight.layout_enabled(),
            "metadata bridge observation must not switch TextArea layout"
        );
        assert!(
            !preflight.allows_text_area_editable_behavior_path_switch(),
            "observed placeholder metadata is only diagnostic and must not authorize default behavior path switch"
        );
    }

    #[test]
    fn text_area_inline_ifc_readiness_audit_keeps_observed_metadata_behavior_blocked() {
        let mut run = TextAreaTextRun::new(
            "readiness audit sees metadata but behavior remains blocked".to_string(),
            0..57,
        );
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 8,
            preedit_text: " IME".to_string(),
            preedit_cursor: Some((1, 2)),
        }));
        run.set_preedit_run(true, Some((0, 1)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose metadata bridge payload");
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );

        assert_eq!(
            audit.metadata_observation_readiness(),
            TextAreaEditableIfcMetadataObservationReadiness::Ready
        );
        assert_eq!(
            audit.state(),
            TextAreaEditableIfcReadinessAuditState::Blocked
        );
        assert!(audit.metadata_blocked_reasons().is_empty());
        for reason in [
            TextAreaEditableIfcBehaviorPathBlockedReason::ProjectionPathUnwired,
            TextAreaEditableIfcBehaviorPathBlockedReason::ImePathUnwired,
            TextAreaEditableIfcBehaviorPathBlockedReason::CaretAffinityPathUnwired,
            TextAreaEditableIfcBehaviorPathBlockedReason::ScrollFollowPathUnwired,
        ] {
            assert!(
                audit.behavior_path_blocked_reasons().contains(&reason),
                "TextArea editable IFC audit must keep behavior path blocked on {reason:?}"
            );
        }
        assert_eq!(
            audit.recommendation(),
            TextAreaEditableIfcBehaviorPathRecommendation::KeepLegacyEditableBehaviorPath
        );
        assert!(!audit.render_enabled());
        assert!(!audit.layout_enabled());
        assert!(!audit.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_readiness_audit_records_no_source_metadata_without_behavior_ready() {
        let mut run = TextAreaTextRun::new(
            "audit no source metadata remains blocked".to_string(),
            0..40,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
        let mut payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose metadata bridge payload");
        payload
            .readiness
            .caret_affinity_diagnostic
            .visual_line_count = 0;
        payload.readiness.caret_affinity_diagnostic.caret_stop_count = 0;
        payload.readiness.scroll_follow_diagnostic.layout_size = [0.0, 0.0];
        payload.readiness.scroll_follow_diagnostic.visual_line_count = 0;
        payload.readiness.scroll_follow_diagnostic.caret_stop_count = 0;
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );

        assert_eq!(
            audit.input().caret_affinity_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedNoAffinitySlots
        );
        assert_eq!(
            audit.input().scroll_follow_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedNoScrollSource
        );
        assert_eq!(
            audit.metadata_observation_readiness(),
            TextAreaEditableIfcMetadataObservationReadiness::Ready
        );
        assert!(
            audit
                .behavior_path_blocked_reasons()
                .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::CaretAffinityPathUnwired),
            "no affinity slots are metadata-observed but behavior path is still unwired"
        );
        assert!(
            audit
                .behavior_path_blocked_reasons()
                .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::ScrollFollowPathUnwired),
            "no scroll source is metadata-observed but behavior path is still unwired"
        );
        assert!(!audit.render_enabled());
        assert!(!audit.layout_enabled());
        assert!(!audit.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_readiness_audit_records_missing_metadata_as_incomplete() {
        let run = TextAreaTextRun::new("unlaid out audit".to_string(), 0..16);
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(
                run.inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
                    .into_iter()
                    .collect(),
            ),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );

        assert_eq!(
            audit.metadata_observation_readiness(),
            TextAreaEditableIfcMetadataObservationReadiness::Incomplete
        );
        assert!(
            audit
                .metadata_blocked_reasons()
                .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::MissingRunMetadata)
        );
        assert_eq!(audit.input().run_metadata_count, 0);
        assert!(!audit.input().projection_metadata_diagnostic_present);
        assert!(!audit.input().ime_metadata_diagnostic_present);
        assert!(!audit.input().caret_affinity_metadata_diagnostic_present);
        assert!(!audit.input().scroll_follow_metadata_diagnostic_present);
        assert!(!audit.render_enabled());
        assert!(!audit.layout_enabled());
        assert!(!audit.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_ime_behavior_prewire_observes_preedit_metadata_and_cursor() {
        let mut run = TextAreaTextRun::new("ime behavior path reads preedit".to_string(), 0..31);
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 4,
            preedit_text: "入力".to_string(),
            preedit_cursor: Some((2, 6)),
        }));
        run.set_preedit_run(true, Some((1, 3)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out preedit run should expose IME metadata source metadata");
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );

        let prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            prewire.state(),
            TextAreaEditableIfcImeBehaviorPathPrewireState::DiagnosticPrewired
        );
        assert!(prewire.blocked_reasons().is_empty());
        assert!(prewire.diagnostic_prewired());
        let diagnostic = prewire
            .diagnostic()
            .expect("preedit IME metadata source should preserve diagnostic metadata");
        assert_eq!(
            diagnostic.ime_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::Observed
        );
        assert_eq!(diagnostic.run_count, 1);
        assert!(diagnostic.has_inline_preedit);
        assert!(diagnostic.has_preedit_run);
        assert_eq!(diagnostic.preedit_cursor_count, 1);
        assert_eq!(diagnostic.preedit_cursors, vec![(1, 3)]);
        assert_eq!(
            diagnostic.effective_content_len,
            "ime 入力behavior path reads preedit".chars().count()
        );
        assert!(
            prewire
                .input()
                .readiness_behavior_blocked_reasons
                .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::ImePathUnwired),
            "#45 metadata source must not rewrite the #44 behavior-path audit"
        );
        assert!(!prewire.ime_behavior_path_ready());
        assert!(!prewire.render_enabled());
        assert!(!prewire.layout_enabled());
        assert!(!prewire.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_ime_behavior_prewire_observes_no_preedit_without_faking_ready() {
        let mut run = TextAreaTextRun::new("plain IME metadata source".to_string(), 0..26);
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out no-preedit run should expose IME metadata source metadata");
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );

        let prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            prewire.state(),
            TextAreaEditableIfcImeBehaviorPathPrewireState::ObservedNoPreedit
        );
        assert!(prewire.diagnostic_prewired());
        let diagnostic = prewire
            .diagnostic()
            .expect("no-preedit IME metadata source should keep explicit diagnostic metadata");
        assert_eq!(
            diagnostic.ime_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedNoPreedit
        );
        assert!(!diagnostic.has_inline_preedit);
        assert!(!diagnostic.has_preedit_run);
        assert_eq!(diagnostic.preedit_cursor_count, 0);
        assert!(diagnostic.preedit_cursors.is_empty());
        assert!(!prewire.ime_behavior_path_ready());
        assert!(!prewire.render_enabled());
        assert!(!prewire.layout_enabled());
        assert!(!prewire.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_ime_behavior_prewire_blocks_missing_metadata_source() {
        let run = TextAreaTextRun::new("unlaid out IME prewire".to_string(), 0..22);
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(
                run.inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
                    .into_iter()
                    .collect(),
            ),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );

        let prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            prewire.state(),
            TextAreaEditableIfcImeBehaviorPathPrewireState::Blocked
        );
        for reason in [
            TextAreaEditableIfcImeBehaviorPathPrewireBlockedReason::MissingRunMetadata,
            TextAreaEditableIfcImeBehaviorPathPrewireBlockedReason::ImeMetadataUnwired,
            TextAreaEditableIfcImeBehaviorPathPrewireBlockedReason::MissingImeMetadataDiagnostic,
        ] {
            assert!(
                prewire.blocked_reasons().contains(&reason),
                "missing IME prewire metadata should block on {reason:?}"
            );
        }
        assert!(!prewire.diagnostic_prewired());
        assert!(prewire.diagnostic().is_none());
        assert!(!prewire.ime_behavior_path_ready());
        assert!(!prewire.render_enabled());
        assert!(!prewire.layout_enabled());
        assert!(!prewire.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_ime_behavior_prewire_keeps_readiness_audit_diagnostic_only() {
        let mut run =
            TextAreaTextRun::new("IME prewire does not authorize rollout".to_string(), 0..39);
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 3,
            preedit_text: "IME".to_string(),
            preedit_cursor: Some((0, 3)),
        }));
        run.set_preedit_run(true, Some((0, 1)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out preedit run should expose IME prewire metadata");
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            audit.state(),
            TextAreaEditableIfcReadinessAuditState::Blocked
        );
        assert!(
            audit
                .behavior_path_blocked_reasons()
                .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::ImePathUnwired),
            "readiness audit must keep IME behavior path blocked after metadata source"
        );
        assert_eq!(
            audit.recommendation(),
            TextAreaEditableIfcBehaviorPathRecommendation::KeepLegacyEditableBehaviorPath
        );
        assert!(prewire.diagnostic_prewired());
        assert!(!audit.render_enabled());
        assert!(!audit.layout_enabled());
        assert!(!audit.allows_text_area_editable_behavior_path_switch());
        assert!(!prewire.render_enabled());
        assert!(!prewire.layout_enabled());
        assert!(!prewire.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_metadata_source_observes_caret_stop_metadata() {
        let mut run = TextAreaTextRun::new(
            "caret affinity behavior path reads visual stops".to_string(),
            0..47,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose caret affinity metadata source metadata");
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );

        let metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
            TextAreaEditableIfcCaretAffinityMetadataSourceInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            metadata_source.state(),
            TextAreaEditableIfcCaretAffinityMetadataSourceState::MetadataObserved
        );
        assert!(metadata_source.blocked_reasons().is_empty());
        assert!(metadata_source.metadata_observed());
        let diagnostic = metadata_source
            .diagnostic()
            .expect("caret affinity metadata source should preserve diagnostic metadata");
        assert_eq!(
            diagnostic.caret_affinity_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
        );
        assert_eq!(diagnostic.run_count, 1);
        assert!(diagnostic.visual_line_count > 0);
        assert!(diagnostic.caret_stop_count > 0);
        assert!(
            diagnostic.multi_stop_line_count > 0,
            "laid out text should expose at least one line with multiple affinity stops"
        );
        assert_eq!(diagnostic.preedit_cursor_count, 0);
        assert_eq!(diagnostic.per_run_caret_diagnostics.len(), 1);
        assert_eq!(
            diagnostic.per_run_caret_diagnostics[0].caret_stop_count,
            diagnostic.caret_stop_count
        );
        assert!(
            metadata_source
                .input()
                .readiness_behavior_blocked_reasons
                .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::CaretAffinityPathUnwired),
            "#46 metadata source must not rewrite the #44 behavior-path audit"
        );
        assert!(!metadata_source.caret_affinity_behavior_path_ready());
        assert!(!metadata_source.render_enabled());
        assert!(!metadata_source.layout_enabled());
        assert!(!metadata_source.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_metadata_source_observes_no_affinity_slots() {
        let mut run = TextAreaTextRun::new(
            "caret affinity no slots remains diagnostic".to_string(),
            0..43,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let mut payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose caret affinity metadata");
        payload
            .readiness
            .caret_affinity_diagnostic
            .visual_line_count = 0;
        payload.readiness.caret_affinity_diagnostic.caret_stop_count = 0;
        payload
            .readiness
            .caret_affinity_diagnostic
            .multi_stop_line_count = 0;
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );

        let metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
            TextAreaEditableIfcCaretAffinityMetadataSourceInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            metadata_source.state(),
            TextAreaEditableIfcCaretAffinityMetadataSourceState::ObservedNoAffinitySlots
        );
        assert!(metadata_source.blocked_reasons().is_empty());
        assert!(metadata_source.metadata_observed());
        let diagnostic = metadata_source
            .diagnostic()
            .expect("no-affinity-slots source should keep explicit diagnostic metadata");
        assert_eq!(
            diagnostic.caret_affinity_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedNoAffinitySlots
        );
        assert_eq!(diagnostic.visual_line_count, 0);
        assert_eq!(diagnostic.caret_stop_count, 0);
        assert_eq!(diagnostic.multi_stop_line_count, 0);
        assert!(!metadata_source.caret_affinity_behavior_path_ready());
        assert!(!metadata_source.render_enabled());
        assert!(!metadata_source.layout_enabled());
        assert!(!metadata_source.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_metadata_source_keeps_readiness_audit_diagnostic_only() {
        let mut run = TextAreaTextRun::new(
            "caret affinity metadata source does not authorize rollout".to_string(),
            0..50,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose caret affinity metadata source metadata");
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
            TextAreaEditableIfcCaretAffinityMetadataSourceInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            audit.state(),
            TextAreaEditableIfcReadinessAuditState::Blocked
        );
        assert!(
            audit
                .behavior_path_blocked_reasons()
                .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::CaretAffinityPathUnwired),
            "readiness audit must keep caret affinity behavior path blocked after metadata source"
        );
        assert_eq!(
            audit.recommendation(),
            TextAreaEditableIfcBehaviorPathRecommendation::KeepLegacyEditableBehaviorPath
        );
        assert!(metadata_source.metadata_observed());
        assert!(!audit.render_enabled());
        assert!(!audit.layout_enabled());
        assert!(!audit.allows_text_area_editable_behavior_path_switch());
        assert!(!metadata_source.caret_affinity_behavior_path_ready());
        assert!(!metadata_source.render_enabled());
        assert!(!metadata_source.layout_enabled());
        assert!(!metadata_source.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_metadata_source_preserves_ime_preedit_metadata() {
        let mut run = TextAreaTextRun::new(
            "caret affinity metadata source leaves IME metadata intact".to_string(),
            0..47,
        );
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 6,
            preedit_text: "入力".to_string(),
            preedit_cursor: Some((1, 3)),
        }));
        run.set_preedit_run(true, Some((1, 2)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out preedit run should expose caret affinity metadata source metadata");
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
            TextAreaEditableIfcCaretAffinityMetadataSourceInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            caret_metadata_source.state(),
            TextAreaEditableIfcCaretAffinityMetadataSourceState::MetadataObserved
        );
        let caret_diagnostic = caret_metadata_source
            .diagnostic()
            .expect("caret metadata source should preserve per-run caret diagnostic");
        assert_eq!(caret_diagnostic.preedit_cursor_count, 1);
        assert_eq!(
            caret_diagnostic.per_run_caret_diagnostics[0].preedit_cursor,
            Some((1, 2))
        );
        assert_eq!(
            ime_prewire.state(),
            TextAreaEditableIfcImeBehaviorPathPrewireState::DiagnosticPrewired
        );
        let ime_diagnostic = ime_prewire
            .diagnostic()
            .expect("IME prewire should still preserve preedit diagnostic metadata");
        assert_eq!(ime_diagnostic.preedit_cursor_count, 1);
        assert_eq!(ime_diagnostic.preedit_cursors, vec![(1, 2)]);
        assert!(!caret_metadata_source.caret_affinity_behavior_path_ready());
        assert!(!ime_prewire.ime_behavior_path_ready());
        assert!(!caret_metadata_source.allows_text_area_editable_behavior_path_switch());
        assert!(!ime_prewire.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_projection_behavior_prewire_observes_projection_segment_metadata() {
        let mut run = TextAreaTextRun::new(
            "projection behavior path reads projected segment metadata".to_string(),
            10..66,
        );
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 10,
            preedit_text: "候補".to_string(),
            preedit_cursor: Some((1, 2)),
        }));
        run.set_preedit_run(true, Some((0, 1)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let mut payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose projection metadata source metadata");
        payload
            .readiness
            .projection_diagnostic
            .projection_segment_count = 2;
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );

        let prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            prewire.state(),
            TextAreaEditableIfcProjectionBehaviorPathPrewireState::DiagnosticPrewired
        );
        assert!(prewire.blocked_reasons().is_empty());
        assert!(prewire.diagnostic_prewired());
        let diagnostic = prewire
            .diagnostic()
            .expect("projection metadata source should preserve diagnostic metadata");
        assert_eq!(
            diagnostic.projection_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::Observed
        );
        assert_eq!(diagnostic.run_count, 1);
        assert_eq!(diagnostic.char_range_count, 1);
        assert_eq!(diagnostic.char_span, 56);
        assert_eq!(
            diagnostic.effective_content_len,
            "projection 候補behavior path reads projected segment metadata"
                .chars()
                .count()
        );
        assert_eq!(diagnostic.inline_preedit_run_count, 1);
        assert_eq!(diagnostic.preedit_run_count, 1);
        assert_eq!(diagnostic.projection_segment_count, 2);
        assert_eq!(diagnostic.per_run_projection_diagnostics.len(), 1);
        assert_eq!(
            diagnostic.per_run_projection_diagnostics[0].projection_segment_count,
            2
        );
        assert!(
            prewire
                .input()
                .readiness_behavior_blocked_reasons
                .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::ProjectionPathUnwired),
            "#47 metadata source must not rewrite the #44 behavior-path audit"
        );
        assert!(!prewire.projection_behavior_path_ready());
        assert!(!prewire.render_enabled());
        assert!(!prewire.layout_enabled());
        assert!(!prewire.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_projection_behavior_prewire_observes_no_projection_segments() {
        let mut run = TextAreaTextRun::new(
            "projection prewire no segment remains diagnostic".to_string(),
            0..47,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose projection metadata");
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );

        let prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            prewire.state(),
            TextAreaEditableIfcProjectionBehaviorPathPrewireState::ObservedNoProjectionSegments
        );
        assert!(prewire.blocked_reasons().is_empty());
        assert!(prewire.diagnostic_prewired());
        let diagnostic = prewire
            .diagnostic()
            .expect("no-projection-segments prewire should keep explicit diagnostic metadata");
        assert_eq!(
            diagnostic.projection_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedNoProjectionSegments
        );
        assert_eq!(diagnostic.projection_segment_count, 0);
        assert_eq!(diagnostic.per_run_projection_diagnostics.len(), 1);
        assert_eq!(
            diagnostic.per_run_projection_diagnostics[0].projection_segment_count,
            0
        );
        assert!(!prewire.projection_behavior_path_ready());
        assert!(!prewire.render_enabled());
        assert!(!prewire.layout_enabled());
        assert!(!prewire.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_projection_behavior_prewire_blocks_missing_metadata_source() {
        let run = TextAreaTextRun::new("unlaid out projection prewire".to_string(), 0..30);
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(
                run.inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
                    .into_iter()
                    .collect(),
            ),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );

        let prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            prewire.state(),
            TextAreaEditableIfcProjectionBehaviorPathPrewireState::Blocked
        );
        for reason in [
            TextAreaEditableIfcProjectionBehaviorPathPrewireBlockedReason::MissingRunMetadata,
            TextAreaEditableIfcProjectionBehaviorPathPrewireBlockedReason::
                ProjectionMetadataUnwired,
            TextAreaEditableIfcProjectionBehaviorPathPrewireBlockedReason::
                MissingProjectionMetadataDiagnostic,
        ] {
            assert!(
                prewire.blocked_reasons().contains(&reason),
                "missing projection prewire metadata should block on {reason:?}"
            );
        }
        assert!(!prewire.diagnostic_prewired());
        assert!(prewire.diagnostic().is_none());
        assert!(!prewire.projection_behavior_path_ready());
        assert!(!prewire.render_enabled());
        assert!(!prewire.layout_enabled());
        assert!(!prewire.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_projection_behavior_prewire_keeps_readiness_audit_diagnostic_only() {
        let mut run = TextAreaTextRun::new(
            "projection prewire does not authorize rollout".to_string(),
            0..45,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let mut payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose projection prewire metadata");
        payload
            .readiness
            .projection_diagnostic
            .projection_segment_count = 1;
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            audit.state(),
            TextAreaEditableIfcReadinessAuditState::Blocked
        );
        assert!(
            audit
                .behavior_path_blocked_reasons()
                .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::ProjectionPathUnwired),
            "readiness audit must keep projection behavior path blocked after metadata source"
        );
        assert_eq!(
            audit.recommendation(),
            TextAreaEditableIfcBehaviorPathRecommendation::KeepLegacyEditableBehaviorPath
        );
        assert!(prewire.diagnostic_prewired());
        assert!(!audit.render_enabled());
        assert!(!audit.layout_enabled());
        assert!(!audit.allows_text_area_editable_behavior_path_switch());
        assert!(!prewire.projection_behavior_path_ready());
        assert!(!prewire.render_enabled());
        assert!(!prewire.layout_enabled());
        assert!(!prewire.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_projection_behavior_prewire_preserves_ime_and_caret_metadata_sources() {
        let mut run = TextAreaTextRun::new(
            "projection prewire leaves IME and caret metadata intact".to_string(),
            0..54,
        );
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 11,
            preedit_text: "入力".to_string(),
            preedit_cursor: Some((1, 2)),
        }));
        run.set_preedit_run(true, Some((1, 3)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let mut payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out preedit run should expose projection prewire metadata");
        payload
            .readiness
            .projection_diagnostic
            .projection_segment_count = 1;
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
            TextAreaEditableIfcCaretAffinityMetadataSourceInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            projection_prewire.state(),
            TextAreaEditableIfcProjectionBehaviorPathPrewireState::DiagnosticPrewired
        );
        assert_eq!(
            ime_prewire.state(),
            TextAreaEditableIfcImeBehaviorPathPrewireState::DiagnosticPrewired
        );
        assert_eq!(
            caret_metadata_source.state(),
            TextAreaEditableIfcCaretAffinityMetadataSourceState::MetadataObserved
        );
        assert_eq!(
            ime_prewire
                .diagnostic()
                .expect("IME prewire should still preserve preedit metadata")
                .preedit_cursors,
            vec![(1, 3)]
        );
        assert_eq!(
            caret_metadata_source
                .diagnostic()
                .expect("caret metadata source should still preserve caret metadata")
                .preedit_cursor_count,
            1
        );
        assert!(!projection_prewire.projection_behavior_path_ready());
        assert!(!ime_prewire.ime_behavior_path_ready());
        assert!(!caret_metadata_source.caret_affinity_behavior_path_ready());
        assert!(!projection_prewire.allows_text_area_editable_behavior_path_switch());
        assert!(!ime_prewire.allows_text_area_editable_behavior_path_switch());
        assert!(!caret_metadata_source.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_scroll_follow_behavior_prewire_observes_scroll_metadata() {
        let mut run = TextAreaTextRun::new(
            "scroll follow behavior path reads layout and caret metadata".to_string(),
            4..61,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose scroll-follow metadata source metadata");
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );

        let prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            prewire.state(),
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::DiagnosticPrewired
        );
        assert!(prewire.blocked_reasons().is_empty());
        assert!(prewire.diagnostic_prewired());
        let diagnostic = prewire
            .diagnostic()
            .expect("scroll-follow metadata source should preserve diagnostic metadata");
        assert_eq!(
            diagnostic.scroll_follow_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::Observed
        );
        assert_eq!(diagnostic.run_count, 1);
        assert_eq!(diagnostic.layout_size_count, 1);
        assert_eq!(diagnostic.char_span, 57);
        assert_eq!(
            diagnostic.effective_content_len,
            "scroll follow behavior path reads layout and caret metadata"
                .chars()
                .count()
        );
        assert!(diagnostic.visual_line_count >= 1);
        assert!(diagnostic.caret_stop_count >= 1);
        assert_eq!(diagnostic.per_run_scroll_follow_diagnostics.len(), 1);
        assert!(
            diagnostic.per_run_scroll_follow_diagnostics[0].layout_size[0] > 0.0
                && diagnostic.per_run_scroll_follow_diagnostics[0].layout_size[1] > 0.0
        );
        assert!(
            prewire
                .input()
                .readiness_behavior_blocked_reasons
                .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::ScrollFollowPathUnwired),
            "#48 metadata source must not rewrite the #44 behavior-path audit"
        );
        assert!(!prewire.scroll_follow_behavior_path_ready());
        assert!(!prewire.render_enabled());
        assert!(!prewire.layout_enabled());
        assert!(!prewire.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_scroll_follow_behavior_prewire_observes_no_scroll_source() {
        let mut run = TextAreaTextRun::new(
            "scroll follow prewire no source remains diagnostic".to_string(),
            0..51,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let mut payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose scroll-follow metadata");
        payload.readiness.scroll_follow_diagnostic.layout_size = [0.0, 0.0];
        payload.readiness.scroll_follow_diagnostic.visual_line_count = 0;
        payload.readiness.scroll_follow_diagnostic.caret_stop_count = 0;
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );

        let prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            prewire.state(),
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::ObservedNoScrollSource
        );
        assert!(prewire.blocked_reasons().is_empty());
        assert!(prewire.diagnostic_prewired());
        let diagnostic = prewire
            .diagnostic()
            .expect("no-scroll-source prewire should keep explicit diagnostic metadata");
        assert_eq!(
            diagnostic.scroll_follow_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedNoScrollSource
        );
        assert_eq!(diagnostic.layout_size_count, 0);
        assert_eq!(diagnostic.visual_line_count, 0);
        assert_eq!(diagnostic.caret_stop_count, 0);
        assert_eq!(diagnostic.per_run_scroll_follow_diagnostics.len(), 1);
        assert_eq!(
            diagnostic.per_run_scroll_follow_diagnostics[0].layout_size,
            [0.0, 0.0]
        );
        assert!(!prewire.scroll_follow_behavior_path_ready());
        assert!(!prewire.render_enabled());
        assert!(!prewire.layout_enabled());
        assert!(!prewire.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_scroll_follow_behavior_prewire_blocks_missing_metadata_source() {
        let run = TextAreaTextRun::new("unlaid out scroll-follow prewire".to_string(), 0..34);
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(
                run.inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
                    .into_iter()
                    .collect(),
            ),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );

        let prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            prewire.state(),
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::Blocked
        );
        for reason in [
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireBlockedReason::MissingRunMetadata,
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireBlockedReason::
                ScrollFollowMetadataUnwired,
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireBlockedReason::
                MissingScrollFollowMetadataDiagnostic,
        ] {
            assert!(
                prewire.blocked_reasons().contains(&reason),
                "missing scroll-follow prewire metadata should block on {reason:?}"
            );
        }
        assert!(!prewire.diagnostic_prewired());
        assert!(prewire.diagnostic().is_none());
        assert!(!prewire.scroll_follow_behavior_path_ready());
        assert!(!prewire.render_enabled());
        assert!(!prewire.layout_enabled());
        assert!(!prewire.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_scroll_follow_behavior_prewire_keeps_readiness_audit_diagnostic_only() {
        let mut run = TextAreaTextRun::new(
            "scroll follow prewire does not authorize rollout".to_string(),
            0..48,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose scroll-follow prewire metadata");
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            audit.state(),
            TextAreaEditableIfcReadinessAuditState::Blocked
        );
        assert!(
            audit
                .behavior_path_blocked_reasons()
                .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::ScrollFollowPathUnwired),
            "readiness audit must keep scroll-follow behavior path blocked after metadata source"
        );
        assert_eq!(
            audit.recommendation(),
            TextAreaEditableIfcBehaviorPathRecommendation::KeepLegacyEditableBehaviorPath
        );
        assert!(prewire.diagnostic_prewired());
        assert!(!audit.render_enabled());
        assert!(!audit.layout_enabled());
        assert!(!audit.allows_text_area_editable_behavior_path_switch());
        assert!(!prewire.scroll_follow_behavior_path_ready());
        assert!(!prewire.render_enabled());
        assert!(!prewire.layout_enabled());
        assert!(!prewire.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_scroll_follow_behavior_prewire_preserves_prior_prewires() {
        let mut run = TextAreaTextRun::new(
            "scroll follow prewire leaves IME caret and projection intact".to_string(),
            0..60,
        );
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 14,
            preedit_text: "入力".to_string(),
            preedit_cursor: Some((1, 2)),
        }));
        run.set_preedit_run(true, Some((1, 3)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let mut payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out preedit run should expose scroll-follow prewire metadata");
        payload
            .readiness
            .projection_diagnostic
            .projection_segment_count = 1;
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
            TextAreaEditableIfcCaretAffinityMetadataSourceInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        assert_eq!(
            scroll_prewire.state(),
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::DiagnosticPrewired
        );
        assert_eq!(
            projection_prewire.state(),
            TextAreaEditableIfcProjectionBehaviorPathPrewireState::DiagnosticPrewired
        );
        assert_eq!(
            ime_prewire.state(),
            TextAreaEditableIfcImeBehaviorPathPrewireState::DiagnosticPrewired
        );
        assert_eq!(
            caret_metadata_source.state(),
            TextAreaEditableIfcCaretAffinityMetadataSourceState::MetadataObserved
        );
        assert_eq!(
            ime_prewire
                .diagnostic()
                .expect("IME prewire should still preserve preedit metadata")
                .preedit_cursors,
            vec![(1, 3)]
        );
        assert_eq!(
            caret_metadata_source
                .diagnostic()
                .expect("caret metadata source should still preserve caret metadata")
                .preedit_cursor_count,
            1
        );
        assert_eq!(
            projection_prewire
                .diagnostic()
                .expect("projection prewire should still preserve projection metadata")
                .projection_segment_count,
            1
        );
        assert!(!scroll_prewire.scroll_follow_behavior_path_ready());
        assert!(!projection_prewire.projection_behavior_path_ready());
        assert!(!ime_prewire.ime_behavior_path_ready());
        assert!(!caret_metadata_source.caret_affinity_behavior_path_ready());
        assert!(!scroll_prewire.allows_text_area_editable_behavior_path_switch());
        assert!(!projection_prewire.allows_text_area_editable_behavior_path_switch());
        assert!(!ime_prewire.allows_text_area_editable_behavior_path_switch());
        assert!(!caret_metadata_source.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_behavior_path_status_allows_behavior_status_observation_only() {
        let mut run = TextAreaTextRun::new(
            "behavior path status observes all metadata source diagnostics".to_string(),
            0..61,
        );
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 18,
            preedit_text: "入力".to_string(),
            preedit_cursor: Some((1, 2)),
        }));
        run.set_preedit_run(true, Some((1, 3)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let mut payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose behavior path status metadata");
        payload
            .readiness
            .projection_diagnostic
            .projection_segment_count = 1;
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
            TextAreaEditableIfcCaretAffinityMetadataSourceInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        let diagnostic = TextAreaEditableIfcBehaviorPathStatus::evaluate(
            TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
                &preflight,
                &audit,
                &ime_prewire,
                &caret_metadata_source,
                &projection_prewire,
                &scroll_prewire,
            ),
        );

        assert_eq!(
            diagnostic.state(),
            TextAreaEditableIfcBehaviorPathStatusState::ReadyForStatusObservation
        );
        assert!(diagnostic.blocked_reasons().is_empty());
        assert_eq!(
            diagnostic.recommendation(),
            TextAreaEditableIfcBehaviorPathStatusRecommendation::KeepLegacyEditableBehaviorPath
        );
        assert!(!diagnostic.render_enabled());
        assert!(!diagnostic.layout_enabled());
        assert!(!diagnostic.allows_text_area_editable_behavior_path_switch());
        assert_eq!(
            diagnostic.report().metadata_bridge_state,
            TextAreaInlineIfcMetadataBridgePreflightState::ReadyForDiagnosticEvaluation
        );
        assert_eq!(
            diagnostic.report().audit_state,
            TextAreaEditableIfcReadinessAuditState::Blocked
        );
        assert_eq!(
            diagnostic.report().audit_recommendation,
            TextAreaEditableIfcBehaviorPathRecommendation::KeepLegacyEditableBehaviorPath
        );
        assert_eq!(
            diagnostic.report().ime_prewire_state,
            TextAreaEditableIfcImeBehaviorPathPrewireState::DiagnosticPrewired
        );
        assert_eq!(
            diagnostic.report().caret_affinity_metadata_source_state,
            TextAreaEditableIfcCaretAffinityMetadataSourceState::MetadataObserved
        );
        assert_eq!(
            diagnostic.report().projection_prewire_state,
            TextAreaEditableIfcProjectionBehaviorPathPrewireState::DiagnosticPrewired
        );
        assert_eq!(
            diagnostic.report().scroll_follow_prewire_state,
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::DiagnosticPrewired
        );
        assert!(diagnostic.report().ime_diagnostic_prewired);
        assert!(diagnostic.report().caret_affinity_metadata_observed);
        assert!(diagnostic.report().projection_diagnostic_prewired);
        assert!(diagnostic.report().scroll_follow_diagnostic_prewired);
        assert!(!diagnostic.report().ime_behavior_path_ready);
        assert!(!diagnostic.report().caret_affinity_behavior_path_ready);
        assert!(!diagnostic.report().projection_behavior_path_ready);
        assert!(!diagnostic.report().scroll_follow_behavior_path_ready);
        for reason in [
            TextAreaEditableIfcBehaviorPathBlockedReason::ProjectionPathUnwired,
            TextAreaEditableIfcBehaviorPathBlockedReason::ImePathUnwired,
            TextAreaEditableIfcBehaviorPathBlockedReason::CaretAffinityPathUnwired,
            TextAreaEditableIfcBehaviorPathBlockedReason::ScrollFollowPathUnwired,
        ] {
            assert!(
                diagnostic
                    .report()
                    .behavior_path_blocked_reasons
                    .contains(&reason),
                "behavior path status must preserve #44 behavior-path blocked reason {reason:?}"
            );
        }
        assert!(!ime_prewire.ime_behavior_path_ready());
        assert!(!caret_metadata_source.caret_affinity_behavior_path_ready());
        assert!(!projection_prewire.projection_behavior_path_ready());
        assert!(!scroll_prewire.scroll_follow_behavior_path_ready());
    }

    #[test]
    fn text_area_inline_ifc_behavior_path_status_blocks_missing_prewire_source() {
        let run = TextAreaTextRun::new("unlaid out behavior path status".to_string(), 0..30);
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(
                run.inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
                    .into_iter()
                    .collect(),
            ),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
            TextAreaEditableIfcCaretAffinityMetadataSourceInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );

        let gate = TextAreaEditableIfcBehaviorPathStatus::evaluate(
            TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
                &preflight,
                &audit,
                &ime_prewire,
                &caret_metadata_source,
                &projection_prewire,
                &scroll_prewire,
            ),
        );

        assert_eq!(
            gate.state(),
            TextAreaEditableIfcBehaviorPathStatusState::Blocked
        );
        for reason in [
            TextAreaEditableIfcBehaviorPathStatusBlockedReason::MetadataBridgeBlocked,
            TextAreaEditableIfcBehaviorPathStatusBlockedReason::MetadataObservationIncomplete,
            TextAreaEditableIfcBehaviorPathStatusBlockedReason::ImePrewireBlocked,
            TextAreaEditableIfcBehaviorPathStatusBlockedReason::ImeDiagnosticNotObserved,
            TextAreaEditableIfcBehaviorPathStatusBlockedReason::CaretAffinityMetadataSourceBlocked,
            TextAreaEditableIfcBehaviorPathStatusBlockedReason::CaretAffinityMetadataNotObserved,
            TextAreaEditableIfcBehaviorPathStatusBlockedReason::ProjectionPrewireBlocked,
            TextAreaEditableIfcBehaviorPathStatusBlockedReason::ProjectionDiagnosticNotObserved,
            TextAreaEditableIfcBehaviorPathStatusBlockedReason::ScrollFollowPrewireBlocked,
            TextAreaEditableIfcBehaviorPathStatusBlockedReason::ScrollFollowDiagnosticNotObserved,
        ] {
            assert!(
                gate.blocked_reasons().contains(&reason),
                "missing behavior path status inputs should block on {reason:?}"
            );
        }
        assert!(!gate.render_enabled());
        assert!(!gate.layout_enabled());
        assert!(!gate.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_behavior_path_status_blocks_one_incomplete_prewire() {
        let mut run = TextAreaTextRun::new(
            "behavior path status keeps one missing prewire blocked".to_string(),
            0..54,
        );
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 12,
            preedit_text: "入力".to_string(),
            preedit_cursor: Some((1, 2)),
        }));
        run.set_preedit_run(true, Some((1, 3)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let mut payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose behavior path status metadata");
        payload
            .readiness
            .projection_diagnostic
            .projection_segment_count = 1;
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
            TextAreaEditableIfcCaretAffinityMetadataSourceInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let mut input =
            TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
                &preflight,
                &audit,
                &ime_prewire,
                &caret_metadata_source,
                &projection_prewire,
                &scroll_prewire,
            );
        input.ime_prewire_state = TextAreaEditableIfcImeBehaviorPathPrewireState::Blocked;
        input.ime_prewire_blocked_reasons =
            vec![TextAreaEditableIfcImeBehaviorPathPrewireBlockedReason::ImeMetadataUnwired];
        input.ime_diagnostic_prewired = false;

        let gate = TextAreaEditableIfcBehaviorPathStatus::evaluate(input);

        assert_eq!(
            gate.state(),
            TextAreaEditableIfcBehaviorPathStatusState::Blocked
        );
        assert!(
            gate.blocked_reasons()
                .contains(&TextAreaEditableIfcBehaviorPathStatusBlockedReason::ImePrewireBlocked)
        );
        assert!(gate.blocked_reasons().contains(
            &TextAreaEditableIfcBehaviorPathStatusBlockedReason::ImeDiagnosticNotObserved
        ));
        assert!(
            gate.report()
                .behavior_path_blocked_reasons
                .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::ImePathUnwired),
            "behavior path status must keep #44 behavior path blocked reasons while blocking one prewire"
        );
        assert!(!gate.report().ime_behavior_path_ready);
        assert!(!gate.report().caret_affinity_behavior_path_ready);
        assert!(!gate.report().projection_behavior_path_ready);
        assert!(!gate.report().scroll_follow_behavior_path_ready);
        assert!(!gate.render_enabled());
        assert!(!gate.layout_enabled());
        assert!(!gate.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_behavior_path_status_uses_prepared_default_render() {
        let mut run = TextAreaTextRun::new(
            "behavior path status still renders through legacy text pass".to_string(),
            0..56,
        );
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 11,
            preedit_text: "入力".to_string(),
            preedit_cursor: Some((1, 2)),
        }));
        run.set_preedit_run(true, Some((1, 3)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let mut payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose behavior path status metadata");
        payload
            .readiness
            .projection_diagnostic
            .projection_segment_count = 1;
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
            TextAreaEditableIfcCaretAffinityMetadataSourceInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let gate = TextAreaEditableIfcBehaviorPathStatus::evaluate(
            TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
                &preflight,
                &audit,
                &ime_prewire,
                &caret_metadata_source,
                &projection_prewire,
                &scroll_prewire,
            ),
        );
        assert_eq!(
            gate.state(),
            TextAreaEditableIfcBehaviorPathStatusState::ReadyForStatusObservation
        );
        assert!(!gate.render_enabled());

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(180, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        run.build(&mut graph, &mut arena, ctx);
        let pass_names = graph
            .pass_descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name.to_string())
            .collect::<Vec<_>>();

        assert_eq!(pass_names.len(), 1);
        assert!(
            pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
            "TextArea behavior path status does not own render switching; TextArea default render should emit TextPreparedInputPass for a valid candidate, got {}",
            pass_names[0]
        );
    }

    #[test]
    fn text_area_inline_ifc_behavior_path_status_reports_blocked_switch() {
        let run = TextAreaTextRun::new("blocked behavior path status".to_string(), 0..32);
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(
                run.inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
                    .into_iter()
                    .collect(),
            ),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
            TextAreaEditableIfcCaretAffinityMetadataSourceInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let diagnostic = TextAreaEditableIfcBehaviorPathStatus::evaluate(
            TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
                &preflight,
                &audit,
                &ime_prewire,
                &caret_metadata_source,
                &projection_prewire,
                &scroll_prewire,
            ),
        );

        assert_eq!(
            diagnostic.state(),
            TextAreaEditableIfcBehaviorPathStatusState::Blocked
        );
        assert_eq!(
            diagnostic.recommendation(),
            TextAreaEditableIfcBehaviorPathStatusRecommendation::KeepLegacyEditableBehaviorPath
        );
        assert!(diagnostic.behavior_path_switch_blocked_reasons().contains(
            &TextAreaEditableIfcBehaviorPathStatusBlockedReason::BehaviorPathsStillNotReady
        ));
        assert!(diagnostic.report().prepared_render_default_observed);
        assert!(!diagnostic.render_enabled());
        assert!(!diagnostic.layout_enabled());
        assert!(!diagnostic.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_behavior_path_status_blocks_diagnostic_only_switch() {
        let diagnostic = text_area_inline_ifc_ready_behavior_path_status();

        assert_eq!(
            diagnostic.state(),
            TextAreaEditableIfcBehaviorPathStatusState::ReadyForStatusObservation
        );
        assert!(diagnostic.blocked_reasons().is_empty());
        assert!(
            diagnostic.behavior_path_switch_blocked_reasons().contains(
                &TextAreaEditableIfcBehaviorPathStatusBlockedReason::StatusObservationOnly
            )
        );
        assert!(diagnostic.behavior_path_switch_blocked_reasons().contains(
            &TextAreaEditableIfcBehaviorPathStatusBlockedReason::BehaviorPathsStillNotReady
        ));
        assert!(!diagnostic.report().ime_behavior_path_ready);
        assert!(!diagnostic.report().caret_affinity_behavior_path_ready);
        assert!(!diagnostic.report().projection_behavior_path_ready);
        assert!(!diagnostic.report().scroll_follow_behavior_path_ready);
        assert!(diagnostic.report().prepared_render_default_observed);
        assert!(!diagnostic.render_enabled());
        assert!(!diagnostic.layout_enabled());
        assert!(!diagnostic.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_behavior_path_status_preserves_layout_with_prepared_render() {
        let diagnostic = text_area_inline_ifc_ready_behavior_path_status();
        let mut run = TextAreaTextRun::new(
            "behavior path status keeps layout unchanged".to_string(),
            0..63,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let before_size = run.layout_state.layout_size;
        let before_fragments = run.inline_fragment_positions();

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(180, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        run.build(&mut graph, &mut arena, ctx);
        let pass_names = graph
            .pass_descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name.to_string())
            .collect::<Vec<_>>();

        assert_eq!(pass_names.len(), 1);
        assert!(
            pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
            "TextArea behavior path status does not own render switching; TextArea default render should emit TextPreparedInputPass for a valid candidate, got {}",
            pass_names[0]
        );
        assert_eq!(before_size.width, run.layout_state.layout_size.width);
        assert_eq!(before_size.height, run.layout_state.layout_size.height);
        assert_eq!(before_fragments, run.inline_fragment_positions());
        assert!(!diagnostic.render_enabled());
        assert!(!diagnostic.layout_enabled());
        assert!(!diagnostic.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_behavior_readiness_report_lists_all_blocked_paths() {
        let (behavior_status, caret_metadata_source) =
            text_area_inline_ifc_behavior_status_and_caret_metadata_source_for_readiness();

        let ready_report = behavior_status.readiness_report(&caret_metadata_source);

        assert_eq!(
            behavior_status.state(),
            TextAreaEditableIfcBehaviorPathStatusState::ReadyForStatusObservation
        );
        assert!(
            behavior_status
                .behavior_path_switch_blocked_reasons()
                .contains(
                    &TextAreaEditableIfcBehaviorPathStatusBlockedReason::BehaviorPathsStillNotReady
                )
        );
        for reason in [
            TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                ImeBehaviorPathNotReady,
            TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                CaretAffinityBehaviorPathNotReady,
            TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                ProjectionBehaviorPathNotReady,
            TextAreaEditableIfcBehaviorPathReadinessBlockedReason::
                ScrollFollowBehaviorPathNotReady,
        ] {
            assert!(
                ready_report.blocked_reasons.contains(&reason),
                "readiness report must split BehaviorPathsStillNotReady into {reason:?}"
            );
        }
        assert_eq!(
            ready_report.ime_readiness_state,
            TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
        );
        assert_eq!(
            ready_report.projection_readiness_state,
            TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
        );
        assert_eq!(
            ready_report.scroll_follow_readiness_state,
            TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
        );
    }

    #[test]
    fn text_area_inline_ifc_behavior_readiness_report_observes_caret_candidate_only() {
        let (behavior_status, caret_metadata_source) =
            text_area_inline_ifc_behavior_status_and_caret_metadata_source_for_readiness();

        let ready_report = behavior_status.readiness_report(&caret_metadata_source);

        assert_eq!(
            caret_metadata_source.state(),
            TextAreaEditableIfcCaretAffinityMetadataSourceState::MetadataObserved
        );
        assert!(caret_metadata_source.metadata_observed());
        assert_eq!(
            ready_report.caret_affinity_readiness_state,
            TextAreaEditableIfcBehaviorPathReadinessState::CaretAffinityCandidateObserved
        );
        assert_eq!(
            ready_report.caret_affinity_candidate,
            TextAreaEditableIfcBehaviorPathReadinessCandidate::CaretAffinityCandidateObserved
        );
        assert_eq!(
            ready_report.caret_affinity_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
        );
        assert!(ready_report.caret_affinity_caret_stop_count > 0);
        assert!(!ready_report.caret_affinity_behavior_path_ready);
        assert!(!behavior_status.report().caret_affinity_behavior_path_ready);
        assert!(!caret_metadata_source.caret_affinity_behavior_path_ready());
        assert!(!ready_report.ime_behavior_path_ready);
        assert!(!ready_report.projection_behavior_path_ready);
        assert!(!ready_report.scroll_follow_behavior_path_ready);
        assert!(!behavior_status.render_enabled());
        assert!(!behavior_status.layout_enabled());
        assert!(!behavior_status.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_behavior_readiness_report_preserves_layout_with_prepared_default() {
        let (behavior_status, caret_metadata_source) =
            text_area_inline_ifc_behavior_status_and_caret_metadata_source_for_readiness();
        let ready_report = behavior_status.readiness_report(&caret_metadata_source);
        let mut run = TextAreaTextRun::new(
            "behavior readiness preserves layout with prepared default render".to_string(),
            0..62,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let before_size = run.layout_state.layout_size;
        let before_fragments = run.inline_fragment_positions();

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(180, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        run.build(&mut graph, &mut arena, ctx);
        let pass_names = graph
            .pass_descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name.to_string())
            .collect::<Vec<_>>();

        assert_eq!(pass_names.len(), 1);
        assert!(
            pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
            "TextArea behavior readiness does not own render switching; TextArea default render should emit TextPreparedInputPass for a valid candidate, got {}",
            pass_names[0]
        );
        assert_eq!(before_size.width, run.layout_state.layout_size.width);
        assert_eq!(before_size.height, run.layout_state.layout_size.height);
        assert_eq!(before_fragments, run.inline_fragment_positions());
        assert!(!ready_report.caret_affinity_behavior_path_ready);
        assert!(!behavior_status.render_enabled());
        assert!(!behavior_status.layout_enabled());
        assert!(!behavior_status.allows_text_area_editable_behavior_path_switch());
    }

    fn text_area_inline_ifc_caret_affinity_equivalence_fixture() -> (
        TextAreaEditableIfcBehaviorPathStatus,
        TextAreaEditableIfcCaretAffinityMetadataSource,
        TextAreaEditableIfcCaretAffinityEquivalenceAudit,
    ) {
        let (behavior_status, caret_metadata_source) =
            text_area_inline_ifc_behavior_status_and_caret_metadata_source_for_readiness();
        let audit = TextAreaEditableIfcCaretAffinityEquivalenceAudit::evaluate(
            TextAreaEditableIfcCaretAffinityEquivalenceAuditInput::
                from_behavior_status_and_caret_affinity_metadata_source(
                    &behavior_status,
                    &caret_metadata_source,
                ),
        );

        (behavior_status, caret_metadata_source, audit)
    }

    fn text_area_inline_ifc_all_equivalence_fixture() -> (
        TextAreaEditableIfcBehaviorPathStatus,
        TextAreaEditableIfcCaretAffinityEquivalenceAudit,
        TextAreaEditableIfcProjectionEquivalenceAudit,
        TextAreaEditableIfcScrollFollowEquivalenceAudit,
        TextAreaEditableIfcImeEquivalenceAudit,
    ) {
        let mut run = TextAreaTextRun::new(
            "observation equivalence status observes IME caret projection scroll".to_string(),
            0..67,
        );
        run.set_inline_preedit(Some(InlinePreedit {
            insert_at_local: 12,
            preedit_text: "入力".to_string(),
            preedit_cursor: Some((1, 2)),
        }));
        run.set_preedit_run(true, Some((1, 3)));
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let mut payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose all equivalence metadata");
        payload
            .readiness
            .projection_diagnostic
            .projection_segment_count = 2;
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
            TextAreaEditableIfcCaretAffinityMetadataSourceInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
        );
        let behavior_status = TextAreaEditableIfcBehaviorPathStatus::evaluate(
            TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
                &preflight,
                &audit,
                &ime_prewire,
                &caret_metadata_source,
                &projection_prewire,
                &scroll_prewire,
            ),
        );
        let caret_audit = TextAreaEditableIfcCaretAffinityEquivalenceAudit::evaluate(
            TextAreaEditableIfcCaretAffinityEquivalenceAuditInput::
                from_behavior_status_and_caret_affinity_metadata_source(
                    &behavior_status,
                    &caret_metadata_source,
                ),
        );
        let projection_audit = TextAreaEditableIfcProjectionEquivalenceAudit::evaluate(
            TextAreaEditableIfcProjectionEquivalenceAuditInput::
                from_behavior_status_and_projection_prewire(&behavior_status, &projection_prewire),
        );
        let scroll_audit = TextAreaEditableIfcScrollFollowEquivalenceAudit::evaluate(
            TextAreaEditableIfcScrollFollowEquivalenceAuditInput::
                from_behavior_status_and_scroll_follow_prewire(&behavior_status, &scroll_prewire),
        );
        let ime_audit = TextAreaEditableIfcImeEquivalenceAudit::evaluate(
            TextAreaEditableIfcImeEquivalenceAuditInput::from_behavior_status_and_ime_prewire(
                &behavior_status,
                &ime_prewire,
            ),
        );

        (
            behavior_status,
            caret_audit,
            projection_audit,
            scroll_audit,
            ime_audit,
        )
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_equivalence_audit_observes_observation_only_candidate() {
        let (behavior_status, caret_metadata_source, audit) =
            text_area_inline_ifc_caret_affinity_equivalence_fixture();
        let metadata_source_diagnostic = caret_metadata_source
            .diagnostic()
            .expect("fixture should expose caret metadata source diagnostic");

        assert_eq!(
            audit.state(),
            TextAreaEditableIfcCaretAffinityEquivalenceAuditState::ReadyForObservationOnly
        );
        assert!(audit.blocked_reasons().is_empty());
        assert_eq!(
            audit.diagnostic().equivalent_candidate,
            TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate::EquivalentCandidateObserved
        );
        assert_eq!(
            audit.recommendation(),
            TextAreaEditableIfcCaretAffinityEquivalenceAuditRecommendation::ObservationOnlyNoOp
        );
        assert_eq!(
            audit.diagnostic().caret_affinity_candidate,
            TextAreaEditableIfcBehaviorPathReadinessCandidate::CaretAffinityCandidateObserved
        );
        assert_eq!(
            audit.diagnostic().caret_affinity_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
        );
        assert_eq!(
            audit.diagnostic().visual_line_count,
            metadata_source_diagnostic.visual_line_count
        );
        assert_eq!(
            audit.diagnostic().caret_stop_count,
            metadata_source_diagnostic.caret_stop_count
        );
        assert_eq!(
            audit.diagnostic().multi_stop_line_count,
            metadata_source_diagnostic.multi_stop_line_count
        );
        assert_eq!(
            audit.diagnostic().preedit_cursor_count,
            metadata_source_diagnostic.preedit_cursor_count
        );
        assert!(!audit.caret_affinity_behavior_path_ready());
        assert!(!audit.diagnostic().caret_affinity_behavior_path_ready);
        assert!(
            !behavior_status
                .readiness_report(&caret_metadata_source)
                .caret_affinity_behavior_path_ready
        );
        assert!(!behavior_status.report().caret_affinity_behavior_path_ready);
        assert!(!audit.render_enabled());
        assert!(!audit.layout_enabled());
        assert!(!audit.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_behavior_input_uses_equivalent_metadata_only() {
        let (behavior_status, caret_metadata_source, audit) =
            text_area_inline_ifc_caret_affinity_equivalence_fixture();
        let behavior_input =
            TextAreaEditableIfcCaretAffinityBehaviorInput::from_equivalence_audit(&audit);

        assert!(behavior_input.equivalent_candidate_observed);
        assert_eq!(
            behavior_input.caret_affinity_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
        );
        assert_eq!(
            behavior_input.visual_line_count,
            audit.diagnostic().visual_line_count
        );
        assert_eq!(
            behavior_input.caret_stop_count,
            audit.diagnostic().caret_stop_count
        );
        assert_eq!(
            behavior_input.multi_stop_line_count,
            audit.diagnostic().multi_stop_line_count
        );
        assert_eq!(
            behavior_input.preedit_cursor_count,
            audit.diagnostic().preedit_cursor_count
        );
        assert_eq!(
            behavior_input.per_run_caret_diagnostics,
            audit.diagnostic().per_run_caret_diagnostics
        );
        assert!(!behavior_input.caret_affinity_behavior_path_ready());
        assert!(!behavior_input.render_enabled());
        assert!(!behavior_input.layout_enabled());
        assert!(!behavior_input.allows_text_area_editable_behavior_path_switch());
        assert!(
            !behavior_status
                .readiness_report(&caret_metadata_source)
                .caret_affinity_behavior_path_ready
        );
        assert!(!behavior_status.report().ime_behavior_path_ready);
        assert!(!behavior_status.report().projection_behavior_path_ready);
        assert!(!behavior_status.report().scroll_follow_behavior_path_ready);
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_behavior_evaluation_reads_behavior_input_metadata() {
        let (behavior_status, caret_metadata_source, audit) =
            text_area_inline_ifc_caret_affinity_equivalence_fixture();
        let behavior_input =
            TextAreaEditableIfcCaretAffinityBehaviorInput::from_equivalence_audit(&audit);
        let evaluation =
            TextAreaEditableIfcCaretAffinityBehaviorEvaluation::evaluate(behavior_input.clone());

        assert_eq!(
            evaluation.state(),
            TextAreaEditableIfcCaretAffinityBehaviorEvaluationState::InputObserved
        );
        assert!(evaluation.blocked_reasons().is_empty());
        assert_eq!(evaluation.input(), &behavior_input);
        assert!(evaluation.diagnostic().equivalent_candidate_observed);
        assert!(evaluation.diagnostic().input_observed);
        assert_eq!(
            evaluation.diagnostic().caret_affinity_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
        );
        assert_eq!(
            evaluation.diagnostic().visual_line_count,
            behavior_input.visual_line_count
        );
        assert_eq!(
            evaluation.diagnostic().caret_stop_count,
            behavior_input.caret_stop_count
        );
        assert_eq!(
            evaluation.diagnostic().multi_stop_line_count,
            behavior_input.multi_stop_line_count
        );
        assert_eq!(
            evaluation.diagnostic().preedit_cursor_count,
            behavior_input.preedit_cursor_count
        );
        assert_eq!(
            evaluation.diagnostic().per_run_caret_diagnostics,
            behavior_input.per_run_caret_diagnostics
        );
        assert!(!evaluation.caret_affinity_behavior_path_ready());
        assert!(!evaluation.diagnostic().caret_affinity_behavior_path_ready);
        assert!(!evaluation.render_enabled());
        assert!(!evaluation.layout_enabled());
        assert!(!evaluation.allows_text_area_editable_behavior_path_switch());
        assert!(
            !behavior_status
                .readiness_report(&caret_metadata_source)
                .caret_affinity_behavior_path_ready
        );
        assert!(!behavior_status.report().ime_behavior_path_ready);
        assert!(!behavior_status.report().projection_behavior_path_ready);
        assert!(!behavior_status.report().scroll_follow_behavior_path_ready);
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_read_only_lookup_reads_behavior_evaluation() {
        let (behavior_status, caret_metadata_source, audit) =
            text_area_inline_ifc_caret_affinity_equivalence_fixture();
        let behavior_input =
            TextAreaEditableIfcCaretAffinityBehaviorInput::from_equivalence_audit(&audit);
        let evaluation =
            TextAreaEditableIfcCaretAffinityBehaviorEvaluation::evaluate(behavior_input.clone());
        let adapter =
            TextAreaEditableIfcCaretAffinityReadOnlyLookupAdapter::from_behavior_evaluation(
                &evaluation,
            );
        let lookup = adapter
            .lookup()
            .expect("observed behavior evaluation should expose read-only lookup");

        assert_eq!(
            adapter.state(),
            TextAreaEditableIfcCaretAffinityReadOnlyLookupState::ReadOnlyLookupObserved
        );
        assert!(adapter.blocked_reasons().is_empty());
        assert_eq!(lookup.visual_line_count, behavior_input.visual_line_count);
        assert_eq!(lookup.caret_stop_count, behavior_input.caret_stop_count);
        assert_eq!(
            lookup.multi_stop_line_count,
            behavior_input.multi_stop_line_count
        );
        assert_eq!(
            lookup.preedit_cursor_count,
            behavior_input.preedit_cursor_count
        );
        assert_eq!(
            lookup.per_run_caret_diagnostics,
            behavior_input.per_run_caret_diagnostics
        );
        assert!(!adapter.caret_affinity_behavior_path_ready());
        assert!(!adapter.render_enabled());
        assert!(!adapter.layout_enabled());
        assert!(!adapter.allows_text_area_editable_behavior_path_switch());
        assert!(
            !behavior_status
                .readiness_report(&caret_metadata_source)
                .caret_affinity_behavior_path_ready
        );
        assert!(!behavior_status.report().ime_behavior_path_ready);
        assert!(!behavior_status.report().projection_behavior_path_ready);
        assert!(!behavior_status.report().scroll_follow_behavior_path_ready);
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_helper_reads_read_only_lookup_metadata() {
        let (behavior_status, caret_metadata_source, audit) =
            text_area_inline_ifc_caret_affinity_equivalence_fixture();
        let behavior_input =
            TextAreaEditableIfcCaretAffinityBehaviorInput::from_equivalence_audit(&audit);
        let evaluation =
            TextAreaEditableIfcCaretAffinityBehaviorEvaluation::evaluate(behavior_input.clone());
        let adapter =
            TextAreaEditableIfcCaretAffinityReadOnlyLookupAdapter::from_behavior_evaluation(
                &evaluation,
            );
        let helper = adapter
            .behavior_helper()
            .expect("observed read-only lookup should expose caret affinity helper");

        assert_eq!(
            helper.line_summary(),
            (
                behavior_input.visual_line_count,
                behavior_input.multi_stop_line_count
            )
        );
        assert_eq!(
            helper.stop_summary(),
            (
                behavior_input.caret_stop_count,
                behavior_input.multi_stop_line_count
            )
        );
        assert_eq!(
            helper.preedit_cursor_metadata(),
            (
                behavior_input.preedit_cursor_count,
                behavior_input
                    .per_run_caret_diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.has_preedit_cursor)
                    .map(|diagnostic| diagnostic.preedit_cursor)
                    .collect::<Vec<_>>()
            )
        );
        assert_eq!(
            helper.per_run_caret_diagnostics(),
            behavior_input.per_run_caret_diagnostics.as_slice()
        );
        assert_eq!(
            helper.caret_stop_snapshots(),
            collect_text_area_editable_ifc_caret_affinity_stop_snapshots(
                &behavior_input.per_run_caret_diagnostics
            )
            .as_slice()
        );
        assert_eq!(
            helper.placement_navigation_summary(),
            TextAreaEditableIfcCaretAffinityPlacementNavigationSummary {
                visual_line_count: behavior_input.visual_line_count,
                caret_stop_count: behavior_input.caret_stop_count,
                multi_stop_line_count: behavior_input.multi_stop_line_count,
                per_run_visual_line_counts: behavior_input
                    .per_run_caret_diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.visual_line_count)
                    .collect(),
                per_run_caret_stop_counts: behavior_input
                    .per_run_caret_diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.caret_stop_count)
                    .collect(),
                per_run_multi_stop_line_counts: behavior_input
                    .per_run_caret_diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.multi_stop_line_count)
                    .collect(),
                has_affinity_slots: behavior_input.multi_stop_line_count > 0,
                preedit_cursor_count: behavior_input.preedit_cursor_count,
                preedit_cursors: behavior_input
                    .per_run_caret_diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.has_preedit_cursor)
                    .map(|diagnostic| diagnostic.preedit_cursor)
                    .collect(),
                caret_stop_snapshot_count: behavior_input.caret_stop_count,
                run_local_char_indices_available: true,
                run_local_geometry_available: true,
            }
        );
        assert!(!helper.caret_affinity_behavior_path_ready());
        assert!(!helper.render_enabled());
        assert!(!helper.layout_enabled());
        assert!(!helper.allows_text_area_editable_behavior_path_switch());
        assert!(!adapter.caret_affinity_behavior_path_ready());
        assert!(!adapter.render_enabled());
        assert!(!adapter.layout_enabled());
        assert!(!adapter.allows_text_area_editable_behavior_path_switch());
        assert!(
            !behavior_status
                .readiness_report(&caret_metadata_source)
                .caret_affinity_behavior_path_ready
        );
        assert!(!behavior_status.report().ime_behavior_path_ready);
        assert!(!behavior_status.report().projection_behavior_path_ready);
        assert!(!behavior_status.report().scroll_follow_behavior_path_ready);
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_helper_provides_ifc_caret_placement() {
        let mut run = TextAreaTextRun::new(
            "caret helper provides visible caret placement from ifc adapter".to_string(),
            0..62,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 140.0);
        let seed_caret_lines = run.seed_caret_stops_for_ifc_snapshot();
        let seed_visual_line_count = seed_caret_lines.len();
        let seed_caret_stop_count = seed_caret_lines
            .iter()
            .map(|line| line.stops.len())
            .sum::<usize>();
        let seed_multi_stop_line_count = seed_caret_lines
            .iter()
            .filter(|line| line.stops.len() > 1)
            .count();
        let seed_caret_stop_snapshots = seed_caret_lines
            .iter()
            .enumerate()
            .flat_map(|(visual_line_index, line)| {
                line.stops
                    .iter()
                    .enumerate()
                    .map(
                        move |(stop_index, stop)| TextAreaEditableIfcCaretAffinityStopSnapshot {
                            run_index: 0,
                            visual_line_index,
                            stop_index,
                            local_char: stop.local_char,
                            local_x: stop.local_x,
                            local_y_top: stop.local_y_top,
                            height: stop.height,
                        },
                    )
            })
            .collect::<Vec<_>>();
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose read-only caret metadata");
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        let readiness_audit = TextAreaEditableIfcReadinessAudit::evaluate(
            TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
        );
        let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcImeBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&readiness_audit, &preflight),
        );
        let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
            TextAreaEditableIfcCaretAffinityMetadataSourceInput::
                from_readiness_audit_and_metadata_bridge_preflight(&readiness_audit, &preflight),
        );
        let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&readiness_audit, &preflight),
        );
        let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
                from_readiness_audit_and_metadata_bridge_preflight(&readiness_audit, &preflight),
        );
        let behavior_status = TextAreaEditableIfcBehaviorPathStatus::evaluate(
            TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
                &preflight,
                &readiness_audit,
                &ime_prewire,
                &caret_metadata_source,
                &projection_prewire,
                &scroll_prewire,
            ),
        );
        let audit = TextAreaEditableIfcCaretAffinityEquivalenceAudit::evaluate(
            TextAreaEditableIfcCaretAffinityEquivalenceAuditInput::
                from_behavior_status_and_caret_affinity_metadata_source(
                    &behavior_status,
                    &caret_metadata_source,
                ),
        );
        let behavior_input =
            TextAreaEditableIfcCaretAffinityBehaviorInput::from_equivalence_audit(&audit);
        let evaluation =
            TextAreaEditableIfcCaretAffinityBehaviorEvaluation::evaluate(behavior_input.clone());
        let adapter =
            TextAreaEditableIfcCaretAffinityReadOnlyLookupAdapter::from_behavior_evaluation(
                &evaluation,
            );
        let helper = adapter
            .behavior_helper()
            .expect("observed read-only lookup should expose caret affinity helper");
        let placement_adapter = adapter
            .placement_read_only_adapter()
            .expect("observed lookup should expose read-only placement adapter");
        let summary = helper.placement_navigation_summary();
        let snapshots = helper.caret_stop_snapshots();

        assert_eq!(summary.visual_line_count, seed_visual_line_count);
        assert_eq!(summary.caret_stop_count, seed_caret_stop_count);
        assert_eq!(summary.multi_stop_line_count, seed_multi_stop_line_count);
        assert_eq!(
            summary.per_run_visual_line_counts,
            vec![seed_visual_line_count]
        );
        assert_eq!(
            summary.per_run_caret_stop_counts,
            vec![seed_caret_stop_count]
        );
        assert_eq!(
            summary.per_run_multi_stop_line_counts,
            vec![seed_multi_stop_line_count]
        );
        assert!(summary.has_affinity_slots);
        assert_eq!(summary.preedit_cursor_count, 0);
        assert!(summary.preedit_cursors.is_empty());
        assert_eq!(summary.caret_stop_snapshot_count, seed_caret_stop_count);
        assert!(
            summary.run_local_char_indices_available,
            "read-only lookup should carry per-stop local_char"
        );
        assert!(
            summary.run_local_geometry_available,
            "read-only lookup should carry per-stop x/y/height geometry"
        );
        assert_eq!(snapshots, seed_caret_stop_snapshots.as_slice());
        assert_eq!(
            snapshots
                .iter()
                .map(|snapshot| snapshot.local_char)
                .collect::<Vec<_>>(),
            seed_caret_stop_snapshots
                .iter()
                .map(|snapshot| snapshot.local_char)
                .collect::<Vec<_>>()
        );
        assert!(
            snapshots.iter().all(|snapshot| snapshot.height > 0.0),
            "read-only snapshot should preserve usable caret geometry"
        );
        let first_snapshot = seed_caret_stop_snapshots
            .first()
            .expect("IFC snapshot seed caret stops should expose at least one snapshot");
        assert_eq!(
            helper.stop_geometry_summary(
                first_snapshot.run_index,
                first_snapshot.visual_line_index,
                first_snapshot.stop_index,
            ),
            Some(TextAreaEditableIfcCaretAffinityStopGeometrySummary {
                run_index: first_snapshot.run_index,
                visual_line_index: first_snapshot.visual_line_index,
                stop_index: first_snapshot.stop_index,
                local_char: first_snapshot.local_char,
                local_x: first_snapshot.local_x,
                local_y_top: first_snapshot.local_y_top,
                height: first_snapshot.height,
            })
        );
        let expected_local_char_candidates = seed_caret_stop_snapshots
            .iter()
            .filter(|snapshot| {
                snapshot.run_index == first_snapshot.run_index
                    && snapshot.local_char == first_snapshot.local_char
            })
            .enumerate()
            .map(|(candidate_index, snapshot)| {
                let affinity = if seed_caret_stop_snapshots
                    .iter()
                    .filter(|candidate| {
                        candidate.run_index == first_snapshot.run_index
                            && candidate.local_char == first_snapshot.local_char
                    })
                    .count()
                    > 1
                    && candidate_index == 0
                {
                    super::super::caret_map::CaretAffinity::Upstream
                } else {
                    super::super::caret_map::CaretAffinity::Downstream
                };
                TextAreaEditableIfcCaretAffinityLocalCharCandidate {
                    run_index: snapshot.run_index,
                    local_char: snapshot.local_char,
                    affinity,
                    visual_line_index: snapshot.visual_line_index,
                    stop_index: snapshot.stop_index,
                    local_x: snapshot.local_x,
                    local_y_top: snapshot.local_y_top,
                    height: snapshot.height,
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            helper.local_char_candidates(first_snapshot.run_index, first_snapshot.local_char),
            expected_local_char_candidates
        );
        assert!(
            helper
                .local_char_candidates(first_snapshot.run_index, usize::MAX)
                .is_empty(),
            "missing local_char must not fabricate read-only candidates"
        );
        let downstream_candidate = helper
            .local_char_candidate_with_affinity(
                first_snapshot.run_index,
                first_snapshot.local_char,
                super::super::caret_map::CaretAffinity::Downstream,
            )
            .expect("read-only lookup should expose downstream-like local_char candidate");
        let downstream_geometry = run
            .local_char_to_screen_position_with_affinity(
                first_snapshot.local_char,
                super::super::caret_map::CaretAffinity::Downstream,
            )
            .expect("IFC caret placement should expose downstream geometry");
        assert_eq!(
            (
                downstream_candidate.local_x,
                downstream_candidate.local_y_top,
                downstream_candidate.height,
            ),
            downstream_geometry
        );
        assert_eq!(
            run.caret_affinity_placement_position_from_ifc(
                first_snapshot.local_char,
                super::super::caret_map::CaretAffinity::Downstream,
            ),
            Some(downstream_geometry),
            "visible caret placement should be sourced from the IFC placement adapter"
        );
        assert_eq!(
            placement_adapter.local_char_candidate_with_affinity(
                first_snapshot.run_index,
                first_snapshot.local_char,
                super::super::caret_map::CaretAffinity::Downstream,
            ),
            Some(TextAreaEditableIfcCaretAffinityPlacementCandidate {
                run_index: downstream_candidate.run_index,
                local_char: downstream_candidate.local_char,
                affinity: downstream_candidate.affinity,
                local_x: downstream_candidate.local_x,
                local_y_top: downstream_candidate.local_y_top,
                height: downstream_candidate.height,
            })
        );
        assert_eq!(
            placement_adapter.local_char_to_run_local_position_with_affinity(
                first_snapshot.run_index,
                first_snapshot.local_char,
                super::super::caret_map::CaretAffinity::Downstream,
            ),
            Some(downstream_geometry)
        );
        let visible_caret_lines = run.caret_stops();
        assert_eq!(visible_caret_lines.len(), summary.visual_line_count);
        assert_eq!(
            visible_caret_lines
                .iter()
                .map(|line| line.stops.len())
                .sum::<usize>(),
            summary.caret_stop_count
        );
        assert_eq!(
            visible_caret_lines
                .iter()
                .enumerate()
                .flat_map(|(visual_line_index, line)| {
                    line.stops
                        .iter()
                        .enumerate()
                        .map(move |(stop_index, stop)| {
                            TextAreaEditableIfcCaretAffinityStopSnapshot {
                                run_index: 0,
                                visual_line_index,
                                stop_index,
                                local_char: stop.local_char,
                                local_x: stop.local_x,
                                local_y_top: stop.local_y_top,
                                height: stop.height,
                            }
                        })
                })
                .collect::<Vec<_>>(),
            snapshots,
            "visible caret_stops should be reconstructed from read-only IFC snapshots"
        );
        assert_eq!(
            visible_caret_lines
                .iter()
                .map(|line| (line.local_y_top, line.local_y_bottom))
                .collect::<Vec<_>>(),
            seed_caret_lines
                .iter()
                .map(|line| (line.local_y_top, line.local_y_bottom))
                .collect::<Vec<_>>(),
            "IFC snapshot-backed caret_stops should preserve visible line geometry"
        );
        let hit_test_pair = snapshots
            .windows(2)
            .find(|pair| {
                pair[0].run_index == 0
                    && pair[1].run_index == 0
                    && pair[0].visual_line_index == pair[1].visual_line_index
                    && pair[1].local_x > pair[0].local_x
            })
            .expect("fixture should expose adjacent IFC caret stops on one visual line");
        let left_snapshot = hit_test_pair[0].clone();
        let right_snapshot = hit_test_pair[1].clone();
        let midpoint =
            left_snapshot.local_x + (right_snapshot.local_x - left_snapshot.local_x) / 2.0;
        let left_hit_x = left_snapshot.local_x + (midpoint - left_snapshot.local_x) / 2.0;
        let right_hit_x = midpoint + (right_snapshot.local_x - midpoint) / 2.0;
        let hit_y = left_snapshot.local_y_top + left_snapshot.height / 2.0;
        assert_eq!(
            run.screen_position_to_local_char_from_ifc(left_hit_x, hit_y),
            Some(left_snapshot.local_char),
            "IFC helper hit-test should choose the nearest snapshot stop before the midpoint"
        );
        assert_eq!(
            run.screen_position_to_local_char(left_hit_x, hit_y),
            Some(left_snapshot.local_char),
            "visible hit-test should be sourced from the IFC snapshot helper"
        );
        assert_eq!(
            run.screen_position_to_local_char_from_ifc(right_hit_x, hit_y),
            Some(right_snapshot.local_char),
            "IFC helper hit-test should choose the nearest snapshot stop after the midpoint"
        );
        assert_eq!(
            run.screen_position_to_local_char(right_hit_x, hit_y),
            Some(right_snapshot.local_char),
            "visible hit-test should stay aligned with IFC snapshot-derived local chars"
        );
        assert!(
            helper
                .local_char_candidate_with_affinity(
                    first_snapshot.run_index,
                    usize::MAX,
                    super::super::caret_map::CaretAffinity::Downstream,
                )
                .is_none(),
            "missing local_char must not expose affinity candidate"
        );
        assert_eq!(
            run.caret_affinity_placement_position_from_ifc(
                usize::MAX,
                super::super::caret_map::CaretAffinity::Downstream,
            ),
            None,
            "missing adapter candidate must not expose IFC placement"
        );
        assert_eq!(
            run.local_char_to_screen_position_with_affinity(
                usize::MAX,
                super::super::caret_map::CaretAffinity::Downstream,
            ),
            None,
            "laid-out visible caret placement should require an IFC adapter candidate"
        );
        assert!(!helper.caret_affinity_behavior_path_ready());
        assert!(!adapter.render_enabled());
        assert!(!adapter.layout_enabled());
        assert!(!adapter.allows_text_area_editable_behavior_path_switch());
        assert!(
            !behavior_status
                .readiness_report(&caret_metadata_source)
                .caret_affinity_behavior_path_ready
        );
        assert!(!behavior_status.report().ime_behavior_path_ready);
        assert!(!behavior_status.report().projection_behavior_path_ready);
        assert!(!behavior_status.report().scroll_follow_behavior_path_ready);
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_behavior_evaluation_blocks_incomplete_input() {
        let (_behavior_status, _caret_metadata_source, audit) =
            text_area_inline_ifc_caret_affinity_equivalence_fixture();
        let mut behavior_input =
            TextAreaEditableIfcCaretAffinityBehaviorInput::from_equivalence_audit(&audit);
        behavior_input.equivalent_candidate_observed = false;
        behavior_input.caret_affinity_metadata_status =
            TextAreaInlineIfcMetadataBridgeStatus::Unwired;
        behavior_input.caret_stop_count = 0;
        behavior_input.multi_stop_line_count = 0;
        behavior_input.preedit_cursor_count += 1;

        let evaluation =
            TextAreaEditableIfcCaretAffinityBehaviorEvaluation::evaluate(behavior_input);

        assert_eq!(
            evaluation.state(),
            TextAreaEditableIfcCaretAffinityBehaviorEvaluationState::Blocked
        );
        for reason in [
            TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::
                EquivalentCandidateMissing,
            TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::
                CaretAffinityMetadataStatusNotObservedCaretStops,
            TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::MissingCaretStops,
            TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::MissingAffinitySlots,
            TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::
                CaretMetadataShapeMismatch,
            TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::
                PreeditCursorMetadataIncomplete,
        ] {
            assert!(
                evaluation.blocked_reasons().contains(&reason),
                "incomplete behavior input should block on {reason:?}"
            );
        }
        assert!(!evaluation.diagnostic().input_observed);
        assert!(!evaluation.caret_affinity_behavior_path_ready());
        assert!(!evaluation.render_enabled());
        assert!(!evaluation.layout_enabled());
        assert!(!evaluation.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_read_only_lookup_blocks_incomplete_evaluation() {
        let (_behavior_status, _caret_metadata_source, audit) =
            text_area_inline_ifc_caret_affinity_equivalence_fixture();
        let mut behavior_input =
            TextAreaEditableIfcCaretAffinityBehaviorInput::from_equivalence_audit(&audit);
        behavior_input.equivalent_candidate_observed = false;
        behavior_input.caret_affinity_metadata_status =
            TextAreaInlineIfcMetadataBridgeStatus::Unwired;
        behavior_input.visual_line_count = 0;
        behavior_input.caret_stop_count = 0;
        behavior_input.multi_stop_line_count = 0;
        behavior_input.preedit_cursor_count += 1;

        let evaluation =
            TextAreaEditableIfcCaretAffinityBehaviorEvaluation::evaluate(behavior_input);
        let adapter =
            TextAreaEditableIfcCaretAffinityReadOnlyLookupAdapter::from_behavior_evaluation(
                &evaluation,
            );

        assert_eq!(
            adapter.state(),
            TextAreaEditableIfcCaretAffinityReadOnlyLookupState::Blocked
        );
        assert_eq!(
            adapter.blocked_reasons(),
            &[TextAreaEditableIfcCaretAffinityReadOnlyLookupBlockedReason::
                BehaviorEvaluationNotObserved]
        );
        assert!(adapter.lookup().is_none());
        assert!(adapter.behavior_helper().is_none());
        assert!(adapter.placement_read_only_adapter().is_none());
        assert!(!adapter.caret_affinity_behavior_path_ready());
        assert!(!adapter.render_enabled());
        assert!(!adapter.layout_enabled());
        assert!(!adapter.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_equivalence_audit_blocks_missing_or_incomplete_sources()
    {
        let (behavior_status, caret_metadata_source, _audit) =
            text_area_inline_ifc_caret_affinity_equivalence_fixture();
        let base_input = TextAreaEditableIfcCaretAffinityEquivalenceAuditInput::
            from_behavior_status_and_caret_affinity_metadata_source(&behavior_status, &caret_metadata_source);

        let mut missing_stops_input = base_input.clone();
        missing_stops_input.visual_line_count = 0;
        missing_stops_input.caret_stop_count = 0;
        missing_stops_input.per_run_caret_diagnostics.clear();
        let missing_stops =
            TextAreaEditableIfcCaretAffinityEquivalenceAudit::evaluate(missing_stops_input);
        assert_eq!(
            missing_stops.state(),
            TextAreaEditableIfcCaretAffinityEquivalenceAuditState::Blocked
        );
        assert!(missing_stops.blocked_reasons().contains(
            &TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::MissingCaretStops
        ));
        assert!(missing_stops.blocked_reasons().contains(
            &TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::
                CaretAffinityMetadataSourceMissing
        ));
        assert_eq!(
            missing_stops.diagnostic().equivalent_candidate,
            TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate::None
        );

        let mut no_affinity_slots_input = base_input.clone();
        no_affinity_slots_input.multi_stop_line_count = 0;
        for diagnostic in &mut no_affinity_slots_input.per_run_caret_diagnostics {
            diagnostic.multi_stop_line_count = 0;
        }
        let no_affinity_slots =
            TextAreaEditableIfcCaretAffinityEquivalenceAudit::evaluate(no_affinity_slots_input);
        assert_eq!(
            no_affinity_slots.state(),
            TextAreaEditableIfcCaretAffinityEquivalenceAuditState::Blocked
        );
        assert!(no_affinity_slots.blocked_reasons().contains(
            &TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::MissingAffinitySlots
        ));
        assert_eq!(
            no_affinity_slots.diagnostic().equivalent_candidate,
            TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate::None
        );

        let mut missing_metadata_source_input = base_input;
        missing_metadata_source_input.caret_affinity_metadata_source_state =
            TextAreaEditableIfcCaretAffinityMetadataSourceState::Blocked;
        missing_metadata_source_input.caret_affinity_metadata_observed = false;
        missing_metadata_source_input.caret_affinity_metadata_status =
            TextAreaInlineIfcMetadataBridgeStatus::Unwired;
        missing_metadata_source_input
            .per_run_caret_diagnostics
            .clear();
        let missing_metadata_source = TextAreaEditableIfcCaretAffinityEquivalenceAudit::evaluate(
            missing_metadata_source_input,
        );
        assert_eq!(
            missing_metadata_source.state(),
            TextAreaEditableIfcCaretAffinityEquivalenceAuditState::Blocked
        );
        for reason in [
            TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::
                CaretAffinityMetadataSourceMissing,
            TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::
                CaretAffinityMetadataSourceNotObserved,
            TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::
                CaretAffinityMetadataStatusNotObservedCaretStops,
        ] {
            assert!(
                missing_metadata_source.blocked_reasons().contains(&reason),
                "missing metadata source should block on {reason:?}"
            );
        }
        assert_eq!(
            missing_metadata_source.diagnostic().equivalent_candidate,
            TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate::None
        );
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_equivalence_audit_keeps_behavior_path_switch_blocked() {
        let (behavior_status, caret_metadata_source, audit) =
            text_area_inline_ifc_caret_affinity_equivalence_fixture();

        assert_eq!(
            behavior_status.state(),
            TextAreaEditableIfcBehaviorPathStatusState::ReadyForStatusObservation
        );
        assert!(
            behavior_status
                .behavior_path_switch_blocked_reasons()
                .contains(
                    &TextAreaEditableIfcBehaviorPathStatusBlockedReason::BehaviorPathsStillNotReady
                )
        );
        assert!(!behavior_status.render_enabled());
        assert!(!behavior_status.layout_enabled());
        assert!(!behavior_status.allows_text_area_editable_behavior_path_switch());
        assert!(!behavior_status.report().caret_affinity_behavior_path_ready);
        assert!(!caret_metadata_source.caret_affinity_behavior_path_ready());
        assert!(
            !behavior_status
                .readiness_report(&caret_metadata_source)
                .caret_affinity_behavior_path_ready
        );
        assert!(!audit.caret_affinity_behavior_path_ready());
        assert!(!audit.diagnostic().caret_affinity_behavior_path_ready);
        assert!(!audit.render_enabled());
        assert!(!audit.layout_enabled());
        assert!(!audit.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_equivalence_audit_keeps_other_paths_blocked() {
        let (_decision, caret_metadata_source, audit) =
            text_area_inline_ifc_caret_affinity_equivalence_fixture();
        let ready_report = _decision.readiness_report(&caret_metadata_source);

        assert_eq!(
            audit.diagnostic().ime_readiness_state,
            TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
        );
        assert_eq!(
            audit.diagnostic().projection_readiness_state,
            TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
        );
        assert_eq!(
            audit.diagnostic().scroll_follow_readiness_state,
            TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
        );
        assert!(!audit.diagnostic().ime_behavior_path_ready);
        assert!(!audit.diagnostic().projection_behavior_path_ready);
        assert!(!audit.diagnostic().scroll_follow_behavior_path_ready);
        assert_eq!(
            ready_report.ime_readiness_state,
            TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
        );
        assert_eq!(
            ready_report.projection_readiness_state,
            TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
        );
        assert_eq!(
            ready_report.scroll_follow_readiness_state,
            TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
        );
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_equivalence_audit_preserves_layout_with_prepared_default()
     {
        let (_decision, _caret_metadata_source, audit) =
            text_area_inline_ifc_caret_affinity_equivalence_fixture();
        let mut run = TextAreaTextRun::new(
            "caret equivalence audit keeps legacy layout and render unchanged".to_string(),
            0..66,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let before_size = run.layout_state.layout_size;
        let before_fragments = run.inline_fragment_positions();

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(180, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        run.build(&mut graph, &mut arena, ctx);
        let pass_names = graph
            .pass_descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name.to_string())
            .collect::<Vec<_>>();

        assert_eq!(pass_names.len(), 1);
        assert!(
            pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
            "TextArea caret equivalence audit does not own render switching; TextArea default render should emit TextPreparedInputPass for a valid candidate, got {}",
            pass_names[0]
        );
        assert_eq!(before_size.width, run.layout_state.layout_size.width);
        assert_eq!(before_size.height, run.layout_state.layout_size.height);
        assert_eq!(before_fragments, run.inline_fragment_positions());
        assert!(!audit.render_enabled());
        assert!(!audit.layout_enabled());
        assert!(!audit.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_caret_affinity_equivalence_keeps_production_not_ready() {
        let (behavior_status, caret_audit, _projection, _scroll, _ime) =
            text_area_inline_ifc_all_equivalence_fixture();

        assert_eq!(
            caret_audit.state(),
            TextAreaEditableIfcCaretAffinityEquivalenceAuditState::ReadyForObservationOnly
        );
        assert!(caret_audit.blocked_reasons().is_empty());
        assert_eq!(
            caret_audit.diagnostic().equivalent_candidate,
            TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate::EquivalentCandidateObserved
        );
        assert_eq!(
            caret_audit.diagnostic().caret_affinity_readiness_state,
            TextAreaEditableIfcBehaviorPathReadinessState::CaretAffinityCandidateObserved
        );
        assert_eq!(
            caret_audit.diagnostic().projection_readiness_state,
            TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
        );
        assert_eq!(
            caret_audit.diagnostic().scroll_follow_readiness_state,
            TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
        );
        assert_eq!(
            caret_audit.diagnostic().ime_readiness_state,
            TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
        );
        assert!(!caret_audit.caret_affinity_behavior_path_ready());
        assert!(!caret_audit.diagnostic().caret_affinity_behavior_path_ready);
        assert_eq!(
            behavior_status.state(),
            TextAreaEditableIfcBehaviorPathStatusState::ReadyForStatusObservation
        );
        assert!(
            behavior_status
                .behavior_path_switch_blocked_reasons()
                .contains(
                    &TextAreaEditableIfcBehaviorPathStatusBlockedReason::BehaviorPathsStillNotReady
                )
        );
        assert!(!caret_audit.render_enabled());
        assert!(!caret_audit.layout_enabled());
        assert!(!caret_audit.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_projection_equivalence_audit_observes_and_blocks_missing_sources() {
        let (_decision, _caret_audit, projection, _scroll, _ime) =
            text_area_inline_ifc_all_equivalence_fixture();

        assert_eq!(
            projection.state(),
            TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly
        );
        assert!(projection.blocked_reasons().is_empty());
        assert_eq!(
            projection.diagnostic().equivalent_candidate,
            TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::EquivalentCandidateObserved
        );
        assert_eq!(
            projection.diagnostic().projection_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::Observed
        );
        assert_eq!(projection.diagnostic().projection_segment_count, 2);
        assert_eq!(projection.diagnostic().run_count, 1);
        assert_eq!(projection.diagnostic().char_range_count, 1);
        assert_eq!(
            projection.diagnostic().per_run_projection_diagnostics.len(),
            1
        );
        assert!(!projection.projection_behavior_path_ready());
        assert!(!projection.render_enabled());
        assert!(!projection.layout_enabled());
        assert!(!projection.allows_text_area_editable_behavior_path_switch());

        let mut no_segments_input = projection.input().clone();
        no_segments_input.projection_prewire_state =
            TextAreaEditableIfcProjectionBehaviorPathPrewireState::ObservedNoProjectionSegments;
        no_segments_input.projection_metadata_status =
            TextAreaInlineIfcMetadataBridgeStatus::ObservedNoProjectionSegments;
        let diagnostic = no_segments_input
            .projection_metadata_diagnostic
            .as_mut()
            .expect("fixture should expose projection diagnostic");
        diagnostic.projection_metadata_status =
            TextAreaInlineIfcMetadataBridgeStatus::ObservedNoProjectionSegments;
        diagnostic.projection_segment_count = 0;
        for per_run in &mut diagnostic.per_run_projection_diagnostics {
            per_run.projection_segment_count = 0;
        }
        let no_segments =
            TextAreaEditableIfcProjectionEquivalenceAudit::evaluate(no_segments_input);
        assert_eq!(
            no_segments.state(),
            TextAreaEditableIfcBehaviorEquivalenceAuditState::Blocked
        );
        assert!(no_segments.blocked_reasons().contains(
            &TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::MissingProjectionSegments
        ));
        assert!(no_segments.blocked_reasons().contains(
            &TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
                ProjectionMetadataStatusNotObserved
        ));
        assert_eq!(
            no_segments.diagnostic().equivalent_candidate,
            TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::None
        );

        let mut missing_input = projection.input().clone();
        missing_input.projection_prewire_state =
            TextAreaEditableIfcProjectionBehaviorPathPrewireState::Blocked;
        missing_input.projection_diagnostic_prewired = false;
        missing_input.projection_metadata_status = TextAreaInlineIfcMetadataBridgeStatus::Unwired;
        missing_input.projection_metadata_diagnostic = None;
        let missing = TextAreaEditableIfcProjectionEquivalenceAudit::evaluate(missing_input);
        assert_eq!(
            missing.state(),
            TextAreaEditableIfcBehaviorEquivalenceAuditState::Blocked
        );
        for reason in [
            TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
                MissingProjectionMetadataDiagnostic,
            TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
                ProjectionPrewireMissing,
            TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
                ProjectionMetadataSourceNotObserved,
            TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
                ProjectionMetadataStatusNotObserved,
        ] {
            assert!(
                missing.blocked_reasons().contains(&reason),
                "missing projection audit source should block on {reason:?}"
            );
        }
    }

    #[test]
    fn text_area_inline_ifc_scroll_follow_equivalence_audit_observes_and_blocks_missing_sources() {
        let (_decision, _caret_audit, _projection, scroll, _ime) =
            text_area_inline_ifc_all_equivalence_fixture();

        assert_eq!(
            scroll.state(),
            TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly
        );
        assert!(scroll.blocked_reasons().is_empty());
        assert_eq!(
            scroll.diagnostic().equivalent_candidate,
            TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::EquivalentCandidateObserved
        );
        assert_eq!(
            scroll.diagnostic().scroll_follow_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::Observed
        );
        assert_eq!(scroll.diagnostic().layout_size_count, 1);
        assert!(scroll.diagnostic().visual_line_count > 0);
        assert!(scroll.diagnostic().caret_stop_count > 0);
        assert!(!scroll.scroll_follow_behavior_path_ready());
        assert!(!scroll.render_enabled());
        assert!(!scroll.layout_enabled());
        assert!(!scroll.allows_text_area_editable_behavior_path_switch());

        let mut no_source_input = scroll.input().clone();
        no_source_input.scroll_follow_prewire_state =
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::ObservedNoScrollSource;
        no_source_input.scroll_follow_metadata_status =
            TextAreaInlineIfcMetadataBridgeStatus::ObservedNoScrollSource;
        let diagnostic = no_source_input
            .scroll_follow_metadata_diagnostic
            .as_mut()
            .expect("fixture should expose scroll diagnostic");
        diagnostic.scroll_follow_metadata_status =
            TextAreaInlineIfcMetadataBridgeStatus::ObservedNoScrollSource;
        diagnostic.layout_size_count = 0;
        diagnostic.visual_line_count = 0;
        diagnostic.caret_stop_count = 0;
        for per_run in &mut diagnostic.per_run_scroll_follow_diagnostics {
            per_run.layout_size = [0.0, 0.0];
            per_run.visual_line_count = 0;
            per_run.caret_stop_count = 0;
        }
        let no_source = TextAreaEditableIfcScrollFollowEquivalenceAudit::evaluate(no_source_input);
        assert_eq!(
            no_source.state(),
            TextAreaEditableIfcBehaviorEquivalenceAuditState::Blocked
        );
        assert!(no_source.blocked_reasons().contains(
            &TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason::MissingScrollSource
        ));
        assert!(no_source.blocked_reasons().contains(
            &TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason::
                ScrollFollowMetadataStatusNotObserved
        ));

        let mut missing_input = scroll.input().clone();
        missing_input.scroll_follow_prewire_state =
            TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::Blocked;
        missing_input.scroll_follow_diagnostic_prewired = false;
        missing_input.scroll_follow_metadata_status =
            TextAreaInlineIfcMetadataBridgeStatus::Unwired;
        missing_input.scroll_follow_metadata_diagnostic = None;
        let missing = TextAreaEditableIfcScrollFollowEquivalenceAudit::evaluate(missing_input);
        assert_eq!(
            missing.state(),
            TextAreaEditableIfcBehaviorEquivalenceAuditState::Blocked
        );
        assert!(missing.blocked_reasons().contains(
            &TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason::
                MissingScrollFollowMetadataDiagnostic
        ));
        assert!(missing.blocked_reasons().contains(
            &TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason::
                ScrollFollowPrewireMissing
        ));
    }

    #[test]
    fn text_area_inline_ifc_ime_equivalence_audit_observes_preedit_and_blocks_no_preedit() {
        let (_decision, _caret_audit, _projection, _scroll, ime) =
            text_area_inline_ifc_all_equivalence_fixture();

        assert_eq!(
            ime.state(),
            TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly
        );
        assert!(ime.blocked_reasons().is_empty());
        assert_eq!(
            ime.diagnostic().equivalent_candidate,
            TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::EquivalentCandidateObserved
        );
        assert_eq!(
            ime.diagnostic().ime_metadata_status,
            TextAreaInlineIfcMetadataBridgeStatus::Observed
        );
        assert!(ime.diagnostic().has_inline_preedit);
        assert!(ime.diagnostic().has_preedit_run);
        assert_eq!(ime.diagnostic().preedit_cursor_count, 1);
        assert_eq!(ime.diagnostic().preedit_cursors, vec![(1, 3)]);
        assert!(!ime.ime_behavior_path_ready());
        assert!(!ime.render_enabled());
        assert!(!ime.layout_enabled());
        assert!(!ime.allows_text_area_editable_behavior_path_switch());

        let mut no_preedit_input = ime.input().clone();
        no_preedit_input.ime_prewire_state =
            TextAreaEditableIfcImeBehaviorPathPrewireState::ObservedNoPreedit;
        no_preedit_input.ime_metadata_status =
            TextAreaInlineIfcMetadataBridgeStatus::ObservedNoPreedit;
        let diagnostic = no_preedit_input
            .ime_metadata_diagnostic
            .as_mut()
            .expect("fixture should expose IME diagnostic");
        diagnostic.ime_metadata_status = TextAreaInlineIfcMetadataBridgeStatus::ObservedNoPreedit;
        diagnostic.has_inline_preedit = false;
        diagnostic.has_preedit_run = false;
        diagnostic.preedit_cursor_count = 0;
        diagnostic.preedit_cursors.clear();
        let no_preedit = TextAreaEditableIfcImeEquivalenceAudit::evaluate(no_preedit_input);
        assert_eq!(
            no_preedit.state(),
            TextAreaEditableIfcBehaviorEquivalenceAuditState::Blocked
        );
        assert!(
            no_preedit
                .blocked_reasons()
                .contains(&TextAreaEditableIfcImeEquivalenceAuditBlockedReason::NoPreeditCase),
            "no-preedit metadata must not be promoted to IME observation-ready"
        );
        assert!(no_preedit.blocked_reasons().contains(
            &TextAreaEditableIfcImeEquivalenceAuditBlockedReason::ImeMetadataStatusNotObserved
        ));
        assert_eq!(
            no_preedit.diagnostic().equivalent_candidate,
            TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::None
        );
    }

    #[test]
    fn text_area_inline_ifc_equivalence_audits_keep_status_observation_only_surface() {
        let (behavior_status, caret, projection, scroll, ime) =
            text_area_inline_ifc_all_equivalence_fixture();

        assert_eq!(
            caret.state(),
            TextAreaEditableIfcCaretAffinityEquivalenceAuditState::ReadyForObservationOnly
        );
        assert_eq!(
            caret.diagnostic().equivalent_candidate,
            TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate::EquivalentCandidateObserved
        );
        assert_eq!(
            projection.state(),
            TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly
        );
        assert_eq!(
            scroll.state(),
            TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly
        );
        assert_eq!(
            ime.state(),
            TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly
        );
        assert_eq!(
            projection.diagnostic().equivalent_candidate,
            TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::EquivalentCandidateObserved
        );
        assert_eq!(
            scroll.diagnostic().equivalent_candidate,
            TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::EquivalentCandidateObserved
        );
        assert_eq!(
            ime.diagnostic().equivalent_candidate,
            TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::EquivalentCandidateObserved
        );
        assert!(!ime.ime_behavior_path_ready());
        assert!(!caret.caret_affinity_behavior_path_ready());
        assert!(!projection.projection_behavior_path_ready());
        assert!(!scroll.scroll_follow_behavior_path_ready());
        assert!(!ime.render_enabled());
        assert!(!caret.render_enabled());
        assert!(!projection.render_enabled());
        assert!(!scroll.render_enabled());
        assert!(!ime.layout_enabled());
        assert!(!caret.layout_enabled());
        assert!(!projection.layout_enabled());
        assert!(!scroll.layout_enabled());
        assert!(!ime.allows_text_area_editable_behavior_path_switch());
        assert!(!caret.allows_text_area_editable_behavior_path_switch());
        assert!(!projection.allows_text_area_editable_behavior_path_switch());
        assert!(!scroll.allows_text_area_editable_behavior_path_switch());
        assert_eq!(
            behavior_status.state(),
            TextAreaEditableIfcBehaviorPathStatusState::ReadyForStatusObservation
        );
        assert!(
            behavior_status
                .behavior_path_switch_blocked_reasons()
                .contains(
                    &TextAreaEditableIfcBehaviorPathStatusBlockedReason::BehaviorPathsStillNotReady
                )
        );
        assert!(!behavior_status.render_enabled());
        assert!(!behavior_status.layout_enabled());
        assert!(!behavior_status.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_behavior_path_status_preserves_layout_with_prepared_default() {
        let (behavior_status, _caret_audit, _projection, _scroll, _ime) =
            text_area_inline_ifc_all_equivalence_fixture();
        let mut run = TextAreaTextRun::new(
            "behavior path audit keeps layout unchanged with prepared default".to_string(),
            0..72,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let before_size = run.layout_state.layout_size;
        let before_fragments = run.inline_fragment_positions();

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(180, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        run.build(&mut graph, &mut arena, ctx);
        let pass_names = graph
            .pass_descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name.to_string())
            .collect::<Vec<_>>();

        assert_eq!(pass_names.len(), 1);
        assert!(
            pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
            "TextArea behavior-path status does not own render switching; TextArea default render should emit TextPreparedInputPass for a valid candidate, got {}",
            pass_names[0]
        );
        assert_eq!(before_size.width, run.layout_state.layout_size.width);
        assert_eq!(before_size.height, run.layout_state.layout_size.height);
        assert_eq!(before_fragments, run.inline_fragment_positions());
        assert!(behavior_status.report().prepared_render_default_observed);
        assert!(!behavior_status.render_enabled());
        assert!(!behavior_status.layout_enabled());
        assert!(!behavior_status.allows_text_area_editable_behavior_path_switch());
    }

    #[test]
    fn text_area_inline_ifc_metadata_bridge_uses_default_prepared_render_graph() {
        let mut run = TextAreaTextRun::new(
            "metadata bridge uses default prepared text pass".to_string(),
            0..53,
        );
        let mut arena = NodeArena::new();
        place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
        let payload = run
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .expect("laid out run should expose metadata bridge payload");
        let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
            TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
        );
        let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
        assert!(!preflight.render_enabled());

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(180, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        run.build(&mut graph, &mut arena, ctx);
        let pass_names = graph
            .pass_descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name.to_string())
            .collect::<Vec<_>>();

        assert_eq!(pass_names.len(), 1);
        assert!(
            pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
            "TextArea metadata bridge does not own render switching; TextArea default render should emit TextPreparedInputPass for a valid candidate, got {}",
            pass_names[0]
        );
    }
}
