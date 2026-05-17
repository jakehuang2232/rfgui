//! Axis-layout place pipeline.
//!
//! Phase F4 of the layout functional refactor. Extracts the body of
//! `Element::place_flex_children` into `place_axis_children` (in-flow
//! flex/inline placement) and `place_absolute_children` (absolute-position
//! children). Caller composes both around its own profile timing scopes.

use crate::time::Instant;

use crate::style::{Align, CrossSize, JustifyContent, Layout};
use crate::view::base_component::{
    DirtyPassMask, Element, InlinePlacement, LayoutPlacement, PlacementSkipFailureReason, Rect,
    cross_item_offset, cross_start_offset, inline_cross_offset, main_axis_start_and_gap,
    with_layout_place_profile,
};
use crate::view::layout::types::FlexLayoutInfo;
use crate::view::node_arena::{NodeArena, NodeKey};

/// Inputs to `place_axis_children`.
///
/// `flex_info` is consumed (taken by value); caller reads its line
/// breakdown. `align` / `justify_content` / `cross_size` are pre-resolved
/// from the container's style.
pub(crate) struct PlaceAxisChildrenInputs<'a> {
    pub layout: Layout,
    pub children: &'a [NodeKey],
    pub flex_info: FlexLayoutInfo,
    pub is_row: bool,
    pub gap: f32,
    pub main_limit: f32,
    pub cross_limit: f32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub visual_offset_x: f32,
    pub visual_offset_y: f32,
    pub child_available_width: f32,
    pub child_available_height: f32,
    pub child_parent_hit_test_clip: Option<Rect>,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub child_percent_base_width: Option<f32>,
    pub child_percent_base_height: Option<f32>,
    pub align: Align,
    pub justify_content: JustifyContent,
    pub cross_size: CrossSize,
}

/// Inputs to `place_absolute_children`.
pub(crate) struct PlaceAbsoluteChildrenInputs<'a> {
    pub children: &'a [NodeKey],
    pub absolute_mask: &'a [bool],
    pub origin_x: f32,
    pub origin_y: f32,
    pub visual_offset_x: f32,
    pub visual_offset_y: f32,
    pub child_available_width: f32,
    pub child_available_height: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub child_percent_base_width: Option<f32>,
    pub child_percent_base_height: Option<f32>,
}

/// Place in-flow children of an axis-layout container.
///
/// For `Layout::Inline`: dispatches `place_inline` with `InlinePlacement`.
/// For `Layout::Flex` / `Layout::Flow`: sizes children via
/// `set_layout_width/height`, runs cross-axis stretch + alignment, then
/// dispatches `place` with `LayoutPlacement`.
pub(crate) fn place_axis_children(inputs: PlaceAxisChildrenInputs<'_>, arena: &mut NodeArena) {
    let PlaceAxisChildrenInputs {
        layout,
        children,
        flex_info: info,
        is_row,
        gap,
        main_limit,
        cross_limit,
        origin_x,
        origin_y,
        visual_offset_x,
        visual_offset_y,
        child_available_width,
        child_available_height,
        child_parent_hit_test_clip,
        viewport_width,
        viewport_height,
        child_percent_base_width,
        child_percent_base_height,
        align,
        justify_content,
        cross_size,
    } = inputs;

    let total_cross = info.total_cross;
    let mut cross_cursor = cross_start_offset(cross_limit, total_cross, align);
    let flex_children_started_at = Instant::now();

    for (line_idx, line) in info.lines.iter().enumerate() {
        let line_main = info.line_main_sum[line_idx];
        // Inline path uses ascent + descent (D2). Flex/Flow keep using
        // line_cross_max so non-inline layouts stay byte-identical until
        // Sprint 5 cleanup. Pure-element / pure-text inline rows produce
        // ascent + descent == line_cross_max (zero descent for elements,
        // baseline+descent collapses back to height for single text run).
        let line_cross = if matches!(layout, Layout::Inline) {
            info.line_ascent[line_idx] + info.line_descent[line_idx]
        } else {
            info.line_cross_max[line_idx]
        };
        let mut line_item_count = 0_usize;
        let mut prev_child_index: Option<usize> = None;
        for item in line {
            if prev_child_index != Some(item.child_index) {
                line_item_count += 1;
                prev_child_index = Some(item.child_index);
            }
        }
        let (mut main_cursor, distributed_gap) =
            main_axis_start_and_gap(main_limit, line_main, gap, line_item_count, justify_content);

        for (item_idx, item) in line.iter().enumerate() {
            let child_idx = item.child_index;
            let item_main = item.main;
            let child_key = children[child_idx];
            record_axis_placement_eligibility(layout, child_key, arena);
            if matches!(layout, Layout::Inline) {
                let alignment_cross = item.cross.max(0.0);
                // D3 per-fragment offset: each item picks one of
                // {Baseline, Top, Middle, Bottom}. Default Baseline aligns
                // the item's typography baseline to the line's
                // line_ascent (max baseline). The container's `align`
                // (which is forced to Start for Inline) is intentionally
                // ignored on this branch.
                let align_offset = inline_cross_offset(
                    info.line_ascent[line_idx],
                    line_cross,
                    item.baseline,
                    alignment_cross,
                    item.vertical_align,
                );
                let (offset_x, offset_y) = if is_row {
                    (
                        main_cursor + item.main_offset,
                        cross_cursor + align_offset + item.cross_offset,
                    )
                } else {
                    (
                        cross_cursor + align_offset + item.cross_offset,
                        main_cursor + item.main_offset,
                    )
                };
                with_layout_place_profile(|profile| profile.child_place_calls += 1);
                arena.with_element_taken(child_key, |child, arena| {
                    child.place_inline(
                        InlinePlacement {
                            node_index: item.node_index,
                            x: origin_x + visual_offset_x + offset_x,
                            y: origin_y + visual_offset_y + offset_y,
                            offset_x,
                            offset_y,
                            parent_x: origin_x,
                            parent_y: origin_y,
                            visual_offset_x,
                            visual_offset_y,
                            available_width: child_available_width,
                            available_height: child_available_height,
                            viewport_width,
                            viewport_height,
                            percent_base_width: child_percent_base_width,
                            percent_base_height: child_percent_base_height,
                        },
                        arena,
                    );
                });
            } else {
                let placement = LayoutPlacement {
                    parent_x: origin_x,
                    parent_y: origin_y,
                    visual_offset_x,
                    visual_offset_y,
                    available_width: child_available_width,
                    available_height: child_available_height,
                    viewport_width,
                    viewport_height,
                    percent_base_width: child_percent_base_width,
                    percent_base_height: child_percent_base_height,
                };
                let replay = flex_axis_child_replay(
                    layout,
                    child_key,
                    arena,
                    item_main,
                    line_cross,
                    main_cursor,
                    cross_cursor,
                    is_row,
                    gap,
                    cross_size,
                    align,
                    placement,
                    child_parent_hit_test_clip,
                );
                match replay {
                    FlexAxisChildReplay::Skip(state) => {
                        with_layout_place_profile(|profile| {
                            profile.skipped_child_place_calls += 1;
                        });
                        arena.with_element_taken(child_key, |child, _arena| {
                            if let Some(child) = child.as_any_mut().downcast_mut::<Element>() {
                                child.restore_flex_axis_replay_state(
                                    state.target_width,
                                    state.target_height,
                                    state.offset_x,
                                    state.offset_y,
                                    state.assign_width,
                                    state.assign_height,
                                );
                            }
                        });
                    }
                    FlexAxisChildReplay::Place => {
                        with_layout_place_profile(|profile| profile.child_place_calls += 1);
                        arena.with_element_taken(child_key, |child, arena| {
                            let (target_width, target_height) = child.layout_target_size();
                            let item_target_main = if matches!(layout, Layout::Flow { .. }) {
                                if is_row {
                                    target_width.max(0.0)
                                } else {
                                    target_height.max(0.0)
                                }
                            } else {
                                item_main
                            };
                            if is_row {
                                child.set_layout_width(item_target_main);
                            } else {
                                child.set_layout_height(item_target_main);
                            }
                            let stretched_cross = if cross_size == CrossSize::Stretch
                                && child.flex_props().allows_cross_stretch(is_row)
                            {
                                if is_row {
                                    child.set_layout_height(line_cross);
                                } else {
                                    child.set_layout_width(line_cross);
                                }
                                Some(line_cross)
                            } else {
                                None
                            };
                            let alignment_cross = child
                                .cross_alignment_size(is_row, stretched_cross, arena)
                                .max(0.0);
                            let cross_offset =
                                cross_item_offset(line_cross, alignment_cross, align);
                            let (offset_x, offset_y) = if is_row {
                                (main_cursor, cross_cursor + cross_offset)
                            } else {
                                (cross_cursor + cross_offset, main_cursor)
                            };
                            child.set_layout_offset(offset_x, offset_y);
                            child.place(placement, arena);
                        });
                    }
                }
            }
            main_cursor += item_main;
            if line
                .get(item_idx + 1)
                .is_some_and(|next| next.child_index != child_idx)
            {
                main_cursor += distributed_gap;
            }
        }

        cross_cursor += line_cross + gap;
    }
    let flex_children_elapsed_ms = flex_children_started_at.elapsed().as_secs_f64() * 1000.0;
    with_layout_place_profile(|profile| {
        profile.non_axis_child_place_ms += flex_children_elapsed_ms;
    });
}

enum FlexAxisChildReplay {
    Place,
    Skip(FlexAxisReplayState),
}

struct FlexAxisReplayState {
    target_width: f32,
    target_height: f32,
    offset_x: f32,
    offset_y: f32,
    assign_width: bool,
    assign_height: bool,
}

fn flex_axis_child_replay(
    layout: Layout,
    child_key: NodeKey,
    arena: &NodeArena,
    item_main: f32,
    line_cross: f32,
    main_cursor: f32,
    cross_cursor: f32,
    is_row: bool,
    gap: f32,
    cross_size: CrossSize,
    align: Align,
    placement: LayoutPlacement,
    child_parent_hit_test_clip: Option<Rect>,
) -> FlexAxisChildReplay {
    if !matches!(layout, Layout::Flex { .. }) || !is_row || gap.abs() > f32::EPSILON {
        return FlexAxisChildReplay::Place;
    }
    let Some(child_parent_hit_test_clip) = child_parent_hit_test_clip else {
        return FlexAxisChildReplay::Place;
    };
    let Some(child_node) = arena.get(child_key) else {
        record_flex_replay_failure(PlacementSkipFailureReason::NonBaseElement);
        return FlexAxisChildReplay::Place;
    };
    let Some(child_element) = child_node.element.as_any().downcast_ref::<Element>() else {
        record_flex_replay_failure(PlacementSkipFailureReason::NonBaseElement);
        return FlexAxisChildReplay::Place;
    };
    if arena.subtree_dirty_intersects(child_key, DirtyPassMask::PLACEMENT) {
        record_flex_replay_failure(PlacementSkipFailureReason::DirtySubtree);
        return FlexAxisChildReplay::Place;
    }
    if let Some(reason) = arena
        .cached_placement_eligibility_metadata(child_key)
        .first_blocker()
    {
        record_flex_replay_failure(reason);
        return FlexAxisChildReplay::Place;
    }

    let target_width = if is_row {
        item_main
    } else {
        child_node.element.layout_target_size().0
    };
    let target_height = if is_row {
        child_node.element.layout_target_size().1
    } else {
        item_main
    };
    let stretched_cross = if cross_size == CrossSize::Stretch
        && child_node.element.flex_props().allows_cross_stretch(is_row)
    {
        Some(line_cross)
    } else {
        None
    };
    let target_width = if !is_row && stretched_cross.is_some() {
        line_cross
    } else {
        target_width
    };
    let target_height = if is_row && stretched_cross.is_some() {
        line_cross
    } else {
        target_height
    };
    let alignment_cross = child_node
        .element
        .cross_alignment_size(is_row, stretched_cross, arena)
        .max(0.0);
    let cross_offset = cross_item_offset(line_cross, alignment_cross, align);
    let (offset_x, offset_y) = if is_row {
        (main_cursor, cross_cursor + cross_offset)
    } else {
        (cross_cursor + cross_offset, main_cursor)
    };
    let require_width_assignment = is_row || stretched_cross.is_some();
    let require_height_assignment = !is_row || stretched_cross.is_some();
    if let Some(reason) = child_element.flex_axis_placement_replay_failure(
        placement,
        child_parent_hit_test_clip,
        target_width,
        target_height,
        offset_x,
        offset_y,
    ) {
        record_flex_replay_failure(reason);
        return FlexAxisChildReplay::Place;
    }

    FlexAxisChildReplay::Skip(FlexAxisReplayState {
        target_width,
        target_height,
        offset_x,
        offset_y,
        assign_width: require_width_assignment,
        assign_height: require_height_assignment,
    })
}

fn record_flex_replay_failure(reason: PlacementSkipFailureReason) {
    with_layout_place_profile(|profile| {
        profile.placement_skip_failures.record(reason);
    });
}

fn record_axis_placement_eligibility(layout: Layout, child_key: NodeKey, arena: &NodeArena) {
    with_layout_place_profile(|profile| {
        profile.axis_placement_eligibility.record_candidate(layout);
    });
    if arena.subtree_dirty_intersects(child_key, DirtyPassMask::PLACEMENT) {
        with_layout_place_profile(|profile| {
            profile.axis_placement_eligibility.record_dirty_subtree();
        });
        return;
    }
    with_layout_place_profile(|profile| {
        profile.axis_placement_eligibility.record_clean_subtree();
    });
    if let Some(reason) = arena
        .cached_placement_eligibility_metadata(child_key)
        .first_blocker()
    {
        with_layout_place_profile(|profile| {
            profile.axis_placement_eligibility.record_blocker(reason);
        });
    } else {
        with_layout_place_profile(|profile| {
            profile
                .axis_placement_eligibility
                .record_potential_replay_candidate(layout);
        });
    }
}

/// Place absolute-positioned children of an axis-layout container.
///
/// Iterates `children` in order, picks the ones masked as absolute, and
/// dispatches `place` for each.
pub(crate) fn place_absolute_children(
    inputs: PlaceAbsoluteChildrenInputs<'_>,
    arena: &mut NodeArena,
) {
    let PlaceAbsoluteChildrenInputs {
        children,
        absolute_mask,
        origin_x,
        origin_y,
        visual_offset_x,
        visual_offset_y,
        child_available_width,
        child_available_height,
        viewport_width,
        viewport_height,
        child_percent_base_width,
        child_percent_base_height,
    } = inputs;

    let absolute_children_started_at = Instant::now();
    for (idx, child_key) in children.iter().copied().enumerate() {
        if !absolute_mask.get(idx).copied().unwrap_or(false) {
            continue;
        }
        with_layout_place_profile(|profile| {
            profile.child_place_calls += 1;
            profile.absolute_child_place_calls += 1;
        });
        arena.with_element_taken(child_key, |child, arena| {
            child.place(
                LayoutPlacement {
                    parent_x: origin_x,
                    parent_y: origin_y,
                    visual_offset_x,
                    visual_offset_y,
                    available_width: child_available_width,
                    available_height: child_available_height,
                    viewport_width,
                    viewport_height,
                    percent_base_width: child_percent_base_width,
                    percent_base_height: child_percent_base_height,
                },
                arena,
            );
        });
    }
    let absolute_children_elapsed_ms =
        absolute_children_started_at.elapsed().as_secs_f64() * 1000.0;
    with_layout_place_profile(|profile| {
        profile.absolute_child_place_ms += absolute_children_elapsed_ms;
    });
}
