// Element-owned RSX event handler prop dispatch.
//
// Consumed by both the cold convert path
// (`renderer_adapter::build_container_element_shell` via
// `Element::ingest_props`) and the incremental fiber_work path
// (`Element::apply_prop` for the `on_*` keys). Replace semantics on
// the incremental path are paired with `Element::clear_rsx_event_handler`.

use crate::ui::PropValue;

/// `&'static str` table of the 23 RSX event handler prop names. Used
/// by the incremental fiber_work whitelist gate so every `on_*` prop
/// that the cold path recognises is also committable incrementally.
pub(crate) const RSX_EVENT_HANDLER_PROPS: &[&str] = &[
    "on_pointer_down",
    "on_pointer_up",
    "on_pointer_move",
    "on_pointer_enter",
    "on_pointer_leave",
    "on_click",
    "on_context_menu",
    "on_wheel",
    "on_key_down",
    "on_key_up",
    "on_focus",
    "on_blur",
    "on_ime_commit",
    "on_ime_enabled",
    "on_ime_disabled",
    "on_drag_start",
    "on_drag_over",
    "on_drag_leave",
    "on_drop",
    "on_drag_end",
    "on_copy",
    "on_cut",
    "on_paste",
];

/// Try to install one of the 23 RSX event-handler props on `element`.
/// Returns `Ok(true)` if `key` matched a handler prop; `Ok(false)` if
/// `key` is not a handler prop; `Err` on `PropValue` decode failure.
pub(crate) fn try_assign_event_handler_prop(
    element: &mut Element,
    key: &str,
    value: &PropValue,
) -> Result<bool, String> {
    match key {
        "on_pointer_down" => {
            let handler = as_mouse_down_handler(value, key)?;
            element.on_pointer_down(move |event, _control| handler.call(event));
        }
        "on_pointer_up" => {
            let handler = as_mouse_up_handler(value, key)?;
            element.on_pointer_up(move |event, _control| handler.call(event));
        }
        "on_pointer_move" => {
            let handler = as_mouse_move_handler(value, key)?;
            element.on_pointer_move(move |event, _control| handler.call(event));
        }
        "on_pointer_enter" => {
            let handler = as_mouse_enter_handler(value, key)?;
            element.on_pointer_enter(move |event| handler.call(event));
        }
        "on_pointer_leave" => {
            let handler = as_mouse_leave_handler(value, key)?;
            element.on_pointer_leave(move |event| handler.call(event));
        }
        "on_click" => {
            let handler = as_click_handler(value, key)?;
            element.on_click(move |event, _control| handler.call(event));
        }
        "on_context_menu" => {
            let handler = as_context_menu_handler(value, key)?;
            element.on_context_menu(move |event, _control| handler.call(event));
        }
        "on_wheel" => {
            let handler = as_wheel_handler(value, key)?;
            element.on_wheel(move |event, _control| handler.call(event));
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
        "on_ime_commit" => {
            let handler = as_ime_commit_handler(value, key)?;
            element.on_ime_commit(move |event, _control| handler.call(event));
        }
        "on_ime_enabled" => {
            let handler = as_ime_enabled_handler(value, key)?;
            element.on_ime_enabled(move |event, _control| handler.call(event));
        }
        "on_ime_disabled" => {
            let handler = as_ime_disabled_handler(value, key)?;
            element.on_ime_disabled(move |event, _control| handler.call(event));
        }
        "on_drag_start" => {
            let handler = as_drag_start_handler(value, key)?;
            element.on_drag_start(move |event, _control| handler.call(event));
        }
        "on_drag_over" => {
            let handler = as_drag_over_handler(value, key)?;
            element.on_drag_over(move |event, _control| handler.call(event));
        }
        "on_drag_leave" => {
            let handler = as_drag_leave_handler(value, key)?;
            element.on_drag_leave(move |event, _control| handler.call(event));
        }
        "on_drop" => {
            let handler = as_drop_handler(value, key)?;
            element.on_drop(move |event, _control| handler.call(event));
        }
        "on_drag_end" => {
            let handler = as_drag_end_handler(value, key)?;
            element.on_drag_end(move |event, _control| handler.call(event));
        }
        "on_copy" => {
            let handler = as_copy_handler(value, key)?;
            element.on_copy(move |event, _control| handler.call(event));
        }
        "on_cut" => {
            let handler = as_cut_handler(value, key)?;
            element.on_cut(move |event, _control| handler.call(event));
        }
        "on_paste" => {
            let handler = as_paste_handler(value, key)?;
            element.on_paste(move |event, _control| handler.call(event));
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn as_mouse_down_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::PointerDownHandlerProp, String> {
    match value {
        PropValue::OnPointerDown(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects pointer down handler value")),
    }
}

fn as_mouse_up_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::PointerUpHandlerProp, String> {
    match value {
        PropValue::OnPointerUp(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects pointer up handler value")),
    }
}

fn as_mouse_move_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::PointerMoveHandlerProp, String> {
    match value {
        PropValue::OnPointerMove(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects pointer move handler value")),
    }
}

fn as_mouse_enter_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::PointerEnterHandlerProp, String> {
    match value {
        PropValue::OnPointerEnter(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects pointer enter handler value")),
    }
}

fn as_mouse_leave_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::PointerLeaveHandlerProp, String> {
    match value {
        PropValue::OnPointerLeave(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects pointer leave handler value")),
    }
}

fn as_click_handler(value: &PropValue, key: &str) -> Result<crate::ui::ClickHandlerProp, String> {
    match value {
        PropValue::OnClick(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects click handler value")),
    }
}

fn as_context_menu_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::ContextMenuHandlerProp, String> {
    match value {
        PropValue::OnContextMenu(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects context menu handler value")),
    }
}

fn as_wheel_handler(value: &PropValue, key: &str) -> Result<crate::ui::WheelHandlerProp, String> {
    match value {
        PropValue::OnWheel(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects wheel handler value")),
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

pub(crate) fn as_blur_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::BlurHandlerProp, String> {
    match value {
        PropValue::OnBlur(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects blur handler value")),
    }
}

macro_rules! as_event_handler_fn {
    ($fn_name:ident, $ty:ty, $variant:ident, $label:expr) => {
        fn $fn_name(value: &PropValue, key: &str) -> Result<$ty, String> {
            match value {
                PropValue::$variant(v) => Ok(v.clone()),
                _ => Err(format!("prop `{}` expects {} handler value", key, $label)),
            }
        }
    };
}

as_event_handler_fn!(
    as_ime_commit_handler,
    crate::ui::ImeCommitHandlerProp,
    OnImeCommit,
    "ime commit"
);
as_event_handler_fn!(
    as_ime_enabled_handler,
    crate::ui::ImeEnabledHandlerProp,
    OnImeEnabled,
    "ime enabled"
);
as_event_handler_fn!(
    as_ime_disabled_handler,
    crate::ui::ImeDisabledHandlerProp,
    OnImeDisabled,
    "ime disabled"
);
as_event_handler_fn!(
    as_drag_start_handler,
    crate::ui::DragStartHandlerProp,
    OnDragStart,
    "drag start"
);
as_event_handler_fn!(
    as_drag_over_handler,
    crate::ui::DragOverHandlerProp,
    OnDragOver,
    "drag over"
);
as_event_handler_fn!(
    as_drag_leave_handler,
    crate::ui::DragLeaveHandlerProp,
    OnDragLeave,
    "drag leave"
);
as_event_handler_fn!(as_drop_handler, crate::ui::DropHandlerProp, OnDrop, "drop");
as_event_handler_fn!(
    as_drag_end_handler,
    crate::ui::DragEndHandlerProp,
    OnDragEnd,
    "drag end"
);
as_event_handler_fn!(as_copy_handler, crate::ui::CopyHandlerProp, OnCopy, "copy");
as_event_handler_fn!(as_cut_handler, crate::ui::CutHandlerProp, OnCut, "cut");
as_event_handler_fn!(
    as_paste_handler,
    crate::ui::PasteHandlerProp,
    OnPaste,
    "paste"
);
