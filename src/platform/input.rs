//! Platform-neutral input event types.
//!
//! Phase 0 scaffolding. The viewport still consumes primitive args in its
//! `dispatch_*` methods today — phase 3 will port them to take
//! `Platform*Event` directly. For now these types define the canonical shape
//! that every backend must produce, so conversion code in future backends
//! (winit, web, headless) has a single target.

/// Mirror of `view::viewport::MouseButton`, kept in the platform layer so
/// backends can build events without importing viewport internals. Phase 3
/// will collapse the two into a single type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlatformMouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlatformMouseEventKind {
    Down(PlatformMouseButton),
    Up(PlatformMouseButton),
    Move { x: f32, y: f32 },
    Click(PlatformMouseButton),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlatformMouseEvent {
    pub kind: PlatformMouseEventKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlatformWheelEvent {
    pub delta_x: f32,
    pub delta_y: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformKeyEvent {
    pub key: String,
    pub code: String,
    pub repeat: bool,
    pub pressed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformTextInput {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformImePreedit {
    pub text: String,
    pub cursor_start: Option<usize>,
    pub cursor_end: Option<usize>,
}
