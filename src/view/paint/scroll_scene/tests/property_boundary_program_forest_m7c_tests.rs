use super::*;

fn transform_fixture(
    matrix: glam::Mat4,
    content: Color,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xd9_1001, 0.0, 0.0, 120.0, 90.0,
    ))));
    let child = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xd9_1002, 2.0, 3.0, 30.0, 20.0,
    ))));
    arena.set_parent(child, Some(root));
    arena.push_child(root, child);
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(40, 80, 120)),
        );
        element.apply_style(style);
    }
    {
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(content),
        );
        crate::view::test_support::get_element_mut::<Element>(&arena, child).apply_style(style);
    }
    let (constraints, placement) = window_layout_inputs();
    measure_and_place(&mut arena, root, constraints, placement);
    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_resolved_transform_for_test(Some(matrix));
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, properties, generations)
}

fn compile_transform(
    matrix: glam::Mat4,
    content: Color,
) -> ValidatedPropertyBoundaryProgramForestScene {
    let (arena, root, properties, generations) = transform_fixture(matrix, content);
    compile_transform_fixture(&arena, root, &properties, &generations)
}

fn compile_transform_fixture(
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
) -> ValidatedPropertyBoundaryProgramForestScene {
    let plan = crate::view::paint::frame_plan::plan_property_boundary_program_forest(
        arena,
        &[root],
        properties,
        generations,
    )
    .expect("pure affine Component transform root");
    assert_eq!(
        plan.roots[0].kind,
        crate::view::paint::frame_plan::PropertyBoundaryProgramRootKind::FrameRootTransformContent
    );
    compile_property_boundary_program_forest_scene(
        arena,
        &[root],
        properties,
        generations,
        plan,
        1.0,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    )
    .expect("M7c compiler token")
}

fn context() -> UiBuildContext {
    UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0)
}

fn assert_transform_prepare_failure_is_atomic(
    mut graph: FrameGraph,
    scene: ValidatedPropertyBoundaryProgramForestScene,
    expected: RetainedPropertyScrollScenePrepareError,
) {
    let mut viewport = Viewport::new();
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

#[derive(Clone, Copy)]
enum MixedRootShape {
    Empty,
    PaintedEmpty,
    Transform,
    Scroll,
}

fn compile_mixed(shapes: &[MixedRootShape]) -> ValidatedPropertyBoundaryProgramForestScene {
    let mut arena = NodeArena::new();
    let mut roots = Vec::with_capacity(shapes.len());
    for (ordinal, shape) in shapes.iter().copied().enumerate() {
        let stable_id = 0xda_0001 + ordinal as u64 * 0x10;
        let root = arena.insert(Node::new(Box::new(Element::new_with_id(
            stable_id, 0.0, 0.0, 120.0, 90.0,
        ))));
        match shape {
            MixedRootShape::Empty => {}
            MixedRootShape::PaintedEmpty => {
                let mut style = Style::new();
                style.insert(
                    PropertyId::BackgroundColor,
                    ParsedValue::color_like(Color::rgb(80, 40, 20)),
                );
                crate::view::test_support::get_element_mut::<Element>(&arena, root)
                    .apply_style(style);
                let (constraints, placement) = window_layout_inputs();
                measure_and_place(&mut arena, root, constraints, placement);
            }
            MixedRootShape::Transform => {
                let child = arena.insert(Node::new(Box::new(Element::new_with_id(
                    stable_id + 1,
                    2.0,
                    3.0,
                    30.0,
                    20.0,
                ))));
                arena.set_parent(child, Some(root));
                arena.push_child(root, child);
                {
                    let mut root_style = Style::new();
                    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
                    crate::view::test_support::get_element_mut::<Element>(&arena, root)
                        .apply_style(root_style);
                    let mut child_style = Style::new();
                    child_style.insert(
                        PropertyId::BackgroundColor,
                        ParsedValue::color_like(Color::rgb(120, 60, 20)),
                    );
                    crate::view::test_support::get_element_mut::<Element>(&arena, child)
                        .apply_style(child_style);
                }
                let (constraints, placement) = window_layout_inputs();
                measure_and_place(&mut arena, root, constraints, placement);
                crate::view::test_support::get_element_mut::<Element>(&arena, root)
                    .set_resolved_transform_for_test(Some(
                        glam::Mat4::from_rotation_z(0.08 * (ordinal + 1) as f32)
                            * glam::Mat4::from_scale(glam::Vec3::new(1.05, 0.95, 1.0)),
                    ));
            }
            MixedRootShape::Scroll => {
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
                    let mut host =
                        crate::view::test_support::get_element_mut::<Element>(&arena, root);
                    host.apply_style(style);
                    host.layout_state.content_size = Size {
                        width: 120.0,
                        height: 240.0,
                    };
                    host.set_scroll_offset((0.0, 20.0));
                    host.clear_local_dirty_flags(
                        DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT),
                    );
                }
                let mut content =
                    crate::view::test_support::get_element_mut::<Element>(&arena, content);
                content.set_background_color_value(Color::rgb(24, 48, 72));
                content
                    .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
            }
        }
        arena.refresh_subtree_dirty_cache(root);
        roots.push(root);
    }
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    let plan = crate::view::paint::frame_plan::plan_property_boundary_program_forest(
        &arena,
        &roots,
        &properties,
        &generations,
    )
    .expect("heterogeneous M7c forest");
    compile_property_boundary_program_forest_scene(
        &arena,
        &roots,
        &properties,
        &generations,
        plan,
        1.0,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    )
    .expect("heterogeneous M7c compiler token")
}

#[test]
fn empty_transform_scroll_permutations_and_multi_transform_roots_commit_jointly() {
    use MixedRootShape::{Empty as E, Scroll as S, Transform as T};
    for shapes in [
        vec![E, T, S],
        vec![E, S, T],
        vec![T, E, S],
        vec![T, S, E],
        vec![S, E, T],
        vec![S, T, E],
        vec![T, T],
        vec![T, E, T],
        vec![T, S, T],
    ] {
        let resident_count = shapes.iter().filter(|shape| matches!(shape, T | S)).count();
        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let prepared = prepare_property_boundary_program_forest_from_pool(
            &mut viewport,
            &mut graph,
            context(),
            compile_mixed(&shapes),
            [0.0; 4],
            owner,
        )
        .expect("M7c joint prepare");
        emit_prepared_property_boundary_program_forest(prepared);
        assert_eq!(
            graph.test_graphics_passes::<TextureCompositePass>().len(),
            resident_count
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test().0,
            resident_count
        );
    }
}

#[test]
fn detached_transform_opaque_cursor_uses_exact_parent_local_max_merge() {
    use MixedRootShape::{PaintedEmpty as P, Transform as T};
    for (shapes, parent_must_be_nonzero) in [(vec![P, T], true), (vec![T], false)] {
        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let prepared = prepare_property_boundary_program_forest_from_pool(
            &mut viewport,
            &mut graph,
            context(),
            compile_mixed(&shapes),
            [0.0; 4],
            owner,
        )
        .expect("opaque max-merge prepare");
        let PreparedPropertyBoundaryProgramRoot::FrameRootTransformContent {
            frame_opaque_span,
            local_opaque_terminal,
            ..
        } = prepared.roots.last().expect("transform root")
        else {
            panic!("last root must be transform")
        };
        let parent_before = frame_opaque_span.start;
        let parent_after = frame_opaque_span.end;
        assert!(*local_opaque_terminal > 0);
        assert_eq!(parent_after, parent_before.max(*local_opaque_terminal));
        if parent_must_be_nonzero {
            assert!(
                parent_before > 0,
                "preceding painted root advances frame cursor"
            );
        } else {
            assert_eq!(parent_before, 0);
            assert!(*local_opaque_terminal > parent_before);
        }
        let expected_terminal = parent_after;
        let state = emit_prepared_property_boundary_program_forest(prepared);
        assert_eq!(state.opaque_rect_order(), expected_terminal);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    }
}

#[test]
fn affine_component_transform_with_children_emits_and_commits_one_resident() {
    let scene = compile_transform(
        glam::Mat4::from_rotation_z(0.2) * glam::Mat4::from_scale(glam::Vec3::new(1.1, 0.9, 1.0)),
        Color::rgb(200, 80, 20),
    );
    let mut viewport = Viewport::new();
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
    .unwrap();
    emit_prepared_property_boundary_program_forest(prepared);
    assert_eq!(
        graph.test_graphics_passes::<TextureCompositePass>().len(),
        1
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    assert_eq!(viewport.retained_surface_transaction_shape_for_test().0, 1);
}

#[test]
fn projective_and_descendant_transform_fail_closed_at_the_owner() {
    let mut projective = glam::Mat4::IDENTITY;
    projective.x_axis.w = 0.25;
    let (arena, root, properties, generations) =
        transform_fixture(projective, Color::rgb(200, 80, 20));
    let rejected = crate::view::paint::frame_plan::plan_property_boundary_program_forest(
        &arena,
        &[root],
        &properties,
        &generations,
    )
    .unwrap_err();
    assert!(rejected.reasons.iter().any(|reason| matches!(
        reason,
        crate::view::paint::frame_plan::PropertyBoundaryProgramRejection::UnsupportedRoot {
            owner,
            kind: crate::view::paint::frame_plan::PropertyBoundaryProgramUnsupportedKind::Transform,
            ..
        } if *owner == root
    )));

    let (arena, root, mut properties, generations) = transform_fixture(
        glam::Mat4::from_scale(glam::Vec3::splat(1.1)),
        Color::rgb(200, 80, 20),
    );
    let child = arena.children_of(root)[0];
    let parent = TransformNodeId(root);
    properties.transforms.insert(
        TransformNodeId(child),
        crate::view::compositor::property_tree::TransformNode {
            owner: child,
            parent: Some(parent),
            viewport_matrix: glam::Mat4::from_scale(glam::Vec3::splat(1.2)),
            generation: 1,
        },
    );
    let rejected = crate::view::paint::frame_plan::plan_property_boundary_program_forest(
        &arena,
        &[root],
        &properties,
        &generations,
    )
    .unwrap_err();
    assert!(rejected.reasons.iter().any(|reason| matches!(
        reason,
        crate::view::paint::frame_plan::PropertyBoundaryProgramRejection::UnsupportedRoot {
            owner,
            kind: crate::view::paint::frame_plan::PropertyBoundaryProgramUnsupportedKind::Transform,
            ..
        } if *owner == child
    )));

    let (arena, root, mut properties, mut generations) = transform_fixture(
        glam::Mat4::from_scale(glam::Vec3::splat(1.1)),
        Color::rgb(200, 80, 20),
    );
    let child = arena.children_of(root)[0];
    crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.5);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let rejected = crate::view::paint::frame_plan::plan_property_boundary_program_forest(
        &arena,
        &[root],
        &properties,
        &generations,
    )
    .unwrap_err();
    assert!(rejected.reasons.iter().any(|reason| matches!(
        reason,
        crate::view::paint::frame_plan::PropertyBoundaryProgramRejection::UnsupportedRoot {
            owner,
            kind: crate::view::paint::frame_plan::PropertyBoundaryProgramUnsupportedKind::Mixed,
            ..
        } if *owner == root
    )));
}

#[test]
fn property_neutral_descendant_outside_strict_native_corpus_rejects_at_owner() {
    let (arena, root, mut properties, mut generations) = transform_fixture(
        glam::Mat4::from_scale(glam::Vec3::splat(1.1)),
        Color::rgb(200, 80, 20),
    );
    let child = arena.children_of(root)[0];
    let mut style = Style::new();
    style.set_box_shadow(vec![crate::style::BoxShadow::new()
        .offset_x(2.0)
        .offset_y(3.0)
        .blur(4.0)]);
    crate::view::test_support::get_element_mut::<Element>(&arena, child).apply_style(style);
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let rejected = crate::view::paint::frame_plan::plan_property_boundary_program_forest(
        &arena,
        &[root],
        &properties,
        &generations,
    )
    .unwrap_err();
    assert!(rejected.reasons.iter().any(|reason| matches!(
        reason,
        crate::view::paint::frame_plan::PropertyBoundaryProgramRejection::UnsupportedRoot {
            owner,
            kind: crate::view::paint::frame_plan::PropertyBoundaryProgramUnsupportedKind::Transform,
            ..
        } if *owner == child
    )));
}

#[test]
fn transform_plan_seal_token_span_receiver_generation_and_geometry_tamper_are_atomic() {
    use MixedRootShape::Transform as T;
    for tamper in [
        "plan",
        "seal",
        "transform-token",
        "span",
        "receiver",
        "generation",
        "transform-geometry",
    ] {
        let mut scene = compile_mixed(&[T, T]);
        scene.tamper_for_test(tamper);
        assert!(!scene.is_canonical(), "{tamper} escaped M7c seal");
        assert_transform_prepare_failure_is_atomic(
            FrameGraph::new(),
            scene,
            RetainedPropertyScrollScenePrepareError::BoundaryDrift,
        );
    }
}

#[test]
fn exact_m7_transform_authority_binding_and_nested_stamp_tamper_are_atomic() {
    use MixedRootShape::{Scroll as S, Transform as T};
    for (tamper, shapes) in [
        ("m7-binding-to-scroll", vec![T, S]),
        ("m7-binding-duplicate", vec![T, T]),
        ("m7-boundary-kind", vec![T]),
        ("m7-boundary-owner", vec![T]),
        ("m7-stable-id", vec![T]),
        ("m7-color-key", vec![T]),
        ("m7-nested-stamp", vec![T]),
    ] {
        let mut scene = compile_mixed(&shapes);
        assert!(scene.is_canonical());
        scene.tamper_for_test(tamper);
        assert_transform_prepare_failure_is_atomic(
            FrameGraph::new(),
            scene,
            RetainedPropertyScrollScenePrepareError::BoundaryDrift,
        );
    }
}

#[test]
fn later_transform_resource_collision_has_zero_graph_and_pool_mutation() {
    use MixedRootShape::Transform as T;
    let scene = compile_mixed(&[T, T]);
    let declarations = scene.transform_resource_declarations_for_test();
    let (second_key, second_desc) = declarations[1].clone();
    let mut graph = FrameGraph::new();
    let _ = graph.declare_persistent_texture_internal::<
        crate::view::render_pass::draw_rect_pass::RenderTargetTag,
    >(second_desc, second_key);
    assert_transform_prepare_failure_is_atomic(
        graph,
        scene,
        RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(second_key),
    );
}

#[test]
fn matrix_only_change_keeps_raster_identity_but_content_change_rerasterizes() {
    let matrix_a = glam::Mat4::from_scale(glam::Vec3::new(1.1, 0.9, 1.0));
    let matrix_b = glam::Mat4::from_rotation_z(0.2) * matrix_a;
    let content_a = Color::rgb(200, 80, 20);
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_property_boundary_program_forest_from_pool(
        &mut viewport,
        &mut graph,
        context(),
        compile_transform(matrix_a, content_a),
        [0.0; 4],
        owner,
    )
    .unwrap();
    let (stamp_a, geometry_a) = match &prepared.roots[0] {
        PreparedPropertyBoundaryProgramRoot::FrameRootTransformContent {
            stamp, geometry, ..
        } => (
            stamp.clone(),
            crate::view::paint::compiler::retained_surface_composite_geometry_stamp(*geometry)
                .unwrap(),
        ),
        _ => panic!("transform root"),
    };
    emit_prepared_property_boundary_program_forest(prepared);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));

    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_property_boundary_program_forest_with_forced_pool_for_test(
        &mut viewport,
        &mut graph,
        context(),
        compile_transform(matrix_b, content_a),
        [0.0; 4],
        owner,
    )
    .unwrap();
    let (stamp_b, geometry_b, expected_quad_position_bits) = match &prepared.roots[0] {
        PreparedPropertyBoundaryProgramRoot::FrameRootTransformContent {
            stamp, geometry, ..
        } => (
            stamp.clone(),
            crate::view::paint::compiler::retained_surface_composite_geometry_stamp(*geometry)
                .unwrap(),
            geometry
                .texture_composite_params()
                .quad_positions
                .map(|quad| quad.map(|point| point.map(f32::to_bits))),
        ),
        _ => panic!("transform root"),
    };
    assert_eq!(stamp_a, stamp_b, "matrix is not raster identity");
    assert_ne!(geometry_a, geometry_b, "matrix changes composite geometry");
    assert_eq!(
        prepared.actions[&stamp_b.identity.resident_key()],
        RetainedSurfaceCompileAction::Reuse
    );
    emit_prepared_property_boundary_program_forest(prepared);
    let composites = graph.test_graphics_passes::<
        crate::view::render_pass::texture_composite_pass::TextureCompositePass,
    >();
    assert_eq!(composites.len(), 1);
    assert_eq!(
        composites[0].test_snapshot().quad_position_bits,
        expected_quad_position_bits,
        "matrix-only reuse must emit the new composite geometry",
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));

    let (arena, root, properties, mut generations) = transform_fixture(matrix_b, content_a);
    let child = arena.children_of(root)[0];
    let mut changed_style = Style::new();
    changed_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(20, 180, 100)),
    );
    crate::view::test_support::get_element_mut::<Element>(&arena, child).apply_style(changed_style);
    generations.sync(&arena, &[root], &properties);
    let changed = compile_transform_fixture(&arena, root, &properties, &generations);
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_property_boundary_program_forest_with_forced_pool_for_test(
        &mut viewport,
        &mut graph,
        context(),
        changed,
        [0.0; 4],
        owner,
    )
    .unwrap();
    let stamp_changed = match &prepared.roots[0] {
        PreparedPropertyBoundaryProgramRoot::FrameRootTransformContent { stamp, .. } => {
            stamp.clone()
        }
        _ => panic!("transform root"),
    };
    assert_ne!(stamp_b, stamp_changed);
    assert_eq!(
        prepared.actions[&stamp_changed.identity.resident_key()],
        RetainedSurfaceCompileAction::Reraster
    );
    emit_prepared_property_boundary_program_forest(prepared);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
}
