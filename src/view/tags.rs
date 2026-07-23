#![allow(missing_docs)]

//! Built-in RSX host tags and their typed prop schemas.

use crate::style::style_props::{AllStyleSet, NoStylePropSchema, StylePropTrait, TextStyleSet};
use crate::style::{
    Align, Animator, BorderRadius, BoxShadow, ColorLike, CrossSize, Cursor, Flex, FontFamily,
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

#[derive(Clone)]
#[props]
pub struct ElementStylePropSchema {
    pub position: Option<Position>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub min_width: Option<Length>,
    pub max_width: Option<Length>,
    pub min_height: Option<Length>,
    pub max_height: Option<Length>,
    pub layout: Option<Layout>,
    pub cross_size: Option<CrossSize>,
    pub align: Option<Align>,
    pub flex: Option<Flex>,
    pub gap: Option<Length>,
    pub scroll_direction: Option<ScrollDirection>,
    pub cursor: Option<Cursor>,
    pub color: Option<Box<dyn ColorLike>>,
    pub border: Option<crate::style::Border>,
    pub background: Option<crate::style::Background>,
    pub background_color: Option<Box<dyn ColorLike>>,
    pub background_image: Option<crate::style::Gradient>,
    pub border_image: Option<crate::style::Gradient>,
    pub font: Option<FontFamily>,
    pub font_size: Option<FontSize>,
    pub font_weight: Option<FontWeight>,
    pub text_wrap: Option<TextWrap>,
    pub line_height: Option<f64>,
    pub vertical_align: Option<VerticalAlign>,
    pub border_radius: Option<BorderRadius>,
    pub hover: Option<HoverElementStylePropSchema>,
    pub selection: Option<SelectionStylePropSchema>,
    pub opacity: Option<Opacity>,
    pub box_shadow: Option<Vec<BoxShadow>>,
    pub padding: Option<Padding>,
    pub transform: Option<Transform>,
    pub transform_origin: Option<TransformOrigin>,
    pub transition: Option<Transitions>,
    pub animator: Option<Animator>,
}

#[derive(Clone)]
#[props]
pub struct HoverElementStylePropSchema {
    pub position: Option<Position>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub min_width: Option<Length>,
    pub max_width: Option<Length>,
    pub min_height: Option<Length>,
    pub max_height: Option<Length>,
    pub layout: Option<Layout>,
    pub cross_size: Option<CrossSize>,
    pub align: Option<Align>,
    pub flex: Option<Flex>,
    pub gap: Option<Length>,
    pub scroll_direction: Option<ScrollDirection>,
    pub cursor: Option<Cursor>,
    pub color: Option<Box<dyn ColorLike>>,
    pub border: Option<crate::style::Border>,
    pub background: Option<crate::style::Background>,
    pub background_color: Option<Box<dyn ColorLike>>,
    pub background_image: Option<crate::style::Gradient>,
    pub border_image: Option<crate::style::Gradient>,
    pub font: Option<FontFamily>,
    pub font_size: Option<FontSize>,
    pub font_weight: Option<FontWeight>,
    pub text_wrap: Option<TextWrap>,
    pub line_height: Option<f64>,
    pub vertical_align: Option<VerticalAlign>,
    pub border_radius: Option<BorderRadius>,
    pub selection: Option<SelectionStylePropSchema>,
    pub opacity: Option<Opacity>,
    pub box_shadow: Option<Vec<BoxShadow>>,
    pub padding: Option<Padding>,
    pub transform: Option<Transform>,
    pub transform_origin: Option<TransformOrigin>,
    pub transition: Option<Transitions>,
    pub animator: Option<Animator>,
}

#[derive(Clone)]
#[props]
pub struct TextStylePropSchema {
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub color: Option<Box<dyn ColorLike>>,
    pub font: Option<FontFamily>,
    pub font_size: Option<FontSize>,
    pub font_weight: Option<FontWeight>,
    pub text_wrap: Option<TextWrap>,
    pub cursor: Option<Cursor>,
    pub hover: Option<HoverTextStylePropSchema>,
    pub opacity: Option<Opacity>,
    pub transform: Option<Transform>,
    pub transform_origin: Option<TransformOrigin>,
    pub transition: Option<Transitions>,
}

#[derive(Clone)]
#[props]
pub struct HoverTextStylePropSchema {
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub color: Option<Box<dyn ColorLike>>,
    pub font: Option<FontFamily>,
    pub font_size: Option<FontSize>,
    pub font_weight: Option<FontWeight>,
    pub text_wrap: Option<TextWrap>,
    pub cursor: Option<Cursor>,
    pub opacity: Option<Opacity>,
    pub transform: Option<Transform>,
    pub transform_origin: Option<TransformOrigin>,
    pub transition: Option<Transitions>,
}

#[derive(Clone)]
#[props]
pub struct SelectionStylePropSchema {
    pub background: Option<crate::style::Background>,
}

#[derive(Clone)]
#[props]
pub struct BorderStylePropSchema {
    pub width: Option<Length>,
    pub color: Option<Box<dyn ColorLike>>,
}

#[props]
pub struct TextPropSchema {
    pub style: Option<TextStylePropSchema>,
    pub align: Option<TextAlign>,
    pub font_size: Option<FontSize>,
    pub line_height: Option<f64>,
    pub font: Option<String>,
    pub opacity: Option<f64>,
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
    pub on_render: Option<TextAreaRenderHandlerProp>,
    pub on_blur: Option<BlurHandlerProp>,
    pub on_change: Option<TextChangeHandlerProp>,
    pub placeholder: Option<String>,
    pub font_size: Option<FontSize>,
    pub font: Option<String>,
    pub multiline: Option<bool>,
    pub auto_wrap: Option<bool>,
    pub read_only: Option<bool>,
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
        if let Some(align) = props.align {
            node = node.with_prop("align", align);
        }
        if let Some(font_size) = props.font_size
            && !is_unset_font_size(font_size)
        {
            node = node.with_prop("font_size", font_size);
        }
        if let Some(line_height) = props.line_height
            && line_height != 0.0
        {
            node = node.with_prop("line_height", line_height);
        }
        if let Some(font) = props.font
            && !font.is_empty()
        {
            node = node.with_prop("font", font);
        }
        if let Some(opacity) = props.opacity
            && opacity != 0.0
        {
            node = node.with_prop("opacity", opacity);
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
    fn render(props: TextAreaPropSchema, children: Vec<RsxNode>) -> RsxNode {
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
        if let Some(font_size) = props.font_size
            && !is_unset_font_size(font_size)
        {
            node = node.with_prop("font_size", font_size);
        }
        if let Some(font) = props.font
            && !font.is_empty()
        {
            node = node.with_prop("font", font);
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
        if let Some(max_length) = props.max_length
            && max_length != 0
        {
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
mod v2_poc_tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::ui::RsxTag;
    use crate::ui::{__rsx_default_inner_option, create_element};

    // Test A: explicit type annotations — baseline.
    #[test]
    fn v2_element_build_explicit() {
        let node = create_element::<Element>(
            {
                let mut init: ElementPropSchema = Default::default();
                init.style = Some({
                    let mut s: ElementStylePropSchema = Default::default();
                    s.background_color = Some(Box::new(crate::style::Color::hex("#000000")));
                    s
                });
                init
            },
            Vec::new(),
            None,
        );
        match node {
            RsxNode::Element(_) => {}
            _ => panic!("expected element node"),
        }
    }

    // Test B: phantom-guided inference, no type name appears.
    #[test]
    fn v2_element_build_inferred() {
        let node = create_element::<Element>(
            {
                let mut init: ElementPropSchema = Default::default();
                init.style = Some({
                    let mut s = __rsx_default_inner_option(&init.style);
                    s.background_color = Some(Box::new(crate::style::Color::hex("#000000")));
                    s
                });
                init
            },
            Vec::new(),
            None,
        );
        match node {
            RsxNode::Element(_) => {}
            _ => panic!("expected element node"),
        }
    }

    // Test C: nested hover — phantom fallback all the way.
    #[test]
    fn v2_element_build_inferred_nested_hover() {
        let node = create_element::<Element>(
            {
                let mut init: ElementPropSchema = Default::default();
                init.style = Some({
                    let mut s = __rsx_default_inner_option(&init.style);
                    s.background_color = Some(Box::new(crate::style::Color::hex("#111111")));
                    s.hover = Some({
                        let mut h = __rsx_default_inner_option(&s.hover);
                        h.background_color = Some(Box::new(crate::style::Color::hex("#222222")));
                        h
                    });
                    s
                });
                init
            },
            Vec::new(),
            None,
        );
        match node {
            RsxNode::Element(_) => {}
            _ => panic!("expected element node"),
        }
    }

    // Verified manually: writing `s.not_a_real_field = ...` above produces
    // `E0609: no field 'not_a_real_field' on type ElementStylePropSchema`.
    // Compile-time field checks survive the phantom-fallback inference.

    // ---------- rsx! macro end-to-end tests ----------
    use crate::style::Length;
    use crate::ui::rsx;

    #[test]
    fn rsx_simple_element() {
        let node = rsx! { <Element /> };
        match node {
            RsxNode::Element(_) => {}
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn rsx_text_area_skeleton_builds() {
        let node = rsx! { <TextArea /> };
        match node {
            RsxNode::Element(ref el) => assert_eq!(el.tag, "TextArea"),
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn rsx_text_area_projection_segment_builds() {
        let node = rsx! {
            <TextAreaProjectionSegment char_range_start=0 char_range_end=5 />
        };
        match node {
            RsxNode::Element(ref el) => {
                assert_eq!(el.tag, "TextAreaProjectionSegment");
                let has_start = el.props.iter().any(|(k, _)| *k == "char_range_start");
                let has_end = el.props.iter().any(|(k, _)| *k == "char_range_end");
                assert!(has_start && has_end);
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn rsx_element_with_style_object() {
        let node = rsx! {
            <Element style={{
                width: Length::px(100.0),
                background_color: crate::style::Color::hex("#111111"),
            }}>
                <Element />
                <Element />
            </Element>
        };
        match node {
            RsxNode::Element(ref el) => {
                assert_eq!(el.children.len(), 2);
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn rsx_nested_hover_object() {
        let node = rsx! {
            <Element style={{
                background_color: crate::style::Color::hex("#111111"),
                hover: {
                    background_color: crate::style::Color::hex("#222222"),
                },
            }} />
        };
        match node {
            RsxNode::Element(_) => {}
            _ => panic!("expected element"),
        }
    }

    // ---------- #[component] + rsx end-to-end ----------

    #[crate::ui::component]
    pub fn V2PanelLabel(text: String, color: Option<crate::style::Color>) -> RsxNode {
        // Render path doesn't matter for this test; just return an empty element.
        // Intentionally use old `rsx!` inside component body to confirm
        // v1 body still compiles within a v2-tagged component.
        let _ = text;
        let _ = color;
        crate::ui::rsx! { <Element /> }
    }

    #[crate::ui::component]
    pub fn V2ContainerOnly(children: Vec<RsxNode>) -> RsxNode {
        crate::ui::rsx! { <Element>{children}</Element> }
    }

    #[test]
    fn rsx_user_component_with_required_prop() {
        let node = rsx! {
            <V2PanelLabel text={"hello".to_string()} />
        };
        match node {
            RsxNode::Element(_) => {}
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn rsx_user_component_optional_prop() {
        let node = rsx! {
            <V2PanelLabel
                text={"greet".to_string()}
                color={crate::style::Color::hex("#aabbcc")} />
        };
        match node {
            RsxNode::Element(_) => {}
            _ => panic!("expected element"),
        }
    }

    #[test]
    #[should_panic(expected = "missing required prop `text`")]
    fn rsx_user_component_missing_required_panics() {
        // Bypass the rsx macro; directly construct init struct and feed to
        // create_element to hit the From impl panic path.
        let init: <V2PanelLabel as crate::ui::RsxTag>::Props = Default::default();
        let _ = crate::ui::create_element::<V2PanelLabel>(init, Vec::new(), None);
    }

    #[test]
    fn rsx_user_component_with_children() {
        let node = rsx! {
            <V2ContainerOnly>
                <Element />
                <Element />
                <Element />
            </V2ContainerOnly>
        };
        // V2ContainerOnly's body `rsx! { <Element>{children}</Element> }` flattens
        // the Vec<RsxNode> into the outer Element's children via IntoRsxChildren,
        // so the returned element has 3 children.
        match node {
            RsxNode::Element(ref el) => assert_eq!(el.children.len(), 3),
            _ => panic!(),
        }
    }

    #[test]
    fn rsx_text_with_literal() {
        let node = rsx! {
            <Element>
                <Text>{"hello"}</Text>
            </Element>
        };
        match node {
            RsxNode::Element(ref el) => assert_eq!(el.children.len(), 1),
            _ => panic!(),
        }
    }

    // Test D: 4000 siblings via new path — share single monomorphization.
    #[test]
    fn v2_element_4000_siblings() {
        let mut children = Vec::with_capacity(4000);
        for _ in 0..4000 {
            children.push(create_element::<Element>(
                Default::default(),
                Vec::new(),
                None,
            ));
        }
        let root = create_element::<Element>(Default::default(), children, None);
        match root {
            RsxNode::Element(ref el) => assert_eq!(el.children.len(), 4000),
            _ => panic!(),
        }
    }
}

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
    ($ty:ty, $label:literal) => {
        impl IntoPropValue for $ty {
            fn into_prop_value(self) -> crate::ui::PropValue {
                into_shared_prop_value(self)
            }
        }

        impl From<$ty> for crate::ui::PropValue {
            fn from(value: $ty) -> Self {
                into_shared_prop_value(value)
            }
        }

        impl FromPropValue for $ty {
            fn from_prop_value(value: crate::ui::PropValue) -> Result<Self, String> {
                from_shared_prop_value(value, $label)
            }
        }
    };
}

impl_shared_style_prop_value!(ElementStylePropSchema, "ElementStylePropSchema");
impl_shared_style_prop_value!(HoverElementStylePropSchema, "HoverElementStylePropSchema");
impl_shared_style_prop_value!(TextStylePropSchema, "TextStylePropSchema");
impl_shared_style_prop_value!(HoverTextStylePropSchema, "HoverTextStylePropSchema");
impl_shared_style_prop_value!(SelectionStylePropSchema, "SelectionStylePropSchema");
impl_shared_style_prop_value!(BorderStylePropSchema, "BorderStylePropSchema");

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

impl SharedStylePropFields for HoverTextStylePropSchema {
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

struct ElementStyleFields<'a> {
    shared: SharedStyleFields<'a>,
    position: &'a Option<Position>,
    min_width: Option<Length>,
    max_width: Option<Length>,
    min_height: Option<Length>,
    max_height: Option<Length>,
    layout: Option<Layout>,
    cross_size: Option<CrossSize>,
    align: Option<Align>,
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
            cross_size: self.cross_size,
            align: self.align,
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
            cross_size: self.cross_size,
            align: self.align,
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
    if let Some(cross_size) = fields.cross_size {
        style.insert(
            crate::style::PropertyId::CrossSize,
            crate::style::ParsedValue::CrossSize(cross_size),
        );
    }
    if let Some(align) = fields.align {
        style.insert(
            crate::style::PropertyId::Align,
            crate::style::ParsedValue::Align(align),
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

impl HoverTextStylePropSchema {
    pub fn to_style(&self) -> Style {
        let mut style = Style::new();
        apply_shared_style_fields(&mut style, &self.shared_style_fields());
        if let Some(transform) = &self.transform {
            style.set_transform(transform.clone());
        }
        if let Some(transform_origin) = self.transform_origin {
            style.set_transform_origin(transform_origin);
        }
        style
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
        if let Some(hover) = &self.hover {
            style.set_hover(hover.to_style());
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
mod style_lowering_tests {
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
}

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

fn is_unset_font_size(font_size: FontSize) -> bool {
    match font_size {
        FontSize::Px(v)
        | FontSize::Percent(v)
        | FontSize::Em(v)
        | FontSize::Rem(v)
        | FontSize::Vw(v)
        | FontSize::Vh(v) => v == 0.0,
    }
}
