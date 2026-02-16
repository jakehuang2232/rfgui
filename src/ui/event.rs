use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct KeyModifiers {
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub meta: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MouseButtons {
    pub left: bool,
    pub right: bool,
    pub middle: bool,
    pub back: bool,
    pub forward: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct EventMeta {
    target_id: u64,
    current_target_id: u64,
    propagation_stopped: bool,
}

impl EventMeta {
    pub fn new(target_id: u64) -> Self {
        Self {
            target_id,
            current_target_id: target_id,
            propagation_stopped: false,
        }
    }

    pub fn target_id(&self) -> u64 {
        self.target_id
    }

    pub fn current_target_id(&self) -> u64 {
        self.current_target_id
    }

    pub fn set_target_id(&mut self, target_id: u64) {
        self.target_id = target_id;
        self.current_target_id = target_id;
    }

    pub fn set_current_target_id(&mut self, current_target_id: u64) {
        self.current_target_id = current_target_id;
    }

    pub fn propagation_stopped(&self) -> bool {
        self.propagation_stopped
    }

    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MouseEventData {
    pub viewport_x: f32,
    pub viewport_y: f32,
    pub local_x: f32,
    pub local_y: f32,
    pub button: Option<MouseButton>,
    pub buttons: MouseButtons,
    pub modifiers: KeyModifiers,
}

#[derive(Debug, Clone)]
pub struct KeyEventData {
    pub key: String,
    pub code: String,
    pub repeat: bool,
    pub modifiers: KeyModifiers,
}

#[derive(Debug, Clone, Copy)]
pub struct MouseDownEvent {
    pub meta: EventMeta,
    pub mouse: MouseEventData,
}

#[derive(Debug, Clone, Copy)]
pub struct MouseUpEvent {
    pub meta: EventMeta,
    pub mouse: MouseEventData,
}

#[derive(Debug, Clone, Copy)]
pub struct MouseMoveEvent {
    pub meta: EventMeta,
    pub mouse: MouseEventData,
}

#[derive(Debug, Clone, Copy)]
pub struct ClickEvent {
    pub meta: EventMeta,
    pub mouse: MouseEventData,
}

#[derive(Debug, Clone)]
pub struct KeyDownEvent {
    pub meta: EventMeta,
    pub key: KeyEventData,
}

#[derive(Debug, Clone)]
pub struct KeyUpEvent {
    pub meta: EventMeta,
    pub key: KeyEventData,
}

#[derive(Debug, Clone)]
pub struct TextInputEvent {
    pub meta: EventMeta,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct ImePreeditEvent {
    pub meta: EventMeta,
    pub text: String,
    pub cursor: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Copy)]
pub struct FocusEvent {
    pub meta: EventMeta,
}

#[derive(Debug, Clone, Copy)]
pub struct BlurEvent {
    pub meta: EventMeta,
}

#[derive(Clone)]
pub struct MouseDownHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut MouseDownEvent)>>,
}

#[derive(Clone)]
pub struct MouseUpHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut MouseUpEvent)>>,
}

#[derive(Clone)]
pub struct MouseMoveHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut MouseMoveEvent)>>,
}

#[derive(Clone)]
pub struct ClickHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut ClickEvent)>>,
}

#[derive(Clone)]
pub struct KeyDownHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut KeyDownEvent)>>,
}

#[derive(Clone)]
pub struct KeyUpHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut KeyUpEvent)>>,
}

#[derive(Clone)]
pub struct FocusHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut FocusEvent)>>,
}

#[derive(Clone)]
pub struct BlurHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut BlurEvent)>>,
}

fn next_handler_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

macro_rules! impl_handler_prop {
    ($ty:ident, $event_ty:ty) => {
        impl $ty {
            pub fn new<F>(handler: F) -> Self
            where
                F: FnMut(&mut $event_ty) + 'static,
            {
                Self {
                    id: next_handler_id(),
                    handler: Rc::new(RefCell::new(handler)),
                }
            }

            pub fn id(&self) -> u64 {
                self.id
            }

            pub fn call(&self, event: &mut $event_ty) {
                (self.handler.borrow_mut())(event);
            }
        }

        impl PartialEq for $ty {
            fn eq(&self, other: &Self) -> bool {
                self.id == other.id
            }
        }

        impl fmt::Debug for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($ty))
                    .field("id", &self.id)
                    .finish()
            }
        }
    };
}

impl_handler_prop!(MouseDownHandlerProp, MouseDownEvent);
impl_handler_prop!(MouseUpHandlerProp, MouseUpEvent);
impl_handler_prop!(MouseMoveHandlerProp, MouseMoveEvent);
impl_handler_prop!(ClickHandlerProp, ClickEvent);
impl_handler_prop!(KeyDownHandlerProp, KeyDownEvent);
impl_handler_prop!(KeyUpHandlerProp, KeyUpEvent);
impl_handler_prop!(FocusHandlerProp, FocusEvent);
impl_handler_prop!(BlurHandlerProp, BlurEvent);

pub fn on_mouse_down<F>(handler: F) -> MouseDownHandlerProp
where
    F: FnMut(&mut MouseDownEvent) + 'static,
{
    MouseDownHandlerProp::new(handler)
}

pub fn on_mouse_up<F>(handler: F) -> MouseUpHandlerProp
where
    F: FnMut(&mut MouseUpEvent) + 'static,
{
    MouseUpHandlerProp::new(handler)
}

pub fn on_mouse_move<F>(handler: F) -> MouseMoveHandlerProp
where
    F: FnMut(&mut MouseMoveEvent) + 'static,
{
    MouseMoveHandlerProp::new(handler)
}

pub fn on_click<F>(handler: F) -> ClickHandlerProp
where
    F: FnMut(&mut ClickEvent) + 'static,
{
    ClickHandlerProp::new(handler)
}

pub fn on_key_down<F>(handler: F) -> KeyDownHandlerProp
where
    F: FnMut(&mut KeyDownEvent) + 'static,
{
    KeyDownHandlerProp::new(handler)
}

pub fn on_key_up<F>(handler: F) -> KeyUpHandlerProp
where
    F: FnMut(&mut KeyUpEvent) + 'static,
{
    KeyUpHandlerProp::new(handler)
}

pub fn on_focus<F>(handler: F) -> FocusHandlerProp
where
    F: FnMut(&mut FocusEvent) + 'static,
{
    FocusHandlerProp::new(handler)
}

pub fn on_blur<F>(handler: F) -> BlurHandlerProp
where
    F: FnMut(&mut BlurEvent) + 'static,
{
    BlurHandlerProp::new(handler)
}
