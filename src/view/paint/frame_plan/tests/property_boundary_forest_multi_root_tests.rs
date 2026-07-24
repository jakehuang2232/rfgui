use super::*;

#[derive(Clone, Copy)]
enum RootRole {
    Transform,
    Effect,
}

struct MultiRootFixture {
    arena: NodeArena,
    roots: [NodeKey; 2],
    children: [NodeKey; 2],
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn root_element(stable_id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(stable_id, 0.0, 0.0, 108.0, 82.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn apply_role(arena: &NodeArena, owner: NodeKey, role: RootRole, ordinal: usize) {
    let mut element = crate::view::test_support::get_element_mut::<Element>(arena, owner);
    match role {
        RootRole::Transform => {
            element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                glam::Vec3::new(2.0 + ordinal as f32, 1.0, 0.0),
            )));
        }
        RootRole::Effect => element.set_opacity(0.54 + ordinal as f32 * 0.07),
    }
}

fn heterogeneous_fixture() -> MultiRootFixture {
    let mut arena = new_test_arena();
    let root_a = commit_element(
        &mut arena,
        Box::new(root_element(0xf5_0101, Color::rgb(25, 55, 95))),
    );
    let child_a = commit_child(
        &mut arena,
        root_a,
        Box::new(root_element(0xf5_0102, Color::rgb(165, 65, 35))),
    );
    let root_b = commit_element(
        &mut arena,
        Box::new(root_element(0xf5_0103, Color::rgb(35, 95, 75))),
    );
    let child_b = commit_child(
        &mut arena,
        root_b,
        Box::new(root_element(0xf5_0104, Color::rgb(135, 75, 155))),
    );
    let constraints = LayoutConstraints {
        max_width: 360.0,
        max_height: 260.0,
        viewport_width: 360.0,
        viewport_height: 260.0,
        percent_base_width: Some(360.0),
        percent_base_height: Some(260.0),
    };
    for root in [root_a, root_b] {
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
    }
    apply_role(&arena, root_a, RootRole::Transform, 0);
    apply_role(&arena, child_a, RootRole::Effect, 1);
    apply_role(&arena, root_b, RootRole::Effect, 2);
    apply_role(&arena, child_b, RootRole::Transform, 3);
    for root in [root_a, root_b] {
        arena.refresh_subtree_dirty_cache(root);
    }
    let roots = [root_a, root_b];
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    MultiRootFixture {
        arena,
        roots,
        children: [child_a, child_b],
        properties,
        generations,
    }
}

fn multi_root_plan(fixture: &MultiRootFixture) -> FramePaintPlan {
    plan_property_effect_scene_with_context(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("heterogeneous multi-root property forest")
}

#[test]
fn heterogeneous_roots_freeze_ordered_nonempty_spans_and_one_frame_root_each() {
    let fixture = heterogeneous_fixture();
    let plan = multi_root_plan(&fixture);
    let forest = &plan
        .property_scene_seal
        .as_ref()
        .unwrap()
        .effect_scaffold
        .as_ref()
        .unwrap()
        .boundary_forest;
    assert_eq!(forest.roots.len(), 2);
    assert_eq!(forest.nodes.len(), 4);
    assert_eq!(forest.roots[0].node_span, 0..2);
    assert_eq!(forest.roots[1].node_span, 2..4);
    for (root_ordinal, root) in forest.roots.iter().enumerate() {
        let nodes = &forest.nodes[root.node_span.start as usize..root.node_span.end as usize];
        assert_eq!(nodes[0].owner, fixture.roots[root_ordinal]);
        assert!(matches!(
            nodes[0].receiver,
            PropertyBoundaryForestReceiver::FrameRoot {
                scene_root_ordinal
            } if scene_root_ordinal as usize == root_ordinal
        ));
        assert!(matches!(
            nodes[1].receiver,
            PropertyBoundaryForestReceiver::Surface {
                parent: PropertyBoundaryForestNodeId(parent),
                ..
            } if parent == root.node_span.start
        ));
    }
}

#[test]
fn materialized_root_steps_and_transaction_witness_preserve_root_preorder() {
    let fixture = heterogeneous_fixture();
    let plan = multi_root_plan(&fixture);
    let spans = plan
        .property_scene_seal
        .as_ref()
        .unwrap()
        .effect_scaffold
        .as_ref()
        .unwrap()
        .production_root_step_spans
        .as_ref()
        .unwrap();
    assert_eq!(spans, &[0..1, 1..2]);
    assert_eq!(plan.steps.len(), 2);
    let witness = plan.property_scene_transaction_witness().unwrap();
    assert_eq!(witness.roots.len(), 2);
    assert_eq!(witness.roots[0].top_level_step_span, 0..1);
    assert_eq!(witness.roots[1].top_level_step_span, 1..2);
    assert_eq!(witness.surfaces.len(), 4);
    assert_eq!(witness.top_level_surfaces.len(), 2);
    assert_eq!(witness.surfaces[0].boundary_root, fixture.roots[0]);
    assert_eq!(witness.surfaces[1].boundary_root, fixture.children[0]);
    assert_eq!(witness.surfaces[2].boundary_root, fixture.roots[1]);
    assert_eq!(witness.surfaces[3].boundary_root, fixture.children[1]);
    assert_eq!(witness.top_level_surfaces[0].surface_ordinal, 0);
    assert_eq!(witness.top_level_surfaces[1].surface_ordinal, 2);
}

#[test]
fn cross_root_parent_root_reorder_and_duplicate_frame_root_tampers_fail_closed() {
    let fixture = heterogeneous_fixture();
    let plan = multi_root_plan(&fixture);

    let mut cross_root = plan.clone();
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
        panic!("second root receiver")
    };
    *parent = PropertyBoundaryForestNodeId(0);
    assert!(!forest.is_structurally_canonical());
    assert!(!property_scene_plan_is_sealed(&cross_root));

    let mut reordered = plan.clone();
    reordered
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest
        .roots
        .swap(0, 1);
    assert!(!property_scene_plan_is_sealed(&reordered));

    let mut duplicate = plan;
    let forest = &mut duplicate
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .boundary_forest;
    forest.nodes[3].receiver = PropertyBoundaryForestReceiver::FrameRoot {
        scene_root_ordinal: 1,
    };
    assert!(!forest.is_reviewed_alternating_mixed_forest());
    assert!(!property_scene_plan_is_sealed(&duplicate));
}

#[test]
fn multi_root_top_owner_mismatch_and_nonalternating_edge_are_rejected() {
    let mut fixture = heterogeneous_fixture();
    let wrapper = commit_element(
        &mut fixture.arena,
        Box::new(root_element(0xf5_01ff, Color::rgb(45, 85, 115))),
    );
    fixture.arena.set_parent(fixture.roots[0], Some(wrapper));
    fixture.arena.set_children(wrapper, vec![fixture.roots[0]]);
    fixture.roots[0] = wrapper;
    fixture.properties.sync(&fixture.arena, &fixture.roots);
    fixture
        .generations
        .sync(&fixture.arena, &fixture.roots, &fixture.properties);
    assert!(
        plan_property_effect_scene_scaffold_with_context(
            &fixture.arena,
            &fixture.roots,
            &fixture.properties,
            &fixture.generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .is_err(),
        "multi-root top property owner must equal its scene root",
    );

    let mut fixture = heterogeneous_fixture();
    {
        let mut child = crate::view::test_support::get_element_mut::<Element>(
            &fixture.arena,
            fixture.children[0],
        );
        child.set_opacity(1.0);
        child.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            5.0, 0.0, 0.0,
        ))));
    }
    fixture.properties.sync(&fixture.arena, &fixture.roots);
    fixture
        .generations
        .sync(&fixture.arena, &fixture.roots, &fixture.properties);
    assert!(
        plan_property_effect_scene_scaffold_with_context(
            &fixture.arena,
            &fixture.roots,
            &fixture.properties,
            &fixture.generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .is_err(),
        "T -> T remains non-alternating within one root",
    );
}
