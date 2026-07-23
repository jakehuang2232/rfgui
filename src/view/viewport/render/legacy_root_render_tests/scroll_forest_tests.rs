use super::*;

#[test]
fn retained_auto_native_scroll_forest_is_final_retained_authority_without_red_fallbacks() {
    let (arena, roots, properties, generations) =
        crate::view::paint::native_scroll_forest_plan_fixture();
    let ctx = UiBuildContext::new(700, 700, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let captured =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let uncaptured =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, false);
    assert_eq!(
        auto_authority_kind(&captured),
        AutoAuthorityKind::NativeScrollForest
    );
    assert_eq!(
        auto_authority_kind(&captured),
        auto_authority_kind(&uncaptured),
        "debug rejection capture cannot change final forest authority"
    );
    let (plan, trace) = match captured {
        AutoAuthorityDecision::NativeScrollForest { plan, trace } => (plan, trace),
        _ => panic!("six-boundary native forest must select its dedicated authority"),
    };
    assert!(
        matches!(uncaptured, AutoAuthorityDecision::NativeScrollForest { .. }),
        "uncaptured selection must own the same forest plan family"
    );

    let mut viewport = Viewport::new();
    let frame_owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("native forest retained frame owner");
    let prepared = crate::view::paint::prepare_native_scroll_forest_transaction_from_pool(
        &viewport,
        &plan,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    )
    .expect("selected native forest prepares atomically");
    let mut graph = FrameGraph::new();
    let mut emit_ctx = UiBuildContext::new(700, 700, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let frame_target = emit_ctx.allocate_target(&mut graph);
    emit_ctx.set_current_target(frame_target);
    graph.add_graphics_pass(crate::view::render_pass::ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: emit_ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: frame_target,
        },
    ));
    let state = crate::view::paint::emit_prepared_native_scroll_forest_transaction(
        &mut viewport,
        &mut graph,
        emit_ctx,
        prepared,
    );
    assert_eq!(
        state.current_target().unwrap().handle(),
        frame_target.handle()
    );
    assert_eq!(graph.declared_persistent_texture_keys().count(), 12);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true));

    let telemetry = PaintAuthorityTelemetry::from_selection(
        ViewportPaintRendererMode::RetainedAuto,
        &RetainedTransformCanarySelection::NativeScrollForestPrepared,
        Some((AutoAuthorityKind::NativeScrollForest, trace)),
    );
    assert_eq!(
        telemetry.final_authority(),
        PaintAuthorityKind::NativeScrollForest
    );
    assert!(telemetry.fallback_boundary_nodes().is_empty());
    assert!(retained_auto_fallback_overlay_records(&telemetry, &roots).is_empty());
    viewport.scene.node_arena = arena;
    let capture = viewport.build_retained_auto_debug_capture(&telemetry, &roots, true, true);
    assert_eq!(
        capture.frame.selected_authority,
        crate::view::debug::DebugFramePaintAuthority::NativeScrollForest
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
fn retained_auto_malformed_native_scroll_forests_stay_atomic_legacy() {
    for tamper in [
        "custom",
        "transform",
        "effect",
        "deferred",
        "plain-root",
        "clip-generation",
        "scroll-generation",
    ] {
        let (mut arena, mut roots, mut properties, mut generations) =
            crate::view::paint::native_scroll_forest_plan_fixture();
        match tamper {
            "custom" => {
                let leaf = arena.find_by_stable_id(0x12f0_06).unwrap();
                let bounds = arena.get(leaf).unwrap().element.box_model_snapshot();
                *arena.get_mut(leaf).unwrap().element = Box::new(UnknownOverlayHost {
                    id: bounds.node_id,
                    bounds,
                });
                arena.refresh_stable_id_index();
                arena.refresh_subtree_dirty_cache(roots[0]);
                (properties, generations) = synced_paint_state(&arena, &roots);
            }
            "transform" => {
                let wrapper = arena.find_by_stable_id(0x12f0_04).unwrap();
                crate::view::test_support::get_element_mut::<Element>(&arena, wrapper)
                    .set_resolved_transform_for_test(Some(glam::Mat4::from_cols_array(&[
                        1.0,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        1.0,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        1.0,
                        0.0,
                        f32::NAN,
                        0.0,
                        0.0,
                        1.0,
                    ])));
                (properties, generations) = synced_paint_state(&arena, &roots);
            }
            "effect" => {
                let wrapper = arena.find_by_stable_id(0x12f0_04).unwrap();
                crate::view::test_support::get_element_mut::<Element>(&arena, wrapper)
                    .set_opacity(f32::NAN);
                (properties, generations) = synced_paint_state(&arena, &roots);
            }
            "deferred" => {
                let leaf = arena.find_by_stable_id(0x12f0_06).unwrap();
                let mut style = Style::new();
                style.insert(
                    PropertyId::Position,
                    ParsedValue::Position(
                        Position::absolute()
                            .left(Length::px(10.0))
                            .top(Length::px(10.0))
                            .clip(ClipMode::Viewport),
                    ),
                );
                crate::view::test_support::get_element_mut::<Element>(&arena, leaf)
                    .apply_style(style);
                (properties, generations) = synced_paint_state(&arena, &roots);
            }
            "plain-root" => {
                let plain = arena.insert(Node::new(Box::new(Element::new_with_id(
                    0x12f0_ff, 0.0, 0.0, 40.0, 40.0,
                ))));
                roots.push(plain);
                (properties, generations) = synced_paint_state(&arena, &roots);
            }
            "clip-generation" => {
                properties
                    .clips
                    .values_mut()
                    .next()
                    .expect("native forest owns contents clips")
                    .generation = 0;
            }
            "scroll-generation" => {
                properties
                    .scrolls
                    .values_mut()
                    .next()
                    .expect("native forest owns scroll nodes")
                    .generation = 0;
            }
            _ => unreachable!(),
        }
        let ctx = UiBuildContext::new(700, 700, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        let viewport = Viewport::new();
        let pool_before = viewport.retained_surface_transaction_shape_for_test();
        let captured = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
            true,
        );
        let uncaptured = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
            false,
        );
        assert!(
            matches!(&captured, AutoAuthorityDecision::Legacy { .. }),
            "{tamper} must fail closed before forest preparation"
        );
        assert!(
            matches!(&uncaptured, AutoAuthorityDecision::Legacy { .. }),
            "{tamper} debug capture cannot change legacy authority"
        );
        assert!(
            auto_authority_trace(&captured)
                .rejections
                .iter()
                .any(|rejection| matches!(
                    rejection,
                    AutoAuthorityRejection::NativeScrollForestPlan { .. }
                )),
            "{tamper} exposes a dedicated native-forest rejection"
        );
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            pool_before
        );

        let telemetry = telemetry_for_auto_decision(captured);
        assert_eq!(telemetry.final_authority(), PaintAuthorityKind::Legacy);
        let mut viewport = viewport;
        viewport.scene.node_arena = arena;
        let capture =
            viewport.build_retained_auto_debug_capture(&telemetry, &roots, true, true);
        assert_eq!(
            capture.frame.selected_authority,
            crate::view::debug::DebugFramePaintAuthority::Legacy
        );
        assert!(
            capture.frame.statistics.fallback_count > 0,
            "{tamper} exposes red fallback authority"
        );
    }
}

#[test]
fn retained_auto_native_scroll_forest_prepare_tamper_preserves_warm_pool_atomically() {
    let (arena, roots, properties, generations) =
        crate::view::paint::native_scroll_forest_plan_fixture();
    let ctx = UiBuildContext::new(700, 700, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let base_plan = match select_retained_auto_authority(
        &arena,
        &roots,
        &properties,
        &generations,
        &ctx,
        true,
    ) {
        AutoAuthorityDecision::NativeScrollForest { plan, .. } => plan,
        _ => panic!("native forest baseline selection"),
    };
    let mut viewport = Viewport::new();
    let baseline_owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("baseline forest frame stage");
    let baseline = crate::view::paint::prepare_native_scroll_forest_transaction_from_pool(
        &viewport,
        &base_plan,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    )
    .unwrap();
    let mut baseline_graph = FrameGraph::new();
    let mut baseline_ctx =
        UiBuildContext::new(700, 700, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let target = baseline_ctx.allocate_target(&mut baseline_graph);
    baseline_ctx.set_current_target(target);
    baseline_graph.add_graphics_pass(crate::view::render_pass::ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: baseline_ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: target,
        },
    ));
    let _ = crate::view::paint::emit_prepared_native_scroll_forest_transaction(
        &mut viewport,
        &mut baseline_graph,
        baseline_ctx,
        baseline,
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(baseline_owner), true));
    let committed_shape = viewport.retained_surface_transaction_shape_for_test();

    for tamper in ["stamp", "descriptor", "geometry"] {
        let mut plan = base_plan.clone();
        plan.tamper_native_scroll_forest_prepare_seal_for_test(tamper);
        let graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        let pool_before = viewport.retained_surface_transaction_shape_for_test();
        let owner = viewport
            .begin_retained_surface_frame_stage()
            .expect("rejected forest still acquires only its frame owner");
        assert!(
            crate::view::paint::prepare_native_scroll_forest_transaction_from_pool(
                &viewport,
                &plan,
                wgpu::TextureFormat::Bgra8UnormSrgb,
            )
            .is_err(),
            "{tamper} must reject before graph mutation or staging"
        );
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            pool_before
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            committed_shape
        );
        let warm =
            crate::view::paint::prepare_native_scroll_forest_transaction_with_forced_pool_for_test(
                &viewport,
                &base_plan,
                wgpu::TextureFormat::Bgra8UnormSrgb,
            )
            .expect("rejected tamper cannot poison the committed forest pool");
        assert!(
            warm.actions_for_test().values().all(
                |action| *action == crate::view::paint::RetainedSurfaceCompileAction::Reuse
            )
        );
    }
}

#[test]
fn retained_auto_scroll_content_effect_final_authority_is_retained_and_not_red() {
    for (outer_transform, neutral_wrapper) in
        [(false, false), (false, true), (true, false), (true, true)]
    {
        let (arena, root, properties, generations) =
            crate::view::paint::retained_auto_scroll_content_effect_fixture(
                outer_transform,
                neutral_wrapper,
            );
        let roots = vec![root];
        let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
            true,
        );
        assert!(
            matches!(
                decision,
                AutoAuthorityDecision::PropertyBoundaryDagScene { .. }
            ),
            "Phase3 outer={outer_transform} wrapper={neutral_wrapper} must select DAG"
        );
        let telemetry = telemetry_for_auto_decision(decision);
        assert_eq!(
            telemetry.final_authority(),
            PaintAuthorityKind::PropertyScene
        );
        assert!(telemetry.fallback_boundary_nodes().is_empty());
        assert!(retained_auto_fallback_overlay_records(&telemetry, &roots).is_empty());
        // Earlier bounded candidates may reject this topology; those are
        // selection diagnostics, not final red fallback authority.
        assert!(!telemetry.selection_rejections.is_empty());

        let mut viewport = Viewport::new();
        viewport.scene.node_arena = arena;
        let capture =
            viewport.build_retained_auto_debug_capture(&telemetry, &roots, true, true);
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
}

#[test]
fn retained_auto_scroll_content_effect_tamper_and_custom_fail_closed_atomically() {
    for tamper in ["custom", "property", "clip", "generation"] {
        let (mut arena, root, mut properties, generations) =
            crate::view::paint::retained_auto_scroll_content_effect_fixture(true, true);
        match tamper {
            "custom" => {
                let leaf = arena.find_by_stable_id(0xb4_3021).unwrap();
                *arena.get_mut(leaf).unwrap().element = Box::new(UnknownOverlayHost {
                    id: 0xb4_3021,
                    bounds: BoxModelSnapshot {
                        node_id: 0xb4_3021,
                        parent_id: Some(0xb4_3020),
                        x: 12.0,
                        y: -8.0,
                        width: 48.0,
                        height: 24.0,
                        border_radius: 0.0,
                        should_render: true,
                    },
                });
                arena.refresh_stable_id_index();
                arena.refresh_subtree_dirty_cache(root);
            }
            "property" => {
                let effect = arena.find_by_stable_id(0xb4_3020).unwrap();
                properties
                    .states
                    .get_mut(&effect)
                    .unwrap()
                    .descendants
                    .effect = None;
            }
            "clip" => {
                let clip = properties
                    .clips
                    .iter_mut()
                    .find(|(id, _)| {
                        id.role
                            == crate::view::compositor::property_tree::ClipNodeRole::ContentsClip
                    })
                    .map(|(_, clip)| clip)
                    .unwrap();
                clip.generation = 0;
            }
            "generation" => {
                let leaf = arena.find_by_stable_id(0xb4_3021).unwrap();
                crate::view::test_support::get_element_mut::<Element>(&arena, leaf)
                    .set_background_color_value(Color::rgb(9, 19, 29));
                arena.refresh_subtree_dirty_cache(root);
            }
            _ => unreachable!(),
        }
        let roots = vec![root];
        let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let viewport = Viewport::new();
        let pool_before = viewport.compositor.retained_surfaces.clone();
        let graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        let captured = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
            true,
        );
        let uncaptured = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
            false,
        );
        assert!(
            matches!(captured, AutoAuthorityDecision::Legacy { .. }),
            "{tamper}"
        );
        assert!(
            matches!(uncaptured, AutoAuthorityDecision::Legacy { .. }),
            "{tamper}"
        );
        assert!(
            auto_authority_trace(&captured)
                .rejections
                .iter()
                .any(|rejection| matches!(
                    rejection,
                    AutoAuthorityRejection::PropertyBoundaryDagPlan { .. }
                ))
        );
        assert!(auto_authority_trace(&uncaptured).rejections.is_empty());
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(viewport.compositor.retained_surfaces, pool_before);
        assert!(viewport.compositor.pending_retained_surfaces.is_none());
    }
}
