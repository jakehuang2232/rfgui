use super::super::*;
use crate::style::{Color, Layout, ParsedValue, PropertyId, Style};
use crate::view::Viewport;
use crate::view::base_component::{Element, LayoutConstraints, LayoutPlacement, UiBuildContext};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::frame_graph::FrameGraph;
use crate::view::node_arena::NodeArena;
use crate::view::paint::frame_plan::{
    PropertyBoundaryForestNodeId, PropertyBoundaryForestReceiver, TransformSurfacePlanContext,
    plan_property_effect_scene_with_context,
};
use crate::view::test_support::{commit_child, commit_element, measure_and_place, new_test_arena};

struct CompilerPlainRootFixture {
    plan: crate::view::paint::FramePaintPlan,
    forest: crate::view::paint::frame_plan::PropertyBoundaryForest,
    stamps: Vec<RetainedSurfaceRasterStamp>,
}

fn element(stable_id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(stable_id, 0.0, 0.0, 102.0, 76.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn fixture() -> CompilerPlainRootFixture {
    let mut arena: NodeArena = new_test_arena();
    let plain_before = commit_element(
        &mut arena,
        Box::new(element(0xf5_4101, Color::rgb(35, 75, 115))),
    );
    let property_a = commit_element(
        &mut arena,
        Box::new(element(0xf5_4102, Color::rgb(25, 55, 95))),
    );
    let child_a = commit_child(
        &mut arena,
        property_a,
        Box::new(element(0xf5_4103, Color::rgb(165, 65, 35))),
    );
    let plain_between = commit_element(
        &mut arena,
        Box::new(element(0xf5_4104, Color::rgb(45, 125, 85))),
    );
    let property_b = commit_element(
        &mut arena,
        Box::new(element(0xf5_4105, Color::rgb(125, 75, 155))),
    );
    let child_b = commit_child(
        &mut arena,
        property_b,
        Box::new(element(0xf5_4106, Color::rgb(55, 135, 175))),
    );
    let plain_after = commit_element(
        &mut arena,
        Box::new(element(0xf5_4107, Color::rgb(145, 95, 45))),
    );
    let roots = vec![
        plain_before,
        property_a,
        plain_between,
        property_b,
        plain_after,
    ];
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
    for &root in &roots {
        measure_and_place(&mut arena, root, constraints, placement);
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
    let plan = plan_property_effect_scene_with_context(
        &arena,
        &roots,
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .unwrap();
    let forest = plan.property_boundary_forest().unwrap();
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(360, 260, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    let stamps = crate::view::paint::prepare_retained_property_scene_stamps_for_test(
        &Viewport::new(),
        &plan,
        &graph,
        &ctx,
    )
    .unwrap();
    CompilerPlainRootFixture {
        plan,
        forest,
        stamps,
    }
}

#[test]
fn empty_root_spans_require_no_fake_stamps_in_the_joint_compiler_transaction() {
    let fixture = fixture();
    assert_eq!(
        fixture
            .forest
            .roots
            .iter()
            .map(|root| root.node_span.clone())
            .collect::<Vec<_>>(),
        vec![0..0, 0..2, 2..2, 2..4, 4..4],
    );
    assert_eq!(fixture.stamps.len(), 4);
    let transaction = RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
        fixture.forest.clone(),
        &fixture.stamps,
    )
    .expect("plain roots add no compiler surface stamp");
    assert!(transaction.is_canonical());
    assert_eq!(transaction.surface_count(), 4);
    assert!(transaction.validates_forest_and_ordered_stamps(&fixture.forest, &fixture.stamps));
    assert!(fixture.plan.property_scene_transaction_witness().is_some());
}

#[test]
fn fake_plain_span_cross_root_parent_empty_forest_and_stamp_tampers_reject() {
    let fixture = fixture();

    let mut fake_span = fixture.forest.clone();
    fake_span.roots[0].node_span = 0..1;
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(fake_span, &fixture.stamps,)
            .is_none()
    );

    let mut cross_root = fixture.forest.clone();
    let PropertyBoundaryForestReceiver::Surface { parent, .. } = &mut cross_root.nodes[3].receiver
    else {
        unreachable!()
    };
    *parent = PropertyBoundaryForestNodeId(0);
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(cross_root, &fixture.stamps,)
            .is_none()
    );

    let mut empty = fixture.forest.clone();
    empty.nodes.clear();
    for root in &mut empty.roots {
        root.node_span = 0..0;
    }
    assert!(RetainedPropertyBoundaryForestTransactionStamp::new_for_test(empty, &[]).is_none());

    let mut reordered = fixture.stamps.clone();
    reordered.swap(1, 2);
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
            fixture.forest.clone(),
            &reordered,
        )
        .is_none()
    );
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
            fixture.forest,
            &fixture.stamps[..3],
        )
        .is_none()
    );
}
