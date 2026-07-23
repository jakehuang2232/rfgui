use super::*;

#[test]
fn phase_5b_cached_placement_metadata_marks_base_only_nested_subtree_replayable() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let candidate = viewport.scene.node_arena.children_of(root_key)[0];
    viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(root_key);

    let metadata = viewport
        .scene
        .node_arena
        .cached_placement_eligibility_metadata(candidate);
    assert!(
        metadata.first_blocker().is_none(),
        "base-only nested Element subtree should have no cached placement replay blocker"
    );
    assert!(!metadata.contains_non_base_element);
    assert!(!metadata.contains_anchor_name);
    assert!(!metadata.contains_anchor_ref);
    assert!(!metadata.contains_absolute_descendant);
    assert!(!metadata.contains_runtime_layout_state);
}

#[test]
fn phase_5b_cached_placement_metadata_marks_text_area_descendant_as_non_base() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_with_text_area_descendant_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 180.0, 100.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let candidate = viewport.scene.node_arena.children_of(root_key)[0];
    viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(root_key);

    let metadata = viewport
        .scene
        .node_arena
        .cached_placement_eligibility_metadata(candidate);
    assert!(metadata.contains_non_base_element);
    assert_eq!(
        metadata.first_blocker(),
        Some(crate::view::base_component::PlacementSkipFailureReason::NonBaseElement)
    );
}

#[test]
fn phase_5b_cached_placement_metadata_marks_anchor_and_absolute_descendants() {
    let mut anchor_viewport = Viewport::new();
    anchor_viewport
        .render_rsx(&nested_grid_with_anchor_descendant_tree())
        .expect("cold render");
    run_layout_for_test(&mut anchor_viewport, 120.0, 80.0);
    let anchor_root = anchor_viewport.scene.ui_root_keys[0];
    let anchor_candidate = anchor_viewport.scene.node_arena.children_of(anchor_root)[0];
    anchor_viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(anchor_root);
    let anchor_metadata = anchor_viewport
        .scene
        .node_arena
        .cached_placement_eligibility_metadata(anchor_candidate);
    assert!(anchor_metadata.contains_anchor_name);
    assert_eq!(
        anchor_metadata.first_blocker(),
        Some(crate::view::base_component::PlacementSkipFailureReason::AnchorName)
    );

    let mut absolute_viewport = Viewport::new();
    absolute_viewport
        .render_rsx(&nested_grid_with_absolute_descendant_tree())
        .expect("cold render");
    run_layout_for_test(&mut absolute_viewport, 120.0, 80.0);
    let absolute_root = absolute_viewport.scene.ui_root_keys[0];
    let absolute_candidate = absolute_viewport
        .scene
        .node_arena
        .children_of(absolute_root)[0];
    absolute_viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(absolute_root);
    let absolute_metadata = absolute_viewport
        .scene
        .node_arena
        .cached_placement_eligibility_metadata(absolute_candidate);
    assert!(absolute_metadata.contains_absolute_descendant);
    assert_eq!(
        absolute_metadata.first_blocker(),
        Some(crate::view::base_component::PlacementSkipFailureReason::AbsoluteDescendant)
    );
}

#[test]
fn phase_5b_cached_placement_metadata_refreshes_after_anchor_mutation() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let candidate = viewport.scene.node_arena.children_of(root_key)[0];
    let descendant = viewport.scene.node_arena.children_of(candidate)[0];
    viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(root_key);
    assert!(
        !viewport
            .scene
            .node_arena
            .cached_placement_eligibility_metadata(candidate)
            .contains_anchor_name
    );

    viewport
        .scene
        .node_arena
        .mutate_element_with_invalidation(descendant, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>()
                .expect("descendant element")
                .set_anchor_name(Some(crate::style::AnchorName::new("phase_5b_anchor")));
            cx.invalidate(crate::view::base_component::DirtyPassMask::PLACEMENT);
        })
        .expect("descendant exists");

    assert!(
        viewport.scene.node_arena.subtree_dirty_intersects(
            candidate,
            crate::view::base_component::DirtyPassMask::PLACEMENT,
        ),
        "dirty cache remains the first guard while metadata may be stale"
    );
    viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(root_key);
    let refreshed = viewport
        .scene
        .node_arena
        .cached_placement_eligibility_metadata(candidate);
    assert!(refreshed.contains_anchor_name);
    assert_eq!(
        refreshed.first_blocker(),
        Some(crate::view::base_component::PlacementSkipFailureReason::AnchorName)
    );
}
