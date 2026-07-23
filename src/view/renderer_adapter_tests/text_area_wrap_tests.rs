use super::*;

#[test]
fn text_area_auto_wrap_false_cascades_nowrap_to_projection_text() {
    // Repro for the bug where `auto_wrap=false` on TextArea did not
    // disable wrapping inside projection subtrees: the projection's inner
    // <Text> kept its default TextWrap::Wrap, and once preceding inline
    // content consumed line space, projection Text could still wrap and
    // force the trailing run to a new visual line even though
    // `solver_wrap=false`.
    use crate::style::TextWrap;

    let tree = host_text_area_node()
        .with_prop("auto_wrap", false)
        .with_prop("content", "abXYZcd")
        .with_prop(
            "on_render",
            crate::ui::on_text_area_render(
                |render: &mut crate::view::base_component::TextAreaRenderString| {
                    render.range(2..5, |_text_area_node| {
                        host_element_node()
                            .with_child(host_text_node().with_child(RsxNode::text("XYZ")))
                    });
                },
            ),
        );
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let projection_key = arena.children_of(root)[1];
    let inner_text_key = first_text_descendant(&arena, projection_key);
    let text_wrap = {
        let node = arena.get(inner_text_key).expect("inner Text node");
        node.element
            .as_any()
            .downcast_ref::<Text>()
            .expect("inner Text component")
            .text_wrap()
    };
    assert_eq!(
        text_wrap,
        TextWrap::NoWrap,
        "projection's inner Text must inherit TextWrap::NoWrap when TextArea auto_wrap=false",
    );
}

#[test]
fn text_area_auto_wrap_false_keeps_projection_and_trailing_run_on_same_line() {
    // End-to-end repro of the visual bug: with `auto_wrap=false`, a
    // projection placed mid-line (after a wide preceding run that consumed
    // most of the available width) used to wrap the projection's inner
    // Text, pushing the trailing plain Run onto a new line. With the cascade
    // fix, the projection emits a single non-breaking fragment and the
    // outer line stays intact (overflowing horizontally instead — the
    // expected NoWrap behavior).
    let tree = host_text_area_node()
        .with_prop("auto_wrap", false)
        .with_prop("content", "aaaaaaaaaaaaaaaaaaaa{{LONG_TOKEN_NAME}}bbbb")
        .with_prop(
            "on_render",
            crate::ui::on_text_area_render(
                |render: &mut crate::view::base_component::TextAreaRenderString| {
                    render.range(20..38, |_text_area_node| {
                        host_element_node().with_child(
                            host_text_node().with_child(RsxNode::text("{{LONG_TOKEN_NAME}}")),
                        )
                    });
                },
            ),
        );
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    let narrow = crate::view::base_component::LayoutConstraints {
        max_width: 120.0,
        max_height: 600.0,
        viewport_width: 120.0,
        viewport_height: 600.0,
        percent_base_width: Some(120.0),
        percent_base_height: Some(600.0),
    };
    let placement = crate::view::base_component::LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 120.0,
        available_height: 600.0,
        viewport_width: 120.0,
        viewport_height: 600.0,
        percent_base_width: Some(120.0),
        percent_base_height: Some(600.0),
    };
    measure_and_place(&mut arena, root, narrow, placement);

    let children = arena.children_of(root);
    assert_eq!(children.len(), 3, "expected Run / projection / Run layout");
    let y_values: Vec<f32> = children
        .iter()
        .map(|key| arena.get(*key).unwrap().element.box_model_snapshot().y)
        .collect();
    assert_eq!(
        y_values[0], y_values[1],
        "projection should stay on the same line as the preceding Run",
    );
    assert_eq!(
        y_values[1], y_values[2],
        "trailing Run should stay on the same line as the projection (no force_break leak)",
    );
}

#[test]
fn text_area_projection_wrap_moves_token_to_next_line_when_remaining_width_is_too_small() {
    let tree = host_text_area_node()
        .with_prop("auto_wrap", true)
        .with_prop(
            "content",
            "First line with a long value that can wrap when auto wrap is enabled. {{API_HOST}}/v1/users/",
        )
        .with_prop(
            "on_render",
            crate::ui::on_text_area_render(
                |render: &mut crate::view::base_component::TextAreaRenderString| {
                    render.range(70..82, |_text_area_node| {
                        host_element_node().with_child(host_text_node().with_child(
                            RsxNode::text("{{API_HOST}}"),
                        ))
                    });
                },
            ),
        );
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    let narrow = crate::view::base_component::LayoutConstraints {
        max_width: 160.0,
        max_height: 600.0,
        viewport_width: 160.0,
        viewport_height: 600.0,
        percent_base_width: Some(160.0),
        percent_base_height: Some(600.0),
    };
    let placement = crate::view::base_component::LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 160.0,
        available_height: 600.0,
        viewport_width: 160.0,
        viewport_height: 600.0,
        percent_base_width: Some(160.0),
        percent_base_height: Some(600.0),
    };
    measure_and_place(&mut arena, root, narrow, placement);

    let children = arena.children_of(root);
    assert_eq!(children.len(), 3, "expected Run / projection / Run layout");
    let text_key = first_text_descendant(&arena, children[1]);
    let fragments = arena
        .get(text_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<Text>()
        .expect("projection Text")
        .inline_fragment_positions();
    assert_eq!(
        fragments.first().map(|(content, _)| content.as_str()),
        Some("{{API_HOST}}"),
        "projection token should not be split to fill a tiny remaining line fragment: {fragments:?}",
    );
}

#[test]
fn text_area_auto_wrap_toggle_cascades_to_existing_projection_text() {
    // Toggling `auto_wrap` after the initial commit goes through the
    // projection reconcile path (identity-matched RSX, same Element +
    // Text shape). Reconcile must re-cascade `TextWrap` onto the live
    // Text node — otherwise the cached default `TextWrap::Wrap` survives
    // and the visual bug recurs every time the user flips the toggle.
    use crate::style::TextWrap;

    let make_tree = |auto_wrap: bool| {
        host_text_area_node()
            .with_prop("auto_wrap", auto_wrap)
            .with_prop("content", "abXYZcd")
            .with_prop(
                "on_render",
                crate::ui::on_text_area_render(
                    |render: &mut crate::view::base_component::TextAreaRenderString| {
                        render.range(2..5, |_text_area_node| {
                            host_element_node()
                                .with_child(host_text_node().with_child(RsxNode::text("XYZ")))
                        });
                    },
                ),
            )
    };
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &make_tree(true));
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let ctx = crate::view::fiber_work::ApplyContext {
        viewport_style: &crate::style::Style::new(),
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    arena.with_element_taken(root, |element, arena_ref| {
        element.apply_prop(
            arena_ref,
            root,
            &ctx,
            "auto_wrap",
            crate::ui::PropValue::Bool(false),
        );
    });
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let projection_key = arena.children_of(root)[1];
    let inner_text_key = first_text_descendant(&arena, projection_key);
    let text_wrap = {
        let node = arena.get(inner_text_key).expect("inner Text node");
        node.element
            .as_any()
            .downcast_ref::<Text>()
            .expect("inner Text component")
            .text_wrap()
    };
    assert_eq!(
        text_wrap,
        TextWrap::NoWrap,
        "after toggling auto_wrap=true→false, projection's existing Text must pick up the new cascade",
    );

    arena.with_element_taken(root, |element, arena_ref| {
        element.apply_prop(
            arena_ref,
            root,
            &ctx,
            "auto_wrap",
            crate::ui::PropValue::Bool(true),
        );
    });
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let inner_text_key = first_text_descendant(&arena, projection_key);
    let text_wrap = {
        let node = arena.get(inner_text_key).expect("inner Text node");
        node.element
            .as_any()
            .downcast_ref::<Text>()
            .expect("inner Text component")
            .text_wrap()
    };
    assert_eq!(
        text_wrap,
        TextWrap::Wrap,
        "after toggling auto_wrap=false→true, projection's existing Text must fall back to its own/default wrap",
    );
}
