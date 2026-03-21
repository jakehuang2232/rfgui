use crate::transition::{
    LayoutField, LayoutTrackRequest, StyleField, StyleTrackRequest, StyleValue, VisualField,
    VisualTrackRequest,
};
use crate::transition::{TrackKey, TrackTarget};
use crate::ui::{
    BlurEvent, ClickEvent, FocusEvent, ImePreeditEvent, KeyDownEvent, KeyUpEvent, MouseDownEvent,
    MouseEnterEvent, MouseLeaveEvent, MouseMoveEvent, MouseUpEvent, TextInputEvent,
};
use crate::view::viewport::ViewportControl;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

mod core;
mod element;
mod image;
mod text;
mod text_area;

pub(crate) use core::*;
pub use element::*;
pub use image::*;
pub use text::*;
pub use text_area::*;

fn next_ui_node_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn collect_box_models(root: &dyn ElementTrait) -> Vec<BoxModelSnapshot> {
    fn walk(node: &dyn ElementTrait, out: &mut Vec<BoxModelSnapshot>) {
        out.push(node.box_model_snapshot());
        if let Some(children) = node.children() {
            for child in children {
                walk(child.as_ref(), out);
            }
        }
    }

    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

pub(crate) fn can_reuse_promoted_subtree(node: &dyn ElementTrait, _ctx: &UiBuildContext) -> bool {
    fn walk(node: &dyn ElementTrait) -> bool {
        let Some(children) = node.children() else {
            return true;
        };
        for child in children {
            if !walk(child.as_ref()) {
                return false;
            }
        }
        true
    }

    walk(node)
}

pub(crate) fn get_debug_element_render_state_by_id(
    root: &dyn ElementTrait,
    node_id: u64,
) -> Option<DebugElementRenderState> {
    if root.id() == node_id {
        return root
            .as_any()
            .downcast_ref::<Element>()
            .map(Element::debug_render_state);
    }
    if let Some(children) = root.children() {
        for child in children {
            if let Some(state) = get_debug_element_render_state_by_id(child.as_ref(), node_id) {
                return Some(state);
            }
        }
    }
    None
}

pub(crate) fn get_debug_promotion_signatures_by_id(
    root: &dyn ElementTrait,
    node_id: u64,
) -> Option<(u64, u64)> {
    if root.id() == node_id {
        return Some((
            root.promotion_self_signature(),
            root.promotion_clip_intersection_signature(),
        ));
    }
    if let Some(children) = root.children() {
        for child in children {
            if let Some(signatures) = get_debug_promotion_signatures_by_id(child.as_ref(), node_id)
            {
                return Some(signatures);
            }
        }
    }
    None
}

pub(crate) fn get_node_ancestry_ids(root: &dyn ElementTrait, node_id: u64) -> Option<Vec<u64>> {
    fn walk(node: &dyn ElementTrait, target_id: u64, path: &mut Vec<u64>) -> bool {
        path.push(node.id());
        if node.id() == target_id {
            return true;
        }
        if let Some(children) = node.children() {
            for child in children {
                if walk(child.as_ref(), target_id, path) {
                    return true;
                }
            }
        }
        path.pop();
        false
    }

    let mut path = Vec::new();
    if walk(root, node_id, &mut path) {
        Some(path)
    } else {
        None
    }
}

pub(crate) fn build_node_by_id(
    node: &mut dyn ElementTrait,
    node_id: u64,
    graph: &mut crate::view::frame_graph::FrameGraph,
    ctx: &mut UiBuildContext,
) -> bool {
    if node.id() == node_id {
        if ctx.is_node_promoted(node_id) {
            if let Some(element) = node.as_any_mut().downcast_mut::<Element>() {
                if let Some(reason) = element.inline_promotion_rendering_reason() {
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
            let can_reuse_subtree = can_reuse_promoted_subtree(node, ctx);
            let can_reuse = matches!(
                update_kind,
                crate::view::promotion::PromotedLayerUpdateKind::Reuse
            ) && can_reuse_subtree;
            let mut node_ctx = UiBuildContext::from_parts(
                ctx.viewport(),
                BuildState::for_layer_subtree_with_ancestor_clip(ctx.ancestor_clip_context()),
            );
            let layer_target =
                node_ctx.allocate_promoted_layer_target(graph, node_id, node.promotion_composite_bounds());
            node_ctx.set_current_target(layer_target);
            let next_state = if let Some(element) = node.as_any_mut().downcast_mut::<Element>() {
                element.build_promoted_layer(
                    graph,
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
                node.build(graph, node_ctx)
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
                UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
            );
            ctx.set_state(next_state);
        }
        return true;
    }
    if let Some(children) = node.children_mut() {
        for child in children.iter_mut() {
            if build_node_by_id(child.as_mut(), node_id, graph, ctx) {
                return true;
            }
        }
    }
    false
}

#[derive(Debug, Clone, Copy)]
pub struct LayoutTransitionSnapshotSeed {
    pub layout_x: f32,
    pub layout_y: f32,
    pub layout_width: f32,
    pub layout_height: f32,
    pub parent_layout_x: f32,
    pub parent_layout_y: f32,
}

pub fn collect_layout_transition_snapshots(
    roots: &[Box<dyn ElementTrait>],
) -> HashMap<u64, LayoutTransitionSnapshotSeed> {
    let mut out = HashMap::new();

    fn walk(
        node: &dyn ElementTrait,
        parent_layout_x: f32,
        parent_layout_y: f32,
        out: &mut HashMap<u64, LayoutTransitionSnapshotSeed>,
    ) {
        let snapshot = node.box_model_snapshot();
        out.insert(
            node.id(),
            LayoutTransitionSnapshotSeed {
                layout_x: snapshot.x,
                layout_y: snapshot.y,
                layout_width: snapshot.width,
                layout_height: snapshot.height,
                parent_layout_x,
                parent_layout_y,
            },
        );

        let (next_parent_x, next_parent_y) = node
            .as_any()
            .downcast_ref::<Element>()
            .map(Element::child_layout_origin)
            .unwrap_or((snapshot.x, snapshot.y));

        if let Some(children) = node.children() {
            for child in children {
                walk(child.as_ref(), next_parent_x, next_parent_y, out);
            }
        }
    }

    for root in roots {
        walk(root.as_ref(), 0.0, 0.0, &mut out);
    }

    out
}

pub fn seed_layout_transition_snapshots(
    roots: &mut [Box<dyn ElementTrait>],
    snapshots: &HashMap<u64, LayoutTransitionSnapshotSeed>,
) {
    fn apply(node: &mut dyn ElementTrait, snapshots: &HashMap<u64, LayoutTransitionSnapshotSeed>) {
        if let Some(seed) = snapshots.get(&node.id()) {
            if let Some(element) = node.as_any_mut().downcast_mut::<Element>() {
                element.seed_layout_transition_snapshot(
                    seed.layout_x,
                    seed.layout_y,
                    seed.layout_width,
                    seed.layout_height,
                    seed.parent_layout_x,
                    seed.parent_layout_y,
                );
            }
        }
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut() {
                apply(child.as_mut(), snapshots);
            }
        }
    }

    for root in roots.iter_mut() {
        apply(root.as_mut(), snapshots);
    }
}

pub fn hit_test(root: &dyn ElementTrait, viewport_x: f32, viewport_y: f32) -> Option<u64> {
    #[derive(Clone, Copy)]
    struct Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    }

    fn point_in_rect(rect: Rect, x: f32, y: f32) -> bool {
        rect.width > 0.0
            && rect.height > 0.0
            && x >= rect.x
            && y >= rect.y
            && x <= rect.x + rect.width
            && y <= rect.y + rect.height
    }

    fn child_allows_outside_parent_hit(
        child: &dyn ElementTrait,
        x: f32,
        y: f32,
        viewport_rect: Rect,
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
            crate::style::ClipMode::Viewport => point_in_rect(viewport_rect, x, y),
            crate::style::ClipMode::AnchorParent => element.has_anchor_name_for_hit_test(),
        }
    }

    fn find_viewport_clip_priority(node: &dyn ElementTrait, x: f32, y: f32) -> Option<u64> {
        fn has_absolute_descendant(node: &dyn ElementTrait) -> bool {
            node.as_any()
                .downcast_ref::<Element>()
                .is_some_and(Element::has_absolute_descendant_for_hit_test)
        }

        fn find_deepest_in_subtree(node: &dyn ElementTrait, x: f32, y: f32) -> Option<u64> {
            let snapshot = node.box_model_snapshot();
            if !snapshot.should_render && !has_absolute_descendant(node) {
                return None;
            }
            if !point_in_box_model(&snapshot, x, y) {
                return None;
            }
            if !node.hit_test_visible_at(x, y) {
                return None;
            }
            if node.intercepts_pointer_at(x, y) {
                return Some(snapshot.node_id);
            }
            if let Some(children) = node.children() {
                for child in children.iter().rev() {
                    if let Some(id) = find_deepest_in_subtree(child.as_ref(), x, y) {
                        return Some(id);
                    }
                }
            }
            Some(snapshot.node_id)
        }

        let snapshot = node.box_model_snapshot();
        if !snapshot.should_render && !has_absolute_descendant(node) {
            return None;
        }

        if let Some(children) = node.children() {
            for child in children.iter().rev() {
                if let Some(id) = find_viewport_clip_priority(child.as_ref(), x, y) {
                    return Some(id);
                }
            }
        }

        let Some(element) = node.as_any().downcast_ref::<Element>() else {
            return None;
        };
        if !element.is_absolute_positioned_for_hit_test()
            || element.clip_mode_for_hit_test() != crate::style::ClipMode::Viewport
        {
            return None;
        }

        if point_in_box_model(&snapshot, x, y) {
            find_deepest_in_subtree(node, x, y)
        } else {
            None
        }
    }

    fn find(node: &dyn ElementTrait, x: f32, y: f32, viewport_rect: Rect) -> Option<u64> {
        let snapshot = node.box_model_snapshot();
        let has_absolute_descendant = node
            .as_any()
            .downcast_ref::<Element>()
            .is_some_and(Element::has_absolute_descendant_for_hit_test);
        if !snapshot.should_render && !has_absolute_descendant {
            return None;
        }
        let in_self = point_in_box_model(&snapshot, x, y) && node.hit_test_visible_at(x, y);
        if in_self && node.intercepts_pointer_at(x, y) {
            return Some(snapshot.node_id);
        }
        if !in_self && !has_absolute_descendant {
            return None;
        }

        if let Some(children) = node.children() {
            for child in children.iter().rev() {
                if !in_self && !child_allows_outside_parent_hit(child.as_ref(), x, y, viewport_rect)
                {
                    continue;
                }
                if let Some(id) = find(child.as_ref(), x, y, viewport_rect) {
                    return Some(id);
                }
            }
        }

        if in_self {
            Some(snapshot.node_id)
        } else {
            None
        }
    }

    let root_snapshot = root.box_model_snapshot();
    let viewport_rect = Rect {
        x: root_snapshot.x,
        y: root_snapshot.y,
        width: root_snapshot.width.max(0.0),
        height: root_snapshot.height.max(0.0),
    };
    if let Some(id) = find_viewport_clip_priority(root, viewport_x, viewport_y) {
        return Some(id);
    }
    find(root, viewport_x, viewport_y, viewport_rect)
}

pub fn dispatch_mouse_down_from_hit_test(
    root: &mut dyn ElementTrait,
    event: &mut MouseDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_id) = hit_test(root, event.mouse.viewport_x, event.mouse.viewport_y) else {
        return false;
    };
    event.meta.set_target_id(target_id);
    dispatch_mouse_down_bubble(root, target_id, event, control)
}

pub fn dispatch_mouse_up_from_hit_test(
    root: &mut dyn ElementTrait,
    event: &mut MouseUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_id) = hit_test(root, event.mouse.viewport_x, event.mouse.viewport_y) else {
        return false;
    };
    event.meta.set_target_id(target_id);
    dispatch_mouse_up_bubble(root, target_id, event, control)
}

pub fn dispatch_mouse_up_to_target(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut MouseUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    event.meta.set_target_id(target_id);
    dispatch_mouse_up_bubble(root, target_id, event, control)
}

pub fn dispatch_mouse_move_from_hit_test(
    root: &mut dyn ElementTrait,
    event: &mut MouseMoveEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_id) = hit_test(root, event.mouse.viewport_x, event.mouse.viewport_y) else {
        return false;
    };
    event.meta.set_target_id(target_id);
    dispatch_mouse_move_bubble(root, target_id, event, control)
}

pub fn dispatch_mouse_move_to_target(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut MouseMoveEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    event.meta.set_target_id(target_id);
    dispatch_mouse_move_bubble(root, target_id, event, control)
}

pub fn dispatch_click_from_hit_test(
    root: &mut dyn ElementTrait,
    event: &mut ClickEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_id) = hit_test(root, event.mouse.viewport_x, event.mouse.viewport_y) else {
        return false;
    };
    event.meta.set_target_id(target_id);
    dispatch_click_bubble(root, target_id, event, control)
}

pub fn dispatch_click_to_target(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut ClickEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    event.meta.set_target_id(target_id);
    dispatch_click_bubble(root, target_id, event, control)
}

pub fn dispatch_scroll_from_hit_test(
    root: &mut dyn ElementTrait,
    viewport_x: f32,
    viewport_y: f32,
    delta_x: f32,
    delta_y: f32,
) -> bool {
    let Some(target_id) = hit_test(root, viewport_x, viewport_y) else {
        return false;
    };
    dispatch_scroll_bubble(root, target_id, delta_x, delta_y)
}

pub fn find_scroll_handler_from_hit_test(
    root: &dyn ElementTrait,
    viewport_x: f32,
    viewport_y: f32,
    delta_x: f32,
    delta_y: f32,
) -> Option<u64> {
    let target_id = hit_test(root, viewport_x, viewport_y)?;
    find_scroll_handler_bubble(root, target_id, delta_x, delta_y)
}

pub fn dispatch_scroll_to_target(
    root: &mut dyn ElementTrait,
    target_id: u64,
    delta_x: f32,
    delta_y: f32,
) -> bool {
    dispatch_scroll_bubble(root, target_id, delta_x, delta_y)
}

pub fn get_scroll_offset_by_id(root: &dyn ElementTrait, node_id: u64) -> Option<(f32, f32)> {
    if root.id() == node_id {
        return Some(root.get_scroll_offset());
    }
    if let Some(children) = root.children() {
        for child in children {
            if let Some(offset) = get_scroll_offset_by_id(child.as_ref(), node_id) {
                return Some(offset);
            }
        }
    }
    None
}

pub fn set_scroll_offset_by_id(
    root: &mut dyn ElementTrait,
    node_id: u64,
    offset: (f32, f32),
) -> bool {
    if root.id() == node_id {
        root.set_scroll_offset(offset);
        return true;
    }
    if let Some(children) = root.children_mut() {
        for child in children.iter_mut() {
            if set_scroll_offset_by_id(child.as_mut(), node_id, offset) {
                return true;
            }
        }
    }
    false
}

pub fn take_style_transition_requests(
    root: &mut dyn ElementTrait,
    out: &mut Vec<StyleTrackRequest>,
) {
    if let Some(children) = root.children_mut() {
        for child in children.iter_mut().rev() {
            take_style_transition_requests(child.as_mut(), out);
        }
    }
    out.extend(root.take_style_transition_requests());
}

pub fn take_layout_transition_requests(
    root: &mut dyn ElementTrait,
    out: &mut Vec<LayoutTrackRequest>,
) {
    if let Some(children) = root.children_mut() {
        for child in children.iter_mut().rev() {
            take_layout_transition_requests(child.as_mut(), out);
        }
    }
    out.extend(root.take_layout_transition_requests());
}

pub fn take_visual_transition_requests(
    root: &mut dyn ElementTrait,
    out: &mut Vec<VisualTrackRequest>,
) {
    if let Some(children) = root.children_mut() {
        for child in children.iter_mut().rev() {
            take_visual_transition_requests(child.as_mut(), out);
        }
    }
    out.extend(root.take_visual_transition_requests());
}

pub fn collect_transition_track_allowlist(
    roots: &[Box<dyn ElementTrait>],
) -> HashSet<TrackKey<TrackTarget>> {
    let mut out = HashSet::new();

    fn walk(node: &dyn ElementTrait, out: &mut HashSet<TrackKey<TrackTarget>>) {
        if let Some(element) = node.as_any().downcast_ref::<Element>() {
            for channel in element.active_transition_channels() {
                out.insert(TrackKey {
                    target: node.id(),
                    channel,
                });
            }
        }
        if let Some(children) = node.children() {
            for child in children {
                walk(child.as_ref(), out);
            }
        }
    }

    for root in roots {
        walk(root.as_ref(), &mut out);
    }

    out
}

pub fn set_style_field_by_id(
    root: &mut dyn ElementTrait,
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
                        element.set_border_radius(value);
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
            }
            return true;
        }
        return false;
    }
    if let Some(children) = root.children_mut() {
        for child in children.iter_mut() {
            if set_style_field_by_id(child.as_mut(), node_id, field, value) {
                return true;
            }
        }
    }
    false
}

pub fn set_layout_field_by_id(
    root: &mut dyn ElementTrait,
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
    if let Some(children) = root.children_mut() {
        for child in children.iter_mut() {
            if set_layout_field_by_id(child.as_mut(), node_id, field, value) {
                if let Some(element) = root.as_any_mut().downcast_mut::<Element>() {
                    element.mark_layout_dirty();
                }
                return true;
            }
        }
    }
    false
}

pub fn set_visual_field_by_id(
    root: &mut dyn ElementTrait,
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
    if let Some(children) = root.children_mut() {
        for child in children.iter_mut() {
            if set_visual_field_by_id(child.as_mut(), node_id, field, value) {
                return true;
            }
        }
    }
    false
}

pub fn update_hover_state(root: &mut dyn ElementTrait, target_id: Option<u64>) -> bool {
    fn walk(node: &mut dyn ElementTrait, target_id: Option<u64>) -> (bool, bool) {
        let self_id = node.id();
        let mut contains_target = target_id == Some(self_id);
        let mut changed = false;

        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                let (child_contains_target, child_changed) = walk(child.as_mut(), target_id);
                contains_target |= child_contains_target;
                changed |= child_changed;
            }
        }

        changed |= node.set_hovered(contains_target);
        (contains_target, changed)
    }

    walk(root, target_id).1
}

fn append_path_to_target(node: &dyn ElementTrait, target_id: u64, path: &mut Vec<u64>) -> bool {
    path.push(node.id());
    if node.id() == target_id {
        return true;
    }
    if let Some(children) = node.children() {
        for child in children {
            if append_path_to_target(child.as_ref(), target_id, path) {
                return true;
            }
        }
    }
    let _ = path.pop();
    false
}

pub fn hover_path_for_target(roots: &[Box<dyn ElementTrait>], target_id: Option<u64>) -> Vec<u64> {
    let Some(target_id) = target_id else {
        return Vec::new();
    };

    for root in roots {
        let mut path = Vec::new();
        if append_path_to_target(root.as_ref(), target_id, &mut path) {
            return path;
        }
    }

    Vec::new()
}

fn dispatch_mouse_enter_to_target(node: &mut dyn ElementTrait, target_id: u64) -> bool {
    if node.id() == target_id {
        let mut event = MouseEnterEvent {
            meta: crate::ui::EventMeta::new(target_id),
        };
        node.dispatch_mouse_enter(&mut event);
        return true;
    }
    if let Some(children) = node.children_mut() {
        for child in children.iter_mut() {
            if dispatch_mouse_enter_to_target(child.as_mut(), target_id) {
                return true;
            }
        }
    }
    false
}

fn dispatch_mouse_leave_to_target(node: &mut dyn ElementTrait, target_id: u64) -> bool {
    if node.id() == target_id {
        let mut event = MouseLeaveEvent {
            meta: crate::ui::EventMeta::new(target_id),
        };
        node.dispatch_mouse_leave(&mut event);
        return true;
    }
    if let Some(children) = node.children_mut() {
        for child in children.iter_mut() {
            if dispatch_mouse_leave_to_target(child.as_mut(), target_id) {
                return true;
            }
        }
    }
    false
}

pub fn dispatch_hover_transition(
    roots: &mut [Box<dyn ElementTrait>],
    previous_target: Option<u64>,
    next_target: Option<u64>,
) -> bool {
    if previous_target == next_target {
        return false;
    }

    let previous_path = hover_path_for_target(roots, previous_target);
    let next_path = hover_path_for_target(roots, next_target);

    let mut common_prefix_len = 0;
    while common_prefix_len < previous_path.len()
        && common_prefix_len < next_path.len()
        && previous_path[common_prefix_len] == next_path[common_prefix_len]
    {
        common_prefix_len += 1;
    }

    let mut dispatched = false;

    for &node_id in previous_path[common_prefix_len..].iter().rev() {
        for root in roots.iter_mut() {
            if dispatch_mouse_leave_to_target(root.as_mut(), node_id) {
                dispatched = true;
                break;
            }
        }
    }

    for &node_id in &next_path[common_prefix_len..] {
        for root in roots.iter_mut() {
            if dispatch_mouse_enter_to_target(root.as_mut(), node_id) {
                dispatched = true;
                break;
            }
        }
    }

    dispatched
}

pub fn cancel_pointer_interactions(root: &mut dyn ElementTrait) -> bool {
    fn walk(node: &mut dyn ElementTrait) -> bool {
        let mut changed = node.cancel_pointer_interaction();
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                changed |= walk(child.as_mut());
            }
        }
        changed
    }

    walk(root)
}

pub fn dispatch_key_down_bubble(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut KeyDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_key_down_impl(root, target_id, event, control)
}

pub fn dispatch_key_up_bubble(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut KeyUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_key_up_impl(root, target_id, event, control)
}

pub fn dispatch_text_input_bubble(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut TextInputEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_text_input_impl(root, target_id, event, control)
}

pub fn dispatch_ime_preedit_bubble(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut ImePreeditEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_ime_preedit_impl(root, target_id, event, control)
}

pub fn dispatch_focus_bubble(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut FocusEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_focus_impl(root, target_id, event, control)
}

pub fn dispatch_blur_bubble(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut BlurEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_blur_impl(root, target_id, event, control)
}

fn dispatch_mouse_down_bubble(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut MouseDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let snapshot = node.box_model_snapshot();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_mouse_down_bubble(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    event.mouse.local_x = event.mouse.viewport_x - snapshot.x;
    event.mouse.local_y = event.mouse.viewport_y - snapshot.y;
    node.dispatch_mouse_down(event, control);
    true
}

fn dispatch_mouse_up_bubble(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut MouseUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let snapshot = node.box_model_snapshot();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_mouse_up_bubble(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    event.mouse.local_x = event.mouse.viewport_x - snapshot.x;
    event.mouse.local_y = event.mouse.viewport_y - snapshot.y;
    node.dispatch_mouse_up(event, control);
    true
}

fn dispatch_mouse_move_bubble(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut MouseMoveEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let snapshot = node.box_model_snapshot();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_mouse_move_bubble(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    event.mouse.local_x = event.mouse.viewport_x - snapshot.x;
    event.mouse.local_y = event.mouse.viewport_y - snapshot.y;
    node.dispatch_mouse_move(event, control);
    true
}

fn dispatch_click_bubble(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut ClickEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let snapshot = node.box_model_snapshot();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_click_bubble(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    event.mouse.local_x = event.mouse.viewport_x - snapshot.x;
    event.mouse.local_y = event.mouse.viewport_y - snapshot.y;
    node.dispatch_click(event, control);
    true
}

fn dispatch_scroll_bubble(node: &mut dyn ElementTrait, target_id: u64, dx: f32, dy: f32) -> bool {
    fn walk(node: &mut dyn ElementTrait, target_id: u64, dx: f32, dy: f32) -> (bool, bool) {
        let node_id = node.id();
        if node_id == target_id {
            let handled = node.scroll_by(dx, dy);
            return (true, handled);
        }

        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                let (found, handled) = walk(child.as_mut(), target_id, dx, dy);
                if !found {
                    continue;
                }
                if handled {
                    return (true, true);
                }
                let self_handled = node.scroll_by(dx, dy);
                return (true, self_handled);
            }
        }

        (false, false)
    }

    walk(node, target_id, dx, dy).1
}

fn find_scroll_handler_bubble(
    node: &dyn ElementTrait,
    target_id: u64,
    dx: f32,
    dy: f32,
) -> Option<u64> {
    fn walk(node: &dyn ElementTrait, target_id: u64, dx: f32, dy: f32) -> (bool, Option<u64>) {
        let node_id = node.id();
        if node_id == target_id {
            if node.can_scroll_by(dx, dy) {
                return (true, Some(node_id));
            }
            return (true, None);
        }

        if let Some(children) = node.children() {
            for child in children.iter().rev() {
                let (found, handled) = walk(child.as_ref(), target_id, dx, dy);
                if !found {
                    continue;
                }
                if handled.is_some() {
                    return (true, handled);
                }
                if node.can_scroll_by(dx, dy) {
                    return (true, Some(node_id));
                }
                return (true, None);
            }
        }

        (false, None)
    }

    walk(node, target_id, dx, dy).1
}

fn dispatch_key_down_impl(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut KeyDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_key_down_impl(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    node.dispatch_key_down(event, control);
    true
}

fn dispatch_key_up_impl(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut KeyUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_key_up_impl(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    node.dispatch_key_up(event, control);
    true
}

fn dispatch_focus_impl(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut FocusEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_focus_impl(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    node.dispatch_focus(event, control);
    true
}

fn dispatch_text_input_impl(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut TextInputEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_text_input_impl(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    node.dispatch_text_input(event, control);
    true
}

fn dispatch_ime_preedit_impl(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut ImePreeditEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_ime_preedit_impl(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    node.dispatch_ime_preedit(event, control);
    true
}

fn dispatch_blur_impl(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut BlurEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_blur_impl(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    node.dispatch_blur(event, control);
    true
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
    root: &dyn ElementTrait,
    node_id: u64,
) -> Option<(f32, f32, f32, f32)> {
    if root.id() == node_id {
        return root.ime_cursor_rect();
    }
    if let Some(children) = root.children() {
        for child in children {
            if let Some(rect) = get_ime_cursor_rect_by_id(child.as_ref(), node_id) {
                return Some(rect);
            }
        }
    }
    None
}

pub fn get_cursor_by_id(root: &dyn ElementTrait, node_id: u64) -> Option<crate::Cursor> {
    if root.id() == node_id {
        return Some(root.cursor());
    }
    if let Some(children) = root.children() {
        for child in children {
            if let Some(cursor) = get_cursor_by_id(child.as_ref(), node_id) {
                return Some(cursor);
            }
        }
    }
    None
}

pub fn subtree_contains_node(root: &dyn ElementTrait, ancestor_id: u64, node_id: u64) -> bool {
    fn find_node<'a>(node: &'a dyn ElementTrait, target_id: u64) -> Option<&'a dyn ElementTrait> {
        if node.id() == target_id {
            return Some(node);
        }
        if let Some(children) = node.children() {
            for child in children {
                if let Some(found) = find_node(child.as_ref(), target_id) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn contains_node_id(node: &dyn ElementTrait, target_id: u64) -> bool {
        if node.id() == target_id {
            return true;
        }
        if let Some(children) = node.children() {
            for child in children {
                if contains_node_id(child.as_ref(), target_id) {
                    return true;
                }
            }
        }
        false
    }

    find_node(root, ancestor_id).is_some_and(|ancestor| contains_node_id(ancestor, node_id))
}

pub fn has_animation_frame_request(root: &dyn ElementTrait) -> bool {
    if root.wants_animation_frame() {
        return true;
    }
    if let Some(children) = root.children() {
        for child in children {
            if has_animation_frame_request(child.as_ref()) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{
        dispatch_click_from_hit_test, dispatch_hover_transition, dispatch_mouse_down_from_hit_test,
        hit_test,
    };
    use crate::AnchorName;
    use crate::style::{
        ClipMode, Length, ParsedValue, Position, PropertyId, ScrollDirection, Style,
    };
    use crate::ui::{
        ClickEvent, EventMeta, KeyModifiers, MouseButton, MouseButtons, MouseDownEvent,
        MouseEventData,
    };
    use crate::view::base_component::{
        Element, EventTarget, LayoutConstraints, LayoutPlacement, Layoutable,
    };
    use crate::view::{Viewport, ViewportControl};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    #[test]
    fn hit_test_allows_absolute_viewport_clip_outside_parent() {
        let mut root = Element::new(0.0, 0.0, 400.0, 300.0);
        let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let child_id = child.id();
        let mut child_style = Style::new();
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
        parent.add_child(Box::new(child));
        root.add_child(Box::new(parent));

        root.measure(LayoutConstraints {
            max_width: 400.0,
            max_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        root.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });

        assert_eq!(hit_test(&root, 135.0, 15.0), Some(child_id));
    }

    #[test]
    fn hit_test_allows_absolute_viewport_clip_when_parent_not_rendered() {
        let mut root = Element::new(0.0, 0.0, 400.0, 300.0);
        root.set_anchor_name(Some(AnchorName::new("root_anchor")));
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
        parent.add_child(Box::new(child));
        root.add_child(Box::new(parent));

        root.measure(LayoutConstraints {
            max_width: 400.0,
            max_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        root.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        assert_eq!(hit_test(&root, 135.0, 15.0), Some(child_id));
    }

    #[test]
    fn hit_test_blocks_absolute_parent_clip_outside_parent() {
        let mut root = Element::new(0.0, 0.0, 400.0, 300.0);
        let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
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
        parent.add_child(Box::new(child));
        root.add_child(Box::new(parent));

        root.measure(LayoutConstraints {
            max_width: 400.0,
            max_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        root.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });

        assert_ne!(hit_test(&root, 135.0, 15.0), Some(child_id));
    }

    #[test]
    fn hit_test_prefers_scrollbar_over_children() {
        let mut root = Element::new(0.0, 0.0, 120.0, 120.0);
        let root_id = root.id();
        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        root.apply_style(root_style);
        root.add_child(Box::new(Element::new(0.0, 0.0, 120.0, 360.0)));

        root.measure(LayoutConstraints {
            max_width: 120.0,
            max_height: 120.0,
            viewport_width: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
            viewport_height: 120.0,
        });
        root.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 120.0,
            available_height: 120.0,
            viewport_width: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
            viewport_height: 120.0,
        });
        let _ = root.set_hovered(true);

        assert_eq!(hit_test(&root, 115.0, 60.0), Some(root_id));
    }

    #[test]
    fn overflow_child_hit_bubbles_but_parent_is_not_targetable_outside_clip() {
        let mut root = Element::new(0.0, 0.0, 200.0, 160.0);
        let mut clip_parent = Element::new(0.0, 0.0, 100.0, 80.0);
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
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(60.0))
                    .top(Length::px(10.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        child.apply_style(child_style);

        parent.add_child(Box::new(child));
        clip_parent.add_child(Box::new(parent));
        root.add_child(Box::new(clip_parent));

        root.measure(LayoutConstraints {
            max_width: 200.0,
            max_height: 160.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(160.0),
            viewport_height: 160.0,
        });
        root.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 160.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(160.0),
            viewport_height: 160.0,
        });

        assert_eq!(hit_test(&root, 115.0, 15.0), Some(child_id));
        assert_eq!(hit_test(&root, 145.0, 15.0), Some(root.id()));

        let mut viewport = Viewport::new();
        let mut control = ViewportControl::new(&mut viewport);
        let mut click_child = ClickEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: 115.0,
                viewport_y: 15.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(MouseButton::Left),
                buttons: MouseButtons::default(),
                modifiers: KeyModifiers::default(),
            },
        };
        assert!(dispatch_click_from_hit_test(
            &mut root,
            &mut click_child,
            &mut control
        ));
        assert_eq!(child_clicks.get(), 1);
        assert_eq!(parent_clicks.get(), 1);

        let mut click_outside = ClickEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: 145.0,
                viewport_y: 15.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(MouseButton::Left),
                buttons: MouseButtons::default(),
                modifiers: KeyModifiers::default(),
            },
        };
        let _ = dispatch_click_from_hit_test(&mut root, &mut click_outside, &mut control);
        assert_eq!(child_clicks.get(), 1);
        assert_eq!(parent_clicks.get(), 1);
    }

    #[test]
    fn hover_transition_dispatches_enter_leave_on_changed_ancestors_only() {
        let order = Rc::new(RefCell::new(Vec::new()));

        let mut root = Element::new(0.0, 0.0, 120.0, 120.0);
        let root_id = root.id();
        let root_order = order.clone();
        root.on_mouse_enter(move |_event| root_order.borrow_mut().push("root-enter"));
        let root_order = order.clone();
        root.on_mouse_leave(move |_event| root_order.borrow_mut().push("root-leave"));

        let mut parent = Element::new(0.0, 0.0, 120.0, 120.0);
        let parent_id = parent.id();
        let parent_order = order.clone();
        parent.on_mouse_enter(move |_event| parent_order.borrow_mut().push("parent-enter"));
        let parent_order = order.clone();
        parent.on_mouse_leave(move |_event| parent_order.borrow_mut().push("parent-leave"));

        let mut child = Element::new(0.0, 0.0, 60.0, 60.0);
        let child_id = child.id();
        let child_order = order.clone();
        child.on_mouse_enter(move |_event| child_order.borrow_mut().push("child-enter"));
        let child_order = order.clone();
        child.on_mouse_leave(move |_event| child_order.borrow_mut().push("child-leave"));

        parent.add_child(Box::new(child));
        root.add_child(Box::new(parent));

        let mut roots: Vec<Box<dyn crate::view::base_component::ElementTrait>> =
            vec![Box::new(root)];

        assert!(dispatch_hover_transition(&mut roots, None, Some(child_id)));
        assert_eq!(
            order.borrow().as_slice(),
            &["root-enter", "parent-enter", "child-enter"]
        );

        order.borrow_mut().clear();
        assert!(dispatch_hover_transition(
            &mut roots,
            Some(child_id),
            Some(parent_id),
        ));
        assert_eq!(order.borrow().as_slice(), &["child-leave"]);

        order.borrow_mut().clear();
        assert!(dispatch_hover_transition(&mut roots, Some(parent_id), None));
        assert_eq!(order.borrow().as_slice(), &["parent-leave", "root-leave"]);

        order.borrow_mut().clear();
        assert!(!dispatch_hover_transition(
            &mut roots,
            Some(root_id),
            Some(root_id)
        ));
        assert!(order.borrow().is_empty());
    }

    #[test]
    fn click_on_scrollbar_does_not_reach_click_handlers() {
        let mut root = Element::new(0.0, 0.0, 120.0, 120.0);
        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        root.apply_style(root_style);

        let child_clicked = Rc::new(Cell::new(false));
        let mut child = Element::new(0.0, 0.0, 120.0, 360.0);
        let child_clicked_flag = child_clicked.clone();
        child.on_click(move |_, _| child_clicked_flag.set(true));
        root.add_child(Box::new(child));

        let root_clicked = Rc::new(Cell::new(false));
        let root_clicked_flag = root_clicked.clone();
        root.on_click(move |_, _| root_clicked_flag.set(true));

        root.measure(LayoutConstraints {
            max_width: 120.0,
            max_height: 120.0,
            viewport_width: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
            viewport_height: 120.0,
        });
        root.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 120.0,
            available_height: 120.0,
            viewport_width: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
            viewport_height: 120.0,
        });
        let _ = root.set_hovered(true);

        let mut viewport = Viewport::new();
        let mut control = ViewportControl::new(&mut viewport);
        let mut click = ClickEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: 115.0,
                viewport_y: 60.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(MouseButton::Left),
                buttons: MouseButtons::default(),
                modifiers: KeyModifiers::default(),
            },
        };

        let handled = dispatch_click_from_hit_test(&mut root, &mut click, &mut control);
        assert!(handled);
        assert!(!child_clicked.get());
        assert!(!root_clicked.get());
    }

    #[test]
    fn mouse_down_on_scrollbar_requests_focus_keep() {
        let mut root = Element::new(0.0, 0.0, 120.0, 120.0);
        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        root.apply_style(root_style);
        root.add_child(Box::new(Element::new(0.0, 0.0, 120.0, 360.0)));

        root.measure(LayoutConstraints {
            max_width: 120.0,
            max_height: 120.0,
            viewport_width: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
            viewport_height: 120.0,
        });
        root.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 120.0,
            available_height: 120.0,
            viewport_width: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
            viewport_height: 120.0,
        });
        let _ = root.set_hovered(true);

        let mut viewport = Viewport::new();
        let meta = EventMeta::new(0);
        let mut control = ViewportControl::new(&mut viewport);
        let mut down = MouseDownEvent {
            meta: meta.clone(),
            mouse: MouseEventData {
                viewport_x: 115.0,
                viewport_y: 60.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(MouseButton::Left),
                buttons: MouseButtons::default(),
                modifiers: KeyModifiers::default(),
            },
            viewport: meta.viewport(),
        };

        let handled = dispatch_mouse_down_from_hit_test(&mut root, &mut down, &mut control);
        assert!(handled);
        assert!(down.meta.keep_focus_requested());
    }
}
