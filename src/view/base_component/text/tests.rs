//! Text unit tests.

#![cfg(test)]

use super::{ElementTrait, Text, measure_text_size};
use crate::style::{
    Color, ColorLike, FontFamily, FontSize, FontWeight, Length, ParsedValue, PropertyId, Scale,
    Style, TextWrap, Transform, TransformOrigin, Translate, VerticalAlign,
};
use crate::view::base_component::{DirtyFlags, LayoutConstraints, LayoutPlacement, Layoutable};
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

/// Glyph ops the frame recorder emits for `text` as the only root.
fn recorded_text_op_count_for_read_only_ifc_test(text: Text) -> usize {
    let mut arena = arena();
    let roots = [crate::view::test_support::commit_element(
        &mut arena,
        Box::new(text),
    )];
    let mut properties = crate::view::compositor::PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = crate::view::compositor::PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    crate::view::paint::record_surface_dag_frame_artifact(&arena, &roots, &properties, &generations)
        .ops
        .iter()
        .filter(|op| matches!(op, crate::view::paint::PaintOp::PreparedText(_)))
        .count()
}

mod style_tests;
mod wrap_tests;
mod measure_cache_tests;
mod auto_size_tests;
mod render_tests;
mod preparation_recovery_tests;
