use crate::ui::RsxNode;
use crate::ui::{
    BlurHandlerProp, ClickHandlerProp, FocusHandlerProp, KeyDownHandlerProp, KeyUpHandlerProp,
    MouseDownHandlerProp, MouseEnterHandlerProp, MouseLeaveHandlerProp, MouseMoveHandlerProp,
    MouseUpHandlerProp, RsxChildrenPolicy, RsxComponent, RsxPropsStyleSchema, RsxStyleSchema,
    TextAreaFocusHandlerProp, TextChangeHandlerProp, props,
};
use crate::{
    Align, BorderRadius, BoxShadow, ColorLike, CrossSize, Cursor, Flex, FontFamily, FontSize,
    FontWeight, Layout, Length, Opacity, Padding, Position, ScrollDirection, Style, TextAlign,
    TextWrap, Transitions,
};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

pub struct Element;
pub struct Text;
pub struct TextArea;
pub struct Image;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFit {
    Contain,
    Cover,
    Fill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageSampling {
    Linear,
    Nearest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageSource {
    Path(PathBuf),
    Rgba {
        width: u32,
        height: u32,
        pixels: Arc<[u8]>,
    },
}

#[props]
pub struct ElementPropSchema {
    pub anchor: Option<String>,
    pub style: Option<Style>,
    pub on_mouse_down: Option<MouseDownHandlerProp>,
    pub on_mouse_up: Option<MouseUpHandlerProp>,
    pub on_mouse_move: Option<MouseMoveHandlerProp>,
    pub on_mouse_enter: Option<MouseEnterHandlerProp>,
    pub on_mouse_leave: Option<MouseLeaveHandlerProp>,
    pub on_click: Option<ClickHandlerProp>,
    pub on_key_down: Option<KeyDownHandlerProp>,
    pub on_key_up: Option<KeyUpHandlerProp>,
    pub on_focus: Option<FocusHandlerProp>,
    pub on_blur: Option<BlurHandlerProp>,
}

pub struct ElementStylePropSchema {
    pub position: Position,
    pub width: Length,
    pub height: Length,
    pub min_width: Length,
    pub max_width: Length,
    pub min_height: Length,
    pub max_height: Length,
    pub layout: Layout,
    pub cross_size: CrossSize,
    pub align: Align,
    pub flex: Flex,
    pub gap: Length,
    pub scroll_direction: ScrollDirection,
    pub cursor: Cursor,
    pub color: Box<dyn ColorLike>,
    pub border: BorderStylePropSchema,
    pub background: Box<dyn ColorLike>,
    pub background_color: Box<dyn ColorLike>,
    pub font: FontFamily,
    pub font_size: FontSize,
    pub font_weight: FontWeight,
    pub text_wrap: TextWrap,
    pub border_radius: BorderRadius,
    pub hover: Style,
    pub selection: SelectionStylePropSchema,
    pub opacity: Opacity,
    pub box_shadow: Vec<BoxShadow>,
    pub padding: Padding,
    pub transition: Transitions,
}

pub struct TextStylePropSchema {
    pub color: Box<dyn ColorLike>,
    pub font: FontFamily,
    pub font_size: FontSize,
    pub font_weight: FontWeight,
    pub text_wrap: TextWrap,
    pub cursor: Cursor,
    pub hover: Style,
    pub opacity: Opacity,
    pub transition: Transitions,
}

pub struct SelectionStylePropSchema {
    pub background: Box<dyn ColorLike>,
}

pub struct BorderStylePropSchema {
    pub width: Length,
    pub color: Box<dyn ColorLike>,
}

impl RsxStyleSchema for ElementStylePropSchema {
    type SelectionSchema = SelectionStylePropSchema;
}

impl RsxStyleSchema for TextStylePropSchema {
    type SelectionSchema = SelectionStylePropSchema;
}

impl RsxPropsStyleSchema for ElementPropSchema {
    type StyleSchema = ElementStylePropSchema;
}

impl RsxPropsStyleSchema for TextPropSchema {
    type StyleSchema = TextStylePropSchema;
}

impl RsxPropsStyleSchema for TextAreaPropSchema {
    type StyleSchema = ElementStylePropSchema;
}

impl RsxPropsStyleSchema for ImagePropSchema {
    type StyleSchema = ElementStylePropSchema;
}

#[props]
pub struct TextPropSchema {
    pub style: Option<Style>,
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
    pub style: Option<Style>,
    pub on_focus: Option<TextAreaFocusHandlerProp>,
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
    pub style: Option<Style>,
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
        if let Some(handler) = props.on_mouse_down {
            node = node.with_prop("on_mouse_down", handler);
        }
        if let Some(handler) = props.on_mouse_up {
            node = node.with_prop("on_mouse_up", handler);
        }
        if let Some(handler) = props.on_mouse_move {
            node = node.with_prop("on_mouse_move", handler);
        }
        if let Some(handler) = props.on_mouse_enter {
            node = node.with_prop("on_mouse_enter", handler);
        }
        if let Some(handler) = props.on_mouse_leave {
            node = node.with_prop("on_mouse_leave", handler);
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
        let mut node = RsxNode::tagged("TextArea", crate::ui::RsxTagDescriptor::of::<TextArea>());
        if let Some(content) = props.content
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
        for child in children {
            node = node.with_child(child);
        }
        node
    }
}

impl RsxComponent<ImagePropSchema> for Image {
    fn render(props: ImagePropSchema, _children: Vec<RsxNode>) -> RsxNode {
        let mut node = RsxNode::tagged("Image", crate::ui::RsxTagDescriptor::of::<Image>())
            .with_prop("source", crate::ui::IntoPropValue::into_prop_value(props.source));
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

impl RsxChildrenPolicy for Element {
    const ACCEPTS_CHILDREN: bool = true;
}

impl RsxChildrenPolicy for Text {
    const ACCEPTS_CHILDREN: bool = true;
}

impl RsxChildrenPolicy for TextArea {
    const ACCEPTS_CHILDREN: bool = true;
}

impl RsxChildrenPolicy for Image {
    const ACCEPTS_CHILDREN: bool = false;
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
