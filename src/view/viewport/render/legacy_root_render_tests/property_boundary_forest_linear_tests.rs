use super::*;

#[derive(Clone, Copy)]
enum LinearRole {
    Transform,
    Effect,
}

struct RetainedAutoLinearFixture {
    arena: NodeArena,
    roots: Vec<NodeKey>,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
    surface_count: usize,
}

fn linear_element(id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(id, 0.0, 0.0, 120.0, 90.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn linear_fixture(
    roles: &[LinearRole],
    neutral_wrappers: bool,
    stable_id_base: u64,
) -> RetainedAutoLinearFixture {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(linear_element(stable_id_base + 1, Color::rgb(25, 55, 95))),
    );
    let mut boundaries = vec![root];
    let mut parent = root;
    let mut next_id = stable_id_base + 1;
    for ordinal in 1..roles.len() {
        if neutral_wrappers {
            next_id += 1;
            parent = commit_child(
                &mut arena,
                parent,
                Box::new(linear_element(
                    next_id,
                    Color::rgb(45, 75 + ordinal as u8, 105),
                )),
            );
        }
        next_id += 1;
        parent = commit_child(
            &mut arena,
            parent,
            Box::new(linear_element(
                next_id,
                Color::rgb(165, 65 + ordinal as u8, 35),
            )),
        );
        boundaries.push(parent);
    }
    commit_child(
        &mut arena,
        parent,
        Box::new(linear_element(next_id + 1, Color::rgb(30, 155, 105))),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    for (ordinal, (&owner, role)) in boundaries.iter().zip(roles).enumerate() {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, owner);
        match role {
            LinearRole::Transform => {
                element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                    glam::Vec3::new(2.0 + ordinal as f32, 1.0, 0.0),
                )));
            }
            LinearRole::Effect => element.set_opacity(0.45 + ordinal as f32 * 0.04),
        }
    }
    arena.refresh_subtree_dirty_cache(root);
    let roots = vec![root];
    let (properties, generations) = synced_paint_state(&arena, &roots);
    RetainedAutoLinearFixture {
        arena,
        roots,
        properties,
        generations,
        surface_count: roles.len(),
    }
}

fn context(dpr: f32) -> UiBuildContext {
    UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, dpr)
}

fn build_selected(
    viewport: &mut Viewport,
    candidate: RecordedArtifactCandidate,
    dpr: f32,
) -> usize {
    emit_selected_artifact_surface(
        "arbitrary-depth linear forest",
        viewport,
        candidate,
        context(dpr),
    )
}

#[test]
fn retained_auto_linear_depth_four_and_five_are_retained_and_never_red() {
    use LinearRole::{Effect, Transform};
    for (roles, neutral, dpr, stable_id_base) in [
        (
            vec![Transform, Effect, Transform, Effect],
            false,
            1.0,
            0xf4_8100,
        ),
        (
            vec![Transform, Effect, Transform, Effect],
            true,
            2.0,
            0xf4_8200,
        ),
        (
            vec![Effect, Transform, Effect, Transform, Effect],
            false,
            1.0,
            0xf4_8300,
        ),
        (
            vec![Effect, Transform, Effect, Transform, Effect],
            true,
            2.0,
            0xf4_8400,
        ),
    ] {
        let fixture = linear_fixture(&roles, neutral, stable_id_base);
        let (telemetry_candidate, trace, surface_count) = selected_artifact_surface(
            "arbitrary-depth linear forest",
            &fixture.arena,
            &fixture.roots,
            &fixture.properties,
            &fixture.generations,
            &context(dpr),
        );
        assert_eq!(surface_count, fixture.surface_count);
        assert!(
            !trace.rejections.iter().any(|rejection| matches!(
                rejection,
                AutoAuthorityRejection::ArtifactPrepare { .. }
            )),
            "selected authority cannot reject itself: {trace:?}",
        );
        let telemetry = telemetry_for_auto_decision(AutoAuthorityDecision::Artifact {
            candidate: telemetry_candidate,
            trace,
        });
        assert_eq!(telemetry.final_authority(), PaintAuthorityKind::Artifact);
        assert!(telemetry.fallback_boundary_nodes().is_empty());
        assert!(retained_auto_fallback_overlay_records(&telemetry, &fixture.roots).is_empty());

        let mut viewport = Viewport::new();
        let (cold, _, _) = selected_artifact_surface(
            "arbitrary-depth linear forest",
            &fixture.arena,
            &fixture.roots,
            &fixture.properties,
            &fixture.generations,
            &context(dpr),
        );
        assert_eq!(build_selected(&mut viewport, cold, dpr), surface_count);
        let (warm, _, _) = selected_artifact_surface(
            "arbitrary-depth linear forest",
            &fixture.arena,
            &fixture.roots,
            &fixture.properties,
            &fixture.generations,
            &context(dpr),
        );
        assert_eq!(build_selected(&mut viewport, warm, dpr), surface_count);

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
}
