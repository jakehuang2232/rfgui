use super::*;

#[test]
fn property_scroll_interleave_scaffold_seals_only_supported_planning_shapes() {
    for shape in [
        ScrollInterleaveFixtureShape::FrameRootScroll,
        ScrollInterleaveFixtureShape::TransformScroll,
        ScrollInterleaveFixtureShape::EffectScroll,
        ScrollInterleaveFixtureShape::TransformEffectScroll,
    ] {
        let (arena, root, properties, generations) = property_scroll_interleave_fixture(shape);
        let plan = plan_property_scroll_interleave_scaffold_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("supported B4-0 planning grammar");
        assert!(property_scene_plan_is_sealed(&plan));
        assert!(plan.property_scene_transaction_witness().is_none());
        assert!(plan.property_scene_context().is_none());
        let scaffold = plan
            .property_scene_seal
            .as_ref()
            .and_then(|seal| seal.scroll_schedule_scaffold.as_ref())
            .expect("sealed scroll schedule");
        assert_eq!(scaffold.boundaries.len(), 1);
        let boundary = &scaffold.boundaries[0];
        assert_eq!(
            boundary.phase.host_before.phase,
            PropertyScrollPhaseKind::HostBeforeChildren
        );
        assert_eq!(
            boundary.phase.content_gap.phase,
            PropertyScrollPhaseKind::DetachedContentComposite
        );
        assert_eq!(
            boundary.phase.overlay_after.phase,
            PropertyScrollPhaseKind::OverlayAfterChildren
        );
        assert_eq!(
            boundary.consumed_properties.projected_output,
            PropertyTreeState::default()
        );
    }
}

#[test]
fn property_boundary_dag_projects_existing_fixed_grammars_with_insertion_parity() {
    for (shape, expected_node_count) in [
        (ScrollInterleaveFixtureShape::FrameRootScroll, 1usize),
        (ScrollInterleaveFixtureShape::TransformScroll, 2),
        (ScrollInterleaveFixtureShape::EffectScroll, 2),
        (ScrollInterleaveFixtureShape::TransformEffectScroll, 3),
    ] {
        let (arena, root, properties, generations) = property_scroll_interleave_fixture(shape);
        let plan = plan_property_scroll_interleave_scaffold_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("existing fixed grammar projects into the typed boundary DAG");
        let scaffold = plan.property_scroll_planning_scaffold().unwrap();
        assert!(property_boundary_dag_is_canonical(scaffold));
        assert_eq!(scaffold.boundary_dag.roots.len(), 1);
        assert_eq!(
            scaffold.boundary_dag.roots[0].node_span,
            0..expected_node_count
        );
        assert_eq!(scaffold.boundary_dag.nodes.len(), expected_node_count);

        let boundary = &scaffold.boundaries[0];
        let projected_consumption = scaffold
            .boundary_dag
            .nodes
            .iter()
            .map(|node| node.consumption)
            .collect::<Vec<_>>();
        assert_eq!(projected_consumption, boundary.consumed_properties.entries);
        let scroll_node = scaffold.boundary_dag.nodes.last().unwrap();
        assert_eq!(
            scroll_node.kind,
            PropertyBoundaryDagNodeKind::Scroll(boundary.clone())
        );

        let mut legacy_seals = Vec::new();
        legacy_seals.extend(scaffold.frame_receiver_insertions.iter().map(|insertion| {
            property_boundary_insertion_seal(
                insertion.insertion_index,
                insertion.before_span.clone(),
                insertion.after_span.clone(),
                insertion.receiver_opaque_before,
                insertion.receiver_opaque_after,
                &insertion.recorded_steps,
            )
        }));
        legacy_seals.extend(scaffold.receiver_insertions.iter().map(|insertion| {
            property_boundary_insertion_seal(
                insertion.insertion_index,
                insertion.before_span.clone(),
                insertion.after_span.clone(),
                insertion.receiver_opaque_before,
                insertion.receiver_opaque_after,
                &insertion.recorded_steps,
            )
        }));
        legacy_seals.extend(scaffold.effect_receiver_insertions.iter().map(|insertion| {
            property_boundary_insertion_seal(
                insertion.insertion_index,
                insertion.before_span.clone(),
                insertion.after_span.clone(),
                insertion.receiver_opaque_before,
                insertion.receiver_opaque_after,
                &insertion.recorded_steps,
            )
        }));
        for insertion in &scaffold.transform_effect_receiver_insertions {
            legacy_seals.push(property_boundary_insertion_seal(
                insertion.outer_insertion_index,
                insertion.outer_before_span.clone(),
                insertion.outer_after_span.clone(),
                insertion.outer_opaque_before,
                insertion.outer_opaque_after,
                &insertion.outer_recorded_steps,
            ));
            legacy_seals.push(property_boundary_insertion_seal(
                insertion.inner.insertion_index,
                insertion.inner.before_span.clone(),
                insertion.inner.after_span.clone(),
                insertion.inner.receiver_opaque_before,
                insertion.inner.receiver_opaque_after,
                &insertion.inner.recorded_steps,
            ));
        }
        let dag_seals = scaffold
            .boundary_dag
            .nodes
            .iter()
            .filter_map(|node| match &node.placement {
                PropertyBoundaryDagPlacement::Cutout {
                    sealed: Some(seal), ..
                } => Some(seal.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(dag_seals.len(), legacy_seals.len());
        assert!(dag_seals.iter().all(|seal| legacy_seals.contains(seal)));
        assert!(property_scene_plan_is_sealed(&plan));
    }
}

#[test]
fn property_boundary_dag_seals_effect_transform_scroll_with_neutral_wrapper_order() {
    for (shape, expected_neutral_path) in [
        (ScrollInterleaveFixtureShape::EffectTransformScroll, 0usize),
        (
            ScrollInterleaveFixtureShape::EffectNeutralTransformNeutralScroll,
            1,
        ),
    ] {
        let (arena, root, properties, generations) = property_scroll_interleave_fixture(shape);
        let plan = plan_property_scroll_interleave_scaffold_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("exact E->T->S DAG grammar");
        assert!(property_scene_plan_is_sealed(&plan));
        let scaffold = plan.property_scroll_planning_scaffold().unwrap();
        assert_eq!(
            scaffold.boundary_dag.existing_grammar(),
            Some(PropertyBoundaryDagGrammar::EffectTransformScroll)
        );
        assert!(matches!(
            scaffold.schedule.steps.as_slice(),
            [
                PropertySceneScheduledStep::RetainedSurface {
                    boundary: PropertyScheduledSurfaceBoundary::Effect(_),
                    parent: None,
                },
                PropertySceneScheduledStep::RetainedSurface {
                    boundary: PropertyScheduledSurfaceBoundary::Transform(_),
                    parent: Some(PropertyScheduledSurfaceBoundaryId::Effect(_)),
                },
                PropertySceneScheduledStep::ScrollBoundary {
                    basis: ScrollCompositeBasis::Transform(_),
                    ..
                },
            ]
        ));
        let [insertion] = scaffold.effect_transform_receiver_insertions.as_slice() else {
            panic!("one E->T->S insertion pair")
        };
        assert!(insertion.outer_artifact_contract.is_canonical());
        assert!(insertion.inner_geometry.matches_rebuilt_contract());
        assert_eq!(
            scaffold.boundaries[0]
                .consumed_properties
                .entries
                .iter()
                .map(|entry| entry.boundary)
                .collect::<Vec<_>>(),
            vec![
                ConsumedPropertyBoundary::Effect(insertion.outer_receiver.id),
                ConsumedPropertyBoundary::Transform(insertion.inner.receiver.id),
                ConsumedPropertyBoundary::ScrollContents {
                    scroll: scaffold.boundaries[0].scroll.id,
                    contents_clip: scaffold.boundaries[0].contents_clip.id,
                },
            ]
        );
        let PropertyBoundaryDagPlacement::Cutout {
            neutral_path: outer_path,
            sealed: Some(outer_seal),
            ..
        } = &scaffold.boundary_dag.nodes[1].placement
        else {
            panic!("T is a typed E cutout")
        };
        let PropertyBoundaryDagPlacement::Cutout {
            neutral_path: inner_path,
            sealed: Some(inner_seal),
            ..
        } = &scaffold.boundary_dag.nodes[2].placement
        else {
            panic!("S is a typed T cutout")
        };
        assert_eq!(outer_path.len(), expected_neutral_path);
        assert_eq!(inner_path.len(), expected_neutral_path);
        assert!(property_boundary_insertion_seal_is_canonical(
            outer_seal,
            insertion.transform_cutout
        ));
        assert!(property_boundary_insertion_seal_is_canonical(
            inner_seal,
            insertion.inner.scroll_cutout
        ));
        if expected_neutral_path != 0 {
            assert!(!insertion.outer_before_span.is_empty());
            assert!(!insertion.outer_after_span.is_empty());
            assert!(!insertion.inner.before_span.is_empty());
            assert!(!insertion.inner.after_span.is_empty());
            assert!(
                insertion.outer_artifact_contract.content().len() > 2,
                "E contract retains wrapper self paint and siblings outside T"
            );
        }
    }
}

#[test]
fn property_boundary_dag_seals_scroll_content_effect_receivers() {
    for (outer_transform, neutral_wrapper, grammar, expected_nodes) in [
        (
            false,
            false,
            PropertyBoundaryDagGrammar::ScrollEffect,
            2usize,
        ),
        (false, true, PropertyBoundaryDagGrammar::ScrollEffect, 2),
        (
            true,
            false,
            PropertyBoundaryDagGrammar::TransformScrollEffect,
            3,
        ),
        (
            true,
            true,
            PropertyBoundaryDagGrammar::TransformScrollEffect,
            3,
        ),
    ] {
        let (arena, root, properties, generations) =
            scroll_content_effect_interleave_fixture(outer_transform, neutral_wrapper);
        let plan = plan_property_scroll_interleave_scaffold_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("exact scroll-content effect DAG grammar");
        assert!(property_scene_plan_is_sealed(&plan));
        let scaffold = plan.property_scroll_planning_scaffold().unwrap();
        assert_eq!(scaffold.boundary_dag.existing_grammar(), Some(grammar));
        assert_eq!(scaffold.boundary_dag.nodes.len(), expected_nodes);
        let scroll_index = usize::from(outer_transform);
        let effect_index = scroll_index + 1;
        let scroll_node = &scaffold.boundary_dag.nodes[scroll_index];
        let effect_node = &scaffold.boundary_dag.nodes[effect_index];
        assert_eq!(
            effect_node.receiver,
            PropertyBoundaryReceiverScope::ScrollContent(scroll_node.id)
        );
        assert!(matches!(
            effect_node.consumption.boundary,
            ConsumedPropertyBoundary::Effect(_)
        ));
        assert_eq!(
            effect_node.consumption.expected_before.scroll,
            Some(scaffold.boundaries[0].scroll.id)
        );
        assert_eq!(
            effect_node.consumption.projected_after.scroll,
            Some(scaffold.boundaries[0].scroll.id)
        );
        assert_eq!(
            effect_node.consumption.expected_before.clip,
            Some(scaffold.boundaries[0].contents_clip.id)
        );
        assert_eq!(
            effect_node.consumption.projected_after.clip,
            Some(scaffold.boundaries[0].contents_clip.id)
        );
        let PropertyBoundaryDagPlacement::Cutout {
            neutral_path,
            sealed: Some(seal),
            ..
        } = &effect_node.placement
        else {
            panic!("scroll-content effect is a typed content cutout")
        };
        assert!(property_boundary_insertion_seal_is_canonical(
            seal,
            scaffold.scroll_content_effect_insertions[0].effect_cutout,
        ));
        assert_eq!(neutral_path.len(), 1 + usize::from(neutral_wrapper));
        let insertion = &scaffold.scroll_content_effect_insertions[0];
        if outer_transform {
            let outer = insertion
                .outer_transform
                .as_ref()
                .expect("T->S->E freezes the outer T insertion");
            assert!(outer.geometry.matches_rebuilt_contract());
            assert_eq!(
                outer.receiver.scroll_cutout.root,
                scaffold.boundaries[0].scroll.owner
            );
            assert_eq!(
                outer.receiver.scroll_cutout.kind,
                PlannedBoundaryKind::Scroll(scaffold.boundaries[0].scroll.id)
            );
        } else {
            assert!(insertion.outer_transform.is_none());
        }
    }
}

#[test]
fn property_boundary_dag_scroll_content_effect_scope_and_consumption_tamper_fail_closed() {
    let (arena, root, properties, generations) =
        scroll_content_effect_interleave_fixture(true, true);
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
    let mut scope = build();
    let scaffold = scope
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap();
    scaffold.boundary_dag.nodes[2].receiver = PropertyBoundaryReceiverScope::FrameRoot {
        scene_root_ordinal: 0,
    };
    assert!(!property_scene_plan_is_sealed(&scope));

    let mut consumption = build();
    let scaffold = consumption
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap();
    scaffold.boundary_dag.nodes[2]
        .consumption
        .projected_after
        .scroll = None;
    assert!(!property_scene_plan_is_sealed(&consumption));

    let mut order = build();
    let scaffold = order
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap();
    scaffold.boundary_dag.nodes.swap(1, 2);
    assert!(!property_scene_plan_is_sealed(&order));

    let mut missing_transform_projection = build();
    missing_transform_projection
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .scroll_content_effect_insertions[0]
        .consumed_transform = None;
    assert!(!property_scene_plan_is_sealed(
        &missing_transform_projection
    ));

    let mut marker = build();
    marker
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .scroll_content_effect_insertions[0]
        .effect_cutout
        .stable_id ^= 1;
    assert!(!property_scene_plan_is_sealed(&marker));

    let mut receiver_artifact_order = build();
    let steps = &mut receiver_artifact_order
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .scroll_content_effect_insertions[0]
        .receiver_recorded_steps;
    assert!(steps.len() >= 2);
    steps.swap(0, 1);
    assert!(!property_scene_plan_is_sealed(&receiver_artifact_order));

    let mut missing_effect_artifact = build();
    missing_effect_artifact
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .scroll_content_effect_insertions[0]
        .effect_recorded_steps
        .pop();
    assert!(!property_scene_plan_is_sealed(&missing_effect_artifact));

    let mut outer_geometry = build();
    outer_geometry
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .scroll_content_effect_insertions[0]
        .outer_transform
        .as_mut()
        .unwrap()
        .geometry
        .source_bounds
        .width += 1.0;
    assert!(!property_scene_plan_is_sealed(&outer_geometry));

    let mut outer_cutout = build();
    outer_cutout
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .scroll_content_effect_insertions[0]
        .outer_transform
        .as_mut()
        .unwrap()
        .receiver
        .scroll_cutout
        .stable_id ^= 1;
    assert!(!property_scene_plan_is_sealed(&outer_cutout));

    let mut outer_span = build();
    outer_span
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .scroll_content_effect_insertions[0]
        .outer_transform
        .as_mut()
        .unwrap()
        .receiver
        .after_span
        .start += 1;
    assert!(!property_scene_plan_is_sealed(&outer_span));
}

#[test]
fn property_scroll_content_effect_rejects_unprepared_inline_outer_transform() {
    let (arena, root, mut properties, mut generations) =
        scroll_content_effect_interleave_fixture(true, false);
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        element.replace_style(Style::new());
        element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
            glam::Vec3::new(7.0, 0.0, 0.0),
        )));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);

    let error = plan_property_scroll_interleave_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect_err("unprepared inline T receiver must remain fail-closed");
    assert!(error.reasons.contains(&FramePaintPlanRejection::Coverage(
        FrameArtifactFallbackReason::LegacyBoundary(
            super::super::super::LegacyPaintReason::MissingPreparedInlineRoot,
        ),
    )));
}

#[test]
fn property_boundary_dag_rejects_unprepared_inline_neutral_wrapper_with_typed_blocker() {
    let (arena, root, mut properties, mut generations) = property_scroll_interleave_fixture(
        ScrollInterleaveFixtureShape::EffectNeutralTransformNeutralScroll,
    );
    let wrapper = arena
        .find_by_stable_id(0xb4_0021)
        .expect("outer neutral wrapper");
    {
        let mut element =
            crate::view::test_support::get_element_mut::<Element>(&arena, wrapper);
        element.replace_style(Style::new());
        element.set_background_color_value(Color::rgb(12, 24, 36));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);

    let error = plan_property_scroll_interleave_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect_err("unprepared inline wrapper must remain fail-closed");
    assert!(error.reasons.contains(&FramePaintPlanRejection::Coverage(
        FrameArtifactFallbackReason::LegacyBoundary(
            super::super::super::LegacyPaintReason::MissingPreparedInlineRoot,
        ),
    )));
}

#[test]
fn property_effect_transform_scroll_tamper_matrix_effect_clip_wrapper_and_order_fail_closed() {
    let (arena, root, properties, generations) = property_scroll_interleave_fixture(
        ScrollInterleaveFixtureShape::EffectNeutralTransformNeutralScroll,
    );
    let build = || {
        plan_property_scroll_interleave_scaffold_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("sealed E->T->S scaffold")
    };

    let mut matrix = build();
    matrix
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .effect_transform_receiver_insertions[0]
        .inner
        .receiver
        .viewport_matrix
        .w_axis
        .x += 1.0;
    assert!(!property_scene_plan_is_sealed(&matrix));

    let mut effect = build();
    effect
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .effect_transform_receiver_insertions[0]
        .outer_receiver
        .opacity = 0.75;
    assert!(!property_scene_plan_is_sealed(&effect));

    let mut clip = build();
    clip.property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .boundaries[0]
        .contents_clip
        .logical_scissor[0] ^= 1;
    assert!(!property_scene_plan_is_sealed(&clip));

    let mut wrapper_path = build();
    let scaffold = wrapper_path
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap();
    let PropertyBoundaryDagPlacement::Cutout { neutral_path, .. } =
        &mut scaffold.boundary_dag.nodes[1].placement
    else {
        panic!("T boundary cutout")
    };
    neutral_path.push(PropertyBoundaryPathOwnerWitness {
        owner: root,
        stable_id: arena.get(root).unwrap().element.stable_id(),
    });
    assert!(!property_scene_plan_is_sealed(&wrapper_path));

    let mut order = build();
    let insertion = &mut order
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .effect_transform_receiver_insertions[0];
    assert!(insertion.outer_recorded_steps.len() >= 3);
    insertion.outer_recorded_steps.swap(0, 1);
    assert!(!property_scene_plan_is_sealed(&order));

    let mut geometry = build();
    let scaffold = geometry
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap();
    scaffold.effect_transform_receiver_insertions[0]
        .inner_geometry
        .source_bounds
        .width += 1.0;
    scaffold.planned_effect_transform_receiver_insertions[0]
        .inner_geometry
        .source_bounds
        .width += 1.0;
    assert!(!property_scene_plan_is_sealed(&geometry));
}

#[test]
fn property_boundary_dag_rejects_receiver_consumption_marker_and_path_tamper() {
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
        .unwrap()
    };

    let mut receiver = build();
    let scaffold = receiver
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap();
    let self_id = scaffold.boundary_dag.nodes[1].id;
    scaffold.boundary_dag.nodes[1].receiver = PropertyBoundaryReceiverScope::Surface(self_id);
    assert!(!property_scene_plan_is_sealed(&receiver));

    let mut consumption = build();
    let scaffold = consumption
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap();
    scaffold.boundary_dag.nodes[0].consumption.projected_after =
        scaffold.boundary_dag.nodes[0].consumption.expected_before;
    assert!(!property_scene_plan_is_sealed(&consumption));

    let mut marker_plan = build();
    let scaffold = marker_plan
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap();
    let PropertyBoundaryDagPlacement::Cutout {
        marker: dag_marker, ..
    } = &mut scaffold.boundary_dag.nodes[1].placement
    else {
        panic!("nested effect is represented by a cutout")
    };
    dag_marker.kind = PlannedBoundaryKind::Scroll(scaffold.boundaries[0].scroll.id);
    assert!(!property_scene_plan_is_sealed(&marker_plan));

    let mut neutral_path_plan = build();
    let scaffold = neutral_path_plan
        .property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap();
    let PropertyBoundaryDagPlacement::Cutout { neutral_path, .. } =
        &mut scaffold.boundary_dag.nodes[2].placement
    else {
        panic!("scroll boundary is represented by a cutout")
    };
    neutral_path.push(PropertyBoundaryPathOwnerWitness {
        owner: root,
        stable_id: 0,
    });
    assert!(!property_scene_plan_is_sealed(&neutral_path_plan));

    let mut span = build();
    span.property_scene_seal
        .as_mut()
        .unwrap()
        .scroll_schedule_scaffold
        .as_mut()
        .unwrap()
        .boundary_dag
        .roots[0]
        .node_span = 0..2;
    assert!(!property_scene_plan_is_sealed(&span));
}
