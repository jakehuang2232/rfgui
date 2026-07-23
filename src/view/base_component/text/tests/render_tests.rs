use super::*;

#[test]
fn text_caret_position_uses_screen_coordinates_once() {
    let mut a = arena();
    let mut text = Text::from_content("XYZ");
    text.measure(
        LayoutConstraints {
            max_width: 120.0,
            max_height: 40.0,
            viewport_width: 300.0,
            viewport_height: 200.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(40.0),
        },
        &mut a,
    );
    text.place(
        LayoutPlacement {
            parent_x: 42.0,
            parent_y: 24.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 120.0,
            available_height: 40.0,
            viewport_width: 300.0,
            viewport_height: 200.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(40.0),
        },
        &mut a,
    );

    let snapshot = text.box_model_snapshot();
    let (x, y, _) = text
        .local_char_to_screen_position(2)
        .expect("caret position");
    assert!(
        x >= snapshot.x - 0.5 && x <= snapshot.x + snapshot.width + 0.5,
        "caret x should stay inside text bounds: x={x}, snapshot=({}, {})",
        snapshot.x,
        snapshot.width
    );
    assert!(
        y >= snapshot.y - 0.5 && y <= snapshot.y + snapshot.height + 0.5,
        "caret y should stay inside text bounds: y={y}, snapshot=({}, {})",
        snapshot.y,
        snapshot.height
    );
    assert_eq!(text.screen_position_to_local_char(x, y), Some(2));
}

#[test]
fn retained_transform_text_bounds_apply_nonzero_inherited_paint_offset() {
    let text = Text::new(3.25, 4.5, 10.0, 5.0, "offset bounds");
    let arena = arena();
    let exact = text
        .retained_transform_output_bounds(&arena, [0.2, -0.3])
        .expect("Text explicitly owns exact transformed-ancestor coverage");
    assert_eq!(
        [exact.x, exact.y, exact.width, exact.height].map(f32::to_bits),
        [3.45, 4.2, 10.0, 5.0].map(f32::to_bits)
    );
    let legacy = text
        .legacy_transform_output_bounds(&arena, [0.2, -0.3])
        .expect("legacy Text coverage");
    assert_eq!(
        [legacy.x, legacy.y, legacy.width, legacy.height].map(f32::to_bits),
        [exact.x, exact.y, exact.width, exact.height].map(f32::to_bits)
    );
}

#[test]
fn text_build_emits_prepared_input_pass_from_shaped_context() {
    let mut text = Text::new(0.0, 0.0, 92.0, 80.0, "prepared text");
    place_text_for_read_only_ifc_test(&mut text, 92.0, 120.0);

    let context = text
        .shaped_context_for_test()
        .expect("measure should install a shaped context")
        .clone();
    let staging = text
        .shaped_staging_input_for_test([0.0, 0.0])
        .expect("placed Text should stage glyphs");
    assert_eq!(
        staging.glyphs.len(),
        context.text_pass_paint_input().glyphs.len(),
        "render must stage exactly the glyphs of the measure-shaped context"
    );
    assert!(
        staging
            .glyphs
            .iter()
            .all(|glyph| glyph.paint.color == text.color.to_rgba_f32()),
        "live color must be injected at bridge time"
    );

    let pass_names = build_text_for_read_only_ifc_test(&mut text);
    assert_eq!(pass_names.len(), 1);
    assert!(pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"));
}

#[test]
fn text_build_skips_non_renderable_text() {
    let mut empty = Text::new(0.0, 0.0, 92.0, 80.0, "");
    place_text_for_read_only_ifc_test(&mut empty, 92.0, 120.0);
    assert!(build_text_for_read_only_ifc_test(&mut empty).is_empty());

    let mut transparent = Text::new(0.0, 0.0, 92.0, 80.0, "transparent candidate text");
    transparent.set_opacity(0.0);
    place_text_for_read_only_ifc_test(&mut transparent, 92.0, 120.0);
    assert!(build_text_for_read_only_ifc_test(&mut transparent).is_empty());

    let mut hidden = Text::new(0.0, 0.0, 92.0, 80.0, "hidden candidate text");
    place_text_for_read_only_ifc_test(&mut hidden, 92.0, 120.0);
    hidden.layout_state.should_render = false;
    assert!(build_text_for_read_only_ifc_test(&mut hidden).is_empty());
}

#[test]
fn text_opacity_marks_paint_and_composite_without_layout() {
    let mut text = Text::new(0.0, 0.0, 92.0, 80.0, "opacity");
    text.clear_local_dirty_flags(DirtyFlags::ALL);

    text.set_opacity(0.5);

    let dirty = text.local_dirty_flags();
    assert!(dirty.contains(DirtyFlags::PAINT));
    assert!(dirty.contains(DirtyFlags::COMPOSITE));
    assert!(!dirty.intersects(DirtyFlags::LAYOUT));
}

#[test]
fn text_align_shifts_glyphs_and_caret_consistently() {
    let build_aligned = |align: crate::style::TextAlign| {
        let mut text = Text::new(0.0, 0.0, 220.0, 40.0, "align me");
        text.set_text_align(align);
        place_text_for_read_only_ifc_test(&mut text, 220.0, 120.0);
        let staging = text
            .shaped_staging_input_for_test([0.0, 0.0])
            .expect("aligned Text should stage glyphs");
        let min_glyph_x = staging
            .glyphs
            .iter()
            .map(|glyph| glyph.paint.local_pos[0])
            .fold(f32::MAX, f32::min);
        let (caret_x, _, _) = text
            .local_char_to_screen_position(0)
            .expect("caret at first char");
        (min_glyph_x, caret_x)
    };

    let (left_glyph_x, left_caret_x) = build_aligned(crate::style::TextAlign::Left);
    let (center_glyph_x, center_caret_x) = build_aligned(crate::style::TextAlign::Center);
    let (right_glyph_x, right_caret_x) = build_aligned(crate::style::TextAlign::Right);
    assert!(center_glyph_x > left_glyph_x + 1.0, "center shifts glyphs");
    assert!(right_glyph_x > center_glyph_x + 1.0, "right shifts further");
    assert!(
        (left_caret_x - left_glyph_x).abs() < 1.0
            && (center_caret_x - center_glyph_x).abs() < 1.0
            && (right_caret_x - right_glyph_x).abs() < 1.0,
        "caret must sit on the aligned glyphs: caret=({left_caret_x},{center_caret_x},{right_caret_x}) glyphs=({left_glyph_x},{center_glyph_x},{right_glyph_x})"
    );
}

#[test]
fn text_color_change_repaints_without_reshaping() {
    let mut a = arena();
    let mut text = Text::from_content("recolor me");
    let constraints = LayoutConstraints {
        max_width: 200.0,
        max_height: 60.0,
        viewport_width: 200.0,
        viewport_height: 60.0,
        percent_base_width: Some(200.0),
        percent_base_height: Some(60.0),
    };
    text.measure(constraints, &mut a);
    let before = text
        .shaped_context_for_test()
        .expect("shaped context installed")
        .clone();

    text.set_color(Color::rgba(200, 30, 30, 255));
    text.measure(constraints, &mut a);
    let after = text
        .shaped_context_for_test()
        .expect("shaped context installed")
        .clone();
    assert!(
        std::sync::Arc::ptr_eq(&before, &after),
        "color change must not reshape the text"
    );
    let staging = text
        .shaped_staging_input_for_test([0.0, 0.0])
        .expect("staged glyphs");
    assert!(
        staging
            .glyphs
            .iter()
            .all(|glyph| glyph.paint.color == text.color.to_rgba_f32()),
        "staged glyphs must carry the new color"
    );
}
