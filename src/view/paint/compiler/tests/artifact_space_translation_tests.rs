use super::*;

fn owner() -> NodeKey {
    let mut keys = SlotMap::<NodeKey, ()>::with_key();
    keys.insert(())
}

fn chunk(owner: NodeKey, slot: u16, bounds: [f32; 4]) -> super::super::super::PaintChunk {
    super::super::super::PaintChunk {
        id: super::super::super::PaintChunkId {
            owner,
            scope: PaintPropertyScope::SelfPaint,
            phase: super::super::super::PaintNodePhase::BeforeChildren,
            slot,
            role: PaintChunkRole::SelfDecoration,
        },
        owner,
        op_range: 0..0,
        bounds: crate::view::base_component::Rect {
            x: bounds[0],
            y: bounds[1],
            width: bounds[2],
            height: bounds[3],
        },
        properties: PropertyTreeState::default(),
        content_revision: super::super::super::PaintContentRevision {
            self_paint_revision: 1,
            composite_revision: 1,
            topology_revision: 1,
        },
        payload_identity: PaintPayloadIdentity::None,
    }
}

#[test]
fn artifact_chunk_pairs_derive_one_bitwise_translation() {
    let owner = owner();
    let host_wrapper = chunk(owner, 0, [12.0, 7.0, 40.0, 18.0]);
    let local_wrapper = chunk(owner, 0, [0.0, 0.0, 40.0, 18.0]);
    let host_glyph = chunk(owner, 1, [20.0, 11.0, 8.0, 6.0]);
    let local_glyph = chunk(owner, 1, [8.0, 4.0, 8.0, 6.0]);
    let pairs = [(&host_wrapper, &local_wrapper), (&host_glyph, &local_glyph)];
    let transition = super::super::super::PaintArtifactSpaceTransition::from_bits(
        [12.0_f32.to_bits(), 7.0_f32.to_bits()],
        [0.0_f32.to_bits(), 0.0_f32.to_bits()],
        1,
    )
    .unwrap();

    assert_eq!(
        artifact_chunk_pair_translation_bits(pairs),
        Some([(-12.0_f32).to_bits(), (-7.0_f32).to_bits()]),
    );
    assert!(artifact_space_transition_matches_chunk_pairs(
        transition, pairs,
    ));
}

#[test]
fn artifact_chunk_pair_translation_rejects_incomplete_or_conflicting_evidence() {
    let owner = owner();
    let host = chunk(owner, 0, [12.0, 7.0, 40.0, 18.0]);
    let local = chunk(owner, 0, [0.0, 0.0, 40.0, 18.0]);
    let host_glyph = chunk(owner, 1, [20.0, 11.0, 8.0, 6.0]);
    let conflicting_glyph = chunk(owner, 1, [9.0, 4.0, 8.0, 6.0]);

    assert_eq!(
        artifact_chunk_pair_translation_bits(std::iter::empty::<(
            &super::super::super::PaintChunk,
            &super::super::super::PaintChunk,
        )>(),),
        None,
        "at least one artifact pair is required",
    );
    let transition = super::super::super::PaintArtifactSpaceTransition::from_bits(
        [12.0_f32.to_bits(), 7.0_f32.to_bits()],
        [0.0_f32.to_bits(), 0.0_f32.to_bits()],
        1,
    )
    .unwrap();
    assert!(
        !artifact_space_transition_matches_chunk_pairs(
            transition,
            std::iter::empty::<(
                &super::super::super::PaintChunk,
                &super::super::super::PaintChunk,
            )>(),
        ),
        "the matcher must reject empty evidence independently of transition canonicality",
    );
    assert_eq!(
        artifact_chunk_pair_translation_bits([(&host, &local), (&host_glyph, &conflicting_glyph),]),
        None,
        "all artifact pairs must derive the same bitwise delta",
    );

    let mut wrong_id = local.clone();
    wrong_id.id.slot += 1;
    assert_eq!(
        artifact_chunk_pair_translation_bits([(&host, &wrong_id)]),
        None,
        "chunk identity must match",
    );
    let mut wrong_owner = local.clone();
    wrong_owner.owner = NodeKey::null();
    assert_eq!(
        artifact_chunk_pair_translation_bits([(&host, &wrong_owner)]),
        None,
        "chunk owner must match",
    );
    let mut wrong_size = local.clone();
    wrong_size.bounds.width += 1.0;
    assert_eq!(
        artifact_chunk_pair_translation_bits([(&host, &wrong_size)]),
        None,
        "translation cannot change artifact chunk size",
    );
    let mut nonfinite = host.clone();
    nonfinite.bounds.x = f32::NAN;
    assert_eq!(
        artifact_chunk_pair_translation_bits([(&nonfinite, &local)]),
        None,
        "non-finite artifact evidence must fail closed",
    );
}

#[test]
fn bitwise_artifact_differential_rejects_a_projected_rounding_alias() {
    let owner = owner();
    let host = chunk(owner, 0, [33_554_432.0, 0.0, 1.0, 1.0]);
    let local = host.clone();
    let transition = super::super::super::PaintArtifactSpaceTransition::from_bits(
        [1.0_f32.to_bits(), 0.0_f32.to_bits()],
        [0.0_f32.to_bits(), 0.0_f32.to_bits()],
        1,
    )
    .unwrap();

    assert_eq!(
        transition.project_bounds_bits(chunk_bounds_bits(&host)),
        Some(chunk_bounds_bits(&local)),
        "f32 projection equality is only a proxy at large coordinates",
    );
    assert!(!artifact_space_transition_matches_chunk_pairs(
        transition,
        [(&host, &local)],
    ));
}
