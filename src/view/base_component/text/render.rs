//! `Renderable` impl for Text: emits TextPass + selection rects.

use crate::view::base_component::{BuildState, ElementTrait, Renderable, UiBuildContext};
use crate::view::frame_graph::FrameGraph;
use crate::view::node_arena::NodeArena;
use crate::view::render_pass::draw_rect_pass::{
    DrawRectInput, DrawRectOutput, RectPassParams, RenderTargetIn,
};
use crate::view::render_pass::text_pass::{TextInput, TextOutput, TextPassFragment, TextPassParams};
use crate::view::render_pass::{DrawRectPass, TextPass};

use super::hit_test::current_text_area_selection_render_context;
use super::Text;

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
        if let Some(selection) = current_text_area_selection_render_context() {
            for rect in self.local_selection_screen_rects(selection.start, selection.end) {
                let mut selection_pass = DrawRectPass::new(
                    RectPassParams {
                        position: [rect.x, rect.y],
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
        let inline_runs = self
            .inline_plan
            .as_ref()
            .map(|plan| plan.runs.as_slice())
            .unwrap_or(&[]);
        let inline_fragment_indices = inline_runs
            .iter()
            .enumerate()
            .filter(|(_, fragment)| fragment.position.is_some() && !fragment.content.is_empty())
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let has_inline_fragments = !inline_fragment_indices.is_empty();
        let fragments = if !has_inline_fragments {
            let layout_buffer = self
                .layout_buffer
                .as_ref()
                .expect("text layout buffer should be prepared during layout")
                .clone();
            vec![TextPassFragment {
                content: self.content.clone(),
                x: self.layout_state.layout_position.x,
                y: self.layout_state.layout_position.y,
                width: self.render_size.width.max(self.layout_state.layout_size.width),
                height: self.render_size.height.max(self.layout_state.layout_size.height),
                color: self.color.to_rgba_f32(),
                opacity,
                layout_buffer: Some(layout_buffer),
            }]
        } else {
            inline_fragment_indices
                .into_iter()
                .filter_map(|index| {
                    let fragment = inline_runs.get(index)?;
                    let position = fragment.position?;
                    let content = fragment.content.clone();
                    let width = fragment.width;
                    let height = fragment.height;
                    let layout_buffer = fragment.layout_buffer.clone()?;
                    Some(TextPassFragment {
                        content,
                        x: position.x,
                        y: position.y,
                        width: width.max(1.0),
                        height: height.max(1.0),
                        color: self.color.to_rgba_f32(),
                        opacity,
                        layout_buffer: Some(layout_buffer),
                    })
                })
                .collect::<Vec<_>>()
        };
        let pass = TextPass::new(
            TextPassParams {
                fragments,
                font_size: self.font_size,
                line_height: self.line_height,
                font_weight: self.font_weight,
                font_families: self.font_families.clone(),
                align: self.align,
                allow_wrap: !has_inline_fragments && self.allow_wrap,
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
