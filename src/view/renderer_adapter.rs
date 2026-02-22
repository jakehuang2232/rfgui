use crate::HexColor;
use crate::Style;
use crate::ui::{Binding, FromPropValue, PropValue, RenderBackend, RsxElementNode, RsxNode};
use crate::view::Viewport;
use crate::view::base_component::{Element, ElementTrait, Text, TextArea};
use crate::view::components::{
    build_button, build_checkbox, build_number_field, build_select, build_slider,
    build_switch_with_ids, ButtonProps, ButtonVariant, CheckboxProps, NumberFieldProps,
    SelectProps, SliderProps, SwitchProps,
};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

pub type ElementFactory =
    Arc<dyn Fn(&RsxElementNode, &[usize]) -> Result<Box<dyn ElementTrait>, String> + Send + Sync>;

fn element_factories() -> &'static RwLock<HashMap<String, ElementFactory>> {
    static FACTORIES: OnceLock<RwLock<HashMap<String, ElementFactory>>> = OnceLock::new();
    FACTORIES.get_or_init(|| RwLock::new(HashMap::new()))
}

pub fn register_element_factory(tag: impl Into<String>, factory: ElementFactory) {
    if let Ok(mut map) = element_factories().write() {
        map.insert(tag.into(), factory);
    }
}

pub fn rsx_to_element(root: &RsxNode) -> Result<Box<dyn ElementTrait>, String> {
    let mut nodes = rsx_to_elements(root)?;
    if nodes.len() != 1 {
        return Err("expected single root element".to_string());
    }
    Ok(nodes.remove(0))
}

pub fn rsx_to_elements(root: &RsxNode) -> Result<Vec<Box<dyn ElementTrait>>, String> {
    let mut out = Vec::new();
    append_nodes(root, &mut out)?;
    Ok(out)
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

    fn request_redraw(&mut self) -> Result<(), String> {
        self.viewport.request_redraw();
        Ok(())
    }
}

fn append_nodes(node: &RsxNode, out: &mut Vec<Box<dyn ElementTrait>>) -> Result<(), String> {
    let mut path = Vec::new();
    append_nodes_with_path(node, out, &mut path)
}

fn append_nodes_with_path(
    node: &RsxNode,
    out: &mut Vec<Box<dyn ElementTrait>>,
    path: &mut Vec<usize>,
) -> Result<(), String> {
    match node {
        RsxNode::Fragment(children) => {
            for (index, child) in children.iter().enumerate() {
                path.push(index);
                append_nodes_with_path(child, out, path)?;
                path.pop();
            }
            Ok(())
        }
        _ => {
            out.push(convert_node(node, path)?);
            Ok(())
        }
    }
}

fn convert_node(node: &RsxNode, path: &[usize]) -> Result<Box<dyn ElementTrait>, String> {
    match node {
        RsxNode::Text(text) => Ok(Box::new(Text::from_content_with_id(
            stable_node_id(path, "TextNode"),
            text.clone(),
        ))),
        RsxNode::Fragment(_) => Err("fragment must be flattened before conversion".to_string()),
        RsxNode::Element(el) => convert_element(el, path),
    }
}

fn convert_element(node: &RsxElementNode, path: &[usize]) -> Result<Box<dyn ElementTrait>, String> {
    match node.tag.as_str() {
        "Text" => convert_text_element(node, path),
        "TextArea" => convert_text_area_element(node, path),
        "Button" => convert_button_element(node),
        "Checkbox" => convert_checkbox_element(node),
        "NumberField" => convert_number_field_element(node),
        "Select" => convert_select_element(node),
        "Slider" => convert_slider_element(node),
        "Switch" => convert_switch_element(node, path),
        _ => {
            if let Ok(map) = element_factories().read() {
                if let Some(factory) = map.get(&node.tag) {
                    return factory(node, path);
                }
            }
            convert_container_element(node, path)
        }
    }
}

fn convert_container_element(node: &RsxElementNode, path: &[usize]) -> Result<Box<dyn ElementTrait>, String> {
    let mut element = Element::new_with_id(
        stable_node_id(path, node.tag.as_str()),
        0.0,
        0.0,
        10_000.0,
        10_000.0,
    );

    for (key, value) in &node.props {
        match key.as_str() {
            "padding" => element.set_padding(as_f32(value, key)?),
            "padding_x" => element.set_padding_x(as_f32(value, key)?),
            "padding_y" => element.set_padding_y(as_f32(value, key)?),
            "padding_left" => element.set_padding_left(as_f32(value, key)?),
            "padding_right" => element.set_padding_right(as_f32(value, key)?),
            "padding_top" => element.set_padding_top(as_f32(value, key)?),
            "padding_bottom" => element.set_padding_bottom(as_f32(value, key)?),
            "opacity" => element.set_opacity(as_f32(value, key)?),
            "style" => element.apply_style(as_style(value, key)?),
            "on_mouse_down" => {
                let handler = as_mouse_down_handler(value, key)?;
                element.on_mouse_down(move |event, _control| handler.call(event));
            }
            "on_mouse_up" => {
                let handler = as_mouse_up_handler(value, key)?;
                element.on_mouse_up(move |event, _control| handler.call(event));
            }
            "on_mouse_move" => {
                let handler = as_mouse_move_handler(value, key)?;
                element.on_mouse_move(move |event, _control| handler.call(event));
            }
            "on_click" => {
                let handler = as_click_handler(value, key)?;
                element.on_click(move |event, _control| handler.call(event));
            }
            "on_key_down" => {
                let handler = as_key_down_handler(value, key)?;
                element.on_key_down(move |event, _control| handler.call(event));
            }
            "on_key_up" => {
                let handler = as_key_up_handler(value, key)?;
                element.on_key_up(move |event, _control| handler.call(event));
            }
            "on_focus" => {
                let handler = as_focus_handler(value, key)?;
                element.on_focus(move |event, _control| handler.call(event));
            }
            "on_blur" => {
                let handler = as_blur_handler(value, key)?;
                element.on_blur(move |event, _control| handler.call(event));
            }
            _ => return Err(format!("unknown prop `{}` on <{}>", key, node.tag)),
        }
    }

    for (index, child) in node.children.iter().enumerate() {
        let mut child_path = path.to_vec();
        child_path.push(index);
        element.add_child(convert_node(child, &child_path)?);
    }

    Ok(Box::new(element))
}

fn convert_text_element(node: &RsxElementNode, path: &[usize]) -> Result<Box<dyn ElementTrait>, String> {
    let mut text_content = String::new();
    let mut text = Text::from_content_with_id(stable_node_id(path, "Text"), "");
    let mut x: Option<f32> = None;
    let mut y: Option<f32> = None;
    let mut width: Option<f32> = None;
    let mut height: Option<f32> = None;

    for (key, value) in &node.props {
        match key.as_str() {
            "content" => {
                text_content = as_owned_string(value, key)?;
            }
            "x" => {
                x = Some(as_f32(value, key)?);
            }
            "y" => {
                y = Some(as_f32(value, key)?);
            }
            "width" => {
                width = Some(as_f32(value, key)?);
            }
            "height" => {
                height = Some(as_f32(value, key)?);
            }
            "font_size" => text.set_font_size(as_f32(value, key)?),
            "font" => text.set_font(as_string(value, key)?),
            "color" => text.set_color(HexColor::new(as_owned_string(value, key)?)),
            "opacity" => text.set_opacity(as_f32(value, key)?),
            _ => return Err(format!("unknown prop `{}` on <Text>", key,)),
        }
    }

    text.set_position(x.unwrap_or(0.0), y.unwrap_or(0.0));
    if let Some(width) = width {
        text.set_width(width);
    } else {
        text.set_auto_width(true);
    }
    if let Some(height) = height {
        text.set_height(height);
    } else {
        text.set_auto_height(true);
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

fn convert_text_area_element(
    node: &RsxElementNode,
    path: &[usize],
) -> Result<Box<dyn ElementTrait>, String> {
    let mut text_content = String::new();
    let mut placeholder = String::new();
    let mut binding: Option<Binding<String>> = None;
    let mut text_area = TextArea::from_content_with_id(stable_node_id(path, "TextArea"), "");
    let mut x: Option<f32> = None;
    let mut y: Option<f32> = None;
    let mut width: Option<f32> = None;
    let mut height: Option<f32> = None;

    for (key, value) in &node.props {
        match key.as_str() {
            "content" => {
                text_content = as_owned_string(value, key)?;
            }
            "placeholder" => {
                placeholder = as_owned_string(value, key)?;
            }
            "binding" => {
                binding = Some(as_binding_string(value, key)?);
            }
            "x" => {
                x = Some(as_f32(value, key)?);
            }
            "y" => {
                y = Some(as_f32(value, key)?);
            }
            "width" => {
                width = Some(as_f32(value, key)?);
            }
            "height" => {
                height = Some(as_f32(value, key)?);
            }
            "font_size" => text_area.set_font_size(as_f32(value, key)?),
            "font" => text_area.set_font(as_string(value, key)?),
            "color" => text_area.set_color(HexColor::new(as_owned_string(value, key)?)),
            "opacity" => text_area.set_opacity(as_f32(value, key)?),
            "multiline" => text_area.set_multiline(as_bool(value, key)?),
            "read_only" => text_area.set_read_only(as_bool(value, key)?),
            "max_length" => text_area.set_max_length(as_usize(value, key)?),
            _ => return Err(format!("unknown prop `{}` on <TextArea>", key,)),
        }
    }

    text_area.set_position(x.unwrap_or(0.0), y.unwrap_or(0.0));
    if let Some(width) = width {
        text_area.set_width(width);
    } else {
        text_area.set_auto_width(true);
    }
    if let Some(height) = height {
        text_area.set_height(height);
    } else {
        text_area.set_auto_height(true);
    }

    if binding.is_none() {
        if text_content.is_empty() {
            for child in &node.children {
                match child {
                    RsxNode::Text(content) => text_content.push_str(content),
                    _ => return Err("<TextArea> children must be text".to_string()),
                }
            }
        }
        text_area.set_text(text_content);
    } else if let Some(bound) = binding.as_ref() {
        text_area.set_text(bound.get());
    }

    if let Some(bound) = binding {
        text_area.bind_text(bound);
    }
    if !placeholder.is_empty() {
        text_area.set_placeholder(placeholder);
    }
    Ok(Box::new(text_area))
}

fn convert_button_element(node: &RsxElementNode) -> Result<Box<dyn ElementTrait>, String> {
    let mut props = ButtonProps::new("");

    for (key, value) in &node.props {
        match key.as_str() {
            "label" => props.label = as_owned_string(value, key)?,
            "width" => props.width = as_f32(value, key)?,
            "height" => props.height = as_f32(value, key)?,
            "variant" => {
                props.variant = match as_string(value, key)?.to_ascii_lowercase().as_str() {
                    "contained" => ButtonVariant::Contained,
                    "outlined" => ButtonVariant::Outlined,
                    "text" => ButtonVariant::Text,
                    other => return Err(format!("unknown Button variant `{other}`")),
                }
            }
            "disabled" => props.disabled = as_bool(value, key)?,
            _ => return Err(format!("unknown prop `{}` on <Button>", key)),
        }
    }

    Ok(Box::new(build_button(props)))
}

fn convert_checkbox_element(node: &RsxElementNode) -> Result<Box<dyn ElementTrait>, String> {
    let mut props = CheckboxProps::new("");

    for (key, value) in &node.props {
        match key.as_str() {
            "label" => props.label = as_owned_string(value, key)?,
            "checked" => props.checked = as_bool(value, key)?,
            "binding" => props.checked_binding = Some(as_binding_bool(value, key)?),
            "width" => props.width = as_f32(value, key)?,
            "height" => props.height = as_f32(value, key)?,
            "disabled" => props.disabled = as_bool(value, key)?,
            _ => return Err(format!("unknown prop `{}` on <Checkbox>", key)),
        }
    }

    Ok(Box::new(build_checkbox(props)))
}

fn convert_number_field_element(node: &RsxElementNode) -> Result<Box<dyn ElementTrait>, String> {
    let mut props = NumberFieldProps::new();

    for (key, value) in &node.props {
        match key.as_str() {
            "value" => props.value = as_f64(value, key)?,
            "binding" => props.value_binding = Some(as_binding_f64(value, key)?),
            "min" => props.min = Some(as_f64(value, key)?),
            "max" => props.max = Some(as_f64(value, key)?),
            "step" => props.step = as_f64(value, key)?,
            "width" => props.width = as_f32(value, key)?,
            "height" => props.height = as_f32(value, key)?,
            "disabled" => props.disabled = as_bool(value, key)?,
            _ => return Err(format!("unknown prop `{}` on <NumberField>", key)),
        }
    }

    Ok(Box::new(build_number_field(props)))
}

fn convert_select_element(node: &RsxElementNode) -> Result<Box<dyn ElementTrait>, String> {
    let mut props = SelectProps::new(Vec::new());

    for (key, value) in &node.props {
        match key.as_str() {
            "options" => props.options = as_vec_string(value, key)?,
            "selected_index" => props.selected_index = as_usize_raw(value, key)?,
            "binding" => props.selected_binding = Some(as_binding_usize(value, key)?),
            "width" => props.width = as_f32(value, key)?,
            "height" => props.height = as_f32(value, key)?,
            "disabled" => props.disabled = as_bool(value, key)?,
            _ => return Err(format!("unknown prop `{}` on <Select>", key)),
        }
    }

    Ok(Box::new(build_select(props)))
}

fn convert_slider_element(node: &RsxElementNode) -> Result<Box<dyn ElementTrait>, String> {
    let mut props = SliderProps::new();

    for (key, value) in &node.props {
        match key.as_str() {
            "value" => props.value = as_f64(value, key)?,
            "binding" => props.value_binding = Some(as_binding_f64(value, key)?),
            "min" => props.min = as_f64(value, key)?,
            "max" => props.max = as_f64(value, key)?,
            "width" => props.width = as_f32(value, key)?,
            "height" => props.height = as_f32(value, key)?,
            "disabled" => props.disabled = as_bool(value, key)?,
            _ => return Err(format!("unknown prop `{}` on <Slider>", key)),
        }
    }

    Ok(Box::new(build_slider(props)))
}

fn convert_switch_element(node: &RsxElementNode, path: &[usize]) -> Result<Box<dyn ElementTrait>, String> {
    let mut props = SwitchProps::new("");

    for (key, value) in &node.props {
        match key.as_str() {
            "label" => props.label = as_owned_string(value, key)?,
            "checked" => props.checked = as_bool(value, key)?,
            "binding" => props.checked_binding = Some(as_binding_bool(value, key)?),
            "disabled" => props.disabled = as_bool(value, key)?,
            _ => return Err(format!("unknown prop `{}` on <Switch>", key)),
        }
    }

    Ok(Box::new(build_switch_with_ids(
        props,
        stable_node_id(path, "Switch"),
        stable_node_id(path, "SwitchTrack"),
        stable_node_id(path, "SwitchThumb"),
        stable_node_id(path, "SwitchLabel"),
    )))
}

fn stable_node_id(path: &[usize], kind: &str) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for &byte in kind.as_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    hash ^= 0xff;
    hash = hash.wrapping_mul(FNV_PRIME);

    for &index in path {
        for byte in (index as u64).to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash ^= 0xfe;
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    if hash == 0 {
        1
    } else {
        hash
    }
}

fn as_f32(value: &PropValue, key: &str) -> Result<f32, String> {
    match value {
        PropValue::I64(v) => Ok(*v as f32),
        PropValue::F64(v) => Ok(*v as f32),
        _ => Err(format!("prop `{key}` expects numeric value")),
    }
}

fn as_f64(value: &PropValue, key: &str) -> Result<f64, String> {
    match value {
        PropValue::I64(v) => Ok(*v as f64),
        PropValue::F64(v) => Ok(*v),
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

fn as_binding_string(value: &PropValue, key: &str) -> Result<Binding<String>, String> {
    Binding::<String>::from_prop_value(value.clone())
        .map_err(|_| format!("prop `{key}` expects Binding<String> value"))
}

fn as_binding_bool(value: &PropValue, key: &str) -> Result<Binding<bool>, String> {
    Binding::<bool>::from_prop_value(value.clone())
        .map_err(|_| format!("prop `{key}` expects Binding<bool> value"))
}

fn as_binding_f64(value: &PropValue, key: &str) -> Result<Binding<f64>, String> {
    Binding::<f64>::from_prop_value(value.clone())
        .map_err(|_| format!("prop `{key}` expects Binding<f64> value"))
}

fn as_binding_usize(value: &PropValue, key: &str) -> Result<Binding<usize>, String> {
    Binding::<usize>::from_prop_value(value.clone())
        .map_err(|_| format!("prop `{key}` expects Binding<usize> value"))
}

fn as_bool(value: &PropValue, key: &str) -> Result<bool, String> {
    match value {
        PropValue::Bool(v) => Ok(*v),
        _ => Err(format!("prop `{key}` expects bool value")),
    }
}

fn as_usize(value: &PropValue, key: &str) -> Result<Option<usize>, String> {
    match value {
        PropValue::I64(v) => {
            if *v < 0 {
                Err(format!("prop `{key}` expects non-negative integer value"))
            } else {
                Ok(Some(*v as usize))
            }
        }
        PropValue::F64(v) => {
            if *v < 0.0 {
                Err(format!("prop `{key}` expects non-negative numeric value"))
            } else {
                Ok(Some(*v as usize))
            }
        }
        _ => Err(format!("prop `{key}` expects numeric value")),
    }
}

fn as_usize_raw(value: &PropValue, key: &str) -> Result<usize, String> {
    match value {
        PropValue::I64(v) => {
            if *v < 0 {
                Err(format!("prop `{key}` expects non-negative integer value"))
            } else {
                Ok(*v as usize)
            }
        }
        PropValue::F64(v) => {
            if *v < 0.0 {
                Err(format!("prop `{key}` expects non-negative numeric value"))
            } else {
                Ok(*v as usize)
            }
        }
        _ => Err(format!("prop `{key}` expects numeric value")),
    }
}

fn as_vec_string(value: &PropValue, key: &str) -> Result<Vec<String>, String> {
    Vec::<String>::from_prop_value(value.clone())
        .map_err(|_| format!("prop `{key}` expects Vec<String> value"))
}

fn as_style(value: &PropValue, key: &str) -> Result<Style, String> {
    match value {
        PropValue::Style(style) => Ok(style.clone()),
        _ => Err(format!("prop `{key}` expects style value")),
    }
}

fn as_mouse_down_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::MouseDownHandlerProp, String> {
    match value {
        PropValue::OnMouseDown(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects mouse down handler value")),
    }
}

fn as_mouse_up_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::MouseUpHandlerProp, String> {
    match value {
        PropValue::OnMouseUp(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects mouse up handler value")),
    }
}

fn as_mouse_move_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::MouseMoveHandlerProp, String> {
    match value {
        PropValue::OnMouseMove(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects mouse move handler value")),
    }
}

fn as_click_handler(value: &PropValue, key: &str) -> Result<crate::ui::ClickHandlerProp, String> {
    match value {
        PropValue::OnClick(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects click handler value")),
    }
}

fn as_key_down_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::KeyDownHandlerProp, String> {
    match value {
        PropValue::OnKeyDown(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects key down handler value")),
    }
}

fn as_key_up_handler(value: &PropValue, key: &str) -> Result<crate::ui::KeyUpHandlerProp, String> {
    match value {
        PropValue::OnKeyUp(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects key up handler value")),
    }
}

fn as_focus_handler(value: &PropValue, key: &str) -> Result<crate::ui::FocusHandlerProp, String> {
    match value {
        PropValue::OnFocus(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects focus handler value")),
    }
}

fn as_blur_handler(value: &PropValue, key: &str) -> Result<crate::ui::BlurHandlerProp, String> {
    match value {
        PropValue::OnBlur(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects blur handler value")),
    }
}

#[cfg(test)]
mod tests {
    use super::rsx_to_elements;
    use crate::ui::RsxNode;
    use crate::{Border, BorderRadius, Color, IntoColor, Length, Style, ParsedValue, PropertyId, Unit};

    fn style_bg(hex: &str) -> Style {
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::Color(IntoColor::<Color>::into_color(Color::hex(hex))),
        );
        style
    }

    fn style_bg_border(bg_hex: &str, border_hex: &str, border_width: f32) -> Style {
        let mut style = style_bg(bg_hex);
        style.set_border(Border::uniform(
            Length::px(border_width),
            &Color::hex(border_hex),
        ));
        style
    }

    fn style_with_radius(mut style: Style, radius: f32) -> Style {
        style.set_border_radius(BorderRadius::uniform(Unit::px(radius)));
        style
    }

    fn style_with_size(mut style: Style, width: f32, height: f32) -> Style {
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
        style
    }

    fn walk_layout(
        node: &mut dyn crate::view::base_component::ElementTrait,
        out: &mut Vec<(f32, f32, f32, f32)>,
    ) {
        let s = node.box_model_snapshot();
        out.push((s.x, s.y, s.width, s.height));
        if let Some(children) = node.children_mut() {
            for child in children {
                walk_layout(child.as_mut(), out);
            }
        }
    }

    #[test]
    fn text_nodes_keep_expected_layout_bounds_in_scene() {
        let first_panel = RsxNode::element("Element")
            .with_prop(
                "style",
                style_with_size(
                    style_with_radius(style_bg_border("#4CC9F0", "#1D3557", 8.0), 10.0),
                    240.0,
                    140.0,
                ),
            )
            .with_child(
                RsxNode::element("Element")
                    .with_prop(
                        "style",
                        style_with_size(style_bg_border("#FFD166", "#EF476F", 3.0), 72.0, 48.0),
                    ),
            )
            .with_child(
                RsxNode::element("Element")
                    .with_prop(
                        "style",
                        style_with_size(style_bg_border("#F72585", "#B5179E", 4.0), 120.0, 80.0),
                    ),
            )
            .with_child(
                RsxNode::element("Text")
                    .with_prop("x", 8)
                    .with_prop("y", 4)
                    .with_prop("font_size", 26)
                    .with_prop("color", "#0F172A")
                    .with_prop("font", "Noto Sans CJK TC")
                    .with_child(RsxNode::text("Hello Rust GUI Text Test")),
            );

        let second_panel = RsxNode::element("Element")
            .with_prop(
                "style",
                style_with_size(
                    style_with_radius(style_bg_border("#1E293B", "#38BDF8", 3.0), 16.0),
                    240.0,
                    180.0,
                ),
            )
            .with_child(
                RsxNode::element("Text")
                    .with_prop("x", 16)
                    .with_prop("y", 16)
                    .with_prop("font_size", 22)
                    .with_prop("color", "#E2E8F0")
                    .with_prop("font", "Noto Sans CJK TC")
                    .with_child(RsxNode::text("Test Component")),
            )
            .with_child(
                RsxNode::element("Text")
                    .with_prop("x", 16)
                    .with_prop("y", 56)
                    .with_prop("font_size", 14)
                    .with_prop("color", "#CBD5E1")
                    .with_prop("font", "Noto Sans CJK TC")
                    .with_child(RsxNode::text("Used to verify event hit-testing and bubbling.")),
            )
            .with_child(
                RsxNode::element("Text")
                    .with_prop("x", 16)
                    .with_prop("y", 80)
                    .with_prop("font_size", 14)
                    .with_prop("color", "#F8FAFC")
                    .with_prop("font", "Noto Sans CJK TC")
                    .with_child(RsxNode::text("Click Count: 0")),
            );

        let tree = RsxNode::fragment(vec![first_panel, second_panel]);

        let mut roots = rsx_to_elements(&tree).expect("convert rsx");
        for root in &mut roots {
            root.measure(crate::view::base_component::LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            });
            root.place(crate::view::base_component::LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 800.0,
                available_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            });
        }

        let mut boxes = Vec::new();
        for root in &mut roots {
            walk_layout(root.as_mut(), &mut boxes);
        }

        assert!(
            boxes
                .iter()
                .any(|&(x, y, w, h)| x == 16.0 && y == 12.0 && w == 216.0 && h == 65.0)
        );
        assert!(
            boxes
                .iter()
                .any(|&(x, y, w, h)| x == 19.0 && y == 19.0 && w == 88.0 && h == 27.5)
        );
        assert!(
            boxes
                .iter()
                .any(|&(x, y, w, h)| (x - 19.0).abs() < 0.1 && (y - 59.0).abs() < 0.1 && (w - 191.66).abs() < 0.1 && (h - 17.5).abs() < 0.1)
        );
        assert!(
            boxes
                .iter()
                .any(|&(x, y, w, h)| (x - 19.0).abs() < 0.1 && (y - 83.0).abs() < 0.1 && (w - 92.12).abs() < 0.1 && (h - 17.5).abs() < 0.1)
        );
    }

    #[test]
    fn element_padding_offsets_child_layout() {
        let tree = RsxNode::element("Element")
            .with_prop(
                "style",
                style_with_size(Style::new(), 200.0, 120.0),
            )
            .with_prop("padding_left", 8)
            .with_prop("padding_top", 12)
            .with_prop("padding_right", 16)
            .with_prop("padding_bottom", 10)
            .with_child(
                RsxNode::element("Text")
                    .with_prop("x", 4)
                    .with_prop("y", 6)
                    .with_prop("content", "inner")
                    .with_prop("width", 300)
                    .with_prop("height", 300),
            );

        let mut roots = rsx_to_elements(&tree).expect("convert rsx");
        for root in &mut roots {
            root.measure(crate::view::base_component::LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            });
            root.place(crate::view::base_component::LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 800.0,
                available_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            });
        }

        let mut boxes = Vec::new();
        for root in &mut roots {
            walk_layout(root.as_mut(), &mut boxes);
        }

        assert!(
            boxes
                .iter()
                .any(|&(x, y, w, h)| x == 0.0 && y == 0.0 && w == 200.0 && h == 120.0)
        );
        assert!(
            boxes
                .iter()
                .any(|&(x, y, w, h)| x == 12.0 && y == 18.0 && w == 172.0 && h == 92.0)
        );
    }
}
