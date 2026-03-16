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
    previous_composition_signatures: &HashMap<u64, u64>,
) -> (
    Vec<PromotedLayerUpdate>,
    HashMap<u64, u64>,
    HashMap<u64, u64>,
) {
    struct WalkState {
        base_signature: u64,
        _composition_signature: u64,
        output_signature: u64,
        has_promoted_output: bool,
    }

    fn hash_composition_state(node: &dyn ElementTrait, hasher: &mut DefaultHasher) {
        node.promotion_clip_intersection_signature().hash(hasher);
        let bounds = node.promotion_composite_bounds();
        bounds.x.to_bits().hash(hasher);
        bounds.y.to_bits().hash(hasher);
        bounds.width.max(0.0).to_bits().hash(hasher);
        bounds.height.max(0.0).to_bits().hash(hasher);
        for radius in bounds.corner_radii {
            radius.to_bits().hash(hasher);
        }
        node.promotion_node_info()
            .opacity
            .clamp(0.0, 1.0)
            .to_bits()
            .hash(hasher);
    }

    fn walk(
        node: &dyn ElementTrait,
        promoted_node_ids: &HashSet<u64>,
        previous_base_signatures: &HashMap<u64, u64>,
        previous_composition_signatures: &HashMap<u64, u64>,
        updates: &mut Vec<PromotedLayerUpdate>,
        next_base_signatures: &mut HashMap<u64, u64>,
        next_composition_signatures: &mut HashMap<u64, u64>,
    ) -> WalkState {
        let mut hasher = DefaultHasher::new();
        let mut composition_hasher = DefaultHasher::new();
        node.promotion_self_signature().hash(&mut hasher);
        node.promotion_clip_intersection_signature()
            .hash(&mut hasher);
        let self_is_promoted = promoted_node_ids.contains(&node.id());
        let mut has_promoted_output = self_is_promoted;
        if self_is_promoted {
            hash_composition_state(node, &mut composition_hasher);
        }
        if let Some(children) = node.children() {
            for (index, child) in children.iter().enumerate() {
                let child_state = walk(
                    child.as_ref(),
                    promoted_node_ids,
                    previous_base_signatures,
                    previous_composition_signatures,
                    updates,
                    next_base_signatures,
                    next_composition_signatures,
                );
                index.hash(&mut hasher);
                child.id().hash(&mut hasher);
                let child_is_promoted = promoted_node_ids.contains(&child.id());
                child_is_promoted.hash(&mut hasher);
                if !child_is_promoted {
                    child_state.base_signature.hash(&mut hasher);
                }

                let child_is_deferred = child
                    .as_any()
                    .downcast_ref::<crate::view::base_component::Element>()
                    .is_some_and(
                        crate::view::base_component::Element::should_append_to_root_viewport_render,
                    );
                if child_is_deferred {
                    continue;
                }
                if child_is_promoted || child_state.has_promoted_output {
                    if !has_promoted_output {
                        hash_composition_state(node, &mut composition_hasher);
                        has_promoted_output = true;
                    }
                    index.hash(&mut composition_hasher);
                    child.id().hash(&mut composition_hasher);
                    child_is_promoted.hash(&mut composition_hasher);
                    child_state.output_signature.hash(&mut composition_hasher);
                }
            }
        }
        let base_signature = hasher.finish();
        let composition_signature = if has_promoted_output {
            composition_hasher.finish()
        } else {
            0
        };
        let output_signature = if has_promoted_output {
            let mut output_hasher = DefaultHasher::new();
            base_signature.hash(&mut output_hasher);
            composition_signature.hash(&mut output_hasher);
            output_hasher.finish()
        } else {
            base_signature
        };
        if promoted_node_ids.contains(&node.id()) {
            let previous_base_signature = previous_base_signatures.get(&node.id()).copied();
            let kind = if previous_base_signature == Some(base_signature) {
                PromotedLayerUpdateKind::Reuse
            } else {
                PromotedLayerUpdateKind::Reraster
            };
            let previous_composition_signature =
                previous_composition_signatures.get(&node.id()).copied();
            let composition_kind = if previous_composition_signature == Some(composition_signature)
            {
                PromotedLayerUpdateKind::Reuse
            } else {
                PromotedLayerUpdateKind::Reraster
            };
            next_base_signatures.insert(node.id(), base_signature);
            next_composition_signatures.insert(node.id(), composition_signature);
            updates.push(PromotedLayerUpdate {
                node_id: node.id(),
                parent_id: node.parent_id(),
                kind,
                base_signature,
                previous_base_signature,
                composition_kind,
                composition_signature,
                previous_composition_signature,
            });
        }
        WalkState {
            base_signature,
            _composition_signature: composition_signature,
            output_signature,
            has_promoted_output,
        }
    }

    let mut updates = Vec::new();
    let mut next_base_signatures = HashMap::new();
    let mut next_composition_signatures = HashMap::new();
    for root in roots {
        walk(
            root.as_ref(),
            promoted_node_ids,
            previous_base_signatures,
            previous_composition_signatures,
            &mut updates,
            &mut next_base_signatures,
            &mut next_composition_signatures,
        );
    }
    updates.sort_by_key(|update| update.node_id);
    (updates, next_base_signatures, next_composition_signatures)
}

pub(crate) fn collect_debug_subtree_signatures(
    roots: &[Box<dyn ElementTrait>],
    promoted_node_ids: &HashSet<u64>,
) -> HashMap<u64, (u64, u64, u64, bool)> {
    struct WalkState {
        base_signature: u64,
        _composition_signature: u64,
        output_signature: u64,
        has_promoted_output: bool,
    }

    fn hash_composition_state(node: &dyn ElementTrait, hasher: &mut DefaultHasher) {
        node.promotion_clip_intersection_signature().hash(hasher);
        let bounds = node.promotion_composite_bounds();
        bounds.x.to_bits().hash(hasher);
        bounds.y.to_bits().hash(hasher);
        bounds.width.max(0.0).to_bits().hash(hasher);
        bounds.height.max(0.0).to_bits().hash(hasher);
        for radius in bounds.corner_radii {
            radius.to_bits().hash(hasher);
        }
        node.promotion_node_info()
            .opacity
            .clamp(0.0, 1.0)
            .to_bits()
            .hash(hasher);
    }

    fn walk(
        node: &dyn ElementTrait,
        promoted_node_ids: &HashSet<u64>,
        out: &mut HashMap<u64, (u64, u64, u64, bool)>,
    ) -> WalkState {
        let mut hasher = DefaultHasher::new();
        let mut composition_hasher = DefaultHasher::new();
        node.promotion_self_signature().hash(&mut hasher);
        node.promotion_clip_intersection_signature()
            .hash(&mut hasher);
        let self_is_promoted = promoted_node_ids.contains(&node.id());
        let mut has_promoted_output = self_is_promoted;
        if self_is_promoted {
            hash_composition_state(node, &mut composition_hasher);
        }
        if let Some(children) = node.children() {
            for (index, child) in children.iter().enumerate() {
                let child_state = walk(child.as_ref(), promoted_node_ids, out);
                index.hash(&mut hasher);
                child.id().hash(&mut hasher);
                let child_is_promoted = promoted_node_ids.contains(&child.id());
                child_is_promoted.hash(&mut hasher);
                if !child_is_promoted {
                    child_state.base_signature.hash(&mut hasher);
                }

                let child_is_deferred = child
                    .as_any()
                    .downcast_ref::<crate::view::base_component::Element>()
                    .is_some_and(
                        crate::view::base_component::Element::should_append_to_root_viewport_render,
                    );
                if child_is_deferred {
                    continue;
                }
                if child_is_promoted || child_state.has_promoted_output {
                    if !has_promoted_output {
                        hash_composition_state(node, &mut composition_hasher);
                        has_promoted_output = true;
                    }
                    index.hash(&mut composition_hasher);
                    child.id().hash(&mut composition_hasher);
                    child_is_promoted.hash(&mut composition_hasher);
                    child_state.output_signature.hash(&mut composition_hasher);
                }
            }
        }
        let base_signature = hasher.finish();
        let composition_signature = if has_promoted_output {
            composition_hasher.finish()
        } else {
            0
        };
        let output_signature = if has_promoted_output {
            let mut output_hasher = DefaultHasher::new();
            base_signature.hash(&mut output_hasher);
            composition_signature.hash(&mut output_hasher);
            output_hasher.finish()
        } else {
            base_signature
        };
        out.insert(
            node.id(),
            (
                base_signature,
                composition_signature,
                output_signature,
                has_promoted_output,
            ),
        );
        WalkState {
            base_signature,
            _composition_signature: composition_signature,
            output_signature,
            has_promoted_output,
        }
    }

    let mut out = HashMap::new();
    for root in roots {
        walk(root.as_ref(), promoted_node_ids, &mut out);
    }
    out
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
    use crate::transition::{
        CHANNEL_STYLE_BACKGROUND_COLOR, ClaimMode, StyleField, StyleTransition,
        StyleTransitionPlugin, StyleValue, TrackKey, TrackTarget, Transition, TransitionFrame,
        TransitionHost, TransitionPluginId,
    };
    use crate::view::base_component::{Element, ElementTrait, set_style_field_by_id};
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    struct TestHost {
        claims: HashMap<TrackKey<TrackTarget>, TransitionPluginId>,
    }

    impl TransitionHost<TrackTarget> for TestHost {
        fn is_channel_registered(&self, channel: crate::transition::ChannelId) -> bool {
            channel == CHANNEL_STYLE_BACKGROUND_COLOR
        }

        fn claim_track(
            &mut self,
            plugin_id: TransitionPluginId,
            key: TrackKey<TrackTarget>,
            mode: ClaimMode,
        ) -> bool {
            if let Some(current) = self.claims.get(&key).copied() {
                if current == plugin_id {
                    return true;
                }
                if matches!(mode, ClaimMode::Replace) {
                    self.claims.insert(key, plugin_id);
                    return true;
                }
                return false;
            }
            self.claims.insert(key, plugin_id);
            true
        }

        fn release_track_claim(
            &mut self,
            plugin_id: TransitionPluginId,
            key: TrackKey<TrackTarget>,
        ) {
            if self.claims.get(&key).copied() == Some(plugin_id) {
                self.claims.remove(&key);
            }
        }

        fn release_all_claims(&mut self, plugin_id: TransitionPluginId) {
            self.claims.retain(|_, owner| *owner != plugin_id);
        }
    }

    fn build_style_transition_lab_like_tree() -> Vec<Box<dyn ElementTrait>> {
        let mut root = Element::new_with_id(1, 0.0, 0.0, 720.0, 520.0);
        let mut intro = Element::new_with_id(2, 16.0, 16.0, 320.0, 20.0);
        intro.set_foreground_color(Color::rgb(226, 232, 240));
        root.add_child(Box::new(intro));

        let mut cards_row = Element::new_with_id(3, 16.0, 52.0, 688.0, 220.0);

        let mut style_card = Element::new_with_id(4, 16.0, 52.0, 220.0, 180.0);
        style_card.set_background_color_value(Color::rgb(31, 41, 55));
        style_card.set_border_radius(12.0);

        let mut card_title = Element::new_with_id(5, 28.0, 64.0, 180.0, 18.0);
        card_title.set_foreground_color(Color::rgb(226, 232, 240));
        style_card.add_child(Box::new(card_title));

        let mut card_status = Element::new_with_id(6, 28.0, 88.0, 180.0, 16.0);
        card_status.set_foreground_color(Color::rgb(148, 163, 184));
        style_card.add_child(Box::new(card_status));

        let mut animated_box = Element::new_with_id(7, 28.0, 116.0, 180.0, 56.0);
        animated_box.set_background_color_value(Color::rgb(34, 197, 94));
        animated_box.set_border_radius(8.0);
        style_card.add_child(Box::new(animated_box));

        let mut controls_row = Element::new_with_id(8, 28.0, 180.0, 180.0, 28.0);
        let mut start_button = Element::new_with_id(9, 28.0, 180.0, 88.0, 28.0);
        start_button.set_background_color_value(Color::rgb(56, 189, 248));
        let mut remove_button = Element::new_with_id(10, 120.0, 180.0, 88.0, 28.0);
        remove_button.set_background_color_value(Color::rgb(56, 189, 248));
        controls_row.add_child(Box::new(start_button));
        controls_row.add_child(Box::new(remove_button));
        style_card.add_child(Box::new(controls_row));

        cards_row.add_child(Box::new(style_card));
        root.add_child(Box::new(cards_row));
        vec![Box::new(root)]
    }

    #[test]
    fn promoted_child_change_does_not_dirty_parent_base() {
        let mut root = Element::new_with_id(1, 0.0, 0.0, 200.0, 200.0);
        let child = Element::new_with_id(2, 0.0, 0.0, 100.0, 100.0);
        root.add_child(Box::new(child));

        let roots: Vec<Box<dyn ElementTrait>> = vec![Box::new(root)];
        let promoted = HashSet::from([1_u64, 2_u64]);
        let (first_updates, first_base_signatures, first_composition_signatures) =
            collect_promoted_layer_updates(&roots, &promoted, &HashMap::new(), &HashMap::new());
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

        let (second_updates, _, _) = collect_promoted_layer_updates(
            &roots,
            &promoted,
            &first_base_signatures,
            &first_composition_signatures,
        );
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

    #[test]
    fn promoted_opacity_change_reuses_base_but_reraster_composition() {
        let mut root = Element::new_with_id(1, 0.0, 0.0, 200.0, 200.0);
        root.set_opacity(0.6);
        let child = Element::new_with_id(2, 0.0, 0.0, 100.0, 100.0);
        root.add_child(Box::new(child));

        let roots: Vec<Box<dyn ElementTrait>> = vec![Box::new(root)];
        let promoted = HashSet::from([1_u64]);
        let (first_updates, first_base_signatures, first_composition_signatures) =
            collect_promoted_layer_updates(&roots, &promoted, &HashMap::new(), &HashMap::new());
        assert_eq!(first_updates.len(), 1);
        assert_eq!(first_updates[0].kind, PromotedLayerUpdateKind::Reraster);
        assert_eq!(
            first_updates[0].composition_kind,
            PromotedLayerUpdateKind::Reraster
        );

        let mut roots = roots;
        let root = roots[0]
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("root should be element");
        root.set_opacity(0.3);

        let (second_updates, _, _) = collect_promoted_layer_updates(
            &roots,
            &promoted,
            &first_base_signatures,
            &first_composition_signatures,
        );
        assert_eq!(second_updates.len(), 1);
        let root_update = &second_updates[0];
        assert_eq!(root_update.node_id, 1);
        assert_eq!(root_update.kind, PromotedLayerUpdateKind::Reuse);
        assert_eq!(
            root_update.composition_kind,
            PromotedLayerUpdateKind::Reraster
        );
    }

    #[test]
    fn style_transition_lab_like_nested_non_promoted_child_change_dirties_promoted_root() {
        let roots = build_style_transition_lab_like_tree();
        let promoted = HashSet::from([1_u64]);
        let (first_updates, first_base_signatures, first_composition_signatures) =
            collect_promoted_layer_updates(&roots, &promoted, &HashMap::new(), &HashMap::new());
        assert_eq!(first_updates.len(), 1);
        assert_eq!(first_updates[0].node_id, 1);
        assert_eq!(first_updates[0].kind, PromotedLayerUpdateKind::Reraster);

        let mut roots = roots;
        let root = roots[0]
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("root should be element");
        let cards_row = root
            .children_mut()
            .and_then(|children| children.get_mut(1))
            .and_then(|child| child.as_any_mut().downcast_mut::<Element>())
            .expect("cards row should be element");
        let style_card = cards_row
            .children_mut()
            .and_then(|children| children.get_mut(0))
            .and_then(|child| child.as_any_mut().downcast_mut::<Element>())
            .expect("style card should be element");
        let animated_box = style_card
            .children_mut()
            .and_then(|children| children.get_mut(2))
            .and_then(|child| child.as_any_mut().downcast_mut::<Element>())
            .expect("animated box should be element");
        animated_box.set_background_color_value(Color::rgb(249, 115, 22));

        let (second_updates, _, _) = collect_promoted_layer_updates(
            &roots,
            &promoted,
            &first_base_signatures,
            &first_composition_signatures,
        );
        assert_eq!(second_updates.len(), 1);
        let root_update = &second_updates[0];
        assert_eq!(root_update.node_id, 1);
        assert_eq!(root_update.kind, PromotedLayerUpdateKind::Reraster);
    }

    #[test]
    fn style_transition_plugin_samples_dirty_promoted_root_in_lab_like_structure() {
        let mut roots = build_style_transition_lab_like_tree();
        let promoted = HashSet::from([1_u64]);
        let (_, first_base_signatures, first_composition_signatures) =
            collect_promoted_layer_updates(&roots, &promoted, &HashMap::new(), &HashMap::new());

        let mut plugin = StyleTransitionPlugin::new();
        let mut host = TestHost::default();
        plugin
            .start_style_track(
                &mut host,
                7,
                StyleField::BackgroundColor,
                StyleValue::Color(Color::rgb(34, 197, 94)),
                StyleValue::Color(Color::rgb(249, 115, 22)),
                StyleTransition::new(1000),
            )
            .expect("style track should start");

        let run = plugin.run_tracks(
            TransitionFrame {
                dt_seconds: 0.016,
                now_seconds: 0.5,
            },
            &mut host,
        );
        assert!(run.needs_paint);

        let samples = plugin.take_samples();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].target, 7);
        assert_eq!(samples[0].field, StyleField::BackgroundColor);
        assert!(set_style_field_by_id(
            roots[0].as_mut(),
            samples[0].target,
            samples[0].field,
            samples[0].value,
        ));

        let (updates, _, _) = collect_promoted_layer_updates(
            &roots,
            &promoted,
            &first_base_signatures,
            &first_composition_signatures,
        );
        assert_eq!(updates.len(), 1);
        let root_update = &updates[0];
        assert_eq!(root_update.node_id, 1);
        assert_eq!(root_update.kind, PromotedLayerUpdateKind::Reraster);
    }
}
