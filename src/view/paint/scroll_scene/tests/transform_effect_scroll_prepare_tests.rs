use super::*;

#[test]
fn transform_effect_scroll_prepare_seals_three_targets_before_graph_mutation() {
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
    .expect("joint T/E/content preparation");
    assert!(prepared.transaction.is_canonical());
    assert_eq!(prepared.roots.len(), 1);
    assert_eq!(prepared.transaction.generic_stamps().len(), 2);
    assert_eq!(prepared.transaction.scroll_groups().len(), 1);
    assert_eq!(prepared.actions.len(), 3);
    assert_eq!(prepared.trace.generic_surface_count, 2);
    assert!(matches!(
        &prepared.transaction.generic_authority,
        RetainedPropertyScrollGenericAuthority::TransformEffectScrollCompiler(contracts)
            if contracts.len() == 1
    ));
    let [outer, inner] = prepared.transaction.generic_stamps() else {
        panic!("ordered outer T and inner E stamps")
    };
    let dependencies = outer
        .ordered_steps
        .iter()
        .filter_map(|step| match step {
            crate::view::paint::RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(
                dependency,
            ) => Some(dependency),
            crate::view::paint::RetainedSurfaceRasterStepStamp::ArtifactSpan(_) => None,
            _ => panic!("outer stamp admits only artifacts and one typed E dependency"),
        })
        .collect::<Vec<_>>();
    let [dependency] = dependencies.as_slice() else {
        panic!("outer owns exactly one typed E dependency")
    };
    assert_eq!(dependency.child_stamp.as_ref(), inner);
    assert_eq!(
        dependency.parent_opaque_order_before,
        dependency.parent_opaque_order_after
    );
    assert_eq!(prepared.graph.declared_persistent_texture_keys().count(), 0);
    assert!(prepared.graph.pass_descriptors().is_empty());
}

#[test]
fn transform_effect_scroll_transaction_cross_binds_each_outer_to_its_inner_boundary() {
    let (mut arena, first_root, _, _) = transform_effect_scroll_fixture();
    let second_root = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_2101, 0.0, 0.0, 120.0, 90.0,
    ))));
    let second_scroll = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_2102, 0.0, 0.0, 120.0, 90.0,
    ))));
    let second_content = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_2110, 0.0, -20.0, 120.0, 240.0,
    ))));
    let second_effect = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_2103, 0.0, 0.0, 120.0, 90.0,
    ))));
    arena.set_parent(second_effect, Some(second_root));
    arena.set_children(second_root, vec![second_effect]);
    arena.set_parent(second_scroll, Some(second_effect));
    arena.set_children(second_effect, vec![second_scroll]);
    arena.set_parent(second_content, Some(second_scroll));
    arena.set_children(second_scroll, vec![second_content]);
    let mut grid = Style::new();
    grid.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut outer =
            crate::view::test_support::get_element_mut::<Element>(&arena, second_root);
        outer.apply_style(grid.clone());
        outer.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
            glam::Vec3::new(17.0, 13.0, 0.0),
        )));
    }
    {
        let mut effect =
            crate::view::test_support::get_element_mut::<Element>(&arena, second_effect);
        effect.apply_style(grid.clone());
        effect.set_opacity(0.75);
        effect.set_background_color_value(Color::rgb(48, 72, 96));
    }
    let mut scroll_style = grid.clone();
    scroll_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    {
        let mut scroll =
            crate::view::test_support::get_element_mut::<Element>(&arena, second_scroll);
        scroll.apply_style(scroll_style);
        scroll.layout_state.content_size = Size {
            width: 120.0,
            height: 240.0,
        };
        scroll.set_scroll_offset((0.0, 20.0));
        scroll.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    {
        let mut content =
            crate::view::test_support::get_element_mut::<Element>(&arena, second_content);
        content.apply_style(grid);
        content.set_background_color_value(Color::rgb(24, 48, 72));
        content.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    let roots = [first_root, second_root];
    for &root in &roots {
        arena.refresh_subtree_dirty_cache(root);
    }
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    let scene = plan_and_validate_transform_effect_scroll_scene(
        &arena,
        &roots,
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        crate::time::Instant::now(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("two exact T -> E -> Scroll roots");
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_retained_transform_effect_scroll_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0; 4],
        owner,
    )
    .unwrap();
    assert!(prepared.transaction.is_canonical());
    let mut swapped = prepared.transaction.clone();
    let first_outer_boundary = swapped.seal.generic_bindings[0].boundary;
    let second_outer_boundary = swapped.seal.generic_bindings[2].boundary;
    swapped.seal.generic_bindings[0].boundary = second_outer_boundary;
    swapped.seal.generic_bindings[2].boundary = first_outer_boundary;
    assert!(
        !swapped.is_canonical(),
        "synchronously swapping only the two outer bindings must not preserve authority"
    );
}

#[test]
fn transform_effect_scroll_prepare_failures_are_graph_pool_and_stage_inert() {
    let (arena, root, properties, generations) = transform_effect_scroll_fixture();
    let make_scene = |budget| {
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
            budget,
        )
        .unwrap()
    };
    let baseline = make_scene(generous_budget());
    let content_bytes = match &baseline.roots[0].boundary.planner.steps[1] {
        ScrollBoundaryStep::ContentComposite { backing, .. } => {
            property_scroll_backing_pair_bytes(backing)
        }
        _ => unreachable!(),
    };
    let inner_values = baseline.roots[0]
        .composite
        .source_bounds_bits
        .map(f32::from_bits);
    let inner_key = crate::view::base_component::isolation_layer_stable_key(
        baseline.roots[0].insertion.inner.receiver_stable_id,
    );
    let inner_color = texture_desc_for_logical_bounds(
        RetainedSurfaceBounds {
            x: inner_values[0],
            y: inner_values[1],
            width: inner_values[2],
            height: inner_values[3],
            corner_radii: [0.0; 4],
        },
        1.0,
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    );
    let (inner_color, inner_depth) =
        persistent_target_texture_descriptors(inner_color, inner_key);
    let outer_key = crate::view::base_component::transformed_layer_stable_key(
        baseline.roots[0].outer_stable_id,
    );
    let outer_color = texture_desc_for_logical_bounds(
        baseline.roots[0].outer_geometry.source_bounds,
        1.0,
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    );
    let (outer_color, outer_depth) =
        persistent_target_texture_descriptors(outer_color, outer_key);
    let aggregate = content_bytes
        .checked_add(canonical_pair_bytes(&inner_color, &inner_depth).unwrap())
        .and_then(|bytes| {
            bytes.checked_add(canonical_pair_bytes(&outer_color, &outer_depth).unwrap())
        })
        .unwrap();

    let (dimension_arena, dimension_root, _, _) = transform_effect_scroll_fixture();
    {
        let mut outer = crate::view::test_support::get_element_mut::<Element>(
            &dimension_arena,
            dimension_root,
        );
        outer.set_background_color_value(Color::rgb(12, 24, 36));
        outer.layout_state.layout_size = Size {
            width: 300.0,
            height: 90.0,
        };
        outer.layout_state.layout_inner_size = outer.layout_state.layout_size;
    }
    dimension_arena.refresh_subtree_dirty_cache(dimension_root);
    let mut dimension_properties = PropertyTrees::default();
    dimension_properties.sync(&dimension_arena, &[dimension_root]);
    let mut dimension_generations = PaintGenerationTracker::default();
    dimension_generations.sync(&dimension_arena, &[dimension_root], &dimension_properties);
    let dimension_scene = plan_and_validate_transform_effect_scroll_scene(
        &dimension_arena,
        &[dimension_root],
        &dimension_properties,
        &dimension_generations,
        1.0,
        [0.0; 2],
        None,
        crate::time::Instant::now(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        ScrollSceneSingleTextureBudget::new(250, generous_budget().max_pair_bytes).unwrap(),
    )
    .expect("scroll backing fits while the outer T target exceeds max_dimension_2d");
    let [dimension_validated_root] = dimension_scene.roots.as_slice() else {
        unreachable!()
    };
    assert!(dimension_validated_root.outer_geometry.source_bounds.width > 250.0);
    let ScrollBoundaryStep::ContentComposite { backing, .. } =
        &dimension_validated_root.boundary.planner.steps[1]
    else {
        unreachable!()
    };
    let backing_fits = match backing {
        PropertyScrollBackingPlan::Single(single) => {
            single.color_desc.width() <= 250 && single.color_desc.height() <= 250
        }
        PropertyScrollBackingPlan::Tiled(tiled) => tiled
            .tiles
            .iter()
            .all(|tile| tile.color_desc.width() <= 250 && tile.color_desc.height() <= 250),
    };
    assert!(backing_fits);
    let mut dimension_viewport = Viewport::new();
    let dimension_owner = dimension_viewport
        .begin_retained_surface_frame_stage()
        .unwrap();
    let mut dimension_graph = FrameGraph::new();
    let dimension_graph_before = dimension_graph.build_state_snapshot_for_test();
    let dimension_pool_before =
        dimension_viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_retained_transform_effect_scroll_scene_from_pool(
            &mut dimension_viewport,
            dimension_scene,
            &mut dimension_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            dimension_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::BoundaryDrift)
    );
    assert_eq!(
        dimension_graph.build_state_snapshot_for_test(),
        dimension_graph_before
    );
    assert_eq!(
        dimension_viewport.retained_surface_transaction_shape_for_test(),
        dimension_pool_before
    );
    assert!(dimension_viewport.retained_property_scroll_scene_stage_is_available());
    assert!(
        dimension_viewport
            .finish_retained_surface_transaction_for_frame(Some(dimension_owner), false)
    );

    let budget_scene = make_scene(
        ScrollSceneSingleTextureBudget::new(8192, aggregate.checked_sub(1).unwrap()).unwrap(),
    );
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_retained_transform_effect_scroll_scene_from_pool(
            &mut viewport,
            budget_scene,
            &mut graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::BoundaryDrift)
    );
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert!(viewport.retained_property_scroll_scene_stage_is_available());
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));

    let collision_scene = make_scene(generous_budget());
    let collision_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut collision_graph = FrameGraph::new();
    let mut declaring_ctx =
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let _ = declaring_ctx.allocate_persistent_target_with_desc(
        &mut collision_graph,
        outer_color.clone(),
        outer_key,
    );
    let collision_before = collision_graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_retained_transform_effect_scroll_scene_from_pool(
            &mut viewport,
            collision_scene,
            &mut collision_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            collision_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(outer_key))
    );
    assert_eq!(
        collision_graph.build_state_snapshot_for_test(),
        collision_before
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert!(
        viewport.finish_retained_surface_transaction_for_frame(Some(collision_owner), false)
    );

    let context_scene = make_scene(generous_budget());
    let context_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut context_graph = FrameGraph::new();
    let context_before = context_graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_retained_transform_effect_scroll_scene_from_pool(
            &mut viewport,
            context_scene,
            &mut context_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Rgba8Unorm, 1.0),
            [0.0; 4],
            context_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::ContextMismatch)
    );
    assert_eq!(
        context_graph.build_state_snapshot_for_test(),
        context_before
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(context_owner), false));

    let stage_scene = make_scene(generous_budget());
    let stale_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(stale_owner), false));
    let mut stage_graph = FrameGraph::new();
    let stage_before = stage_graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_retained_transform_effect_scroll_scene_from_pool(
            &mut viewport,
            stage_scene,
            &mut stage_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            stale_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::StageUnavailable)
    );
    assert_eq!(stage_graph.build_state_snapshot_for_test(), stage_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert!(viewport.retained_property_scroll_scene_stage_is_available());
}
