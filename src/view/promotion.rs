#![allow(missing_docs)]

//! Layer-promotion scoring and diagnostic data exposed by the viewport.
use rustc_hash::{FxHashMap, FxHashSet};

use crate::transition::{
    CHANNEL_SCROLL_X, CHANNEL_SCROLL_Y, CHANNEL_STYLE_OPACITY, CHANNEL_VISUAL_X, CHANNEL_VISUAL_Y,
    ChannelId, TrackKey, TrackTarget, TransitionPluginId,
};
use std::cmp::Reverse;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PromotionNodeInfo {
    pub estimated_pass_count: u16,
    pub opacity: f32,
    pub has_rounded_clip: bool,
    pub has_box_shadow: bool,
    pub has_border: bool,
    pub is_scroll_container: bool,
    pub is_hovered: bool,
}

impl Default for PromotionNodeInfo {
    fn default() -> Self {
        Self {
            estimated_pass_count: 0,
            // Neutral hosts are fully opaque.  Zero made the default trait
            // implementation semantically transparent and also awarded an
            // unintended opacity-effect promotion score to custom subtrees.
            opacity: 1.0,
            has_rounded_clip: false,
            has_box_shadow: false,
            has_border: false,
            is_scroll_container: false,
            is_hovered: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromotionHardReason {
    ActiveOpacityAnimation,
    ActiveTransformAnimation,
    ActiveScrollLinkedMovement,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PromotionScoreBreakdown {
    pub effect_score: i32,
    pub subtree_complexity_score: i32,
    pub repaint_reuse_score: i32,
    pub animation_score: i32,
    pub interaction_score: i32,
    pub visibility_score: i32,
    pub area_cost: i32,
    pub reraster_risk: i32,
}

impl PromotionScoreBreakdown {
    pub fn total(self) -> i32 {
        self.effect_score
            + self.subtree_complexity_score
            + self.repaint_reuse_score
            + self.animation_score
            + self.interaction_score
            + self.visibility_score
            - self.area_cost
            - self.reraster_risk
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportPromotionConfig {
    pub enabled: bool,
    pub base_threshold: i32,
    pub max_layers: usize,
    pub max_surface_bytes_multiplier: f32,
    pub prefetch_viewport_distance: f32,
}

impl Default for ViewportPromotionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_threshold: 35,
            max_layers: 12,
            max_surface_bytes_multiplier: 12.0,
            prefetch_viewport_distance: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PromotionDecision {
    pub node_id: u64,
    pub parent_id: Option<u64>,
    pub score: i32,
    pub threshold: i32,
    pub should_promote: bool,
    pub hard_reason: Option<PromotionHardReason>,
    pub budget_rejection: Option<PromotionBudgetRejection>,
    pub breakdown: PromotionScoreBreakdown,
    pub subtree_node_count: usize,
    pub estimated_pass_count: usize,
    pub visible_area_ratio: f32,
    pub viewport_coverage: f32,
    pub distance_to_viewport: f32,
    pub estimated_memory_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromotionBudgetRejection {
    LayerLimit,
    SurfaceBytesLimit,
}

#[derive(Clone, Debug, Default)]
pub struct PromotionState {
    pub decisions: Vec<PromotionDecision>,
    pub promoted_node_ids: FxHashSet<u64>,
    pub total_estimated_memory_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromotedLayerUpdateKind {
    Reuse,
    Reraster,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromotedLayerUpdate {
    pub node_id: u64,
    pub parent_id: Option<u64>,
    pub kind: PromotedLayerUpdateKind,
    pub base_signature: u64,
    pub previous_base_signature: Option<u64>,
    pub composition_kind: PromotedLayerUpdateKind,
    pub composition_signature: u64,
    pub previous_composition_signature: Option<u64>,
    pub base_generation: u64,
    pub previous_base_generation: Option<u64>,
    pub composition_generation: u64,
    pub previous_composition_generation: Option<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct PromotionCandidate {
    pub node_id: u64,
    pub parent_id: Option<u64>,
    pub subtree_node_count: usize,
    pub estimated_pass_count: usize,
    pub visible_area_ratio: f32,
    pub viewport_coverage: f32,
    pub distance_to_viewport: f32,
    pub info: PromotionNodeInfo,
    pub base_memory_bytes: usize,
    pub composition_memory_bytes: usize,
    pub mask_memory_bytes: usize,
    pub has_active_animator: bool,
    pub has_composite_only_animator: bool,
    pub active_channels: FxHashSet<ChannelId>,
}

pub(crate) fn evaluate_promotion(
    candidates: Vec<PromotionCandidate>,
    viewport_size: (f32, f32),
    scale_factor: f32,
    config: ViewportPromotionConfig,
) -> PromotionState {
    if !config.enabled {
        return PromotionState::default();
    }
    let max_surface_bytes = estimate_surface_budget_bytes(viewport_size, scale_factor, config);
    let mut state = PromotionState::default();
    let topology_candidates = candidates
        .iter()
        .map(|candidate| (candidate.node_id, candidate.clone()))
        .collect::<FxHashMap<_, _>>();
    let mut accepted_order = Vec::new();

    let mut hard = Vec::new();
    let mut scored = Vec::new();
    for candidate in candidates {
        if let Some(reason) = hard_reason(&candidate.active_channels) {
            hard.push((candidate, reason));
        } else {
            scored.push(candidate);
        }
    }

    hard.sort_by(|(left, left_reason), (right, right_reason)| {
        hard_reason_priority(*left_reason)
            .cmp(&hard_reason_priority(*right_reason))
            .then_with(|| right.visible_area_ratio.total_cmp(&left.visible_area_ratio))
            .then_with(|| right.subtree_node_count.cmp(&left.subtree_node_count))
            .then_with(|| left.node_id.cmp(&right.node_id))
    });
    for (candidate, reason) in hard {
        let estimated_memory_bytes = candidate.base_memory_bytes;
        let budget_rejection = promotion_layer_rejection(&state, config);
        let should_promote = budget_rejection.is_none();
        if should_promote {
            state.total_estimated_memory_bytes = state
                .total_estimated_memory_bytes
                .saturating_add(estimated_memory_bytes);
            state.promoted_node_ids.insert(candidate.node_id);
            accepted_order.push(candidate.node_id);
        }
        state.decisions.push(PromotionDecision {
            node_id: candidate.node_id,
            parent_id: candidate.parent_id,
            score: 100,
            threshold: 0,
            should_promote,
            hard_reason: Some(reason),
            budget_rejection,
            breakdown: PromotionScoreBreakdown::default(),
            subtree_node_count: candidate.subtree_node_count,
            estimated_pass_count: candidate.estimated_pass_count,
            visible_area_ratio: candidate.visible_area_ratio,
            viewport_coverage: candidate.viewport_coverage,
            distance_to_viewport: candidate.distance_to_viewport,
            estimated_memory_bytes,
        });
    }

    let mut scored_with_breakdown = scored
        .into_iter()
        .map(|candidate| {
            let breakdown = score_candidate(&candidate);
            (candidate, breakdown)
        })
        .collect::<Vec<_>>();
    scored_with_breakdown.sort_by_key(|(candidate, breakdown)| {
        (
            Reverse(breakdown.total()),
            Reverse(candidate.subtree_node_count),
            Reverse(candidate.estimated_pass_count),
            candidate.node_id,
        )
    });

    for (candidate, breakdown) in scored_with_breakdown {
        let threshold =
            effective_threshold(&candidate, &state, max_surface_bytes, viewport_size, config);
        let score = breakdown.total();
        let estimated_memory_bytes = candidate.base_memory_bytes;
        let budget_rejection = (score >= threshold)
            .then(|| promotion_layer_rejection(&state, config))
            .flatten();
        let should_promote = score >= threshold && budget_rejection.is_none();
        if should_promote {
            state.promoted_node_ids.insert(candidate.node_id);
            accepted_order.push(candidate.node_id);
            state.total_estimated_memory_bytes = state
                .total_estimated_memory_bytes
                .saturating_add(estimated_memory_bytes);
        }
        state.decisions.push(PromotionDecision {
            node_id: candidate.node_id,
            parent_id: candidate.parent_id,
            score,
            threshold,
            should_promote,
            hard_reason: None,
            budget_rejection,
            breakdown,
            subtree_node_count: candidate.subtree_node_count,
            estimated_pass_count: candidate.estimated_pass_count,
            visible_area_ratio: candidate.visible_area_ratio,
            viewport_coverage: candidate.viewport_coverage,
            distance_to_viewport: candidate.distance_to_viewport,
            estimated_memory_bytes,
        });
    }

    state.total_estimated_memory_bytes =
        topology_memory_bytes(&state.promoted_node_ids, &topology_candidates);
    while state.total_estimated_memory_bytes > max_surface_bytes {
        let Some(node_id) = accepted_order.pop() else {
            break;
        };
        if !state.promoted_node_ids.remove(&node_id) {
            continue;
        }
        if let Some(decision) = state
            .decisions
            .iter_mut()
            .find(|decision| decision.node_id == node_id)
        {
            decision.should_promote = false;
            decision.budget_rejection = Some(PromotionBudgetRejection::SurfaceBytesLimit);
        }
        state.total_estimated_memory_bytes =
            topology_memory_bytes(&state.promoted_node_ids, &topology_candidates);
    }

    state.decisions.sort_by_key(|decision| decision.node_id);
    state
}

/// Shadow-only evaluator that preserves legacy desirability ordering while
/// replacing candidate-additive memory accounting with a full-set projected
/// peak supplied by the retained-raster planner.
#[cfg(test)]
pub(crate) fn evaluate_promotion_with_projected_surface_cost(
    candidates: Vec<PromotionCandidate>,
    viewport_size: (f32, f32),
    scale_factor: f32,
    config: ViewportPromotionConfig,
    mut projected_peak_for: impl FnMut(&FxHashSet<u64>) -> usize,
) -> PromotionState {
    if !config.enabled {
        return PromotionState::default();
    }
    let max_surface_bytes = estimate_surface_budget_bytes(viewport_size, scale_factor, config);
    let mut state = PromotionState::default();
    let mut hard = Vec::new();
    let mut scored = Vec::new();
    for candidate in candidates {
        if let Some(reason) = hard_reason(&candidate.active_channels) {
            hard.push((candidate, reason));
        } else {
            scored.push(candidate);
        }
    }
    hard.sort_by(|(left, left_reason), (right, right_reason)| {
        hard_reason_priority(*left_reason)
            .cmp(&hard_reason_priority(*right_reason))
            .then_with(|| right.visible_area_ratio.total_cmp(&left.visible_area_ratio))
            .then_with(|| right.subtree_node_count.cmp(&left.subtree_node_count))
            .then_with(|| left.node_id.cmp(&right.node_id))
    });
    for (candidate, reason) in hard {
        let (budget_rejection, projected_peak) = projected_admission(
            &state,
            candidate.node_id,
            config,
            max_surface_bytes,
            &mut projected_peak_for,
        );
        let should_promote = budget_rejection.is_none();
        if should_promote {
            state.promoted_node_ids.insert(candidate.node_id);
            state.total_estimated_memory_bytes = projected_peak;
        }
        state.decisions.push(PromotionDecision {
            node_id: candidate.node_id,
            parent_id: candidate.parent_id,
            score: 100,
            threshold: 0,
            should_promote,
            hard_reason: Some(reason),
            budget_rejection,
            breakdown: PromotionScoreBreakdown::default(),
            subtree_node_count: candidate.subtree_node_count,
            estimated_pass_count: candidate.estimated_pass_count,
            visible_area_ratio: candidate.visible_area_ratio,
            viewport_coverage: candidate.viewport_coverage,
            distance_to_viewport: candidate.distance_to_viewport,
            estimated_memory_bytes: projected_peak,
        });
    }

    let mut scored_with_breakdown = scored
        .into_iter()
        .map(|candidate| {
            let breakdown = score_candidate(&candidate);
            (candidate, breakdown)
        })
        .collect::<Vec<_>>();
    scored_with_breakdown.sort_by_key(|(candidate, breakdown)| {
        (
            Reverse(breakdown.total()),
            Reverse(candidate.subtree_node_count),
            Reverse(candidate.estimated_pass_count),
            candidate.node_id,
        )
    });
    for (candidate, breakdown) in scored_with_breakdown {
        let threshold =
            effective_threshold(&candidate, &state, max_surface_bytes, viewport_size, config);
        let score = breakdown.total();
        let (budget_rejection, projected_peak) = if score >= threshold {
            projected_admission(
                &state,
                candidate.node_id,
                config,
                max_surface_bytes,
                &mut projected_peak_for,
            )
        } else {
            (None, state.total_estimated_memory_bytes)
        };
        let should_promote = score >= threshold && budget_rejection.is_none();
        if should_promote {
            state.promoted_node_ids.insert(candidate.node_id);
            state.total_estimated_memory_bytes = projected_peak;
        }
        state.decisions.push(PromotionDecision {
            node_id: candidate.node_id,
            parent_id: candidate.parent_id,
            score,
            threshold,
            should_promote,
            hard_reason: None,
            budget_rejection,
            breakdown,
            subtree_node_count: candidate.subtree_node_count,
            estimated_pass_count: candidate.estimated_pass_count,
            visible_area_ratio: candidate.visible_area_ratio,
            viewport_coverage: candidate.viewport_coverage,
            distance_to_viewport: candidate.distance_to_viewport,
            estimated_memory_bytes: projected_peak,
        });
    }
    state.decisions.sort_by_key(|decision| decision.node_id);
    state
}

#[cfg(test)]
fn projected_admission(
    state: &PromotionState,
    node_id: u64,
    config: ViewportPromotionConfig,
    max_surface_bytes: usize,
    projected_peak_for: &mut impl FnMut(&FxHashSet<u64>) -> usize,
) -> (Option<PromotionBudgetRejection>, usize) {
    if state.promoted_node_ids.len() >= config.max_layers {
        return (
            Some(PromotionBudgetRejection::LayerLimit),
            state.total_estimated_memory_bytes,
        );
    }
    let mut tentative = state.promoted_node_ids.clone();
    tentative.insert(node_id);
    let projected_peak = projected_peak_for(&tentative);
    if projected_peak > max_surface_bytes {
        return (
            Some(PromotionBudgetRejection::SurfaceBytesLimit),
            projected_peak,
        );
    }
    (None, projected_peak)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShadowPolicyConfig {
    pub(crate) admission_score_margin: i32,
    pub(crate) retention_score_margin: i32,
    pub(crate) admission_stable_frames: u16,
    pub(crate) demotion_grace_frames: u16,
    pub(crate) admission_budget_ratio: f32,
    pub(crate) retention_budget_ratio: f32,
}

impl Default for ShadowPolicyConfig {
    fn default() -> Self {
        Self {
            admission_score_margin: 8,
            retention_score_margin: 6,
            admission_stable_frames: 3,
            demotion_grace_frames: 3,
            admission_budget_ratio: 0.75,
            retention_budget_ratio: 0.9,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ShadowPromotionPolicyState {
    pub(crate) promoted_node_ids: FxHashSet<u64>,
    pub(crate) admission_streaks: FxHashMap<u64, u16>,
    pub(crate) demotion_streaks: FxHashMap<u64, u16>,
}

impl ShadowPromotionPolicyState {
    pub(crate) fn sync_legacy_and_reset(&mut self, legacy: &PromotionState) {
        self.promoted_node_ids = legacy.promoted_node_ids.clone();
        self.admission_streaks.clear();
        self.demotion_streaks.clear();
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ShadowPolicyBudgetProjection {
    pub(crate) logical_planned_bytes: usize,
    pub(crate) projected_peak_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShadowPolicyTransitionKind {
    Admit,
    Retain,
    Drop,
    Reject,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShadowPolicyTransition {
    pub(crate) node_id: u64,
    pub(crate) kind: ShadowPolicyTransitionKind,
    pub(crate) admission_streak: u16,
    pub(crate) demotion_streak: u16,
    pub(crate) logical_planned_bytes: usize,
    pub(crate) projected_peak_bytes: usize,
    pub(crate) budget_rejection: Option<PromotionBudgetRejection>,
}

pub(crate) struct ShadowPolicyResult {
    pub(crate) state: PromotionState,
    pub(crate) transitions: Vec<ShadowPolicyTransition>,
}

pub(crate) fn evaluate_shadow_promotion_policy(
    candidates: Vec<PromotionCandidate>,
    policy_state: &mut ShadowPromotionPolicyState,
    policy: ShadowPolicyConfig,
    viewport_size: (f32, f32),
    scale_factor: f32,
    config: ViewportPromotionConfig,
    mut projection_for: impl FnMut(&FxHashSet<u64>) -> ShadowPolicyBudgetProjection,
) -> ShadowPolicyResult {
    struct Assessed {
        candidate: PromotionCandidate,
        hard_reason: Option<PromotionHardReason>,
        breakdown: PromotionScoreBreakdown,
        score: i32,
    }

    if !config.enabled {
        policy_state.promoted_node_ids.clear();
        policy_state.admission_streaks.clear();
        policy_state.demotion_streaks.clear();
        return ShadowPolicyResult {
            state: PromotionState::default(),
            transitions: Vec::new(),
        };
    }

    let max_surface_bytes = estimate_surface_budget_bytes(viewport_size, scale_factor, config);
    let low_water =
        (max_surface_bytes as f64 * policy.admission_budget_ratio.clamp(0.0, 1.0) as f64) as usize;
    let high_water =
        (max_surface_bytes as f64 * policy.retention_budget_ratio.clamp(0.0, 1.0) as f64) as usize;
    let present_ids = candidates
        .iter()
        .map(|candidate| candidate.node_id)
        .collect::<FxHashSet<_>>();
    policy_state
        .admission_streaks
        .retain(|node_id, _| present_ids.contains(node_id));
    policy_state
        .demotion_streaks
        .retain(|node_id, _| present_ids.contains(node_id));
    policy_state
        .promoted_node_ids
        .retain(|node_id| present_ids.contains(node_id));

    let mut assessed = candidates
        .into_iter()
        .map(|candidate| {
            let hard_reason = hard_reason(&candidate.active_channels);
            let breakdown = score_candidate(&candidate);
            let score = if hard_reason.is_some() {
                100
            } else {
                breakdown.total()
            };
            Assessed {
                candidate,
                hard_reason,
                breakdown,
                score,
            }
        })
        .collect::<Vec<_>>();
    assessed.sort_by(|left, right| {
        let left_group = if left.hard_reason.is_some() {
            0
        } else if policy_state
            .promoted_node_ids
            .contains(&left.candidate.node_id)
        {
            1
        } else {
            2
        };
        let right_group = if right.hard_reason.is_some() {
            0
        } else if policy_state
            .promoted_node_ids
            .contains(&right.candidate.node_id)
        {
            1
        } else {
            2
        };
        let group_order = left_group.cmp(&right_group);
        if group_order != std::cmp::Ordering::Equal {
            return group_order;
        }
        if let (Some(left_reason), Some(right_reason)) = (left.hard_reason, right.hard_reason) {
            return hard_reason_priority(left_reason)
                .cmp(&hard_reason_priority(right_reason))
                .then_with(|| {
                    right
                        .candidate
                        .visible_area_ratio
                        .total_cmp(&left.candidate.visible_area_ratio)
                })
                .then_with(|| {
                    right
                        .candidate
                        .subtree_node_count
                        .cmp(&left.candidate.subtree_node_count)
                })
                .then_with(|| left.candidate.node_id.cmp(&right.candidate.node_id));
        }
        right
            .score
            .cmp(&left.score)
            .then_with(|| {
                right
                    .candidate
                    .subtree_node_count
                    .cmp(&left.candidate.subtree_node_count)
            })
            .then_with(|| {
                right
                    .candidate
                    .estimated_pass_count
                    .cmp(&left.candidate.estimated_pass_count)
            })
            .then_with(|| left.candidate.node_id.cmp(&right.candidate.node_id))
    });

    let previous = policy_state.promoted_node_ids.clone();
    let mut state = PromotionState::default();
    let mut transitions = Vec::new();
    for assessed in assessed {
        let node_id = assessed.candidate.node_id;
        let was_retained = previous.contains(&node_id);
        let threshold = if assessed.hard_reason.is_some() {
            0
        } else {
            effective_threshold(
                &assessed.candidate,
                &state,
                max_surface_bytes,
                viewport_size,
                config,
            )
        };
        let mut should_promote = false;
        let mut budget_rejection = None;
        let mut projection = ShadowPolicyBudgetProjection::default();

        if assessed.hard_reason.is_some() {
            let mut tentative = state.promoted_node_ids.clone();
            tentative.insert(node_id);
            projection = projection_for(&tentative);
            let surface_cost = if was_retained {
                projection.logical_planned_bytes
            } else {
                projection.projected_peak_bytes
            };
            budget_rejection = if state.promoted_node_ids.len() >= config.max_layers {
                Some(PromotionBudgetRejection::LayerLimit)
            } else if surface_cost > max_surface_bytes {
                Some(PromotionBudgetRejection::SurfaceBytesLimit)
            } else {
                None
            };
            should_promote = budget_rejection.is_none();
            policy_state.admission_streaks.remove(&node_id);
            policy_state.demotion_streaks.remove(&node_id);
        } else if was_retained {
            let below_retention = assessed.score < threshold - policy.retention_score_margin;
            let demotion_streak = if below_retention {
                *policy_state
                    .demotion_streaks
                    .entry(node_id)
                    .and_modify(|streak| *streak = streak.saturating_add(1))
                    .or_insert(1)
            } else {
                policy_state.demotion_streaks.remove(&node_id);
                0
            };
            let grace_expired =
                below_retention && demotion_streak >= policy.demotion_grace_frames.max(1);
            if !grace_expired && state.promoted_node_ids.len() < config.max_layers {
                let mut tentative = state.promoted_node_ids.clone();
                tentative.insert(node_id);
                projection = projection_for(&tentative);
                should_promote = projection.logical_planned_bytes <= high_water
                    && projection.logical_planned_bytes <= max_surface_bytes;
                if !should_promote {
                    budget_rejection = Some(PromotionBudgetRejection::SurfaceBytesLimit);
                }
            } else if state.promoted_node_ids.len() >= config.max_layers {
                budget_rejection = Some(PromotionBudgetRejection::LayerLimit);
            }
            policy_state.admission_streaks.remove(&node_id);
        } else {
            policy_state.demotion_streaks.remove(&node_id);
            let eligible = assessed.score >= threshold + policy.admission_score_margin;
            let streak = if eligible {
                *policy_state
                    .admission_streaks
                    .entry(node_id)
                    .and_modify(|streak| *streak = streak.saturating_add(1))
                    .or_insert(1)
            } else {
                policy_state.admission_streaks.remove(&node_id);
                0
            };
            if eligible && streak >= policy.admission_stable_frames.max(1) {
                if state.promoted_node_ids.len() >= config.max_layers {
                    budget_rejection = Some(PromotionBudgetRejection::LayerLimit);
                } else {
                    let mut tentative = state.promoted_node_ids.clone();
                    tentative.insert(node_id);
                    projection = projection_for(&tentative);
                    should_promote = projection.projected_peak_bytes <= low_water
                        && projection.projected_peak_bytes <= max_surface_bytes;
                    if !should_promote {
                        budget_rejection = Some(PromotionBudgetRejection::SurfaceBytesLimit);
                    }
                }
            }
        }

        if should_promote {
            state.promoted_node_ids.insert(node_id);
            state.total_estimated_memory_bytes = projection.projected_peak_bytes;
            policy_state.admission_streaks.remove(&node_id);
        }
        let kind = match (was_retained, should_promote) {
            (false, true) => ShadowPolicyTransitionKind::Admit,
            (true, true) => ShadowPolicyTransitionKind::Retain,
            (true, false) => ShadowPolicyTransitionKind::Drop,
            (false, false) => ShadowPolicyTransitionKind::Reject,
        };
        transitions.push(ShadowPolicyTransition {
            node_id,
            kind,
            admission_streak: policy_state
                .admission_streaks
                .get(&node_id)
                .copied()
                .unwrap_or(0),
            demotion_streak: policy_state
                .demotion_streaks
                .get(&node_id)
                .copied()
                .unwrap_or(0),
            logical_planned_bytes: projection.logical_planned_bytes,
            projected_peak_bytes: projection.projected_peak_bytes,
            budget_rejection,
        });
        state.decisions.push(PromotionDecision {
            node_id,
            parent_id: assessed.candidate.parent_id,
            score: assessed.score,
            threshold,
            should_promote,
            hard_reason: assessed.hard_reason,
            budget_rejection,
            breakdown: assessed.breakdown,
            subtree_node_count: assessed.candidate.subtree_node_count,
            estimated_pass_count: assessed.candidate.estimated_pass_count,
            visible_area_ratio: assessed.candidate.visible_area_ratio,
            viewport_coverage: assessed.candidate.viewport_coverage,
            distance_to_viewport: assessed.candidate.distance_to_viewport,
            estimated_memory_bytes: projection.projected_peak_bytes,
        });
    }
    policy_state.promoted_node_ids = state.promoted_node_ids.clone();
    state.decisions.sort_by_key(|decision| decision.node_id);
    transitions.sort_by_key(|transition| transition.node_id);
    ShadowPolicyResult { state, transitions }
}

fn estimate_surface_budget_bytes(
    viewport_size: (f32, f32),
    scale_factor: f32,
    config: ViewportPromotionConfig,
) -> usize {
    let scale = scale_factor.max(0.0001);
    let viewport_area =
        (viewport_size.0.max(1.0) * scale).ceil() * (viewport_size.1.max(1.0) * scale).ceil();
    ((viewport_area * 4.0) * config.max_surface_bytes_multiplier.max(1.0)) as usize
}

fn hard_reason(active_channels: &FxHashSet<ChannelId>) -> Option<PromotionHardReason> {
    if active_channels.contains(&CHANNEL_STYLE_OPACITY) {
        return Some(PromotionHardReason::ActiveOpacityAnimation);
    }
    if active_channels.contains(&CHANNEL_VISUAL_X) || active_channels.contains(&CHANNEL_VISUAL_Y) {
        return Some(PromotionHardReason::ActiveTransformAnimation);
    }
    if active_channels.contains(&CHANNEL_SCROLL_X) || active_channels.contains(&CHANNEL_SCROLL_Y) {
        return Some(PromotionHardReason::ActiveScrollLinkedMovement);
    }
    None
}

fn hard_reason_priority(reason: PromotionHardReason) -> u8 {
    match reason {
        PromotionHardReason::ActiveOpacityAnimation => 0,
        PromotionHardReason::ActiveTransformAnimation => 1,
        PromotionHardReason::ActiveScrollLinkedMovement => 2,
    }
}

fn promotion_layer_rejection(
    state: &PromotionState,
    config: ViewportPromotionConfig,
) -> Option<PromotionBudgetRejection> {
    if state.promoted_node_ids.len() >= config.max_layers {
        return Some(PromotionBudgetRejection::LayerLimit);
    }
    None
}

fn topology_memory_bytes(
    promoted_node_ids: &FxHashSet<u64>,
    candidates: &FxHashMap<u64, PromotionCandidate>,
) -> usize {
    let mut ancestors_with_promoted_descendants = FxHashSet::default();
    for &node_id in promoted_node_ids {
        let mut parent_id = candidates
            .get(&node_id)
            .and_then(|candidate| candidate.parent_id);
        while let Some(parent) = parent_id {
            if !ancestors_with_promoted_descendants.insert(parent) {
                break;
            }
            parent_id = candidates
                .get(&parent)
                .and_then(|candidate| candidate.parent_id);
        }
    }

    candidates.values().fold(0usize, |total, candidate| {
        let has_promoted_descendant =
            ancestors_with_promoted_descendants.contains(&candidate.node_id);
        let base = promoted_node_ids
            .contains(&candidate.node_id)
            .then_some(candidate.base_memory_bytes)
            .unwrap_or(0);
        let composition = (promoted_node_ids.contains(&candidate.node_id)
            && has_promoted_descendant)
            .then_some(candidate.composition_memory_bytes)
            .unwrap_or(0);
        let mask = has_promoted_descendant
            .then_some(candidate.mask_memory_bytes)
            .unwrap_or(0);
        total
            .saturating_add(base)
            .saturating_add(composition)
            .saturating_add(mask)
    })
}

fn score_candidate(candidate: &PromotionCandidate) -> PromotionScoreBreakdown {
    let mut effect_score = 0;
    if candidate.info.has_box_shadow {
        effect_score += 14;
    }
    if candidate.info.has_rounded_clip {
        effect_score += 8;
    }
    if candidate.info.has_border {
        effect_score += 4;
    }
    if candidate.info.opacity < 0.999 && candidate.subtree_node_count > 1 {
        effect_score += 10;
    }

    let subtree_complexity_score = ((candidate.subtree_node_count as i32 - 1).max(0) * 2).min(14)
        + ((candidate.estimated_pass_count as i32 - 1).max(0)).min(10);

    let repaint_reuse_score = if candidate.subtree_node_count >= 3 {
        8 + ((candidate.estimated_pass_count as i32).min(4) * 2)
    } else {
        0
    };

    let animation_score = if candidate.has_active_animator {
        let mut score = if candidate.visible_area_ratio > 0.0 {
            12
        } else {
            6
        };
        if candidate.estimated_pass_count >= 3 {
            score += 4;
        }
        if candidate.subtree_node_count >= 6 {
            score += 4;
        }
        if candidate.has_composite_only_animator {
            score += 4;
        }
        score.min(20)
    } else {
        0
    };

    let mut interaction_score = 0;
    if candidate.info.is_scroll_container {
        interaction_score += 6;
    }
    if candidate.info.is_hovered {
        interaction_score += 4;
    }

    let visibility_score = if candidate.visible_area_ratio >= 0.75 {
        10
    } else if candidate.visible_area_ratio >= 0.25 {
        6
    } else if candidate.visible_area_ratio > 0.0 {
        2
    } else {
        0
    };

    let area_cost = if candidate.viewport_coverage >= 1.5 {
        35
    } else if candidate.viewport_coverage >= 1.0 {
        28
    } else if candidate.viewport_coverage >= 0.5 {
        18
    } else if candidate.viewport_coverage >= 0.25 {
        10
    } else {
        3
    };

    let mut reraster_risk = 0;
    if candidate.info.is_hovered {
        reraster_risk += 4;
    }
    if candidate.info.is_scroll_container && candidate.visible_area_ratio > 0.0 {
        reraster_risk += 6;
    }
    if candidate.active_channels.len() >= 2 {
        reraster_risk += 8;
    }

    PromotionScoreBreakdown {
        effect_score,
        subtree_complexity_score,
        repaint_reuse_score,
        animation_score,
        interaction_score,
        visibility_score,
        area_cost,
        reraster_risk,
    }
}

fn effective_threshold(
    candidate: &PromotionCandidate,
    state: &PromotionState,
    max_surface_bytes: usize,
    viewport_size: (f32, f32),
    config: ViewportPromotionConfig,
) -> i32 {
    let mut threshold = config.base_threshold;
    let used_layers = state.promoted_node_ids.len();
    let layer_pressure = if config.max_layers == 0 {
        1.0
    } else {
        used_layers as f32 / config.max_layers as f32
    };
    if layer_pressure >= 1.0 {
        threshold += 20;
    } else if layer_pressure >= 0.8 {
        threshold += 10;
    }

    let memory_pressure = if max_surface_bytes == 0 {
        1.0
    } else {
        state.total_estimated_memory_bytes as f32 / max_surface_bytes as f32
    };
    if memory_pressure >= 1.0 {
        threshold += 20;
    } else if memory_pressure >= 0.8 {
        threshold += 15;
    }

    let viewport_main_axis = viewport_size.0.max(viewport_size.1).max(1.0);
    let prefetch_limit = viewport_main_axis * config.prefetch_viewport_distance.max(0.0);
    if candidate.visible_area_ratio == 0.0 && candidate.distance_to_viewport > prefetch_limit {
        threshold += 20;
    } else if candidate.visible_area_ratio == 0.0 {
        threshold += 8;
    } else if candidate.visible_area_ratio >= 0.5 {
        threshold -= 5;
    }

    threshold.max(0)
}

pub(crate) fn active_channels_by_node(
    claims: &FxHashMap<TrackKey<TrackTarget>, TransitionPluginId>,
) -> FxHashMap<u64, FxHashSet<ChannelId>> {
    let mut out = FxHashMap::<u64, FxHashSet<ChannelId>>::default();
    for key in claims.keys() {
        out.entry(key.target).or_default().insert(key.channel);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_node_info_is_opaque_and_has_no_opacity_effect_score() {
        let info = PromotionNodeInfo::default();
        assert_eq!(info.opacity, 1.0);

        let mut neutral = candidate(99);
        neutral.info = info;
        assert_eq!(score_candidate(&neutral).effect_score, 0);
    }

    fn candidate(node_id: u64) -> PromotionCandidate {
        PromotionCandidate {
            node_id,
            parent_id: None,
            subtree_node_count: 8,
            estimated_pass_count: 6,
            visible_area_ratio: 1.0,
            viewport_coverage: 0.25,
            distance_to_viewport: 0.0,
            info: PromotionNodeInfo {
                estimated_pass_count: 6,
                opacity: 1.0,
                has_rounded_clip: true,
                has_box_shadow: true,
                has_border: true,
                is_scroll_container: false,
                is_hovered: false,
            },
            base_memory_bytes: 320 * 240 * 8,
            composition_memory_bytes: 0,
            mask_memory_bytes: 0,
            has_active_animator: false,
            has_composite_only_animator: false,
            active_channels: FxHashSet::default(),
        }
    }

    #[test]
    fn hard_reason_promotes_without_scoring() {
        let mut c = candidate(1);
        c.active_channels.insert(CHANNEL_STYLE_OPACITY);
        let state = evaluate_promotion(
            vec![c],
            (1280.0, 720.0),
            1.0,
            ViewportPromotionConfig::default(),
        );
        assert_eq!(state.decisions.len(), 1);
        assert!(state.decisions[0].should_promote);
        assert_eq!(
            state.decisions[0].hard_reason,
            Some(PromotionHardReason::ActiveOpacityAnimation)
        );
        assert_eq!(state.decisions[0].threshold, 0);
    }

    #[test]
    fn far_offscreen_candidate_gets_higher_threshold() {
        let mut near = candidate(1);
        near.visible_area_ratio = 0.0;
        near.distance_to_viewport = 100.0;

        let mut far = candidate(2);
        far.visible_area_ratio = 0.0;
        far.distance_to_viewport = 2000.0;

        let state = evaluate_promotion(
            vec![near, far],
            (1280.0, 720.0),
            1.0,
            ViewportPromotionConfig::default(),
        );
        let near = state.decisions.iter().find(|d| d.node_id == 1).unwrap();
        let far = state.decisions.iter().find(|d| d.node_id == 2).unwrap();
        assert!(far.threshold > near.threshold);
    }

    #[test]
    fn layer_budget_pressure_raises_threshold_for_later_candidates() {
        let config = ViewportPromotionConfig {
            max_layers: 1,
            ..ViewportPromotionConfig::default()
        };
        let state = evaluate_promotion(
            vec![candidate(1), candidate(2)],
            (1280.0, 720.0),
            1.0,
            config,
        );
        let first = state.decisions.iter().find(|d| d.node_id == 1).unwrap();
        let second = state.decisions.iter().find(|d| d.node_id == 2).unwrap();
        assert!(second.threshold >= first.threshold);
    }

    #[test]
    fn max_layers_is_a_hard_cap_for_scored_candidates() {
        let config = ViewportPromotionConfig {
            max_layers: 1,
            ..ViewportPromotionConfig::default()
        };
        let state = evaluate_promotion(
            vec![candidate(1), candidate(2)],
            (1280.0, 720.0),
            1.0,
            config,
        );

        assert_eq!(state.promoted_node_ids.len(), 1);
        let rejected = state
            .decisions
            .iter()
            .find(|decision| !decision.should_promote)
            .expect("one otherwise-qualified candidate should hit the layer cap");
        assert_eq!(
            rejected.budget_rejection,
            Some(PromotionBudgetRejection::LayerLimit)
        );
    }

    #[test]
    fn hard_candidates_obey_layer_cap_in_priority_order() {
        let mut opacity = candidate(1);
        opacity.active_channels.insert(CHANNEL_STYLE_OPACITY);
        let mut scroll = candidate(2);
        scroll.active_channels.insert(CHANNEL_SCROLL_Y);
        let config = ViewportPromotionConfig {
            max_layers: 1,
            ..ViewportPromotionConfig::default()
        };

        let state = evaluate_promotion(vec![scroll, opacity], (1280.0, 720.0), 1.0, config);

        assert_eq!(state.promoted_node_ids, FxHashSet::from_iter([1]));
        let scroll = state
            .decisions
            .iter()
            .find(|decision| decision.node_id == 2)
            .unwrap();
        assert_eq!(
            scroll.budget_rejection,
            Some(PromotionBudgetRejection::LayerLimit)
        );
    }

    #[test]
    fn surface_budget_rejects_candidate_that_would_exceed_it() {
        let mut oversized = candidate(1);
        // 100 x 100 viewport at the minimum 1x multiplier = 40,000 bytes.
        oversized.base_memory_bytes = 40_001;
        let config = ViewportPromotionConfig {
            max_surface_bytes_multiplier: 1.0,
            ..ViewportPromotionConfig::default()
        };

        let state = evaluate_promotion(vec![oversized], (100.0, 100.0), 1.0, config);

        assert!(state.promoted_node_ids.is_empty());
        assert_eq!(
            state.decisions[0].budget_rejection,
            Some(PromotionBudgetRejection::SurfaceBytesLimit)
        );
    }

    #[test]
    fn surface_budget_scales_with_physical_viewport_size() {
        let mut candidate_2x = candidate(1);
        candidate_2x.base_memory_bytes = 100_000;
        let config = ViewportPromotionConfig {
            max_surface_bytes_multiplier: 1.0,
            ..ViewportPromotionConfig::default()
        };

        let state_1x = evaluate_promotion(vec![candidate_2x.clone()], (100.0, 100.0), 1.0, config);
        let state_2x = evaluate_promotion(vec![candidate_2x], (100.0, 100.0), 2.0, config);

        assert!(state_1x.promoted_node_ids.is_empty());
        assert_eq!(state_2x.promoted_node_ids, FxHashSet::from_iter([1]));
    }

    #[test]
    fn topology_budget_charges_final_surface_only_for_promoted_descendants() {
        let mut parent = candidate(1);
        parent.base_memory_bytes = 10_000;
        parent.composition_memory_bytes = 25_000;
        let mut child = candidate(2);
        child.parent_id = Some(1);
        child.base_memory_bytes = 10_000;
        let config = ViewportPromotionConfig {
            max_surface_bytes_multiplier: 1.0,
            ..ViewportPromotionConfig::default()
        };

        // Both base targets fit in 40 KB, but the parent's final target only
        // exists when the child is also promoted, taking the topology to 45 KB.
        let state = evaluate_promotion(vec![parent, child], (100.0, 100.0), 1.0, config);

        assert_eq!(state.promoted_node_ids, FxHashSet::from_iter([1]));
        assert_eq!(state.total_estimated_memory_bytes, 10_000);
        let child = state
            .decisions
            .iter()
            .find(|decision| decision.node_id == 2)
            .unwrap();
        assert_eq!(
            child.budget_rejection,
            Some(PromotionBudgetRejection::SurfaceBytesLimit)
        );
    }

    #[test]
    fn active_animator_boosts_candidate_score_without_hard_promote() {
        let mut animated = candidate(1);
        animated.subtree_node_count = 1;
        animated.estimated_pass_count = 1;
        animated.viewport_coverage = 0.01;
        animated.info.estimated_pass_count = 1;
        animated.info.has_box_shadow = false;
        animated.info.has_border = false;
        animated.has_active_animator = true;

        let state = evaluate_promotion(
            vec![animated],
            (1280.0, 720.0),
            1.0,
            ViewportPromotionConfig::default(),
        );
        assert_eq!(state.decisions.len(), 1);
        let decision = &state.decisions[0];
        assert_eq!(decision.hard_reason, None);
        assert_eq!(decision.breakdown.animation_score, 12);
        assert!(decision.score > decision.breakdown.animation_score);
        assert!(!decision.should_promote);
    }

    #[test]
    fn active_animator_gets_bounded_complexity_bonus() {
        let mut animated = candidate(1);
        animated.has_active_animator = true;

        let state = evaluate_promotion(
            vec![animated],
            (1280.0, 720.0),
            1.0,
            ViewportPromotionConfig::default(),
        );
        assert_eq!(state.decisions.len(), 1);
        let decision = &state.decisions[0];
        assert_eq!(decision.breakdown.animation_score, 20);
    }

    #[test]
    fn composite_only_animator_gets_extra_bonus() {
        let mut animated = candidate(1);
        animated.subtree_node_count = 1;
        animated.estimated_pass_count = 1;
        animated.viewport_coverage = 0.01;
        animated.info.estimated_pass_count = 1;
        animated.info.has_box_shadow = false;
        animated.info.has_border = false;
        animated.has_active_animator = true;
        animated.has_composite_only_animator = true;

        let state = evaluate_promotion(
            vec![animated],
            (1280.0, 720.0),
            1.0,
            ViewportPromotionConfig::default(),
        );
        let decision = &state.decisions[0];
        assert_eq!(decision.breakdown.animation_score, 16);
    }

    #[test]
    fn projected_cost_recomputes_parent_child_set_and_blocks_peak() {
        let mut parent = candidate(1);
        let mut child = candidate(2);
        child.parent_id = Some(1);
        parent.active_channels.insert(CHANNEL_STYLE_OPACITY);
        child.active_channels.insert(CHANNEL_STYLE_OPACITY);
        let state = evaluate_promotion_with_projected_surface_cost(
            vec![parent, child],
            (10.0, 10.0),
            1.0,
            ViewportPromotionConfig {
                max_surface_bytes_multiplier: 1.0,
                ..Default::default()
            },
            |set| match set.len() {
                0 => 0,
                1 => 300,
                _ => 500,
            },
        );
        assert_eq!(state.promoted_node_ids, FxHashSet::from_iter([1]));
        assert_eq!(state.total_estimated_memory_bytes, 300);
        assert_eq!(
            state.decisions[1].budget_rejection,
            Some(PromotionBudgetRejection::SurfaceBytesLimit)
        );
    }

    #[test]
    fn projected_retiring_peak_blocks_admission_without_assuming_reclaim() {
        let mut hard = candidate(1);
        hard.active_channels.insert(CHANNEL_STYLE_OPACITY);
        let state = evaluate_promotion_with_projected_surface_cost(
            vec![hard],
            (10.0, 10.0),
            1.0,
            ViewportPromotionConfig {
                max_surface_bytes_multiplier: 1.0,
                ..Default::default()
            },
            |_| 401,
        );
        assert!(state.promoted_node_ids.is_empty());
        assert_eq!(
            state.decisions[0].budget_rejection,
            Some(PromotionBudgetRejection::SurfaceBytesLimit)
        );
    }

    #[test]
    fn projected_evaluator_preserves_hard_layer_cap_tie_order_and_legacy_state() {
        let mut second = candidate(2);
        let mut first = candidate(1);
        second.active_channels.insert(CHANNEL_STYLE_OPACITY);
        first.active_channels.insert(CHANNEL_STYLE_OPACITY);
        let candidates = vec![second, first];
        let legacy = evaluate_promotion(
            candidates.clone(),
            (100.0, 100.0),
            1.0,
            ViewportPromotionConfig {
                max_layers: 1,
                ..Default::default()
            },
        );
        let legacy_ids = legacy.promoted_node_ids.clone();
        let legacy_decisions = legacy.decisions.clone();
        let shadow = evaluate_promotion_with_projected_surface_cost(
            candidates,
            (100.0, 100.0),
            1.0,
            ViewportPromotionConfig {
                max_layers: 1,
                ..Default::default()
            },
            |_| 1,
        );
        assert_eq!(shadow.promoted_node_ids, FxHashSet::from_iter([1]));
        assert_eq!(
            shadow.decisions[1].budget_rejection,
            Some(PromotionBudgetRejection::LayerLimit)
        );
        assert_eq!(legacy.promoted_node_ids, legacy_ids);
        assert_eq!(legacy.decisions, legacy_decisions);
    }

    fn test_shadow_policy() -> ShadowPolicyConfig {
        ShadowPolicyConfig {
            admission_score_margin: 0,
            retention_score_margin: 0,
            admission_stable_frames: 2,
            demotion_grace_frames: 2,
            admission_budget_ratio: 0.5,
            retention_budget_ratio: 0.9,
        }
    }

    fn easy_candidate(node_id: u64) -> PromotionCandidate {
        let mut candidate = candidate(node_id);
        candidate.viewport_coverage = 0.01;
        candidate
    }

    fn weak_candidate(node_id: u64) -> PromotionCandidate {
        let mut candidate = candidate(node_id);
        candidate.subtree_node_count = 1;
        candidate.estimated_pass_count = 1;
        candidate.viewport_coverage = 2.0;
        candidate.info = PromotionNodeInfo::default();
        candidate
    }

    fn tiny_policy_projection(_: &FxHashSet<u64>) -> ShadowPolicyBudgetProjection {
        ShadowPolicyBudgetProjection {
            logical_planned_bytes: 10,
            projected_peak_bytes: 10,
        }
    }

    #[test]
    fn shadow_policy_requires_stable_admission_and_graceful_demotion() {
        let mut policy_state = ShadowPromotionPolicyState::default();
        let config = ViewportPromotionConfig {
            base_threshold: 0,
            max_surface_bytes_multiplier: 1.0,
            ..Default::default()
        };
        let first = evaluate_shadow_promotion_policy(
            vec![easy_candidate(1)],
            &mut policy_state,
            test_shadow_policy(),
            (100.0, 100.0),
            1.0,
            config,
            tiny_policy_projection,
        );
        assert!(first.state.promoted_node_ids.is_empty());
        assert_eq!(policy_state.admission_streaks.get(&1), Some(&1));
        let second = evaluate_shadow_promotion_policy(
            vec![easy_candidate(1)],
            &mut policy_state,
            test_shadow_policy(),
            (100.0, 100.0),
            1.0,
            config,
            tiny_policy_projection,
        );
        assert!(second.state.promoted_node_ids.contains(&1));

        let first_weak = evaluate_shadow_promotion_policy(
            vec![weak_candidate(1)],
            &mut policy_state,
            test_shadow_policy(),
            (100.0, 100.0),
            1.0,
            config,
            tiny_policy_projection,
        );
        assert!(first_weak.state.promoted_node_ids.contains(&1));
        let second_weak = evaluate_shadow_promotion_policy(
            vec![weak_candidate(1)],
            &mut policy_state,
            test_shadow_policy(),
            (100.0, 100.0),
            1.0,
            config,
            tiny_policy_projection,
        );
        assert!(second_weak.state.promoted_node_ids.is_empty());
    }

    #[test]
    fn shadow_policy_oscillation_resets_admission_streak() {
        let mut state = ShadowPromotionPolicyState::default();
        let config = ViewportPromotionConfig {
            base_threshold: 0,
            max_surface_bytes_multiplier: 1.0,
            ..Default::default()
        };
        evaluate_shadow_promotion_policy(
            vec![easy_candidate(1)],
            &mut state,
            test_shadow_policy(),
            (100.0, 100.0),
            1.0,
            config,
            tiny_policy_projection,
        );
        evaluate_shadow_promotion_policy(
            vec![weak_candidate(1)],
            &mut state,
            test_shadow_policy(),
            (100.0, 100.0),
            1.0,
            config,
            tiny_policy_projection,
        );
        let third = evaluate_shadow_promotion_policy(
            vec![easy_candidate(1)],
            &mut state,
            test_shadow_policy(),
            (100.0, 100.0),
            1.0,
            config,
            tiny_policy_projection,
        );
        assert!(third.state.promoted_node_ids.is_empty());
        assert_eq!(state.admission_streaks.get(&1), Some(&1));
    }

    #[test]
    fn shadow_policy_low_water_blocks_new_but_retiring_peak_does_not_drop_retained() {
        let mut state = ShadowPromotionPolicyState::default();
        let mut policy = test_shadow_policy();
        policy.admission_stable_frames = 1;
        let config = ViewportPromotionConfig {
            base_threshold: 0,
            max_surface_bytes_multiplier: 1.0,
            ..Default::default()
        };
        let rejected = evaluate_shadow_promotion_policy(
            vec![easy_candidate(1)],
            &mut state,
            policy,
            (10.0, 10.0),
            1.0,
            config,
            |_| ShadowPolicyBudgetProjection {
                logical_planned_bytes: 100,
                projected_peak_bytes: 250,
            },
        );
        assert!(rejected.state.promoted_node_ids.is_empty());

        state.promoted_node_ids.insert(1);
        let retained = evaluate_shadow_promotion_policy(
            vec![easy_candidate(1)],
            &mut state,
            policy,
            (10.0, 10.0),
            1.0,
            config,
            |_| ShadowPolicyBudgetProjection {
                logical_planned_bytes: 100,
                projected_peak_bytes: 10_000,
            },
        );
        assert!(retained.state.promoted_node_ids.contains(&1));
    }

    #[test]
    fn shadow_policy_hard_is_immediate_but_layer_capped_and_high_water_drops_low_priority() {
        let mut state = ShadowPromotionPolicyState::default();
        let mut first = easy_candidate(1);
        let mut second = easy_candidate(2);
        first.active_channels.insert(CHANNEL_STYLE_OPACITY);
        second.active_channels.insert(CHANNEL_STYLE_OPACITY);
        let hard = evaluate_shadow_promotion_policy(
            vec![second, first],
            &mut state,
            ShadowPolicyConfig::default(),
            (100.0, 100.0),
            1.0,
            ViewportPromotionConfig {
                max_layers: 1,
                ..Default::default()
            },
            |_| ShadowPolicyBudgetProjection {
                logical_planned_bytes: 1,
                projected_peak_bytes: 1,
            },
        );
        assert_eq!(hard.state.promoted_node_ids, FxHashSet::from_iter([1]));
        assert_eq!(
            hard.state.decisions[1].budget_rejection,
            Some(PromotionBudgetRejection::LayerLimit)
        );

        state.promoted_node_ids = FxHashSet::from_iter([1, 2]);
        let mut high_policy = test_shadow_policy();
        high_policy.retention_budget_ratio = 0.5;
        let retained = evaluate_shadow_promotion_policy(
            vec![easy_candidate(2), easy_candidate(1)],
            &mut state,
            high_policy,
            (10.0, 10.0),
            1.0,
            ViewportPromotionConfig {
                base_threshold: 0,
                max_surface_bytes_multiplier: 1.0,
                ..Default::default()
            },
            |set| ShadowPolicyBudgetProjection {
                logical_planned_bytes: set.len() * 150,
                projected_peak_bytes: set.len() * 150,
            },
        );
        assert_eq!(retained.state.promoted_node_ids.len(), 1);
    }

    #[test]
    fn shadow_policy_disappearance_and_fallback_clear_streaks() {
        let mut state = ShadowPromotionPolicyState::default();
        state.admission_streaks.insert(1, 2);
        state.demotion_streaks.insert(2, 2);
        state.promoted_node_ids.insert(2);
        evaluate_shadow_promotion_policy(
            Vec::new(),
            &mut state,
            test_shadow_policy(),
            (100.0, 100.0),
            1.0,
            ViewportPromotionConfig::default(),
            |_| ShadowPolicyBudgetProjection::default(),
        );
        assert!(state.admission_streaks.is_empty());
        assert!(state.demotion_streaks.is_empty());
        assert!(state.promoted_node_ids.is_empty());

        state.admission_streaks.insert(3, 2);
        let legacy = PromotionState {
            promoted_node_ids: FxHashSet::from_iter([9]),
            ..PromotionState::default()
        };
        state.sync_legacy_and_reset(&legacy);
        assert_eq!(state.promoted_node_ids, legacy.promoted_node_ids);
        assert!(state.admission_streaks.is_empty());
        assert!(state.demotion_streaks.is_empty());
    }

    #[test]
    fn shadow_policy_disabled_clears_state_and_rejects_hard_candidates() {
        let mut state = ShadowPromotionPolicyState {
            promoted_node_ids: FxHashSet::from_iter([99]),
            admission_streaks: FxHashMap::from_iter([(1, 2)]),
            demotion_streaks: FxHashMap::from_iter([(99, 2)]),
        };
        let mut hard = easy_candidate(1);
        hard.active_channels.insert(CHANNEL_STYLE_OPACITY);
        let result = evaluate_shadow_promotion_policy(
            vec![hard],
            &mut state,
            ShadowPolicyConfig::default(),
            (100.0, 100.0),
            1.0,
            ViewportPromotionConfig {
                enabled: false,
                ..Default::default()
            },
            tiny_policy_projection,
        );
        assert!(result.state.promoted_node_ids.is_empty());
        assert!(result.state.decisions.is_empty());
        assert!(state.promoted_node_ids.is_empty());
        assert!(state.admission_streaks.is_empty());
        assert!(state.demotion_streaks.is_empty());
    }

    #[test]
    fn shadow_policy_hard_sort_prefers_visibility_before_subtree_under_cap() {
        let mut state = ShadowPromotionPolicyState::default();
        let mut more_visible = easy_candidate(10);
        more_visible.visible_area_ratio = 0.8;
        more_visible.subtree_node_count = 1;
        more_visible.estimated_pass_count = 1;
        more_visible.active_channels.insert(CHANNEL_STYLE_OPACITY);
        let mut larger_subtree = easy_candidate(20);
        larger_subtree.visible_area_ratio = 0.7;
        larger_subtree.subtree_node_count = 100;
        larger_subtree.estimated_pass_count = 100;
        larger_subtree.active_channels.insert(CHANNEL_STYLE_OPACITY);
        let result = evaluate_shadow_promotion_policy(
            vec![larger_subtree, more_visible],
            &mut state,
            ShadowPolicyConfig::default(),
            (100.0, 100.0),
            1.0,
            ViewportPromotionConfig {
                max_layers: 1,
                ..Default::default()
            },
            tiny_policy_projection,
        );
        assert_eq!(result.state.promoted_node_ids, FxHashSet::from_iter([10]));
        assert_eq!(
            result
                .state
                .decisions
                .iter()
                .find(|decision| decision.node_id == 20)
                .expect("hard candidate decision")
                .budget_rejection,
            Some(PromotionBudgetRejection::LayerLimit)
        );
    }
}
