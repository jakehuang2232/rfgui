use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AlternatingBoundaryRole {
    Transform,
    Effect,
}

struct AlternatingForestFixture {
    arena: NodeArena,
    roots: Vec<NodeKey>,
    chains: Vec<Vec<NodeKey>>,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn alternating_element(id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(id, 0.0, 0.0, 96.0, 72.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn alternating_forest_fixture(
    chains: &[&[AlternatingBoundaryRole]],
    neutral_wrappers: bool,
    stable_id_base: u64,
) -> AlternatingForestFixture {
    let mut arena = new_test_arena();
    let mut roots = Vec::new();
    let mut boundary_chains = Vec::new();
    let mut next_id = stable_id_base;
    for (root_ordinal, roles) in chains.iter().enumerate() {
        assert!(!roles.is_empty());
        next_id += 1;
        let root = commit_element(
            &mut arena,
            Box::new(alternating_element(
                next_id,
                Color::rgb(25 + root_ordinal as u8, 55, 95),
            )),
        );
        roots.push(root);
        let mut boundaries = vec![root];
        let mut parent = root;
        for (role_ordinal, _) in roles.iter().enumerate().skip(1) {
            if neutral_wrappers {
                next_id += 1;
                parent = commit_child(
                    &mut arena,
                    parent,
                    Box::new(alternating_element(
                        next_id,
                        Color::rgb(40, 75 + role_ordinal as u8, 105),
                    )),
                );
            }
            next_id += 1;
            parent = commit_child(
                &mut arena,
                parent,
                Box::new(alternating_element(
                    next_id,
                    Color::rgb(165, 60 + role_ordinal as u8, 35),
                )),
            );
            boundaries.push(parent);
        }
        boundary_chains.push(boundaries);
    }
    let constraints = LayoutConstraints {
        max_width: 320.0,
        max_height: 240.0,
        viewport_width: 320.0,
        viewport_height: 240.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(240.0),
    };
    for (root, boundaries, roles) in roots
        .iter()
        .copied()
        .zip(&boundary_chains)
        .zip(chains)
        .map(|((root, boundaries), roles)| (root, boundaries, roles))
    {
        measure_and_place(
            &mut arena,
            root,
            constraints,
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 320.0,
                available_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
        );
        for (ordinal, (&owner, role)) in boundaries.iter().zip(*roles).enumerate() {
            let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, owner);
            match role {
                AlternatingBoundaryRole::Transform => {
                    element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                        glam::Vec3::new(2.0 + ordinal as f32, 1.0, 0.0),
                    )));
                }
                AlternatingBoundaryRole::Effect => {
                    element.set_opacity(0.5 + ordinal as f32 * 0.05);
                }
            }
        }
        arena.refresh_subtree_dirty_cache(root);
    }
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    AlternatingForestFixture {
        arena,
        roots,
        chains: boundary_chains,
        properties,
        generations,
    }
}

fn executable_alternating_plan(fixture: &AlternatingForestFixture) -> FramePaintPlan {
    plan_property_effect_scene_with_context(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("reviewed alternating property forest")
}

fn effect_transform_forest_fixture(
    neutral_wrapper: bool,
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (arena, root, child, grandchild, _, _) = planning_only_nested_effect_fixture();
    crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(1.0);
    crate::view::test_support::get_element_mut::<Element>(&arena, grandchild).set_opacity(1.0);
    let transform_owner = if neutral_wrapper { grandchild } else { child };
    crate::view::test_support::get_element_mut::<Element>(&arena, transform_owner)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            3.0, 2.0, 0.0,
        ))));
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, child, transform_owner, properties, generations)
}

#[test]
fn property_boundary_forest_seals_effect_transform_direct_and_neutral_paths() {
    for neutral_wrapper in [false, true] {
        let (arena, root, wrapper, transform_owner, properties, generations) =
            effect_transform_forest_fixture(neutral_wrapper);
        let plan = plan_property_effect_scene_scaffold_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect("reviewed E->T forest slice");
        let scaffold = plan
            .property_scene_seal
            .as_ref()
            .and_then(|seal| seal.effect_scaffold.as_ref())
            .expect("effect scaffold");
        assert_eq!(scaffold.boundary_forest.roots.len(), 1);
        assert_eq!(scaffold.boundary_forest.nodes.len(), 2);
        let [effect, transform] = scaffold.boundary_forest.nodes.as_slice() else {
            panic!("exact E->T nodes")
        };
        assert_eq!(effect.id, PropertyBoundaryForestNodeId(0));
        assert_eq!(effect.owner, root);
        assert_eq!(effect.stable_key.role, PropertyBoundaryForestRole::Effect);
        assert_eq!(
            effect.persistent_color_key,
            crate::view::base_component::isolation_layer_stable_key(effect.stable_key.stable_id)
        );
        assert_eq!(transform.id, PropertyBoundaryForestNodeId(1));
        assert_eq!(transform.owner, transform_owner);
        assert_eq!(
            transform.stable_key.role,
            PropertyBoundaryForestRole::Transform
        );
        let PropertyBoundaryForestReceiver::Surface {
            parent,
            path,
            projection,
        } = &transform.receiver
        else {
            panic!("transform surface receiver")
        };
        assert_eq!(*parent, PropertyBoundaryForestNodeId(0));
        let expected_path = if neutral_wrapper {
            vec![wrapper, transform_owner]
        } else {
            vec![transform_owner]
        };
        assert_eq!(
            path.iter().map(|witness| witness.owner).collect::<Vec<_>>(),
            expected_path
        );
        assert!(matches!(
            projection,
            PropertyBoundaryForestProjectionWitness::ConsumedEffect {
                effect,
                expected_before: Some(before),
                projected_after: None,
            } if effect.owner == root && *before == effect.id
        ));
        assert!(property_effect_scaffold_is_canonical(
            &plan,
            plan.property_scene_seal.as_ref().expect("seal"),
            scaffold,
        ));
    }
}

#[test]
fn property_boundary_forest_materializes_effect_transform_without_fixed_grammar() {
    for neutral_wrapper in [false, true] {
        let (arena, root, _, transform_owner, properties, generations) =
            effect_transform_forest_fixture(neutral_wrapper);
        let plan = plan_property_effect_scene_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect("E->T forest materializes recursively");
        let witness = plan
            .property_scene_transaction_witness()
            .expect("property forest transaction witness");
        assert!(matches!(
            witness.surfaces.as_slice(),
            [
                PropertySceneTransactionSurfaceWitness {
                    kind: PropertySceneTransactionSurfaceKind::Effect(_),
                    parent_surface: None,
                    ..
                },
                PropertySceneTransactionSurfaceWitness {
                    boundary_root,
                    kind: PropertySceneTransactionSurfaceKind::Transform(_),
                    parent_surface: Some(parent),
                    ..
                }
            ] if *boundary_root == transform_owner && *parent == root
        ));
        let [PaintPlanStep::RetainedSurface(effect)] = plan.steps.as_slice() else {
            panic!("single top-level effect surface")
        };
        if !neutral_wrapper {
            assert!(effect.aggregate_opaque_order_span.end > 1);
            let mut stale_aggregate = plan.clone();
            let PaintPlanStep::RetainedSurface(effect) = &mut stale_aggregate.steps[0] else {
                unreachable!()
            };
            effect.aggregate_opaque_order_span = 0..1;
            assert!(
                !property_scene_plan_is_sealed(&stale_aggregate),
                "a seal that omits the transform child's replayed opaque terminal must reject",
            );
        }
    }
}

#[test]
fn property_boundary_forest_seal_rejects_role_path_and_projection_tamper() {
    let (arena, root, _, _, properties, generations) = effect_transform_forest_fixture(true);
    let baseline = plan_property_effect_scene_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("baseline forest");

    let mut role = baseline.clone();
    role.property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest
        .nodes[1]
        .stable_key
        .role = PropertyBoundaryForestRole::Effect;
    assert!(!property_scene_plan_is_sealed(&role));

    let mut path = baseline.clone();
    let PropertyBoundaryForestReceiver::Surface { path: owners, .. } = &mut path
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest
        .nodes[1]
        .receiver
    else {
        panic!("surface receiver")
    };
    owners.pop();
    assert!(!property_scene_plan_is_sealed(&path));

    let mut projection_plan = baseline;
    let PropertyBoundaryForestReceiver::Surface {
        projection: projection_witness,
        ..
    } = &mut projection_plan
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest
        .nodes[1]
        .receiver
    else {
        panic!("surface receiver")
    };
    let PropertyBoundaryForestProjectionWitness::ConsumedEffect {
        projected_after, ..
    } = projection_witness
    else {
        panic!("effect projection")
    };
    *projected_after = Some(EffectNodeId(root));
    assert!(!property_scene_plan_is_sealed(&projection_plan));
}

#[test]
fn property_boundary_forest_rejects_non_alternating_roles() {
    let (arena, root, child, grandchild, mut properties, mut generations) =
        planning_only_nested_effect_fixture();
    crate::view::test_support::get_element_mut::<Element>(&arena, grandchild)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            2.0, 0.0, 0.0,
        ))));
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let error = plan_property_effect_scene_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect_err("E->E->T is not a strictly alternating property chain");
    assert!(error.reasons.iter().any(|reason| matches!(
        reason,
        FramePaintPlanRejection::UnsupportedPropertyInterleave(owner, _)
            if *owner == root || *owner == child
    )));
}

fn assert_depth_three_alternating_plan(
    fixture: &AlternatingForestFixture,
    expected_roles: [PropertyBoundaryForestRole; 3],
) -> FramePaintPlan {
    plan_property_effect_scene_scaffold_with_context(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("reviewed depth-three scaffold");
    let plan = executable_alternating_plan(fixture);
    let scaffold = plan
        .property_scene_seal
        .as_ref()
        .and_then(|seal| seal.effect_scaffold.as_ref())
        .expect("effect scaffold");
    assert_eq!(scaffold.boundary_forest.roots.len(), 1);
    assert_eq!(scaffold.boundary_forest.nodes.len(), 3);
    let boundaries = &fixture.chains[0];
    for (ordinal, ((node, expected_role), &expected_owner)) in scaffold
        .boundary_forest
        .nodes
        .iter()
        .zip(expected_roles)
        .zip(boundaries)
        .enumerate()
    {
        assert_eq!(node.id, PropertyBoundaryForestNodeId(ordinal as u32));
        assert_eq!(node.owner, expected_owner);
        assert_eq!(node.stable_key.role, expected_role);
        if ordinal == 0 {
            assert!(matches!(
                node.receiver,
                PropertyBoundaryForestReceiver::FrameRoot {
                    scene_root_ordinal: 0
                }
            ));
            continue;
        }
        let PropertyBoundaryForestReceiver::Surface {
            parent,
            path,
            projection,
        } = &node.receiver
        else {
            panic!("nested boundary receiver")
        };
        assert_eq!(*parent, PropertyBoundaryForestNodeId(ordinal as u32 - 1));
        let parent_owner = boundaries[ordinal - 1];
        let mut expected_path = Vec::new();
        let mut cursor = expected_owner;
        loop {
            expected_path.push(cursor);
            let next = fixture.arena.parent_of(cursor).expect("descendant path");
            if next == parent_owner {
                break;
            }
            cursor = next;
        }
        expected_path.reverse();
        assert_eq!(
            path.iter().map(|entry| entry.owner).collect::<Vec<_>>(),
            expected_path
        );
        match (expected_roles[ordinal - 1], expected_role, projection) {
            (
                PropertyBoundaryForestRole::Transform,
                PropertyBoundaryForestRole::Effect,
                PropertyBoundaryForestProjectionWitness::ConsumedTransform {
                    transform,
                    expected_before,
                    projected_after,
                },
            ) => {
                assert_eq!(transform.owner, parent_owner);
                assert_eq!(*expected_before, Some(transform.id));
                assert_eq!(*projected_after, transform.parent);
            }
            (
                PropertyBoundaryForestRole::Effect,
                PropertyBoundaryForestRole::Transform,
                PropertyBoundaryForestProjectionWitness::ConsumedEffect {
                    effect,
                    expected_before,
                    projected_after,
                },
            ) => {
                assert_eq!(effect.owner, parent_owner);
                assert_eq!(*expected_before, Some(effect.id));
                assert_eq!(*projected_after, effect.parent);
            }
            _ => panic!("exact alternating projection"),
        }
    }
    let witness = plan
        .property_scene_transaction_witness()
        .expect("depth-three transaction witness");
    assert_eq!(witness.surfaces.len(), 3);
    assert_eq!(witness.surfaces[0].parent_surface, None);
    assert_eq!(witness.surfaces[1].parent_surface, Some(boundaries[0]));
    assert_eq!(witness.surfaces[2].parent_surface, Some(boundaries[1]));
    plan
}

#[test]
fn property_boundary_forest_materializes_transform_effect_transform_direct_and_neutral() {
    use AlternatingBoundaryRole::{Effect, Transform};
    for (neutral, stable_id_base) in [(false, 0xf4_3100), (true, 0xf4_3200)] {
        let fixture =
            alternating_forest_fixture(&[&[Transform, Effect, Transform]], neutral, stable_id_base);
        assert_depth_three_alternating_plan(
            &fixture,
            [
                PropertyBoundaryForestRole::Transform,
                PropertyBoundaryForestRole::Effect,
                PropertyBoundaryForestRole::Transform,
            ],
        );
    }
}

#[test]
fn property_boundary_forest_materializes_effect_transform_effect_direct_and_neutral() {
    use AlternatingBoundaryRole::{Effect, Transform};
    for (neutral, stable_id_base) in [(false, 0xf4_3300), (true, 0xf4_3400)] {
        let fixture =
            alternating_forest_fixture(&[&[Effect, Transform, Effect]], neutral, stable_id_base);
        assert_depth_three_alternating_plan(
            &fixture,
            [
                PropertyBoundaryForestRole::Effect,
                PropertyBoundaryForestRole::Transform,
                PropertyBoundaryForestRole::Effect,
            ],
        );
    }
}

#[test]
fn property_boundary_forest_tamper_branch_and_multi_root_reject() {
    use AlternatingBoundaryRole::{Effect, Transform};
    let baseline = alternating_forest_fixture(&[&[Transform, Effect, Transform]], true, 0xf4_3500);
    let plan = assert_depth_three_alternating_plan(
        &baseline,
        [
            PropertyBoundaryForestRole::Transform,
            PropertyBoundaryForestRole::Effect,
            PropertyBoundaryForestRole::Transform,
        ],
    );

    let mut role = plan.clone();
    role.property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest
        .nodes[2]
        .stable_key
        .role = PropertyBoundaryForestRole::Effect;
    assert!(!property_scene_plan_is_sealed(&role));

    let mut path = plan.clone();
    let PropertyBoundaryForestReceiver::Surface { path: owners, .. } = &mut path
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest
        .nodes[2]
        .receiver
    else {
        unreachable!()
    };
    owners.pop();
    assert!(!property_scene_plan_is_sealed(&path));

    let mut projection = plan.clone();
    let PropertyBoundaryForestReceiver::Surface {
        projection: witness,
        ..
    } = &mut projection
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest
        .nodes[2]
        .receiver
    else {
        unreachable!()
    };
    let PropertyBoundaryForestProjectionWitness::ConsumedEffect {
        effect,
        projected_after,
        ..
    } = witness
    else {
        unreachable!()
    };
    *projected_after = Some(effect.id);
    assert!(!property_scene_plan_is_sealed(&projection));

    let mut duplicate_frame_root = plan.clone();
    let forest = &mut duplicate_frame_root
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
    assert!(!forest.is_reviewed_alternating_mixed_forest());
    assert!(!property_scene_plan_is_sealed(&duplicate_frame_root));

    let mut cross_root_parent = plan;
    let forest = &mut cross_root_parent
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest;
    forest.roots[0].node_span = 0..2;
    forest.roots.push(PropertyBoundaryForestRoot {
        scene_root_ordinal: 1,
        root: forest.nodes[2].owner,
        stable_id: forest.nodes[2].stable_key.stable_id,
        node_span: 2..3,
    });
    forest.nodes[2].scene_root_ordinal = 1;
    assert!(
        !forest.is_structurally_canonical(),
        "a surface parent cannot cross scene-root spans",
    );

    let mut branch = alternating_forest_fixture(&[&[Transform, Effect]], false, 0xf4_3700);
    let sibling = commit_child(
        &mut branch.arena,
        branch.roots[0],
        Box::new(alternating_element(0xf4_37ff, Color::rgb(90, 45, 130))),
    );
    crate::view::test_support::get_element_mut::<Element>(&branch.arena, sibling).set_opacity(0.65);
    branch.arena.refresh_subtree_dirty_cache(branch.roots[0]);
    branch.properties.sync(&branch.arena, &branch.roots);
    branch
        .generations
        .sync(&branch.arena, &branch.roots, &branch.properties);
    let branched = plan_property_effect_scene_scaffold_with_context(
        &branch.arena,
        &branch.roots,
        &branch.properties,
        &branch.generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    );
    let forest = &branched
        .expect("one transform parent may own multiple effect children")
        .property_scene_seal
        .unwrap()
        .effect_scaffold
        .unwrap()
        .boundary_forest;
    assert_eq!(forest.nodes.len(), 3);
    for child in &forest.nodes[1..] {
        assert!(matches!(
            child.receiver,
            PropertyBoundaryForestReceiver::Surface {
                parent: PropertyBoundaryForestNodeId(0),
                ..
            }
        ));
    }

    {
        let mut element =
            crate::view::test_support::get_element_mut::<Element>(&branch.arena, sibling);
        element.set_opacity(1.0);
        element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
            glam::Vec3::new(4.0, 0.0, 0.0),
        )));
    }
    branch.properties.sync(&branch.arena, &branch.roots);
    branch
        .generations
        .sync(&branch.arena, &branch.roots, &branch.properties);
    assert!(
        plan_property_effect_scene_scaffold_with_context(
            &branch.arena,
            &branch.roots,
            &branch.properties,
            &branch.generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .is_err(),
        "non-alternating transform sibling still fails closed",
    );

    let multi_root = alternating_forest_fixture(
        &[&[Transform, Effect], &[Effect, Transform]],
        false,
        0xf4_3800,
    );
    let multi_root_plan = plan_property_effect_scene_scaffold_with_context(
        &multi_root.arena,
        &multi_root.roots,
        &multi_root.properties,
        &multi_root.generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("heterogeneous alternating property roots share one forest");
    let multi_root_forest = &multi_root_plan
        .property_scene_seal
        .unwrap()
        .effect_scaffold
        .unwrap()
        .boundary_forest;
    assert_eq!(multi_root_forest.roots.len(), 2);
    assert_eq!(multi_root_forest.nodes.len(), 4);
    assert_eq!(multi_root_forest.roots[0].node_span, 0..2);
    assert_eq!(multi_root_forest.roots[1].node_span, 2..4);
}
