use super::*;

#[test]
fn inline_element_ifc_decoration_package_keeps_multiple_sibling_sources_separate() {
    const FIRST_SOURCE: InlineIfcSourceId = InlineIfcSourceId(51);
    const SECOND_SOURCE: InlineIfcSourceId = InlineIfcSourceId(52);
    let first_content = "alpha beta gamma delta";
    let second_content = "epsilon zeta eta theta";
    let parent_width = 170.0;
    let inset = 8.0;

    let mut arena = new_test_arena();
    let mut parent = Element::new(0.0, 0.0, parent_width, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    parent_style.insert(
        PropertyId::Width,
        ParsedValue::Length(Length::px(parent_width)),
    );
    parent.apply_style(parent_style);
    let parent_key = commit_element(&mut arena, Box::new(parent));

    let mut first = Element::new(0.0, 0.0, 0.0, 0.0);
    let mut first_style = Style::new();
    first_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    first_style.insert(PropertyId::Width, ParsedValue::Auto);
    first_style.insert(PropertyId::Height, ParsedValue::Auto);
    first_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#bfdbfe")),
    );
    first_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#2563eb")));
    first_style.set_padding(crate::style::Padding::uniform(Length::px(7.0)));
    first.apply_style(first_style);
    let first_key = commit_child(&mut arena, parent_key, Box::new(first));
    commit_child(
        &mut arena,
        first_key,
        Box::new(Text::from_content(first_content)),
    );

    let mut second = Element::new(0.0, 0.0, 0.0, 0.0);
    let mut second_style = Style::new();
    second_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    second_style.insert(PropertyId::Width, ParsedValue::Auto);
    second_style.insert(PropertyId::Height, ParsedValue::Auto);
    second_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#fecaca")),
    );
    second_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#dc2626")));
    second_style.set_padding(crate::style::Padding::uniform(Length::px(7.0)));
    second.apply_style(second_style);
    let second_key = commit_child(&mut arena, parent_key, Box::new(second));
    commit_child(
        &mut arena,
        second_key,
        Box::new(Text::from_content(second_content)),
    );

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: parent_width,
            max_height: 220.0,
            viewport_width: parent_width,
            viewport_height: 220.0,
            percent_base_width: Some(parent_width),
            percent_base_height: Some(220.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: parent_width,
            available_height: 220.0,
            viewport_width: parent_width,
            viewport_height: 220.0,
            percent_base_width: Some(parent_width),
            percent_base_height: Some(220.0),
        },
    );

    let first_legacy = crate::view::test_support::get_element::<Element>(&arena, first_key)
        .inline_fragment_rects()
        .to_vec();
    let second_legacy = crate::view::test_support::get_element::<Element>(&arena, second_key)
        .inline_fragment_rects()
        .to_vec();
    assert!(
        first_legacy.len() >= 2 && second_legacy.len() >= 2,
        "fixture should wrap both legacy sibling wrappers; first={first_legacy:?} second={second_legacy:?}"
    );

    let ifc = InlineFormattingContext::build(
        InlineIfcInput::new(vec![
            InlineIfcItem::Span {
                source: FIRST_SOURCE,
                style: Some(InlineIfcStyle {
                    font_size: 16.0,
                    line_height: 1.25,
                    brush: [11, 22, 33, 255],
                    ..InlineIfcStyle::default()
                }),
                children: vec![InlineIfcItem::TextSpan {
                    source: FIRST_SOURCE,
                    text: first_content.to_string(),
                    style: None,
                }],
                edge_insets: [0.0; 2],
            },
            InlineIfcItem::Span {
                source: SECOND_SOURCE,
                style: Some(InlineIfcStyle {
                    font_size: 16.0,
                    line_height: 1.25,
                    brush: [44, 55, 66, 255],
                    ..InlineIfcStyle::default()
                }),
                children: vec![InlineIfcItem::TextSpan {
                    source: SECOND_SOURCE,
                    text: second_content.to_string(),
                    style: None,
                }],
                edge_insets: [0.0; 2],
            },
        ])
        .with_max_width(parent_width - inset * 2.0),
    );
    let first_style =
        InlineIfcElementDecorationDrawRectStyle::from_fill_style(&InlineIfcStyle {
            brush: [11, 22, 33, 255],
            ..InlineIfcStyle::default()
        });
    let second_style =
        InlineIfcElementDecorationDrawRectStyle::from_fill_style(&InlineIfcStyle {
            brush: [44, 55, 66, 255],
            ..InlineIfcStyle::default()
        });
    let first_package = ifc.element_decoration_draw_rect_package(
        FIRST_SOURCE,
        InlineIfcDecorationBoxInsets::new(inset, inset, inset, inset),
        first_style,
    );
    let second_package = ifc.element_decoration_draw_rect_package(
        SECOND_SOURCE,
        InlineIfcDecorationBoxInsets::new(inset, inset, inset, inset),
        second_style,
    );

    assert_eq!(first_package.source, FIRST_SOURCE);
    assert_eq!(second_package.source, SECOND_SOURCE);
    assert!(
        first_package
            .fragments
            .iter()
            .all(|fragment| fragment.source == FIRST_SOURCE
                && fragment.style_key == first_package.style_key),
        "first package should not contain second sibling metadata: {first_package:?}"
    );
    assert!(
        second_package
            .fragments
            .iter()
            .all(|fragment| fragment.source == SECOND_SOURCE
                && fragment.style_key == second_package.style_key),
        "second package should not contain first sibling metadata: {second_package:?}"
    );
    assert_eq!(
        first_package.fragments.len(),
        first_legacy.len(),
        "first sibling IFC package should split like legacy; legacy={first_legacy:?} package={first_package:?}"
    );
    assert_eq!(
        second_package.fragments.len(),
        second_legacy.len(),
        "second sibling IFC package should split like legacy; legacy={second_legacy:?} package={second_package:?}"
    );
}

#[test]
fn inline_element_ifc_demo_coverage_fixes_nested_and_atomic_specs() {
    const OUTER_SOURCE: InlineIfcSourceId = InlineIfcSourceId(101);
    const INNER_SOURCE: InlineIfcSourceId = InlineIfcSourceId(102);
    const ATOMIC_SOURCE: InlineIfcSourceId = InlineIfcSourceId(103);

    let specs = [
        InlineElementIfcDemoSpec {
            name: "Inline Element Test nested wrappers",
            max_width: 150.0,
            include_atomic_box: false,
        },
        InlineElementIfcDemoSpec {
            name: "Mixed Text / Element atomic chip",
            max_width: 170.0,
            include_atomic_box: true,
        },
    ];

    for spec in specs {
        let mut children = vec![
            InlineIfcItem::TextSpan {
                source: OUTER_SOURCE,
                text: "Permission prefix ".to_string(),
                style: None,
            },
            InlineIfcItem::Span {
                source: INNER_SOURCE,
                style: Some(InlineIfcStyle {
                    font_size: 16.0,
                    line_height: 1.25,
                    brush: [59, 130, 246, 255],
                    ..InlineIfcStyle::default()
                }),
                children: vec![InlineIfcItem::TextSpan {
                    source: INNER_SOURCE,
                    text: "restriction including limitation".to_string(),
                    style: None,
                }],
                edge_insets: [0.0; 2],
            },
            InlineIfcItem::TextSpan {
                source: OUTER_SOURCE,
                text: " suffix text continues".to_string(),
                style: None,
            },
        ];
        if spec.include_atomic_box {
            children.insert(
                1,
                InlineIfcItem::AtomicInlineBox {
                    source: ATOMIC_SOURCE,
                    measurement: InlineIfcMeasuredAtomicBox::new(
                        InlineIfcSize::new(42.0, 20.0),
                        InlineIfcAtomicMeasureConstraints::new(Some(spec.max_width)),
                    ),
                },
            );
        }

        let ifc = InlineFormattingContext::build(
            InlineIfcInput::new(vec![InlineIfcItem::Span {
                source: OUTER_SOURCE,
                style: Some(InlineIfcStyle {
                    font_size: 16.0,
                    line_height: 1.25,
                    brush: [191, 219, 254, 255],
                    ..InlineIfcStyle::default()
                }),
                children,
                edge_insets: [0.0; 2],
            }])
            .with_max_width(spec.max_width),
        );
        let snapshot = ifc.text_layout_snapshot();
        let outer_package = ifc.element_decoration_draw_rect_package(
            OUTER_SOURCE,
            InlineIfcDecorationBoxInsets::new(8.0, 8.0, 8.0, 8.0),
            InlineIfcElementDecorationDrawRectStyle::from_fill_style(&InlineIfcStyle {
                brush: [191, 219, 254, 255],
                ..InlineIfcStyle::default()
            }),
        );
        let inner_package = ifc.element_decoration_draw_rect_package(
            INNER_SOURCE,
            InlineIfcDecorationBoxInsets::new(8.0, 8.0, 8.0, 8.0),
            InlineIfcElementDecorationDrawRectStyle::from_fill_style(&InlineIfcStyle {
                brush: [59, 130, 246, 255],
                ..InlineIfcStyle::default()
            }),
        );
        let atomic_package = ifc.atomic_box_placement_package(ATOMIC_SOURCE);

        assert!(
            !snapshot.lines.is_empty()
                && snapshot.lines.iter().any(|line| !line.glyphs.is_empty()),
            "{} should expose text glyph demo payload: {snapshot:?}",
            spec.name
        );
        assert!(
            !outer_package.fragments.is_empty(),
            "{} should expose outer decoration draw-rect package",
            spec.name
        );
        assert!(
            !inner_package.fragments.is_empty(),
            "{} should expose inner decoration draw-rect package",
            spec.name
        );
        assert!(
            outer_package
                .fragments
                .iter()
                .all(|fragment| fragment.source == OUTER_SOURCE)
        );
        assert!(
            inner_package
                .fragments
                .iter()
                .all(|fragment| fragment.source == INNER_SOURCE)
        );
        if spec.include_atomic_box {
            assert_eq!(atomic_package.placements.len(), 1);
            assert_eq!(atomic_package.placements[0].source, ATOMIC_SOURCE);
            assert!(
                snapshot
                    .lines
                    .iter()
                    .flat_map(|line| &line.glyphs)
                    .all(|glyph| glyph.source != ATOMIC_SOURCE),
                "{} atomic box should stay out of glyph payload",
                spec.name
            );
            assert!(
                outer_package
                    .fragments
                    .iter()
                    .all(|fragment| fragment.source != ATOMIC_SOURCE)
            );
        } else {
            assert!(atomic_package.placements.is_empty());
        }
    }
}

/// A child following a mostly-full line receives only the remaining
/// residue width. The solver later places that child on a fresh line
/// if the residue cannot fit it, and the unified IFC package must keep
/// the projected atomic box coherent on that fresh line. Repro: a
/// `TextAreaTextRun` that fills most of the row followed by a
/// projection segment wrapping a short Text.
#[test]
fn text_area_projection_segment_wraps_when_first_fragment_cannot_fit_residue() {
    use crate::view::base_component::text_area::{TextAreaProjectionSegment, TextAreaTextRun};

    let mut arena = new_test_arena();
    let mut parent = Element::new(0.0, 0.0, 220.0, 0.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
    parent.apply_style(style);
    let parent_key = commit_element(&mut arena, Box::new(parent));

    let prev_key = commit_child(
        &mut arena,
        parent_key,
        Box::new(Element::new(0.0, 0.0, 214.0, 18.0)),
    );

    let mut badge = Element::new(0.0, 0.0, 0.0, 0.0);
    let mut badge_style = Style::new();
    badge_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    badge_style.insert(PropertyId::Width, ParsedValue::Auto);
    badge_style.insert(PropertyId::Height, ParsedValue::Auto);
    badge.apply_style(badge_style);
    badge.set_padding_left(8.0);
    badge.set_padding_right(8.0);
    let projection_index = arena.children_of(parent_key).len();
    let projection_key = crate::view::renderer_adapter::arena_insert_child(
        &mut arena,
        parent_key,
        projection_index,
        crate::view::renderer_adapter::ElementDescriptor {
            element: Box::new(TextAreaProjectionSegment::new()),
            children: vec![crate::view::renderer_adapter::ElementDescriptor {
                element: Box::new(badge),
                children: vec![crate::view::renderer_adapter::ElementDescriptor::leaf(
                    Box::new(Text::from_content("{{API_HOST}}")),
                )],
                side_slots: vec![],
            }],
            side_slots: vec![],
        },
    );

    let suffix_key = commit_child(
        &mut arena,
        parent_key,
        Box::new(TextAreaTextRun::new("/v1/users/".to_string(), 0..10)),
    );

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 220.0,
            max_height: 240.0,
            viewport_width: 220.0,
            viewport_height: 240.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(240.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 220.0,
            available_height: 240.0,
            viewport_width: 220.0,
            viewport_height: 240.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(240.0),
        },
    );

    let prev = child_snapshot(&arena, prev_key);
    let projection = child_snapshot(&arena, projection_key);
    let suffix = child_snapshot(&arena, suffix_key);
    assert!(
        projection.y > prev.y + 1.0,
        "projection must wrap to a fresh line when the residue cannot fit it: prev={prev:?}, projection={projection:?}"
    );
    assert!(
        suffix.y >= projection.y - 1.0 && suffix.y < projection.y + projection.height.max(1.0),
        "suffix should stay within the projection line box: projection={projection:?}, suffix={suffix:?}"
    );
    assert!(
        suffix.x >= projection.x + projection.width - 1.0,
        "suffix should be placed after the projection, not overlap it: projection={projection:?}, suffix={suffix:?}"
    );
}

// ---- vertical-align as style prop (Style builder + cascade) ----
