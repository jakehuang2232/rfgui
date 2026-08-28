use super::*;

#[derive(Clone, Copy)]
enum BranchRole {
    Transform,
    Effect,
}

struct RetainedAutoBranchFixture {
    arena: NodeArena,
    roots: Vec<NodeKey>,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn branch_element(id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(id, 0.0, 0.0, 120.0, 90.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn apply_role(arena: &NodeArena, owner: NodeKey, role: BranchRole, ordinal: usize) {
    let mut element = crate::view::test_support::get_element_mut::<Element>(arena, owner);
    match role {
        BranchRole::Transform => {
            element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                glam::Vec3::new(2.0 + ordinal as f32, 1.0, 0.0),
            )));
        }
        BranchRole::Effect => element.set_opacity(0.48 + ordinal as f32 * 0.08),
    }
}

fn branch_fixture(
    root_role: BranchRole,
    child_role: BranchRole,
    neutral_wrappers: bool,
    stable_id_base: u64,
) -> RetainedAutoBranchFixture {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(branch_element(stable_id_base + 1, Color::rgb(25, 55, 95))),
    );
    let mut next_id = stable_id_base + 1;
    let mut branches = Vec::new();
    for ordinal in 0..2 {
        let parent = if neutral_wrappers {
            next_id += 1;
            commit_child(
                &mut arena,
                root,
                Box::new(branch_element(
                    next_id,
                    Color::rgb(45, 85 + ordinal as u8 * 10, 115),
                )),
            )
        } else {
            root
        };
        next_id += 1;
        let branch = commit_child(
            &mut arena,
            parent,
            Box::new(branch_element(
                next_id,
                Color::rgb(165, 65 + ordinal as u8 * 10, 35),
            )),
        );
        next_id += 1;
        commit_child(
            &mut arena,
            branch,
            Box::new(branch_element(
                next_id,
                Color::rgb(30, 145 + ordinal as u8 * 10, 105),
            )),
        );
        branches.push(branch);
    }
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    apply_role(&arena, root, root_role, 0);
    for (ordinal, branch) in branches.into_iter().enumerate() {
        apply_role(&arena, branch, child_role, ordinal + 1);
    }
    arena.refresh_subtree_dirty_cache(root);
    let roots = vec![root];
    let (properties, generations) = synced_paint_state(&arena, &roots);
    RetainedAutoBranchFixture {
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
    fixture: &RetainedAutoBranchFixture,
    dpr: f32,
) -> (RecordedArtifactCandidate, AutoAuthorityTrace, usize) {
    selected_artifact_surface(
        "single-root alternating branch",
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
        "single-root alternating branch",
        viewport,
        candidate,
        selection_context(dpr),
    )
}

#[test]
fn retained_auto_branching_direct_and_neutral_both_select_artifact() {
    use BranchRole::{Effect, Transform};
    for (root_role, child_role, neutral, dpr, stable_id_base) in [
        (Transform, Effect, false, 1.0, 0xf4_9100),
        (Transform, Effect, true, 2.0, 0xf4_9200),
        (Effect, Transform, false, 1.0, 0xf4_9300),
        (Effect, Transform, true, 2.0, 0xf4_9400),
    ] {
        let fixture = branch_fixture(root_role, child_role, neutral, stable_id_base);
        let (_, trace, _) = selected_artifact(&fixture, dpr);
        assert!(
            !trace.rejections.iter().any(|rejection| matches!(
                rejection,
                AutoAuthorityRejection::ArtifactPrepare { .. }
            )),
            "selected authority cannot reject itself: {trace:?}",
        );
    }
}

#[test]
fn retained_auto_branching_debug_is_artifact_retained_and_never_red() {
    use BranchRole::{Effect, Transform};
    for (root_role, child_role, neutral, dpr, stable_id_base) in [
        (Transform, Effect, false, 1.0, 0xf4_9500),
        (Transform, Effect, true, 2.0, 0xf4_9600),
        (Effect, Transform, false, 1.0, 0xf4_9700),
        (Effect, Transform, true, 2.0, 0xf4_9800),
    ] {
        let fixture = branch_fixture(root_role, child_role, neutral, stable_id_base);
        let (telemetry_candidate, trace, surface_count) = selected_artifact(&fixture, dpr);
        let telemetry = telemetry_for_auto_decision(AutoAuthorityDecision::Artifact {
            candidate: telemetry_candidate,
            trace,
        });
        assert_eq!(telemetry.final_authority(), PaintAuthorityKind::Artifact);
        assert!(telemetry.fallback_boundary_nodes().is_empty());
        assert!(retained_auto_fallback_overlay_records(&telemetry, &fixture.roots).is_empty());

        let mut viewport = Viewport::new();
        let (cold, _, _) = selected_artifact(&fixture, dpr);
        assert_eq!(build_selected(&mut viewport, cold, dpr), surface_count);
        let (warm, _, _) = selected_artifact(&fixture, dpr);
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
