use super::*;

fn compile_segment_scene(
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
    scale_factor: f32,
) -> ValidatedNestedScrollSegmentScene {
    let scene = plan_and_validate_nested_scroll_segment_scene(
        arena,
        &[root],
        properties,
        generations,
        scale_factor,
        [0.0; 2],
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("M4 linear nested-scroll segment compiler");
    assert!(scene.is_canonical());
    scene
}

fn three_deep_segment_fixture() -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let (mut arena, root, inner, third, _properties, _generations) =
        crate::view::paint::frame_plan::tests::nested_scroll_plan_fixture();
    let leaf = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x1251_03, 10.0, 20.0, 100.0, 900.0,
    ))));
    arena.set_parent(leaf, Some(third));
    arena.push_child(third, leaf);
    let mut scroll_style = Style::new();
    scroll_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    scroll_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, third);
        element.apply_style(scroll_style);
        element.layout_state.content_size = Size {
            width: 100.0,
            height: 900.0,
        };
        element.clear_local_dirty_flags(DirtyFlags::ALL);
    }
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, leaf);
        element.layout_state.layout_position.x = 10.0;
        element.layout_state.layout_position.y = 20.0;
        element.layout_state.layout_size = Size {
            width: 100.0,
            height: 900.0,
        };
        element.layout_state.layout_inner_position.x = 10.0;
        element.layout_state.layout_inner_position.y = 20.0;
        element.layout_state.layout_inner_size = Size {
            width: 100.0,
            height: 900.0,
        };
        element.layout_state.content_size = Size {
            width: 100.0,
            height: 900.0,
        };
        element.set_background_color_value(Color::rgb(48, 72, 96));
        element.clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    assert_eq!(arena.parent_of(third), Some(inner));
    (arena, root, properties, generations)
}

#[test]
fn nested_scroll_m4_freezes_ordered_direct_segments_and_one_leaf_resident_at_dpr1_and_dpr2() {
    let (arena, root, _inner, _leaf, properties, generations) =
        crate::view::paint::frame_plan::tests::nested_scroll_plan_fixture();
    let dpr1 = compile_segment_scene(&arena, root, &properties, &generations, 1.0);
    let dpr1_stamp = dpr1.persistent_leaf_stamp_for_test().clone();
    assert_eq!(dpr1.program_shape_for_test(), (5, 2, 1, 2));
    assert_eq!(dpr1.transaction.scroll_groups.len(), 1);
    assert_eq!(dpr1.transaction.seal.ordered_boundaries.len(), 2);
    assert_eq!(dpr1.transaction.seal.scroll_bindings.len(), 1);

    let dpr2 = compile_segment_scene(&arena, root, &properties, &generations, 2.0);
    let dpr2_stamp = dpr2.persistent_leaf_stamp_for_test().clone();
    assert_eq!(dpr2.program_shape_for_test(), (5, 2, 1, 2));
    assert_eq!(dpr2_stamp.target.scale_factor_bits, 2.0_f32.to_bits());
    assert_eq!(
        dpr2_stamp.target.color.width(),
        dpr1_stamp.target.color.width() * 2
    );
    assert_eq!(
        dpr2_stamp.target.color.height(),
        dpr1_stamp.target.color.height() * 2
    );
    assert_eq!(
        dpr2_stamp.target.depth.width(),
        dpr1_stamp.target.depth.width() * 2
    );
    assert_eq!(
        dpr2_stamp.target.depth.height(),
        dpr1_stamp.target.depth.height() * 2
    );
}

#[test]
fn nested_scroll_m4_pool_prepare_is_graphless_atomic_and_parent_scroll_is_composition_only() {
    let (arena, root, inner, leaf, mut properties, mut generations) =
        crate::view::paint::frame_plan::tests::nested_scroll_plan_fixture();
    let outer = root;
    let cold_scene = compile_segment_scene(&arena, root, &properties, &generations, 1.0);
    let cold_stamp = cold_scene.persistent_leaf_stamp_for_test().clone();
    let viewport = Viewport::new();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    let cold = prepare_nested_scroll_segment_transaction_from_pool(&viewport, cold_scene)
        .expect("cold M4 pool freeze");
    assert_eq!(
        cold.action_for_test(),
        RetainedSurfaceCompileAction::Reraster
    );
    assert!(cold.transaction_is_canonical_for_test());
    assert_eq!(cold.stamp_for_test(), &cold_stamp);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );

    move_nested_scroll_fixture(&arena, outer, inner, leaf);
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let moved_scene = compile_segment_scene(&arena, root, &properties, &generations, 1.0);
    assert_eq!(
        moved_scene.persistent_leaf_stamp_for_test(),
        &cold_stamp,
        "parent offset belongs to composite/spatial state, not leaf raster identity"
    );
    let moved = prepare_nested_scroll_segment_transaction_from_pool(&viewport, moved_scene)
        .expect("warm M4 pool freeze after parent scroll");
    assert_eq!(moved.stamp_for_test(), &cold_stamp);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before,
        "both pool freezes remain read-only and atomic"
    );
}

#[test]
fn nested_scroll_m4_chain_shape_is_not_capped_at_two_boundaries() {
    let (arena, root, properties, generations) = three_deep_segment_fixture();
    let scene = compile_segment_scene(&arena, root, &properties, &generations, 1.0);
    assert_eq!(scene.program_shape_for_test(), (7, 3, 1, 3));
    assert_eq!(scene.transaction.seal.ordered_boundaries.len(), 3);
    assert_eq!(scene.transaction.scroll_groups.len(), 1);
    assert!(scene.transaction.is_canonical());
}

#[test]
fn nested_scroll_m4_fails_closed_when_parent_content_requires_more_segments() {
    let (mut arena, root, _inner, _leaf, _properties, _generations) =
        crate::view::paint::frame_plan::tests::nested_scroll_plan_fixture();
    let sibling = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x1251_04, 10.0, 40.0, 40.0, 20.0,
    ))));
    arena.set_parent(sibling, Some(root));
    arena.push_child(root, sibling);
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, sibling);
        element.set_background_color_value(Color::rgb(96, 48, 24));
        element.clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);

    let error = match plan_and_validate_nested_scroll_segment_scene(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    ) {
        Ok(_) => panic!("M4 admits only the exact leaf-only segment slice"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        PropertyScrollScenePlanError::InvalidContract(
            "nested-scroll-segment-admission" | "nested-scroll-segment-compile"
        )
    ));
}

#[test]
fn nested_scroll_m5b2_production_facade_prepares_and_emits_the_nested_segment() {
    let (arena, root, _inner, _leaf, properties, generations) =
        crate::view::paint::frame_plan::tests::nested_scroll_plan_fixture();
    let scene = PropertyBoundaryDagCompiler::plan_and_validate(
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
    .expect("M5b-2 opens the production DAG nested segment");
    assert_eq!(scene.nested_scroll_chain_depth(), Some(2));
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_property_boundary_dag_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        owner,
    )
    .expect("M5b-2 prepares through the production DAG facade");
    let outcome = emit_prepared_property_boundary_dag_scene(prepared);
    assert_eq!(outcome.trace.scroll_group_count, 1);
    assert!(!viewport.retained_property_scroll_scene_stage_is_available());
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
}
