//! Low-level retained host elements and traversal helpers used to build custom elements.
use rustc_hash::{FxHashMap, FxHashSet};

use crate::transition::{
    AnimationRequest, LayoutField, LayoutTrackRequest, StyleField, StyleTrackRequest, StyleValue,
    VisualField, VisualTrackRequest,
};
use crate::transition::{ChannelId, TrackKey, TrackTarget};
use crate::ui::{
    BlurEvent, ClickEvent, FocusEvent, ImePreeditEvent, KeyDownEvent, KeyUpEvent, PointerDownEvent,
    PointerEnterEvent, PointerLeaveEvent, PointerMoveEvent, PointerUpEvent, TextInputEvent,
};
use crate::view::viewport::ViewportControl;
use std::sync::atomic::{AtomicU64, Ordering};

mod core;
mod element;
mod image;
mod svg;
mod text;
mod text_area;

pub(crate) use core::*;
pub use element::*;
pub use image::*;
pub use svg::*;
pub use text::*;
pub use text_area::*;

fn next_ui_node_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn collect_box_models(
    root_key: crate::view::node_arena::NodeKey,
    arena: &crate::view::node_arena::NodeArena,
) -> Vec<BoxModelSnapshot> {
    fn walk(
        node: &dyn ElementTrait,
        arena: &crate::view::node_arena::NodeArena,
        out: &mut Vec<BoxModelSnapshot>,
    ) {
        out.push(node.box_model_snapshot());
        for child_key in node.children() {
            if let Some(child_node) = arena.get(*child_key) {
                walk(child_node.element.as_ref(), arena, out);
            }
        }
    }

    let mut out = Vec::new();
    if let Some(root_node) = arena.get(root_key) {
        walk(root_node.element.as_ref(), arena, &mut out);
    }
    out
}

/// Recursive walker kept as a reference / correctness oracle. The hot
/// layout paths now read [`NodeArena::cached_subtree_dirty`] instead,
/// which is refreshed once per pass by
/// [`NodeArena::refresh_subtree_dirty_cache`]. Kept `pub(crate)` + allow
/// dead for any future slow-path callers and for parity with existing
/// tests.
#[allow(dead_code)]
pub(crate) fn subtree_dirty_flags(
    root: &dyn ElementTrait,
    arena: &crate::view::node_arena::NodeArena,
) -> DirtyFlags {
    let mut flags = root.local_dirty_flags();
    for child_key in root.children() {
        if let Some(child_node) = arena.get(*child_key) {
            flags = flags.union(subtree_dirty_flags(child_node.element.as_ref(), arena));
        }
    }
    flags
}

pub(crate) fn clear_subtree_dirty_flags(
    root: &mut dyn ElementTrait,
    flags: DirtyFlags,
    arena: &crate::view::node_arena::NodeArena,
) {
    root.clear_local_dirty_flags(flags);
    // Collect keys before recursing to avoid borrowing `root` while we
    // re-enter the arena for each child.
    let child_keys: Vec<crate::view::node_arena::NodeKey> = root.children().to_vec();
    for child_key in child_keys {
        if let Some(mut child_node) = arena.get_mut(child_key) {
            clear_subtree_dirty_flags(child_node.element.as_mut(), flags, arena);
        }
    }
}

pub(crate) fn can_reuse_promoted_subtree(
    node: &dyn ElementTrait,
    _ctx: &UiBuildContext,
    arena: &crate::view::node_arena::NodeArena,
) -> bool {
    fn walk(node: &dyn ElementTrait, arena: &crate::view::node_arena::NodeArena) -> bool {
        for child_key in node.children() {
            let Some(child_node) = arena.get(*child_key) else {
                continue;
            };
            if !walk(child_node.element.as_ref(), arena) {
                return false;
            }
        }
        true
    }

    walk(node, arena)
}

pub(crate) fn round_layout_value(value: f32) -> f32 {
    if value.is_finite() {
        value.round()
    } else {
        value
    }
}

pub(crate) fn get_debug_element_render_state_by_id(
    root: &dyn ElementTrait,
    node_id: u64,
    arena: &crate::view::node_arena::NodeArena,
) -> Option<DebugElementRenderState> {
    if root.id() == node_id {
        return root
            .as_any()
            .downcast_ref::<Element>()
            .map(Element::debug_render_state);
    }
    for child_key in root.children() {
        let Some(child_node) = arena.get(*child_key) else {
            continue;
        };
        if let Some(state) =
            get_debug_element_render_state_by_id(child_node.element.as_ref(), node_id, arena)
        {
            return Some(state);
        }
    }
    None
}

pub(crate) fn get_debug_promotion_signatures_by_id(
    root: &dyn ElementTrait,
    node_id: u64,
    arena: &crate::view::node_arena::NodeArena,
) -> Option<(u64, u64)> {
    if root.id() == node_id {
        return Some((
            root.promotion_self_signature(),
            root.promotion_clip_intersection_signature(),
        ));
    }
    for child_key in root.children() {
        let Some(child_node) = arena.get(*child_key) else {
            continue;
        };
        if let Some(signatures) =
            get_debug_promotion_signatures_by_id(child_node.element.as_ref(), node_id, arena)
        {
            return Some(signatures);
        }
    }
    None
}

pub(crate) fn get_node_ancestry_ids(
    root: &dyn ElementTrait,
    node_id: u64,
    arena: &crate::view::node_arena::NodeArena,
) -> Option<Vec<u64>> {
    fn walk(
        node: &dyn ElementTrait,
        target_id: u64,
        path: &mut Vec<u64>,
        arena: &crate::view::node_arena::NodeArena,
    ) -> bool {
        path.push(node.id());
        if node.id() == target_id {
            return true;
        }
        for child_key in node.children() {
            let Some(child_node) = arena.get(*child_key) else {
                continue;
            };
            if walk(child_node.element.as_ref(), target_id, path, arena) {
                return true;
            }
        }
        path.pop();
        false
    }

    let mut path = Vec::new();
    if walk(root, node_id, &mut path, arena) {
        Some(path)
    } else {
        None
    }
}

pub(crate) fn build_node_by_id(
    node: &mut dyn ElementTrait,
    node_id: u64,
    graph: &mut crate::view::frame_graph::FrameGraph,
    arena: &mut crate::view::node_arena::NodeArena,
    ctx: &mut UiBuildContext,
) -> bool {
    if node.id() == node_id {
        if let Some(element) = node.as_any().downcast_ref::<Element>() {
            trace_promoted_build(
                "deferred-build-node",
                node_id,
                element.box_model_snapshot().parent_id,
                format!(
                    "promoted={} target={:?}",
                    ctx.is_node_promoted(node_id),
                    ctx.current_target().and_then(|target| target.handle())
                ),
            );
        }
        if ctx.is_node_promoted(node_id) {
            if let Some(element) = node.as_any_mut().downcast_mut::<Element>() {
                if let Some(reason) = element.inline_promotion_rendering_reason(arena) {
                    if reason != "child-scissor-clip-inline"
                        && reason != "child-stencil-clip-inline"
                    {
                        crate::view::viewport::record_debug_reuse_path(
                            crate::view::viewport::DebugReusePathRecord {
                                node_id,
                                context: crate::view::viewport::DebugReusePathContext::Deferred,
                                requested: ctx.promoted_update_kind(node_id).unwrap_or(
                                    crate::view::promotion::PromotedLayerUpdateKind::Reraster,
                                ),
                                can_reuse: false,
                                actual: crate::view::promotion::PromotedLayerUpdateKind::Reraster,
                                reason: Some(reason),
                                clip_rect: element.absolute_clip_scissor_rect(),
                            },
                        );
                        let next_state = element.build(
                            graph,
                            arena,
                            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
                        );
                        ctx.set_state(next_state);
                        return true;
                    }
                }
            }
            let update_kind = ctx
                .promoted_update_kind(node_id)
                .unwrap_or(crate::view::promotion::PromotedLayerUpdateKind::Reraster);
            let can_reuse_subtree = can_reuse_promoted_subtree(node, ctx, arena);
            let can_reuse = matches!(
                update_kind,
                crate::view::promotion::PromotedLayerUpdateKind::Reuse
            ) && can_reuse_subtree;
            let mut node_ctx = UiBuildContext::from_parts(
                ctx.viewport(),
                BuildState::for_layer_subtree_with_ancestor_clip(ctx.ancestor_clip_context()),
            );
            let layer_target = node_ctx.allocate_promoted_layer_target(
                graph,
                node_id,
                node.promotion_composite_bounds(),
            );
            node_ctx.set_current_target(layer_target);
            let next_state = if let Some(element) = node.as_any_mut().downcast_mut::<Element>() {
                element.build_promoted_layer(
                    graph,
                    arena,
                    node_ctx,
                    update_kind,
                    can_reuse,
                    crate::view::viewport::DebugReusePathContext::Deferred,
                )
            } else if can_reuse {
                crate::view::viewport::record_debug_reuse_path(
                    crate::view::viewport::DebugReusePathRecord {
                        node_id,
                        context: crate::view::viewport::DebugReusePathContext::Deferred,
                        requested: update_kind,
                        can_reuse,
                        actual: crate::view::promotion::PromotedLayerUpdateKind::Reuse,
                        reason: None,
                        clip_rect: None,
                    },
                );
                node_ctx.into_state()
            } else {
                crate::view::viewport::record_debug_reuse_path(
                    crate::view::viewport::DebugReusePathRecord {
                        node_id,
                        context: crate::view::viewport::DebugReusePathContext::Deferred,
                        requested: update_kind,
                        can_reuse,
                        actual: crate::view::promotion::PromotedLayerUpdateKind::Reraster,
                        reason: if matches!(
                            update_kind,
                            crate::view::promotion::PromotedLayerUpdateKind::Reuse
                        ) {
                            Some("reuse-blocked")
                        } else {
                            None
                        },
                        clip_rect: None,
                    },
                );
                graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
                    crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
                    crate::view::render_pass::clear_pass::ClearInput {
                        pass_context: node_ctx.graphics_pass_context(),
                        clear_depth_stencil: true,
                    },
                    crate::view::render_pass::clear_pass::ClearOutput {
                        render_target: layer_target,
                    },
                ));
                node.build(graph, arena, node_ctx)
            };
            ctx.merge_child_state_side_effects(&next_state);
            let layer_target = next_state.current_target().unwrap_or(layer_target);
            let composite_bounds = node.promotion_composite_bounds();
            let opacity = if node.as_any().downcast_ref::<Element>().is_some() {
                1.0
            } else {
                node.promotion_node_info().opacity.clamp(0.0, 1.0)
            };
            let parent_target = ctx.current_target().unwrap_or_else(|| {
                let target = ctx.allocate_target(graph);
                ctx.set_current_target(target);
                target
            });
            ctx.set_current_target(parent_target);
            graph.add_graphics_pass(
                crate::view::render_pass::composite_layer_pass::CompositeLayerPass::new(
                    crate::view::render_pass::composite_layer_pass::CompositeLayerParams {
                        rect_pos: [composite_bounds.x, composite_bounds.y],
                        rect_size: [composite_bounds.width, composite_bounds.height],
                        corner_radii: composite_bounds.corner_radii,
                        opacity,
                        scissor_rect: None,
                        clear_target: false,
                    },
                    crate::view::render_pass::composite_layer_pass::CompositeLayerInput {
                        layer: crate::view::render_pass::composite_layer_pass::LayerIn::with_handle(
                            layer_target
                                .handle()
                                .expect("promoted deferred target should exist"),
                        ),
                        pass_context: ctx.graphics_pass_context(),
                    },
                    crate::view::render_pass::composite_layer_pass::CompositeLayerOutput {
                        render_target: parent_target,
                    },
                ),
            );
            ctx.set_current_target(parent_target);
        } else {
            let next_state = node.build(
                graph,
                arena,
                UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
            );
            ctx.set_state(next_state);
        }
        return true;
    }
    // Recurse into arena-resident children. The current `node` is already
    // out of the arena (taken by our caller via `with_element_taken`), so
    // we clone the child-key list and reborrow the arena per child.
    let child_keys: Vec<crate::view::node_arena::NodeKey> = node
        .as_any()
        .downcast_ref::<Element>()
        .map(|el| el.children().to_vec())
        .unwrap_or_default();
    for child_key in child_keys {
        let found = arena
            .with_element_taken(child_key, |child, arena| {
                build_node_by_id(child.as_mut(), node_id, graph, arena, ctx)
            })
            .unwrap_or(false);
        if found {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LayoutTransitionSnapshotSeed {
    pub layout_x: f32,
    pub layout_y: f32,
    pub flow_x: f32,
    pub flow_y: f32,
    pub layout_width: f32,
    pub layout_height: f32,
    pub parent_layout_x: f32,
    pub parent_layout_y: f32,
}

pub(crate) fn collect_layout_transition_snapshots(
    arena: &crate::view::node_arena::NodeArena,
    root_keys: &[crate::view::node_arena::NodeKey],
) -> FxHashMap<u64, LayoutTransitionSnapshotSeed> {
    let mut out = FxHashMap::default();

    fn walk(
        arena: &crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
        parent_layout_x: f32,
        parent_layout_y: f32,
        out: &mut FxHashMap<u64, LayoutTransitionSnapshotSeed>,
    ) {
        let Some(node) = arena.get(key) else { return };
        let snapshot = node.element.box_model_snapshot();
        let can_seed_snapshot = node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .map(Element::can_seed_layout_transition_snapshot)
            .unwrap_or(true);
        let (flow_x, flow_y) = node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .map(Element::layout_flow_origin)
            .unwrap_or((snapshot.x, snapshot.y));
        if can_seed_snapshot {
            out.insert(
                node.element.id(),
                LayoutTransitionSnapshotSeed {
                    layout_x: snapshot.x,
                    layout_y: snapshot.y,
                    flow_x,
                    flow_y,
                    layout_width: snapshot.width,
                    layout_height: snapshot.height,
                    parent_layout_x,
                    parent_layout_y,
                },
            );
        }

        let (next_parent_x, next_parent_y) = node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .map(Element::child_layout_origin)
            .unwrap_or((snapshot.x, snapshot.y));

        let children: Vec<_> = node.children.clone();
        drop(node);
        for child_key in children {
            walk(arena, child_key, next_parent_x, next_parent_y, out);
        }
    }

    for &root_key in root_keys {
        walk(arena, root_key, 0.0, 0.0, &mut out);
    }

    out
}

pub(crate) fn seed_layout_transition_snapshots(
    arena: &mut crate::view::node_arena::NodeArena,
    root_keys: &[crate::view::node_arena::NodeKey],
    snapshots: &FxHashMap<u64, LayoutTransitionSnapshotSeed>,
) {
    fn apply(
        arena: &mut crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
        snapshots: &FxHashMap<u64, LayoutTransitionSnapshotSeed>,
    ) {
        let _ = arena.with_element_taken(key, |element, arena| {
            if let Some(seed) = snapshots.get(&element.id()) {
                if let Some(el) = element.as_any_mut().downcast_mut::<Element>() {
                    el.seed_layout_transition_snapshot(
                        seed.layout_x,
                        seed.layout_y,
                        seed.flow_x,
                        seed.flow_y,
                        seed.layout_width,
                        seed.layout_height,
                        seed.parent_layout_x,
                        seed.parent_layout_y,
                    );
                }
            }
            let children: Vec<_> = element.children().to_vec();
            for child_key in children {
                apply(arena, child_key, snapshots);
            }
        });
    }

    for &root_key in root_keys {
        apply(arena, root_key, snapshots);
    }
}

#[derive(Clone, Copy)]
struct HitTestRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

fn hit_test_point_for_node(node: &dyn ElementTrait, x: f32, y: f32) -> (f32, f32) {
    node.as_any()
        .downcast_ref::<Element>()
        .and_then(|element| element.map_viewport_to_paint_space(x, y))
        .unwrap_or((x, y))
}

fn hit_test_point_in_rect(rect: HitTestRect, x: f32, y: f32) -> bool {
    rect.width > 0.0
        && rect.height > 0.0
        && x >= rect.x
        && y >= rect.y
        && x <= rect.x + rect.width
        && y <= rect.y + rect.height
}

fn hit_test_has_absolute_descendant(node: &dyn ElementTrait) -> bool {
    node.as_any()
        .downcast_ref::<Element>()
        .is_some_and(Element::has_absolute_descendant_for_hit_test)
}

fn find_deepest_in_viewport_clip_subtree(
    arena: &crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    x: f32,
    y: f32,
) -> Option<u64> {
    let node = arena.get(key)?;
    let (x, y) = hit_test_point_for_node(node.element.as_ref(), x, y);
    let snapshot = node.element.box_model_snapshot();
    if !snapshot.should_render && !hit_test_has_absolute_descendant(node.element.as_ref()) {
        return None;
    }
    if !point_in_box_model(&snapshot, x, y) {
        return None;
    }
    if !node.element.hit_test_visible_at(x, y) {
        return None;
    }
    if node.element.intercepts_pointer_at(x, y) {
        return Some(snapshot.node_id);
    }
    let children: Vec<_> = node.children.clone();
    drop(node);
    for child_key in children.iter().rev() {
        if let Some(id) = find_deepest_in_viewport_clip_subtree(arena, *child_key, x, y) {
            return Some(id);
        }
    }
    Some(snapshot.node_id)
}

fn find_viewport_clip_priority(
    arena: &crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    x: f32,
    y: f32,
) -> Option<u64> {
    let node = arena.get(key)?;
    let snapshot = node.element.box_model_snapshot();
    if !snapshot.should_render && !hit_test_has_absolute_descendant(node.element.as_ref()) {
        return None;
    }

    let children: Vec<_> = node.children.clone();
    let is_abs_viewport_clip = node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .map(|element| {
            element.is_absolute_positioned_for_hit_test()
                && element.clip_mode_for_hit_test() == crate::style::ClipMode::Viewport
        })
        .unwrap_or(false);
    drop(node);

    for child_key in children.iter().rev() {
        if let Some(id) = find_viewport_clip_priority(arena, *child_key, x, y) {
            return Some(id);
        }
    }

    if !is_abs_viewport_clip {
        return None;
    }

    if point_in_box_model(&snapshot, x, y) {
        find_deepest_in_viewport_clip_subtree(arena, key, x, y)
    } else {
        None
    }
}

pub fn hit_test(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    viewport_x: f32,
    viewport_y: f32,
) -> Option<u64> {
    fn child_allows_outside_parent_hit(
        child: &dyn ElementTrait,
        x: f32,
        y: f32,
        viewport_rect: HitTestRect,
    ) -> bool {
        let Some(element) = child.as_any().downcast_ref::<Element>() else {
            return false;
        };
        if element.has_absolute_descendant_for_hit_test() {
            return true;
        }
        if !element.is_absolute_positioned_for_hit_test() {
            return false;
        }
        match element.clip_mode_for_hit_test() {
            crate::style::ClipMode::Parent => false,
            crate::style::ClipMode::Viewport => hit_test_point_in_rect(viewport_rect, x, y),
            crate::style::ClipMode::AnchorParent => element.has_anchor_name_for_hit_test(),
        }
    }

    fn find(
        arena: &crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
        x: f32,
        y: f32,
        viewport_rect: HitTestRect,
    ) -> Option<u64> {
        let node = arena.get(key)?;
        let (x, y) = hit_test_point_for_node(node.element.as_ref(), x, y);
        let snapshot = node.element.box_model_snapshot();
        let has_absolute_descendant = node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .is_some_and(Element::has_absolute_descendant_for_hit_test);
        if !snapshot.should_render && !has_absolute_descendant {
            return None;
        }
        let in_self =
            point_in_box_model(&snapshot, x, y) && node.element.hit_test_visible_at(x, y);
        if in_self && node.element.intercepts_pointer_at(x, y) {
            return Some(snapshot.node_id);
        }
        if !in_self && !has_absolute_descendant {
            return None;
        }

        let children: Vec<_> = node.children.clone();
        drop(node);
        for child_key in children.iter().rev() {
            if !in_self {
                let Some(child_node) = arena.get(*child_key) else { continue };
                let allowed = child_allows_outside_parent_hit(
                    child_node.element.as_ref(),
                    x,
                    y,
                    viewport_rect,
                );
                drop(child_node);
                if !allowed {
                    continue;
                }
            }
            if let Some(id) = find(arena, *child_key, x, y, viewport_rect) {
                return Some(id);
            }
        }

        if in_self {
            Some(snapshot.node_id)
        } else {
            None
        }
    }

    let root_snapshot = arena.get(root_key)?.element.box_model_snapshot();
    let viewport_rect = HitTestRect {
        x: root_snapshot.x,
        y: root_snapshot.y,
        width: root_snapshot.width.max(0.0),
        height: root_snapshot.height.max(0.0),
    };
    if let Some(id) = find_viewport_clip_priority(arena, root_key, viewport_x, viewport_y) {
        return Some(id);
    }
    find(arena, root_key, viewport_x, viewport_y, viewport_rect)
}

/// Hit-test across multiple UI roots in a single pass.
///
/// 1. Searches all roots for **nested** viewport-clipped absolute elements
///    (dropdowns, popovers) which render on top of everything via deferred
///    rendering. Root-level viewport-clip elements (Window shells) are
///    intentionally skipped so they don't shadow deeper targets in other roots.
/// 2. If no nested viewport-clip match is found, falls back to the normal
///    per-root `hit_test` (reverse order — last root has highest priority).
///
/// Returns `Some((root_index, target_node_id))`.
pub fn hit_test_roots(
    arena: &crate::view::node_arena::NodeArena,
    root_keys: &[crate::view::node_arena::NodeKey],
    viewport_x: f32,
    viewport_y: f32,
) -> Option<(usize, u64)> {
    let mut fallback: Option<(usize, u64)> = None;
    for (idx, &root_key) in root_keys.iter().enumerate().rev() {
        let Some(root) = arena.get(root_key) else { continue };
        let snapshot = root.element.box_model_snapshot();
        let has_abs = root
            .element
            .as_any()
            .downcast_ref::<Element>()
            .is_some_and(Element::has_absolute_descendant_for_hit_test);
        if !snapshot.should_render && !has_abs {
            continue;
        }
        let children: Vec<_> = root.children.clone();
        drop(root);
        for child_key in children.iter().rev() {
            if let Some(id) = find_viewport_clip_priority(arena, *child_key, viewport_x, viewport_y)
            {
                return Some((idx, id));
            }
        }
        if fallback.is_none() {
            if let Some(id) = hit_test(arena, root_key, viewport_x, viewport_y) {
                fallback = Some((idx, id));
            }
        }
    }
    fallback
}

/// Build the `target → root` path (DOM `composedPath` order) for the given
/// target within `root`. Returns empty if the target is not in the tree.
fn composed_path_for_target(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_id: u64,
) -> Vec<crate::ui::NodeId> {
    let mut path = Vec::new();
    if append_path_to_target(arena, root_key, target_id, &mut path) {
        path.reverse();
        path.into_iter().map(crate::ui::NodeId::from).collect()
    } else {
        Vec::new()
    }
}

pub fn dispatch_pointer_down_from_hit_test(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    event: &mut PointerDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_id) = hit_test(arena, root_key, event.pointer.viewport_x, event.pointer.viewport_y) else {
        return false;
    };
    event.meta.set_target_id(target_id.into());
    event.meta.set_path(composed_path_for_target(arena, root_key, target_id));
    dispatch_pointer_down_bubble(arena, root_key, target_id, event, control)
}

pub fn dispatch_pointer_up_from_hit_test(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    event: &mut PointerUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_id) = hit_test(arena, root_key, event.pointer.viewport_x, event.pointer.viewport_y) else {
        return false;
    };
    event.meta.set_target_id(target_id.into());
    event.meta.set_path(composed_path_for_target(arena, root_key, target_id));
    dispatch_pointer_up_bubble(arena, root_key, target_id, event, control)
}

pub(crate) fn dispatch_pointer_up_to_target(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut PointerUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    event.meta.set_target_id(target_id.into());
    event.meta.set_path(composed_path_for_target(arena, root_key, target_id));
    dispatch_pointer_up_bubble(arena, root_key, target_id, event, control)
}

pub fn dispatch_pointer_move_from_hit_test(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    event: &mut PointerMoveEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_id) = hit_test(arena, root_key, event.pointer.viewport_x, event.pointer.viewport_y) else {
        return false;
    };
    event.meta.set_target_id(target_id.into());
    event.meta.set_path(composed_path_for_target(arena, root_key, target_id));
    dispatch_pointer_move_bubble(arena, root_key, target_id, event, control)
}

pub(crate) fn dispatch_pointer_move_to_target(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut PointerMoveEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    event.meta.set_target_id(target_id.into());
    event.meta.set_path(composed_path_for_target(arena, root_key, target_id));
    dispatch_pointer_move_bubble(arena, root_key, target_id, event, control)
}

pub fn dispatch_click_from_hit_test(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    event: &mut ClickEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_id) = hit_test(arena, root_key, event.pointer.viewport_x, event.pointer.viewport_y) else {
        return false;
    };
    event.meta.set_target_id(target_id.into());
    event.meta.set_path(composed_path_for_target(arena, root_key, target_id));
    dispatch_click_bubble(arena, root_key, target_id, event, control)
}

pub(crate) fn dispatch_click_to_target(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut ClickEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    event.meta.set_target_id(target_id.into());
    event.meta.set_path(composed_path_for_target(arena, root_key, target_id));
    dispatch_click_bubble(arena, root_key, target_id, event, control)
}

pub fn dispatch_scroll_from_hit_test(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    viewport_x: f32,
    viewport_y: f32,
    delta_x: f32,
    delta_y: f32,
) -> bool {
    let Some(target_id) = hit_test(arena, root_key, viewport_x, viewport_y) else {
        return false;
    };
    dispatch_scroll_bubble(arena, root_key, target_id, delta_x, delta_y)
}

pub(crate) fn find_scroll_handler_from_target(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_id: u64,
    delta_x: f32,
    delta_y: f32,
) -> Option<u64> {
    find_scroll_handler_bubble(arena, root_key, target_id, delta_x, delta_y)
}

pub(crate) fn dispatch_scroll_to_target(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_id: u64,
    delta_x: f32,
    delta_y: f32,
) -> bool {
    dispatch_scroll_bubble(arena, root_key, target_id, delta_x, delta_y)
}

pub fn get_scroll_offset_by_id(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    node_id: u64,
) -> Option<(f32, f32)> {
    let node = arena.get(root_key)?;
    if node.element.id() == node_id {
        return Some(node.element.get_scroll_offset());
    }
    let children: Vec<_> = node.children.clone();
    drop(node);
    for child_key in children {
        if let Some(offset) = get_scroll_offset_by_id(arena, child_key, node_id) {
            return Some(offset);
        }
    }
    None
}

pub fn set_scroll_offset_by_id(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    node_id: u64,
    offset: (f32, f32),
) -> bool {
    arena
        .with_element_taken(root_key, |element, arena| {
            if element.id() == node_id {
                element.set_scroll_offset(offset);
                return true;
            }
            let children: Vec<_> = element.children().to_vec();
            for child_key in children {
                if set_scroll_offset_by_id(arena, child_key, node_id, offset) {
                    return true;
                }
            }
            false
        })
        .unwrap_or(false)
}

pub(crate) fn take_style_transition_requests(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    out: &mut Vec<StyleTrackRequest>,
) {
    let _ = arena.with_element_taken(root_key, |element, arena| {
        let children: Vec<_> = element.children().to_vec();
        for child_key in children.into_iter().rev() {
            take_style_transition_requests(arena, child_key, out);
        }
        out.extend(element.take_style_transition_requests());
    });
}

pub(crate) fn take_animation_requests(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    out: &mut Vec<AnimationRequest>,
) {
    let _ = arena.with_element_taken(root_key, |element, arena| {
        let children: Vec<_> = element.children().to_vec();
        for child_key in children.into_iter().rev() {
            take_animation_requests(arena, child_key, out);
        }
        out.extend(element.take_animation_requests());
    });
}

pub(crate) fn take_layout_transition_requests(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    out: &mut Vec<LayoutTrackRequest>,
) {
    let _ = arena.with_element_taken(root_key, |element, arena| {
        let children: Vec<_> = element.children().to_vec();
        for child_key in children.into_iter().rev() {
            take_layout_transition_requests(arena, child_key, out);
        }
        out.extend(element.take_layout_transition_requests());
    });
}

pub(crate) fn take_visual_transition_requests(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    out: &mut Vec<VisualTrackRequest>,
) {
    let _ = arena.with_element_taken(root_key, |element, arena| {
        let children: Vec<_> = element.children().to_vec();
        for child_key in children.into_iter().rev() {
            take_visual_transition_requests(arena, child_key, out);
        }
        out.extend(element.take_visual_transition_requests());
    });
}

pub(crate) fn collect_transition_track_allowlist(
    arena: &crate::view::node_arena::NodeArena,
    root_keys: &[crate::view::node_arena::NodeKey],
) -> FxHashSet<TrackKey<TrackTarget>> {
    let mut out = FxHashSet::default();

    fn walk(
        arena: &crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
        out: &mut FxHashSet<TrackKey<TrackTarget>>,
    ) {
        let Some(node) = arena.get(key) else { return };
        if let Some(element) = node.element.as_any().downcast_ref::<Element>() {
            for channel in element.active_transition_channels() {
                out.insert(TrackKey {
                    target: node.element.id(),
                    channel,
                });
            }
        }
        let children: Vec<_> = node.children.clone();
        drop(node);
        for child_key in children {
            walk(arena, child_key, out);
        }
    }

    for &root_key in root_keys {
        walk(arena, root_key, &mut out);
    }

    out
}

pub(crate) fn collect_node_id_allowlist(
    arena: &crate::view::node_arena::NodeArena,
    root_keys: &[crate::view::node_arena::NodeKey],
) -> FxHashSet<u64> {
    let mut out = FxHashSet::default();

    fn walk(
        arena: &crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
        out: &mut FxHashSet<u64>,
    ) {
        let Some(node) = arena.get(key) else { return };
        out.insert(node.element.id());
        let children: Vec<_> = node.children.clone();
        drop(node);
        for child_key in children {
            walk(arena, child_key, out);
        }
    }

    for &root_key in root_keys {
        walk(arena, root_key, &mut out);
    }

    out
}

pub(crate) fn reconcile_transition_runtime_state(
    arena: &mut crate::view::node_arena::NodeArena,
    root_keys: &[crate::view::node_arena::NodeKey],
    active_channels_by_node: &FxHashMap<u64, FxHashSet<ChannelId>>,
) -> bool {
    fn walk(
        arena: &mut crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
        active_channels_by_node: &FxHashMap<u64, FxHashSet<ChannelId>>,
    ) -> bool {
        arena
            .with_element_taken(key, |element, arena| {
                let mut changed = false;
                let node_id = element.id();
                if let Some(el) = element.as_any_mut().downcast_mut::<Element>() {
                    changed |=
                        el.reconcile_transition_runtime_state(active_channels_by_node.get(&node_id));
                }
                let children: Vec<_> = element.children().to_vec();
                for child_key in children {
                    changed |= walk(arena, child_key, active_channels_by_node);
                }
                changed
            })
            .unwrap_or(false)
    }

    let mut changed = false;
    for &root_key in root_keys {
        changed |= walk(arena, root_key, active_channels_by_node);
    }
    changed
}

pub(crate) fn set_style_field_by_id(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    node_id: u64,
    field: StyleField,
    value: StyleValue,
) -> bool {
    arena
        .with_element_taken(root_key, |root, arena| {
            set_style_field_by_id_inner(root.as_mut(), arena, node_id, field, value)
        })
        .unwrap_or(false)
}

fn set_style_field_by_id_inner(
    root: &mut dyn ElementTrait,
    arena: &mut crate::view::node_arena::NodeArena,
    node_id: u64,
    field: StyleField,
    value: StyleValue,
) -> bool {
    if root.id() == node_id {
        if let Some(element) = root.as_any_mut().downcast_mut::<Element>() {
            match field {
                StyleField::Opacity => {
                    if let StyleValue::Scalar(value) = value {
                        element.set_opacity(value);
                    } else {
                        return false;
                    }
                }
                StyleField::BorderRadius => {
                    if let StyleValue::Scalar(value) = value {
                        element.set_border_radius_transition_sample(value);
                    } else {
                        return false;
                    }
                }
                StyleField::BackgroundColor => {
                    if let StyleValue::Color(color) = value {
                        element.set_background_color_value(color);
                    } else {
                        return false;
                    }
                }
                StyleField::Color => {
                    if let StyleValue::Color(color) = value {
                        element.set_foreground_color(color);
                    } else {
                        return false;
                    }
                }
                StyleField::BorderTopColor => {
                    if let StyleValue::Color(color) = value {
                        element.set_border_top_color(color);
                    } else {
                        return false;
                    }
                }
                StyleField::BorderRightColor => {
                    if let StyleValue::Color(color) = value {
                        element.set_border_right_color(color);
                    } else {
                        return false;
                    }
                }
                StyleField::BorderBottomColor => {
                    if let StyleValue::Color(color) = value {
                        element.set_border_bottom_color(color);
                    } else {
                        return false;
                    }
                }
                StyleField::BorderLeftColor => {
                    if let StyleValue::Color(color) = value {
                        element.set_border_left_color(color);
                    } else {
                        return false;
                    }
                }
                StyleField::BoxShadow => {
                    if let StyleValue::BoxShadow(box_shadows) = value {
                        element.set_box_shadows(box_shadows);
                    } else {
                        return false;
                    }
                }
                StyleField::Transform => match value {
                    StyleValue::Transform(transform) => {
                        element.set_transform_value(transform);
                    }
                    StyleValue::TransformProgress { from, to, progress } => {
                        element.set_transform_progress_value(from, to, progress);
                    }
                    _ => {
                        return false;
                    }
                },
                StyleField::TransformOrigin => match value {
                    StyleValue::TransformOrigin(transform_origin) => {
                        element.set_transform_origin_value(transform_origin);
                    }
                    StyleValue::TransformOriginProgress { from, to, progress } => {
                        element.set_transform_origin_progress_value(from, to, progress);
                    }
                    _ => return false,
                },
            }
            return true;
        }
        return false;
    }
    let children: Vec<_> = root.children().to_vec();
    for child_key in children {
        if set_style_field_by_id(arena, child_key, node_id, field, value.clone()) {
            return true;
        }
    }
    false
}

pub(crate) fn set_layout_field_by_id(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    node_id: u64,
    field: LayoutField,
    value: f32,
) -> bool {
    arena
        .with_element_taken(root_key, |root, arena| {
            set_layout_field_by_id_inner(root.as_mut(), arena, node_id, field, value)
        })
        .unwrap_or(false)
}

fn set_layout_field_by_id_inner(
    root: &mut dyn ElementTrait,
    arena: &mut crate::view::node_arena::NodeArena,
    node_id: u64,
    field: LayoutField,
    value: f32,
) -> bool {
    if root.id() == node_id {
        if let Some(element) = root.as_any_mut().downcast_mut::<Element>() {
            match field {
                LayoutField::Width => element.set_layout_transition_width(value),
                LayoutField::Height => element.set_layout_transition_height(value),
                LayoutField::X | LayoutField::Y => return false,
            }
            return true;
        }
        return false;
    }
    let children: Vec<_> = root.children().to_vec();
    for child_key in children {
        if set_layout_field_by_id(arena, child_key, node_id, field, value) {
            if let Some(element) = root.as_any_mut().downcast_mut::<Element>() {
                element.mark_layout_dirty();
            }
            return true;
        }
    }
    false
}

pub(crate) fn set_visual_field_by_id(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    node_id: u64,
    field: VisualField,
    value: f32,
) -> bool {
    arena
        .with_element_taken(root_key, |root, arena| {
            set_visual_field_by_id_inner(root.as_mut(), arena, node_id, field, value)
        })
        .unwrap_or(false)
}

fn set_visual_field_by_id_inner(
    root: &mut dyn ElementTrait,
    arena: &mut crate::view::node_arena::NodeArena,
    node_id: u64,
    field: VisualField,
    value: f32,
) -> bool {
    if root.id() == node_id {
        if let Some(element) = root.as_any_mut().downcast_mut::<Element>() {
            match field {
                VisualField::X => element.set_layout_transition_x(value),
                VisualField::Y => element.set_layout_transition_y(value),
            }
            return true;
        }
        return false;
    }
    let children: Vec<_> = root.children().to_vec();
    for child_key in children {
        if set_visual_field_by_id(arena, child_key, node_id, field, value) {
            return true;
        }
    }
    false
}

pub(crate) fn update_hover_state(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_id: Option<u64>,
) -> bool {
    fn walk(
        arena: &mut crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
        target_id: Option<u64>,
    ) -> (bool, bool) {
        arena
            .with_element_taken(key, |element, arena| {
                let self_id = element.id();
                let mut contains_target = target_id == Some(self_id);
                let mut changed = false;
                let children: Vec<_> = element.children().to_vec();
                for child_key in children.into_iter().rev() {
                    let (child_contains_target, child_changed) = walk(arena, child_key, target_id);
                    contains_target |= child_contains_target;
                    changed |= child_changed;
                }
                changed |= element.set_hovered(contains_target);
                (contains_target, changed)
            })
            .unwrap_or((false, false))
    }

    walk(arena, root_key, target_id).1
}

fn append_path_to_target(
    arena: &crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    target_id: u64,
    path: &mut Vec<u64>,
) -> bool {
    let Some(node) = arena.get(key) else { return false };
    let self_id = node.element.id();
    path.push(self_id);
    if self_id == target_id {
        return true;
    }
    let children: Vec<_> = node.children.clone();
    drop(node);
    for child_key in children {
        if append_path_to_target(arena, child_key, target_id, path) {
            return true;
        }
    }
    let _ = path.pop();
    false
}

pub(crate) fn hover_path_for_target(
    arena: &crate::view::node_arena::NodeArena,
    root_keys: &[crate::view::node_arena::NodeKey],
    target_id: Option<u64>,
) -> Vec<u64> {
    let Some(target_id) = target_id else {
        return Vec::new();
    };

    for &root_key in root_keys {
        let mut path = Vec::new();
        if append_path_to_target(arena, root_key, target_id, &mut path) {
            return path;
        }
    }

    Vec::new()
}

fn dispatch_pointer_enter_to_target(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    target_id: u64,
    related: Option<u64>,
) -> bool {
    arena
        .with_element_taken(key, |element, arena| {
            if element.id() == target_id {
                let mut meta = crate::ui::EventMeta::new(target_id.into());
                meta.set_related_target(
                    related.map(|id| crate::ui::EventTarget::bare(crate::ui::NodeId::from(id))),
                );
                let mut event = PointerEnterEvent { meta };
                element.dispatch_pointer_enter(&mut event);
                return true;
            }
            let children: Vec<_> = element.children().to_vec();
            for child_key in children {
                if dispatch_pointer_enter_to_target(arena, child_key, target_id, related) {
                    return true;
                }
            }
            false
        })
        .unwrap_or(false)
}

fn dispatch_pointer_leave_to_target(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    target_id: u64,
    related: Option<u64>,
) -> bool {
    arena
        .with_element_taken(key, |element, arena| {
            if element.id() == target_id {
                let mut meta = crate::ui::EventMeta::new(target_id.into());
                meta.set_related_target(
                    related.map(|id| crate::ui::EventTarget::bare(crate::ui::NodeId::from(id))),
                );
                let mut event = PointerLeaveEvent { meta };
                element.dispatch_pointer_leave(&mut event);
                return true;
            }
            let children: Vec<_> = element.children().to_vec();
            for child_key in children {
                if dispatch_pointer_leave_to_target(arena, child_key, target_id, related) {
                    return true;
                }
            }
            false
        })
        .unwrap_or(false)
}

pub(crate) fn dispatch_hover_transition(
    arena: &mut crate::view::node_arena::NodeArena,
    root_keys: &[crate::view::node_arena::NodeKey],
    previous_target: Option<u64>,
    next_target: Option<u64>,
) -> bool {
    if previous_target == next_target {
        return false;
    }

    let previous_path = hover_path_for_target(arena, root_keys, previous_target);
    let next_path = hover_path_for_target(arena, root_keys, next_target);

    let mut common_prefix_len = 0;
    while common_prefix_len < previous_path.len()
        && common_prefix_len < next_path.len()
        && previous_path[common_prefix_len] == next_path[common_prefix_len]
    {
        common_prefix_len += 1;
    }

    let mut dispatched = false;

    for &node_id in previous_path[common_prefix_len..].iter().rev() {
        for &root_key in root_keys {
            if dispatch_pointer_leave_to_target(arena, root_key, node_id, next_target) {
                dispatched = true;
                break;
            }
        }
    }

    for &node_id in &next_path[common_prefix_len..] {
        for &root_key in root_keys {
            if dispatch_pointer_enter_to_target(arena, root_key, node_id, previous_target) {
                dispatched = true;
                break;
            }
        }
    }

    dispatched
}

pub(crate) fn cancel_pointer_interactions(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
) -> bool {
    fn walk(
        arena: &mut crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
    ) -> bool {
        arena
            .with_element_taken(key, |element, arena| {
                let mut changed = element.cancel_pointer_interaction();
                let children: Vec<_> = element.children().to_vec();
                for child_key in children.into_iter().rev() {
                    changed |= walk(arena, child_key);
                }
                changed
            })
            .unwrap_or(false)
    }

    walk(arena, root_key)
}

pub(crate) fn dispatch_key_down_bubble(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut KeyDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_key_down_impl(arena, root_key, target_id, event, control)
}

pub(crate) fn dispatch_key_up_bubble(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut KeyUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_key_up_impl(arena, root_key, target_id, event, control)
}

pub(crate) fn dispatch_text_input_bubble(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut TextInputEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_text_input_impl(arena, root_key, target_id, event, control)
}

pub(crate) fn dispatch_ime_preedit_bubble(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut ImePreeditEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_ime_preedit_impl(arena, root_key, target_id, event, control)
}

pub(crate) fn dispatch_focus_bubble(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut FocusEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_focus_impl(arena, root_key, target_id, event, control)
}

pub(crate) fn dispatch_blur_bubble(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut BlurEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_blur_impl(arena, root_key, target_id, event, control)
}

fn local_point_for_node(
    node: &dyn ElementTrait,
    snapshot: &BoxModelSnapshot,
    viewport_x: f32,
    viewport_y: f32,
) -> (f32, f32) {
    let (paint_x, paint_y) = node
        .as_any()
        .downcast_ref::<Element>()
        .and_then(|element| element.map_viewport_to_paint_space(viewport_x, viewport_y))
        .unwrap_or((viewport_x, viewport_y));
    (paint_x - snapshot.x, paint_y - snapshot.y)
}

fn dispatch_pointer_down_bubble(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut PointerDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    arena
        .with_element_taken(key, |element, arena| {
            let node_id = element.id();
            let snapshot = element.box_model_snapshot();
            let mut found = node_id == target_id;

            if !found {
                let children: Vec<_> = element.children().to_vec();
                for child_key in children.into_iter().rev() {
                    if dispatch_pointer_down_bubble(arena, child_key, target_id, event, control) {
                        found = true;
                        break;
                    }
                }
            }

            if !found || event.meta.propagation_stopped() {
                return found;
            }

            let (local_x, local_y) = local_point_for_node(
                element.as_ref(),
                &snapshot,
                event.pointer.viewport_x,
                event.pointer.viewport_y,
            );
            event.pointer.local_x = local_x;
            event.pointer.local_y = local_y;
            let ct = crate::ui::EventTarget {
                id: node_id.into(),
                bounds: crate::ui::Rect::new(
                    snapshot.x,
                    snapshot.y,
                    snapshot.width,
                    snapshot.height,
                ),
                local_bounds: crate::ui::Rect::new(0.0, 0.0, snapshot.width, snapshot.height),
            };
            event.meta.set_current_target(ct);
            element.dispatch_pointer_down(event, control);
            true
        })
        .unwrap_or(false)
}

fn dispatch_pointer_up_bubble(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut PointerUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    arena
        .with_element_taken(key, |element, arena| {
            let node_id = element.id();
            let snapshot = element.box_model_snapshot();
            let mut found = node_id == target_id;

            if !found {
                let children: Vec<_> = element.children().to_vec();
                for child_key in children.into_iter().rev() {
                    if dispatch_pointer_up_bubble(arena, child_key, target_id, event, control) {
                        found = true;
                        break;
                    }
                }
            }

            if !found || event.meta.propagation_stopped() {
                return found;
            }

            let (local_x, local_y) = local_point_for_node(
                element.as_ref(),
                &snapshot,
                event.pointer.viewport_x,
                event.pointer.viewport_y,
            );
            event.pointer.local_x = local_x;
            event.pointer.local_y = local_y;
            let ct = crate::ui::EventTarget {
                id: node_id.into(),
                bounds: crate::ui::Rect::new(
                    snapshot.x,
                    snapshot.y,
                    snapshot.width,
                    snapshot.height,
                ),
                local_bounds: crate::ui::Rect::new(0.0, 0.0, snapshot.width, snapshot.height),
            };
            event.meta.set_current_target(ct);
            element.dispatch_pointer_up(event, control);
            true
        })
        .unwrap_or(false)
}

fn dispatch_pointer_move_bubble(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut PointerMoveEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    arena
        .with_element_taken(key, |element, arena| {
            let node_id = element.id();
            let snapshot = element.box_model_snapshot();
            let mut found = node_id == target_id;

            if !found {
                let children: Vec<_> = element.children().to_vec();
                for child_key in children.into_iter().rev() {
                    if dispatch_pointer_move_bubble(arena, child_key, target_id, event, control) {
                        found = true;
                        break;
                    }
                }
            }

            if !found || event.meta.propagation_stopped() {
                return found;
            }

            let (local_x, local_y) = local_point_for_node(
                element.as_ref(),
                &snapshot,
                event.pointer.viewport_x,
                event.pointer.viewport_y,
            );
            event.pointer.local_x = local_x;
            event.pointer.local_y = local_y;
            let ct = crate::ui::EventTarget {
                id: node_id.into(),
                bounds: crate::ui::Rect::new(
                    snapshot.x,
                    snapshot.y,
                    snapshot.width,
                    snapshot.height,
                ),
                local_bounds: crate::ui::Rect::new(0.0, 0.0, snapshot.width, snapshot.height),
            };
            event.meta.set_current_target(ct);
            element.dispatch_pointer_move(event, control);
            true
        })
        .unwrap_or(false)
}

fn dispatch_click_bubble(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut ClickEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    arena
        .with_element_taken(key, |element, arena| {
            let node_id = element.id();
            let snapshot = element.box_model_snapshot();
            let mut found = node_id == target_id;

            if !found {
                let children: Vec<_> = element.children().to_vec();
                for child_key in children.into_iter().rev() {
                    if dispatch_click_bubble(arena, child_key, target_id, event, control) {
                        found = true;
                        break;
                    }
                }
            }

            if !found || event.meta.propagation_stopped() {
                return found;
            }

            let (local_x, local_y) = local_point_for_node(
                element.as_ref(),
                &snapshot,
                event.pointer.viewport_x,
                event.pointer.viewport_y,
            );
            event.pointer.local_x = local_x;
            event.pointer.local_y = local_y;
            let ct = crate::ui::EventTarget {
                id: node_id.into(),
                bounds: crate::ui::Rect::new(
                    snapshot.x,
                    snapshot.y,
                    snapshot.width,
                    snapshot.height,
                ),
                local_bounds: crate::ui::Rect::new(0.0, 0.0, snapshot.width, snapshot.height),
            };
            event.meta.set_current_target(ct);
            element.dispatch_click(event, control);
            true
        })
        .unwrap_or(false)
}

fn dispatch_scroll_bubble(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    target_id: u64,
    dx: f32,
    dy: f32,
) -> bool {
    fn walk(
        arena: &mut crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
        target_id: u64,
        dx: f32,
        dy: f32,
    ) -> (bool, bool) {
        arena
            .with_element_taken(key, |element, arena| {
                let node_id = element.id();
                if node_id == target_id {
                    let handled = element.scroll_by(dx, dy);
                    return (true, handled);
                }
                let children: Vec<_> = element.children().to_vec();
                for child_key in children.into_iter().rev() {
                    let (found, handled) = walk(arena, child_key, target_id, dx, dy);
                    if !found {
                        continue;
                    }
                    if handled {
                        return (true, true);
                    }
                    let self_handled = element.scroll_by(dx, dy);
                    return (true, self_handled);
                }
                (false, false)
            })
            .unwrap_or((false, false))
    }

    walk(arena, key, target_id, dx, dy).1
}

fn find_scroll_handler_bubble(
    arena: &crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    target_id: u64,
    dx: f32,
    dy: f32,
) -> Option<u64> {
    fn walk(
        arena: &crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
        target_id: u64,
        dx: f32,
        dy: f32,
    ) -> (bool, Option<u64>) {
        let Some(node) = arena.get(key) else { return (false, None) };
        let node_id = node.element.id();
        if node_id == target_id {
            let can = node.element.can_scroll_by(dx, dy);
            return if can { (true, Some(node_id)) } else { (true, None) };
        }
        let children: Vec<_> = node.children.clone();
        let can_self = node.element.can_scroll_by(dx, dy);
        drop(node);
        for child_key in children.iter().rev() {
            let (found, handled) = walk(arena, *child_key, target_id, dx, dy);
            if !found {
                continue;
            }
            if handled.is_some() {
                return (true, handled);
            }
            if can_self {
                return (true, Some(node_id));
            }
            return (true, None);
        }
        (false, None)
    }

    walk(arena, key, target_id, dx, dy).1
}

fn dispatch_key_down_impl(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut KeyDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    arena
        .with_element_taken(key, |element, arena| {
            let node_id = element.id();
            let mut found = node_id == target_id;
            if !found {
                let children: Vec<_> = element.children().to_vec();
                for child_key in children.into_iter().rev() {
                    if dispatch_key_down_impl(arena, child_key, target_id, event, control) {
                        found = true;
                        break;
                    }
                }
            }
            if !found || event.meta.propagation_stopped() {
                return found;
            }
            event.meta.set_current_target_id(node_id.into());
            element.dispatch_key_down(event, control);
            true
        })
        .unwrap_or(false)
}

fn dispatch_key_up_impl(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut KeyUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    arena
        .with_element_taken(key, |element, arena| {
            let node_id = element.id();
            let mut found = node_id == target_id;
            if !found {
                let children: Vec<_> = element.children().to_vec();
                for child_key in children.into_iter().rev() {
                    if dispatch_key_up_impl(arena, child_key, target_id, event, control) {
                        found = true;
                        break;
                    }
                }
            }
            if !found || event.meta.propagation_stopped() {
                return found;
            }
            event.meta.set_current_target_id(node_id.into());
            element.dispatch_key_up(event, control);
            true
        })
        .unwrap_or(false)
}

fn dispatch_focus_impl(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut FocusEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    arena
        .with_element_taken(key, |element, arena| {
            let node_id = element.id();
            let mut found = node_id == target_id;
            if !found {
                let children: Vec<_> = element.children().to_vec();
                for child_key in children.into_iter().rev() {
                    if dispatch_focus_impl(arena, child_key, target_id, event, control) {
                        found = true;
                        break;
                    }
                }
            }
            if !found || event.meta.propagation_stopped() {
                return found;
            }
            event.meta.set_current_target_id(node_id.into());
            element.dispatch_focus(event, control);
            true
        })
        .unwrap_or(false)
}

fn dispatch_text_input_impl(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut TextInputEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    arena
        .with_element_taken(key, |element, arena| {
            let node_id = element.id();
            let mut found = node_id == target_id;
            if !found {
                let children: Vec<_> = element.children().to_vec();
                for child_key in children.into_iter().rev() {
                    if dispatch_text_input_impl(arena, child_key, target_id, event, control) {
                        found = true;
                        break;
                    }
                }
            }
            if !found || event.meta.propagation_stopped() {
                return found;
            }
            event.meta.set_current_target_id(node_id.into());
            element.dispatch_text_input(event, control);
            true
        })
        .unwrap_or(false)
}

fn dispatch_ime_preedit_impl(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut ImePreeditEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    arena
        .with_element_taken(key, |element, arena| {
            let node_id = element.id();
            let mut found = node_id == target_id;
            if !found {
                let children: Vec<_> = element.children().to_vec();
                for child_key in children.into_iter().rev() {
                    if dispatch_ime_preedit_impl(arena, child_key, target_id, event, control) {
                        found = true;
                        break;
                    }
                }
            }
            if !found || event.meta.propagation_stopped() {
                return found;
            }
            event.meta.set_current_target_id(node_id.into());
            element.dispatch_ime_preedit(event, control);
            true
        })
        .unwrap_or(false)
}

fn dispatch_blur_impl(
    arena: &mut crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    target_id: u64,
    event: &mut BlurEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    arena
        .with_element_taken(key, |element, arena| {
            let node_id = element.id();
            let mut found = node_id == target_id;
            if !found {
                let children: Vec<_> = element.children().to_vec();
                for child_key in children.into_iter().rev() {
                    if dispatch_blur_impl(arena, child_key, target_id, event, control) {
                        found = true;
                        break;
                    }
                }
            }
            if !found || event.meta.propagation_stopped() {
                return found;
            }
            event.meta.set_current_target_id(node_id.into());
            element.dispatch_blur(event, control);
            true
        })
        .unwrap_or(false)
}

fn point_in_box_model(snapshot: &BoxModelSnapshot, x: f32, y: f32) -> bool {
    if snapshot.width <= 0.0 || snapshot.height <= 0.0 {
        return false;
    }

    let left = snapshot.x;
    let top = snapshot.y;
    let right = left + snapshot.width;
    let bottom = top + snapshot.height;
    if x < left || x > right || y < top || y > bottom {
        return false;
    }

    let r = snapshot
        .border_radius
        .max(0.0)
        .min(snapshot.width * 0.5)
        .min(snapshot.height * 0.5);
    if r <= 0.0 {
        return true;
    }

    let tl = (left + r, top + r);
    let tr = (right - r, top + r);
    let bl = (left + r, bottom - r);
    let br = (right - r, bottom - r);

    if x < tl.0 && y < tl.1 {
        return distance_sq(x, y, tl.0, tl.1) <= r * r;
    }
    if x > tr.0 && y < tr.1 {
        return distance_sq(x, y, tr.0, tr.1) <= r * r;
    }
    if x < bl.0 && y > bl.1 {
        return distance_sq(x, y, bl.0, bl.1) <= r * r;
    }
    if x > br.0 && y > br.1 {
        return distance_sq(x, y, br.0, br.1) <= r * r;
    }

    true
}

fn distance_sq(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x1 - x2;
    let dy = y1 - y2;
    dx * dx + dy * dy
}

pub fn get_ime_cursor_rect_by_id(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    node_id: u64,
) -> Option<(f32, f32, f32, f32)> {
    let node = arena.get(root_key)?;
    if node.element.id() == node_id {
        return node.element.ime_cursor_rect();
    }
    let children: Vec<_> = node.children.clone();
    drop(node);
    for child_key in children {
        if let Some(rect) = get_ime_cursor_rect_by_id(arena, child_key, node_id) {
            return Some(rect);
        }
    }
    None
}

pub fn get_cursor_by_id(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    node_id: u64,
) -> Option<crate::Cursor> {
    let node = arena.get(root_key)?;
    if node.element.id() == node_id {
        return Some(node.element.cursor());
    }
    let children: Vec<_> = node.children.clone();
    drop(node);
    for child_key in children {
        if let Some(cursor) = get_cursor_by_id(arena, child_key, node_id) {
            return Some(cursor);
        }
    }
    None
}

pub(crate) fn select_all_text_by_id(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    node_id: u64,
) -> bool {
    arena
        .with_element_taken(root_key, |element, arena| {
            if element.id() == node_id {
                if let Some(text_area) = element.as_any_mut().downcast_mut::<TextArea>() {
                    text_area.select_all();
                    return true;
                }
                return false;
            }
            let children: Vec<_> = element.children().to_vec();
            for child_key in children {
                if select_all_text_by_id(arena, child_key, node_id) {
                    return true;
                }
            }
            false
        })
        .unwrap_or(false)
}

pub(crate) fn select_text_range_by_id(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    node_id: u64,
    start: usize,
    end: usize,
) -> bool {
    arena
        .with_element_taken(root_key, |element, arena| {
            if element.id() == node_id {
                if let Some(text_area) = element.as_any_mut().downcast_mut::<TextArea>() {
                    text_area.select_range(start, end);
                    return true;
                }
                return false;
            }
            let children: Vec<_> = element.children().to_vec();
            for child_key in children {
                if select_text_range_by_id(arena, child_key, node_id, start, end) {
                    return true;
                }
            }
            false
        })
        .unwrap_or(false)
}

pub fn subtree_contains_node(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    ancestor_id: u64,
    node_id: u64,
) -> bool {
    fn find_ancestor(
        arena: &crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
        target_id: u64,
    ) -> Option<crate::view::node_arena::NodeKey> {
        let node = arena.get(key)?;
        if node.element.id() == target_id {
            return Some(key);
        }
        let children: Vec<_> = node.children.clone();
        drop(node);
        for child_key in children {
            if let Some(k) = find_ancestor(arena, child_key, target_id) {
                return Some(k);
            }
        }
        None
    }

    fn contains_node_id(
        arena: &crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
        target_id: u64,
    ) -> bool {
        let Some(node) = arena.get(key) else { return false };
        if node.element.id() == target_id {
            return true;
        }
        let children: Vec<_> = node.children.clone();
        drop(node);
        for child_key in children {
            if contains_node_id(arena, child_key, target_id) {
                return true;
            }
        }
        false
    }

    match find_ancestor(arena, root_key, ancestor_id) {
        Some(k) => contains_node_id(arena, k, node_id),
        None => false,
    }
}

pub fn has_animation_frame_request(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
) -> bool {
    let Some(node) = arena.get(root_key) else { return false };
    if node.element.wants_animation_frame() {
        return true;
    }
    let children: Vec<_> = node.children.clone();
    drop(node);
    for child_key in children {
        if has_animation_frame_request(arena, child_key) {
            return true;
        }
    }
    false
}

/// Forward `EventTarget` methods to an inner field (typically `element`).
///
/// Two forms:
/// - `forward_event_target!(full element)` — forwards every method, used by
///   wrappers that want the inner `Element` to own all event state
///   (scroll, hover, transitions…). Image / Svg.
/// - `forward_event_target!(dispatch_only element)` — only forwards the
///   pointer/keyboard/focus dispatch pair + `cursor`; the remaining methods
///   fall back to trait defaults. Text.
macro_rules! forward_event_target {
    (full $field:ident) => {
        $crate::view::base_component::forward_event_target!(@dispatch $field);
        $crate::view::base_component::forward_event_target!(@state_and_requests $field);
    };
    (dispatch_only $field:ident) => {
        $crate::view::base_component::forward_event_target!(@dispatch $field);
        fn cursor(&self) -> $crate::Cursor {
            self.$field.cursor()
        }
    };
    (@dispatch $field:ident) => {
        fn dispatch_pointer_down(
            &mut self,
            event: &mut $crate::ui::PointerDownEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
        ) {
            self.$field.dispatch_pointer_down(event, control);
        }
        fn dispatch_pointer_up(
            &mut self,
            event: &mut $crate::ui::PointerUpEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
        ) {
            self.$field.dispatch_pointer_up(event, control);
        }
        fn dispatch_pointer_move(
            &mut self,
            event: &mut $crate::ui::PointerMoveEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
        ) {
            self.$field.dispatch_pointer_move(event, control);
        }
        fn dispatch_click(
            &mut self,
            event: &mut $crate::ui::ClickEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
        ) {
            self.$field.dispatch_click(event, control);
        }
        fn dispatch_key_down(
            &mut self,
            event: &mut $crate::ui::KeyDownEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
        ) {
            self.$field.dispatch_key_down(event, control);
        }
        fn dispatch_key_up(
            &mut self,
            event: &mut $crate::ui::KeyUpEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
        ) {
            self.$field.dispatch_key_up(event, control);
        }
        fn dispatch_focus(
            &mut self,
            event: &mut $crate::ui::FocusEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
        ) {
            self.$field.dispatch_focus(event, control);
        }
        fn dispatch_blur(
            &mut self,
            event: &mut $crate::ui::BlurEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
        ) {
            self.$field.dispatch_blur(event, control);
        }
    };
    (@state_and_requests $field:ident) => {
        fn dispatch_pointer_enter(&mut self, event: &mut $crate::ui::PointerEnterEvent) {
            self.$field.dispatch_pointer_enter(event);
        }
        fn dispatch_pointer_leave(&mut self, event: &mut $crate::ui::PointerLeaveEvent) {
            self.$field.dispatch_pointer_leave(event);
        }
        fn cancel_pointer_interaction(&mut self) -> bool {
            self.$field.cancel_pointer_interaction()
        }
        fn set_hovered(&mut self, hovered: bool) -> bool {
            self.$field.set_hovered(hovered)
        }
        fn scroll_by(&mut self, dx: f32, dy: f32) -> bool {
            self.$field.scroll_by(dx, dy)
        }
        fn can_scroll_by(&self, dx: f32, dy: f32) -> bool {
            self.$field.can_scroll_by(dx, dy)
        }
        fn get_scroll_offset(&self) -> (f32, f32) {
            self.$field.get_scroll_offset()
        }
        fn set_scroll_offset(&mut self, offset: (f32, f32)) {
            self.$field.set_scroll_offset(offset);
        }
        fn cursor(&self) -> $crate::Cursor {
            self.$field.cursor()
        }
        fn wants_animation_frame(&self) -> bool {
            self.$field.wants_animation_frame()
        }
        fn take_style_transition_requests(
            &mut self,
        ) -> Vec<$crate::transition::StyleTrackRequest> {
            self.$field.take_style_transition_requests()
        }
        fn take_layout_transition_requests(
            &mut self,
        ) -> Vec<$crate::transition::LayoutTrackRequest> {
            self.$field.take_layout_transition_requests()
        }
        fn take_visual_transition_requests(
            &mut self,
        ) -> Vec<$crate::transition::VisualTrackRequest> {
            self.$field.take_visual_transition_requests()
        }
    };
}

pub(crate) use forward_event_target;

#[cfg(test)]
mod tests {
    use super::{
        dispatch_click_from_hit_test, dispatch_hover_transition,
        dispatch_pointer_down_from_hit_test, hit_test,
    };
    use crate::style::{
        Angle, ClipMode, Length, ParsedValue, Position, PropertyId, Rotate, ScrollDirection, Style,
        Transform, TransformOrigin, Translate,
    };
    use crate::ui::{
        ClickEvent, EventMeta, KeyModifiers, NodeId, PointerButton, PointerButtons,
        PointerDownEvent, PointerEventData,
    };
    use crate::view::base_component::{
        Element, EventTarget, LayoutConstraints, LayoutPlacement, Layoutable,
    };
    use crate::view::test_support::{commit_child, commit_element, measure_and_place, new_test_arena};
    use crate::view::{Viewport, ViewportControl};
    use crate::{AnchorName, Color, Layout};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    fn constraints(w: f32, h: f32) -> LayoutConstraints {
        LayoutConstraints {
            max_width: w,
            max_height: h,
            viewport_width: w,
            percent_base_width: Some(w),
            percent_base_height: Some(h),
            viewport_height: h,
        }
    }

    fn placement(w: f32, h: f32) -> LayoutPlacement {
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: w,
            available_height: h,
            viewport_width: w,
            percent_base_width: Some(w),
            percent_base_height: Some(h),
            viewport_height: h,
        }
    }

    #[test]
    fn hit_test_allows_absolute_viewport_clip_outside_parent() {
        let mut root = Element::new(0.0, 0.0, 400.0, 300.0);
        root.set_background_color_value(Color::rgb(16, 16, 16));
        let parent = Element::new(0.0, 0.0, 100.0, 80.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let child_id = child.id();
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff0000")),
        );
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(130.0))
                    .top(Length::px(10.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(&mut arena, root_key, constraints(400.0, 300.0), placement(400.0, 300.0));

        assert_eq!(hit_test(&arena, root_key, 135.0, 15.0), Some(child_id));
    }

    #[test]
    fn hit_test_maps_points_through_translated_parent_transform() {
        let root = Element::new(0.0, 0.0, 400.0, 300.0);
        let mut parent = Element::new(0.0, 0.0, 100.0, 100.0);
        let mut parent_style = Style::new();
        parent_style.set_transform(Transform::new([Translate::x(Length::px(100.0))]));
        parent.apply_style(parent_style);

        let mut child = Element::new(10.0, 10.0, 20.0, 20.0);
        let child_id = child.id();
        child.set_background_color_value(Color::rgb(255, 0, 0));

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(&mut arena, root_key, constraints(400.0, 300.0), placement(400.0, 300.0));

        assert_eq!(hit_test(&arena, root_key, 115.0, 15.0), Some(child_id));
    }

    #[test]
    fn hit_test_maps_points_through_rotated_parent_transform() {
        let root = Element::new(0.0, 0.0, 400.0, 300.0);
        let mut parent = Element::new(0.0, 0.0, 100.0, 100.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        parent_style.set_transform(Transform::new([Rotate::z(Angle::deg(90.0))]));
        parent_style.set_transform_origin(TransformOrigin::center());
        parent.apply_style(parent_style);

        let mut child = Element::new(70.0, 10.0, 20.0, 20.0);
        let child_id = child.id();
        child.set_background_color_value(Color::rgb(255, 0, 0));

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(&mut arena, root_key, constraints(400.0, 300.0), placement(400.0, 300.0));

        assert_eq!(hit_test(&arena, root_key, 80.0, 80.0), Some(child_id));
    }

    #[test]
    fn hit_test_allows_absolute_viewport_clip_when_parent_not_rendered() {
        let mut root = Element::new(0.0, 0.0, 400.0, 300.0);
        root.set_anchor_name(Some(AnchorName::new("root_anchor")));
        root.set_background_color_value(Color::rgb(16, 16, 16));
        let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(500.0))
                    .top(Length::px(0.0))
                    .clip(ClipMode::Parent),
            ),
        );
        parent.apply_style(parent_style);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let child_id = child.id();
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff0000")),
        );
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(130.0))
                    .top(Length::px(10.0))
                    .anchor("root_anchor")
                    .clip(ClipMode::Viewport),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(&mut arena, root_key, constraints(400.0, 300.0), placement(400.0, 300.0));

        assert_eq!(hit_test(&arena, root_key, 135.0, 15.0), Some(child_id));
    }

    #[test]
    fn hit_test_blocks_absolute_parent_clip_outside_parent() {
        let root = Element::new(0.0, 0.0, 400.0, 300.0);
        let parent = Element::new(0.0, 0.0, 100.0, 80.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let child_id = child.id();
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(130.0))
                    .top(Length::px(10.0))
                    .clip(ClipMode::Parent),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(&mut arena, root_key, constraints(400.0, 300.0), placement(400.0, 300.0));

        assert_ne!(hit_test(&arena, root_key, 135.0, 15.0), Some(child_id));
    }

    #[test]
    fn hit_test_prefers_scrollbar_over_children() {
        let mut root = Element::new(0.0, 0.0, 120.0, 120.0);
        let root_id = root.id();
        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#101010")),
        );
        root_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        root.apply_style(root_style);
        let mut child = Element::new(0.0, 0.0, 120.0, 360.0);
        child.set_background_color_value(Color::rgb(255, 0, 0));

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let _child_key = commit_child(&mut arena, root_key, Box::new(child));

        measure_and_place(&mut arena, root_key, constraints(120.0, 120.0), placement(120.0, 120.0));
        arena.with_element_taken(root_key, |el, _a| {
            if let Some(e) = el.as_any_mut().downcast_mut::<Element>() {
                let _ = e.set_hovered(true);
            }
        });

        assert_eq!(hit_test(&arena, root_key, 115.0, 60.0), Some(root_id));
    }

    #[test]
    fn overflow_child_hit_bubbles_but_parent_is_not_targetable_outside_clip() {
        let mut root = Element::new(0.0, 0.0, 200.0, 160.0);
        let root_id = root.id();
        root.set_background_color_value(Color::rgb(16, 16, 16));
        let mut clip_parent = Element::new(0.0, 0.0, 100.0, 80.0);
        clip_parent.set_background_color_value(Color::rgb(32, 32, 32));
        let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
        let parent_clicks = Rc::new(Cell::new(0));
        let parent_clicks_binding = parent_clicks.clone();
        parent.on_click(move |_event, _control| {
            parent_clicks_binding.set(parent_clicks_binding.get() + 1);
        });
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(50.0))
                    .top(Length::px(0.0))
                    .clip(ClipMode::Parent),
            ),
        );
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let child_id = child.id();
        let child_clicks = Rc::new(Cell::new(0));
        let child_clicks_binding = child_clicks.clone();
        child.on_click(move |_event, _control| {
            child_clicks_binding.set(child_clicks_binding.get() + 1);
        });
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff0000")),
        );
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(60.0))
                    .top(Length::px(10.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let clip_parent_key = commit_child(&mut arena, root_key, Box::new(clip_parent));
        let parent_key = commit_child(&mut arena, clip_parent_key, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(&mut arena, root_key, constraints(200.0, 160.0), placement(200.0, 160.0));

        assert_eq!(hit_test(&arena, root_key, 115.0, 15.0), Some(child_id));
        assert_eq!(hit_test(&arena, root_key, 145.0, 15.0), Some(root_id));

        let mut viewport = Viewport::new();
        let mut control = ViewportControl::new(&mut viewport);
        let mut click_child = ClickEvent {
            meta: EventMeta::new(NodeId::default()),
            pointer: PointerEventData {
                viewport_x: 115.0,
                viewport_y: 15.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(PointerButton::Left),
                buttons: PointerButtons::default(),
                modifiers: KeyModifiers::default(),
                pointer_id: 0,
                pointer_type: crate::platform::input::PointerType::Mouse,
                pressure: 0.0,
                timestamp: crate::time::Instant::now(),
            },
        };
        assert!(dispatch_click_from_hit_test(
            &mut arena,
            root_key,
            &mut click_child,
            &mut control
        ));
        assert_eq!(child_clicks.get(), 1);
        assert_eq!(parent_clicks.get(), 1);

        let mut click_outside = ClickEvent {
            meta: EventMeta::new(NodeId::default()),
            pointer: PointerEventData {
                viewport_x: 145.0,
                viewport_y: 15.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(PointerButton::Left),
                buttons: PointerButtons::default(),
                modifiers: KeyModifiers::default(),
                pointer_id: 0,
                pointer_type: crate::platform::input::PointerType::Mouse,
                pressure: 0.0,
                timestamp: crate::time::Instant::now(),
            },
        };
        let _ = dispatch_click_from_hit_test(
            &mut arena,
            root_key,
            &mut click_outside,
            &mut control,
        );
        assert_eq!(child_clicks.get(), 1);
        assert_eq!(parent_clicks.get(), 1);
    }

    #[test]
    fn hover_transition_dispatches_enter_leave_on_changed_ancestors_only() {
        let order = Rc::new(RefCell::new(Vec::new()));

        let mut root = Element::new(0.0, 0.0, 120.0, 120.0);
        let root_id = root.id();
        let root_order = order.clone();
        root.on_pointer_enter(move |_event| root_order.borrow_mut().push("root-enter"));
        let root_order = order.clone();
        root.on_pointer_leave(move |_event| root_order.borrow_mut().push("root-leave"));

        let mut parent = Element::new(0.0, 0.0, 120.0, 120.0);
        let parent_id = parent.id();
        let parent_order = order.clone();
        parent.on_pointer_enter(move |_event| parent_order.borrow_mut().push("parent-enter"));
        let parent_order = order.clone();
        parent.on_pointer_leave(move |_event| parent_order.borrow_mut().push("parent-leave"));

        let mut child = Element::new(0.0, 0.0, 60.0, 60.0);
        let child_id = child.id();
        let child_order = order.clone();
        child.on_pointer_enter(move |_event| child_order.borrow_mut().push("child-enter"));
        let child_order = order.clone();
        child.on_pointer_leave(move |_event| child_order.borrow_mut().push("child-leave"));

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        let roots = [root_key];

        assert!(dispatch_hover_transition(&mut arena, &roots, None, Some(child_id)));
        assert_eq!(
            order.borrow().as_slice(),
            &["root-enter", "parent-enter", "child-enter"]
        );

        order.borrow_mut().clear();
        assert!(dispatch_hover_transition(
            &mut arena,
            &roots,
            Some(child_id),
            Some(parent_id),
        ));
        assert_eq!(order.borrow().as_slice(), &["child-leave"]);

        order.borrow_mut().clear();
        assert!(dispatch_hover_transition(
            &mut arena,
            &roots,
            Some(parent_id),
            None,
        ));
        assert_eq!(order.borrow().as_slice(), &["parent-leave", "root-leave"]);

        order.borrow_mut().clear();
        assert!(!dispatch_hover_transition(
            &mut arena,
            &roots,
            Some(root_id),
            Some(root_id),
        ));
        assert!(order.borrow().is_empty());
    }

    #[test]
    fn click_on_scrollbar_does_not_reach_click_handlers() {
        let mut root = Element::new(0.0, 0.0, 120.0, 120.0);
        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#101010")),
        );
        root_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        root.apply_style(root_style);

        let child_clicked = Rc::new(Cell::new(false));
        let mut child = Element::new(0.0, 0.0, 120.0, 360.0);
        child.set_background_color_value(Color::rgb(255, 0, 0));
        let child_clicked_flag = child_clicked.clone();
        child.on_click(move |_, _| child_clicked_flag.set(true));

        let root_clicked = Rc::new(Cell::new(false));
        let root_clicked_flag = root_clicked.clone();
        root.on_click(move |_, _| root_clicked_flag.set(true));

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let _child_key = commit_child(&mut arena, root_key, Box::new(child));

        measure_and_place(&mut arena, root_key, constraints(120.0, 120.0), placement(120.0, 120.0));
        arena.with_element_taken(root_key, |el, _a| {
            if let Some(e) = el.as_any_mut().downcast_mut::<Element>() {
                let _ = e.set_hovered(true);
            }
        });

        let mut viewport = Viewport::new();
        let mut control = ViewportControl::new(&mut viewport);
        let mut click = ClickEvent {
            meta: EventMeta::new(NodeId::default()),
            pointer: PointerEventData {
                viewport_x: 115.0,
                viewport_y: 60.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(PointerButton::Left),
                buttons: PointerButtons::default(),
                modifiers: KeyModifiers::default(),
                pointer_id: 0,
                pointer_type: crate::platform::input::PointerType::Mouse,
                pressure: 0.0,
                timestamp: crate::time::Instant::now(),
            },
        };

        let handled = dispatch_click_from_hit_test(&mut arena, root_key, &mut click, &mut control);
        assert!(handled);
        assert!(!child_clicked.get());
        assert!(!root_clicked.get());
    }

    #[test]
    fn mouse_down_on_scrollbar_requests_focus_keep() {
        let mut root = Element::new(0.0, 0.0, 120.0, 120.0);
        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#101010")),
        );
        root_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        root.apply_style(root_style);
        let mut child = Element::new(0.0, 0.0, 120.0, 360.0);
        child.set_background_color_value(Color::rgb(255, 0, 0));

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let _child_key = commit_child(&mut arena, root_key, Box::new(child));

        measure_and_place(&mut arena, root_key, constraints(120.0, 120.0), placement(120.0, 120.0));
        arena.with_element_taken(root_key, |el, _a| {
            if let Some(e) = el.as_any_mut().downcast_mut::<Element>() {
                let _ = e.set_hovered(true);
            }
        });

        let mut viewport = Viewport::new();
        let meta = EventMeta::new(NodeId::default());
        let mut control = ViewportControl::new(&mut viewport);
        let mut down = PointerDownEvent {
            meta: meta.clone(),
            pointer: PointerEventData {
                viewport_x: 115.0,
                viewport_y: 60.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(PointerButton::Left),
                buttons: PointerButtons::default(),
                modifiers: KeyModifiers::default(),
                pointer_id: 0,
                pointer_type: crate::platform::input::PointerType::Mouse,
                pressure: 0.0,
                timestamp: crate::time::Instant::now(),
            },
            viewport: meta.viewport(),
        };

        let handled = dispatch_pointer_down_from_hit_test(
            &mut arena,
            root_key,
            &mut down,
            &mut control,
        );
        assert!(handled);
        assert!(down.meta.keep_focus_requested());
    }
}
