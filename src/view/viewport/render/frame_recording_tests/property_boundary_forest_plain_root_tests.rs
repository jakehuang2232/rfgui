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
) -> (PreparedArtifactSurfaceFrame, usize) {
    let reordered_state;
    let (properties, generations) = if roots == fixture.roots {
        (&fixture.properties, &fixture.generations)
    } else {
        reordered_state = synced_paint_state(&fixture.arena, roots);
        (&reordered_state.0, &reordered_state.1)
    };
    selected_artifact_surface(
        "plain-root property forest",
        &fixture.arena,
        roots,
        properties,
        generations,
        &selection_context(dpr),
    )
}

fn build_selected(viewport: &mut Viewport, frame: PreparedArtifactSurfaceFrame, dpr: f32) -> usize {
    emit_selected_artifact_surface(
        "plain-root property forest",
        viewport,
        frame,
        selection_context(dpr),
    )
}

#[test]
fn retained_auto_selects_artifact_for_plain_roots_in_both_orders() {
    let fixture = fixture();
    for (roots, dpr) in [
        (fixture.roots.clone(), 1.0),
        (fixture.roots.iter().copied().rev().collect::<Vec<_>>(), 2.0),
    ] {
        let (cold, surface_count) = selected(&fixture, &roots, dpr);
        let mut viewport = Viewport::new();
        assert_eq!(surface_count, 4);
        assert_eq!(build_selected(&mut viewport, cold, dpr), 4);
        let (warm, _) = selected(&fixture, &roots, dpr);
        assert_eq!(build_selected(&mut viewport, warm, dpr), 4);
    }
}
