use super::*;

fn decision(source: u32, outcome: SurfaceMaterializationOutcome) -> SurfaceMaterializationDecision {
    SurfaceMaterializationDecision {
        source: SurfaceDagNodeId(source),
        target: NodeKey::default(),
        nested_target_count: usize::from(source < 2),
        has_own_raster_content: outcome == SurfaceMaterializationOutcome::RetainedOwnRasterContent,
        outcome,
    }
}

#[test]
fn adjacent_pass_through_candidates_keep_one_typed_composite_boundary() {
    let mut sole_nested_child = [Some(SurfaceDagNodeId(1)), Some(SurfaceDagNodeId(2)), None];
    let mut decisions = [
        decision(0, SurfaceMaterializationOutcome::EliminatedPassThrough),
        decision(1, SurfaceMaterializationOutcome::EliminatedPassThrough),
        decision(2, SurfaceMaterializationOutcome::RetainedOwnRasterContent),
    ];

    retain_uncomposed_adjacent_pass_through_boundaries(&mut sole_nested_child, &mut decisions);

    assert_eq!(
        decisions.map(|decision| decision.outcome),
        [
            SurfaceMaterializationOutcome::RetainedUncomposedBoundary,
            SurfaceMaterializationOutcome::EliminatedPassThrough,
            SurfaceMaterializationOutcome::RetainedOwnRasterContent,
        ]
    );
    assert_eq!(sole_nested_child, [None, Some(SurfaceDagNodeId(2)), None]);
}

#[test]
fn materialization_translation_proof_rejects_scale_rotation_perspective_and_nonfinite() {
    for matrix in [
        glam::Mat4::from_scale(glam::Vec3::new(1.0 + f32::EPSILON, 1.0, 1.0)),
        glam::Mat4::from_scale(glam::Vec3::new(2.0, 1.0, 1.0)),
        glam::Mat4::from_rotation_z(0.25),
        glam::Mat4::from_cols(
            glam::Vec4::X,
            glam::Vec4::Y,
            glam::Vec4::new(0.0, 0.0, 1.0, -1.0),
            glam::Vec4::W,
        ),
        glam::Mat4::from_translation(glam::Vec3::new(f32::NAN, 0.0, 0.0)),
        glam::Mat4::from_translation(glam::Vec3::new(0.0, f32::INFINITY, 0.0)),
        glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, 1.0)),
    ] {
        assert_eq!(finite_translation(matrix), None, "{matrix:?}");
    }
    assert_eq!(
        finite_translation(glam::Mat4::from_translation(glam::Vec3::new(
            -9.5, 4.0, 0.0
        ))),
        Some([-9.5, 4.0])
    );
}

#[test]
fn materialization_identity_effect_has_no_isolation_but_opacity_does() {
    let owner = NodeKey::default();
    let id = EffectNodeId(owner);
    let transition = PropertyStateTransition::between(
        PropertyTreeState::default(),
        PropertyTreeState::default(),
    );
    for opacity in [1.0, 0.625, 0.0, f32::NAN] {
        let effects = FxHashMap::from_iter([(
            id,
            EffectNodeSnapshot {
                id,
                owner,
                parent: None,
                opacity,
                generation: 1,
            },
        )]);
        let result = boundary_transfer(
            SurfaceDagNodeKind::Effect(id),
            transition,
            None,
            &FxHashMap::default(),
            &effects,
            &FxHashMap::default(),
        )
        .expect("complete snapshot set");
        if opacity == 1.0 {
            assert!(result.is_ok());
        } else {
            assert_eq!(
                result,
                Err(SurfaceMaterializationOutcome::RetainedIsolation)
            );
        }
    }
}

#[test]
fn materialization_adjacent_chain_snapshot_is_independent_of_source_order() {
    for order in [[0, 1, 2, 3], [3, 2, 1, 0], [1, 3, 0, 2]] {
        let mut children = [None; 4];
        let mut decisions = (0..4)
            .map(|id| decision(id, SurfaceMaterializationOutcome::EliminatedPassThrough))
            .collect::<Vec<_>>();
        for edge in order.windows(2) {
            children[edge[0]] = Some(SurfaceDagNodeId(edge[1] as u32));
        }
        decisions[order[3]].outcome = SurfaceMaterializationOutcome::RetainedOwnRasterContent;
        retain_uncomposed_adjacent_pass_through_boundaries(&mut children, &mut decisions);
        for id in &order[..2] {
            assert_eq!(
                decisions[*id].outcome,
                SurfaceMaterializationOutcome::RetainedUncomposedBoundary
            );
            assert_eq!(children[*id], None);
        }
        assert_eq!(
            decisions[order[2]].outcome,
            SurfaceMaterializationOutcome::EliminatedPassThrough
        );
        assert_eq!(children[order[2]], Some(SurfaceDagNodeId(order[3] as u32)));
    }
}

#[test]
fn materialization_missing_snapshots_are_structural_errors() {
    let owner = NodeKey::default();
    let transition = PropertyStateTransition::between(
        PropertyTreeState::default(),
        PropertyTreeState::default(),
    );
    for kind in [
        SurfaceDagNodeKind::Transform(TransformNodeId(owner)),
        SurfaceDagNodeKind::Effect(EffectNodeId(owner)),
        SurfaceDagNodeKind::ScrollContent {
            scroll: ScrollNodeId(owner),
            contents_clip: ClipNodeId {
                owner,
                role: ClipNodeRole::ContentsClip,
            },
        },
    ] {
        assert_eq!(
            boundary_transfer(
                kind,
                transition,
                None,
                &FxHashMap::default(),
                &FxHashMap::default(),
                &FxHashMap::default()
            ),
            Err(SurfaceDagError::MissingMaterializationSnapshot(kind))
        );
    }
}

#[test]
fn materialization_missing_parent_precedes_nontranslation_retention() {
    let owner = NodeKey::default();
    let mut arena = crate::view::node_arena::NodeArena::new();
    let parent = TransformNodeId(arena.insert(crate::view::node_arena::Node::new(Box::new(
        crate::view::base_component::Element::new_with_id(0xb4_7f01, 0.0, 0.0, 1.0, 1.0),
    ))));
    let id = TransformNodeId(owner);
    let transforms = FxHashMap::from_iter([(
        id,
        TransformNodeSnapshot {
            id,
            owner,
            parent: Some(parent),
            local_matrix: glam::Mat4::from_scale(glam::Vec3::splat(2.0)),
            local_origin: glam::Vec3::ZERO,
            local_generation: 1,
            generation: 1,
            owner_viewport_position: glam::Vec2::ZERO,
            owner_viewport_transform: glam::Mat4::IDENTITY,
        },
    )]);
    let transition = PropertyStateTransition::between(
        PropertyTreeState::default(),
        PropertyTreeState::default(),
    );
    assert_eq!(
        boundary_transfer(
            SurfaceDagNodeKind::Transform(id),
            transition,
            None,
            &transforms,
            &FxHashMap::default(),
            &FxHashMap::default()
        ),
        Err(SurfaceDagError::MissingMaterializationSnapshot(
            SurfaceDagNodeKind::Transform(parent)
        ))
    );
}
