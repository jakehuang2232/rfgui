use crate::{
    ButtonProps, ButtonVariant, CheckboxProps, NumberFieldProps, SelectProps, SliderProps,
    SwitchProps, build_button_rsx, build_checkbox_rsx, build_number_field_rsx, build_select_rsx,
    build_slider_rsx, build_switch_rsx,
};
use rfgui::ui::{Binding, ClickHandlerProp, FromPropValue, PropValue, RsxElementNode};
use rfgui::view::base_component::ElementTrait;
use rfgui::{register_element_factory, rsx_to_element_scoped};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

thread_local! {
    static UNCONTROLLED_SWITCH_BINDINGS: RefCell<HashMap<u64, Binding<bool>>> =
        RefCell::new(HashMap::new());
    static UNCONTROLLED_SELECT_BINDINGS: RefCell<HashMap<u64, Binding<usize>>> =
        RefCell::new(HashMap::new());
}

pub fn register_mui_components() {
    register_element_factory("Button", Arc::new(convert_button_element));
    register_element_factory("Checkbox", Arc::new(convert_checkbox_element));
    register_element_factory("NumberField", Arc::new(convert_number_field_element));
    register_element_factory("Select", Arc::new(convert_select_element));
    register_element_factory("Slider", Arc::new(convert_slider_element));
    register_element_factory("Switch", Arc::new(convert_switch_element));
}

fn convert_button_element(
    node: &RsxElementNode,
    path: &[u64],
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
    rsx_to_element_scoped(&build_button_rsx(props), &scope)
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
    rsx_to_element_scoped(&build_checkbox_rsx(props), &scope)
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
    rsx_to_element_scoped(&build_number_field_rsx(props), &scope)
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
    let state_id = stable_node_id(&scope, "SelectUncontrolledState");
    if props.selected_binding.is_none() {
        props.selected_binding = Some(uncontrolled_select_binding(state_id, props.selected_index));
    }
    rsx_to_element_scoped(&build_select_rsx(props), &scope)
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
    rsx_to_element_scoped(&build_slider_rsx(props), &scope)
}

fn convert_switch_element(
    node: &RsxElementNode,
    path: &[u64],
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
    rsx_to_element_scoped(&build_switch_rsx(props), &scope)
}

fn uncontrolled_switch_binding(state_id: u64, initial_checked: bool) -> Binding<bool> {
    UNCONTROLLED_SWITCH_BINDINGS.with(|store| {
        let mut store = store.borrow_mut();
        if let Some(binding) = store.get(&state_id) {
            return binding.clone();
        }
        let binding = Binding::new(initial_checked);
        store.insert(state_id, binding.clone());
        binding
    })
}

fn uncontrolled_select_binding(state_id: u64, initial_selected_index: usize) -> Binding<usize> {
    UNCONTROLLED_SELECT_BINDINGS.with(|store| {
        let mut store = store.borrow_mut();
        if let Some(binding) = store.get(&state_id) {
            return binding.clone();
        }
        let binding = Binding::new(initial_selected_index);
        store.insert(state_id, binding.clone());
        binding
    })
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

fn as_usize_raw(value: &PropValue, key: &str) -> Result<usize, String> {
    match value {
        PropValue::I64(v) if *v >= 0 => Ok(*v as usize),
        PropValue::F64(v) if *v >= 0.0 => Ok(*v as usize),
        PropValue::I64(_) | PropValue::F64(_) => {
            Err(format!("prop `{key}` expects non-negative numeric value"))
        }
        _ => Err(format!("prop `{key}` expects numeric value")),
    }
}

fn as_vec_string(value: &PropValue, key: &str) -> Result<Vec<String>, String> {
    Vec::<String>::from_prop_value(value.clone())
        .map_err(|_| format!("prop `{key}` expects Vec<String> value"))
}

fn as_click_handler(value: &PropValue, key: &str) -> Result<ClickHandlerProp, String> {
    match value {
        PropValue::OnClick(handler) => Ok(handler.clone()),
        _ => Err(format!("prop `{key}` expects click handler value")),
    }
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

#[cfg(test)]
mod tests {
    use super::register_mui_components;
    use crate::{Checkbox, Select};
    use rfgui::ui::{EventMeta, MouseButton, MouseEventData, global_state, rsx, take_state_dirty};

    #[test]
    fn checkbox_click_updates_binding() {
        let checked = global_state(|| false);

        let tree =
            rsx! { <Checkbox label="Enable" binding={checked.binding()} width=180 height=30 /> };

        let mut roots = rfgui::rsx_to_elements(&tree).expect("convert checkbox");
        let root = roots.get_mut(0).expect("has root");
        root.measure(rfgui::view::base_component::LayoutConstraints {
            max_width: 320.0,
            max_height: 120.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(120.0),
        });
        root.place(rfgui::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 320.0,
            available_height: 120.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(120.0),
        });

        let mut viewport = rfgui::view::Viewport::new();
        let mut control = rfgui::view::ViewportControl::new(&mut viewport);
        let mut click = rfgui::ui::ClickEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: 8.0,
                viewport_y: 8.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(MouseButton::Left),
                buttons: rfgui::ui::MouseButtons::default(),
                modifiers: rfgui::ui::KeyModifiers::default(),
            },
        };

        let handled = rfgui::view::base_component::dispatch_click_from_hit_test(
            root.as_mut(),
            &mut click,
            &mut control,
        );
        assert!(handled);
        assert!(checked.get());
    }

    #[test]
    fn register_factory_still_supports_dynamic_checkbox_tag() {
        register_mui_components();
        let checked = global_state(|| false);
        let tree = rfgui::ui::RsxNode::element("Checkbox")
            .with_prop("label", "Enable")
            .with_prop("binding", rfgui::ui::IntoPropValue::into_prop_value(checked.binding()))
            .with_prop("width", 180)
            .with_prop("height", 30);
        let converted = rfgui::rsx_to_elements(&tree);
        assert!(converted.is_ok());
    }

    #[test]
    fn select_trigger_click_does_not_change_binding_value() {
        let selected = global_state(|| 0_usize);
        let tree = rsx! {
            <Select
                options={vec![
                    String::from("Option A"),
                    String::from("Option B"),
                    String::from("Option C"),
                ]}
                binding={selected.binding()}
                width=180
                height=36
            />
        };

        let mut roots = rfgui::rsx_to_elements(&tree).expect("convert select");
        let mut viewport = rfgui::view::Viewport::new();

        click_once(roots[0].as_mut(), &mut viewport, 10.0, 10.0);
        assert_eq!(selected.get(), 0);
        assert!(take_state_dirty());
    }

    #[test]
    fn select_open_state_renders_menu_options() {
        register_mui_components();
        let selected = global_state(|| 0_usize);
        let tree = rfgui::ui::RsxNode::element("Select")
            .with_prop(
                "options",
                rfgui::ui::IntoPropValue::into_prop_value(vec![
                    String::from("Option A"),
                    String::from("Option B"),
                    String::from("Option C"),
                ]),
            )
            .with_prop(
                "binding",
                rfgui::ui::IntoPropValue::into_prop_value(selected.binding()),
            )
            .with_prop("width", 180)
            .with_prop("height", 36);

        let mut roots = rfgui::rsx_to_elements(&tree).expect("convert select");
        let mut viewport = rfgui::view::Viewport::new();
        click_once(roots[0].as_mut(), &mut viewport, 10.0, 10.0);
        assert!(take_state_dirty());

        let roots = rfgui::rsx_to_elements(&tree).expect("rebuild opened select");
        let root = roots.first().expect("has root");
        let children = root.children().expect("select root has children");
        assert_eq!(children.len(), 2);
        let menu_children = children[1].children().expect("menu has options");
        assert_eq!(menu_children.len(), 3);
    }

    fn click_once(
        root: &mut dyn rfgui::view::base_component::ElementTrait,
        viewport: &mut rfgui::view::Viewport,
        x: f32,
        y: f32,
    ) {
        root.measure(rfgui::view::base_component::LayoutConstraints {
            max_width: 320.0,
            max_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        });
        root.place(rfgui::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 320.0,
            available_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        });

        let mut control = rfgui::view::ViewportControl::new(viewport);
        let mut click = rfgui::ui::ClickEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(MouseButton::Left),
                buttons: rfgui::ui::MouseButtons::default(),
                modifiers: rfgui::ui::KeyModifiers::default(),
            },
        };

        let handled = rfgui::view::base_component::dispatch_click_from_hit_test(
            root,
            &mut click,
            &mut control,
        );
        assert!(handled);
    }
}
