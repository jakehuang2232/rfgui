use super::*;

#[test]
fn effect_transform_scroll_joint_prepare_is_atomic_for_every_preflight_failure() {
    let (arena, root, properties, generations) = effect_transform_scroll_neutral_fixture();
    let sampled_at = crate::time::Instant::now();
    let target_format = wgpu::TextureFormat::Bgra8UnormSrgb;
    let make_scene = |budget| {
        PropertyBoundaryDagCompiler::plan_and_validate(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            sampled_at,
            target_format,
            budget,
        )
    };

    let budget_viewport = Viewport::new();
    let budget_graph = FrameGraph::new();
    let budget_pool = budget_viewport.retained_surface_transaction_shape_for_test();
    let budget_topology = budget_graph.build_state_snapshot_for_test();
    assert!(make_scene(ScrollSceneSingleTextureBudget::new(1, 1).unwrap()).is_err());
    assert_eq!(
        budget_graph.build_state_snapshot_for_test(),
        budget_topology
    );
    assert_eq!(
        budget_viewport.retained_surface_transaction_shape_for_test(),
        budget_pool
    );
    assert!(budget_viewport.retained_property_scroll_scene_stage_is_available());

    let context_scene = make_scene(generous_budget()).unwrap();
    let mut context_viewport = Viewport::new();
    let context_owner = context_viewport
        .begin_retained_surface_frame_stage()
        .unwrap();
    let mut context_graph = FrameGraph::new();
    let context_topology = context_graph.build_state_snapshot_for_test();
    let context_pool = context_viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_property_boundary_dag_scene_from_pool(
            &mut context_viewport,
            context_scene,
            &mut context_graph,
            UiBuildContext::new(800, 600, wgpu::TextureFormat::Rgba8Unorm, 1.0),
            [0.0; 4],
            context_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::ContextMismatch)
    );
    assert_eq!(
        context_graph.build_state_snapshot_for_test(),
        context_topology
    );
    assert_eq!(
        context_viewport.retained_surface_transaction_shape_for_test(),
        context_pool
    );
    assert!(context_viewport.retained_property_scroll_scene_stage_is_available());
    assert!(
        context_viewport
            .finish_retained_surface_transaction_for_frame(Some(context_owner), false)
    );

    let stale_scene = make_scene(generous_budget()).unwrap();
    let mut stale_viewport = Viewport::new();
    let stale_owner = stale_viewport.begin_retained_surface_frame_stage().unwrap();
    assert!(
        stale_viewport.finish_retained_surface_transaction_for_frame(Some(stale_owner), false)
    );
    let mut stale_graph = FrameGraph::new();
    let stale_topology = stale_graph.build_state_snapshot_for_test();
    let stale_pool = stale_viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_property_boundary_dag_scene_from_pool(
            &mut stale_viewport,
            stale_scene,
            &mut stale_graph,
            UiBuildContext::new(800, 600, target_format, 1.0),
            [0.0; 4],
            stale_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::StageUnavailable)
    );
    assert_eq!(stale_graph.build_state_snapshot_for_test(), stale_topology);
    assert_eq!(
        stale_viewport.retained_surface_transaction_shape_for_test(),
        stale_pool
    );
    assert!(stale_viewport.retained_property_scroll_scene_stage_is_available());

    let collision_scene = make_scene(generous_budget()).unwrap();
    let ValidatedPropertyBoundaryDagScene::EffectTransformScroll(scene) = &collision_scene
    else {
        panic!("fixture remains exact E->T->S")
    };
    let outer_key = crate::view::base_component::isolation_layer_stable_key(
        scene.roots[0].insertion.outer_stable_id,
    );
    let outer_values = scene.roots[0]
        .insertion
        .outer_raster_bounds_bits
        .map(f32::from_bits);
    let outer_desc = texture_desc_for_logical_bounds(
        RetainedSurfaceBounds {
            x: outer_values[0],
            y: outer_values[1],
            width: outer_values[2],
            height: outer_values[3],
            corner_radii: [0.0; 4],
        },
        1.0,
        None,
        target_format,
    );
    let (outer_color, _) = persistent_target_texture_descriptors(outer_desc, outer_key);
    let mut collision_viewport = Viewport::new();
    let collision_owner = collision_viewport
        .begin_retained_surface_frame_stage()
        .unwrap();
    let mut collision_graph = FrameGraph::new();
    let mut declaring_ctx = UiBuildContext::new(800, 600, target_format, 1.0);
    let _ = declaring_ctx.allocate_persistent_target_with_desc(
        &mut collision_graph,
        outer_color,
        outer_key,
    );
    let collision_topology = collision_graph.build_state_snapshot_for_test();
    let collision_pool = collision_viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_property_boundary_dag_scene_from_pool(
            &mut collision_viewport,
            collision_scene,
            &mut collision_graph,
            UiBuildContext::new(800, 600, target_format, 1.0),
            [0.0; 4],
            collision_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(outer_key))
    );
    assert_eq!(
        collision_graph.build_state_snapshot_for_test(),
        collision_topology
    );
    assert_eq!(
        collision_viewport.retained_surface_transaction_shape_for_test(),
        collision_pool
    );
    assert!(collision_viewport.retained_property_scroll_scene_stage_is_available());
    assert!(
        collision_viewport
            .finish_retained_surface_transaction_for_frame(Some(collision_owner), false)
    );

    let exact_scene = make_scene(generous_budget()).unwrap();
    let mut exact_viewport = Viewport::new();
    let exact_owner = exact_viewport.begin_retained_surface_frame_stage().unwrap();
    let mut exact_graph = FrameGraph::new();
    let exact_topology = exact_graph.build_state_snapshot_for_test();
    let exact_pool = exact_viewport.retained_surface_transaction_shape_for_test();
    {
        let prepared = prepare_property_boundary_dag_scene_from_pool(
            &mut exact_viewport,
            exact_scene,
            &mut exact_graph,
            UiBuildContext::new(800, 600, target_format, 1.0),
            [0.0; 4],
            exact_owner,
        )
        .unwrap();
        assert!(prepared.action_set_is_exact_for_test());
        assert!(prepared.rejects_action_set_mismatch_for_test());
        assert_eq!(
            prepared.graph_build_state_snapshot_for_test(),
            exact_topology,
            "full-set freeze and mismatch rejection stay graph-inert"
        );
    }
    assert_eq!(exact_graph.build_state_snapshot_for_test(), exact_topology);
    assert_eq!(
        exact_viewport.retained_surface_transaction_shape_for_test(),
        exact_pool
    );
    assert!(exact_viewport.retained_property_scroll_scene_stage_is_available());
    assert!(
        exact_viewport.finish_retained_surface_transaction_for_frame(Some(exact_owner), false)
    );
}

#[test]
fn transform_effect_scroll_validated_scene_seals_joint_graph_inert_authority() {
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
    .expect("exact T -> E -> Scroll planner authority");
    assert!(scene.is_canonical());
    let [validated] = scene.roots.as_slice() else {
        panic!("one joint root")
    };
    assert_eq!(validated.outer_receiver.owner, root);
    assert_eq!(
        validated.insertion.inner.receiver.opacity.to_bits(),
        0.625_f32.to_bits()
    );
    assert_eq!(
        validated.composite.source_bounds_bits,
        [0.0_f32, 0.0, 120.0, 90.0].map(f32::to_bits)
    );
    assert!(validated.boundary.is_canonical());
    assert!(
        validated
            .insertion
            .validates_outer_recorded_steps(&validated.outer_steps)
    );
    assert!(
        validated
            .insertion
            .inner
            .validates_recorded_steps(&validated.inner_steps)
    );

    let mut effect_cutout_drift = scene.clone();
    effect_cutout_drift.roots[0]
        .insertion
        .effect_cutout
        .stable_id += 1;
    for step in &mut effect_cutout_drift.roots[0].outer_steps {
        if let crate::view::paint::frame_recorder::RecordedTransformSurfaceStep::Boundary(
            marker,
        ) = step
        {
            marker.stable_id += 1;
        }
    }
    assert!(!effect_cutout_drift.is_canonical());

    let mut scroll_cutout_drift = scene;
    scroll_cutout_drift.roots[0]
        .insertion
        .inner
        .scroll_cutout
        .stable_id += 1;
    for step in &mut scroll_cutout_drift.roots[0].inner_steps {
        if let crate::view::paint::frame_recorder::RecordedTransformSurfaceStep::Boundary(
            marker,
        ) = step
        {
            marker.stable_id += 1;
        }
    }
    assert!(!scroll_cutout_drift.is_canonical());
}

#[test]
fn transform_effect_scroll_planner_rejects_adjacent_topologies_and_context_drift() {
    fn assert_rejected(name: &str, mutate: impl FnOnce(&mut NodeArena, NodeKey)) {
        let (mut arena, root, _, _) = transform_effect_scroll_fixture();
        mutate(&mut arena, root);
        arena.refresh_subtree_dirty_cache(root);
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        assert!(
            plan_and_validate_transform_effect_scroll_scene(
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
            .is_err(),
            "{name} must not enter the exact T -> E -> Scroll authority"
        );
    }

    for (name, matrix) in [
        (
            "scale",
            glam::Mat4::from_scale(glam::Vec3::new(1.1, 1.0, 1.0)),
        ),
        ("rotation", glam::Mat4::from_rotation_z(0.2)),
        (
            "perspective",
            glam::Mat4::from_cols_array(&[
                1.0, 0.0, 0.0, 0.01, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ]),
        ),
    ] {
        assert_rejected(name, |arena, root| {
            crate::view::test_support::get_element_mut::<Element>(arena, root)
                .set_resolved_transform_for_test(Some(matrix));
        });
    }

    assert_rejected("wrong T/E parent chain", |arena, root| {
        let effect = arena.children_of(root)[0];
        let bridge = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xb4_20a0, 0.0, 0.0, 120.0, 90.0,
        ))));
        arena.set_children(root, vec![bridge]);
        arena.set_parent(bridge, Some(root));
        arena.set_children(bridge, vec![effect]);
        arena.set_parent(effect, Some(bridge));
    });
    assert_rejected("outer transform is not parentless", |arena, root| {
        let wrapper = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xb4_20a5, 0.0, 0.0, 120.0, 90.0,
        ))));
        arena.set_parent(root, Some(wrapper));
        arena.set_children(wrapper, vec![root]);
    });
    assert_rejected("outer multi-child sibling", |arena, root| {
        let sibling = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xb4_20a1, 0.0, 0.0, 8.0, 8.0,
        ))));
        arena.set_parent(sibling, Some(root));
        arena.push_child(root, sibling);
    });
    assert_rejected("effect multi-child sibling", |arena, root| {
        let effect = arena.children_of(root)[0];
        let sibling = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xb4_20a2, 0.0, 0.0, 8.0, 8.0,
        ))));
        arena.set_parent(sibling, Some(effect));
        arena.push_child(effect, sibling);
    });
    assert_rejected("wrong E/Scroll parent chain", |arena, root| {
        let effect = arena.children_of(root)[0];
        let scroll = arena.children_of(effect)[0];
        let bridge = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xb4_20a3, 0.0, 0.0, 120.0, 90.0,
        ))));
        arena.set_children(effect, vec![bridge]);
        arena.set_parent(bridge, Some(effect));
        arena.set_children(bridge, vec![scroll]);
        arena.set_parent(scroll, Some(bridge));
    });
    assert_rejected("scroll multi-content", |arena, root| {
        let effect = arena.children_of(root)[0];
        let scroll = arena.children_of(effect)[0];
        let sibling = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xb4_20a4, 0.0, 0.0, 8.0, 8.0,
        ))));
        arena.set_parent(sibling, Some(scroll));
        arena.push_child(scroll, sibling);
    });
    assert_rejected("co-located transform and effect", |arena, root| {
        crate::view::test_support::get_element_mut::<Element>(arena, root).set_opacity(0.5);
    });
    assert_rejected("nested scroll", |arena, root| {
        let effect = arena.children_of(root)[0];
        let scroll = arena.children_of(effect)[0];
        let content = arena.children_of(scroll)[0];
        let mut style = Style::new();
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        let mut content = crate::view::test_support::get_element_mut::<Element>(arena, content);
        content.apply_style(style);
        content.layout_state.content_size = Size {
            width: 120.0,
            height: 480.0,
        };
        content.set_scroll_offset((0.0, 5.0));
    });
    assert_rejected("scroll owns transform", |arena, root| {
        let effect = arena.children_of(root)[0];
        let scroll = arena.children_of(effect)[0];
        crate::view::test_support::get_element_mut::<Element>(arena, scroll)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                glam::Vec3::new(3.0, 0.0, 0.0),
            )));
    });
    assert_rejected("scroll owns effect", |arena, root| {
        let effect = arena.children_of(root)[0];
        let scroll = arena.children_of(effect)[0];
        crate::view::test_support::get_element_mut::<Element>(arena, scroll).set_opacity(0.5);
    });
    assert_rejected("scroll content descendant owns transform", |arena, root| {
        let effect = arena.children_of(root)[0];
        let scroll = arena.children_of(effect)[0];
        let content = arena.children_of(scroll)[0];
        let descendant = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xb4_20a6, 0.0, 0.0, 16.0, 16.0,
        ))));
        arena.set_parent(descendant, Some(content));
        arena.set_children(content, vec![descendant]);
        crate::view::test_support::get_element_mut::<Element>(arena, descendant)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                glam::Vec3::new(2.0, 0.0, 0.0),
            )));
    });
    assert_rejected("scroll content descendant owns effect", |arena, root| {
        let effect = arena.children_of(root)[0];
        let scroll = arena.children_of(effect)[0];
        let content = arena.children_of(scroll)[0];
        let descendant = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xb4_20a7, 0.0, 0.0, 16.0, 16.0,
        ))));
        arena.set_parent(descendant, Some(content));
        arena.set_children(content, vec![descendant]);
        crate::view::test_support::get_element_mut::<Element>(arena, descendant)
            .set_opacity(0.5);
    });
    assert_rejected("active layout transition", |arena, root| {
        let effect = arena.children_of(root)[0];
        crate::view::test_support::get_element_mut::<Element>(arena, effect)
            .set_layout_transition_width(119.0);
    });
    assert_rejected("deferred viewport clip", |arena, root| {
        let effect = arena.children_of(root)[0];
        let scroll = arena.children_of(effect)[0];
        let content = arena.children_of(scroll)[0];
        let mut style = Style::new();
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                crate::style::Position::absolute()
                    .left(crate::style::Length::px(1.0))
                    .clip(crate::style::ClipMode::Viewport),
            ),
        );
        crate::view::test_support::get_element_mut::<Element>(arena, content)
            .apply_style(style);
    });

    let (arena, root, properties, generations) = transform_effect_scroll_fixture();
    for (name, offset, scissor) in [
        ("paint offset", [0.25, 0.0], None),
        ("outer scissor", [0.0; 2], Some([0, 0, 120, 90])),
    ] {
        assert!(
            plan_and_validate_transform_effect_scroll_scene(
                &arena,
                &[root],
                &properties,
                &generations,
                1.0,
                offset,
                scissor,
                crate::time::Instant::now(),
                wgpu::TextureFormat::Bgra8UnormSrgb,
                generous_budget(),
            )
            .is_err(),
            "{name} must fail closed"
        );
    }
}
