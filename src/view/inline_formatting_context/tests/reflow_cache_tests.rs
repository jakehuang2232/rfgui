use super::*;

#[test]
fn resized_cached_runs_match_fresh_shaping_after_eviction() {
    let mut rich = cache_fixture_input();
    rich.items.push(InlineIfcItem::TextSpan {
        source: INNER,
        text: " 中文 e\u{301} office שלום مرحبا\n\nlicense tail ".repeat(12),
        style: Some(style_with_size([70, 80, 90, 255], 700, 19.0)),
    });
    for input in [
        plain_text_input(""),
        plain_text_input("license words ".repeat(50).as_str()),
        rich,
    ] {
        let mut cache = InlineIfcCache::new();
        // Each pass crosses the four-entry cache capacity and changes the
        // wrap policy, which must invalidate the reusable shaped runs.
        for wrap in [true, false, true] {
            for align in [
                InlineIfcAlignment::Left,
                InlineIfcAlignment::Center,
                InlineIfcAlignment::Right,
            ] {
                for width in [160.0, 240.0, 164.0, 168.0, 172.0, 176.0, 160.0] {
                    let options = InlineIfcLayoutOptions::new(Some(width), wrap).with_align(align);
                    let fresh = InlineFormattingContext::build_with_options(input.clone(), options);
                    let update = cache.update_with_options(input.clone(), options);
                    let cached = update.entry.context();
                    assert_eq!(
                        cached.text_layout_snapshot_ref(),
                        fresh.text_layout_snapshot_ref()
                    );
                    assert_eq!(
                        cached.visual_caret_stops_ref(),
                        fresh.visual_caret_stops_ref()
                    );
                    assert_eq!(
                        cached.text_pass_paint_input_ref(),
                        fresh.text_pass_paint_input_ref()
                    );
                    assert_eq!(
                        cached.inline_box_placements(),
                        fresh.inline_box_placements()
                    );
                    assert!(cache.len() <= INLINE_IFC_CACHE_MAX_ENTRIES);
                }
            }
        }
    }
}

#[test]
fn reflow_requires_unchanged_text_style_paint_and_inline_box_measurement() {
    let input = cache_fixture_input();
    let context = InlineFormattingContext::build(input.clone());
    let width_key =
        input.cache_key_with_layout_options(InlineIfcLayoutOptions::new(Some(220.0), true));
    assert!(context.can_reflow(&width_key));
    let no_wrap =
        input.cache_key_with_layout_options(InlineIfcLayoutOptions::new(Some(220.0), false));
    assert!(!context.can_reflow(&no_wrap));
    for mutation in 0..5 {
        let mut changed = input.clone();
        let InlineIfcItem::Span { children, .. } = &mut changed.items[0] else {
            unreachable!()
        };
        if mutation == 4 {
            let InlineIfcItem::AtomicInlineBox { measurement, .. } = &mut children[1] else {
                unreachable!()
            };
            *measurement = measured_box(48.0, 24.0);
        } else {
            let InlineIfcItem::TextSpan { text, style, .. } = &mut children[0] else {
                unreachable!()
            };
            match mutation {
                0 => text.push('!'),
                1 => style.as_mut().unwrap().font_size += 2.0,
                2 => style.as_mut().unwrap().brush = [120, 30, 60, 255],
                3 => style.as_mut().unwrap().vertical_align = crate::style::VerticalAlign::Top,
                _ => unreachable!(),
            }
        }
        assert!(
            !context.can_reflow(&changed.cache_key()),
            "mutation={mutation}"
        );
        let mut cache = InlineIfcCache::new();
        cache.update(input.clone());
        let actual = cache.update(changed.clone());
        let expected = InlineFormattingContext::build(changed);
        assert_eq!(
            actual.entry.context().text_pass_paint_input_ref(),
            expected.text_pass_paint_input_ref()
        );
    }
}
