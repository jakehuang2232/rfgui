use super::*;

struct RetainedAutoMultiRootFixture {
    arena: NodeArena,
    roots: Vec<NodeKey>,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn root_element(stable_id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(stable_id, 0.0, 0.0, 118.0, 88.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn multi_root_fixture() -> RetainedAutoMultiRootFixture {
    let mut arena = new_test_arena();
    let root_a = commit_element(
        &mut arena,
        Box::new(root_element(0xf5_2101, Color::rgb(25, 55, 95))),
    );
    let child_a = commit_child(
        &mut arena,
        root_a,
        Box::new(root_element(0xf5_2102, Color::rgb(165, 65, 35))),
    );
    commit_child(
        &mut arena,
        child_a,
        Box::new(root_element(0xf5_2103, Color::rgb(45, 145, 105))),
    );
    let root_b = commit_element(
        &mut arena,
        Box::new(root_element(0xf5_2104, Color::rgb(35, 75, 115))),
    );
    let child_b = commit_child(
        &mut arena,
        root_b,
        Box::new(root_element(0xf5_2105, Color::rgb(135, 75, 155))),
    );
    commit_child(
        &mut arena,
        child_b,
        Box::new(root_element(0xf5_2106, Color::rgb(55, 135, 175))),
    );
    let (measure, place) = constraints();
    for root in [root_a, root_b] {
        measure_and_place(&mut arena, root, measure, place);
    }
    crate::view::test_support::get_element_mut::<Element>(&arena, root_a)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            2.0, 1.0, 0.0,
        ))));
    crate::view::test_support::get_element_mut::<Element>(&arena, child_a).set_opacity(0.58);
    crate::view::test_support::get_element_mut::<Element>(&arena, root_b).set_opacity(0.64);
    crate::view::test_support::get_element_mut::<Element>(&arena, child_b)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            4.0, 1.0, 0.0,
        ))));
    for root in [root_a, root_b] {
        arena.refresh_subtree_dirty_cache(root);
    }
    let roots = vec![root_a, root_b];
    let (properties, generations) = synced_paint_state(&arena, &roots);
    RetainedAutoMultiRootFixture {
        arena,
        roots,
        properties,
        generations,
    }
}

fn selection_context(dpr: f32) -> UiBuildContext {
    UiBuildContext::new(360, 260, wgpu::TextureFormat::Bgra8Unorm, dpr)
}

fn selected_artifact(
    fixture: &RetainedAutoMultiRootFixture,
    dpr: f32,
) -> (RecordedArtifactCandidate, AutoAuthorityTrace, usize) {
    selected_artifact_surface(
        "heterogeneous multi-root forest",
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        &selection_context(dpr),
    )
}

fn build_selected(
    viewport: &mut Viewport,
    candidate: RecordedArtifactCandidate,
    dpr: f32,
) -> usize {
    emit_selected_artifact_surface(
        "heterogeneous multi-root forest",
        viewport,
        candidate,
        selection_context(dpr),
    )
}

#[test]
fn retained_auto_selects_one_artifact_program_for_heterogeneous_roots() {
    let fixture = multi_root_fixture();
    for dpr in [1.0, 2.0] {
        let (candidate, trace, surface_count) = selected_artifact(&fixture, dpr);
        assert!(
            !trace.rejections.iter().any(|rejection| matches!(
                rejection,
                AutoAuthorityRejection::ArtifactPrepare { .. }
            )),
            "selected authority cannot reject itself: {trace:?}",
        );
        let mut viewport = Viewport::new();
        assert_eq!(surface_count, 4);
        assert_eq!(build_selected(&mut viewport, candidate, dpr), 4);
    }
}

#[test]
fn multi_root_debug_is_presented_retained_and_has_no_fallback_overlay() {
    let fixture = multi_root_fixture();
    let (telemetry_candidate, trace, surface_count) = selected_artifact(&fixture, 1.0);
    let telemetry = telemetry_for_auto_decision(AutoAuthorityDecision::Artifact {
        candidate: telemetry_candidate,
        trace,
    });
    assert_eq!(telemetry.final_authority(), PaintAuthorityKind::Artifact);
    assert!(telemetry.fallback_boundary_nodes().is_empty());
    assert!(retained_auto_fallback_overlay_records(&telemetry, &fixture.roots).is_empty());

    let mut viewport = Viewport::new();
    assert_eq!(surface_count, 4);
    let (cold, _, _) = selected_artifact(&fixture, 1.0);
    assert_eq!(build_selected(&mut viewport, cold, 1.0), 4);
    let (warm, _, _) = selected_artifact(&fixture, 1.0);
    assert_eq!(build_selected(&mut viewport, warm, 1.0), 4);

    viewport.scene.node_arena = fixture.arena;
    let capture =
        viewport.build_retained_auto_debug_capture(&telemetry, &fixture.roots, true, true);
    assert_eq!(
        capture.frame.selected_authority,
        crate::view::debug::DebugFramePaintAuthority::Artifact
    );
    assert_eq!(
        capture.frame.disposition,
        crate::view::debug::DebugFrameDisposition::Presented
    );
    assert_eq!(capture.frame.statistics.fallback_count, 0);
    assert!(capture.frame.fallback_stages.is_empty());
    assert!(capture.nodes.iter().all(|node| node.fallbacks.is_empty()));
}
