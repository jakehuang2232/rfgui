use super::*;

struct RetainedAutoPlainRootFixture {
    arena: NodeArena,
    roots: Vec<NodeKey>,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn element(stable_id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(stable_id, 0.0, 0.0, 108.0, 80.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn fixture() -> RetainedAutoPlainRootFixture {
    let mut arena = new_test_arena();
    let plain_before = commit_element(
        &mut arena,
        Box::new(element(0xf5_6101, Color::rgb(35, 75, 115))),
    );
    let property_a = commit_element(
        &mut arena,
        Box::new(element(0xf5_6102, Color::rgb(25, 55, 95))),
    );
    let child_a = commit_child(
        &mut arena,
        property_a,
        Box::new(element(0xf5_6103, Color::rgb(165, 65, 35))),
    );
    let plain_between = commit_element(
        &mut arena,
        Box::new(element(0xf5_6104, Color::rgb(45, 125, 85))),
    );
    let property_b = commit_element(
        &mut arena,
        Box::new(element(0xf5_6105, Color::rgb(125, 75, 155))),
    );
    let child_b = commit_child(
        &mut arena,
        property_b,
        Box::new(element(0xf5_6106, Color::rgb(55, 135, 175))),
    );
    let plain_after = commit_element(
        &mut arena,
        Box::new(element(0xf5_6107, Color::rgb(145, 95, 45))),
    );
    let roots = vec![
        plain_before,
        property_a,
        plain_between,
        property_b,
        plain_after,
    ];
    let (measure, place) = constraints();
    for &root in &roots {
        measure_and_place(&mut arena, root, measure, place);
    }
    crate::view::test_support::get_element_mut::<Element>(&arena, property_a)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            2.0, 1.0, 0.0,
        ))));
    crate::view::test_support::get_element_mut::<Element>(&arena, child_a).set_opacity(0.57);
    crate::view::test_support::get_element_mut::<Element>(&arena, property_b).set_opacity(0.63);
    crate::view::test_support::get_element_mut::<Element>(&arena, child_b)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            4.0, 1.0, 0.0,
        ))));
    for &root in &roots {
        arena.refresh_subtree_dirty_cache(root);
    }
    let (properties, generations) = synced_paint_state(&arena, &roots);
    RetainedAutoPlainRootFixture {
        arena,
        roots,
        properties,
        generations,
    }
}

fn selection_context(dpr: f32) -> UiBuildContext {
    UiBuildContext::new(360, 260, wgpu::TextureFormat::Bgra8Unorm, dpr)
}

fn selected(
    fixture: &RetainedAutoPlainRootFixture,
    roots: &[NodeKey],
    dpr: f32,
) -> (crate::view::paint::FramePaintPlan, AutoAuthorityTrace) {
    let reordered_state;
    let (properties, generations) = if roots == fixture.roots {
        (&fixture.properties, &fixture.generations)
    } else {
        reordered_state = synced_paint_state(&fixture.arena, roots);
        (&reordered_state.0, &reordered_state.1)
    };
    let AutoAuthorityDecision::PropertyScene { plan, trace } = select_retained_auto_authority(
        &fixture.arena,
        roots,
        properties,
        generations,
        &selection_context(dpr),
        true,
    ) else {
        panic!("plain roots must remain inside the joint PropertyScene")
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
    crate::view::paint::build_retained_property_scene_with_forced_pool_for_test(
        viewport, plan, &mut graph, ctx,
    )
    .unwrap()
    .into_parts()
    .1
}

#[test]
fn retained_auto_selects_property_scene_for_plain_roots_in_both_orders() {
    let fixture = fixture();
    for (roots, dpr) in [
        (fixture.roots.clone(), 1.0),
        (fixture.roots.iter().copied().rev().collect::<Vec<_>>(), 2.0),
    ] {
        let (plan, trace) = selected(&fixture, &roots, dpr);
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
        let mut viewport = Viewport::new();
        let build = build_selected(&mut viewport, &plan, dpr);
        assert_eq!((build.root_count, build.surface_count), (5, 4));
        viewport.finish_retained_surface_transaction(false);
    }
}

#[test]
fn plain_root_debug_is_presented_retained_without_any_fallback_overlay() {
    let fixture = fixture();
    let (plan, trace) = selected(&fixture, &fixture.roots, 1.0);
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
    let cold = build_selected(&mut viewport, &plan, 1.0);
    assert_eq!((cold.root_count, cold.surface_count), (5, 4));
    assert_eq!((cold.reraster_count, cold.reuse_count), (4, 0));
    viewport.finish_retained_surface_transaction(true);
    let warm = build_selected(&mut viewport, &plan, 1.0);
    assert_eq!((warm.reraster_count, warm.reuse_count), (0, 4));
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
