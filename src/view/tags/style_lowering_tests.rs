use super::*;
use crate::style::style_props::{StylePropSet, validate_style};
use crate::style::{Color, ParsedValue, PropertyId, Transition, TransitionProperty};
use crate::ui::HostStyleTag;
use std::any::TypeId;

fn color(hex: &'static str) -> Box<dyn ColorLike> {
    Box::new(Color::hex(hex))
}

fn transition() -> Transitions {
    Transitions::single(Transition::new(TransitionProperty::Opacity, 120))
}

fn host_style_prop_type<T>() -> TypeId
where
    T: HostStyleTag,
    <T as HostStyleTag>::StyleProp: 'static,
{
    TypeId::of::<<T as HostStyleTag>::StyleProp>()
}

fn host_style_accepts<T>(property: PropertyId) -> bool
where
    T: HostStyleTag,
{
    <<T::StyleProp as StylePropTrait>::Accepted as StylePropSet>::accepts(property)
}

fn hover_text_style() -> HoverTextStylePropSchema {
    HoverTextStylePropSchema {
        width: Some(Length::px(120.0)),
        height: Some(Length::px(32.0)),
        color: Some(color("#123456")),
        font: Some(FontFamily::new(["Inter"])),
        font_size: Some(FontSize::px(17.0)),
        font_weight: Some(FontWeight::new(600)),
        text_wrap: Some(TextWrap::NoWrap),
        cursor: Some(Cursor::Text),
        opacity: Some(Opacity::new(0.75)),
        transform: Some(Transform::new([crate::style::Translate::x(Length::px(
            6.0,
        ))])),
        transform_origin: Some(TransformOrigin::px(3.0, 4.0)),
        transition: Some(transition()),
    }
}

#[test]
fn built_in_host_tags_declare_style_prop_contract() {
    assert_eq!(
        host_style_prop_type::<Element>(),
        TypeId::of::<ElementStylePropSchema>()
    );
    assert_eq!(
        host_style_prop_type::<Text>(),
        TypeId::of::<TextStylePropSchema>()
    );
    assert_eq!(
        host_style_prop_type::<TextArea>(),
        TypeId::of::<ElementStylePropSchema>()
    );
    assert_eq!(
        host_style_prop_type::<Image>(),
        TypeId::of::<ElementStylePropSchema>()
    );
    assert_eq!(
        host_style_prop_type::<Svg>(),
        TypeId::of::<ElementStylePropSchema>()
    );
    assert_eq!(
        host_style_prop_type::<TextAreaProjectionSegment>(),
        TypeId::of::<NoStylePropSchema>()
    );
}

#[test]
fn host_tag_style_contract_exposes_accepted_set() {
    assert!(host_style_accepts::<Element>(PropertyId::BackgroundColor));
    assert!(host_style_accepts::<TextArea>(PropertyId::BackgroundColor));
    assert!(host_style_accepts::<Image>(PropertyId::BackgroundColor));
    assert!(host_style_accepts::<Svg>(PropertyId::BackgroundColor));

    assert!(host_style_accepts::<Text>(PropertyId::Color));
    assert!(host_style_accepts::<Text>(PropertyId::FontSize));
    assert!(!host_style_accepts::<Text>(PropertyId::BackgroundColor));
    assert!(!host_style_accepts::<TextAreaProjectionSegment>(
        PropertyId::Color
    ));
}

#[test]
fn host_tag_style_contract_matches_validation() {
    let element_style = ElementStylePropSchema {
        background_color: Some(color("#224466")),
        ..Default::default()
    }
    .to_style();
    assert_eq!(
        validate_style::<<ElementStylePropSchema as StylePropTrait>::Accepted>(&element_style),
        Ok(())
    );

    let text_style = TextStylePropSchema {
        color: Some(color("#224466")),
        ..Default::default()
    }
    .to_style();
    assert_eq!(
        validate_style::<<TextStylePropSchema as StylePropTrait>::Accepted>(&text_style),
        Ok(())
    );

    assert_eq!(
        validate_style::<<TextStylePropSchema as StylePropTrait>::Accepted>(&element_style),
        Err(
            crate::style::style_props::StylePropError::unsupported_property(
                PropertyId::BackgroundColor
            )
        )
    );
}

fn text_style() -> TextStylePropSchema {
    let hover = hover_text_style();
    TextStylePropSchema {
        width: hover.width,
        height: hover.height,
        color: hover.color.clone(),
        font: hover.font.clone(),
        font_size: hover.font_size,
        font_weight: hover.font_weight,
        text_wrap: hover.text_wrap,
        cursor: hover.cursor,
        hover: None,
        opacity: hover.opacity,
        transform: hover.transform.clone(),
        transform_origin: hover.transform_origin,
        transition: hover.transition.clone(),
    }
}

fn element_style() -> ElementStylePropSchema {
    let text = text_style();
    ElementStylePropSchema {
        width: text.width,
        height: text.height,
        color: text.color.clone(),
        font: text.font.clone(),
        font_size: text.font_size,
        font_weight: text.font_weight,
        text_wrap: text.text_wrap,
        cursor: text.cursor,
        opacity: text.opacity,
        transform: text.transform.clone(),
        transform_origin: text.transform_origin,
        transition: text.transition.clone(),
        background_color: Some(color("#abcdef")),
        layout: Some(Layout::Inline),
        ..Default::default()
    }
}

fn assert_shared_fields(style: &Style) {
    assert!(matches!(
        style.get(PropertyId::Width),
        Some(ParsedValue::Length(_))
    ));
    assert!(matches!(
        style.get(PropertyId::Height),
        Some(ParsedValue::Length(_))
    ));
    assert!(matches!(
        style.get(PropertyId::Color),
        Some(ParsedValue::Color(_))
    ));
    assert!(matches!(
        style.get(PropertyId::FontFamily),
        Some(ParsedValue::FontFamily(_))
    ));
    assert!(matches!(
        style.get(PropertyId::FontSize),
        Some(ParsedValue::FontSize(_))
    ));
    assert!(matches!(
        style.get(PropertyId::FontWeight),
        Some(ParsedValue::FontWeight(_))
    ));
    assert_eq!(
        style.get(PropertyId::TextWrap),
        Some(&ParsedValue::TextWrap(TextWrap::NoWrap))
    );
    assert_eq!(
        style.get(PropertyId::Cursor),
        Some(&ParsedValue::Cursor(Cursor::Text))
    );
    assert_eq!(
        style.get(PropertyId::Opacity),
        Some(&ParsedValue::Opacity(Opacity::new(0.75)))
    );
    assert!(matches!(
        style.get(PropertyId::Transform),
        Some(ParsedValue::Transform(_))
    ));
    assert_eq!(
        style.get(PropertyId::TransformOrigin),
        Some(&ParsedValue::TransformOrigin(TransformOrigin::px(3.0, 4.0)))
    );
    assert!(matches!(
        style.get(PropertyId::Transition),
        Some(ParsedValue::Transition(_))
    ));
}

#[test]
fn text_style_lowering_keeps_shared_fields() {
    let style = text_style().to_style();

    assert_shared_fields(&style);
    assert!(style.hover().is_none());
}

#[test]
fn element_style_lowering_keeps_shared_and_element_fields() {
    let style = element_style().to_style();

    assert_shared_fields(&style);
    assert_eq!(
        style.get(PropertyId::Layout),
        Some(&ParsedValue::Layout(Layout::Inline))
    );
    assert!(matches!(
        style.get(PropertyId::BackgroundColor),
        Some(ParsedValue::Color(_))
    ));
}

#[test]
fn hover_lowering_keeps_shared_fields() {
    let schema = TextStylePropSchema {
        hover: Some(hover_text_style()),
        ..text_style()
    };
    let style = schema.to_style();

    assert_shared_fields(style.hover().expect("hover style should lower"));
}

#[test]
fn inherent_and_trait_to_style_match_for_element_style() {
    let schema = ElementStylePropSchema {
        hover: Some(HoverElementStylePropSchema {
            background_color: Some(color("#111111")),
            width: Some(Length::px(80.0)),
            ..Default::default()
        }),
        ..element_style()
    };

    assert_eq!(
        ElementStylePropSchema::to_style(&schema),
        <ElementStylePropSchema as StylePropTrait>::to_style(&schema)
    );
}

#[test]
fn inherent_and_trait_to_style_match_for_text_style() {
    let schema = TextStylePropSchema {
        hover: Some(hover_text_style()),
        ..text_style()
    };

    assert_eq!(
        TextStylePropSchema::to_style(&schema),
        <TextStylePropSchema as StylePropTrait>::to_style(&schema)
    );
}
