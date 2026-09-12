use super::*;

#[test]
fn live_input_comparison_matches_owned_keys_for_nested_and_normalized_inputs() {
    let source = InlineIfcElementRootSource::new(InlineIfcInput::new(vec![InlineIfcItem::Span {
        source: ROOT,
        style: Some(style([1, 2, 3, 255], 700)),
        children: vec![
            InlineIfcItem::TextSpan {
                source: INNER,
                text: "live 👋".into(),
                style: None,
            },
            InlineIfcItem::AtomicInlineBox {
                source: BOX_NODE,
                measurement: measured_box(12.0, 16.0),
            },
            InlineIfcItem::GapSpacer {
                source: ROOT,
                width: 4.0,
            },
        ],
        edge_insets: [-0.0, 3.0],
    }]));
    let mut variants = vec![source.clone()];
    for scalar in [0.0, -0.0, -1.0, 20.0, f32::NAN, f32::INFINITY] {
        let mut changed = source.clone();
        let InlineIfcItem::Span {
            style,
            edge_insets,
            children,
            ..
        } = &mut changed.input.items[0]
        else {
            unreachable!()
        };
        style.as_mut().unwrap().font_size = scalar;
        style.as_mut().unwrap().line_height = scalar;
        edge_insets[0] = scalar;
        let InlineIfcItem::GapSpacer { width, .. } = &mut children[2] else {
            unreachable!()
        };
        *width = scalar;
        changed.layout_options.max_width = Some(scalar);
        variants.push(changed);
    }
    for index in 0..3 {
        let mut changed = source.clone();
        let InlineIfcItem::Span { children, .. } = &mut changed.input.items[0] else {
            unreachable!()
        };
        children.remove(index);
        variants.push(changed);
    }
    let mut changed = source.clone();
    let InlineIfcItem::Span {
        style, children, ..
    } = &mut changed.input.items[0]
    else {
        unreachable!()
    };
    style.as_mut().unwrap().brush = [9, 8, 7, 128];
    let InlineIfcItem::TextSpan { text, source, .. } = &mut children[0] else {
        unreachable!()
    };
    *source = SECOND_BOX_NODE;
    text.push('!');
    children.swap(1, 2);
    variants.push(changed);

    // Compare every live variant to every stored key. This covers both
    // acceptance and refusal, including inherited paint and shape differences.
    for live in &variants {
        for stored in &variants {
            let key = stored.cache_key();
            assert_eq!(live.matches_cache_key(&key), live.cache_key() == key);
        }
    }
}
