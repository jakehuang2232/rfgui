use super::*;

struct RetainedAutoPropertyForestFixture {
    arena: NodeArena,
    roots: Vec<NodeKey>,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn styled_element(id: u64, x: f32, y: f32, width: f32, height: f32, color: Color) -> Element {
    let mut element = Element::new_with_id(id, x, y, width, height);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn effect_transform_fixture(
    stable_id_base: u64,
    neutral_wrapper: bool,
) -> RetainedAutoPropertyForestFixture {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(styled_element(
            stable_id_base + 1,
            0.0,
            0.0,
            120.0,
            90.0,
            Color::rgb(30, 50, 90),
        )),
    );
    let nested_parent = if neutral_wrapper {
        commit_child(
            &mut arena,
            root,
            Box::new(styled_element(
                stable_id_base + 2,
                4.0,
                3.0,
                80.0,
                58.0,
                Color::rgb(40, 70, 100),
            )),
        )
    } else {
        root
    };
    let transform = commit_child(
        &mut arena,
        nested_parent,
        Box::new(styled_element(
            stable_id_base + 3,
            8.0,
            7.0,
            44.0,
            30.0,
            Color::rgb(180, 70, 30),
        )),
    );
    commit_child(
        &mut arena,
        transform,
        Box::new(styled_element(
            stable_id_base + 4,
            2.0,
            2.0,
            12.0,
            9.0,
            Color::rgb(20, 160, 100),
        )),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.5);
    crate::view::test_support::get_element_mut::<Element>(&arena, transform)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            3.0, 2.0, 0.0,
        ))));
    arena.refresh_subtree_dirty_cache(root);
    let roots = vec![root];
    let (properties, generations) = synced_paint_state(&arena, &roots);
    RetainedAutoPropertyForestFixture {
        arena,
        roots,
        properties,
        generations,
    }
}

fn selection_context(dpr: f32) -> UiBuildContext {
    UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, dpr)
}

fn selected_artifact(
    fixture: &RetainedAutoPropertyForestFixture,
    dpr: f32,
) -> (PreparedArtifactSurfaceFrame, usize) {
    selected_artifact_surface(
        "E -> T forest",
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        &selection_context(dpr),
    )
}

fn build_selected(viewport: &mut Viewport, frame: PreparedArtifactSurfaceFrame, dpr: f32) -> usize {
    emit_selected_artifact_surface("E -> T forest", viewport, frame, selection_context(dpr))
}

#[test]
fn retained_auto_dpr1_and_dpr2_stage_stable_artifact_residents_and_never_red() {
    for (dpr, neutral_wrapper) in [(1.0, false), (2.0, true)] {
        let stable_id_base = if neutral_wrapper {
            0xf4_2210
        } else {
            0xf4_2200
        };
        let fixture = effect_transform_fixture(stable_id_base, neutral_wrapper);
        let (_, surface_count) = selected_artifact(&fixture, dpr);

        let mut viewport = Viewport::new();
        let (cold, _) = selected_artifact(&fixture, dpr);
        assert_eq!(build_selected(&mut viewport, cold, dpr), surface_count);
        let (warm, _) = selected_artifact(&fixture, dpr);
        assert_eq!(build_selected(&mut viewport, warm, dpr), surface_count);
    }
}
