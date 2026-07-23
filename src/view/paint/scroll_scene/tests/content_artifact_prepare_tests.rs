use super::*;

#[test]
fn exact_plan_splits_three_named_artifacts_and_prepares_one_content_surface() {
    let plan = plan_at_offset([0.0, 20.0]);
    let (host_before, content_local, overlay_artifact) = plan.existing_recorded_artifacts();
    let [host] = host_before.chunks.as_slice() else {
        panic!("host-before must remain one chunk")
    };
    let [content] = content_local.chunks.as_slice() else {
        panic!("exact direct Element leaf must remain one content chunk")
    };
    let [overlay] = overlay_artifact.chunks.as_slice() else {
        panic!("overlay must remain one chunk even while hidden")
    };
    assert_eq!(
        (host.owner, host.id.scope, host.id.phase, host.id.role),
        (
            plan.boundary_root,
            PaintPropertyScope::SelfPaint,
            PaintNodePhase::BeforeChildren,
            PaintChunkRole::SelfDecoration,
        )
    );
    assert_eq!(
        (
            content.owner,
            content.id.scope,
            content.id.phase,
            content.id.role
        ),
        (
            plan.content_root,
            PaintPropertyScope::SelfPaint,
            PaintNodePhase::BeforeChildren,
            PaintChunkRole::SelfDecoration,
        )
    );
    assert_eq!(
        (overlay.owner, overlay.id.phase, overlay.id.role),
        (
            plan.boundary_root,
            PaintNodePhase::AfterChildren,
            PaintChunkRole::ScrollbarOverlay,
        )
    );
    assert_eq!(host.properties, Default::default());
    assert_eq!(content.properties, Default::default());
    assert_eq!(overlay.properties, Default::default());
    assert_eq!(
        content_local.owner_nodes,
        vec![PaintOwnerSnapshot {
            owner: plan.content_root,
            parent: None,
        }]
    );

    let graph = FrameGraph::new();
    let prepared = prepare(&plan, &graph, wgpu::TextureFormat::Bgra8UnormSrgb).unwrap();
    let stamp = prepared.content_stamp();
    assert_eq!(
        stamp.identity.role,
        RetainedSurfaceRasterRole::ScrollContent
    );
    assert_eq!(stamp.identity.boundary_root, plan.content_root);
    assert_eq!(stamp.identity.stable_id, plan.content_stable_id);
    assert_eq!(
        prepared.content_key(),
        scroll_content_layer_stable_key(plan.content_stable_id)
    );
    assert!(stamp.scroll_host.is_none());
    assert!(stamp.clip_nodes.is_empty());
    assert_eq!(stamp.ordered_steps.len(), 1);
    assert_eq!(graph.declared_persistent_texture_keys().count(), 0);
    assert!(graph.pass_descriptors().is_empty());
}

#[test]
fn offset_only_changes_composite_geometry_but_reuses_identical_content_stamp() {
    let first_plan = plan_at_offset([0.0, 20.0]);
    let second_plan = plan_at_offset([7.5, 47.25]);
    let graph = FrameGraph::new();
    let first = prepare(&first_plan, &graph, wgpu::TextureFormat::Bgra8UnormSrgb).unwrap();
    let second = prepare(&second_plan, &graph, wgpu::TextureFormat::Bgra8UnormSrgb).unwrap();
    assert_eq!(first.content_stamp(), second.content_stamp());
    assert_eq!(
        crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
            first.content_stamp().clone(),
            second.content_stamp(),
        ),
        RetainedSurfaceCompileAction::Reuse
    );
    let first_params = first.content_geometry().texture_composite_params();
    let second_params = second.content_geometry().texture_composite_params();
    assert_ne!(
        first_params.bounds.map(f32::to_bits),
        second_params.bounds.map(f32::to_bits)
    );
    assert_eq!(
        first_params.uv_bounds.unwrap().map(f32::to_bits),
        second_params.uv_bounds.unwrap().map(f32::to_bits)
    );
    assert_eq!(
        first.content_geometry().source_bounds_bits(),
        second.content_geometry().source_bounds_bits()
    );
}

#[test]
fn prepare_independently_rejects_malicious_host_content_and_overlay_artifacts() {
    let baseline = plan_at_offset([0.0, 20.0]);
    let graph = FrameGraph::new();

    let mut bad_host = baseline.clone();
    bad_host.existing_recorded_artifacts_mut().0.chunks[0]
        .id
        .phase = PaintNodePhase::AfterChildren;
    assert!(matches!(
        prepare(&bad_host, &graph, wgpu::TextureFormat::Bgra8UnormSrgb),
        Err(ScrollScenePrepareError::ArtifactStore)
    ));

    let mut bad_content = baseline.clone();
    bad_content.existing_recorded_artifacts_mut().1.chunks[0]
        .id
        .owner = baseline.boundary_root;
    assert!(matches!(
        prepare(&bad_content, &graph, wgpu::TextureFormat::Bgra8UnormSrgb),
        Err(ScrollScenePrepareError::ArtifactStore)
    ));

    let mut bad_overlay = baseline;
    bad_overlay.existing_recorded_artifacts_mut().2.chunks[0]
        .id
        .role = PaintChunkRole::SelfDecoration;
    assert!(matches!(
        prepare(&bad_overlay, &graph, wgpu::TextureFormat::Bgra8UnormSrgb),
        Err(ScrollScenePrepareError::ArtifactStore)
    ));
    assert_eq!(graph.declared_persistent_texture_keys().count(), 0);
    assert!(graph.pass_descriptors().is_empty());
}

#[test]
fn stamp_bound_geometry_rejects_key_bounds_and_clip_drift() {
    let plan = plan_at_offset([7.5, 47.25]);
    let graph = FrameGraph::new();
    let prepared = prepare(&plan, &graph, wgpu::TextureFormat::Bgra8UnormSrgb).unwrap();
    let expected_params = prepared.content_geometry().texture_composite_params();
    assert_eq!(
        expected_params.uv_bounds.unwrap().map(f32::to_bits),
        prepared.content_stamp().target.source_bounds_bits
    );
    assert_eq!(
        expected_params.scissor_rect,
        Some(plan.contents_clip.logical_scissor)
    );

    let mut wrong_key = prepared.content_stamp().clone();
    wrong_key.identity.color_key = PersistentTextureKey::retained(
        RetainedTextureRole::ScrollHostColor,
        plan.content_stable_id,
    );
    assert!(
        PreparedScrollContentCompositeGeometry::from_validated_content_stamp(
            &wrong_key,
            plan.scroll,
            plan.contents_clip,
        )
        .is_none()
    );

    let mut wrong_bounds = prepared.content_stamp().clone();
    wrong_bounds.target.source_bounds_bits[2] = 301.0_f32.to_bits();
    assert!(
        PreparedScrollContentCompositeGeometry::from_validated_content_stamp(
            &wrong_bounds,
            plan.scroll,
            plan.contents_clip,
        )
        .is_none()
    );

    let mut wrong_clip = plan.contents_clip;
    wrong_clip.logical_scissor[2] -= 1;
    assert!(
        PreparedScrollContentCompositeGeometry::from_validated_content_stamp(
            prepared.content_stamp(),
            plan.scroll,
            wrong_clip,
        )
        .is_none()
    );
}

#[test]
fn single_texture_dimension_and_color_depth_pair_budget_are_strict_boundaries() {
    let plan = plan_at_offset([0.0, 20.0]);
    let graph = FrameGraph::new();
    let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let baseline = prepare_scroll_scene(plan.clone(), &graph, &ctx, generous_budget()).unwrap();
    let exact =
        ScrollSceneSingleTextureBudget::new(300, baseline.content_pair_bytes()).unwrap();
    assert!(prepare_scroll_scene(plan.clone(), &graph, &ctx, exact).is_ok());
    let one_byte_short =
        ScrollSceneSingleTextureBudget::new(300, baseline.content_pair_bytes() - 1).unwrap();
    assert_eq!(
        prepare_scroll_scene(plan.clone(), &graph, &ctx, one_byte_short).err(),
        Some(ScrollScenePrepareError::ActiveTileBudget)
    );
    let one_pixel_short = ScrollSceneSingleTextureBudget::new(299, u64::MAX).unwrap();
    assert_eq!(
        prepare_scroll_scene(plan.clone(), &graph, &ctx, one_pixel_short).err(),
        Some(ScrollScenePrepareError::SingleTextureLimit)
    );
    let unknown_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Rgba32Float, 1.0);
    assert_eq!(
        prepare_scroll_scene(plan.clone(), &graph, &unknown_ctx, generous_budget()).err(),
        Some(ScrollScenePrepareError::RasterCostUnknown)
    );
}

#[test]
fn prepare_success_and_rejections_are_graph_inert_and_action_is_typestate_owned() {
    let plan = plan_at_offset([0.0, 20.0]);
    let graph = FrameGraph::new();
    let before = (
        graph.pass_descriptors().len(),
        graph.declared_persistent_texture_keys().count(),
        graph.declared_persistent_textures().count(),
    );
    let prepared = prepare(&plan, &graph, wgpu::TextureFormat::Bgra8UnormSrgb).unwrap();
    assert_eq!(
        before,
        (
            graph.pass_descriptors().len(),
            graph.declared_persistent_texture_keys().count(),
            graph.declared_persistent_textures().count(),
        )
    );
    let frozen = prepared.freeze_content_action(RetainedSurfaceCompileAction::Reuse);
    assert_eq!(frozen.content_action(), RetainedSurfaceCompileAction::Reuse);
    let emission = frozen.into_emission_parts();
    assert_eq!(
        emission.content_stamp().identity.role,
        RetainedSurfaceRasterRole::ScrollContent
    );
    assert_eq!(
        emission.content_action(),
        RetainedSurfaceCompileAction::Reuse
    );

    let mut bad_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    bad_ctx.translate_paint_offset(0.25, 0.0);
    assert_eq!(
        prepare_scroll_scene(plan.clone(), &graph, &bad_ctx, generous_budget()).err(),
        Some(ScrollScenePrepareError::ContextMismatch)
    );
    assert_eq!(
        before,
        (
            graph.pass_descriptors().len(),
            graph.declared_persistent_texture_keys().count(),
            graph.declared_persistent_textures().count(),
        )
    );
}

#[test]
fn declared_content_key_foreign_parent_and_frozen_witness_reject_without_new_graph_state() {
    let plan = plan_at_offset([0.0, 20.0]);
    let mut declared_graph = FrameGraph::new();
    let mut declaration_ctx =
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let _ = declaration_ctx.allocate_persistent_target_with_key(
        &mut declared_graph,
        scroll_content_layer_stable_key(plan.content_stable_id),
        content_zero_bounds(plan.scroll),
    );
    let before = (
        declared_graph.pass_descriptors().len(),
        declared_graph.declared_persistent_texture_keys().count(),
        declared_graph.declared_persistent_textures().count(),
    );
    let fresh_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    assert!(matches!(
        prepare_scroll_scene(plan.clone(), &declared_graph, &fresh_ctx, generous_budget(),),
        Err(ScrollScenePrepareError::PersistentKeyAlreadyDeclared(_))
    ));
    assert_eq!(
        before,
        (
            declared_graph.pass_descriptors().len(),
            declared_graph.declared_persistent_texture_keys().count(),
            declared_graph.declared_persistent_textures().count(),
        )
    );

    let mut source_graph = FrameGraph::new();
    let mut foreign_ctx =
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let foreign_target = foreign_ctx.allocate_target(&mut source_graph);
    foreign_ctx.set_current_target(foreign_target);
    let untouched_graph = FrameGraph::new();
    assert_eq!(
        prepare_scroll_scene(
            plan.clone(),
            &untouched_graph,
            &foreign_ctx,
            generous_budget(),
        )
        .err(),
        Some(ScrollScenePrepareError::ParentTarget)
    );
    assert!(untouched_graph.pass_descriptors().is_empty());
    assert_eq!(
        untouched_graph.declared_persistent_texture_keys().count(),
        0
    );

    let mut drifted = plan.clone();
    drifted.planned_scroll_witness.offset.y += 1.0;
    assert_eq!(
        prepare(
            &drifted,
            &untouched_graph,
            wgpu::TextureFormat::Bgra8UnormSrgb
        )
        .err(),
        Some(ScrollScenePrepareError::FrozenWitness)
    );
    assert!(untouched_graph.pass_descriptors().is_empty());
}

#[test]
fn scene_plan_cannot_bypass_the_a1_direct_element_leaf_gate() {
    let (mut arena, root, child, mut properties, mut generations) =
        fixture_at_offset([0.0, 20.0]);
    let grandchild = arena.insert(Node::new(Box::new(Element::new_with_id(
        82_003, 0.0, 0.0, 10.0, 10.0,
    ))));
    arena.set_parent(grandchild, Some(child));
    arena.push_child(child, grandchild);
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    assert!(
        plan_single_root_scroll_scene(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
        )
        .is_err()
    );
    assert!(
        plan_property_scroll_scene_scaffold(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .is_err()
    );
}
