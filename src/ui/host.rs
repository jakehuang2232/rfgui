use crate::ui::{RsxChildrenPolicy, RsxNode, RsxPropSchema, RsxProps, RsxTag};

pub struct Element;
pub struct Text;

pub struct ElementPropSchema {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub opacity: f64,
    pub border_width: f64,
    pub border_radius: f64,
    pub background: String,
    pub background_color: String,
    pub border_color: String,
    pub children: Vec<RsxNode>,
}

pub struct TextPropSchema {
    pub content: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub font_size: f64,
    pub font: String,
    pub opacity: f64,
    pub children: Vec<RsxNode>,
}

impl RsxTag for Element {
    fn rsx_render(mut props: RsxProps, children: Vec<RsxNode>) -> Result<RsxNode, String> {
        let mut node = RsxNode::element("Element");

        for key in [
            "x",
            "y",
            "width",
            "height",
            "opacity",
            "border_width",
            "border_radius",
            "background",
            "background_color",
            "border_color",
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

        for key in ["x", "y", "width", "height", "font_size", "font", "opacity"] {
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
