//! Axis-layout place pipeline.
//!
//! Phase F4 of the layout functional refactor. Extracts the body of
//! `Element::place_flex_children` into `place_axis_children` (in-flow
//! flex/inline placement) and `place_absolute_children` (absolute-position
//! children). Caller composes both around its own profile timing scopes.

use crate::time::Instant;

use crate::style::{Align, CrossSize, JustifyContent, Layout};
use crate::view::base_component::{
    DirtyPassMask, Element, ElementTrait, LayoutPlacement, PlacementSkipFailureReason, Rect,
    cross_item_offset, cross_start_offset, layout_place_profile_enabled, main_axis_start_and_gap,
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
/// For `Layout::Inline` / `Layout::Flex` / `Layout::Flow`: sizes children via
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
    let flex_children_started_at = layout_place_profile_enabled().then(Instant::now);

    for (line_idx, line) in info.lines.iter().enumerate() {
        let line_main = info.line_main_sum[line_idx];
        let line_cross = info.line_cross_max[line_idx];
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
                        let cross_offset = cross_item_offset(line_cross, alignment_cross, align);
                        let (offset_x, offset_y) = if is_row {
                            (main_cursor, cross_cursor + cross_offset)
                        } else {
                            (cross_cursor + cross_offset, main_cursor)
                        };
                        child.set_layout_offset(offset_x, offset_y);
                        // Translation fast-path: if the cheap setters above
                        // left this child's relative layout untouched and a
                        // pure ancestor move is the only change, shift the
                        // whole clean subtree by the origin delta instead of
                        // re-running place over it.
                        if let Some((dx, dy)) = translation_replay_delta(
                            child.as_ref(),
                            child_key,
                            arena,
                            placement,
                            child_parent_hit_test_clip,
                        ) {
                            child.translate_in_place(dx, dy);
                            let mut count = 1;
                            for descendant in arena.children_of(child_key) {
                                translate_subtree_walk(descendant, dx, dy, arena, &mut count);
                            }
                            with_layout_place_profile(|profile| {
                                profile.translated_subtree_roots += 1;
                                profile.translated_subtree_nodes += count;
                            });
                        } else {
                            with_layout_place_profile(|profile| profile.child_place_calls += 1);
                            child.place(placement, arena);
                        }
                    });
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
    if let Some(started_at) = flex_children_started_at {
        let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
        with_layout_place_profile(|profile| {
            profile.non_axis_child_place_ms += elapsed_ms;
        });
    }
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
        arena,
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

/// Decide whether an in-flow axis child, already run through this frame's
/// cheap layout setters, can take the translation fast-path. The caller
/// invokes this *after* `set_layout_width/height/offset`, so a sibling
/// reflow that moved the child shows up as fresh local placement dirt and
/// is rejected here. Returns `Some((dx, dy))` only when ALL hold:
///
/// - the new placement differs from the child's last one by the parent
///   origin alone (`translation_only_delta`), and that delta is non-zero;
/// - the setters added no placement dirt (relative offset/size unchanged);
/// - the child's whole subtree is placement-clean and translatable
///   (no anchors / absolute / runtime state / inline IFC / TextArea / …);
/// - the inherited hit-test clip itself only translated by the same
///   `(dx, dy)` — i.e. an ancestor did not resize and reshape the clip.
///
/// Otherwise returns `None` and the caller runs a full `place`.
fn translation_replay_delta(
    child: &dyn ElementTrait,
    child_key: NodeKey,
    arena: &NodeArena,
    placement: LayoutPlacement,
    child_parent_hit_test_clip: Option<Rect>,
) -> Option<(f32, f32)> {
    static DISABLE_TRANSLATE_FASTPATH: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *DISABLE_TRANSLATE_FASTPATH
        .get_or_init(|| std::env::var_os("RFGUI_DISABLE_TRANSLATE_FASTPATH").is_some())
    {
        return None;
    }
    let (dx, dy) = placement.translation_only_delta(child.last_placement()?)?;
    if dx == 0.0 && dy == 0.0 {
        return None;
    }
    // The setters just ran; any change to this child's own relative offset
    // or assigned size shows up as fresh PLACEMENT dirt.
    if child
        .local_dirty_flags()
        .intersects(DirtyPassMask::PLACEMENT)
    {
        return None;
    }
    if arena.subtree_dirty_intersects(child_key, DirtyPassMask::PLACEMENT) {
        return None;
    }
    if !arena
        .cached_placement_eligibility_metadata(child_key)
        .is_translatable()
    {
        return None;
    }
    // The inherited hit-test clip must move rigidly with the subtree. If an
    // ancestor resized (e.g. an expanding panel), its child clip changes
    // shape rather than just translating, and the cached descendant clips
    // would go stale — fall back to a full place in that case.
    if let Some(old_clip) = child.hit_test_clip_rect() {
        let parent_clip = child_parent_hit_test_clip?;
        let shifted = Rect {
            x: old_clip.x + dx,
            y: old_clip.y + dy,
            width: old_clip.width,
            height: old_clip.height,
        };
        if !rects_approx_eq(shifted, parent_clip) {
            return None;
        }
    }
    Some((dx, dy))
}

/// Component-wise approximate equality for two rects (sub-pixel epsilon),
/// matching the tolerance the placement-skip gates use elsewhere.
fn rects_approx_eq(a: Rect, b: Rect) -> bool {
    const EPS: f32 = 0.01;
    (a.x - b.x).abs() <= EPS
        && (a.y - b.y).abs() <= EPS
        && (a.width - b.width).abs() <= EPS
        && (a.height - b.height).abs() <= EPS
}

/// Recursive worker for the translation fast-path: shift `key`'s already
/// resolved absolute geometry by `(dx, dy)` and recurse. `count`
/// accumulates the nodes shifted (for profiling).
fn translate_subtree_walk(key: NodeKey, dx: f32, dy: f32, arena: &NodeArena, count: &mut usize) {
    if let Some(mut node) = arena.get_mut(key) {
        node.element.translate_in_place(dx, dy);
        *count += 1;
    }
    for child in arena.children_of(key) {
        translate_subtree_walk(child, dx, dy, arena, count);
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

    let absolute_children_started_at = layout_place_profile_enabled().then(Instant::now);
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
    if let Some(started_at) = absolute_children_started_at {
        let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
        with_layout_place_profile(|profile| {
            profile.absolute_child_place_ms += elapsed_ms;
        });
    }
}
