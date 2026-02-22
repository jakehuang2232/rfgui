use crate::ui::{
    BlurHandlerProp, ClickHandlerProp, FocusHandlerProp, KeyDownHandlerProp, KeyUpHandlerProp,
    MouseDownHandlerProp, MouseMoveHandlerProp, MouseUpHandlerProp, RsxChildrenPolicy, RsxNode,
    RsxPropSchema, RsxProps, RsxTag,
};
use crate::{
    AlignItems, BorderRadius, ColorLike, Display, FlowDirection, FlowWrap, JustifyContent, Length,
    Opacity, Padding, ScrollDirection, Style, Transitions,
};

pub struct Element;
pub struct Text;
pub struct TextArea;

pub struct ElementPropSchema {
    pub padding: f64,
    pub padding_x: f64,
    pub padding_y: f64,
    pub padding_left: f64,
    pub padding_right: f64,
    pub padding_top: f64,
    pub padding_bottom: f64,
    pub opacity: f64,
    pub style: Style,
    pub on_mouse_down: MouseDownHandlerProp,
    pub on_mouse_up: MouseUpHandlerProp,
    pub on_mouse_move: MouseMoveHandlerProp,
    pub on_click: ClickHandlerProp,
    pub on_key_down: KeyDownHandlerProp,
    pub on_key_up: KeyUpHandlerProp,
    pub on_focus: FocusHandlerProp,
    pub on_blur: BlurHandlerProp,
    pub children: Vec<RsxNode>,
}

pub struct ElementStylePropSchema {
    pub width: Length,
    pub height: Length,
    pub display: Display,
    pub flow_direction: FlowDirection,
    pub flow_wrap: FlowWrap,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub gap: Length,
    pub scroll_direction: ScrollDirection,
    pub border: BorderStylePropSchema,
    pub background: Box<dyn ColorLike>,
    pub background_color: Box<dyn ColorLike>,
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
    pub content: String,
    pub color: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub font_size: f64,
    pub font: String,
    pub opacity: f64,
    pub children: String,
}

pub struct TextAreaPropSchema {
    pub content: String,
    pub binding: crate::ui::Binding<String>,
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
    pub children: String,
}

impl RsxTag for Element {
    fn rsx_render(mut props: RsxProps, children: Vec<RsxNode>) -> Result<RsxNode, String> {
        let mut node = RsxNode::element("Element");

        for key in [
            "style",
            "on_mouse_down",
            "on_mouse_up",
            "on_mouse_move",
            "on_click",
            "on_key_down",
            "on_key_up",
            "on_focus",
            "on_blur",
        ] {
            if let Some(value) = props.remove_raw(key) {
                node = node.with_prop(key, value);
            }
        }

        props.reject_remaining("Element")?;

        for child in children {
            node = node.with_child(child);
        }

        Ok(node)
    }
}

impl RsxPropSchema for Element {
    type PropsSchema = ElementPropSchema;
}

impl RsxChildrenPolicy for Element {
    const ACCEPTS_CHILDREN: bool = true;
}

impl RsxTag for Text {
    fn rsx_render(mut props: RsxProps, children: Vec<RsxNode>) -> Result<RsxNode, String> {
        let mut node = RsxNode::element("Text");

        if let Some(content) = props.remove_t::<String>("content")? {
            node = node.with_prop("content", content);
        }

        for key in [
            "content",
            "color",
            "x",
            "y",
            "width",
            "height",
            "font_size",
            "font",
            "opacity",
        ] {
            if let Some(value) = props.remove_raw(key) {
                node = node.with_prop(key, value);
            }
        }

        props.reject_remaining("Text")?;

        for child in children {
            node = node.with_child(child);
        }

        Ok(node)
    }
}

impl RsxPropSchema for Text {
    type PropsSchema = TextPropSchema;
}

impl RsxChildrenPolicy for Text {
    const ACCEPTS_CHILDREN: bool = true;
}

impl RsxTag for TextArea {
    fn rsx_render(mut props: RsxProps, children: Vec<RsxNode>) -> Result<RsxNode, String> {
        let mut node = RsxNode::element("TextArea");

        if let Some(content) = props.remove_t::<String>("content")? {
            node = node.with_prop("content", content);
        }
        if let Some(placeholder) = props.remove_t::<String>("placeholder")? {
            node = node.with_prop("placeholder", placeholder);
        }
        if let Some(multiline) = props.remove_t::<bool>("multiline")? {
            node = node.with_prop("multiline", multiline);
        }
        if let Some(read_only) = props.remove_t::<bool>("read_only")? {
            node = node.with_prop("read_only", read_only);
        }
        if let Some(binding) = props.remove_t::<crate::ui::Binding<String>>("binding")? {
            node = node.with_prop("binding", crate::ui::IntoPropValue::into_prop_value(binding));
        }

        for key in [
            "content",
            "binding",
            "placeholder",
            "color",
            "x",
            "y",
            "width",
            "height",
            "font_size",
            "font",
            "opacity",
            "multiline",
            "read_only",
            "max_length",
        ] {
            if let Some(value) = props.remove_raw(key) {
                node = node.with_prop(key, value);
            }
        }

        props.reject_remaining("TextArea")?;

        for child in children {
            node = node.with_child(child);
        }

        Ok(node)
    }
}

impl RsxPropSchema for TextArea {
    type PropsSchema = TextAreaPropSchema;
}

impl RsxChildrenPolicy for TextArea {
    const ACCEPTS_CHILDREN: bool = true;
}
