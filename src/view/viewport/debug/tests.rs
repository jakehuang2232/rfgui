use super::*;

#[test]
fn layout_traversal_trace_marks_gate_counts_as_candidates() {
    let profile = super::super::frame::LayoutTraversalProfile {
        root_count: 1,
        measure_candidate_clean_children: 2,
        measure_dirty_children: 1,
        placement_candidate_clean_children: 3,
        placement_dirty_children: 0,
        skipped_child_place_calls: 2,
        ..Default::default()
    };
    let root = TraceRenderNode::with_children(
        "layout_traversal",
        0.0,
        build_layout_traversal_trace_nodes(&profile),
    );
    let trace = format_trace_render_tree(&root);

    assert!(trace.contains("measure_clean_child_candidates (clean=2, dirty=1)"));
    assert!(trace.contains("placement_clean_child_candidates (clean=3, dirty=0)"));
    assert!(trace.contains("skipped_child_place_calls (count=2)"));
}

#[test]
fn layout_place_trace_nests_overlapping_place_timings() {
    let profile = crate::view::base_component::LayoutPlaceProfile {
        node_count: 4,
        place_self_ms: 1.0,
        place_children_ms: 10.0,
        place_flex_children_ms: 8.0,
        place_layout_flex_ms: 3.0,
        place_layout_flow_ms: 5.0,
        non_axis_child_place_ms: 7.0,
        absolute_child_place_ms: 2.0,
        child_place_calls: 6,
        absolute_child_place_calls: 2,
        update_content_size_ms: 1.0,
        clamp_scroll_ms: 0.5,
        recompute_hit_test_ms: 0.25,
        ..Default::default()
    };
    let root =
        TraceRenderNode::with_children("place", 40.0, build_layout_place_trace_nodes(&profile));
    let trace = format_trace_render_tree(&root);

    assert!(trace.contains("├─ place_children"));
    assert!(trace.contains("│  ├─ place_flex_children"));
    assert!(trace.contains("│  │  ├─ place_layout_inline"));
    assert!(trace.contains("│  │  ├─ place_layout_flex"));
    assert!(trace.contains("│  │  └─ place_layout_flow"));
    assert!(trace.contains("│  ├─ child_place (calls=6)"));
    assert!(trace.contains("│  ├─ absolute_child_place (calls=2)"));
    assert!(trace.contains("child_place (calls=6) \u{1b}[32m7.000ms\u{1b}[0m"));
    assert!(trace.contains("absolute_child_place (calls=2) \u{1b}[32m2.000ms\u{1b}[0m"));
}
