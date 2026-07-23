use super::*;



const ROOT: InlineIfcSourceId = InlineIfcSourceId(1);
const OUTER: InlineIfcSourceId = InlineIfcSourceId(2);
const INNER: InlineIfcSourceId = InlineIfcSourceId(3);
const BOX_NODE: InlineIfcSourceId = InlineIfcSourceId(4);
const SECOND_BOX_NODE: InlineIfcSourceId = InlineIfcSourceId(5);

fn style(brush: [u8; 4], weight: u16) -> InlineIfcStyle {
    InlineIfcStyle {
        brush,
        font_weight: weight,
        ..InlineIfcStyle::default()
    }
}

fn style_with_size(brush: [u8; 4], weight: u16, font_size: f32) -> InlineIfcStyle {
    InlineIfcStyle {
        brush,
        font_weight: weight,
        font_size,
        ..InlineIfcStyle::default()
    }
}

fn style_with_metrics(
    brush: [u8; 4],
    weight: u16,
    font_size: f32,
    line_height: f32,
) -> InlineIfcStyle {
    InlineIfcStyle {
        brush,
        font_weight: weight,
        font_size,
        line_height,
        ..InlineIfcStyle::default()
    }
}

fn measured_box(width: f32, height: f32) -> InlineIfcMeasuredAtomicBox {
    InlineIfcMeasuredAtomicBox::new(
        InlineIfcSize::new(width, height),
        InlineIfcAtomicMeasureConstraints::new(Some(180.0)),
    )
}

fn fixture(max_width: f32) -> InlineFormattingContext {
    let input = InlineIfcInput::new(vec![InlineIfcItem::Span {
        source: ROOT,
        style: Some(style([1, 1, 1, 255], 400)),
        children: vec![
            InlineIfcItem::TextSpan {
                source: ROOT,
                text: "plain ".to_string(),
                style: None,
            },
            InlineIfcItem::Span {
                source: OUTER,
                style: Some(style([2, 2, 2, 255], 400)),
                children: vec![
                    InlineIfcItem::TextSpan {
                        source: OUTER,
                        text: "outer ".to_string(),
                        style: None,
                    },
                    InlineIfcItem::Span {
                        source: INNER,
                        style: Some(style([3, 3, 3, 255], 700)),
                        children: vec![InlineIfcItem::TextSpan {
                            source: INNER,
                            text: "strong".to_string(),
                            style: None,
                        }],
                        edge_insets: [0.0; 2],
                    },
                    InlineIfcItem::TextSpan {
                        source: OUTER,
                        text: " tail wraps after ".to_string(),
                        style: None,
                    },
                    InlineIfcItem::AtomicInlineBox {
                        source: BOX_NODE,
                        measurement: measured_box(28.0, 18.0),
                    },
                    InlineIfcItem::TextSpan {
                        source: OUTER,
                        text: " box".to_string(),
                        style: None,
                    },
                ],
                edge_insets: [0.0; 2],
            },
        ],
        edge_insets: [0.0; 2],
    }])
    .with_max_width(max_width);
    InlineFormattingContext::build(input)
}

fn cache_fixture_input() -> InlineIfcInput {
    InlineIfcInput::new(vec![InlineIfcItem::Span {
        source: ROOT,
        style: Some(style_with_metrics([1, 1, 1, 255], 400, 14.0, 1.2)),
        children: vec![
            InlineIfcItem::TextSpan {
                source: OUTER,
                text: "cache me ".to_string(),
                style: Some(style_with_metrics([2, 2, 2, 255], 400, 14.0, 1.2)),
            },
            InlineIfcItem::AtomicInlineBox {
                source: BOX_NODE,
                measurement: measured_box(24.0, 12.0),
            },
        ],
        edge_insets: [0.0; 2],
    }])
    .with_max_width(180.0)
}

fn cache_invalidation(
    previous: &InlineIfcInput,
    next: &InlineIfcInput,
) -> InlineIfcInvalidation {
    next.cache_key().invalidation_from(&previous.cache_key())
}



fn plain_text_input(text: &str) -> InlineIfcInput {
    InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
        source: ROOT,
        text: text.to_string(),
        style: Some(style([1, 1, 1, 255], 400)),
    }])
}






















fn assert_reshape_miss(cache: &InlineIfcCache, input: &InlineIfcInput) {
    let InlineIfcCacheLookup::Miss { invalidation } = cache.lookup_input(input) else {
        panic!("shape input change should miss the IFC cache");
    };
    assert_eq!(invalidation, InlineIfcInvalidation::Reshape);
}

fn assert_cache_update_reshape(cache: &mut InlineIfcCache, input: InlineIfcInput) {
    let expected_key = input.cache_key();
    let update = cache.update(input);
    assert_eq!(update.invalidation, InlineIfcInvalidation::Reshape);
    assert!(update.rebuilt);
    assert_eq!(update.entry.cache_key(), &expected_key);
}

fn text_layout_snapshot_shape(
    snapshot: &InlineIfcTextLayoutSnapshot,
) -> Vec<(
    usize,
    u32,
    u32,
    u32,
    u32,
    u32,
    Range<usize>,
    Vec<(
        InlineIfcSourceId,
        Range<usize>,
        u32,
        u32,
        u32,
        u32,
        u32,
        u64,
        u32,
        u64,
    )>,
)> {
    snapshot
        .lines
        .iter()
        .map(|line| {
            (
                line.line_index,
                f32_cache_bits(line.x),
                f32_cache_bits(line.y),
                f32_cache_bits(line.width),
                f32_cache_bits(line.height),
                f32_cache_bits(line.baseline),
                line.range.clone(),
                line.glyphs
                    .iter()
                    .map(|glyph| {
                        (
                            glyph.source,
                            glyph.cluster_range.clone(),
                            glyph.glyph_id,
                            f32_cache_bits(glyph.x),
                            f32_cache_bits(glyph.y),
                            f32_cache_bits(glyph.advance),
                            f32_cache_bits(glyph.font_size),
                            glyph.font_data_id,
                            glyph.font_index,
                            glyph.normalized_coords_hash,
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

fn assert_font_render_handle(font_data: &Option<FontData>, font_data_id: u64, font_index: u32) {
    let font_data = font_data
        .as_ref()
        .expect("IFC glyph payload should carry a renderable FontData handle");
    assert_eq!(font_data.data.id(), font_data_id);
    assert_eq!(font_data.index, font_index);
}

mod cache_tests;
mod builder_tests;
mod shaping_tests;
mod glyph_output_tests;
mod paint_payload_tests;
mod decoration_tests;
mod snapshot_tests;
mod text_pass_adapter_tests;
mod atomic_inline_box_tests;
mod hit_test_and_caret_tests;
mod selection_tests;
