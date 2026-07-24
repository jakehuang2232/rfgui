use super::{Element, ElementTrait, RetainedPaintProperties, Text};
use crate::style::{BoxShadow, ScrollDirection};
use crate::view::base_component::TextArea;

#[test]
fn default_contract_is_property_neutral() {
    let text_area = TextArea::new();
    assert_eq!(
        text_area.retained_paint_properties(),
        RetainedPaintProperties::default()
    );
}

#[test]
fn element_contract_observes_retained_paint_semantics() {
    let mut element = Element::new(0.0, 0.0, 100.0, 50.0);
    element.set_opacity(0.4);
    element.set_border_radius(6.0);
    element.set_box_shadows(vec![BoxShadow::new().offset(2.0)]);
    element.border_widths.left = 1.0;
    element.scroll_direction = ScrollDirection::Vertical;

    assert_eq!(
        element.retained_paint_properties(),
        RetainedPaintProperties {
            opacity: 0.4,
            has_rounded_clip: true,
            has_box_shadow: true,
            has_border: true,
            is_scroll_container: true,
        }
    );
}

#[test]
fn text_contract_preserves_native_opacity() {
    let mut text = Text::from_content("retained");
    text.set_opacity(0.35);

    assert_eq!(
        text.retained_paint_properties(),
        RetainedPaintProperties {
            opacity: 0.35,
            ..Default::default()
        }
    );
}
