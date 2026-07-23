//! Text unit tests.

#![cfg(test)]

use super::{ElementTrait, Text, measure_text_size};
use crate::style::{
    Color, ColorLike, FontFamily, FontSize, FontWeight, Length, ParsedValue, PropertyId, Rotate,
    Scale, Style, TextWrap, Transform, TransformEntry, TransformOrigin, Translate, VerticalAlign,
};
use crate::view::base_component::{
    DirtyFlags, LayoutConstraints, LayoutPlacement, Layoutable, Renderable, UiBuildContext,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::inline_formatting_context::InlineIfcAlignment;
use crate::view::node_arena::NodeArena;

fn arena() -> NodeArena {
    NodeArena::new()
}

fn place_text_for_read_only_ifc_test(text: &mut Text, width: f32, height: f32) {
    let mut a = arena();
    text.measure(
        LayoutConstraints {
            max_width: width,
            max_height: height,
            viewport_width: width,
            percent_base_width: Some(width),
            percent_base_height: Some(height),
            viewport_height: height,
        },
        &mut a,
    );
    text.place(
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: height,
            viewport_width: width,
            percent_base_width: Some(width),
            percent_base_height: Some(height),
            viewport_height: height,
        },
        &mut a,
    );
}

fn build_text_for_read_only_ifc_test(text: &mut Text) -> Vec<String> {
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(200, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    let mut arena = arena();

    text.build(&mut graph, &mut arena, ctx);

    graph
        .pass_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name.to_string())
        .collect()
}

mod style_tests;
mod wrap_tests;
mod measure_cache_tests;
mod auto_size_tests;
mod render_tests;
