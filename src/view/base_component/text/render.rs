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

use super::Text;
use super::hit_test::current_text_area_selection_render_context;
use super::paint_cache::{TextPaintKey, TextPaintMemo};
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
        // Both renderers consume the same immutable, memoized text payload.
        // Rebuilding a Vec here would defeat the prepared-input warm lookup.
        let Some(payload) = self.prepared_shadow_text_payload(ctx.paint_offset(), opacity) else {
            return ctx.into_state();
        };
        let Some(op) = &payload.op else {
            return ctx.into_state();
        };
        let pass = TextPreparedInputPass::new(
            op.params.clone(),
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
    /// Exact visibility gate shared by legacy build and retained recording.
    /// Callers supply the effective opacity. Neutral root-effect recording
    /// uses 1.0, while baked paths use the clamped local opacity.
    pub(super) fn is_paint_visible(&self, effective_opacity: f32) -> bool {
        self.layout_state.should_render
            && !self.backface_culled
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

    /// The immutable source recording reads: owned by an inline formatting
    /// context, or shaped by this Text itself. `None` without one.
    fn shadow_text_paint_source(&self, opacity: f32) -> Option<ShadowTextPaintSource<'_>> {
        if let Some(input) = self.inline_ifc_owned_paint_input() {
            let installed = self.inline_ifc_owned_paint_bounds()?;
            let bounds = super::super::Rect {
                x: installed.x,
                y: installed.y,
                width: installed.width,
                height: installed.height,
            };
            return Some(ShadowTextPaintSource {
                bounds,
                input: self.is_paint_visible(opacity).then_some(input),
                color_override: None,
            });
        }
        let bounds = self.standalone_paint_bounds();
        if !self.is_paint_visible(opacity) {
            return Some(ShadowTextPaintSource {
                bounds,
                input: None,
                color_override: None,
            });
        }
        let input = self
            .shaped_context
            .as_ref()?
            .prepared_text_pass_paint_input_ref()?;
        Some(ShadowTextPaintSource {
            bounds,
            input: Some(input),
            color_override: Some(self.color.to_rgba_f32()),
        })
    }

    /// Recording materializes the glyph vector and frozen identity on a cache miss.
    pub(super) fn prepared_shadow_text_payload(
        &self,
        paint_offset: [f32; 2],
        opacity: f32,
    ) -> Option<Arc<PreparedShadowTextPayload>> {
        let source = self.shadow_text_paint_source(opacity)?;
        let Some(input) = source.input.filter(|input| !input.glyphs.is_empty()) else {
            return Some(Arc::new(PreparedShadowTextPayload {
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
            return Some(memo.payload.clone());
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
        let op = crate::view::paint::PreparedTextOp::new(params)?;
        let payload = Arc::new(PreparedShadowTextPayload {
            bounds: source.bounds,
            op: Some(op),
        });
        *self.paint_memo.borrow_mut() = Some(TextPaintMemo {
            key,
            payload: payload.clone(),
        });
        Some(payload)
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
            crate::view::inline_text_pass_adapter::inline_ifc_paint_input_to_text_pass_staging_input_with_color(
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
