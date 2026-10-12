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
) -> (PreparedArtifactSurfaceFrame, usize) {
    selected_artifact_surface(
        "heterogeneous multi-root forest",
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        &selection_context(dpr),
    )
}

fn build_selected(viewport: &mut Viewport, frame: PreparedArtifactSurfaceFrame, dpr: f32) -> usize {
    emit_selected_artifact_surface(
        "heterogeneous multi-root forest",
        viewport,
        frame,
        selection_context(dpr),
    )
}

#[test]
fn retained_auto_selects_one_artifact_program_for_heterogeneous_roots() {
    let fixture = multi_root_fixture();
    for dpr in [1.0, 2.0] {
        let (cold, surface_count) = selected_artifact(&fixture, dpr);
        let mut viewport = Viewport::new();
        assert_eq!(surface_count, 4);
        assert_eq!(build_selected(&mut viewport, cold, dpr), 4);
        let (warm, _) = selected_artifact(&fixture, dpr);
        assert_eq!(build_selected(&mut viewport, warm, dpr), 4);
    }
}
