use crate::style::color::{Color, ColorLike};
use std::collections::HashMap;
use std::ops::Add;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyId {
    Display,
    FlexDirection,
    FlexWrap,
    JustifyContent,
    AlignItems,
    Position,
    Width,
    Height,
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
    MarginTop,
    MarginRight,
    MarginBottom,
    MarginLeft,
    PaddingTop,
    PaddingRight,
    PaddingBottom,
    PaddingLeft,
    Gap,
    ScrollDirection,
    Color,
    BackgroundColor,
    FontSize,
    FontWeight,
    LineHeight,
    BorderRadius,
    BorderTopLeftRadius,
    BorderTopRightRadius,
    BorderBottomRightRadius,
    BorderBottomLeftRadius,
    BorderWidth,
    BorderColor,
    BorderTopWidth,
    BorderRightWidth,
    BorderBottomWidth,
    BorderLeftWidth,
    BorderTopColor,
    BorderRightColor,
    BorderBottomColor,
    BorderLeftColor,
    Opacity,
    Transition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionProperty {
    All,
    Position,
    PositionX,
    PositionY,
    X,
    Y,
    Width,
    Height,
    Gap,
    Padding,
    BorderWidth,
    BorderColor,
    BorderRadius,
    Opacity,
    BackgroundColor,
    Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionTiming {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    pub property: TransitionProperty,
    pub duration_ms: u32,
    pub delay_ms: u32,
    pub timing: TransitionTiming,
}

impl Transition {
    pub const fn new(property: TransitionProperty, duration_ms: u32) -> Self {
        Self {
            property,
            duration_ms,
            delay_ms: 0,
            timing: TransitionTiming::Linear,
        }
    }

    pub const fn delay(mut self, delay_ms: u32) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    pub const fn timing(mut self, timing: TransitionTiming) -> Self {
        self.timing = timing;
        self
    }

    pub const fn linear(self) -> Self {
        self.timing(TransitionTiming::Linear)
    }

    pub const fn ease_in(self) -> Self {
        self.timing(TransitionTiming::EaseIn)
    }

    pub const fn ease_out(self) -> Self {
        self.timing(TransitionTiming::EaseOut)
    }

    pub const fn ease_in_out(self) -> Self {
        self.timing(TransitionTiming::EaseInOut)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Transitions(Vec<Transition>);

impl Transitions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn single(transition: Transition) -> Self {
        Self(vec![transition])
    }

    pub fn as_slice(&self) -> &[Transition] {
        &self.0
    }

    pub fn into_vec(self) -> Vec<Transition> {
        self.0
    }
}

impl From<Transition> for Transitions {
    fn from(value: Transition) -> Self {
        Self::single(value)
    }
}

impl From<Vec<Transition>> for Transitions {
    fn from(value: Vec<Transition>) -> Self {
        Self(value)
    }
}

impl<const N: usize> From<[Transition; N]> for Transitions {
    fn from(value: [Transition; N]) -> Self {
        Self(value.to_vec())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Block,
    Inline,
    Flow,
    InlineFlex,
    Grid,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    None,
    Vertical,
    Horizontal,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Px(f32),
    Percent(f32),
    Zero,
}

impl Length {
    pub const fn px(value: f32) -> Self {
        Self::Px(value)
    }

    pub const fn percent(value: f32) -> Self {
        Self::Percent(value)
    }
}

pub struct Unit;

impl Unit {
    pub const fn px(value: f32) -> Length {
        Length::Px(value)
    }

    pub const fn percent(value: f32) -> Length {
        Length::Percent(value)
    }

    pub const fn pct(value: f32) -> Length {
        Length::Percent(value)
    }

    pub const fn precent(value: f32) -> Length {
        Length::Percent(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Padding {
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,
}

impl Padding {
    pub const fn new() -> Self {
        Self {
            top: Length::Zero,
            right: Length::Zero,
            bottom: Length::Zero,
            left: Length::Zero,
        }
    }

    pub const fn uniform(value: Length) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub const fn x(mut self, value: Length) -> Self {
        self.left = value;
        self.right = value;
        self
    }

    pub const fn y(mut self, value: Length) -> Self {
        self.top = value;
        self.bottom = value;
        self
    }

    pub const fn xy(mut self, x: Length, y: Length) -> Self {
        self.left = x;
        self.right = x;
        self.top = y;
        self.bottom = y;
        self
    }

    pub const fn top(mut self, value: Length) -> Self {
        self.top = value;
        self
    }

    pub const fn right(mut self, value: Length) -> Self {
        self.right = value;
        self
    }

    pub const fn bottom(mut self, value: Length) -> Self {
        self.bottom = value;
        self
    }

    pub const fn left(mut self, value: Length) -> Self {
        self.left = value;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderRadius {
    pub top_left: Length,
    pub top_right: Length,
    pub bottom_right: Length,
    pub bottom_left: Length,
}

impl BorderRadius {
    pub const fn new() -> Self {
        Self::uniform(Length::Zero)
    }

    pub const fn all(value: Length) -> Self {
        Self::uniform(value)
    }

    pub const fn uniform(value: Length) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }

    pub const fn top(mut self, value: Length) -> Self {
        self.top_left = value;
        self.top_right = value;
        self
    }

    pub const fn right(mut self, value: Length) -> Self {
        self.top_right = value;
        self.bottom_right = value;
        self
    }

    pub const fn bottom(mut self, value: Length) -> Self {
        self.bottom_left = value;
        self.bottom_right = value;
        self
    }

    pub const fn left(mut self, value: Length) -> Self {
        self.top_left = value;
        self.bottom_left = value;
        self
    }

    pub const fn top_left(mut self, value: Length) -> Self {
        self.top_left = value;
        self
    }

    pub const fn top_right(mut self, value: Length) -> Self {
        self.top_right = value;
        self
    }

    pub const fn bottom_right(mut self, value: Length) -> Self {
        self.bottom_right = value;
        self
    }

    pub const fn bottom_left(mut self, value: Length) -> Self {
        self.bottom_left = value;
        self
    }

    #[allow(non_snake_case)]
    pub const fn topLeft(self, value: Length) -> Self {
        self.top_left(value)
    }

    #[allow(non_snake_case)]
    pub const fn topRight(self, value: Length) -> Self {
        self.top_right(value)
    }

    #[allow(non_snake_case)]
    pub const fn bottomRight(self, value: Length) -> Self {
        self.bottom_right(value)
    }

    #[allow(non_snake_case)]
    pub const fn bottomLeft(self, value: Length) -> Self {
        self.bottom_left(value)
    }
}

pub trait IntoBorderRadius {
    fn into_border_radius(self) -> BorderRadius;
}

impl IntoBorderRadius for BorderRadius {
    fn into_border_radius(self) -> BorderRadius {
        self
    }
}

impl IntoBorderRadius for Length {
    fn into_border_radius(self) -> BorderRadius {
        BorderRadius::uniform(self)
    }
}

impl IntoBorderRadius for f32 {
    fn into_border_radius(self) -> BorderRadius {
        BorderRadius::uniform(Length::px(self))
    }
}

impl IntoBorderRadius for f64 {
    fn into_border_radius(self) -> BorderRadius {
        BorderRadius::uniform(Length::px(self as f32))
    }
}

impl IntoBorderRadius for i32 {
    fn into_border_radius(self) -> BorderRadius {
        BorderRadius::uniform(Length::px(self as f32))
    }
}

impl IntoBorderRadius for i64 {
    fn into_border_radius(self) -> BorderRadius {
        BorderRadius::uniform(Length::px(self as f32))
    }
}

pub struct Border {
    pub uniform_width: Length,
    pub uniform_color: Box<dyn ColorLike>,
    pub top_width: Option<Length>,
    pub right_width: Option<Length>,
    pub bottom_width: Option<Length>,
    pub left_width: Option<Length>,
    pub top_color: Option<Box<dyn ColorLike>>,
    pub right_color: Option<Box<dyn ColorLike>>,
    pub bottom_color: Option<Box<dyn ColorLike>>,
    pub left_color: Option<Box<dyn ColorLike>>,
}

impl Border {
    pub fn all(width: Length, color: &dyn ColorLike) -> Self {
        Self::uniform(width, color)
    }

    pub fn uniform(width: Length, color: &dyn ColorLike) -> Self {
        Self {
            uniform_width: width,
            uniform_color: color.box_clone(),
            top_width: None,
            right_width: None,
            bottom_width: None,
            left_width: None,
            top_color: None,
            right_color: None,
            bottom_color: None,
            left_color: None,
        }
    }

    pub fn width(mut self, width: Length) -> Self {
        self.uniform_width = width;
        self
    }

    pub fn color(mut self, color: &dyn ColorLike) -> Self {
        self.uniform_color = color.box_clone();
        self
    }

    pub fn x(mut self, width: Length) -> Self {
        self.left_width = Some(width);
        self.right_width = Some(width);
        self
    }

    pub fn y(mut self, width: Length) -> Self {
        self.top_width = Some(width);
        self.bottom_width = Some(width);
        self
    }

    pub fn top(mut self, width: Option<Length>, color: Option<&dyn ColorLike>) -> Self {
        self.top_width = width;
        self.top_color = color.map(|c| c.box_clone());
        self
    }

    pub fn right(mut self, width: Option<Length>, color: Option<&dyn ColorLike>) -> Self {
        self.right_width = width;
        self.right_color = color.map(|c| c.box_clone());
        self
    }

    pub fn bottom(mut self, width: Option<Length>, color: Option<&dyn ColorLike>) -> Self {
        self.bottom_width = width;
        self.bottom_color = color.map(|c| c.box_clone());
        self
    }

    pub fn left(mut self, width: Option<Length>, color: Option<&dyn ColorLike>) -> Self {
        self.left_width = width;
        self.left_color = color.map(|c| c.box_clone());
        self
    }

    pub fn resolved_top_width(&self) -> Length {
        self.top_width.unwrap_or(self.uniform_width)
    }

    pub fn resolved_right_width(&self) -> Length {
        self.right_width.unwrap_or(self.uniform_width)
    }

    pub fn resolved_bottom_width(&self) -> Length {
        self.bottom_width.unwrap_or(self.uniform_width)
    }

    pub fn resolved_left_width(&self) -> Length {
        self.left_width.unwrap_or(self.uniform_width)
    }

    pub fn resolved_top_color(&self) -> &dyn ColorLike {
        self.top_color
            .as_deref()
            .unwrap_or(self.uniform_color.as_ref())
    }

    pub fn resolved_right_color(&self) -> &dyn ColorLike {
        self.right_color
            .as_deref()
            .unwrap_or(self.uniform_color.as_ref())
    }

    pub fn resolved_bottom_color(&self) -> &dyn ColorLike {
        self.bottom_color
            .as_deref()
            .unwrap_or(self.uniform_color.as_ref())
    }

    pub fn resolved_left_color(&self) -> &dyn ColorLike {
        self.left_color
            .as_deref()
            .unwrap_or(self.uniform_color.as_ref())
    }

    pub fn top_width(mut self, width: Length) -> Self {
        self.top_width = Some(width);
        self
    }

    pub fn right_width(mut self, width: Length) -> Self {
        self.right_width = Some(width);
        self
    }

    pub fn bottom_width(mut self, width: Length) -> Self {
        self.bottom_width = Some(width);
        self
    }

    pub fn left_width(mut self, width: Length) -> Self {
        self.left_width = Some(width);
        self
    }

    pub fn top_color(mut self, color: &dyn ColorLike) -> Self {
        self.top_color = Some(color.box_clone());
        self
    }

    pub fn right_color(mut self, color: &dyn ColorLike) -> Self {
        self.right_color = Some(color.box_clone());
        self
    }

    pub fn bottom_color(mut self, color: &dyn ColorLike) -> Self {
        self.bottom_color = Some(color.box_clone());
        self
    }

    pub fn left_color(mut self, color: &dyn ColorLike) -> Self {
        self.left_color = Some(color.box_clone());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontWeight(u16);

impl FontWeight {
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineHeight(f32);

impl LineHeight {
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> f32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Opacity(f32);

impl Opacity {
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> f32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedValue {
    Display(Display),
    FlexDirection(FlexDirection),
    FlexWrap(FlexWrap),
    JustifyContent(JustifyContent),
    AlignItems(AlignItems),
    ScrollDirection(ScrollDirection),
    Position(Position),
    Auto,
    Length(Length),
    FontWeight(FontWeight),
    LineHeight(LineHeight),
    Opacity(Opacity),
    Transition(Transitions),
    Color(Color),
}

impl ParsedValue {
    pub fn color_like<T: ColorLike>(color: T) -> Self {
        let [r, g, b, a] = color.to_rgba_u8();
        Self::Color(Color::rgba(r, g, b, a))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub property: PropertyId,
    pub value: ParsedValue,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Style {
    declarations: Vec<Declaration>,
    index: HashMap<PropertyId, usize>,
    hover: Option<Box<Style>>,
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_declarations(declarations: Vec<Declaration>) -> Self {
        let mut parsed = Self::new();
        for declaration in declarations {
            parsed.insert(declaration.property, declaration.value);
        }
        parsed
    }

    pub fn insert(&mut self, property: PropertyId, value: ParsedValue) {
        let declaration = Declaration { property, value };
        match self.index.get(&property).copied() {
            Some(i) => self.declarations[i] = declaration,
            None => {
                self.declarations.push(declaration);
                let idx = self.declarations.len() - 1;
                self.index.insert(property, idx);
            }
        }
    }

    pub fn insert_color_like<T: ColorLike>(&mut self, property: PropertyId, color: T) {
        self.insert(property, ParsedValue::color_like(color));
    }

    pub fn get(&self, property: PropertyId) -> Option<&ParsedValue> {
        self.index
            .get(&property)
            .and_then(|i| self.declarations.get(*i))
            .map(|decl| &decl.value)
    }

    pub fn declarations(&self) -> &[Declaration] {
        &self.declarations
    }

    pub fn hover(&self) -> Option<&Style> {
        self.hover.as_deref()
    }

    pub fn set_hover(&mut self, hover: Style) {
        self.hover = Some(Box::new(hover));
    }

    pub fn with_hover(mut self, hover: Style) -> Self {
        self.set_hover(hover);
        self
    }

    pub fn merge(self, rhs: Self) -> Self {
        let mut merged = self;
        for declaration in rhs.declarations {
            merged.insert(declaration.property, declaration.value);
        }
        merged.hover = match (merged.hover.take(), rhs.hover) {
            (Some(lhs), Some(rhs)) => Some(Box::new((*lhs).merge(*rhs))),
            (Some(lhs), None) => Some(lhs),
            (None, Some(rhs)) => Some(rhs),
            (None, None) => None,
        };
        merged
    }

    pub fn set_padding(&mut self, padding: Padding) {
        self.insert(PropertyId::PaddingTop, ParsedValue::Length(padding.top));
        self.insert(PropertyId::PaddingRight, ParsedValue::Length(padding.right));
        self.insert(
            PropertyId::PaddingBottom,
            ParsedValue::Length(padding.bottom),
        );
        self.insert(PropertyId::PaddingLeft, ParsedValue::Length(padding.left));
    }

    pub fn with_padding(mut self, padding: Padding) -> Self {
        self.set_padding(padding);
        self
    }

    pub fn set_border_radius(&mut self, border_radius: BorderRadius) {
        self.insert(
            PropertyId::BorderTopLeftRadius,
            ParsedValue::Length(border_radius.top_left),
        );
        self.insert(
            PropertyId::BorderTopRightRadius,
            ParsedValue::Length(border_radius.top_right),
        );
        self.insert(
            PropertyId::BorderBottomRightRadius,
            ParsedValue::Length(border_radius.bottom_right),
        );
        self.insert(
            PropertyId::BorderBottomLeftRadius,
            ParsedValue::Length(border_radius.bottom_left),
        );
    }

    pub fn with_border_radius(mut self, border_radius: BorderRadius) -> Self {
        self.set_border_radius(border_radius);
        self
    }

    pub fn set_border(&mut self, border: Border) {
        let [top_r, top_g, top_b, top_a] = border.resolved_top_color().to_rgba_u8();
        let [right_r, right_g, right_b, right_a] = border.resolved_right_color().to_rgba_u8();
        let [bottom_r, bottom_g, bottom_b, bottom_a] = border.resolved_bottom_color().to_rgba_u8();
        let [left_r, left_g, left_b, left_a] = border.resolved_left_color().to_rgba_u8();
        self.insert(
            PropertyId::BorderTopWidth,
            ParsedValue::Length(border.resolved_top_width()),
        );
        self.insert(
            PropertyId::BorderRightWidth,
            ParsedValue::Length(border.resolved_right_width()),
        );
        self.insert(
            PropertyId::BorderBottomWidth,
            ParsedValue::Length(border.resolved_bottom_width()),
        );
        self.insert(
            PropertyId::BorderLeftWidth,
            ParsedValue::Length(border.resolved_left_width()),
        );
        self.insert(
            PropertyId::BorderTopColor,
            ParsedValue::Color(Color::rgba(top_r, top_g, top_b, top_a)),
        );
        self.insert(
            PropertyId::BorderRightColor,
            ParsedValue::Color(Color::rgba(right_r, right_g, right_b, right_a)),
        );
        self.insert(
            PropertyId::BorderBottomColor,
            ParsedValue::Color(Color::rgba(bottom_r, bottom_g, bottom_b, bottom_a)),
        );
        self.insert(
            PropertyId::BorderLeftColor,
            ParsedValue::Color(Color::rgba(left_r, left_g, left_b, left_a)),
        );
    }

    pub fn with_border(mut self, border: Border) -> Self {
        self.set_border(border);
        self
    }
}

impl Add for Style {
    type Output = Style;

    fn add(self, rhs: Self) -> Self::Output {
        self.merge(rhs)
    }
}
