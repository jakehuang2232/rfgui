use super::*;

use crate::view::base_component::{
    Rect, ScrollAxisSnapshot, ScrollContentsClipWitness, ScrollbarInteractionWitness,
    ScrollbarOverlayWitness, ScrollbarPaintStateWitness, Size,
};
use crate::view::compositor::property_tree::{
    ClipNodeId, ClipNodeSnapshot, PropertyTreeState, ScrollNodeId, ScrollNodeSnapshot,
};
use glam::Vec2;

fn legacy_local_clip_stamp() -> RetainedSurfaceRasterStamp {
    crate::view::paint::tests::atomic_projection_content_stamp_for_test("projected", 0xc3_b001)
        .expect("legacy local-clip fixture must produce one canonical stamp")
}

fn artifact_projection(
    stamp: &RetainedSurfaceRasterStamp,
    generation: u64,
    live_at_boundary: bool,
) -> Result<SurfaceDagClipProjection, SurfaceDagError> {
    let [legacy_local] = stamp.clip_nodes.as_slice() else {
        panic!("legacy fixture must carry one detached local clip")
    };
    let boundary_id = ClipNodeId {
        owner: stamp.identity.boundary_root,
        role: ClipNodeRole::ContentsClip,
    };
    assert_ne!(legacy_local.id, boundary_id);
    let boundary = ClipNodeSnapshot {
        id: boundary_id,
        owner: boundary_id.owner,
        parent: None,
        logical_scissor: [0, 0, 120, 90],
        behavior: ClipBehavior::Intersect,
        generation: 17,
    };
    let live_local = ClipNodeSnapshot {
        parent: Some(boundary_id),
        generation,
        ..*legacy_local
    };
    let viewport = Rect {
        x: 0.0,
        y: 0.0,
        width: 120.0,
        height: 90.0,
    };
    let scroll = ScrollNodeSnapshot {
        id: ScrollNodeId(boundary_id.owner),
        owner: boundary_id.owner,
        parent: None,
        offset: Vec2::ZERO,
        configured_axis: ScrollAxisSnapshot::Vertical,
        viewport,
        content_size: Size {
            width: 120.0,
            height: 180.0,
        },
        layout_content_bounds_at_zero: Rect {
            height: 180.0,
            ..viewport
        },
        scrollbar_overlay: ScrollbarOverlayWitness {
            vertical_track: None,
            vertical_thumb: None,
            horizontal_track: None,
            horizontal_thumb: None,
            interaction: ScrollbarInteractionWitness {
                hovered: false,
                dragging_axis: None,
                has_interaction_timestamp: false,
            },
            paint_state: ScrollbarPaintStateWitness::HiddenNow,
            sampled_alpha: 0.0,
            shadow_blur_radius: 0.0,
        },
        contents_clip: ScrollContentsClipWitness::ExactRect(boundary.logical_scissor),
        generation: 19,
    };
    let artifact = PaintArtifact {
        clip_nodes: vec![live_local, boundary],
        scroll_nodes: vec![scroll],
        ..PaintArtifact::default()
    };
    let rebase =
        crate::view::paint::SurfaceDagClipRebase::try_from_artifact(&artifact, boundary_id)?;
    rebase.project_clip_space(
        &artifact,
        PropertyTreeState {
            clip: Some(if live_at_boundary {
                boundary_id
            } else {
                live_local.id
            }),
            scroll: Some(scroll.id),
            ..PropertyTreeState::default()
        },
    )
}

fn parts_for_projection(
    stamp: RetainedSurfaceRasterStamp,
    projection: &SurfaceDagClipProjection,
) -> RetainedSurfaceRasterStampParts {
    let mut ordered_steps = stamp.ordered_steps;
    for step in &mut ordered_steps {
        if let RetainedSurfaceRasterStepStamp::ArtifactSpan(span) = step {
            span.clip_nodes = projection.local_clips().to_vec();
        }
    }
    RetainedSurfaceRasterStampParts {
        identity: stamp.identity,
        target: stamp.target,
        owner_topology: stamp.owner_topology,
        clip_nodes: Vec::new(),
        chunks: stamp.chunks,
        op_count: stamp.op_count,
        opaque_order_span: stamp.opaque_order_span,
        ordered_steps,
        scroll_host: stamp.scroll_host,
        property_effect: stamp.property_effect,
        native_scroll_children: stamp.native_scroll_children,
    }
}

fn artifact_live_stamp(
    legacy: RetainedSurfaceRasterStamp,
    generation: u64,
) -> RetainedSurfaceRasterStamp {
    let projection = artifact_projection(&legacy, generation, false)
        .expect("nonzero artifact-only clip projection");
    RetainedSurfaceRasterStamp::from_artifact_live_clip_projection(
        parts_for_projection(legacy, &projection),
        projection,
    )
    .expect("typed projection must mint an ArtifactLive stamp")
}

fn semantics_name(semantics: LocalClipGenerationSemantics) -> &'static str {
    match semantics {
        LocalClipGenerationSemantics::LegacyDetached => "legacy-detached",
        LocalClipGenerationSemantics::ArtifactLive => "artifact-live",
    }
}

#[test]
fn local_clip_generation_semantics_are_an_exhaustive_closed_set() {
    assert_eq!(
        [
            semantics_name(LocalClipGenerationSemantics::LegacyDetached),
            semantics_name(LocalClipGenerationSemantics::ArtifactLive),
        ],
        ["legacy-detached", "artifact-live"],
    );
}

#[test]
fn legacy_semantics_keeps_the_detached_generation_gate_strict() {
    let legacy = legacy_local_clip_stamp();
    assert_eq!(
        legacy.local_clip_generation_semantics,
        Some(LocalClipGenerationSemantics::LegacyDetached),
    );
    assert!(retained_surface_raster_stamp_is_canonical(&legacy));

    let mut live_generation = legacy;
    live_generation.clip_nodes[0].generation = 29;
    let [RetainedSurfaceRasterStepStamp::ArtifactSpan(span)] =
        live_generation.ordered_steps.as_mut_slice()
    else {
        panic!("local clip fixture owns one artifact span")
    };
    span.clip_nodes[0].generation = 29;
    assert!(!retained_surface_raster_stamp_is_canonical(
        &live_generation
    ));
}

#[test]
fn artifact_live_semantics_accepts_only_the_typed_nonzero_projection() {
    let legacy = legacy_local_clip_stamp();
    let projection =
        artifact_projection(&legacy, 29, false).expect("nonzero artifact-only clip projection");
    let artifact_live = RetainedSurfaceRasterStamp::from_artifact_live_clip_projection(
        parts_for_projection(legacy, &projection),
        projection,
    )
    .expect("nonzero artifact projection must mint the live semantics");
    assert_eq!(
        artifact_live.local_clip_generation_semantics,
        Some(LocalClipGenerationSemantics::ArtifactLive),
    );
    assert_eq!(artifact_live.clip_nodes[0].generation, 29);
    assert!(retained_surface_raster_stamp_is_canonical(&artifact_live));

    let stale_legacy = legacy_local_clip_stamp();
    let stale_projection = artifact_projection(&stale_legacy, 31, false)
        .expect("second nonzero artifact-only clip projection");
    let mut stale_parts = parts_for_projection(stale_legacy, &stale_projection);
    stale_parts.clip_nodes = stale_projection.local_clips().to_vec();
    assert!(
        RetainedSurfaceRasterStamp::from_artifact_live_clip_projection(
            stale_parts,
            stale_projection,
        )
        .is_none(),
        "a caller-supplied second clip copy must fail closed",
    );

    let zero_legacy = legacy_local_clip_stamp();
    assert!(
        artifact_projection(&zero_legacy, 0, false).is_err(),
        "zero generation must fail before an ArtifactLive stamp can be minted",
    );
}

#[test]
fn empty_artifact_projection_carries_no_local_clip_semantics() {
    let legacy = legacy_local_clip_stamp();
    let empty = artifact_projection(&legacy, 29, true)
        .expect("boundary-local artifact-only clip projection");
    assert!(empty.local_clips().is_empty());
    let stamp = RetainedSurfaceRasterStamp::from_artifact_live_clip_projection(
        parts_for_projection(legacy, &empty),
        empty,
    )
    .expect("boundary-local projection has no detached local clip");
    assert_eq!(stamp.local_clip_generation_semantics, None);
}

#[test]
fn semantics_change_keeps_the_resident_key_but_forces_reraster() {
    let artifact_live = artifact_live_stamp(legacy_local_clip_stamp(), 29);
    let legacy_semantics = artifact_live
        .clone()
        .with_local_clip_generation_semantics_for_test(Some(
            LocalClipGenerationSemantics::LegacyDetached,
        ));

    // This guards the type placement: moving semantics into raster identity
    // would change the persistent key and turn a raster refresh into a spurious
    // resident reallocation.
    assert_eq!(
        legacy_semantics.identity.resident_key(),
        artifact_live.identity.resident_key(),
    );
    assert_eq!(
        crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
            legacy_semantics,
            &artifact_live,
        ),
        RetainedSurfaceCompileAction::Reraster,
    );
}

#[test]
fn artifact_live_generation_is_a_raster_input_not_a_resident_identity() {
    let legacy = legacy_local_clip_stamp();
    let resident = artifact_live_stamp(legacy.clone(), 29);
    let unchanged = artifact_live_stamp(legacy.clone(), 29);
    let changed = artifact_live_stamp(legacy, 31);

    assert_eq!(
        resident.identity.resident_key(),
        changed.identity.resident_key()
    );
    assert_eq!(
        crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
            resident.clone(),
            &unchanged,
        ),
        RetainedSurfaceCompileAction::Reuse,
    );
    assert_eq!(
        crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
            resident, &changed,
        ),
        RetainedSurfaceCompileAction::Reraster,
    );
}
