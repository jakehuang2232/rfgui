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
use crate::view::base_component::{InlineMeasureContext, LayoutConstraints, Size};
use crate::view::layout::flex_solver::{FlexSolverInputs, compute_flex_info};
use crate::view::layout::types::FlexLayoutInfo;
use crate::view::node_arena::{NodeArena, NodeKey};

/// Inputs to `measure_axis_children`.
///
/// Caller resolves `inner_width`, `inline_*` and child constraints first;
/// this fn only walks children and dispatches to `measure` /
/// `measure_inline` based on `layout`.
pub(crate) struct MeasureChildrenInputs<'a> {
    pub layout: Layout,
    pub children: &'a [NodeKey],
    pub inner_width: f32,
    pub child_available_width: f32,
    pub child_available_height: f32,
    pub child_percent_base_width: Option<f32>,
    pub child_percent_base_height: Option<f32>,
    pub viewport_width: f32,
    pub viewport_height: f32,
    /// Inline-mode wrap flag.
    pub inline_wrap: bool,
    /// Inline-mode gap.
    pub inline_gap: f32,
    /// First-line available width override from outer inline FC.
    pub inline_first_available_width: Option<f32>,
}

/// Recursive child-measure pass for an axis-layout container.
///
/// For `Layout::Inline`: tracks current line width, dispatches
/// `measure_inline` with the right `first_available_width`.
/// For `Layout::Flex` / `Layout::Flow`: dispatches plain `measure`.
pub(crate) fn measure_axis_children(
    inputs: MeasureChildrenInputs<'_>,
    arena: &mut NodeArena,
) {
    let MeasureChildrenInputs {
        layout,
        children,
        inner_width,
        child_available_width,
        child_available_height,
        child_percent_base_width,
        child_percent_base_height,
        viewport_width,
        viewport_height,
        inline_wrap,
        inline_gap,
        mut inline_first_available_width,
    } = inputs;

    let mut current_line_width = 0.0_f32;
    let mut line_has_content = false;
    for child_key in children.iter().copied() {
        if matches!(layout, Layout::Inline) {
            let first_available_width = if let Some(width) = inline_first_available_width.take() {
                width
            } else if !line_has_content {
                inner_width
            } else {
                (inner_width - current_line_width - inline_gap).max(0.0)
            };
            let node_sizes = arena
                .with_element_taken(child_key, |child, arena| {
                    child.measure_inline(
                        InlineMeasureContext {
                            first_available_width,
                            full_available_width: inner_width,
                            viewport_width,
                            viewport_height,
                            percent_base_width: child_percent_base_width,
                            percent_base_height: child_percent_base_height,
                        },
                        arena,
                    );
                    child.get_inline_nodes_size(arena)
                })
                .unwrap_or_default();
            if node_sizes.is_empty() {
                continue;
            }
            for (node_index, node) in node_sizes.into_iter().enumerate() {
                let item_width = node.width.max(0.0);
                let inserts_gap = node_index == 0 && line_has_content;
                let next_width = if !line_has_content {
                    item_width
                } else if inserts_gap {
                    current_line_width + inline_gap + item_width
                } else {
                    current_line_width + item_width
                };
                if inline_wrap && line_has_content && next_width > inner_width {
                    current_line_width = item_width;
                } else {
                    current_line_width = next_width;
                }
                line_has_content = true;
            }
        } else {
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
    pub inner_width: f32,
    pub child_available_width: f32,
    pub child_available_height: f32,
    pub child_percent_base_width: Option<f32>,
    pub child_percent_base_height: Option<f32>,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub inline_wrap: bool,
    pub inline_gap: f32,
    pub inline_first_available_width: Option<f32>,
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
            layout: inputs.layout,
            children: inputs.children,
            inner_width: inputs.inner_width,
            child_available_width: inputs.child_available_width,
            child_available_height: inputs.child_available_height,
            child_percent_base_width: inputs.child_percent_base_width,
            child_percent_base_height: inputs.child_percent_base_height,
            viewport_width: inputs.viewport_width,
            viewport_height: inputs.viewport_height,
            inline_wrap: inputs.inline_wrap,
            inline_gap: inputs.inline_gap,
            inline_first_available_width: inputs.inline_first_available_width,
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
