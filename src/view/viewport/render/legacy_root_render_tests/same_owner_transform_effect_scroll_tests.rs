// Existing bridge preflight/dispatch contracts remain protected directly.
// General Auto selection is asserted in generic_selection_tests.
use super::*;

#[test]
fn compatibility_fully_same_owner_transform_effect_scroll_is_retained_and_not_red() {
    let (arena, roots, _, _) = prepared_same_owner_transform_scroll_scene();
    let root = roots[0];
    crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.625);
    arena.refresh_subtree_dirty_cache(root);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);

    let AutoAuthorityDecision::TransformEffectScrollScene { scene, trace } =
        compatibility_decision(&arena, &roots, &properties, &generations, &ctx, true)
    else {
        panic!("fully same-owner T+E+S must select retained transform-effect-scroll authority")
    };
    assert!(scene.is_canonical());
    assert!(
        !trace.rejections.is_empty(),
        "earlier candidate rejections must remain observable"
    );
    assert!(
        trace.rejections.iter().all(|rejection| !matches!(
            rejection,
            AutoAuthorityRejection::TransformEffectScrollPlan { .. }
        )),
        "the selected authority cannot reject itself: {trace:?}"
    );

    let telemetry =
        telemetry_for_auto_decision(AutoAuthorityDecision::TransformEffectScrollScene {
            scene,
            trace,
        });
    assert_eq!(
        telemetry.final_authority(),
        PaintAuthorityKind::PropertyScene
    );
    assert!(telemetry.fallback_boundary_nodes().is_empty());
    assert!(retained_auto_fallback_overlay_records(&telemetry, &roots).is_empty());

    let mut viewport = Viewport::new();
    viewport.scene.node_arena = arena;
    let capture = viewport.build_retained_auto_debug_capture(&telemetry, &roots, true, true);
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
