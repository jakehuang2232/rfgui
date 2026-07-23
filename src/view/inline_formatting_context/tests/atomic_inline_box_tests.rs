use super::*;

#[test]
fn atomic_inline_box_uses_measured_size_for_parley_placement() {
    let input = InlineIfcInput::new(vec![
        InlineIfcItem::TextSpan {
            source: ROOT,
            text: "before ".to_string(),
            style: None,
        },
        InlineIfcItem::AtomicInlineBox {
            source: BOX_NODE,
            measurement: measured_box(42.0, 21.0),
        },
        InlineIfcItem::TextSpan {
            source: ROOT,
            text: " after".to_string(),
            style: None,
        },
    ])
    .with_max_width(240.0);
    let ifc = InlineFormattingContext::build(input);

    let placement = ifc
        .inline_box_placements()
        .into_iter()
        .find(|placement| placement.source == BOX_NODE)
        .expect("measured atomic box should have a placement");

    assert!((placement.width - 42.0).abs() < 0.01);
    assert!((placement.height - 21.0).abs() < 0.01);
    assert_eq!(ifc.source_for_inline_box(placement.id), Some(BOX_NODE));
}

#[test]
fn atomic_inline_box_remains_whole_when_remaining_line_width_is_too_small() {
    let input = InlineIfcInput::new(vec![
        InlineIfcItem::TextSpan {
            source: ROOT,
            text: "prefix ".to_string(),
            style: None,
        },
        InlineIfcItem::AtomicInlineBox {
            source: BOX_NODE,
            measurement: measured_box(80.0, 16.0),
        },
        InlineIfcItem::TextSpan {
            source: ROOT,
            text: " suffix".to_string(),
            style: None,
        },
    ])
    .with_max_width(64.0);
    let ifc = InlineFormattingContext::build(input);
    let placements = ifc.inline_box_placements();

    assert_eq!(
        placements.len(),
        1,
        "atomic inline boxes should produce one positioned box, not text/glyph fragments"
    );
    assert_eq!(placements[0].source, BOX_NODE);
    assert!((placements[0].width - 80.0).abs() < 0.01);
    assert!((placements[0].height - 16.0).abs() < 0.01);
}

#[test]
fn atomic_measure_constraints_preserve_future_element_measure_inputs() {
    let constraints = InlineIfcAtomicMeasureConstraints {
        max_width: Some(144.0),
        available_height: Some(96.0),
        viewport: Some(InlineIfcSize::new(320.0, 240.0)),
        percent_base: InlineIfcPercentBase::new(Some(180.0), Some(72.0)),
        sizing: InlineIfcAtomicSizingRules {
            min_width: Some(24.0),
            max_width: Some(128.0),
            min_height: Some(12.0),
            max_height: Some(64.0),
            intrinsic_size: Some(InlineIfcIntrinsicSize::new(
                18.0,
                160.0,
                Some(80.0),
                Some(32.0),
            )),
        },
    };
    let input = InlineIfcInput::new(vec![InlineIfcItem::AtomicInlineBox {
        source: BOX_NODE,
        measurement: InlineIfcMeasuredAtomicBox::new(
            InlineIfcSize::new(64.0, 20.0),
            constraints,
        ),
    }])
    .with_max_width(180.0);
    let ifc = InlineFormattingContext::build(input);
    let mapping = ifc.inline_boxes().first().expect("expected inline box");

    assert_eq!(mapping.measurement.constraints.max_width, Some(144.0));
    assert_eq!(mapping.measurement.constraints.available_height, Some(96.0));
    assert_eq!(
        mapping.measurement.constraints.viewport,
        Some(InlineIfcSize::new(320.0, 240.0))
    );
    assert_eq!(
        mapping.measurement.constraints.percent_base,
        InlineIfcPercentBase::new(Some(180.0), Some(72.0))
    );
    assert_eq!(
        mapping
            .measurement
            .constraints
            .sizing
            .intrinsic_size
            .map(|size| size.max_content_width),
        Some(160.0)
    );
}

#[test]
fn multiple_atomic_inline_boxes_keep_distinct_sources_and_measurements() {
    let input = InlineIfcInput::new(vec![
        InlineIfcItem::AtomicInlineBox {
            source: BOX_NODE,
            measurement: measured_box(20.0, 10.0),
        },
        InlineIfcItem::TextSpan {
            source: ROOT,
            text: " gap ".to_string(),
            style: None,
        },
        InlineIfcItem::AtomicInlineBox {
            source: SECOND_BOX_NODE,
            measurement: measured_box(36.0, 12.0),
        },
    ])
    .with_max_width(160.0);
    let ifc = InlineFormattingContext::build(input);
    let placements = ifc.inline_box_placements();

    assert_eq!(placements.len(), 2);
    assert_ne!(placements[0].id, placements[1].id);
    assert_eq!(ifc.source_for_inline_box(placements[0].id), Some(BOX_NODE));
    assert_eq!(
        ifc.source_for_inline_box(placements[1].id),
        Some(SECOND_BOX_NODE)
    );
    assert!((placements[0].width - 20.0).abs() < 0.01);
    assert!((placements[1].width - 36.0).abs() < 0.01);

    let first_package = ifc.atomic_box_placement_package(BOX_NODE);
    let second_package = ifc.atomic_box_placement_package(SECOND_BOX_NODE);
    assert_eq!(first_package.source, BOX_NODE);
    assert_eq!(second_package.source, SECOND_BOX_NODE);
    assert_eq!(first_package.placements.len(), 1);
    assert_eq!(second_package.placements.len(), 1);
    assert_eq!(first_package.placements[0].source, BOX_NODE);
    assert_eq!(second_package.placements[0].source, SECOND_BOX_NODE);
    assert_ne!(
        first_package.placements[0].id,
        second_package.placements[0].id
    );
    assert!((first_package.placements[0].rect.width - 20.0).abs() < 0.01);
    assert!((second_package.placements[0].rect.width - 36.0).abs() < 0.01);
}
