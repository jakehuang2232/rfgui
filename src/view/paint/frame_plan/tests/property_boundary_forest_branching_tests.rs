use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BranchRole {
    Transform,
    Effect,
}

struct BranchFixture {
    arena: NodeArena,
    root: NodeKey,
    children: [NodeKey; 2],
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn branch_element(id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(id, 0.0, 0.0, 112.0, 84.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn apply_branch_role(arena: &NodeArena, owner: NodeKey, role: BranchRole, ordinal: usize) {
    let mut element = crate::view::test_support::get_element_mut::<Element>(arena, owner);
    match role {
        BranchRole::Transform => {
            element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                glam::Vec3::new(2.0 + ordinal as f32, 1.0, 0.0),
            )));
        }
        BranchRole::Effect => element.set_opacity(0.5 + ordinal as f32 * 0.08),
    }
}

fn branch_fixture(
    root_role: BranchRole,
    child_role: BranchRole,
    neutral_wrappers: bool,
    stable_id_base: u64,
) -> BranchFixture {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(branch_element(stable_id_base + 1, Color::rgb(25, 55, 95))),
    );
    let mut children = Vec::new();
    let mut next_id = stable_id_base + 1;
    for ordinal in 0..2 {
        let parent = if neutral_wrappers {
            next_id += 1;
            commit_child(
                &mut arena,
                root,
                Box::new(branch_element(
                    next_id,
                    Color::rgb(45, 85 + ordinal as u8 * 10, 115),
                )),
            )
        } else {
            root
        };
        next_id += 1;
        children.push(commit_child(
            &mut arena,
            parent,
            Box::new(branch_element(
                next_id,
                Color::rgb(165, 65 + ordinal as u8 * 10, 35),
            )),
        ));
    }
    let constraints = LayoutConstraints {
        max_width: 360.0,
        max_height: 260.0,
        viewport_width: 360.0,
        viewport_height: 260.0,
        percent_base_width: Some(360.0),
        percent_base_height: Some(260.0),
    };
    measure_and_place(
        &mut arena,
        root,
        constraints,
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 360.0,
            available_height: 260.0,
            viewport_width: 360.0,
            viewport_height: 260.0,
            percent_base_width: Some(360.0),
            percent_base_height: Some(260.0),
        },
    );
    apply_branch_role(&arena, root, root_role, 0);
    for (ordinal, child) in children.iter().copied().enumerate() {
        apply_branch_role(&arena, child, child_role, ordinal + 1);
    }
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    BranchFixture {
        arena,
        root,
        children: children.try_into().expect("two branch children"),
        properties,
        generations,
    }
}

fn branch_plan(fixture: &BranchFixture) -> FramePaintPlan {
    plan_property_effect_scene_with_context(
        &fixture.arena,
        &[fixture.root],
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("reviewed single-root branching property forest")
}

fn top_surface(plan: &FramePaintPlan) -> &RetainedSurfacePlan {
    let [PaintPlanStep::RetainedSurface(surface)] = plan.steps.as_slice() else {
        panic!("one top-level branching surface")
    };
    surface
}

#[test]
fn alternating_transform_and_effect_parents_admit_two_direct_or_neutral_siblings() {
    use BranchRole::{Effect, Transform};
    for (root_role, child_role, neutral, stable_id_base) in [
        (Transform, Effect, false, 0xf4_7100),
        (Transform, Effect, true, 0xf4_7200),
        (Effect, Transform, false, 0xf4_7300),
        (Effect, Transform, true, 0xf4_7400),
    ] {
        let fixture = branch_fixture(root_role, child_role, neutral, stable_id_base);
        let plan = branch_plan(&fixture);
        let scaffold = plan
            .property_scene_seal
            .as_ref()
            .and_then(|seal| seal.effect_scaffold.as_ref())
            .expect("branch scaffold");
        assert_eq!(scaffold.boundary_forest.roots.len(), 1);
        assert_eq!(scaffold.boundary_forest.nodes.len(), 3);
        assert_eq!(scaffold.boundary_forest.roots[0].node_span, 0..3);
        assert!(matches!(
            scaffold.boundary_forest.nodes[0].receiver,
            PropertyBoundaryForestReceiver::FrameRoot {
                scene_root_ordinal: 0
            }
        ));
        for (ordinal, expected_owner) in fixture.children.iter().copied().enumerate() {
            let node = &scaffold.boundary_forest.nodes[ordinal + 1];
            assert_eq!(node.id, PropertyBoundaryForestNodeId((ordinal + 1) as u32));
            assert_eq!(node.owner, expected_owner);
            let PropertyBoundaryForestReceiver::Surface {
                parent,
                path,
                projection,
            } = &node.receiver
            else {
                panic!("branch child receiver")
            };
            assert_eq!(*parent, PropertyBoundaryForestNodeId(0));
            assert_eq!(path.last().map(|entry| entry.owner), Some(expected_owner));
            assert_eq!(path.len(), if neutral { 2 } else { 1 });
            assert!(matches!(
                (root_role, child_role, projection),
                (
                    Transform,
                    Effect,
                    PropertyBoundaryForestProjectionWitness::ConsumedTransform { .. }
                ) | (
                    Effect,
                    Transform,
                    PropertyBoundaryForestProjectionWitness::ConsumedEffect { .. }
                )
            ));
        }
        assert!(property_scene_plan_is_sealed(&plan));
    }
}

#[test]
fn branch_materialization_preserves_child_order_and_role_aware_opaque_cursor() {
    use BranchRole::{Effect, Transform};
    for (root_role, child_role, stable_id_base) in [
        (Transform, Effect, 0xf4_7500),
        (Effect, Transform, 0xf4_7600),
    ] {
        let fixture = branch_fixture(root_role, child_role, true, stable_id_base);
        let plan = branch_plan(&fixture);
        let surface = top_surface(&plan);
        let mut seen_children = Vec::new();
        let mut cursor = 0_u32;
        for step in &surface.raster_steps {
            match step {
                PaintPlanStep::ArtifactSpan(span) => {
                    assert_eq!(span.opaque_order_span.start, cursor);
                    cursor = span.opaque_order_span.end;
                }
                PaintPlanStep::RetainedSurface(child) => {
                    seen_children.push(child.boundary_root);
                    cursor = match child_role {
                        Effect => {
                            assert!(matches!(child.kind, SurfaceKind::NestedIsolation(_)));
                            cursor
                        }
                        Transform => {
                            assert!(matches!(child.kind, SurfaceKind::Transform(_)));
                            cursor.max(child.aggregate_opaque_order_span.end)
                        }
                    };
                }
            }
        }
        assert_eq!(seen_children, fixture.children);
        assert_eq!(surface.aggregate_opaque_order_span, 0..cursor);
        assert!(property_scene_plan_is_sealed(&plan));
    }
}

#[test]
fn branch_seal_rejects_parent_path_projection_and_raster_order_tamper() {
    use BranchRole::{Effect, Transform};
    let fixture = branch_fixture(Transform, Effect, true, 0xf4_7700);
    let plan = branch_plan(&fixture);

    let mut parent = plan.clone();
    let forest = &mut parent
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest;
    let PropertyBoundaryForestReceiver::Surface {
        parent: receiver, ..
    } = &mut forest.nodes[2].receiver
    else {
        unreachable!()
    };
    *receiver = PropertyBoundaryForestNodeId(1);
    assert!(!property_scene_plan_is_sealed(&parent));

    let mut path = plan.clone();
    let forest = &mut path
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest;
    let PropertyBoundaryForestReceiver::Surface {
        path: receiver_path,
        ..
    } = &mut forest.nodes[2].receiver
    else {
        unreachable!()
    };
    receiver_path.pop();
    assert!(!property_scene_plan_is_sealed(&path));

    let mut projection = plan.clone();
    let forest = &mut projection
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest;
    let PropertyBoundaryForestReceiver::Surface {
        projection: receiver_projection,
        ..
    } = &mut forest.nodes[2].receiver
    else {
        unreachable!()
    };
    let PropertyBoundaryForestProjectionWitness::ConsumedTransform {
        projected_after,
        transform,
        ..
    } = receiver_projection
    else {
        unreachable!()
    };
    *projected_after = Some(transform.id);
    assert!(!property_scene_plan_is_sealed(&projection));

    let mut order = plan;
    let PaintPlanStep::RetainedSurface(root) = &mut order.steps[0] else {
        unreachable!()
    };
    let child_steps = root
        .raster_steps
        .iter()
        .enumerate()
        .filter_map(|(index, step)| {
            matches!(step, PaintPlanStep::RetainedSurface(_)).then_some(index)
        })
        .collect::<Vec<_>>();
    root.raster_steps.swap(child_steps[0], child_steps[1]);
    assert!(!property_scene_plan_is_sealed(&order));
}

#[test]
fn nonalternating_or_multiple_frame_root_branch_shapes_remain_rejected() {
    use BranchRole::{Effect, Transform};
    let mut nonalternating = branch_fixture(Transform, Effect, false, 0xf4_7800);
    apply_branch_role(
        &nonalternating.arena,
        nonalternating.children[1],
        Transform,
        2,
    );
    nonalternating
        .properties
        .sync(&nonalternating.arena, &[nonalternating.root]);
    nonalternating.generations.sync(
        &nonalternating.arena,
        &[nonalternating.root],
        &nonalternating.properties,
    );
    assert!(
        plan_property_effect_scene_scaffold_with_context(
            &nonalternating.arena,
            &[nonalternating.root],
            &nonalternating.properties,
            &nonalternating.generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .is_err()
    );

    let fixture = branch_fixture(Transform, Effect, false, 0xf4_7900);
    let mut multiple_frame_roots = branch_plan(&fixture);
    let forest = &mut multiple_frame_roots
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest;
    forest.nodes[1].receiver = PropertyBoundaryForestReceiver::FrameRoot {
        scene_root_ordinal: 0,
    };
    assert!(
        !forest.is_reviewed_alternating_mixed_forest(),
        "the reviewed tree predicate requires exactly one FrameRoot receiver",
    );
    assert!(!property_scene_plan_is_sealed(&multiple_frame_roots));
}
