#![allow(missing_docs)]

//! Typed gradient declarations used by `background-image` and related properties.

use crate::style::color::{Color, ColorLike, StyleColor};
use crate::style::parsed_style::{Angle, Length};

/// A single color stop in a gradient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorStop {
    pub color: StyleColor,
    pub position: Option<Length>,
}

impl ColorStop {
    pub fn new<C: ColorLike>(color: C, position: Option<Length>) -> Self {
        Self {
            color: color.to_style_color(),
            position,
        }
    }
}

/// CSS `<side-or-corner>` keyword for linear gradient direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideOrCorner {
    Top,
    Right,
    Bottom,
    Left,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Direction specification for a linear gradient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientLine {
    Angle(Angle),
    ToSide(SideOrCorner),
}

impl From<Angle> for GradientLine {
    fn from(value: Angle) -> Self {
        Self::Angle(value)
    }
}

impl From<SideOrCorner> for GradientLine {
    fn from(value: SideOrCorner) -> Self {
        Self::ToSide(value)
    }
}

/// Gradient center position, relative to the paint box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position2D {
    pub x: Length,
    pub y: Length,
}

impl Position2D {
    pub const fn new(x: Length, y: Length) -> Self {
        Self { x, y }
    }

    pub fn center() -> Self {
        Self {
            x: Length::percent(50.0),
            y: Length::percent(50.0),
        }
    }
}

impl Default for Position2D {
    fn default() -> Self {
        Self::center()
    }
}

/// Radial gradient shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RadialShape {
    Circle,
    Ellipse,
}

/// Radial gradient sizing keyword (CSS `<radial-size>`), or explicit radii.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RadialSize {
    ClosestSide,
    ClosestCorner,
    FarthestSide,
    FarthestCorner,
    Explicit { rx: Length, ry: Length },
}

impl Default for RadialSize {
    fn default() -> Self {
        Self::FarthestCorner
    }
}

/// A typed gradient value.
#[derive(Debug, Clone, PartialEq)]
pub enum Gradient {
    Linear {
        line: GradientLine,
        stops: Vec<ColorStop>,
        repeating: bool,
    },
    Radial {
        shape: RadialShape,
        size: RadialSize,
        position: Position2D,
        stops: Vec<ColorStop>,
        repeating: bool,
    },
    Conic {
        from: Angle,
        position: Position2D,
        stops: Vec<ColorStop>,
        repeating: bool,
    },
}

impl Gradient {
    pub fn linear<L: Into<GradientLine>>(line: L) -> LinearBuilder {
        LinearBuilder {
            line: line.into(),
            stops: Vec::new(),
            repeating: false,
        }
    }

    pub fn radial() -> RadialBuilder {
        RadialBuilder {
            shape: RadialShape::Ellipse,
            size: RadialSize::default(),
            position: Position2D::center(),
            stops: Vec::new(),
            repeating: false,
        }
    }

    pub fn rainbow<L: Into<GradientLine>>(line: L) -> Gradient {
        const STOPS: [&str; 7] = [
            "#ff0000", // red
            "#ff7f00", // orange
            "#ffff00", // yellow
            "#00ff00", // green
            "#0000ff", // blue
            "#4b0082", // indigo
            "#9400d3", // violet
        ];
        let mut builder = Self::linear(line);
        for hex in STOPS {
            builder = builder.stop(Color::hex(hex), None);
        }
        builder.build()
    }

    pub fn conic(from: Angle) -> ConicBuilder {
        ConicBuilder {
            from,
            position: Position2D::center(),
            stops: Vec::new(),
            repeating: false,
        }
    }

    pub fn stops(&self) -> &[ColorStop] {
        match self {
            Self::Linear { stops, .. }
            | Self::Radial { stops, .. }
            | Self::Conic { stops, .. } => stops,
        }
    }

    pub fn is_repeating(&self) -> bool {
        match self {
            Self::Linear { repeating, .. }
            | Self::Radial { repeating, .. }
            | Self::Conic { repeating, .. } => *repeating,
        }
    }
}

pub struct LinearBuilder {
    line: GradientLine,
    stops: Vec<ColorStop>,
    repeating: bool,
}

impl LinearBuilder {
    pub fn stop<C: ColorLike>(mut self, color: C, position: Option<Length>) -> Self {
        self.stops.push(ColorStop::new(color, position));
        self
    }

    pub fn stops<I, C>(mut self, iter: I) -> Self
    where
        I: IntoIterator<Item = (C, Option<Length>)>,
        C: ColorLike,
    {
        for (color, position) in iter {
            self.stops.push(ColorStop::new(color, position));
        }
        self
    }

    pub fn repeating(mut self) -> Self {
        self.repeating = true;
        self
    }

    pub fn build(self) -> Gradient {
        Gradient::Linear {
            line: self.line,
            stops: self.stops,
            repeating: self.repeating,
        }
    }
}

impl From<LinearBuilder> for Gradient {
    fn from(value: LinearBuilder) -> Self {
        value.build()
    }
}

pub struct RadialBuilder {
    shape: RadialShape,
    size: RadialSize,
    position: Position2D,
    stops: Vec<ColorStop>,
    repeating: bool,
}

impl RadialBuilder {
    pub fn circle(mut self) -> Self {
        self.shape = RadialShape::Circle;
        self
    }

    pub fn ellipse(mut self) -> Self {
        self.shape = RadialShape::Ellipse;
        self
    }

    pub fn size(mut self, size: RadialSize) -> Self {
        self.size = size;
        self
    }

    pub fn at(mut self, position: Position2D) -> Self {
        self.position = position;
        self
    }

    pub fn at_center(self) -> Self {
        self.at(Position2D::center())
    }

    pub fn stop<C: ColorLike>(mut self, color: C, position: Option<Length>) -> Self {
        self.stops.push(ColorStop::new(color, position));
        self
    }

    pub fn stops<I, C>(mut self, iter: I) -> Self
    where
        I: IntoIterator<Item = (C, Option<Length>)>,
        C: ColorLike,
    {
        for (color, position) in iter {
            self.stops.push(ColorStop::new(color, position));
        }
        self
    }

    pub fn repeating(mut self) -> Self {
        self.repeating = true;
        self
    }

    pub fn build(self) -> Gradient {
        Gradient::Radial {
            shape: self.shape,
            size: self.size,
            position: self.position,
            stops: self.stops,
            repeating: self.repeating,
        }
    }
}

impl From<RadialBuilder> for Gradient {
    fn from(value: RadialBuilder) -> Self {
        value.build()
    }
}

pub struct ConicBuilder {
    from: Angle,
    position: Position2D,
    stops: Vec<ColorStop>,
    repeating: bool,
}

impl ConicBuilder {
    pub fn at(mut self, position: Position2D) -> Self {
        self.position = position;
        self
    }

    pub fn stop<C: ColorLike>(mut self, color: C, position: Option<Length>) -> Self {
        self.stops.push(ColorStop::new(color, position));
        self
    }

    pub fn stops<I, C>(mut self, iter: I) -> Self
    where
        I: IntoIterator<Item = (C, Option<Length>)>,
        C: ColorLike,
    {
        for (color, position) in iter {
            self.stops.push(ColorStop::new(color, position));
        }
        self
    }

    pub fn repeating(mut self) -> Self {
        self.repeating = true;
        self
    }

    pub fn build(self) -> Gradient {
        Gradient::Conic {
            from: self.from,
            position: self.position,
            stops: self.stops,
            repeating: self.repeating,
        }
    }
}

impl From<ConicBuilder> for Gradient {
    fn from(value: ConicBuilder) -> Self {
        value.build()
    }
}
