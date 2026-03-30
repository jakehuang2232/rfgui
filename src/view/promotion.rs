#![allow(missing_docs)]

//! Layer-promotion scoring and diagnostic data exposed by the viewport.

use crate::transition::{
    CHANNEL_SCROLL_X, CHANNEL_SCROLL_Y, CHANNEL_STYLE_OPACITY, CHANNEL_VISUAL_X, CHANNEL_VISUAL_Y,
    ChannelId, TrackKey, TrackTarget, TransitionPluginId,
};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PromotionNodeInfo {
    pub estimated_pass_count: u16,
    pub opacity: f32,
    pub has_rounded_clip: bool,
    pub has_box_shadow: bool,
    pub has_border: bool,
    pub is_scroll_container: bool,
    pub is_hovered: bool,
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
    pub breakdown: PromotionScoreBreakdown,
    pub subtree_node_count: usize,
    pub estimated_pass_count: usize,
    pub visible_area_ratio: f32,
    pub viewport_coverage: f32,
    pub distance_to_viewport: f32,
    pub estimated_memory_bytes: usize,
}

#[derive(Clone, Debug, Default)]
pub struct PromotionState {
    pub decisions: Vec<PromotionDecision>,
    pub promoted_node_ids: HashSet<u64>,
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
}

#[derive(Clone, Debug)]
pub(crate) struct PromotionCandidate {
    pub node_id: u64,
    pub parent_id: Option<u64>,
    pub width: f32,
    pub height: f32,
    pub subtree_node_count: usize,
    pub estimated_pass_count: usize,
    pub visible_area_ratio: f32,
    pub viewport_coverage: f32,
    pub distance_to_viewport: f32,
    pub info: PromotionNodeInfo,
    pub active_channels: HashSet<ChannelId>,
}

pub(crate) fn evaluate_promotion(
    candidates: Vec<PromotionCandidate>,
    viewport_size: (f32, f32),
    config: ViewportPromotionConfig,
) -> PromotionState {
    if !config.enabled {
        return PromotionState::default();
    }
    let max_surface_bytes = estimate_surface_budget_bytes(viewport_size, config);
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

    hard.sort_by_key(|(candidate, _)| Reverse(candidate.subtree_node_count));
    for (candidate, reason) in hard {
        let estimated_memory_bytes = estimate_memory_bytes(candidate.width, candidate.height);
        state.total_estimated_memory_bytes = state
            .total_estimated_memory_bytes
            .saturating_add(estimated_memory_bytes);
        state.promoted_node_ids.insert(candidate.node_id);
        state.decisions.push(PromotionDecision {
            node_id: candidate.node_id,
            parent_id: candidate.parent_id,
            score: 100,
            threshold: 0,
            should_promote: true,
            hard_reason: Some(reason),
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
        )
    });

    for (candidate, breakdown) in scored_with_breakdown {
        let threshold =
            effective_threshold(&candidate, &state, max_surface_bytes, viewport_size, config);
        let score = breakdown.total();
        let estimated_memory_bytes = estimate_memory_bytes(candidate.width, candidate.height);
        let should_promote = score >= threshold;
        if should_promote {
            state.promoted_node_ids.insert(candidate.node_id);
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
            breakdown,
            subtree_node_count: candidate.subtree_node_count,
            estimated_pass_count: candidate.estimated_pass_count,
            visible_area_ratio: candidate.visible_area_ratio,
            viewport_coverage: candidate.viewport_coverage,
            distance_to_viewport: candidate.distance_to_viewport,
            estimated_memory_bytes,
        });
    }

    state.decisions.sort_by_key(|decision| decision.node_id);
    state
}

fn estimate_surface_budget_bytes(
    viewport_size: (f32, f32),
    config: ViewportPromotionConfig,
) -> usize {
    let viewport_area = viewport_size.0.max(1.0) * viewport_size.1.max(1.0);
    ((viewport_area * 4.0) * config.max_surface_bytes_multiplier.max(1.0)) as usize
}

fn hard_reason(active_channels: &HashSet<ChannelId>) -> Option<PromotionHardReason> {
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

fn estimate_memory_bytes(width: f32, height: f32) -> usize {
    (width.max(1.0) * height.max(1.0) * 4.0).round() as usize
}

pub(crate) fn active_channels_by_node(
    claims: &HashMap<TrackKey<TrackTarget>, TransitionPluginId>,
) -> HashMap<u64, HashSet<ChannelId>> {
    let mut out = HashMap::<u64, HashSet<ChannelId>>::new();
    for key in claims.keys() {
        out.entry(key.target).or_default().insert(key.channel);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(node_id: u64) -> PromotionCandidate {
        PromotionCandidate {
            node_id,
            parent_id: None,
            width: 320.0,
            height: 240.0,
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
            active_channels: HashSet::new(),
        }
    }

    #[test]
    fn hard_reason_promotes_without_scoring() {
        let mut c = candidate(1);
        c.active_channels.insert(CHANNEL_STYLE_OPACITY);
        let state =
            evaluate_promotion(vec![c], (1280.0, 720.0), ViewportPromotionConfig::default());
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
        let state = evaluate_promotion(vec![candidate(1), candidate(2)], (1280.0, 720.0), config);
        let first = state.decisions.iter().find(|d| d.node_id == 1).unwrap();
        let second = state.decisions.iter().find(|d| d.node_id == 2).unwrap();
        assert!(second.threshold >= first.threshold);
    }
}
