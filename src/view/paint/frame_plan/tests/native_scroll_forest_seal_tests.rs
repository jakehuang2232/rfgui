use super::*;

#[test]
fn native_scroll_forest_scaffold_seals_dense_dfs_parent_edges_and_dpr() {
    for scale_factor in [1.0, 2.0] {
        let (arena, roots, properties, generations) = native_scroll_forest_plan_fixture();
        let plan = plan_native_scroll_forest_scaffold_with_context(
            &arena,
            &roots,
            &properties,
            &generations,
            scale_factor,
            TransformSurfacePlanContext::default(),
        )
        .expect("native scroll forest scaffold");
        assert!(property_scene_plan_is_sealed(&plan));
        assert!(plan.property_scene_transaction_witness().is_none());
        assert!(plan.property_scene_context().is_none());
        let forest = plan
            .native_scroll_forest_planning_scaffold()
            .expect("dedicated forest seal");
        assert_eq!(forest.roots.len(), 2);
        assert_eq!(forest.roots[0].boundary_span, 0..4);
        assert_eq!(forest.roots[1].boundary_span, 4..6);
        assert_eq!(
            forest
                .boundaries
                .iter()
                .map(|boundary| boundary.id)
                .collect::<Vec<_>>(),
            (0..6).map(NativeScrollBoundaryId).collect::<Vec<_>>()
        );
        assert_eq!(forest.boundaries[0].parent, None);
        assert_eq!(forest.boundaries[1].parent, Some(NativeScrollBoundaryId(0)));
        assert_eq!(forest.boundaries[2].parent, Some(NativeScrollBoundaryId(1)));
        assert_eq!(forest.boundaries[3].parent, Some(NativeScrollBoundaryId(1)));
        assert_eq!(forest.boundaries[4].parent, None);
        assert_eq!(forest.boundaries[5].parent, None);
        let rounded_parent = &forest.programs[1];
        let child_positions = rounded_parent
            .content_steps
            .iter()
            .enumerate()
            .filter_map(|(index, step)| {
                matches!(step, NativeScrollForestContentProgramStep::ChildBoundary(_))
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        assert_eq!(child_positions.len(), 2);
        assert!(
            rounded_parent.content_steps[child_positions[0] + 1..child_positions[1]]
                .iter()
                .any(|step| matches!(step, NativeScrollForestContentProgramStep::Artifact(_)))
        );
        assert!(
            rounded_parent
                .host_before
                .artifact()
                .chunks
                .iter()
                .any(|chunk| {
                    chunk.id.slot == RETAINED_CHILD_MASK_SLOT
                        && chunk.id.phase == PaintNodePhase::BeforeChildren
                })
        );
        assert!(
            rounded_parent
                .overlay_after
                .artifact()
                .chunks
                .iter()
                .any(|chunk| {
                    chunk.id.slot == RETAINED_CHILD_MASK_SLOT
                        && chunk.id.phase == PaintNodePhase::AfterChildren
                })
        );
        assert_eq!(
            rounded_parent
                .child_dependencies
                .iter()
                .map(|dependency| dependency.child)
                .collect::<Vec<_>>(),
            [NativeScrollBoundaryId(2), NativeScrollBoundaryId(3)]
        );
        assert!(
            rounded_parent
                .child_dependencies
                .windows(2)
                .all(|pair| pair[0].parent_opaque_after <= pair[1].parent_opaque_before)
        );
        assert_eq!(
            rounded_parent.content_program_opaque_terminal,
            rounded_parent
                .child_dependencies
                .last()
                .map(|dependency| dependency.parent_opaque_after)
                .unwrap_or(rounded_parent.compiler_stamp.content_opaque_count)
        );
        let content_bounds = forest.boundaries[1].scroll.layout_content_bounds_at_zero;
        let color_key = crate::view::base_component::scroll_content_layer_stable_key(
            rounded_parent.receiver_stable_id,
        );
        let color = crate::view::base_component::texture_desc_for_logical_bounds(
            crate::view::base_component::RetainedSurfaceBounds {
                x: content_bounds.x,
                y: content_bounds.y,
                width: content_bounds.width,
                height: content_bounds.height,
                corner_radii: [0.0; 4],
            },
            scale_factor,
            None,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        );
        let (color, depth) = crate::view::base_component::persistent_target_texture_descriptors(
            color, color_key,
        );
        let stamp =
            crate::view::paint::compiler::validated_native_scroll_forest_content_raster_stamp(
                forest.boundaries[1].admission.content_root,
                rounded_parent.receiver_stable_id,
                crate::view::paint::compiler::RetainedSurfaceRasterInputs {
                    color,
                    depth,
                    scale_factor_bits: forest.scale_factor_bits,
                    source_bounds_bits: [
                        content_bounds.x.to_bits(),
                        content_bounds.y.to_bits(),
                        content_bounds.width.to_bits(),
                        content_bounds.height.to_bits(),
                    ],
                },
                rounded_parent.compiler_stamp.content_artifact_span.clone(),
                rounded_parent.child_dependencies.clone(),
                0..rounded_parent.content_program_opaque_terminal,
            )
            .expect("typed native forest content raster stamp");
        assert!(crate::view::paint::compiler::native_scroll_forest_content_raster_stamp_is_canonical(
            &stamp
        ));
        assert!(
            !crate::view::paint::compiler::retained_surface_raster_stamp_is_canonical(&stamp)
        );
        for boundary in &forest.boundaries {
            let expected = boundary
                .parent
                .map(|parent| forest.boundaries[parent.0 as usize].projection.live_input)
                .unwrap_or_default();
            assert_eq!(boundary.projection.projected_output, expected);
        }
        let signature = forest
            .schedule
            .steps
            .iter()
            .map(|step| match step {
                NativeScrollForestScheduledStep::ChildBoundary { parent, child } => {
                    (0, child.0, parent.map(|parent| parent.0))
                }
                NativeScrollForestScheduledStep::Artifact {
                    boundary,
                    phase: NativeScrollArtifactPhase::HostBefore,
                } => (1, boundary.0, None),
                NativeScrollForestScheduledStep::ContentReceiver { boundary, .. } => {
                    (2, boundary.0, None)
                }
                NativeScrollForestScheduledStep::Artifact {
                    boundary,
                    phase: NativeScrollArtifactPhase::OverlayAfter,
                } => (3, boundary.0, None),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            signature,
            vec![
                (0, 0, None),
                (1, 0, None),
                (0, 1, Some(0)),
                (1, 1, None),
                (0, 2, Some(1)),
                (1, 2, None),
                (2, 2, None),
                (3, 2, None),
                (2, 1, None),
                (0, 3, Some(1)),
                (1, 3, None),
                (2, 3, None),
                (3, 3, None),
                (3, 1, None),
                (3, 0, None),
                (0, 4, None),
                (1, 4, None),
                (2, 4, None),
                (3, 4, None),
                (0, 5, None),
                (1, 5, None),
                (2, 5, None),
                (3, 5, None),
            ]
        );
    }
}

#[test]
fn native_scroll_forest_scaffold_rejects_topology_schedule_projection_and_axis_tampering() {
    let (arena, roots, properties, generations) = native_scroll_forest_plan_fixture();
    let build = || {
        plan_native_scroll_forest_scaffold_with_context(
            &arena,
            &roots,
            &properties,
            &generations,
            1.0,
            TransformSurfacePlanContext::default(),
        )
        .unwrap()
    };
    fn forest(plan: &mut FramePaintPlan) -> &mut NativeScrollForestScaffold {
        plan.property_scene_seal
            .as_mut()
            .unwrap()
            .native_scroll_forest_scaffold
            .as_mut()
            .unwrap()
    }
    let mut cycle = build();
    let scaffold = forest(&mut cycle);
    scaffold.boundaries[0].parent = Some(NativeScrollBoundaryId(2));
    scaffold.planned_boundaries = scaffold.boundaries.clone();
    assert!(!property_scene_plan_is_sealed(&cycle));

    let mut duplicate = build();
    let scaffold = forest(&mut duplicate);
    scaffold.boundaries[1].id = NativeScrollBoundaryId(0);
    scaffold.planned_boundaries = scaffold.boundaries.clone();
    assert!(!property_scene_plan_is_sealed(&duplicate));

    let mut parent = build();
    let scaffold = forest(&mut parent);
    scaffold.boundaries[2].parent = Some(NativeScrollBoundaryId(0));
    scaffold.planned_boundaries = scaffold.boundaries.clone();
    assert!(!property_scene_plan_is_sealed(&parent));

    let mut reordered = build();
    let scaffold = forest(&mut reordered);
    scaffold.schedule.steps.swap(1, 2);
    scaffold.planned_schedule = scaffold.schedule.clone();
    assert!(!property_scene_plan_is_sealed(&reordered));

    let mut projection = build();
    let scaffold = forest(&mut projection);
    scaffold.boundaries[2].projection.projected_output = PropertyTreeState::default();
    scaffold.planned_boundaries = scaffold.boundaries.clone();
    assert!(!property_scene_plan_is_sealed(&projection));

    let mut span = build();
    let scaffold = forest(&mut span);
    scaffold.roots[0].boundary_span.end = 5;
    scaffold.planned_roots = scaffold.roots.clone();
    assert!(!property_scene_plan_is_sealed(&span));

    let mut dpr = build();
    let scaffold = forest(&mut dpr);
    scaffold.scale_factor_bits = 2.0_f32.to_bits();
    assert!(!property_scene_plan_is_sealed(&dpr));

    let mut axis = build();
    let scaffold = forest(&mut axis);
    scaffold.boundaries[1].scroll.configured_axis =
        crate::view::base_component::ScrollAxisSnapshot::Vertical;
    scaffold.planned_boundaries = scaffold.boundaries.clone();
    assert!(!property_scene_plan_is_sealed(&axis));

    let mut nonfinite = build();
    let scaffold = forest(&mut nonfinite);
    scaffold.boundaries[2].scroll.offset.x = f32::NAN;
    scaffold.planned_boundaries = scaffold.boundaries.clone();
    assert!(!property_scene_plan_is_sealed(&nonfinite));

    let mut clip_parent = build();
    let scaffold = forest(&mut clip_parent);
    scaffold.boundaries[2].contents_clip.parent = None;
    scaffold.planned_boundaries = scaffold.boundaries.clone();
    assert!(!property_scene_plan_is_sealed(&clip_parent));

    let mut host_identity = build();
    let scaffold = forest(&mut host_identity);
    scaffold.programs[0].host_before.identity.op_count += 1;
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&host_identity));

    let mut wrong_edge = build();
    let scaffold = forest(&mut wrong_edge);
    scaffold.programs[1].edge = scaffold.programs[0].edge;
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&wrong_edge));

    let mut wrong_marker = build();
    let scaffold = forest(&mut wrong_marker);
    let marker = scaffold.programs[1]
        .content_steps
        .iter_mut()
        .find_map(|step| match step {
            NativeScrollForestContentProgramStep::ChildBoundary(child) => Some(child),
            NativeScrollForestContentProgramStep::Artifact(_) => None,
        })
        .expect("nested boundary program has one child marker");
    *marker = NativeScrollBoundaryId(0);
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&wrong_marker));

    let mut compiler_stamp = build();
    let scaffold = forest(&mut compiler_stamp);
    scaffold.programs[2].compiler_stamp.content_op_count += 1;
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&compiler_stamp));

    let mut child_dependency = build();
    let scaffold = forest(&mut child_dependency);
    scaffold.programs[1].child_dependencies[0].offset_bits[0] ^= 1;
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&child_dependency));

    let mut dependency_id = build();
    let scaffold = forest(&mut dependency_id);
    scaffold.programs[1].child_dependencies[0].child = NativeScrollBoundaryId(3);
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&dependency_id));

    let mut dependency_stable = build();
    let scaffold = forest(&mut dependency_stable);
    scaffold.programs[1].child_dependencies[0].content_stable_id += 1;
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&dependency_stable));

    let mut dependency_host = build();
    let scaffold = forest(&mut dependency_host);
    scaffold.programs[1].child_dependencies[0]
        .host_identity
        .op_count += 1;
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&dependency_host));

    let mut dependency_generation = build();
    let scaffold = forest(&mut dependency_generation);
    scaffold.programs[1].child_dependencies[0].scroll.generation += 1;
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&dependency_generation));

    let mut dependency_scissor = build();
    let scaffold = forest(&mut dependency_scissor);
    scaffold.programs[1].child_dependencies[0].composite_scissor[2] += 1;
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&dependency_scissor));

    let mut dependency_cursor = build();
    let scaffold = forest(&mut dependency_cursor);
    scaffold.programs[1].child_dependencies[0].parent_opaque_after += 1;
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&dependency_cursor));

    let mut dependency_terminal = build();
    let scaffold = forest(&mut dependency_terminal);
    scaffold.programs[1].content_program_opaque_terminal += 1;
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&dependency_terminal));

    let mut receiver_stable = build();
    let scaffold = forest(&mut receiver_stable);
    scaffold.programs[2].receiver_stable_id += 1;
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&receiver_stable));

    let mut missing_mask_close = build();
    let scaffold = forest(&mut missing_mask_close);
    let overlay = &mut scaffold.programs[1].overlay_after;
    let close = overlay
        .recorded_artifact
        .chunks
        .iter()
        .position(|chunk| {
            chunk.id.slot == RETAINED_CHILD_MASK_SLOT
                && chunk.id.phase == PaintNodePhase::AfterChildren
        })
        .expect("rounded forest boundary has a mask close");
    let removed = overlay.recorded_artifact.chunks.remove(close);
    let removed_ops = removed.op_range.end - removed.op_range.start;
    overlay
        .recorded_artifact
        .ops
        .drain(removed.op_range.clone());
    for chunk in &mut overlay.recorded_artifact.chunks[close..] {
        chunk.op_range = chunk.op_range.start - removed_ops..chunk.op_range.end - removed_ops;
    }
    overlay.identity = property_scroll_receiver_artifact_identity(&overlay.recorded_artifact)
        .expect("tampered identity remains representable");
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&missing_mask_close));

    let mut wrong_mask_scope = build();
    let scaffold = forest(&mut wrong_mask_scope);
    let host = &mut scaffold.programs[1].host_before;
    let open = host
        .recorded_artifact
        .chunks
        .iter_mut()
        .find(|chunk| chunk.id.slot == RETAINED_CHILD_MASK_SLOT)
        .expect("rounded forest boundary has a mask open");
    open.id.scope = PaintPropertyScope::SelfPaint;
    host.identity = property_scroll_receiver_artifact_identity(&host.recorded_artifact)
        .expect("tampered identity remains representable");
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&wrong_mask_scope));

    let mut live_mask_properties = build();
    let scaffold = forest(&mut live_mask_properties);
    let boundary = scaffold.boundaries[1].clone();
    let host = &mut scaffold.programs[1].host_before;
    let open = host
        .recorded_artifact
        .chunks
        .iter_mut()
        .find(|chunk| chunk.id.slot == RETAINED_CHILD_MASK_SLOT)
        .expect("rounded forest boundary has a mask open");
    open.properties = PropertyTreeState {
        clip: Some(boundary.contents_clip.id),
        scroll: Some(boundary.scroll.id),
        ..Default::default()
    };
    host.identity = property_scroll_receiver_artifact_identity(&host.recorded_artifact)
        .expect("tampered identity remains representable");
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&live_mask_properties));

    let mut wrong_mask_order = build();
    let scaffold = forest(&mut wrong_mask_order);
    let overlay = &mut scaffold.programs[1].overlay_after;
    let first_ops = overlay.recorded_artifact.ops
        [overlay.recorded_artifact.chunks[0].op_range.clone()]
    .to_vec();
    let second_ops = overlay.recorded_artifact.ops
        [overlay.recorded_artifact.chunks[1].op_range.clone()]
    .to_vec();
    overlay.recorded_artifact.chunks.swap(0, 1);
    overlay.recorded_artifact.ops.clear();
    overlay.recorded_artifact.ops.extend(second_ops);
    let split = overlay.recorded_artifact.ops.len();
    overlay.recorded_artifact.ops.extend(first_ops);
    overlay.recorded_artifact.chunks[0].op_range = 0..split;
    overlay.recorded_artifact.chunks[1].op_range = split..overlay.recorded_artifact.ops.len();
    overlay.identity = property_scroll_receiver_artifact_identity(&overlay.recorded_artifact)
        .expect("tampered identity remains representable");
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(!property_scene_plan_is_sealed(&wrong_mask_order));
}

#[test]
fn native_scroll_forest_planner_rejects_unprepared_inline_wrapper_and_disabled_scroll_axis() {
    let (arena, roots, mut properties, mut generations) = native_scroll_forest_plan_fixture();
    let mut inline = Style::new();
    inline.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    crate::view::test_support::get_element_mut::<Element>(&arena, roots[0]).apply_style(inline);
    arena.refresh_subtree_dirty_cache(roots[0]);
    properties.sync(&arena, &roots);
    generations.sync(&arena, &roots, &properties);
    assert!(
        plan_native_scroll_forest_scaffold_with_context(
            &arena,
            &roots,
            &properties,
            &generations,
            1.0,
            TransformSurfacePlanContext::default(),
        )
        .is_err(),
        "unprepared Inline wrapper must fail closed"
    );

    let (arena, roots, mut properties, mut generations) = native_scroll_forest_plan_fixture();
    let first_scroll = arena.children_of(roots[0])[0];
    let mut disabled = Style::new();
    disabled.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::None),
    );
    disabled.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    crate::view::test_support::get_element_mut::<Element>(&arena, first_scroll)
        .apply_style(disabled);
    arena.refresh_subtree_dirty_cache(roots[0]);
    properties.sync(&arena, &roots);
    generations.sync(&arena, &roots, &properties);
    assert!(
        plan_native_scroll_forest_scaffold_with_context(
            &arena,
            &roots,
            &properties,
            &generations,
            1.0,
            TransformSurfacePlanContext::default(),
        )
        .is_err(),
        "disabled scroll axis cannot be reinterpreted as a neutral wrapper"
    );
}
