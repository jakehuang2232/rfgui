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
            .filter(|action| **action == ArtifactSurfaceChildMaskAction::Push)
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
            ArtifactSurfaceChildMaskStep::ArtifactSpan { chunk_actions, .. } => {
                chunk_actions.contains(&ArtifactSurfaceChildMaskAction::Push)
            }
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
