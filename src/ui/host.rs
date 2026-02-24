use crate::ui::RsxNode;
use crate::ui::{
    BlurHandlerProp, ClickHandlerProp, FocusHandlerProp, KeyDownHandlerProp, KeyUpHandlerProp,
    MouseDownHandlerProp, MouseMoveHandlerProp, MouseUpHandlerProp, RsxChildrenPolicy,
    RsxComponent,
};
use crate::{
    AlignItems, BorderRadius, ColorLike, Display, FlowDirection, FlowWrap, FontFamily, FontWeight,
    JustifyContent, Length, Opacity, Padding, Position, ScrollDirection, Style, TextAlign,
    Transitions,
};

pub struct Element;
pub struct Text;
pub struct TextArea;

pub struct ElementPropSchema {
    pub anchor: String,
    pub padding: f64,
    pub padding_x: f64,
    pub padding_y: f64,
    pub padding_left: f64,
    pub padding_right: f64,
    pub padding_top: f64,
    pub padding_bottom: f64,
    pub opacity: f64,
    pub style: Option<Style>,
    pub on_mouse_down: Option<MouseDownHandlerProp>,
    pub on_mouse_up: Option<MouseUpHandlerProp>,
    pub on_mouse_move: Option<MouseMoveHandlerProp>,
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
    pub display: Display,
    pub flow_direction: FlowDirection,
    pub flow_wrap: FlowWrap,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub gap: Length,
    pub scroll_direction: ScrollDirection,
    pub color: Box<dyn ColorLike>,
    pub border: BorderStylePropSchema,
    pub background: Box<dyn ColorLike>,
    pub background_color: Box<dyn ColorLike>,
    pub font: FontFamily,
    pub font_weight: FontWeight,
    pub border_radius: BorderRadius,
    pub hover: Style,
    pub opacity: Opacity,
    pub padding: Padding,
    pub transition: Transitions,
}

pub struct BorderStylePropSchema {
    pub width: Length,
    pub color: Box<dyn ColorLike>,
}

pub struct TextPropSchema {
    pub style: Option<Style>,
    pub align: TextAlign,
    pub font_size: f64,
    pub line_height: f64,
    pub font: String,
    pub opacity: f64,
    pub children: Vec<RsxNode>,
}

pub struct TextAreaPropSchema {
    pub content: String,
    pub binding: Option<crate::ui::Binding<String>>,
    pub placeholder: String,
    pub color: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub font_size: f64,
    pub font: String,
    pub opacity: f64,
    pub multiline: bool,
    pub read_only: bool,
    pub max_length: i64,
    pub children: Vec<RsxNode>,
}

impl Default for ElementPropSchema {
    fn default() -> Self {
        Self {
            anchor: String::new(),
            padding: 0.0,
            padding_x: 0.0,
            padding_y: 0.0,
            padding_left: 0.0,
            padding_right: 0.0,
            padding_top: 0.0,
            padding_bottom: 0.0,
            opacity: 0.0,
            style: None,
            on_mouse_down: None,
            on_mouse_up: None,
            on_mouse_move: None,
            on_click: None,
            on_key_down: None,
            on_key_up: None,
            on_focus: None,
            on_blur: None,
            children: Vec::new(),
        }
    }
}

impl crate::ui::OptionalDefault for ElementPropSchema {
    fn optional_default() -> Self {
        Self::default()
    }
}

impl Default for TextPropSchema {
    fn default() -> Self {
        Self {
            style: None,
            align: TextAlign::Left,
            font_size: 0.0,
            line_height: 0.0,
            font: String::new(),
            opacity: 0.0,
            children: Vec::new(),
        }
    }
}

impl crate::ui::OptionalDefault for TextPropSchema {
    fn optional_default() -> Self {
        Self::default()
    }
}

impl Default for TextAreaPropSchema {
    fn default() -> Self {
        Self {
            content: String::new(),
            binding: None,
            placeholder: String::new(),
            color: String::new(),
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            font_size: 0.0,
            font: String::new(),
            opacity: 0.0,
            multiline: false,
            read_only: false,
            max_length: 0,
            children: Vec::new(),
        }
    }
}

impl crate::ui::OptionalDefault for TextAreaPropSchema {
    fn optional_default() -> Self {
        Self::default()
    }
}

impl RsxComponent for Element {
    type Props = ElementPropSchema;

    fn render(props: Self::Props) -> RsxNode {
        let mut node = RsxNode::element("Element");
        if !props.anchor.is_empty() {
            node = node.with_prop("anchor", props.anchor);
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

impl RsxComponent for Text {
    type Props = TextPropSchema;

    fn render(props: Self::Props) -> RsxNode {
        let mut node = RsxNode::element("Text");
        if let Some(style) = props.style {
            node = node.with_prop("style", style);
        }
        node = node.with_prop("align", props.align);
        if props.font_size != 0.0 {
            node = node.with_prop("font_size", props.font_size);
        }
        if props.line_height != 0.0 {
            node = node.with_prop("line_height", props.line_height);
        }
        if !props.font.is_empty() {
            node = node.with_prop("font", props.font);
        }
        if props.opacity != 0.0 {
            node = node.with_prop("opacity", props.opacity);
        }
        for child in props.children {
            node = node.with_child(child);
        }
        node
    }
}

impl RsxComponent for TextArea {
    type Props = TextAreaPropSchema;

    fn render(props: Self::Props) -> RsxNode {
        let mut node = RsxNode::element("TextArea");
        if !props.content.is_empty() {
            node = node.with_prop("content", props.content);
        }
        if let Some(binding) = props.binding {
            node = node.with_prop(
                "binding",
                crate::ui::IntoPropValue::into_prop_value(binding),
            );
        }
        if !props.placeholder.is_empty() {
            node = node.with_prop("placeholder", props.placeholder);
        }
        if !props.color.is_empty() {
            node = node.with_prop("color", props.color);
        }
        if props.x != 0.0 {
            node = node.with_prop("x", props.x);
        }
        if props.y != 0.0 {
            node = node.with_prop("y", props.y);
        }
        if props.width != 0.0 {
            node = node.with_prop("width", props.width);
        }
        if props.height != 0.0 {
            node = node.with_prop("height", props.height);
        }
        if props.font_size != 0.0 {
            node = node.with_prop("font_size", props.font_size);
        }
        if !props.font.is_empty() {
            node = node.with_prop("font", props.font);
        }
        if props.opacity != 0.0 {
            node = node.with_prop("opacity", props.opacity);
        }
        node = node.with_prop("multiline", props.multiline);
        node = node.with_prop("read_only", props.read_only);
        if props.max_length != 0 {
            node = node.with_prop("max_length", props.max_length);
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
