use super::*;

fn assert_parley_cursor_equivalence(context: &InlineFormattingContext) {
    let index = caret_index::CaretIndex::new(&context.layout, context.backing_text.len());
    for byte in 0..=context.backing_text.len() + 2 {
        for affinity in [
            InlineIfcCaretAffinity::Downstream,
            InlineIfcCaretAffinity::Upstream,
        ] {
            let expected =
                ParleyCursor::from_byte_index(&context.layout, byte, affinity.to_parley())
                    .geometry(&context.layout, 0.0);
            let actual = index.rect(byte, affinity);
            assert_eq!(
                (actual.x, actual.y, actual.height),
                (
                    expected.x0 as f32,
                    expected.y0 as f32,
                    (expected.y1 - expected.y0) as f32
                ),
                "byte={byte} affinity={affinity:?} text={:?}",
                context.backing_text
            );
        }
    }
}

#[test]
fn indexed_caret_geometry_matches_parley_for_every_byte_and_affinity() {
    for text in [
        "",
        "\n",
        "\n\n",
        "abc\n\n",
        " \t  \n \n",
        "abc\r\ndef\u{2028}ghi\u{2029}",
        "office ffi café e\u{301} 中文日本語 👨‍👩‍👧‍👦 🇹🇼",
        "कर्मक्षेत्र हिन्दी বাংলা தமிழ் ภาษาไทย",
        "abc\u{00ad}def a\u{200b}b c\u{200d}d \u{feff}tail",
        "hello שלום world مرحبا 123 שלום\nمرحبا",
        "\u{202b}abc 123\u{202c} xyz \u{2067}مرحبا\u{2069}",
        "first line with many words and trailing spaces    \nTail ",
    ] {
        for width in [1.0, 48.0, 120.0, 400.0] {
            for wrap in [true, false] {
                for align in [
                    InlineIfcAlignment::Left,
                    InlineIfcAlignment::Center,
                    InlineIfcAlignment::Right,
                ] {
                    let options = InlineIfcLayoutOptions::new(Some(width), wrap).with_align(align);
                    let context = InlineFormattingContext::build_with_options(
                        plain_text_input(text),
                        options,
                    );
                    assert_parley_cursor_equivalence(&context);
                }
            }
        }
    }
}

#[test]
fn indexed_carets_preserve_styled_runs_and_inline_box_gaps_after_reflow() {
    for width in [1.0, 50.0, 150.0, 400.0] {
        assert_parley_cursor_equivalence(&fixture(width));
        let mut input = cache_fixture_input();
        input.items.insert(
            0,
            InlineIfcItem::AtomicInlineBox {
                source: SECOND_BOX_NODE,
                measurement: measured_box(30.0, 60.0),
            },
        );
        input.items.push(InlineIfcItem::TextSpan {
            source: INNER,
            text: "שלום \n\n مرحبا tail\n".into(),
            style: Some(style_with_size([20, 30, 40, 255], 700, 24.0)),
        });
        let mut cache = InlineIfcCache::new();
        for width in [width, width + 4.0, width + 40.0, width] {
            let update = cache.update_with_options(
                input.clone(),
                InlineIfcLayoutOptions::new(Some(width), true),
            );
            assert_parley_cursor_equivalence(update.entry.context());
        }
        let only_boxes = InlineFormattingContext::build(
            InlineIfcInput::new(vec![InlineIfcItem::AtomicInlineBox {
                source: BOX_NODE,
                measurement: measured_box(30.0, 60.0),
            }])
            .with_max_width(width),
        );
        assert_parley_cursor_equivalence(&only_boxes);
    }
}
