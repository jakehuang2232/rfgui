use crate::Cursor;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ViewportListenerHandle(pub u64);

#[derive(Clone)]
pub struct MouseUpUntilHandler {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut MouseUpEvent) -> bool>>,
}

impl MouseUpUntilHandler {
    pub fn new<F>(handler: F) -> Self
    where
        F: FnMut(&mut MouseUpEvent) -> bool + 'static,
    {
        Self {
            id: next_handler_id(),
            handler: Rc::new(RefCell::new(handler)),
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn call(&self, event: &mut MouseUpEvent) -> bool {
        (self.handler.borrow_mut())(event)
    }
}

impl PartialEq for MouseUpUntilHandler {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl fmt::Debug for MouseUpUntilHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MouseUpUntilHandler")
            .field("id", &self.id)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub enum ViewportListenerAction {
    AddMouseMoveListener(MouseMoveHandlerProp),
    AddMouseUpListener(MouseUpHandlerProp),
    AddMouseUpListenerUntil(MouseUpUntilHandler),
    SetFocus(Option<u64>),
    SetCursor(Option<Cursor>),
    RemoveListener(ViewportListenerHandle),
}

#[derive(Default)]
struct EventMetaState {
    target_id: u64,
    current_target_id: u64,
    propagation_stopped: bool,
    keep_focus_requested: bool,
    pointer_capture_target_id: Option<u64>,
    viewport_listener_actions: Vec<ViewportListenerAction>,
}

#[derive(Clone)]
pub struct EventMeta {
    state: Rc<RefCell<EventMetaState>>,
}

impl fmt::Debug for EventMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.state.borrow();
        f.debug_struct("EventMeta")
            .field("target_id", &state.target_id)
            .field("current_target_id", &state.current_target_id)
            .field("propagation_stopped", &state.propagation_stopped)
            .finish()
    }
}

impl EventMeta {
    pub fn new(target_id: u64) -> Self {
        Self {
            state: Rc::new(RefCell::new(EventMetaState {
                target_id,
                current_target_id: target_id,
                ..EventMetaState::default()
            })),
        }
    }

    pub fn target_id(&self) -> u64 {
        self.state.borrow().target_id
    }

    pub fn current_target_id(&self) -> u64 {
        self.state.borrow().current_target_id
    }

    pub fn set_target_id(&mut self, target_id: u64) {
        let mut state = self.state.borrow_mut();
        state.target_id = target_id;
        state.current_target_id = target_id;
    }

    pub fn set_current_target_id(&mut self, current_target_id: u64) {
        self.state.borrow_mut().current_target_id = current_target_id;
    }

    pub fn propagation_stopped(&self) -> bool {
        self.state.borrow().propagation_stopped
    }

    pub fn stop_propagation(&mut self) {
        self.state.borrow_mut().propagation_stopped = true;
    }

    pub fn request_keep_focus(&mut self) {
        self.state.borrow_mut().keep_focus_requested = true;
    }

    pub fn keep_focus_requested(&self) -> bool {
        self.state.borrow().keep_focus_requested
    }

    pub fn request_pointer_capture(&mut self) {
        let current_target_id = self.state.borrow().current_target_id;
        self.state.borrow_mut().pointer_capture_target_id = Some(current_target_id);
    }

    pub fn pointer_capture_target_id(&self) -> Option<u64> {
        self.state.borrow().pointer_capture_target_id
    }

    pub fn viewport(&self) -> EventViewport {
        EventViewport {
            state: self.state.clone(),
        }
    }

    pub fn take_viewport_listener_actions(&mut self) -> Vec<ViewportListenerAction> {
        std::mem::take(&mut self.state.borrow_mut().viewport_listener_actions)
    }
}

#[derive(Clone)]
pub struct EventViewport {
    state: Rc<RefCell<EventMetaState>>,
}

impl fmt::Debug for EventViewport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pending = self.state.borrow().viewport_listener_actions.len();
        f.debug_struct("EventViewport")
            .field("pending_actions", &pending)
            .finish()
    }
}

impl EventViewport {
    pub fn add_mouse_move_listener<F>(&mut self, handler: F) -> ViewportListenerHandle
    where
        F: FnMut(&mut MouseMoveEvent) + 'static,
    {
        let handler_prop = MouseMoveHandlerProp::new(handler);
        let handle = ViewportListenerHandle(handler_prop.id());
        self.state
            .borrow_mut()
            .viewport_listener_actions
            .push(ViewportListenerAction::AddMouseMoveListener(handler_prop));
        handle
    }

    pub fn add_mouse_up_listener<F>(&mut self, handler: F) -> ViewportListenerHandle
    where
        F: FnMut(&mut MouseUpEvent) + 'static,
    {
        let handler_prop = MouseUpHandlerProp::new(handler);
        let handle = ViewportListenerHandle(handler_prop.id());
        self.state
            .borrow_mut()
            .viewport_listener_actions
            .push(ViewportListenerAction::AddMouseUpListener(handler_prop));
        handle
    }

    pub fn add_mouse_up_listener_until<F>(&mut self, handler: F) -> ViewportListenerHandle
    where
        F: FnMut(&mut MouseUpEvent) -> bool + 'static,
    {
        let handler_prop = MouseUpUntilHandler::new(handler);
        let handle = ViewportListenerHandle(handler_prop.id());
        self.state.borrow_mut().viewport_listener_actions.push(
            ViewportListenerAction::AddMouseUpListenerUntil(handler_prop),
        );
        handle
    }

    pub fn remove_listener(&mut self, handle: ViewportListenerHandle) {
        self.state
            .borrow_mut()
            .viewport_listener_actions
            .push(ViewportListenerAction::RemoveListener(handle));
    }

    pub fn set_cursor(&mut self, cursor: Option<Cursor>) {
        self.state
            .borrow_mut()
            .viewport_listener_actions
            .push(ViewportListenerAction::SetCursor(cursor));
    }

    pub fn set_focus(&mut self, node_id: Option<u64>) {
        self.state
            .borrow_mut()
            .viewport_listener_actions
            .push(ViewportListenerAction::SetFocus(node_id));
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

#[derive(Debug, Clone)]
pub struct MouseDownEvent {
    pub meta: EventMeta,
    pub mouse: MouseEventData,
    pub viewport: EventViewport,
}

#[derive(Debug, Clone)]
pub struct MouseUpEvent {
    pub meta: EventMeta,
    pub mouse: MouseEventData,
    pub viewport: EventViewport,
}

#[derive(Debug, Clone)]
pub struct MouseMoveEvent {
    pub meta: EventMeta,
    pub mouse: MouseEventData,
    pub viewport: EventViewport,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct FocusEvent {
    pub meta: EventMeta,
}

#[derive(Debug, Clone)]
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
                F: for<'a> FnMut(&'a mut $event_ty) + 'static,
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

        impl<F> From<F> for $ty
        where
            F: for<'a> FnMut(&'a mut $event_ty) + 'static,
        {
            fn from(handler: F) -> Self {
                $ty::new(handler)
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
    F: for<'a> FnMut(&'a mut MouseDownEvent) + 'static,
{
    MouseDownHandlerProp::new(handler)
}

pub fn on_mouse_up<F>(handler: F) -> MouseUpHandlerProp
where
    F: for<'a> FnMut(&'a mut MouseUpEvent) + 'static,
{
    MouseUpHandlerProp::new(handler)
}

pub fn on_mouse_move<F>(handler: F) -> MouseMoveHandlerProp
where
    F: for<'a> FnMut(&'a mut MouseMoveEvent) + 'static,
{
    MouseMoveHandlerProp::new(handler)
}

pub fn on_click<F>(handler: F) -> ClickHandlerProp
where
    F: for<'a> FnMut(&'a mut ClickEvent) + 'static,
{
    ClickHandlerProp::new(handler)
}

pub fn on_key_down<F>(handler: F) -> KeyDownHandlerProp
where
    F: for<'a> FnMut(&'a mut KeyDownEvent) + 'static,
{
    KeyDownHandlerProp::new(handler)
}

pub fn on_key_up<F>(handler: F) -> KeyUpHandlerProp
where
    F: for<'a> FnMut(&'a mut KeyUpEvent) + 'static,
{
    KeyUpHandlerProp::new(handler)
}

pub fn on_focus<F>(handler: F) -> FocusHandlerProp
where
    F: for<'a> FnMut(&'a mut FocusEvent) + 'static,
{
    FocusHandlerProp::new(handler)
}

pub fn on_blur<F>(handler: F) -> BlurHandlerProp
where
    F: for<'a> FnMut(&'a mut BlurEvent) + 'static,
{
    BlurHandlerProp::new(handler)
}
