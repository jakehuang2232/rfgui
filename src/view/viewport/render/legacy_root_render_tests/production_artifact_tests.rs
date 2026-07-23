use super::*;

#[test]
fn production_safe_leaf_uses_direct_legacy_build_without_artifact_recording() {
    let (arena, roots) = prepared_safe_leaf();
    crate::view::paint::take_full_artifact_record_count();
    crate::view::paint::take_artifact_compile_count();
    let production_graph = build_roots_graph(arena, &roots, true);
    assert_eq!(
        crate::view::paint::take_full_artifact_record_count(),
        0,
        "production legacy authority must not invoke the full artifact recorder"
    );
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 0);

    let (legacy_arena, legacy_roots) = prepared_safe_leaf();
    let direct_legacy_graph = build_roots_graph(legacy_arena, &legacy_roots, false);
    assert!(!production_graph.test_rect_pass_snapshots().is_empty());
    assert_eq!(
        production_graph.test_rect_pass_snapshots(),
        direct_legacy_graph.test_rect_pass_snapshots(),
        "production dispatch must preserve the direct legacy pass snapshot"
    );
}

#[test]
fn production_artifact_canary_compiles_an_eligible_whole_frame() {
    let (arena, roots) = prepared_safe_leaf();
    crate::view::paint::take_full_artifact_record_count();
    crate::view::paint::take_artifact_compile_count();
    let artifact_graph = build_roots_graph_with_renderer_mode(
        arena,
        &roots,
        ViewportPaintRendererMode::ArtifactCanary,
    );
    assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);
    assert!(artifact_graph
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
        .is_empty(), "opacity=1 must stay on the direct M6A target");

    let (legacy_arena, legacy_roots) = prepared_safe_leaf();
    let legacy_graph = build_roots_graph(legacy_arena, &legacy_roots, false);
    assert_eq!(
        artifact_graph.test_rect_pass_snapshots(),
        legacy_graph.test_rect_pass_snapshots(),
        "the canary must preserve the eligible frame's pass semantics"
    );
}

#[test]
fn production_artifact_canary_compiles_a_real_contents_clipped_frame() {
    let (arena, roots) = prepared_contents_clipped_leaf();
    crate::view::paint::take_full_artifact_record_count();
    crate::view::paint::take_artifact_compile_count();
    let mut graph = build_roots_graph_with_renderer_mode(
        arena,
        &roots,
        ViewportPaintRendererMode::ArtifactCanary,
    );
    assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);
    let snapshots = graph.test_rect_pass_snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].effective_scissor_rect, Some([4, 6, 24, 18]));
    assert!(
        graph.test_compile_snapshot().is_ok(),
        "clip-enabled artifact graph must compile strictly",
    );
}

#[test]
fn production_clip_policy_admits_exact_deferred_root_and_rejects_other_boundaries() {
    let (arena, roots) = prepared_safe_leaf();
    assert!(matches!(
        artifact_canary_attempt(&arena, &roots),
        PropertyNeutralArtifactAttempt::Compiled { eligibility, .. }
            if eligibility.eligible
    ));

    let mut deferred = colored_element(0x8c30, 10.0, Color::rgb(230, 20, 30));
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(4.0))
                .clip(ClipMode::Viewport),
        ),
    );
    deferred.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(deferred));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    crate::view::paint::take_full_artifact_record_count();
    let attempt = artifact_canary_attempt(&arena, &[root]);
    assert!(matches!(
        attempt,
        PropertyNeutralArtifactAttempt::Compiled { eligibility, .. }
            if eligibility.eligible
    ));
    assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);

    let mut arena = new_test_arena();
    let effect = commit_element(
        &mut arena,
        Box::new(colored_element(0x8c31, 10.0, Color::rgb(230, 20, 30))),
    );
    arena
        .get_mut(effect)
        .expect("effect root")
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("Element")
        .set_opacity(0.5);
    let neutral = commit_element(
        &mut arena,
        Box::new(colored_element(0x8c32, 110.0, Color::rgb(20, 210, 40))),
    );
    measure_and_place(&mut arena, effect, measure, place);
    measure_and_place(&mut arena, neutral, measure, place);
    let reasons = preflight_fallback_reasons(&arena, &[effect, neutral]);
    assert!(
        reasons.contains(
            &crate::view::paint::FrameArtifactFallbackReason::PropertyBoundary(effect),
        )
    );

    let mut transformed = colored_element(0x8c33, 10.0, Color::rgb(230, 20, 30));
    let mut style = Style::new();
    style.set_transform(Transform::new([Translate::x(Length::px(3.0))]));
    transformed.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(transformed));
    measure_and_place(&mut arena, root, measure, place);
    let reasons = preflight_fallback_reasons(&arena, &[root]);
    assert!(reasons.contains(
        &crate::view::paint::FrameArtifactFallbackReason::LegacyBoundary(
            crate::view::paint::LegacyPaintReason::Transform,
        ),
    ));

    let mut scroller = colored_element(0x8c34, 10.0, Color::rgb(230, 20, 30));
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    scroller.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(scroller));
    measure_and_place(&mut arena, root, measure, place);
    let reasons = preflight_fallback_reasons(&arena, &[root]);
    assert!(reasons.contains(
        &crate::view::paint::FrameArtifactFallbackReason::LegacyBoundary(
            crate::view::paint::LegacyPaintReason::ScrollContainer,
        ),
    ));
}

#[test]
fn production_root_opacity_with_clip_records_and_compiles_once() {
    let mut clipped = colored_element(0x8c40, 10.0, Color::rgb(230, 20, 30));
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(4.0))
                .top(Length::px(5.0))
                .clip(ClipMode::AnchorParent),
        ),
    );
    clipped.apply_style(style);
    clipped.set_opacity(0.5);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(clipped));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let roots = vec![root];

    crate::view::paint::take_full_artifact_record_count();
    crate::view::paint::take_artifact_compile_count();
    let graph = build_roots_graph_with_renderer_mode(
        arena,
        &roots,
        ViewportPaintRendererMode::ArtifactCanary,
    );
    assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);
    let rects = graph.test_rect_pass_snapshots();
    assert!(!rects.is_empty());
    assert!(rects.iter().all(|rect| {
        rect.opacity_bits == 1.0_f32.to_bits() && rect.effective_scissor_rect.is_some()
    }));
    let composites = graph.test_graphics_passes::<
        crate::view::render_pass::composite_layer_pass::CompositeLayerPass,
    >();
    assert_eq!(composites.len(), 1);
    assert_eq!(
        composites[0].test_params().opacity.to_bits(),
        0.5_f32.to_bits()
    );
}

#[test]
fn production_artifact_canary_culls_hidden_parent_and_paintable_child() {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0x8c41, 0.0, 0.0, 0.0, 10.0)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    assert!(
        !arena
            .get(root)
            .unwrap()
            .element
            .box_model_snapshot()
            .should_render
    );
    let child = commit_child(
        &mut arena,
        root,
        Box::new(colored_element(0x8c42, 10.0, Color::rgb(20, 210, 40))),
    );
    measure_and_place(&mut arena, child, measure, place);
    assert!(
        arena
            .get(child)
            .unwrap()
            .element
            .box_model_snapshot()
            .should_render
    );
    let visible = commit_element(
        &mut arena,
        Box::new(colored_element(0x8c43, 110.0, Color::rgb(30, 60, 220))),
    );
    measure_and_place(&mut arena, visible, measure, place);
    let roots = vec![root, visible];

    crate::view::paint::take_full_artifact_record_count();
    crate::view::paint::take_artifact_compile_count();
    let graph = build_roots_graph_with_renderer_mode(
        arena,
        &roots,
        ViewportPaintRendererMode::ArtifactCanary,
    );
    assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);
    let rects = graph.test_rect_pass_snapshots();
    assert_eq!(rects.len(), 1, "only the visible sibling root may paint");
    assert_eq!(rects[0].position_bits[0], 110.0_f32.to_bits());
    assert!(
        graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
            .is_empty()
    );
}

#[test]
fn production_artifact_canary_uses_one_root_group_composite_for_root_effect() {
    let (arena, roots) = prepared_safe_leaf();
    arena
        .get_mut(roots[0])
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .set_opacity(0.5);
    crate::view::paint::take_full_artifact_record_count();
    crate::view::paint::take_artifact_compile_count();
    let graph = build_roots_graph_with_renderer_mode(
        arena,
        &roots,
        ViewportPaintRendererMode::ArtifactCanary,
    );
    assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);
    let composites = graph
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>();
    assert_eq!(composites.len(), 1);
    assert_eq!(
        composites[0].test_params().opacity.to_bits(),
        0.5_f32.to_bits()
    );
    assert!(
        graph
            .test_rect_pass_snapshots()
            .iter()
            .all(|rect| rect.opacity_bits == 1.0_f32.to_bits())
    );
}

#[test]
fn production_root_effect_second_opacity_only_frame_has_zero_raster_passes() {
    fn build(
        arena: &NodeArena,
        roots: &[NodeKey],
        committed: RootEffectRetainedState,
        pair_resident: bool,
    ) -> (FrameGraph, PendingRootEffectTransaction) {
        let mut properties = PropertyTrees::default();
        properties.sync(arena, roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(arena, roots, &properties);
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: target,
            },
        ));
        let root = roots[0];
        let key = crate::view::base_component::root_effect_stable_key(root);
        let desc = ctx.persistent_full_viewport_target_desc(key);
        let plan = RootEffectBuildPlan {
            committed,
            key,
            target: crate::view::paint::RootEffectRasterInputs {
                width: desc.width(),
                height: desc.height(),
                format: desc.format(),
                sample_count: desc.sample_count(),
                scale_factor_bits: ctx.viewport().scale_factor().to_bits(),
            },
            pair_resident,
        };
        let attempt = try_build_property_neutral_artifact_frame(
            &mut graph,
            arena,
            roots,
            &properties,
            &generations,
            ViewportPaintRendererMode::ArtifactCanary,
            &ctx,
            Some(&plan),
        );
        let PropertyNeutralArtifactAttempt::Compiled {
            root_effect_transaction: Some(transaction),
            ..
        } = attempt
        else {
            panic!("root effect artifact should compile");
        };
        (graph, transaction)
    }

    let (mut arena, roots) = prepared_safe_leaf();
    set_opacity_with_invalidation(&mut arena, roots[0], 0.5);
    let (_first_graph, first_transaction) =
        build(&arena, &roots, RootEffectRetainedState::Invalid, false);
    let PendingRootEffectTransaction::Commit { stamp, key, .. } = first_transaction else {
        panic!("first frame must stage a retained commit");
    };

    set_opacity_with_invalidation(&mut arena, roots[0], 0.25);
    let (mut second_graph, second_transaction) = build(
        &arena,
        &roots,
        RootEffectRetainedState::Resident { stamp, key },
        true,
    );
    assert!(matches!(
        second_transaction,
        PendingRootEffectTransaction::Commit {
            action: crate::view::paint::RootEffectCompileAction::Reuse,
            ..
        }
    ));
    let snapshot = second_graph.test_compile_snapshot().unwrap();
    assert!(matches!(
        snapshot.pass_payloads(),
        [
            crate::view::frame_graph::FramePassTestPayload::Clear(_),
            crate::view::frame_graph::FramePassTestPayload::CompositeLayer(_)
        ]
    ));
}

#[test]
fn production_artifact_canary_dispatches_outer_shadow_atomically_for_m6a_and_c1() {
    let (arena, roots) = prepared_outer_shadow_leaf(1.0, 0.0);
    crate::view::paint::take_full_artifact_record_count();
    crate::view::paint::take_artifact_compile_count();
    let m6a = build_roots_graph_with_renderer_mode(
        arena,
        &roots,
        ViewportPaintRendererMode::ArtifactCanary,
    );
    assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);
    assert!(m6a
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
        .is_empty());
    assert_eq!(
        m6a.test_graphics_passes::<crate::view::render_pass::shadow_module::ShadowFillPass>()
            .len(),
        1
    );

    let (arena, roots) = prepared_outer_shadow_leaf(0.4, 0.0);
    crate::view::paint::take_full_artifact_record_count();
    crate::view::paint::take_artifact_compile_count();
    let c1 = build_roots_graph_with_renderer_mode(
        arena,
        &roots,
        ViewportPaintRendererMode::ArtifactCanary,
    );
    assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);
    let composites = c1.test_graphics_passes::<
        crate::view::render_pass::composite_layer_pass::CompositeLayerPass,
    >();
    assert_eq!(composites.len(), 1);
    assert_eq!(
        composites[0].test_params().opacity.to_bits(),
        0.4_f32.to_bits()
    );
    let fills =
        c1.test_graphics_passes::<crate::view::render_pass::shadow_module::ShadowFillPass>();
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].test_snapshot().color_bits[3], 1.0_f32.to_bits());

    let (arena, roots) = prepared_outer_shadow_leaf(1.0, 0.000_5);
    crate::view::paint::take_full_artifact_record_count();
    crate::view::paint::take_artifact_compile_count();
    let tiny_blur = build_roots_graph_with_renderer_mode(
        arena,
        &roots,
        ViewportPaintRendererMode::ArtifactCanary,
    );
    assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);
    assert_eq!(
        tiny_blur
            .test_graphics_passes::<crate::view::render_pass::shadow_module::ShadowFillPass>()
            .len(),
        1,
        "tiny positive blur remains an owned retained shadow"
    );
    assert_eq!(
        tiny_blur
            .pass_descriptors()
            .iter()
            .filter(|pass| pass.name.ends_with("blur_module::BlurStagePass"))
            .count(),
        0,
        "the shared physical blur threshold must not add blur stages for radius 0.0005"
    );
}

#[test]
fn production_artifact_canary_falls_back_the_entire_non_neutral_frame() {
    let (arena, roots) = prepared_mixed_eligibility_roots();
    arena
        .get_mut(roots[0])
        .expect("safe root exists")
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("safe root is Element")
        .set_opacity(0.5);
    crate::view::paint::take_full_artifact_record_count();
    crate::view::paint::take_artifact_compile_count();
    let canary_graph = build_roots_graph_with_renderer_mode(
        arena,
        &roots,
        ViewportPaintRendererMode::ArtifactCanary,
    );
    assert_eq!(
        crate::view::paint::take_full_artifact_record_count(),
        0,
        "a non-neutral reachable node must reject before every full hook"
    );
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 0);

    let (legacy_arena, legacy_roots) = prepared_mixed_eligibility_roots();
    legacy_arena
        .get_mut(legacy_roots[0])
        .expect("safe root exists")
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("safe root is Element")
        .set_opacity(0.5);
    let legacy_graph = build_roots_graph(legacy_arena, &legacy_roots, false);
    assert_eq!(
        canary_graph.test_rect_pass_snapshots(),
        legacy_graph.test_rect_pass_snapshots(),
        "one property boundary must keep every root on legacy"
    );
}

#[test]
fn production_multi_root_frame_never_mixes_artifact_and_legacy_authority() {
    let (arena, roots) = prepared_mixed_eligibility_roots();
    crate::view::paint::take_full_artifact_record_count();
    let production_graph = build_roots_graph(arena, &roots, true);
    assert_eq!(
        crate::view::paint::take_full_artifact_record_count(),
        0,
        "safe roots must not record artifacts beside legacy-only roots"
    );

    let (legacy_arena, legacy_roots) = prepared_mixed_eligibility_roots();
    let direct_legacy_graph = build_roots_graph(legacy_arena, &legacy_roots, false);
    assert_eq!(
        production_graph.test_rect_pass_snapshots(),
        direct_legacy_graph.test_rect_pass_snapshots(),
        "every root in the frame must use the same direct legacy authority"
    );
}
