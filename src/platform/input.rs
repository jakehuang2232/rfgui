//! Platform-neutral input event types.
//!
//! Phase 0 scaffolding. The viewport still consumes primitive args in its
//! `dispatch_*` methods today — phase 3 will port them to take
//! `Platform*Event` directly. For now these types define the canonical shape
//! that every backend must produce, so conversion code in future backends
//! (winit, web, headless) has a single target.

use bitflags::bitflags;
use smol_str::SmolStr;

use crate::time::Instant;

/// Mirror of `view::viewport::PointerButton`, kept in the platform layer so
/// backends can build events without importing viewport internals. Phase 3
/// will collapse the two into a single type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlatformPointerButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

/// Kind of pointer device producing an event. Lives in the platform layer so
/// backends can tag events without importing the ui-layer `PointerEventData`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerType {
    Mouse,
    Pen,
    Touch,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlatformPointerEventKind {
    Down(PlatformPointerButton),
    Up(PlatformPointerButton),
    Move { x: f32, y: f32 },
    Click(PlatformPointerButton),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlatformPointerEvent {
    pub kind: PlatformPointerEventKind,
    pub pointer_id: u64,
    pub pointer_type: PointerType,
    pub pressure: f32,
}

/// Unit interpretation of [`PlatformWheelEvent::delta_x`] / `delta_y`.
/// Mirrors the W3C `WheelEvent.deltaMode` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WheelDeltaMode {
    /// Deltas are logical pixels. Typical for trackpads, high-resolution
    /// mice, and web browsers after normalization.
    #[default]
    Pixel,
    /// Deltas count lines of text (classic mouse wheels). The engine
    /// multiplies by `WheelConfig::mouse_line_step` downstream.
    Line,
    /// Deltas count viewport pages (Page Up / Page Down style scrolling).
    Page,
}

/// Lifecycle phase of a wheel / trackpad gesture. Lets handlers react to
/// gesture boundaries (start a scroll animation on `Began`, resume on
/// `Momentum`, snap on `Ended`). Most platforms only emit `Changed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WheelPhase {
    /// Gesture just started (first event after fingers touch).
    Began,
    /// Continuous delta while the gesture is active. Default for single
    /// mouse-wheel ticks that have no explicit phase.
    #[default]
    Changed,
    /// Fingers lifted; momentum (if any) begins separately.
    Ended,
    /// Inertial / kinetic scrolling after the fingers lifted. Comes from
    /// the OS, not user motion.
    Momentum,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlatformWheelEvent {
    pub delta_x: f32,
    pub delta_y: f32,
    /// Pointer position (viewport-space logical pixels) at the time of the
    /// event. Lets hit-testing target the node under the pointer when
    /// routing to user `on_wheel` handlers.
    pub position: (f32, f32),
    pub modifiers: Modifiers,
    pub delta_mode: WheelDeltaMode,
    pub phase: WheelPhase,
    pub timestamp: Instant,
}

bitflags! {
    /// Modifier key state (modifier keys held + lock states).
    ///
    /// Shared by keyboard and pointer events. Use [`Modifiers::command`] for
    /// the platform-canonical shortcut modifier (Cmd on macOS, Ctrl elsewhere).
    /// Use [`Modifiers::exactly`] for shortcut matching (ignores lock state).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Modifiers: u8 {
        const SHIFT       = 1 << 0;
        const CTRL        = 1 << 1;
        const ALT         = 1 << 2;
        const META        = 1 << 3; // Cmd on macOS, Win on Windows, Super on X11
        const CAPS_LOCK   = 1 << 4;
        const NUM_LOCK    = 1 << 5;
        const SCROLL_LOCK = 1 << 6;
    }
}

impl Modifiers {
    #[inline]
    pub fn shift(&self) -> bool {
        self.contains(Self::SHIFT)
    }
    #[inline]
    pub fn ctrl(&self) -> bool {
        self.contains(Self::CTRL)
    }
    #[inline]
    pub fn alt(&self) -> bool {
        self.contains(Self::ALT)
    }
    #[inline]
    pub fn meta(&self) -> bool {
        self.contains(Self::META)
    }
    #[inline]
    pub fn caps_lock(&self) -> bool {
        self.contains(Self::CAPS_LOCK)
    }
    #[inline]
    pub fn num_lock(&self) -> bool {
        self.contains(Self::NUM_LOCK)
    }
    #[inline]
    pub fn scroll_lock(&self) -> bool {
        self.contains(Self::SCROLL_LOCK)
    }

    /// Platform-canonical primary shortcut modifier.
    /// macOS: `META` (Cmd); others: `CTRL`.
    #[inline]
    pub fn command(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.meta()
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.ctrl()
        }
    }

    /// True if any non-lock modifier held.
    #[inline]
    pub fn any(&self) -> bool {
        self.intersects(Self::SHIFT | Self::CTRL | Self::ALT | Self::META)
    }

    /// True if no non-lock modifier held.
    #[inline]
    pub fn none(&self) -> bool {
        !self.any()
    }

    /// Non-lock subset (for shortcut matching).
    #[inline]
    pub fn keys_only(&self) -> Self {
        *self & (Self::SHIFT | Self::CTRL | Self::ALT | Self::META)
    }

    /// Lock-only subset.
    #[inline]
    pub fn locks_only(&self) -> Self {
        *self & (Self::CAPS_LOCK | Self::NUM_LOCK | Self::SCROLL_LOCK)
    }

    /// True if non-lock modifier set exactly equals `other.keys_only()`.
    /// Use for keyboard shortcut matching (Caps/Num/Scroll Lock ignored).
    #[inline]
    pub fn exactly(&self, other: Self) -> bool {
        self.keys_only() == other.keys_only()
    }
}

/// Physical key identifier (layout-independent).
///
/// Variants named after the US-QWERTY physical position. A French AZERTY
/// keyboard pressing the key at top-left letter position still reports
/// [`Key::KeyQ`] — layout-derived text lives in `PlatformKeyEvent::characters`.
///
/// Follows the W3C UI Events `KeyboardEvent.code` spec where possible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    // Letters
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,

    // Main digit row
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,

    // Numpad
    NumberPad0,
    NumberPad1,
    NumberPad2,
    NumberPad3,
    NumberPad4,
    NumberPad5,
    NumberPad6,
    NumberPad7,
    NumberPad8,
    NumberPad9,
    NumberPadAdd,
    NumberPadSubtract,
    NumberPadMultiply,
    NumberPadDivide,
    NumberPadDecimal,
    NumberPadEnter,
    NumberPadEqual,

    // Modifiers (left/right split)
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    MetaLeft,
    MetaRight,

    // Function row
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,

    // Navigation
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,

    // Control / editing
    Enter,
    Tab,
    Space,
    Backspace,
    Escape,
    CapsLock,
    NumLock,
    ScrollLock,
    PrintScreen,
    Pause,
    ContextMenu,

    // Symbols (US physical position)
    Backquote,
    Minus,
    Equal,
    BracketLeft,
    BracketRight,
    Backslash,
    Semicolon,
    Quote,
    Comma,
    Period,
    Slash,

    // East-Asian physical keys
    IntlYen,       // JIS ¥|
    IntlRo,        // JIS ろ
    IntlBackslash, // ISO / 106-key extra \ between Left-Shift and Z
    Lang1,         // KR 한/영 toggle; JP Hiragana/Katakana
    Lang2,         // KR Hanja; JP Eisu
    Convert,       // JP 変換
    NonConvert,    // JP 無変換
    KanaMode,      // JP kana mode toggle

    // Media
    AudioVolumeUp,
    AudioVolumeDown,
    AudioVolumeMute,
    MediaPlayPause,
    MediaStop,
    MediaTrackNext,
    MediaTrackPrev,

    // Browser / system
    BrowserBack,
    BrowserForward,
    BrowserRefresh,
    BrowserHome,
    LaunchMail,
    LaunchApp1,
    LaunchApp2,

    /// Escape hatch: unknown vendor / OS-added key.
    /// Carries raw platform scancode when available so user code can still
    /// bind custom shortcuts.
    Unidentified(Option<u32>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformKeyEvent {
    /// Physical key.
    pub key: Key,
    /// Layout-applied text output for this press. `None` for non-character
    /// keys (arrows, function keys, shortcuts that swallow text, …).
    pub characters: Option<SmolStr>,
    pub modifiers: Modifiers,
    pub repeat: bool,
    /// True if an IME composition is active at the time of this key event.
    /// Handlers should typically early-return (let IME consume the key) when
    /// set — e.g. Enter during preedit commits the composition, not a newline.
    pub is_composing: bool,
    /// true = KeyDown, false = KeyUp.
    pub pressed: bool,
    /// Timestamp captured at event ingestion (backend entry).
    pub timestamp: Instant,
}

/// Origin classification for a [`PlatformTextInput`]. Mirrors the
/// subset of `InputEvent.inputType` values rfgui keeps at the platform
/// layer so backends can distinguish paste / drop / IME commit from
/// plain typing. The ui layer re-exports this as [`crate::ui::InputType`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlatformInputType {
    #[default]
    Typing,
    Paste,
    Drop,
    ImeCommit,
    Programmatic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformTextInput {
    pub text: String,
    pub input_type: PlatformInputType,
    /// True if the platform reports this text as arriving while an IME
    /// composition is still open (rare; the commit path usually closes
    /// the composition first and flips this back to false).
    pub is_composing: bool,
}

/// Style hint for a preedit span (see [`PlatformPreeditAttribute`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformPreeditStyle {
    Underline,
    DottedUnderline,
    Highlight,
}

/// Styled span inside a preedit string. Byte offsets index into
/// [`PlatformImePreedit::text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformPreeditAttribute {
    pub start: usize,
    pub end: usize,
    pub style: PlatformPreeditStyle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformImePreedit {
    pub text: String,
    pub cursor_start: Option<usize>,
    pub cursor_end: Option<usize>,
    /// Selection range inside `text` (byte offsets), independent of the
    /// caret. Most backends only report caret, leaving this `None`.
    pub selection_start: Option<usize>,
    pub selection_end: Option<usize>,
    /// Styled sub-runs for underline / highlight. Empty when the
    /// backend only delivers plain preedit text.
    pub attributes: Vec<PlatformPreeditAttribute>,
}
