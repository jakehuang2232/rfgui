#![allow(missing_docs)]

//! Background value types used by the typed style system.

use crate::style::color::{Color, ColorLike, HexColor, OklchColor};
use crate::style::gradient::{ConicBuilder, Gradient, LinearBuilder, RadialBuilder};

/// A typed background declaration — accepts either a solid color or a gradient.
#[derive(Clone)]
pub enum Background {
    Color(Box<dyn ColorLike>),
    Gradient(Gradient),
}

impl From<Gradient> for Background {
    fn from(value: Gradient) -> Self {
        Self::Gradient(value)
    }
}

impl From<LinearBuilder> for Background {
    fn from(value: LinearBuilder) -> Self {
        Self::Gradient(value.build())
    }
}

impl From<RadialBuilder> for Background {
    fn from(value: RadialBuilder) -> Self {
        Self::Gradient(value.build())
    }
}

impl From<ConicBuilder> for Background {
    fn from(value: ConicBuilder) -> Self {
        Self::Gradient(value.build())
    }
}

impl From<Box<dyn ColorLike>> for Background {
    fn from(value: Box<dyn ColorLike>) -> Self {
        Self::Color(value)
    }
}

impl From<Color> for Background {
    fn from(value: Color) -> Self {
        Self::Color(Box::new(value))
    }
}

impl From<OklchColor> for Background {
    fn from(value: OklchColor) -> Self {
        Self::Color(Box::new(value))
    }
}

impl<'a> From<HexColor<'a>> for Background {
    fn from(value: HexColor<'a>) -> Self {
        Self::Color(Box::new(Color::rgba(
            value.to_rgba_u8()[0],
            value.to_rgba_u8()[1],
            value.to_rgba_u8()[2],
            value.to_rgba_u8()[3],
        )))
    }
}

impl From<&str> for Background {
    fn from(value: &str) -> Self {
        Color::hex(value).into()
    }
}

impl From<String> for Background {
    fn from(value: String) -> Self {
        value.as_str().into()
    }
}
