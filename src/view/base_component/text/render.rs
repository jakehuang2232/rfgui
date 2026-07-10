//! `Renderable` impl for Text: emits the prepared glyph pass + selection
//! rects, consuming the same shaped context measure produced.

use crate::view::base_component::{BuildState, ElementTrait, Renderable, UiBuildContext};
use crate::view::frame_graph::FrameGraph;
use crate::view::node_arena::NodeArena;
use crate::view::render_pass::DrawRectPass;
use crate::view::render_pass::draw_rect_pass::{
    DrawRectInput, DrawRectOutput, RectPassParams, RenderTargetIn,
};
use crate::view::render_pass::text_pass::TextPreparedInputPass;
use crate::view::render_pass::text_pass::{
    TextInput, TextOutput, TextPassPreparedFragment, TextPassPreparedParams,
    TextPassPreparedStagingInput,
};

use super::Text;
use super::hit_test::current_text_area_selection_render_context;
use crate::view::inline_text_pass_adapter::{
    inline_ifc_paint_input_to_text_pass_staging_input,
    inline_ifc_paint_input_to_text_pass_staging_input_with_color,
};

impl Renderable for Text {
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        _arena: &mut NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        if !self.layout_state.should_render || self.content.is_empty() {
            return ctx.into_state();
        }
        let opacity = if ctx.is_node_promoted(self.stable_id()) {
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
        self.emit_selection_underlay(graph, &mut ctx);
        let [x, y] = ctx.paint_point(
            self.layout_state.layout_position.x,
            self.layout_state.layout_position.y,
        );
        let staging_input = if let Some(input) = self.inline_ifc_owned_paint_input.as_ref() {
            inline_ifc_paint_input_to_text_pass_staging_input(input, [x, y], opacity, 0, 1.0)
        } else {
            let Some(input) = self.shaped_staging_input([x, y], opacity) else {
                return ctx.into_state();
            };
            input
        };
        if staging_input.glyphs.is_empty() {
            return ctx.into_state();
        }
        let pass = TextPreparedInputPass::new(
            TextPassPreparedParams {
                staging_input,
                fragments: vec![TextPassPreparedFragment {
                    origin: [x, y],
                    size: [
                        self.render_size
                            .width
                            .max(self.layout_state.layout_size.width),
                        self.render_size
                            .height
                            .max(self.layout_state.layout_size.height),
                    ],
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
        ctx.into_state()
    }
}

impl Text {
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
        let Some(input_target) = ctx.current_target() else {
            return;
        };
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
                DrawRectInput {
                    pass_context: ctx.graphics_pass_context(),
                    ..Default::default()
                },
                DrawRectOutput {
                    render_target: input_target,
                    ..Default::default()
                },
            );
            selection_pass.set_input(
                input_target
                    .handle()
                    .map(RenderTargetIn::with_handle)
                    .unwrap_or_default(),
            );
            graph.add_graphics_pass(selection_pass);
        }
        ctx.set_current_target(input_target);
    }

    /// Staging input for the prepared glyph pass, built from the shaped
    /// context measure installed. Live color is injected here so color
    /// changes repaint without reshaping.
    fn shaped_staging_input(
        &self,
        origin: [f32; 2],
        opacity: f32,
    ) -> Option<TextPassPreparedStagingInput> {
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
    ) -> Option<TextPassPreparedStagingInput> {
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
