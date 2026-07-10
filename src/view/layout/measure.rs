//! Axis-layout measure pipeline.
//!
//! Phase F3 of the layout functional refactor. Extracted from
//! `Element::measure_flex_children`. Two layered free functions:
//! - `measure_axis_children`: recursive child measure pass (inline /
//!   non-inline branches).
//! - `measure_axis`: orchestrates child measure + `compute_flex_info`,
//!   returns owned outputs the caller writes back into its component
//!   state.

use crate::style::Layout;
use crate::view::base_component::{LayoutConstraints, Size};
use crate::view::layout::flex_solver::{FlexSolverInputs, compute_flex_info};
use crate::view::layout::types::FlexLayoutInfo;
use crate::view::node_arena::{NodeArena, NodeKey};

/// Inputs to `measure_axis_children`.
///
/// Caller resolves child constraints first; this fn only walks children and
/// dispatches to `measure`.
pub(crate) struct MeasureChildrenInputs<'a> {
    pub children: &'a [NodeKey],
    pub child_available_width: f32,
    pub child_available_height: f32,
    pub child_percent_base_width: Option<f32>,
    pub child_percent_base_height: Option<f32>,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Recursive child-measure pass for an axis-layout container.
pub(crate) fn measure_axis_children(inputs: MeasureChildrenInputs<'_>, arena: &mut NodeArena) {
    let MeasureChildrenInputs {
        children,
        child_available_width,
        child_available_height,
        child_percent_base_width,
        child_percent_base_height,
        viewport_width,
        viewport_height,
    } = inputs;

    for child_key in children.iter().copied() {
        arena.with_element_taken(child_key, |child, arena| {
            child.measure(
                LayoutConstraints {
                    max_width: child_available_width,
                    max_height: child_available_height,
                    viewport_width,
                    viewport_height,
                    percent_base_width: child_percent_base_width,
                    percent_base_height: child_percent_base_height,
                },
                arena,
            );
        });
    }
}

/// Inputs to `measure_axis`.
pub(crate) struct MeasureAxisInputs<'a> {
    pub layout: Layout,
    pub children: &'a [NodeKey],
    pub absolute_mask: &'a [bool],
    pub is_row: bool,
    pub is_real_flex: bool,
    pub solver_wrap: bool,
    pub solver_gap: f32,
    pub main_limit: f32,
    pub child_available_width: f32,
    pub child_available_height: f32,
    pub child_percent_base_width: Option<f32>,
    pub child_percent_base_height: Option<f32>,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Outputs of `measure_axis`. Caller writes these back into its
/// component state (auto-fill width/height, content_size, flex_info cache).
pub(crate) struct MeasureAxisOutputs {
    pub flex_info: FlexLayoutInfo,
    pub content_size: Size,
}

/// Orchestrate child measure + axis solver for an axis-layout container.
///
/// Returns `(flex_info, content_size)`. Auto-size for the container itself
/// is the caller's responsibility (it knows which dimensions are `Auto`
/// and the relevant insets to add).
pub(crate) fn measure_axis(
    inputs: MeasureAxisInputs<'_>,
    arena: &mut NodeArena,
) -> MeasureAxisOutputs {
    measure_axis_children(
        MeasureChildrenInputs {
            children: inputs.children,
            child_available_width: inputs.child_available_width,
            child_available_height: inputs.child_available_height,
            child_percent_base_width: inputs.child_percent_base_width,
            child_percent_base_height: inputs.child_percent_base_height,
            viewport_width: inputs.viewport_width,
            viewport_height: inputs.viewport_height,
        },
        arena,
    );

    let flex_info = compute_flex_info(
        FlexSolverInputs {
            layout_kind: inputs.layout,
            children: inputs.children,
            absolute_mask: inputs.absolute_mask,
            is_row: inputs.is_row,
            is_real_flex: inputs.is_real_flex,
            wrap: inputs.solver_wrap,
            gap: inputs.solver_gap,
            main_limit: inputs.main_limit,
            child_available_width: inputs.child_available_width,
            child_available_height: inputs.child_available_height,
            viewport_width: inputs.viewport_width,
            viewport_height: inputs.viewport_height,
            child_percent_base_width: inputs.child_percent_base_width,
            child_percent_base_height: inputs.child_percent_base_height,
        },
        arena,
    );

    let content_size = if inputs.is_row {
        Size {
            width: flex_info.total_main,
            height: flex_info.total_cross,
        }
    } else {
        Size {
            width: flex_info.total_cross,
            height: flex_info.total_main,
        }
    };

    MeasureAxisOutputs {
        flex_info,
        content_size,
    }
}
