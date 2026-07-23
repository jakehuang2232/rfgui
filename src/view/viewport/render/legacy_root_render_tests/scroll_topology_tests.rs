use super::*;

#[test]
fn retained_auto_transform_scroll_selects_and_emits_one_atomic_scene() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, properties, generations) = prepared_transform_scroll_scene(
        glam::Mat4::from_translation(glam::Vec3::new(7.0, 5.0, 0.0)),
    );
    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let AutoAuthorityDecision::TransformScrollScene { scene, trace } = decision else {
        panic!("exact T->S must select the transform-scroll property scene")
    };
    assert!(scene.is_canonical());
    assert!(matches!(
        trace.rejections.as_slice(),
        [
            AutoAuthorityRejection::PropertyScrollPlan { .. },
            AutoAuthorityRejection::PropertyScrollPlan { .. }
        ]
    ));

    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = crate::view::paint::prepare_retained_transform_scroll_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        ctx,
        [0.0, 0.0, 0.0, 1.0],
        owner,
    )
    .unwrap();
    let outcome = crate::view::paint::emit_prepared_retained_transform_scroll_scene(prepared);
    let (_, trace) = outcome.into_parts();
    assert_eq!(trace.root_count, 1);
    assert_eq!(trace.generic_surface_count, 1);
    assert_eq!(trace.scroll_group_count, 1);
    assert_eq!(trace.reraster_count, 2);
    assert_eq!(trace.reuse_count, 0);
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .len(),
        3,
        "one root clear plus receiver/content reraster clears"
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
}

#[test]
fn retained_auto_same_owner_transform_scroll_is_final_retained_and_not_red() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, properties, generations) = prepared_same_owner_transform_scroll_scene();
    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    assert!(
        matches!(decision, AutoAuthorityDecision::TransformScrollScene { .. }),
        "same-owner native T+S must select the production transform-scroll authority"
    );
    let telemetry = telemetry_for_auto_decision(decision);
    assert_eq!(
        telemetry.final_authority(),
        PaintAuthorityKind::PropertyScene
    );
    assert!(telemetry.fallback_boundary_nodes().is_empty());
    assert!(retained_auto_fallback_overlay_records(&telemetry, &roots).is_empty());

    let mut viewport = Viewport::new();
    viewport.scene.node_arena = arena;
    let capture = viewport.build_retained_auto_debug_capture(&telemetry, &roots, true, true);
    assert_eq!(
        capture.frame.selected_authority,
        crate::view::debug::DebugFramePaintAuthority::PropertyScene
    );
    assert_eq!(
        capture.frame.disposition,
        crate::view::debug::DebugFrameDisposition::Presented
    );
    assert!(capture.frame.fallback_stages.is_empty());
    assert_eq!(capture.frame.statistics.fallback_count, 0);
    assert!(capture.nodes.iter().all(|node| node.fallbacks.is_empty()));
}

#[test]
fn retained_auto_same_owner_effect_scroll_is_final_retained_and_not_red() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, properties, generations) = prepared_same_owner_effect_scroll_scene();
    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    assert!(
        matches!(decision, AutoAuthorityDecision::EffectScrollScene { .. }),
        "same-owner native E+S must select the production effect-scroll authority"
    );
    let telemetry = telemetry_for_auto_decision(decision);
    assert_eq!(
        telemetry.final_authority(),
        PaintAuthorityKind::PropertyScene
    );
    assert!(telemetry.fallback_boundary_nodes().is_empty());
    assert!(retained_auto_fallback_overlay_records(&telemetry, &roots).is_empty());

    let mut viewport = Viewport::new();
    viewport.scene.node_arena = arena;
    let capture = viewport.build_retained_auto_debug_capture(&telemetry, &roots, true, true);
    assert_eq!(
        capture.frame.selected_authority,
        crate::view::debug::DebugFramePaintAuthority::PropertyScene
    );
    assert_eq!(
        capture.frame.disposition,
        crate::view::debug::DebugFrameDisposition::Presented
    );
    assert!(capture.frame.fallback_stages.is_empty());
    assert_eq!(capture.frame.statistics.fallback_count, 0);
    assert!(capture.nodes.iter().all(|node| node.fallbacks.is_empty()));
}

#[test]
fn retained_auto_effect_scroll_selects_and_emits_one_atomic_scene() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, _, _) = prepared_transform_scroll_scene(glam::Mat4::IDENTITY);
    crate::view::test_support::get_element_mut::<Element>(&arena, roots[0])
        .set_resolved_transform_for_test(None);
    crate::view::test_support::get_element_mut::<Element>(&arena, roots[0]).set_opacity(0.5);
    arena.refresh_subtree_dirty_cache(roots[0]);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let AutoAuthorityDecision::EffectScrollScene { scene, trace } = decision else {
        panic!("exact E->S must select the effect-scroll property scene")
    };
    assert!(scene.is_canonical());
    assert!(matches!(
        trace.rejections.as_slice(),
        [
            AutoAuthorityRejection::PropertyScrollPlan { .. },
            AutoAuthorityRejection::PropertyScrollPlan { .. },
            AutoAuthorityRejection::TransformScrollPlan { .. }
        ]
    ));

    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = crate::view::paint::prepare_retained_effect_scroll_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        ctx,
        [0.0, 0.0, 0.0, 1.0],
        owner,
    )
    .unwrap();
    let outcome = crate::view::paint::emit_prepared_retained_effect_scroll_scene(prepared);
    let (_, trace) = outcome.into_parts();
    assert_eq!(trace.root_count, 1);
    assert_eq!(trace.generic_surface_count, 1);
    assert_eq!(trace.effect_surface_count, 1);
    assert_eq!(trace.scroll_group_count, 1);
    assert_eq!(trace.reraster_count, 2);
    assert_eq!(trace.reuse_count, 0);
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .len(),
        3,
        "one root clear plus effect/content reraster clears"
    );
    let composites = graph.test_graphics_passes::<
        crate::view::render_pass::composite_layer_pass::CompositeLayerPass,
    >();
    assert_eq!(composites.len(), 1);
    assert_eq!(
        composites[0].test_snapshot().opacity_bits,
        0.5_f32.to_bits()
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
}

#[test]
fn retained_auto_exact_multi_scroll_selects_one_atomic_scene() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, properties, generations) = prepared_exact_multi_scroll_scene();
    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let AutoAuthorityDecision::PropertyScrollScene { scene, trace } = decision else {
        panic!("two exact top-level scroll roots must select one property scene")
    };
    assert_eq!(scene.boundary_count(), 2);
    assert!(matches!(
        trace.rejections.as_slice(),
        [AutoAuthorityRejection::PropertyScrollPlan { .. }]
    ));
    assert_eq!(properties.scrolls.len(), 2);
}

#[test]
fn retained_auto_occupied_pending_falls_back_without_finishing_foreign_owner() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, properties, generations) = prepared_exact_scroll_scene();
    let AutoAuthorityDecision::PropertyScrollScene { scene, .. } =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true)
    else {
        panic!("exact B0 scroll must select PropertyScene before prepare")
    };

    let mut viewport = Viewport::new();
    assert!(viewport.stage_retained_surface_clear());
    let foreign_pending = viewport.compositor.pending_retained_surfaces.clone();
    let foreign_owner = viewport.compositor.pending_retained_surface_owner;
    let resident_before = viewport.compositor.retained_surfaces.clone();
    let frame_owner = viewport.begin_retained_surface_frame_stage();
    assert!(frame_owner.is_none());

    let mut graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    assert!(scene.is_canonical());
    assert!(!viewport.retained_property_scroll_scene_stage_is_available());
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);

    let mut legacy_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let output = legacy_ctx.allocate_target(&mut graph);
    graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: legacy_ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: output,
        },
    ));
    assert!(!viewport.stage_retained_surface_clear());
    assert!(!viewport.finish_retained_surface_transaction_for_frame(frame_owner, true));
    assert_eq!(
        viewport.compositor.pending_retained_surfaces,
        foreign_pending
    );
    assert_eq!(
        viewport.compositor.pending_retained_surface_owner,
        foreign_owner
    );
    assert_eq!(viewport.compositor.retained_surfaces, resident_before);

    viewport.finish_retained_surface_transaction(true);
    assert!(viewport.compositor.pending_retained_surfaces.is_none());
}

#[test]
fn retained_auto_selects_supported_scroll_topologies_and_rejects_the_rest() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let assert_typed_rejection = |arena: &NodeArena, roots: &[NodeKey]| {
        let (properties, generations) = synced_paint_state(arena, roots);
        let AutoAuthorityDecision::Legacy { trace } =
            select_retained_auto_authority(arena, roots, &properties, &generations, &ctx, true)
        else {
            panic!("unsupported scroll topology must remain whole-frame legacy")
        };
        let expected = matches!(
            trace.rejections.as_slice(),
            [
                AutoAuthorityRejection::PropertyScrollPlan { .. },
                AutoAuthorityRejection::PropertyScrollPlan { .. },
                AutoAuthorityRejection::TransformScrollPlan { .. },
                AutoAuthorityRejection::EffectScrollPlan { .. },
                AutoAuthorityRejection::TransformEffectScrollPlan { .. },
                AutoAuthorityRejection::PropertyBoundaryDagPlan { .. },
                AutoAuthorityRejection::DirectScrollTransformPlan { .. }
            ]
        );
        assert!(expected, "typed scroll rejection: {:?}", trace.rejections);
    };

    let (transform_arena, transform_roots, transform_properties, transform_generations) =
        prepared_same_owner_transform_scroll_scene();
    let transform_decision = select_retained_auto_authority(
        &transform_arena,
        &transform_roots,
        &transform_properties,
        &transform_generations,
        &ctx,
        true,
    );
    assert!(
        matches!(
            transform_decision,
            AutoAuthorityDecision::TransformScrollScene { .. }
        ),
        "co-located native T+S must use its typed retained authority"
    );

    let (effect_arena, effect_roots, effect_properties, effect_generations) =
        prepared_same_owner_effect_scroll_scene();
    let effect_decision = select_retained_auto_authority(
        &effect_arena,
        &effect_roots,
        &effect_properties,
        &effect_generations,
        &ctx,
        true,
    );
    assert!(
        matches!(
            effect_decision,
            AutoAuthorityDecision::EffectScrollScene { .. }
        ),
        "co-located native E+S must use its typed retained authority"
    );

    let (mut nested_arena, nested_roots, mut nested_properties, mut nested_generations) =
        prepared_exact_nested_scroll_scene();
    let outer = nested_roots[0];
    let inner = nested_arena.children_of(outer)[0];
    let extra_leaf = nested_arena.insert(Node::new(Box::new(Element::new_with_id(
        0xe2_b320, 10.0, 20.0, 100.0, 60.0,
    ))));
    nested_arena.set_parent(extra_leaf, Some(inner));
    nested_arena.push_child(inner, extra_leaf);
    nested_arena
        .get_mut(extra_leaf)
        .expect("extra nested leaf")
        .element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    nested_arena.refresh_subtree_dirty_cache(outer);
    nested_properties.sync(&nested_arena, &nested_roots);
    nested_generations.sync(&nested_arena, &nested_roots, &nested_properties);
    assert_eq!(nested_roots.len(), 1);
    assert_eq!(nested_properties.scrolls.len(), 2);
    let captured = select_retained_auto_authority(
        &nested_arena,
        &nested_roots,
        &nested_properties,
        &nested_generations,
        &ctx,
        true,
    );
    let uncaptured = select_retained_auto_authority(
        &nested_arena,
        &nested_roots,
        &nested_properties,
        &nested_generations,
        &ctx,
        false,
    );
    assert!(matches!(&captured, AutoAuthorityDecision::Legacy { .. }));
    assert!(matches!(&uncaptured, AutoAuthorityDecision::Legacy { .. }));
    assert_eq!(
        auto_authority_kind(&captured),
        auto_authority_kind(&uncaptured),
        "trace capture must not change malformed nested-scroll authority"
    );
    assert!(matches!(
        auto_authority_trace(&captured).rejections.as_slice(),
        [
            AutoAuthorityRejection::NestedScrollPlan { .. },
            AutoAuthorityRejection::PropertyScrollPlan { .. },
            AutoAuthorityRejection::PropertyScrollPlan { .. },
            AutoAuthorityRejection::TransformScrollPlan { .. },
            AutoAuthorityRejection::EffectScrollPlan { .. },
            AutoAuthorityRejection::TransformEffectScrollPlan { .. },
            AutoAuthorityRejection::NativeScrollForestPlan { .. },
            AutoAuthorityRejection::PropertyBoundaryDagPlan { .. },
            AutoAuthorityRejection::DirectScrollTransformPlan { .. }
        ]
    ));
    assert!(auto_authority_trace(&uncaptured).rejections.is_empty());

    for matrix in [
        glam::Mat4::from_scale(glam::Vec3::new(1.1, 1.0, 1.0)),
        glam::Mat4::from_rotation_z(0.2),
    ] {
        let (arena, roots, _, _) = prepared_transform_scroll_scene(matrix);
        assert_typed_rejection(&arena, &roots);
    }

    let (mut clipped_arena, clipped_roots, _, _) =
        prepared_transform_scroll_scene(glam::Mat4::IDENTITY);
    let clipped_effect = clipped_roots[0];
    crate::view::test_support::get_element_mut::<Element>(&clipped_arena, clipped_effect)
        .set_resolved_transform_for_test(None);
    crate::view::test_support::get_element_mut::<Element>(&clipped_arena, clipped_effect)
        .set_opacity(0.5);
    let clip_root = clipped_arena.insert(Node::new(Box::new(TransparentContentsClipParent {
        id: 0xe2_c3e0,
        scissor: [4, 6, 100, 70],
        children: Vec::new(),
    })));
    clipped_arena.set_parent(clipped_effect, Some(clip_root));
    clipped_arena.push_child(clip_root, clipped_effect);
    clipped_arena.refresh_subtree_dirty_cache(clip_root);
    assert_typed_rejection(&clipped_arena, &[clip_root]);

    let (mut nested_effect_arena, nested_effect_roots, _, _) =
        prepared_transform_scroll_scene(glam::Mat4::IDENTITY);
    let outer_effect = nested_effect_roots[0];
    crate::view::test_support::get_element_mut::<Element>(&nested_effect_arena, outer_effect)
        .set_resolved_transform_for_test(None);
    crate::view::test_support::get_element_mut::<Element>(&nested_effect_arena, outer_effect)
        .set_opacity(0.5);
    let scroll = nested_effect_arena.children_of(outer_effect)[0];
    let mut inner_effect = Element::new_with_id(0xe2_c3e1, 0.0, 0.0, 120.0, 90.0);
    let mut inner_style = Style::new();
    inner_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    inner_effect.apply_style(inner_style);
    inner_effect.set_opacity(0.25);
    let inner_effect = nested_effect_arena.insert(Node::new(Box::new(inner_effect)));
    nested_effect_arena.set_parent(inner_effect, Some(outer_effect));
    nested_effect_arena.set_children(outer_effect, vec![inner_effect]);
    nested_effect_arena.set_parent(scroll, Some(inner_effect));
    nested_effect_arena.set_children(inner_effect, vec![scroll]);
    nested_effect_arena.refresh_subtree_dirty_cache(outer_effect);
    assert_typed_rejection(&nested_effect_arena, &nested_effect_roots);

    let (scroll_effect_arena, scroll_effect_root, properties, generations) =
        crate::view::paint::retained_auto_scroll_content_effect_fixture(false, false);
    assert!(matches!(
        select_retained_auto_authority(
            &scroll_effect_arena,
            &[scroll_effect_root],
            &properties,
            &generations,
            &ctx,
            true,
        ),
        AutoAuthorityDecision::PropertyBoundaryDagScene { .. }
    ));

    let (scroll_transform_arena, scroll_transform_roots, _, _) = prepared_exact_scroll_scene();
    let scroll_transform_child =
        scroll_transform_arena.children_of(scroll_transform_roots[0])[0];
    crate::view::test_support::get_element_mut::<Element>(
        &scroll_transform_arena,
        scroll_transform_child,
    )
    .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
        glam::Vec3::new(3.0, 0.0, 0.0),
    )));
    scroll_transform_arena.refresh_subtree_dirty_cache(scroll_transform_roots[0]);
    let (scroll_transform_properties, scroll_transform_generations) =
        synced_paint_state(&scroll_transform_arena, &scroll_transform_roots);
    let AutoAuthorityDecision::DirectScrollTransformScene { scene, trace } =
        select_retained_auto_authority(
            &scroll_transform_arena,
            &scroll_transform_roots,
            &scroll_transform_properties,
            &scroll_transform_generations,
            &ctx,
            true,
        )
    else {
        panic!("exact S->T must select only after all older scroll authorities reject")
    };
    assert!(scene.is_canonical());
    assert!(matches!(
        trace.rejections.as_slice(),
        [
            AutoAuthorityRejection::PropertyScrollPlan { .. },
            AutoAuthorityRejection::PropertyScrollPlan { .. },
            AutoAuthorityRejection::TransformScrollPlan { .. },
            AutoAuthorityRejection::EffectScrollPlan { .. },
            AutoAuthorityRejection::TransformEffectScrollPlan { .. },
            AutoAuthorityRejection::PropertyBoundaryDagPlan { .. }
        ]
    ));
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = crate::view::paint::prepare_direct_scroll_transform_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        owner,
    )
    .unwrap();
    let outcome = crate::view::paint::emit_prepared_direct_scroll_transform_scene(prepared);
    let (_, build_trace) = outcome.into_parts();
    assert_eq!(build_trace.root_count, 1);
    assert_eq!(build_trace.generic_surface_count, 1);
    assert_eq!(build_trace.scroll_group_count, 0);
    assert_eq!(build_trace.reraster_count, 1);
    assert_eq!(build_trace.reuse_count, 0);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));

    let (effect_scroll_arena, effect_scroll_roots, _, _) = prepared_transform_scroll_scene(
        glam::Mat4::from_translation(glam::Vec3::new(3.0, 0.0, 0.0)),
    );
    crate::view::test_support::get_element_mut::<Element>(
        &effect_scroll_arena,
        effect_scroll_roots[0],
    )
    .set_resolved_transform_for_test(None);
    crate::view::test_support::get_element_mut::<Element>(
        &effect_scroll_arena,
        effect_scroll_roots[0],
    )
    .set_opacity(0.5);
    effect_scroll_arena.refresh_subtree_dirty_cache(effect_scroll_roots[0]);
    let (effect_scroll_properties, effect_scroll_generations) =
        synced_paint_state(&effect_scroll_arena, &effect_scroll_roots);
    let AutoAuthorityDecision::EffectScrollScene { scene, trace } =
        select_retained_auto_authority(
            &effect_scroll_arena,
            &effect_scroll_roots,
            &effect_scroll_properties,
            &effect_scroll_generations,
            &ctx,
            true,
        )
    else {
        panic!("exact direct E->S must select the effect-scroll property scene")
    };
    assert!(scene.is_canonical());
    assert!(matches!(
        trace.rejections.as_slice(),
        [
            AutoAuthorityRejection::PropertyScrollPlan { .. },
            AutoAuthorityRejection::PropertyScrollPlan { .. },
            AutoAuthorityRejection::TransformScrollPlan { .. }
        ]
    ));

    let (mut transform_effect_scroll_arena, transform_effect_scroll_roots, _, _) =
        prepared_transform_scroll_scene(glam::Mat4::from_translation(glam::Vec3::new(
            3.0, 0.0, 0.0,
        )));
    let transform_root = transform_effect_scroll_roots[0];
    let scroll = transform_effect_scroll_arena.children_of(transform_root)[0];
    let mut effect = Element::new_with_id(0xe2_c3f0, 0.0, 0.0, 120.0, 90.0);
    let mut effect_style = Style::new();
    effect_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    effect.apply_style(effect_style);
    effect.set_opacity(0.5);
    let effect = transform_effect_scroll_arena.insert(Node::new(Box::new(effect)));
    transform_effect_scroll_arena.set_parent(effect, Some(transform_root));
    transform_effect_scroll_arena.set_children(transform_root, vec![effect]);
    transform_effect_scroll_arena.set_parent(scroll, Some(effect));
    transform_effect_scroll_arena.set_children(effect, vec![scroll]);
    transform_effect_scroll_arena.refresh_subtree_dirty_cache(transform_root);
    let (properties, generations) = synced_paint_state(
        &transform_effect_scroll_arena,
        &transform_effect_scroll_roots,
    );
    let AutoAuthorityDecision::TransformEffectScrollScene { scene, trace } =
        select_retained_auto_authority(
            &transform_effect_scroll_arena,
            &transform_effect_scroll_roots,
            &properties,
            &generations,
            &ctx,
            true,
        )
    else {
        panic!("exact T->E->S must select the transform-effect-scroll property scene")
    };
    assert!(scene.is_canonical());
    assert!(matches!(
        trace.rejections.as_slice(),
        [
            AutoAuthorityRejection::PropertyScrollPlan { .. },
            AutoAuthorityRejection::PropertyScrollPlan { .. },
            AutoAuthorityRejection::TransformScrollPlan { .. },
            AutoAuthorityRejection::EffectScrollPlan { .. }
        ]
    ));

    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared =
        crate::view::paint::prepare_retained_transform_effect_scroll_scene_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            [0.0, 0.0, 0.0, 1.0],
            owner,
        )
        .unwrap();
    let outcome =
        crate::view::paint::emit_prepared_retained_transform_effect_scroll_scene(prepared);
    let (_, trace) = outcome.into_parts();
    assert_eq!(trace.root_count, 1);
    assert_eq!(trace.generic_surface_count, 2);
    assert_eq!(trace.effect_surface_count, 1);
    assert_eq!(trace.scroll_group_count, 1);
    assert_eq!(trace.reraster_count, 3);
    assert_eq!(trace.reuse_count, 0);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
}

#[test]
fn retained_auto_selects_and_executes_effect_transform_scroll_boundary_dag() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    for with_neutral_wrappers in [false, true] {
        let (mut arena, transform_roots, _, _) = prepared_transform_scroll_scene(
            glam::Mat4::from_translation(glam::Vec3::new(3.0, 5.0, 0.0)),
        );
        let transform = transform_roots[0];
        let scroll = arena.children_of(transform)[0];
        let effect = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xe2_c4f0, 0.0, 0.0, 168.0, 112.0,
        ))));
        let mut effect_style = Style::new();
        effect_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        {
            let mut element =
                crate::view::test_support::get_element_mut::<Element>(&arena, effect);
            element.apply_style(effect_style);
            element.set_opacity(0.625);
        }
        arena.set_children(effect, vec![transform]);
        arena.set_parent(transform, Some(effect));

        if with_neutral_wrappers {
            let outer_wrapper = arena.insert(Node::new(Box::new(Element::new_with_id(
                0xe2_c4f1, 0.0, 0.0, 150.0, 105.0,
            ))));
            let inner_wrapper = arena.insert(Node::new(Box::new(Element::new_with_id(
                0xe2_c4f2, 0.0, 0.0, 140.0, 100.0,
            ))));
            for wrapper in [outer_wrapper, inner_wrapper] {
                let mut style = Style::new();
                style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
                crate::view::test_support::get_element_mut::<Element>(&arena, wrapper)
                    .apply_style(style);
            }
            arena.set_children(effect, vec![outer_wrapper]);
            arena.set_parent(outer_wrapper, Some(effect));
            arena.set_children(outer_wrapper, vec![transform]);
            arena.set_parent(transform, Some(outer_wrapper));
            arena.set_children(transform, vec![inner_wrapper]);
            arena.set_parent(inner_wrapper, Some(transform));
            arena.set_children(inner_wrapper, vec![scroll]);
            arena.set_parent(scroll, Some(inner_wrapper));
        }

        arena.refresh_subtree_dirty_cache(effect);
        let roots = vec![effect];
        let (properties, generations) = synced_paint_state(&arena, &roots);
        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
            true,
        );
        let AutoAuthorityDecision::PropertyBoundaryDagScene { scene, .. } = decision else {
            panic!(
                "exact E->T->S (neutral_wrappers={with_neutral_wrappers}) must select the production BoundaryDag authority"
            )
        };
        assert!(matches!(
            scene,
            crate::view::paint::ValidatedPropertyBoundaryDagScene::EffectTransformScroll(_)
        ));

        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let prepared = crate::view::paint::prepare_property_boundary_dag_scene_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            [0.0, 0.0, 0.0, 1.0],
            owner,
        )
        .unwrap_or_else(|error| {
            panic!(
                "production BoundaryDag joint prepare (neutral_wrappers={with_neutral_wrappers}): {error:?}"
            )
        });
        let outcome = crate::view::paint::emit_prepared_property_boundary_dag_scene(prepared);
        let (_, trace) = outcome.into_parts();
        assert_eq!(
            (
                trace.root_count,
                trace.generic_surface_count,
                trace.effect_surface_count,
                trace.scroll_group_count,
                trace.reraster_count,
            ),
            (1, 2, 1, 1, 3)
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    }
}

#[test]
fn retained_auto_same_owner_transform_effect_scroll_is_retained_and_not_red() {
    let (arena, roots, _, _) = prepared_transform_scroll_scene(glam::Mat4::from_translation(
        glam::Vec3::new(3.0, 5.0, 0.0),
    ));
    let root = roots[0];
    crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.625);
    arena.refresh_subtree_dirty_cache(root);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    assert!(
        matches!(
            decision,
            AutoAuthorityDecision::TransformEffectScrollScene { .. }
        ),
        "same-owner T/E with descendant S must select production retained authority"
    );
    let telemetry = telemetry_for_auto_decision(decision);
    assert_eq!(
        telemetry.final_authority(),
        PaintAuthorityKind::PropertyScene
    );
    assert!(telemetry.fallback_boundary_nodes().is_empty());
    assert!(retained_auto_fallback_overlay_records(&telemetry, &roots).is_empty());

    let mut viewport = Viewport::new();
    viewport.scene.node_arena = arena;
    let capture = viewport.build_retained_auto_debug_capture(&telemetry, &roots, true, true);
    assert_eq!(
        capture.frame.selected_authority,
        crate::view::debug::DebugFramePaintAuthority::PropertyScene
    );
    assert_eq!(
        capture.frame.disposition,
        crate::view::debug::DebugFrameDisposition::Presented
    );
    assert!(capture.frame.fallback_stages.is_empty());
    assert_eq!(capture.frame.statistics.fallback_count, 0);
    assert!(capture.nodes.iter().all(|node| node.fallbacks.is_empty()));
}

#[test]
fn retained_auto_does_not_treat_plain_overflow_as_an_authored_scroll_boundary() {
    let mut arena = new_test_arena();
    let mut root_element = colored_element(0xe2_a320, 0.0, Color::rgb(20, 40, 80));
    let mut layout_style = Style::new();
    layout_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_element.apply_style(layout_style);
    let root = commit_element(&mut arena, Box::new(root_element));
    let child = commit_child(
        &mut arena,
        root,
        Box::new(Element::new_with_id(0xe2_a321, 0.0, 0.0, 120.0, 120.0)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    assert!(arena.get(child).is_some());
    assert!(!super::super::reachable_tree_has_scroll_container(&arena, &[root]));
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let decision = auto_decision(&arena, &[root], &ctx);
    assert!(!matches!(
        decision,
        AutoAuthorityDecision::PropertyScrollScene { .. }
    ));
    if let AutoAuthorityDecision::Legacy { trace } = decision {
        assert!(!trace.rejections.iter().any(|rejection| matches!(
            rejection,
            AutoAuthorityRejection::PropertyScrollPlan { .. }
        )));
    }
}
