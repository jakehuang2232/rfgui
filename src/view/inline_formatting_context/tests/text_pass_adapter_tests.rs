use super::*;

#[test]
fn text_pass_adapter_preserves_snapshot_glyph_payload() {
    let ifc = fixture(180.0);
    let snapshot = ifc.text_layout_snapshot();
    let adapter = snapshot.text_pass_paint_input();
    let line = snapshot
        .lines
        .iter()
        .find(|line| !line.glyphs.is_empty())
        .expect("fixture should produce text glyphs");
    let glyph = line.glyphs.first().expect("line should have a glyph");
    let adapted = adapter
        .glyphs
        .iter()
        .find(|adapted| {
            adapted.line_index == line.line_index
                && adapted.glyph_id == glyph.glyph_id
                && adapted.cluster_range == glyph.cluster_range
        })
        .expect("adapter should expose the snapshot glyph");
    let adapted_line = adapter
        .lines
        .iter()
        .find(|adapted_line| adapted_line.line_index == line.line_index)
        .expect("adapter should expose the snapshot line");

    assert_eq!(adapted_line.x, line.x);
    assert_eq!(adapted_line.y, line.y);
    assert_eq!(adapted_line.width, line.width);
    assert_eq!(adapted_line.height, line.height);
    assert_eq!(adapted_line.baseline, line.baseline);
    assert_eq!(adapted_line.range, line.range);
    let batch = adapter.batch_of(adapted);
    assert_eq!(adapted.source, glyph.source);
    assert_eq!(batch.batch_key, glyph.batch_key);
    assert_font_render_handle(
        &batch.font_data,
        batch.batch_key.font_data_id,
        batch.batch_key.font_index,
    );
    assert_font_render_handle(&glyph.font_data, glyph.font_data_id, glyph.font_index);
    assert_eq!(batch.batch_key.font_data_id, glyph.font_data_id);
    assert_eq!(batch.batch_key.font_index, glyph.font_index);
    assert_eq!(
        batch.batch_key.normalized_coords_hash,
        glyph.normalized_coords_hash
    );
    assert_eq!(batch.font_size(), glyph.font_size);
    assert_eq!(adapted.x, glyph.x);
    assert_eq!(adapted.baseline_y, line.y + line.baseline);
    assert_eq!(adapted.glyph_x, glyph.x - line.x);
    assert_eq!(adapted.glyph_y, glyph.y - (line.y + line.baseline));
    assert_eq!(adapted.advance, glyph.advance);
    assert_eq!(batch.color(), brush_to_text_color(glyph.batch_key.brush));
}

#[test]
fn text_pass_adapter_batches_by_color_and_font_identity() {
    let input = InlineIfcInput::new(vec![
        InlineIfcItem::TextSpan {
            source: ROOT,
            text: "red ".to_string(),
            style: Some(style_with_size([255, 0, 0, 255], 400, 14.0)),
        },
        InlineIfcItem::TextSpan {
            source: OUTER,
            text: "green ".to_string(),
            style: Some(style_with_size([0, 180, 0, 255], 400, 14.0)),
        },
        InlineIfcItem::TextSpan {
            source: INNER,
            text: "large".to_string(),
            style: Some(style_with_size([0, 180, 0, 255], 700, 20.0)),
        },
    ])
    .with_max_width(400.0);
    let adapter = InlineFormattingContext::build(input).text_pass_paint_input();

    assert!(
        adapter.batches.len() >= 3,
        "brush, font size, and font weight changes should split adapter batches: {adapter:?}",
    );
    for (index, batch) in adapter.batches.iter().enumerate() {
        assert!(
            adapter
                .glyphs
                .iter()
                .any(|glyph| glyph.batch as usize == index),
            "every batch holds a glyph",
        );
        assert_eq!(batch.color(), brush_to_text_color(batch.batch_key.brush));
        assert!((batch.font_size() - batch.batch_key.font_size()).abs() < 0.01);
        assert_font_render_handle(
            &batch.font_data,
            batch.batch_key.font_data_id,
            batch.batch_key.font_index,
        );
    }
    let distinct = adapter
        .batches
        .iter()
        .map(|batch| batch.batch_key)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        distinct.len(),
        adapter.batches.len(),
        "batches are distinct"
    );
}

#[test]
fn text_pass_adapter_keeps_atomic_inline_boxes_out_of_glyphs() {
    let ifc = fixture(180.0);
    let snapshot = ifc.text_layout_snapshot();
    let adapter = snapshot.text_pass_paint_input();

    assert_eq!(snapshot.inline_boxes.len(), 1);
    assert_eq!(snapshot.inline_boxes[0].source, BOX_NODE);
    assert!(adapter.glyphs.iter().all(|glyph| glyph.source != BOX_NODE));
}

#[test]
fn text_pass_adapter_is_available_only_through_explicit_snapshot_conversion() {
    let ifc = fixture(180.0);
    let snapshot = ifc.text_layout_snapshot();

    assert!(
        !snapshot.lines.is_empty(),
        "snapshot construction should not require the text pass adapter"
    );

    let from_snapshot = snapshot.text_pass_paint_input();
    let from_context = ifc.text_pass_paint_input();
    assert_eq!(from_snapshot, from_context);
    assert!(!from_context.glyphs.is_empty());
    assert!(!from_context.batches.is_empty());
}
