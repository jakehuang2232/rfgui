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
                | PropertyId::Transform
                | PropertyId::TransformOrigin
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
mod tests;
