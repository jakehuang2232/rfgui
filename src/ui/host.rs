use crate::ui::{
    BlurHandlerProp, ClickHandlerProp, FocusHandlerProp, KeyDownHandlerProp, KeyUpHandlerProp,
    MouseDownHandlerProp, MouseMoveHandlerProp, MouseUpHandlerProp, RsxChildrenPolicy, RsxNode,
    RsxPropSchema, RsxProps, RsxTag,
};
use crate::{
    AlignItems, BorderRadius, ColorLike, Display, FlexDirection, FlexWrap, JustifyContent, Length,
    Opacity, Padding, ScrollDirection, Style, Transitions,
};

pub struct Element;
pub struct Text;

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
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
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
