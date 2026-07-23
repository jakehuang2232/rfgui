use super::*;

#[test]
fn property_effect_scroll_checkpoint_freezes_cutout_geometry_and_effect_neutral_identity() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::EffectScroll);
    let plan = plan_property_scroll_interleave_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("lifecycle-independent E->S schedule");
    let scaffold = plan.property_scroll_planning_scaffold().unwrap();
    assert!(property_scene_plan_is_sealed(&plan));
    assert!(scaffold.receiver_insertions.is_empty());
    assert!(scaffold.effect_receiver_insertions.len() <= 1);
    assert!(matches!(
        scaffold.schedule.steps.as_slice(),
        [
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Effect(_),
                parent: None,
            },
            PropertySceneScheduledStep::ScrollBoundary {
                basis: ScrollCompositeBasis::Effect(_),
                ..
            }
        ]
    ));
}

#[test]
fn property_effect_scroll_checkpoint_rejects_raster_and_marker_drift() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::EffectScroll);
    let build = || {
        plan_property_scroll_interleave_scaffold_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .unwrap()
    };
    let mut schedule = build();
    let scaffold = schedule
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap();
    scaffold.schedule.steps.swap(0, 1);
    assert!(!property_scene_plan_is_sealed(&schedule));
}

#[test]
fn property_transform_effect_scroll_insertion_freezes_nested_receivers_and_stack() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::TransformEffectScroll);
    let plan = plan_property_scroll_interleave_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("exact T->E->S planning scaffold");
    let scaffold = plan.property_scroll_planning_scaffold().unwrap();
    assert!(scaffold.receiver_insertions.is_empty());
    assert!(scaffold.effect_receiver_insertions.is_empty());
    let [insertion] = scaffold.transform_effect_receiver_insertions.as_slice() else {
        panic!("exact T->E->S owns one nested insertion")
    };
    assert!(
        crate::view::paint::compiler::direct_translation_bits(
            insertion.outer_geometry.viewport_transform
        )
        .is_some()
    );
    assert!(
        insertion.outer_geometry.source_bounds.width
            > f32::from_bits(insertion.inner.raster_bounds_bits[2])
    );
    assert_eq!(
        insertion.outer_geometry.source_bounds.y.to_bits(),
        0.0_f32.to_bits()
    );
    assert!(insertion.outer_geometry.source_bounds.height < 240.0);
    assert_eq!(insertion.inner.receiver.parent, None);
    assert_eq!(
        insertion.inner.artifact_contract.live_effect_chain(),
        [insertion.inner.receiver]
    );
    let boundary = &scaffold.boundaries[0];
    assert!(matches!(
        boundary.consumed_properties.entries.as_slice(),
        [
            ConsumedPropertyEntry {
                boundary: ConsumedPropertyBoundary::Transform(_),
                ..
            },
            ConsumedPropertyEntry {
                boundary: ConsumedPropertyBoundary::Effect(_),
                ..
            },
            ConsumedPropertyEntry {
                boundary: ConsumedPropertyBoundary::ScrollContents { .. },
                ..
            }
        ]
    ));
    assert_eq!(
        boundary.consumed_properties.projected_output,
        PropertyTreeState::default()
    );
    assert!(property_scene_plan_is_sealed(&plan));
}

#[test]
fn property_scroll_interleave_scaffold_rejects_scroll_descendant_transform() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
    let error = plan_property_scroll_interleave_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect_err("unsupported interleave must fail closed");
    assert!(error.reasons.iter().any(|reason| matches!(
        reason,
        FramePaintPlanRejection::UnsupportedPropertyInterleave(_)
            | FramePaintPlanRejection::ScrollBoundary(_)
    )));
}

#[test]
fn property_scroll_interleave_scaffold_seals_same_owner_transform_scroll_roles() {
    let (arena, root, properties, generations) = property_scroll_interleave_fixture(
        ScrollInterleaveFixtureShape::CoLocatedTransformScroll,
    );
    let plan = plan_property_scroll_interleave_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("same-owner native T+S scaffold");
    let scaffold = plan.property_scroll_planning_scaffold().unwrap();
    let [insertion] = scaffold.same_owner_transform_scroll_insertions.as_slice() else {
        panic!("one typed same-owner T+S insertion")
    };
    assert!(insertion.is_canonical());
    assert_eq!(insertion.owner, root);
    assert_eq!(insertion.receiver.insertion_index, 0);
    assert_eq!(insertion.receiver.before_span, 0..0);
    assert_eq!(insertion.receiver.after_span, 1..1);
    assert_eq!(insertion.receiver.receiver_opaque_before, 0);
    assert_eq!(insertion.receiver.receiver_opaque_after, 0);
    assert!(scaffold.receiver_insertions.is_empty());
    assert_eq!(scaffold.boundary_dag.nodes.len(), 2);
    assert!(property_boundary_dag_is_canonical(scaffold));
    assert!(property_scene_plan_is_sealed(&plan));
}

#[test]
fn property_scroll_interleave_scaffold_seals_same_owner_effect_scroll_roles() {
    let (arena, root, properties, generations) = same_owner_effect_scroll_fixture();
    let plan = plan_property_scroll_interleave_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("same-owner native E+S scaffold");
    let scaffold = plan.property_scroll_planning_scaffold().unwrap();
    let [insertion] = scaffold.same_owner_effect_scroll_insertions.as_slice() else {
        panic!("one typed same-owner E+S insertion")
    };
    assert!(insertion.is_canonical());
    assert_eq!(insertion.owner, root);
    assert_eq!(insertion.receiver.insertion_index, 0);
    assert_eq!(insertion.receiver.before_span, 0..0);
    assert_eq!(insertion.receiver.after_span, 1..1);
    assert_eq!(insertion.receiver.receiver_opaque_before, 0);
    assert_eq!(insertion.receiver.receiver_opaque_after, 0);
    assert!(scaffold.effect_receiver_insertions.is_empty());
    assert_eq!(scaffold.boundary_dag.nodes.len(), 2);
    assert!(property_boundary_dag_is_canonical(scaffold));
    assert!(property_scene_plan_is_sealed(&plan));
}

#[test]
fn property_scroll_interleave_scaffold_seal_rejects_schedule_stack_and_phase_drift() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::TransformEffectScroll);
    let build = || {
        plan_property_scroll_interleave_scaffold_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("sealed scaffold")
    };
    let mut schedule = build();
    schedule
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .schedule
        .steps
        .swap(0, 1);
    assert!(!property_scene_plan_is_sealed(&schedule));

    let mut stack = build();
    stack
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .boundaries[0]
        .consumed_properties
        .entries[0]
        .projected_after = PropertyTreeState::default();
    assert!(!property_scene_plan_is_sealed(&stack));

    let mut phase = build();
    phase
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .boundaries[0]
        .phase
        .overlay_after
        .phase = PropertyScrollPhaseKind::HostBeforeChildren;
    assert!(!property_scene_plan_is_sealed(&phase));

    let mut incomplete = build();
    let incomplete_scaffold = incomplete
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap();
    incomplete_scaffold
        .transform_effect_receiver_insertions
        .clear();
    incomplete_scaffold
        .planned_transform_effect_receiver_insertions
        .clear();
    assert!(!property_scene_plan_is_sealed(&incomplete));

    let mut reordered_stack = build();
    let reordered_scaffold = reordered_stack
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap();
    reordered_scaffold.boundaries[0]
        .consumed_properties
        .entries
        .swap(0, 1);
    reordered_scaffold.planned_boundaries[0]
        .consumed_properties
        .entries
        .swap(0, 1);
    assert!(!property_scene_plan_is_sealed(&reordered_stack));

    let mut geometry = build();
    let geometry_scaffold = geometry
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap();
    geometry_scaffold.transform_effect_receiver_insertions[0]
        .outer_geometry
        .source_bounds
        .width += 1.0;
    geometry_scaffold.planned_transform_effect_receiver_insertions[0]
        .outer_geometry
        .source_bounds
        .width += 1.0;
    assert!(!property_scene_plan_is_sealed(&geometry));
}

#[test]
fn property_scroll_receiver_insertion_seal_rejects_drop_duplicate_reorder_and_retarget() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::TransformScroll);
    let build = || {
        let mut plan = plan_property_scroll_interleave_scaffold_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .unwrap();
        let scaffold = plan
            .property_scene_seal
            .as_mut()
            .unwrap()
            .scroll_schedule_scaffold
            .as_mut()
            .unwrap();
        let [
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Transform(receiver),
                ..
            },
            PropertySceneScheduledStep::ScrollBoundary {
                boundary_ordinal, ..
            },
        ] = scaffold.schedule.steps.as_slice()
        else {
            panic!("T->S schedule")
        };
        let boundary = &scaffold.boundaries[*boundary_ordinal as usize];
        let artifact = PropertyScrollReceiverArtifactIdentity {
            owner_topology: Vec::new(),
            clip_nodes: Vec::new(),
            effect_nodes: Vec::new(),
            chunks: Vec::new(),
            op_count: 0,
            opaque_count: 0,
        };
        let cutout = super::super::super::PlannedBoundary {
            root: boundary.scroll.owner,
            stable_id: arena
                .get(boundary.scroll.owner)
                .unwrap()
                .element
                .stable_id(),
            kind: super::super::super::PlannedBoundaryKind::Scroll(boundary.scroll.id),
        };
        let insertion = PropertyScrollReceiverInsertionContract {
            scene_root_ordinal: 0,
            receiver: *receiver,
            receiver_stable_id: arena.get(root).unwrap().element.stable_id(),
            scroll_boundary_ordinal: *boundary_ordinal,
            scroll_cutout: cutout,
            insertion_index: 1,
            before_span: 0..1,
            after_span: 2..3,
            receiver_opaque_before: 0,
            receiver_opaque_after: 0,
            recorded_steps: vec![
                PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact.clone()),
                PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(cutout),
                PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact),
            ],
        };
        scaffold.receiver_insertions = vec![insertion.clone()];
        scaffold.planned_receiver_insertions = vec![insertion];
        assert!(property_scene_plan_is_sealed(&plan));
        plan
    };

    let mut dropped = build();
    dropped
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .receiver_insertions
        .clear();
    assert!(!property_scene_plan_is_sealed(&dropped));

    let mut duplicated = build();
    let scaffold = duplicated
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap();
    scaffold
        .receiver_insertions
        .push(scaffold.receiver_insertions[0].clone());
    scaffold.planned_receiver_insertions = scaffold.receiver_insertions.clone();
    assert!(!property_scene_plan_is_sealed(&duplicated));

    let mut reordered = build();
    let insertion = &mut reordered
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .receiver_insertions[0];
    insertion.recorded_steps.swap(0, 1);
    assert!(!property_scene_plan_is_sealed(&reordered));

    let mut retargeted = build();
    let scaffold = retargeted
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap();
    let mut wrong_receiver = scaffold.receiver_insertions[0].receiver;
    wrong_receiver.owner = scaffold.boundaries[0].scroll.owner;
    scaffold.receiver_insertions[0].receiver = wrong_receiver;
    assert!(!property_scene_plan_is_sealed(&retargeted));
}
