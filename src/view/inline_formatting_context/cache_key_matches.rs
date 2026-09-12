use super::*;

pub(super) fn items_match(
    items: &[InlineIfcItem],
    inherited: &InlineIfcStyle,
    content: &[InlineIfcContentKeyItem],
    paint: &[InlineIfcPaintKeyItem],
) -> bool {
    items.len() == content.len()
        && items.len() == paint.len()
        && items
            .iter()
            .zip(content)
            .zip(paint)
            .all(|((item, content), paint)| match (item, content, paint) {
                (
                    InlineIfcItem::TextSpan {
                        source,
                        text,
                        style,
                    },
                    InlineIfcContentKeyItem::Text {
                        source: c_source,
                        text: c_text,
                        shape_style,
                    },
                    InlineIfcPaintKeyItem::Text {
                        source: p_source,
                        paint_style,
                    },
                ) => {
                    let style = style.as_ref().unwrap_or(inherited);
                    source == c_source
                        && source == p_source
                        && text == c_text
                        && InlineIfcStyleKey::from_style(style) == *shape_style
                        && InlineIfcPaintStyleKey::from_style(style) == *paint_style
                }
                (
                    InlineIfcItem::Span {
                        source,
                        style,
                        children,
                        edge_insets,
                    },
                    InlineIfcContentKeyItem::Span {
                        source: c_source,
                        shape_style,
                        children: c_children,
                        edge_insets_bits,
                    },
                    InlineIfcPaintKeyItem::Span {
                        source: p_source,
                        paint_style,
                        children: p_children,
                    },
                ) => {
                    let style = style.as_ref().unwrap_or(inherited);
                    source == c_source
                        && source == p_source
                        && edge_insets.map(f32::to_bits) == *edge_insets_bits
                        && InlineIfcStyleKey::from_style(style) == *shape_style
                        && InlineIfcPaintStyleKey::from_style(style) == *paint_style
                        && items_match(children, style, c_children, p_children)
                }
                (
                    InlineIfcItem::AtomicInlineBox {
                        source,
                        measurement,
                    },
                    InlineIfcContentKeyItem::AtomicInlineBox {
                        source: c_source,
                        shape_key,
                    },
                    InlineIfcPaintKeyItem::AtomicInlineBox { source: p_source },
                ) => {
                    source == c_source
                        && source == p_source
                        && InlineIfcAtomicBoxShapeKey::from_measurement(measurement) == *shape_key
                }
                (
                    InlineIfcItem::GapSpacer { source, width },
                    InlineIfcContentKeyItem::GapSpacer {
                        source: c_source,
                        width_bits,
                    },
                    InlineIfcPaintKeyItem::GapSpacer { source: p_source },
                ) => {
                    source == c_source
                        && source == p_source
                        && width.max(0.0).to_bits() == *width_bits
                }
                _ => false,
            })
}
