use super::*;

#[derive(Clone, Copy)]
enum DepthThreeTopology {
    TransformEffectTransform,
    EffectTransformEffect,
}

struct RetainedAutoDepthThreeFixture {
    arena: NodeArena,
    roots: Vec<NodeKey>,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn depth_three_element(id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(id, 0.0, 0.0, 120.0, 90.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn depth_three_fixture(
    topology: DepthThreeTopology,
    neutral_wrappers: bool,
    stable_id_base: u64,
) -> RetainedAutoDepthThreeFixture {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(depth_three_element(
            stable_id_base + 1,
            Color::rgb(25, 55, 95),
        )),
    );
    let middle_parent = if neutral_wrappers {
        commit_child(
            &mut arena,
            root,
            Box::new(depth_three_element(
                stable_id_base + 2,
                Color::rgb(45, 75, 105),
            )),
        )
    } else {
        root
    };
    let middle = commit_child(
        &mut arena,
        middle_parent,
        Box::new(depth_three_element(
            stable_id_base + 3,
            Color::rgb(165, 65, 35),
        )),
    );
    let leaf_parent = if neutral_wrappers {
        commit_child(
            &mut arena,
            middle,
            Box::new(depth_three_element(
                stable_id_base + 4,
                Color::rgb(70, 85, 115),
            )),
        )
    } else {
        middle
    };
    let leaf = commit_child(
        &mut arena,
        leaf_parent,
        Box::new(depth_three_element(
            stable_id_base + 5,
            Color::rgb(185, 75, 30),
        )),
    );
    commit_child(
        &mut arena,
        leaf,
        Box::new(depth_three_element(
            stable_id_base + 6,
            Color::rgb(30, 155, 105),
        )),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let set_transform = |arena: &NodeArena, owner, x| {
        crate::view::test_support::get_element_mut::<Element>(arena, owner)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                x, 2.0, 0.0,
            ))));
    };
    match topology {
        DepthThreeTopology::TransformEffectTransform => {
            set_transform(&arena, root, 2.0);
            crate::view::test_support::get_element_mut::<Element>(&arena, middle).set_opacity(0.55);
            set_transform(&arena, leaf, 5.0);
        }
        DepthThreeTopology::EffectTransformEffect => {
            crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.55);
            set_transform(&arena, middle, 4.0);
            crate::view::test_support::get_element_mut::<Element>(&arena, leaf).set_opacity(0.7);
        }
    }
    arena.refresh_subtree_dirty_cache(root);
    let roots = vec![root];
    let (properties, generations) = synced_paint_state(&arena, &roots);
    RetainedAutoDepthThreeFixture {
        arena,
        roots,
        properties,
        generations,
    }
}

fn selection_context(dpr: f32) -> UiBuildContext {
    UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, dpr)
}

fn selected_property_scene(
    fixture: &RetainedAutoDepthThreeFixture,
    dpr: f32,
) -> (crate::view::paint::FramePaintPlan, AutoAuthorityTrace) {
    let AutoAuthorityDecision::PropertyScene { plan, trace } = select_retained_auto_authority(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        &selection_context(dpr),
        true,
    ) else {
        panic!("depth-three alternating forest must select PropertyScene")
    };
    (plan, trace)
}

fn build_selected(
    viewport: &mut Viewport,
    plan: &crate::view::paint::FramePaintPlan,
    dpr: f32,
) -> crate::view::paint::RetainedPropertySceneBuildTrace {
    let mut graph = FrameGraph::new();
    let mut ctx = selection_context(dpr);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    let outcome = crate::view::paint::build_retained_property_scene_with_forced_pool_for_test(
        viewport, plan, &mut graph, ctx,
    )
    .expect("selected depth-three forest executes");
    outcome.into_parts().1
}

#[test]
fn retained_auto_depth_three_direct_and_neutral_both_select_property_scene() {
    for (topology, neutral, stable_id_base) in [
        (
            DepthThreeTopology::TransformEffectTransform,
            false,
            0xf4_5100,
        ),
        (
            DepthThreeTopology::TransformEffectTransform,
            true,
            0xf4_5200,
        ),
        (DepthThreeTopology::EffectTransformEffect, false, 0xf4_5300),
        (DepthThreeTopology::EffectTransformEffect, true, 0xf4_5400),
    ] {
        let fixture = depth_three_fixture(topology, neutral, stable_id_base);
        let (_, trace) = selected_property_scene(&fixture, 1.0);
        assert!(
            !trace.rejections.iter().any(|rejection| matches!(
                rejection,
                AutoAuthorityRejection::Plan {
                    authority: AutoAuthorityKind::PropertyScene,
                    ..
                }
            )),
            "selected authority cannot reject itself: {trace:?}",
        );
    }
}

#[test]
fn retained_auto_depth_three_dpr1_dpr2_cold_warm_debug_is_retained_and_never_red() {
    for (topology, neutral, dpr, stable_id_base) in [
        (
            DepthThreeTopology::TransformEffectTransform,
            false,
            1.0,
            0xf4_5500,
        ),
        (
            DepthThreeTopology::TransformEffectTransform,
            true,
            2.0,
            0xf4_5600,
        ),
        (
            DepthThreeTopology::EffectTransformEffect,
            false,
            1.0,
            0xf4_5700,
        ),
        (
            DepthThreeTopology::EffectTransformEffect,
            true,
            2.0,
            0xf4_5800,
        ),
    ] {
        let fixture = depth_three_fixture(topology, neutral, stable_id_base);
        let (plan, trace) = selected_property_scene(&fixture, dpr);
        let telemetry = telemetry_for_auto_decision(AutoAuthorityDecision::PropertyScene {
            plan: plan.clone(),
            trace,
        });
        assert_eq!(
            telemetry.final_authority(),
            PaintAuthorityKind::PropertyScene
        );
        assert!(telemetry.fallback_boundary_nodes().is_empty());
        assert!(retained_auto_fallback_overlay_records(&telemetry, &fixture.roots).is_empty());

        let mut viewport = Viewport::new();
        let cold = build_selected(&mut viewport, &plan, dpr);
        assert_eq!((cold.reraster_count, cold.reuse_count), (3, 0));
        viewport.finish_retained_surface_transaction(true);
        let warm = build_selected(&mut viewport, &plan, dpr);
        assert_eq!((warm.reraster_count, warm.reuse_count), (0, 3));
        assert_eq!(viewport.retained_surface_transaction_shape_for_test().0, 3);
        viewport.finish_retained_surface_transaction(false);

        viewport.scene.node_arena = fixture.arena;
        let capture =
            viewport.build_retained_auto_debug_capture(&telemetry, &fixture.roots, true, true);
        assert_eq!(
            capture.frame.selected_authority,
            crate::view::debug::DebugFramePaintAuthority::PropertyScene
        );
        assert_eq!(
            capture.frame.disposition,
            crate::view::debug::DebugFrameDisposition::Presented
        );
        assert_eq!(capture.frame.statistics.fallback_count, 0);
        assert!(capture.frame.fallback_stages.is_empty());
        assert!(capture.nodes.iter().all(|node| node.fallbacks.is_empty()));
    }
}
