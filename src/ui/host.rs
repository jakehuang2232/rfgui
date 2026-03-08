use crate::ui::RsxNode;
use crate::ui::{
    BlurHandlerProp, ClickHandlerProp, FocusHandlerProp, KeyDownHandlerProp, KeyUpHandlerProp,
    MouseDownHandlerProp, MouseEnterHandlerProp, MouseLeaveHandlerProp, MouseMoveHandlerProp,
    MouseUpHandlerProp, RsxChildrenPolicy, RsxComponent, props,
};
use crate::{
    AlignItems, BorderRadius, BoxShadow, ColorLike, Cursor, Layout, FontFamily, FontSize,
    FontWeight, Length, Opacity, Padding, Position, ScrollDirection, Style, TextAlign, Transitions,
};

pub struct Element;
pub struct Text;
pub struct TextArea;

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
    pub children: Vec<RsxNode>,
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
    pub align_items: AlignItems,
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
    pub border_radius: BorderRadius,
    pub hover: Style,
    pub opacity: Opacity,
    pub box_shadow: Vec<BoxShadow>,
    pub padding: Padding,
    pub transition: Transitions,
}

pub struct BorderStylePropSchema {
    pub width: Length,
    pub color: Box<dyn ColorLike>,
}

#[props]
pub struct TextPropSchema {
    pub style: Option<Style>,
    pub align: Option<TextAlign>,
    pub font_size: Option<FontSize>,
    pub line_height: Option<f64>,
    pub font: Option<String>,
    pub opacity: Option<f64>,
    pub children: Vec<RsxNode>,
}

#[props]
pub struct TextAreaPropSchema {
    pub content: Option<String>,
    pub binding: Option<crate::ui::Binding<String>>,
    pub placeholder: Option<String>,
    pub color: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub font_size: Option<FontSize>,
    pub font: Option<String>,
    pub opacity: Option<f64>,
    pub multiline: Option<bool>,
    pub read_only: Option<bool>,
    pub max_length: Option<i64>,
    pub children: Vec<RsxNode>,
}

impl RsxComponent<ElementPropSchema> for Element {
    fn render(props: ElementPropSchema) -> RsxNode {
        let mut node = RsxNode::element("Element");
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
        for child in props.children {
            node = node.with_child(child);
        }
        node
    }
}

impl RsxComponent<TextPropSchema> for Text {
    fn render(props: TextPropSchema) -> RsxNode {
        let mut node = RsxNode::element("Text");
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
        for child in props.children {
            node = node.with_child(child);
        }
        node
    }
}

impl RsxComponent<TextAreaPropSchema> for TextArea {
    fn render(props: TextAreaPropSchema) -> RsxNode {
        let mut node = RsxNode::element("TextArea");
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
        if let Some(placeholder) = props.placeholder
            && !placeholder.is_empty()
        {
            node = node.with_prop("placeholder", placeholder);
        }
        if let Some(color) = props.color
            && !color.is_empty()
        {
            node = node.with_prop("color", color);
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
        if let Some(width) = props.width
            && width != 0.0
        {
            node = node.with_prop("width", width);
        }
        if let Some(height) = props.height
            && height != 0.0
        {
            node = node.with_prop("height", height);
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
        for child in props.children {
            node = node.with_child(child);
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

fn is_unset_font_size(font_size: FontSize) -> bool {
    matches!(font_size, FontSize::Px(v) if v == 0.0)
}
