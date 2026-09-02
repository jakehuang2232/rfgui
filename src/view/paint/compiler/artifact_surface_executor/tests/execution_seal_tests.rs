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

#[test]
fn scroll_boundary_masks_wrap_nested_composites_in_the_receiver_program() {
    fn significant_steps(program: &ArtifactSurfaceChildMaskTargetProgram) -> Vec<&'static str> {
        let mut observed = Vec::new();
        for step in &program.steps {
            match step {
                ArtifactSurfaceChildMaskStep::ArtifactSpan { chunk_actions, .. } => {
                    for action in chunk_actions {
                        match action {
                            ArtifactSurfaceChildMaskAction::Push(_) => observed.push("push"),
                            ArtifactSurfaceChildMaskAction::Pop => observed.push("pop"),
                            ArtifactSurfaceChildMaskAction::Unchanged => {}
                        }
                    }
                }
                ArtifactSurfaceChildMaskStep::NestedSurface(_) => observed.push("nested"),
            }
        }
        observed
    }

    fn mask_owners(
        program: &ArtifactSurfaceChildMaskTargetProgram,
        steps: &[PreparedArtifactSurfaceRasterStep],
    ) -> Vec<crate::view::node_arena::NodeKey> {
        let mut owners = Vec::new();
        for step in &program.steps {
            let ArtifactSurfaceChildMaskStep::ArtifactSpan {
                step_index,
                chunk_actions,
            } = step
            else {
                continue;
            };
            let PreparedArtifactSurfaceRasterStep::ArtifactSpan(span) = &steps[*step_index] else {
                unreachable!("sealed artifact-span action points at an artifact span")
            };
            for (chunk, action) in span.chunks().iter().zip(chunk_actions) {
                if !matches!(action, ArtifactSurfaceChildMaskAction::Unchanged) {
                    owners.push(chunk.source().owner);
                }
            }
        }
        owners
    }

    let execution =
        seal_prepared_artifact_surface_execution(prepared_nested_scroll_surface_frame());
    assert_eq!(execution.root_programs.len(), 1);
    assert_eq!(execution.node_programs.len(), 2);

    let root_program = &execution.root_programs[0];
    let root_steps = &execution.frame.raster_plan().roots()[0].steps;
    assert_eq!(significant_steps(root_program), ["push", "nested", "pop"]);
    assert_eq!(root_program.max_mask_depth, 1);

    let outer_index = execution
        .frame
        .raster_plan()
        .nodes()
        .iter()
        .position(|node| matches!(node.receiver(), SurfaceDagExecutionTargetId::SceneRoot(_)))
        .expect("outer ScrollContent surface");
    let inner_index = execution
        .frame
        .raster_plan()
        .nodes()
        .iter()
        .position(|node| matches!(node.receiver(), SurfaceDagExecutionTargetId::Surface(_)))
        .expect("inner ScrollContent surface");
    let outer_node = &execution.frame.raster_plan().nodes()[outer_index];
    let inner_node = &execution.frame.raster_plan().nodes()[inner_index];
    let outer_program = &execution.node_programs[outer_index];
    let inner_program = &execution.node_programs[inner_index];

    assert_eq!(
        significant_steps(outer_program),
        ["push", "nested", "pop"],
        "the outer resident excludes its own boundary mask but retains the inner boundary around the nested composite",
    );
    assert!(significant_steps(inner_program).is_empty());
    assert_eq!(
        (
            root_program.max_mask_depth,
            outer_program.max_mask_depth,
            inner_program.max_mask_depth,
        ),
        (1, 1, 0),
        "mask depth moves from each detached target to its receiver without changing the nesting maximum",
    );

    let outer_owner = outer_node.identity().boundary_root;
    let inner_owner = inner_node.identity().boundary_root;
    assert_eq!(
        mask_owners(root_program, root_steps),
        [outer_owner, outer_owner]
    );
    assert_eq!(
        mask_owners(outer_program, outer_node.steps()),
        [inner_owner, inner_owner],
    );
    assert!(mask_owners(inner_program, inner_node.steps()).is_empty());
}
