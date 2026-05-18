use crate::style::ClipMode;
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::popup_stack::PopupStack;

use super::{BoxModelSnapshot, Element, ElementTrait};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HitTestSource {
    PopupStack,
    ViewportRoot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HitTestTarget {
    pub root_key: NodeKey,
    pub target_key: NodeKey,
    pub source: HitTestSource,
}

#[derive(Clone, Copy)]
struct HitTestQuery {
    x: f32,
    y: f32,
}

pub(crate) fn hit_test_pointer_target(
    arena: &NodeArena,
    popup_stack: &PopupStack,
    root_keys: &[NodeKey],
    viewport_x: f32,
    viewport_y: f32,
) -> Option<HitTestTarget> {
    let query = HitTestQuery {
        x: viewport_x,
        y: viewport_y,
    };
    hit_test_popup_stack(arena, popup_stack, query)
        .or_else(|| hit_test_viewport_roots(arena, root_keys, query))
}

pub fn hit_test(
    arena: &NodeArena,
    root_key: NodeKey,
    viewport_x: f32,
    viewport_y: f32,
) -> Option<NodeKey> {
    hit_test_subtree(
        arena,
        root_key,
        HitTestQuery {
            x: viewport_x,
            y: viewport_y,
        },
        ParentGate::Open,
    )
}

pub fn hit_test_stacked(
    arena: &NodeArena,
    popup_stack: &PopupStack,
    viewport_x: f32,
    viewport_y: f32,
) -> Option<(NodeKey, NodeKey)> {
    hit_test_popup_stack(
        arena,
        popup_stack,
        HitTestQuery {
            x: viewport_x,
            y: viewport_y,
        },
    )
    .map(|target| (target.root_key, target.target_key))
}

pub fn hit_test_roots(
    arena: &NodeArena,
    root_keys: &[NodeKey],
    viewport_x: f32,
    viewport_y: f32,
) -> Option<(usize, NodeKey)> {
    let query = HitTestQuery {
        x: viewport_x,
        y: viewport_y,
    };
    root_keys
        .iter()
        .enumerate()
        .rev()
        .find_map(|(root_index, &root_key)| {
            hit_test_subtree(arena, root_key, query, ParentGate::Open)
                .map(|target_key| (root_index, target_key))
        })
}

fn hit_test_popup_stack(
    arena: &NodeArena,
    popup_stack: &PopupStack,
    query: HitTestQuery,
) -> Option<HitTestTarget> {
    for stable_id in popup_stack.iter_top_down() {
        let Some(popup_key) = arena.find_by_stable_id(stable_id) else {
            continue;
        };
        let Some(target_key) = hit_test_subtree(arena, popup_key, query, ParentGate::Open) else {
            continue;
        };
        return Some(HitTestTarget {
            root_key: arena.root_for(popup_key),
            target_key,
            source: HitTestSource::PopupStack,
        });
    }
    None
}

fn hit_test_viewport_roots(
    arena: &NodeArena,
    root_keys: &[NodeKey],
    query: HitTestQuery,
) -> Option<HitTestTarget> {
    root_keys.iter().rev().find_map(|&root_key| {
        hit_test_subtree(arena, root_key, query, ParentGate::Open).map(|target_key| HitTestTarget {
            root_key,
            target_key,
            source: HitTestSource::ViewportRoot,
        })
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ParentGate {
    Open,
    OutsideParentSelf,
}

fn hit_test_subtree(
    arena: &NodeArena,
    key: NodeKey,
    query: HitTestQuery,
    parent_gate: ParentGate,
) -> Option<NodeKey> {
    let node = arena.get(key)?;
    let element = node.element.as_ref();
    if parent_gate == ParentGate::OutsideParentSelf && !element_can_escape_parent_hit_gate(element)
    {
        return None;
    }

    let (hit_x, hit_y) = hit_test_point_for_node(element, query.x, query.y);
    let child_query = HitTestQuery { x: hit_x, y: hit_y };
    let snapshot = element.box_model_snapshot();
    let has_escape_descendant = element_has_parent_hit_gate_escape_descendant(element);
    if !snapshot.should_render && !has_escape_descendant {
        return None;
    }

    let in_self =
        point_in_box_model(&snapshot, hit_x, hit_y) && element.hit_test_visible_at(hit_x, hit_y);
    if !in_self && !has_escape_descendant {
        return None;
    }

    if in_self && element.intercepts_pointer_at(hit_x, hit_y) {
        return Some(key);
    }

    let child_gate = if in_self {
        ParentGate::Open
    } else {
        ParentGate::OutsideParentSelf
    };
    let children = node.children.clone();
    drop(node);

    for child_key in children.iter().rev() {
        if let Some(target_key) = hit_test_subtree(arena, *child_key, child_query, child_gate) {
            return Some(target_key);
        }
    }

    if in_self { Some(key) } else { None }
}

fn hit_test_point_for_node(node: &dyn ElementTrait, x: f32, y: f32) -> (f32, f32) {
    node.as_any()
        .downcast_ref::<Element>()
        .and_then(|element| element.map_viewport_to_paint_space(x, y))
        .unwrap_or((x, y))
}

fn element_has_parent_hit_gate_escape_descendant(node: &dyn ElementTrait) -> bool {
    node.as_any()
        .downcast_ref::<Element>()
        .is_some_and(Element::has_absolute_descendant_for_hit_test)
}

fn element_can_escape_parent_hit_gate(node: &dyn ElementTrait) -> bool {
    let Some(element) = node.as_any().downcast_ref::<Element>() else {
        return false;
    };
    if element.has_absolute_descendant_for_hit_test() {
        return true;
    }
    element_is_parent_hit_gate_escape(element)
}

fn element_is_parent_hit_gate_escape(element: &Element) -> bool {
    if !element.is_absolute_positioned_for_hit_test() {
        return false;
    }
    match element.clip_mode_for_hit_test() {
        ClipMode::Parent => false,
        // The element's own `hit_test_visible_at` and box test still decide the
        // final hit. This gate only decides whether traversal may pass the
        // immediate parent bounds.
        ClipMode::Viewport => true,
        ClipMode::AnchorParent => true,
    }
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
