use crate::view::base_component::{BoxModelSnapshot, ElementTrait};
use crate::view::promotion::{PromotedLayerUpdate, PromotedLayerUpdateKind, PromotionCandidate};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

pub(crate) fn collect_promotion_candidates(
    roots: &[Box<dyn ElementTrait>],
    active_channels: &HashMap<u64, HashSet<crate::transition::ChannelId>>,
    viewport_size: (f32, f32),
) -> Vec<PromotionCandidate> {
    fn walk(
        node: &dyn ElementTrait,
        active_channels: &HashMap<u64, HashSet<crate::transition::ChannelId>>,
        viewport_size: (f32, f32),
        out: &mut Vec<PromotionCandidate>,
    ) -> (usize, usize) {
        let snapshot = node.box_model_snapshot();
        let info = node.promotion_node_info();
        let mut subtree_node_count = 1usize;
        let mut estimated_pass_count = info.estimated_pass_count.max(1) as usize;

        if let Some(children) = node.children() {
            for child in children {
                let (child_nodes, child_passes) =
                    walk(child.as_ref(), active_channels, viewport_size, out);
                subtree_node_count += child_nodes;
                estimated_pass_count += child_passes;
            }
        }

        let (visible_area_ratio, viewport_coverage, distance_to_viewport) =
            visibility_metrics(snapshot, viewport_size);
        out.push(PromotionCandidate {
            node_id: snapshot.node_id,
            parent_id: snapshot.parent_id,
            width: snapshot.width.max(0.0),
            height: snapshot.height.max(0.0),
            subtree_node_count,
            estimated_pass_count,
            visible_area_ratio,
            viewport_coverage,
            distance_to_viewport,
            info,
            active_channels: active_channels
                .get(&snapshot.node_id)
                .cloned()
                .unwrap_or_default(),
        });

        (subtree_node_count, estimated_pass_count)
    }

    let mut out = Vec::new();
    for root in roots {
        walk(root.as_ref(), active_channels, viewport_size, &mut out);
    }
    out
}

pub(crate) fn collect_promoted_layer_updates(
    roots: &[Box<dyn ElementTrait>],
    promoted_node_ids: &HashSet<u64>,
    previous_base_signatures: &HashMap<u64, u64>,
) -> (Vec<PromotedLayerUpdate>, HashMap<u64, u64>) {
    fn walk(
        node: &dyn ElementTrait,
        promoted_node_ids: &HashSet<u64>,
        previous_base_signatures: &HashMap<u64, u64>,
        updates: &mut Vec<PromotedLayerUpdate>,
        next_base_signatures: &mut HashMap<u64, u64>,
    ) -> u64 {
        let mut hasher = DefaultHasher::new();
        node.promotion_self_signature().hash(&mut hasher);
        if let Some(children) = node.children() {
            for (index, child) in children.iter().enumerate() {
                let child_signature = walk(
                    child.as_ref(),
                    promoted_node_ids,
                    previous_base_signatures,
                    updates,
                    next_base_signatures,
                );
                index.hash(&mut hasher);
                child.id().hash(&mut hasher);
                let child_is_promoted = promoted_node_ids.contains(&child.id());
                child_is_promoted.hash(&mut hasher);
                if !child_is_promoted {
                    child_signature.hash(&mut hasher);
                }
            }
        }
        let base_signature = hasher.finish();
        if promoted_node_ids.contains(&node.id()) {
            let previous_base_signature = previous_base_signatures.get(&node.id()).copied();
            let kind = if previous_base_signature == Some(base_signature) {
                PromotedLayerUpdateKind::Reuse
            } else {
                PromotedLayerUpdateKind::Reraster
            };
            next_base_signatures.insert(node.id(), base_signature);
            updates.push(PromotedLayerUpdate {
                node_id: node.id(),
                parent_id: node.parent_id(),
                kind,
                base_signature,
                previous_base_signature,
            });
        }
        base_signature
    }

    let mut updates = Vec::new();
    let mut next_base_signatures = HashMap::new();
    for root in roots {
        walk(
            root.as_ref(),
            promoted_node_ids,
            previous_base_signatures,
            &mut updates,
            &mut next_base_signatures,
        );
    }
    updates.sort_by_key(|update| update.node_id);
    (updates, next_base_signatures)
}

fn visibility_metrics(snapshot: BoxModelSnapshot, viewport_size: (f32, f32)) -> (f32, f32, f32) {
    let viewport = Rect {
        x: 0.0,
        y: 0.0,
        width: viewport_size.0.max(0.0),
        height: viewport_size.1.max(0.0),
    };
    let rect = Rect {
        x: snapshot.x,
        y: snapshot.y,
        width: snapshot.width.max(0.0),
        height: snapshot.height.max(0.0),
    };
    let intersection = intersect_rect(rect, viewport);
    let rect_area = (rect.width * rect.height).max(0.0);
    let viewport_area = (viewport.width * viewport.height).max(1.0);
    let intersection_area = (intersection.width * intersection.height).max(0.0);
    let visible_area_ratio = if rect_area <= f32::EPSILON {
        0.0
    } else {
        intersection_area / rect_area
    };
    let viewport_coverage = if viewport_area <= f32::EPSILON {
        0.0
    } else {
        rect_area / viewport_area
    };
    let distance_to_viewport = rect_distance(rect, viewport);
    (visible_area_ratio, viewport_coverage, distance_to_viewport)
}

#[derive(Clone, Copy)]
struct Rect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

fn intersect_rect(a: Rect, b: Rect) -> Rect {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = (a.x + a.width).min(b.x + b.width);
    let bottom = (a.y + a.height).min(b.y + b.height);
    Rect {
        x: left,
        y: top,
        width: (right - left).max(0.0),
        height: (bottom - top).max(0.0),
    }
}

fn rect_distance(a: Rect, b: Rect) -> f32 {
    let dx = if a.x + a.width < b.x {
        b.x - (a.x + a.width)
    } else if b.x + b.width < a.x {
        a.x - (b.x + b.width)
    } else {
        0.0
    };
    let dy = if a.y + a.height < b.y {
        b.y - (a.y + a.height)
    } else if b.y + b.height < a.y {
        a.y - (b.y + b.height)
    } else {
        0.0
    };
    dx.max(dy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Color;
    use crate::view::base_component::{Element, ElementTrait};

    #[test]
    fn promoted_child_change_does_not_dirty_parent_base() {
        let mut root = Element::new_with_id(1, 0.0, 0.0, 200.0, 200.0);
        let child = Element::new_with_id(2, 0.0, 0.0, 100.0, 100.0);
        root.add_child(Box::new(child));

        let roots: Vec<Box<dyn ElementTrait>> = vec![Box::new(root)];
        let promoted = HashSet::from([1_u64, 2_u64]);
        let (first_updates, first_signatures) =
            collect_promoted_layer_updates(&roots, &promoted, &HashMap::new());
        assert!(
            first_updates
                .iter()
                .all(|update| update.kind == PromotedLayerUpdateKind::Reraster)
        );

        let mut roots = roots;
        let root = roots[0]
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("root should be element");
        let child = root
            .children_mut()
            .and_then(|children| children.get_mut(0))
            .and_then(|child| child.as_any_mut().downcast_mut::<Element>())
            .expect("child should be element");
        child.set_background_color_value(Color::rgb(255, 0, 0));

        let (second_updates, _) =
            collect_promoted_layer_updates(&roots, &promoted, &first_signatures);
        let parent = second_updates
            .iter()
            .find(|update| update.node_id == 1)
            .unwrap();
        let child = second_updates
            .iter()
            .find(|update| update.node_id == 2)
            .unwrap();
        assert_eq!(parent.kind, PromotedLayerUpdateKind::Reuse);
        assert_eq!(child.kind, PromotedLayerUpdateKind::Reraster);
    }
}
