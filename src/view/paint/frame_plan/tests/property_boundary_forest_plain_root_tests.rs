use super::*;

pub(super) struct PlainRootFixture {
    pub(super) arena: NodeArena,
    pub(super) roots: Vec<NodeKey>,
    pub(super) property_roots: [NodeKey; 2],
    pub(super) properties: PropertyTrees,
    pub(super) generations: PaintGenerationTracker,
}

fn root_element(stable_id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(stable_id, 0.0, 0.0, 104.0, 78.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

pub(super) fn plain_root_fixture() -> PlainRootFixture {
    let mut arena = new_test_arena();
    let plain_before = commit_element(
        &mut arena,
        Box::new(root_element(0xf5_3101, Color::rgb(35, 75, 115))),
    );
    let property_a = commit_element(
        &mut arena,
        Box::new(root_element(0xf5_3102, Color::rgb(25, 55, 95))),
    );
    let child_a = commit_child(
        &mut arena,
        property_a,
        Box::new(root_element(0xf5_3103, Color::rgb(165, 65, 35))),
    );
    let plain_between = commit_element(
        &mut arena,
        Box::new(root_element(0xf5_3104, Color::rgb(45, 125, 85))),
    );
    let property_b = commit_element(
        &mut arena,
        Box::new(root_element(0xf5_3105, Color::rgb(125, 75, 155))),
    );
    let child_b = commit_child(
        &mut arena,
        property_b,
        Box::new(root_element(0xf5_3106, Color::rgb(55, 135, 175))),
    );
    let plain_after = commit_element(
        &mut arena,
        Box::new(root_element(0xf5_3107, Color::rgb(145, 95, 45))),
    );
    let roots = vec![
        plain_before,
        property_a,
        plain_between,
        property_b,
        plain_after,
    ];
    let (measure, place) = {
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
        (constraints, placement)
    };
    for &root in &roots {
        measure_and_place(&mut arena, root, measure, place);
    }
    crate::view::test_support::get_element_mut::<Element>(&arena, property_a)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            2.0, 1.0, 0.0,
        ))));
    crate::view::test_support::get_element_mut::<Element>(&arena, child_a).set_opacity(0.57);
    crate::view::test_support::get_element_mut::<Element>(&arena, property_b).set_opacity(0.63);
    crate::view::test_support::get_element_mut::<Element>(&arena, child_b)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            4.0, 1.0, 0.0,
        ))));
    for &root in &roots {
        arena.refresh_subtree_dirty_cache(root);
    }
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    PlainRootFixture {
        arena,
        roots,
        property_roots: [property_a, property_b],
        properties,
        generations,
    }
}

fn plan(fixture: &PlainRootFixture) -> FramePaintPlan {
    plan_property_effect_scene_with_context(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("plain roots share the sealed property scene without fake surfaces")
}

#[test]
fn plain_roots_before_between_and_after_property_roots_own_empty_ordered_spans() {
    let fixture = plain_root_fixture();
    let plan = plan(&fixture);
    let forest = &plan
        .property_scene_seal
        .as_ref()
        .unwrap()
        .effect_scaffold
        .as_ref()
        .unwrap()
        .boundary_forest;
    assert_eq!(forest.roots.len(), 5);
    assert_eq!(forest.nodes.len(), 4);
    assert_eq!(
        forest
            .roots
            .iter()
            .map(|root| root.node_span.clone())
            .collect::<Vec<_>>(),
        vec![0..0, 0..2, 2..2, 2..4, 4..4],
    );
    assert!(forest.is_structurally_canonical());
    assert!(forest.is_reviewed_alternating_mixed_forest());
}

#[test]
fn plain_roots_are_sealed_only_by_artifact_root_steps_and_joint_witness_order() {
    let fixture = plain_root_fixture();
    let plan = plan(&fixture);
    let scaffold = plan
        .property_scene_seal
        .as_ref()
        .unwrap()
        .effect_scaffold
        .as_ref()
        .unwrap();
    assert_eq!(
        scaffold.production_root_step_spans.as_ref().unwrap(),
        &[0..1, 1..2, 2..3, 3..4, 4..5],
    );
    let schedule = scaffold.production_root_step_schedule.as_ref().unwrap();
    assert!(matches!(
        schedule[0].as_slice(),
        [PropertyEffectRootStepKind::NormalArtifact]
    ));
    assert!(matches!(
        schedule[2].as_slice(),
        [PropertyEffectRootStepKind::NormalArtifact]
    ));
    assert!(matches!(
        schedule[4].as_slice(),
        [PropertyEffectRootStepKind::NormalArtifact]
    ));
    assert_eq!(plan.steps.len(), 5);
    let witness = plan.property_scene_transaction_witness().unwrap();
    assert_eq!(witness.roots.len(), 5);
    assert_eq!(witness.surfaces.len(), 4);
    assert_eq!(witness.top_level_surfaces.len(), 2);
    assert_eq!(witness.top_level_surfaces[0].scene_root_ordinal, 1);
    assert_eq!(witness.top_level_surfaces[1].scene_root_ordinal, 3);
}

#[test]
fn forged_plain_surface_and_nonempty_plain_node_span_fail_closed() {
    let fixture = plain_root_fixture();
    let plan = plan(&fixture);

    let mut forged = plan.clone();
    forged.steps[0] = forged.steps[1].clone();
    let scaffold = forged
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap();
    let fake_kind = PropertyEffectRootStepKind::LateBoundary(PropertyBoundaryId::Transform(
        TransformNodeId(fixture.property_roots[0]),
    ));
    scaffold.production_root_step_schedule.as_mut().unwrap()[0][0] = fake_kind;
    scaffold
        .planned_production_root_step_schedule
        .as_mut()
        .unwrap()[0][0] = fake_kind;
    assert!(!property_scene_plan_is_sealed(&forged));

    let mut claimed = plan;
    let forest = &mut claimed
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest;
    forest.roots[0].node_span = 0..1;
    assert!(!forest.is_structurally_canonical());
    assert!(!property_scene_plan_is_sealed(&claimed));
}

#[test]
fn an_all_plain_empty_forest_and_cross_root_parent_still_fail_closed() {
    let fixture = plain_root_fixture();
    let plan = plan(&fixture);

    let mut empty = plan.clone();
    let forest = &mut empty
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest;
    forest.nodes.clear();
    for root in &mut forest.roots {
        root.node_span = 0..0;
    }
    assert!(!forest.is_structurally_canonical());
    assert!(!property_scene_plan_is_sealed(&empty));

    let mut cross_root = plan;
    let forest = &mut cross_root
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest;
    let PropertyBoundaryForestReceiver::Surface { parent, .. } = &mut forest.nodes[3].receiver
    else {
        unreachable!()
    };
    *parent = PropertyBoundaryForestNodeId(0);
    assert!(!forest.is_structurally_canonical());
    assert!(!property_scene_plan_is_sealed(&cross_root));
}
