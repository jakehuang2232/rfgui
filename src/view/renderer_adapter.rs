use crate::ui::{PropValue, RenderBackend, RsxElementNode, RsxNode};
use crate::view::base_component::{Element, ElementTrait, Text};
use crate::view::Viewport;
use crate::HexColor;

pub fn rsx_to_element(root: &RsxNode) -> Result<Box<dyn ElementTrait>, String> {
    convert_node(root)
}

pub struct ViewportRenderBackend<'a> {
    viewport: &'a mut Viewport,
    current_root: Option<RsxNode>,
}

impl<'a> ViewportRenderBackend<'a> {
    pub fn new(viewport: &'a mut Viewport) -> Self {
        Self {
            viewport,
            current_root: None,
        }
    }

    fn root_mut(&mut self, root: u64) -> Result<&mut RsxNode, String> {
        if root != 0 {
            return Err(format!("invalid root id: {root}"));
        }
        self.current_root
            .as_mut()
            .ok_or_else(|| "root is not initialized".to_string())
    }
}

impl<'a> RenderBackend for ViewportRenderBackend<'a> {
    type NodeId = u64;

    fn create_root(&mut self, node: &RsxNode) -> Result<Self::NodeId, String> {
        self.current_root = Some(node.clone());
        Ok(0)
    }

    fn replace_root(&mut self, root: Self::NodeId, node: &RsxNode) -> Result<(), String> {
        if root != 0 {
            return Err(format!("invalid root id: {root}"));
        }
        self.current_root = Some(node.clone());
        Ok(())
    }

    fn update_root_props(
        &mut self,
        root: Self::NodeId,
        props: &[(String, PropValue)],
    ) -> Result<(), String> {
        let root_node = self.root_mut(root)?;
        let RsxNode::Element(element) = root_node else {
            return Err("cannot update props on non-element root".to_string());
        };
        element.props = props.to_vec();
        Ok(())
    }

    fn replace_root_children(
        &mut self,
        root: Self::NodeId,
        children: &[RsxNode],
    ) -> Result<(), String> {
        let root_node = self.root_mut(root)?;
        match root_node {
            RsxNode::Element(element) => {
                element.children = children.to_vec();
            }
            RsxNode::Fragment(fragment_children) => {
                *fragment_children = children.to_vec();
            }
            RsxNode::Text(_) => {
                return Err("cannot replace children on text root".to_string());
            }
        }
        Ok(())
    }

    fn draw_frame(&mut self) -> Result<(), String> {
        let Some(root) = self.current_root.as_ref() else {
            return Ok(());
        };
        self.viewport.render_rsx(root)
    }
}

fn convert_node(node: &RsxNode) -> Result<Box<dyn ElementTrait>, String> {
    match node {
        RsxNode::Text(text) => Ok(Box::new(Text::from_content(text.clone()))),
        RsxNode::Fragment(children) => {
            let mut container = Element::default();
            for child in children {
                container.add_child(convert_node(child)?);
            }
            Ok(Box::new(container))
        }
        RsxNode::Element(el) => convert_element(el),
    }
}

fn convert_element(node: &RsxElementNode) -> Result<Box<dyn ElementTrait>, String> {
    match node.tag.as_str() {
        "Text" => convert_text_element(node),
        _ => convert_container_element(node),
    }
}

fn convert_container_element(node: &RsxElementNode) -> Result<Box<dyn ElementTrait>, String> {
    let mut element = Element::default();

    for (key, value) in &node.props {
        match key.as_str() {
            "x" => element.set_x(as_f32(value, key)?),
            "y" => element.set_y(as_f32(value, key)?),
            "width" => element.set_width(as_f32(value, key)?),
            "height" => element.set_height(as_f32(value, key)?),
            "opacity" => element.set_opacity(as_f32(value, key)?),
            "border_width" => element.set_border_width(as_f32(value, key)?),
            "border_radius" => element.set_border_radius(as_f32(value, key)?),
            "background" | "background_color" => {
                element.set_background_color(HexColor::new(as_owned_string(value, key)?))
            }
            "border_color" => element.set_border_color(HexColor::new(as_owned_string(value, key)?)),
            _ => {
                return Err(format!(
                    "unknown prop `{}` on <{}>",
                    key,
                    node.tag
                ))
            }
        }
    }

    for child in &node.children {
        element.add_child(convert_node(child)?);
    }

    Ok(Box::new(element))
}

fn convert_text_element(node: &RsxElementNode) -> Result<Box<dyn ElementTrait>, String> {
    let mut text_content = String::new();
    let mut text = Text::from_content("");

    for (key, value) in &node.props {
        match key.as_str() {
            "content" => {
                text_content = as_owned_string(value, key)?;
            }
            "x" => {
                let y = 0.0;
                text.set_position(as_f32(value, key)?, y);
            }
            "y" => {
                let x = 0.0;
                text.set_position(x, as_f32(value, key)?);
            }
            "width" => {
                let h = 10_000.0;
                text.set_size(as_f32(value, key)?, h);
            }
            "height" => {
                let w = 10_000.0;
                text.set_size(w, as_f32(value, key)?);
            }
            "font_size" => text.set_font_size(as_f32(value, key)?),
            "font" => text.set_font(as_string(value, key)?),
            "opacity" => text.set_opacity(as_f32(value, key)?),
            _ => {
                return Err(format!(
                    "unknown prop `{}` on <Text>",
                    key,
                ))
            }
        }
    }

    if text_content.is_empty() {
        for child in &node.children {
            match child {
                RsxNode::Text(content) => text_content.push_str(content),
                _ => return Err("<Text> children must be text".to_string()),
            }
        }
    }

    text.set_text(text_content);
    Ok(Box::new(text))
}

fn as_f32(value: &PropValue, key: &str) -> Result<f32, String> {
    match value {
        PropValue::I64(v) => Ok(*v as f32),
        PropValue::F64(v) => Ok(*v as f32),
        _ => Err(format!("prop `{key}` expects numeric value")),
    }
}

fn as_string<'a>(value: &'a PropValue, key: &str) -> Result<&'a str, String> {
    match value {
        PropValue::String(v) => Ok(v.as_str()),
        _ => Err(format!("prop `{key}` expects string value")),
    }
}

fn as_owned_string(value: &PropValue, key: &str) -> Result<String, String> {
    Ok(as_string(value, key)?.to_string())
}
