use super::*;

#[test]
fn glyph_output_groups_nested_span_styles_by_source_byte_ranges() {
    let ifc = fixture(240.0);
    let groups = ifc.glyph_groups();
    let outer_start = ifc.backing_text().find("outer").unwrap();
    let strong_start = ifc.backing_text().find("strong").unwrap();

    let outer_group = groups
        .iter()
        .find(|group| group.range.contains(&outer_start))
        .expect("outer text should produce a glyph group");
    let strong_group = groups
        .iter()
        .find(|group| group.range.contains(&strong_start))
        .expect("nested strong text should produce a glyph group");

    assert_eq!(outer_group.source, OUTER);
    assert_eq!(outer_group.style.brush, [2, 2, 2, 255]);
    assert_eq!(outer_group.style.font_weight, 400);
    assert_eq!(strong_group.source, INNER);
    assert_eq!(strong_group.style.brush, [3, 3, 3, 255]);
    assert_eq!(strong_group.style.font_weight, 700);
    assert!(
        strong_group
            .glyphs
            .iter()
            .all(|glyph| glyph.style == strong_group.style
                && glyph.font_size > 0.0
                && glyph.advance >= 0.0),
        "glyph items should carry resolved style and font identity: {strong_group:?}"
    );
}

#[test]
fn glyph_output_does_not_depend_on_parley_item_boundaries_for_style_lookup() {
    let input = InlineIfcInput::new(vec![
        InlineIfcItem::TextSpan {
            source: ROOT,
            text: "aa ".to_string(),
            style: Some(style([10, 0, 0, 255], 400)),
        },
        InlineIfcItem::TextSpan {
            source: OUTER,
            text: "bb ".to_string(),
            style: Some(style([0, 10, 0, 255], 700)),
        },
        InlineIfcItem::TextSpan {
            source: INNER,
            text: "cc".to_string(),
            style: Some(style([0, 0, 10, 255], 400)),
        },
    ])
    .with_max_width(300.0);
    let ifc = InlineFormattingContext::build(input);
    let line_zero_groups = ifc
        .glyph_groups()
        .into_iter()
        .filter(|group| group.line_index == 0)
        .collect::<Vec<_>>();

    assert!(
        line_zero_groups
            .iter()
            .any(|group| group.source == ROOT && group.style.brush == [10, 0, 0, 255]),
        "first style should be recovered from IFC style ranges: {line_zero_groups:?}"
    );
    assert!(
        line_zero_groups
            .iter()
            .any(|group| group.source == OUTER && group.style.font_weight == 700),
        "middle style should be recovered from IFC style ranges: {line_zero_groups:?}"
    );
    assert!(
        line_zero_groups
            .iter()
            .any(|group| group.source == INNER && group.style.brush == [0, 0, 10, 255]),
        "last style should be recovered from IFC style ranges: {line_zero_groups:?}"
    );
}

#[test]
fn atomic_inline_box_placement_and_glyph_output_do_not_share_sources() {
    let ifc = fixture(180.0);
    let glyphs = ifc.glyph_items();
    let placements = ifc.inline_box_placements();

    assert!(!glyphs.is_empty(), "text should still produce glyph output");
    assert_eq!(placements.len(), 1);
    assert_eq!(placements[0].source, BOX_NODE);
    assert!(
        glyphs.iter().all(|glyph| glyph.source != BOX_NODE),
        "atomic boxes should not leak into glyph output: {glyphs:?}"
    );
}

#[test]
fn glyph_snapshot_and_text_pass_payloads_carry_font_render_handle() {
    let ifc = fixture(180.0);
    let glyph_item = ifc
        .glyph_items()
        .into_iter()
        .next()
        .expect("fixture should produce IFC glyph items");
    assert_font_render_handle(
        &glyph_item.font_data,
        glyph_item.font_data_id,
        glyph_item.font_index,
    );
    assert_eq!(
        glyph_item.normalized_coords_hash, 0,
        "default fixture fonts should still carry the normalized coords hash field"
    );

    let snapshot = ifc.text_layout_snapshot();
    let snapshot_glyph = snapshot
        .lines
        .iter()
        .flat_map(|line| line.glyphs.iter())
        .find(|glyph| {
            glyph.glyph_id == glyph_item.glyph_id
                && glyph.cluster_range == glyph_item.cluster_range
        })
        .expect("snapshot should preserve the IFC glyph");
    assert_font_render_handle(
        &snapshot_glyph.font_data,
        snapshot_glyph.font_data_id,
        snapshot_glyph.font_index,
    );
    assert_eq!(snapshot_glyph.font_data_id, glyph_item.font_data_id);
    assert_eq!(snapshot_glyph.font_index, glyph_item.font_index);
    assert_eq!(
        snapshot_glyph.normalized_coords_hash,
        glyph_item.normalized_coords_hash
    );
    assert_eq!(
        snapshot_glyph.batch_key.normalized_coords_hash,
        snapshot_glyph.normalized_coords_hash
    );

    let adapter = snapshot.text_pass_paint_input();
    let adapter_glyph = adapter
        .glyphs
        .iter()
        .find(|glyph| {
            glyph.glyph_id == snapshot_glyph.glyph_id
                && glyph.cluster_range == snapshot_glyph.cluster_range
        })
        .expect("text-pass adapter should preserve the snapshot glyph");
    assert_font_render_handle(
        &adapter_glyph.font_data,
        adapter_glyph.font_data_id,
        adapter_glyph.font_index,
    );
    assert_eq!(adapter_glyph.font_data_id, snapshot_glyph.font_data_id);
    assert_eq!(adapter_glyph.font_index, snapshot_glyph.font_index);
    assert_eq!(
        adapter_glyph.normalized_coords_hash,
        snapshot_glyph.normalized_coords_hash
    );
    assert_eq!(
        adapter_glyph.batch_key.normalized_coords_hash,
        adapter_glyph.normalized_coords_hash
    );
}
