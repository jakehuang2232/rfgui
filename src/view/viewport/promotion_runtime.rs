//! Layer promotion runtime — evaluation, update collection, and compositing.

use super::*;

impl Viewport {
    pub(super) fn update_promotion_state(&mut self, roots: &[Box<dyn crate::view::base_component::ElementTrait>]) {
        let previous_promoted_node_ids = self.compositor.promotion_state.promoted_node_ids.clone();
        let active_animator_hints = self.transitions.animation_plugin.active_promotion_hints();
        let active_channels = active_channels_by_node(&self.transitions.transition_claims);
        let candidates = collect_promotion_candidates(
            roots,
            &active_animator_hints,
            &active_channels,
            (self.logical_width, self.logical_height),
        );
        let next_promotion_state = evaluate_promotion(
            candidates,
            (self.logical_width, self.logical_height),
            self.compositor.promotion_config,
        );
        let promotion_topology_changed =
            previous_promoted_node_ids != next_promotion_state.promoted_node_ids;
        self.compositor.promotion_state = next_promotion_state;
        if promotion_topology_changed {
            self.compositor.promoted_base_signatures.clear();
            self.compositor.promoted_composition_signatures.clear();
            self.compositor.promoted_layer_updates.clear();
            self.compositor.promoted_reuse_cooldown_frames = Self::PROMOTED_REUSE_COOLDOWN_FRAMES;
        }
        let (mut updates, next_base_signatures, next_composition_signatures) =
            collect_promoted_layer_updates(
                roots,
                &self.compositor.promotion_state.promoted_node_ids,
                &self.compositor.promoted_base_signatures,
                &self.compositor.promoted_composition_signatures,
            );
        if self.debug_options.trace_reuse_path {
            let subtree_signatures =
                collect_debug_subtree_signatures(roots, &self.compositor.promotion_state.promoted_node_ids);
            let previous_subtree_signatures = &self.compositor.debug_previous_subtree_signatures;
            let mut sampled_roots = snapshot_debug_style_sample_records()
                .into_iter()
                .filter_map(|record| record.promoted_root.map(|root| (record.target, root)))
                .collect::<Vec<_>>();
            sampled_roots.sort_unstable();
            sampled_roots.dedup();
            for (target, root_id) in sampled_roots {
                if let Some(update) = updates.iter().find(|update| update.node_id == root_id) {
                    let ancestry = roots
                        .iter()
                        .rev()
                        .find_map(|root| {
                            crate::view::base_component::get_node_ancestry_ids(root.as_ref(), target)
                        })
                        .unwrap_or_default();
                    let walk_desc = ancestry
                        .into_iter()
                        .filter_map(|node_id| {
                            subtree_signatures
                                .get(&node_id)
                                .map(|(base, comp, output, has_output)| {
                                    let prev = previous_subtree_signatures.get(&node_id).copied();
                                    let prev_desc = prev
                                        .map(|(prev_base, prev_comp, prev_output, prev_has_out)| {
                                            format!(
                                                "prev_base={prev_base},prev_comp={prev_comp},prev_out={prev_output},prev_has_out={prev_has_out},"
                                            )
                                        })
                                        .unwrap_or_default();
                                    format!(
                                        "{node_id}[{prev_desc}base={base},comp={comp},out={output},has_out={has_output}]"
                                    )
                                })
                        })
                        .collect::<Vec<_>>()
                        .join(" -> ");
                    record_debug_style_promotion(format!(
                        "target={} promoted_root={} kind={:?} prev_base={:?} base={} prev_comp={:?} comp={} walk={}",
                        target,
                        root_id,
                        update.kind,
                        update.previous_base_signature,
                        update.base_signature,
                        update.previous_composition_signature,
                        update.composition_signature,
                        walk_desc
                    ));
                }
            }
            self.compositor.debug_previous_subtree_signatures = subtree_signatures;
        } else {
            self.compositor.debug_previous_subtree_signatures.clear();
        }
        if self.compositor.promoted_reuse_cooldown_frames > 0 {
            for update in &mut updates {
                update.kind = PromotedLayerUpdateKind::Reraster;
                update.composition_kind = PromotedLayerUpdateKind::Reraster;
            }
            self.compositor.promoted_reuse_cooldown_frames =
                self.compositor.promoted_reuse_cooldown_frames.saturating_sub(1);
        }
        self.compositor.promoted_layer_updates = updates;
        self.compositor.promoted_base_signatures = next_base_signatures;
        self.compositor.promoted_composition_signatures = next_composition_signatures;
    }

    pub(super) fn apply_promotion_runtime(&self, ctx: &mut crate::view::base_component::UiBuildContext) {
        let promoted_update_kinds = self
            .compositor
            .promoted_layer_updates
            .iter()
            .map(|update| (update.node_id, update.kind))
            .collect::<HashMap<_, _>>();
        let promoted_composition_update_kinds = self
            .compositor
            .promoted_layer_updates
            .iter()
            .map(|update| (update.node_id, update.composition_kind))
            .collect::<HashMap<_, _>>();
        ctx.set_promoted_runtime(
            Arc::new(self.compositor.promotion_state.promoted_node_ids.clone()),
            Arc::new(promoted_update_kinds),
            Arc::new(promoted_composition_update_kinds),
        );
    }

    pub(super) fn composite_promoted_root(
        &self,
        graph: &mut FrameGraph,
        ctx: &mut crate::view::base_component::UiBuildContext,
        root: &dyn crate::view::base_component::ElementTrait,
        layer_target: crate::view::render_pass::draw_rect_pass::RenderTargetOut,
    ) {
        let composite_bounds = root.promotion_composite_bounds();
        let opacity = if root
            .as_any()
            .downcast_ref::<crate::view::base_component::Element>()
            .is_some()
        {
            1.0
        } else {
            root.promotion_node_info().opacity.clamp(0.0, 1.0)
        };
        let parent_target = ctx.current_target().unwrap_or_else(|| {
            let target = ctx.allocate_target(graph);
            ctx.set_current_target(target);
            target
        });
        ctx.set_current_target(parent_target);
        let pass = crate::view::render_pass::composite_layer_pass::CompositeLayerPass::new(
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
                        .expect("promoted root layer target should exist"),
                ),
                pass_context: ctx.graphics_pass_context(),
            },
            crate::view::render_pass::composite_layer_pass::CompositeLayerOutput {
                render_target: parent_target,
            },
        );
        graph.add_graphics_pass(pass);
        ctx.set_current_target(parent_target);
    }
}
