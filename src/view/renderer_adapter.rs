use crate::HexColor;
use crate::Style;
use crate::ui::{Binding, FromPropValue, PropValue, RenderBackend, RsxElementNode, RsxNode};
use crate::view::Viewport;
use crate::view::base_component::{Element, ElementTrait, Text, TextArea};
use crate::view::components::{
    ButtonProps, ButtonVariant, CheckboxProps, NumberFieldProps, SelectProps, SliderProps,
    SwitchProps, build_button_rsx, build_checkbox_rsx, build_number_field_with_ids,
    build_select_with_ids, build_slider_with_ids, build_switch_rsx,
};
use crate::{ParsedValue, PropertyId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, OnceLock, RwLock};

pub type ElementFactory =
    Arc<dyn Fn(&RsxElementNode, &[u64]) -> Result<Box<dyn ElementTrait>, String> + Send + Sync>;

thread_local! {
    static UNCONTROLLED_SWITCH_BINDINGS: RefCell<HashMap<u64, Binding<bool>>> =
        RefCell::new(HashMap::new());
}

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

#[derive(Clone, Debug, Default)]
struct InheritedTextStyle {
    font_families: Vec<String>,
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
    append_nodes_with_path(node, out, &mut path, &InheritedTextStyle::default())
}

fn append_nodes_with_path(
    node: &RsxNode,
    out: &mut Vec<Box<dyn ElementTrait>>,
    path: &mut Vec<u64>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<(), String> {
    match node {
        RsxNode::Fragment(children) => {
            for (index, child) in children.iter().enumerate() {
                path.push(child_identity_token(child, index));
                append_nodes_with_path(child, out, path, inherited_text_style)?;
                path.pop();
            }
            Ok(())
        }
        _ => {
            out.push(convert_node(node, path, inherited_text_style)?);
            Ok(())
        }
    }
}

fn convert_node(
    node: &RsxNode,
    path: &[u64],
    inherited_text_style: &InheritedTextStyle,
) -> Result<Box<dyn ElementTrait>, String> {
    match node {
        RsxNode::Text(text) => {
            let mut text_node =
                Text::from_content_with_id(stable_node_id(path, "TextNode"), text.clone());
            if !inherited_text_style.font_families.is_empty() {
                text_node.set_fonts(inherited_text_style.font_families.clone());
            }
            Ok(Box::new(text_node))
        }
        RsxNode::Fragment(_) => Err("fragment must be flattened before conversion".to_string()),
        RsxNode::Element(el) => convert_element(el, path, inherited_text_style),
    }
}

fn convert_element(
    node: &RsxElementNode,
    path: &[u64],
    inherited_text_style: &InheritedTextStyle,
) -> Result<Box<dyn ElementTrait>, String> {
    match node.tag.as_str() {
        "Text" => convert_text_element(node, path, inherited_text_style),
        "TextArea" => convert_text_area_element(node, path, inherited_text_style),
        "Button" => convert_button_element(node, path, inherited_text_style),
        "Checkbox" => convert_checkbox_element(node, path),
        "NumberField" => convert_number_field_element(node, path),
        "Select" => convert_select_element(node, path),
        "Slider" => convert_slider_element(node, path),
        "Switch" => convert_switch_element(node, path, inherited_text_style),
        _ => {
            if let Ok(map) = element_factories().read() {
                if let Some(factory) = map.get(&node.tag) {
                    return factory(node, path);
                }
            }
            convert_container_element(node, path, inherited_text_style)
        }
    }
}

fn convert_container_element(
    node: &RsxElementNode,
    path: &[u64],
    inherited_text_style: &InheritedTextStyle,
) -> Result<Box<dyn ElementTrait>, String> {
    let mut element = Element::new_with_id(
        stable_node_id(path, node.tag.as_str()),
        0.0,
        0.0,
        10_000.0,
        10_000.0,
    );
    let mut base_style = Style::new();
    base_style.insert(PropertyId::Width, ParsedValue::Auto);
    base_style.insert(PropertyId::Height, ParsedValue::Auto);

    let mut child_inherited_text_style = inherited_text_style.clone();
    let mut user_style = Style::new();
    let mut has_user_style = false;
    for (key, value) in &node.props {
        if key.as_str() == "style" {
            let style = as_style(value, key)?;
            if let Some(ParsedValue::FontFamily(font_family)) = style.get(PropertyId::FontFamily) {
                child_inherited_text_style.font_families = font_family.as_slice().to_vec();
            }
            user_style = user_style + style;
            has_user_style = true;
        }
    }
    let effective_style = if has_user_style {
        base_style + user_style
    } else {
        base_style
    };
    element.apply_style(effective_style);

    for (key, value) in &node.props {
        if key.as_str() == "key" {
            continue;
        }
        match key.as_str() {
            "padding" => element.set_padding(as_f32(value, key)?),
            "padding_x" => element.set_padding_x(as_f32(value, key)?),
            "padding_y" => element.set_padding_y(as_f32(value, key)?),
            "padding_left" => element.set_padding_left(as_f32(value, key)?),
            "padding_right" => element.set_padding_right(as_f32(value, key)?),
            "padding_top" => element.set_padding_top(as_f32(value, key)?),
            "padding_bottom" => element.set_padding_bottom(as_f32(value, key)?),
            "opacity" => element.set_opacity(as_f32(value, key)?),
            "style" => {
                // Style has already been merged and applied once to avoid initial mount transitions.
            }
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

    // Reuse a single path buffer for all children to avoid per-child path allocations.
    let mut child_path = Vec::with_capacity(path.len().saturating_add(1));
    child_path.extend_from_slice(path);
    for (index, child) in node.children.iter().enumerate() {
        child_path.push(child_identity_token(child, index));
        element.add_child(convert_node(
            child,
            &child_path,
            &child_inherited_text_style,
        )?);
        child_path.pop();
    }

    Ok(Box::new(element))
}

fn convert_text_element(
    node: &RsxElementNode,
    path: &[u64],
    inherited_text_style: &InheritedTextStyle,
) -> Result<Box<dyn ElementTrait>, String> {
    let mut text_content = String::new();
    let mut text = Text::from_content_with_id(stable_node_id(path, "Text"), "");
    let mut x: Option<f32> = None;
    let mut y: Option<f32> = None;
    let mut width: Option<f32> = None;
    let mut height: Option<f32> = None;
    let mut has_explicit_font = false;

    for (key, value) in &node.props {
        if key.as_str() == "key" {
            continue;
        }
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
            "line_height" => text.set_line_height(as_f32(value, key)?),
            "font" => {
                text.set_font(as_string(value, key)?);
                has_explicit_font = true;
            }
            "color" => text.set_color(HexColor::new(as_owned_string(value, key)?)),
            "opacity" => text.set_opacity(as_f32(value, key)?),
            _ => return Err(format!("unknown prop `{}` on <Text>", key,)),
        }
    }

    text.set_position(x.unwrap_or(0.0), y.unwrap_or(0.0));
    if !has_explicit_font && !inherited_text_style.font_families.is_empty() {
        text.set_fonts(inherited_text_style.font_families.clone());
    }
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
    path: &[u64],
    inherited_text_style: &InheritedTextStyle,
) -> Result<Box<dyn ElementTrait>, String> {
    let mut text_content = String::new();
    let mut placeholder = String::new();
    let mut binding: Option<Binding<String>> = None;
    let mut text_area = TextArea::from_content_with_id(stable_node_id(path, "TextArea"), "");
    let mut x: Option<f32> = None;
    let mut y: Option<f32> = None;
    let mut width: Option<f32> = None;
    let mut height: Option<f32> = None;
    let mut has_explicit_font = false;

    for (key, value) in &node.props {
        if key.as_str() == "key" {
            continue;
        }
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
            "font" => {
                text_area.set_font(as_string(value, key)?);
                has_explicit_font = true;
            }
            "color" => text_area.set_color(HexColor::new(as_owned_string(value, key)?)),
            "opacity" => text_area.set_opacity(as_f32(value, key)?),
            "multiline" => text_area.set_multiline(as_bool(value, key)?),
            "read_only" => text_area.set_read_only(as_bool(value, key)?),
            "max_length" => text_area.set_max_length(as_usize(value, key)?),
            _ => return Err(format!("unknown prop `{}` on <TextArea>", key,)),
        }
    }

    text_area.set_position(x.unwrap_or(0.0), y.unwrap_or(0.0));
    if !has_explicit_font && !inherited_text_style.font_families.is_empty() {
        text_area.set_fonts(inherited_text_style.font_families.clone());
    }
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

fn convert_button_element(
    node: &RsxElementNode,
    path: &[u64],
    inherited_text_style: &InheritedTextStyle,
) -> Result<Box<dyn ElementTrait>, String> {
    let mut props = ButtonProps::new("");

    for (key, value) in &node.props {
        if key.as_str() == "key" {
            continue;
        }
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
            "on_click" => props.on_click = Some(as_click_handler(value, key)?),
            _ => return Err(format!("unknown prop `{}` on <Button>", key)),
        }
    }

    let scope = component_boundary_path(path, "Button");
    let button_node = build_button_rsx(props);
    convert_node(&button_node, &scope, inherited_text_style)
}

fn convert_checkbox_element(
    node: &RsxElementNode,
    path: &[u64],
) -> Result<Box<dyn ElementTrait>, String> {
    let mut props = CheckboxProps::new("");

    for (key, value) in &node.props {
        if key.as_str() == "key" {
            continue;
        }
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

    let scope = component_boundary_path(path, "Checkbox");
    let checkbox_node = build_checkbox_rsx(props);
    convert_node(&checkbox_node, &scope, &InheritedTextStyle::default())
}

fn convert_number_field_element(
    node: &RsxElementNode,
    path: &[u64],
) -> Result<Box<dyn ElementTrait>, String> {
    let mut props = NumberFieldProps::new();

    for (key, value) in &node.props {
        if key.as_str() == "key" {
            continue;
        }
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

    let scope = component_boundary_path(path, "NumberField");
    Ok(Box::new(build_number_field_with_ids(
        props,
        stable_node_id(&scope, "NumberField"),
        stable_node_id(&scope, "NumberFieldMinus"),
        stable_node_id(&scope, "NumberFieldPlus"),
        stable_node_id(&scope, "NumberFieldValue"),
    )))
}

fn convert_select_element(
    node: &RsxElementNode,
    path: &[u64],
) -> Result<Box<dyn ElementTrait>, String> {
    let mut props = SelectProps::new(Vec::new());

    for (key, value) in &node.props {
        if key.as_str() == "key" {
            continue;
        }
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

    let scope = component_boundary_path(path, "Select");
    Ok(Box::new(build_select_with_ids(
        props,
        stable_node_id(&scope, "Select"),
        stable_node_id(&scope, "SelectText"),
        stable_node_id(&scope, "SelectIcon"),
    )))
}

fn convert_slider_element(
    node: &RsxElementNode,
    path: &[u64],
) -> Result<Box<dyn ElementTrait>, String> {
    let mut props = SliderProps::new();

    for (key, value) in &node.props {
        if key.as_str() == "key" {
            continue;
        }
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

    let scope = component_boundary_path(path, "Slider");
    Ok(Box::new(build_slider_with_ids(
        props,
        stable_node_id(&scope, "Slider"),
        stable_node_id(&scope, "SliderRail"),
        stable_node_id(&scope, "SliderActive"),
        stable_node_id(&scope, "SliderThumb"),
        stable_node_id(&scope, "SliderValue"),
    )))
}

fn convert_switch_element(
    node: &RsxElementNode,
    path: &[u64],
    inherited_text_style: &InheritedTextStyle,
) -> Result<Box<dyn ElementTrait>, String> {
    let mut props = SwitchProps::new("");

    for (key, value) in &node.props {
        if key.as_str() == "key" {
            continue;
        }
        match key.as_str() {
            "label" => props.label = as_owned_string(value, key)?,
            "checked" => props.checked = as_bool(value, key)?,
            "binding" => props.checked_binding = Some(as_binding_bool(value, key)?),
            "disabled" => props.disabled = as_bool(value, key)?,
            _ => return Err(format!("unknown prop `{}` on <Switch>", key)),
        }
    }

    let scope = component_boundary_path(path, "Switch");
    if props.checked_binding.is_none() {
        let state_id = stable_node_id(&scope, "SwitchUncontrolledState");
        props.checked_binding = Some(uncontrolled_switch_binding(state_id, props.checked));
    }
    let switch_node = build_switch_rsx(props);
    convert_node(&switch_node, &scope, inherited_text_style)
}

fn uncontrolled_switch_binding(state_id: u64, initial_checked: bool) -> Binding<bool> {
    UNCONTROLLED_SWITCH_BINDINGS.with(|store| {
        let mut store = store.borrow_mut();
        if let Some(binding) = store.get(&state_id) {
            return binding.clone();
        }
        let binding = Binding::from_cell(Rc::new(RefCell::new(initial_checked)));
        store.insert(state_id, binding.clone());
        binding
    })
}

fn stable_node_id(path: &[u64], kind: &str) -> u64 {
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
        for byte in index.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash ^= 0xfe;
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    if hash == 0 { 1 } else { hash }
}

fn component_boundary_path(path: &[u64], component_tag: &str) -> Vec<u64> {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET_BASIS;
    hash ^= 0x43;
    hash = hash.wrapping_mul(FNV_PRIME);
    for &byte in component_tag.as_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash &= !(1_u64 << 63);

    let mut scoped = path.to_vec();
    scoped.push(hash);
    scoped
}

fn child_identity_token(node: &RsxNode, fallback_index: usize) -> u64 {
    const KEYED_BIT: u64 = 1_u64 << 63;
    let keyed = match node {
        RsxNode::Element(el) => identity_token_from_keyed_props(&el.props),
        _ => None,
    };
    keyed.unwrap_or(fallback_index as u64 | KEYED_BIT)
}

fn identity_token_from_keyed_props(props: &[(String, PropValue)]) -> Option<u64> {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    for (key, value) in props {
        if key.as_str() != "key" {
            continue;
        }
        let mut hash = FNV_OFFSET_BASIS;
        hash ^= 0x42;
        hash = hash.wrapping_mul(FNV_PRIME);
        match value {
            PropValue::String(v) => {
                for &byte in v.as_bytes() {
                    hash ^= u64::from(byte);
                    hash = hash.wrapping_mul(FNV_PRIME);
                }
            }
            PropValue::I64(v) => {
                for byte in v.to_le_bytes() {
                    hash ^= u64::from(byte);
                    hash = hash.wrapping_mul(FNV_PRIME);
                }
            }
            PropValue::F64(v) => {
                for byte in v.to_le_bytes() {
                    hash ^= u64::from(byte);
                    hash = hash.wrapping_mul(FNV_PRIME);
                }
            }
            PropValue::Bool(v) => {
                hash ^= if *v { 1 } else { 0 };
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            _ => return None,
        }
        return Some(hash & !(1_u64 << 63));
    }
    None
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
    use super::{
        component_boundary_path, identity_token_from_keyed_props, rsx_to_elements, stable_node_id,
        uncontrolled_switch_binding,
    };
    use crate::ui::{
        ClickEvent, EventMeta, IntoPropValue, MouseButton, MouseEventData, PropValue, RsxNode,
    };
    use crate::ui::host::{Button, Checkbox, Element, NumberField, Select, Slider, Switch};
    use crate::{
        Border, BorderRadius, Color, Display, FlowDirection, FlowWrap, IntoColor, Length,
        ParsedValue, PropertyId, Style, Unit,
    };

    fn style_bg(hex: &str) -> Style {
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::Color(IntoColor::<Color>::into_color(Color::hex(hex))),
        );
        style
    }

    #[test]
    fn identity_token_uses_key_value_stably() {
        let props_a = vec![
            ("key".to_string(), PropValue::String("item-a".to_string())),
            ("label".to_string(), PropValue::String("A".to_string())),
        ];
        let props_b = vec![
            ("label".to_string(), PropValue::String("A".to_string())),
            ("key".to_string(), PropValue::String("item-a".to_string())),
        ];
        let token_a = identity_token_from_keyed_props(&props_a).expect("token should exist");
        let token_b = identity_token_from_keyed_props(&props_b).expect("token should exist");
        assert_eq!(token_a, token_b);
    }

    #[test]
    fn key_prop_is_accepted_for_builtin_component() {
        let node = RsxNode::element("Checkbox")
            .with_prop("key", "feature-1")
            .with_prop("label", "Feature");
        let converted = rsx_to_elements(&node);
        assert!(converted.is_ok());
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

    #[test]
    fn switch_click_toggles_binding() {
        let switch_on = crate::ui::global_state(|| false);
        let tree = RsxNode::element("Switch")
            .with_prop("label", "Dark mode")
            .with_prop("binding", switch_on.binding().into_prop_value());

        let mut roots = rsx_to_elements(&tree).expect("convert switch");
        let root = roots.get_mut(0).expect("has root");

        root.measure(crate::view::base_component::LayoutConstraints {
            max_width: 300.0,
            max_height: 100.0,
            percent_base_width: Some(300.0),
            percent_base_height: Some(100.0),
        });
        root.place(crate::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 300.0,
            available_height: 100.0,
            percent_base_width: Some(300.0),
            percent_base_height: Some(100.0),
        });

        let mut viewport = crate::view::Viewport::new();
        let mut control = crate::view::ViewportControl::new(&mut viewport);
        let mut click = ClickEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: 10.0,
                viewport_y: 10.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(MouseButton::Left),
                buttons: crate::ui::MouseButtons::default(),
                modifiers: crate::ui::KeyModifiers::default(),
            },
        };

        let handled = crate::view::base_component::dispatch_click_from_hit_test(
            root.as_mut(),
            &mut click,
            &mut control,
        );
        assert!(handled);
        assert!(switch_on.get());
    }

    #[test]
    fn switch_click_toggles_without_binding() {
        let tree = RsxNode::element("Switch")
            .with_prop("label", "Dark mode")
            .with_prop("checked", false);

        let mut roots = rsx_to_elements(&tree).expect("convert switch");
        let root = roots.get_mut(0).expect("has root");

        root.measure(crate::view::base_component::LayoutConstraints {
            max_width: 300.0,
            max_height: 100.0,
            percent_base_width: Some(300.0),
            percent_base_height: Some(100.0),
        });
        root.place(crate::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 300.0,
            available_height: 100.0,
            percent_base_width: Some(300.0),
            percent_base_height: Some(100.0),
        });

        let mut viewport = crate::view::Viewport::new();
        let mut control = crate::view::ViewportControl::new(&mut viewport);
        let mut click = ClickEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: 10.0,
                viewport_y: 10.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(MouseButton::Left),
                buttons: crate::ui::MouseButtons::default(),
                modifiers: crate::ui::KeyModifiers::default(),
            },
        };
        let handled = crate::view::base_component::dispatch_click_from_hit_test(
            root.as_mut(),
            &mut click,
            &mut control,
        );
        assert!(handled);
        let scope = component_boundary_path(&[], "Switch");
        let state_id = stable_node_id(&scope, "SwitchUncontrolledState");
        let uncontrolled = uncontrolled_switch_binding(state_id, false);
        assert!(uncontrolled.get());
    }

    #[test]
    fn switch_click_toggles_binding_in_mixed_component_tree() {
        let checked = crate::ui::global_state(|| false);
        let number_value = crate::ui::global_state(|| 1.0_f64);
        let selected_index = crate::ui::global_state(|| 0_usize);
        let slider_value = crate::ui::global_state(|| 30.0_f64);
        let switch_on = crate::ui::global_state(|| false);

        let tree = crate::ui::rsx! {
            <Element style={{
                width: Length::px(420.0),
                height: Length::px(320.0),
                display: Display::Flow,
                flow_direction: FlowDirection::Column,
                flow_wrap: FlowWrap::NoWrap,
            }}>
                <Button label="Contained" width=98 height=34 variant="contained" />
                <Checkbox label="Enable feature" binding={checked.binding()} width=180 height=30 />
                <NumberField binding={number_value.binding()} min=0.0 max=10.0 step=0.5 width=136 height=36 />
                <Select
                    options={vec![String::from("Option A"), String::from("Option B")]}
                    binding={selected_index.binding()}
                    width=140
                    height=36
                />
                <Slider binding={slider_value.binding()} min=0.0 max=100.0 width=180 height=30 />
                <Switch label="Dark mode" binding={switch_on.binding()} />
            </Element>
        };

        let mut roots = rsx_to_elements(&tree).expect("convert mixed tree");
        let root = roots.get_mut(0).expect("has root");
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

        let mut viewport = crate::view::Viewport::new();
        let mut control = crate::view::ViewportControl::new(&mut viewport);
        let mut click = ClickEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: 12.0,
                viewport_y: 182.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(MouseButton::Left),
                buttons: crate::ui::MouseButtons::default(),
                modifiers: crate::ui::KeyModifiers::default(),
            },
        };

        let handled = crate::view::base_component::dispatch_click_from_hit_test(
            root.as_mut(),
            &mut click,
            &mut control,
        );
        assert!(handled);
        assert!(switch_on.get());
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

    fn collect_boxes(
        node: &mut dyn crate::view::base_component::ElementTrait,
        out: &mut Vec<crate::view::base_component::BoxModelSnapshot>,
    ) {
        out.push(node.box_model_snapshot());
        if let Some(children) = node.children_mut() {
            for child in children {
                collect_boxes(child.as_mut(), out);
            }
        }
    }

    #[test]
    fn switch_binding_value_changes_thumb_layout_on_rebuild() {
        let switch_on = crate::ui::global_state(|| false);
        let tree = RsxNode::element("Switch")
            .with_prop("label", "Dark mode")
            .with_prop("binding", switch_on.binding().into_prop_value());

        let mut roots_off = rsx_to_elements(&tree).expect("convert switch off");
        let root_off = roots_off.get_mut(0).expect("has root");
        root_off.measure(crate::view::base_component::LayoutConstraints {
            max_width: 300.0,
            max_height: 100.0,
            percent_base_width: Some(300.0),
            percent_base_height: Some(100.0),
        });
        root_off.place(crate::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 300.0,
            available_height: 100.0,
            percent_base_width: Some(300.0),
            percent_base_height: Some(100.0),
        });
        let mut boxes_off = Vec::new();
        collect_boxes(root_off.as_mut(), &mut boxes_off);
        let thumb_x_off = boxes_off
            .iter()
            .filter(|s| (s.width - 20.0).abs() < 0.1 && (s.height - 20.0).abs() < 0.1)
            .map(|s| s.x)
            .fold(f32::NEG_INFINITY, f32::max);

        switch_on.set(true);

        let mut roots_on = rsx_to_elements(&tree).expect("convert switch on");
        let root_on = roots_on.get_mut(0).expect("has root");
        root_on.measure(crate::view::base_component::LayoutConstraints {
            max_width: 300.0,
            max_height: 100.0,
            percent_base_width: Some(300.0),
            percent_base_height: Some(100.0),
        });
        root_on.place(crate::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 300.0,
            available_height: 100.0,
            percent_base_width: Some(300.0),
            percent_base_height: Some(100.0),
        });
        let mut boxes_on = Vec::new();
        collect_boxes(root_on.as_mut(), &mut boxes_on);
        let thumb_x_on = boxes_on
            .iter()
            .filter(|s| (s.width - 20.0).abs() < 0.1 && (s.height - 20.0).abs() < 0.1)
            .map(|s| s.x)
            .fold(f32::NEG_INFINITY, f32::max);

        assert!(thumb_x_on > thumb_x_off, "thumb should move right when checked");
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
            .with_child(RsxNode::element("Element").with_prop(
                "style",
                style_with_size(style_bg_border("#FFD166", "#EF476F", 3.0), 72.0, 48.0),
            ))
            .with_child(RsxNode::element("Element").with_prop(
                "style",
                style_with_size(style_bg_border("#F72585", "#B5179E", 4.0), 120.0, 80.0),
            ))
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
                    .with_child(RsxNode::text(
                        "Used to verify event hit-testing and bubbling.",
                    )),
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

        assert!(boxes.iter().any(|&(x, y, w, h)| (x - 16.0).abs() < 0.1
            && (y - 12.0).abs() < 0.1
            && (w - 216.0).abs() < 0.1
            && (h - 65.0).abs() < 0.1));
        assert!(boxes.iter().any(|&(x, y, w, h)| (x - 19.0).abs() < 0.1
            && (y - 19.0).abs() < 0.1
            && w > 120.0
            && h > 20.0));
        assert!(boxes.iter().any(|&(x, y, w, h)| (x - 19.0).abs() < 0.1
            && (y - 59.0).abs() < 0.1
            && (w - 218.0).abs() < 0.1
            && (h - 35.0).abs() < 0.1));
        assert!(boxes.iter().any(|&(x, y, w, h)| (x - 19.0).abs() < 0.1
            && (y - 83.0).abs() < 0.1
            && w > 80.0
            && h > 12.0));
    }

    #[test]
    fn element_padding_offsets_child_layout() {
        let tree = RsxNode::element("Element")
            .with_prop("style", style_with_size(Style::new(), 200.0, 120.0))
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

    #[test]
    fn flow_row_without_explicit_size_uses_children_content_size() {
        let mut row_style = Style::new();
        row_style.insert(PropertyId::Display, ParsedValue::Display(Display::Flow));
        row_style.insert(
            PropertyId::FlowDirection,
            ParsedValue::FlowDirection(FlowDirection::Row),
        );
        row_style.insert(
            PropertyId::FlowWrap,
            ParsedValue::FlowWrap(FlowWrap::NoWrap),
        );
        row_style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(8.0)));

        let tree = RsxNode::element("Element")
            .with_prop("style", row_style)
            .with_child(
                RsxNode::element("Button")
                    .with_prop("label", "Contained")
                    .with_prop("width", 98)
                    .with_prop("height", 34)
                    .with_prop("variant", "contained"),
            )
            .with_child(
                RsxNode::element("Button")
                    .with_prop("label", "Outlined")
                    .with_prop("width", 98)
                    .with_prop("height", 34)
                    .with_prop("variant", "outlined"),
            )
            .with_child(
                RsxNode::element("Button")
                    .with_prop("label", "Text")
                    .with_prop("width", 70)
                    .with_prop("height", 34)
                    .with_prop("variant", "text"),
            );

        let mut roots = rsx_to_elements(&tree).expect("convert rsx");
        let root = roots.first_mut().expect("single root");
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

        let snapshot = root.box_model_snapshot();
        assert_eq!(snapshot.width, 282.0);
        assert_eq!(snapshot.height, 34.0);
    }
}
