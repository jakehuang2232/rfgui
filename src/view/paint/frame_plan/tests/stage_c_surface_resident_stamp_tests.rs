use super::stage_c_surface_raster_plan_tests::{
    depth_four_effect_artifact, depth_three_effect_artifact, raster_context,
    scroll_surface_artifact,
};
use super::*;
use crate::view::compositor::property_tree::{
    ClipBehavior, ClipNodeId, ClipNodeRole, ClipNodeSnapshot,
};
use crate::view::paint::{
    ArtifactSurfaceCompositeGeometryStamp, ResolvedClip, RetainedSurfaceCompileAction,
    RetainedSurfaceRasterRole, RetainedSurfaceResidentKey, SurfaceDagExecutionTargetId,
    prepare_artifact_surface_raster_plan, seal_prepared_artifact_surface_frame,
};

fn prepared_depth_four() -> crate::view::paint::PreparedArtifactSurfaceFrame {
    let plan = prepare_artifact_surface_raster_plan(depth_four_effect_artifact(), raster_context())
        .expect("depth-four raster plan");
    seal_prepared_artifact_surface_frame(plan).expect("depth-four resident seal")
}

fn sealed_depth_four() -> crate::view::paint::SealedArtifactSurfaceResidentSet {
    prepared_depth_four().residents().clone()
}

fn prepared_co_located() -> crate::view::paint::PreparedArtifactSurfaceFrame {
    let plan =
        prepare_artifact_surface_raster_plan(co_located_surface_artifact(), raster_context())
            .expect("co-located role raster plan");
    seal_prepared_artifact_surface_frame(plan).expect("co-located role resident seal")
}

fn prepared_depth_three(
    artifact: PaintArtifact,
) -> crate::view::paint::PreparedArtifactSurfaceFrame {
    let plan = prepare_artifact_surface_raster_plan(artifact, raster_context())
        .expect("depth-three raster plan");
    seal_prepared_artifact_surface_frame(plan).expect("depth-three resident seal")
}

fn sealed_stamps(
    sealed: &crate::view::paint::SealedArtifactSurfaceResidentSet,
) -> impl Iterator<Item = &crate::view::paint::RetainedSurfaceRasterStamp> {
    sealed
        .ordered_entries()
        .iter()
        .map(crate::view::paint::SealedArtifactSurfaceResidentEntry::stamp)
}

pub(super) fn scroll_artifact_with_a_local_clip() -> PaintArtifact {
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
    let prepared = prepared_depth_four();
    let sealed = prepared.residents();
    assert!(prepared.is_canonical());
    assert_eq!(prepared.raster_plan().nodes().len(), sealed.len());
    assert!(sealed.is_canonical());
    assert!(sealed.len() >= 4);
    assert!(sealed_stamps(sealed).all(|stamp| stamp.has_artifact_surface_program_for_test()));
    assert!(sealed_stamps(sealed).all(|stamp| stamp.ordered_steps.is_empty()));
    assert!(sealed_stamps(sealed).all(|stamp| {
        !stamp.clip_nodes.is_empty()
            || stamp
                .local_clip_generation_semantics_name_for_test()
                .is_none()
    }));
    assert!(sealed_stamps(sealed).all(|stamp| {
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
    let prepared = seal_prepared_artifact_surface_frame(plan).expect("local-clip resident seal");
    let sealed = prepared.residents();
    let scroll = sealed_stamps(sealed)
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
    let prepared = prepared_co_located();
    assert_eq!(prepared.raster_plan().nodes().len(), 3);
    let sealed = prepared.residents();
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
    let changed = seal_prepared_artifact_surface_frame(
        prepare_artifact_surface_raster_plan(changed_artifact, raster_context())
            .expect("changed raster plan"),
    )
    .expect("changed resident seal");
    let changed = changed.residents();
    let (resident, candidate) = sealed_stamps(&baseline)
        .zip(sealed_stamps(changed))
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

/// Proves that composite geometry does not pollute V2 raster equality when
/// localized raster inputs are identical. Natural artifact-path scroll-offset
/// invariance still requires a real detached-scroll fixture.
#[test]
fn root_scroll_offset_changes_plan_geometry_but_reuses_every_raster_stamp() {
    let baseline_artifact = scroll_surface_artifact();
    let baseline = seal_prepared_artifact_surface_frame(
        prepare_artifact_surface_raster_plan(baseline_artifact, raster_context())
            .expect("baseline V2 scroll raster plan"),
    )
    .expect("baseline V2 scroll resident seal");
    let baseline_geometry = baseline
        .raster_plan()
        .nodes()
        .iter()
        .map(|node| node.geometry())
        .collect::<Vec<_>>();

    let mut moved_artifact = scroll_surface_artifact();
    let scroll_owner = moved_artifact
        .scroll_nodes
        .first()
        .expect("moved V2 scroll snapshot")
        .owner;
    let moved_chunk_indices = moved_artifact
        .chunks
        .iter()
        .enumerate()
        .filter_map(|(index, chunk)| (chunk.owner != scroll_owner).then_some(index))
        .collect::<Vec<_>>();
    for chunk_index in moved_chunk_indices {
        let op_range = moved_artifact.chunks[chunk_index].op_range.clone();
        for op in &mut moved_artifact.ops[op_range.clone()] {
            let PaintOp::DrawRect(rect) = op else {
                panic!("V2 scroll placement fixture owns only one draw-rect payload")
            };
            rect.params.position[1] -= 7.0;
        }
        moved_artifact.chunks[chunk_index].bounds.y -= 7.0;
        moved_artifact.chunks[chunk_index].payload_identity = moved_artifact.chunks[chunk_index]
            .payload_identity
            .rebuild_from_localized_ops(&moved_artifact.ops[op_range])
            .expect("moved authored payload identity");
    }
    let moved_scroll = moved_artifact
        .scroll_nodes
        .first_mut()
        .expect("moved V2 scroll snapshot");
    moved_scroll.offset.y += 7.0;
    let moved = seal_prepared_artifact_surface_frame(
        prepare_artifact_surface_raster_plan(moved_artifact, raster_context())
            .expect("moved V2 scroll raster plan"),
    )
    .expect("moved V2 scroll resident seal");
    let moved_geometry = moved
        .raster_plan()
        .nodes()
        .iter()
        .map(|node| node.geometry())
        .collect::<Vec<_>>();

    assert_ne!(baseline_geometry, moved_geometry);
    assert_eq!(baseline.residents(), moved.residents());

    let mut viewport = crate::view::viewport::Viewport::new();
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("baseline V2 resident owner");
    assert!(viewport.stage_artifact_surface_resident_set(owner, baseline.residents().clone()));
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    let actions = viewport
        .artifact_surface_compile_actions_for_forced_test(moved.residents())
        .expect("moved V2 pool actions");
    assert!(
        actions
            .iter()
            .all(|(_, action)| *action == RetainedSurfaceCompileAction::Reuse)
    );
}

#[test]
fn nested_geometry_invalidates_the_receiver_but_not_the_child_raster() {
    let baseline = prepared_depth_three(depth_three_effect_artifact());
    let (parent, child, generation) = baseline
        .raster_plan()
        .nodes()
        .iter()
        .find_map(|node| {
            let SurfaceDagExecutionTargetId::Surface(parent) = node.receiver() else {
                return None;
            };
            if !baseline
                .raster_plan()
                .nodes()
                .get(parent.index())
                .is_some_and(|parent| {
                    matches!(parent.receiver(), SurfaceDagExecutionTargetId::SceneRoot(_))
                })
            {
                return None;
            }
            let ArtifactSurfaceCompositeGeometryStamp::Effect { generation, .. } = node.geometry()
            else {
                return None;
            };
            Some((parent, node.execution_id(), generation))
        })
        .expect("depth-three fixture owns a nested effect below a scene-root surface");

    let mut changed_artifact = depth_three_effect_artifact();
    let changed_effect = changed_artifact
        .effect_nodes
        .iter_mut()
        .find(|effect| effect.generation == generation)
        .expect("nested effect snapshot");
    changed_effect.generation = changed_effect
        .generation
        .checked_add(100)
        .expect("nested effect generation increment");
    let changed = prepared_depth_three(changed_artifact);

    assert_ne!(
        baseline.raster_plan().nodes()[child.index()].geometry(),
        changed.raster_plan().nodes()[child.index()].geometry(),
    );
    assert_eq!(
        baseline.residents().ordered_entries()[child.index()].stamp(),
        changed.residents().ordered_entries()[child.index()].stamp(),
        "a surface's own placement is not its raster identity",
    );
    assert_ne!(
        baseline.residents().ordered_entries()[parent.index()].stamp(),
        changed.residents().ordered_entries()[parent.index()].stamp(),
        "nested placement is the receiver's raster dependency",
    );
    assert_eq!(
        sealed_stamps(baseline.residents())
            .zip(sealed_stamps(changed.residents()))
            .filter(|(left, right)| left != right)
            .count(),
        1,
    );
}

#[test]
fn empty_nested_receiver_keeps_cold_raster_authority_without_advancing_parent_order() {
    let visible = prepared_co_located();
    let mut empty_plan =
        prepare_artifact_surface_raster_plan(co_located_surface_artifact(), raster_context())
            .expect("co-located empty-receiver raster plan");
    let (parent, child) = empty_plan
        .force_first_nested_receiver_clip_empty_for_test()
        .expect("co-located fixture owns a non-effect nested receiver");
    let empty =
        seal_prepared_artifact_surface_frame(empty_plan).expect("empty receiver resident seal");
    assert!(empty.is_canonical());

    let visible_dependency = visible
        .residents()
        .nested_dependencies_for_test()
        .into_iter()
        .find(|(actual_parent, actual_child, ..)| {
            *actual_parent == parent && *actual_child == child
        })
        .expect("visible nested dependency");
    assert_ne!(visible_dependency.2, ResolvedClip::Empty);
    assert!(visible_dependency.4 > visible_dependency.3);

    let empty_dependency = empty
        .residents()
        .nested_dependencies_for_test()
        .into_iter()
        .find(|(actual_parent, actual_child, ..)| {
            *actual_parent == parent && *actual_child == child
        })
        .expect("empty nested dependency");
    assert_eq!(empty_dependency.2, ResolvedClip::Empty);
    assert_eq!(empty_dependency.4, empty_dependency.3);

    let viewport = crate::view::viewport::Viewport::new();
    let actions = viewport
        .artifact_surface_compile_actions_from_pool(empty.residents())
        .expect("canonical cold empty-receiver actions");
    assert_eq!(
        actions[child.index()].1,
        RetainedSurfaceCompileAction::Reraster,
        "an invisible cold child still needs valid raster content for a later visible frame",
    );
}

#[test]
fn artifact_pool_preserves_three_co_located_sealed_keys_cold_and_warm() {
    let prepared = prepared_co_located();
    let residents = prepared.residents();
    let sealed_keys = residents.resident_keys_for_test();
    assert_eq!(sealed_keys.len(), 3);

    let mut viewport = crate::view::viewport::Viewport::new();
    let production_cold = viewport
        .artifact_surface_compile_actions_from_pool(residents)
        .expect("canonical production cold artifact actions");
    assert_eq!(
        production_cold
            .iter()
            .map(|(key, _)| *key)
            .collect::<Vec<_>>(),
        sealed_keys,
    );
    assert!(
        production_cold
            .iter()
            .all(|(_, action)| *action == RetainedSurfaceCompileAction::Reraster)
    );
    let cold = viewport
        .artifact_surface_compile_actions_for_forced_test(residents)
        .expect("canonical cold artifact actions");
    assert_eq!(
        cold.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
        sealed_keys,
    );
    assert!(
        cold.iter()
            .all(|(_, action)| *action == RetainedSurfaceCompileAction::Reraster)
    );

    let owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("artifact frame owner");
    assert!(viewport.stage_artifact_surface_resident_set(owner, residents.clone()));
    assert_eq!(
        viewport.pending_artifact_surface_resident_keys_for_test(),
        Some(sealed_keys.clone()),
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    assert_eq!(
        viewport.committed_retained_surface_resident_keys_for_test(),
        sealed_keys.iter().copied().collect(),
    );

    let warm = viewport
        .artifact_surface_compile_actions_for_forced_test(residents)
        .expect("canonical warm artifact actions");
    assert_eq!(
        warm.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
        sealed_keys,
    );
    assert!(
        warm.iter()
            .all(|(_, action)| *action == RetainedSurfaceCompileAction::Reuse)
    );
}

#[test]
fn one_depth_three_revision_change_rerasterizes_only_its_surface() {
    let baseline = prepared_depth_three(depth_three_effect_artifact());
    assert_eq!(baseline.residents().len(), 3);
    let baseline_keys = baseline.residents().resident_keys_for_test();
    let mut viewport = crate::view::viewport::Viewport::new();
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("baseline frame owner");
    assert!(viewport.stage_artifact_surface_resident_set(owner, baseline.residents().clone(),));
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));

    let mut changed_artifact = depth_three_effect_artifact();
    changed_artifact.chunks[1]
        .content_revision
        .topology_revision = changed_artifact.chunks[1]
        .content_revision
        .topology_revision
        .checked_add(1)
        .expect("fixture revision increment");
    let changed = prepared_depth_three(changed_artifact);
    assert_eq!(changed.residents().resident_keys_for_test(), baseline_keys);
    let actions = viewport
        .artifact_surface_compile_actions_for_forced_test(changed.residents())
        .expect("changed artifact actions");
    assert_eq!(
        actions
            .iter()
            .filter(|(_, action)| *action == RetainedSurfaceCompileAction::Reraster)
            .count(),
        1,
    );
    assert_eq!(
        actions
            .iter()
            .filter(|(_, action)| *action == RetainedSurfaceCompileAction::Reuse)
            .count(),
        2,
    );
}

#[test]
fn empty_clip_cannot_retain_a_nonzero_opaque_cursor_advance() {
    let plan =
        prepare_artifact_surface_raster_plan(scroll_artifact_with_a_local_clip(), raster_context())
            .expect("nonempty scroll clip plan");
    let mut sealed = seal_prepared_artifact_surface_frame(plan)
        .expect("nonempty scroll clip seal")
        .residents()
        .clone();
    assert!(sealed.is_canonical());
    assert!(sealed.force_first_resolved_clip_empty_without_cursor_for_test());
    assert!(!sealed.is_canonical());
}

#[test]
fn artifact_pool_rejections_preserve_committed_and_pending_exact_keys() {
    let prepared = prepared_co_located();
    let residents = prepared.residents().clone();
    let sealed_keys = residents.resident_keys_for_test();
    let mut viewport = crate::view::viewport::Viewport::new();
    let committed_owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("committed frame owner");
    assert!(viewport.stage_artifact_surface_resident_set(committed_owner, residents.clone(),));
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(committed_owner), true,));

    assert!(!viewport.stage_artifact_surface_resident_set(committed_owner, residents.clone(),));
    assert_eq!(
        viewport.committed_retained_surface_resident_keys_for_test(),
        sealed_keys.iter().copied().collect(),
    );

    let pending_owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("pending frame owner");
    let mut invalid = residents.clone();
    assert!(invalid.remove_first_span_boundary_owner_for_test());
    assert!(!viewport.stage_artifact_surface_resident_set(pending_owner, invalid.clone(),));
    assert_eq!(
        viewport.pending_artifact_surface_resident_keys_for_test(),
        None
    );
    assert_eq!(
        viewport.committed_retained_surface_resident_keys_for_test(),
        sealed_keys.iter().copied().collect(),
    );

    assert!(viewport.stage_artifact_surface_resident_set(pending_owner, residents,));
    assert!(!viewport.stage_artifact_surface_resident_set(pending_owner, invalid));
    assert_eq!(
        viewport.pending_artifact_surface_resident_keys_for_test(),
        Some(sealed_keys),
    );
}
