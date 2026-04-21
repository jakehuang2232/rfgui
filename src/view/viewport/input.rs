use super::*;

#[derive(Clone)]
pub(super) enum ViewportPointerUpListener {
    Persistent(crate::ui::PointerUpHandlerProp),
    Until(PointerUpUntilHandler),
}

impl ViewportPointerUpListener {
    pub(super) fn id(&self) -> u64 {
        match self {
            Self::Persistent(handler) => handler.id(),
            Self::Until(handler) => handler.id(),
        }
    }
}

pub use crate::platform::input::PlatformPointerButton as PointerButton;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ViewportDebugOptions {
    pub trace_fps: bool,
    pub trace_render_time: bool,
    pub trace_layout_detail: bool,
    pub trace_compile_detail: bool,
    pub trace_execute_detail: bool,
    pub trace_reuse_path: bool,
    pub geometry_overlay: bool,
}

impl ViewportDebugOptions {
    pub(super) fn from_env() -> Self {
        Self {
            trace_fps: std::env::var("RFGUI_TRACE_FPS").is_ok(),
            trace_render_time: std::env::var("RFGUI_TRACE_RENDER_TIME").is_ok(),
            trace_layout_detail: std::env::var("RFGUI_TRACE_LAYOUT_DETAIL").is_ok(),
            trace_compile_detail: std::env::var("RFGUI_TRACE_COMPILE_DETAIL").is_ok(),
            trace_execute_detail: std::env::var("RFGUI_TRACE_EXECUTE_DETAIL").is_ok(),
            trace_reuse_path: std::env::var("RFGUI_TRACE_REUSE_PATH").is_ok(),
            geometry_overlay: std::env::var("RFGUI_DEBUG_GEOMETRY_OVERLAY").is_ok(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct InputState {
    pub focused_node_id: Option<crate::view::node_arena::NodeKey>,
    pub selects: Vec<u64>,
    pub pointer_capture_node_id: Option<crate::view::node_arena::NodeKey>,
    /// Node currently holding keyboard capture (modal overlays, tooltips
    /// absorbing Esc). When `Some`, key events route to this node
    /// regardless of focus. `None` = no active capture.
    pub keyboard_capture_node_id: Option<crate::view::node_arena::NodeKey>,
    pub hovered_node_id: Option<crate::view::node_arena::NodeKey>,
    pub pointer_position_viewport: Option<(f32, f32)>,
    pub pending_click: Option<PendingClick>,
    /// Last fired click, kept to compute `click_count` for consecutive
    /// clicks. Reset once the double-click window closes or the pointer
    /// drifts beyond the slop radius.
    pub last_click: Option<LastClick>,
    pub pressed_pointer_buttons: FxHashSet<PointerButton>,
    pub pressed_keys: FxHashSet<String>,
    pub modifiers: crate::platform::Modifiers,
    /// Reason tagged on the next [`FocusEvent`] / [`BlurEvent`] flushed
    /// by `sync_focus_dispatch`. Callers that mutate focus (pointer
    /// click, Tab key, programmatic `set_focus`) set this before
    /// triggering the sync. Defaults to `Programmatic`.
    pub pending_focus_reason: crate::ui::FocusReason,
    /// Active drag operation, `Some` from the moment
    /// [`crate::ui::EventViewport::start_drag`] is applied until the
    /// pointer_up that releases the drag. Normal pointer_move /
    /// pointer_up dispatch paths check this and route to drag events
    /// instead.
    pub drag_state: Option<DragState>,
}

/// Per-drag engine state. Lives inside [`InputState`] for the lifetime
/// of one drag gesture. Pointer_move / pointer_up paths consult this
/// to route `DragOver` / `DragLeave` / `Drop` / `DragEnd` instead of
/// their normal dispatchers.
#[derive(Debug, Clone)]
pub struct DragState {
    pub source_id: crate::view::node_arena::NodeKey,
    pub data: crate::ui::DataTransfer,
    #[allow(dead_code)]
    pub effect_allowed: crate::ui::DragEffect,
    /// Node most recently entered by `DragOver`. Used to fire a
    /// `DragLeave` on target transitions.
    pub last_over_target: Option<crate::view::node_arena::NodeKey>,
    /// Drop effect chosen by the last handler (or `None` if no target
    /// accepted). Fed into `DropEvent` / `DragEndEvent.effect`.
    pub last_drop_effect: Option<crate::ui::DragEffect>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PendingClick {
    pub button: PointerButton,
    pub target_id: crate::view::node_arena::NodeKey,
    pub viewport_x: f32,
    pub viewport_y: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct LastClick {
    pub button: PointerButton,
    pub target_id: crate::view::node_arena::NodeKey,
    pub viewport_x: f32,
    pub viewport_y: f32,
    pub timestamp: crate::time::Instant,
    pub count: u32,
}

/// Multi-click interval. Matches the common desktop default (Windows/macOS
/// both use ~500ms). Two clicks closer than this on the same target and
/// within [`CLICK_COUNT_MAX_TRAVEL_SQ`] increment the click count.
pub(super) const CLICK_COUNT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
/// Max squared pointer travel (logical px²) between consecutive clicks to
/// still count as a multi-click. Same slop radius as the down/up click
/// validity check.
pub(super) const CLICK_COUNT_MAX_TRAVEL_SQ: f32 = 25.0;

/// Compute the next `click_count` given the previously recorded click (if
/// any) and the new click's metadata. Resets to `1` when the time or
/// distance threshold is exceeded, or when the button/target changes.
pub(super) fn compute_click_count(
    previous: Option<LastClick>,
    button: PointerButton,
    target_id: crate::view::node_arena::NodeKey,
    viewport_x: f32,
    viewport_y: f32,
    now: crate::time::Instant,
) -> u32 {
    let Some(prev) = previous else { return 1 };
    if prev.button != button || prev.target_id != target_id {
        return 1;
    }
    if now.duration_since(prev.timestamp) > CLICK_COUNT_INTERVAL {
        return 1;
    }
    if distance_sq(viewport_x, viewport_y, prev.viewport_x, prev.viewport_y)
        > CLICK_COUNT_MAX_TRAVEL_SQ
    {
        return 1;
    }
    prev.count.saturating_add(1)
}

pub(super) fn is_valid_click_candidate(
    pending_click: PendingClick,
    button: PointerButton,
    hit_target: Option<crate::view::node_arena::NodeKey>,
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

fn distance_sq(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x1 - x2;
    let dy = y1 - y2;
    dx * dx + dy * dy
}
