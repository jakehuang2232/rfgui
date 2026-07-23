use super::*;

#[test]
fn transform_effect_scroll_emit_orders_hco_effect_and_translation_once() {
    let (arena, root, properties, generations) = transform_effect_scroll_fixture();
    let scene = plan_and_validate_transform_effect_scroll_scene(
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
    .unwrap();
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_retained_transform_effect_scroll_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.125, 0.25, 0.5, 1.0],
        owner,
    )
    .unwrap();
    let outcome = emit_prepared_retained_transform_effect_scroll_scene(prepared);
    assert_eq!(outcome.trace.root_count, 1);
    assert_eq!(outcome.trace.generic_surface_count, 2);
    let effect_composites = graph.test_graphics_passes::<CompositeLayerPass>();
    assert_eq!(effect_composites.len(), 1);
    assert_eq!(
        effect_composites[0].test_snapshot().opacity_bits,
        0.625_f32.to_bits()
    );
    assert_eq!(
        graph.test_graphics_passes::<TextureCompositePass>().len(),
        2
    );
    assert_eq!(
        graph
            .test_graphics_passes::<ClearPass>()
            .iter()
            .filter(|pass| {
                pass.test_snapshot().color_bits == [0.125_f32, 0.25, 0.5, 1.0].map(f32::to_bits)
            })
            .count(),
        1
    );
    assert!(!viewport.retained_property_scroll_scene_stage_is_available());
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
}

#[test]
fn effect_transform_scroll_action_matrix_keeps_opacity_final_and_translation_parent_local() {
    fn actions(
        prepared: &PreparedRetainedEffectTransformScrollScene<'_>,
    ) -> [RetainedSurfaceCompileAction; 3] {
        let [root] = prepared.roots.as_slice() else {
            panic!("one E -> T -> Scroll root")
        };
        let content_key = root.inner.boundary.group.active_resident_keys()[0];
        [
            prepared.actions[&root.outer_stamp.identity.resident_key()],
            prepared.actions[&root.inner.receiver_stamp.identity.resident_key()],
            prepared.actions[&content_key],
        ]
    }

    let reuse = RetainedSurfaceCompileAction::Reuse;
    let reraster = RetainedSurfaceCompileAction::Reraster;
    let sampled_at = crate::time::Instant::now();
    let (arena, root, mut properties, mut generations) = effect_transform_scroll_fixture();
    let transform = arena.children_of(root)[0];
    let ctx = || UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let mut viewport = Viewport::new();

    let first_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut first_graph = FrameGraph::new();
    let first = prepare_retained_effect_transform_scroll_scene_from_pool(
        &mut viewport,
        validated_effect_transform_scroll_fixture_scene(
            &arena,
            root,
            &properties,
            &generations,
            sampled_at,
            1.0,
        ),
        &mut first_graph,
        ctx(),
        [0.0; 4],
        first_owner,
    )
    .unwrap();
    assert_eq!(actions(&first), [reraster, reraster, reraster]);
    assert!(
        crate::view::paint::retained_surface_executor::legacy_property_executor_rejects_effect_transform_scroll_child_for_test(
            &first.roots[0].outer_stamp,
        )
    );
    let [
        crate::view::paint::RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(
            dependency,
        ),
    ] = first.roots[0]
        .outer_stamp
        .ordered_steps
        .iter()
        .filter(|step| {
            matches!(
                step,
                crate::view::paint::RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(
                    _
                )
            )
        })
        .collect::<Vec<_>>()
        .as_slice()
    else {
        panic!("outer E owns exactly one typed transform child")
    };
    assert!(dependency.child_stamp.ordered_steps.iter().any(|step| {
        matches!(
            step,
            crate::view::paint::RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
        )
    }));
    let mut child_transform_drift = first.roots[0].outer_stamp.clone();
    let Some(crate::view::paint::RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(
        dependency,
    )) = child_transform_drift.ordered_steps.iter_mut().find(|step| {
        matches!(
            step,
            crate::view::paint::RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
        )
    })
    else {
        panic!("outer E stamp owns one typed transform child")
    };
    dependency.child_transform = crate::view::compositor::property_tree::TransformNodeId(root);
    assert!(
        !crate::view::paint::compiler::effect_transform_scroll_outer_raster_stamp_validates_contract(
            &child_transform_drift,
            &first.roots[0].outer_artifact_contract,
            first.roots[0].inner.receiver.id,
            first.roots[0].inner.geometry,
        )
    );
    let _ = emit_prepared_retained_effect_transform_scroll_scene(first);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(first_owner), true));

    crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.375);
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let opacity_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut opacity_graph = FrameGraph::new();
    let mut opacity = prepare_retained_effect_transform_scroll_scene_from_pool(
        &mut viewport,
        validated_effect_transform_scroll_fixture_scene(
            &arena,
            root,
            &properties,
            &generations,
            sampled_at,
            1.0,
        ),
        &mut opacity_graph,
        ctx(),
        [0.0; 4],
        opacity_owner,
    )
    .unwrap();
    opacity.refresh_actions_from_committed_test_pool();
    assert_eq!(actions(&opacity), [reuse, reuse, reuse]);
    let _ = emit_prepared_retained_effect_transform_scroll_scene(opacity);
    let final_effects = opacity_graph.test_graphics_passes::<CompositeLayerPass>();
    let final_effect = final_effects
        .last()
        .expect("opacity-only reuse still emits final E composite");
    assert_eq!(
        final_effect.test_snapshot().opacity_bits,
        0.375_f32.to_bits()
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(opacity_owner), true));

    crate::view::test_support::get_element_mut::<Element>(&arena, transform)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            19.0, 11.0, 0.0,
        ))));
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let translation_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut translation_graph = FrameGraph::new();
    let mut translation = prepare_retained_effect_transform_scroll_scene_from_pool(
        &mut viewport,
        validated_effect_transform_scroll_fixture_scene(
            &arena,
            root,
            &properties,
            &generations,
            sampled_at,
            1.0,
        ),
        &mut translation_graph,
        ctx(),
        [0.0; 4],
        translation_owner,
    )
    .unwrap();
    translation.refresh_actions_from_committed_test_pool();
    assert_eq!(actions(&translation), [reraster, reuse, reuse]);
    let _ = emit_prepared_retained_effect_transform_scroll_scene(translation);
    assert!(
        viewport.finish_retained_surface_transaction_for_frame(Some(translation_owner), true)
    );
}

#[test]
fn effect_transform_scroll_combined_inner_token_rejects_cross_cutout_mask_tampering() {
    let (arena, root, properties, generations) = effect_transform_scroll_neutral_fixture();
    let scene = validated_effect_transform_scroll_fixture_scene(
        &arena,
        root,
        &properties,
        &generations,
        crate::time::Instant::now(),
        1.0,
    );
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    {
        let prepared = prepare_retained_effect_transform_scroll_scene_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            owner,
        )
        .unwrap();
        assert!(
            prepared.roots[0]
                .validated_inner_steps
                .rejects_effect_transform_scroll_inner_tampering(),
            "combined inner token must reject mask reorder/missing end, foreign marker, and owner-topology drift"
        );
    }
    assert!(viewport.retained_property_scroll_scene_stage_is_available());
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));
}

#[test]
fn transform_effect_scroll_action_matrix_separates_outer_effect_and_content() {
    fn actions(
        prepared: &PreparedRetainedTransformEffectScrollScene<'_>,
    ) -> [RetainedSurfaceCompileAction; 3] {
        let [root] = prepared.roots.as_slice() else {
            panic!("one T -> E -> Scroll root")
        };
        let content_key = root.inner.boundary.group.active_resident_keys()[0];
        [
            prepared.actions[&root.outer_stamp.identity.resident_key()],
            prepared.actions[&root.inner.receiver_stamp.identity.resident_key()],
            prepared.actions[&content_key],
        ]
    }

    fn assert_clear_delta(graph: &FrameGraph, expected: usize) {
        assert_eq!(
            graph.test_graphics_passes::<ClearPass>().len(),
            expected + 1,
            "one root clear plus one target clear per frozen reraster action"
        );
    }

    let reuse = RetainedSurfaceCompileAction::Reuse;
    let reraster = RetainedSurfaceCompileAction::Reraster;
    let sampled_at = crate::time::Instant::now();
    let (arena, root, mut properties, mut generations) = transform_effect_scroll_fixture();
    let effect = arena.children_of(root)[0];
    let scroll = arena.children_of(effect)[0];
    let content = arena.children_of(scroll)[0];
    let ctx = || UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let mut viewport = Viewport::new();

    // Seed all three residents and retain their exact pair-witness keys.
    let first_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut first_graph = FrameGraph::new();
    let first = prepare_retained_transform_effect_scroll_scene_from_pool(
        &mut viewport,
        validated_transform_effect_scroll_fixture_scene(
            &arena,
            root,
            &properties,
            &generations,
            sampled_at,
        ),
        &mut first_graph,
        ctx(),
        [0.0; 4],
        first_owner,
    )
    .unwrap();
    assert_eq!(actions(&first), [reraster, reraster, reraster]);
    let [first_root] = first.roots.as_slice() else {
        unreachable!()
    };
    let outer_color_key = first_root.outer_stamp.identity.color_key;
    let inner_color_key = first_root.inner.receiver_stamp.identity.color_key;
    let content_resident_key = first_root.inner.boundary.group.active_resident_keys()[0];
    let content_color_key = first
        .transaction
        .ordered_stamps()
        .into_iter()
        .find(|stamp| stamp.identity.resident_key() == content_resident_key)
        .unwrap()
        .identity
        .color_key;
    let _ = emit_prepared_retained_transform_effect_scroll_scene(first);
    assert_clear_delta(&first_graph, 3);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(first_owner), true));

    // Unchanged state reuses all targets.
    let unchanged_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut unchanged_graph = FrameGraph::new();
    let mut unchanged = prepare_retained_transform_effect_scroll_scene_from_pool(
        &mut viewport,
        validated_transform_effect_scroll_fixture_scene(
            &arena,
            root,
            &properties,
            &generations,
            sampled_at,
        ),
        &mut unchanged_graph,
        ctx(),
        [0.0; 4],
        unchanged_owner,
    )
    .unwrap();
    unchanged.refresh_actions_from_committed_test_pool();
    assert_eq!(actions(&unchanged), [reuse, reuse, reuse]);
    let _ = emit_prepared_retained_transform_effect_scroll_scene(unchanged);
    assert_clear_delta(&unchanged_graph, 0);
    assert!(
        viewport.finish_retained_surface_transaction_for_frame(Some(unchanged_owner), true)
    );

    // Translation is final-composite geometry, not an outer raster dependency.
    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            19.0, 11.0, 0.0,
        ))));
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let translation_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut translation_graph = FrameGraph::new();
    let mut translation = prepare_retained_transform_effect_scroll_scene_from_pool(
        &mut viewport,
        validated_transform_effect_scroll_fixture_scene(
            &arena,
            root,
            &properties,
            &generations,
            sampled_at,
        ),
        &mut translation_graph,
        ctx(),
        [0.0; 4],
        translation_owner,
    )
    .unwrap();
    translation.refresh_actions_from_committed_test_pool();
    assert_eq!(actions(&translation), [reuse, reuse, reuse]);
    let expected_geometry = translation.roots[0]
        .outer_geometry
        .texture_composite_params();
    let _ = emit_prepared_retained_transform_effect_scroll_scene(translation);
    assert_clear_delta(&translation_graph, 0);
    let final_composite = translation_graph
        .test_graphics_passes::<TextureCompositePass>()
        .last()
        .expect("translation-only reuse still emits the final outer composite")
        .test_snapshot();
    assert_eq!(
        final_composite.bounds_bits,
        expected_geometry.bounds.map(f32::to_bits)
    );
    assert_eq!(
        final_composite.quad_position_bits,
        expected_geometry
            .quad_positions
            .map(|positions| positions.map(|point| point.map(f32::to_bits)))
    );
    assert!(
        viewport.finish_retained_surface_transaction_for_frame(Some(translation_owner), true)
    );

    // Effect opacity is consumed while rastering T, leaving E/content reusable.
    crate::view::test_support::get_element_mut::<Element>(&arena, effect).set_opacity(0.375);
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let opacity_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut opacity_graph = FrameGraph::new();
    let mut opacity = prepare_retained_transform_effect_scroll_scene_from_pool(
        &mut viewport,
        validated_transform_effect_scroll_fixture_scene(
            &arena,
            root,
            &properties,
            &generations,
            sampled_at,
        ),
        &mut opacity_graph,
        ctx(),
        [0.0; 4],
        opacity_owner,
    )
    .unwrap();
    opacity.refresh_actions_from_committed_test_pool();
    assert_eq!(actions(&opacity), [reraster, reuse, reuse]);
    let _ = emit_prepared_retained_transform_effect_scroll_scene(opacity);
    assert_clear_delta(&opacity_graph, 1);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(opacity_owner), true));

    // Scroll offset belongs to E's H/C/O raster; T transitively consumes E.
    crate::view::test_support::get_element_mut::<Element>(&arena, scroll)
        .set_scroll_offset((0.0, 37.0));
    {
        let mut content_node = arena.get_mut(content).unwrap();
        content_node.element.set_layout_offset(0.0, -37.0);
        let content_element = content_node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap();
        content_element.layout_state.layout_position.y = -37.0;
        content_element.layout_state.layout_inner_position.y = -37.0;
        content_element.layout_state.layout_flow_position.y = -37.0;
        content_element.layout_state.layout_flow_inner_position.y = -37.0;
        content_element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena
        .get_mut(scroll)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let scroll_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut scroll_graph = FrameGraph::new();
    let mut scrolled = prepare_retained_transform_effect_scroll_scene_from_pool(
        &mut viewport,
        validated_transform_effect_scroll_fixture_scene(
            &arena,
            root,
            &properties,
            &generations,
            sampled_at,
        ),
        &mut scroll_graph,
        ctx(),
        [0.0; 4],
        scroll_owner,
    )
    .unwrap();
    scrolled.refresh_actions_from_committed_test_pool();
    assert_eq!(actions(&scrolled), [reraster, reraster, reuse]);
    let _ = emit_prepared_retained_transform_effect_scroll_scene(scrolled);
    assert_clear_delta(&scroll_graph, 2);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(scroll_owner), true));

    // Content revision invalidates content, then E and T through typed dependencies.
    crate::view::test_support::get_element_mut::<Element>(&arena, content)
        .set_background_color_value(Color::rgb(72, 48, 24));
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let content_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut content_graph = FrameGraph::new();
    let mut changed_content = prepare_retained_transform_effect_scroll_scene_from_pool(
        &mut viewport,
        validated_transform_effect_scroll_fixture_scene(
            &arena,
            root,
            &properties,
            &generations,
            sampled_at,
        ),
        &mut content_graph,
        ctx(),
        [0.0; 4],
        content_owner,
    )
    .unwrap();
    changed_content.refresh_actions_from_committed_test_pool();
    assert_eq!(actions(&changed_content), [reraster, reraster, reraster]);
    let _ = emit_prepared_retained_transform_effect_scroll_scene(changed_content);
    assert_clear_delta(&content_graph, 3);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(content_owner), true));

    // A pair miss is local, except content miss must first rebuild E's H/C/O target.
    for (name, missing_color, expected) in [
        ("T pair", outer_color_key, [reraster, reuse, reuse]),
        ("E pair", inner_color_key, [reuse, reraster, reuse]),
        (
            "content pair",
            content_color_key,
            [reuse, reraster, reraster],
        ),
    ] {
        viewport.forget_retained_surface_pair_witness_for_test(missing_color);
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let mut prepared = prepare_retained_transform_effect_scroll_scene_from_pool(
            &mut viewport,
            validated_transform_effect_scroll_fixture_scene(
                &arena,
                root,
                &properties,
                &generations,
                sampled_at,
            ),
            &mut graph,
            ctx(),
            [0.0; 4],
            owner,
        )
        .unwrap();
        prepared.refresh_actions_from_committed_test_pool();
        assert_eq!(actions(&prepared), expected, "{name}");
        let reraster_count = expected
            .iter()
            .filter(|action| **action == reraster)
            .count();
        let _ = emit_prepared_retained_transform_effect_scroll_scene(prepared);
        assert_clear_delta(&graph, reraster_count);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    }
}
