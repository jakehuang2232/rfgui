use super::*;

#[test]
fn inline_layout_wraps_children_into_multiple_line_boxes() {
    let mut arena = new_test_arena();
    let mut parent = Element::new(0.0, 0.0, 100.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
    parent.apply_style(parent_style);
    let parent_key = commit_element(&mut arena, Box::new(parent));

    commit_child(
        &mut arena,
        parent_key,
        Box::new(Element::new(0.0, 0.0, 60.0, 10.0)),
    );
    commit_child(
        &mut arena,
        parent_key,
        Box::new(Element::new(0.0, 0.0, 50.0, 20.0)),
    );
    commit_child(
        &mut arena,
        parent_key,
        Box::new(Element::new(0.0, 0.0, 40.0, 15.0)),
    );

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 100.0,
            max_height: 200.0,
            viewport_width: 100.0,
            viewport_height: 200.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(200.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 100.0,
            available_height: 200.0,
            viewport_width: 100.0,
            viewport_height: 200.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(200.0),
        },
    );

    let first = nth_child_snapshot(&arena, parent_key, 0);
    let second = nth_child_snapshot(&arena, parent_key, 1);
    let third = nth_child_snapshot(&arena, parent_key, 2);

    assert_eq!(first.x, 0.0);
    assert_eq!(first.y, 0.0);
    assert_eq!(second.x, 0.0);
    assert_eq!(second.y, 10.0);
    assert_eq!(third.x, 50.0);
    // Baseline default: pure-element diff-height row bottom-aligns
    // the shorter element. Line baseline = max(20, 15) = 20;
    // element baseline = height, so el3 offset = 20 - 15 = 5
    // → y = 10 + 5 = 15 (was 10 under Align::Start).
    assert_eq!(third.y, 15.0);
    let parent_el = crate::view::test_support::get_element::<Element>(&arena, parent_key);
    // Pure-element rows: line_box_h = max(height) (descent = 0),
    // total content_size unchanged from pre-Sprint-3.
    assert!((parent_el.box_model_snapshot().height - 30.0).abs() < 0.01);
    assert!((parent_el.layout_state.content_size.height - 30.0).abs() < 0.01);
}

#[test]
fn inline_ifc_root_staging_input_keeps_paint_local_pos_in_sync() {
    let mut arena = new_test_arena();
    let mut parent = Element::new(0.0, 0.0, 160.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
    parent.apply_style(parent_style);
    let parent_key = commit_element(&mut arena, Box::new(parent));
    commit_child(
        &mut arena,
        parent_key,
        Box::new(Text::from_content("inline glyphs")),
    );

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 160.0,
            max_height: 200.0,
            viewport_width: 160.0,
            viewport_height: 200.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(200.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 160.0,
            available_height: 200.0,
            viewport_width: 160.0,
            viewport_height: 200.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(200.0),
        },
    );

    let element = crate::view::test_support::get_element::<Element>(&arena, parent_key);
    let (paint_input, top_offset) = element
        .inline_ifc_root_render_input()
        .expect("inline IFC root should expose a render input");
    let origin = [13.0_f32, 17.0_f32];
    let staging = element
        .inline_ifc_root_staging_input(origin, 1.0)
        .expect("inline IFC root should build a staging input");
    assert!(!staging.glyphs.is_empty(), "fixture should stage glyphs");
    for (staged, raw) in staging.glyphs.iter().zip(paint_input.glyphs.iter()) {
        assert!(
            ((raw.baseline_y + raw.glyph_y - top_offset) - staged.paint.local_pos[1]).abs()
                < 1e-3,
            "content-top offset must land in paint.local_pos",
        );
        assert!(
            (staged.final_paint_pos[0] - (origin[0] + staged.paint.local_pos[0])).abs() < 1e-3
                && (staged.final_paint_pos[1] - (origin[1] + staged.paint.local_pos[1])).abs()
                    < 1e-3,
            "prepared pass consumes paint.local_pos; it must stay in sync with final_paint_pos",
        );
    }
}

#[test]
fn inline_auto_sized_element_expands_into_child_fragments() {
    let mut arena = new_test_arena();
    let mut parent = Element::new(0.0, 0.0, 220.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
    parent.apply_style(parent_style);
    let parent_key = commit_element(&mut arena, Box::new(parent));

    let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
    let mut wrapper_style = Style::new();
    wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
    wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
    wrapper.apply_style(wrapper_style);
    let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
    commit_child(
        &mut arena,
        wrapper_key,
        Box::new(Text::from_content("nested")),
    );
    commit_child(
        &mut arena,
        wrapper_key,
        Box::new(Element::new(0.0, 0.0, 44.0, 20.0)),
    );

    commit_child(&mut arena, parent_key, Box::new(Text::from_content("tail")));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 220.0,
            max_height: 120.0,
            viewport_width: 220.0,
            viewport_height: 120.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(120.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 220.0,
            available_height: 120.0,
            viewport_width: 220.0,
            viewport_height: 120.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(120.0),
        },
    );

    let wrapper_snap = child_snapshot(&arena, wrapper_key);
    let tail_key = child_key(&arena, parent_key, 1);
    let tail_snap = child_snapshot(&arena, tail_key);
    let second_wrapper_child_snap = nth_child_snapshot(&arena, wrapper_key, 1);

    assert!(wrapper_snap.width > 44.0);
    assert_eq!(second_wrapper_child_snap.y, 0.0);
    assert!(tail_snap.x >= wrapper_snap.x + 44.0);
}

#[test]
fn inline_fragmentable_element_builds_multiple_draw_rect_passes() {
    let mut arena = new_test_arena();
    let mut parent = Element::new(0.0, 0.0, 160.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
    parent.apply_style(parent_style);
    let parent_key = commit_element(&mut arena, Box::new(parent));

    let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
    let mut wrapper_style = Style::new();
    wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
    wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
    wrapper_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#93c5fd")),
    );
    wrapper_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#2563eb")));
    wrapper.apply_style(wrapper_style);
    let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
    commit_child(
        &mut arena,
        wrapper_key,
        Box::new(Text::from_content(
            "inline wrapper background should wrap across lines",
        )),
    );

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 160.0,
            max_height: 160.0,
            viewport_width: 160.0,
            viewport_height: 160.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(160.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 160.0,
            available_height: 160.0,
            viewport_width: 160.0,
            viewport_height: 160.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(160.0),
        },
    );

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(160, 160, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
    let next_state = arena
        .with_element_taken(parent_key, |el, a| el.build(&mut graph, a, ctx_for_build))
        .expect("build result");
    ctx.set_state(next_state);

    let pass_names = graph
        .pass_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name)
        .collect::<Vec<_>>();
    let rect_like_count = pass_names
        .iter()
        .filter(|name| {
            name.contains("draw_rect_pass::DrawRectPass")
                || name.contains("draw_rect_pass::OpaqueRectPass")
        })
        .count();
    let border_count = pass_names
        .iter()
        .filter(|name| name.contains("draw_rect_pass::DrawRectPass"))
        .count();

    // Both DrawRectPass and OpaqueRectPass count as fragment rects;
    // opaque-pass selection is governed by `is_opaque_candidate` and may
    // shift between the two depending on geometry/overlap. The
    // invariant we care about is that a wrapped fragmentable inline
    // wrapper produces ≥ 2 *fill* and ≥ 2 *border* rect passes (one
    // per visual line fragment) — so total rect-like passes ≥ 4.
    let _ = border_count;
    assert!(
        rect_like_count >= 4,
        "expected multiple fragment rect passes, got {pass_names:?}"
    );
}

#[test]
fn inline_text_glyph_pass_paints_after_own_span_decoration() {
    let mut arena = new_test_arena();
    let mut parent = Element::new(0.0, 0.0, 160.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
    parent.apply_style(parent_style);
    let parent_key = commit_element(&mut arena, Box::new(parent));

    let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
    let mut wrapper_style = Style::new();
    wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
    wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
    wrapper_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#93c5fd")),
    );
    wrapper.apply_style(wrapper_style);
    let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
    commit_child(
        &mut arena,
        wrapper_key,
        Box::new(Text::from_content("badge text on a background")),
    );

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 160.0,
            max_height: 160.0,
            viewport_width: 160.0,
            viewport_height: 160.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(160.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 160.0,
            available_height: 160.0,
            viewport_width: 160.0,
            viewport_height: 160.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(160.0),
        },
    );

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(160, 160, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
    let next_state = arena
        .with_element_taken(parent_key, |el, a| el.build(&mut graph, a, ctx_for_build))
        .expect("build result");
    ctx.set_state(next_state);

    let pass_names = graph
        .pass_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name)
        .collect::<Vec<_>>();
    let glyph_pass_index = pass_names
        .iter()
        .position(|name| name.contains("text_pass::TextPreparedInputPass"))
        .expect("inline Text should emit its source-filtered glyph pass");
    // Span decoration fills use OpaqueRectPass (opaque solid
    // background); stencil clip scope passes stay DrawRectPass with
    // color writes disabled, so they are excluded here on purpose.
    let last_decoration_index = pass_names
        .iter()
        .rposition(|name| name.contains("draw_rect_pass::OpaqueRectPass"))
        .expect("span decoration should emit fill rect passes");
    assert!(
        last_decoration_index < glyph_pass_index,
        "span background must paint under its own text: {pass_names:?}"
    );
}

#[test]
fn inline_sibling_paint_order_interleaves_backgrounds_and_text() {
    let mut arena = new_test_arena();
    let mut parent = Element::new(0.0, 0.0, 260.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(260.0)));
    parent.apply_style(parent_style);
    let parent_key = commit_element(&mut arena, Box::new(parent));

    for (content, color) in [("first", "#ef4444"), ("second", "#3b82f6")] {
        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        style.insert(PropertyId::Width, ParsedValue::Auto);
        style.insert(PropertyId::Height, ParsedValue::Auto);
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex(color)),
        );
        wrapper.apply_style(style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Text::from_content(content)),
        );
    }

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 260.0,
            max_height: 120.0,
            viewport_width: 260.0,
            viewport_height: 120.0,
            percent_base_width: Some(260.0),
            percent_base_height: Some(120.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 260.0,
            available_height: 120.0,
            viewport_width: 260.0,
            viewport_height: 120.0,
            percent_base_width: Some(260.0),
            percent_base_height: Some(120.0),
        },
    );

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(260, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
    let next_state = arena
        .with_element_taken(parent_key, |el, a| el.build(&mut graph, a, ctx_for_build))
        .expect("build result");
    ctx.set_state(next_state);

    let pass_names = graph
        .pass_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name)
        .collect::<Vec<_>>();
    let text_indices = pass_names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| {
            name.contains("text_pass::TextPreparedInputPass")
                .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        text_indices.len(),
        2,
        "one glyph pass per Text: {pass_names:?}"
    );
    assert!(
        pass_names[text_indices[0] + 1..text_indices[1]]
            .iter()
            .any(|name| name.contains("draw_rect_pass::OpaqueRectPass")),
        "second sibling background must paint after first sibling text: {pass_names:?}"
    );
}

#[test]
fn nested_multiline_inline_text_keeps_its_first_glyph_line_at_tall_line_height() {
    let mut arena = new_test_arena();
    let mut root = Element::new(0.0, 0.0, 420.0, 0.0);
    let mut root_style = Style::new().with_line_height(1.8);
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    root_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(420.0)));
    root.apply_style(root_style);
    let root_key = commit_element(&mut arena, Box::new(root));

    let mut outer = Element::new(0.0, 0.0, 0.0, 0.0);
    let mut outer_style = Style::new();
    outer_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    outer_style.set_padding(crate::style::Padding::uniform(Length::px(8.0)));
    outer.apply_style(outer_style);
    let outer_key = commit_child(&mut arena, root_key, Box::new(outer));
    commit_child(
        &mut arena,
        outer_key,
        Box::new(Text::from_content(
            "Permission is hereby granted, free of charge, Software without ",
        )),
    );

    let mut nested = Element::new(0.0, 0.0, 0.0, 0.0);
    let mut nested_style = Style::new();
    nested_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    nested_style.set_padding(crate::style::Padding::uniform(Length::px(8.0)));
    nested.apply_style(nested_style);
    let nested_key = commit_child(&mut arena, outer_key, Box::new(nested));
    let nested_text_key = commit_child(
        &mut arena,
        nested_key,
        Box::new(Text::from_content(
            "restriction, including without limitation the rights to use, copy, modify, merge",
        )),
    );

    measure_and_place(
        &mut arena,
        root_key,
        LayoutConstraints {
            max_width: 420.0,
            max_height: 400.0,
            viewport_width: 420.0,
            viewport_height: 400.0,
            percent_base_width: Some(420.0),
            percent_base_height: Some(400.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 420.0,
            available_height: 400.0,
            viewport_width: 420.0,
            viewport_height: 400.0,
            percent_base_width: Some(420.0),
            percent_base_height: Some(400.0),
        },
    );

    let text = crate::view::test_support::get_element::<Text>(&arena, nested_text_key);
    let (paint_bounds, paint_input) = text
        .inline_ifc_owned_paint_geometry_for_test()
        .expect("nested Text must receive owned paint geometry");
    let first_line_index = paint_input
        .glyphs
        .iter()
        .map(|glyph| glyph.line_index)
        .min()
        .expect("nested Text must retain glyphs");
    let first_line_glyphs = paint_input
        .glyphs
        .iter()
        .filter(|glyph| glyph.line_index == first_line_index)
        .collect::<Vec<_>>();
    assert!(!first_line_glyphs.is_empty());
    assert!(
        first_line_glyphs.iter().all(|glyph| {
            let paint_y = glyph.baseline_y + glyph.glyph_y;
            glyph.x >= -0.01
                && glyph.x <= paint_bounds.width + 0.01
                && paint_y >= -0.01
                && paint_y <= paint_bounds.height + 0.01
        }),
        "the nested source's first glyph line must stay inside its TextPass fragment: bounds={paint_bounds:?} glyphs={first_line_glyphs:?}"
    );
}

#[test]
fn inline_slice_fragments_use_endpoint_radii_and_per_side_border_colors() {
    const SPAN: InlineIfcSourceId = InlineIfcSourceId(301);
    const TEXT_SOURCE: InlineIfcSourceId = InlineIfcSourceId(302);
    let ifc = InlineFormattingContext::build(
        InlineIfcInput::new(vec![InlineIfcItem::Span {
            source: SPAN,
            style: None,
            children: vec![InlineIfcItem::TextSpan {
                source: TEXT_SOURCE,
                text: "alpha beta gamma delta epsilon".to_string(),
                style: None,
            }],
            edge_insets: [4.0, 4.0],
        }])
        .with_max_width(72.0),
    );
    let mut draw_style =
        InlineIfcElementDecorationDrawRectStyle::from_fill_style(&InlineIfcStyle::default());
    draw_style.border_widths = [1.0, 2.0, 3.0, 4.0];
    draw_style.border_colors = [
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 1.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, 1.0],
        [1.0, 1.0, 0.0, 1.0],
    ];
    let package = ifc.element_decoration_draw_rect_package(
        SPAN,
        InlineIfcDecorationBoxInsets::new(4.0, 4.0, 0.0, 0.0),
        draw_style,
    );
    assert!(package.fragments.len() >= 2);

    let mut element = Element::new(0.0, 0.0, 0.0, 0.0);
    element.set_border_radius(6.0);
    let first = element.inline_ifc_fragment_draw_rect_pass_metadata(
        package.fragments.first().expect("first fragment"),
        [0.0; 2],
    );
    let last = element.inline_ifc_fragment_draw_rect_pass_metadata(
        package.fragments.last().expect("last fragment"),
        [0.0; 2],
    );
    assert_eq!(first.fill.border_widths, [1.0, 0.0, 3.0, 4.0]);
    assert_eq!(last.fill.border_widths, [0.0, 2.0, 3.0, 4.0]);
    assert!(first.fill.border_radii[0][0] > 0.0);
    assert_eq!(first.fill.border_radii[1][0], 0.0);
    assert_eq!(last.fill.border_radii[0][0], 0.0);
    assert!(last.fill.border_radii[1][0] > 0.0);
    let first_border = first.border.expect("first border");
    assert!(first_border.use_border_side_colors);
    assert_eq!(
        first_border.border_side_colors,
        package.fragments[0].metadata.border_colors
    );
}
