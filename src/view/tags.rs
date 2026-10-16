#![allow(missing_docs)]

//! Built-in RSX host tags and their typed prop schemas.

use crate::style::style_props::{AllStyleSet, NoStylePropSchema, StylePropTrait, TextStyleSet};
use crate::style::{
    Animator, BackfaceVisibility, BorderRadius, BoxShadow, ColorLike, Cursor, Flex, FontFamily,
    FontSize, FontWeight, IntoAnimationStyle, Layout, Length, Opacity, Padding, Position,
    ScrollDirection, SelectionStyle, Style, TextAlign, TextWrap, Transform, TransformOrigin,
    Transitions, VerticalAlign,
};
use crate::ui::RsxNode;
use crate::ui::{
    BlurHandlerProp, ClickHandlerProp, DragEndHandlerProp, DragLeaveHandlerProp,
    DragOverHandlerProp, DragStartHandlerProp, DropHandlerProp, FocusHandlerProp, FromPropValue,
    IntoPropValue, KeyDownHandlerProp, KeyUpHandlerProp, PointerDownHandlerProp,
    PointerEnterHandlerProp, PointerLeaveHandlerProp, PointerMoveHandlerProp, PointerUpHandlerProp,
    RsxComponent, SharedPropValue, TextAreaFocusHandlerProp, TextAreaRenderHandlerProp,
    TextChangeHandlerProp, props,
};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// The built-in container host tag.
pub struct Element;
/// The built-in single-line or wrapped text host tag.
pub struct Text;
/// The built-in editable text host tag.
pub struct TextArea;
/// Internal host tag emitted by [`TextArea`]'s schema render to wrap
/// each user projection in the inline child list. Carries the source
/// `char_range` for hit-test / caret / IME routing.
///
/// Not intended for direct author use — `<TextArea>` is the only
/// emitter. Manually placing a `<TextAreaProjectionSegment>` outside a
/// TextArea inline-flow context is unsupported.
pub struct TextAreaProjectionSegment;
/// The built-in image host tag.
pub struct Image;
/// The built-in svg host tag.
pub struct Svg;

/// Controls how an image is fitted into its allocated box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFit {
    Contain,
    Cover,
    Fill,
}

/// Controls how image textures are sampled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageSampling {
    Linear,
    Nearest,
}

/// Declares the source backing an [`Image`] host tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageSource {
    Path(PathBuf),
    Rgba {
        width: u32,
        height: u32,
        pixels: Arc<[u8]>,
    },
}

/// Declares the source backing an [`Svg`] host tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SvgSource {
    Path(PathBuf),
    Content(String),
}

#[props]
pub struct ElementPropSchema {
    pub anchor: Option<String>,
    pub debug_type: Option<crate::view::debug::DebugType>,
    pub style: Option<ElementStylePropSchema>,
    pub on_pointer_down: Option<PointerDownHandlerProp>,
    pub on_pointer_up: Option<PointerUpHandlerProp>,
    pub on_pointer_move: Option<PointerMoveHandlerProp>,
    pub on_pointer_enter: Option<PointerEnterHandlerProp>,
    pub on_pointer_leave: Option<PointerLeaveHandlerProp>,
    pub on_click: Option<ClickHandlerProp>,
    pub on_drag_start: Option<DragStartHandlerProp>,
    pub on_drag_over: Option<DragOverHandlerProp>,
    pub on_drag_leave: Option<DragLeaveHandlerProp>,
    pub on_drop: Option<DropHandlerProp>,
    pub on_drag_end: Option<DragEndHandlerProp>,
    pub on_key_down: Option<KeyDownHandlerProp>,
    pub on_key_up: Option<KeyUpHandlerProp>,
    pub on_focus: Option<FocusHandlerProp>,
    pub on_blur: Option<BlurHandlerProp>,
}

/// The `style` prop of `<Element>`, `<Image>`, `<Svg>` and `<TextArea>`.
///
/// Every field is optional. An unset field keeps its default, or for an
/// inherited field the value of the nearest ancestor that sets it. The
/// inherited fields are [`color`](Self::color), [`cursor`](Self::cursor),
/// [`font`](Self::font), [`font_size`](Self::font_size),
/// [`font_weight`](Self::font_weight), [`line_height`](Self::line_height),
/// [`selection`](Self::selection), [`text_wrap`](Self::text_wrap) and
/// [`vertical_align`](Self::vertical_align). All other fields apply to this
/// element only.
///
/// `<TextArea>` takes this schema but applies only `color`, `cursor`, `font`,
/// `font_size`, `font_weight`, `line_height`, `selection` and
/// `vertical_align`.
///
/// Children that overflow an element's content box (inside its border and
/// padding) are clipped to it.
#[derive(Clone)]
#[props]
pub struct ElementStylePropSchema {
    /// How the element is placed; see [`Position`]. Default
    /// [`Position::static_`]: the element stays in its parent's flow.
    ///
    /// - Only [`Position::absolute`] changes placement. Under static the
    ///   element stays in flow and its insets are ignored.
    /// - There is no CSS `relative` or `fixed`. For `fixed`, anchor an
    ///   absolute element to the viewport and clip it there, as `Snackbar`
    ///   does: `Position::absolute().anchor(Anchor::Viewport)
    ///   .clip(ClipMode::Viewport)`. To nudge an element from where it sits
    ///   in flow, use a [`transform`](Self::transform) translation.
    /// - An absolute element leaves the flow. It takes no room in its
    ///   parent's layout and does not count toward the parent's auto size or
    ///   scroll extent.
    /// - It is placed against its anchor: by default the parent's content
    ///   box (inside padding and border), moved by the parent's scroll
    ///   offset. [`Position::anchor`] can pick a named element (one whose
    ///   [`ElementPropSchema::anchor`] is set and that is placed earlier in
    ///   the same layout pass), the parent's border box, an ancestor or the
    ///   viewport; see [`Anchor`](crate::style::Anchor). A name that is not
    ///   found falls back to the default.
    /// - The `top`, `right`, `bottom` and `left` insets are distances from
    ///   the anchor's edges. A left or right percent resolves against the
    ///   anchor's width, a top or bottom percent against its height. Setting
    ///   both `left` and `right` sets the width; both `top` and `bottom` set
    ///   the height.
    /// - [`Position::origin`] shifts the element by a fraction of its own
    ///   size. [`Position::collision`] flips the element to the other side
    ///   of the anchor, or shifts it inside the viewport or the parent, when
    ///   it would overflow. [`Position::clip`] picks what clips it; see
    ///   [`ClipMode`](crate::style::ClipMode).
    ///
    /// `TransitionProperty::Position`, `X` and `Y` animate a change in where
    /// the element is placed. Not inherited.
    pub position: Option<Position>,
    /// The element's width. Default auto.
    ///
    /// - A [`Length::vw`] or [`Length::vh`] value resolves against the
    ///   viewport. A percent resolves against the parent's content-box width
    ///   (inside its padding and border). [`Length::calc`] combines units.
    /// - Auto sizes the element to its content, unless the parent's layout
    ///   assigns a width, as `flex` grow and shrink or a cross-axis stretch
    ///   do.
    /// - [`min_width`](Self::min_width) and [`max_width`](Self::max_width)
    ///   clamp the result.
    /// - In a [`Layout::Inline`] parent, an `Inline` element with no width
    ///   and no height flows as a span across the parent's lines. Setting
    ///   either makes it one box on a line.
    ///
    /// Changes animate with `TransitionProperty::Width` or `All`. Not
    /// inherited.
    pub width: Option<Length>,
    /// The element's height. Default auto. Units work as for
    /// [`width`](Self::width), with a percent resolving against the parent's
    /// content-box height.
    ///
    /// A percent needs a parent whose height is known: set explicitly, or
    /// assigned by the parent's own layout. Otherwise, for example when the
    /// parent sizes to its content, the percent does not resolve and gives
    /// no height.
    ///
    /// [`min_height`](Self::min_height) and [`max_height`](Self::max_height)
    /// clamp the result. Changes animate with `TransitionProperty::Height` or
    /// `All`. Not inherited.
    pub height: Option<Length>,
    /// Lower bound for the width. Default 0. A percent resolves against the
    /// parent's content-box width. When it is larger than
    /// [`max_width`](Self::max_width), `min_width` wins.
    ///
    /// In a [`Layout::Flex`] row, setting it (even to zero) replaces the
    /// automatic minimum, so a child without a width or basis can shrink
    /// below its content size. Not inherited.
    pub min_width: Option<Length>,
    /// Upper bound for the width. Default none. A percent resolves against
    /// the parent's content-box width. It never goes below
    /// [`min_width`](Self::min_width). Not inherited.
    pub max_width: Option<Length>,
    /// Lower bound for the height. Default 0. A percent resolves against the
    /// parent's content-box height. When it is larger than
    /// [`max_height`](Self::max_height), `min_height` wins.
    ///
    /// In a [`Layout::Flex`] column, setting it (even to zero) replaces the
    /// automatic minimum, so a child without a height or basis can shrink
    /// below its content size. Not inherited.
    pub min_height: Option<Length>,
    /// Upper bound for the height. Default none. A percent resolves against
    /// the parent's content-box height. It never goes below
    /// [`min_height`](Self::min_height). Not inherited.
    pub max_height: Option<Length>,
    /// How the element arranges its children; see [`Layout`]. Default
    /// [`Layout::Inline`]. Not inherited.
    pub layout: Option<Layout>,
    /// How this element grows, shrinks and sets its starting size when its
    /// parent's layout is [`Layout::Flex`]; any other parent layout ignores
    /// it. See [`Flex`], built with [`flex()`](crate::style::flex).
    ///
    /// Default: grow 0, shrink 1, basis auto. Negative grow or shrink counts
    /// as 0. A percent basis resolves against the parent's content-box size
    /// along its main axis. [`Layout::Flex`] describes how an auto basis
    /// behaves. Not inherited.
    pub flex: Option<Flex>,
    /// Space between children. Default 0.
    ///
    /// - [`Layout::Flex`] and [`Layout::Flow`]: between neighbouring
    ///   children on a line and, in a wrapping `Flow`, between lines. A
    ///   percent resolves against the content-box size along the main axis.
    /// - [`Layout::Inline`]: horizontal space between neighbouring children
    ///   on a line. It adds no space between lines. A percent resolves
    ///   against the content-box width.
    ///
    /// `TransitionProperty::Gap` animates it. Not inherited.
    pub gap: Option<Length>,
    /// Lets the user scroll the content along the chosen axis, or both, when
    /// it is larger than the element. Default [`ScrollDirection::None`].
    ///
    /// Children are measured and placed with an available size of
    /// 1,000,000 px along the scroll axis, so the element's size does not
    /// limit them there. Percent sizes of children still resolve against the
    /// element's visible content box. An `Inline` element still breaks its
    /// lines at its own width. Not inherited.
    pub scroll_direction: Option<ScrollDirection>,
    /// The mouse cursor while the pointer is over this element. Default
    /// [`Cursor::Default`]. Inherited: descendants, including child text,
    /// use it unless they set their own.
    ///
    /// The cursor shown is the one of the element directly under the
    /// pointer, so over a child it is the child's value.
    pub cursor: Option<Cursor>,
    /// Text color. The element draws no text itself: descendant `Text`,
    /// `TextArea` and bare text use it unless they set their own. Inherited.
    /// When nothing sets a color, text is drawn in `#111111`.
    ///
    /// A `Color` [`transition`](Self::transition) or a color keyframe in
    /// [`animator`](Self::animator) recolors the inheriting `Text` every
    /// frame, as CSS inherits the animated value. A `TextArea` does not
    /// follow the animation: it takes the new color when the style changes.
    pub color: Option<Box<dyn ColorLike>>,
    /// Border widths and colors per side; see [`Border`](crate::style::Border).
    /// Default: no border.
    ///
    /// A border takes room: it adds to an auto size and shrinks the content
    /// box. A percent width resolves against the parent's content box: left
    /// and right against its width, top and bottom against its height. Each
    /// side is limited to half the element's shorter side.
    /// [`border_image`](Self::border_image), when set, replaces the colors.
    ///
    /// `TransitionProperty::BorderColor` animates the colors and
    /// `TransitionProperty::BorderWidth` the widths. Not inherited.
    pub border: Option<crate::style::Border>,
    /// Shorthand for [`background_color`](Self::background_color) (from a
    /// color) or [`background_image`](Self::background_image) (from a
    /// gradient). Each of those fields, when set, wins over the matching
    /// part of this one. Not inherited.
    pub background: Option<crate::style::Background>,
    /// Fill color of the area inside the border. Default transparent. Wins
    /// over a color from [`background`](Self::background).
    ///
    /// When a [`background_image`](Self::background_image) is set, the
    /// gradient is drawn instead and this color does not show. Animates with
    /// `TransitionProperty::BackgroundColor`. Not inherited.
    pub background_color: Option<Box<dyn ColorLike>>,
    /// Gradient fill of the area inside the border, laid out over the
    /// element's box. Wins over a gradient from
    /// [`background`](Self::background), and is drawn instead of
    /// [`background_color`](Self::background_color).
    ///
    /// An inline span (an `Inline` element without width or height in an
    /// `Inline` parent) does not draw it. Does not animate. Not inherited.
    pub background_image: Option<crate::style::Gradient>,
    /// Gradient that paints the border instead of the border colors. It
    /// shows only where [`border`](Self::border) gives a side a width.
    ///
    /// An inline span (an `Inline` element without width or height in an
    /// `Inline` parent) does not draw it. Does not animate. Not inherited.
    pub border_image: Option<crate::style::Gradient>,
    /// Font families for descendant text, in order of preference; see
    /// [`FontFamily`]. Default: none, which uses the default font.
    /// Inherited.
    pub font: Option<FontFamily>,
    /// Text size for descendant text. A number is pixels; see [`FontSize`]
    /// for the other units. `em` and percent are relative to the inherited
    /// size, `rem` to the root size (the viewport style's font size, 16 px
    /// unless set), and `vw` and `vh` to the viewport. Negative sizes count
    /// as 0.
    ///
    /// Default 16 px. Inherited. No transition property animates it.
    pub font_size: Option<FontSize>,
    /// Weight for descendant text, from 100 to 900 (400 normal, 700 bold).
    /// Values outside that range are clamped. Default 400. Inherited.
    pub font_weight: Option<FontWeight>,
    /// Whether text breaks into lines at the available width
    /// ([`TextWrap::Wrap`], the default) or stays on one line
    /// ([`TextWrap::NoWrap`]). On an element with [`Layout::Inline`], this
    /// element's own value decides whether its inline content wraps.
    /// Inherited.
    pub text_wrap: Option<TextWrap>,
    /// Line height as a multiplier of the font size, not a length: `1.5`
    /// makes each line 1.5 times the font size tall. Negative values count
    /// as 0.
    ///
    /// Default 1.2; a `Text` that gets no line height from anywhere uses
    /// 1.25. Inherited. Does not animate.
    pub line_height: Option<f64>,
    /// Where an item sits vertically in its line: on the baseline (the
    /// default) or at the top, middle or bottom of the line box; see
    /// [`VerticalAlign`].
    ///
    /// Only items on the lines of a [`Layout::Inline`] element read it, such
    /// as text and inline child elements. Other layouts ignore it.
    /// Inherited.
    pub vertical_align: Option<VerticalAlign>,
    /// Corner radii; see [`BorderRadius`]. Default 0. A percent resolves
    /// against the element's own shorter side.
    ///
    /// Rounds the background, the border and the shadows, and clips
    /// children to the rounded content box. Pointer hit testing still uses
    /// the full rectangle. Does not change layout. Animates with
    /// `TransitionProperty::BorderRadius`. Not inherited.
    pub border_radius: Option<BorderRadius>,
    /// Overrides applied while the pointer is over this element or one of
    /// its descendants; see [`HoverElementStylePropSchema`].
    pub hover: Option<HoverElementStylePropSchema>,
    /// Text selection style; see [`SelectionStylePropSchema`]. Only a
    /// `TextArea` draws a selection, using the highlight color from its own
    /// style or else from the nearest ancestor that sets one, and its
    /// default color when none does. Inherited.
    pub selection: Option<SelectionStylePropSchema>,
    /// Opacity from 0 (invisible) to 1 (opaque). Values outside are
    /// clamped. Default 1.
    ///
    /// It fades the element and all of its descendants as one group, like
    /// CSS `opacity`. Paint only: layout does not change, and a fully
    /// transparent element still receives pointer events. Animates with
    /// `TransitionProperty::Opacity`. Not inherited.
    pub opacity: Option<Opacity>,
    /// Shadows drawn around the element, or inside it for an `inset`
    /// shadow; see [`BoxShadow`]. Default none.
    ///
    /// Paint only: shadows do not change layout size or position. They
    /// follow [`border_radius`](Self::border_radius). Animates with
    /// `TransitionProperty::BoxShadow`. Not inherited.
    pub box_shadow: Option<Vec<BoxShadow>>,
    /// Space between the border and the content, per side; see [`Padding`].
    /// Default 0.
    ///
    /// Padding adds to an auto size and shrinks the content box, and
    /// children that overflow into it are clipped. A percent resolves
    /// against the parent's content box: left and right against its width,
    /// top and bottom against its height (CSS uses the width for all four).
    ///
    /// `TransitionProperty::Padding` animates it. Not inherited.
    pub padding: Option<Padding>,
    /// Translates, scales, rotates or projects the element and its
    /// descendants as drawn; see [`Transform`]. Entries combine in order, as
    /// in CSS. A percent translation resolves against the element's own
    /// size, and [`transform_origin`](Self::transform_origin) sets the
    /// pivot.
    ///
    /// Layout does not change: the parent and siblings still see the
    /// untransformed box. Pointer hit testing follows the transform.
    /// Animates with `TransitionProperty::Transform`. Not inherited.
    pub transform: Option<Transform>,
    /// The point that [`transform`](Self::transform) scales and rotates
    /// around; see [`TransformOrigin`]. Default the center (50%, 50%).
    /// Percents resolve against the element's own size. Animates with
    /// `TransitionProperty::TransformOrigin`. Not inherited.
    pub transform_origin: Option<TransformOrigin>,
    /// Whether the element stays visible when its own transform turns its
    /// back to the viewer; see [`BackfaceVisibility`]. Default visible. A
    /// hidden, back-facing element and its subtree are neither drawn nor
    /// hit-tested. Does not animate. Not inherited.
    pub backface_visibility: Option<BackfaceVisibility>,
    /// Animations for changes to this element's style and layout. Each
    /// [`Transition`](crate::style::Transition) names a
    /// [`TransitionProperty`](crate::style::TransitionProperty), a duration
    /// and a delay in milliseconds, and a timing curve (linear by default).
    ///
    /// - `Opacity`, `Color`, `BackgroundColor`, `BorderColor` (all four
    ///   sides), `BorderRadius`, `BoxShadow`, `Transform` and
    ///   `TransformOrigin` animate when the computed value changes, for
    ///   example when [`hover`](Self::hover) starts or after a re-render.
    /// - `Padding`, `BorderWidth` (both all four sides) and `Gap` animate
    ///   the same way. A change between a pixel and a percent value stays
    ///   correct while the percent base changes, as in CSS. The element
    ///   lays out again every frame of the animation, and so do its
    ///   children.
    /// - `Width` and `Height` animate the laid-out size, whatever changed
    ///   it: this element's style, its content or its parent's layout.
    /// - `Position`, `X` (or `PositionX`) and `Y` (or `PositionY`) animate a
    ///   change in where the element sits relative to its parent.
    /// - `All` covers the style values above plus `Width` and `Height`, but
    ///   not the position.
    ///
    /// An auto-sized element whose padding, border width or gap animates
    /// changes size every frame. With a `Width` or `Height` transition (or
    /// `All`) on it or on an auto-sized ancestor, that size transition
    /// restarts toward each new size, so the size trails the padding.
    ///
    /// The style an element is first built with does not animate. Not
    /// inherited.
    pub transition: Option<Transitions>,
    /// Keyframe animations that run on this element; see [`Animator`] and
    /// [`Animation`](crate::style::Animation). Keyframes animate opacity,
    /// color, background color, border colors, border radius, box shadow,
    /// transform, transform origin, and pixel widths and heights.
    ///
    /// The animator starts when it is first set or when its value changes.
    /// Setting an equal value again does not restart it, and removing it
    /// stops it. Durations default to 0, so give the animator or each
    /// animation a duration. Not inherited.
    pub animator: Option<Animator>,
}

/// Style overrides for [`ElementStylePropSchema::hover`], applied while the
/// element is hovered: while the pointer is over the element or over one of
/// its descendants.
///
/// While hovered, each field set here replaces the same field of the base
/// style; unset fields keep the base value. When the pointer leaves, the
/// base style returns. Both changes animate like any other style change,
/// using the [`transition`](Self::transition) list of the state being
/// entered: this one when hover starts, the base one when it ends.
///
/// - `<Element>`, `<Image>` and `<Svg>` apply hover. `<TextArea>` does not.
/// - The inherited fields (`color`, `cursor`, `font`, `font_size`,
///   `font_weight`, `line_height`, `text_wrap` and `vertical_align`) pass
///   down while hovered, as the base values do: a hover `color` recolors
///   child text that does not set its own, and a hover `cursor` also shows
///   over such a child. When hover ends, the descendants take the base
///   values again.
/// - A hover `background` color cannot hide a base `background_image`,
///   because a gradient is drawn instead of the color.
#[derive(Clone)]
#[props]
pub struct HoverElementStylePropSchema {
    /// While hovered, replaces [`ElementStylePropSchema::position`].
    pub position: Option<Position>,
    /// While hovered, replaces [`ElementStylePropSchema::width`].
    pub width: Option<Length>,
    /// While hovered, replaces [`ElementStylePropSchema::height`].
    pub height: Option<Length>,
    /// While hovered, replaces [`ElementStylePropSchema::min_width`].
    pub min_width: Option<Length>,
    /// While hovered, replaces [`ElementStylePropSchema::max_width`].
    pub max_width: Option<Length>,
    /// While hovered, replaces [`ElementStylePropSchema::min_height`].
    pub min_height: Option<Length>,
    /// While hovered, replaces [`ElementStylePropSchema::max_height`].
    pub max_height: Option<Length>,
    /// While hovered, replaces [`ElementStylePropSchema::layout`].
    pub layout: Option<Layout>,
    /// While hovered, replaces [`ElementStylePropSchema::flex`].
    pub flex: Option<Flex>,
    /// While hovered, replaces [`ElementStylePropSchema::gap`].
    pub gap: Option<Length>,
    /// While hovered, replaces [`ElementStylePropSchema::scroll_direction`].
    pub scroll_direction: Option<ScrollDirection>,
    /// While hovered, replaces [`ElementStylePropSchema::cursor`], also for
    /// descendants that do not set their own.
    pub cursor: Option<Cursor>,
    /// While hovered, replaces [`ElementStylePropSchema::color`], also for
    /// descendant text that does not set its own.
    pub color: Option<Box<dyn ColorLike>>,
    /// While hovered, replaces [`ElementStylePropSchema::border`], all four
    /// sides.
    pub border: Option<crate::style::Border>,
    /// While hovered, replaces the matching part of
    /// [`ElementStylePropSchema::background`]: a color replaces the
    /// background color and a gradient replaces the background image.
    pub background: Option<crate::style::Background>,
    /// While hovered, replaces [`ElementStylePropSchema::background_color`].
    pub background_color: Option<Box<dyn ColorLike>>,
    /// While hovered, replaces [`ElementStylePropSchema::background_image`].
    pub background_image: Option<crate::style::Gradient>,
    /// While hovered, replaces [`ElementStylePropSchema::border_image`].
    pub border_image: Option<crate::style::Gradient>,
    /// While hovered, replaces [`ElementStylePropSchema::font`], also for
    /// descendant text that does not set its own.
    pub font: Option<FontFamily>,
    /// While hovered, replaces [`ElementStylePropSchema::font_size`], also
    /// for descendant text that does not set its own.
    pub font_size: Option<FontSize>,
    /// While hovered, replaces [`ElementStylePropSchema::font_weight`], also
    /// for descendant text that does not set its own.
    pub font_weight: Option<FontWeight>,
    /// While hovered, replaces [`ElementStylePropSchema::text_wrap`], also
    /// for descendants that do not set their own.
    pub text_wrap: Option<TextWrap>,
    /// While hovered, replaces [`ElementStylePropSchema::line_height`], also
    /// for descendants that do not set their own.
    pub line_height: Option<f64>,
    /// While hovered, replaces [`ElementStylePropSchema::vertical_align`],
    /// also for descendants that do not set their own.
    pub vertical_align: Option<VerticalAlign>,
    /// While hovered, replaces [`ElementStylePropSchema::border_radius`], all
    /// four corners.
    pub border_radius: Option<BorderRadius>,
    /// Has no effect: hover overrides do not reach a `TextArea`, the only
    /// host that draws a selection.
    pub selection: Option<SelectionStylePropSchema>,
    /// While hovered, replaces [`ElementStylePropSchema::opacity`].
    pub opacity: Option<Opacity>,
    /// While hovered, replaces [`ElementStylePropSchema::box_shadow`], the
    /// whole list.
    pub box_shadow: Option<Vec<BoxShadow>>,
    /// While hovered, replaces [`ElementStylePropSchema::padding`], all four
    /// sides.
    pub padding: Option<Padding>,
    /// While hovered, replaces [`ElementStylePropSchema::transform`], the
    /// whole list.
    pub transform: Option<Transform>,
    /// While hovered, replaces [`ElementStylePropSchema::transform_origin`].
    pub transform_origin: Option<TransformOrigin>,
    /// While hovered, replaces
    /// [`ElementStylePropSchema::backface_visibility`].
    pub backface_visibility: Option<BackfaceVisibility>,
    /// While hovered, replaces [`ElementStylePropSchema::transition`], the
    /// whole list. This list animates the change into the hovered state.
    pub transition: Option<Transitions>,
    /// While hovered, replaces [`ElementStylePropSchema::animator`].
    pub animator: Option<Animator>,
}

/// The `style` prop of `<Text>`, which sets everything about the text.
///
/// `<Text>` applies every field. It takes no hover style and no event
/// handlers; wrap it in an `<Element>` for those.
///
/// An unset `color`, `cursor`, `font`, `font_size`, `font_weight`,
/// `line_height` or `text_wrap` takes the value of the nearest ancestor that
/// sets it, as does the vertical alignment, or else the default. Removing a
/// field from the style on a later update does the same.
///
/// A Text box is sized by its text: it wraps at the width its parent gives
/// it and is as tall as its lines. To give text a size, set it on a
/// wrapping `<Element>`.
///
/// In a parent with [`Layout::Inline`] (the default), the parent lays the
/// text out in its own lines. There `text_wrap` and
/// [`text_align`](Self::text_align) have no effect.
#[derive(Clone)]
#[props]
pub struct TextStylePropSchema {
    /// Text color. Default: the inherited color, or `#111111` when no
    /// ancestor sets one.
    pub color: Option<Box<dyn ColorLike>>,
    /// Font families in order of preference; see [`FontFamily`]. Default:
    /// inherited.
    pub font: Option<FontFamily>,
    /// Text size; see [`ElementStylePropSchema::font_size`] for the units.
    /// `em` and percent are relative to the inherited size. Default:
    /// inherited, 16 px at the root.
    pub font_size: Option<FontSize>,
    /// Weight from 100 to 900 (400 normal, 700 bold); values outside are
    /// clamped. Default: inherited, else 400.
    pub font_weight: Option<FontWeight>,
    /// Whether the text wraps at the box width ([`TextWrap::Wrap`]) or stays
    /// on one line ([`TextWrap::NoWrap`]). Default: inherited, else `Wrap`.
    ///
    /// No effect in a [`Layout::Inline`] parent: the `text_wrap` of the
    /// element that lays out those lines decides.
    pub text_wrap: Option<TextWrap>,
    /// Line height as a multiplier of the font size, not a length: `1.5`
    /// makes each line 1.5 times the font size tall. Negative values count
    /// as 0. Default: inherited, else 1.25.
    pub line_height: Option<f64>,
    /// Horizontal alignment of each line inside the text box: left (the
    /// default), center or right. Not inherited.
    ///
    /// No effect in a [`Layout::Inline`] parent (the default layout): its
    /// lines always start at the left. It applies when the parent lays the
    /// `Text` out as a box, as [`Layout::Flow`] does.
    pub text_align: Option<TextAlign>,
    /// The mouse cursor while the pointer is over this text. Default:
    /// inherited, else [`Cursor::Default`].
    pub cursor: Option<Cursor>,
    /// Opacity from 0 (invisible) to 1 (opaque). Values outside are
    /// clamped. Default 1. Paint only.
    pub opacity: Option<Opacity>,
    /// Translates, scales or rotates the drawn text without changing its
    /// layout; see [`ElementStylePropSchema::transform`]. A percent
    /// translation resolves against the text box size.
    pub transform: Option<Transform>,
    /// The point that [`transform`](Self::transform) scales and rotates
    /// around. Default the center of the text box. Percents resolve against
    /// the text box size.
    pub transform_origin: Option<TransformOrigin>,
    /// Whether the text stays visible when its transform turns its back to
    /// the viewer; see [`BackfaceVisibility`]. Default visible.
    pub backface_visibility: Option<BackfaceVisibility>,
    /// Animations for changes to this text's style, as
    /// [`ElementStylePropSchema::transition`] runs them for an element:
    /// `Color`, `Opacity`, `Transform`, `TransformOrigin` and `All` animate
    /// when this style changes their value. The other properties do not
    /// apply to text. The style the text is first built with does not
    /// animate, and a color it inherits follows the animating ancestor
    /// instead; see [`ElementStylePropSchema::color`].
    pub transition: Option<Transitions>,
}

/// Text selection style, used by [`ElementStylePropSchema::selection`] and
/// [`HoverElementStylePropSchema::selection`]. Only a `TextArea` draws a
/// selection.
#[derive(Clone)]
#[props]
pub struct SelectionStylePropSchema {
    /// Highlight color for selected text. Only a color is kept; a gradient
    /// is ignored.
    pub background: Option<crate::style::Background>,
}

/// Props of the `<Text>` host tag.
#[props]
pub struct TextPropSchema {
    /// The text's style; see [`TextStylePropSchema`].
    pub style: Option<TextStylePropSchema>,
}

#[props]
pub struct TextAreaProjectionSegmentPropSchema {
    pub char_range_start: Option<i64>,
    pub char_range_end: Option<i64>,
}

#[props]
pub struct TextAreaPropSchema {
    pub content: Option<String>,
    pub binding: Option<crate::ui::Binding<String>>,
    pub style: Option<ElementStylePropSchema>,
    pub on_focus: Option<TextAreaFocusHandlerProp>,
    /// Projects content ranges as RSX built from host tags only; see
    /// `TextAreaRenderString::range`.
    pub on_render: Option<TextAreaRenderHandlerProp>,
    pub on_blur: Option<BlurHandlerProp>,
    pub on_change: Option<TextChangeHandlerProp>,
    pub placeholder: Option<String>,
    pub multiline: Option<bool>,
    pub auto_wrap: Option<bool>,
    pub read_only: Option<bool>,
    /// Most characters the content may hold: longer content is cut, and
    /// typing stops at the limit. `0` allows none. Default: no limit.
    pub max_length: Option<i64>,
}

#[props]
pub struct ImagePropSchema {
    pub source: ImageSource,
    pub style: Option<ElementStylePropSchema>,
    pub fit: Option<ImageFit>,
    pub sampling: Option<ImageSampling>,
    pub loading: Option<RsxNode>,
    pub error: Option<RsxNode>,
}

#[props]
pub struct SvgPropSchema {
    pub source: SvgSource,
    pub style: Option<ElementStylePropSchema>,
    pub fit: Option<ImageFit>,
    pub sampling: Option<ImageSampling>,
    pub loading: Option<RsxNode>,
    pub error: Option<RsxNode>,
}

impl RsxComponent<ElementPropSchema> for Element {
    fn render(props: ElementPropSchema, children: Vec<RsxNode>) -> RsxNode {
        let mut node =
            RsxNode::tagged("Element", crate::ui::RsxTagDescriptor::for_tag::<Element>());
        if let Some(anchor) = props.anchor {
            node = node.with_prop("anchor", anchor);
        }
        if let Some(debug_type) = props.debug_type {
            node = node.with_prop(
                "debug_type",
                crate::ui::IntoPropValue::into_prop_value(debug_type),
            );
        }
        if let Some(style) = props.style {
            node = node.with_prop("style", style);
        }
        if let Some(handler) = props.on_pointer_down {
            node = node.with_prop("on_pointer_down", handler);
        }
        if let Some(handler) = props.on_pointer_up {
            node = node.with_prop("on_pointer_up", handler);
        }
        if let Some(handler) = props.on_pointer_move {
            node = node.with_prop("on_pointer_move", handler);
        }
        if let Some(handler) = props.on_pointer_enter {
            node = node.with_prop("on_pointer_enter", handler);
        }
        if let Some(handler) = props.on_pointer_leave {
            node = node.with_prop("on_pointer_leave", handler);
        }
        if let Some(handler) = props.on_click {
            node = node.with_prop("on_click", handler);
        }
        if let Some(handler) = props.on_drag_start {
            node = node.with_prop("on_drag_start", handler);
        }
        if let Some(handler) = props.on_drag_over {
            node = node.with_prop("on_drag_over", handler);
        }
        if let Some(handler) = props.on_drag_leave {
            node = node.with_prop("on_drag_leave", handler);
        }
        if let Some(handler) = props.on_drop {
            node = node.with_prop("on_drop", handler);
        }
        if let Some(handler) = props.on_drag_end {
            node = node.with_prop("on_drag_end", handler);
        }
        if let Some(handler) = props.on_key_down {
            node = node.with_prop("on_key_down", handler);
        }
        if let Some(handler) = props.on_key_up {
            node = node.with_prop("on_key_up", handler);
        }
        if let Some(handler) = props.on_focus {
            node = node.with_prop("on_focus", handler);
        }
        if let Some(handler) = props.on_blur {
            node = node.with_prop("on_blur", handler);
        }
        for child in children {
            node = node.with_child(child);
        }
        node
    }
}

impl RsxComponent<TextPropSchema> for Text {
    fn render(props: TextPropSchema, children: Vec<RsxNode>) -> RsxNode {
        let mut node = RsxNode::tagged("Text", crate::ui::RsxTagDescriptor::for_tag::<Text>());
        if let Some(style) = props.style {
            node = node.with_prop("style", style);
        }
        for child in children {
            node = node.with_child(child);
        }
        node
    }
}

impl RsxComponent<TextAreaProjectionSegmentPropSchema> for TextAreaProjectionSegment {
    fn render(props: TextAreaProjectionSegmentPropSchema, children: Vec<RsxNode>) -> RsxNode {
        let mut node = RsxNode::tagged(
            "TextAreaProjectionSegment",
            crate::ui::RsxTagDescriptor::for_tag::<TextAreaProjectionSegment>(),
        );
        if let Some(v) = props.char_range_start {
            node = node.with_prop("char_range_start", v);
        }
        if let Some(v) = props.char_range_end {
            node = node.with_prop("char_range_end", v);
        }
        for child in children {
            node = node.with_child(child);
        }
        node
    }
}

impl RsxComponent<TextAreaPropSchema> for TextArea {
    fn render(mut props: TextAreaPropSchema, children: Vec<RsxNode>) -> RsxNode {
        props.binding = props.binding.map(|binding| binding.snapshot());
        let mut resolved_content = props
            .binding
            .as_ref()
            .map(crate::ui::Binding::get)
            .or(props.content.clone());
        if resolved_content.is_none() {
            let mut content = String::new();
            for child in &children {
                append_text_area_content_child(&mut content, child);
            }
            resolved_content = Some(content);
        }

        let mut node = RsxNode::tagged(
            "TextArea",
            crate::ui::RsxTagDescriptor::for_tag::<TextArea>(),
        );
        if let Some(content) = resolved_content
            && !content.is_empty()
        {
            node = node.with_prop("content", content);
        }
        if let Some(binding) = props.binding {
            node = node.with_prop(
                "binding",
                crate::ui::IntoPropValue::into_prop_value(binding),
            );
        }
        if let Some(style) = props.style {
            node = node.with_prop("style", style);
        }
        if let Some(handler) = props.on_focus {
            node = node.with_prop("on_focus", handler);
        }
        if let Some(handler) = props.on_blur {
            node = node.with_prop("on_blur", handler);
        }
        if let Some(handler) = props.on_change {
            node = node.with_prop("on_change", handler);
        }
        if let Some(placeholder) = props.placeholder
            && !placeholder.is_empty()
        {
            node = node.with_prop("placeholder", placeholder);
        }
        if let Some(multiline) = props.multiline {
            node = node.with_prop("multiline", multiline);
        }
        if let Some(auto_wrap) = props.auto_wrap {
            node = node.with_prop("auto_wrap", auto_wrap);
        }
        if let Some(read_only) = props.read_only {
            node = node.with_prop("read_only", read_only);
        }
        if let Some(max_length) = props.max_length {
            node = node.with_prop("max_length", max_length);
        }
        if let Some(handler) = props.on_render {
            node = node.with_prop("on_render", handler);
        }
        node
    }
}

fn append_text_area_content_child(out: &mut String, node: &RsxNode) {
    match node {
        RsxNode::Text(content) => out.push_str(&content.content),
        RsxNode::Fragment(fragment) => {
            for child in &fragment.children {
                append_text_area_content_child(out, child);
            }
        }
        RsxNode::Element(_) => {}
        RsxNode::Component(_) => {}
        RsxNode::Provider(_) => {}
        RsxNode::ErrorBoundary(_) => {}
    }
}

impl RsxComponent<ImagePropSchema> for Image {
    fn render(props: ImagePropSchema, _children: Vec<RsxNode>) -> RsxNode {
        let mut node = RsxNode::tagged("Image", crate::ui::RsxTagDescriptor::for_tag::<Image>())
            .with_prop(
                "source",
                crate::ui::IntoPropValue::into_prop_value(props.source),
            );
        if let Some(style) = props.style {
            node = node.with_prop("style", style);
        }
        if let Some(fit) = props.fit {
            node = node.with_prop("fit", crate::ui::IntoPropValue::into_prop_value(fit));
        }
        if let Some(sampling) = props.sampling {
            node = node.with_prop(
                "sampling",
                crate::ui::IntoPropValue::into_prop_value(sampling),
            );
        }
        if let Some(loading) = props.loading {
            node = node.with_prop(
                "loading",
                crate::ui::IntoPropValue::into_prop_value(loading),
            );
        }
        if let Some(error) = props.error {
            node = node.with_prop("error", crate::ui::IntoPropValue::into_prop_value(error));
        }
        node
    }
}

impl RsxComponent<SvgPropSchema> for Svg {
    fn render(props: SvgPropSchema, _children: Vec<RsxNode>) -> RsxNode {
        let mut node = RsxNode::tagged("Svg", crate::ui::RsxTagDescriptor::for_tag::<Svg>())
            .with_prop(
                "source",
                crate::ui::IntoPropValue::into_prop_value(props.source),
            );
        if let Some(style) = props.style {
            node = node.with_prop("style", style);
        }
        if let Some(fit) = props.fit {
            node = node.with_prop("fit", crate::ui::IntoPropValue::into_prop_value(fit));
        }
        if let Some(sampling) = props.sampling {
            node = node.with_prop(
                "sampling",
                crate::ui::IntoPropValue::into_prop_value(sampling),
            );
        }
        if let Some(loading) = props.loading {
            node = node.with_prop(
                "loading",
                crate::ui::IntoPropValue::into_prop_value(loading),
            );
        }
        if let Some(error) = props.error {
            node = node.with_prop("error", crate::ui::IntoPropValue::into_prop_value(error));
        }
        node
    }
}

#[cfg(test)]
mod v2_poc_tests;

// ---------- v2 path: trivial RsxTag for all-Option schemas ----------
macro_rules! impl_rsx_tag_v2_trivial {
    ($tag:ty, $props:ty, $style_prop:ty, $accepts:expr) => {
        impl crate::ui::RsxTag for $tag {
            type Props = $props;
            type StrictProps = $props;
            const ACCEPTS_CHILDREN: bool = $accepts;
            const IS_HOST_TAG: bool = true;
            const HOST_BUILDER: Option<crate::ui::ErasedHostBuilder> =
                Some(crate::view::host_element::erased_host_builder::<$tag>);

            fn into_strict(props: Self::Props) -> Self::StrictProps {
                props
            }

            fn create_node(
                props: Self::StrictProps,
                children: Vec<RsxNode>,
                _key: Option<crate::ui::RsxKey>,
            ) -> RsxNode {
                <$tag as RsxComponent<$props>>::render(props, children)
            }
        }

        impl crate::ui::component::sealed::Sealed for $tag {}
        impl crate::ui::HostTag for $tag {}
        impl crate::ui::HostStyleTag for $tag {
            type StyleProp = $style_prop;
        }
    };
}

impl_rsx_tag_v2_trivial!(Element, ElementPropSchema, ElementStylePropSchema, true);
impl_rsx_tag_v2_trivial!(Text, TextPropSchema, TextStylePropSchema, true);
impl_rsx_tag_v2_trivial!(TextArea, TextAreaPropSchema, ElementStylePropSchema, true);
impl_rsx_tag_v2_trivial!(
    TextAreaProjectionSegment,
    TextAreaProjectionSegmentPropSchema,
    NoStylePropSchema,
    true
);

// ---------- HostBuilder impls for built-in host tags ----------
//
// Host-builder dispatch is now the conversion path for built-in
// descriptors. The descriptor bodies still delegate to
// `view::renderer_adapter`; moving those bodies into the relevant
// `base_component/*` modules remains pending cleanup.

macro_rules! impl_host_builder_via_adapter {
    ($tag:ty, $delegate:path) => {
        impl crate::view::host_element::HostBuilder for $tag {
            fn build_descriptor(
                node: &crate::ui::RsxElementNode,
                path: &[u64],
                ctx: &crate::view::host_element::BuildCtx,
            ) -> Result<crate::view::renderer_adapter::ElementDescriptor, String> {
                $delegate(node, path, ctx.global_path.clone(), &ctx.inherited)
            }
        }
    };
}

impl crate::view::host_element::HostBuilder for Element {
    fn build_descriptor(
        node: &crate::ui::RsxElementNode,
        path: &[u64],
        ctx: &crate::view::host_element::BuildCtx,
    ) -> Result<crate::view::renderer_adapter::ElementDescriptor, String> {
        crate::view::renderer_adapter::convert_container_element_desc(
            node,
            path,
            ctx.global_path.clone(),
            &ctx.inherited,
        )
    }
}

impl crate::view::host_element::HostBuilder for Text {
    fn build_descriptor(
        node: &crate::ui::RsxElementNode,
        path: &[u64],
        ctx: &crate::view::host_element::BuildCtx,
    ) -> Result<crate::view::renderer_adapter::ElementDescriptor, String> {
        crate::view::renderer_adapter::convert_text_element(
            node,
            path,
            ctx.global_path.clone(),
            &ctx.inherited,
        )
        .map(crate::view::renderer_adapter::ElementDescriptor::leaf)
    }
}

impl_host_builder_via_adapter!(
    TextArea,
    crate::view::renderer_adapter::convert_text_area_element_desc
);
impl_host_builder_via_adapter!(
    TextAreaProjectionSegment,
    crate::view::renderer_adapter::convert_text_area_projection_segment_element_desc
);
impl_host_builder_via_adapter!(
    Image,
    crate::view::renderer_adapter::convert_image_element_desc
);
impl_host_builder_via_adapter!(Svg, crate::view::renderer_adapter::convert_svg_element_desc);

// ---------- v2 path: Init struct pattern for tags with required props ----------
#[doc(hidden)]
pub struct __ImagePropsInit {
    pub source: Option<ImageSource>,
    pub style: Option<ElementStylePropSchema>,
    pub fit: Option<ImageFit>,
    pub sampling: Option<ImageSampling>,
    pub loading: Option<RsxNode>,
    pub error: Option<RsxNode>,
}

impl Default for __ImagePropsInit {
    fn default() -> Self {
        Self {
            source: None,
            style: None,
            fit: None,
            sampling: None,
            loading: None,
            error: None,
        }
    }
}

impl From<__ImagePropsInit> for ImagePropSchema {
    fn from(i: __ImagePropsInit) -> Self {
        Self {
            source: i.source.expect("missing required prop `source` on <Image>"),
            style: i.style,
            fit: i.fit,
            sampling: i.sampling,
            loading: i.loading,
            error: i.error,
        }
    }
}

impl crate::ui::RsxTag for Image {
    type Props = __ImagePropsInit;
    type StrictProps = ImagePropSchema;
    const ACCEPTS_CHILDREN: bool = false;
    const IS_HOST_TAG: bool = true;
    const HOST_BUILDER: Option<crate::ui::ErasedHostBuilder> =
        Some(crate::view::host_element::erased_host_builder::<Image>);

    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<RsxNode>,
        _key: Option<crate::ui::RsxKey>,
    ) -> RsxNode {
        <Image as RsxComponent<ImagePropSchema>>::render(props, children)
    }
}

impl crate::ui::component::sealed::Sealed for Image {}
impl crate::ui::HostTag for Image {}
impl crate::ui::HostStyleTag for Image {
    type StyleProp = ElementStylePropSchema;
}

#[doc(hidden)]
pub struct __SvgPropsInit {
    pub source: Option<SvgSource>,
    pub style: Option<ElementStylePropSchema>,
    pub fit: Option<ImageFit>,
    pub sampling: Option<ImageSampling>,
    pub loading: Option<RsxNode>,
    pub error: Option<RsxNode>,
}

impl Default for __SvgPropsInit {
    fn default() -> Self {
        Self {
            source: None,
            style: None,
            fit: None,
            sampling: None,
            loading: None,
            error: None,
        }
    }
}

impl From<__SvgPropsInit> for SvgPropSchema {
    fn from(i: __SvgPropsInit) -> Self {
        Self {
            source: i.source.expect("missing required prop `source` on <Svg>"),
            style: i.style,
            fit: i.fit,
            sampling: i.sampling,
            loading: i.loading,
            error: i.error,
        }
    }
}

impl crate::ui::RsxTag for Svg {
    type Props = __SvgPropsInit;
    type StrictProps = SvgPropSchema;
    const ACCEPTS_CHILDREN: bool = false;
    const IS_HOST_TAG: bool = true;
    const HOST_BUILDER: Option<crate::ui::ErasedHostBuilder> =
        Some(crate::view::host_element::erased_host_builder::<Svg>);

    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<RsxNode>,
        _key: Option<crate::ui::RsxKey>,
    ) -> RsxNode {
        <Svg as RsxComponent<SvgPropSchema>>::render(props, children)
    }
}

impl crate::ui::component::sealed::Sealed for Svg {}
impl crate::ui::HostTag for Svg {}
impl crate::ui::HostStyleTag for Svg {
    type StyleProp = ElementStylePropSchema;
}

fn into_shared_prop_value<T: 'static>(value: T) -> crate::ui::PropValue {
    crate::ui::PropValue::Shared(SharedPropValue::new(Rc::new(value)))
}

/// Host `style` payload. Hosts consume only the lowered [`Style`], which is
/// computed once and shared by reconcile comparison and host application.
struct LoweredStyleProp<T> {
    schema: T,
    lowered: std::cell::OnceCell<Style>,
}

impl<T: StylePropTrait> LoweredStyleProp<T> {
    fn lowered(&self) -> &Style {
        self.lowered.get_or_init(|| self.schema.to_style())
    }
}

/// Host `style` props are rebuilt on every render; equal lowerings reconcile
/// as an unchanged prop.
fn into_lowered_style_prop_value<T: StylePropTrait + 'static>(value: T) -> crate::ui::PropValue {
    crate::ui::PropValue::Shared(SharedPropValue::with_value_eq(
        Rc::new(LoweredStyleProp {
            schema: value,
            lowered: std::cell::OnceCell::new(),
        }),
        lowered_style_eq::<T>,
    ))
}

fn lowered_style_eq<T: StylePropTrait + 'static>(
    a: &dyn std::any::Any,
    b: &dyn std::any::Any,
) -> bool {
    match (
        a.downcast_ref::<LoweredStyleProp<T>>(),
        b.downcast_ref::<LoweredStyleProp<T>>(),
    ) {
        (Some(a), Some(b)) => a.lowered() == b.lowered(),
        _ => false,
    }
}

fn from_lowered_style_prop_value<T: Clone + 'static>(
    value: crate::ui::PropValue,
    expected: &str,
) -> Result<T, String> {
    match value {
        crate::ui::PropValue::Shared(shared) => shared
            .into_inner()
            .downcast::<LoweredStyleProp<T>>()
            .map(|rc| match Rc::try_unwrap(rc) {
                Ok(prop) => prop.schema,
                Err(rc) => rc.schema.clone(),
            })
            .map_err(|_| format!("expected {expected} value")),
        _ => Err(format!("expected {expected} value")),
    }
}

/// The lowered style of a host `style` prop built from schema `T`.
pub(crate) fn lowered_host_style<T: StylePropTrait + 'static>(
    value: &crate::ui::PropValue,
) -> Option<Style> {
    match value {
        crate::ui::PropValue::Shared(shared) => shared
            .value()
            .downcast_ref::<LoweredStyleProp<T>>()
            .map(|prop| prop.lowered().clone()),
        _ => None,
    }
}

fn from_shared_prop_value<T: Clone + 'static>(
    value: crate::ui::PropValue,
    expected: &str,
) -> Result<T, String> {
    match value {
        crate::ui::PropValue::Shared(shared) => shared
            .into_inner()
            .downcast::<T>()
            .map(|rc| Rc::try_unwrap(rc).unwrap_or_else(|rc| (*rc).clone()))
            .map_err(|_| format!("expected {expected} value")),
        _ => Err(format!("expected {expected} value")),
    }
}

macro_rules! impl_shared_style_prop_value {
    ($ty:ty, $label:literal, $into:ident, $from:ident) => {
        impl IntoPropValue for $ty {
            fn into_prop_value(self) -> crate::ui::PropValue {
                $into(self)
            }
        }

        impl From<$ty> for crate::ui::PropValue {
            fn from(value: $ty) -> Self {
                $into(value)
            }
        }

        impl FromPropValue for $ty {
            fn from_prop_value(value: crate::ui::PropValue) -> Result<Self, String> {
                $from(value, $label)
            }
        }
    };
}

impl_shared_style_prop_value!(
    ElementStylePropSchema,
    "ElementStylePropSchema",
    into_lowered_style_prop_value,
    from_lowered_style_prop_value
);
impl_shared_style_prop_value!(
    HoverElementStylePropSchema,
    "HoverElementStylePropSchema",
    into_shared_prop_value,
    from_shared_prop_value
);
impl_shared_style_prop_value!(
    TextStylePropSchema,
    "TextStylePropSchema",
    into_lowered_style_prop_value,
    from_lowered_style_prop_value
);
impl_shared_style_prop_value!(
    SelectionStylePropSchema,
    "SelectionStylePropSchema",
    into_shared_prop_value,
    from_shared_prop_value
);

fn apply_box_color(
    style: &mut Style,
    property: crate::style::PropertyId,
    color: &Option<Box<dyn ColorLike>>,
) {
    if let Some(color) = color {
        style.insert(property, crate::style::style_color_value(color.clone()));
    }
}

fn apply_background(style: &mut Style, background: Option<&crate::style::Background>) {
    let Some(background) = background else {
        return;
    };
    match background {
        crate::style::Background::Color(color) => {
            style.insert(
                crate::style::PropertyId::BackgroundColor,
                crate::style::style_color_value(color.clone()),
            );
        }
        crate::style::Background::Gradient(gradient) => {
            style.insert(
                crate::style::PropertyId::BackgroundImage,
                crate::style::ParsedValue::Gradient(gradient.clone()),
            );
        }
    }
}

fn apply_selection(selection: &Option<SelectionStylePropSchema>) -> Option<SelectionStyle> {
    let selection = selection.as_ref()?;
    let mut output = SelectionStyle::new();
    if let Some(background) = &selection.background {
        match background {
            crate::style::Background::Color(color) => output.set_background(color.clone()),
            crate::style::Background::Gradient(_) => {
                // Selection highlight only supports solid colors; gradients are ignored.
            }
        }
    }
    Some(output)
}

struct SharedStyleFields<'a> {
    width: Option<Length>,
    height: Option<Length>,
    color: &'a Option<Box<dyn ColorLike>>,
    font: &'a Option<FontFamily>,
    font_size: Option<FontSize>,
    font_weight: Option<FontWeight>,
    text_wrap: Option<TextWrap>,
    cursor: Option<Cursor>,
    opacity: Option<Opacity>,
    transition: &'a Option<Transitions>,
}

trait SharedStylePropFields {
    fn shared_style_fields(&self) -> SharedStyleFields<'_>;
}

impl SharedStylePropFields for ElementStylePropSchema {
    fn shared_style_fields(&self) -> SharedStyleFields<'_> {
        SharedStyleFields {
            width: self.width,
            height: self.height,
            color: &self.color,
            font: &self.font,
            font_size: self.font_size,
            font_weight: self.font_weight,
            text_wrap: self.text_wrap,
            cursor: self.cursor,
            opacity: self.opacity,
            transition: &self.transition,
        }
    }
}

impl SharedStylePropFields for HoverElementStylePropSchema {
    fn shared_style_fields(&self) -> SharedStyleFields<'_> {
        SharedStyleFields {
            width: self.width,
            height: self.height,
            color: &self.color,
            font: &self.font,
            font_size: self.font_size,
            font_weight: self.font_weight,
            text_wrap: self.text_wrap,
            cursor: self.cursor,
            opacity: self.opacity,
            transition: &self.transition,
        }
    }
}

impl SharedStylePropFields for TextStylePropSchema {
    fn shared_style_fields(&self) -> SharedStyleFields<'_> {
        SharedStyleFields {
            width: None,
            height: None,
            color: &self.color,
            font: &self.font,
            font_size: self.font_size,
            font_weight: self.font_weight,
            text_wrap: self.text_wrap,
            cursor: self.cursor,
            opacity: self.opacity,
            transition: &self.transition,
        }
    }
}

struct ElementStyleFields<'a> {
    shared: SharedStyleFields<'a>,
    position: &'a Option<Position>,
    min_width: Option<Length>,
    max_width: Option<Length>,
    min_height: Option<Length>,
    max_height: Option<Length>,
    layout: Option<Layout>,
    flex: Option<Flex>,
    gap: Option<Length>,
    scroll_direction: Option<ScrollDirection>,
    border: &'a Option<crate::style::Border>,
    background: &'a Option<crate::style::Background>,
    background_color: &'a Option<Box<dyn ColorLike>>,
    background_image: &'a Option<crate::style::Gradient>,
    border_image: &'a Option<crate::style::Gradient>,
    line_height: Option<f64>,
    vertical_align: Option<VerticalAlign>,
    border_radius: Option<BorderRadius>,
    selection: &'a Option<SelectionStylePropSchema>,
    box_shadow: &'a Option<Vec<BoxShadow>>,
    padding: Option<Padding>,
    transform: &'a Option<Transform>,
    transform_origin: Option<TransformOrigin>,
    backface_visibility: Option<BackfaceVisibility>,
    animator: &'a Option<Animator>,
}

trait ElementStylePropFields: SharedStylePropFields {
    fn element_style_fields(&self) -> ElementStyleFields<'_>;
}

impl ElementStylePropFields for ElementStylePropSchema {
    fn element_style_fields(&self) -> ElementStyleFields<'_> {
        ElementStyleFields {
            shared: self.shared_style_fields(),
            position: &self.position,
            min_width: self.min_width,
            max_width: self.max_width,
            min_height: self.min_height,
            max_height: self.max_height,
            layout: self.layout,
            flex: self.flex,
            gap: self.gap,
            scroll_direction: self.scroll_direction,
            border: &self.border,
            background: &self.background,
            background_color: &self.background_color,
            background_image: &self.background_image,
            border_image: &self.border_image,
            line_height: self.line_height,
            vertical_align: self.vertical_align,
            border_radius: self.border_radius,
            selection: &self.selection,
            box_shadow: &self.box_shadow,
            padding: self.padding,
            transform: &self.transform,
            transform_origin: self.transform_origin,
            backface_visibility: self.backface_visibility,
            animator: &self.animator,
        }
    }
}

impl ElementStylePropFields for HoverElementStylePropSchema {
    fn element_style_fields(&self) -> ElementStyleFields<'_> {
        ElementStyleFields {
            shared: self.shared_style_fields(),
            position: &self.position,
            min_width: self.min_width,
            max_width: self.max_width,
            min_height: self.min_height,
            max_height: self.max_height,
            layout: self.layout,
            flex: self.flex,
            gap: self.gap,
            scroll_direction: self.scroll_direction,
            border: &self.border,
            background: &self.background,
            background_color: &self.background_color,
            background_image: &self.background_image,
            border_image: &self.border_image,
            line_height: self.line_height,
            vertical_align: self.vertical_align,
            border_radius: self.border_radius,
            selection: &self.selection,
            box_shadow: &self.box_shadow,
            padding: self.padding,
            transform: &self.transform,
            transform_origin: self.transform_origin,
            backface_visibility: self.backface_visibility,
            animator: &self.animator,
        }
    }
}

fn apply_shared_size_style_fields(style: &mut Style, fields: &SharedStyleFields<'_>) {
    if let Some(width) = fields.width {
        crate::style::insert_style_length(style, crate::style::PropertyId::Width, width);
    }
    if let Some(height) = fields.height {
        crate::style::insert_style_length(style, crate::style::PropertyId::Height, height);
    }
}

fn apply_shared_color_style_field(style: &mut Style, fields: &SharedStyleFields<'_>) {
    apply_box_color(style, crate::style::PropertyId::Color, fields.color);
}

fn apply_shared_typography_style_fields(style: &mut Style, fields: &SharedStyleFields<'_>) {
    if let Some(font) = fields.font {
        style.insert(
            crate::style::PropertyId::FontFamily,
            crate::style::ParsedValue::FontFamily(font.clone()),
        );
    }
    if let Some(font_size) = fields.font_size {
        crate::style::insert_style_font_size(style, crate::style::PropertyId::FontSize, font_size);
    }
    if let Some(font_weight) = fields.font_weight {
        crate::style::insert_style_font_weight(
            style,
            crate::style::PropertyId::FontWeight,
            font_weight,
        );
    }
    if let Some(text_wrap) = fields.text_wrap {
        crate::style::insert_style_text_wrap(style, crate::style::PropertyId::TextWrap, text_wrap);
    }
}

fn apply_shared_cursor_style_field(style: &mut Style, fields: &SharedStyleFields<'_>) {
    if let Some(cursor) = fields.cursor {
        style.insert(
            crate::style::PropertyId::Cursor,
            crate::style::ParsedValue::Cursor(cursor),
        );
    }
}

fn apply_shared_opacity_style_field(style: &mut Style, fields: &SharedStyleFields<'_>) {
    if let Some(opacity) = fields.opacity {
        style.insert(
            crate::style::PropertyId::Opacity,
            crate::style::ParsedValue::Opacity(opacity),
        );
    }
}

fn apply_shared_transition_style_field(style: &mut Style, fields: &SharedStyleFields<'_>) {
    if let Some(transition) = fields.transition {
        style.insert(
            crate::style::PropertyId::Transition,
            crate::style::ParsedValue::Transition(transition.clone()),
        );
    }
}

fn apply_backface_visibility(style: &mut Style, backface_visibility: Option<BackfaceVisibility>) {
    if let Some(backface_visibility) = backface_visibility {
        style.insert(
            crate::style::PropertyId::BackfaceVisibility,
            crate::style::ParsedValue::BackfaceVisibility(backface_visibility),
        );
    }
}

fn apply_shared_style_fields(style: &mut Style, fields: &SharedStyleFields<'_>) {
    apply_shared_size_style_fields(style, fields);
    apply_shared_color_style_field(style, fields);
    apply_shared_typography_style_fields(style, fields);
    apply_shared_cursor_style_field(style, fields);
    apply_shared_opacity_style_field(style, fields);
    apply_shared_transition_style_field(style, fields);
}

fn apply_element_style_fields<T>(style: &mut Style, schema: &T)
where
    T: ElementStylePropFields,
{
    let fields = schema.element_style_fields();
    let shared = &fields.shared;

    if let Some(position) = fields.position.clone() {
        style.insert(
            crate::style::PropertyId::Position,
            crate::style::ParsedValue::Position(position),
        );
    }
    apply_shared_size_style_fields(style, &shared);
    if let Some(min_width) = fields.min_width {
        crate::style::insert_style_length(style, crate::style::PropertyId::MinWidth, min_width);
    }
    if let Some(max_width) = fields.max_width {
        crate::style::insert_style_length(style, crate::style::PropertyId::MaxWidth, max_width);
    }
    if let Some(min_height) = fields.min_height {
        crate::style::insert_style_length(style, crate::style::PropertyId::MinHeight, min_height);
    }
    if let Some(max_height) = fields.max_height {
        crate::style::insert_style_length(style, crate::style::PropertyId::MaxHeight, max_height);
    }
    if let Some(layout) = fields.layout {
        style.insert(
            crate::style::PropertyId::Layout,
            crate::style::ParsedValue::Layout(layout),
        );
    }
    if let Some(flex) = fields.flex {
        crate::style::insert_style_flex(style, crate::style::PropertyId::Flex, flex);
    }
    if let Some(gap) = fields.gap {
        crate::style::insert_style_length(style, crate::style::PropertyId::Gap, gap);
    }
    if let Some(scroll_direction) = fields.scroll_direction {
        style.insert(
            crate::style::PropertyId::ScrollDirection,
            crate::style::ParsedValue::ScrollDirection(scroll_direction),
        );
    }
    apply_shared_cursor_style_field(style, &shared);
    apply_shared_color_style_field(style, &shared);
    apply_background(style, fields.background.as_ref());
    apply_box_color(
        style,
        crate::style::PropertyId::BackgroundColor,
        fields.background_color,
    );
    if let Some(gradient) = fields.background_image {
        style.insert(
            crate::style::PropertyId::BackgroundImage,
            crate::style::ParsedValue::Gradient(gradient.clone()),
        );
    }
    if let Some(gradient) = fields.border_image {
        style.insert(
            crate::style::PropertyId::BorderImage,
            crate::style::ParsedValue::Gradient(gradient.clone()),
        );
    }
    if let Some(border) = fields.border {
        style.set_border(border.clone());
    }
    apply_shared_typography_style_fields(style, &shared);
    if let Some(line_height) = fields.line_height {
        style.set_line_height(line_height as f32);
    }
    if let Some(vertical_align) = fields.vertical_align {
        style.set_vertical_align(vertical_align);
    }
    if let Some(border_radius) = fields.border_radius {
        style.set_border_radius(border_radius);
    }
    if let Some(opacity) = shared.opacity {
        style.insert(
            crate::style::PropertyId::Opacity,
            crate::style::ParsedValue::Opacity(opacity),
        );
    }
    if let Some(box_shadow) = fields.box_shadow {
        style.insert(
            crate::style::PropertyId::BoxShadow,
            crate::style::ParsedValue::BoxShadow(box_shadow.clone()),
        );
    }
    if let Some(padding) = fields.padding {
        style.set_padding(padding);
    }
    if let Some(transform) = fields.transform {
        style.set_transform(transform.clone());
    }
    if let Some(transform_origin) = fields.transform_origin {
        style.set_transform_origin(transform_origin);
    }
    apply_backface_visibility(style, fields.backface_visibility);
    apply_shared_transition_style_field(style, &shared);
    if let Some(animator) = fields.animator {
        style.insert(
            crate::style::PropertyId::Animator,
            crate::style::ParsedValue::Animator(animator.clone()),
        );
    }
    if let Some(selection) = apply_selection(fields.selection) {
        style.set_selection(selection);
    }
}

impl IntoAnimationStyle for ElementStylePropSchema {
    fn into_animation_style(self) -> Style {
        self.to_style()
    }
}

impl HoverElementStylePropSchema {
    pub fn to_style(&self) -> Style {
        let mut style = Style::new();
        apply_element_style_fields(&mut style, self);
        style
    }
}

impl ElementStylePropSchema {
    pub fn to_style(&self) -> Style {
        let mut style = Style::new();
        apply_element_style_fields(&mut style, self);
        if let Some(hover) = &self.hover {
            style.set_hover(hover.to_style());
        }
        style
    }
}

impl StylePropTrait for ElementStylePropSchema {
    type Accepted = AllStyleSet;

    fn to_style(&self) -> Style {
        ElementStylePropSchema::to_style(self)
    }
}

impl TextStylePropSchema {
    pub fn to_style(&self) -> Style {
        let mut style = Style::new();
        apply_shared_style_fields(&mut style, &self.shared_style_fields());
        if let Some(transform) = &self.transform {
            style.set_transform(transform.clone());
        }
        if let Some(transform_origin) = self.transform_origin {
            style.set_transform_origin(transform_origin);
        }
        apply_backface_visibility(&mut style, self.backface_visibility);
        if let Some(line_height) = self.line_height {
            style.set_line_height(line_height as f32);
        }
        if let Some(text_align) = self.text_align {
            style.set_text_align(text_align);
        }
        style
    }
}

impl StylePropTrait for TextStylePropSchema {
    type Accepted = TextStyleSet;

    fn to_style(&self) -> Style {
        TextStylePropSchema::to_style(self)
    }
}

#[cfg(test)]
mod style_lowering_tests;

#[cfg(test)]
mod style_prop_identity_tests;

impl crate::ui::IntoPropValue for ImageFit {
    fn into_prop_value(self) -> crate::ui::PropValue {
        crate::ui::PropValue::Shared(crate::ui::SharedPropValue::new(Rc::new(self)))
    }
}

impl crate::ui::IntoPropValue for ImageSampling {
    fn into_prop_value(self) -> crate::ui::PropValue {
        crate::ui::PropValue::Shared(crate::ui::SharedPropValue::new(Rc::new(self)))
    }
}

impl crate::ui::FromPropValue for ImageSampling {
    fn from_prop_value(value: crate::ui::PropValue) -> Result<Self, String> {
        match value {
            crate::ui::PropValue::Shared(shared) => shared
                .value()
                .downcast::<ImageSampling>()
                .map(|value| *value)
                .map_err(|_| "expected ImageSampling value".to_string()),
            _ => Err("expected ImageSampling value".to_string()),
        }
    }
}

impl crate::ui::FromPropValue for ImageFit {
    fn from_prop_value(value: crate::ui::PropValue) -> Result<Self, String> {
        match value {
            crate::ui::PropValue::Shared(shared) => shared
                .value()
                .downcast::<ImageFit>()
                .map(|value| *value)
                .map_err(|_| "expected ImageFit value".to_string()),
            _ => Err("expected ImageFit value".to_string()),
        }
    }
}

impl crate::ui::IntoPropValue for ImageSource {
    fn into_prop_value(self) -> crate::ui::PropValue {
        crate::ui::PropValue::Shared(crate::ui::SharedPropValue::new(Rc::new(self)))
    }
}

impl crate::ui::IntoPropValue for SvgSource {
    fn into_prop_value(self) -> crate::ui::PropValue {
        crate::ui::PropValue::Shared(crate::ui::SharedPropValue::new(Rc::new(self)))
    }
}

impl From<SvgSource> for crate::ui::PropValue {
    fn from(value: SvgSource) -> Self {
        crate::ui::IntoPropValue::into_prop_value(value)
    }
}

impl crate::ui::FromPropValue for ImageSource {
    fn from_prop_value(value: crate::ui::PropValue) -> Result<Self, String> {
        match value {
            crate::ui::PropValue::Shared(shared) => shared
                .value()
                .downcast::<ImageSource>()
                .map(|value| (*value).clone())
                .map_err(|_| "expected ImageSource value".to_string()),
            _ => Err("expected ImageSource value".to_string()),
        }
    }
}

impl crate::ui::FromPropValue for SvgSource {
    fn from_prop_value(value: crate::ui::PropValue) -> Result<Self, String> {
        match value {
            crate::ui::PropValue::Shared(shared) => shared
                .value()
                .downcast::<SvgSource>()
                .map(|value| (*value).clone())
                .map_err(|_| "expected SvgSource value".to_string()),
            _ => Err("expected SvgSource value".to_string()),
        }
    }
}

impl crate::ui::IntoPropValue for RsxNode {
    fn into_prop_value(self) -> crate::ui::PropValue {
        crate::ui::PropValue::Shared(crate::ui::SharedPropValue::new(Rc::new(self)))
    }
}

impl crate::ui::FromPropValue for RsxNode {
    fn from_prop_value(value: crate::ui::PropValue) -> Result<Self, String> {
        match value {
            crate::ui::PropValue::Shared(shared) => shared
                .value()
                .downcast::<RsxNode>()
                .map(|value| (*value).clone())
                .map_err(|_| "expected RsxNode value".to_string()),
            _ => Err("expected RsxNode value".to_string()),
        }
    }
}
