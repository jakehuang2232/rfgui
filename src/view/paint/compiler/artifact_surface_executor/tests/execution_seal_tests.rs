use super::*;

#[test]
fn execution_seal_keeps_the_frame_and_one_child_mask_program_together() {
    let execution = seal_prepared_artifact_surface_execution(prepared_child_mask_surface_frame());
    assert!(execution.frame.is_canonical());
    assert_eq!(
        execution.root_programs.len(),
        execution.frame.raster_plan().roots().len(),
    );
    assert_eq!(
        execution.node_programs.len(),
        execution.frame.raster_plan().nodes().len(),
    );
    for (program, root) in execution
        .root_programs
        .iter()
        .zip(execution.frame.raster_plan().roots())
    {
        assert_eq!(
            program.target,
            ArtifactSurfaceRasterTargetId::SceneRoot(root.scene_root()),
        );
    }
    for (program, node) in execution
        .node_programs
        .iter()
        .zip(execution.frame.raster_plan().nodes())
    {
        assert_eq!(
            program.target,
            ArtifactSurfaceRasterTargetId::Surface(node.source()),
        );
    }

    let programs = execution
        .root_programs
        .iter()
        .chain(&execution.node_programs)
        .collect::<Vec<_>>();
    let actions = programs
        .iter()
        .flat_map(|program| &program.steps)
        .flat_map(|step| match step {
            ArtifactSurfaceChildMaskStep::ArtifactSpan { chunk_actions, .. } => {
                chunk_actions.as_slice()
            }
            ArtifactSurfaceChildMaskStep::NestedSurface(_) => &[],
        })
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action, ArtifactSurfaceChildMaskAction::Push(_)))
            .count(),
        1,
    );
    assert_eq!(
        actions
            .iter()
            .filter(|action| **action == ArtifactSurfaceChildMaskAction::Pop)
            .count(),
        1,
    );
    assert_eq!(
        programs.iter().map(|program| program.max_mask_depth).max(),
        Some(1),
    );
    let root_push_scissors = execution
        .root_programs
        .iter()
        .flat_map(|program| &program.steps)
        .flat_map(|step| match step {
            ArtifactSurfaceChildMaskStep::ArtifactSpan { chunk_actions, .. } => {
                chunk_actions.as_slice()
            }
            ArtifactSurfaceChildMaskStep::NestedSurface(_) => &[],
        })
        .filter_map(|action| match action {
            ArtifactSurfaceChildMaskAction::Push(scissor) => Some(*scissor),
            ArtifactSurfaceChildMaskAction::Unchanged | ArtifactSurfaceChildMaskAction::Pop => None,
        })
        .collect::<Vec<_>>();
    assert!(
        root_push_scissors
            .iter()
            .all(|scissor| matches!(scissor, GraphicsPassScissor::Logical(_))),
        "scene-root child masks remain in logical space",
    );
    let mask_chunk = execution
        .frame
        .raster_plan()
        .roots()
        .iter()
        .flat_map(|root| root.steps())
        .flat_map(step_chunks)
        .find(|chunk| {
            chunk.source().id.slot == RETAINED_CHILD_MASK_SLOT
                && chunk.source().id.phase == PaintNodePhase::BeforeChildren
        })
        .expect("child-mask push chunk");
    let projection = ArtifactSurfaceRasterOriginProjection::new(
        mask_chunk.localized_bounds_bits(),
        1.0_f32.to_bits(),
    )
    .expect("child-mask raster-origin projection");
    let mut projected_mask_chunk = mask_chunk.clone();
    projected_mask_chunk.localized_bounds_bits = projection
        .project_bounds_bits(mask_chunk.localized_bounds_bits())
        .expect("projected child-mask bounds");
    assert_eq!(
        ArtifactSurfaceChildMaskAction::from_chunk(&projected_mask_chunk, Some(projection)),
        ArtifactSurfaceChildMaskAction::Push(GraphicsPassScissor::TargetPhysical([0, 0, 120, 90,])),
        "a detached target seals the same child mask in target-physical space",
    );
    let contract_projection = ArtifactSurfaceRasterOriginProjection::new(
        [26.0_f32, 19.0, 26.0, 20.0].map(f32::to_bits),
        1.0_f32.to_bits(),
    )
    .expect("child-mask contract projection");
    assert_eq!(
        contract_projection.target_physical_scissor_for_projected_bounds(
            [0.0_f32, 0.0, 26.0, 20.0].map(f32::to_bits),
        ),
        Some(GraphicsPassScissor::TargetPhysical([0, 0, 26, 20])),
    );

    let parent = execution
        .root_programs
        .iter()
        .find(|program| {
            program
                .steps
                .iter()
                .any(|step| matches!(step, ArtifactSurfaceChildMaskStep::NestedSurface(_)))
        })
        .expect("scene root must nest the child effect surface");
    for (expected_index, step) in parent.steps.iter().enumerate() {
        if let ArtifactSurfaceChildMaskStep::ArtifactSpan { step_index, .. } = step {
            assert_eq!(*step_index, expected_index);
        }
    }
    let nested_index = parent
        .steps
        .iter()
        .position(|step| matches!(step, ArtifactSurfaceChildMaskStep::NestedSurface(_)))
        .expect("parent nested-surface step");
    let push_index = parent
        .steps
        .iter()
        .position(|step| match step {
            ArtifactSurfaceChildMaskStep::ArtifactSpan { chunk_actions, .. } => chunk_actions
                .iter()
                .any(|action| matches!(action, ArtifactSurfaceChildMaskAction::Push(_))),
            ArtifactSurfaceChildMaskStep::NestedSurface(_) => false,
        })
        .expect("parent mask push step");
    let pop_index = parent
        .steps
        .iter()
        .position(|step| match step {
            ArtifactSurfaceChildMaskStep::ArtifactSpan { chunk_actions, .. } => {
                chunk_actions.contains(&ArtifactSurfaceChildMaskAction::Pop)
            }
            ArtifactSurfaceChildMaskStep::NestedSurface(_) => false,
        })
        .expect("parent mask pop step");
    assert!(push_index < nested_index && nested_index < pop_index);
}
