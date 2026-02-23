use crate::{
    ButtonProps, ButtonVariant, CheckboxProps, NumberFieldProps, SelectProps, SliderProps,
    SwitchProps, build_button_rsx, build_checkbox_rsx, build_number_field_rsx, build_select_rsx,
    build_slider_rsx, build_switch_rsx,
};
use rfgui::ui::RsxNode;
use rfgui::ui::{
    Binding, ClickHandlerProp, PropValue, RsxChildrenPolicy, RsxPropSchema, RsxProps, RsxTag,
};

pub struct Button;
pub struct Checkbox;
pub struct NumberField;
pub struct Select;
pub struct Slider;
pub struct Switch;

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
    pub binding: rfgui::ui::Binding<bool>,
    pub width: f64,
    pub height: f64,
    pub disabled: bool,
}

pub struct NumberFieldPropSchema {
    pub value: f64,
    pub binding: rfgui::ui::Binding<f64>,
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
    pub binding: rfgui::ui::Binding<usize>,
    pub width: f64,
    pub height: f64,
    pub disabled: bool,
}

pub struct SliderPropSchema {
    pub value: f64,
    pub binding: rfgui::ui::Binding<f64>,
    pub min: f64,
    pub max: f64,
    pub width: f64,
    pub height: f64,
    pub disabled: bool,
}

pub struct SwitchPropSchema {
    pub label: String,
    pub checked: bool,
    pub binding: rfgui::ui::Binding<bool>,
    pub disabled: bool,
}

impl RsxTag for Button {
    fn rsx_render(mut props: RsxProps, _children: Vec<RsxNode>) -> Result<RsxNode, String> {
        let _ = props.remove_raw("key");
        let mut mapped = ButtonProps::new("");
        if let Some(v) = props.remove_t::<String>("label")? {
            mapped.label = v;
        }
        if let Some(v) = props.remove_t::<f64>("width")? {
            mapped.width = v as f32;
        }
        if let Some(v) = props.remove_t::<f64>("height")? {
            mapped.height = v as f32;
        }
        if let Some(v) = props.remove_t::<String>("variant")? {
            mapped.variant = match v.to_ascii_lowercase().as_str() {
                "contained" => ButtonVariant::Contained,
                "outlined" => ButtonVariant::Outlined,
                "text" => ButtonVariant::Text,
                other => return Err(format!("unknown Button variant `{other}`")),
            };
        }
        if let Some(v) = props.remove_t::<bool>("disabled")? {
            mapped.disabled = v;
        }
        if let Some(v) = props.remove_raw("on_click") {
            mapped.on_click = match v {
                PropValue::OnClick(handler) => Some(handler),
                _ => return Err("prop `on_click` expects click handler value".to_string()),
            };
        }

        props.reject_remaining("Button")?;
        Ok(build_button_rsx(mapped))
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
        let _ = props.remove_raw("key");
        let mut mapped = CheckboxProps::new("");
        if let Some(v) = props.remove_t::<String>("label")? {
            mapped.label = v;
        }
        if let Some(v) = props.remove_t::<bool>("checked")? {
            mapped.checked = v;
        }
        if let Some(v) = props.remove_t::<Binding<bool>>("binding")? {
            mapped.checked_binding = Some(v);
        }
        if let Some(v) = props.remove_t::<f64>("width")? {
            mapped.width = v as f32;
        }
        if let Some(v) = props.remove_t::<f64>("height")? {
            mapped.height = v as f32;
        }
        if let Some(v) = props.remove_t::<bool>("disabled")? {
            mapped.disabled = v;
        }

        props.reject_remaining("Checkbox")?;
        Ok(build_checkbox_rsx(mapped))
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
        let _ = props.remove_raw("key");
        let mut mapped = NumberFieldProps::new();
        if let Some(v) = props.remove_t::<f64>("value")? {
            mapped.value = v;
        }
        if let Some(v) = props.remove_t::<Binding<f64>>("binding")? {
            mapped.value_binding = Some(v);
        }
        if let Some(v) = props.remove_t::<f64>("min")? {
            mapped.min = Some(v);
        }
        if let Some(v) = props.remove_t::<f64>("max")? {
            mapped.max = Some(v);
        }
        if let Some(v) = props.remove_t::<f64>("step")? {
            mapped.step = v;
        }
        if let Some(v) = props.remove_t::<f64>("width")? {
            mapped.width = v as f32;
        }
        if let Some(v) = props.remove_t::<f64>("height")? {
            mapped.height = v as f32;
        }
        if let Some(v) = props.remove_t::<bool>("disabled")? {
            mapped.disabled = v;
        }

        props.reject_remaining("NumberField")?;
        Ok(build_number_field_rsx(mapped))
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
        let _ = props.remove_raw("key");
        let mut mapped = SelectProps::new(Vec::new());
        if let Some(v) = props.remove_t::<Vec<String>>("options")? {
            mapped.options = v;
        }
        if let Some(v) = props.remove_t::<i64>("selected_index")? {
            if v < 0 {
                return Err("prop `selected_index` expects non-negative value".to_string());
            }
            mapped.selected_index = v as usize;
        }
        if let Some(v) = props.remove_t::<Binding<usize>>("binding")? {
            mapped.selected_binding = Some(v);
        }
        if let Some(v) = props.remove_t::<f64>("width")? {
            mapped.width = v as f32;
        }
        if let Some(v) = props.remove_t::<f64>("height")? {
            mapped.height = v as f32;
        }
        if let Some(v) = props.remove_t::<bool>("disabled")? {
            mapped.disabled = v;
        }

        props.reject_remaining("Select")?;
        Ok(build_select_rsx(mapped))
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
        let _ = props.remove_raw("key");
        let mut mapped = SliderProps::new();
        if let Some(v) = props.remove_t::<f64>("value")? {
            mapped.value = v;
        }
        if let Some(v) = props.remove_t::<Binding<f64>>("binding")? {
            mapped.value_binding = Some(v);
        }
        if let Some(v) = props.remove_t::<f64>("min")? {
            mapped.min = v;
        }
        if let Some(v) = props.remove_t::<f64>("max")? {
            mapped.max = v;
        }
        if let Some(v) = props.remove_t::<f64>("width")? {
            mapped.width = v as f32;
        }
        if let Some(v) = props.remove_t::<f64>("height")? {
            mapped.height = v as f32;
        }
        if let Some(v) = props.remove_t::<bool>("disabled")? {
            mapped.disabled = v;
        }

        props.reject_remaining("Slider")?;
        Ok(build_slider_rsx(mapped))
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
        let _ = props.remove_raw("key");
        let mut mapped = SwitchProps::new("");
        if let Some(v) = props.remove_t::<String>("label")? {
            mapped.label = v;
        }
        if let Some(v) = props.remove_t::<bool>("checked")? {
            mapped.checked = v;
        }
        if let Some(v) = props.remove_t::<Binding<bool>>("binding")? {
            mapped.checked_binding = Some(v);
        }
        if let Some(v) = props.remove_t::<bool>("disabled")? {
            mapped.disabled = v;
        }

        props.reject_remaining("Switch")?;
        Ok(build_switch_rsx(mapped))
    }
}

impl RsxPropSchema for Switch {
    type PropsSchema = SwitchPropSchema;
}

impl RsxChildrenPolicy for Switch {
    const ACCEPTS_CHILDREN: bool = false;
}
