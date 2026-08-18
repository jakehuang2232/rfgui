use super::stage_c_surface_raster_plan_tests::{
    depth_four_effect_artifact, raster_context, scroll_surface_artifact,
};
use super::*;
use crate::view::compositor::property_tree::{
    ClipBehavior, ClipNodeId, ClipNodeRole, ClipNodeSnapshot,
};
use crate::view::paint::{
    RetainedSurfaceCompileAction, RetainedSurfaceRasterRole, RetainedSurfaceResidentKey,
    prepare_artifact_surface_raster_plan, seal_artifact_surface_resident_set,
};

fn sealed_depth_four() -> crate::view::paint::SealedArtifactSurfaceResidentSet {
    let plan = prepare_artifact_surface_raster_plan(depth_four_effect_artifact(), raster_context())
        .expect("depth-four raster plan");
    seal_artifact_surface_resident_set(plan).expect("depth-four resident seal")
}

fn scroll_artifact_with_a_local_clip() -> PaintArtifact {
    let mut artifact = scroll_surface_artifact();
    let scroll_owner = artifact
        .scroll_nodes
        .first()
        .expect("scroll fixture snapshot")
        .owner;
    let receiver_clip = ClipNodeId {
        owner: scroll_owner,
        role: ClipNodeRole::ContentsClip,
    };
    let chunk_owner = artifact
        .chunks
        .iter()
        .find(|chunk| chunk.owner != scroll_owner)
        .expect("scroll fixture child chunk")
        .owner;
    let local_clip = ClipNodeId {
        owner: chunk_owner,
        role: ClipNodeRole::SelfClip,
    };
    artifact.clip_nodes.push(ClipNodeSnapshot {
        id: local_clip,
        owner: chunk_owner,
        parent: Some(receiver_clip),
        logical_scissor: [4, 6, 80, 70],
        behavior: ClipBehavior::Replace,
        generation: 29,
    });
    for chunk in &mut artifact.chunks {
        if chunk.owner == chunk_owner {
            chunk.properties.clip = Some(local_clip);
        }
    }
    artifact
}

fn co_located_surface_artifact() -> PaintArtifact {
    let (arena, root, properties, _) = same_owner_transform_effect_scroll_roles_fixture();
    let mut artifact = scroll_surface_artifact();
    assert_eq!(
        artifact.owner_nodes.first().map(|owner| owner.owner),
        Some(root)
    );

    for endpoint in &mut artifact.owner_property_states {
        let state = properties
            .node_state_for(endpoint.owner)
            .expect("co-located owner property state");
        endpoint.stable_id = arena
            .get(endpoint.owner)
            .expect("co-located owner")
            .element
            .stable_id();
        endpoint.paint = state.paint;
        endpoint.descendants = state.descendants;
    }
    for chunk in &mut artifact.chunks {
        chunk.properties = properties
            .node_state_for(chunk.owner)
            .expect("co-located chunk property state")
            .paint;
    }
    artifact.clip_nodes.clear();
    artifact.effect_nodes.clear();
    artifact.transform_nodes.clear();
    artifact.layout_position_nodes.clear();
    artifact.visual_offset_nodes.clear();
    artifact.scroll_nodes.clear();
    super::super::super::frame_recorder::populate_referenced_property_snapshots_for_test(
        &mut artifact,
        &properties,
    )
    .expect("co-located property closure");
    artifact
}

#[test]
fn artifact_surface_resident_set_seals_the_depth_four_program() {
    let sealed = sealed_depth_four();
    assert!(sealed.is_canonical());
    assert!(sealed.stamps().len() >= 4);
    assert!(
        sealed
            .stamps()
            .iter()
            .all(|stamp| stamp.has_artifact_surface_program_for_test())
    );
    assert!(
        sealed
            .stamps()
            .iter()
            .all(|stamp| stamp.ordered_steps.is_empty())
    );
    assert!(sealed.stamps().iter().all(|stamp| {
        !stamp.clip_nodes.is_empty()
            || stamp
                .local_clip_generation_semantics_name_for_test()
                .is_none()
    }));
    assert!(sealed.stamps().iter().all(|stamp| {
        stamp
            .artifact_surface_program_step_names_for_test()
            .is_some_and(|steps| {
                steps
                    .iter()
                    .all(|step| matches!(*step, "artifact-span" | "nested-surface"))
            })
    }));
}

#[test]
fn artifact_surface_resident_set_records_live_local_clip_generation_authority() {
    let plan =
        prepare_artifact_surface_raster_plan(scroll_artifact_with_a_local_clip(), raster_context())
            .expect("local-clip scroll raster plan");
    let sealed = seal_artifact_surface_resident_set(plan).expect("local-clip resident seal");
    let scroll = sealed
        .stamps()
        .iter()
        .find(|stamp| stamp.identity.role == RetainedSurfaceRasterRole::ScrollContent)
        .expect("scroll-content resident");
    assert_eq!(
        scroll.local_clip_generation_semantics_name_for_test(),
        Some("artifact-live")
    );
    assert_eq!(scroll.clip_nodes.len(), 1);
    assert_eq!(scroll.clip_nodes[0].generation, 29);
    assert!(sealed.is_canonical());
}

#[test]
fn co_located_surface_roles_seal_to_three_distinct_generic_resident_keys() {
    let plan =
        prepare_artifact_surface_raster_plan(co_located_surface_artifact(), raster_context())
            .expect("co-located role raster plan");
    assert_eq!(plan.nodes().len(), 3);

    let sealed = seal_artifact_surface_resident_set(plan).expect("co-located role resident seal");
    let keys = sealed.resident_keys_for_test();
    assert_eq!(keys.len(), 3);
    assert_eq!(keys.iter().copied().collect::<FxHashSet<_>>().len(), 3);
    assert!(
        keys.iter()
            .all(|key| matches!(key, RetainedSurfaceResidentKey::Surface { .. }))
    );
    assert_eq!(
        keys.iter()
            .filter_map(|key| match key {
                RetainedSurfaceResidentKey::Surface { role, .. } => Some(*role),
                _ => None,
            })
            .collect::<FxHashSet<_>>(),
        FxHashSet::from_iter([
            RetainedSurfaceRasterRole::Transform,
            RetainedSurfaceRasterRole::PropertyEffect,
            RetainedSurfaceRasterRole::ScrollContent,
        ]),
    );
    assert!(sealed.is_canonical());
}

#[test]
fn artifact_program_rejects_a_simultaneous_legacy_step_authority() {
    let mut sealed = sealed_depth_four();
    assert!(sealed.inject_legacy_step_into_first_artifact_program_for_test());
    assert!(!sealed.is_canonical());
}

#[test]
fn artifact_surface_resident_set_rejects_a_missing_rebased_boundary_owner() {
    let mut sealed = sealed_depth_four();
    assert!(sealed.remove_first_span_boundary_owner_for_test());
    assert!(!sealed.is_canonical());
}

#[test]
fn artifact_surface_resident_set_rejects_a_missing_chunk_revision() {
    let mut sealed = sealed_depth_four();
    assert!(sealed.zero_first_span_topology_revision_for_test());
    assert!(!sealed.is_canonical());
}

#[test]
fn artifact_surface_resident_set_rejects_a_nested_receiver_mismatch() {
    let mut sealed = sealed_depth_four();
    assert!(sealed.redirect_first_nested_surface_to_parent_for_test());
    assert!(!sealed.is_canonical());
}

#[test]
fn artifact_program_change_keeps_the_resident_key_but_forces_reraster() {
    let baseline = sealed_depth_four();
    let mut changed_artifact = depth_four_effect_artifact();
    for chunk in &mut changed_artifact.chunks {
        chunk.content_revision.topology_revision = chunk
            .content_revision
            .topology_revision
            .checked_add(1)
            .expect("fixture revision increment");
    }
    let changed = seal_artifact_surface_resident_set(
        prepare_artifact_surface_raster_plan(changed_artifact, raster_context())
            .expect("changed raster plan"),
    )
    .expect("changed resident seal");
    let (resident, candidate) = baseline
        .stamps()
        .iter()
        .zip(changed.stamps())
        .find(|(resident, candidate)| resident != candidate)
        .expect("revision change must alter one resident stamp");

    // This guards the type placement. The complete artifact program belongs
    // on the stamp, not the identity: a source-authority or program change is
    // the same resident allocation with changed raster inputs.
    assert_eq!(
        baseline.resident_keys_for_test(),
        changed.resident_keys_for_test()
    );
    assert_eq!(
        crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
            resident.clone(),
            candidate,
        ),
        RetainedSurfaceCompileAction::Reraster,
    );
    assert_eq!(
        crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
            resident.clone(),
            resident,
        ),
        RetainedSurfaceCompileAction::Reuse,
    );
}
