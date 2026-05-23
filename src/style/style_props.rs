#![allow(dead_code)]

// Staged style prop normalization API; later view-layer integration will call this module.
use std::fmt;

use super::{ComputedStyle, PropertyId, Style};

pub(crate) trait StylePropSet {
    fn accepts(property: PropertyId) -> bool;
}

pub(crate) trait StylePropTrait {
    type Accepted: StylePropSet;

    fn to_style(&self) -> Style;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NoStylePropSchema;

pub(crate) struct NoStyleSet;

impl StylePropSet for NoStyleSet {
    fn accepts(_property: PropertyId) -> bool {
        false
    }
}

impl StylePropTrait for NoStylePropSchema {
    type Accepted = NoStyleSet;

    fn to_style(&self) -> Style {
        Style::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StylePropMeta {
    pub(crate) id: PropertyId,
    pub(crate) inherited: bool,
    pub(crate) animatable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StylePropError {
    UnsupportedProperty { property: PropertyId },
}

impl StylePropError {
    pub(crate) const fn unsupported_property(property: PropertyId) -> Self {
        Self::UnsupportedProperty { property }
    }
}

impl fmt::Display for StylePropError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedProperty { property } => {
                write!(f, "unsupported style property {property:?}")
            }
        }
    }
}

pub(crate) struct AllStyleSet;

impl StylePropSet for AllStyleSet {
    fn accepts(_property: PropertyId) -> bool {
        true
    }
}

pub(crate) struct TextStyleSet;

impl StylePropSet for TextStyleSet {
    fn accepts(property: PropertyId) -> bool {
        matches!(
            property,
            PropertyId::Width
                | PropertyId::Height
                | PropertyId::Color
                | PropertyId::FontFamily
                | PropertyId::FontSize
                | PropertyId::FontWeight
                | PropertyId::LineHeight
                | PropertyId::TextWrap
                | PropertyId::Cursor
                | PropertyId::Opacity
                | PropertyId::Transition
        )
    }
}

macro_rules! style_prop_registry {
    ($($property:ident => {
        inherited: $inherited:expr,
        animatable: $animatable:expr $(,)?
    }),+ $(,)?) => {
        pub(crate) const STYLE_PROP_REGISTRY: &[StylePropMeta] = &[
            $(
                StylePropMeta {
                    id: PropertyId::$property,
                    inherited: $inherited,
                    animatable: $animatable,
                },
            )+
        ];

        #[allow(dead_code)]
        pub(crate) const ALL_STYLE_PROPERTIES: &[PropertyId] = &[
            $(PropertyId::$property,)+
        ];

        pub(crate) fn property_is_inherited(property: PropertyId) -> bool {
            match property {
                $(PropertyId::$property => $inherited,)+
            }
        }

        pub(crate) fn apply_inherited_properties(
            parent: &ComputedStyle,
            child: &mut ComputedStyle,
        ) {
            $(
                if $inherited {
                    apply_inherited_property(PropertyId::$property, parent, child);
                }
            )+
        }
    };
}

style_prop_registry! {
    Layout => { inherited: false, animatable: false },
    CrossSize => { inherited: false, animatable: false },
    Align => { inherited: false, animatable: false },
    Flex => { inherited: false, animatable: true },
    Position => { inherited: false, animatable: true },
    Width => { inherited: false, animatable: true },
    Height => { inherited: false, animatable: true },
    MinWidth => { inherited: false, animatable: true },
    MinHeight => { inherited: false, animatable: true },
    MaxWidth => { inherited: false, animatable: true },
    MaxHeight => { inherited: false, animatable: true },
    MarginTop => { inherited: false, animatable: true },
    MarginRight => { inherited: false, animatable: true },
    MarginBottom => { inherited: false, animatable: true },
    MarginLeft => { inherited: false, animatable: true },
    PaddingTop => { inherited: false, animatable: true },
    PaddingRight => { inherited: false, animatable: true },
    PaddingBottom => { inherited: false, animatable: true },
    PaddingLeft => { inherited: false, animatable: true },
    Gap => { inherited: false, animatable: true },
    ScrollDirection => { inherited: false, animatable: false },
    Cursor => { inherited: true, animatable: false },
    Color => { inherited: true, animatable: true },
    BackgroundColor => { inherited: false, animatable: true },
    BackgroundImage => { inherited: false, animatable: false },
    BorderImage => { inherited: false, animatable: false },
    FontFamily => { inherited: true, animatable: false },
    FontSize => { inherited: true, animatable: true },
    FontWeight => { inherited: true, animatable: false },
    LineHeight => { inherited: true, animatable: false },
    TextWrap => { inherited: true, animatable: false },
    BorderRadius => { inherited: false, animatable: true },
    BorderTopLeftRadius => { inherited: false, animatable: true },
    BorderTopRightRadius => { inherited: false, animatable: true },
    BorderBottomRightRadius => { inherited: false, animatable: true },
    BorderBottomLeftRadius => { inherited: false, animatable: true },
    BorderWidth => { inherited: false, animatable: true },
    BorderColor => { inherited: false, animatable: true },
    BorderTopWidth => { inherited: false, animatable: true },
    BorderRightWidth => { inherited: false, animatable: true },
    BorderBottomWidth => { inherited: false, animatable: true },
    BorderLeftWidth => { inherited: false, animatable: true },
    BorderTopColor => { inherited: false, animatable: true },
    BorderRightColor => { inherited: false, animatable: true },
    BorderBottomColor => { inherited: false, animatable: true },
    BorderLeftColor => { inherited: false, animatable: true },
    Opacity => { inherited: false, animatable: true },
    BoxShadow => { inherited: false, animatable: true },
    Transform => { inherited: false, animatable: true },
    TransformOrigin => { inherited: false, animatable: true },
    Transition => { inherited: false, animatable: false },
    Animator => { inherited: false, animatable: false },
    VerticalAlign => { inherited: true, animatable: false },
}

pub(crate) fn style_prop_meta(property: PropertyId) -> Option<&'static StylePropMeta> {
    STYLE_PROP_REGISTRY.iter().find(|meta| meta.id == property)
}

pub(crate) fn all_style_properties() -> impl Iterator<Item = PropertyId> + 'static {
    STYLE_PROP_REGISTRY.iter().map(|meta| meta.id)
}

fn apply_inherited_property(
    property: PropertyId,
    parent: &ComputedStyle,
    child: &mut ComputedStyle,
) {
    // Only one-to-one ComputedStyle fields are copied here. Shorthands and
    // edge/corner fields should be added only when their computed mapping is
    // explicit and lossless.
    match property {
        PropertyId::Color => child.color = parent.color,
        PropertyId::Cursor => child.cursor = parent.cursor,
        PropertyId::FontFamily => child.font_families = parent.font_families.clone(),
        PropertyId::FontSize => child.font_size = parent.font_size,
        PropertyId::FontWeight => child.font_weight = parent.font_weight,
        PropertyId::LineHeight => child.line_height = parent.line_height,
        PropertyId::TextWrap => child.text_wrap = parent.text_wrap,
        PropertyId::VerticalAlign => child.vertical_align = parent.vertical_align,
        _ => {}
    }
}

pub(crate) fn validate_style<S>(style: &Style) -> Result<(), StylePropError>
where
    S: StylePropSet,
{
    validate_style_node::<S>(style)
}

fn validate_style_node<S>(style: &Style) -> Result<(), StylePropError>
where
    S: StylePropSet,
{
    for declaration in style.declarations() {
        if !S::accepts(declaration.property) {
            return Err(StylePropError::unsupported_property(declaration.property));
        }
    }

    if let Some(hover) = style.hover() {
        validate_style_node::<S>(hover)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{
        Color, ComputedStyle, Cursor, FontFamily, FontSize, FontWeight, Layout, Length, LineHeight,
        Opacity, ParsedValue, SizeValue, TextWrap, Transition, TransitionProperty, Transitions,
        VerticalAlign,
    };

    struct TestStyleProp(Style);

    impl StylePropTrait for TestStyleProp {
        type Accepted = TextStyleSet;

        fn to_style(&self) -> Style {
            self.0.clone()
        }
    }

    fn assert_valid<S>(style: &Style)
    where
        S: StylePropSet,
    {
        assert_eq!(validate_style::<S>(style), Ok(()));
    }

    fn assert_rejects<S>(style: &Style, property: PropertyId)
    where
        S: StylePropSet,
    {
        assert_eq!(
            validate_style::<S>(style),
            Err(StylePropError::unsupported_property(property))
        );
    }

    #[test]
    fn all_style_set_accepts_every_registered_property() {
        for property in ALL_STYLE_PROPERTIES {
            assert!(AllStyleSet::accepts(*property), "{property:?}");
        }
    }

    #[test]
    fn registry_and_all_properties_stay_in_lockstep() {
        let from_registry: Vec<_> = all_style_properties().collect();

        assert_eq!(from_registry, ALL_STYLE_PROPERTIES);
        assert_eq!(STYLE_PROP_REGISTRY.len(), ALL_STYLE_PROPERTIES.len());
    }

    #[test]
    fn registry_has_unique_property_ids() {
        for (index, property) in ALL_STYLE_PROPERTIES.iter().enumerate() {
            assert!(
                !ALL_STYLE_PROPERTIES[..index].contains(property),
                "duplicate style property {property:?}"
            );
        }
    }

    #[test]
    fn registry_covers_current_property_id_variants() {
        let expected = [
            PropertyId::Layout,
            PropertyId::CrossSize,
            PropertyId::Align,
            PropertyId::Flex,
            PropertyId::Position,
            PropertyId::Width,
            PropertyId::Height,
            PropertyId::MinWidth,
            PropertyId::MinHeight,
            PropertyId::MaxWidth,
            PropertyId::MaxHeight,
            PropertyId::MarginTop,
            PropertyId::MarginRight,
            PropertyId::MarginBottom,
            PropertyId::MarginLeft,
            PropertyId::PaddingTop,
            PropertyId::PaddingRight,
            PropertyId::PaddingBottom,
            PropertyId::PaddingLeft,
            PropertyId::Gap,
            PropertyId::ScrollDirection,
            PropertyId::Cursor,
            PropertyId::Color,
            PropertyId::BackgroundColor,
            PropertyId::BackgroundImage,
            PropertyId::BorderImage,
            PropertyId::FontFamily,
            PropertyId::FontSize,
            PropertyId::FontWeight,
            PropertyId::LineHeight,
            PropertyId::TextWrap,
            PropertyId::BorderRadius,
            PropertyId::BorderTopLeftRadius,
            PropertyId::BorderTopRightRadius,
            PropertyId::BorderBottomRightRadius,
            PropertyId::BorderBottomLeftRadius,
            PropertyId::BorderWidth,
            PropertyId::BorderColor,
            PropertyId::BorderTopWidth,
            PropertyId::BorderRightWidth,
            PropertyId::BorderBottomWidth,
            PropertyId::BorderLeftWidth,
            PropertyId::BorderTopColor,
            PropertyId::BorderRightColor,
            PropertyId::BorderBottomColor,
            PropertyId::BorderLeftColor,
            PropertyId::Opacity,
            PropertyId::BoxShadow,
            PropertyId::Transform,
            PropertyId::TransformOrigin,
            PropertyId::Transition,
            PropertyId::Animator,
            PropertyId::VerticalAlign,
        ];

        for property in expected {
            assert!(
                style_prop_meta(property).is_some(),
                "missing registry metadata for {property:?}"
            );
        }
        assert_eq!(ALL_STYLE_PROPERTIES.len(), expected.len());
    }

    #[test]
    fn style_prop_meta_finds_text_style_fields() {
        for property in [
            PropertyId::Width,
            PropertyId::Height,
            PropertyId::Color,
            PropertyId::FontFamily,
            PropertyId::FontSize,
            PropertyId::FontWeight,
            PropertyId::LineHeight,
            PropertyId::TextWrap,
            PropertyId::Cursor,
            PropertyId::Opacity,
            PropertyId::Transition,
        ] {
            assert!(TextStyleSet::accepts(property), "{property:?}");
            assert!(
                style_prop_meta(property).is_some(),
                "missing text style metadata for {property:?}"
            );
        }
    }

    #[test]
    fn inherited_metadata_marks_text_cascade_fields() {
        for property in [
            PropertyId::Color,
            PropertyId::FontFamily,
            PropertyId::FontSize,
            PropertyId::FontWeight,
            PropertyId::LineHeight,
            PropertyId::TextWrap,
            PropertyId::Cursor,
            PropertyId::VerticalAlign,
        ] {
            assert!(
                style_prop_meta(property).is_some_and(|meta| meta.inherited),
                "{property:?} should be inherited"
            );
        }
    }

    #[test]
    fn property_is_inherited_uses_registry_metadata() {
        for property in [
            PropertyId::Color,
            PropertyId::FontFamily,
            PropertyId::FontSize,
            PropertyId::FontWeight,
            PropertyId::LineHeight,
            PropertyId::TextWrap,
            PropertyId::Cursor,
            PropertyId::VerticalAlign,
        ] {
            assert!(property_is_inherited(property), "{property:?}");
        }

        for property in [
            PropertyId::Width,
            PropertyId::BackgroundColor,
            PropertyId::Opacity,
        ] {
            assert!(!property_is_inherited(property), "{property:?}");
        }
    }

    #[test]
    fn apply_inherited_properties_copies_registry_inherited_computed_fields() {
        let mut parent = ComputedStyle::default();
        parent.color = Color::rgb(0x12, 0x34, 0x56);
        parent.font_families = vec!["Inter".to_string(), "system-ui".to_string()];
        parent.font_size = 22.0;
        parent.font_weight = 650;
        parent.line_height = 1.7;
        parent.text_wrap = TextWrap::NoWrap;
        parent.cursor = Cursor::Pointer;
        parent.vertical_align = VerticalAlign::Middle;

        let mut child = ComputedStyle::default();
        apply_inherited_properties(&parent, &mut child);

        assert_eq!(child.color, parent.color);
        assert_eq!(child.font_families, parent.font_families);
        assert_eq!(child.font_size, parent.font_size);
        assert_eq!(child.font_weight, parent.font_weight);
        assert_eq!(child.line_height, parent.line_height);
        assert_eq!(child.text_wrap, parent.text_wrap);
        assert_eq!(child.cursor, parent.cursor);
        assert_eq!(child.vertical_align, parent.vertical_align);
    }

    #[test]
    fn apply_inherited_properties_does_not_copy_non_inherited_computed_fields() {
        let mut parent = ComputedStyle::default();
        parent.width = SizeValue::Length(Length::px(240.0));
        parent.background_color = Color::rgb(0xaa, 0xbb, 0xcc);
        parent.opacity = 0.42;

        let mut child = ComputedStyle::default();
        child.width = SizeValue::Length(Length::px(80.0));
        child.background_color = Color::rgb(0x01, 0x02, 0x03);
        child.opacity = 0.9;

        apply_inherited_properties(&parent, &mut child);

        assert_eq!(child.width, SizeValue::Length(Length::px(80.0)));
        assert_eq!(child.background_color, Color::rgb(0x01, 0x02, 0x03));
        assert_eq!(child.opacity, 0.9);
    }

    #[test]
    fn registry_marks_animatable_style_language_fields() {
        for property in [
            PropertyId::Width,
            PropertyId::BackgroundColor,
            PropertyId::Transform,
        ] {
            assert!(
                style_prop_meta(property).is_some_and(|meta| meta.animatable),
                "{property:?} should be marked animatable"
            );
        }

        for property in [PropertyId::Transition, PropertyId::Animator] {
            assert!(
                style_prop_meta(property).is_some_and(|meta| !meta.animatable),
                "{property:?} schedules style changes but is not directly animatable"
            );
        }
    }

    #[test]
    fn all_style_set_accepts_existing_declarations() {
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(12, 34, 56)),
        );
        style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Opacity,
                120,
            ))),
        );

        assert_valid::<AllStyleSet>(&style);
    }

    #[test]
    fn text_style_set_accepts_text_fields() {
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(32.0)));
        style.insert(
            PropertyId::Color,
            ParsedValue::color_like(Color::rgb(12, 34, 56)),
        );
        style.insert(
            PropertyId::FontFamily,
            ParsedValue::FontFamily(FontFamily::new(["Inter"])),
        );
        style.insert(
            PropertyId::FontSize,
            ParsedValue::FontSize(FontSize::px(16.0)),
        );
        style.insert(
            PropertyId::FontWeight,
            ParsedValue::FontWeight(FontWeight::new(600)),
        );
        style.insert(
            PropertyId::LineHeight,
            ParsedValue::LineHeight(LineHeight::new(1.4)),
        );
        style.insert(
            PropertyId::TextWrap,
            ParsedValue::TextWrap(TextWrap::NoWrap),
        );
        style.insert(PropertyId::Cursor, ParsedValue::Cursor(Cursor::Text));
        style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.7)));
        style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Opacity,
                120,
            ))),
        );

        assert_valid::<TextStyleSet>(&style);
    }

    #[test]
    fn text_style_set_rejects_background_and_layout_fields() {
        let mut background = Style::new();
        background.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(12, 34, 56)),
        );
        assert_rejects::<TextStyleSet>(&background, PropertyId::BackgroundColor);

        let mut layout = Style::new();
        layout.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        assert_rejects::<TextStyleSet>(&layout, PropertyId::Layout);
    }

    #[test]
    fn validate_style_recurses_into_hover_style() {
        let mut hover = Style::new();
        hover.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(12, 34, 56)),
        );

        let style = Style::new().with_hover(hover);

        assert_rejects::<TextStyleSet>(&style, PropertyId::BackgroundColor);
    }

    #[test]
    fn style_prop_trait_lowers_to_style_without_computing() {
        let mut style = Style::new();
        style.insert(
            PropertyId::Color,
            ParsedValue::color_like(Color::rgb(12, 34, 56)),
        );
        let prop = TestStyleProp(style.clone());

        assert_eq!(prop.to_style(), style);
        assert_valid::<TextStyleSet>(&prop.to_style());
    }
}
