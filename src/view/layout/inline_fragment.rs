//! Inline-fragment place pipeline for fragmentable inline-formatting-context
//! containers.
//!
//! Phase F5 of the layout functional refactor. Extracted from the
//! fragmentable-inline branch of `Layoutable::place_inline` for `Element`
//! (`view/base_component/element/layout_trait.rs`).
//!
//! When a `Layout::Inline` container is `is_fragmentable_inline_element()`
//! (Auto width + Auto height), it can split across multiple line boxes.
//! Each call places one line-box fragment indexed by `placement.node_index`,
//! growing the container's `layout_size`/`inline_paint_fragments` to wrap
//! all fragments emitted so far.

use crate::style::{Align, FlowDirection, Length};
use crate::view::base_component::{
    InlinePlacement, Position, Rect, Size, cross_item_offset, resolve_px,
};
use crate::view::layout::types::{FlexLayoutInfo, LayoutState};
use crate::view::node_arena::{NodeArena, NodeKey};

/// Inputs to `place_inline_fragment`.
///
/// `flex_info` is optional: when the container has more than one in-flow
/// inline child, it supplies the line breakdown. With <= 1 child the fn
/// walks `children` directly to find the placed fragment.
pub(crate) struct PlaceInlineFragmentInputs<'a> {
    pub placement: InlinePlacement,
    pub children: &'a [NodeKey],
    pub absolute_mask: &'a [bool],
    pub flex_info: Option<&'a FlexLayoutInfo>,
    pub left_inset: f32,
    pub right_inset: f32,
    pub top_inset: f32,
    pub bottom_inset: f32,
    pub gap_length: Length,
    pub direction: FlowDirection,
    pub align: Align,
}

/// Place one line-box fragment of a fragmentable inline container.
///
/// Mutates `layout_state` (position/size/inner/flow/content/should_render)
/// and `inline_paint_fragments` (extends or pushes a `Rect`). Caller
/// clears `DirtyFlags::PLACE | BOX_MODEL | HIT_TEST` afterward.
pub(crate) fn place_inline_fragment(
    inputs: PlaceInlineFragmentInputs<'_>,
    layout_state: &mut LayoutState,
    inline_paint_fragments: &mut Vec<Rect>,
    arena: &mut NodeArena,
) {
    let PlaceInlineFragmentInputs {
        placement,
        children,
        absolute_mask,
        flex_info,
        left_inset,
        right_inset,
        top_inset,
        bottom_inset,
        gap_length,
        direction,
        align,
    } = inputs;

    let is_row = matches!(direction, FlowDirection::Row);

    if placement.node_index == 0 {
        inline_paint_fragments.clear();
        layout_state.layout_flow_position = Position {
            x: placement.x,
            y: placement.y,
        };
        layout_state.layout_position = layout_state.layout_flow_position;
        layout_state.layout_size = Size {
            width: 0.0,
            height: 0.0,
        };
        layout_state.layout_inner_position = layout_state.layout_position;
        layout_state.layout_flow_inner_position = layout_state.layout_flow_position;
        layout_state.layout_inner_size = layout_state.layout_size;
        layout_state.should_render = false;
    }

    let inline_child_count = children
        .iter()
        .enumerate()
        .filter(|(idx, _)| !absolute_mask.get(*idx).copied().unwrap_or(false))
        .count();

    let (line_width, line_height, total_nodes) = if inline_child_count > 1 {
        let Some(info) = flex_info else {
            return;
        };
        let total_nodes = info.lines.len();
        let Some(line) = info.lines.get(placement.node_index) else {
            return;
        };
        let line_width = info
            .line_main_sum
            .get(placement.node_index)
            .copied()
            .unwrap_or(0.0)
            .max(0.0);
        let line_height = info
            .line_cross_max
            .get(placement.node_index)
            .copied()
            .unwrap_or(0.0)
            .max(0.0);
        let mut main_cursor = 0.0_f32;
        let mut prev_child_index: Option<usize> = None;
        let gap = resolve_px(
            gap_length,
            line_width.max(0.0),
            placement.viewport_width,
            placement.viewport_height,
        );

        for item in line {
            if prev_child_index != Some(item.child_index) && prev_child_index.is_some() {
                main_cursor += gap;
            }
            let align_offset = cross_item_offset(line_height, item.cross.max(0.0), align);
            let content_origin_x = placement.x
                + if placement.node_index == 0 {
                    left_inset
                } else {
                    0.0
                };
            let (x, y) = if is_row {
                (
                    content_origin_x + main_cursor + item.main_offset,
                    placement.y + align_offset + item.cross_offset,
                )
            } else {
                (
                    placement.x + align_offset + item.cross_offset,
                    placement.y + main_cursor + item.main_offset,
                )
            };
            let child_key = children[item.child_index];
            arena.with_element_taken(child_key, |child, arena| {
                child.place_inline(
                    InlinePlacement {
                        x,
                        y,
                        node_index: item.node_index,
                        ..placement
                    },
                    arena,
                );
            });
            main_cursor += item.main.max(0.0);
            prev_child_index = Some(item.child_index);
        }

        (line_width, line_height, total_nodes)
    } else {
        let mut current = 0_usize;
        let mut total_nodes = 0_usize;
        let mut target: Option<(usize, usize, f32, f32)> = None;
        for (child_idx, child_key) in children.iter().enumerate() {
            if absolute_mask.get(child_idx).copied().unwrap_or(false) {
                continue;
            }
            let nodes = arena
                .get(*child_key)
                .map(|n| n.element.get_inline_nodes_size(arena))
                .unwrap_or_default();
            total_nodes += nodes.len();
            if target.is_none() && placement.node_index < current + nodes.len() {
                let local_index = placement.node_index - current;
                let node = nodes[local_index];
                target = Some((child_idx, local_index, node.width, node.height));
            }
            current += nodes.len();
        }
        let Some((child_idx, local_index, width, height)) = target else {
            return;
        };
        let child_key = children[child_idx];
        arena.with_element_taken(child_key, |child, arena| {
            child.place_inline(
                InlinePlacement {
                    x: placement.x
                        + if placement.node_index == 0 {
                            left_inset
                        } else {
                            0.0
                        },
                    y: placement.y,
                    node_index: local_index,
                    ..placement
                },
                arena,
            );
        });
        (width.max(0.0), height.max(0.0), total_nodes)
    };

    let is_first_fragment = placement.node_index == 0;
    let is_last_fragment = placement.node_index + 1 == total_nodes;
    let left = placement.x;
    let top = placement.y - top_inset;
    let outer_width = line_width
        + if is_first_fragment { left_inset } else { 0.0 }
        + if is_last_fragment { right_inset } else { 0.0 };
    let outer_height = line_height + top_inset + bottom_inset;
    let right = placement.x + outer_width;
    let bottom = top + outer_height;
    let should_extend_existing = inline_paint_fragments
        .last()
        .is_some_and(|fragment| (fragment.y - top).abs() < 0.5);
    if should_extend_existing {
        if let Some(fragment) = inline_paint_fragments.last_mut() {
            let fragment_right = fragment.x + fragment.width;
            let fragment_bottom = fragment.y + fragment.height;
            fragment.x = fragment.x.min(left);
            fragment.y = fragment.y.min(top);
            fragment.width = fragment_right.max(right) - fragment.x;
            fragment.height = fragment_bottom.max(bottom) - fragment.y;
        }
    } else {
        inline_paint_fragments.push(Rect {
            x: left,
            y: top,
            width: (right - left).max(0.0),
            height: (bottom - top).max(0.0),
        });
    }
    if layout_state.should_render {
        let current_right = layout_state.layout_position.x + layout_state.layout_size.width;
        let current_bottom = layout_state.layout_position.y + layout_state.layout_size.height;
        layout_state.layout_position.x = layout_state.layout_position.x.min(left);
        layout_state.layout_position.y = layout_state.layout_position.y.min(top);
        layout_state.layout_flow_position = layout_state.layout_position;
        layout_state.layout_size.width =
            current_right.max(right) - layout_state.layout_position.x;
        layout_state.layout_size.height =
            current_bottom.max(bottom) - layout_state.layout_position.y;
    } else {
        layout_state.layout_position = Position { x: left, y: top };
        layout_state.layout_flow_position = layout_state.layout_position;
        layout_state.layout_size = Size {
            width: (right - left).max(0.0),
            height: (bottom - top).max(0.0),
        };
    }
    layout_state.layout_inner_position = layout_state.layout_position;
    layout_state.layout_flow_inner_position = layout_state.layout_flow_position;
    layout_state.layout_inner_size = layout_state.layout_size;
    layout_state.content_size = layout_state.layout_size;
    layout_state.should_render =
        layout_state.layout_size.width > 0.0 && layout_state.layout_size.height > 0.0;
}
