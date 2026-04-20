#![allow(missing_docs)]

//! Event payloads and handler prop types used by the retained UI runtime.

use crate::Cursor;
use crate::platform::input::{Key, Modifiers, PointerType};
use crate::ui::node_id::{EventTarget, NodeId};
use crate::view::base_component::TextAreaRenderString;
use smol_str::SmolStr;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

/// Deprecated alias kept for source-level compatibility. Prefer
/// [`crate::platform::input::Modifiers`] (re-exported as [`Modifiers`] here).
///
/// Old field-style access (`.shift`, `.ctrl`, …) no longer compiles; use the
/// accessor methods (`.shift()`, `.ctrl()`, …) on `Modifiers`.
#[allow(dead_code)]
pub type KeyModifiers = Modifiers;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PointerButtons {
    pub left: bool,
    pub right: bool,
    pub middle: bool,
    pub back: bool,
    pub forward: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ViewportListenerHandle(pub u64);

#[derive(Clone)]
pub struct PointerUpUntilHandler {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut PointerUpEvent) -> bool>>,
}

impl PointerUpUntilHandler {
    pub fn new<F>(handler: F) -> Self
    where
        F: FnMut(&mut PointerUpEvent) -> bool + 'static,
    {
        Self {
            id: next_handler_id(),
            handler: Rc::new(RefCell::new(handler)),
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn call(&self, event: &mut PointerUpEvent) -> bool {
        (self.handler.borrow_mut())(event)
    }
}

impl PartialEq for PointerUpUntilHandler {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl fmt::Debug for PointerUpUntilHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PointerUpUntilHandler")
            .field("id", &self.id)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub enum ViewportListenerAction {
    AddPointerMoveListener(PointerMoveHandlerProp),
    AddPointerUpListener(PointerUpHandlerProp),
    AddPointerUpListenerUntil(PointerUpUntilHandler),
    SetFocus(Option<NodeId>),
    SetCursor(Option<Cursor>),
    SelectTextRangeAll(NodeId),
    SelectTextRange {
        target_id: NodeId,
        start: usize,
        end: usize,
    },
    RemoveListener(ViewportListenerHandle),
}

#[derive(Default)]
struct EventMetaState {
    target: EventTarget,
    current_target: EventTarget,
    /// For pair-style events: the "other" node.
    ///
    /// - `PointerEnter` / `PointerLeave`: previously / next-hovered node
    /// - `Focus`: blur-source node (where focus moved from)
    /// - `Blur`: focus-destination node (where focus moved to)
    ///
    /// `None` when no counterpart exists (e.g. first focus, pointer
    /// entering from outside the window).
    related_target: Option<EventTarget>,
    propagation_stopped: bool,
    keep_focus_requested: bool,
    pointer_capture_target_id: Option<NodeId>,
    /// Ancestor chain from `target` up to the containing root (inclusive of
    /// both ends). Populated by the viewport dispatch layer when it has the
    /// tree available; empty for synthetic events where no path was computed.
    path: Rc<Vec<NodeId>>,
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
            .field("target", &state.target)
            .field("current_target", &state.current_target)
            .field("propagation_stopped", &state.propagation_stopped)
            .finish()
    }
}

impl EventMeta {
    pub fn new(target_id: NodeId) -> Self {
        let target = EventTarget::bare(target_id);
        Self {
            state: Rc::new(RefCell::new(EventMetaState {
                target,
                current_target: target,
                ..EventMetaState::default()
            })),
        }
    }

    /// Construct an `EventMeta` seeded with a full [`EventTarget`]
    /// (id + bounds). Prefer this over [`Self::new`] at dispatch sites
    /// that already know the node's geometry.
    pub fn with_target(target: EventTarget) -> Self {
        Self {
            state: Rc::new(RefCell::new(EventMetaState {
                target,
                current_target: target,
                ..EventMetaState::default()
            })),
        }
    }

    /// Node the event is dispatched to (the hit-test result or focused
    /// node). Stable for the duration of the dispatch.
    pub fn target(&self) -> EventTarget {
        self.state.borrow().target
    }

    /// Node currently processing the event as it bubbles. Changes as the
    /// dispatch walks up ancestors.
    pub fn current_target(&self) -> EventTarget {
        self.state.borrow().current_target
    }

    pub fn target_id(&self) -> NodeId {
        self.state.borrow().target.id
    }

    pub fn current_target_id(&self) -> NodeId {
        self.state.borrow().current_target.id
    }

    pub fn set_target_id(&mut self, target_id: NodeId) {
        let target = EventTarget::bare(target_id);
        let mut state = self.state.borrow_mut();
        state.target = target;
        state.current_target = target;
    }

    pub fn set_current_target_id(&mut self, current_target_id: NodeId) {
        self.state.borrow_mut().current_target.id = current_target_id;
    }

    /// Replace the `target` (the node that was originally hit). Usually
    /// called once at dispatch start.
    pub fn set_target(&mut self, target: EventTarget) {
        let mut state = self.state.borrow_mut();
        state.target = target;
        state.current_target = target;
    }

    /// Update the `current_target` as the event bubbles up. Prefer this
    /// over [`Self::set_current_target_id`] so ancestor bounds propagate
    /// into handlers.
    pub fn set_current_target(&mut self, current_target: EventTarget) {
        self.state.borrow_mut().current_target = current_target;
    }

    pub fn propagation_stopped(&self) -> bool {
        self.state.borrow().propagation_stopped
    }

    pub fn stop_propagation(&mut self) {
        self.state.borrow_mut().propagation_stopped = true;
    }

    pub fn keep_focus(&mut self) {
        self.state.borrow_mut().keep_focus_requested = true;
    }

    pub fn keep_focus_requested(&self) -> bool {
        self.state.borrow().keep_focus_requested
    }

    pub fn request_pointer_capture(&mut self) {
        let current_target_id = self.state.borrow().current_target.id;
        self.state.borrow_mut().pointer_capture_target_id = Some(current_target_id);
    }

    pub fn pointer_capture_target_id(&self) -> Option<NodeId> {
        self.state.borrow().pointer_capture_target_id
    }

    /// Ancestor chain walked during dispatch: `target` first, root last
    /// (DOM `composedPath()` ordering). Empty when the dispatch site did
    /// not populate it (e.g. synthetic focus events, unit tests).
    ///
    /// Clones the shared path handle — cheap (`Rc` bump). Handlers that
    /// need to iterate many times should cache the returned `Rc`.
    pub fn composed_path(&self) -> Rc<Vec<NodeId>> {
        Rc::clone(&self.state.borrow().path)
    }

    /// Replace the dispatch path. Called by the viewport dispatcher before
    /// walking ancestors; user code should not need this.
    pub(crate) fn set_path(&mut self, path: Vec<NodeId>) {
        self.state.borrow_mut().path = Rc::new(path);
    }

    /// Counterpart node for pair-style events (see
    /// [`EventMetaState::related_target`] for semantics per event).
    pub fn related_target(&self) -> Option<EventTarget> {
        self.state.borrow().related_target
    }

    /// Set the `related_target`. Called by the dispatcher when it knows the
    /// counterpart (previous hover, focus source, …).
    pub(crate) fn set_related_target(&mut self, related: Option<EventTarget>) {
        self.state.borrow_mut().related_target = related;
    }

    pub fn viewport(&self) -> EventViewport {
        EventViewport {
            state: self.state.clone(),
        }
    }

    pub fn take_viewport_listener_actions(&mut self) -> Vec<ViewportListenerAction> {
        std::mem::take(&mut self.state.borrow_mut().viewport_listener_actions)
    }

    pub(crate) fn text_selection_target(&self, target_id: NodeId) -> TextSelectionTarget {
        TextSelectionTarget {
            state: self.state.clone(),
            target_id,
        }
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
    pub fn add_pointer_move_listener<F>(&mut self, handler: F) -> ViewportListenerHandle
    where
        F: FnMut(&mut PointerMoveEvent) + 'static,
    {
        let handler_prop = PointerMoveHandlerProp::new(handler);
        let handle = ViewportListenerHandle(handler_prop.id());
        self.state
            .borrow_mut()
            .viewport_listener_actions
            .push(ViewportListenerAction::AddPointerMoveListener(handler_prop));
        handle
    }

    pub fn add_pointer_up_listener<F>(&mut self, handler: F) -> ViewportListenerHandle
    where
        F: FnMut(&mut PointerUpEvent) + 'static,
    {
        let handler_prop = PointerUpHandlerProp::new(handler);
        let handle = ViewportListenerHandle(handler_prop.id());
        self.state
            .borrow_mut()
            .viewport_listener_actions
            .push(ViewportListenerAction::AddPointerUpListener(handler_prop));
        handle
    }

    pub fn add_pointer_up_listener_until<F>(&mut self, handler: F) -> ViewportListenerHandle
    where
        F: FnMut(&mut PointerUpEvent) -> bool + 'static,
    {
        let handler_prop = PointerUpUntilHandler::new(handler);
        let handle = ViewportListenerHandle(handler_prop.id());
        self.state.borrow_mut().viewport_listener_actions.push(
            ViewportListenerAction::AddPointerUpListenerUntil(handler_prop),
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

    pub fn set_focus(&mut self, node_id: Option<NodeId>) {
        self.state
            .borrow_mut()
            .viewport_listener_actions
            .push(ViewportListenerAction::SetFocus(node_id));
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerEventData {
    pub viewport_x: f32,
    pub viewport_y: f32,
    pub local_x: f32,
    pub local_y: f32,
    pub button: Option<PointerButton>,
    pub buttons: PointerButtons,
    pub modifiers: KeyModifiers,
    pub pointer_id: u64,
    pub pointer_type: PointerType,
    pub pressure: f32,
    pub timestamp: crate::time::Instant,
}

#[derive(Debug, Clone)]
pub struct KeyEventData {
    /// Physical key identifier (layout-independent).
    pub key: Key,
    /// Layout-applied text output. `None` for non-character keys.
    pub characters: Option<SmolStr>,
    pub modifiers: Modifiers,
    pub repeat: bool,
    /// True while an IME composition is active. Handlers typically early-return
    /// so the IME can consume the key (e.g. Enter commits, not newline).
    pub is_composing: bool,
    pub timestamp: crate::time::Instant,
}

impl KeyEventData {
    /// Convenience for shortcut matching: physical key equality plus exact
    /// (non-lock) modifier set, and not during IME composition.
    #[inline]
    pub fn shortcut(&self, key: Key, mods: Modifiers) -> bool {
        self.key == key && self.modifiers.exactly(mods) && !self.is_composing
    }
}

#[derive(Debug, Clone)]
pub struct PointerDownEvent {
    pub meta: EventMeta,
    pub pointer: PointerEventData,
    pub viewport: EventViewport,
}

#[derive(Debug, Clone)]
pub struct PointerUpEvent {
    pub meta: EventMeta,
    pub pointer: PointerEventData,
    pub viewport: EventViewport,
}

#[derive(Debug, Clone)]
pub struct PointerMoveEvent {
    pub meta: EventMeta,
    pub pointer: PointerEventData,
    pub viewport: EventViewport,
}

#[derive(Debug, Clone)]
pub struct PointerEnterEvent {
    pub meta: EventMeta,
}

#[derive(Debug, Clone)]
pub struct PointerLeaveEvent {
    pub meta: EventMeta,
}

#[derive(Debug, Clone)]
pub struct ClickEvent {
    pub meta: EventMeta,
    pub pointer: PointerEventData,
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
pub struct TextChangeEvent {
    pub meta: EventMeta,
    pub value: String,
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

#[derive(Clone)]
pub struct TextSelectionTarget {
    state: Rc<RefCell<EventMetaState>>,
    target_id: NodeId,
}

impl fmt::Debug for TextSelectionTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextSelectionTarget")
            .field("target_id", &self.target_id)
            .finish()
    }
}

impl TextSelectionTarget {
    pub fn select_all(&mut self) {
        self.state
            .borrow_mut()
            .viewport_listener_actions
            .push(ViewportListenerAction::SelectTextRangeAll(self.target_id));
    }

    pub fn select_range(&mut self, start: usize, end: usize) {
        self.state.borrow_mut().viewport_listener_actions.push(
            ViewportListenerAction::SelectTextRange {
                target_id: self.target_id,
                start,
                end,
            },
        );
    }
}

#[derive(Debug, Clone)]
pub struct TextAreaFocusEvent {
    pub meta: EventMeta,
    pub target: TextSelectionTarget,
}

#[derive(Debug, Clone)]
pub struct BlurEvent {
    pub meta: EventMeta,
}

#[derive(Clone)]
pub struct PointerDownHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut PointerDownEvent)>>,
}

#[derive(Clone)]
pub struct PointerUpHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut PointerUpEvent)>>,
}

#[derive(Clone)]
pub struct PointerMoveHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut PointerMoveEvent)>>,
}

#[derive(Clone)]
pub struct PointerEnterHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut PointerEnterEvent)>>,
}

#[derive(Clone)]
pub struct PointerLeaveHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut PointerLeaveEvent)>>,
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

#[derive(Clone)]
pub struct TextAreaFocusHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut TextAreaFocusEvent)>>,
}

#[derive(Clone)]
pub struct TextChangeHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut TextChangeEvent)>>,
}

#[derive(Clone)]
pub struct TextAreaRenderHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(&mut TextAreaRenderString)>>,
}

pub struct NoArgHandler<F>(F);

pub fn no_arg_handler<F>(handler: F) -> NoArgHandler<F> {
    NoArgHandler(handler)
}

pub trait IntoEventHandlerProp<T> {
    fn into_event_handler_prop(self) -> T;
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

macro_rules! impl_into_event_handler_prop {
    ($handler_ty:ty, $event_ty:ty, $into_fn:ident) => {
        impl IntoEventHandlerProp<$handler_ty> for $handler_ty {
            fn into_event_handler_prop(self) -> $handler_ty {
                self
            }
        }

        impl<F> IntoEventHandlerProp<$handler_ty> for F
        where
            F: for<'a> FnMut(&'a mut $event_ty) + 'static,
        {
            fn into_event_handler_prop(self) -> $handler_ty {
                <$handler_ty>::new(self)
            }
        }

        impl<F> IntoEventHandlerProp<$handler_ty> for NoArgHandler<F>
        where
            F: FnMut() + 'static,
        {
            fn into_event_handler_prop(self) -> $handler_ty {
                let mut handler = self.0;
                <$handler_ty>::new(move |_event: &mut $event_ty| {
                    handler();
                })
            }
        }

        pub fn $into_fn<H>(handler: H) -> $handler_ty
        where
            H: IntoEventHandlerProp<$handler_ty>,
        {
            handler.into_event_handler_prop()
        }
    };
}

impl_handler_prop!(PointerDownHandlerProp, PointerDownEvent);
impl_handler_prop!(PointerUpHandlerProp, PointerUpEvent);
impl_handler_prop!(PointerMoveHandlerProp, PointerMoveEvent);
impl_handler_prop!(PointerEnterHandlerProp, PointerEnterEvent);
impl_handler_prop!(PointerLeaveHandlerProp, PointerLeaveEvent);
impl_handler_prop!(ClickHandlerProp, ClickEvent);
impl_handler_prop!(KeyDownHandlerProp, KeyDownEvent);
impl_handler_prop!(KeyUpHandlerProp, KeyUpEvent);
impl_handler_prop!(FocusHandlerProp, FocusEvent);
impl_handler_prop!(BlurHandlerProp, BlurEvent);
impl_handler_prop!(TextAreaFocusHandlerProp, TextAreaFocusEvent);
impl_handler_prop!(TextChangeHandlerProp, TextChangeEvent);
impl_handler_prop!(TextAreaRenderHandlerProp, TextAreaRenderString);

impl_into_event_handler_prop!(
    PointerDownHandlerProp,
    PointerDownEvent,
    into_pointer_down_handler
);
impl_into_event_handler_prop!(PointerUpHandlerProp, PointerUpEvent, into_pointer_up_handler);
impl_into_event_handler_prop!(
    PointerMoveHandlerProp,
    PointerMoveEvent,
    into_pointer_move_handler
);
impl_into_event_handler_prop!(
    PointerEnterHandlerProp,
    PointerEnterEvent,
    into_pointer_enter_handler
);
impl_into_event_handler_prop!(
    PointerLeaveHandlerProp,
    PointerLeaveEvent,
    into_pointer_leave_handler
);
impl_into_event_handler_prop!(ClickHandlerProp, ClickEvent, into_click_handler);
impl_into_event_handler_prop!(KeyDownHandlerProp, KeyDownEvent, into_key_down_handler);
impl_into_event_handler_prop!(KeyUpHandlerProp, KeyUpEvent, into_key_up_handler);
impl_into_event_handler_prop!(FocusHandlerProp, FocusEvent, into_focus_handler);
impl_into_event_handler_prop!(BlurHandlerProp, BlurEvent, into_blur_handler);
impl_into_event_handler_prop!(
    TextAreaFocusHandlerProp,
    TextAreaFocusEvent,
    into_text_area_focus_handler
);
impl_into_event_handler_prop!(
    TextChangeHandlerProp,
    TextChangeEvent,
    into_text_change_handler
);
impl_into_event_handler_prop!(
    TextAreaRenderHandlerProp,
    TextAreaRenderString,
    into_text_area_render_handler
);

pub fn on_pointer_down<F>(handler: F) -> PointerDownHandlerProp
where
    F: FnMut(&mut PointerDownEvent) + 'static,
{
    PointerDownHandlerProp::new(handler)
}

pub fn on_pointer_up<F>(handler: F) -> PointerUpHandlerProp
where
    F: FnMut(&mut PointerUpEvent) + 'static,
{
    PointerUpHandlerProp::new(handler)
}

pub fn on_pointer_move<F>(handler: F) -> PointerMoveHandlerProp
where
    F: FnMut(&mut PointerMoveEvent) + 'static,
{
    PointerMoveHandlerProp::new(handler)
}

pub fn on_pointer_enter<F>(handler: F) -> PointerEnterHandlerProp
where
    F: FnMut(&mut PointerEnterEvent) + 'static,
{
    PointerEnterHandlerProp::new(handler)
}

pub fn on_pointer_leave<F>(handler: F) -> PointerLeaveHandlerProp
where
    F: FnMut(&mut PointerLeaveEvent) + 'static,
{
    PointerLeaveHandlerProp::new(handler)
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

pub fn on_text_area_focus<F>(handler: F) -> TextAreaFocusHandlerProp
where
    F: FnMut(&mut TextAreaFocusEvent) + 'static,
{
    TextAreaFocusHandlerProp::new(handler)
}

pub fn on_blur<F>(handler: F) -> BlurHandlerProp
where
    F: FnMut(&mut BlurEvent) + 'static,
{
    BlurHandlerProp::new(handler)
}

pub fn on_change<F>(handler: F) -> TextChangeHandlerProp
where
    F: FnMut(&mut TextChangeEvent) + 'static,
{
    TextChangeHandlerProp::new(handler)
}

pub fn on_text_area_render<F>(handler: F) -> TextAreaRenderHandlerProp
where
    F: FnMut(&mut TextAreaRenderString) + 'static,
{
    TextAreaRenderHandlerProp::new(handler)
}
