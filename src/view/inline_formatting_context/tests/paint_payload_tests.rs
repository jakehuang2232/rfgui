use super::*;

#[test]
fn text_paint_payload_preserves_nested_style_and_font_identity() {
    let ifc = fixture(240.0);
    let output = ifc.text_paint_output();
    let outer_start = ifc.backing_text().find("outer").unwrap();
    let strong_start = ifc.backing_text().find("strong").unwrap();

    let outer_run = output
        .runs
        .iter()
        .find(|run| run.range.contains(&outer_start))
        .expect("outer text should produce a text paint run");
    let strong_run = output
        .runs
        .iter()
        .find(|run| run.range.contains(&strong_start))
        .expect("nested strong text should produce a text paint run");

    assert_eq!(outer_run.source, OUTER);
    assert_eq!(outer_run.style.brush, [2, 2, 2, 255]);
    assert_eq!(outer_run.style.font_weight, 400);
    assert_eq!(strong_run.source, INNER);
    assert_eq!(strong_run.style.brush, [3, 3, 3, 255]);
    assert_eq!(strong_run.style.font_weight, 700);
    assert_ne!(outer_run.batch_key, strong_run.batch_key);
    assert!(outer_run.glyphs.iter().all(|glyph| {
        glyph.batch_key == outer_run.batch_key
            && glyph.font_data_id == outer_run.batch_key.font_data_id
            && glyph.font_index == outer_run.batch_key.font_index
            && glyph.normalized_coords_hash == outer_run.batch_key.normalized_coords_hash
            && glyph.font_data.is_some()
            && (glyph.font_size - outer_run.batch_key.font_size()).abs() < 0.01
    }));
    assert!(strong_run.glyphs.iter().all(|glyph| {
        glyph.batch_key == strong_run.batch_key
            && glyph.font_data_id == strong_run.batch_key.font_data_id
            && glyph.font_index == strong_run.batch_key.font_index
            && glyph.normalized_coords_hash == strong_run.batch_key.normalized_coords_hash
            && glyph.font_data.is_some()
            && (glyph.font_size - strong_run.batch_key.font_size()).abs() < 0.01
    }));
}

#[test]
fn text_paint_payload_keeps_atomic_box_sources_out_of_glyphs() {
    let ifc = fixture(180.0);
    let output = ifc.text_paint_output();
    let placements = ifc.inline_box_placements();

    assert!(
        !output.glyphs.is_empty(),
        "text should produce paint glyphs"
    );
    assert_eq!(placements.len(), 1);
    assert_eq!(placements[0].source, BOX_NODE);
    assert!(
        output.glyphs.iter().all(|glyph| glyph.source != BOX_NODE),
        "paint glyphs should not inherit atomic inline box source ids: {output:?}"
    );
    assert!(
        output.runs.iter().all(|run| run.source != BOX_NODE),
        "paint runs should not inherit atomic inline box source ids: {output:?}"
    );
}

#[test]
fn decoration_paint_payload_preserves_multiline_rects_and_style() {
    let ifc = fixture(72.0);
    let fragments = ifc
        .decoration_paint_fragments()
        .into_iter()
        .filter(|fragment| fragment.source == OUTER)
        .collect::<Vec<_>>();

    assert!(
        fragments.len() >= 2,
        "narrow layout should produce multiline decoration paint fragments: {fragments:?}"
    );
    assert!(
        fragments.iter().all(|fragment| {
            fragment.rect.width >= 0.0
                && fragment.rect.height > 0.0
                && !fragment.range.is_empty()
                && fragment.style.is_some()
        }),
        "decoration paint fragments should preserve drawable rects and resolved style: {fragments:?}"
    );
    assert!(
        fragments
            .windows(2)
            .any(|pair| pair[0].line_index != pair[1].line_index),
        "decoration paint fragments should preserve line identity: {fragments:?}"
    );
}

#[test]
fn atomic_inline_box_mixed_text_stays_out_of_text_and_decoration_payloads() {
    let ifc = fixture(120.0);
    let snapshot = ifc.text_layout_snapshot();
    let package = ifc.atomic_box_placement_package(BOX_NODE);

    assert!(
        snapshot
            .inline_boxes
            .iter()
            .any(|placement| placement.source == BOX_NODE),
        "atomic inline box should have a placement in the mixed IFC snapshot: {snapshot:?}"
    );
    assert!(
        snapshot
            .lines
            .iter()
            .flat_map(|line| &line.glyphs)
            .all(|glyph| glyph.source != BOX_NODE),
        "atomic inline box must not enter text glyph payload: {snapshot:?}"
    );
    assert!(
        snapshot
            .decorations
            .iter()
            .all(|fragment| fragment.source != BOX_NODE),
        "atomic inline box must not enter span decoration payload: {snapshot:?}"
    );
    assert!(
        snapshot.lines.iter().any(|line| !line.glyphs.is_empty())
            && !snapshot.inline_boxes.is_empty(),
        "mixed text and atomic box should coexist in the same IFC snapshot: {snapshot:?}"
    );
    assert_eq!(package.source, BOX_NODE);
    assert_eq!(
        package.placements.len(),
        1,
        "atomic placement package should expose one placement for this fixture: {package:?}"
    );
    let placement = package.placements.first().expect("atomic placement");
    assert_eq!(placement.source, BOX_NODE);
    assert_eq!(ifc.source_for_inline_box(placement.id), Some(BOX_NODE));
    assert_eq!(
        placement.insertion_byte,
        ifc.inline_boxes()[0].insertion_byte
    );
    assert!((placement.rect.width - 28.0).abs() < 0.01);
    assert!((placement.rect.height - 18.0).abs() < 0.01);
    assert_eq!(
        placement.measurement.measured_size,
        InlineIfcSize::new(28.0, 18.0)
    );
}

#[test]
fn text_paint_batch_key_distinguishes_brush_and_font_identity() {
    let input = InlineIfcInput::new(vec![
        InlineIfcItem::TextSpan {
            source: ROOT,
            text: "red ".to_string(),
            style: Some(style_with_size([220, 0, 0, 255], 400, 14.0)),
        },
        InlineIfcItem::TextSpan {
            source: OUTER,
            text: "green ".to_string(),
            style: Some(style_with_size([0, 160, 0, 255], 400, 14.0)),
        },
        InlineIfcItem::TextSpan {
            source: INNER,
            text: "large".to_string(),
            style: Some(style_with_size([0, 160, 0, 255], 700, 20.0)),
        },
    ])
    .with_max_width(300.0);
    let ifc = InlineFormattingContext::build(input);
    let runs = ifc.text_paint_runs();
    let red_run = runs
        .iter()
        .find(|run| run.source == ROOT)
        .expect("red run should exist");
    let green_run = runs
        .iter()
        .find(|run| run.source == OUTER)
        .expect("green run should exist");
    let large_run = runs
        .iter()
        .find(|run| run.source == INNER)
        .expect("large run should exist");

    assert_ne!(
        red_run.batch_key, green_run.batch_key,
        "batch keys must separate different brushes"
    );
    assert_ne!(
        green_run.batch_key, large_run.batch_key,
        "batch keys must separate different font paint identities"
    );
    assert_eq!(red_run.batch_key.brush, [220, 0, 0, 255]);
    assert_eq!(green_run.batch_key.brush, [0, 160, 0, 255]);
    assert_eq!(large_run.batch_key.brush, [0, 160, 0, 255]);
    assert!((large_run.batch_key.font_size() - 20.0).abs() < 0.01);
    assert_eq!(large_run.batch_key.font_weight, 700);
}
