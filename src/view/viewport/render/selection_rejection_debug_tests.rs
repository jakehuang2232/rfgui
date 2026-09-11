//! Mapping from candidate rejections onto observational debug records.
//!
//! Only artifact rejections populate `legacy_debug_boundaries`, so without
//! these records every ladder path that returns Legacy before reaching the
//! artifact candidate produces a whole-frame fallback that names no component.

use super::{PaintAuthoritySelectionRejection, selection_rejection_debug_records};
use crate::view::debug::DebugFallbackStage;
use crate::view::debug::census::fallback_detail_label;
use crate::view::paint::{
    FrameArtifactEligibility, FrameArtifactFallbackReason, LegacyPaintReason,
};

fn codes(rejection: &PaintAuthoritySelectionRejection) -> Vec<String> {
    selection_rejection_debug_records(rejection)
        .into_iter()
        .map(|record| fallback_detail_label(&record.detail))
        .collect()
}

#[test]
fn artifact_rejections_emit_non_boundary_reasons_at_selection_stage() {
    let mut arena = crate::view::node_arena::NodeArena::new();
    let owner = arena.insert(crate::view::node_arena::Node::new(Box::new(
        crate::view::base_component::Element::new_with_id(1, 0.0, 0.0, 10.0, 10.0),
    )));
    let eligibility = FrameArtifactEligibility {
        reasons: vec![
            FrameArtifactFallbackReason::LegacyBoundary(LegacyPaintReason::UnknownHost),
            FrameArtifactFallbackReason::PropertyBoundary(owner),
            FrameArtifactFallbackReason::RootCount(2),
        ],
        ..FrameArtifactEligibility::default()
    };

    for rejection in [
        PaintAuthoritySelectionRejection::Auto(super::AutoAuthorityRejection::Artifact {
            eligibility: eligibility.clone(),
        }),
        PaintAuthoritySelectionRejection::Artifact(eligibility),
    ] {
        let records = selection_rejection_debug_records(&rejection);
        assert_eq!(records.len(), 2);
        assert!(
            records
                .iter()
                .all(|record| record.stage == DebugFallbackStage::Selection)
        );
        assert_eq!(records[0].owner, Some(owner));
        assert_eq!(records[1].owner, None);
        assert_eq!(
            records
                .iter()
                .map(|record| fallback_detail_label(&record.detail))
                .collect::<Vec<_>>(),
            vec!["property-boundary".to_string(), "root-count".to_string(),]
        );
    }
}

/// The census-only coverage pass may add records, never duplicate them.
mod census_coverage_dedupe {
    use super::super::census_coverage_fallback_additions;
    use crate::view::debug::{
        DebugFallbackCategory, DebugFallbackDetail, DebugFallbackStage,
        DebugRetainedAutoFallbackCaptureInput,
    };
    use crate::view::node_arena::{Node, NodeArena, NodeKey};
    use crate::view::paint::{CoverageOrder, LegacyPaintReason, PaintCoverageItem, PaintNodePhase};

    fn keys(count: usize) -> (NodeArena, Vec<NodeKey>) {
        let mut arena = NodeArena::new();
        let keys = (0..count)
            .map(|index| {
                arena.insert(Node::new(Box::new(
                    crate::view::base_component::Element::new_with_id(
                        index as u64 + 1,
                        0.0,
                        0.0,
                        10.0,
                        10.0,
                    ),
                )))
            })
            .collect();
        (arena, keys)
    }

    fn boundary(root: NodeKey, reason: LegacyPaintReason) -> PaintCoverageItem {
        PaintCoverageItem::LegacyBoundary {
            order: CoverageOrder {
                root_index: 0,
                child_path: Vec::new(),
                phase: PaintNodePhase::BeforeChildren,
                slot: 0,
            },
            root,
            stable_id: 0,
            reason,
        }
    }

    fn existing(owner: NodeKey) -> DebugRetainedAutoFallbackCaptureInput {
        DebugRetainedAutoFallbackCaptureInput {
            stage: DebugFallbackStage::Selection,
            category: DebugFallbackCategory::UnsupportedHost,
            detail: DebugFallbackDetail::Boundary {
                reason: "unknown-host",
            },
            owner: Some(owner),
            stable_id: None,
            element_type: None,
            bounds: None,
        }
    }

    fn no_identity(_: NodeKey) -> Option<(u64, &'static str, crate::view::debug::DebugRect)> {
        None
    }

    #[test]
    fn an_owner_already_reported_by_another_path_is_not_added_again() {
        let (_arena, keys) = keys(1);
        let items = vec![boundary(keys[0], LegacyPaintReason::ScrollContainer)];

        let additions =
            census_coverage_fallback_additions(&items, &[existing(keys[0])], no_identity);

        assert!(
            additions.is_empty(),
            "the artifact path already reported this owner"
        );
    }

    #[test]
    fn an_owner_reported_only_by_the_walk_is_added_once() {
        let (_arena, keys) = keys(2);
        let items = vec![
            boundary(keys[0], LegacyPaintReason::ScrollContainer),
            boundary(keys[1], LegacyPaintReason::ChildClip),
        ];

        let additions =
            census_coverage_fallback_additions(&items, &[existing(keys[0])], no_identity);

        assert_eq!(additions.len(), 1);
        assert_eq!(additions[0].owner, Some(keys[1]));
    }

    #[test]
    fn a_repeated_owner_inside_one_walk_is_added_once() {
        let (_arena, keys) = keys(1);
        let items = vec![
            boundary(keys[0], LegacyPaintReason::ScrollContainer),
            boundary(keys[0], LegacyPaintReason::ChildClip),
        ];

        let additions = census_coverage_fallback_additions(&items, &[], no_identity);

        assert_eq!(additions.len(), 1);
    }

    #[test]
    fn non_boundary_coverage_items_contribute_nothing() {
        let (_arena, keys) = keys(1);
        let items = vec![PaintCoverageItem::TransparentNode {
            order: CoverageOrder {
                root_index: 0,
                child_path: Vec::new(),
                phase: PaintNodePhase::BeforeChildren,
                slot: 0,
            },
            owner: keys[0],
            stable_id: 0,
            properties: Default::default(),
            content_revision: crate::view::paint::PaintContentRevision {
                self_paint_revision: 0,
                composite_revision: 0,
                topology_revision: 0,
            },
        }];

        assert!(census_coverage_fallback_additions(&items, &[], no_identity).is_empty());
    }

    #[test]
    fn additions_are_stamped_as_recording_stage_boundaries() {
        let (_arena, keys) = keys(1);
        let items = vec![boundary(keys[0], LegacyPaintReason::ScrollContainer)];

        let additions = census_coverage_fallback_additions(&items, &[], no_identity);

        assert_eq!(additions[0].stage, DebugFallbackStage::Recording);
        assert_eq!(
            additions[0].category,
            DebugFallbackCategory::PropertyTopology
        );
    }
}

/// Turning census capture on must not move any decision.
///
/// The census flag reaches authority selection through
/// `capture_paint_authority_telemetry`, which is threaded into
/// `AutoAuthorityTrace` and the circuit breaker. Those are the only two places
/// the flag can touch selection, so purity is proven where it enters.
mod capture_flag_is_decision_neutral {
    use super::super::{
        AutoAuthorityRejection, AutoAuthorityTrace, FramePaintSelection, RetainedAutoDecision,
        RetainedAutoTerminalFailureStage, retained_auto_circuit_breaker_selection,
    };

    use std::cell::Cell;

    fn rejection() -> AutoAuthorityRejection {
        AutoAuthorityRejection::Artifact {
            eligibility: crate::view::paint::FrameArtifactEligibility {
                reasons: vec![crate::view::paint::FrameArtifactFallbackReason::RootCount(
                    0,
                )],
                ..Default::default()
            },
        }
    }

    #[test]
    fn capture_off_never_evaluates_the_rejection_closure() {
        // A closure that ran only when capture is on would make the flag
        // observable to anything the closure touches.
        let evaluated = Cell::new(0);
        let mut trace = AutoAuthorityTrace::new(false);

        for _ in 0..3 {
            trace.capture(|| {
                evaluated.set(evaluated.get() + 1);
                rejection()
            });
        }

        assert_eq!(evaluated.get(), 0);
        assert!(trace.rejections.is_empty());
    }

    #[test]
    fn capture_on_records_every_rejection_in_order() {
        let mut trace = AutoAuthorityTrace::new(true);

        trace.capture(rejection);
        trace.capture(rejection);

        assert_eq!(trace.rejections.len(), 2);
    }

    #[test]
    fn the_circuit_breaker_decides_the_same_with_capture_on_or_off() {
        for stage in [
            RetainedAutoTerminalFailureStage::Compile,
            RetainedAutoTerminalFailureStage::Execute,
        ] {
            let off = retained_auto_circuit_breaker_selection(Some(stage), false);
            let on = retained_auto_circuit_breaker_selection(Some(stage), true);

            assert!(matches!(
                off,
                Some(FramePaintSelection::Auto(
                    RetainedAutoDecision::Legacy { .. }
                ))
            ));
            assert!(matches!(
                on,
                Some(FramePaintSelection::Auto(
                    RetainedAutoDecision::Legacy { .. }
                ))
            ));
        }

        assert!(retained_auto_circuit_breaker_selection(None, false).is_none());
        assert!(retained_auto_circuit_breaker_selection(None, true).is_none());
    }
}

/// Live-snapshot drift the planner could not report.
mod census_live_snapshot_additions {
    use super::super::census_live_snapshot_fallback_additions;
    use crate::view::compositor::paint_generation::{LiveSnapshotField, LiveSnapshotMismatch};
    use crate::view::debug::{
        DebugFallbackCategory, DebugFallbackDetail, DebugFallbackStage,
        DebugRetainedAutoFallbackCaptureInput,
    };
    use crate::view::node_arena::{Node, NodeArena, NodeKey};

    fn keys(count: usize) -> (NodeArena, Vec<NodeKey>) {
        let mut arena = NodeArena::new();
        let keys = (0..count)
            .map(|index| {
                arena.insert(Node::new(Box::new(
                    crate::view::base_component::Element::new_with_id(
                        index as u64 + 1,
                        0.0,
                        0.0,
                        10.0,
                        10.0,
                    ),
                )))
            })
            .collect();
        (arena, keys)
    }

    fn no_identity(_: NodeKey) -> Option<(u64, &'static str, crate::view::debug::DebugRect)> {
        None
    }

    fn planner_record(
        owner: Option<NodeKey>,
        field: LiveSnapshotField,
    ) -> DebugRetainedAutoFallbackCaptureInput {
        DebugRetainedAutoFallbackCaptureInput {
            stage: DebugFallbackStage::Planning,
            category: DebugFallbackCategory::Validation,
            detail: DebugFallbackDetail::Code { code: field.code() },
            owner,
            stable_id: None,
            element_type: None,
            bounds: None,
        }
    }

    #[test]
    fn every_drifting_node_becomes_one_record() {
        let (_arena, keys) = keys(2);
        let mismatches = [
            LiveSnapshotMismatch {
                owner: Some(keys[0]),
                field: LiveSnapshotField::Children,
            },
            LiveSnapshotMismatch {
                owner: Some(keys[1]),
                field: LiveSnapshotField::SelfSignature,
            },
        ];

        let additions = census_live_snapshot_fallback_additions(&mismatches, &[], no_identity);

        assert_eq!(additions.len(), 2);
        assert!(
            additions
                .iter()
                .all(|record| record.stage == DebugFallbackStage::Planning
                    && record.category == DebugFallbackCategory::Validation)
        );
    }

    #[test]
    fn the_node_the_planner_already_named_is_not_repeated() {
        let (_arena, keys) = keys(1);
        let mismatches = [LiveSnapshotMismatch {
            owner: Some(keys[0]),
            field: LiveSnapshotField::SelfSignature,
        }];

        let additions = census_live_snapshot_fallback_additions(
            &mismatches,
            &[planner_record(
                Some(keys[0]),
                LiveSnapshotField::SelfSignature,
            )],
            no_identity,
        );

        assert!(additions.is_empty());
    }

    #[test]
    fn a_planner_record_naming_its_candidate_still_dedupes() {
        // The planner wraps its code with the grammar that raised it, so the
        // same drift arrives as `CandidateCode` while this pass produces a
        // bare `Code`. Comparing whole details would double count it.
        let (_arena, keys) = keys(1);
        let mismatches = [LiveSnapshotMismatch {
            owner: Some(keys[0]),
            field: LiveSnapshotField::SelfSignature,
        }];
        let planner = DebugRetainedAutoFallbackCaptureInput {
            detail: DebugFallbackDetail::CandidateCode {
                candidate: "property-boundary-dag",
                code: LiveSnapshotField::SelfSignature.code(),
            },
            ..planner_record(Some(keys[0]), LiveSnapshotField::SelfSignature)
        };

        let additions =
            census_live_snapshot_fallback_additions(&mismatches, &[planner], no_identity);

        assert!(additions.is_empty());
    }

    #[test]
    fn the_same_owner_drifting_on_a_different_field_is_still_reported() {
        let (_arena, keys) = keys(1);
        let mismatches = [LiveSnapshotMismatch {
            owner: Some(keys[0]),
            field: LiveSnapshotField::Children,
        }];

        let additions = census_live_snapshot_fallback_additions(
            &mismatches,
            &[DebugRetainedAutoFallbackCaptureInput {
                detail: DebugFallbackDetail::CandidateCode {
                    candidate: "property-boundary-dag",
                    code: LiveSnapshotField::SelfSignature.code(),
                },
                ..planner_record(Some(keys[0]), LiveSnapshotField::SelfSignature)
            }],
            no_identity,
        );

        assert_eq!(
            additions.len(),
            1,
            "a different field is a different invariant"
        );
    }

    #[test]
    fn a_whole_scene_mismatch_reports_no_owner() {
        let mismatches = [LiveSnapshotMismatch {
            owner: None,
            field: LiveSnapshotField::ObservedRoots,
        }];

        let additions = census_live_snapshot_fallback_additions(&mismatches, &[], no_identity);

        assert_eq!(additions.len(), 1);
        assert_eq!(additions[0].owner, None);
    }
}
