use super::*;

#[derive(Clone, Copy)]
enum RootShape {
    Empty,
    PaintedEmpty,
    Scroll,
}

struct Fixture {
    arena: NodeArena,
    roots: Vec<NodeKey>,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn fixture(shapes: &[RootShape]) -> Fixture {
    let mut arena = NodeArena::new();
    let mut roots = Vec::with_capacity(shapes.len());
    for (ordinal, shape) in shapes.iter().copied().enumerate() {
        let stable_id = 0xd8_0001 + ordinal as u64 * 0x10;
        let root = arena.insert(Node::new(Box::new(Element::new_with_id(
            stable_id, 0.0, 0.0, 120.0, 90.0,
        ))));
        if matches!(shape, RootShape::Scroll) {
            let content = arena.insert(Node::new(Box::new(Element::new_with_id(
                stable_id + 1,
                0.0,
                -20.0,
                120.0,
                240.0,
            ))));
            arena.set_parent(content, Some(root));
            arena.push_child(root, content);
            let mut style = Style::new();
            style.insert(
                PropertyId::ScrollDirection,
                ParsedValue::ScrollDirection(ScrollDirection::Vertical),
            );
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            {
                let mut host = crate::view::test_support::get_element_mut::<Element>(&arena, root);
                host.apply_style(style);
                host.layout_state.content_size = Size {
                    width: 120.0,
                    height: 240.0,
                };
                host.set_scroll_offset((0.0, 20.0));
                host.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
            }
            {
                let mut content =
                    crate::view::test_support::get_element_mut::<Element>(&arena, content);
                content.set_background_color_value(Color::rgb(24, 48, 72));
                content
                    .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
            }
        } else if matches!(shape, RootShape::PaintedEmpty) {
            let mut root_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, root);
            root_element.set_background_color_value(Color::rgb(80, 40, 20));
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena.refresh_subtree_dirty_cache(root);
        roots.push(root);
    }
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    Fixture {
        arena,
        roots,
        properties,
        generations,
    }
}

fn compile(shapes: &[RootShape]) -> ValidatedPropertyBoundaryProgramForestScene {
    let fixture = fixture(shapes);
    let plan = crate::view::paint::frame_plan::plan_property_boundary_program_forest(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
    )
    .expect("M7a program forest");
    compile_property_boundary_program_forest_scene(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        plan,
        1.0,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    )
    .expect("M7b compiler token")
}

fn context() -> UiBuildContext {
    UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0)
}

fn commit_one_scroll_resident(viewport: &mut Viewport) {
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_property_boundary_program_forest_from_pool(
        viewport,
        &mut graph,
        context(),
        compile(&[RootShape::Scroll]),
        [0.0; 4],
        owner,
    )
    .expect("seed scroll resident prepare");
    emit_prepared_property_boundary_program_forest(prepared);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    assert_eq!(viewport.property_scroll_active_resident_count_for_test(), 1);
}

fn assert_failed_prepare_is_atomic(
    mut viewport: Viewport,
    mut graph: FrameGraph,
    scene: ValidatedPropertyBoundaryProgramForestScene,
    expected: RetainedPropertyScrollScenePrepareError,
) {
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let graph_before = graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_property_boundary_program_forest_from_pool(
            &mut viewport,
            &mut graph,
            context(),
            scene,
            [0.0; 4],
            owner,
        )
        .err(),
        Some(expected)
    );
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert!(viewport.retained_surface_frame_stage_owner_is_active(owner));
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));
}

#[test]
fn multi_root_permutations_emit_one_exact_joint_graph_and_pool_delta() {
    for shapes in [
        vec![RootShape::Empty, RootShape::Scroll],
        vec![RootShape::Scroll, RootShape::Empty],
        vec![RootShape::Scroll, RootShape::Scroll],
        vec![
            RootShape::Empty,
            RootShape::Scroll,
            RootShape::Empty,
            RootShape::Scroll,
        ],
    ] {
        let scroll_count = shapes
            .iter()
            .filter(|shape| matches!(shape, RootShape::Scroll))
            .count();
        let scene = compile(&shapes);
        assert!(scene.is_canonical());
        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let empty_graph = graph.build_state_snapshot_for_test();
        let prepared = prepare_property_boundary_program_forest_from_pool(
            &mut viewport,
            &mut graph,
            context(),
            scene,
            [0.0; 4],
            owner,
        )
        .expect("joint M7b prepare");
        let state = emit_prepared_property_boundary_program_forest(prepared);
        assert_ne!(graph.build_state_snapshot_for_test(), empty_graph);
        assert_eq!(
            graph.declared_persistent_texture_keys().count(),
            scroll_count * 2
        );
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                .len(),
            scroll_count + 1
        );
        assert_eq!(
            graph
                .test_graphics_passes::<
                    crate::view::render_pass::texture_composite_pass::TextureCompositePass,
                >()
                .len(),
            scroll_count
        );
        assert!(state.current_target().is_some());
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test().0,
            scroll_count
        );
    }
}

#[test]
fn all_empty_forest_atomically_replaces_a_stale_scroll_active_set() {
    let scene = compile(&[RootShape::PaintedEmpty, RootShape::Empty]);
    assert!(
        scene
            .seal
            .roots
            .iter()
            .all(|root| matches!(root, PropertyBoundaryProgramCompilerRootSeal::Empty { .. }))
    );
    assert!(scene.seal.roots.iter().any(|root| matches!(
        root,
        PropertyBoundaryProgramCompilerRootSeal::Empty {
            opaque_terminal,
            artifacts,
            ..
        } if *opaque_terminal > 0 && !artifacts.is_empty()
    )));
    let mut viewport = Viewport::new();
    commit_one_scroll_resident(&mut viewport);

    let mut failed_scene = compile(&[RootShape::PaintedEmpty, RootShape::Empty]);
    failed_scene.tamper_for_test("empty-token");
    let failed_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut failed_graph = FrameGraph::new();
    let failed_graph_before = failed_graph.build_state_snapshot_for_test();
    let failed_pool_before = viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_property_boundary_program_forest_from_pool(
            &mut viewport,
            &mut failed_graph,
            context(),
            failed_scene,
            [0.0; 4],
            failed_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::BoundaryDrift)
    );
    assert_eq!(
        failed_graph.build_state_snapshot_for_test(),
        failed_graph_before
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        failed_pool_before
    );
    assert_eq!(viewport.property_scroll_active_resident_count_for_test(), 1);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(failed_owner), true));
    assert_eq!(viewport.property_scroll_active_resident_count_for_test(), 1);

    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_property_boundary_program_forest_from_pool(
        &mut viewport,
        &mut graph,
        context(),
        scene,
        [0.0; 4],
        owner,
    )
    .expect("empty forest prepare");
    emit_prepared_property_boundary_program_forest(prepared);
    assert_eq!(graph.declared_persistent_texture_keys().count(), 0);
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::ClearPass>()
            .len(),
        1,
        "empty roots add no resident clear"
    );
    assert_eq!(
        graph
            .test_graphics_passes::<
                crate::view::render_pass::texture_composite_pass::TextureCompositePass,
            >()
            .len(),
        0,
        "empty roots add no detached-content composite"
    );
    assert_eq!(viewport.property_scroll_active_resident_count_for_test(), 1);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test().1,
        Some(0)
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    assert_eq!(viewport.property_scroll_active_resident_count_for_test(), 0);
}

#[test]
fn plan_seal_token_span_receiver_and_generation_tamper_have_zero_mutation() {
    for tamper in ["plan", "seal", "token", "span", "receiver", "generation"] {
        let mut scene = compile(&[
            RootShape::PaintedEmpty,
            RootShape::Scroll,
            RootShape::Scroll,
        ]);
        scene.tamper_for_test(tamper);
        assert!(!scene.is_canonical(), "{tamper} escaped compiler seal");
        assert_failed_prepare_is_atomic(
            Viewport::new(),
            FrameGraph::new(),
            scene,
            RetainedPropertyScrollScenePrepareError::BoundaryDrift,
        );
    }
}

#[test]
fn cumulative_cursor_overflow_and_nonzero_parent_clip_are_atomic_prepare_failures() {
    let mut overflow = compile(&[RootShape::PaintedEmpty, RootShape::Scroll]);
    assert!(overflow.force_cumulative_cursor_overflow_for_test());
    assert_failed_prepare_is_atomic(
        Viewport::new(),
        FrameGraph::new(),
        overflow,
        RetainedPropertyScrollScenePrepareError::BoundaryDrift,
    );

    let scene = compile(&[RootShape::PaintedEmpty, RootShape::Scroll]);
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    let mut clipped = context();
    assert_eq!(clipped.push_clip_id(), Some(1));
    assert!(clipped.graphics_pass_context().scissor_rect.is_none());
    assert_eq!(
        prepare_property_boundary_program_forest_from_pool(
            &mut viewport,
            &mut graph,
            clipped,
            scene,
            [0.0; 4],
            owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::ContextMismatch)
    );
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert_eq!(viewport.property_scroll_active_resident_count_for_test(), 0);
    assert!(viewport.retained_surface_frame_stage_owner_is_active(owner));
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
}

#[test]
fn second_root_collision_and_duplicate_resource_leave_no_partial_writes() {
    let scene = compile(&[RootShape::Scroll, RootShape::Scroll]);
    let declarations = scene.resource_declarations_for_test();
    let (second_key, second_desc) = declarations[1].clone();
    let mut collision_graph = FrameGraph::new();
    let _ = collision_graph.declare_persistent_texture_internal::<
        crate::view::render_pass::draw_rect_pass::RenderTargetTag,
    >(second_desc, second_key);
    assert_failed_prepare_is_atomic(
        Viewport::new(),
        collision_graph,
        scene,
        RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(second_key),
    );

    let mut duplicate = compile(&[RootShape::Scroll, RootShape::Scroll]);
    assert!(duplicate.duplicate_second_resource_for_test());
    let first_key = duplicate.resource_declarations_for_test()[0].0;
    assert_failed_prepare_is_atomic(
        Viewport::new(),
        FrameGraph::new(),
        duplicate,
        RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(first_key),
    );
}
