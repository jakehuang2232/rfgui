use crate::ui::RsxNode;
use crate::ui::{
    BlurHandlerProp, ClickHandlerProp, FocusHandlerProp, KeyDownHandlerProp, KeyUpHandlerProp,
    MouseDownHandlerProp, MouseMoveHandlerProp, MouseUpHandlerProp, RsxChildrenPolicy,
    RsxPropSchema, RsxProps, RsxTag,
};
use crate::{
    AlignItems, BorderRadius, ColorLike, Display, FlowDirection, FlowWrap, FontFamily,
    JustifyContent, Length, Opacity, Padding, ScrollDirection, Style, Transitions,
};

pub struct Element;
pub struct Text;
pub struct TextArea;
pub struct Button;
pub struct Checkbox;
pub struct NumberField;
pub struct Select;
pub struct Slider;
pub struct Switch;

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
    pub font: FontFamily,
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
    pub line_height: f64,
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

pub struct ButtonPropSchema {
    pub label: String,
    pub width: f64,
    pub height: f64,
    pub variant: String,
    pub disabled: bool,
    pub on_click: ClickHandlerProp,
}

pub struct CheckboxPropSchema {
    pub label: String,
    pub checked: bool,
    pub binding: crate::ui::Binding<bool>,
    pub width: f64,
    pub height: f64,
    pub disabled: bool,
}

pub struct NumberFieldPropSchema {
    pub value: f64,
    pub binding: crate::ui::Binding<f64>,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub width: f64,
    pub height: f64,
    pub disabled: bool,
}

pub struct SelectPropSchema {
    pub options: Vec<String>,
    pub selected_index: i64,
    pub binding: crate::ui::Binding<usize>,
    pub width: f64,
    pub height: f64,
    pub disabled: bool,
}

pub struct SliderPropSchema {
    pub value: f64,
    pub binding: crate::ui::Binding<f64>,
    pub min: f64,
    pub max: f64,
    pub width: f64,
    pub height: f64,
    pub disabled: bool,
}

pub struct SwitchPropSchema {
    pub label: String,
    pub checked: bool,
    pub binding: crate::ui::Binding<bool>,
    pub disabled: bool,
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
            "line_height",
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
            node = node.with_prop(
                "binding",
                crate::ui::IntoPropValue::into_prop_value(binding),
            );
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

impl RsxTag for Button {
    fn rsx_render(mut props: RsxProps, _children: Vec<RsxNode>) -> Result<RsxNode, String> {
        let mut node = RsxNode::element("Button");

        for key in ["label", "width", "height", "variant", "disabled", "on_click"] {
            if let Some(value) = props.remove_raw(key) {
                node = node.with_prop(key, value);
            }
        }

        props.reject_remaining("Button")?;
        Ok(node)
    }
}

impl RsxPropSchema for Button {
    type PropsSchema = ButtonPropSchema;
}

impl RsxChildrenPolicy for Button {
    const ACCEPTS_CHILDREN: bool = false;
}

impl RsxTag for Checkbox {
    fn rsx_render(mut props: RsxProps, _children: Vec<RsxNode>) -> Result<RsxNode, String> {
        let mut node = RsxNode::element("Checkbox");

        if let Some(binding) = props.remove_t::<crate::ui::Binding<bool>>("binding")? {
            node = node.with_prop(
                "binding",
                crate::ui::IntoPropValue::into_prop_value(binding),
            );
        }

        for key in ["label", "checked", "binding", "width", "height", "disabled"] {
            if let Some(value) = props.remove_raw(key) {
                node = node.with_prop(key, value);
            }
        }

        props.reject_remaining("Checkbox")?;
        Ok(node)
    }
}

impl RsxPropSchema for Checkbox {
    type PropsSchema = CheckboxPropSchema;
}

impl RsxChildrenPolicy for Checkbox {
    const ACCEPTS_CHILDREN: bool = false;
}

impl RsxTag for NumberField {
    fn rsx_render(mut props: RsxProps, _children: Vec<RsxNode>) -> Result<RsxNode, String> {
        let mut node = RsxNode::element("NumberField");

        if let Some(binding) = props.remove_t::<crate::ui::Binding<f64>>("binding")? {
            node = node.with_prop(
                "binding",
                crate::ui::IntoPropValue::into_prop_value(binding),
            );
        }

        for key in [
            "value", "binding", "min", "max", "step", "width", "height", "disabled",
        ] {
            if let Some(value) = props.remove_raw(key) {
                node = node.with_prop(key, value);
            }
        }

        props.reject_remaining("NumberField")?;
        Ok(node)
    }
}

impl RsxPropSchema for NumberField {
    type PropsSchema = NumberFieldPropSchema;
}

impl RsxChildrenPolicy for NumberField {
    const ACCEPTS_CHILDREN: bool = false;
}

impl RsxTag for Select {
    fn rsx_render(mut props: RsxProps, _children: Vec<RsxNode>) -> Result<RsxNode, String> {
        let mut node = RsxNode::element("Select");

        if let Some(options) = props.remove_t::<Vec<String>>("options")? {
            node = node.with_prop(
                "options",
                crate::ui::IntoPropValue::into_prop_value(options),
            );
        }
        if let Some(binding) = props.remove_t::<crate::ui::Binding<usize>>("binding")? {
            node = node.with_prop(
                "binding",
                crate::ui::IntoPropValue::into_prop_value(binding),
            );
        }

        for key in [
            "options",
            "selected_index",
            "binding",
            "width",
            "height",
            "disabled",
        ] {
            if let Some(value) = props.remove_raw(key) {
                node = node.with_prop(key, value);
            }
        }

        props.reject_remaining("Select")?;
        Ok(node)
    }
}

impl RsxPropSchema for Select {
    type PropsSchema = SelectPropSchema;
}

impl RsxChildrenPolicy for Select {
    const ACCEPTS_CHILDREN: bool = false;
}

impl RsxTag for Slider {
    fn rsx_render(mut props: RsxProps, _children: Vec<RsxNode>) -> Result<RsxNode, String> {
        let mut node = RsxNode::element("Slider");

        if let Some(binding) = props.remove_t::<crate::ui::Binding<f64>>("binding")? {
            node = node.with_prop(
                "binding",
                crate::ui::IntoPropValue::into_prop_value(binding),
            );
        }

        for key in [
            "value", "binding", "min", "max", "width", "height", "disabled",
        ] {
            if let Some(value) = props.remove_raw(key) {
                node = node.with_prop(key, value);
            }
        }

        props.reject_remaining("Slider")?;
        Ok(node)
    }
}

impl RsxPropSchema for Slider {
    type PropsSchema = SliderPropSchema;
}

impl RsxChildrenPolicy for Slider {
    const ACCEPTS_CHILDREN: bool = false;
}

impl RsxTag for Switch {
    fn rsx_render(mut props: RsxProps, _children: Vec<RsxNode>) -> Result<RsxNode, String> {
        let mut node = RsxNode::element("Switch");

        if let Some(binding) = props.remove_t::<crate::ui::Binding<bool>>("binding")? {
            node = node.with_prop(
                "binding",
                crate::ui::IntoPropValue::into_prop_value(binding),
            );
        }

        for key in ["label", "checked", "binding", "disabled"] {
            if let Some(value) = props.remove_raw(key) {
                node = node.with_prop(key, value);
            }
        }

        props.reject_remaining("Switch")?;
        Ok(node)
    }
}

impl RsxPropSchema for Switch {
    type PropsSchema = SwitchPropSchema;
}

impl RsxChildrenPolicy for Switch {
    const ACCEPTS_CHILDREN: bool = false;
}
