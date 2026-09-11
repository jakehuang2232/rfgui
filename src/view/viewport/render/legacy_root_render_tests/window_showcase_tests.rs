use super::*;

#[test]
fn retained_auto_routes_nested_effects_and_transform_interleave_to_artifact() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, _, child, _) = prepared_nested_opacity_tree();
    let decision = auto_decision(&arena, &roots, &ctx);
    assert_eq!(auto_authority_kind(&decision), AutoAuthorityKind::Artifact);
    assert!(auto_authority_trace(&decision).rejections.is_empty());
    assert_eq!(
        telemetry_for_auto_decision(decision)
            .snapshot()
            .authority_label,
        "retained-auto:artifact"
    );

    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            3.0, 0.0, 0.0,
        ))));
    let rejected = auto_decision(&arena, &roots, &ctx);
    assert_eq!(auto_authority_kind(&rejected), AutoAuthorityKind::Artifact);
    assert!(auto_authority_trace(&rejected).rejections.is_empty());
    assert_eq!(
        telemetry_for_auto_decision(rejected)
            .snapshot()
            .authority_label,
        "retained-auto:artifact"
    );
}

#[test]
fn retained_auto_text_area_zero_and_bounded_scroll_select_artifact_and_invalid_states_legacy() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    for scroll_y in [0.0, 9.0] {
        let (arena, roots, root) = prepared_auto_text_area(scroll_y, false);
        let (properties, _) = synced_paint_state(&arena, &roots);
        assert!(properties.scrolls.is_empty());
        let state = properties.node_state_for(root).unwrap();
        assert_ne!(state.paint.clip, state.descendants.clip);
        let AutoAuthorityDecision::Artifact { candidate, trace } =
            auto_decision(&arena, &roots, &ctx)
        else {
            panic!("bounded TextArea scroll {scroll_y} must select Artifact")
        };
        assert!(candidate.eligibility.eligible);
        assert!(trace.rejections.is_empty());
    }

    let (arena, roots, _) = prepared_auto_text_area(f32::NAN, false);
    {
        let AutoAuthorityDecision::Legacy { trace } = auto_decision(&arena, &roots, &ctx) else {
            panic!("invalid TextArea scroll state must select Legacy")
        };
        assert!(matches!(
            trace.rejections.first(),
            Some(AutoAuthorityRejection::Artifact { eligibility })
                if eligibility.reasons.contains(
                    &crate::view::paint::FrameArtifactFallbackReason::LegacyBoundary(
                        crate::view::paint::LegacyPaintReason::StatefulPaint,
                    )
                )
        ));
    }

    let (arena, roots, _) = prepared_auto_text_area(0.0, true);
    let AutoAuthorityDecision::Artifact { candidate, trace } = auto_decision(&arena, &roots, &ctx)
    else {
        panic!("pending caret-follow is paint-neutral and must select Artifact")
    };
    assert!(candidate.eligibility.eligible);
    assert!(trace.rejections.is_empty());
}
