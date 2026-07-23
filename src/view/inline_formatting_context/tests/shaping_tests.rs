use super::*;

#[test]
fn text_brush_decodes_srgb_rgb_and_keeps_linear_alpha() {
    let brush = [40, 44, 52, 128];
    let color = brush_to_text_color(brush);
    assert_eq!(
        color,
        [
            crate::style::srgb_to_linear(40),
            crate::style::srgb_to_linear(44),
            crate::style::srgb_to_linear(52),
            128.0 / 255.0,
        ]
    );
    assert_ne!(
        color[0],
        40.0 / 255.0,
        "packed sRGB must not be sent to the linear render pipeline"
    );
}

#[test]
fn span_edge_insets_reserve_line_advance() {
    // A span's horizontal border+padding must occupy line space:
    // without the reserved advance, neighbours overlap the span's
    // decoration box (padding painted over adjacent glyphs).
    let inset = 8.0_f32;
    let build = |edge_insets: [f32; 2]| {
        let input = InlineIfcInput::new(vec![
            InlineIfcItem::TextSpan {
                source: InlineIfcSourceId(11),
                text: "aa".to_string(),
                style: None,
            },
            InlineIfcItem::Span {
                source: InlineIfcSourceId(12),
                style: None,
                children: vec![InlineIfcItem::TextSpan {
                    source: InlineIfcSourceId(13),
                    text: "bb".to_string(),
                    style: None,
                }],
                edge_insets,
            },
            InlineIfcItem::TextSpan {
                source: InlineIfcSourceId(14),
                text: "cc".to_string(),
                style: None,
            },
        ]);
        InlineFormattingContext::build(input)
    };

    let plain = build([0.0; 2]);
    let padded = build([inset, inset]);

    let span_extent = |ifc: &InlineFormattingContext, source: u64| -> (f32, f32) {
        let rects = ifc.source_line_rects(InlineIfcSourceId(source));
        let rect = rects.first().expect("source rect");
        (rect.x, rect.x + rect.width)
    };

    let (_, plain_bb_end) = span_extent(&plain, 13);
    let (plain_cc_start, _) = span_extent(&plain, 14);
    let (padded_bb_start, padded_bb_end) = span_extent(&padded, 13);
    let (padded_cc_start, _) = span_extent(&padded, 14);

    // Left inset shifts the span's own glyphs right.
    let (plain_bb_start, _) = span_extent(&plain, 13);
    assert!(
        (padded_bb_start - plain_bb_start - inset).abs() < 0.6,
        "left inset must shift the span glyphs: plain={plain_bb_start} padded={padded_bb_start}"
    );
    // Right inset reserves space before the following text.
    let plain_gap = plain_cc_start - plain_bb_end;
    let padded_gap = padded_cc_start - padded_bb_end;
    assert!(
        (padded_gap - plain_gap - inset).abs() < 0.6,
        "right inset must reserve advance before the next span: plain_gap={plain_gap} padded_gap={padded_gap}"
    );
    // Spacers stay invisible to atomic consumers.
    assert!(
        padded
            .inline_box_placements()
            .iter()
            .all(|placement| placement.role == InlineIfcInlineBoxRole::SpanEdgeSpacer),
        "fixture has no real atomics; only spacers may appear"
    );
    assert!(
        padded
            .hit_test_point(padded_bb_start - inset / 2.0, 4.0)
            .is_none()
            || !matches!(
                padded
                    .hit_test_point(padded_bb_start - inset / 2.0, 4.0)
                    .map(|hit| hit.target),
                Some(InlineIfcHitTarget::InlineBox { .. })
            ),
        "spacer must not be a hit target"
    );
}

#[test]
fn wrap_epsilon_keeps_snug_content_on_one_line() {
    let text = "epsilon slack";
    let unconstrained = InlineFormattingContext::build_with_options(
        plain_text_input(text),
        InlineIfcLayoutOptions::new(None, false),
    );
    let intrinsic_width = unconstrained
        .text_pass_paint_input()
        .glyphs
        .iter()
        .map(|glyph| glyph.x + glyph.advance)
        .fold(0.0f32, f32::max);
    assert!(intrinsic_width > 10.0, "fixture should shape glyphs");
    let line_count = |max_width: f32| {
        InlineFormattingContext::build_with_options(
            plain_text_input(text),
            InlineIfcLayoutOptions::new(Some(max_width), true),
        )
        .text_layout_snapshot()
        .lines
        .len()
    };

    assert_eq!(
        line_count(intrinsic_width - 1.0),
        1,
        "content within the 2px slack must not wrap"
    );
    assert!(
        line_count(intrinsic_width - 3.0) > 1,
        "content beyond the slack must wrap"
    );
}

#[test]
fn layout_align_shifts_lines_within_the_constraint() {
    let text = "align me";
    let max_width = 220.0;
    let glyph_extent = |align: InlineIfcAlignment| {
        let glyphs = InlineFormattingContext::build_with_options(
            plain_text_input(text),
            InlineIfcLayoutOptions::new(Some(max_width), true).with_align(align),
        )
        .text_pass_paint_input()
        .glyphs;
        let left = glyphs.iter().map(|glyph| glyph.x).fold(f32::MAX, f32::min);
        let right = glyphs
            .iter()
            .map(|glyph| glyph.x + glyph.advance)
            .fold(0.0f32, f32::max);
        (left, right)
    };

    let (left_l, _) = glyph_extent(InlineIfcAlignment::Left);
    let (left_c, _) = glyph_extent(InlineIfcAlignment::Center);
    let (left_r, right_r) = glyph_extent(InlineIfcAlignment::Right);
    assert!(left_l.abs() <= 1.0, "left-aligned line starts at zero");
    assert!(left_c > left_l + 1.0, "center must shift right of left");
    assert!(left_r > left_c + 1.0, "right must shift right of center");
    assert!(
        ((left_c - left_l) - (left_r - left_c)).abs() <= 1.0,
        "center should sit halfway between left and right"
    );
    assert!(
        right_r <= max_width + INLINE_IFC_WRAP_EPSILON + 0.01,
        "right-aligned glyphs must stay inside the constraint, right={right_r}"
    );
}

/// Safety net for dropping the legacy 240-char cluster-break guard
/// (`parley_safe_text`): long real-world content must still shape
/// without hanging or producing degenerate output.
#[test]
fn shaping_survives_long_content_without_chunk_guard() {
    let long_ascii = "a".repeat(100_000);
    let ifc = InlineFormattingContext::build_with_options(
        plain_text_input(&long_ascii),
        InlineIfcLayoutOptions::new(Some(400.0), true),
    );
    let lines = ifc.text_layout_snapshot().lines.len();
    assert!(
        lines > 100,
        "100k ascii should wrap into many lines: {lines}"
    );

    let long_cjk = "\u{4E2D}".repeat(10_000);
    let ifc = InlineFormattingContext::build_with_options(
        plain_text_input(&long_cjk),
        InlineIfcLayoutOptions::new(Some(400.0), true),
    );
    let lines = ifc.text_layout_snapshot().lines.len();
    assert!(lines > 100, "10k CJK should wrap into many lines: {lines}");
}

/// Parley (through 0.11) counts a shaping cluster's chars in a `u8`
/// (`map_len` in `shape::fill_cluster_in_place`), so a single grapheme
/// segment with thousands of combining marks used to panic with "attempt
/// to add with overflow". The `icu_segmenter` shim now splits oversized
/// grapheme segments at the boundary level (byte offsets untouched, see
/// `vendor/icu_segmenter_rfgui_shim`), replacing the legacy
/// zero-width-space insertion (`parley_safe_text`).
#[test]
fn shaping_survives_overlong_combining_cluster() {
    let combining = format!("a{}", "\u{0301}".repeat(2_000));
    let ifc = InlineFormattingContext::build_with_options(
        plain_text_input(&combining),
        InlineIfcLayoutOptions::new(Some(400.0), true),
    );
    assert!(!ifc.text_layout_snapshot().lines.is_empty());
}
