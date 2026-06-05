//! `Renderable` impl for Text: emits TextPass + selection rects.

use crate::view::base_component::{BuildState, ElementTrait, Renderable, UiBuildContext};
use crate::view::frame_graph::FrameGraph;
use crate::view::node_arena::NodeArena;
use crate::view::render_pass::draw_rect_pass::{
    DrawRectInput, DrawRectOutput, RectPassParams, RenderTargetIn,
};
use crate::view::render_pass::text_pass::TextPreparedInputPass;
#[cfg(test)]
use crate::view::render_pass::text_pass::build_text_pass_prepared_staging_probe;
use crate::view::render_pass::text_pass::{
    TextInput, TextOutput, TextPassFragment, TextPassParams, TextPassPreparedFragment,
    TextPassPreparedParams,
};
use crate::view::render_pass::{DrawRectPass, TextPass};

use super::Text;
#[cfg(test)]
use super::TextReadOnlyIfcStagingProbe;
use super::hit_test::current_text_area_selection_render_context;
use crate::view::inline_formatting_context::InlineIfcStyle;
#[cfg(test)]
use crate::view::inline_text_pass_adapter::InlineTextPassBridgePackage;
#[cfg(test)]
use crate::view::inline_text_pass_adapter::inline_text_pass_prepared_equivalent_probe_for_test;
use crate::view::inline_text_pass_adapter::{
    TextReadOnlyIfcBridgeInput, build_inline_text_pass_prepared_input,
    build_text_read_only_ifc_bridge_package_from_input,
    inline_prepared_input_to_text_pass_staging_input,
};

impl Renderable for Text {
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        _arena: &mut NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        #[cfg(test)]
        {
            self.read_only_ifc_staging_probe = None;
        }

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
            let [x, y] = ctx.paint_point(
                self.layout_state.layout_position.x,
                self.layout_state.layout_position.y,
            );
            #[cfg(test)]
            self.capture_text_read_only_ifc_staging_probe_for_test([x, y], 0, opacity);
            let read_only_ifc_decision = self.text_read_only_ifc_render_decision();
            if read_only_ifc_decision.uses_prepared_render_pass() {
                if let Some(staging_input) =
                    self.text_read_only_ifc_prepared_staging_input_with_opacity([x, y], 0, opacity)
                {
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
                    return ctx.into_state();
                }
            }
            vec![TextPassFragment {
                content: self.content.clone(),
                x,
                y,
                width: self
                    .render_size
                    .width
                    .max(self.layout_state.layout_size.width),
                height: self
                    .render_size
                    .height
                    .max(self.layout_state.layout_size.height),
                color: self.color.to_rgba_f32(),
                opacity,
                text_layout: self.text_layout.clone(),
            }]
        } else {
            inline_fragment_indices
                .into_iter()
                .filter_map(|index| {
                    let fragment = inline_runs.get(index)?;
                    let position = fragment.position?;
                    let [x, y] = ctx.paint_point(position.x, position.y);
                    let content = fragment.content.clone();
                    let width = fragment.width;
                    let height = fragment.height;
                    Some(TextPassFragment {
                        content,
                        x,
                        y,
                        width: width.max(1.0),
                        height: height.max(1.0),
                        color: self.color.to_rgba_f32(),
                        opacity,
                        text_layout: fragment.text_layout.clone(),
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

impl Text {
    pub(crate) fn text_read_only_ifc_render_decision(
        &self,
    ) -> super::TextReadOnlyIfcRenderDecision {
        self.read_only_ifc_staging_mode.render_decision()
    }

    #[cfg(test)]
    pub(crate) fn text_read_only_ifc_bridge_input(
        &self,
        origin: [f32; 2],
        fragment_index: u32,
    ) -> Option<TextReadOnlyIfcBridgeInput> {
        self.text_read_only_ifc_bridge_input_with_opacity(
            origin,
            fragment_index,
            self.opacity.clamp(0.0, 1.0),
        )
    }

    fn text_read_only_ifc_bridge_input_with_opacity(
        &self,
        origin: [f32; 2],
        fragment_index: u32,
        opacity: f32,
    ) -> Option<TextReadOnlyIfcBridgeInput> {
        if !self.text_read_only_ifc_render_decision().captures_probe() {
            return None;
        }
        if !self.layout_state.should_render || self.content.is_empty() {
            return None;
        }
        if opacity <= 0.0 {
            return None;
        }
        let has_inline_fragments = self.inline_plan.as_ref().is_some_and(|plan| {
            plan.runs
                .iter()
                .any(|fragment| fragment.position.is_some() && !fragment.content.is_empty())
        });
        if has_inline_fragments {
            return None;
        }

        let layout_width = self
            .render_size
            .width
            .max(self.layout_state.layout_size.width)
            .max(1.0);
        let layout_height = self
            .render_size
            .height
            .max(self.layout_state.layout_size.height)
            .max(1.0);
        let mut input = TextReadOnlyIfcBridgeInput::new(
            self.content.clone(),
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
        input.origin = origin;
        input.layout_size = [layout_width, layout_height];
        input.width_constraint = Some(layout_width);
        input.allow_wrap = self.allow_wrap;
        Some(input)
    }

    #[cfg(test)]
    pub(crate) fn text_read_only_ifc_bridge_package(
        &self,
        origin: [f32; 2],
        fragment_index: u32,
    ) -> Option<InlineTextPassBridgePackage> {
        let input = self.text_read_only_ifc_bridge_input(origin, fragment_index)?;
        Some(build_text_read_only_ifc_bridge_package_from_input(&input))
    }

    #[cfg(test)]
    pub(crate) fn text_read_only_ifc_prepared_staging_input(
        &self,
        origin: [f32; 2],
        fragment_index: u32,
    ) -> Option<crate::view::render_pass::text_pass::TextPassPreparedStagingInput> {
        self.text_read_only_ifc_prepared_staging_input_with_opacity(
            origin,
            fragment_index,
            self.opacity.clamp(0.0, 1.0),
        )
    }

    fn text_read_only_ifc_prepared_staging_input_with_opacity(
        &self,
        origin: [f32; 2],
        fragment_index: u32,
        opacity: f32,
    ) -> Option<crate::view::render_pass::text_pass::TextPassPreparedStagingInput> {
        if !self
            .text_read_only_ifc_render_decision()
            .uses_prepared_render_pass()
        {
            return None;
        }
        let input =
            self.text_read_only_ifc_bridge_input_with_opacity(origin, fragment_index, opacity)?;
        let package = build_text_read_only_ifc_bridge_package_from_input(&input);
        let prepared_input = build_inline_text_pass_prepared_input(&input, &package, 1.0);
        Some(inline_prepared_input_to_text_pass_staging_input(
            &prepared_input,
        ))
    }

    #[cfg(test)]
    fn capture_text_read_only_ifc_staging_probe_for_test(
        &mut self,
        origin: [f32; 2],
        fragment_index: u32,
        opacity: f32,
    ) {
        let mode = self.read_only_ifc_staging_mode;
        let Some(input) =
            self.text_read_only_ifc_bridge_input_with_opacity(origin, fragment_index, opacity)
        else {
            return;
        };
        let package = build_text_read_only_ifc_bridge_package_from_input(&input);
        let prepared_input = build_inline_text_pass_prepared_input(&input, &package, 1.0);
        let text_pass_staging_input =
            inline_prepared_input_to_text_pass_staging_input(&prepared_input);
        let text_pass_staging_probe =
            build_text_pass_prepared_staging_probe(&text_pass_staging_input);
        let prepared_equivalent =
            inline_text_pass_prepared_equivalent_probe_for_test(&input, &package, 1.0);
        self.read_only_ifc_staging_probe = Some(TextReadOnlyIfcStagingProbe {
            mode,
            input,
            package,
            prepared_input,
            prepared_equivalent,
            text_pass_staging_input,
            text_pass_staging_probe,
        });
    }

    #[cfg(test)]
    pub(crate) fn text_read_only_ifc_staging_probe_for_test(
        &self,
    ) -> Option<&TextReadOnlyIfcStagingProbe> {
        self.read_only_ifc_staging_probe.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn text_read_only_ifc_bridge_input_for_test(
        &self,
        origin: [f32; 2],
        fragment_index: u32,
    ) -> Option<TextReadOnlyIfcBridgeInput> {
        self.text_read_only_ifc_bridge_input(origin, fragment_index)
    }

    #[cfg(test)]
    pub(crate) fn text_read_only_ifc_bridge_package_for_test(
        &self,
        origin: [f32; 2],
        fragment_index: u32,
    ) -> Option<InlineTextPassBridgePackage> {
        self.text_read_only_ifc_bridge_package(origin, fragment_index)
    }
}
