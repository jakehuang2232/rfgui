use super::*;

fn validated_scene(
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
    scale_factor: f32,
) -> ValidatedTransformEffectScrollScene {
    plan_and_validate_transform_effect_scroll_scene(
        arena,
        &[root],
        properties,
        generations,
        scale_factor,
        [0.0; 2],
        None,
        crate::time::Instant::now(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("fully same-owner T+E+S scene")
}

fn assert_cold_dependency_order(graph: &mut FrameGraph) {
    let snapshot = graph.test_compile_snapshot().expect("cold graph compiles");
    let payloads = snapshot.pass_payloads();
    let (effect_index, effect) = payloads
        .iter()
        .enumerate()
        .find_map(|(index, payload)| match payload {
            FramePassTestPayload::CompositeLayer(effect) => Some((index, effect)),
            _ => None,
        })
        .expect("final E composite");
    let inner_target = effect.layer_handle.expect("E samples inner target");
    let outer_target = effect.output_target.expect("E writes outer target");
    let (transform_index, transform) = payloads
        .iter()
        .enumerate()
        .find_map(|(index, payload)| match payload {
            FramePassTestPayload::TextureComposite(transform)
                if transform.source_handle == Some(outer_target) =>
            {
                Some((index, transform))
            }
            _ => None,
        })
        .expect("final T composite");
    let (content_composite_index, content_composite) = payloads
        .iter()
        .enumerate()
        .find_map(|(index, payload)| match payload {
            FramePassTestPayload::TextureComposite(content)
                if content.output_target == Some(inner_target)
                    && content.source_handle != Some(outer_target) =>
            {
                Some((index, content))
            }
            _ => None,
        })
        .expect("C composite into H/O target");
    let content_target = content_composite
        .source_handle
        .expect("C composite samples content target");
    let first_content_pass = payloads
        .iter()
        .position(|payload| match payload {
            FramePassTestPayload::Clear(pass) => pass.output_target == Some(content_target),
            FramePassTestPayload::DrawRect(pass) => pass.output_target == Some(content_target),
            _ => false,
        })
        .expect("content raster pass");
    let inner_passes = payloads
        .iter()
        .enumerate()
        .filter_map(|(index, payload)| match payload {
            FramePassTestPayload::Clear(pass) if pass.output_target == Some(inner_target) => {
                Some(index)
            }
            FramePassTestPayload::DrawRect(pass) if pass.output_target == Some(inner_target) => {
                Some(index)
            }
            FramePassTestPayload::TextureComposite(pass)
                if pass.output_target == Some(inner_target) =>
            {
                Some(index)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!inner_passes.is_empty());
    assert!(first_content_pass < inner_passes[0], "C must precede H/O");
    assert!(
        content_composite_index < effect_index,
        "H/C/O must precede E"
    );
    assert!(inner_passes.iter().all(|index| *index < effect_index));
    assert!(effect_index < transform_index, "E must precede final T");
    assert_eq!(transform.opacity_bits, 1.0_f32.to_bits());
}

#[test]
fn fully_same_owner_transform_effect_scroll_cold_warm_dpr1_dpr2() {
    for scale_factor in [1.0, 2.0] {
        let (arena, root, properties, generations) =
            fully_same_owner_transform_effect_scroll_fixture();
        let mut viewport = Viewport::new();

        let cold_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut cold_graph = FrameGraph::new();
        let cold = prepare_retained_transform_effect_scroll_scene_from_pool(
            &mut viewport,
            validated_scene(&arena, root, &properties, &generations, scale_factor),
            &mut cold_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, scale_factor),
            [0.0; 4],
            cold_owner,
        )
        .expect("cold triple prepare");
        assert_eq!(cold.trace.reraster_count, 3);
        let [prepared_root] = cold.roots.as_slice() else {
            panic!("one triple root")
        };
        let resident_keys = [
            prepared_root.outer_stamp.identity.resident_key(),
            prepared_root.inner.receiver_stamp.identity.resident_key(),
            prepared_root.inner.boundary.group.active_resident_keys()[0],
        ];
        assert_ne!(resident_keys[0], resident_keys[1]);
        assert_ne!(resident_keys[0], resident_keys[2]);
        assert_ne!(resident_keys[1], resident_keys[2]);
        let outcome = emit_prepared_retained_transform_effect_scroll_scene(cold);
        assert_eq!(outcome.trace.reraster_count, 3);
        assert_cold_dependency_order(&mut cold_graph);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

        let warm_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut warm_graph = FrameGraph::new();
        let mut warm = prepare_retained_transform_effect_scroll_scene_from_pool(
            &mut viewport,
            validated_scene(&arena, root, &properties, &generations, scale_factor),
            &mut warm_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, scale_factor),
            [0.0; 4],
            warm_owner,
        )
        .expect("warm triple prepare");
        warm.refresh_actions_from_committed_test_pool();
        assert_eq!(warm.trace.reraster_count, 0);
        assert_eq!(warm.trace.reuse_count, 3);
        let outcome = emit_prepared_retained_transform_effect_scroll_scene(warm);
        assert_eq!(outcome.trace.reuse_count, 3);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true));
    }
}

#[test]
fn fully_same_owner_transform_effect_scroll_invalidation_matrix_is_typed() {
    fn actions(
        prepared: &PreparedRetainedTransformEffectScrollScene<'_>,
    ) -> [RetainedSurfaceCompileAction; 3] {
        let [root] = prepared.roots.as_slice() else {
            panic!("one triple root")
        };
        let content = root.inner.boundary.group.active_resident_keys()[0];
        [
            prepared.actions[&root.outer_stamp.identity.resident_key()],
            prepared.actions[&root.inner.receiver_stamp.identity.resident_key()],
            prepared.actions[&content],
        ]
    }

    fn run_frame(
        viewport: &mut Viewport,
        arena: &NodeArena,
        root: NodeKey,
        properties: &PropertyTrees,
        generations: &PaintGenerationTracker,
        label: &str,
        expected: [RetainedSurfaceCompileAction; 3],
    ) {
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let mut prepared = prepare_retained_transform_effect_scroll_scene_from_pool(
            viewport,
            validated_scene(arena, root, properties, generations, 1.0),
            &mut graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            owner,
        )
        .expect("typed triple prepare");
        prepared.refresh_actions_from_committed_test_pool();
        assert_eq!(actions(&prepared), expected, "{label}");
        let _ = emit_prepared_retained_transform_effect_scroll_scene(prepared);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    }

    let reuse = RetainedSurfaceCompileAction::Reuse;
    let reraster = RetainedSurfaceCompileAction::Reraster;
    let (arena, root, mut properties, mut generations) =
        fully_same_owner_transform_effect_scroll_fixture();
    let children = arena.children_of(root);
    let [content] = children.as_slice() else {
        panic!("triple host owns one content leaf")
    };
    let content = *content;
    let mut viewport = Viewport::new();

    run_frame(
        &mut viewport,
        &arena,
        root,
        &properties,
        &generations,
        "cold",
        [reraster, reraster, reraster],
    );
    run_frame(
        &mut viewport,
        &arena,
        root,
        &properties,
        &generations,
        "unchanged",
        [reuse, reuse, reuse],
    );

    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            19.0, 11.0, 0.0,
        ))));
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    run_frame(
        &mut viewport,
        &arena,
        root,
        &properties,
        &generations,
        "translation",
        [reuse, reuse, reuse],
    );

    crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.375);
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    run_frame(
        &mut viewport,
        &arena,
        root,
        &properties,
        &generations,
        "opacity",
        [reraster, reuse, reuse],
    );

    crate::view::test_support::get_element_mut::<Element>(&arena, root)
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
        .get_mut(root)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    run_frame(
        &mut viewport,
        &arena,
        root,
        &properties,
        &generations,
        "scroll offset",
        [reraster, reraster, reuse],
    );

    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_background_color_value(Color::rgb(80, 48, 24));
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    run_frame(
        &mut viewport,
        &arena,
        root,
        &properties,
        &generations,
        "host paint",
        [reraster, reraster, reuse],
    );

    crate::view::test_support::get_element_mut::<Element>(&arena, content)
        .set_background_color_value(Color::rgb(24, 80, 48));
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    run_frame(
        &mut viewport,
        &arena,
        root,
        &properties,
        &generations,
        "content paint",
        [reraster, reraster, reraster],
    );
}

#[test]
fn fully_same_owner_transform_effect_scroll_role_tamper_is_prepare_atomic() {
    let (arena, root, properties, generations) = fully_same_owner_transform_effect_scroll_fixture();
    let mut viewport = Viewport::new();

    for tamper_outer in [true, false] {
        let mut scene = validated_scene(&arena, root, &properties, &generations, 1.0);
        let same_owner = scene.roots[0]
            .same_owner_insertion
            .as_mut()
            .expect("triple role authority");
        if tamper_outer {
            same_owner.transform.id = TransformNodeId(same_owner.content_root);
        } else {
            same_owner.effect_scroll.content_stable_id ^= 1;
        }
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        let pool_before = viewport.retained_surface_transaction_shape_for_test();
        let error = prepare_retained_transform_effect_scroll_scene_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            owner,
        )
        .err();
        assert_eq!(
            error,
            Some(RetainedPropertyScrollScenePrepareError::BoundaryDrift)
        );
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            pool_before
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));
    }
}
