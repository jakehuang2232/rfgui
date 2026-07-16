//! Layer promotion runtime — evaluation, update collection, and compositing.

use super::*;

impl Viewport {
    fn current_viewport_raster_fingerprint(
        &self,
    ) -> crate::view::compositor::raster_cache::ViewportRasterFingerprint {
        crate::view::compositor::raster_cache::ViewportRasterFingerprint {
            logical_width_bits: self.logical_width.to_bits(),
            logical_height_bits: self.logical_height.to_bits(),
            scale_factor_bits: self.scale_factor.to_bits(),
            target_format: self.gpu.surface_target_format,
            sample_count: self.gpu.msaa_sample_count.max(1),
        }
    }

    pub(super) fn maybe_evaluate_raster_budget_readiness(&mut self) {
        self.maybe_evaluate_raster_budget_readiness_inner(false);
    }

    fn maybe_evaluate_raster_budget_readiness_inner(&mut self, force: bool) {
        if !force && !self.debug_options.trace_reuse_path {
            return;
        }
        let current_source =
            crate::view::compositor::raster_cache::RasterSourceFingerprint::capture(
                &self.compositor.promotion_state,
                &self.compositor.shadow_layer_tree,
            );
        let observed = self
            .compositor
            .raster_cache
            .observed_consistency(&current_source, self.current_viewport_raster_fingerprint());
        self.compositor.raster_budget_readiness =
            crate::view::compositor::raster_cache::evaluate_shadow_budget_readiness(
                observed,
                self.compositor.prospective_raster_plan.as_ref(),
                &self.compositor.raster_plan_parity,
            );
        if self.debug_options.trace_reuse_path {
            record_debug_style_promotion(format!(
                "observed-raster-snapshot-consistency consistent={} fallback_reasons={:?}; shadow-raster-budget-readiness ready={} prospective_ready={} parity_ready={} prospective_fallback_reasons={:?}",
                self.compositor.raster_budget_readiness.observed.consistent,
                self.compositor
                    .raster_budget_readiness
                    .observed
                    .fallback_reasons,
                self.compositor.raster_budget_readiness.ready,
                self.compositor.raster_budget_readiness.prospective_ready,
                self.compositor.raster_budget_readiness.parity_ready,
                self.compositor
                    .raster_budget_readiness
                    .prospective_fallback_reasons,
            ));
        }
    }

    pub(super) fn maybe_plan_prospective_raster_resources(&mut self) {
        if !cfg!(test) && !self.debug_options.trace_reuse_path {
            return;
        }
        let plan = crate::view::compositor::raster_cache::plan_prospective_raster_resources(
            &self.scene.node_arena,
            &self.scene.ui_root_keys,
            &self.compositor.promotion_state.promoted_node_ids,
            self.current_viewport_raster_fingerprint(),
        );
        self.compositor.prospective_raster_plan = Some(plan);
        self.compositor.raster_plan_parity = Default::default();
        let current_source =
            crate::view::compositor::raster_cache::RasterSourceFingerprint::capture(
                &self.compositor.promotion_state,
                &self.compositor.shadow_layer_tree,
            );
        let observed = self
            .compositor
            .raster_cache
            .observed_consistency(&current_source, self.current_viewport_raster_fingerprint());
        self.compositor.raster_budget_readiness =
            crate::view::compositor::raster_cache::evaluate_shadow_budget_readiness(
                observed,
                self.compositor.prospective_raster_plan.as_ref(),
                &self.compositor.raster_plan_parity,
            );
        if self.debug_options.trace_reuse_path {
            let plan = self
                .compositor
                .prospective_raster_plan
                .as_ref()
                .expect("prospective plan should be installed");
            record_debug_style_promotion(format!(
                "prospective-raster-plan resources={} total_bytes={} confidence={:?} errors={:?} budget_ready={} parity_ready={}",
                plan.resources.len(),
                plan.total_bytes,
                plan.confidence,
                plan.errors,
                self.compositor.raster_budget_readiness.prospective_ready,
                self.compositor.raster_budget_readiness.parity_ready,
            ));
        }
    }

    pub(super) fn sync_compositor_property_trees(&mut self) {
        self.compositor
            .property_trees
            .sync(&self.scene.node_arena, &self.scene.ui_root_keys);
    }

    #[cfg(test)]
    pub(super) fn compositor_property_tree_epoch(&self) -> u64 {
        self.compositor.property_trees.epoch()
    }

    pub(super) fn update_promotion_state(&mut self) {
        let active_animator_hints = self.transitions.animation_plugin.active_promotion_hints();
        let active_channels = active_channels_by_node(&self.transitions.transition_claims);
        let candidates = collect_promotion_candidates(
            &self.scene.node_arena,
            &self.scene.ui_root_keys,
            &active_animator_hints,
            &active_channels,
            (self.logical_width, self.logical_height),
            self.scale_factor,
            self.gpu.surface_target_format,
        );
        let shadow_candidates = self
            .debug_options
            .trace_reuse_path
            .then(|| candidates.clone());
        let next_promotion_state = evaluate_promotion(
            candidates,
            (self.logical_width, self.logical_height),
            self.scale_factor,
            self.compositor.promotion_config,
        );
        if let Some(shadow_candidates) = shadow_candidates {
            let readiness = &self.compositor.raster_budget_readiness;
            let mut fallback_reasons = Vec::new();
            if !readiness.observed.consistent {
                fallback_reasons.push(
                    crate::view::compositor::raster_cache::ShadowEvaluationFallbackReason::ObservedSnapshotInconsistent,
                );
            }
            if !readiness.prospective_ready {
                fallback_reasons.push(
                    crate::view::compositor::raster_cache::ShadowEvaluationFallbackReason::ProspectivePlanNotReady,
                );
            }
            if !readiness.parity_ready {
                fallback_reasons.push(
                    crate::view::compositor::raster_cache::ShadowEvaluationFallbackReason::PreviousParityNotReady,
                );
            }
            let used_observed_cost = fallback_reasons.is_empty();
            let (shadow_state, final_projection, accumulator_metrics, policy_transitions) =
                if used_observed_cost {
                    let arena = &self.scene.node_arena;
                    let roots = &self.scene.ui_root_keys;
                    let viewport = self.current_viewport_raster_fingerprint();
                    let snapshot = &self.compositor.raster_cache.snapshot;
                    let mut accumulator =
                        crate::view::compositor::raster_cache::ProspectivePlanAccumulator::new(
                            arena, roots, viewport, snapshot,
                        );
                    let policy_result = crate::view::promotion::evaluate_shadow_promotion_policy(
                        shadow_candidates,
                        &mut self.compositor.shadow_promotion_policy_state,
                        self.compositor.shadow_policy_config,
                        (self.logical_width, self.logical_height),
                        self.scale_factor,
                        self.compositor.promotion_config,
                        |tentative| {
                            let projection = accumulator.project_tentative(tentative);
                            crate::view::promotion::ShadowPolicyBudgetProjection {
                                logical_planned_bytes: projection
                                    .planned_promotion_bytes
                                    .min(usize::MAX as u64)
                                    as usize,
                                projected_peak_bytes: projection
                                    .projected_peak_bytes
                                    .min(usize::MAX as u64)
                                    as usize,
                            }
                        },
                    );
                    let state = policy_result.state;
                    accumulator.finish(&state.promoted_node_ids);
                    let projection = accumulator.current_projection();
                    (
                        state,
                        projection,
                        accumulator.metrics(),
                        policy_result.transitions,
                    )
                } else {
                    self.compositor
                        .shadow_promotion_policy_state
                        .sync_legacy_and_reset(&next_promotion_state);
                    (
                        next_promotion_state.clone(),
                        Default::default(),
                        Default::default(),
                        Vec::new(),
                    )
                };
            let decision_diffs =
                crate::view::compositor::raster_cache::diff_shadow_promotion_decisions(
                    &next_promotion_state,
                    &shadow_state,
                );
            self.compositor.shadow_promotion_evaluation =
                crate::view::compositor::raster_cache::ShadowPromotionEvaluation {
                    used_observed_cost,
                    fallback_reasons,
                    state: shadow_state,
                    final_projection,
                    decision_diffs,
                    accumulator_metrics,
                    policy_transitions,
                };
            self.compositor.shadow_rollout_safety.observe_if_enabled(
                self.debug_options.trace_reuse_path,
                &self.compositor.raster_budget_readiness,
                &next_promotion_state,
                &self.compositor.shadow_promotion_evaluation,
            );
            let rollout_eligible = crate::view::compositor::raster_cache::shadow_rollout_eligible(
                self.debug_options.trace_reuse_path,
                self.compositor.promotion_config.enabled,
                &self.compositor.shadow_rollout_safety,
                &self.compositor.raster_cache.snapshot,
                self.compositor.prospective_raster_plan.as_ref(),
                Default::default(),
            );
            let shadow = &self.compositor.shadow_promotion_evaluation;
            let safety = &self.compositor.shadow_rollout_safety;
            record_debug_style_promotion(format!(
                "shadow-rollout-summary observed={} prospective={} parity={} full={} observed_streak={} prospective_streak={} parity_streak={} full_streak={} fallback_observed={:?} fallback_prospective={:?} legacy_count={} shadow_count={} diff_count={} admit={} retain={} drop={} reject={} planned_logical={} resident={} retiring={} incremental={} projected_peak={} validation_errors={} unknown_cost={} eligible={}",
                safety.last.observed_consistent,
                safety.last.prospective_ready,
                safety.last.parity_compatible,
                safety.last.full_ready,
                safety.consecutive_observed_consistent_frames,
                safety.consecutive_prospective_ready_frames,
                safety.consecutive_parity_compatible_frames,
                safety.consecutive_full_ready_frames,
                safety.last.observed_fallback_reasons,
                safety.last.prospective_fallback_reasons,
                safety.last.legacy_promoted_count,
                safety.last.shadow_promoted_count,
                safety.last.decision_diff_count,
                safety.last.admit_count,
                safety.last.retain_count,
                safety.last.drop_count,
                safety.last.reject_count,
                safety.last.planned_logical_bytes,
                safety.last.resident_bytes,
                safety.last.retiring_bytes,
                safety.last.incremental_bytes,
                safety.last.projected_peak_bytes,
                self.compositor
                    .raster_cache
                    .snapshot
                    .validation_errors
                    .len(),
                !self
                    .compositor
                    .raster_cache
                    .snapshot
                    .all_costs_budget_usable,
                rollout_eligible,
            ));
            let transition_count = |kind| {
                shadow
                    .policy_transitions
                    .iter()
                    .filter(|transition| transition.kind == kind)
                    .count()
            };
            record_debug_style_promotion(format!(
                "observed-cost-shadow used={} fallback_reasons={:?} promoted={} planned_promotion_bytes={} current_promotion_resident_bytes={} promotion_retiring_bytes={} incremental_resident_bytes={} projected_peak_bytes={} differing_nodes={} transitions(admit={},retain={},drop={},reject={}) planner_calls={} planner_visits={} precompute_visits={} accumulator_try_calls={} accumulator_ancestor_visits={}",
                shadow.used_observed_cost,
                shadow.fallback_reasons,
                shadow.state.promoted_node_ids.len(),
                shadow.final_projection.planned_promotion_bytes,
                shadow.final_projection.current_promotion_resident_bytes,
                shadow.final_projection.promotion_retiring_bytes,
                shadow.final_projection.incremental_resident_bytes,
                shadow.final_projection.projected_peak_bytes,
                shadow.decision_diffs.len(),
                transition_count(crate::view::promotion::ShadowPolicyTransitionKind::Admit),
                transition_count(crate::view::promotion::ShadowPolicyTransitionKind::Retain),
                transition_count(crate::view::promotion::ShadowPolicyTransitionKind::Drop),
                transition_count(crate::view::promotion::ShadowPolicyTransitionKind::Reject),
                shadow.accumulator_metrics.full_planner_calls,
                shadow.accumulator_metrics.full_planner_node_visits,
                shadow.accumulator_metrics.precompute_node_visits,
                shadow.accumulator_metrics.accumulator_try_calls,
                shadow.accumulator_metrics.accumulator_ancestor_visits,
            ));
            for transition in &shadow.policy_transitions {
                let always_report = matches!(
                    transition.kind,
                    crate::view::promotion::ShadowPolicyTransitionKind::Admit
                        | crate::view::promotion::ShadowPolicyTransitionKind::Drop
                );
                let has_diagnostic_state = transition.admission_streak > 0
                    || transition.demotion_streak > 0
                    || transition.budget_rejection.is_some();
                if always_report || has_diagnostic_state {
                    record_debug_style_promotion(format!(
                        "observed-cost-shadow-transition {transition:?}"
                    ));
                }
            }
            for diff in &shadow.decision_diffs {
                record_debug_style_promotion(format!("observed-cost-shadow-diff {diff:?}"));
            }
        }
        let promotion_topology_changed = self.compositor.promotion_state.promoted_node_ids
            != next_promotion_state.promoted_node_ids;
        self.compositor.promotion_state = next_promotion_state;
        if promotion_topology_changed {
            self.compositor.promoted_base_signatures.clear();
            self.compositor.promoted_composition_signatures.clear();
            self.compositor.promoted_base_generations.clear();
            self.compositor.promoted_composition_generations.clear();
            self.compositor.promoted_layer_updates.clear();
            self.compositor.promoted_reuse_cooldown_frames = Self::PROMOTED_REUSE_COOLDOWN_FRAMES;
        }
        let collection = collect_promoted_layer_updates_with_generations(
            &self.scene.node_arena,
            &self.scene.ui_root_keys,
            &self.compositor.promotion_state.promoted_node_ids,
            &self.compositor.promoted_base_signatures,
            &self.compositor.promoted_composition_signatures,
            &mut self.compositor.paint_generations,
            &self.compositor.property_trees,
            &self.compositor.promoted_base_generations,
            &self.compositor.promoted_composition_generations,
            self.debug_options.trace_reuse_path,
        );
        let mut updates = collection.updates;
        let next_base_signatures = collection.base_signatures;
        let next_composition_signatures = collection.composition_signatures;
        let next_base_generations = collection.base_generations;
        let next_composition_generations = collection.composition_generations;
        if self.debug_options.trace_reuse_path {
            let subtree_signatures = collection.debug_subtree_signatures;
            let previous_subtree_signatures = &self.compositor.debug_previous_subtree_signatures;
            let mut sampled_roots = take_debug_style_sample_records()
                .into_iter()
                .filter_map(|record| record.promoted_root.map(|root| (record.target, root)))
                .collect::<Vec<_>>();
            sampled_roots.sort_unstable();
            sampled_roots.dedup();
            for (target, root_id) in sampled_roots {
                if let Some(update) = updates.iter().find(|update| update.node_id == root_id) {
                    let ancestry = self
                        .scene
                        .ui_root_keys
                        .iter()
                        .rev()
                        .find_map(|&rk| {
                            let root_node = self.scene.node_arena.get(rk)?;
                            crate::view::viewport::debug::get_node_ancestry_ids(
                                root_node.element.as_ref(),
                                target,
                                &self.scene.node_arena,
                            )
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
                        "target={} promoted_root={} kind={:?} prev_base={:?} base={} prev_base_gen={:?} base_gen={} prev_comp={:?} comp={} prev_comp_gen={:?} comp_gen={} walk={}",
                        target,
                        root_id,
                        update.kind,
                        update.previous_base_signature,
                        update.base_signature,
                        update.previous_base_generation,
                        update.base_generation,
                        update.previous_composition_signature,
                        update.composition_signature,
                        update.previous_composition_generation,
                        update.composition_generation,
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
            self.compositor.promoted_reuse_cooldown_frames = self
                .compositor
                .promoted_reuse_cooldown_frames
                .saturating_sub(1);
        }
        self.compositor.promoted_layer_updates = updates;
        self.compositor.promoted_base_signatures = next_base_signatures;
        self.compositor.promoted_composition_signatures = next_composition_signatures;
        self.compositor.promoted_base_generations = next_base_generations;
        self.compositor.promoted_composition_generations = next_composition_generations;
    }

    /// Reconcile observational compositor topology after the authoritative
    /// promotion walk has finalized this frame's decisions and generations.
    /// The resulting tree does not drive rendering or resource lifetime.
    pub(super) fn sync_shadow_layer_tree(&mut self) {
        let volatile_epoch = self.compositor.shadow_layer_tree.epoch.wrapping_add(1);
        let result = crate::view::compositor::layerizer::layerize_shadow_tree(
            &self.scene.node_arena,
            &self.scene.ui_root_keys,
            &self.compositor.promotion_state,
            &self.compositor.promoted_layer_updates,
            &self.compositor.property_trees,
            &self.compositor.paint_generations,
            volatile_epoch,
        );
        let coverage = result.coverage_stats.clone();
        self.compositor
            .shadow_layer_tree
            .reconcile(result.layers, result.validation_errors);

        if self.debug_options.trace_reuse_path {
            let tree = &self.compositor.shadow_layer_tree;
            record_debug_style_promotion(format!(
                "shadow-layer-tree epoch={} layers={} roots={} added={} removed={} reparented={} reordered={} raster_changed={} composite_changed={} topology_changed={} metadata_changed={} validation_errors={:?}",
                tree.epoch,
                tree.layers.len(),
                tree.root_children().len(),
                tree.last_diff.added.len(),
                tree.last_diff.removed.len(),
                tree.last_diff.reparented.len(),
                tree.last_diff.reordered.len(),
                tree.last_diff.raster_changed.len(),
                tree.last_diff.composite_changed.len(),
                tree.last_diff.topology_changed.len(),
                tree.last_diff.metadata_changed.len(),
                tree.validation_errors,
            ));
            record_debug_style_promotion(format!(
                "paint-coverage total_nodes={} artifact_nodes={} artifact_chunks={} legacy_boundaries={} legacy_covered_nodes={} legacy_by_reason={:?} promoted_boundaries={} validation_errors={} authority_eligible={} authority_ineligible_reasons={:?}",
                coverage.total_nodes,
                coverage.artifact_nodes,
                coverage.artifact_chunks,
                coverage.legacy_boundaries,
                coverage.legacy_covered_nodes,
                coverage.legacy_by_reason,
                coverage.promoted_boundaries,
                coverage.validation_errors,
                coverage.authority_eligible,
                coverage.authority_ineligible_reasons,
            ));
        }
    }

    pub(super) fn maybe_sync_shadow_layer_tree(&mut self) {
        self.maybe_sync_shadow_layer_tree_inner(cfg!(test));
    }

    fn maybe_sync_shadow_layer_tree_inner(&mut self, enabled_for_tests: bool) {
        if enabled_for_tests || self.debug_options.trace_reuse_path {
            self.sync_shadow_layer_tree();
        }
    }

    pub(super) fn maybe_sync_raster_cache_observation(
        &mut self,
        graph: &crate::view::frame_graph::FrameGraph,
    ) {
        if !cfg!(test) && !self.debug_options.trace_reuse_path {
            return;
        }
        let declared = graph
            .declared_persistent_textures()
            .map(|(stable_key, desc)| (stable_key, desc.clone()))
            .collect::<Vec<_>>();
        let resident = self
            .frame
            .offscreen_render_target_pool
            .persistent_resident_observations();
        let source_fingerprint =
            crate::view::compositor::raster_cache::RasterSourceFingerprint::capture(
                &self.compositor.promotion_state,
                &self.compositor.shadow_layer_tree,
            );
        let viewport_fingerprint = self.current_viewport_raster_fingerprint();
        self.compositor.raster_cache.reconcile(
            self.frame.frame_number,
            source_fingerprint,
            viewport_fingerprint,
            declared
                .iter()
                .map(|(stable_key, desc)| (*stable_key, desc)),
            resident,
            &self.compositor.shadow_layer_tree,
            &self.scene.node_arena,
        );
        self.compositor.raster_plan_parity = self
            .compositor
            .prospective_raster_plan
            .as_ref()
            .map(|plan| {
                crate::view::compositor::raster_cache::compare_plan_to_snapshot(
                    plan,
                    &self.compositor.raster_cache.snapshot,
                )
            })
            .unwrap_or_default();
        let committed_source = self
            .compositor
            .raster_cache
            .snapshot
            .source_fingerprint
            .clone()
            .unwrap_or_default();
        let observed = self
            .compositor
            .raster_cache
            .observed_consistency(&committed_source, viewport_fingerprint);
        self.compositor.raster_budget_readiness =
            crate::view::compositor::raster_cache::evaluate_shadow_budget_readiness(
                observed,
                self.compositor.prospective_raster_plan.as_ref(),
                &self.compositor.raster_plan_parity,
            );

        if self.debug_options.trace_reuse_path {
            let snapshot = &self.compositor.raster_cache.snapshot;
            record_debug_style_promotion(format!(
                "raster-cache epoch={} status={:?} last_attempted_frame={} last_committed_frame={:?} entries={} declared_bytes={} resident_bytes={} retiring_bytes={} stale_resident_bytes={} descriptor_mismatches={} promotion_declared_bytes={} promotion_resident_bytes={} promotion_retiring_bytes={} known_non_promotion_declared_bytes={} unidentified_declared_bytes={} unidentified_resident_bytes={} declared_coverage={:.3} resident_coverage={:.3} retiring_coverage={:.3} promotion_association_coverage={:.3} all_costs_exact={} all_costs_budget_usable={} unknown_keys={:?} validation_errors={:?} plan_subset_checked={} actual_is_compatible_subset={} incompatibility_count={} planned_not_declared_count={} total_budget_ready={}",
                snapshot.epoch,
                snapshot.observation_status,
                snapshot.last_attempted_frame,
                snapshot.last_committed_frame,
                snapshot.entries.len(),
                snapshot.declared_bytes,
                snapshot.resident_bytes,
                snapshot.retiring_bytes,
                snapshot.stale_resident_bytes,
                snapshot.descriptor_mismatch_count,
                snapshot.promotion_declared_bytes,
                snapshot.promotion_resident_bytes,
                snapshot.promotion_retiring_bytes,
                snapshot.known_non_promotion_declared_bytes,
                snapshot.unidentified_declared_bytes,
                snapshot.unidentified_resident_bytes,
                snapshot.association_coverage,
                snapshot.resident_association_coverage,
                snapshot.retiring_association_coverage,
                snapshot.promotion_association_coverage,
                snapshot.all_costs_exact,
                snapshot.all_costs_budget_usable,
                snapshot.unknown_declared_keys,
                snapshot.validation_errors,
                self.compositor.raster_plan_parity.checked,
                self.compositor
                    .raster_plan_parity
                    .actual_is_compatible_subset,
                self.compositor.raster_plan_parity.incompatibilities.len(),
                self.compositor
                    .raster_plan_parity
                    .planned_not_declared
                    .len(),
                self.compositor.raster_budget_readiness.ready,
            ));
            for incompatibility in &self.compositor.raster_plan_parity.incompatibilities {
                match incompatibility {
                    crate::view::compositor::raster_cache::RasterParityError::UnexpectedActual(key) => {
                        let actual = snapshot.entries.get(key).and_then(|entry| entry.declared);
                        record_debug_style_promotion(format!(
                            "plan-actual-incompatibility kind=unexpected_actual key={key:?} actual={actual:?}"
                        ));
                    }
                    crate::view::compositor::raster_cache::RasterParityError::DescriptorMismatch(key) => {
                        let planned = self
                            .compositor
                            .prospective_raster_plan
                            .as_ref()
                            .and_then(|plan| plan.resources.get(key))
                            .map(|resource| resource.descriptor);
                        let actual = snapshot.entries.get(key).and_then(|entry| entry.declared);
                        record_debug_style_promotion(format!(
                            "plan-actual-incompatibility kind=descriptor_mismatch key={key:?} planned={planned:?} actual={actual:?}"
                        ));
                    }
                }
            }
        }
    }

    pub(super) fn maybe_mark_raster_cache_observation_failed(&mut self, compiled: bool) {
        if !cfg!(test) && !self.debug_options.trace_reuse_path {
            return;
        }
        let status = if compiled {
            crate::view::compositor::raster_cache::RasterObservationStatus::StaleExecuteFailure
        } else {
            crate::view::compositor::raster_cache::RasterObservationStatus::StaleCompileFailure
        };
        self.compositor
            .raster_cache
            .mark_failed_observation(self.frame.frame_number, status);
        if self.debug_options.trace_reuse_path {
            self.compositor
                .shadow_rollout_safety
                .reset_after_observation_failure(status);
        }
        if self.debug_options.trace_reuse_path {
            let snapshot = &self.compositor.raster_cache.snapshot;
            record_debug_style_promotion(format!(
                "raster-cache epoch={} status={:?} last_attempted_frame={} last_committed_frame={:?} stale_snapshot=true",
                snapshot.epoch,
                snapshot.observation_status,
                snapshot.last_attempted_frame,
                snapshot.last_committed_frame,
            ));
        }
    }

    pub(super) fn apply_promotion_runtime(
        &self,
        ctx: &mut crate::view::base_component::UiBuildContext,
    ) {
        let updates = &self.compositor.promoted_layer_updates;
        let mut promoted_update_kinds =
            FxHashMap::with_capacity_and_hasher(updates.len(), Default::default());
        let mut promoted_composition_update_kinds =
            FxHashMap::with_capacity_and_hasher(updates.len(), Default::default());
        for update in updates {
            promoted_update_kinds.insert(update.node_id, update.kind);
            promoted_composition_update_kinds.insert(update.node_id, update.composition_kind);
        }
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
        let composite_bounds =
            crate::view::viewport::scene_helpers::paint_snapped_promotion_composite_bounds(
                root,
                root.promotion_composite_bounds(),
                ctx.paint_offset(),
            );
        let opacity = crate::view::base_component::promoted_composite_opacity(root);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::base_component::Element;
    use crate::view::node_arena::Node;

    #[test]
    fn consecutive_viewport_syncs_advance_epoch_without_mutating_promotion_runtime() {
        let mut viewport = Viewport::new();
        viewport.set_size(320, 240);
        let root = viewport
            .scene
            .node_arena
            .insert(Node::new(Box::new(Element::new_with_id(
                1, 0.0, 0.0, 120.0, 80.0,
            ))));
        viewport.scene.node_arena.push_root(root);
        viewport.scene.ui_root_keys.push(root);
        viewport.update_promotion_state();

        let decisions = viewport.compositor.promotion_state.decisions.clone();
        let promoted_node_ids = viewport
            .compositor
            .promotion_state
            .promoted_node_ids
            .clone();
        let estimated_bytes = viewport
            .compositor
            .promotion_state
            .total_estimated_memory_bytes;
        let updates = viewport.compositor.promoted_layer_updates.clone();

        viewport.sync_compositor_property_trees();
        let first_epoch = viewport.compositor_property_tree_epoch();
        viewport.sync_compositor_property_trees();

        assert_eq!(first_epoch, 1);
        assert_eq!(viewport.compositor_property_tree_epoch(), 2);
        assert_eq!(viewport.compositor.promotion_state.decisions, decisions);
        assert_eq!(
            viewport.compositor.promotion_state.promoted_node_ids,
            promoted_node_ids
        );
        assert_eq!(
            viewport
                .compositor
                .promotion_state
                .total_estimated_memory_bytes,
            estimated_bytes
        );
        assert_eq!(viewport.compositor.promoted_layer_updates, updates);
    }

    #[test]
    fn property_tree_sync_does_not_run_the_paint_generation_traversal() {
        let mut viewport = Viewport::new();
        let root = viewport
            .scene
            .node_arena
            .insert(Node::new(Box::new(Element::new_with_id(
                1, 0.0, 0.0, 120.0, 80.0,
            ))));
        viewport.scene.node_arena.push_root(root);
        viewport.scene.ui_root_keys.push(root);

        viewport.sync_compositor_property_trees();
        assert_eq!(viewport.compositor_property_tree_epoch(), 1);
        assert_eq!(viewport.compositor.paint_generations.epoch(), 0);

        viewport.update_promotion_state();
        assert_eq!(viewport.compositor.paint_generations.epoch(), 1);
    }

    #[test]
    fn shadow_layer_sync_does_not_mutate_authoritative_promotion_runtime() {
        let mut viewport = Viewport::new();
        viewport.set_size(320, 240);
        let root = viewport
            .scene
            .node_arena
            .insert(Node::new(Box::new(Element::new_with_id(
                1, 0.0, 0.0, 120.0, 80.0,
            ))));
        viewport.scene.node_arena.push_root(root);
        viewport.scene.ui_root_keys.push(root);
        viewport.sync_compositor_property_trees();
        viewport.update_promotion_state();

        let decisions = viewport.compositor.promotion_state.decisions.clone();
        let promoted_node_ids = viewport
            .compositor
            .promotion_state
            .promoted_node_ids
            .clone();
        let estimated_memory_bytes = viewport
            .compositor
            .promotion_state
            .total_estimated_memory_bytes;
        let updates = viewport.compositor.promoted_layer_updates.clone();
        let base_signatures = viewport.compositor.promoted_base_signatures.clone();
        let composition_signatures = viewport.compositor.promoted_composition_signatures.clone();
        let base_generations = viewport.compositor.promoted_base_generations.clone();
        let composition_generations = viewport.compositor.promoted_composition_generations.clone();
        let cooldown = viewport.compositor.promoted_reuse_cooldown_frames;

        viewport.sync_shadow_layer_tree();

        assert_eq!(viewport.compositor.promotion_state.decisions, decisions);
        assert_eq!(
            viewport.compositor.promotion_state.promoted_node_ids,
            promoted_node_ids
        );
        assert_eq!(
            viewport
                .compositor
                .promotion_state
                .total_estimated_memory_bytes,
            estimated_memory_bytes
        );
        assert_eq!(viewport.compositor.promoted_layer_updates, updates);
        assert_eq!(
            viewport.compositor.promoted_base_signatures,
            base_signatures
        );
        assert_eq!(
            viewport.compositor.promoted_composition_signatures,
            composition_signatures
        );
        assert_eq!(
            viewport.compositor.promoted_base_generations,
            base_generations
        );
        assert_eq!(
            viewport.compositor.promoted_composition_generations,
            composition_generations
        );
        assert_eq!(viewport.compositor.promoted_reuse_cooldown_frames, cooldown);
        assert!(viewport.compositor.shadow_layer_tree.epoch > 0);
    }

    #[test]
    fn production_shadow_gate_keeps_epoch_idle_until_trace_is_enabled() {
        let mut viewport = Viewport::new();
        viewport.set_size(320, 240);
        let root = viewport
            .scene
            .node_arena
            .insert(Node::new(Box::new(Element::new_with_id(
                1, 0.0, 0.0, 120.0, 80.0,
            ))));
        viewport.scene.node_arena.push_root(root);
        viewport.scene.ui_root_keys.push(root);
        viewport.sync_compositor_property_trees();
        viewport.update_promotion_state();

        viewport.maybe_sync_shadow_layer_tree_inner(false);
        assert_eq!(viewport.compositor.shadow_layer_tree.epoch, 0);

        viewport.debug_options.trace_reuse_path = true;
        viewport.maybe_sync_shadow_layer_tree_inner(false);
        assert_eq!(viewport.compositor.shadow_layer_tree.epoch, 1);
    }

    #[test]
    fn raster_budget_readiness_gate_is_shadow_only_and_trace_off_is_noop() {
        let mut viewport = Viewport::new();
        viewport.compositor.raster_budget_readiness =
            crate::view::compositor::raster_cache::ShadowRasterBudgetReadiness {
                observed: crate::view::compositor::raster_cache::ObservedSnapshotConsistency {
                    consistent: true,
                    fallback_reasons: Vec::new(),
                },
                prospective_ready: true,
                parity_ready: true,
                ready: true,
                prospective_fallback_reasons: Vec::new(),
            };
        let promotion_before = viewport.compositor.promotion_state.clone();

        viewport.maybe_evaluate_raster_budget_readiness_inner(false);
        assert!(viewport.compositor.raster_budget_readiness.ready);
        assert_eq!(
            viewport.compositor.promotion_state.decisions,
            promotion_before.decisions
        );
        assert_eq!(
            viewport.compositor.promotion_state.promoted_node_ids,
            promotion_before.promoted_node_ids
        );

        viewport.maybe_evaluate_raster_budget_readiness_inner(true);
        assert!(!viewport.compositor.raster_budget_readiness.ready);
        assert_eq!(
            viewport
                .compositor
                .raster_budget_readiness
                .observed
                .fallback_reasons,
            vec![crate::view::compositor::raster_cache::RasterBudgetFallbackReason::NeverObserved]
        );
        assert_eq!(
            viewport.compositor.promotion_state.decisions,
            promotion_before.decisions
        );
        assert_eq!(
            viewport.compositor.promotion_state.promoted_node_ids,
            promotion_before.promoted_node_ids
        );
    }

    #[test]
    fn three_successful_previous_frames_advance_rollout_streak_before_current_parity_pending() {
        let mut viewport = Viewport::new();
        viewport.set_size(320, 240);
        viewport.debug_options.trace_reuse_path = true;
        let root = viewport
            .scene
            .node_arena
            .insert(Node::new(Box::new(Element::new_with_id(
                1, 0.0, 0.0, 120.0, 80.0,
            ))));
        viewport.scene.node_arena.push_root(root);
        viewport.scene.ui_root_keys.push(root);
        viewport.sync_compositor_property_trees();

        for expected_streak in 1..=3 {
            viewport.compositor.raster_budget_readiness =
                crate::view::compositor::raster_cache::ShadowRasterBudgetReadiness {
                    observed: crate::view::compositor::raster_cache::ObservedSnapshotConsistency {
                        consistent: true,
                        fallback_reasons: Vec::new(),
                    },
                    prospective_ready: true,
                    parity_ready: true,
                    ready: true,
                    prospective_fallback_reasons: Vec::new(),
                };
            viewport.update_promotion_state();
            assert_eq!(
                viewport
                    .compositor
                    .shadow_rollout_safety
                    .consecutive_full_ready_frames,
                expected_streak
            );

            // The current frame now installs a new plan and marks parity
            // pending. This must not retroactively erase the previous fully
            // observed frame that was finalized above.
            viewport.maybe_plan_prospective_raster_resources();
            assert!(!viewport.compositor.raster_plan_parity.checked);
            assert_eq!(
                viewport
                    .compositor
                    .shadow_rollout_safety
                    .consecutive_full_ready_frames,
                expected_streak
            );
        }
    }
}
