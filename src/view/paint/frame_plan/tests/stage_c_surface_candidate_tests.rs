//! C2a artifact-derived surface-kind and clip-rebase mapping.
//!
//! Receiver reconstruction is intentionally absent from this batch. These
//! tests freeze only the mapping that is shared by both sides of the pending
//! nested-scroll projected-state decision.

use super::*;
use crate::view::compositor::property_tree::{
    ClipNodeId, ClipNodeRole, EffectNodeId, ScrollNodeId, TransformNodeId,
};
use crate::view::paint::{
    LayerizationPolicy, SurfaceDagError, SurfaceDagNodeKind, SurfaceDagTargetId,
    derive_artifact_surface_candidates,
};

fn surface_target_name(target: SurfaceDagTargetId) -> &'static str {
    match target {
        SurfaceDagTargetId::SceneRoot(_) => "scene-root",
        SurfaceDagTargetId::Surface(_) => "surface",
    }
}

#[test]
fn stage_c_surface_dag_type_domains_are_exhaustive() {
    fn policy_name(policy: LayerizationPolicy) -> &'static str {
        match policy {
            LayerizationPolicy::PreservePropertyBoundaries => "preserve-property-boundaries",
        }
    }

    fn kind_name(kind: SurfaceDagNodeKind) -> &'static str {
        match kind {
            SurfaceDagNodeKind::Transform(_) => "transform",
            SurfaceDagNodeKind::Effect(_) => "effect",
            SurfaceDagNodeKind::ScrollContent { .. } => "scroll-content",
        }
    }

    assert_eq!(
        policy_name(LayerizationPolicy::PreservePropertyBoundaries),
        "preserve-property-boundaries",
    );
    assert_eq!(
        kind_name(SurfaceDagNodeKind::Transform(TransformNodeId(
            NodeKey::null(),
        ))),
        "transform",
    );
}

#[test]
fn stage_c_same_owner_artifact_derives_three_surface_kinds_in_canonical_order() {
    let (arena, root, properties, generations) = same_owner_transform_effect_scroll_roles_fixture();
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("C2a same-owner artifact");
    let candidates = derive_artifact_surface_candidates(
        &artifact,
        LayerizationPolicy::PreservePropertyBoundaries,
    )
    .expect("C2a same-owner candidates");
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.kind())
            .collect::<Vec<_>>(),
        [
            SurfaceDagNodeKind::Transform(TransformNodeId(root)),
            SurfaceDagNodeKind::Effect(EffectNodeId(root)),
            SurfaceDagNodeKind::ScrollContent {
                scroll: ScrollNodeId(root),
                contents_clip: ClipNodeId {
                    owner: root,
                    role: ClipNodeRole::ContentsClip,
                },
            },
        ],
    );
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.target() == root)
    );
    assert!(
        candidates.iter().all(|candidate| {
            surface_target_name(candidate.scene_root_receiver()) == "scene-root"
        })
    );
    assert!(
        candidates
            .windows(2)
            .all(|pair| pair[0].cursor() == pair[1].cursor()),
        "co-located surface kinds share one generic target cursor",
    );
    assert_eq!(candidates[0].clip_rebase(), None);
    assert_eq!(candidates[1].clip_rebase(), None);
    let rebase = candidates[2]
        .clip_rebase()
        .expect("scroll content carries a clip-space obligation");
    assert_eq!(rebase.outer_clip(), None);
    assert_eq!(
        rebase.local_clip(),
        ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        },
    );
}

#[test]
fn stage_c_nested_scroll_candidates_preserve_outer_and_local_clip_ids() {
    let (arena, roots, properties, generations) = native_scroll_forest_plan_fixture();
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &roots, &properties, &generations)
            .expect("C2a native-forest artifact");
    let candidates = derive_artifact_surface_candidates(
        &artifact,
        LayerizationPolicy::PreservePropertyBoundaries,
    )
    .expect("C2a native-forest candidates");
    let scrolls = candidates
        .iter()
        .filter(|candidate| matches!(candidate.kind(), SurfaceDagNodeKind::ScrollContent { .. }))
        .collect::<Vec<_>>();
    assert_eq!(scrolls.len(), artifact.scroll_nodes.len());
    for candidate in scrolls {
        let SurfaceDagNodeKind::ScrollContent {
            scroll,
            contents_clip,
        } = candidate.kind()
        else {
            unreachable!("filtered to scroll-content candidates")
        };
        assert_eq!(candidate.target(), scroll.0);
        let rebase = candidate
            .clip_rebase()
            .expect("scroll content carries clip rebase");
        assert_eq!(rebase.local_clip(), contents_clip);
        assert_eq!(
            rebase.outer_clip(),
            artifact
                .clip_nodes
                .iter()
                .find(|snapshot| snapshot.id == contents_clip)
                .expect("artifact owns scroll contents clip")
                .parent,
        );
    }
    assert!(
        candidates.iter().all(|candidate| !matches!(
            candidate.kind(),
            SurfaceDagNodeKind::Transform(_) | SurfaceDagNodeKind::Effect(_)
        )),
        "clip, layout-position and visual-offset stores do not mint surfaces",
    );
}

#[test]
fn stage_c_named_anchor_spatial_closure_mints_only_its_authored_transform_surface() {
    let (arena, root, _, child, properties, generations) =
        stage_c_named_anchor_classification_fixture();
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("C2a named-anchor artifact");
    assert!(!artifact.layout_position_nodes.is_empty());
    assert!(!artifact.visual_offset_nodes.is_empty());
    let candidates = derive_artifact_surface_candidates(
        &artifact,
        LayerizationPolicy::PreservePropertyBoundaries,
    )
    .expect("C2a named-anchor candidates");
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.kind())
            .collect::<Vec<_>>(),
        [SurfaceDagNodeKind::Transform(TransformNodeId(child))],
        "anchor layout and visual edges remain coordinate-space data",
    );
}

#[test]
fn stage_c_scroll_surface_rejects_a_missing_contents_clip_with_exact_identity() {
    let (arena, root, properties, generations) = same_owner_transform_effect_scroll_roles_fixture();
    let mut artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("C2a same-owner artifact");
    let expected = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    artifact
        .clip_nodes
        .retain(|snapshot| snapshot.id != expected);
    // Keep this C2a test focused on the scroll snapshot's required contents
    // clip. Batch 1 separately proves that a dangling owner endpoint rejects
    // at artifact construction, so this synthetic endpoint must remain valid
    // to reach the candidate-specific contract below.
    for snapshot in &mut artifact.owner_property_states {
        for state in [&mut snapshot.paint, &mut snapshot.descendants] {
            if state.clip == Some(expected) {
                state.clip = None;
            }
        }
    }

    assert_eq!(
        derive_artifact_surface_candidates(
            &artifact,
            LayerizationPolicy::PreservePropertyBoundaries,
        ),
        Err(SurfaceDagError::MissingScrollContentsClip {
            scroll: ScrollNodeId(root),
            expected,
        }),
    );
}
