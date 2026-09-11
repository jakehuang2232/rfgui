//! `Renderable` impl for Text: emits the prepared glyph pass + selection
//! rects, consuming the same shaped context measure produced.

use crate::view::base_component::{BuildState, Renderable, UiBuildContext};
use crate::view::frame_graph::FrameGraph;
use crate::view::node_arena::NodeArena;
use crate::view::render_pass::DrawRectPass;
use crate::view::render_pass::draw_rect_pass::{
    DrawRectInput, DrawRectOutput, RectPassParams, RectRenderMode,
};
use crate::view::render_pass::text_pass::TextPreparedInputPass;
use crate::view::render_pass::text_pass::{
    TextInput, TextOutput, TextPassPreparedFragment, TextPassPreparedParams,
};

use super::super::ShadowPaintBlocker;
use super::Text;
use super::hit_test::current_text_area_selection_render_context;
use super::paint_cache::{TextPaintKey, TextPaintMemo};
use crate::view::inline_text_pass_adapter::{
    inline_ifc_paint_input_to_text_pass_staging_input,
    inline_ifc_paint_input_to_text_pass_staging_input_with_color,
};
use std::sync::Arc;

impl Renderable for Text {
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        _arena: &mut NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        let opacity = self.opacity.clamp(0.0, 1.0);
        if !self.is_paint_visible(opacity) {
            return ctx.into_state();
        }

        let Some(input_target) = ctx.current_target() else {
            return ctx.into_state();
        };
        self.emit_selection_underlay(graph, &mut ctx);
        let params = if let Some(input) = self.inline_ifc_owned_paint_input() {
            let paint_bounds = self
                .inline_ifc_owned_paint_bounds()
                .expect("inline-owned paint input must install paint bounds");
            let [x, y] = ctx.paint_point(paint_bounds.x, paint_bounds.y);
            let staging_input =
                inline_ifc_paint_input_to_text_pass_staging_input(input, [x, y], opacity, 0, 1.0);
            if staging_input.glyphs.is_empty() {
                return ctx.into_state();
            }
            TextPassPreparedParams {
                staging_input,
                fragments: vec![TextPassPreparedFragment {
                    origin: [x, y],
                    size: [paint_bounds.width, paint_bounds.height],
                }],
                scissor_rect: None,
                stencil_clip_id: None,
            }
        } else {
            let Ok(payload) = self.prepared_standalone_text_payload(ctx.paint_offset(), opacity)
            else {
                return ctx.into_state();
            };
            let Some(params) = payload.params else {
                return ctx.into_state();
            };
            params
        };
        let pass = TextPreparedInputPass::new(
            params,
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

impl Text {
    /// Closed admission oracle for the nested-scroll direct Text primitive.
    /// Inline-IFC-owned geometry and zero-op/unprepared standalone payloads
    /// remain outside this authority even though the general Text recorder can
    /// represent them in other frame-artifact paths.
    pub(crate) fn is_exact_standalone_retained_text_leaf(&self) -> bool {
        if self.inline_ifc_owned.is_some() {
            return false;
        }
        let Ok(payload) = self.prepared_standalone_text_payload([0.0, 0.0], self.opacity) else {
            return false;
        };
        let Some(params) = payload.params else {
            return false;
        };
        params.fragments.len() == 1
            && !params.staging_input.glyphs.is_empty()
            && params.staging_input.scale_factor.to_bits() == 1.0_f32.to_bits()
            && params
                .staging_input
                .glyphs
                .iter()
                .all(|glyph| glyph.paint.fragment_index == 0)
            && params.scissor_rect.is_none()
            && params.stencil_clip_id.is_none()
            && crate::view::paint::PreparedTextOp::new(params).is_some()
    }

    /// Exact visibility gate shared by legacy build and retained recording.
    /// Callers supply the effective opacity. Neutral root-effect recording
    /// uses 1.0, while baked paths use the clamped local opacity.
    pub(super) fn is_paint_visible(&self, effective_opacity: f32) -> bool {
        self.layout_state.should_render
            && !self.content.is_empty()
            && effective_opacity.is_finite()
            && effective_opacity > 0.0
    }

    pub(super) fn standalone_paint_bounds(&self) -> super::super::Rect {
        super::super::Rect {
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self.size.width.max(self.layout_state.layout_size.width),
            height: self.size.height.max(self.layout_state.layout_size.height),
        }
    }

    /// Pure retained-paint bridge shared by legacy and artifact recording.
    /// It only borrows the payload eagerly prepared by `measure`.
    pub(super) fn prepared_standalone_text_payload(
        &self,
        paint_offset: [f32; 2],
        opacity: f32,
    ) -> Result<StandaloneTextPaintPayload, ShadowPaintBlocker> {
        if self.inline_ifc_owned.is_some() {
            return Err(ShadowPaintBlocker::InlineIfc);
        }
        let bounds = self.standalone_paint_bounds();
        if !self.is_paint_visible(opacity) {
            return Ok(StandaloneTextPaintPayload { params: None });
        }
        let context = self
            .shaped_context
            .as_ref()
            .ok_or(ShadowPaintBlocker::MissingPreparedText)?;
        let paint_input = context
            .prepared_text_pass_paint_input_ref()
            .ok_or(ShadowPaintBlocker::MissingPreparedText)?;
        let origin = [bounds.x + paint_offset[0], bounds.y + paint_offset[1]];
        let staging_input = inline_ifc_paint_input_to_text_pass_staging_input_with_color(
            paint_input,
            origin,
            opacity,
            0,
            1.0,
            Some(self.color.to_rgba_f32()),
        );
        let params = (!staging_input.glyphs.is_empty()).then(|| TextPassPreparedParams {
            staging_input,
            fragments: vec![TextPassPreparedFragment {
                origin,
                size: [bounds.width, bounds.height],
            }],
            scissor_rect: None,
            stencil_clip_id: None,
        });
        Ok(StandaloneTextPaintPayload { params })
    }

    /// Resolve the same immutable source for capability and recording. Empty
    /// paint stays distinct from a missing or invalid prepared payload.
    fn shadow_text_paint_source(
        &self,
        opacity: f32,
    ) -> Result<ShadowTextPaintSource<'_>, ShadowPaintBlocker> {
        if let Some(input) = self.inline_ifc_owned_paint_input() {
            let installed = self
                .inline_ifc_owned_paint_bounds()
                .ok_or(ShadowPaintBlocker::MissingPreparedText)?;
            let bounds = super::super::Rect {
                x: installed.x,
                y: installed.y,
                width: installed.width,
                height: installed.height,
            };
            if !self.is_paint_visible(opacity) {
                return Ok(ShadowTextPaintSource {
                    bounds,
                    input: None,
                    color_override: None,
                });
            }
            if input.glyphs.is_empty()
                && self
                    .content
                    .chars()
                    .any(|character| !character.is_whitespace())
            {
                return Err(ShadowPaintBlocker::MissingPreparedText);
            }
            return Ok(ShadowTextPaintSource {
                bounds,
                input: Some(input),
                color_override: None,
            });
        }
        let bounds = self.standalone_paint_bounds();
        if !self.is_paint_visible(opacity) {
            return Ok(ShadowTextPaintSource {
                bounds,
                input: None,
                color_override: None,
            });
        }
        let input = self
            .shaped_context
            .as_ref()
            .and_then(|context| context.prepared_text_pass_paint_input_ref())
            .ok_or(ShadowPaintBlocker::MissingPreparedText)?;
        Ok(ShadowTextPaintSource {
            bounds,
            input: Some(input),
            color_override: Some(self.color.to_rgba_f32()),
        })
    }

    pub(super) fn validate_shadow_text_payload(
        &self,
        paint_offset: [f32; 2],
        opacity: f32,
    ) -> Result<(), ShadowPaintBlocker> {
        let source = self.shadow_text_paint_source(opacity)?;
        let Some(input) = source.input.filter(|input| !input.glyphs.is_empty()) else {
            return Ok(());
        };
        let fragment = source.fragment(paint_offset);
        let key = TextPaintKey::new(self, &source, paint_offset, opacity);
        if self
            .paint_memo
            .borrow()
            .as_ref()
            .is_some_and(|memo| memo.key.matches(&key))
        {
            return Ok(());
        }
        if !crate::view::paint::PreparedTextOp::validate_unclipped_glyph_stream(
            1.0,
            &[fragment],
            source.staged_glyphs(input, fragment.origin, opacity),
        ) {
            return Err(ShadowPaintBlocker::MissingPreparedText);
        }
        Ok(())
    }

    /// Recording materializes the glyph vector and frozen identity on a cache miss.
    /// Capability uses the same source, conversion and field validators as a
    /// stream, so it does not build a complete op merely to discard it.
    pub(super) fn prepared_shadow_text_payload(
        &self,
        paint_offset: [f32; 2],
        opacity: f32,
    ) -> Result<Arc<PreparedShadowTextPayload>, ShadowPaintBlocker> {
        let source = self.shadow_text_paint_source(opacity)?;
        let Some(input) = source.input.filter(|input| !input.glyphs.is_empty()) else {
            return Ok(Arc::new(PreparedShadowTextPayload {
                bounds: source.bounds,
                op: None,
            }));
        };
        let key = TextPaintKey::new(self, &source, paint_offset, opacity);
        if let Some(memo) = self
            .paint_memo
            .borrow()
            .as_ref()
            .filter(|memo| memo.key.matches(&key))
        {
            return Ok(memo.payload.clone());
        }
        let fragment = source.fragment(paint_offset);
        let params = TextPassPreparedParams {
            staging_input: crate::view::render_pass::text_pass::TextPassPreparedStagingInput {
                scale_factor: 1.0,
                glyphs: source
                    .staged_glyphs(input, fragment.origin, opacity)
                    .collect(),
            },
            fragments: vec![fragment],
            scissor_rect: None,
            stencil_clip_id: None,
        };
        let op = crate::view::paint::PreparedTextOp::new(params)
            .ok_or(ShadowPaintBlocker::MissingPreparedText)?;
        let payload = Arc::new(PreparedShadowTextPayload {
            bounds: source.bounds,
            op: Some(op),
        });
        *self.paint_memo.borrow_mut() = Some(TextPaintMemo {
            key,
            payload: payload.clone(),
        });
        Ok(payload)
    }

    /// Emit the TextArea-selection underlay rects for this Text when a
    /// selection render context is active. Works for both self-rendered
    /// and inline-IFC-owned texts (owned geometry answers the local
    /// selection query per fragment).
    fn emit_selection_underlay(
        &self,
        graph: &mut crate::view::frame_graph::FrameGraph,
        ctx: &mut UiBuildContext,
    ) {
        let Some(selection) = current_text_area_selection_render_context() else {
            return;
        };
        if ctx.current_target().is_none() {
            return;
        }
        for rect in self.local_selection_screen_rects(selection.start, selection.end) {
            let [rect_x, rect_y] = ctx.paint_point(rect.x, rect.y);
            let mut selection_pass = DrawRectPass::new(
                RectPassParams {
                    position: [rect_x, rect_y],
                    size: [rect.width.max(1.0), rect.height.max(1.0)],
                    fill_color: selection.fill,
                    opacity: 1.0,
                    ..Default::default()
                },
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            selection_pass.set_render_mode(RectRenderMode::FillOnly);
            ctx.emit_draw_rect_pass(graph, selection_pass);
        }
    }

    /// Staging input for the prepared glyph pass, built from the shaped
    /// context measure installed. Live color is injected here so color
    /// changes repaint without reshaping.
    #[cfg(test)]
    fn shaped_staging_input(
        &self,
        origin: [f32; 2],
        opacity: f32,
    ) -> Option<crate::view::render_pass::text_pass::TextPassPreparedStagingInput> {
        let context = self.shaped_context.as_ref()?;
        let paint_input = context.text_pass_paint_input_ref();
        Some(
            inline_ifc_paint_input_to_text_pass_staging_input_with_color(
                &paint_input,
                origin,
                opacity,
                0,
                1.0,
                Some(self.color.to_rgba_f32()),
            ),
        )
    }

    #[cfg(test)]
    pub(crate) fn shaped_staging_input_for_test(
        &self,
        origin: [f32; 2],
    ) -> Option<crate::view::render_pass::text_pass::TextPassPreparedStagingInput> {
        self.shaped_staging_input(origin, self.opacity.clamp(0.0, 1.0))
    }

    #[cfg(test)]
    pub(crate) fn shaped_context_for_test(
        &self,
    ) -> Option<&std::sync::Arc<crate::view::inline_formatting_context::InlineFormattingContext>>
    {
        self.shaped_context.as_ref()
    }
}

pub(super) struct StandaloneTextPaintPayload {
    pub(super) params: Option<TextPassPreparedParams>,
}

pub(super) struct PreparedShadowTextPayload {
    pub(super) bounds: super::super::Rect,
    pub(super) op: Option<crate::view::paint::PreparedTextOp>,
}

pub(super) struct ShadowTextPaintSource<'a> {
    pub(super) bounds: super::super::Rect,
    input: Option<&'a crate::view::inline_formatting_context::InlineIfcTextPassPaintInput>,
    pub(super) color_override: Option<[f32; 4]>,
}

impl ShadowTextPaintSource<'_> {
    fn fragment(&self, paint_offset: [f32; 2]) -> TextPassPreparedFragment {
        TextPassPreparedFragment {
            origin: [
                self.bounds.x + paint_offset[0],
                self.bounds.y + paint_offset[1],
            ],
            size: [self.bounds.width, self.bounds.height],
        }
    }

    fn staged_glyphs<'a>(
        &self,
        input: &'a crate::view::inline_formatting_context::InlineIfcTextPassPaintInput,
        origin: [f32; 2],
        opacity: f32,
    ) -> impl ExactSizeIterator<
        Item = crate::view::render_pass::text_pass::TextPassPreparedStagingGlyphInput,
    > + 'a {
        let color_override = self.color_override;
        input.glyphs.iter().map(move |glyph| {
            let raster =
                crate::view::inline_text_pass_adapter::inline_ifc_glyph_to_text_pass_raster_input(
                    glyph,
                );
            let mut paint =
                crate::view::inline_text_pass_adapter::inline_ifc_glyph_to_text_pass_paint_input(
                    glyph, opacity, 0,
                );
            if let Some(color) = color_override {
                paint.color = color;
            }
            crate::view::render_pass::text_pass::TextPassPreparedStagingGlyphInput {
                raster,
                paint,
                final_paint_pos: [
                    origin[0] + paint.local_pos[0],
                    origin[1] + paint.local_pos[1],
                ],
            }
        })
    }
}

#[cfg(test)]
mod preflight_tests;
