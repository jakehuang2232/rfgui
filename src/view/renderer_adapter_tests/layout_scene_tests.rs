use super::*;

#[test]
fn rsx_fragmentable_badge_text_aligns_with_sibling_inline_text() {
    for vertical_align in [
        VerticalAlign::Baseline,
        VerticalAlign::Top,
        VerticalAlign::Middle,
        VerticalAlign::Bottom,
    ] {
        let tree = rsx! {
            <HostElement style={ElementStylePropSchema {
                layout: Some(Layout::Inline),
                width: Some(Length::px(960.0)),
                gap: Some(Length::px(8.0)),
                line_height: Some(1.2),
                vertical_align: Some(vertical_align),
                ..empty_element_style()
            }}>
                Inline text starts here,
                <HostElement style={ElementStylePropSchema {
                    padding: Some(Padding::uniform(Length::px(8.0))),
                    ..empty_element_style()
                }}>
                    badge test test test test test test test
                </HostElement>
                <HostText>then more text continues after the badge,</HostText>
                <HostElement style={ElementStylePropSchema {
                    width: Some(Length::px(90.0)),
                    height: Some(Length::px(50.0)),
                    padding: Some(Padding::uniform(Length::px(8.0))),
                    ..empty_element_style()
                }}>
                    <HostText>note note note note note note note</HostText>
                </HostElement>
            </HostElement>
        };
        let mut arena = crate::view::node_arena::NodeArena::new();
        let roots = commit_rsx_tree(&mut arena, &tree);
        let root = roots[0];
        measure_and_place(
            &mut arena,
            root,
            crate::view::base_component::LayoutConstraints {
                max_width: 960.0,
                max_height: 240.0,
                viewport_width: 960.0,
                viewport_height: 240.0,
                percent_base_width: Some(960.0),
                percent_base_height: Some(240.0),
            },
            crate::view::base_component::LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 960.0,
                available_height: 240.0,
                viewport_width: 960.0,
                viewport_height: 240.0,
                percent_base_width: Some(960.0),
                percent_base_height: Some(240.0),
            },
        );

        let root_children = arena.children_of(root);
        let lead_key = root_children[0];
        let badge_key = root_children[1];
        let trailing_key = root_children[2];
        let note_key = root_children[3];
        let badge_text_key = arena.children_of(badge_key)[0];
        let note_text_key = arena.children_of(note_key)[0];
        let badge_paint_rect = {
            let node = arena.get(badge_key).expect("badge node");
            node.element
                .as_any()
                .downcast_ref::<BaseElement>()
                .expect("badge element")
                .inline_fragment_rects()[0]
        };

        let lead_fragment = {
            let node = arena.get(lead_key).expect("lead node");
            node.element
                .as_any()
                .downcast_ref::<Text>()
                .expect("lead text")
                .inline_fragment_positions()[0]
                .1
        };
        let badge_fragment = {
            let node = arena.get(badge_text_key).expect("badge text node");
            let fragments = node
                .element
                .as_any()
                .downcast_ref::<Text>()
                .expect("badge text")
                .inline_fragment_positions();
            fragments
                .iter()
                .find(|(content, _)| content.contains("badge"))
                .unwrap_or_else(|| panic!("expected visible badge fragment, got {fragments:?}"))
                .1
        };
        let trailing_fragment = {
            let node = arena.get(trailing_key).expect("trailing node");
            node.element
                .as_any()
                .downcast_ref::<Text>()
                .expect("trailing text")
                .inline_fragment_positions()[0]
                .1
        };
        let note_fragment = {
            let node = arena.get(note_text_key).expect("note text node");
            node.element
                .as_any()
                .downcast_ref::<Text>()
                .expect("note text")
                .inline_fragment_positions()[0]
                .1
        };
        assert!(
            (lead_fragment.y - badge_fragment.y).abs() < 0.5,
            "{vertical_align:?}: lead_y={} badge_y={} trailing_y={} note_y={} should match",
            lead_fragment.y,
            badge_fragment.y,
            trailing_fragment.y,
            note_fragment.y
        );
        let painted_above_text = lead_fragment.y - badge_paint_rect.y;
        assert!(
            (painted_above_text - 8.0).abs() < 0.5,
            "{vertical_align:?}: padding should start above aligned text; badge_paint_y={} lead_text_y={} delta={}",
            badge_paint_rect.y,
            lead_fragment.y,
            painted_above_text
        );
        assert!(
            trailing_fragment.x > badge_fragment.x,
            "{vertical_align:?}: trailing text should follow badge text: trailing_x={} badge_x={}",
            trailing_fragment.x,
            badge_fragment.x
        );
        let trailing_gap = trailing_fragment.x - (badge_paint_rect.x + badge_paint_rect.width);
        assert!(
            trailing_gap >= 7.5,
            "{vertical_align:?}: Layout::Inline gap must reserve at least 8px between the badge decoration and trailing text; gap={trailing_gap} badge={badge_paint_rect:?} trailing={trailing_fragment:?}",
        );

        let note_snapshot = arena
            .get(note_key)
            .expect("note host")
            .element
            .box_model_snapshot();
        assert!(
            (note_snapshot.width - 90.0).abs() < 0.5 && (note_snapshot.height - 50.0).abs() < 0.5,
            "{vertical_align:?}: atomic note host must keep its measured 90x50 size, got {note_snapshot:?}",
        );
        assert!(
            note_snapshot.should_render,
            "{vertical_align:?}: atomic note host must remain inside the IFC root clip"
        );
        let root_snapshot = arena
            .get(root)
            .expect("inline root")
            .element
            .box_model_snapshot();
        assert!(
            root_snapshot.y + root_snapshot.height + 0.5 >= note_snapshot.y + note_snapshot.height,
            "{vertical_align:?}: atomic note must fit inside the IFC root height; root={root_snapshot:?} note={note_snapshot:?}",
        );
    }
}

#[test]
fn text_nodes_keep_expected_layout_bounds_in_scene() {
    let first_panel = host_element_node()
        .with_prop(
            "style",
            style_with_size(
                style_with_radius(style_bg_border("#4CC9F0", "#1D3557", 8.0), 10.0),
                240.0,
                140.0,
            ),
        )
        .with_child(host_element_node().with_prop(
            "style",
            style_with_size(style_bg_border("#FFD166", "#EF476F", 3.0), 72.0, 48.0),
        ))
        .with_child(host_element_node().with_prop(
            "style",
            style_with_size(style_bg_border("#F72585", "#B5179E", 4.0), 120.0, 80.0),
        ))
        .with_child(
            host_text_node()
                .with_prop("font_size", 26)
                .with_prop("style", text_style_with_color("#0F172A"))
                .with_prop("font", "Noto Sans CJK TC")
                .with_child(RsxNode::text("Hello Rust GUI Text Test")),
        );

    let second_panel = host_element_node()
        .with_prop(
            "style",
            style_with_size(
                style_with_radius(style_bg_border("#1E293B", "#38BDF8", 3.0), 16.0),
                240.0,
                180.0,
            ),
        )
        .with_child(
            host_text_node()
                .with_prop("font_size", 22)
                .with_prop("style", text_style_with_color("#E2E8F0"))
                .with_prop("font", "Noto Sans CJK TC")
                .with_child(RsxNode::text("Test Component")),
        )
        .with_child(
            host_text_node()
                .with_prop("font_size", 14)
                .with_prop("style", text_style_with_color("#CBD5E1"))
                .with_prop("font", "Noto Sans CJK TC")
                .with_child(RsxNode::text(
                    "Used to verify event hit-testing and bubbling.",
                )),
        )
        .with_child(
            host_text_node()
                .with_prop("font_size", 14)
                .with_prop("style", text_style_with_color("#F8FAFC"))
                .with_prop("font", "Noto Sans CJK TC")
                .with_child(RsxNode::text("Click Count: 0")),
        );

    let tree = RsxNode::fragment(vec![first_panel, second_panel]);

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    for root in &roots {
        measure_and_place(&mut arena, *root, std_constraints(), std_placement());
    }

    let mut boxes = Vec::new();
    for root in &roots {
        walk_layout(&arena, *root, &mut boxes);
    }
    println!("boxes={boxes:?}");

    assert!(boxes.iter().any(|&(x, y, w, h)| (x - 3.0).abs() < 0.1
        && (y - 3.0).abs() < 0.1
        && w > 120.0
        && h > 20.0));
    assert!(boxes.iter().any(|&(x, y, w, h)| (x - 3.0).abs() < 0.1
        && (y - 3.0).abs() < 0.1
        && w > 80.0
        && h > 12.0));
}

#[test]
fn element_padding_offsets_child_layout() {
    let tree = host_element_node()
        .with_prop(
            "style",
            style_with_size(empty_element_style(), 200.0, 120.0),
        )
        .with_prop("padding_left", 8)
        .with_prop("padding_top", 12)
        .with_prop("padding_right", 16)
        .with_prop("padding_bottom", 10)
        .with_child(
            host_text_node()
                .with_prop("style", text_style_with_size(300.0, 300.0))
                .with_child(RsxNode::text("inner")),
        );

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    for root in &roots {
        measure_and_place(&mut arena, *root, std_constraints(), std_placement());
    }

    let mut boxes = Vec::new();
    for root in &roots {
        walk_layout(&arena, *root, &mut boxes);
    }

    assert!(
        boxes
            .iter()
            .any(|&(x, y, w, h)| x == 0.0 && y == 0.0 && w == 200.0 && h == 120.0)
    );
    assert!(
        boxes
            .iter()
            .any(|&(x, y, w, h)| x == 8.0 && y == 12.0 && w > 0.0 && h > 0.0),
        "boxes={boxes:?}"
    );
}

#[test]
fn flow_row_without_explicit_size_uses_children_content_size() {
    let row_style = ElementStylePropSchema {
        layout: Some(Layout::flex().row().into()),
        gap: Some(Length::px(8.0)),
        ..empty_element_style()
    };

    let tree = host_element_node()
        .with_prop("style", row_style)
        .with_child(
            host_element_node()
                .with_prop("style", style_with_size(empty_element_style(), 98.0, 34.0)),
        )
        .with_child(
            host_element_node()
                .with_prop("style", style_with_size(empty_element_style(), 98.0, 34.0)),
        )
        .with_child(
            host_element_node()
                .with_prop("style", style_with_size(empty_element_style(), 70.0, 34.0)),
        );

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let snapshot = arena.get(root).unwrap().element.box_model_snapshot();
    assert_eq!(snapshot.width, 282.0);
    assert_eq!(snapshot.height, 34.0);
}

#[test]
fn nested_container_percent_height_without_definite_parent_does_not_keep_placeholder_size() {
    let root_style = ElementStylePropSchema {
        width: Some(Length::px(200.0)),
        ..empty_element_style()
    };

    let child_style = ElementStylePropSchema {
        height: Some(Length::percent(100.0)),
        ..empty_element_style()
    };

    let tree = host_element_node()
        .with_prop("style", root_style)
        .with_child(host_element_node().with_prop("style", child_style));

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let child_key = *arena.children_of(root).first().expect("child");
    let root_snapshot = arena.get(root).unwrap().element.box_model_snapshot();
    let child_snapshot = arena.get(child_key).unwrap().element.box_model_snapshot();
    assert_eq!(root_snapshot.height, 0.0);
    assert_eq!(child_snapshot.height, 0.0);
}

// ---------- TextArea (v2 — formerly TextArea) acceptance ----------
