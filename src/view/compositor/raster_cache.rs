//! Observational metadata for persistent raster resources.
//!
//! This module deliberately owns no GPU handles and never touches, evicts, or
//! destroys render targets. The offscreen render-target pool remains the sole
//! authority for physical resource lifetime.

#![allow(dead_code)]

use rustc_hash::{FxHashMap, FxHashSet};

use crate::view::base_component::{
    Element, persistent_depth_stencil_stable_key, persistent_target_texture_descriptors,
    promoted_clip_mask_stable_key, promoted_final_layer_stable_key, promoted_layer_stable_key,
    root_effect_stable_key, texture_desc_for_logical_bounds, transformed_layer_stable_key,
};
use crate::view::frame_graph::{PersistentTextureKey, RetainedTextureRole, TextureDesc};
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::promotion::PromotionState;
use crate::view::raster_cost::{CostConfidence, DescriptorPayloadBytes, raster_payload_bytes};
use crate::view::render_pass::render_target::PersistentRenderTargetObservation;

use super::layer_tree::{LayerId, LayerTree};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RasterCacheKey(pub(crate) PersistentTextureKey);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RasterResourceRole {
    BaseColor,
    BaseDepthStencil,
    FinalColor,
    FinalDepthStencil,
    ClipMaskColor,
    ClipMaskDepthStencil,
    TransformedColor,
    TransformedDepthStencil,
    RootEffectColor,
    RootEffectDepthStencil,
    UnknownPersistent,
}

impl RasterResourceRole {
    fn is_promotion(self) -> bool {
        matches!(
            self,
            Self::BaseColor
                | Self::BaseDepthStencil
                | Self::FinalColor
                | Self::FinalDepthStencil
                | Self::ClipMaskColor
                | Self::ClipMaskDepthStencil
        )
    }

    fn is_known(self) -> bool {
        !matches!(self, Self::UnknownPersistent)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LayerResourceAssociation {
    pub(crate) layer: Option<LayerId>,
    pub(crate) owner: Option<NodeKey>,
    pub(crate) owner_stable_id: Option<u64>,
    pub(crate) role: RasterResourceRole,
}

impl LayerResourceAssociation {
    fn unknown() -> Self {
        Self {
            layer: None,
            owner: None,
            owner_stable_id: None,
            role: RasterResourceRole::UnknownPersistent,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RasterDescriptor {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) dimension: wgpu::TextureDimension,
    pub(crate) sample_count: u32,
}

impl From<&TextureDesc> for RasterDescriptor {
    fn from(desc: &TextureDesc) -> Self {
        Self {
            width: desc.width().max(1),
            height: desc.height().max(1),
            format: desc.format(),
            dimension: desc.dimension(),
            sample_count: desc.sample_count().max(1),
        }
    }
}

impl From<PersistentRenderTargetObservation> for RasterDescriptor {
    fn from(observation: PersistentRenderTargetObservation) -> Self {
        Self {
            width: observation.width.max(1),
            height: observation.height.max(1),
            format: observation.format,
            dimension: observation.dimension,
            sample_count: observation.sample_count.max(1),
        }
    }
}

pub(crate) fn descriptor_payload_bytes(desc: RasterDescriptor) -> DescriptorPayloadBytes {
    raster_payload_bytes(desc.width, desc.height, desc.format, desc.sample_count)
}

pub(crate) fn plan_prospective_raster_resources(
    arena: &NodeArena,
    ui_root_keys: &[NodeKey],
    tentative_promoted_node_ids: &FxHashSet<u64>,
    viewport: ViewportRasterFingerprint,
) -> ProspectiveRasterPlan {
    #[derive(Clone, Copy, Default)]
    struct NodePlanningFacts {
        has_composited_promoted_descendant: bool,
        has_direct_deferred_descendant: bool,
        transform_subtree_bounds: Option<crate::view::base_component::PromotionCompositeBounds>,
    }

    let mut plan = ProspectiveRasterPlan {
        confidence: CostConfidence::Exact,
        ..ProspectiveRasterPlan::default()
    };
    let mut visit_state = FxHashMap::<NodeKey, bool>::default();
    let mut facts = FxHashMap::<NodeKey, NodePlanningFacts>::default();
    let mut postorder = Vec::new();
    let mut reachable_by_stable_id = FxHashMap::<u64, NodeKey>::default();
    let mut pending = ui_root_keys
        .iter()
        .rev()
        .copied()
        .map(|key| (key, false))
        .collect::<Vec<_>>();
    while let Some((owner, expanded)) = pending.pop() {
        let Some(node) = arena.get(owner) else {
            continue;
        };
        if expanded {
            if visit_state.get(&owner).copied() == Some(true) {
                continue;
            }
            let mut node_facts = NodePlanningFacts::default();
            for &child_key in node.element.children() {
                let Some(child) = arena.get(child_key) else {
                    continue;
                };
                let deferred = child.element.is_deferred_to_root_viewport_render();
                node_facts.has_direct_deferred_descendant |= deferred;
                if !deferred {
                    node_facts.has_composited_promoted_descendant |= tentative_promoted_node_ids
                        .contains(&child.element.stable_id())
                        || facts
                            .get(&child_key)
                            .is_some_and(|facts| facts.has_composited_promoted_descendant);
                }
            }
            node_facts.transform_subtree_bounds = node
                .element
                .retained_transform_raster_seed_bounds()
                .map(|mut bounds| {
                    for &child_key in node.element.children() {
                        let Some(child) = arena.get(child_key) else {
                            continue;
                        };
                        let child_bounds = facts
                            .get(&child_key)
                            .and_then(|facts| facts.transform_subtree_bounds)
                            .unwrap_or_else(|| child.element.promotion_composite_bounds());
                        bounds = Element::union_promotion_bounds(bounds, child_bounds);
                    }
                    bounds
                });
            facts.insert(owner, node_facts);
            reachable_by_stable_id.insert(node.element.stable_id(), owner);
            postorder.push(owner);
            plan.planner_node_visits = plan.planner_node_visits.saturating_add(1);
            visit_state.insert(owner, true);
            continue;
        }
        if visit_state.contains_key(&owner) {
            continue;
        }
        visit_state.insert(owner, false);
        pending.push((owner, true));
        for &child_key in node.element.children().iter().rev() {
            if !visit_state.contains_key(&child_key) {
                pending.push((child_key, false));
            }
        }
    }

    for &stable_id in tentative_promoted_node_ids {
        if !reachable_by_stable_id.contains_key(&stable_id) {
            plan.errors
                .push(ProspectivePlanError::MissingPromotedNode(stable_id));
        }
    }

    for owner in postorder {
        let Some(node) = arena.get(owner) else {
            continue;
        };
        let stable_id = node.element.stable_id();
        let node_facts = facts.get(&owner).copied().unwrap_or_default();
        if tentative_promoted_node_ids.contains(&stable_id) {
            insert_planned_target_pair(
                &mut plan,
                promoted_layer_stable_key(stable_id),
                node.element.promotion_composite_bounds(),
                RasterResourceRole::BaseColor,
                RasterResourceRole::BaseDepthStencil,
                owner,
                viewport,
            );
            if node_facts.has_composited_promoted_descendant {
                insert_planned_target_pair(
                    &mut plan,
                    promoted_final_layer_stable_key(stable_id),
                    node.element.promotion_composite_bounds(),
                    RasterResourceRole::FinalColor,
                    RasterResourceRole::FinalDepthStencil,
                    owner,
                    viewport,
                );
            }
        }
        if (node_facts.has_composited_promoted_descendant
            || node_facts.has_direct_deferred_descendant)
            && node.element.promotion_requires_mask_surface(arena)
        {
            insert_planned_target_pair(
                &mut plan,
                promoted_clip_mask_stable_key(stable_id),
                node.element.promotion_composite_bounds(),
                RasterResourceRole::ClipMaskColor,
                RasterResourceRole::ClipMaskDepthStencil,
                owner,
                viewport,
            );
        }
        if node.element.has_retained_transform_surface()
            && let Some(bounds) = node_facts.transform_subtree_bounds
        {
            insert_planned_target_pair(
                &mut plan,
                transformed_layer_stable_key(stable_id),
                bounds,
                RasterResourceRole::TransformedColor,
                RasterResourceRole::TransformedDepthStencil,
                owner,
                viewport,
            );
        }
    }
    plan
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProspectivePlanAccumulatorMetrics {
    pub(crate) full_planner_calls: usize,
    pub(crate) full_planner_node_visits: usize,
    pub(crate) precompute_node_visits: usize,
    pub(crate) accumulator_try_calls: usize,
    pub(crate) accumulator_ancestor_visits: usize,
}

#[derive(Default)]
struct PendingProspectiveDelta {
    stable_id: u64,
    ancestors: Vec<NodeKey>,
    resources: Vec<PlannedRasterResource>,
    planned_promotion_bytes: u64,
    incremental_resident_bytes: u64,
}

pub(crate) struct ProspectivePlanAccumulator<'a> {
    arena: &'a NodeArena,
    viewport: ViewportRasterFingerprint,
    snapshot: &'a RasterCacheSnapshot,
    reachable_by_stable_id: FxHashMap<u64, NodeKey>,
    promoted: FxHashSet<u64>,
    promoted_descendant_refcounts: FxHashMap<NodeKey, usize>,
    plan: ProspectiveRasterPlan,
    pending: Option<PendingProspectiveDelta>,
    current_projection: ShadowBudgetProjection,
    metrics: ProspectivePlanAccumulatorMetrics,
}

impl<'a> ProspectivePlanAccumulator<'a> {
    pub(crate) fn new(
        arena: &'a NodeArena,
        ui_root_keys: &[NodeKey],
        viewport: ViewportRasterFingerprint,
        snapshot: &'a RasterCacheSnapshot,
    ) -> Self {
        let plan =
            plan_prospective_raster_resources(arena, ui_root_keys, &FxHashSet::default(), viewport);
        let mut reachable_by_stable_id = FxHashMap::default();
        let mut visited = FxHashSet::default();
        let mut pending = ui_root_keys.to_vec();
        while let Some(key) = pending.pop() {
            if !visited.insert(key) {
                continue;
            }
            let Some(node) = arena.get(key) else {
                continue;
            };
            reachable_by_stable_id.insert(node.element.stable_id(), key);
            pending.extend(node.element.children().iter().copied());
        }
        let metrics = ProspectivePlanAccumulatorMetrics {
            full_planner_calls: 1,
            full_planner_node_visits: plan.planner_node_visits,
            precompute_node_visits: visited.len(),
            ..ProspectivePlanAccumulatorMetrics::default()
        };
        let current_projection = project_shadow_promotion_peak(&plan, snapshot);
        Self {
            arena,
            viewport,
            snapshot,
            reachable_by_stable_id,
            promoted: FxHashSet::default(),
            promoted_descendant_refcounts: FxHashMap::default(),
            plan,
            pending: None,
            current_projection,
            metrics,
        }
    }

    pub(crate) fn project_tentative(
        &mut self,
        tentative: &FxHashSet<u64>,
    ) -> ShadowBudgetProjection {
        self.resolve_pending(tentative);
        self.metrics.accumulator_try_calls = self.metrics.accumulator_try_calls.saturating_add(1);
        let additions = tentative
            .difference(&self.promoted)
            .copied()
            .collect::<Vec<_>>();
        if additions.len() != 1 {
            return ShadowBudgetProjection {
                projected_peak_bytes: u64::MAX,
                ..self.current_projection
            };
        }
        if !self.plan.ready() || !self.reachable_by_stable_id.contains_key(&additions[0]) {
            return ShadowBudgetProjection {
                projected_peak_bytes: u64::MAX,
                ..self.current_projection
            };
        }
        let delta = self.build_delta(additions[0]);
        let mut projection = self.current_projection;
        if delta
            .resources
            .iter()
            .any(|resource| !resource.cost.confidence.budget_usable())
        {
            projection.projected_peak_bytes = u64::MAX;
            self.pending = Some(delta);
            return projection;
        }
        projection.planned_promotion_bytes = projection
            .planned_promotion_bytes
            .saturating_add(delta.planned_promotion_bytes);
        projection.incremental_resident_bytes = projection
            .incremental_resident_bytes
            .saturating_add(delta.incremental_resident_bytes);
        projection.projected_peak_bytes = projection
            .current_promotion_resident_bytes
            .saturating_add(projection.incremental_resident_bytes);
        self.pending = Some(delta);
        projection
    }

    pub(crate) fn finish(&mut self, final_set: &FxHashSet<u64>) {
        self.resolve_pending(final_set);
    }

    pub(crate) fn plan(&self) -> &ProspectiveRasterPlan {
        &self.plan
    }

    pub(crate) fn metrics(&self) -> ProspectivePlanAccumulatorMetrics {
        self.metrics
    }

    pub(crate) fn current_projection(&self) -> ShadowBudgetProjection {
        self.current_projection
    }

    fn resolve_pending(&mut self, accepted_set: &FxHashSet<u64>) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        if !accepted_set.contains(&pending.stable_id) {
            return;
        }
        self.promoted.insert(pending.stable_id);
        for ancestor in pending.ancestors {
            *self
                .promoted_descendant_refcounts
                .entry(ancestor)
                .or_default() += 1;
        }
        for resource in pending.resources {
            if self
                .plan
                .resources
                .insert(resource.key, resource.clone())
                .is_none()
            {
                self.plan.total_bytes = self.plan.total_bytes.saturating_add(resource.cost.bytes);
                self.plan.confidence = self.plan.confidence.combine(resource.cost.confidence);
            }
        }
        self.current_projection.planned_promotion_bytes = self
            .current_projection
            .planned_promotion_bytes
            .saturating_add(pending.planned_promotion_bytes);
        self.current_projection.incremental_resident_bytes = self
            .current_projection
            .incremental_resident_bytes
            .saturating_add(pending.incremental_resident_bytes);
        self.current_projection.projected_peak_bytes = self
            .current_projection
            .current_promotion_resident_bytes
            .saturating_add(self.current_projection.incremental_resident_bytes);
    }

    fn build_delta(&mut self, stable_id: u64) -> PendingProspectiveDelta {
        let Some(&owner) = self.reachable_by_stable_id.get(&stable_id) else {
            return PendingProspectiveDelta {
                stable_id,
                ..PendingProspectiveDelta::default()
            };
        };
        let Some(node) = self.arena.get(owner) else {
            return PendingProspectiveDelta {
                stable_id,
                ..PendingProspectiveDelta::default()
            };
        };
        let mut resources = Vec::new();
        self.append_pair_delta(
            &mut resources,
            promoted_layer_stable_key(stable_id),
            node.element.promotion_composite_bounds(),
            RasterResourceRole::BaseColor,
            RasterResourceRole::BaseDepthStencil,
            owner,
        );
        if self
            .promoted_descendant_refcounts
            .get(&owner)
            .copied()
            .unwrap_or(0)
            > 0
        {
            self.append_pair_delta(
                &mut resources,
                promoted_final_layer_stable_key(stable_id),
                node.element.promotion_composite_bounds(),
                RasterResourceRole::FinalColor,
                RasterResourceRole::FinalDepthStencil,
                owner,
            );
        }

        let mut ancestors = Vec::new();
        let mut child = owner;
        while let Some(parent) = self.arena.parent_of(child) {
            let Some(child_node) = self.arena.get(child) else {
                break;
            };
            if child_node.element.is_deferred_to_root_viewport_render() {
                break;
            }
            self.metrics.accumulator_ancestor_visits =
                self.metrics.accumulator_ancestor_visits.saturating_add(1);
            let old_count = self
                .promoted_descendant_refcounts
                .get(&parent)
                .copied()
                .unwrap_or(0);
            // Admissions are monotonic during one evaluation. If this node
            // already has a promoted descendant, the same earlier propagation
            // has already marked every ancestor above it, so no remaining
            // resource can transition from absent to present.
            if old_count > 0 {
                break;
            }
            ancestors.push(parent);
            let Some(parent_node) = self.arena.get(parent) else {
                break;
            };
            let parent_id = parent_node.element.stable_id();
            if self.promoted.contains(&parent_id) {
                self.append_pair_delta(
                    &mut resources,
                    promoted_final_layer_stable_key(parent_id),
                    parent_node.element.promotion_composite_bounds(),
                    RasterResourceRole::FinalColor,
                    RasterResourceRole::FinalDepthStencil,
                    parent,
                );
            }
            if parent_node
                .element
                .promotion_requires_mask_surface(self.arena)
            {
                self.append_pair_delta(
                    &mut resources,
                    promoted_clip_mask_stable_key(parent_id),
                    parent_node.element.promotion_composite_bounds(),
                    RasterResourceRole::ClipMaskColor,
                    RasterResourceRole::ClipMaskDepthStencil,
                    parent,
                );
            }
            child = parent;
        }
        let planned_promotion_bytes = resources
            .iter()
            .filter(|resource| resource.role.is_promotion())
            .map(|resource| resource.cost.bytes)
            .sum();
        let incremental_resident_bytes = resources
            .iter()
            .filter(|resource| resource.role.is_promotion())
            .filter(|resource| {
                !self
                    .snapshot
                    .entries
                    .get(&resource.key)
                    .and_then(|entry| entry.resident)
                    .is_some_and(|resident| resident == resource.descriptor)
            })
            .map(|resource| resource.cost.bytes)
            .sum();
        PendingProspectiveDelta {
            stable_id,
            ancestors,
            resources,
            planned_promotion_bytes,
            incremental_resident_bytes,
        }
    }

    fn append_pair_delta(
        &self,
        resources: &mut Vec<PlannedRasterResource>,
        color_key: PersistentTextureKey,
        bounds: crate::view::base_component::PromotionCompositeBounds,
        color_role: RasterResourceRole,
        depth_role: RasterResourceRole,
        owner: NodeKey,
    ) {
        let mut pair = ProspectiveRasterPlan {
            confidence: CostConfidence::Exact,
            ..ProspectiveRasterPlan::default()
        };
        insert_planned_target_pair(
            &mut pair,
            color_key,
            bounds,
            color_role,
            depth_role,
            owner,
            self.viewport,
        );
        for resource in pair.resources.into_values() {
            if !self.plan.resources.contains_key(&resource.key)
                && !resources
                    .iter()
                    .any(|existing| existing.key == resource.key)
            {
                resources.push(resource);
            }
        }
    }
}

fn insert_planned_target_pair(
    plan: &mut ProspectiveRasterPlan,
    color_key: PersistentTextureKey,
    bounds: crate::view::base_component::PromotionCompositeBounds,
    color_role: RasterResourceRole,
    depth_role: RasterResourceRole,
    owner: NodeKey,
    viewport: ViewportRasterFingerprint,
) {
    let color_desc = texture_desc_for_logical_bounds(
        bounds,
        f32::from_bits(viewport.scale_factor_bits),
        None,
        viewport.target_format,
    );
    let (color_desc, depth_desc) = persistent_target_texture_descriptors(color_desc, color_key);
    insert_planned_resource(plan, color_key, &color_desc, color_role, owner);
    let depth_key = persistent_depth_stencil_stable_key(color_key)
        .expect("planned retained color role must have depth counterpart");
    insert_planned_resource(plan, depth_key, &depth_desc, depth_role, owner);
}

fn insert_planned_resource(
    plan: &mut ProspectiveRasterPlan,
    key: PersistentTextureKey,
    desc: &TextureDesc,
    role: RasterResourceRole,
    owner: NodeKey,
) {
    let key = RasterCacheKey(key);
    let descriptor = RasterDescriptor::from(desc);
    let cost = descriptor_payload_bytes(descriptor);
    if !cost.confidence.budget_usable() {
        plan.errors.push(ProspectivePlanError::UnknownCost(key));
    }
    let resource = PlannedRasterResource {
        key,
        descriptor,
        role,
        owner,
        cost,
    };
    if plan.resources.insert(key, resource).is_some() {
        plan.errors
            .push(ProspectivePlanError::DuplicateResourceKey(key));
        return;
    }
    plan.total_bytes = plan.total_bytes.saturating_add(cost.bytes);
    plan.confidence = plan.confidence.combine(cost.confidence);
}

pub(crate) fn compare_plan_to_snapshot(
    plan: &ProspectiveRasterPlan,
    snapshot: &RasterCacheSnapshot,
) -> RasterPlanParity {
    let actual = snapshot
        .entries
        .iter()
        .filter_map(|(&key, entry)| {
            if matches!(
                key.0,
                PersistentTextureKey::Retained {
                    role: RetainedTextureRole::RootEffectColor
                        | RetainedTextureRole::RootEffectDepthStencil,
                    ..
                }
            ) {
                return None;
            }
            match key.0 {
                PersistentTextureKey::Retained { .. }
                | PersistentTextureKey::RetainedScrollContentTile { .. } => {
                    entry.declared.map(|desc| (key, desc))
                }
                PersistentTextureKey::Generic(_) => None,
            }
        })
        .collect::<FxHashMap<_, _>>();
    let mut incompatibilities = Vec::new();
    let mut planned_not_declared = Vec::new();
    for (&key, planned) in &plan.resources {
        match actual.get(&key) {
            None => planned_not_declared.push(key),
            Some(actual_desc) if *actual_desc != planned.descriptor => {
                incompatibilities.push(RasterParityError::DescriptorMismatch(key));
            }
            Some(_) => {}
        }
    }
    for &key in actual.keys() {
        if !plan.resources.contains_key(&key) {
            incompatibilities.push(RasterParityError::UnexpectedActual(key));
        }
    }
    incompatibilities.sort_by_key(|error| format!("{error:?}"));
    planned_not_declared.sort_by_key(|key| format!("{key:?}"));
    RasterPlanParity {
        checked: true,
        actual_is_compatible_subset: incompatibilities.is_empty(),
        incompatibilities,
        planned_not_declared,
    }
}

pub(crate) fn evaluate_shadow_budget_readiness(
    observed: ObservedSnapshotConsistency,
    plan: Option<&ProspectiveRasterPlan>,
    parity: &RasterPlanParity,
) -> ShadowRasterBudgetReadiness {
    let mut reasons = Vec::new();
    let prospective_ready = match plan {
        None => {
            reasons.push(ProspectiveReadinessFallbackReason::PlanNotBuilt);
            false
        }
        Some(plan) => {
            if !plan.errors.is_empty() {
                reasons.push(ProspectiveReadinessFallbackReason::PlannerErrors);
            }
            if !plan.confidence.budget_usable() {
                reasons.push(ProspectiveReadinessFallbackReason::UnknownCost);
            }
            plan.ready()
        }
    };
    let parity_ready = if !parity.checked {
        reasons.push(ProspectiveReadinessFallbackReason::ParityNotChecked);
        false
    } else if !parity.actual_is_compatible_subset {
        reasons.push(ProspectiveReadinessFallbackReason::ParityMismatch);
        false
    } else {
        true
    };
    ShadowRasterBudgetReadiness {
        ready: observed.consistent && prospective_ready && parity_ready,
        observed,
        prospective_ready,
        parity_ready,
        prospective_fallback_reasons: reasons,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ShadowBudgetProjection {
    pub(crate) planned_promotion_bytes: u64,
    pub(crate) current_promotion_resident_bytes: u64,
    pub(crate) promotion_retiring_bytes: u64,
    pub(crate) incremental_resident_bytes: u64,
    pub(crate) projected_peak_bytes: u64,
}

pub(crate) fn project_shadow_promotion_peak(
    plan: &ProspectiveRasterPlan,
    snapshot: &RasterCacheSnapshot,
) -> ShadowBudgetProjection {
    let mut projection = ShadowBudgetProjection {
        current_promotion_resident_bytes: snapshot.promotion_resident_bytes,
        promotion_retiring_bytes: snapshot.promotion_retiring_bytes,
        ..ShadowBudgetProjection::default()
    };
    for (&key, resource) in &plan.resources {
        if !resource.role.is_promotion() {
            continue;
        }
        projection.planned_promotion_bytes = projection
            .planned_promotion_bytes
            .saturating_add(resource.cost.bytes);
        let has_matching_resident = snapshot
            .entries
            .get(&key)
            .and_then(|entry| entry.resident)
            .is_some_and(|resident| resident == resource.descriptor);
        if !has_matching_resident {
            projection.incremental_resident_bytes = projection
                .incremental_resident_bytes
                .saturating_add(resource.cost.bytes);
        }
    }
    // `promotion_resident_bytes` already includes retiring residents. Keep
    // them in the peak until the pool actually reports their retirement;
    // adding `promotion_retiring_bytes` again would double-count the subset.
    projection.projected_peak_bytes = projection
        .current_promotion_resident_bytes
        .saturating_add(projection.incremental_resident_bytes);
    projection
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShadowEvaluationFallbackReason {
    ObservedSnapshotInconsistent,
    ProspectivePlanNotReady,
    PreviousParityNotReady,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShadowPromotionDecisionDiff {
    pub(crate) node_id: u64,
    pub(crate) legacy_should_promote: bool,
    pub(crate) shadow_should_promote: bool,
    pub(crate) legacy_budget_rejection: Option<crate::view::promotion::PromotionBudgetRejection>,
    pub(crate) shadow_budget_rejection: Option<crate::view::promotion::PromotionBudgetRejection>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ShadowPromotionEvaluation {
    pub(crate) used_observed_cost: bool,
    pub(crate) fallback_reasons: Vec<ShadowEvaluationFallbackReason>,
    pub(crate) state: PromotionState,
    pub(crate) final_projection: ShadowBudgetProjection,
    pub(crate) decision_diffs: Vec<ShadowPromotionDecisionDiff>,
    pub(crate) accumulator_metrics: ProspectivePlanAccumulatorMetrics,
    pub(crate) policy_transitions: Vec<crate::view::promotion::ShadowPolicyTransition>,
}

pub(crate) fn fallback_shadow_promotion_evaluation(
    legacy: &PromotionState,
    fallback_reasons: Vec<ShadowEvaluationFallbackReason>,
) -> ShadowPromotionEvaluation {
    ShadowPromotionEvaluation {
        used_observed_cost: false,
        fallback_reasons,
        state: legacy.clone(),
        final_projection: ShadowBudgetProjection::default(),
        decision_diffs: Vec::new(),
        accumulator_metrics: ProspectivePlanAccumulatorMetrics::default(),
        policy_transitions: Vec::new(),
    }
}

pub(crate) fn diff_shadow_promotion_decisions(
    legacy: &PromotionState,
    shadow: &PromotionState,
) -> Vec<ShadowPromotionDecisionDiff> {
    let shadow_by_id = shadow
        .decisions
        .iter()
        .map(|decision| (decision.node_id, decision))
        .collect::<FxHashMap<_, _>>();
    legacy
        .decisions
        .iter()
        .filter_map(|legacy_decision| {
            let shadow_decision = shadow_by_id.get(&legacy_decision.node_id)?;
            (legacy_decision.should_promote != shadow_decision.should_promote
                || legacy_decision.budget_rejection != shadow_decision.budget_rejection)
                .then_some(ShadowPromotionDecisionDiff {
                    node_id: legacy_decision.node_id,
                    legacy_should_promote: legacy_decision.should_promote,
                    shadow_should_promote: shadow_decision.should_promote,
                    legacy_budget_rejection: legacy_decision.budget_rejection,
                    shadow_budget_rejection: shadow_decision.budget_rejection,
                })
        })
        .collect()
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ShadowRolloutSafetySnapshot {
    pub(crate) observed_consistent: bool,
    pub(crate) prospective_ready: bool,
    pub(crate) parity_compatible: bool,
    pub(crate) full_ready: bool,
    pub(crate) observed_fallback_reasons: Vec<RasterBudgetFallbackReason>,
    pub(crate) prospective_fallback_reasons: Vec<ProspectiveReadinessFallbackReason>,
    pub(crate) legacy_promoted_count: usize,
    pub(crate) shadow_promoted_count: usize,
    pub(crate) decision_diff_count: usize,
    pub(crate) admit_count: usize,
    pub(crate) retain_count: usize,
    pub(crate) drop_count: usize,
    pub(crate) reject_count: usize,
    pub(crate) planned_logical_bytes: u64,
    pub(crate) resident_bytes: u64,
    pub(crate) retiring_bytes: u64,
    pub(crate) incremental_bytes: u64,
    pub(crate) projected_peak_bytes: u64,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ShadowRolloutSafetyState {
    pub(crate) consecutive_observed_consistent_frames: u64,
    pub(crate) consecutive_prospective_ready_frames: u64,
    pub(crate) consecutive_parity_compatible_frames: u64,
    pub(crate) consecutive_full_ready_frames: u64,
    pub(crate) last: ShadowRolloutSafetySnapshot,
}

impl ShadowRolloutSafetyState {
    pub(crate) fn observe_if_enabled(
        &mut self,
        enabled: bool,
        readiness: &ShadowRasterBudgetReadiness,
        legacy: &PromotionState,
        shadow: &ShadowPromotionEvaluation,
    ) {
        if enabled {
            self.observe(readiness, legacy, shadow);
        }
    }

    pub(crate) fn observe(
        &mut self,
        readiness: &ShadowRasterBudgetReadiness,
        legacy: &PromotionState,
        shadow: &ShadowPromotionEvaluation,
    ) {
        let bump_or_reset = |counter: &mut u64, ready: bool| {
            *counter = if ready { counter.saturating_add(1) } else { 0 };
        };
        bump_or_reset(
            &mut self.consecutive_observed_consistent_frames,
            readiness.observed.consistent,
        );
        bump_or_reset(
            &mut self.consecutive_prospective_ready_frames,
            readiness.prospective_ready,
        );
        bump_or_reset(
            &mut self.consecutive_parity_compatible_frames,
            readiness.parity_ready,
        );
        bump_or_reset(&mut self.consecutive_full_ready_frames, readiness.ready);
        let count_transition = |kind| {
            shadow
                .policy_transitions
                .iter()
                .filter(|transition| transition.kind == kind)
                .count()
        };
        self.last = ShadowRolloutSafetySnapshot {
            observed_consistent: readiness.observed.consistent,
            prospective_ready: readiness.prospective_ready,
            parity_compatible: readiness.parity_ready,
            full_ready: readiness.ready,
            observed_fallback_reasons: readiness.observed.fallback_reasons.clone(),
            prospective_fallback_reasons: readiness.prospective_fallback_reasons.clone(),
            legacy_promoted_count: legacy.promoted_node_ids.len(),
            shadow_promoted_count: shadow.state.promoted_node_ids.len(),
            decision_diff_count: shadow.decision_diffs.len(),
            admit_count: count_transition(
                crate::view::promotion::ShadowPolicyTransitionKind::Admit,
            ),
            retain_count: count_transition(
                crate::view::promotion::ShadowPolicyTransitionKind::Retain,
            ),
            drop_count: count_transition(crate::view::promotion::ShadowPolicyTransitionKind::Drop),
            reject_count: count_transition(
                crate::view::promotion::ShadowPolicyTransitionKind::Reject,
            ),
            planned_logical_bytes: shadow.final_projection.planned_promotion_bytes,
            resident_bytes: shadow.final_projection.current_promotion_resident_bytes,
            retiring_bytes: shadow.final_projection.promotion_retiring_bytes,
            incremental_bytes: shadow.final_projection.incremental_resident_bytes,
            projected_peak_bytes: shadow.final_projection.projected_peak_bytes,
        };
    }

    pub(crate) fn reset_after_observation_failure(&mut self, status: RasterObservationStatus) {
        self.consecutive_observed_consistent_frames = 0;
        self.consecutive_parity_compatible_frames = 0;
        self.consecutive_full_ready_frames = 0;
        self.last.observed_consistent = false;
        self.last.parity_compatible = false;
        self.last.full_ready = false;
        self.last.observed_fallback_reasons =
            vec![RasterBudgetFallbackReason::StaleObservation(status)];
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ShadowRolloutEligibilityConfig {
    pub(crate) required_full_ready_frames: u64,
    pub(crate) required_parity_frames: u64,
}

impl Default for ShadowRolloutEligibilityConfig {
    fn default() -> Self {
        Self {
            required_full_ready_frames: 120,
            required_parity_frames: 120,
        }
    }
}

pub(crate) fn shadow_rollout_eligible(
    trace_enabled: bool,
    promotion_enabled: bool,
    state: &ShadowRolloutSafetyState,
    snapshot: &RasterCacheSnapshot,
    plan: Option<&ProspectiveRasterPlan>,
    config: ShadowRolloutEligibilityConfig,
) -> bool {
    trace_enabled
        && promotion_enabled
        && state.consecutive_full_ready_frames >= config.required_full_ready_frames
        && state.consecutive_parity_compatible_frames >= config.required_parity_frames
        && snapshot.validation_errors.is_empty()
        && snapshot.all_costs_budget_usable
        && plan.is_some_and(ProspectiveRasterPlan::ready)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RasterCacheEntry {
    pub(crate) key: RasterCacheKey,
    pub(crate) association: LayerResourceAssociation,
    pub(crate) declared: Option<RasterDescriptor>,
    pub(crate) resident: Option<RasterDescriptor>,
    pub(crate) declared_bytes: u64,
    pub(crate) resident_bytes: u64,
    pub(crate) cost_confidence: CostConfidence,
    pub(crate) first_seen_epoch: u64,
    pub(crate) last_declared_epoch: u64,
    pub(crate) last_resident_epoch: u64,
    pub(crate) pool_last_used_epoch: Option<u64>,
    pub(crate) descriptor_mismatch: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RasterCacheValidationError {
    DuplicateDeclaredKey(RasterCacheKey),
    DuplicateResidentKey(RasterCacheKey),
    ConflictingAssociation(RasterCacheKey),
    DeclaredResidentDescriptorMismatch(RasterCacheKey),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RasterObservationStatus {
    #[default]
    NeverObserved,
    Current,
    StaleCompileFailure,
    StaleExecuteFailure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RasterLayerTopologyFingerprint {
    id: LayerId,
    parent: Option<LayerId>,
    stable_id: Option<u64>,
    composition_path: Vec<NodeKey>,
    bounds_bits: [u32; 8],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RasterSourceFingerprint {
    promoted_node_ids: Vec<u64>,
    layers: Vec<RasterLayerTopologyFingerprint>,
}

impl RasterSourceFingerprint {
    pub(crate) fn capture(promotion: &PromotionState, layer_tree: &LayerTree) -> Self {
        let mut promoted_node_ids = promotion
            .promoted_node_ids
            .iter()
            .copied()
            .collect::<Vec<_>>();
        promoted_node_ids.sort_unstable();
        let mut layers = layer_tree
            .layers
            .values()
            .map(|layer| RasterLayerTopologyFingerprint {
                id: layer.id,
                parent: layer.parent,
                stable_id: layer.stable_id,
                composition_path: layer.composition_path.clone(),
                bounds_bits: [
                    layer.bounds.x.to_bits(),
                    layer.bounds.y.to_bits(),
                    layer.bounds.width.to_bits(),
                    layer.bounds.height.to_bits(),
                    layer.bounds.corner_radii[0].to_bits(),
                    layer.bounds.corner_radii[1].to_bits(),
                    layer.bounds.corner_radii[2].to_bits(),
                    layer.bounds.corner_radii[3].to_bits(),
                ],
            })
            .collect::<Vec<_>>();
        layers.sort_by_key(|layer| {
            (
                layer.stable_id.unwrap_or_default(),
                format!("{:?}", layer.id),
            )
        });
        Self {
            promoted_node_ids,
            layers,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ViewportRasterFingerprint {
    pub(crate) logical_width_bits: u32,
    pub(crate) logical_height_bits: u32,
    pub(crate) scale_factor_bits: u32,
    pub(crate) target_format: wgpu::TextureFormat,
    pub(crate) sample_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RasterBudgetFallbackReason {
    NeverObserved,
    StaleObservation(RasterObservationStatus),
    PromotionSetMismatch,
    LayerTopologyMismatch,
    ViewportRasterFingerprintMismatch,
    ValidationErrors,
    UnknownCost,
    DeclaredCoverageIncomplete,
    ResidentCoverageIncomplete,
    RetiringCoverageIncomplete,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ObservedSnapshotConsistency {
    pub(crate) consistent: bool,
    pub(crate) fallback_reasons: Vec<RasterBudgetFallbackReason>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProspectiveReadinessFallbackReason {
    PlanNotBuilt,
    PlannerErrors,
    UnknownCost,
    ParityNotChecked,
    ParityMismatch,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ShadowRasterBudgetReadiness {
    pub(crate) observed: ObservedSnapshotConsistency,
    pub(crate) prospective_ready: bool,
    pub(crate) parity_ready: bool,
    pub(crate) ready: bool,
    pub(crate) prospective_fallback_reasons: Vec<ProspectiveReadinessFallbackReason>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ProspectivePlanError {
    MissingPromotedNode(u64),
    DuplicateResourceKey(RasterCacheKey),
    UnknownCost(RasterCacheKey),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlannedRasterResource {
    pub(crate) key: RasterCacheKey,
    pub(crate) descriptor: RasterDescriptor,
    pub(crate) role: RasterResourceRole,
    pub(crate) owner: NodeKey,
    pub(crate) cost: DescriptorPayloadBytes,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ProspectiveRasterPlan {
    pub(crate) resources: FxHashMap<RasterCacheKey, PlannedRasterResource>,
    pub(crate) total_bytes: u64,
    pub(crate) confidence: CostConfidence,
    pub(crate) errors: Vec<ProspectivePlanError>,
    pub(crate) planner_node_visits: usize,
}

impl ProspectiveRasterPlan {
    pub(crate) fn ready(&self) -> bool {
        self.errors.is_empty() && self.confidence.budget_usable()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RasterParityError {
    UnexpectedActual(RasterCacheKey),
    DescriptorMismatch(RasterCacheKey),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RasterPlanParity {
    pub(crate) checked: bool,
    pub(crate) actual_is_compatible_subset: bool,
    pub(crate) incompatibilities: Vec<RasterParityError>,
    pub(crate) planned_not_declared: Vec<RasterCacheKey>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RasterCacheSnapshot {
    pub(crate) epoch: u64,
    pub(crate) entries: FxHashMap<RasterCacheKey, RasterCacheEntry>,
    pub(crate) declared_bytes: u64,
    pub(crate) resident_bytes: u64,
    pub(crate) retiring_bytes: u64,
    pub(crate) stale_resident_bytes: u64,
    pub(crate) descriptor_mismatch_count: usize,
    pub(crate) promotion_declared_bytes: u64,
    pub(crate) promotion_resident_bytes: u64,
    pub(crate) promotion_retiring_bytes: u64,
    pub(crate) known_non_promotion_declared_bytes: u64,
    pub(crate) unidentified_declared_bytes: u64,
    pub(crate) unidentified_resident_bytes: u64,
    pub(crate) association_coverage: f32,
    pub(crate) resident_association_coverage: f32,
    pub(crate) retiring_association_coverage: f32,
    pub(crate) promotion_association_coverage: f32,
    pub(crate) all_costs_exact: bool,
    pub(crate) all_costs_budget_usable: bool,
    pub(crate) unknown_declared_keys: Vec<RasterCacheKey>,
    pub(crate) observation_status: RasterObservationStatus,
    pub(crate) last_attempted_frame: u64,
    pub(crate) last_committed_frame: Option<u64>,
    pub(crate) source_fingerprint: Option<RasterSourceFingerprint>,
    pub(crate) viewport_fingerprint: Option<ViewportRasterFingerprint>,
    pub(crate) validation_errors: Vec<RasterCacheValidationError>,
}

#[derive(Default)]
pub(crate) struct RasterCache {
    pub(crate) epoch: u64,
    pub(crate) snapshot: RasterCacheSnapshot,
}

impl RasterCache {
    pub(crate) fn observed_consistency(
        &self,
        current_source: &RasterSourceFingerprint,
        current_viewport: ViewportRasterFingerprint,
    ) -> ObservedSnapshotConsistency {
        let snapshot = &self.snapshot;
        if snapshot.last_committed_frame.is_none()
            || snapshot.source_fingerprint.is_none()
            || snapshot.viewport_fingerprint.is_none()
        {
            return ObservedSnapshotConsistency {
                consistent: false,
                fallback_reasons: vec![RasterBudgetFallbackReason::NeverObserved],
            };
        }

        let mut fallback_reasons = Vec::new();
        if snapshot.observation_status != RasterObservationStatus::Current {
            fallback_reasons.push(RasterBudgetFallbackReason::StaleObservation(
                snapshot.observation_status,
            ));
        }
        let observed_source = snapshot
            .source_fingerprint
            .as_ref()
            .expect("successful snapshot should retain source fingerprint");
        if observed_source.promoted_node_ids != current_source.promoted_node_ids {
            fallback_reasons.push(RasterBudgetFallbackReason::PromotionSetMismatch);
        }
        if observed_source.layers != current_source.layers {
            fallback_reasons.push(RasterBudgetFallbackReason::LayerTopologyMismatch);
        }
        if snapshot.viewport_fingerprint != Some(current_viewport) {
            fallback_reasons.push(RasterBudgetFallbackReason::ViewportRasterFingerprintMismatch);
        }
        if !snapshot.validation_errors.is_empty() {
            fallback_reasons.push(RasterBudgetFallbackReason::ValidationErrors);
        }
        if !snapshot.all_costs_budget_usable {
            fallback_reasons.push(RasterBudgetFallbackReason::UnknownCost);
        }
        if snapshot.association_coverage < 1.0 {
            fallback_reasons.push(RasterBudgetFallbackReason::DeclaredCoverageIncomplete);
        }
        if snapshot.resident_association_coverage < 1.0 {
            fallback_reasons.push(RasterBudgetFallbackReason::ResidentCoverageIncomplete);
        }
        if snapshot.retiring_association_coverage < 1.0 {
            fallback_reasons.push(RasterBudgetFallbackReason::RetiringCoverageIncomplete);
        }
        ObservedSnapshotConsistency {
            consistent: fallback_reasons.is_empty(),
            fallback_reasons,
        }
    }

    pub(crate) fn mark_failed_observation(&mut self, frame: u64, status: RasterObservationStatus) {
        debug_assert!(matches!(
            status,
            RasterObservationStatus::StaleCompileFailure
                | RasterObservationStatus::StaleExecuteFailure
        ));
        self.snapshot.observation_status = status;
        self.snapshot.last_attempted_frame = frame;
    }

    pub(crate) fn reconcile<'a>(
        &mut self,
        frame: u64,
        source_fingerprint: RasterSourceFingerprint,
        viewport_fingerprint: ViewportRasterFingerprint,
        declared: impl IntoIterator<Item = (PersistentTextureKey, &'a TextureDesc)>,
        resident: impl IntoIterator<Item = PersistentRenderTargetObservation>,
        layer_tree: &LayerTree,
        arena: &NodeArena,
    ) {
        self.epoch = self.epoch.wrapping_add(1);
        let expected = expected_associations(layer_tree, arena);
        let previous = &self.snapshot.entries;
        let mut entries = FxHashMap::<RasterCacheKey, RasterCacheEntry>::default();
        let ExpectedAssociations {
            associations,
            mut errors,
        } = expected;

        for (stable_key, desc) in declared {
            let key = RasterCacheKey(stable_key);
            if entries.contains_key(&key) {
                errors.push(RasterCacheValidationError::DuplicateDeclaredKey(key));
                continue;
            }
            let descriptor = RasterDescriptor::from(desc);
            let cost = descriptor_payload_bytes(descriptor);
            let old = previous.get(&key);
            entries.insert(
                key,
                RasterCacheEntry {
                    key,
                    association: observed_association(&associations, old, key),
                    declared: Some(descriptor),
                    resident: None,
                    declared_bytes: cost.bytes,
                    resident_bytes: 0,
                    cost_confidence: cost.confidence,
                    first_seen_epoch: old.map_or(self.epoch, |entry| entry.first_seen_epoch),
                    last_declared_epoch: self.epoch,
                    last_resident_epoch: old.map_or(0, |entry| entry.last_resident_epoch),
                    pool_last_used_epoch: old.and_then(|entry| entry.pool_last_used_epoch),
                    descriptor_mismatch: false,
                },
            );
        }

        let mut seen_resident = FxHashSet::default();
        for observation in resident {
            let key = RasterCacheKey(observation.stable_key);
            if !seen_resident.insert(key) {
                errors.push(RasterCacheValidationError::DuplicateResidentKey(key));
                continue;
            }
            let descriptor = RasterDescriptor::from(observation);
            let cost = descriptor_payload_bytes(descriptor);
            let old = previous.get(&key);
            let entry = entries.entry(key).or_insert_with(|| RasterCacheEntry {
                key,
                association: observed_association(&associations, old, key),
                declared: None,
                resident: None,
                declared_bytes: 0,
                resident_bytes: 0,
                cost_confidence: CostConfidence::Exact,
                first_seen_epoch: old.map_or(self.epoch, |entry| entry.first_seen_epoch),
                last_declared_epoch: old.map_or(0, |entry| entry.last_declared_epoch),
                last_resident_epoch: 0,
                pool_last_used_epoch: None,
                descriptor_mismatch: false,
            });
            if entry
                .declared
                .is_some_and(|declared| declared != descriptor)
            {
                entry.descriptor_mismatch = true;
                errors.push(RasterCacheValidationError::DeclaredResidentDescriptorMismatch(key));
            }
            entry.resident = Some(descriptor);
            entry.resident_bytes = cost.bytes;
            entry.cost_confidence = entry.cost_confidence.combine(cost.confidence);
            entry.last_resident_epoch = self.epoch;
            entry.pool_last_used_epoch = Some(observation.last_used_epoch);
        }

        let mut snapshot = RasterCacheSnapshot {
            epoch: self.epoch,
            entries,
            all_costs_exact: true,
            all_costs_budget_usable: true,
            validation_errors: errors,
            observation_status: RasterObservationStatus::Current,
            last_attempted_frame: frame,
            last_committed_frame: Some(frame),
            source_fingerprint: Some(source_fingerprint),
            viewport_fingerprint: Some(viewport_fingerprint),
            ..RasterCacheSnapshot::default()
        };
        let mut declared_count = 0usize;
        let mut associated_declared_count = 0usize;
        let mut resident_count = 0usize;
        let mut associated_resident_count = 0usize;
        let mut retiring_count = 0usize;
        let mut associated_retiring_count = 0usize;
        for entry in snapshot.entries.values() {
            snapshot.declared_bytes = snapshot.declared_bytes.saturating_add(entry.declared_bytes);
            snapshot.resident_bytes = snapshot.resident_bytes.saturating_add(entry.resident_bytes);
            let retiring =
                entry.resident.is_some() && (entry.declared.is_none() || entry.descriptor_mismatch);
            if retiring {
                snapshot.retiring_bytes =
                    snapshot.retiring_bytes.saturating_add(entry.resident_bytes);
                retiring_count = retiring_count.saturating_add(1);
                if entry.association.role.is_known() {
                    associated_retiring_count = associated_retiring_count.saturating_add(1);
                }
                if entry.association.role.is_promotion() {
                    snapshot.promotion_retiring_bytes = snapshot
                        .promotion_retiring_bytes
                        .saturating_add(entry.resident_bytes);
                }
            }
            if entry.descriptor_mismatch {
                snapshot.stale_resident_bytes = snapshot
                    .stale_resident_bytes
                    .saturating_add(entry.resident_bytes);
                snapshot.descriptor_mismatch_count =
                    snapshot.descriptor_mismatch_count.saturating_add(1);
            }
            if entry.declared.is_some() {
                declared_count = declared_count.saturating_add(1);
                if entry.association.role.is_promotion() {
                    snapshot.promotion_declared_bytes = snapshot
                        .promotion_declared_bytes
                        .saturating_add(entry.declared_bytes);
                    associated_declared_count = associated_declared_count.saturating_add(1);
                } else if entry.association.role.is_known() {
                    snapshot.known_non_promotion_declared_bytes = snapshot
                        .known_non_promotion_declared_bytes
                        .saturating_add(entry.declared_bytes);
                    associated_declared_count = associated_declared_count.saturating_add(1);
                } else {
                    snapshot.unidentified_declared_bytes = snapshot
                        .unidentified_declared_bytes
                        .saturating_add(entry.declared_bytes);
                    snapshot.unknown_declared_keys.push(entry.key);
                }
            }
            if entry.resident.is_some() {
                resident_count = resident_count.saturating_add(1);
                if entry.association.role.is_known() {
                    associated_resident_count = associated_resident_count.saturating_add(1);
                } else {
                    snapshot.unidentified_resident_bytes = snapshot
                        .unidentified_resident_bytes
                        .saturating_add(entry.resident_bytes);
                }
                if entry.association.role.is_promotion() {
                    snapshot.promotion_resident_bytes = snapshot
                        .promotion_resident_bytes
                        .saturating_add(entry.resident_bytes);
                }
            }
            snapshot.all_costs_exact &= entry.cost_confidence == CostConfidence::Exact;
            snapshot.all_costs_budget_usable &= entry.cost_confidence.budget_usable();
        }
        snapshot.association_coverage = if declared_count == 0 {
            1.0
        } else {
            associated_declared_count as f32 / declared_count as f32
        };
        snapshot.resident_association_coverage = if resident_count == 0 {
            1.0
        } else {
            associated_resident_count as f32 / resident_count as f32
        };
        snapshot.retiring_association_coverage = if retiring_count == 0 {
            1.0
        } else {
            associated_retiring_count as f32 / retiring_count as f32
        };
        let promotion_or_unknown_count = snapshot
            .entries
            .values()
            .filter(|entry| {
                entry.declared.is_some()
                    && (entry.association.role.is_promotion() || !entry.association.role.is_known())
            })
            .count();
        let promotion_count = snapshot
            .entries
            .values()
            .filter(|entry| entry.declared.is_some() && entry.association.role.is_promotion())
            .count();
        snapshot.promotion_association_coverage = if promotion_or_unknown_count == 0 {
            1.0
        } else {
            promotion_count as f32 / promotion_or_unknown_count as f32
        };
        snapshot
            .unknown_declared_keys
            .sort_by_key(|key| format!("{:?}", key.0));
        self.snapshot = snapshot;
    }
}

struct ExpectedAssociations {
    associations: FxHashMap<RasterCacheKey, LayerResourceAssociation>,
    errors: Vec<RasterCacheValidationError>,
}

fn observed_association(
    expected: &FxHashMap<RasterCacheKey, LayerResourceAssociation>,
    previous: Option<&RasterCacheEntry>,
    key: RasterCacheKey,
) -> LayerResourceAssociation {
    expected
        .get(&key)
        .copied()
        .filter(|association| association.role.is_known())
        .or_else(|| {
            previous
                .map(|entry| entry.association)
                .filter(|association| association.role.is_known())
        })
        .unwrap_or_else(LayerResourceAssociation::unknown)
}

fn expected_associations(layer_tree: &LayerTree, arena: &NodeArena) -> ExpectedAssociations {
    let mut expected = expected_associations_with_mask(layer_tree, |host| {
        let node = arena.get(host)?;
        Some((
            node.element.stable_id(),
            node.element.promotion_requires_mask_surface(arena),
        ))
    });
    // Transformed subtree targets are not compositor-promotion layers, but
    // they are known persistent resources. Associate them from active arena
    // identity rather than decoding arbitrary stable-key bits.
    for (owner, node) in arena.iter() {
        let stable_id = node.element.stable_id();
        insert_target_pair(
            &mut expected,
            transformed_layer_stable_key(stable_id),
            LayerResourceAssociation {
                layer: None,
                owner: Some(owner),
                owner_stable_id: Some(stable_id),
                role: RasterResourceRole::TransformedColor,
            },
            RasterResourceRole::TransformedDepthStencil,
        );
        insert_target_pair(
            &mut expected,
            root_effect_stable_key(owner),
            LayerResourceAssociation {
                layer: None,
                owner: Some(owner),
                owner_stable_id: Some(stable_id),
                role: RasterResourceRole::RootEffectColor,
            },
            RasterResourceRole::RootEffectDepthStencil,
        );
    }
    expected
}

fn expected_associations_with_mask(
    layer_tree: &LayerTree,
    mut mask_host: impl FnMut(NodeKey) -> Option<(u64, bool)>,
) -> ExpectedAssociations {
    let mut expected = ExpectedAssociations {
        associations: FxHashMap::default(),
        errors: Vec::new(),
    };
    let mut children_by_parent = FxHashSet::default();
    for layer in layer_tree.layers.values() {
        if let Some(parent) = layer.parent {
            children_by_parent.insert(parent);
        }
    }

    for layer in layer_tree.layers.values() {
        let LayerId::Promoted(owner) = layer.id else {
            continue;
        };
        let Some(stable_id) = layer.stable_id else {
            continue;
        };
        insert_target_pair(
            &mut expected,
            promoted_layer_stable_key(stable_id),
            LayerResourceAssociation {
                layer: Some(layer.id),
                owner: Some(owner),
                owner_stable_id: Some(stable_id),
                role: RasterResourceRole::BaseColor,
            },
            RasterResourceRole::BaseDepthStencil,
        );
        if children_by_parent.contains(&layer.id) {
            insert_target_pair(
                &mut expected,
                promoted_final_layer_stable_key(stable_id),
                LayerResourceAssociation {
                    layer: Some(layer.id),
                    owner: Some(owner),
                    owner_stable_id: Some(stable_id),
                    role: RasterResourceRole::FinalColor,
                },
                RasterResourceRole::FinalDepthStencil,
            );
        }
    }

    for child in layer_tree.layers.values() {
        if !matches!(child.id, LayerId::Promoted(_)) {
            continue;
        }
        let containing_layer = child.parent.unwrap_or(LayerId::SceneRoot);
        let mut hosts = child.composition_path.clone();
        if let LayerId::Promoted(parent_owner) = containing_layer {
            hosts.push(parent_owner);
        }
        for host in hosts {
            let Some((stable_id, requires_mask)) = mask_host(host) else {
                continue;
            };
            if !requires_mask {
                continue;
            }
            insert_target_pair(
                &mut expected,
                promoted_clip_mask_stable_key(stable_id),
                LayerResourceAssociation {
                    layer: Some(containing_layer),
                    owner: Some(host),
                    owner_stable_id: Some(stable_id),
                    role: RasterResourceRole::ClipMaskColor,
                },
                RasterResourceRole::ClipMaskDepthStencil,
            );
        }
    }
    expected
}

fn insert_target_pair(
    expected: &mut ExpectedAssociations,
    color_key: PersistentTextureKey,
    color: LayerResourceAssociation,
    depth_role: RasterResourceRole,
) {
    insert_association(expected, RasterCacheKey(color_key), color);
    if let Some(depth_key) = persistent_depth_stencil_stable_key(color_key) {
        insert_association(
            expected,
            RasterCacheKey(depth_key),
            LayerResourceAssociation {
                role: depth_role,
                ..color
            },
        );
    }
}

fn insert_association(
    expected: &mut ExpectedAssociations,
    key: RasterCacheKey,
    association: LayerResourceAssociation,
) {
    if expected
        .associations
        .insert(key, association)
        .is_some_and(|previous| previous != association)
    {
        expected
            .errors
            .push(RasterCacheValidationError::ConflictingAssociation(key));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{
        Angle, BorderRadius, ClipMode, Length, ParsedValue, Position, PropertyId, Rotate, Style,
        Transform,
    };
    use crate::view::base_component::Element;
    use crate::view::compositor::layer_tree::{
        CompositingReason, LayerBounds, PaintOrderKey, PendingLayer,
    };
    use crate::view::node_arena::Node;

    fn descriptor(format: wgpu::TextureFormat, sample_count: u32) -> RasterDescriptor {
        RasterDescriptor {
            width: 10,
            height: 20,
            format,
            dimension: wgpu::TextureDimension::D2,
            sample_count,
        }
    }

    #[test]
    fn descriptor_bytes_include_resolve_only_for_msaa() {
        assert_eq!(
            descriptor_payload_bytes(descriptor(wgpu::TextureFormat::Rgba8Unorm, 1)),
            DescriptorPayloadBytes {
                bytes: 10 * 20 * 4,
                confidence: CostConfidence::Exact,
            }
        );
        assert_eq!(
            descriptor_payload_bytes(descriptor(wgpu::TextureFormat::Rgba8Unorm, 4)),
            DescriptorPayloadBytes {
                bytes: 10 * 20 * 4 * 5,
                confidence: CostConfidence::Exact,
            }
        );
    }

    #[test]
    fn supported_color_and_depth_formats_have_expected_cost() {
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Bgra8Unorm,
        ] {
            assert_eq!(descriptor_payload_bytes(descriptor(format, 1)).bytes, 800);
        }
        let depth =
            descriptor_payload_bytes(descriptor(wgpu::TextureFormat::Depth24PlusStencil8, 1));
        assert_eq!(depth.bytes, 1600);
        assert_eq!(depth.confidence, CostConfidence::ConservativeUpperBound);
        assert_eq!(
            descriptor_payload_bytes(descriptor(wgpu::TextureFormat::Rgba16Float, 1)).bytes,
            1600
        );
        assert_eq!(
            descriptor_payload_bytes(descriptor(wgpu::TextureFormat::R32Float, 1)).confidence,
            CostConfidence::Unknown
        );
    }

    fn texture_desc(width: u32, height: u32) -> TextureDesc {
        TextureDesc::new(
            width,
            height,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureDimension::D2,
        )
    }

    fn resident(stable_key: u64, width: u32, height: u32) -> PersistentRenderTargetObservation {
        resident_key(PersistentTextureKey::Generic(stable_key), width, height)
    }

    fn resident_key(
        stable_key: PersistentTextureKey,
        width: u32,
        height: u32,
    ) -> PersistentRenderTargetObservation {
        PersistentRenderTargetObservation {
            stable_key,
            width,
            height,
            format: wgpu::TextureFormat::Rgba8Unorm,
            dimension: wgpu::TextureDimension::D2,
            sample_count: 1,
            last_used_epoch: 99,
        }
    }

    fn generic(key: u64) -> PersistentTextureKey {
        PersistentTextureKey::Generic(key)
    }

    fn depth(key: PersistentTextureKey) -> PersistentTextureKey {
        persistent_depth_stencil_stable_key(key).expect("color key should have a depth role")
    }

    fn source() -> RasterSourceFingerprint {
        RasterSourceFingerprint::default()
    }

    fn viewport() -> ViewportRasterFingerprint {
        ViewportRasterFingerprint {
            logical_width_bits: 100.0f32.to_bits(),
            logical_height_bits: 100.0f32.to_bits(),
            scale_factor_bits: 1.0f32.to_bits(),
            target_format: wgpu::TextureFormat::Rgba8Unorm,
            sample_count: 1,
        }
    }

    #[test]
    fn reconcile_distinguishes_declared_resident_and_retiring_resources() {
        let arena = NodeArena::new();
        let tree = LayerTree::default();
        let desc = texture_desc(10, 20);
        let mut cache = RasterCache::default();

        cache.reconcile(
            1,
            source(),
            viewport(),
            [(generic(7), &desc)],
            [resident(7, 10, 20)],
            &tree,
            &arena,
        );
        assert_eq!(cache.snapshot.declared_bytes, 800);
        assert_eq!(cache.snapshot.resident_bytes, 800);
        assert_eq!(cache.snapshot.retiring_bytes, 0);
        assert_eq!(cache.snapshot.unidentified_declared_bytes, 800);
        assert_eq!(cache.snapshot.association_coverage, 0.0);
        assert_eq!(
            cache.snapshot.entries[&RasterCacheKey(generic(7))].first_seen_epoch,
            1
        );

        cache.reconcile(
            2,
            source(),
            viewport(),
            std::iter::empty::<(PersistentTextureKey, &TextureDesc)>(),
            [resident(7, 10, 20)],
            &tree,
            &arena,
        );
        let retiring = &cache.snapshot.entries[&RasterCacheKey(generic(7))];
        assert!(retiring.declared.is_none());
        assert!(retiring.resident.is_some());
        assert_eq!(retiring.first_seen_epoch, 1);
        assert_eq!(cache.snapshot.retiring_bytes, 800);

        let resized = texture_desc(20, 20);
        cache.reconcile(
            3,
            source(),
            viewport(),
            [(generic(7), &resized)],
            [resident(7, 10, 20)],
            &tree,
            &arena,
        );
        let mismatch = &cache.snapshot.entries[&RasterCacheKey(generic(7))];
        assert!(mismatch.descriptor_mismatch);
        assert_eq!(cache.snapshot.descriptor_mismatch_count, 1);
        assert_eq!(cache.snapshot.stale_resident_bytes, 800);
        assert_eq!(cache.snapshot.retiring_bytes, 800);
        assert_eq!(
            cache.snapshot.validation_errors,
            vec![
                RasterCacheValidationError::DeclaredResidentDescriptorMismatch(RasterCacheKey(
                    generic(7)
                ))
            ]
        );

        cache.reconcile(
            4,
            source(),
            viewport(),
            [(generic(7), &resized)],
            [resident(7, 20, 20)],
            &tree,
            &arena,
        );
        assert_eq!(
            cache.snapshot.entries[&RasterCacheKey(generic(7))].declared_bytes,
            1600
        );
        assert_eq!(
            cache.snapshot.entries[&RasterCacheKey(generic(7))].resident_bytes,
            1600
        );
        assert_eq!(cache.snapshot.retiring_bytes, 0);
        assert_eq!(cache.snapshot.descriptor_mismatch_count, 0);
    }

    #[test]
    fn duplicate_declared_key_is_reported_without_double_counting() {
        let arena = NodeArena::new();
        let tree = LayerTree::default();
        let desc = texture_desc(10, 20);
        let mut cache = RasterCache::default();
        cache.reconcile(
            1,
            source(),
            viewport(),
            [(generic(7), &desc), (generic(7), &desc)],
            std::iter::empty(),
            &tree,
            &arena,
        );

        assert_eq!(cache.snapshot.entries.len(), 1);
        assert_eq!(cache.snapshot.declared_bytes, 800);
        assert_eq!(
            cache.snapshot.validation_errors,
            vec![RasterCacheValidationError::DuplicateDeclaredKey(
                RasterCacheKey(generic(7))
            )]
        );
    }

    fn pending_layer(
        id: LayerId,
        owner: Option<NodeKey>,
        stable_id: Option<u64>,
        parent: Option<LayerId>,
    ) -> PendingLayer {
        PendingLayer {
            id,
            owner,
            stable_id,
            parent,
            paint_order: PaintOrderKey::default(),
            composition_path: Vec::new(),
            reason: if id == LayerId::SceneRoot {
                CompositingReason::SceneRoot
            } else {
                CompositingReason::Heuristic {
                    score: 50,
                    threshold: 35,
                }
            },
            bounds: LayerBounds::scene(),
            properties: Default::default(),
            raster_revision: 1,
            composite_revision: 1,
            topology_revision: 1,
            items: Vec::new(),
        }
    }

    #[test]
    fn promoted_base_and_parent_final_pairs_associate_with_layers() {
        let mut arena = NodeArena::new();
        let parent = arena.insert(Node::new(Box::new(Element::new_with_id(
            11, 0.0, 0.0, 20.0, 20.0,
        ))));
        let child = arena.insert(Node::new(Box::new(Element::new_with_id(
            12, 0.0, 0.0, 10.0, 10.0,
        ))));
        let parent_layer = LayerId::Promoted(parent);
        let child_layer = LayerId::Promoted(child);
        let mut tree = LayerTree::default();
        tree.reconcile(
            vec![
                pending_layer(LayerId::SceneRoot, None, None, None),
                pending_layer(
                    parent_layer,
                    Some(parent),
                    Some(11),
                    Some(LayerId::SceneRoot),
                ),
                pending_layer(child_layer, Some(child), Some(12), Some(parent_layer)),
            ],
            Vec::new(),
        );

        let keys = [
            promoted_layer_stable_key(11),
            depth(promoted_layer_stable_key(11)),
            promoted_final_layer_stable_key(11),
            depth(promoted_final_layer_stable_key(11)),
            promoted_layer_stable_key(12),
            depth(promoted_layer_stable_key(12)),
            generic(0xDEAD),
        ];
        let descriptions = keys
            .iter()
            .map(|&key| (key, texture_desc(4, 4)))
            .collect::<Vec<_>>();
        let mut cache = RasterCache::default();
        cache.reconcile(
            1,
            source(),
            viewport(),
            descriptions.iter().map(|(key, desc)| (*key, desc)),
            std::iter::empty(),
            &tree,
            &arena,
        );

        assert_eq!(
            cache.snapshot.entries[&RasterCacheKey(promoted_layer_stable_key(11))].association,
            LayerResourceAssociation {
                layer: Some(parent_layer),
                owner: Some(parent),
                owner_stable_id: Some(11),
                role: RasterResourceRole::BaseColor,
            }
        );
        assert_eq!(
            cache.snapshot.entries[&RasterCacheKey(promoted_final_layer_stable_key(11))]
                .association
                .role,
            RasterResourceRole::FinalColor
        );
        assert_eq!(
            cache.snapshot.entries[&RasterCacheKey(depth(promoted_final_layer_stable_key(11)))]
                .association
                .role,
            RasterResourceRole::FinalDepthStencil
        );
        assert_eq!(
            cache.snapshot.entries[&RasterCacheKey(generic(0xDEAD))]
                .association
                .role,
            RasterResourceRole::UnknownPersistent
        );
        assert_eq!(cache.snapshot.promotion_declared_bytes, 6 * 4 * 4 * 4);
        assert_eq!(cache.snapshot.unidentified_declared_bytes, 4 * 4 * 4);
    }

    #[test]
    fn clip_mask_pair_associates_host_with_containing_layer() {
        let mut arena = NodeArena::new();
        let parent = arena.insert(Node::new(Box::new(Element::new_with_id(
            11, 0.0, 0.0, 20.0, 20.0,
        ))));
        let host = arena.insert(Node::new(Box::new(Element::new_with_id(
            13, 0.0, 0.0, 20.0, 20.0,
        ))));
        let child = arena.insert(Node::new(Box::new(Element::new_with_id(
            12, 0.0, 0.0, 10.0, 10.0,
        ))));
        let parent_layer = LayerId::Promoted(parent);
        let mut child_pending = pending_layer(
            LayerId::Promoted(child),
            Some(child),
            Some(12),
            Some(parent_layer),
        );
        child_pending.composition_path = vec![host];
        let mut tree = LayerTree::default();
        tree.reconcile(
            vec![
                pending_layer(LayerId::SceneRoot, None, None, None),
                pending_layer(
                    parent_layer,
                    Some(parent),
                    Some(11),
                    Some(LayerId::SceneRoot),
                ),
                child_pending,
            ],
            Vec::new(),
        );

        let expected =
            expected_associations_with_mask(&tree, |key| (key == host).then_some((13, true)));
        let color_key = RasterCacheKey(promoted_clip_mask_stable_key(13));
        let depth_key = RasterCacheKey(depth(color_key.0));
        assert_eq!(
            expected.associations[&color_key],
            LayerResourceAssociation {
                layer: Some(parent_layer),
                owner: Some(host),
                owner_stable_id: Some(13),
                role: RasterResourceRole::ClipMaskColor,
            }
        );
        assert_eq!(
            expected.associations[&depth_key].role,
            RasterResourceRole::ClipMaskDepthStencil
        );
        assert!(expected.errors.is_empty());
    }

    #[test]
    fn transformed_pair_is_known_non_promotion_without_key_decoding() {
        let mut arena = NodeArena::new();
        let owner = arena.insert(Node::new(Box::new(Element::new_with_id(
            21, 0.0, 0.0, 10.0, 10.0,
        ))));
        let tree = LayerTree::default();
        let color = transformed_layer_stable_key(21);
        let depth = depth(color);
        let color_desc = texture_desc(4, 4);
        let depth_desc = TextureDesc::new(
            4,
            4,
            wgpu::TextureFormat::Depth24PlusStencil8,
            wgpu::TextureDimension::D2,
        );
        let mut cache = RasterCache::default();
        cache.reconcile(
            1,
            source(),
            viewport(),
            [(color, &color_desc), (depth, &depth_desc)],
            std::iter::empty(),
            &tree,
            &arena,
        );

        assert_eq!(
            cache.snapshot.entries[&RasterCacheKey(color)].association,
            LayerResourceAssociation {
                layer: None,
                owner: Some(owner),
                owner_stable_id: Some(21),
                role: RasterResourceRole::TransformedColor,
            }
        );
        assert_eq!(
            cache.snapshot.entries[&RasterCacheKey(depth)]
                .association
                .role,
            RasterResourceRole::TransformedDepthStencil
        );
        assert_eq!(cache.snapshot.promotion_declared_bytes, 0);
        assert_eq!(cache.snapshot.unidentified_declared_bytes, 0);
        assert_eq!(cache.snapshot.association_coverage, 1.0);
        assert_eq!(cache.snapshot.promotion_association_coverage, 1.0);
        assert!(!cache.snapshot.all_costs_exact);
        assert!(cache.snapshot.all_costs_budget_usable);
    }

    #[test]
    fn root_effect_pair_is_known_non_promotion_and_bytes_remain_observed() {
        let mut arena = NodeArena::new();
        let owner = arena.insert(Node::new(Box::new(Element::new_with_id(
            22, 0.0, 0.0, 10.0, 10.0,
        ))));
        let tree = LayerTree::default();
        let color = root_effect_stable_key(owner);
        let depth = depth(color);
        let color_desc = texture_desc(4, 4);
        let depth_desc = TextureDesc::new(
            4,
            4,
            wgpu::TextureFormat::Depth24PlusStencil8,
            wgpu::TextureDimension::D2,
        );
        let expected_bytes = descriptor_payload_bytes((&color_desc).into()).bytes
            + descriptor_payload_bytes((&depth_desc).into()).bytes;
        let mut cache = RasterCache::default();
        cache.reconcile(
            1,
            source(),
            viewport(),
            [(color, &color_desc), (depth, &depth_desc)],
            std::iter::empty(),
            &tree,
            &arena,
        );

        assert_eq!(
            cache.snapshot.entries[&RasterCacheKey(color)].association,
            LayerResourceAssociation {
                layer: None,
                owner: Some(owner),
                owner_stable_id: Some(22),
                role: RasterResourceRole::RootEffectColor,
            }
        );
        assert_eq!(
            cache.snapshot.entries[&RasterCacheKey(depth)]
                .association
                .role,
            RasterResourceRole::RootEffectDepthStencil
        );
        assert_eq!(
            cache.snapshot.known_non_promotion_declared_bytes,
            expected_bytes
        );
        assert_eq!(cache.snapshot.promotion_declared_bytes, 0);
        assert_eq!(cache.snapshot.unidentified_declared_bytes, 0);
    }

    #[test]
    fn promotion_parity_ignores_typed_root_effect_even_without_association() {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(Element::new_with_id(
            23, 0.0, 0.0, 10.0, 10.0,
        ))));
        let color = RasterCacheKey(root_effect_stable_key(root));
        let depth = RasterCacheKey(depth(color.0));
        let descriptor = RasterDescriptor::from(&texture_desc(4, 4));
        let mut snapshot = RasterCacheSnapshot::default();
        for key in [color, depth] {
            snapshot.entries.insert(
                key,
                RasterCacheEntry {
                    key,
                    association: LayerResourceAssociation::unknown(),
                    declared: Some(descriptor),
                    resident: None,
                    declared_bytes: 64,
                    resident_bytes: 0,
                    cost_confidence: CostConfidence::Exact,
                    first_seen_epoch: 1,
                    last_declared_epoch: 1,
                    last_resident_epoch: 0,
                    pool_last_used_epoch: None,
                    descriptor_mismatch: false,
                },
            );
        }

        let parity = compare_plan_to_snapshot(&ProspectiveRasterPlan::default(), &snapshot);
        assert!(parity.actual_is_compatible_subset);
        assert!(parity.incompatibilities.is_empty());
    }

    #[test]
    fn failed_observation_marks_last_snapshot_stale_without_committing_epoch() {
        let arena = NodeArena::new();
        let tree = LayerTree::default();
        let desc = texture_desc(4, 4);
        let mut cache = RasterCache::default();
        cache.reconcile(
            10,
            source(),
            viewport(),
            [(generic(7), &desc)],
            std::iter::empty(),
            &tree,
            &arena,
        );
        assert_eq!(cache.epoch, 1);
        assert_eq!(cache.snapshot.last_committed_frame, Some(10));

        cache.mark_failed_observation(11, RasterObservationStatus::StaleExecuteFailure);
        assert_eq!(cache.epoch, 1);
        assert_eq!(
            cache.snapshot.observation_status,
            RasterObservationStatus::StaleExecuteFailure
        );
        assert_eq!(cache.snapshot.last_attempted_frame, 11);
        assert_eq!(cache.snapshot.last_committed_frame, Some(10));
        assert_eq!(cache.snapshot.declared_bytes, 64);
    }

    #[test]
    fn resident_only_retiring_resource_keeps_previous_promotion_association() {
        let mut arena = NodeArena::new();
        let owner = arena.insert(Node::new(Box::new(Element::new_with_id(
            31, 0.0, 0.0, 10.0, 10.0,
        ))));
        let layer = LayerId::Promoted(owner);
        let mut mounted_tree = LayerTree::default();
        mounted_tree.reconcile(
            vec![
                pending_layer(LayerId::SceneRoot, None, None, None),
                pending_layer(layer, Some(owner), Some(31), Some(LayerId::SceneRoot)),
            ],
            Vec::new(),
        );
        let key = promoted_layer_stable_key(31);
        let desc = texture_desc(4, 4);
        let mut cache = RasterCache::default();
        cache.reconcile(
            1,
            RasterSourceFingerprint::capture(&PromotionState::default(), &mounted_tree),
            viewport(),
            [(key, &desc)],
            [resident_key(key, 4, 4)],
            &mounted_tree,
            &arena,
        );

        let unmounted_tree = LayerTree::default();
        cache.reconcile(
            2,
            source(),
            viewport(),
            std::iter::empty(),
            [resident_key(key, 4, 4)],
            &unmounted_tree,
            &arena,
        );
        let entry = &cache.snapshot.entries[&RasterCacheKey(key)];
        assert_eq!(entry.association.role, RasterResourceRole::BaseColor);
        assert_eq!(cache.snapshot.promotion_resident_bytes, 64);
        assert_eq!(cache.snapshot.promotion_retiring_bytes, 64);
        assert_eq!(cache.snapshot.unidentified_resident_bytes, 0);
        assert_eq!(cache.snapshot.resident_association_coverage, 1.0);
        assert_eq!(cache.snapshot.retiring_association_coverage, 1.0);
    }

    #[test]
    fn zero_declared_never_observed_cache_is_not_budget_ready() {
        let cache = RasterCache::default();
        assert_eq!(
            cache.observed_consistency(&source(), viewport()),
            ObservedSnapshotConsistency {
                consistent: false,
                fallback_reasons: vec![RasterBudgetFallbackReason::NeverObserved],
            }
        );
    }

    #[test]
    fn budget_readiness_reports_stale_source_viewport_and_metadata_fallbacks() {
        let mut arena = NodeArena::new();
        let _owner = arena.insert(Node::new(Box::new(Element::new_with_id(
            41, 0.0, 0.0, 10.0, 10.0,
        ))));
        let key = transformed_layer_stable_key(41);
        let desc = texture_desc(4, 4);
        let tree = LayerTree::default();
        let mut cache = RasterCache::default();
        cache.reconcile(
            10,
            source(),
            viewport(),
            [(key, &desc)],
            [resident_key(key, 4, 4)],
            &tree,
            &arena,
        );
        assert!(cache.observed_consistency(&source(), viewport()).consistent);

        cache.mark_failed_observation(11, RasterObservationStatus::StaleCompileFailure);
        let mut mismatched_source = source();
        mismatched_source.promoted_node_ids.push(99);
        mismatched_source
            .layers
            .push(RasterLayerTopologyFingerprint {
                id: LayerId::SceneRoot,
                parent: None,
                stable_id: None,
                composition_path: Vec::new(),
                bounds_bits: [0; 8],
            });
        let mut mismatched_viewport = viewport();
        mismatched_viewport.scale_factor_bits = 2.0f32.to_bits();
        cache
            .snapshot
            .validation_errors
            .push(RasterCacheValidationError::DuplicateDeclaredKey(
                RasterCacheKey(key),
            ));
        cache.snapshot.all_costs_exact = false;
        cache.snapshot.all_costs_budget_usable = false;
        cache.snapshot.association_coverage = 0.5;
        cache.snapshot.resident_association_coverage = 0.5;
        cache.snapshot.retiring_association_coverage = 0.5;

        let readiness = cache.observed_consistency(&mismatched_source, mismatched_viewport);
        assert!(!readiness.consistent);
        for reason in [
            RasterBudgetFallbackReason::StaleObservation(
                RasterObservationStatus::StaleCompileFailure,
            ),
            RasterBudgetFallbackReason::PromotionSetMismatch,
            RasterBudgetFallbackReason::LayerTopologyMismatch,
            RasterBudgetFallbackReason::ViewportRasterFingerprintMismatch,
            RasterBudgetFallbackReason::ValidationErrors,
            RasterBudgetFallbackReason::UnknownCost,
            RasterBudgetFallbackReason::DeclaredCoverageIncomplete,
            RasterBudgetFallbackReason::ResidentCoverageIncomplete,
            RasterBudgetFallbackReason::RetiringCoverageIncomplete,
        ] {
            assert!(readiness.fallback_reasons.contains(&reason));
        }
    }

    fn append_child(arena: &mut NodeArena, parent: NodeKey, child: NodeKey) {
        arena.set_parent(child, Some(parent));
        arena.push_child(parent, child);
    }

    #[test]
    fn prospective_plan_adds_parent_final_pair_for_promoted_child() {
        let mut arena = NodeArena::new();
        let parent = arena.insert(Node::new(Box::new(Element::new_with_id(
            51, 0.0, 0.0, 40.0, 40.0,
        ))));
        let child = arena.insert(Node::new(Box::new(Element::new_with_id(
            52, 0.0, 0.0, 20.0, 20.0,
        ))));
        append_child(&mut arena, parent, child);
        let promoted = FxHashSet::from_iter([51, 52]);

        let plan = plan_prospective_raster_resources(&arena, &[parent], &promoted, viewport());

        assert!(plan.errors.is_empty());
        assert_eq!(plan.resources.len(), 6);
        assert!(
            plan.resources
                .contains_key(&RasterCacheKey(promoted_final_layer_stable_key(51)))
        );
        assert!(
            !plan
                .resources
                .contains_key(&RasterCacheKey(promoted_final_layer_stable_key(52)))
        );
    }

    #[test]
    fn prospective_plan_tracks_mask_toggle_and_transform_addition() {
        let mut arena = NodeArena::new();
        let mut parent_element = Element::new_with_id(61, 0.0, 0.0, 40.0, 40.0);
        let mut rounded = Style::new();
        rounded.set_border_radius(BorderRadius::uniform(Length::px(10.0)));
        parent_element.apply_style(rounded);
        let parent = arena.insert(Node::new(Box::new(parent_element)));
        let mut transformed = Element::new_with_id(62, 0.0, 0.0, 20.0, 20.0);
        let mut transform_style = Style::new();
        transform_style.set_transform(Transform::new([Rotate::z(Angle::deg(15.0))]));
        transformed.apply_style(transform_style);
        let child = arena.insert(Node::new(Box::new(transformed)));
        append_child(&mut arena, parent, child);
        let promoted = FxHashSet::from_iter([62]);

        let plan = plan_prospective_raster_resources(&arena, &[parent], &promoted, viewport());

        assert!(
            plan.resources
                .contains_key(&RasterCacheKey(promoted_clip_mask_stable_key(61)))
        );
        assert!(
            plan.resources
                .contains_key(&RasterCacheKey(transformed_layer_stable_key(62)))
        );
        assert_eq!(
            plan.resources[&RasterCacheKey(promoted_clip_mask_stable_key(61))].role,
            RasterResourceRole::ClipMaskColor
        );
    }

    #[test]
    fn prospective_plan_matches_reachable_and_deferred_render_boundaries() {
        let mut arena = NodeArena::new();
        let mut parent_element = Element::new_with_id(63, 0.0, 0.0, 40.0, 40.0);
        let mut rounded = Style::new();
        rounded.set_border_radius(BorderRadius::uniform(Length::px(10.0)));
        parent_element.apply_style(rounded);
        let parent = arena.insert(Node::new(Box::new(parent_element)));

        let mut deferred_element = Element::new_with_id(64, 0.0, 0.0, 20.0, 20.0);
        let mut deferred_style = Style::new();
        deferred_style.insert(
            PropertyId::Position,
            ParsedValue::Position(Position::absolute().clip(ClipMode::Viewport)),
        );
        deferred_element.apply_style(deferred_style);
        let deferred = arena.insert(Node::new(Box::new(deferred_element)));
        append_child(&mut arena, parent, deferred);

        let mut detached_transform = Element::new_with_id(65, 0.0, 0.0, 20.0, 20.0);
        let mut transform_style = Style::new();
        transform_style.set_transform(Transform::new([Rotate::z(Angle::deg(15.0))]));
        detached_transform.apply_style(transform_style);
        arena.insert(Node::new(Box::new(detached_transform)));

        let promoted = FxHashSet::from_iter([63, 64]);
        let plan = plan_prospective_raster_resources(&arena, &[parent], &promoted, viewport());

        assert!(
            plan.resources
                .contains_key(&RasterCacheKey(promoted_clip_mask_stable_key(63)))
        );
        assert!(
            !plan
                .resources
                .contains_key(&RasterCacheKey(promoted_final_layer_stable_key(63)))
        );
        assert!(
            !plan
                .resources
                .contains_key(&RasterCacheKey(transformed_layer_stable_key(65)))
        );
    }

    #[test]
    fn prospective_planner_visits_each_node_once_on_deep_promoted_chain() {
        const NODE_COUNT: usize = 512;
        let mut arena = NodeArena::new();
        let mut keys = Vec::with_capacity(NODE_COUNT);
        for index in 0..NODE_COUNT {
            keys.push(arena.insert(Node::new(Box::new(Element::new_with_id(
                10_000 + index as u64,
                0.0,
                0.0,
                10.0,
                10.0,
            )))));
        }
        for pair in keys.windows(2) {
            append_child(&mut arena, pair[0], pair[1]);
        }
        let promoted = (0..NODE_COUNT)
            .map(|index| 10_000 + index as u64)
            .collect::<FxHashSet<_>>();

        let plan = plan_prospective_raster_resources(&arena, &keys[..1], &promoted, viewport());

        assert_eq!(plan.planner_node_visits, NODE_COUNT);
        assert_eq!(plan.resources.len(), NODE_COUNT * 4 - 2);
        assert!(plan.errors.is_empty());
    }

    fn permutations(values: &mut [u64], start: usize, out: &mut Vec<Vec<u64>>) {
        if start == values.len() {
            out.push(values.to_vec());
            return;
        }
        for index in start..values.len() {
            values.swap(start, index);
            permutations(values, start + 1, out);
            values.swap(start, index);
        }
    }

    fn assert_accumulator_matches_full_plan(
        accumulator: &ProspectivePlanAccumulator<'_>,
        full: &ProspectiveRasterPlan,
    ) {
        assert_eq!(accumulator.plan().resources, full.resources);
        assert_eq!(accumulator.plan().total_bytes, full.total_bytes);
        assert_eq!(accumulator.plan().confidence, full.confidence);
        assert_eq!(accumulator.plan().errors, full.errors);
    }

    #[test]
    fn accumulator_matches_full_planner_for_all_small_tree_admission_prefixes() {
        let mut arena = NodeArena::new();
        let mut root_element = Element::new_with_id(20_000, 0.0, 0.0, 40.0, 40.0);
        let mut rounded = Style::new();
        rounded.set_border_radius(BorderRadius::uniform(Length::px(8.0)));
        root_element.apply_style(rounded);
        let root = arena.insert(Node::new(Box::new(root_element)));
        let a = arena.insert(Node::new(Box::new(Element::new_with_id(
            20_001, 0.0, 0.0, 20.0, 20.0,
        ))));
        let mut transformed = Element::new_with_id(20_002, 0.0, 0.0, 10.0, 10.0);
        let mut transform_style = Style::new();
        transform_style.set_transform(Transform::new([Rotate::z(Angle::deg(10.0))]));
        transformed.apply_style(transform_style);
        let b = arena.insert(Node::new(Box::new(transformed)));
        let mut deferred = Element::new_with_id(20_003, 0.0, 0.0, 10.0, 10.0);
        let mut deferred_style = Style::new();
        deferred_style.insert(
            PropertyId::Position,
            ParsedValue::Position(Position::absolute().clip(ClipMode::Viewport)),
        );
        deferred.apply_style(deferred_style);
        let c = arena.insert(Node::new(Box::new(deferred)));
        append_child(&mut arena, root, a);
        append_child(&mut arena, a, b);
        append_child(&mut arena, root, c);

        let mut orders = Vec::new();
        permutations(&mut [20_000, 20_001, 20_002, 20_003], 0, &mut orders);
        for order in orders {
            let snapshot = RasterCacheSnapshot::default();
            let mut accumulator =
                ProspectivePlanAccumulator::new(&arena, &[root], viewport(), &snapshot);
            let mut accepted = FxHashSet::default();
            for stable_id in order {
                let mut tentative = accepted.clone();
                tentative.insert(stable_id);
                let projected = accumulator.project_tentative(&tentative);
                let tentative_full =
                    plan_prospective_raster_resources(&arena, &[root], &tentative, viewport());
                assert_eq!(
                    projected,
                    project_shadow_promotion_peak(&tentative_full, &snapshot)
                );
                accepted = tentative;
                accumulator.finish(&accepted);
                let full =
                    plan_prospective_raster_resources(&arena, &[root], &accepted, viewport());
                assert_accumulator_matches_full_plan(&accumulator, &full);
            }
            let metrics = accumulator.metrics();
            assert_eq!(metrics.full_planner_calls, 1);
            assert_eq!(metrics.full_planner_node_visits, 4);
            assert_eq!(metrics.precompute_node_visits, 4);
            assert_eq!(metrics.accumulator_try_calls, 4);
        }
    }

    #[test]
    fn accumulator_discards_rejected_tentative_delta() {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(Element::new_with_id(
            30_000, 0.0, 0.0, 20.0, 20.0,
        ))));
        let child = arena.insert(Node::new(Box::new(Element::new_with_id(
            30_001, 0.0, 0.0, 10.0, 10.0,
        ))));
        append_child(&mut arena, root, child);
        let snapshot = RasterCacheSnapshot::default();
        let mut accumulator =
            ProspectivePlanAccumulator::new(&arena, &[root], viewport(), &snapshot);
        accumulator.project_tentative(&FxHashSet::from_iter([30_001]));
        let accepted = FxHashSet::from_iter([30_000]);
        accumulator.project_tentative(&accepted);
        accumulator.finish(&accepted);
        let full = plan_prospective_raster_resources(&arena, &[root], &accepted, viewport());
        assert_accumulator_matches_full_plan(&accumulator, &full);
    }

    #[test]
    fn accumulator_deep_chain_worst_orders_keep_ancestor_visits_linear() {
        const NODE_COUNT: usize = 256;
        let mut arena = NodeArena::new();
        let mut keys = Vec::with_capacity(NODE_COUNT);
        let mut ids = Vec::with_capacity(NODE_COUNT);
        for index in 0..NODE_COUNT {
            let stable_id = 40_000 + index as u64;
            ids.push(stable_id);
            keys.push(arena.insert(Node::new(Box::new(Element::new_with_id(
                stable_id, 0.0, 0.0, 10.0, 10.0,
            )))));
        }
        for pair in keys.windows(2) {
            append_child(&mut arena, pair[0], pair[1]);
        }
        let snapshot = RasterCacheSnapshot::default();
        let all_promoted = ids.iter().copied().collect::<FxHashSet<_>>();
        let full = plan_prospective_raster_resources(&arena, &keys[..1], &all_promoted, viewport());

        for order in [ids.clone(), ids.iter().rev().copied().collect::<Vec<_>>()] {
            let mut accumulator =
                ProspectivePlanAccumulator::new(&arena, &keys[..1], viewport(), &snapshot);
            let mut accepted = FxHashSet::default();
            for stable_id in order {
                let mut tentative = accepted.clone();
                tentative.insert(stable_id);
                accumulator.project_tentative(&tentative);
                accepted = tentative;
                accumulator.finish(&accepted);
            }
            assert_accumulator_matches_full_plan(&accumulator, &full);
            assert!(
                accumulator.metrics().accumulator_ancestor_visits <= NODE_COUNT * 2,
                "ancestor visits must stay linear: {:?}",
                accumulator.metrics()
            );
        }
    }

    #[test]
    fn prospective_plan_reports_missing_nodes_and_unknown_cost() {
        let arena = NodeArena::new();
        let missing = FxHashSet::from_iter([999]);
        let missing_plan = plan_prospective_raster_resources(&arena, &[], &missing, viewport());
        assert_eq!(
            missing_plan.errors,
            vec![ProspectivePlanError::MissingPromotedNode(999)]
        );
        assert!(!missing_plan.ready());

        let mut arena = NodeArena::new();
        arena.insert(Node::new(Box::new(Element::new_with_id(
            71, 0.0, 0.0, 10.0, 10.0,
        ))));
        let mut unknown_viewport = viewport();
        unknown_viewport.target_format = wgpu::TextureFormat::R32Float;
        let unknown_plan = plan_prospective_raster_resources(
            &arena,
            &arena.iter().map(|(key, _)| key).collect::<Vec<_>>(),
            &FxHashSet::from_iter([71]),
            unknown_viewport,
        );
        assert_eq!(unknown_plan.confidence, CostConfidence::Unknown);
        assert!(
            unknown_plan
                .errors
                .iter()
                .any(|error| matches!(error, ProspectivePlanError::UnknownCost(_)))
        );
        assert!(!unknown_plan.ready());
    }

    #[test]
    fn total_shadow_readiness_requires_plan_and_post_execute_parity() {
        let observed = ObservedSnapshotConsistency {
            consistent: true,
            fallback_reasons: Vec::new(),
        };
        let without_plan =
            evaluate_shadow_budget_readiness(observed.clone(), None, &RasterPlanParity::default());
        assert!(!without_plan.ready);
        assert!(
            without_plan
                .prospective_fallback_reasons
                .contains(&ProspectiveReadinessFallbackReason::PlanNotBuilt)
        );
        assert!(
            without_plan
                .prospective_fallback_reasons
                .contains(&ProspectiveReadinessFallbackReason::ParityNotChecked)
        );

        let plan = ProspectiveRasterPlan {
            confidence: CostConfidence::ConservativeUpperBound,
            ..ProspectiveRasterPlan::default()
        };
        let before_execute = evaluate_shadow_budget_readiness(
            observed.clone(),
            Some(&plan),
            &RasterPlanParity::default(),
        );
        assert!(before_execute.prospective_ready);
        assert!(!before_execute.parity_ready);
        assert!(!before_execute.ready);

        let after_execute = evaluate_shadow_budget_readiness(
            observed,
            Some(&plan),
            &RasterPlanParity {
                checked: true,
                actual_is_compatible_subset: true,
                incompatibilities: Vec::new(),
                planned_not_declared: Vec::new(),
            },
        );
        assert!(after_execute.ready);
    }

    #[test]
    fn planned_vs_actual_parity_matches_canonical_keys_and_descriptors() {
        let mut arena = NodeArena::new();
        arena.insert(Node::new(Box::new(Element::new_with_id(
            81, 0.0, 0.0, 10.0, 10.0,
        ))));
        let roots = arena.iter().map(|(key, _)| key).collect::<Vec<_>>();
        let plan = plan_prospective_raster_resources(
            &arena,
            &roots,
            &FxHashSet::from_iter([81]),
            viewport(),
        );
        let mut snapshot = RasterCacheSnapshot::default();
        for (&key, resource) in &plan.resources {
            snapshot.entries.insert(
                key,
                RasterCacheEntry {
                    key,
                    association: LayerResourceAssociation::unknown(),
                    declared: Some(resource.descriptor),
                    resident: None,
                    declared_bytes: resource.cost.bytes,
                    resident_bytes: 0,
                    cost_confidence: resource.cost.confidence,
                    first_seen_epoch: 1,
                    last_declared_epoch: 1,
                    last_resident_epoch: 0,
                    pool_last_used_epoch: None,
                    descriptor_mismatch: false,
                },
            );
        }
        assert_eq!(
            compare_plan_to_snapshot(&plan, &snapshot),
            RasterPlanParity {
                checked: true,
                actual_is_compatible_subset: true,
                incompatibilities: Vec::new(),
                planned_not_declared: Vec::new(),
            }
        );

        let removed = *plan.resources.keys().next().expect("plan resource");
        snapshot.entries.remove(&removed);
        let subset = compare_plan_to_snapshot(&plan, &snapshot);
        assert!(subset.actual_is_compatible_subset);
        assert!(subset.planned_not_declared.contains(&removed));

        let descriptor_key = *snapshot.entries.keys().next().expect("remaining resource");
        let original_descriptor = snapshot.entries[&descriptor_key]
            .declared
            .expect("declared descriptor");
        snapshot
            .entries
            .get_mut(&descriptor_key)
            .expect("remaining resource")
            .declared = Some(RasterDescriptor {
            width: original_descriptor.width.saturating_add(1),
            ..original_descriptor
        });
        let descriptor_mismatch = compare_plan_to_snapshot(&plan, &snapshot);
        assert!(!descriptor_mismatch.actual_is_compatible_subset);
        assert!(
            descriptor_mismatch
                .incompatibilities
                .contains(&RasterParityError::DescriptorMismatch(descriptor_key))
        );
        snapshot
            .entries
            .get_mut(&descriptor_key)
            .expect("remaining resource")
            .declared = Some(original_descriptor);

        let unexpected = RasterCacheKey(transformed_layer_stable_key(999));
        let mut entry = snapshot
            .entries
            .values()
            .next()
            .expect("remaining actual resource")
            .clone();
        entry.key = unexpected;
        snapshot.entries.insert(unexpected, entry);
        let mismatch = compare_plan_to_snapshot(&plan, &snapshot);
        assert!(!mismatch.actual_is_compatible_subset);
        assert!(
            mismatch
                .incompatibilities
                .contains(&RasterParityError::UnexpectedActual(unexpected))
        );
    }

    #[test]
    fn shadow_projection_counts_incremental_and_keeps_retiring_resident_in_peak() {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(Element::new_with_id(
            91, 0.0, 0.0, 10.0, 10.0,
        ))));
        let plan = plan_prospective_raster_resources(
            &arena,
            &[root],
            &FxHashSet::from_iter([91]),
            viewport(),
        );
        let planned = plan
            .resources
            .values()
            .filter(|resource| resource.role.is_promotion())
            .map(|resource| resource.cost.bytes)
            .sum::<u64>();
        let mut snapshot = RasterCacheSnapshot {
            promotion_resident_bytes: 777,
            promotion_retiring_bytes: 333,
            ..RasterCacheSnapshot::default()
        };
        let projection = project_shadow_promotion_peak(&plan, &snapshot);
        assert_eq!(projection.planned_promotion_bytes, planned);
        assert_eq!(projection.incremental_resident_bytes, planned);
        assert_eq!(projection.current_promotion_resident_bytes, 777);
        assert_eq!(projection.promotion_retiring_bytes, 333);
        assert_eq!(projection.projected_peak_bytes, 777 + planned);

        for (&key, resource) in &plan.resources {
            if !resource.role.is_promotion() {
                continue;
            }
            snapshot.entries.insert(
                key,
                RasterCacheEntry {
                    key,
                    association: LayerResourceAssociation::unknown(),
                    declared: None,
                    resident: Some(resource.descriptor),
                    declared_bytes: 0,
                    resident_bytes: resource.cost.bytes,
                    cost_confidence: resource.cost.confidence,
                    first_seen_epoch: 1,
                    last_declared_epoch: 0,
                    last_resident_epoch: 1,
                    pool_last_used_epoch: Some(1),
                    descriptor_mismatch: false,
                },
            );
        }
        let reused = project_shadow_promotion_peak(&plan, &snapshot);
        assert_eq!(reused.incremental_resident_bytes, 0);
        assert_eq!(reused.projected_peak_bytes, 777);
    }

    #[test]
    fn gate_fallback_copies_legacy_state_and_has_no_diff() {
        let legacy = PromotionState {
            promoted_node_ids: FxHashSet::from_iter([7]),
            total_estimated_memory_bytes: 123,
            ..PromotionState::default()
        };
        let fallback = fallback_shadow_promotion_evaluation(
            &legacy,
            vec![ShadowEvaluationFallbackReason::PreviousParityNotReady],
        );
        assert!(!fallback.used_observed_cost);
        assert_eq!(fallback.state.promoted_node_ids, legacy.promoted_node_ids);
        assert_eq!(
            fallback.state.total_estimated_memory_bytes,
            legacy.total_estimated_memory_bytes
        );
        assert!(fallback.decision_diffs.is_empty());
        assert_eq!(
            fallback.fallback_reasons,
            vec![ShadowEvaluationFallbackReason::PreviousParityNotReady]
        );
    }

    fn rollout_readiness(
        observed: bool,
        prospective: bool,
        parity: bool,
    ) -> ShadowRasterBudgetReadiness {
        ShadowRasterBudgetReadiness {
            observed: ObservedSnapshotConsistency {
                consistent: observed,
                fallback_reasons: (!observed)
                    .then_some(RasterBudgetFallbackReason::NeverObserved)
                    .into_iter()
                    .collect(),
            },
            prospective_ready: prospective,
            parity_ready: parity,
            ready: observed && prospective && parity,
            prospective_fallback_reasons: (!prospective)
                .then_some(ProspectiveReadinessFallbackReason::PlanNotBuilt)
                .into_iter()
                .chain((!parity).then_some(ProspectiveReadinessFallbackReason::ParityNotChecked))
                .collect(),
        }
    }

    #[test]
    fn rollout_safety_streaks_advance_and_each_gate_failure_resets() {
        let legacy = PromotionState::default();
        let shadow = ShadowPromotionEvaluation::default();
        let full = rollout_readiness(true, true, true);
        let mut safety = ShadowRolloutSafetyState::default();
        safety.observe(&full, &legacy, &shadow);
        safety.observe(&full, &legacy, &shadow);
        assert_eq!(safety.consecutive_observed_consistent_frames, 2);
        assert_eq!(safety.consecutive_prospective_ready_frames, 2);
        assert_eq!(safety.consecutive_parity_compatible_frames, 2);
        assert_eq!(safety.consecutive_full_ready_frames, 2);

        safety.observe(&rollout_readiness(false, true, true), &legacy, &shadow);
        assert_eq!(safety.consecutive_observed_consistent_frames, 0);
        assert_eq!(safety.consecutive_full_ready_frames, 0);
        assert_eq!(
            safety.last.observed_fallback_reasons,
            vec![RasterBudgetFallbackReason::NeverObserved]
        );
        safety.observe(&rollout_readiness(true, false, true), &legacy, &shadow);
        assert_eq!(safety.consecutive_prospective_ready_frames, 0);
        assert_eq!(safety.consecutive_full_ready_frames, 0);
        safety.observe(&rollout_readiness(true, true, false), &legacy, &shadow);
        assert_eq!(safety.consecutive_parity_compatible_frames, 0);
        assert_eq!(safety.consecutive_full_ready_frames, 0);
        assert!(
            safety
                .last
                .prospective_fallback_reasons
                .contains(&ProspectiveReadinessFallbackReason::ParityNotChecked)
        );
    }

    #[test]
    fn rollout_observation_failure_and_trace_off_reset_or_preserve_correctly() {
        let legacy = PromotionState::default();
        let shadow = ShadowPromotionEvaluation::default();
        let full = rollout_readiness(true, true, true);
        let mut safety = ShadowRolloutSafetyState::default();
        safety.observe(&full, &legacy, &shadow);
        let before = safety.clone();
        safety.observe_if_enabled(
            false,
            &rollout_readiness(false, false, false),
            &legacy,
            &shadow,
        );
        assert_eq!(
            safety.consecutive_full_ready_frames,
            before.consecutive_full_ready_frames
        );
        assert_eq!(safety.last.full_ready, before.last.full_ready);
        safety.reset_after_observation_failure(RasterObservationStatus::StaleExecuteFailure);
        assert_eq!(safety.consecutive_observed_consistent_frames, 0);
        assert_eq!(safety.consecutive_parity_compatible_frames, 0);
        assert_eq!(safety.consecutive_full_ready_frames, 0);
        assert_eq!(safety.consecutive_prospective_ready_frames, 1);
        assert_eq!(
            safety.last.observed_fallback_reasons,
            vec![RasterBudgetFallbackReason::StaleObservation(
                RasterObservationStatus::StaleExecuteFailure
            )]
        );
    }

    #[test]
    fn rollout_eligibility_is_pure_and_requires_kill_switches_and_clean_costs() {
        let mut state = ShadowRolloutSafetyState {
            consecutive_full_ready_frames: 3,
            consecutive_parity_compatible_frames: 3,
            ..ShadowRolloutSafetyState::default()
        };
        let snapshot = RasterCacheSnapshot {
            all_costs_budget_usable: true,
            ..RasterCacheSnapshot::default()
        };
        let plan = ProspectiveRasterPlan {
            confidence: CostConfidence::Exact,
            ..ProspectiveRasterPlan::default()
        };
        let config = ShadowRolloutEligibilityConfig {
            required_full_ready_frames: 3,
            required_parity_frames: 3,
        };
        let before = state.clone();
        assert!(shadow_rollout_eligible(
            true,
            true,
            &state,
            &snapshot,
            Some(&plan),
            config
        ));
        assert!(!shadow_rollout_eligible(
            false,
            true,
            &state,
            &snapshot,
            Some(&plan),
            config
        ));
        assert!(!shadow_rollout_eligible(
            true,
            false,
            &state,
            &snapshot,
            Some(&plan),
            config
        ));
        let unknown_cost_snapshot = RasterCacheSnapshot {
            all_costs_budget_usable: false,
            ..snapshot.clone()
        };
        assert!(!shadow_rollout_eligible(
            true,
            true,
            &state,
            &unknown_cost_snapshot,
            Some(&plan),
            config
        ));
        let validation_snapshot = RasterCacheSnapshot {
            validation_errors: vec![RasterCacheValidationError::DuplicateDeclaredKey(
                RasterCacheKey(promoted_layer_stable_key(1)),
            )],
            ..snapshot.clone()
        };
        assert!(!shadow_rollout_eligible(
            true,
            true,
            &state,
            &validation_snapshot,
            Some(&plan),
            config
        ));
        assert_eq!(
            state.consecutive_full_ready_frames,
            before.consecutive_full_ready_frames
        );
        state.consecutive_full_ready_frames = 2;
        assert!(!shadow_rollout_eligible(
            true,
            true,
            &state,
            &snapshot,
            Some(&plan),
            config
        ));
    }
}
