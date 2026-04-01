use super::*;

#[derive(Clone)]
pub(super) enum ViewportMouseUpListener {
    Persistent(crate::ui::MouseUpHandlerProp),
    Until(MouseUpUntilHandler),
}

impl ViewportMouseUpListener {
    pub(super) fn id(&self) -> u64 {
        match self {
            Self::Persistent(handler) => handler.id(),
            Self::Until(handler) => handler.id(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ViewportDebugOptions {
    pub trace_fps: bool,
    pub trace_render_time: bool,
    pub trace_reuse_path: bool,
    pub geometry_overlay: bool,
}

impl ViewportDebugOptions {
    pub(super) fn from_env() -> Self {
        Self {
            trace_fps: std::env::var("RFGUI_TRACE_FPS").is_ok(),
            trace_render_time: std::env::var("RFGUI_TRACE_RENDER_TIME").is_ok(),
            trace_reuse_path: std::env::var("RFGUI_TRACE_REUSE_PATH").is_ok(),
            geometry_overlay: std::env::var("RFGUI_DEBUG_GEOMETRY_OVERLAY").is_ok(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct InputState {
    pub focused_node_id: Option<u64>,
    pub selects: Vec<u64>,
    pub pointer_capture_node_id: Option<u64>,
    pub hovered_node_id: Option<u64>,
    pub mouse_position_viewport: Option<(f32, f32)>,
    pub pending_click: Option<PendingClick>,
    pub pressed_mouse_buttons: HashSet<MouseButton>,
    pub pressed_keys: HashSet<String>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PendingClick {
    pub button: MouseButton,
    pub target_id: u64,
    pub viewport_x: f32,
    pub viewport_y: f32,
}

pub(super) fn is_valid_click_candidate(
    pending_click: PendingClick,
    button: MouseButton,
    hit_target: Option<u64>,
    up_x: f32,
    up_y: f32,
) -> bool {
    if pending_click.button != button {
        return false;
    }
    if hit_target != Some(pending_click.target_id) {
        return false;
    }
    const CLICK_MAX_TRAVEL_SQ: f32 = 25.0;
    distance_sq(
        up_x,
        up_y,
        pending_click.viewport_x,
        pending_click.viewport_y,
    ) <= CLICK_MAX_TRAVEL_SQ
}

pub(super) fn to_ui_mouse_button(button: MouseButton) -> crate::ui::MouseButton {
    match button {
        MouseButton::Left => crate::ui::MouseButton::Left,
        MouseButton::Right => crate::ui::MouseButton::Right,
        MouseButton::Middle => crate::ui::MouseButton::Middle,
        MouseButton::Back => crate::ui::MouseButton::Back,
        MouseButton::Forward => crate::ui::MouseButton::Forward,
        MouseButton::Other(v) => crate::ui::MouseButton::Other(v),
    }
}

fn distance_sq(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x1 - x2;
    let dy = y1 - y2;
    dx * dx + dy * dy
}
