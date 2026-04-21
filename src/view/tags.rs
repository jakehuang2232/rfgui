#![allow(missing_docs)]

//! Built-in RSX host tags and their typed prop schemas.

use crate::ui::RsxNode;
use crate::ui::{
    BlurHandlerProp, ClickHandlerProp, FocusHandlerProp, FromPropValue, IntoPropValue,
    KeyDownHandlerProp, KeyUpHandlerProp, PointerDownHandlerProp, PointerEnterHandlerProp,
    PointerLeaveHandlerProp, PointerMoveHandlerProp, PointerUpHandlerProp,
    RsxComponent, SharedPropValue, TextAreaFocusHandlerProp,
    TextAreaRenderHandlerProp, TextChangeHandlerProp, props,
};
use crate::{
    Align, Animator, BorderRadius, BoxShadow, ColorLike, CrossSize, Cursor, Flex, FontFamily,
    FontSize, FontWeight, IntoAnimationStyle, Layout, Length, Opacity, Padding, Position,
    ScrollDirection, SelectionStyle, Style, TextAlign, TextWrap, Transform, TransformOrigin,
    Transitions,
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
    pub style: Option<ElementStylePropSchema>,
    pub on_pointer_down: Option<PointerDownHandlerProp>,
    pub on_pointer_up: Option<PointerUpHandlerProp>,
    pub on_pointer_move: Option<PointerMoveHandlerProp>,
    pub on_pointer_enter: Option<PointerEnterHandlerProp>,
    pub on_pointer_leave: Option<PointerLeaveHandlerProp>,
    pub on_click: Option<ClickHandlerProp>,
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
    pub border: Option<crate::Border>,
    pub background: Option<crate::Background>,
    pub background_color: Option<Box<dyn ColorLike>>,
    pub background_image: Option<crate::Gradient>,
    pub border_image: Option<crate::Gradient>,
    pub font: Option<FontFamily>,
    pub font_size: Option<FontSize>,
    pub font_weight: Option<FontWeight>,
    pub text_wrap: Option<TextWrap>,
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
    pub border: Option<crate::Border>,
    pub background: Option<crate::Background>,
    pub background_color: Option<Box<dyn ColorLike>>,
    pub background_image: Option<crate::Gradient>,
    pub border_image: Option<crate::Gradient>,
    pub font: Option<FontFamily>,
    pub font_size: Option<FontSize>,
    pub font_weight: Option<FontWeight>,
    pub text_wrap: Option<TextWrap>,
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
    pub transition: Option<Transitions>,
}

#[derive(Clone)]
#[props]
pub struct SelectionStylePropSchema {
    pub background: Option<crate::Background>,
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
pub struct TextAreaPropSchema {
    pub content: Option<String>,
    pub binding: Option<crate::ui::Binding<String>>,
    pub style: Option<ElementStylePropSchema>,
    pub on_focus: Option<TextAreaFocusHandlerProp>,
    pub on_render: Option<TextAreaRenderHandlerProp>,
    pub on_blur: Option<BlurHandlerProp>,
    pub on_change: Option<TextChangeHandlerProp>,
    pub placeholder: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub font_size: Option<FontSize>,
    pub font: Option<String>,
    pub opacity: Option<f64>,
    pub multiline: Option<bool>,
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
        let mut node = RsxNode::tagged("Element", crate::ui::RsxTagDescriptor::of::<Element>());
        if let Some(anchor) = props.anchor {
            node = node.with_prop("anchor", anchor);
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
        let mut node = RsxNode::tagged("Text", crate::ui::RsxTagDescriptor::of::<Text>());
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

        let mut node = RsxNode::tagged("TextArea", crate::ui::RsxTagDescriptor::of::<TextArea>());
        if let Some(content) = resolved_content.clone()
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
        if let Some(x) = props.x
            && x != 0.0
        {
            node = node.with_prop("x", x);
        }
        if let Some(y) = props.y
            && y != 0.0
        {
            node = node.with_prop("y", y);
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
        if let Some(opacity) = props.opacity
            && opacity != 0.0
        {
            node = node.with_prop("opacity", opacity);
        }
        if let Some(multiline) = props.multiline {
            node = node.with_prop("multiline", multiline);
        }
        if let Some(read_only) = props.read_only {
            node = node.with_prop("read_only", read_only);
        }
        if let Some(max_length) = props.max_length
            && max_length != 0
        {
            node = node.with_prop("max_length", max_length);
        }
        // 軌 1 #12: on_render handler passed straight through as prop
        // and executed by the TextArea host against its own content in
        // `rebuild_projection_tree_if_dirty`. No projection RSX
        // children are emitted here — projection subtrees are fully
        // owned by the TextArea, invisible to the reconciler.
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
        let mut node = RsxNode::tagged("Image", crate::ui::RsxTagDescriptor::of::<Image>())
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
        let mut node = RsxNode::tagged("Svg", crate::ui::RsxTagDescriptor::of::<Svg>()).with_prop(
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

#[doc(hidden)]
pub mod v2_bench {
    use super::*;

    #[inline(never)]
    pub fn bench_v1_20() -> RsxNode {
        crate::ui::rsx! {
            <Element>
                <Element>a00</Element>
                <Element>a01</Element>
                <Element>a02</Element>
                <Element>a03</Element>
                <Element>a04</Element>
                <Element>a05</Element>
                <Element>a06</Element>
                <Element>a07</Element>
                <Element>a08</Element>
                <Element>a09</Element>
                <Element>a10</Element>
                <Element>a11</Element>
                <Element>a12</Element>
                <Element>a13</Element>
                <Element>a14</Element>
                <Element>a15</Element>
                <Element>a16</Element>
                <Element>a17</Element>
                <Element>a18</Element>
                <Element>a19</Element>
            </Element>
        }
    }

    #[inline(never)]
    pub fn bench_v2_20() -> RsxNode {
        crate::ui::rsx! {
            <Element>
                <Element>a00</Element>
                <Element>a01</Element>
                <Element>a02</Element>
                <Element>a03</Element>
                <Element>a04</Element>
                <Element>a05</Element>
                <Element>a06</Element>
                <Element>a07</Element>
                <Element>a08</Element>
                <Element>a09</Element>
                <Element>a10</Element>
                <Element>a11</Element>
                <Element>a12</Element>
                <Element>a13</Element>
                <Element>a14</Element>
                <Element>a15</Element>
                <Element>a16</Element>
                <Element>a17</Element>
                <Element>a18</Element>
                <Element>a19</Element>
            </Element>
        }
    }
}

#[cfg(test)]
mod v2_poc_tests {
    use super::*;
    use crate::ui::{__rsx_default_inner_option, create_element};
    #[allow(unused_imports)]
    use crate::ui::RsxTag;

    // Test A: explicit type annotations — baseline.
    #[test]
    fn v2_element_build_explicit() {
        let node = create_element::<Element>(
            {
                let mut init: ElementPropSchema = Default::default();
                init.style = Some({
                    let mut s: ElementStylePropSchema = Default::default();
                    s.background_color = Some(Box::new(crate::Color::hex("#000000")));
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
                    s.background_color = Some(Box::new(crate::Color::hex("#000000")));
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
                    s.background_color = Some(Box::new(crate::Color::hex("#111111")));
                    s.hover = Some({
                        let mut h = __rsx_default_inner_option(&s.hover);
                        h.background_color = Some(Box::new(crate::Color::hex("#222222")));
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
    use crate::ui::rsx;
    use crate::Length;

    #[test]
    fn rsx_simple_element() {
        let node = rsx! { <Element /> };
        match node {
            RsxNode::Element(_) => {}
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn rsx_element_with_style_object() {
        let node = rsx! {
            <Element style={{
                width: Length::px(100.0),
                background_color: crate::Color::hex("#111111"),
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
                background_color: crate::Color::hex("#111111"),
                hover: {
                    background_color: crate::Color::hex("#222222"),
                },
            }} />
        };
        match node {
            RsxNode::Element(_) => {}
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn bench_v1_v2_produce_equivalent_output() {
        let a = super::v2_bench::bench_v1_20();
        let b = super::v2_bench::bench_v2_20();
        match (&a, &b) {
            (RsxNode::Element(ae), RsxNode::Element(be)) => {
                assert_eq!(ae.children.len(), be.children.len());
            }
            _ => panic!("expected elements"),
        }
    }

    // ---------- #[component] + rsx end-to-end ----------

    #[crate::ui::component]
    pub fn V2PanelLabel(text: String, color: Option<crate::Color>) -> RsxNode {
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
                color={crate::Color::hex("#aabbcc")} />
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
    ($tag:ty, $props:ty, $accepts:expr) => {
        impl crate::ui::RsxTag for $tag {
            type Props = $props;
            type StrictProps = $props;
            const ACCEPTS_CHILDREN: bool = $accepts;
            const IS_HOST_TAG: bool = true;

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
    };
}

impl_rsx_tag_v2_trivial!(Element, ElementPropSchema, true);
impl_rsx_tag_v2_trivial!(Text, TextPropSchema, true);
impl_rsx_tag_v2_trivial!(TextArea, TextAreaPropSchema, true);

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
            source: i
                .source
                .expect("missing required prop `source` on <Image>"),
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
            source: i
                .source
                .expect("missing required prop `source` on <Svg>"),
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
    property: crate::PropertyId,
    color: &Option<Box<dyn ColorLike>>,
) {
    if let Some(color) = color {
        style.insert(property, crate::style_color_value(color.clone()));
    }
}

fn apply_background(style: &mut Style, background: Option<&crate::Background>) {
    let Some(background) = background else {
        return;
    };
    match background {
        crate::Background::Color(color) => {
            style.insert(
                crate::PropertyId::BackgroundColor,
                crate::style_color_value(color.clone()),
            );
        }
        crate::Background::Gradient(gradient) => {
            style.insert(
                crate::PropertyId::BackgroundImage,
                crate::ParsedValue::Gradient(gradient.clone()),
            );
        }
    }
}

fn apply_selection(selection: &Option<SelectionStylePropSchema>) -> Option<SelectionStyle> {
    let selection = selection.as_ref()?;
    let mut output = SelectionStyle::new();
    if let Some(background) = &selection.background {
        match background {
            crate::Background::Color(color) => output.set_background(color.clone()),
            crate::Background::Gradient(_) => {
                // Selection highlight only supports solid colors; gradients are ignored.
            }
        }
    }
    Some(output)
}

fn apply_element_style_fields(style: &mut Style, schema: &HoverElementStylePropSchema) {
    if let Some(position) = schema.position.clone() {
        style.insert(
            crate::PropertyId::Position,
            crate::ParsedValue::Position(position),
        );
    }
    if let Some(width) = schema.width {
        crate::insert_style_length(style, crate::PropertyId::Width, width);
    }
    if let Some(height) = schema.height {
        crate::insert_style_length(style, crate::PropertyId::Height, height);
    }
    if let Some(min_width) = schema.min_width {
        crate::insert_style_length(style, crate::PropertyId::MinWidth, min_width);
    }
    if let Some(max_width) = schema.max_width {
        crate::insert_style_length(style, crate::PropertyId::MaxWidth, max_width);
    }
    if let Some(min_height) = schema.min_height {
        crate::insert_style_length(style, crate::PropertyId::MinHeight, min_height);
    }
    if let Some(max_height) = schema.max_height {
        crate::insert_style_length(style, crate::PropertyId::MaxHeight, max_height);
    }
    if let Some(layout) = schema.layout {
        style.insert(
            crate::PropertyId::Layout,
            crate::ParsedValue::Layout(layout),
        );
    }
    if let Some(cross_size) = schema.cross_size {
        style.insert(
            crate::PropertyId::CrossSize,
            crate::ParsedValue::CrossSize(cross_size),
        );
    }
    if let Some(align) = schema.align {
        style.insert(crate::PropertyId::Align, crate::ParsedValue::Align(align));
    }
    if let Some(flex) = schema.flex {
        crate::insert_style_flex(style, crate::PropertyId::Flex, flex);
    }
    if let Some(gap) = schema.gap {
        crate::insert_style_length(style, crate::PropertyId::Gap, gap);
    }
    if let Some(scroll_direction) = schema.scroll_direction {
        style.insert(
            crate::PropertyId::ScrollDirection,
            crate::ParsedValue::ScrollDirection(scroll_direction),
        );
    }
    if let Some(cursor) = schema.cursor {
        style.insert(
            crate::PropertyId::Cursor,
            crate::ParsedValue::Cursor(cursor),
        );
    }
    apply_box_color(style, crate::PropertyId::Color, &schema.color);
    apply_background(style, schema.background.as_ref());
    apply_box_color(
        style,
        crate::PropertyId::BackgroundColor,
        &schema.background_color,
    );
    if let Some(gradient) = &schema.background_image {
        style.insert(
            crate::PropertyId::BackgroundImage,
            crate::ParsedValue::Gradient(gradient.clone()),
        );
    }
    if let Some(gradient) = &schema.border_image {
        style.insert(
            crate::PropertyId::BorderImage,
            crate::ParsedValue::Gradient(gradient.clone()),
        );
    }
    if let Some(border) = &schema.border {
        style.set_border(border.clone());
    }
    if let Some(font) = &schema.font {
        style.insert(
            crate::PropertyId::FontFamily,
            crate::ParsedValue::FontFamily(font.clone()),
        );
    }
    if let Some(font_size) = schema.font_size {
        crate::insert_style_font_size(style, crate::PropertyId::FontSize, font_size);
    }
    if let Some(font_weight) = schema.font_weight {
        crate::insert_style_font_weight(style, crate::PropertyId::FontWeight, font_weight);
    }
    if let Some(text_wrap) = schema.text_wrap {
        crate::insert_style_text_wrap(style, crate::PropertyId::TextWrap, text_wrap);
    }
    if let Some(border_radius) = schema.border_radius {
        style.set_border_radius(border_radius);
    }
    if let Some(opacity) = schema.opacity {
        style.insert(
            crate::PropertyId::Opacity,
            crate::ParsedValue::Opacity(opacity),
        );
    }
    if let Some(box_shadow) = &schema.box_shadow {
        style.insert(
            crate::PropertyId::BoxShadow,
            crate::ParsedValue::BoxShadow(box_shadow.clone()),
        );
    }
    if let Some(padding) = schema.padding {
        style.set_padding(padding);
    }
    if let Some(transform) = &schema.transform {
        style.set_transform(transform.clone());
    }
    if let Some(transform_origin) = schema.transform_origin {
        style.set_transform_origin(transform_origin);
    }
    if let Some(transition) = &schema.transition {
        style.insert(
            crate::PropertyId::Transition,
            crate::ParsedValue::Transition(transition.clone()),
        );
    }
    if let Some(animator) = &schema.animator {
        style.insert(
            crate::PropertyId::Animator,
            crate::ParsedValue::Animator(animator.clone()),
        );
    }
    if let Some(selection) = apply_selection(&schema.selection) {
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
        let hover_view = HoverElementStylePropSchema {
            position: self.position.clone(),
            width: self.width,
            height: self.height,
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
            cursor: self.cursor,
            color: self.color.clone(),
            border: self.border.clone(),
            background: self.background.clone(),
            background_color: self.background_color.clone(),
            background_image: self.background_image.clone(),
            border_image: self.border_image.clone(),
            font: self.font.clone(),
            font_size: self.font_size,
            font_weight: self.font_weight,
            text_wrap: self.text_wrap,
            selection: self.selection.clone(),
            border_radius: self.border_radius,
            opacity: self.opacity,
            box_shadow: self.box_shadow.clone(),
            padding: self.padding,
            transform: self.transform.clone(),
            transform_origin: self.transform_origin,
            transition: self.transition.clone(),
            animator: self.animator.clone(),
        };
        apply_element_style_fields(&mut style, &hover_view);
        if let Some(hover) = &self.hover {
            style.set_hover(hover.to_style());
        }
        style
    }
}

impl HoverTextStylePropSchema {
    pub fn to_style(&self) -> Style {
        let mut style = Style::new();
        if let Some(width) = self.width {
            crate::insert_style_length(&mut style, crate::PropertyId::Width, width);
        }
        if let Some(height) = self.height {
            crate::insert_style_length(&mut style, crate::PropertyId::Height, height);
        }
        apply_box_color(&mut style, crate::PropertyId::Color, &self.color);
        if let Some(font) = &self.font {
            style.insert(
                crate::PropertyId::FontFamily,
                crate::ParsedValue::FontFamily(font.clone()),
            );
        }
        if let Some(font_size) = self.font_size {
            crate::insert_style_font_size(&mut style, crate::PropertyId::FontSize, font_size);
        }
        if let Some(font_weight) = self.font_weight {
            crate::insert_style_font_weight(&mut style, crate::PropertyId::FontWeight, font_weight);
        }
        if let Some(text_wrap) = self.text_wrap {
            crate::insert_style_text_wrap(&mut style, crate::PropertyId::TextWrap, text_wrap);
        }
        if let Some(cursor) = self.cursor {
            style.insert(
                crate::PropertyId::Cursor,
                crate::ParsedValue::Cursor(cursor),
            );
        }
        if let Some(opacity) = self.opacity {
            style.insert(
                crate::PropertyId::Opacity,
                crate::ParsedValue::Opacity(opacity),
            );
        }
        if let Some(transition) = &self.transition {
            style.insert(
                crate::PropertyId::Transition,
                crate::ParsedValue::Transition(transition.clone()),
            );
        }
        style
    }
}

impl TextStylePropSchema {
    pub fn to_style(&self) -> Style {
        let mut style = HoverTextStylePropSchema {
            width: self.width,
            height: self.height,
            color: self.color.clone(),
            font: self.font.clone(),
            font_size: self.font_size,
            font_weight: self.font_weight,
            text_wrap: self.text_wrap,
            cursor: self.cursor,
            opacity: self.opacity,
            transition: self.transition.clone(),
        }
        .to_style();
        if let Some(hover) = &self.hover {
            style.set_hover(hover.to_style());
        }
        style
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
