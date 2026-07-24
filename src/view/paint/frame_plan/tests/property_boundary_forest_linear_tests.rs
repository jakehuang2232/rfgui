use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinearRole {
    Transform,
    Effect,
}

struct LinearFixture {
    arena: NodeArena,
    root: NodeKey,
    boundaries: Vec<NodeKey>,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn linear_element(id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(id, 0.0, 0.0, 112.0, 84.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn linear_fixture(
    roles: &[LinearRole],
    neutral_wrappers: bool,
    stable_id_base: u64,
) -> LinearFixture {
    assert!(!roles.is_empty());
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(linear_element(stable_id_base + 1, Color::rgb(25, 55, 95))),
    );
    let mut boundaries = vec![root];
    let mut parent = root;
    let mut next_id = stable_id_base + 1;
    for ordinal in 1..roles.len() {
        if neutral_wrappers {
            next_id += 1;
            parent = commit_child(
                &mut arena,
                parent,
                Box::new(linear_element(
                    next_id,
                    Color::rgb(45, 75 + ordinal as u8, 105),
                )),
            );
        }
        next_id += 1;
        parent = commit_child(
            &mut arena,
            parent,
            Box::new(linear_element(
                next_id,
                Color::rgb(165, 65 + ordinal as u8, 35),
            )),
        );
        boundaries.push(parent);
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
    for (ordinal, (&owner, role)) in boundaries.iter().zip(roles).enumerate() {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, owner);
        match role {
            LinearRole::Transform => {
                element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                    glam::Vec3::new(2.0 + ordinal as f32, 1.0, 0.0),
                )));
            }
            LinearRole::Effect => element.set_opacity(0.45 + ordinal as f32 * 0.04),
        }
    }
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    LinearFixture {
        arena,
        root,
        boundaries,
        properties,
        generations,
    }
}

fn sync_fixture(fixture: &mut LinearFixture) {
    fixture.properties.sync(&fixture.arena, &[fixture.root]);
    fixture
        .generations
        .sync(&fixture.arena, &[fixture.root], &fixture.properties);
}

fn linear_plan(fixture: &LinearFixture) -> FramePaintPlan {
    plan_property_effect_scene_with_context(
        &fixture.arena,
        &[fixture.root],
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("reviewed arbitrary-depth linear property forest")
}

fn assert_linear_plan(
    fixture: &LinearFixture,
    expected_roles: &[PropertyBoundaryForestRole],
) -> FramePaintPlan {
    let plan = linear_plan(fixture);
    let scaffold = plan
        .property_scene_seal
        .as_ref()
        .and_then(|seal| seal.effect_scaffold.as_ref())
        .expect("linear effect scaffold");
    assert_eq!(scaffold.boundary_forest.roots.len(), 1);
    assert_eq!(scaffold.boundary_forest.nodes.len(), expected_roles.len());
    for (ordinal, (node, expected_role)) in scaffold
        .boundary_forest
        .nodes
        .iter()
        .zip(expected_roles)
        .enumerate()
    {
        assert_eq!(node.id, PropertyBoundaryForestNodeId(ordinal as u32));
        assert_eq!(node.stable_key.role, *expected_role);
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
            panic!("linear descendant receiver")
        };
        assert_eq!(*parent, PropertyBoundaryForestNodeId(ordinal as u32 - 1));
        let parent_owner = scaffold.boundary_forest.nodes[ordinal - 1].owner;
        if parent_owner == node.owner {
            assert!(path.is_empty());
        } else {
            assert_eq!(path.last().map(|entry| entry.owner), Some(node.owner));
        }
        match (expected_roles[ordinal - 1], *expected_role, projection) {
            (
                PropertyBoundaryForestRole::Transform,
                PropertyBoundaryForestRole::Effect,
                PropertyBoundaryForestProjectionWitness::ConsumedTransform {
                    transform,
                    projected_after,
                    ..
                },
            ) => {
                let expected = (0..ordinal - 1).rev().find_map(|ancestor| {
                    (expected_roles[ancestor] == PropertyBoundaryForestRole::Transform).then_some(
                        TransformNodeId(scaffold.boundary_forest.nodes[ancestor].owner),
                    )
                });
                assert_eq!(transform.owner, parent_owner);
                assert_eq!(*projected_after, expected);
            }
            (
                PropertyBoundaryForestRole::Effect,
                PropertyBoundaryForestRole::Transform,
                PropertyBoundaryForestProjectionWitness::ConsumedEffect {
                    effect,
                    projected_after,
                    ..
                },
            ) => {
                let expected = (0..ordinal - 1).rev().find_map(|ancestor| {
                    (expected_roles[ancestor] == PropertyBoundaryForestRole::Effect)
                        .then_some(EffectNodeId(scaffold.boundary_forest.nodes[ancestor].owner))
                });
                assert_eq!(effect.owner, parent_owner);
                assert_eq!(*projected_after, expected);
            }
            _ => panic!("strict alternating projection"),
        }
    }
    let witness = plan
        .property_scene_transaction_witness()
        .expect("linear transaction witness");
    assert_eq!(witness.surfaces.len(), expected_roles.len());
    plan
}

#[test]
fn arbitrary_depth_linear_chains_materialize_without_depth_grammar() {
    use LinearRole::{Effect, Transform};
    for (roles, expected, neutral, stable_id_base) in [
        (
            vec![Transform, Effect, Transform, Effect],
            vec![
                PropertyBoundaryForestRole::Transform,
                PropertyBoundaryForestRole::Effect,
                PropertyBoundaryForestRole::Transform,
                PropertyBoundaryForestRole::Effect,
            ],
            false,
            0xf4_6100,
        ),
        (
            vec![Effect, Transform, Effect, Transform, Effect],
            vec![
                PropertyBoundaryForestRole::Effect,
                PropertyBoundaryForestRole::Transform,
                PropertyBoundaryForestRole::Effect,
                PropertyBoundaryForestRole::Transform,
                PropertyBoundaryForestRole::Effect,
            ],
            true,
            0xf4_6200,
        ),
    ] {
        let fixture = linear_fixture(&roles, neutral, stable_id_base);
        assert_linear_plan(&fixture, &expected);
    }
}

#[test]
fn canonical_same_owner_transform_effect_pair_works_inside_a_deeper_chain() {
    use LinearRole::{Effect, Transform};
    let mut fixture = linear_fixture(&[Effect, Transform, Transform, Effect], true, 0xf4_6300);
    crate::view::test_support::get_element_mut::<Element>(&fixture.arena, fixture.boundaries[1])
        .set_opacity(0.6);
    sync_fixture(&mut fixture);
    let expected = [
        PropertyBoundaryForestRole::Effect,
        PropertyBoundaryForestRole::Transform,
        PropertyBoundaryForestRole::Effect,
        PropertyBoundaryForestRole::Transform,
        PropertyBoundaryForestRole::Effect,
    ];
    let plan = assert_linear_plan(&fixture, &expected);
    let scaffold = plan
        .property_scene_seal
        .as_ref()
        .and_then(|seal| seal.effect_scaffold.as_ref())
        .unwrap();
    assert_eq!(
        scaffold.boundary_forest.nodes[1].owner,
        scaffold.boundary_forest.nodes[2].owner
    );
    let PropertyBoundaryForestReceiver::Surface {
        path, projection, ..
    } = &scaffold.boundary_forest.nodes[2].receiver
    else {
        unreachable!()
    };
    assert!(path.is_empty());
    assert!(matches!(
        projection,
        PropertyBoundaryForestProjectionWitness::ConsumedTransform { .. }
    ));

    let mut forbidden_effect_transform = scaffold.boundary_forest.clone();
    forbidden_effect_transform.nodes.truncate(2);
    forbidden_effect_transform.roots[0].node_span = 0..2;
    forbidden_effect_transform.nodes[1].owner = forbidden_effect_transform.nodes[0].owner;
    let PropertyBoundaryForestReceiver::Surface { path, .. } =
        &mut forbidden_effect_transform.nodes[1].receiver
    else {
        unreachable!()
    };
    path.clear();
    assert!(
        forbidden_effect_transform.is_structurally_canonical(),
        "generic forest tokens only validate self-consistency"
    );
    assert!(
        !forbidden_effect_transform.is_reviewed_alternating_mixed_forest(),
        "same-owner E -> T must remain closed by mixed production admission"
    );

    let mut reversed_same_owner = plan;
    reversed_same_owner
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest
        .nodes
        .swap(1, 2);
    assert!(!property_scene_plan_is_sealed(&reversed_same_owner));
}

#[test]
fn nonalternating_remains_rejected_and_heterogeneous_multiroot_is_admitted() {
    use LinearRole::{Effect, Transform};
    for (roles, stable_id_base) in [
        (vec![Transform, Transform, Effect], 0xf4_6400),
        (vec![Effect, Effect, Transform], 0xf4_6500),
    ] {
        let fixture = linear_fixture(&roles, false, stable_id_base);
        assert!(
            plan_property_effect_scene_scaffold_with_context(
                &fixture.arena,
                &[fixture.root],
                &fixture.properties,
                &fixture.generations,
                TransformSurfacePlanContext::new([0.0, 0.0], None),
            )
            .is_err()
        );
    }

    let mut arena = new_test_arena();
    let first_root = commit_element(
        &mut arena,
        Box::new(linear_element(0xf4_6701, Color::rgb(25, 55, 95))),
    );
    let first_child = commit_child(
        &mut arena,
        first_root,
        Box::new(linear_element(0xf4_6702, Color::rgb(165, 65, 35))),
    );
    let second_root = commit_element(
        &mut arena,
        Box::new(linear_element(0xf4_6801, Color::rgb(35, 65, 105))),
    );
    let second_child = commit_child(
        &mut arena,
        second_root,
        Box::new(linear_element(0xf4_6802, Color::rgb(175, 75, 45))),
    );
    let constraints = LayoutConstraints {
        max_width: 360.0,
        max_height: 260.0,
        viewport_width: 360.0,
        viewport_height: 260.0,
        percent_base_width: Some(360.0),
        percent_base_height: Some(260.0),
    };
    let placement = LayoutPlacement {
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
    };
    measure_and_place(&mut arena, first_root, constraints, placement);
    measure_and_place(&mut arena, second_root, constraints, placement);
    crate::view::test_support::get_element_mut::<Element>(&arena, first_root)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            2.0, 1.0, 0.0,
        ))));
    crate::view::test_support::get_element_mut::<Element>(&arena, first_child).set_opacity(0.55);
    crate::view::test_support::get_element_mut::<Element>(&arena, second_root).set_opacity(0.6);
    crate::view::test_support::get_element_mut::<Element>(&arena, second_child)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            3.0, 1.0, 0.0,
        ))));
    arena.refresh_subtree_dirty_cache(first_root);
    arena.refresh_subtree_dirty_cache(second_root);
    let roots = vec![first_root, second_root];
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    let plan = plan_property_effect_scene_scaffold_with_context(
        &arena,
        &roots,
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("heterogeneous alternating roots share one reviewed forest");
    let forest = &plan
        .property_scene_seal
        .unwrap()
        .effect_scaffold
        .unwrap()
        .boundary_forest;
    assert_eq!(forest.roots.len(), 2);
    assert_eq!(forest.roots[0].node_span, 0..2);
    assert_eq!(forest.roots[1].node_span, 2..4);
}
