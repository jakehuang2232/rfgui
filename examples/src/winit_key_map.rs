//! Shared winit → rfgui key/modifier mapping used by both native and web
//! runners.

use rfgui::platform::{Key as RfKey, Modifiers};
use winit::keyboard::{KeyCode, ModifiersState, NativeKeyCode, PhysicalKey};

pub fn physical_key_to_rf(key: &PhysicalKey) -> RfKey {
    match key {
        PhysicalKey::Code(code) => key_code_to_rf(*code),
        PhysicalKey::Unidentified(nk) => RfKey::Unidentified(native_scancode(*nk)),
    }
}

fn native_scancode(nk: NativeKeyCode) -> Option<u32> {
    match nk {
        NativeKeyCode::Unidentified => None,
        NativeKeyCode::Android(code) => Some(code),
        NativeKeyCode::MacOS(code) => Some(code as u32),
        NativeKeyCode::Windows(code) => Some(code as u32),
        NativeKeyCode::Xkb(code) => Some(code),
    }
}

pub fn key_code_to_rf(code: KeyCode) -> RfKey {
    use KeyCode as K;
    match code {
        K::KeyA => RfKey::KeyA, K::KeyB => RfKey::KeyB, K::KeyC => RfKey::KeyC,
        K::KeyD => RfKey::KeyD, K::KeyE => RfKey::KeyE, K::KeyF => RfKey::KeyF,
        K::KeyG => RfKey::KeyG, K::KeyH => RfKey::KeyH, K::KeyI => RfKey::KeyI,
        K::KeyJ => RfKey::KeyJ, K::KeyK => RfKey::KeyK, K::KeyL => RfKey::KeyL,
        K::KeyM => RfKey::KeyM, K::KeyN => RfKey::KeyN, K::KeyO => RfKey::KeyO,
        K::KeyP => RfKey::KeyP, K::KeyQ => RfKey::KeyQ, K::KeyR => RfKey::KeyR,
        K::KeyS => RfKey::KeyS, K::KeyT => RfKey::KeyT, K::KeyU => RfKey::KeyU,
        K::KeyV => RfKey::KeyV, K::KeyW => RfKey::KeyW, K::KeyX => RfKey::KeyX,
        K::KeyY => RfKey::KeyY, K::KeyZ => RfKey::KeyZ,

        K::Digit0 => RfKey::Digit0, K::Digit1 => RfKey::Digit1, K::Digit2 => RfKey::Digit2,
        K::Digit3 => RfKey::Digit3, K::Digit4 => RfKey::Digit4, K::Digit5 => RfKey::Digit5,
        K::Digit6 => RfKey::Digit6, K::Digit7 => RfKey::Digit7, K::Digit8 => RfKey::Digit8,
        K::Digit9 => RfKey::Digit9,

        K::Numpad0 => RfKey::NumberPad0, K::Numpad1 => RfKey::NumberPad1,
        K::Numpad2 => RfKey::NumberPad2, K::Numpad3 => RfKey::NumberPad3,
        K::Numpad4 => RfKey::NumberPad4, K::Numpad5 => RfKey::NumberPad5,
        K::Numpad6 => RfKey::NumberPad6, K::Numpad7 => RfKey::NumberPad7,
        K::Numpad8 => RfKey::NumberPad8, K::Numpad9 => RfKey::NumberPad9,
        K::NumpadAdd => RfKey::NumberPadAdd,
        K::NumpadSubtract => RfKey::NumberPadSubtract,
        K::NumpadMultiply => RfKey::NumberPadMultiply,
        K::NumpadDivide => RfKey::NumberPadDivide,
        K::NumpadDecimal => RfKey::NumberPadDecimal,
        K::NumpadEnter => RfKey::NumberPadEnter,
        K::NumpadEqual => RfKey::NumberPadEqual,

        K::ShiftLeft => RfKey::ShiftLeft, K::ShiftRight => RfKey::ShiftRight,
        K::ControlLeft => RfKey::ControlLeft, K::ControlRight => RfKey::ControlRight,
        K::AltLeft => RfKey::AltLeft, K::AltRight => RfKey::AltRight,
        K::SuperLeft => RfKey::MetaLeft, K::SuperRight => RfKey::MetaRight,

        K::F1 => RfKey::F1, K::F2 => RfKey::F2, K::F3 => RfKey::F3, K::F4 => RfKey::F4,
        K::F5 => RfKey::F5, K::F6 => RfKey::F6, K::F7 => RfKey::F7, K::F8 => RfKey::F8,
        K::F9 => RfKey::F9, K::F10 => RfKey::F10, K::F11 => RfKey::F11, K::F12 => RfKey::F12,
        K::F13 => RfKey::F13, K::F14 => RfKey::F14, K::F15 => RfKey::F15, K::F16 => RfKey::F16,
        K::F17 => RfKey::F17, K::F18 => RfKey::F18, K::F19 => RfKey::F19, K::F20 => RfKey::F20,
        K::F21 => RfKey::F21, K::F22 => RfKey::F22, K::F23 => RfKey::F23, K::F24 => RfKey::F24,

        K::ArrowUp => RfKey::ArrowUp, K::ArrowDown => RfKey::ArrowDown,
        K::ArrowLeft => RfKey::ArrowLeft, K::ArrowRight => RfKey::ArrowRight,
        K::Home => RfKey::Home, K::End => RfKey::End,
        K::PageUp => RfKey::PageUp, K::PageDown => RfKey::PageDown,
        K::Insert => RfKey::Insert, K::Delete => RfKey::Delete,

        K::Enter => RfKey::Enter, K::Tab => RfKey::Tab, K::Space => RfKey::Space,
        K::Backspace => RfKey::Backspace, K::Escape => RfKey::Escape,
        K::CapsLock => RfKey::CapsLock, K::NumLock => RfKey::NumLock,
        K::ScrollLock => RfKey::ScrollLock,
        K::PrintScreen => RfKey::PrintScreen, K::Pause => RfKey::Pause,
        K::ContextMenu => RfKey::ContextMenu,

        K::Backquote => RfKey::Backquote, K::Minus => RfKey::Minus, K::Equal => RfKey::Equal,
        K::BracketLeft => RfKey::BracketLeft, K::BracketRight => RfKey::BracketRight,
        K::Backslash => RfKey::Backslash,
        K::Semicolon => RfKey::Semicolon, K::Quote => RfKey::Quote,
        K::Comma => RfKey::Comma, K::Period => RfKey::Period, K::Slash => RfKey::Slash,

        K::IntlYen => RfKey::IntlYen,
        K::IntlRo => RfKey::IntlRo,
        K::IntlBackslash => RfKey::IntlBackslash,
        K::Lang1 => RfKey::Lang1, K::Lang2 => RfKey::Lang2,
        K::Convert => RfKey::Convert,
        K::NonConvert => RfKey::NonConvert,
        K::KanaMode => RfKey::KanaMode,

        K::AudioVolumeUp => RfKey::AudioVolumeUp,
        K::AudioVolumeDown => RfKey::AudioVolumeDown,
        K::AudioVolumeMute => RfKey::AudioVolumeMute,
        K::MediaPlayPause => RfKey::MediaPlayPause,
        K::MediaStop => RfKey::MediaStop,
        K::MediaTrackNext => RfKey::MediaTrackNext,
        K::MediaTrackPrevious => RfKey::MediaTrackPrev,

        K::BrowserBack => RfKey::BrowserBack,
        K::BrowserForward => RfKey::BrowserForward,
        K::BrowserRefresh => RfKey::BrowserRefresh,
        K::BrowserHome => RfKey::BrowserHome,
        K::LaunchMail => RfKey::LaunchMail,
        K::LaunchApp1 => RfKey::LaunchApp1,
        K::LaunchApp2 => RfKey::LaunchApp2,

        _ => RfKey::Unidentified(None),
    }
}

pub fn winit_modifiers_to_rf(mods: ModifiersState) -> Modifiers {
    let mut out = Modifiers::empty();
    if mods.shift_key() { out |= Modifiers::SHIFT; }
    if mods.control_key() { out |= Modifiers::CTRL; }
    if mods.alt_key() { out |= Modifiers::ALT; }
    if mods.super_key() { out |= Modifiers::META; }
    out
}
