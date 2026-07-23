use super::*;

#[test]
fn direct_scroll_transform_transaction_is_one_generic_t_and_no_scroll_group() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
    let scaffold = super::super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0, 0.0],
        None,
    )
    .expect("direct S->T scaffold");
    let geometry = super::super::super::scroll_scene::plan_direct_scroll_transform_geometry(
        &arena,
        scaffold,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        super::super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(u32::MAX, u64::MAX)
            .unwrap(),
    )
    .expect("direct S->T geometry");
    let transaction =
        super::super::super::scroll_scene::compile_direct_scroll_transform_transaction(geometry)
            .expect("authority-specific direct S->T transaction");
    assert!(transaction.is_canonical());
    assert_eq!(transaction.transaction_shape_for_test(), [1, 1, 1, 1, 0, 0]);
    let stamp = transaction.stamp_for_test();
    assert_eq!(
        stamp.identity.role,
        crate::view::paint::RetainedSurfaceRasterRole::Transform
    );
    assert!(stamp.scroll_host.is_none());
    assert!(stamp.property_effect.is_none());
    assert!(matches!(
        stamp.ordered_steps.as_slice(),
        [crate::view::paint::RetainedSurfaceRasterStepStamp::ArtifactSpan(_)]
    ));
    let base_stamp = stamp.clone();
    let canonical_transaction = transaction.clone();

    let mut binding_tamper = transaction.clone();
    binding_tamper.tamper_transaction_binding_for_test();
    assert!(!binding_tamper.is_canonical());
    let mut synchronized_tamper = transaction;
    synchronized_tamper.tamper_synchronized_root_contract_for_test();
    assert!(!synchronized_tamper.is_canonical());
    assert!(!synchronized_tamper.inner_transaction_is_canonical_for_test());

    for variant in 0..4 {
        let mut authority_tamper = canonical_transaction.clone();
        authority_tamper.tamper_authority_for_test(variant);
        assert!(!authority_tamper.inner_transaction_is_canonical_for_test());
        assert!(!authority_tamper.is_canonical());
    }
    for variant in 0..3 {
        let mut boundary_tamper = canonical_transaction.clone();
        boundary_tamper.tamper_boundary_for_test(variant);
        assert!(!boundary_tamper.inner_transaction_is_canonical_for_test());
        assert!(!boundary_tamper.is_canonical());
    }
    let mut root_owner_tamper = canonical_transaction.clone();
    root_owner_tamper.tamper_root_owner_for_test();
    assert!(!root_owner_tamper.inner_transaction_is_canonical_for_test());
    assert!(!root_owner_tamper.is_canonical());
    let mut source_tamper = canonical_transaction.clone();
    source_tamper.tamper_synchronized_source_bounds_for_test();
    assert!(!source_tamper.inner_transaction_is_canonical_for_test());
    assert!(!source_tamper.is_canonical());
    let mut descriptor_tamper = canonical_transaction.clone();
    descriptor_tamper.tamper_synchronized_descriptor_for_test();
    assert!(!descriptor_tamper.is_canonical());
    let mut span_tamper = canonical_transaction;
    span_tamper.tamper_synchronized_artifact_span_for_test();
    assert!(!span_tamper.inner_transaction_is_canonical_for_test());
    assert!(!span_tamper.is_canonical());

    let (moved_arena, moved_root, _, _) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
    let moved_child = moved_arena.children_of(moved_root)[0];
    {
        let mut root_element =
            crate::view::test_support::get_element_mut::<Element>(&moved_arena, moved_root);
        root_element.set_scroll_offset((0.0, 40.0));
        root_element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    {
        let mut child_element =
            crate::view::test_support::get_element_mut::<Element>(&moved_arena, moved_child);
        child_element.layout_state.layout_position.y = -40.0;
        child_element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
            glam::Vec3::new(9.0, 0.0, 0.0),
        )));
        child_element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    moved_arena.refresh_subtree_dirty_cache(moved_root);
    let mut moved_properties = PropertyTrees::default();
    moved_properties.sync(&moved_arena, &[moved_root]);
    let mut moved_generations = PaintGenerationTracker::default();
    moved_generations.sync(&moved_arena, &[moved_root], &moved_properties);
    let moved_scaffold =
        super::super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
            &moved_arena,
            &[moved_root],
            &moved_properties,
            &moved_generations,
            1.0,
            [0.0, 0.0],
            None,
        )
        .expect("moved direct S->T scaffold");
    let moved_geometry = super::super::super::scroll_scene::plan_direct_scroll_transform_geometry(
        &moved_arena,
        moved_scaffold,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        super::super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(u32::MAX, u64::MAX)
            .unwrap(),
    )
    .expect("moved direct S->T geometry");
    let moved_transaction =
        super::super::super::scroll_scene::compile_direct_scroll_transform_transaction(moved_geometry)
            .expect("moved direct S->T transaction");
    assert_eq!(
        moved_transaction.stamp_for_test(),
        &base_stamp,
        "T matrix and S offset stay out of the T raster stamp",
    );
}

#[test]
fn direct_scroll_transform_prepare_freezes_action_before_graph_mutation() {
    let transaction = exact_direct_scroll_transform_transaction_for_test();
    let mut viewport = Viewport::new();
    let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut invalid_graph = FrameGraph::new();
    let invalid = super::super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
        &mut viewport,
        transaction.clone(),
        &mut invalid_graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Rgba8Unorm, 1.0),
        [0.0; 4],
        frame_owner,
    );
    assert!(matches!(
        invalid,
        Err(
            super::super::super::scroll_scene::RetainedPropertyScrollScenePrepareError::ContextMismatch
        )
    ));
    assert_eq!(invalid_graph.declared_persistent_texture_keys().count(), 0);
    assert!(viewport.retained_property_scroll_scene_stage_is_available());

    let mut graph = FrameGraph::new();
    let prepared = super::super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
        &mut viewport,
        transaction,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.125, 0.25, 0.5, 1.0],
        frame_owner,
    )
    .expect("direct S->T preflight");
    assert_eq!(
        prepared.action_for_test(),
        crate::view::paint::RetainedSurfaceCompileAction::Reraster
    );
    assert_eq!(prepared.transaction_shape_for_test(), [1, 0]);
    assert_eq!(prepared.graph_declared_key_count_for_test(), 0);
    let parent_terminal = prepared.parent_terminal_for_test();
    let outcome =
        super::super::super::scroll_scene::emit_prepared_direct_scroll_transform_scene(prepared);
    let (state, _) = outcome.into_parts();
    assert_eq!(state.opaque_rect_order(), parent_terminal);
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::ClearPass>()
            .len(),
        2
    );
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::texture_composite_pass::TextureCompositePass>()
            .len(),
        1
    );
    assert!(!viewport.retained_property_scroll_scene_stage_is_available());
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true));
}

#[test]
fn direct_scroll_transform_prepare_rejections_are_graph_pool_and_owner_atomic() {
    use super::super::super::scroll_scene::RetainedPropertyScrollScenePrepareError as PrepareError;

    macro_rules! reject_case {
        ($transaction:expr, $graph:expr, $ctx:expr, $clear:expr, $expected:expr) => {{
            let mut viewport = Viewport::new();
            let owner = viewport.begin_retained_surface_frame_stage().unwrap();
            let mut graph = $graph;
            let graph_before = graph.build_state_snapshot_for_test();
            let pool_before = viewport.retained_surface_transaction_shape_for_test();
            assert_eq!(
                super::super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
                    &mut viewport,
                    $transaction,
                    &mut graph,
                    $ctx,
                    $clear,
                    owner,
                )
                .err(),
                Some($expected)
            );
            assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
            assert_eq!(
                viewport.retained_surface_transaction_shape_for_test(),
                pool_before
            );
            assert!(viewport.retained_surface_frame_stage_owner_is_active(owner));
            assert!(viewport.retained_property_scroll_scene_stage_is_available());
            assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));
        }};
    }

    let default_ctx =
        || UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let base = exact_direct_scroll_transform_transaction_for_test();

    let mut descriptor = base.clone();
    descriptor.tamper_synchronized_descriptor_for_test();
    reject_case!(
        descriptor,
        FrameGraph::new(),
        default_ctx(),
        [0.0; 4],
        PrepareError::BoundaryDrift
    );
    for pair in [false, true] {
        let mut budget = base.clone();
        budget.tamper_synchronized_backing_budget_for_test(pair);
        reject_case!(
            budget,
            FrameGraph::new(),
            default_ctx(),
            [0.0; 4],
            PrepareError::DescriptorPair
        );
    }

    let (color_key, color_desc, depth_desc) = base.backing_for_test();
    let depth_key = color_key.depth_stencil().unwrap();
    for (key, desc) in [(color_key, color_desc), (depth_key, depth_desc)] {
        let mut graph = FrameGraph::new();
        let _ = graph.declare_persistent_texture_internal::<()>(desc, key);
        reject_case!(
            base.clone(),
            graph,
            default_ctx(),
            [0.0; 4],
            PrepareError::PersistentKeyAlreadyDeclared(color_key)
        );
    }

    let mut offset = default_ctx();
    offset.set_paint_offset([0.25, 0.0]);
    let mut scissor = default_ctx();
    scissor.replace_scissor_rect(Some([0, 0, 1, 1]));
    let mut cursor = default_ctx();
    let _ = cursor.next_opaque_rect_order();
    let mut transform = default_ctx();
    transform.set_current_render_transform(Some(glam::Mat4::IDENTITY));
    let contexts = [
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 2.0),
        offset,
        scissor,
        cursor,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Rgba8Unorm, 1.0),
        transform,
    ];
    for ctx in contexts {
        reject_case!(
            base.clone(),
            FrameGraph::new(),
            ctx,
            [0.0; 4],
            PrepareError::ContextMismatch
        );
    }
    reject_case!(
        base.clone(),
        FrameGraph::new(),
        default_ctx(),
        [f32::NAN, 0.0, 0.0, 0.0],
        PrepareError::ContextMismatch
    );

    let mut foreign_graph = FrameGraph::new();
    let mut foreign_ctx = default_ctx();
    let foreign_target = foreign_ctx.allocate_target(&mut foreign_graph);
    foreign_ctx.set_current_target(foreign_target);
    reject_case!(
        base.clone(),
        FrameGraph::new(),
        foreign_ctx,
        [0.0; 4],
        PrepareError::ParentTarget
    );

    let mut stale_viewport = Viewport::new();
    let stale_owner = stale_viewport.begin_retained_surface_frame_stage().unwrap();
    assert!(
        stale_viewport.finish_retained_surface_transaction_for_frame(Some(stale_owner), false)
    );
    let mut stale_graph = FrameGraph::new();
    let graph_before = stale_graph.build_state_snapshot_for_test();
    let pool_before = stale_viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        super::super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
            &mut stale_viewport,
            base,
            &mut stale_graph,
            default_ctx(),
            [0.0; 4],
            stale_owner,
        )
        .err(),
        Some(PrepareError::StageUnavailable)
    );
    assert_eq!(stale_graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        stale_viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );

    let mut occupied_viewport = Viewport::new();
    let occupied_owner = occupied_viewport
        .begin_retained_surface_frame_stage()
        .unwrap();
    let mut seed_graph = FrameGraph::new();
    let seed = super::super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
        &mut occupied_viewport,
        exact_direct_scroll_transform_transaction_for_test(),
        &mut seed_graph,
        default_ctx(),
        [0.0; 4],
        occupied_owner,
    )
    .unwrap();
    let _ = super::super::super::scroll_scene::emit_prepared_direct_scroll_transform_scene(seed);
    assert!(!occupied_viewport.retained_property_scroll_scene_stage_is_available());
    let mut occupied_graph = FrameGraph::new();
    let graph_before = occupied_graph.build_state_snapshot_for_test();
    let pool_before = occupied_viewport.retained_surface_transaction_shape_for_test();
    let owner_active_before =
        occupied_viewport.retained_surface_frame_stage_owner_is_active(occupied_owner);
    assert_eq!(
        super::super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
            &mut occupied_viewport,
            exact_direct_scroll_transform_transaction_for_test(),
            &mut occupied_graph,
            default_ctx(),
            [0.0; 4],
            occupied_owner,
        )
        .err(),
        Some(PrepareError::StageUnavailable)
    );
    assert_eq!(occupied_graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        occupied_viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert_eq!(
        occupied_viewport.retained_surface_frame_stage_owner_is_active(occupied_owner),
        owner_active_before
    );
    assert!(
        occupied_viewport
            .finish_retained_surface_transaction_for_frame(Some(occupied_owner), true)
    );
}

#[test]
fn direct_scroll_transform_action_matrix_keeps_composite_inputs_dynamic() {
    let (arena, root, mut properties, mut generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
    let child = arena.children_of(root)[0];
    let ctx = || UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let mut viewport = Viewport::new();

    let cold_transaction = direct_scroll_transform_transaction_from_fixture_for_test(
        &arena,
        root,
        &properties,
        &generations,
    );
    let color_key = cold_transaction.backing_for_test().0;
    let cold_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut cold_graph = FrameGraph::new();
    let cold = super::super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
        &mut viewport,
        cold_transaction,
        &mut cold_graph,
        ctx(),
        [0.0; 4],
        cold_owner,
    )
    .unwrap();
    assert_eq!(
        cold.action_for_test(),
        crate::view::paint::RetainedSurfaceCompileAction::Reraster
    );
    let _ = super::super::super::take_artifact_compile_count();
    let _ = super::super::super::scroll_scene::emit_prepared_direct_scroll_transform_scene(cold);
    assert_eq!(super::super::super::take_artifact_compile_count(), 3);
    assert_eq!(
        cold_graph
            .test_graphics_passes::<crate::view::render_pass::ClearPass>()
            .len(),
        2
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

    let mut run =
        |transaction: super::super::super::scroll_scene::ValidatedDirectScrollTransformTransaction,
         expected,
         expected_content_clears| {
            let owner = viewport.begin_retained_surface_frame_stage().unwrap();
            let mut graph = FrameGraph::new();
            let mut prepared =
                super::super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
                    &mut viewport,
                    transaction,
                    &mut graph,
                    ctx(),
                    [0.0; 4],
                    owner,
                )
                .unwrap();
            prepared.refresh_action_from_committed_test_pool();
            assert_eq!(prepared.action_for_test(), expected);
            let composite = prepared.composite_params_for_test();
            let _ = super::super::super::take_artifact_compile_count();
            let _ = super::super::super::scroll_scene::emit_prepared_direct_scroll_transform_scene(
                prepared,
            );
            assert_eq!(
                super::super::super::take_artifact_compile_count(),
                if expected == crate::view::paint::RetainedSurfaceCompileAction::Reraster {
                    3
                } else {
                    2
                }
            );
            let composites = graph.test_graphics_passes::<
                crate::view::render_pass::texture_composite_pass::TextureCompositePass,
            >();
            let matching = composites
                .iter()
                .filter(|pass| {
                    let snapshot = pass.test_snapshot();
                    snapshot.bounds_bits == composite.bounds.map(f32::to_bits)
                        && snapshot.quad_position_bits
                            == composite
                                .quad_positions
                                .map(|quad| quad.map(|point| point.map(f32::to_bits)))
                        && snapshot.uv_bounds_bits
                            == composite.uv_bounds.map(|uv| uv.map(f32::to_bits))
                        && snapshot.explicit_scissor_rect == composite.scissor_rect
                })
                .collect::<Vec<_>>();
            assert_eq!(
                matching.len(),
                1,
                "final direct S->T composite is exact-once"
            );
            let content_target = matching[0]
                .test_snapshot()
                .source_handle
                .expect("direct S->T composite samples its persistent T target");
            let content_clears = graph
                .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                .iter()
                .filter(|pass| pass.test_snapshot().output_target == Some(content_target))
                .count();
            assert_eq!(content_clears, expected_content_clears);
            assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
        };
    let reuse = crate::view::paint::RetainedSurfaceCompileAction::Reuse;
    let reraster = crate::view::paint::RetainedSurfaceCompileAction::Reraster;

    run(
        direct_scroll_transform_transaction_from_fixture_for_test(
            &arena,
            root,
            &properties,
            &generations,
        ),
        reuse,
        0,
    );

    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            11.0, 4.0, 0.0,
        ))));
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    run(
        direct_scroll_transform_transaction_from_fixture_for_test(
            &arena,
            root,
            &properties,
            &generations,
        ),
        reuse,
        0,
    );

    {
        let mut root_element =
            crate::view::test_support::get_element_mut::<Element>(&arena, root);
        root_element.set_scroll_offset((0.0, 37.0));
        root_element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    {
        let mut child_element =
            crate::view::test_support::get_element_mut::<Element>(&arena, child);
        child_element.layout_state.layout_position.y = -37.0;
        child_element.layout_state.layout_inner_position.y = -37.0;
        child_element.layout_state.layout_flow_position.y = -37.0;
        child_element.layout_state.layout_flow_inner_position.y = -37.0;
        child_element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    run(
        direct_scroll_transform_transaction_from_fixture_for_test(
            &arena,
            root,
            &properties,
            &generations,
        ),
        reuse,
        0,
    );

    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_sampled_scrollbar_alpha_for_test(1.0);
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    run(
        direct_scroll_transform_transaction_from_fixture_for_test(
            &arena,
            root,
            &properties,
            &generations,
        ),
        reuse,
        0,
    );

    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_background_color_value(Color::rgb(18, 36, 54));
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    run(
        direct_scroll_transform_transaction_from_fixture_for_test(
            &arena,
            root,
            &properties,
            &generations,
        ),
        reuse,
        0,
    );

    {
        let mut root_element =
            crate::view::test_support::get_element_mut::<Element>(&arena, root);
        root_element.layout_state.layout_size.height = 72.0;
        root_element.layout_state.layout_inner_size.height = 72.0;
        root_element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    run(
        direct_scroll_transform_transaction_from_fixture_for_test(
            &arena,
            root,
            &properties,
            &generations,
        ),
        reuse,
        0,
    );

    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_background_color_value(Color::rgb(72, 48, 24));
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    run(
        direct_scroll_transform_transaction_from_fixture_for_test(
            &arena,
            root,
            &properties,
            &generations,
        ),
        reraster,
        1,
    );

    drop(run);
    viewport.forget_retained_surface_pair_witness_for_test(color_key);
    let pair_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut pair_graph = FrameGraph::new();
    let mut pair = super::super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
        &mut viewport,
        direct_scroll_transform_transaction_from_fixture_for_test(
            &arena,
            root,
            &properties,
            &generations,
        ),
        &mut pair_graph,
        ctx(),
        [0.0; 4],
        pair_owner,
    )
    .unwrap();
    pair.refresh_action_from_committed_test_pool();
    assert_eq!(pair.action_for_test(), reraster);
    let composite = pair.composite_params_for_test();
    let _ = super::super::super::take_artifact_compile_count();
    let _ = super::super::super::scroll_scene::emit_prepared_direct_scroll_transform_scene(pair);
    assert_eq!(super::super::super::take_artifact_compile_count(), 3);
    let pair_composites = pair_graph
        .test_graphics_passes::<crate::view::render_pass::texture_composite_pass::TextureCompositePass>();
    let matching = pair_composites
        .iter()
        .filter(|pass| {
            let snapshot = pass.test_snapshot();
            snapshot.bounds_bits == composite.bounds.map(f32::to_bits)
                && snapshot.quad_position_bits
                    == composite
                        .quad_positions
                        .map(|quad| quad.map(|point| point.map(f32::to_bits)))
                && snapshot.uv_bounds_bits == composite.uv_bounds.map(|uv| uv.map(f32::to_bits))
                && snapshot.explicit_scissor_rect == composite.scissor_rect
        })
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 1);
    let content_target = matching[0].test_snapshot().source_handle.unwrap();
    assert_eq!(
        pair_graph
            .test_graphics_passes::<crate::view::render_pass::ClearPass>()
            .iter()
            .filter(|pass| pass.test_snapshot().output_target == Some(content_target))
            .count(),
        1
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(pair_owner), true));
}
