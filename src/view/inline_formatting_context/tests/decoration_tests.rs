use super::*;

#[test]
fn decoration_fragments_describe_multiline_span_rects() {
    let ifc = fixture(72.0);
    let fragments = ifc
        .decoration_fragments()
        .into_iter()
        .filter(|fragment| fragment.source == OUTER)
        .collect::<Vec<_>>();

    assert!(
        fragments.len() >= 2,
        "narrow layout should split a decorated span across lines: {fragments:?}"
    );
    assert!(
        fragments.iter().all(|fragment| {
            fragment.x1 >= fragment.x0
                && fragment.y1 > fragment.y0
                && !fragment.range.is_empty()
                && fragment.style.is_some()
        }),
        "decoration fragments should have drawable rects and source-byte style: {fragments:?}"
    );
    assert!(
        fragments
            .windows(2)
            .any(|pair| pair[0].line_index != pair[1].line_index),
        "span fragments should preserve line identity: {fragments:?}"
    );
}

#[test]
fn element_decoration_payload_expands_span_rects_with_slice_insets() {
    let ifc = fixture(72.0);
    let raw_fragments = ifc
        .decoration_paint_fragments()
        .into_iter()
        .filter(|fragment| fragment.source == OUTER)
        .collect::<Vec<_>>();
    let expanded = ifc.element_decoration_paint_fragments(
        OUTER,
        InlineIfcDecorationBoxInsets::new(8.0, 6.0, 4.0, 2.0),
    );

    assert!(
        raw_fragments.len() >= 2,
        "fixture should split OUTER decoration across lines: {raw_fragments:?}"
    );
    assert_eq!(expanded.len(), raw_fragments.len());

    let first = expanded.first().expect("first expanded fragment");
    let first_raw = raw_fragments.first().expect("first raw fragment");
    assert!(first.is_first_for_source);
    assert!(!first.is_last_for_source);
    assert!((first.rect.x - (first_raw.rect.x - 8.0)).abs() < 0.01);
    assert!((first.rect.y - (first_raw.rect.y - 4.0)).abs() < 0.01);
    assert!((first.rect.width - (first_raw.rect.width + 8.0)).abs() < 0.01);
    assert!((first.rect.height - (first_raw.rect.height + 6.0)).abs() < 0.01);

    let last = expanded.last().expect("last expanded fragment");
    let last_raw = raw_fragments.last().expect("last raw fragment");
    assert!(!last.is_first_for_source);
    assert!(last.is_last_for_source);
    assert!((last.rect.x - last_raw.rect.x).abs() < 0.01);
    assert!((last.rect.right() - (last_raw.rect.right() + 6.0)).abs() < 0.01);
    assert!((last.rect.bottom() - (last_raw.rect.bottom() + 2.0)).abs() < 0.01);
}

#[test]
fn span_decoration_horizontal_insets_do_not_overlap_following_text() {
    const SPAN: InlineIfcSourceId = InlineIfcSourceId(201);
    const INNER_TEXT: InlineIfcSourceId = InlineIfcSourceId(202);
    const SUFFIX: InlineIfcSourceId = InlineIfcSourceId(203);
    let inset = 8.0;
    let ifc = InlineFormattingContext::build(
        InlineIfcInput::new(vec![
            InlineIfcItem::Span {
                source: SPAN,
                style: None,
                children: vec![InlineIfcItem::TextSpan {
                    source: INNER_TEXT,
                    text: "badge".to_string(),
                    style: None,
                }],
                edge_insets: [inset, inset],
            },
            InlineIfcItem::TextSpan {
                source: SUFFIX,
                text: "then".to_string(),
                style: None,
            },
        ])
        .with_max_width(240.0),
    );

    let package = ifc.element_decoration_paint_fragments(
        SPAN,
        InlineIfcDecorationBoxInsets::new(inset, inset, 0.0, 0.0),
    );
    let suffix = ifc
        .source_line_rects(SUFFIX)
        .into_iter()
        .next()
        .expect("suffix rect");
    let fragment = package.first().expect("span decoration fragment");
    assert!(fragment.is_first_for_source && fragment.is_last_for_source);
    assert!(
        fragment.rect.right() <= suffix.x + 0.6,
        "span right padding must end before following text: fragment={fragment:?} suffix={suffix:?}"
    );
}

#[test]
fn atomic_only_span_still_produces_decoration_geometry() {
    const SPAN: InlineIfcSourceId = InlineIfcSourceId(211);
    const ATOMIC: InlineIfcSourceId = InlineIfcSourceId(212);
    let ifc = InlineFormattingContext::build(
        InlineIfcInput::new(vec![InlineIfcItem::Span {
            source: SPAN,
            style: None,
            children: vec![InlineIfcItem::AtomicInlineBox {
                source: ATOMIC,
                measurement: InlineIfcMeasuredAtomicBox::new(
                    InlineIfcSize::new(42.0, 20.0),
                    InlineIfcAtomicMeasureConstraints::new(Some(120.0)),
                ),
            }],
            edge_insets: [8.0, 8.0],
        }])
        .with_max_width(120.0),
    );

    let atomic = ifc
        .atomic_box_placement_package(ATOMIC)
        .placements
        .into_iter()
        .next()
        .expect("atomic placement");
    let fragments = ifc.element_decoration_paint_fragments(
        SPAN,
        InlineIfcDecorationBoxInsets::new(8.0, 8.0, 4.0, 4.0),
    );
    let fragment = fragments.first().expect("atomic-only span decoration");
    assert!(fragment.is_first_for_source && fragment.is_last_for_source);
    assert!(fragment.rect.x <= atomic.rect.x - 7.9);
    assert!(fragment.rect.right() >= atomic.rect.right() + 7.9);
    assert!(fragment.rect.y <= atomic.rect.y - 3.9);
    assert!(fragment.rect.bottom() >= atomic.rect.bottom() + 3.9);
}

#[test]
fn fragmentable_span_vertical_align_moves_glyph_and_caret_together() {
    const SPAN: InlineIfcSourceId = InlineIfcSourceId(221);
    const TEXT: InlineIfcSourceId = InlineIfcSourceId(222);
    const ATOMIC: InlineIfcSourceId = InlineIfcSourceId(223);
    let build = |vertical_align| {
        InlineFormattingContext::build(
            InlineIfcInput::new(vec![
                InlineIfcItem::Span {
                    source: SPAN,
                    style: Some(InlineIfcStyle {
                        vertical_align,
                        ..InlineIfcStyle::default()
                    }),
                    children: vec![InlineIfcItem::TextSpan {
                        source: TEXT,
                        text: "text".to_string(),
                        style: None,
                    }],
                    edge_insets: [0.0; 2],
                },
                InlineIfcItem::AtomicInlineBox {
                    source: ATOMIC,
                    measurement: InlineIfcMeasuredAtomicBox::new(
                        InlineIfcSize::new(20.0, 48.0),
                        InlineIfcAtomicMeasureConstraints::new(Some(160.0)),
                    ),
                },
            ])
            .with_max_width(160.0),
        )
    };
    let baseline = build(crate::style::VerticalAlign::Baseline);
    let top = build(crate::style::VerticalAlign::Top);
    let baseline_glyph_y = baseline.text_pass_paint_input_for_source(TEXT).glyphs[0].baseline_y
        + baseline.text_pass_paint_input_for_source(TEXT).glyphs[0].glyph_y;
    let top_glyph_y = top.text_pass_paint_input_for_source(TEXT).glyphs[0].baseline_y
        + top.text_pass_paint_input_for_source(TEXT).glyphs[0].glyph_y;
    assert!(top_glyph_y < baseline_glyph_y - 1.0);

    let baseline_caret = baseline
        .caret_geometry_for_byte(0, InlineIfcCaretAffinity::Downstream)
        .expect("baseline caret");
    let top_caret = top
        .caret_geometry_for_byte(0, InlineIfcCaretAffinity::Downstream)
        .expect("top caret");
    assert!(top_caret.y < baseline_caret.y - 1.0);
    assert!((top_caret.y - top.source_text_line_rects(TEXT)[0].1.y).abs() < 0.6);
    let top_selection = top
        .selection_rects_for_source_range(TEXT, 0..4)
        .into_iter()
        .next()
        .expect("top selection rect");
    assert!((top_selection.rect.y - top_caret.y).abs() < 0.6);
}

#[test]
fn element_decoration_draw_rect_package_preserves_source_style_and_slice_metadata() {
    let ifc = fixture(72.0);
    let outer_style = style([2, 2, 2, 255], 400);
    let draw_style = InlineIfcElementDecorationDrawRectStyle::new(
        InlineIfcPaintStyleKey::from_style(&outer_style),
        [0.1, 0.2, 0.3, 1.0],
        0.5,
        [1.0, 2.0, 3.0, 4.0],
        [0.8, 0.7, 0.6, 1.0],
    );
    let insets = InlineIfcDecorationBoxInsets::new(8.0, 6.0, 4.0, 2.0);

    let package = ifc.element_decoration_draw_rect_package(OUTER, insets, draw_style);

    assert_eq!(package.source, OUTER);
    assert_eq!(package.style_key, draw_style.style_key);
    assert_eq!(package.slice_insets, insets);
    assert!(
        package.fragments.len() >= 2,
        "wrapped outer span should produce multiple draw rect fragments: {package:?}"
    );
    for fragment in &package.fragments {
        assert_eq!(fragment.source, OUTER);
        assert_eq!(fragment.style_key, draw_style.style_key);
        assert_eq!(fragment.slice_insets, insets);
        assert_eq!(
            fragment.metadata.position,
            [fragment.rect.x, fragment.rect.y]
        );
        assert_eq!(
            fragment.metadata.size,
            [fragment.rect.width, fragment.rect.height]
        );
        assert_eq!(fragment.metadata.fill_color, draw_style.fill_color);
        assert_eq!(fragment.metadata.opacity, draw_style.opacity);
        assert_eq!(fragment.metadata.border_widths, draw_style.border_widths);
        assert_eq!(fragment.metadata.border_colors, draw_style.border_colors);
    }
    assert!(
        package
            .fragments
            .first()
            .is_some_and(|fragment| fragment.is_first_for_source)
    );
    assert!(
        package
            .fragments
            .last()
            .is_some_and(|fragment| fragment.is_last_for_source)
    );
}

#[test]
fn element_package_distributor_splits_nested_atomic_and_missing_sources() {
    let ifc = fixture(72.0);
    let outer_draw_style = InlineIfcElementDecorationDrawRectStyle::new(
        InlineIfcPaintStyleKey::from_style(&style([2, 2, 2, 255], 400)),
        [0.2, 0.3, 0.4, 1.0],
        0.8,
        [1.0, 1.0, 1.0, 1.0],
        [0.1, 0.1, 0.1, 1.0],
    );
    let inner_draw_style = InlineIfcElementDecorationDrawRectStyle::new(
        InlineIfcPaintStyleKey::from_style(&style([3, 3, 3, 255], 700)),
        [0.7, 0.2, 0.2, 1.0],
        0.7,
        [2.0, 2.0, 2.0, 2.0],
        [0.6, 0.0, 0.0, 1.0],
    );
    let missing = InlineIfcSourceId(999);
    let distributor = ifc.element_package_distributor(
        InlineIfcElementPackageDistributionInput::new()
            .with_decoration_source(InlineIfcElementDecorationPackageSource::new(
                OUTER,
                InlineIfcDecorationBoxInsets::new(4.0, 5.0, 1.0, 2.0),
                outer_draw_style,
            ))
            .with_decoration_source(InlineIfcElementDecorationPackageSource::new(
                INNER,
                InlineIfcDecorationBoxInsets::new(1.0, 1.0, 1.0, 1.0),
                inner_draw_style,
            ))
            .with_decoration_source(InlineIfcElementDecorationPackageSource::new(
                missing,
                InlineIfcDecorationBoxInsets::new(9.0, 9.0, 9.0, 9.0),
                outer_draw_style,
            ))
            .with_atomic_source(BOX_NODE)
            .with_atomic_source(missing),
    );

    let outer = distributor
        .decoration_package(OUTER)
        .expect("outer span should receive decoration package");
    let inner = distributor
        .decoration_package(INNER)
        .expect("inner span should receive decoration package");
    let atomic = distributor
        .atomic_package(BOX_NODE)
        .expect("atomic source should receive placement package");

    assert!(outer.fragments.len() >= 2);
    assert!(!inner.fragments.is_empty());
    assert!(outer.fragments.iter().all(|fragment| {
        fragment.source == OUTER
            && fragment.style_key == outer_draw_style.style_key
            && fragment.metadata.fill_color == outer_draw_style.fill_color
            && fragment.metadata.border_widths == outer_draw_style.border_widths
    }));
    assert!(inner.fragments.iter().all(|fragment| {
        fragment.source == INNER
            && fragment.style_key == inner_draw_style.style_key
            && fragment.metadata.fill_color == inner_draw_style.fill_color
            && fragment.metadata.border_widths == inner_draw_style.border_widths
    }));
    assert_eq!(atomic.source, BOX_NODE);
    assert_eq!(atomic.placements.len(), 1);
    assert_eq!(atomic.placements[0].source, BOX_NODE);
    assert!(
        distributor.decoration_package(BOX_NODE).is_none(),
        "atomic source must not be synthesized into decoration package"
    );
    assert!(distributor.package(missing).is_none());
    assert_eq!(distributor.packages().count(), 3);
}

#[test]
fn element_package_distributor_keeps_multiple_sibling_sources_separate() {
    let sibling = InlineIfcSourceId(6);
    let ifc = InlineFormattingContext::build(
        InlineIfcInput::new(vec![
            InlineIfcItem::Span {
                source: OUTER,
                style: Some(style_with_metrics([10, 20, 30, 255], 400, 15.0, 1.25)),
                children: vec![InlineIfcItem::TextSpan {
                    source: OUTER,
                    text: "first inline sibling wraps ".to_string(),
                    style: None,
                }],
                edge_insets: [0.0; 2],
            },
            InlineIfcItem::Span {
                source: sibling,
                style: Some(style_with_metrics([40, 50, 60, 255], 700, 15.0, 1.25)),
                children: vec![InlineIfcItem::TextSpan {
                    source: sibling,
                    text: "second inline sibling wraps too".to_string(),
                    style: None,
                }],
                edge_insets: [0.0; 2],
            },
        ])
        .with_max_width(96.0),
    );
    let outer_style = InlineIfcElementDecorationDrawRectStyle::new(
        InlineIfcPaintStyleKey::from_style(&style_with_metrics(
            [10, 20, 30, 255],
            400,
            15.0,
            1.25,
        )),
        [0.1, 0.2, 0.3, 1.0],
        0.9,
        [1.0, 2.0, 3.0, 4.0],
        [0.0, 0.0, 0.0, 1.0],
    );
    let sibling_style = InlineIfcElementDecorationDrawRectStyle::new(
        InlineIfcPaintStyleKey::from_style(&style_with_metrics(
            [40, 50, 60, 255],
            700,
            15.0,
            1.25,
        )),
        [0.4, 0.5, 0.6, 1.0],
        0.85,
        [4.0, 3.0, 2.0, 1.0],
        [1.0, 0.0, 0.0, 1.0],
    );

    let distributor = ifc.element_package_distributor(
        InlineIfcElementPackageDistributionInput::new()
            .with_decoration_source(InlineIfcElementDecorationPackageSource::new(
                OUTER,
                InlineIfcDecorationBoxInsets::new(1.0, 2.0, 3.0, 4.0),
                outer_style,
            ))
            .with_decoration_source(InlineIfcElementDecorationPackageSource::new(
                sibling,
                InlineIfcDecorationBoxInsets::new(4.0, 3.0, 2.0, 1.0),
                sibling_style,
            )),
    );
    let outer = distributor
        .decoration_package(OUTER)
        .expect("outer sibling package");
    let sibling_package = distributor
        .decoration_package(sibling)
        .expect("second sibling package");

    assert!(outer.fragments.len() >= 2);
    assert!(sibling_package.fragments.len() >= 2);
    assert!(outer.fragments.iter().all(|fragment| {
        fragment.source == OUTER
            && fragment.style_key == outer_style.style_key
            && fragment.metadata.fill_color == outer_style.fill_color
    }));
    assert!(sibling_package.fragments.iter().all(|fragment| {
        fragment.source == sibling
            && fragment.style_key == sibling_style.style_key
            && fragment.metadata.fill_color == sibling_style.fill_color
    }));
    assert_ne!(outer.style_key, sibling_package.style_key);
    assert_eq!(distributor.atomic_package(OUTER), None);
    assert_eq!(distributor.packages().count(), 2);
}

#[test]
fn nested_span_decoration_keeps_source_style_identity_separate() {
    let ifc = fixture(180.0);
    let outer = ifc
        .decoration_paint_fragments()
        .into_iter()
        .find(|fragment| fragment.source == OUTER && fragment.range.start < fragment.range.end)
        .expect("outer decoration should exist");
    let inner = ifc
        .decoration_paint_fragments()
        .into_iter()
        .find(|fragment| fragment.source == INNER)
        .expect("inner decoration should exist");

    assert_eq!(
        outer.style.as_ref().map(|style| style.brush),
        Some([2, 2, 2, 255])
    );
    assert_eq!(
        inner.style.as_ref().map(|style| style.brush),
        Some([3, 3, 3, 255])
    );
    assert_eq!(
        outer.style.as_ref().map(InlineIfcPaintStyleKey::from_style),
        Some(InlineIfcPaintStyleKey::from_style(&style(
            [2, 2, 2, 255],
            400
        )))
    );
    assert_eq!(
        inner.style.as_ref().map(InlineIfcPaintStyleKey::from_style),
        Some(InlineIfcPaintStyleKey::from_style(&style(
            [3, 3, 3, 255],
            700
        )))
    );
}
