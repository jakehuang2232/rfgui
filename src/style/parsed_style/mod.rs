#![allow(missing_docs)]

//! Typed parsed-style declarations accepted by RSX and style construction APIs.

use crate::style::color::{Color, ColorLike, IntoColor, OklchColor, StyleColor};

use std::collections::HashMap;
use std::ops::Add;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyId {
    Layout,
    CrossSize,
    Align,
    Flex,
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
    Cursor,
    Color,
    BackgroundColor,
    FontFamily,
    FontSize,
    FontWeight,
    LineHeight,
    TextWrap,
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
    BoxShadow,
    Transform,
    TransformOrigin,
    Transition,
    Animator,
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
    BoxShadow,
    Transform,
    TransformOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionTiming {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Count(u32),
    Infinite,
}

impl Repeat {
    pub const fn once() -> Self {
        Self::Count(1)
    }

    pub const fn times(count: u32) -> Self {
        Self::Count(count)
    }

    pub const fn infinite() -> Self {
        Self::Infinite
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillMode {
    None,
    Forwards,
    Backwards,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Running,
    Paused,
}

pub trait IntoAnimationStyle {
    fn into_animation_style(self) -> Style;
}

impl IntoAnimationStyle for Style {
    fn into_animation_style(self) -> Style {
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Keyframe {
    progress: f32,
    style: Style,
}

impl Keyframe {
    pub fn new<T>(progress: f32, style: T) -> Self
    where
        T: IntoAnimationStyle,
    {
        assert!(
            (0.0..=1.0).contains(&progress),
            "animation keyframe progress must be within 0.0..=1.0"
        );
        Self {
            progress,
            style: style.into_animation_style(),
        }
    }

    pub const fn progress(&self) -> f32 {
        self.progress
    }

    pub fn style(&self) -> &Style {
        &self.style
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    keyframes: Vec<Keyframe>,
    duration_ms: Option<u32>,
    delay_ms: Option<i32>,
    timing: Option<TransitionTiming>,
    repeat: Option<Repeat>,
    direction: Option<Direction>,
    fill_mode: Option<FillMode>,
    play_state: Option<PlayState>,
}

impl Animation {
    pub fn new<const N: usize>(keyframes: [Keyframe; N]) -> Self {
        let mut keyframes = keyframes.to_vec();
        keyframes.sort_by(|lhs, rhs| lhs.progress.total_cmp(&rhs.progress));
        Self {
            keyframes,
            duration_ms: None,
            delay_ms: None,
            timing: None,
            repeat: None,
            direction: None,
            fill_mode: None,
            play_state: None,
        }
    }

    pub fn keyframes(&self) -> &[Keyframe] {
        &self.keyframes
    }

    pub const fn duration(mut self, duration_ms: u32) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    pub const fn delay(mut self, delay_ms: i32) -> Self {
        self.delay_ms = Some(delay_ms);
        self
    }

    pub const fn timing(mut self, timing: TransitionTiming) -> Self {
        self.timing = Some(timing);
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

    pub const fn repeat(mut self, repeat: Repeat) -> Self {
        self.repeat = Some(repeat);
        self
    }

    pub const fn direction(mut self, direction: Direction) -> Self {
        self.direction = Some(direction);
        self
    }

    pub const fn fill_mode(mut self, fill_mode: FillMode) -> Self {
        self.fill_mode = Some(fill_mode);
        self
    }

    pub const fn play_state(mut self, play_state: PlayState) -> Self {
        self.play_state = Some(play_state);
        self
    }

    pub const fn duration_ms(&self) -> Option<u32> {
        self.duration_ms
    }

    pub const fn delay_ms(&self) -> Option<i32> {
        self.delay_ms
    }

    pub const fn timing_value(&self) -> Option<TransitionTiming> {
        self.timing
    }

    pub const fn repeat_value(&self) -> Option<Repeat> {
        self.repeat
    }

    pub const fn direction_value(&self) -> Option<Direction> {
        self.direction
    }

    pub const fn fill_mode_value(&self) -> Option<FillMode> {
        self.fill_mode
    }

    pub const fn play_state_value(&self) -> Option<PlayState> {
        self.play_state
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Animator {
    animations: Vec<Animation>,
    duration_ms: Option<u32>,
    delay_ms: Option<i32>,
    timing: Option<TransitionTiming>,
    repeat: Option<Repeat>,
    direction: Option<Direction>,
    fill_mode: Option<FillMode>,
    play_state: Option<PlayState>,
}

impl Animator {
    pub fn new<const N: usize>(animations: [Animation; N]) -> Self {
        Self {
            animations: animations.to_vec(),
            duration_ms: None,
            delay_ms: None,
            timing: None,
            repeat: None,
            direction: None,
            fill_mode: None,
            play_state: None,
        }
    }

    pub fn animations(&self) -> &[Animation] {
        &self.animations
    }

    pub fn from_vec(animations: Vec<Animation>) -> Self {
        Self {
            animations,
            duration_ms: None,
            delay_ms: None,
            timing: None,
            repeat: None,
            direction: None,
            fill_mode: None,
            play_state: None,
        }
    }

    pub const fn duration(mut self, duration_ms: u32) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    pub const fn delay(mut self, delay_ms: i32) -> Self {
        self.delay_ms = Some(delay_ms);
        self
    }

    pub const fn timing(mut self, timing: TransitionTiming) -> Self {
        self.timing = Some(timing);
        self
    }

    pub const fn repeat(mut self, repeat: Repeat) -> Self {
        self.repeat = Some(repeat);
        self
    }

    pub const fn direction(mut self, direction: Direction) -> Self {
        self.direction = Some(direction);
        self
    }

    pub const fn fill_mode(mut self, fill_mode: FillMode) -> Self {
        self.fill_mode = Some(fill_mode);
        self
    }

    pub const fn play_state(mut self, play_state: PlayState) -> Self {
        self.play_state = Some(play_state);
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

    pub fn is_empty(&self) -> bool {
        self.animations.is_empty()
    }

    pub fn resolved_duration_ms(&self, animation: &Animation) -> u32 {
        animation.duration_ms.unwrap_or(self.duration_ms.unwrap_or(0))
    }

    pub fn resolved_delay_ms(&self, animation: &Animation) -> i32 {
        animation.delay_ms.unwrap_or(self.delay_ms.unwrap_or(0))
    }

    pub fn resolved_timing(&self, animation: &Animation) -> TransitionTiming {
        animation.timing.unwrap_or(self.timing.unwrap_or(TransitionTiming::EaseInOut))
    }

    pub fn resolved_repeat(&self, animation: &Animation) -> Repeat {
        animation.repeat.unwrap_or(self.repeat.unwrap_or(Repeat::once()))
    }

    pub fn resolved_direction(&self, animation: &Animation) -> Direction {
        animation
            .direction
            .unwrap_or(self.direction.unwrap_or(Direction::Normal))
    }

    pub fn resolved_fill_mode(&self, animation: &Animation) -> FillMode {
        animation
            .fill_mode
            .unwrap_or(self.fill_mode.unwrap_or(FillMode::None))
    }

    pub fn resolved_play_state(&self, animation: &Animation) -> PlayState {
        animation
            .play_state
            .unwrap_or(self.play_state.unwrap_or(PlayState::Running))
    }
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
pub enum Layout {
    Block,
    Inline,
    Flex {
        direction: FlowDirection,
        justify_content: JustifyContent,
        cross_axis: CrossAxis,
    },
    Flow {
        direction: FlowDirection,
        wrap: FlowWrap,
        justify_content: JustifyContent,
        cross_axis: CrossAxis,
    },
    InlineFlex,
    Grid,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlexLayout {
    direction: FlowDirection,
    justify_content: JustifyContent,
    cross_axis: CrossAxis,
}

impl FlexLayout {
    pub const fn row(mut self) -> Self {
        self.direction = FlowDirection::Row;
        self
    }

    pub const fn column(mut self) -> Self {
        self.direction = FlowDirection::Column;
        self
    }

    pub const fn justify_content(mut self, justify_content: JustifyContent) -> Self {
        self.justify_content = justify_content;
        self
    }

    pub const fn cross_size(mut self, cross_size: CrossSize) -> Self {
        self.cross_axis.size = cross_size;
        self
    }

    pub const fn align(mut self, align: Align) -> Self {
        self.cross_axis.align = align;
        self
    }
}

impl From<FlexLayout> for Layout {
    fn from(value: FlexLayout) -> Self {
        Self::Flex {
            direction: value.direction,
            justify_content: value.justify_content,
            cross_axis: value.cross_axis,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlowLayout {
    direction: FlowDirection,
    wrap: FlowWrap,
    justify_content: JustifyContent,
    cross_axis: CrossAxis,
}

impl FlowLayout {
    pub const fn row(mut self) -> Self {
        self.direction = FlowDirection::Row;
        self
    }

    pub const fn column(mut self) -> Self {
        self.direction = FlowDirection::Column;
        self
    }

    pub const fn wrap(mut self) -> Self {
        self.wrap = FlowWrap::Wrap;
        self
    }

    pub const fn no_wrap(mut self) -> Self {
        self.wrap = FlowWrap::NoWrap;
        self
    }

    pub const fn justify_content(mut self, justify_content: JustifyContent) -> Self {
        self.justify_content = justify_content;
        self
    }

    pub const fn cross_size(mut self, cross_size: CrossSize) -> Self {
        self.cross_axis.size = cross_size;
        self
    }

    pub const fn align(mut self, align: Align) -> Self {
        self.cross_axis.align = align;
        self
    }
}

impl From<FlowLayout> for Layout {
    fn from(value: FlowLayout) -> Self {
        Self::Flow {
            direction: value.direction,
            wrap: value.wrap,
            justify_content: value.justify_content,
            cross_axis: value.cross_axis,
        }
    }
}

impl Layout {
    pub const fn flex() -> FlexLayout {
        FlexLayout {
            direction: FlowDirection::Row,
            justify_content: JustifyContent::Start,
            cross_axis: CrossAxis::new(CrossSize::Fit, Align::Start),
        }
    }

    pub const fn flow() -> FlowLayout {
        FlowLayout {
            direction: FlowDirection::Row,
            wrap: FlowWrap::NoWrap,
            justify_content: JustifyContent::Start,
            cross_axis: CrossAxis::new(CrossSize::Fit, Align::Start),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowDirection {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowWrap {
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
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrossAxis {
    pub size: CrossSize,
    pub align: Align,
}

impl CrossAxis {
    pub const fn new(size: CrossSize, align: Align) -> Self {
        Self { size, align }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossSize {
    Fit,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    None,
    Vertical,
    Horizontal,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    Default,
    ContextMenu,
    Help,
    Pointer,
    Progress,
    Wait,
    Cell,
    Crosshair,
    Text,
    VerticalText,
    Alias,
    Copy,
    Move,
    NoDrop,
    NotAllowed,
    Grab,
    Grabbing,
    EResize,
    NResize,
    NeResize,
    NwResize,
    SResize,
    SeResize,
    SwResize,
    WResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    ColResize,
    RowResize,
    AllScroll,
    ZoomIn,
    ZoomOut,
    DndAsk,
    AllResize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionMode {
    Static,
    Relative,
    Absolute,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collision {
    None,
    Flip,
    Fit,
    FlipFit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionBoundary {
    Viewport,
    Parent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipMode {
    Parent,
    Viewport,
    AnchorParent,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnchorName(String);

impl AnchorName {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&str> for AnchorName {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for AnchorName {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    mode: PositionMode,
    anchor: Option<AnchorName>,
    top: Option<Length>,
    right: Option<Length>,
    bottom: Option<Length>,
    left: Option<Length>,
    collision: Collision,
    collision_boundary: CollisionBoundary,
    clip_mode: ClipMode,
}

impl Position {
    pub const fn static_() -> Self {
        Self::new(PositionMode::Static)
    }

    pub const fn relative() -> Self {
        Self::new(PositionMode::Relative)
    }

    pub const fn absolute() -> Self {
        Self::new(PositionMode::Absolute)
    }

    pub const fn fixed() -> Self {
        Self::new(PositionMode::Fixed)
    }

    pub const fn mode(&self) -> PositionMode {
        self.mode
    }

    pub fn anchor(mut self, anchor: impl Into<AnchorName>) -> Self {
        self.anchor = Some(anchor.into());
        self
    }

    pub const fn top(mut self, value: Length) -> Self {
        self.top = Some(value);
        self
    }

    pub const fn right(mut self, value: Length) -> Self {
        self.right = Some(value);
        self
    }

    pub const fn bottom(mut self, value: Length) -> Self {
        self.bottom = Some(value);
        self
    }

    pub const fn left(mut self, value: Length) -> Self {
        self.left = Some(value);
        self
    }

    pub const fn collision(mut self, collision: Collision, boundary: CollisionBoundary) -> Self {
        self.collision = collision;
        self.collision_boundary = boundary;
        self
    }

    pub const fn clip(mut self, mode: ClipMode) -> Self {
        self.clip_mode = mode;
        self
    }

    pub fn anchor_name(&self) -> Option<&AnchorName> {
        self.anchor.as_ref()
    }

    pub const fn top_inset(&self) -> Option<Length> {
        self.top
    }

    pub const fn right_inset(&self) -> Option<Length> {
        self.right
    }

    pub const fn bottom_inset(&self) -> Option<Length> {
        self.bottom
    }

    pub const fn left_inset(&self) -> Option<Length> {
        self.left
    }

    pub const fn collision_mode(&self) -> Collision {
        self.collision
    }

    pub const fn collision_boundary(&self) -> CollisionBoundary {
        self.collision_boundary
    }

    pub const fn clip_mode(&self) -> ClipMode {
        self.clip_mode
    }

    const fn new(mode: PositionMode) -> Self {
        Self {
            mode,
            anchor: None,
            top: None,
            right: None,
            bottom: None,
            left: None,
            collision: Collision::None,
            collision_boundary: CollisionBoundary::Viewport,
            clip_mode: ClipMode::Parent,
        }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::static_()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Px(f32),
    Percent(f32),
    Vw(f32),
    Vh(f32),
    Calc(LengthCalc),
    Zero,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Angle {
    Deg(f32),
    Rad(f32),
}

impl Angle {
    pub const fn deg(value: f32) -> Self {
        Self::Deg(value)
    }

    pub const fn rad(value: f32) -> Self {
        Self::Rad(value)
    }

    pub fn to_radians(self) -> f32 {
        match self {
            Self::Deg(value) => value.to_radians(),
            Self::Rad(value) => value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TransformKind {
    Translate { x: Length, y: Length, z: f32 },
    Scale { x: f32, y: f32, z: f32 },
    Rotate { x: Angle, y: Angle, z: Angle },
    Perspective { depth: f32 },
    Matrix { matrix: [f32; 16] },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformEntry {
    kind: TransformKind,
}

pub struct Translate;
pub struct Scale;
pub struct Rotate;
pub struct Perspective;

impl TransformEntry {
    pub const fn y(mut self, value: Angle) -> Self {
        match self.kind {
            TransformKind::Rotate { x, z, .. } => {
                self.kind = TransformKind::Rotate { x, y: value, z };
            }
            _ => {}
        }
        self
    }

    pub const fn z(mut self, value: Angle) -> Self {
        match self.kind {
            TransformKind::Rotate { x, y, .. } => {
                self.kind = TransformKind::Rotate { x, y, z: value };
            }
            _ => {}
        }
        self
    }

    pub const fn with_y(mut self, value: Length) -> Self {
        match self.kind {
            TransformKind::Translate { x, z, .. } => {
                self.kind = TransformKind::Translate { x, y: value, z };
            }
            _ => {}
        }
        self
    }

    pub const fn with_z(mut self, value: f32) -> Self {
        match self.kind {
            TransformKind::Translate { x, y, .. } => {
                self.kind = TransformKind::Translate { x, y, z: value };
            }
            TransformKind::Scale { x, y, .. } => {
                self.kind = TransformKind::Scale { x, y, z: value };
            }
            _ => {}
        }
        self
    }

    pub(crate) const fn kind(self) -> TransformKind {
        self.kind
    }

    pub(crate) const fn from_matrix(matrix: [f32; 16]) -> Self {
        Self {
            kind: TransformKind::Matrix { matrix },
        }
    }

    pub(crate) const fn identity_like(kind: TransformKind) -> Self {
        match kind {
            TransformKind::Translate { .. } => Translate::xy(Length::Zero, Length::Zero),
            TransformKind::Scale { .. } => Scale::xy(1.0, 1.0).with_z(1.0),
            TransformKind::Rotate { .. } => Rotate::x(Angle::deg(0.0))
                .y(Angle::deg(0.0))
                .z(Angle::deg(0.0)),
            TransformKind::Perspective { .. } => Perspective::px(f32::INFINITY),
            TransformKind::Matrix { .. } => Self::from_matrix([
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ]),
        }
    }
}

impl Translate {
    pub const fn x(value: Length) -> TransformEntry {
        TransformEntry {
            kind: TransformKind::Translate {
                x: value,
                y: Length::Zero,
                z: 0.0,
            },
        }
    }

    pub const fn xy(x: Length, y: Length) -> TransformEntry {
        TransformEntry {
            kind: TransformKind::Translate { x, y, z: 0.0 },
        }
    }
}

impl Scale {
    pub const fn xy(x: f32, y: f32) -> TransformEntry {
        TransformEntry {
            kind: TransformKind::Scale { x, y, z: 1.0 },
        }
    }

    pub const fn uniform(value: f32) -> TransformEntry {
        Self::xy(value, value)
    }
}

impl Rotate {
    pub const fn x(value: Angle) -> TransformEntry {
        TransformEntry {
            kind: TransformKind::Rotate {
                x: value,
                y: Angle::deg(0.0),
                z: Angle::deg(0.0),
            },
        }
    }

    pub const fn y(value: Angle) -> TransformEntry {
        TransformEntry {
            kind: TransformKind::Rotate {
                x: Angle::deg(0.0),
                y: value,
                z: Angle::deg(0.0),
            },
        }
    }

    pub const fn z(value: Angle) -> TransformEntry {
        TransformEntry {
            kind: TransformKind::Rotate {
                x: Angle::deg(0.0),
                y: Angle::deg(0.0),
                z: value,
            },
        }
    }

    pub const fn deg(value: f32) -> TransformEntry {
        Self::z(Angle::deg(value))
    }

    pub const fn rad(value: f32) -> TransformEntry {
        Self::z(Angle::rad(value))
    }
}

impl Perspective {
    pub const fn px(value: f32) -> TransformEntry {
        TransformEntry {
            kind: TransformKind::Perspective { depth: value },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Transform(Vec<TransformEntry>);

impl Transform {
    pub fn new<const N: usize>(entries: [TransformEntry; N]) -> Self {
        Self(entries.to_vec())
    }

    pub(crate) fn from_vec(entries: Vec<TransformEntry>) -> Self {
        Self(entries)
    }

    pub fn as_slice(&self) -> &[TransformEntry] {
        self.0.as_slice()
    }

    pub fn into_vec(self) -> Vec<TransformEntry> {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformOrigin {
    x: Length,
    y: Length,
    z: f32,
}

impl TransformOrigin {
    pub const fn new(x: Length, y: Length) -> Self {
        Self { x, y, z: 0.0 }
    }

    pub const fn center() -> Self {
        Self::new(Length::percent(50.0), Length::percent(50.0))
    }

    pub const fn px(x: f32, y: f32) -> Self {
        Self::new(Length::px(x), Length::px(y))
    }

    pub const fn percent(x: f32, y: f32) -> Self {
        Self::new(Length::percent(x), Length::percent(y))
    }

    pub const fn with_z(mut self, z: f32) -> Self {
        self.z = z;
        self
    }

    pub const fn x(self) -> Length {
        self.x
    }

    pub const fn y(self) -> Length {
        self.y
    }

    pub const fn z(self) -> f32 {
        self.z
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Operator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlusOp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubtractOp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultiplyOp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DivideOp;

impl Operator {
    #[allow(non_upper_case_globals)]
    pub const plus: PlusOp = PlusOp;
    #[allow(non_upper_case_globals)]
    pub const subtract: SubtractOp = SubtractOp;
    #[allow(non_upper_case_globals)]
    pub const multiply: MultiplyOp = MultiplyOp;
    #[allow(non_upper_case_globals)]
    pub const divide: DivideOp = DivideOp;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LengthCalc {
    px: f32,
    percent: f32,
    vw: f32,
    vh: f32,
}

impl LengthCalc {
    pub const fn zero() -> Self {
        Self {
            px: 0.0,
            percent: 0.0,
            vw: 0.0,
            vh: 0.0,
        }
    }

    const fn from_length(length: Length) -> Self {
        match length {
            Length::Px(v) => Self {
                px: v,
                ..Self::zero()
            },
            Length::Percent(v) => Self {
                percent: v,
                ..Self::zero()
            },
            Length::Vw(v) => Self {
                vw: v,
                ..Self::zero()
            },
            Length::Vh(v) => Self {
                vh: v,
                ..Self::zero()
            },
            Length::Calc(v) => v,
            Length::Zero => Self::zero(),
        }
    }

    fn resolve(
        self,
        percent_base: Option<f32>,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Option<f32> {
        if self.percent != 0.0 && percent_base.is_none() {
            return None;
        }
        let percent = percent_base.unwrap_or(0.0).max(0.0) * self.percent * 0.01;
        let vw = viewport_width.max(0.0) * self.vw * 0.01;
        let vh = viewport_height.max(0.0) * self.vh * 0.01;
        Some(self.px + percent + vw + vh)
    }

    const fn resolve_without_percent_base(self, viewport_width: f32, viewport_height: f32) -> f32 {
        let vw = if viewport_width < 0.0 {
            0.0
        } else {
            viewport_width
        } * self.vw
            * 0.01;
        let vh = if viewport_height < 0.0 {
            0.0
        } else {
            viewport_height
        } * self.vh
            * 0.01;
        self.px + vw + vh
    }

    pub const fn has_percent(self) -> bool {
        self.percent != 0.0
    }
}

#[doc(hidden)]
pub trait CalcNumber {
    fn into_calc_number(self) -> f32;
}

impl CalcNumber for f32 {
    fn into_calc_number(self) -> f32 {
        self
    }
}

impl CalcNumber for f64 {
    fn into_calc_number(self) -> f32 {
        self as f32
    }
}

impl CalcNumber for i32 {
    fn into_calc_number(self) -> f32 {
        self as f32
    }
}

#[doc(hidden)]
pub trait CalcRule<Op, Rhs> {
    fn calc(lhs: Length, op: Op, rhs: Rhs) -> Length;
}

impl Length {
    pub const fn px(value: f32) -> Self {
        Self::Px(value)
    }

    pub const fn percent(value: f32) -> Self {
        Self::Percent(value)
    }

    pub const fn full() -> Self {
        Self::Percent(100.0)
    }

    pub const fn vw(value: f32) -> Self {
        Self::Vw(value)
    }

    pub const fn vh(value: f32) -> Self {
        Self::Vh(value)
    }

    pub fn calc<Op, Rhs>(lhs: Length, operator: Op, rhs: Rhs) -> Self
    where
        Self: CalcRule<Op, Rhs>,
    {
        <Self as CalcRule<Op, Rhs>>::calc(lhs, operator, rhs)
    }

    pub fn resolve_with_base(
        self,
        percent_base: Option<f32>,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Option<f32> {
        match self {
            Self::Px(v) => Some(v),
            Self::Percent(v) => percent_base.map(|base| base.max(0.0) * v * 0.01),
            Self::Vw(v) => Some(viewport_width.max(0.0) * v * 0.01),
            Self::Vh(v) => Some(viewport_height.max(0.0) * v * 0.01),
            Self::Calc(calc) => calc.resolve(percent_base, viewport_width, viewport_height),
            Self::Zero => Some(0.0),
        }
    }

    pub fn resolve_without_percent_base(self, viewport_width: f32, viewport_height: f32) -> f32 {
        match self {
            Self::Px(v) => v,
            Self::Percent(_) => 0.0,
            Self::Vw(v) => viewport_width.max(0.0) * v * 0.01,
            Self::Vh(v) => viewport_height.max(0.0) * v * 0.01,
            Self::Calc(calc) => calc.resolve_without_percent_base(viewport_width, viewport_height),
            Self::Zero => 0.0,
        }
    }

    pub const fn needs_percent_base(self) -> bool {
        match self {
            Self::Percent(_) => true,
            Self::Calc(calc) => calc.has_percent(),
            _ => false,
        }
    }
}

impl CalcRule<PlusOp, Length> for Length {
    fn calc(lhs: Length, _op: PlusOp, rhs: Length) -> Length {
        let left = LengthCalc::from_length(lhs);
        let right = LengthCalc::from_length(rhs);
        Length::Calc(LengthCalc {
            px: left.px + right.px,
            percent: left.percent + right.percent,
            vw: left.vw + right.vw,
            vh: left.vh + right.vh,
        })
    }
}

impl<N: CalcNumber> CalcRule<PlusOp, N> for Length {
    fn calc(lhs: Length, _op: PlusOp, rhs: N) -> Length {
        let left = LengthCalc::from_length(lhs);
        Length::Calc(LengthCalc {
            px: left.px + rhs.into_calc_number(),
            percent: left.percent,
            vw: left.vw,
            vh: left.vh,
        })
    }
}

impl CalcRule<SubtractOp, Length> for Length {
    fn calc(lhs: Length, _op: SubtractOp, rhs: Length) -> Length {
        let left = LengthCalc::from_length(lhs);
        let right = LengthCalc::from_length(rhs);
        Length::Calc(LengthCalc {
            px: left.px - right.px,
            percent: left.percent - right.percent,
            vw: left.vw - right.vw,
            vh: left.vh - right.vh,
        })
    }
}

impl<N: CalcNumber> CalcRule<SubtractOp, N> for Length {
    fn calc(lhs: Length, _op: SubtractOp, rhs: N) -> Length {
        let left = LengthCalc::from_length(lhs);
        Length::Calc(LengthCalc {
            px: left.px - rhs.into_calc_number(),
            percent: left.percent,
            vw: left.vw,
            vh: left.vh,
        })
    }
}

impl<N: CalcNumber> CalcRule<MultiplyOp, N> for Length {
    fn calc(lhs: Length, _op: MultiplyOp, rhs: N) -> Length {
        let left = LengthCalc::from_length(lhs);
        let factor = rhs.into_calc_number();
        Length::Calc(LengthCalc {
            px: left.px * factor,
            percent: left.percent * factor,
            vw: left.vw * factor,
            vh: left.vh * factor,
        })
    }
}

impl<N: CalcNumber> CalcRule<DivideOp, N> for Length {
    fn calc(lhs: Length, _op: DivideOp, rhs: N) -> Length {
        let divisor = rhs.into_calc_number();
        if divisor == 0.0 {
            return Length::Zero;
        }
        let left = LengthCalc::from_length(lhs);
        Length::Calc(LengthCalc {
            px: left.px / divisor,
            percent: left.percent / divisor,
            vw: left.vw / divisor,
            vh: left.vh / divisor,
        })
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

    pub const fn vw(value: f32) -> Length {
        Length::Vw(value)
    }

    pub const fn vh(value: f32) -> Length {
        Length::Vh(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFamily(Vec<String>);

impl FontFamily {
    pub fn new<I, S>(families: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let values = families
            .into_iter()
            .map(Into::into)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Self(values)
    }

    pub fn from_csv(raw: impl AsRef<str>) -> Self {
        Self::new(
            raw.as_ref()
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty()),
        )
    }

    pub fn as_slice(&self) -> &[String] {
        self.0.as_slice()
    }

    pub fn into_vec(self) -> Vec<String> {
        self.0
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

#[derive(Clone)]
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    pub color: StyleColor,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub inset: bool,
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self::new()
    }
}

impl BoxShadow {
    pub const fn new() -> Self {
        Self {
            color: StyleColor::Srgb(Color::rgb(0, 0, 0)),
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 0.0,
            inset: false,
        }
    }

    pub fn color<T: ColorLike>(mut self, color: T) -> Self {
        self.color = color.to_style_color();
        self
    }

    pub const fn offset(mut self, value: f32) -> Self {
        self.offset_x = value;
        self.offset_y = value;
        self
    }

    pub const fn offset_x(mut self, value: f32) -> Self {
        self.offset_x = value;
        self
    }

    pub const fn offset_y(mut self, value: f32) -> Self {
        self.offset_y = value;
        self
    }

    pub const fn blur(mut self, value: f32) -> Self {
        self.blur = if value < 0.0 { 0.0 } else { value };
        self
    }

    pub const fn spread(mut self, value: f32) -> Self {
        self.spread = value;
        self
    }

    pub const fn inset(mut self, value: bool) -> Self {
        self.inset = value;
        self
    }
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
    pub const THIN: Self = Self(100);
    pub const EXTRA_LIGHT: Self = Self(200);
    pub const LIGHT: Self = Self(300);
    pub const NORMAL: Self = Self(400);
    pub const MEDIUM: Self = Self(500);
    pub const SEMI_BOLD: Self = Self(600);
    pub const BOLD: Self = Self(700);
    pub const EXTRA_BOLD: Self = Self(800);
    pub const BLACK: Self = Self(900);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn thin() -> Self {
        Self::THIN
    }

    pub const fn extra_light() -> Self {
        Self::EXTRA_LIGHT
    }

    pub const fn light() -> Self {
        Self::LIGHT
    }

    pub const fn normal() -> Self {
        Self::NORMAL
    }

    pub const fn medium() -> Self {
        Self::MEDIUM
    }

    pub const fn semi_bold() -> Self {
        Self::SEMI_BOLD
    }

    pub const fn bold() -> Self {
        Self::BOLD
    }

    pub const fn extra_bold() -> Self {
        Self::EXTRA_BOLD
    }

    pub const fn black() -> Self {
        Self::BLACK
    }

    pub const fn value(self) -> u16 {
        self.0
    }
}

pub trait IntoFontWeight {
    fn into_font_weight(self) -> FontWeight;
}

impl IntoFontWeight for FontWeight {
    fn into_font_weight(self) -> FontWeight {
        self
    }
}

impl IntoFontWeight for u16 {
    fn into_font_weight(self) -> FontWeight {
        FontWeight::new(self)
    }
}

impl IntoFontWeight for u32 {
    fn into_font_weight(self) -> FontWeight {
        FontWeight::new(self as u16)
    }
}

impl IntoFontWeight for usize {
    fn into_font_weight(self) -> FontWeight {
        FontWeight::new(self as u16)
    }
}

impl IntoFontWeight for i32 {
    fn into_font_weight(self) -> FontWeight {
        FontWeight::new(self.max(0) as u16)
    }
}

impl IntoFontWeight for i64 {
    fn into_font_weight(self) -> FontWeight {
        FontWeight::new(self.max(0) as u16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontSize {
    Px(f32),
    Em(f32),
    Rem(f32),
    Percent(f32),
    Vw(f32),
    Vh(f32),
}

impl FontSize {
    pub const fn px(value: f32) -> Self {
        Self::Px(value)
    }

    pub const fn em(value: f32) -> Self {
        Self::Em(value)
    }

    pub const fn rem(value: f32) -> Self {
        Self::Rem(value)
    }

    pub const fn percent(value: f32) -> Self {
        Self::Percent(value)
    }

    pub const fn vw(value: f32) -> Self {
        Self::Vw(value)
    }

    pub const fn vh(value: f32) -> Self {
        Self::Vh(value)
    }

    pub fn resolve_px(
        self,
        parent_font_size_px: f32,
        root_font_size_px: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> f32 {
        let parent = parent_font_size_px.max(0.0);
        let root = root_font_size_px.max(0.0);
        let vw = viewport_width.max(0.0);
        let vh = viewport_height.max(0.0);
        match self {
            Self::Px(value) => value.max(0.0),
            Self::Em(value) => (parent * value).max(0.0),
            Self::Rem(value) => (root * value).max(0.0),
            Self::Percent(value) => (parent * value * 0.01).max(0.0),
            Self::Vw(value) => (vw * value * 0.01).max(0.0),
            Self::Vh(value) => (vh * value * 0.01).max(0.0),
        }
    }
}

pub trait IntoFontSize {
    fn into_font_size(self) -> FontSize;
}

impl IntoFontSize for FontSize {
    fn into_font_size(self) -> FontSize {
        self
    }
}

impl IntoFontSize for f32 {
    fn into_font_size(self) -> FontSize {
        FontSize::px(self)
    }
}

impl IntoFontSize for f64 {
    fn into_font_size(self) -> FontSize {
        FontSize::px(self as f32)
    }
}

impl IntoFontSize for i32 {
    fn into_font_size(self) -> FontSize {
        FontSize::px(self as f32)
    }
}

impl IntoFontSize for i64 {
    fn into_font_size(self) -> FontSize {
        FontSize::px(self as f32)
    }
}

impl IntoFontSize for u32 {
    fn into_font_size(self) -> FontSize {
        FontSize::px(self as f32)
    }
}

impl IntoFontSize for usize {
    fn into_font_size(self) -> FontSize {
        FontSize::px(self as f32)
    }
}

impl From<f32> for FontSize {
    fn from(value: f32) -> Self {
        FontSize::px(value)
    }
}

impl From<f64> for FontSize {
    fn from(value: f64) -> Self {
        FontSize::px(value as f32)
    }
}

impl From<i32> for FontSize {
    fn from(value: i32) -> Self {
        FontSize::px(value as f32)
    }
}

impl From<i64> for FontSize {
    fn from(value: i64) -> Self {
        FontSize::px(value as f32)
    }
}

impl From<u32> for FontSize {
    fn from(value: u32) -> Self {
        FontSize::px(value as f32)
    }
}

impl From<usize> for FontSize {
    fn from(value: usize) -> Self {
        FontSize::px(value as f32)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextWrap {
    Wrap,
    NoWrap,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flex {
    grow: f32,
    shrink: f32,
    basis: Option<Length>,
}

impl Flex {
    pub const fn new() -> Self {
        Self {
            grow: 0.0,
            shrink: 1.0,
            basis: None,
        }
    }

    pub const fn grow(mut self, value: f32) -> Self {
        self.grow = value;
        self
    }

    pub const fn shrink(mut self, value: f32) -> Self {
        self.shrink = value;
        self
    }

    pub fn basis<T>(mut self, value: T) -> Self
    where
        T: Into<Option<Length>>,
    {
        self.basis = value.into();
        self
    }

    pub const fn basis_auto(mut self) -> Self {
        self.basis = None;
        self
    }

    pub const fn grow_value(self) -> f32 {
        self.grow
    }

    pub const fn shrink_value(self) -> f32 {
        self.shrink
    }

    pub const fn basis_value(self) -> Option<Length> {
        self.basis
    }
}

pub const fn flex() -> Flex {
    Flex::new()
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedValue {
    Layout(Layout),
    CrossSize(CrossSize),
    Align(Align),
    Flex(Flex),
    ScrollDirection(ScrollDirection),
    Cursor(Cursor),
    Position(Position),
    Auto,
    Length(Length),
    FontSize(FontSize),
    FontFamily(FontFamily),
    FontWeight(FontWeight),
    LineHeight(LineHeight),
    TextWrap(TextWrap),
    Opacity(Opacity),
    BoxShadow(Vec<BoxShadow>),
    Transform(Transform),
    TransformOrigin(TransformOrigin),
    Transition(Transitions),
    Animator(Animator),
    Color(StyleColor),
}

impl ParsedValue {
    pub fn color_like<T: ColorLike>(color: T) -> Self {
        Self::Color(color.to_style_color())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub property: PropertyId,
    pub value: ParsedValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SelectionStyle {
    background: Option<StyleColor>,
}

impl SelectionStyle {
    pub const fn new() -> Self {
        Self { background: None }
    }

    pub fn background<T: ColorLike>(mut self, color: T) -> Self {
        self.background = Some(color.to_style_color());
        self
    }

    pub fn background_color(&self) -> Option<&StyleColor> {
        self.background.as_ref()
    }

    pub fn set_background<T: ColorLike>(&mut self, color: T) {
        self.background = Some(color.to_style_color());
    }

    pub fn merge(self, rhs: Self) -> Self {
        Self {
            background: rhs.background.or(self.background),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Style {
    declarations: Vec<Declaration>,
    index: HashMap<PropertyId, usize>,
    hover: Option<Box<Style>>,
    selection: Option<Box<SelectionStyle>>,
}

pub trait IntoStyleFieldValue<T> {
    fn into_style_field_value(self) -> T;
}

impl IntoStyleFieldValue<Length> for Length {
    fn into_style_field_value(self) -> Length {
        self
    }
}

impl IntoStyleFieldValue<Length> for f32 {
    fn into_style_field_value(self) -> Length {
        Length::px(self)
    }
}

impl IntoStyleFieldValue<Length> for f64 {
    fn into_style_field_value(self) -> Length {
        Length::px(self as f32)
    }
}

impl IntoStyleFieldValue<Length> for i32 {
    fn into_style_field_value(self) -> Length {
        Length::px(self as f32)
    }
}

impl IntoStyleFieldValue<Length> for i64 {
    fn into_style_field_value(self) -> Length {
        Length::px(self as f32)
    }
}

impl IntoStyleFieldValue<Length> for u32 {
    fn into_style_field_value(self) -> Length {
        Length::px(self as f32)
    }
}

impl IntoStyleFieldValue<Length> for usize {
    fn into_style_field_value(self) -> Length {
        Length::px(self as f32)
    }
}

impl IntoStyleFieldValue<FontSize> for FontSize {
    fn into_style_field_value(self) -> FontSize {
        self
    }
}

impl IntoStyleFieldValue<FontSize> for f32 {
    fn into_style_field_value(self) -> FontSize {
        self.into_font_size()
    }
}

impl IntoStyleFieldValue<FontSize> for f64 {
    fn into_style_field_value(self) -> FontSize {
        self.into_font_size()
    }
}

impl IntoStyleFieldValue<FontSize> for i32 {
    fn into_style_field_value(self) -> FontSize {
        self.into_font_size()
    }
}

impl IntoStyleFieldValue<FontSize> for i64 {
    fn into_style_field_value(self) -> FontSize {
        self.into_font_size()
    }
}

impl IntoStyleFieldValue<FontSize> for u32 {
    fn into_style_field_value(self) -> FontSize {
        self.into_font_size()
    }
}

impl IntoStyleFieldValue<FontSize> for usize {
    fn into_style_field_value(self) -> FontSize {
        self.into_font_size()
    }
}

impl IntoStyleFieldValue<StyleColor> for Color {
    fn into_style_field_value(self) -> StyleColor {
        self.into()
    }
}

impl IntoStyleFieldValue<StyleColor> for OklchColor {
    fn into_style_field_value(self) -> StyleColor {
        self.into()
    }
}

impl<'a> IntoStyleFieldValue<StyleColor> for crate::style::HexColor<'a> {
    fn into_style_field_value(self) -> StyleColor {
        self.to_style_color()
    }
}

impl IntoStyleFieldValue<StyleColor> for &str {
    fn into_style_field_value(self) -> StyleColor {
        StyleColor::Srgb(self.into_color())
    }
}

impl IntoStyleFieldValue<StyleColor> for String {
    fn into_style_field_value(self) -> StyleColor {
        StyleColor::Srgb(self.into_color())
    }
}

impl IntoStyleFieldValue<StyleColor> for Box<dyn ColorLike> {
    fn into_style_field_value(self) -> StyleColor {
        self.to_style_color()
    }
}

impl IntoStyleFieldValue<FontWeight> for FontWeight {
    fn into_style_field_value(self) -> FontWeight {
        self
    }
}

impl IntoStyleFieldValue<FontWeight> for u16 {
    fn into_style_field_value(self) -> FontWeight {
        self.into_font_weight()
    }
}

impl IntoStyleFieldValue<FontWeight> for u32 {
    fn into_style_field_value(self) -> FontWeight {
        self.into_font_weight()
    }
}

impl IntoStyleFieldValue<FontWeight> for usize {
    fn into_style_field_value(self) -> FontWeight {
        self.into_font_weight()
    }
}

impl IntoStyleFieldValue<FontWeight> for i32 {
    fn into_style_field_value(self) -> FontWeight {
        self.into_font_weight()
    }
}

impl IntoStyleFieldValue<FontWeight> for i64 {
    fn into_style_field_value(self) -> FontWeight {
        self.into_font_weight()
    }
}

impl IntoStyleFieldValue<Flex> for Flex {
    fn into_style_field_value(self) -> Flex {
        self
    }
}

impl IntoStyleFieldValue<TextWrap> for TextWrap {
    fn into_style_field_value(self) -> TextWrap {
        self
    }
}

pub fn insert_style_length<V>(style: &mut Style, property: PropertyId, value: V)
where
    V: IntoStyleFieldValue<Length>,
{
    style.insert(
        property,
        ParsedValue::Length(value.into_style_field_value()),
    );
}

pub fn insert_style_font_size<V>(style: &mut Style, property: PropertyId, value: V)
where
    V: IntoStyleFieldValue<FontSize>,
{
    style.insert(
        property,
        ParsedValue::FontSize(value.into_style_field_value()),
    );
}

pub fn style_color_value<V>(value: V) -> ParsedValue
where
    V: IntoStyleFieldValue<StyleColor>,
{
    ParsedValue::Color(value.into_style_field_value())
}

pub fn insert_style_font_weight<V>(style: &mut Style, property: PropertyId, value: V)
where
    V: IntoStyleFieldValue<FontWeight>,
{
    style.insert(
        property,
        ParsedValue::FontWeight(value.into_style_field_value()),
    );
}

pub fn insert_style_flex<V>(style: &mut Style, property: PropertyId, value: V)
where
    V: IntoStyleFieldValue<Flex>,
{
    style.insert(property, ParsedValue::Flex(value.into_style_field_value()));
}

pub fn insert_style_text_wrap<V>(style: &mut Style, property: PropertyId, value: V)
where
    V: IntoStyleFieldValue<TextWrap>,
{
    style.insert(
        property,
        ParsedValue::TextWrap(value.into_style_field_value()),
    );
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

    pub fn selection(&self) -> Option<&SelectionStyle> {
        self.selection.as_deref()
    }

    pub fn set_hover(&mut self, hover: Style) {
        self.hover = Some(Box::new(hover));
    }

    pub fn set_selection(&mut self, selection: SelectionStyle) {
        self.selection = Some(Box::new(selection));
    }

    pub fn with_hover(mut self, hover: Style) -> Self {
        self.set_hover(hover);
        self
    }

    pub fn with_selection(mut self, selection: SelectionStyle) -> Self {
        self.set_selection(selection);
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
        merged.selection = match (merged.selection.take(), rhs.selection) {
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

    pub fn set_cursor(&mut self, cursor: Cursor) {
        self.insert(PropertyId::Cursor, ParsedValue::Cursor(cursor));
    }

    pub fn with_cursor(mut self, cursor: Cursor) -> Self {
        self.set_cursor(cursor);
        self
    }

    pub fn set_text_wrap(&mut self, text_wrap: TextWrap) {
        self.insert(PropertyId::TextWrap, ParsedValue::TextWrap(text_wrap));
    }

    pub fn with_text_wrap(mut self, text_wrap: TextWrap) -> Self {
        self.set_text_wrap(text_wrap);
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
            ParsedValue::Color(Color::rgba(top_r, top_g, top_b, top_a).into()),
        );
        self.insert(
            PropertyId::BorderRightColor,
            ParsedValue::Color(Color::rgba(right_r, right_g, right_b, right_a).into()),
        );
        self.insert(
            PropertyId::BorderBottomColor,
            ParsedValue::Color(Color::rgba(bottom_r, bottom_g, bottom_b, bottom_a).into()),
        );
        self.insert(
            PropertyId::BorderLeftColor,
            ParsedValue::Color(Color::rgba(left_r, left_g, left_b, left_a).into()),
        );
    }

    pub fn with_border(mut self, border: Border) -> Self {
        self.set_border(border);
        self
    }

    pub fn set_box_shadow(&mut self, box_shadow: Vec<BoxShadow>) {
        self.insert(PropertyId::BoxShadow, ParsedValue::BoxShadow(box_shadow));
    }

    pub fn with_box_shadow(mut self, box_shadow: Vec<BoxShadow>) -> Self {
        self.set_box_shadow(box_shadow);
        self
    }

    pub fn set_transform(&mut self, transform: Transform) {
        self.insert(PropertyId::Transform, ParsedValue::Transform(transform));
    }

    pub fn with_transform(mut self, transform: Transform) -> Self {
        self.set_transform(transform);
        self
    }

    pub fn set_transform_origin(&mut self, transform_origin: TransformOrigin) {
        self.insert(
            PropertyId::TransformOrigin,
            ParsedValue::TransformOrigin(transform_origin),
        );
    }

    pub fn with_transform_origin(mut self, transform_origin: TransformOrigin) -> Self {
        self.set_transform_origin(transform_origin);
        self
    }

    pub fn set_flex(&mut self, flex: Flex) {
        self.insert(PropertyId::Flex, ParsedValue::Flex(flex));
    }

    pub fn with_flex(mut self, flex: Flex) -> Self {
        self.set_flex(flex);
        self
    }
}

impl Add for Style {
    type Output = Style;

    fn add(self, rhs: Self) -> Self::Output {
        self.merge(rhs)
    }
}
