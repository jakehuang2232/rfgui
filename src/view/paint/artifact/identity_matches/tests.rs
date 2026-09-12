use super::*;

#[test]
fn shared_shadow_replay_preserves_original_when_a_copy_is_corrupted() {
    let original = PreparedShadowOp::new(
        ShadowMesh::rounded_rect(2.0, 3.0, 16.0, 12.0, 2.0),
        ShadowParams {
            color: [0.0, 0.0, 0.0, 1.0],
            ..Default::default()
        },
    )
    .unwrap();
    let mut copy = original.clone();
    assert!(Arc::ptr_eq(&original.mesh, &copy.mesh));
    assert!(Arc::ptr_eq(
        &original.identity.vertices_bits,
        &copy.identity.vertices_bits
    ));
    assert!(copy.has_canonical_identity());
    Arc::make_mut(&mut copy.mesh).vertices[0][0] += 1.0;
    assert!(!copy.has_canonical_identity());
    assert!(original.has_canonical_identity());

    for value in [-1.0, -0.0, 0.0, 2.0, f32::NAN, f32::INFINITY] {
        let mut changed = original.clone();
        changed.params.blur_radius = value;
        assert_eq!(
            changed.has_canonical_identity(),
            PreparedShadowIdentity::from_parts(&changed.mesh, changed.params).as_ref()
                == Some(&changed.identity)
        );
    }
    let identity = PaintPayloadIdentity::prepared_shadows([&original]);
    assert!(identity.matches_shadows_with_decoration([&original], std::iter::empty()));
    assert!(!identity.matches_shadows_with_decoration(std::iter::empty(), std::iter::empty()));
    assert!(!identity.matches_shadows_with_decoration([&original, &original], std::iter::empty()));
    // Identity comparison alone is deliberately insufficient for a mutated op;
    // canonicality is the separate compiler check asserted above.
    assert!(identity.matches_shadows_with_decoration([&copy], std::iter::empty()));
}
