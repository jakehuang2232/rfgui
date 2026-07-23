use super::*;

#[test]
fn nested_scroll_planner_seals_exact_graph_inert_schedule_and_state_projection() {
    let (arena, outer, inner, leaf, properties, generations) = nested_scroll_plan_fixture();
    let plan = plan_nested_scroll_scene_scaffold_with_context(
        &arena,
        &[outer],
        &properties,
        &generations,
        1.0,
        TransformSurfacePlanContext::default(),
    )
    .expect("exact nested-scroll planning scaffold");
    assert!(property_scene_plan_is_sealed(&plan));
    assert!(plan.steps().is_empty());
    assert!(plan.property_scene_transaction_witness().is_none());
    assert!(plan.property_scene_context().is_none());
    assert!(plan.property_scroll_planning_scaffold().is_none());
    assert!(plan.property_scroll_receiver_insertions().is_none());
    assert!(plan.property_effect_scroll_receiver_insertions().is_none());
    assert!(
        plan.property_transform_effect_scroll_receiver_insertions()
            .is_none()
    );
    let scaffold = plan
        .nested_scroll_planning_scaffold()
        .expect("dedicated nested-scroll seal");
    assert_eq!(scaffold.boundaries.len(), 2);
    assert_eq!(scaffold.boundaries[0].boundary_root, outer);
    assert_eq!(scaffold.boundaries[0].parent, None);
    assert_eq!(scaffold.boundaries[1].boundary_root, inner);
    assert_eq!(
        scaffold.boundaries[1].parent,
        Some(NestedScrollBoundarySlot::Outer)
    );
    let outer_state = scaffold.boundaries[0].content_state;
    let inner_state = scaffold.boundaries[1].content_state;
    assert_eq!(
        scaffold.boundaries[0].projected_receiver_state,
        PropertyTreeState::default()
    );
    assert_eq!(scaffold.boundaries[1].projected_receiver_state, outer_state);
    assert!(matches!(
        scaffold.schedule.steps.as_slice(),
        [
            NestedScrollSceneScheduledStep::HostBefore {
                boundary: NestedScrollBoundarySlot::Outer,
                ..
            },
            NestedScrollSceneScheduledStep::HostBefore {
                boundary: NestedScrollBoundarySlot::Inner,
                ..
            },
            NestedScrollSceneScheduledStep::ContentReceiver(receiver),
            NestedScrollSceneScheduledStep::OverlayAfter {
                boundary: NestedScrollBoundarySlot::Inner,
                ..
            },
            NestedScrollSceneScheduledStep::OverlayAfter {
                boundary: NestedScrollBoundarySlot::Outer,
                ..
            },
        ] if receiver.witness.content_root() == leaf
            && receiver.live_input == inner_state
            && receiver.projected_output == outer_state
    ));
}

#[test]
fn nested_scroll_planner_accepts_retained_baseline_baseline_and_rejects_context_or_property_expansion()
 {
    let (arena, outer, _inner, _leaf, properties, generations) = nested_scroll_plan_fixture();
    let plan = |arena: &NodeArena,
                properties: &PropertyTrees,
                generations: &PaintGenerationTracker,
                scale_factor: f32,
                context: TransformSurfacePlanContext| {
        plan_nested_scroll_scene_scaffold_with_context(
            arena,
            &[outer],
            properties,
            generations,
            scale_factor,
            context,
        )
    };
    assert!(
        plan(
            &arena,
            &properties,
            &generations,
            2.0,
            TransformSurfacePlanContext::default(),
        )
        .is_err()
    );
    assert!(
        plan(
            &arena,
            &properties,
            &generations,
            1.0,
            TransformSurfacePlanContext::new([1.0, 0.0], None),
        )
        .is_err()
    );
    assert!(
        plan(
            &arena,
            &properties,
            &generations,
            1.0,
            TransformSurfacePlanContext::new([0.0, 0.0], Some([0, 0, 10, 10])),
        )
        .is_err()
    );
    let baseline = plan(
        &arena,
        &properties,
        &generations,
        1.0,
        TransformSurfacePlanContext::default(),
    )
    .expect("retained-compatible retained nested-scroll baseline");
    assert!(property_scene_plan_is_sealed(&baseline));

    for expansion in 0..4 {
        let (mut arena, outer, inner, leaf, mut properties, mut generations) =
            nested_scroll_plan_fixture();
        match expansion {
            0 => crate::view::test_support::get_element_mut::<Element>(&arena, leaf)
                .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                    glam::Vec3::new(1.0, 0.0, 0.0),
                ))),
            1 => crate::view::test_support::get_element_mut::<Element>(&arena, leaf)
                .set_opacity(0.5),
            2 => {
                let sibling = arena.insert(Node::new(Box::new(Element::new_with_id(
                    0x1251_03, 10.0, 20.0, 10.0, 10.0,
                ))));
                arena.set_parent(sibling, Some(inner));
                arena.push_child(inner, sibling);
            }
            3 => {
                let mut style = Style::new();
                style.insert(
                    PropertyId::ScrollDirection,
                    ParsedValue::ScrollDirection(ScrollDirection::Vertical),
                );
                style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
                let mut leaf =
                    crate::view::test_support::get_element_mut::<Element>(&arena, leaf);
                leaf.apply_style(style);
                leaf.layout_state.content_size.height = 900.0;
            }
            _ => unreachable!(),
        }
        arena.refresh_subtree_dirty_cache(outer);
        properties.sync(&arena, &[outer]);
        generations.sync(&arena, &[outer], &properties);
        assert!(
            plan_nested_scroll_scene_scaffold_with_context(
                &arena,
                &[outer],
                &properties,
                &generations,
                1.0,
                TransformSurfacePlanContext::default(),
            )
            .is_err(),
            "property/topology expansion {expansion} must fail closed"
        );
    }
}

#[test]
fn nested_scroll_seal_rejects_schedule_parent_generation_and_admission_drift() {
    let (arena, outer, _inner, _leaf, properties, generations) = nested_scroll_plan_fixture();
    let build = || {
        plan_nested_scroll_scene_scaffold_with_context(
            &arena,
            &[outer],
            &properties,
            &generations,
            1.0,
            TransformSurfacePlanContext::default(),
        )
        .unwrap()
    };
    fn scaffold(plan: &mut FramePaintPlan) -> &mut NestedScrollSceneScaffold {
        plan.property_scene_seal
            .as_mut()
            .unwrap()
            .nested_scroll_scaffold
            .as_mut()
            .unwrap()
    }

    let mut reordered = build();
    let nested = scaffold(&mut reordered);
    nested.schedule.steps.swap(0, 1);
    nested.planned_schedule.steps.swap(0, 1);
    assert!(!property_scene_plan_is_sealed(&reordered));

    let mut dropped = build();
    let nested = scaffold(&mut dropped);
    nested.schedule.steps.pop();
    nested.planned_schedule.steps.pop();
    assert!(!property_scene_plan_is_sealed(&dropped));

    let mut duplicated = build();
    let nested = scaffold(&mut duplicated);
    let duplicate = nested.schedule.steps[0].clone();
    nested.schedule.steps.insert(1, duplicate.clone());
    nested.planned_schedule.steps.insert(1, duplicate);
    assert!(!property_scene_plan_is_sealed(&duplicated));

    let mut retargeted = build();
    let nested = scaffold(&mut retargeted);
    let NestedScrollSceneScheduledStep::HostBefore { boundary, .. } =
        &mut nested.schedule.steps[0]
    else {
        unreachable!()
    };
    *boundary = NestedScrollBoundarySlot::Inner;
    nested.planned_schedule = nested.schedule.clone();
    assert!(!property_scene_plan_is_sealed(&retargeted));

    let mut parent = build();
    let nested = scaffold(&mut parent);
    nested.boundaries[1].parent = None;
    nested.planned_boundaries[1].parent = None;
    assert!(!property_scene_plan_is_sealed(&parent));

    let mut generation = build();
    let nested = scaffold(&mut generation);
    nested.boundaries[1].scroll.generation = 0;
    nested.planned_boundaries[1].scroll.generation = 0;
    assert!(!property_scene_plan_is_sealed(&generation));

    let mut stable = build();
    scaffold(&mut stable).boundaries[1].stable_id += 1;
    assert!(!property_scene_plan_is_sealed(&stable));

    let mut admission = build();
    let nested = scaffold(&mut admission);
    nested.admission.outer_source_bounds.x += 0.5;
    nested.planned_admission.outer_source_bounds.x += 0.5;
    assert!(!property_scene_plan_is_sealed(&admission));
}

#[test]
fn nested_scroll_seal_rejects_artifact_and_receiver_identity_drift() {
    let (arena, outer, _inner, _leaf, properties, generations) = nested_scroll_plan_fixture();
    let build = || {
        plan_nested_scroll_scene_scaffold_with_context(
            &arena,
            &[outer],
            &properties,
            &generations,
            1.0,
            TransformSurfacePlanContext::default(),
        )
        .unwrap()
    };
    fn scaffold(plan: &mut FramePaintPlan) -> &mut NestedScrollSceneScaffold {
        plan.property_scene_seal
            .as_mut()
            .unwrap()
            .nested_scroll_scaffold
            .as_mut()
            .unwrap()
    }

    let mut artifact_plan = build();
    let nested = scaffold(&mut artifact_plan);
    let NestedScrollSceneScheduledStep::HostBefore {
        artifact: identity, ..
    } = &mut nested.schedule.steps[0]
    else {
        unreachable!()
    };
    identity.identity.op_count += 1;
    nested.planned_schedule = nested.schedule.clone();
    assert!(!property_scene_plan_is_sealed(&artifact_plan));

    let mut opaque = build();
    let nested = scaffold(&mut opaque);
    let NestedScrollSceneScheduledStep::HostBefore { artifact, .. } =
        &mut nested.schedule.steps[0]
    else {
        unreachable!()
    };
    artifact.identity.opaque_count = if artifact.identity.opaque_count == 0 {
        1
    } else {
        0
    };
    nested.planned_schedule = nested.schedule.clone();
    assert!(!property_scene_plan_is_sealed(&opaque));

    let mut topology = build();
    let nested = scaffold(&mut topology);
    let NestedScrollSceneScheduledStep::HostBefore { artifact, .. } =
        &mut nested.schedule.steps[0]
    else {
        unreachable!()
    };
    artifact.identity.owner_topology.push(PaintOwnerSnapshot {
        owner: nested.admission.inner_boundary_root,
        parent: None,
    });
    nested.planned_schedule = nested.schedule.clone();
    assert!(!property_scene_plan_is_sealed(&topology));

    let mut receiver_plan = build();
    let nested = scaffold(&mut receiver_plan);
    let NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
        &mut nested.schedule.steps[2]
    else {
        unreachable!()
    };
    receiver.projected_output = PropertyTreeState::default();
    nested.planned_schedule = nested.schedule.clone();
    assert!(!property_scene_plan_is_sealed(&receiver_plan));

    let mut receiver_artifact = build();
    let nested = scaffold(&mut receiver_artifact);
    let NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
        &mut nested.schedule.steps[2]
    else {
        unreachable!()
    };
    receiver.artifact.identity.chunks[0].properties = PropertyTreeState::default();
    nested.planned_schedule = nested.schedule.clone();
    assert!(!property_scene_plan_is_sealed(&receiver_artifact));

    let mut revision = build();
    let nested = scaffold(&mut revision);
    let NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
        &mut nested.schedule.steps[2]
    else {
        unreachable!()
    };
    receiver.artifact.identity.chunks[0]
        .content_revision
        .topology_revision += 1;
    nested.planned_schedule = nested.schedule.clone();
    assert!(!property_scene_plan_is_sealed(&revision));

    let mut clip_snapshot = build();
    let nested = scaffold(&mut clip_snapshot);
    let NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
        &mut nested.schedule.steps[2]
    else {
        unreachable!()
    };
    receiver.artifact.identity.clip_nodes[0].generation += 1;
    nested.planned_schedule = nested.schedule.clone();
    assert!(!property_scene_plan_is_sealed(&clip_snapshot));

    let mut effect_snapshot = build();
    let nested = scaffold(&mut effect_snapshot);
    let NestedScrollSceneScheduledStep::HostBefore { artifact, .. } =
        &mut nested.schedule.steps[0]
    else {
        unreachable!()
    };
    artifact.identity.effect_nodes.push(EffectNodeSnapshot {
        id: EffectNodeId(nested.admission.content_leaf),
        owner: nested.admission.content_leaf,
        parent: None,
        opacity: 1.0,
        generation: 1,
    });
    nested.planned_schedule = nested.schedule.clone();
    assert!(!property_scene_plan_is_sealed(&effect_snapshot));

    let mut payload = build();
    let nested = scaffold(&mut payload);
    let NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
        &mut nested.schedule.steps[2]
    else {
        unreachable!()
    };
    receiver.artifact.identity.chunks[0].payload_identity =
        if receiver.artifact.identity.chunks[0].payload_identity
            == crate::view::paint::PaintPayloadIdentity::None
        {
            crate::view::paint::PaintPayloadIdentity::PreparedTexts(Arc::from([]))
        } else {
            crate::view::paint::PaintPayloadIdentity::None
        };
    nested.planned_schedule = nested.schedule.clone();
    assert!(!property_scene_plan_is_sealed(&payload));

    let mut duplicate_chunk = build();
    let nested = scaffold(&mut duplicate_chunk);
    let NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
        &mut nested.schedule.steps[2]
    else {
        unreachable!()
    };
    receiver
        .artifact
        .identity
        .chunks
        .push(receiver.artifact.identity.chunks[0].clone());
    nested.planned_schedule = nested.schedule.clone();
    assert!(!property_scene_plan_is_sealed(&duplicate_chunk));
}
