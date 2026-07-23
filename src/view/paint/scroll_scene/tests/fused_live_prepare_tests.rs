use super::*;

#[test]
fn validated_content_artifact_can_emit_repeatedly_to_distinct_targets() {
    let plan = plan_at_offset([0.0, 20.0]);
    let source_graph = FrameGraph::new();
    let source_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let prepared = prepare_scroll_scene(plan, &source_graph, &source_ctx, generous_budget())
        .expect("validated scroll scene");

    let mut graph = FrameGraph::new();
    let mut first_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let first = first_ctx.allocate_target(&mut graph);
    first_ctx.set_current_target(first);
    emit_validated_scroll_scene_content_artifact(&prepared.content, &mut graph, &mut first_ctx);

    let mut second_ctx =
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let second = second_ctx.allocate_target(&mut graph);
    second_ctx.set_current_target(second);
    emit_validated_scroll_scene_content_artifact(
        &prepared.content,
        &mut graph,
        &mut second_ctx,
    );

    let mut outputs = graph
        .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::DrawRectPass>()
        .into_iter()
        .map(|pass| pass.test_snapshot().output_target)
        .collect::<Vec<_>>();
    outputs.extend(
        graph
            .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::OpaqueRectPass>()
            .into_iter()
            .map(|pass| pass.test_snapshot().output_target),
    );
    assert_eq!(outputs.len(), 2);
    assert!(outputs.contains(&first.handle()));
    assert!(outputs.contains(&second.handle()));
    assert_ne!(first.handle(), second.handle());
}

#[test]
fn fused_live_prepare_accepts_fresh_authorities_and_is_graph_inert() {
    let (arena, root, child, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let graph = FrameGraph::new();
    let before = graph.build_state_snapshot_for_test();

    let prepared = prepare_live(&arena, root, &properties, &generations, &graph).unwrap();

    assert_eq!(prepared.content_stamp().identity.boundary_root, child);
    assert_eq!(graph.build_state_snapshot_for_test(), before);
}

#[test]
fn reraster_emits_host_content_composite_overlay_and_stages_target_local_cursors() {
    let mut graph = FrameGraph::new();
    let (prepared, ctx, parent) = prepared_scene_for_emit(&mut graph);
    assert_eq!(prepared.host_parent_terminal, 0);
    assert_eq!(prepared.content_local_terminal, 1);
    assert_eq!(prepared.parent_terminal, 0);
    crate::view::paint::take_artifact_compile_count();

    let frozen = prepared.freeze_content_action(RetainedSurfaceCompileAction::Reraster);
    let (state, staging, trace) = emit_frozen_scroll_scene(frozen, &mut graph, ctx);
    let stamp = staging.single_stamp();

    assert_eq!(
        state.current_target().and_then(|target| target.handle()),
        parent.handle()
    );
    assert_eq!(state.opaque_rect_order(), 0);
    assert_eq!(
        stamp.identity.role,
        RetainedSurfaceRasterRole::ScrollContent
    );
    assert_eq!(trace.action, RetainedSurfaceCompileAction::Reraster);
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 3);
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::ClearPass>()
            .len(),
        1,
        "only the detached content target is cleared by the scene executor"
    );
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len(),
        1
    );
    let declared = graph
        .declared_persistent_texture_keys()
        .collect::<FxHashSet<_>>();
    assert_eq!(declared.len(), 2);
    assert!(declared.contains(&stamp.identity.color_key));
    assert!(declared.contains(&stamp.identity.color_key.depth_stencil().unwrap()));
}

#[test]
fn reuse_skips_content_clear_and_artifact_but_keeps_host_composite_overlay() {
    let mut graph = FrameGraph::new();
    let (prepared, ctx, parent) = prepared_scene_for_emit(&mut graph);
    crate::view::paint::take_artifact_compile_count();

    let frozen = prepared.freeze_content_action(RetainedSurfaceCompileAction::Reuse);
    let (state, staging, trace) = emit_frozen_scroll_scene(frozen, &mut graph, ctx);
    let stamp = staging.single_stamp();

    assert_eq!(
        state.current_target().and_then(|target| target.handle()),
        parent.handle()
    );
    assert_eq!(state.opaque_rect_order(), 0);
    assert_eq!(trace.action, RetainedSurfaceCompileAction::Reuse);
    assert_eq!(
        crate::view::paint::take_artifact_compile_count(),
        2,
        "host-before and overlay compile every frame; content does not"
    );
    assert!(
        graph
            .test_graphics_passes::<crate::view::render_pass::ClearPass>()
            .is_empty()
    );
    let composites =
        graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
    assert_eq!(composites.len(), 1);
    assert!(composites[0].test_snapshot().source_handle.is_some());
    assert!(
        graph
            .declared_persistent_texture_keys()
            .any(|key| key == stamp.identity.color_key),
        "composite source key remains the declared content target"
    );
}

#[test]
fn production_builder_uses_pool_action_and_stages_only_scroll_content() {
    let (arena, root, child, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let mut viewport = Viewport::new();
    viewport.install_scroll_scene_live_authorities_for_test(properties, generations);
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let parent = ctx.allocate_target(&mut graph);
    ctx.set_current_target(parent);
    graph.add_graphics_pass(ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: parent,
        },
    ));
    crate::view::paint::take_artifact_compile_count();

    let outcome = build_scroll_scene_from_pool(&mut viewport, &arena, &[root], &mut graph, ctx)
        .expect("fresh exact scene");
    let (state, trace) = outcome.into_parts();

    assert_eq!(trace.action, RetainedSurfaceCompileAction::Reraster);
    assert_eq!(trace.content_root, child);
    assert_eq!(state.opaque_rect_order(), 0);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, Some(1))
    );
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 3);
    assert_eq!(
        graph.test_graphics_passes::<ClearPass>().len(),
        2,
        "common parent clear plus detached content clear"
    );
    assert_eq!(
        graph
            .declared_persistent_texture_keys()
            .filter(|key| matches!(
                key,
                PersistentTextureKey::Retained {
                    role: RetainedTextureRole::ScrollContentColor
                        | RetainedTextureRole::ScrollContentDepthStencil,
                    ..
                }
            ))
            .count(),
        2
    );
    assert!(
        graph
            .declared_persistent_texture_keys()
            .all(|key| !matches!(
                key,
                PersistentTextureKey::Retained {
                    role: RetainedTextureRole::ScrollHostColor
                        | RetainedTextureRole::ScrollHostDepthStencil,
                    ..
                }
            ))
    );
    viewport.finish_retained_surface_transaction(false);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, None)
    );
}

#[test]
fn successful_scene_commit_releases_the_previous_baked_scroll_host_pair() {
    let (arena, root, _child, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let baked_plan = super::super::super::plan_single_root_scroll_host_surface(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
    )
    .expect("exact baked scroll fixture");
    let old_host_key = crate::view::base_component::scroll_host_layer_stable_key(82_001);
    let mut viewport = Viewport::new();
    viewport.install_scroll_scene_live_authorities_for_test(properties, generations);

    let mut baked_graph = FrameGraph::new();
    let mut baked_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let baked_parent = baked_ctx.allocate_target(&mut baked_graph);
    baked_ctx.set_current_target(baked_parent);
    super::super::super::build_retained_scroll_host_surface_from_pool(
        &mut viewport,
        &baked_plan,
        &mut baked_graph,
        baked_ctx,
    )
    .expect("baked host stages first");
    viewport.finish_retained_surface_transaction(true);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (1, None)
    );

    let mut scene_graph = FrameGraph::new();
    let mut scene_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let scene_parent = scene_ctx.allocate_target(&mut scene_graph);
    scene_ctx.set_current_target(scene_parent);
    build_scroll_scene_from_pool(&mut viewport, &arena, &[root], &mut scene_graph, scene_ctx)
        .expect("detached scene stages replacement");
    viewport.finish_retained_surface_transaction(true);

    assert_eq!(
        viewport.retained_surface_release_log_for_test(),
        &[old_host_key],
        "successful content commit releases the old baked color/depth pair once"
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (1, None)
    );
}

#[test]
fn switching_to_scene_mode_releases_a_committed_baked_scroll_host_pair() {
    let (arena, root, _child, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let baked_plan = super::super::super::plan_single_root_scroll_host_surface(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
    )
    .expect("exact baked scroll fixture");
    let old_host_key = crate::view::base_component::scroll_host_layer_stable_key(82_001);
    let mut viewport = Viewport::new();
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let parent = ctx.allocate_target(&mut graph);
    ctx.set_current_target(parent);
    super::super::super::build_retained_scroll_host_surface_from_pool(
        &mut viewport,
        &baked_plan,
        &mut graph,
        ctx,
    )
    .expect("baked host stages first");
    viewport.finish_retained_surface_transaction(true);

    viewport.set_paint_renderer_mode(
        crate::view::viewport::ViewportPaintRendererMode::RetainedScrollSceneCanary,
    );

    assert_eq!(
        viewport.retained_surface_release_log_for_test(),
        &[old_host_key]
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, None)
    );
}

#[test]
fn fused_live_prepare_rejects_stale_payload_generation_without_graph_mutation() {
    let (arena, root, child, properties, generations) = fixture_at_offset([0.0, 20.0]);
    arena
        .get_mut(child)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .set_background_color_value(Color::rgb(12, 34, 56));
    let graph = FrameGraph::new();
    let before = graph.build_state_snapshot_for_test();

    assert!(matches!(
        prepare_live(&arena, root, &properties, &generations, &graph,),
        Err(ScrollSceneFromLiveError::LiveSnapshotDrift)
    ));
    assert_eq!(graph.build_state_snapshot_for_test(), before);
}

#[test]
fn fused_live_prepare_rejects_scroll_clip_generation_drift_without_graph_mutation() {
    let (arena, root, _child, mut properties, generations) = fixture_at_offset([0.0, 20.0]);
    arena
        .get_mut(root)
        .unwrap()
        .element
        .set_scroll_offset((0.0, 47.0));
    properties.sync(&arena, &[root]);
    let graph = FrameGraph::new();
    let before = graph.build_state_snapshot_for_test();

    assert!(matches!(
        prepare_live(&arena, root, &properties, &generations, &graph,),
        Err(ScrollSceneFromLiveError::LiveSnapshotDrift)
    ));
    assert_eq!(graph.build_state_snapshot_for_test(), before);
}
