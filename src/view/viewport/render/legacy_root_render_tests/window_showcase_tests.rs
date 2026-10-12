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
