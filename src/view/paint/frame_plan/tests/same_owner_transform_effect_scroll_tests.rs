use super::*;

#[test]
fn same_owner_transform_effect_scroll_plans_marker_only_nested_receivers() {
    let (arena, root, properties, generations) = same_owner_transform_effect_scroll_roles_fixture();
    let plan = plan_property_scroll_interleave_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("same-owner T+E+S planning scaffold");
    let scaffold = plan.property_scroll_planning_scaffold().unwrap();

    assert!(scaffold.transform_effect_receiver_insertions.is_empty());
    let [insertion] = scaffold
        .same_owner_transform_effect_scroll_insertions
        .as_slice()
    else {
        panic!("same-owner T+E+S owns one typed insertion")
    };
    assert_eq!(insertion.owner, root);
    assert_eq!(insertion.transform.owner, root);
    assert_eq!(insertion.effect.owner, root);
    assert_eq!(insertion.scroll.owner, root);
    assert_eq!(insertion.contents_clip.owner, root);
    assert_eq!(insertion.receiver.outer_before_span, 0..0);
    assert_eq!(insertion.receiver.outer_after_span, 1..1);
    assert_eq!(insertion.receiver.inner.before_span, 0..0);
    assert_eq!(insertion.receiver.inner.after_span, 1..1);
    assert_eq!(
        insertion
            .receiver
            .outer_geometry
            .source_bounds
            .width
            .to_bits(),
        insertion.scroll.viewport.width.to_bits()
    );
    assert_eq!(
        insertion
            .receiver
            .outer_geometry
            .source_bounds
            .height
            .to_bits(),
        insertion.scroll.viewport.height.to_bits()
    );
    assert!(insertion.is_canonical());
    assert!(property_scene_plan_is_sealed(&plan));
}
