use super::*;

#[derive(Clone, Copy)]
enum RootShape {
    Empty,
    Scroll,
}

fn program_forest_fixture(
    shapes: &[RootShape],
) -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = NodeArena::new();
    let mut roots = Vec::with_capacity(shapes.len());
    for (ordinal, shape) in shapes.iter().copied().enumerate() {
        let stable_id = 0xd7_0000 + ordinal as u64 * 0x10 + 1;
        let root = arena.insert(Node::new(Box::new(Element::new_with_id(
            stable_id, 0.0, 0.0, 120.0, 90.0,
        ))));
        if matches!(shape, RootShape::Scroll) {
            let content = arena.insert(Node::new(Box::new(Element::new_with_id(
                stable_id + 1,
                0.0,
                -20.0,
                120.0,
                240.0,
            ))));
            arena.set_parent(content, Some(root));
            arena.push_child(root, content);
            let mut style = Style::new();
            style.insert(
                PropertyId::ScrollDirection,
                ParsedValue::ScrollDirection(ScrollDirection::Vertical),
            );
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            {
                let mut element =
                    crate::view::test_support::get_element_mut::<Element>(&arena, root);
                element.apply_style(style);
                element.layout_state.content_size = Size {
                    width: 120.0,
                    height: 240.0,
                };
                element.set_scroll_offset((0.0, 20.0));
                element
                    .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
            }
            arena
                .get_mut(content)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena.refresh_subtree_dirty_cache(root);
        roots.push(root);
    }
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    (arena, roots, properties, generations)
}

fn plan_fixture(
    shapes: &[RootShape],
) -> (
    PropertyBoundaryProgramForestPlan,
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (arena, roots, properties, generations) = program_forest_fixture(shapes);
    let plan = plan_property_boundary_program_forest(&arena, &roots, &properties, &generations)
        .expect("fixture must be admitted by the independent M7a collector");
    (plan, arena, roots, properties, generations)
}

fn has_reason(
    result: &Result<PropertyBoundaryProgramForestPlan, PropertyBoundaryProgramPlanError>,
    predicate: impl Fn(&PropertyBoundaryProgramRejection) -> bool,
) -> bool {
    result
        .as_ref()
        .err()
        .is_some_and(|error| error.reasons.iter().any(predicate))
}

#[test]
fn empty_scroll_empty_scroll_has_exact_root_local_programs() {
    let (plan, _, roots, _, _) = plan_fixture(&[
        RootShape::Empty,
        RootShape::Scroll,
        RootShape::Empty,
        RootShape::Scroll,
    ]);
    assert!(property_boundary_program_forest_is_canonical(&plan));
    assert_eq!(plan.roots.len(), 4);
    assert_eq!(plan.nodes.len(), 2);
    assert_eq!(plan.operations.len(), 2);
    assert_eq!(plan.boundaries.len(), 2);
    assert_eq!(plan.residents.len(), 2);
    assert_eq!(plan.roots[0].kind, PropertyBoundaryProgramRootKind::Empty);
    assert_eq!(plan.roots[0].node_span, 0..0);
    assert_eq!(
        plan.roots[1].kind,
        PropertyBoundaryProgramRootKind::FrameRootScroll
    );
    assert_eq!(plan.roots[1].node_span, 0..1);
    assert_eq!(plan.roots[2].kind, PropertyBoundaryProgramRootKind::Empty);
    assert_eq!(plan.roots[2].node_span, 1..1);
    assert_eq!(
        plan.roots[3].kind,
        PropertyBoundaryProgramRootKind::FrameRootScroll
    );
    assert_eq!(plan.roots[3].node_span, 1..2);
    assert_eq!(plan.roots[1].root, roots[1]);
    assert_eq!(plan.roots[3].root, roots[3]);
    assert_eq!(
        plan.nodes[1].receiver,
        PropertyBoundaryProgramReceiver::FrameRoot {
            scene_root_ordinal: 3,
        }
    );
}

#[test]
fn root_permutations_and_multiple_scroll_roots_are_independently_admitted() {
    for shapes in [
        vec![RootShape::Scroll, RootShape::Empty],
        vec![RootShape::Empty, RootShape::Scroll],
        vec![RootShape::Scroll, RootShape::Scroll],
        vec![RootShape::Scroll, RootShape::Empty, RootShape::Scroll],
    ] {
        let (plan, _, _, _, _) = plan_fixture(&shapes);
        assert!(property_boundary_program_forest_is_canonical(&plan));
        assert_eq!(
            plan.nodes.len(),
            shapes
                .iter()
                .filter(|shape| matches!(shape, RootShape::Scroll))
                .count()
        );
    }
}

#[test]
fn duplicate_root_parent_root_topology_and_stable_alias_fail_closed() {
    let (arena, roots, properties, generations) = program_forest_fixture(&[RootShape::Empty]);
    let duplicate = plan_property_boundary_program_forest(
        &arena,
        &[roots[0], roots[0]],
        &properties,
        &generations,
    );
    assert!(has_reason(&duplicate, |reason| matches!(
        reason,
        PropertyBoundaryProgramRejection::DuplicateRoot(_)
    )));

    let (arena, roots, properties, generations) = program_forest_fixture(&[RootShape::Scroll]);
    let child = arena.children_of(roots[0])[0];
    let parent_root =
        plan_property_boundary_program_forest(&arena, &[child], &properties, &generations);
    assert!(has_reason(&parent_root, |reason| matches!(
        reason,
        PropertyBoundaryProgramRejection::RootHasParent(owner) if *owner == child
    )));

    let (mut arena, roots, properties, generations) = program_forest_fixture(&[RootShape::Scroll]);
    arena.set_arena_children_without_mirror_for_test(roots[0], Vec::new());
    let topology = plan_property_boundary_program_forest(&arena, &roots, &properties, &generations);
    assert!(has_reason(&topology, |reason| matches!(
        reason,
        PropertyBoundaryProgramRejection::TopologyMismatch(_)
    )));

    let (arena, roots, properties, generations) =
        program_forest_fixture(&[RootShape::Empty, RootShape::Empty]);
    let duplicate_id = arena.get(roots[0]).unwrap().element.stable_id();
    crate::view::test_support::get_element_mut::<Element>(&arena, roots[1])
        .set_stable_id_for_test(duplicate_id);
    let stable_alias =
        plan_property_boundary_program_forest(&arena, &roots, &properties, &generations);
    assert!(has_reason(&stable_alias, |reason| matches!(
        reason,
        PropertyBoundaryProgramRejection::DuplicateStableId { .. }
    )));
}

#[test]
fn identity_overflow_and_missing_property_state_have_typed_rejections() {
    if let Ok(overflow) = usize::try_from(u64::from(u32::MAX) + 1) {
        for kind in [
            PropertyBoundaryProgramIdentityKind::RootOrdinal,
            PropertyBoundaryProgramIdentityKind::Node,
            PropertyBoundaryProgramIdentityKind::Operation,
            PropertyBoundaryProgramIdentityKind::Boundary,
            PropertyBoundaryProgramIdentityKind::Resident,
        ] {
            assert_eq!(
                property_boundary_program_checked_identity(overflow, kind),
                Err(PropertyBoundaryProgramRejection::IdentityOverflow {
                    kind,
                    value: overflow,
                })
            );
        }
    }

    let (arena, roots, mut properties, generations) = program_forest_fixture(&[RootShape::Empty]);
    properties.states.remove(&roots[0]);
    let missing = plan_property_boundary_program_forest(&arena, &roots, &properties, &generations);
    assert!(has_reason(&missing, |reason| matches!(
        reason,
        PropertyBoundaryProgramRejection::MissingPropertyState(owner)
            if *owner == roots[0]
    )));

    let exact_error = PropertyTreeValidationError::ScrollContractUnavailable(roots[0]);
    properties.validation_errors.push(exact_error);
    let validation =
        plan_property_boundary_program_forest(&arena, &roots, &properties, &generations);
    assert!(has_reason(&validation, |reason| {
        *reason == PropertyBoundaryProgramRejection::PropertyTreeValidation(exact_error)
    }));
}

#[test]
fn empty_root_cannot_hide_payload_and_spans_kind_receiver_are_sealed() {
    let (plan, _, _, _, _) = plan_fixture(&[RootShape::Empty, RootShape::Scroll]);

    let mut hidden = plan.clone();
    hidden.roots[0].node_span = 0..1;
    hidden.planned_roots[0].node_span = 0..1;
    assert!(!property_boundary_program_forest_is_canonical(&hidden));

    let mut span = plan.clone();
    span.roots[1].operation_span = 0..0;
    span.planned_roots[1].operation_span = 0..0;
    assert!(!property_boundary_program_forest_is_canonical(&span));

    let mut boundary_span = plan.clone();
    boundary_span.roots[1].boundary_span = 0..0;
    boundary_span.planned_roots[1].boundary_span = 0..0;
    assert!(!property_boundary_program_forest_is_canonical(
        &boundary_span
    ));

    let mut resident_span = plan.clone();
    resident_span.roots[1].resident_span = 0..0;
    resident_span.planned_roots[1].resident_span = 0..0;
    assert!(!property_boundary_program_forest_is_canonical(
        &resident_span
    ));

    let mut kind = plan.clone();
    kind.roots[1].kind = PropertyBoundaryProgramRootKind::Empty;
    kind.planned_roots[1].kind = PropertyBoundaryProgramRootKind::Empty;
    assert!(!property_boundary_program_forest_is_canonical(&kind));

    let mut receiver = plan.clone();
    receiver.nodes[0].receiver = PropertyBoundaryProgramReceiver::FrameRoot {
        scene_root_ordinal: 0,
    };
    receiver.planned_nodes[0].receiver = receiver.nodes[0].receiver;
    assert!(!property_boundary_program_forest_is_canonical(&receiver));
}

#[test]
fn identity_cross_references_and_cross_root_receivers_are_sealed() {
    let (plan, _, _, _, _) = plan_fixture(&[RootShape::Scroll]);
    for identity in ["node", "operation", "boundary", "resident"] {
        let mut tampered = plan.clone();
        match identity {
            "node" => {
                tampered.nodes[0].id = PropertyBoundaryProgramNodeId(7);
                tampered.planned_nodes[0].id = PropertyBoundaryProgramNodeId(7);
            }
            "operation" => {
                tampered.operations[0].id = PropertyBoundaryProgramOperationId(7);
                tampered.planned_operations[0].id = PropertyBoundaryProgramOperationId(7);
            }
            "boundary" => {
                tampered.boundaries[0].id = PropertyBoundaryProgramBoundaryId(7);
                tampered.planned_boundaries[0].id = PropertyBoundaryProgramBoundaryId(7);
            }
            "resident" => {
                tampered.residents[0].id = PropertyBoundaryProgramResidentId(7);
                tampered.planned_residents[0].id = PropertyBoundaryProgramResidentId(7);
            }
            _ => unreachable!(),
        }
        assert!(
            !property_boundary_program_forest_is_canonical(&tampered),
            "{identity} identity tamper escaped the seal"
        );
    }

    let mut cross_reference = plan.clone();
    cross_reference.operations[0].node = PropertyBoundaryProgramNodeId(7);
    cross_reference.planned_operations[0].node = PropertyBoundaryProgramNodeId(7);
    assert!(!property_boundary_program_forest_is_canonical(
        &cross_reference
    ));

    let (plan, _, _, _, _) = plan_fixture(&[RootShape::Scroll, RootShape::Scroll]);
    for (root_index, receiver_ordinal) in [(0, 1), (1, 0)] {
        let mut cross_root = plan.clone();
        let receiver = PropertyBoundaryProgramReceiver::FrameRoot {
            scene_root_ordinal: receiver_ordinal,
        };
        cross_root.nodes[root_index].receiver = receiver;
        cross_root.operations[root_index].receiver = receiver;
        cross_root.residents[root_index].receiver = receiver;
        cross_root.planned_nodes[root_index].receiver = receiver;
        cross_root.planned_operations[root_index].receiver = receiver;
        cross_root.planned_residents[root_index].receiver = receiver;
        assert!(
            !property_boundary_program_forest_is_canonical(&cross_root),
            "root {root_index} accepted a cross-root receiver"
        );
    }
}

#[test]
fn affine_transform_is_typed_while_mixed_and_nested_scroll_roots_reject() {
    let (arena, roots, _, _) = program_forest_fixture(&[RootShape::Empty]);
    crate::view::test_support::get_element_mut::<Element>(&arena, roots[0])
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::X)));
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    let transform =
        plan_property_boundary_program_forest(&arena, &roots, &properties, &generations)
            .expect("pure affine frame-root transform");
    assert_eq!(
        transform.roots[0].kind,
        PropertyBoundaryProgramRootKind::FrameRootTransformContent
    );
    assert!(property_boundary_program_forest_is_canonical(&transform));

    let (arena, roots, _, _) = program_forest_fixture(&[RootShape::Scroll]);
    crate::view::test_support::get_element_mut::<Element>(&arena, roots[0])
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::X)));
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    let mixed = plan_property_boundary_program_forest(&arena, &roots, &properties, &generations);
    assert!(has_reason(&mixed, |reason| matches!(
        reason,
        PropertyBoundaryProgramRejection::UnsupportedRoot {
            kind: PropertyBoundaryProgramUnsupportedKind::Mixed,
            ..
        }
    )));

    let (arena, root, inner, properties, generations) = nested_scroll_fixture();
    let nested = plan_property_boundary_program_forest(&arena, &[root], &properties, &generations);
    assert!(has_reason(&nested, |reason| matches!(
        reason,
        PropertyBoundaryProgramRejection::UnsupportedRoot {
            owner,
            kind: PropertyBoundaryProgramUnsupportedKind::NestedScroll,
            ..
        } if *owner == inner
    )));
}

#[test]
fn omitted_and_extra_property_coverage_rejects_the_whole_forest() {
    let (arena, roots, mut properties, generations) = program_forest_fixture(&[RootShape::Scroll]);
    properties.scrolls.remove(&ScrollNodeId(roots[0]));
    let omitted = plan_property_boundary_program_forest(&arena, &roots, &properties, &generations);
    assert!(has_reason(&omitted, |reason| matches!(
        reason,
        PropertyBoundaryProgramRejection::PropertyCoverage(_)
    )));

    let (arena, roots, properties, _) =
        program_forest_fixture(&[RootShape::Empty, RootShape::Scroll]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots[..1], &properties);
    let extra =
        plan_property_boundary_program_forest(&arena, &roots[..1], &properties, &generations);
    assert!(has_reason(&extra, |reason| matches!(
        reason,
        PropertyBoundaryProgramRejection::ExtraPropertyState(_)
            | PropertyBoundaryProgramRejection::PropertyCoverage(_)
    )));
}
