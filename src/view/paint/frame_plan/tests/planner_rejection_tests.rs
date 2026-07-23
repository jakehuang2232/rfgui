use super::*;

#[test]
fn planner_rejects_nested_subroot_and_arena_trait_topology_drift() {
    let (arena, root, properties, generations) = exact_transform_fixture();
    let child = arena
        .get(root)
        .expect("root")
        .element
        .children()
        .first()
        .copied()
        .expect("child");
    let nested =
        plan_single_root_transform_surface(&arena, &[child], &properties, &generations)
            .expect_err("an arena child cannot masquerade as a frame root");
    assert!(
        nested
            .reasons
            .contains(&FramePaintPlanRejection::RootHasParent(child))
    );

    let (mut parent_drift_arena, parent_drift_root, properties, generations) =
        exact_transform_fixture();
    let parent_drift_child = parent_drift_arena
        .get(parent_drift_root)
        .expect("root")
        .element
        .children()[0];
    parent_drift_arena.set_parent(parent_drift_child, None);
    let parent_drift = plan_single_root_transform_surface(
        &parent_drift_arena,
        &[parent_drift_root],
        &properties,
        &generations,
    )
    .expect_err("foreign child parent edge must fail closed");
    assert!(
        parent_drift
            .reasons
            .contains(&FramePaintPlanRejection::TopologyMismatch(
                parent_drift_child
            ))
    );

    let (mut mirror_arena, mirror_root, properties, generations) = exact_transform_fixture();
    mirror_arena.set_arena_children_without_mirror_for_test(mirror_root, Vec::new());
    let mirror = plan_single_root_transform_surface(
        &mirror_arena,
        &[mirror_root],
        &properties,
        &generations,
    )
    .expect_err("arena/trait child mirror drift must fail closed");
    assert!(
        mirror
            .reasons
            .contains(&FramePaintPlanRejection::TopologyMismatch(mirror_root))
    );
}

#[test]
fn planner_rejects_zero_stable_id_for_root_or_descendant() {
    let (arena, root, properties, generations) = exact_transform_fixture();
    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_stable_id_for_test(0);
    let root_error =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect_err("stable id zero cannot own a persistent transform surface");
    assert!(
        root_error
            .reasons
            .contains(&FramePaintPlanRejection::InvalidStableId(root))
    );

    let (arena, root, properties, generations) = exact_transform_fixture();
    let child = arena.get(root).expect("root").element.children()[0];
    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_stable_id_for_test(0);
    let child_error =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect_err("every reachable owner requires a nonzero paint identity");
    assert!(
        child_error
            .reasons
            .contains(&FramePaintPlanRejection::InvalidStableId(child))
    );
}

#[test]
fn planner_accepts_retained_baseline_baseline_and_rejects_property_identity_or_root_shape_boundaries()
 {
    let (arena, root, properties, generations) = exact_transform_fixture();
    let child = arena.get(root).expect("root").element.children()[0];
    let multi_root =
        plan_single_root_transform_surface(&arena, &[root, child], &properties, &generations)
            .expect_err("M10C1 is exact single-root only");
    assert_eq!(
        multi_root.reasons,
        vec![FramePaintPlanRejection::RootCount(2)]
    );

    let baseline =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("retained-compatible retained transform baseline");
    let _ = only_surface(&baseline);

    let (arena, root, properties, generations) = exact_transform_fixture();
    let child = arena.get(root).expect("root").element.children()[0];
    let root_id = arena.get(root).expect("root").element.stable_id();
    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_stable_id_for_test(root_id);
    let duplicate =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect_err("duplicate nonzero stable ids cannot prove owning identity");
    assert!(
        duplicate
            .reasons
            .contains(&FramePaintPlanRejection::DuplicateStableId(root_id))
    );

    for property in ["clip", "effect", "scroll"] {
        let (arena, root, mut properties, generations) = exact_transform_fixture();
        let child = arena.get(root).expect("root").element.children()[0];
        let state = properties.states.get_mut(&child).expect("child state");
        match property {
            "clip" => {
                state.paint.clip = Some(crate::view::compositor::property_tree::ClipNodeId {
                    owner: child,
                    role: crate::view::compositor::property_tree::ClipNodeRole::SelfClip,
                });
            }
            "effect" => {
                state.paint.effect =
                    Some(crate::view::compositor::property_tree::EffectNodeId(child));
            }
            "scroll" => {
                state.paint.scroll =
                    Some(crate::view::compositor::property_tree::ScrollNodeId(child));
            }
            _ => unreachable!(),
        }
        let error =
            plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
                .expect_err("non-transform property authority must stay out of M10C1");
        assert!(error.reasons.iter().any(|reason| match (property, reason) {
            ("clip", FramePaintPlanRejection::ClipBoundary(owner))
            | ("effect", FramePaintPlanRejection::EffectBoundary(owner))
            | ("scroll", FramePaintPlanRejection::ScrollBoundary(owner)) => *owner == child,
            _ => false,
        }));
    }
}

#[test]
fn planner_explicitly_rejects_transformed_descendant_before_execution() {
    let (arena, root, mut properties, generations) = exact_transform_fixture();
    let child = arena.get(root).expect("root").element.children()[0];
    let root_transform = properties.transforms[&TransformNodeId(root)];
    properties.transforms.insert(
        TransformNodeId(child),
        crate::view::compositor::property_tree::TransformNode {
            owner: child,
            parent: Some(TransformNodeId(root)),
            viewport_matrix: root_transform.viewport_matrix,
            generation: 1,
        },
    );
    properties
        .states
        .get_mut(&child)
        .expect("child state")
        .paint
        .transform = Some(TransformNodeId(child));
    let nested = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect_err("a second transform boundary requires recursive planning in a later slice");
    assert!(
        nested
            .reasons
            .contains(&FramePaintPlanRejection::WrongTransformBoundary(child))
    );
    assert_eq!(
        nested.reasons,
        vec![
            FramePaintPlanRejection::UnexpectedTransform(child),
            FramePaintPlanRejection::WrongTransformBoundary(child),
        ],
        "an undeclared nested surface and its incomplete property scope must both fail closed before plan ownership is built"
    );
}

#[test]
fn planner_rejects_nonfinite_parented_and_wrong_transform_boundaries() {
    let (arena, root, mut properties, generations) = exact_transform_fixture();
    properties
        .transforms
        .get_mut(&TransformNodeId(root))
        .expect("root transform")
        .viewport_matrix = glam::Mat4::from_cols_array(&[
        f32::NAN,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]);
    let nonfinite =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect_err("nonfinite transform evidence must fail before recording");
    assert!(
        nonfinite
            .reasons
            .contains(&FramePaintPlanRejection::InvalidRootTransform(root))
    );

    let (arena, root, mut properties, generations) = exact_transform_fixture();
    let child = arena.get(root).expect("root").element.children()[0];
    properties
        .transforms
        .get_mut(&TransformNodeId(root))
        .expect("root transform")
        .parent = Some(TransformNodeId(child));
    let parented =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect_err("frame-root transform must be parentless");
    assert!(
        parented
            .reasons
            .contains(&FramePaintPlanRejection::InvalidRootTransform(root))
    );
}

#[test]
fn planner_rejects_deferred_unknown_missing_and_cyclic_inputs() {
    let (arena, _root, properties, generations) = exact_transform_fixture();
    let missing = NodeKey::null();
    assert_eq!(
        plan_single_root_transform_surface(&arena, &[missing], &properties, &generations,)
            .expect_err("missing root must fail closed")
            .reasons,
        vec![FramePaintPlanRejection::MissingRoot(missing)]
    );

    let mut unknown_arena = new_test_arena();
    let unknown = commit_element(
        &mut unknown_arena,
        Box::new(UnknownHost {
            id: 0xc1_3000,
            width: 10.0,
            height: 10.0,
        }),
    );
    assert_eq!(
        plan_single_root_transform_surface(
            &unknown_arena,
            &[unknown],
            &PropertyTrees::default(),
            &PaintGenerationTracker::default(),
        )
        .expect_err("only concrete Element may own the first transform surface")
        .reasons,
        vec![FramePaintPlanRejection::UnknownRootHost(unknown)]
    );

    let (arena, root, properties, generations) = exact_transform_fixture();
    let child = arena.get(root).expect("root").element.children()[0];
    let mut deferred_style = Style::new();
    deferred_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            crate::style::Position::absolute()
                .left(crate::style::Length::px(0.0))
                .clip(crate::style::ClipMode::Viewport),
        ),
    );
    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .apply_style(deferred_style);
    let deferred =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect_err("deferred subtree changes frame ordering");
    assert!(
        deferred
            .reasons
            .contains(&FramePaintPlanRejection::DeferredBoundary(child))
    );

    let (arena, root, mut properties, generations) = exact_transform_fixture();
    properties.transforms.remove(&TransformNodeId(root));
    let missing_transform =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect_err("root transform identity is mandatory");
    assert!(
        missing_transform
            .reasons
            .contains(&FramePaintPlanRejection::MissingRootTransform(root))
    );

    let (arena, root, mut properties, generations) = exact_transform_fixture();
    let child = arena.get(root).expect("root").element.children()[0];
    properties.states.remove(&child);
    let missing_state =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect_err("every reachable owner requires a property snapshot");
    assert!(
        missing_state
            .reasons
            .contains(&FramePaintPlanRejection::MissingPropertyState(child))
    );

    let (mut arena, root, properties, generations) = exact_transform_fixture();
    let child = arena.get(root).expect("root").element.children()[0];
    arena.push_child(child, root);
    arena.set_parent(root, Some(child));
    let cycle = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect_err("cycle cannot prove canonical owner topology");
    assert!(
        cycle
            .reasons
            .contains(&FramePaintPlanRejection::RootHasParent(root))
    );
    assert!(
        cycle
            .reasons
            .contains(&FramePaintPlanRejection::DuplicateNodeKey(root))
    );
}

#[test]
fn whole_subtree_must_be_recordable_and_surface_artifact_store_is_strict() {
    let mut root = Element::new_with_id(0xc1_3100, 0.0, 0.0, 30.0, 20.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.set_transform(Transform::new([Rotate::z(Angle::deg(4.0))]));
    root.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root));
    commit_child(
        &mut arena,
        root,
        Box::new(UnknownHost {
            id: 0xc1_3101,
            width: 8.0,
            height: 6.0,
        }),
    );
    measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 80.0,
            max_height: 60.0,
            viewport_width: 80.0,
            viewport_height: 60.0,
            percent_base_width: Some(80.0),
            percent_base_height: Some(60.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 80.0,
            available_height: 60.0,
            viewport_width: 80.0,
            viewport_height: 60.0,
            percent_base_width: Some(80.0),
            percent_base_height: Some(60.0),
        },
    );
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    let error = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect_err("one unknown descendant makes the whole surface ineligible");
    assert!(error.reasons.contains(&FramePaintPlanRejection::Coverage(
        super::super::super::FrameArtifactFallbackReason::LegacyBoundary(
            super::super::super::LegacyPaintReason::UnknownHost,
        )
    )));
    {
        let root_node = arena.get(root).expect("root node");
        let root_element = root_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .expect("Element root");
        assert!(
            root_element
                .exact_transform_surface_geometry_snapshot(&arena, [0.0, 0.0], None)
                .is_none(),
            "custom descendants do not silently acquire exact retained bounds authority"
        );
        assert!(
            root_element
                .transform_surface_geometry_snapshot(&arena, [0.0, 0.0], None)
                .is_some(),
            "legacy compatibility bounds must remain available"
        );
    }
    let mut graph = FrameGraph::new();
    let ctx = UiBuildContext::new(80, 60, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    arena
        .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
        .expect("legacy custom descendant build");
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::DrawRectPass>()
            .len(),
        1,
        "exact rejection must not blank the legacy custom draw"
    );

    let (arena, root, properties, generations) = exact_transform_fixture();
    let plan = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect("baseline surface plan");
    let PaintPlanStep::RetainedSurface(surface) = &plan.steps[0] else {
        panic!("one retained surface")
    };
    let validate = |artifact: &PaintArtifact| {
        super::super::super::compiler::validate_transform_surface_artifact_for_plan(
            artifact,
            root,
            TransformNodeId(root),
        )
    };

    let mut wrong_transform = only_span(surface).artifact.clone();
    wrong_transform.chunks[0].properties.transform = None;
    assert!(!validate(&wrong_transform));

    let mut wrong_topology = only_span(surface).artifact.clone();
    let child = arena.get(root).expect("root").element.children()[0];
    wrong_topology
        .owner_nodes
        .iter_mut()
        .find(|snapshot| snapshot.owner == root)
        .expect("root owner snapshot")
        .parent = Some(child);
    assert!(!validate(&wrong_topology));

    let mut wrong_payload = only_span(surface).artifact.clone();
    let PaintOp::DrawRect(rect) = &mut wrong_payload.ops[0] else {
        panic!("fixture begins with a decoration draw")
    };
    rect.params.opacity = 0.25;
    assert!(!validate(&wrong_payload));
}

#[test]
fn planning_is_validation_only_and_does_not_compile_or_mutate_inputs() {
    let (arena, root, properties, generations) = exact_transform_fixture();
    let children_before = arena.children_of(root);
    let root_parent_before = arena.parent_of(root);
    let property_epoch_before = properties.epoch;
    let state_count_before = properties.states.len();
    let _ = super::super::super::take_artifact_compile_count();
    let _ = super::super::super::take_full_artifact_record_count();

    let plan = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect("pure planning succeeds");
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(super::super::super::take_artifact_compile_count(), 0);
    assert!(super::super::super::take_full_artifact_record_count() > 0);
    assert_eq!(arena.children_of(root), children_before);
    assert_eq!(arena.parent_of(root), root_parent_before);
    assert_eq!(properties.epoch, property_epoch_before);
    assert_eq!(properties.states.len(), state_count_before);
}
