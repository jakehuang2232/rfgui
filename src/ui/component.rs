#![allow(missing_docs)]

//! Component traits and helper APIs used by typed RSX components.

use std::any::TypeId;
use std::ptr::NonNull;

use crate::ui::{
    GlobalKey, RsxKey, RsxNode, RsxNodeIdentity, RsxTagDescriptor, build_scope,
    current_build_depth, register_global_key, with_component_key,
};


pub trait RsxChildrenPolicy {
    const ACCEPTS_CHILDREN: bool;
}

pub trait OptionalDefault: Sized {
    fn optional_default() -> Self;
}

pub trait IntoOptionalProp<T> {
    fn into_optional_prop(self) -> Option<T>;
}

pub trait BooleanPropMarker {}

impl BooleanPropMarker for bool {}

impl<T> IntoOptionalProp<T> for Option<T> {
    fn into_optional_prop(self) -> Option<T> {
        self
    }
}

impl<T> IntoOptionalProp<T> for T {
    fn into_optional_prop(self) -> Option<T> {
        Some(self)
    }
}

macro_rules! impl_event_into_optional_prop {
    ($handler_ty:ty, $event_ty:ty) => {
        impl<F> IntoOptionalProp<$handler_ty> for F
        where
            F: FnMut(&mut $event_ty) + 'static,
        {
            fn into_optional_prop(self) -> Option<$handler_ty> {
                Some(<$handler_ty>::new(self))
            }
        }
    };
}

macro_rules! impl_no_arg_event_into_optional_prop {
    ($handler_ty:ty, $into_fn:path) => {
        impl<F> IntoOptionalProp<$handler_ty> for crate::ui::NoArgHandler<F>
        where
            F: FnMut() + 'static,
        {
            fn into_optional_prop(self) -> Option<$handler_ty> {
                Some($into_fn(self))
            }
        }
    };
}

impl_event_into_optional_prop!(crate::ui::PointerDownHandlerProp, crate::ui::PointerDownEvent);
impl_event_into_optional_prop!(crate::ui::PointerUpHandlerProp, crate::ui::PointerUpEvent);
impl_event_into_optional_prop!(crate::ui::PointerMoveHandlerProp, crate::ui::PointerMoveEvent);
impl_event_into_optional_prop!(crate::ui::PointerEnterHandlerProp, crate::ui::PointerEnterEvent);
impl_event_into_optional_prop!(crate::ui::PointerLeaveHandlerProp, crate::ui::PointerLeaveEvent);
impl_event_into_optional_prop!(crate::ui::ClickHandlerProp, crate::ui::ClickEvent);
impl_event_into_optional_prop!(
    crate::ui::ContextMenuHandlerProp,
    crate::ui::ContextMenuEvent
);
impl_event_into_optional_prop!(crate::ui::WheelHandlerProp, crate::ui::WheelEvent);
impl_event_into_optional_prop!(crate::ui::ImeCommitHandlerProp, crate::ui::ImeCommitEvent);
impl_event_into_optional_prop!(crate::ui::ImeEnabledHandlerProp, crate::ui::ImeEnabledEvent);
impl_event_into_optional_prop!(crate::ui::ImeDisabledHandlerProp, crate::ui::ImeDisabledEvent);
impl_event_into_optional_prop!(crate::ui::DragStartHandlerProp, crate::ui::DragStartEvent);
impl_event_into_optional_prop!(crate::ui::DragOverHandlerProp, crate::ui::DragOverEvent);
impl_event_into_optional_prop!(crate::ui::DragLeaveHandlerProp, crate::ui::DragLeaveEvent);
impl_event_into_optional_prop!(crate::ui::DropHandlerProp, crate::ui::DropEvent);
impl_event_into_optional_prop!(crate::ui::DragEndHandlerProp, crate::ui::DragEndEvent);
impl_event_into_optional_prop!(crate::ui::CopyHandlerProp, crate::ui::CopyEvent);
impl_event_into_optional_prop!(crate::ui::CutHandlerProp, crate::ui::CutEvent);
impl_event_into_optional_prop!(crate::ui::PasteHandlerProp, crate::ui::PasteEvent);
impl_event_into_optional_prop!(crate::ui::KeyDownHandlerProp, crate::ui::KeyDownEvent);
impl_event_into_optional_prop!(crate::ui::KeyUpHandlerProp, crate::ui::KeyUpEvent);
impl_event_into_optional_prop!(crate::ui::FocusHandlerProp, crate::ui::FocusEvent);
impl_event_into_optional_prop!(crate::ui::BlurHandlerProp, crate::ui::BlurEvent);
impl_event_into_optional_prop!(
    crate::ui::TextAreaFocusHandlerProp,
    crate::ui::TextAreaFocusEvent
);
impl_event_into_optional_prop!(crate::ui::TextChangeHandlerProp, crate::ui::TextChangeEvent);
impl_event_into_optional_prop!(
    crate::ui::TextAreaRenderHandlerProp,
    crate::view::base_component::TextAreaRenderString
);
impl_no_arg_event_into_optional_prop!(
    crate::ui::PointerDownHandlerProp,
    crate::ui::into_pointer_down_handler
);
impl_no_arg_event_into_optional_prop!(
    crate::ui::PointerUpHandlerProp,
    crate::ui::into_pointer_up_handler
);
impl_no_arg_event_into_optional_prop!(
    crate::ui::PointerMoveHandlerProp,
    crate::ui::into_pointer_move_handler
);
impl_no_arg_event_into_optional_prop!(
    crate::ui::PointerEnterHandlerProp,
    crate::ui::into_pointer_enter_handler
);
impl_no_arg_event_into_optional_prop!(
    crate::ui::PointerLeaveHandlerProp,
    crate::ui::into_pointer_leave_handler
);
impl_no_arg_event_into_optional_prop!(crate::ui::ClickHandlerProp, crate::ui::into_click_handler);
impl_no_arg_event_into_optional_prop!(
    crate::ui::KeyDownHandlerProp,
    crate::ui::into_key_down_handler
);
impl_no_arg_event_into_optional_prop!(crate::ui::KeyUpHandlerProp, crate::ui::into_key_up_handler);
impl_no_arg_event_into_optional_prop!(crate::ui::FocusHandlerProp, crate::ui::into_focus_handler);
impl_no_arg_event_into_optional_prop!(crate::ui::BlurHandlerProp, crate::ui::into_blur_handler);
impl_no_arg_event_into_optional_prop!(
    crate::ui::TextAreaFocusHandlerProp,
    crate::ui::into_text_area_focus_handler
);
impl_no_arg_event_into_optional_prop!(
    crate::ui::TextChangeHandlerProp,
    crate::ui::into_text_change_handler
);

impl<'a> IntoOptionalProp<crate::Color> for crate::HexColor<'a> {
    fn into_optional_prop(self) -> Option<crate::Color> {
        Some(crate::IntoColor::<crate::Color>::into_color(self))
    }
}

impl IntoOptionalProp<Box<dyn crate::ColorLike>> for &str {
    fn into_optional_prop(self) -> Option<Box<dyn crate::ColorLike>> {
        Some(Box::new(crate::IntoColor::<crate::Color>::into_color(self)))
    }
}

impl IntoOptionalProp<Box<dyn crate::ColorLike>> for String {
    fn into_optional_prop(self) -> Option<Box<dyn crate::ColorLike>> {
        Some(Box::new(crate::IntoColor::<crate::Color>::into_color(self)))
    }
}

impl IntoOptionalProp<Box<dyn crate::ColorLike>> for crate::Color {
    fn into_optional_prop(self) -> Option<Box<dyn crate::ColorLike>> {
        Some(Box::new(self))
    }
}

impl<'a> IntoOptionalProp<Box<dyn crate::ColorLike>> for crate::HexColor<'a> {
    fn into_optional_prop(self) -> Option<Box<dyn crate::ColorLike>> {
        Some(Box::new(crate::IntoColor::<crate::Color>::into_color(self)))
    }
}

// Background accepts colors, gradients, and gradient builders.
// (Background → Option<Background> is handled by the blanket `impl<T> IntoOptionalProp<T> for T`.)
impl IntoOptionalProp<crate::Background> for crate::Gradient {
    fn into_optional_prop(self) -> Option<crate::Background> {
        Some(crate::Background::Gradient(self))
    }
}

impl IntoOptionalProp<crate::Background> for crate::LinearBuilder {
    fn into_optional_prop(self) -> Option<crate::Background> {
        Some(crate::Background::Gradient(self.build()))
    }
}

impl IntoOptionalProp<crate::Background> for crate::RadialBuilder {
    fn into_optional_prop(self) -> Option<crate::Background> {
        Some(crate::Background::Gradient(self.build()))
    }
}

impl IntoOptionalProp<crate::Background> for crate::ConicBuilder {
    fn into_optional_prop(self) -> Option<crate::Background> {
        Some(crate::Background::Gradient(self.build()))
    }
}

impl IntoOptionalProp<crate::Background> for Box<dyn crate::ColorLike> {
    fn into_optional_prop(self) -> Option<crate::Background> {
        Some(crate::Background::Color(self))
    }
}

impl IntoOptionalProp<crate::Background> for crate::Color {
    fn into_optional_prop(self) -> Option<crate::Background> {
        Some(crate::Background::Color(Box::new(self)))
    }
}

impl<'a> IntoOptionalProp<crate::Background> for crate::HexColor<'a> {
    fn into_optional_prop(self) -> Option<crate::Background> {
        Some(crate::Background::Color(Box::new(
            crate::IntoColor::<crate::Color>::into_color(self),
        )))
    }
}

impl IntoOptionalProp<crate::Background> for &str {
    fn into_optional_prop(self) -> Option<crate::Background> {
        Some(crate::Background::Color(Box::new(
            crate::IntoColor::<crate::Color>::into_color(self),
        )))
    }
}

impl IntoOptionalProp<crate::Background> for String {
    fn into_optional_prop(self) -> Option<crate::Background> {
        Some(crate::Background::Color(Box::new(
            crate::IntoColor::<crate::Color>::into_color(self),
        )))
    }
}

// Gradient (for background_image / border_image fields): accept builders too.
impl IntoOptionalProp<crate::Gradient> for crate::LinearBuilder {
    fn into_optional_prop(self) -> Option<crate::Gradient> {
        Some(self.build())
    }
}

impl IntoOptionalProp<crate::Gradient> for crate::RadialBuilder {
    fn into_optional_prop(self) -> Option<crate::Gradient> {
        Some(self.build())
    }
}

impl IntoOptionalProp<crate::Gradient> for crate::ConicBuilder {
    fn into_optional_prop(self) -> Option<crate::Gradient> {
        Some(self.build())
    }
}

impl IntoOptionalProp<String> for &str {
    fn into_optional_prop(self) -> Option<String> {
        Some(self.to_string())
    }
}

macro_rules! impl_numeric_into_optional_length {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoOptionalProp<crate::Length> for $ty {
                fn into_optional_prop(self) -> Option<crate::Length> {
                    Some(crate::Length::px(self as f32))
                }
            }
        )*
    };
}

impl_numeric_into_optional_length!(i32, i64, u32, usize, f32, f64);

impl IntoOptionalProp<crate::BorderRadius> for crate::Length {
    fn into_optional_prop(self) -> Option<crate::BorderRadius> {
        Some(crate::BorderRadius::uniform(self))
    }
}

macro_rules! impl_numeric_into_optional_border_radius {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoOptionalProp<crate::BorderRadius> for $ty {
                fn into_optional_prop(self) -> Option<crate::BorderRadius> {
                    Some(crate::BorderRadius::uniform(crate::Length::px(self as f32)))
                }
            }
        )*
    };
}

impl_numeric_into_optional_border_radius!(i32, i64, u32, usize, f32, f64);

macro_rules! impl_numeric_into_optional_font_weight {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoOptionalProp<crate::FontWeight> for $ty {
                fn into_optional_prop(self) -> Option<crate::FontWeight> {
                    Some(crate::FontWeight::new((self as i64).max(0) as u16))
                }
            }
        )*
    };
}

impl_numeric_into_optional_font_weight!(i32, i64, u32, usize, u16);

macro_rules! impl_numeric_into_optional_opacity {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoOptionalProp<crate::Opacity> for $ty {
                fn into_optional_prop(self) -> Option<crate::Opacity> {
                    Some(crate::Opacity::new(self as f32))
                }
            }
        )*
    };
}

impl_numeric_into_optional_opacity!(i32, i64, u32, usize, f32, f64);

impl IntoOptionalProp<crate::Layout> for crate::FlexLayout {
    fn into_optional_prop(self) -> Option<crate::Layout> {
        Some(self.into())
    }
}

impl IntoOptionalProp<crate::Layout> for crate::FlowLayout {
    fn into_optional_prop(self) -> Option<crate::Layout> {
        Some(self.into())
    }
}

impl IntoOptionalProp<crate::Transitions> for crate::Transition {
    fn into_optional_prop(self) -> Option<crate::Transitions> {
        Some(self.into())
    }
}

impl IntoOptionalProp<crate::Transitions> for Vec<crate::Transition> {
    fn into_optional_prop(self) -> Option<crate::Transitions> {
        Some(self.into())
    }
}

impl<const N: usize> IntoOptionalProp<crate::Transitions> for [crate::Transition; N] {
    fn into_optional_prop(self) -> Option<crate::Transitions> {
        Some(self.into())
    }
}

impl IntoOptionalProp<crate::Animator> for crate::Animation {
    fn into_optional_prop(self) -> Option<crate::Animator> {
        Some(crate::Animator::new([self]))
    }
}

impl IntoOptionalProp<crate::Animator> for Vec<crate::Animation> {
    fn into_optional_prop(self) -> Option<crate::Animator> {
        Some(crate::Animator::from_vec(self))
    }
}

impl<const N: usize> IntoOptionalProp<crate::Animator> for [crate::Animation; N] {
    fn into_optional_prop(self) -> Option<crate::Animator> {
        Some(crate::Animator::new(self))
    }
}

impl IntoOptionalProp<f64> for i32 {
    fn into_optional_prop(self) -> Option<f64> {
        Some(self as f64)
    }
}

impl IntoOptionalProp<f64> for i64 {
    fn into_optional_prop(self) -> Option<f64> {
        Some(self as f64)
    }
}

impl IntoOptionalProp<f64> for u32 {
    fn into_optional_prop(self) -> Option<f64> {
        Some(self as f64)
    }
}

impl IntoOptionalProp<f64> for usize {
    fn into_optional_prop(self) -> Option<f64> {
        Some(self as f64)
    }
}

impl IntoOptionalProp<f64> for f32 {
    fn into_optional_prop(self) -> Option<f64> {
        Some(self as f64)
    }
}

impl IntoOptionalProp<i64> for i32 {
    fn into_optional_prop(self) -> Option<i64> {
        Some(self as i64)
    }
}

impl IntoOptionalProp<i64> for u32 {
    fn into_optional_prop(self) -> Option<i64> {
        Some(self as i64)
    }
}

impl IntoOptionalProp<crate::FontSize> for f32 {
    fn into_optional_prop(self) -> Option<crate::FontSize> {
        Some(crate::FontSize::px(self))
    }
}

impl IntoOptionalProp<crate::FontSize> for f64 {
    fn into_optional_prop(self) -> Option<crate::FontSize> {
        Some(crate::FontSize::px(self as f32))
    }
}

impl IntoOptionalProp<crate::FontSize> for i32 {
    fn into_optional_prop(self) -> Option<crate::FontSize> {
        Some(crate::FontSize::px(self as f32))
    }
}

impl IntoOptionalProp<crate::FontSize> for i64 {
    fn into_optional_prop(self) -> Option<crate::FontSize> {
        Some(crate::FontSize::px(self as f32))
    }
}

impl IntoOptionalProp<crate::FontSize> for u32 {
    fn into_optional_prop(self) -> Option<crate::FontSize> {
        Some(crate::FontSize::px(self as f32))
    }
}

impl IntoOptionalProp<crate::FontSize> for usize {
    fn into_optional_prop(self) -> Option<crate::FontSize> {
        Some(crate::FontSize::px(self as f32))
    }
}

pub trait RsxComponent<Props>: Sized {
    fn render(props: Props, children: Vec<RsxNode>) -> RsxNode;
}

pub trait IntoRsxChildren {
    fn into_rsx_children(self) -> Vec<RsxNode>;
}

impl IntoRsxChildren for () {
    fn into_rsx_children(self) -> Vec<RsxNode> {
        Vec::new()
    }
}

impl IntoRsxChildren for RsxNode {
    fn into_rsx_children(self) -> Vec<RsxNode> {
        vec![self]
    }
}

impl IntoRsxChildren for &str {
    fn into_rsx_children(self) -> Vec<RsxNode> {
        vec![RsxNode::text(self)]
    }
}

impl IntoRsxChildren for String {
    fn into_rsx_children(self) -> Vec<RsxNode> {
        vec![RsxNode::text(self)]
    }
}

impl IntoRsxChildren for Vec<RsxNode> {
    fn into_rsx_children(self) -> Vec<RsxNode> {
        self
    }
}

impl IntoRsxChildren for Option<RsxNode> {
    fn into_rsx_children(self) -> Vec<RsxNode> {
        self.into_iter().collect()
    }
}

pub fn append_rsx_child_node<T>(children: &mut Vec<RsxNode>, value: T)
where
    T: IntoRsxChildren,
{
    children.extend(value.into_rsx_children());
}

// ---------- React parity P0: compile-time type-erased component dispatch ----------
//
// Infrastructure for the eventual React-style lazy top-down render pipeline
// (see memory: project_react_parity_lazy_render.md). Dead code in P0 —
// no runtime path produces or consumes these yet. P2 wires `rsx-macro` to
// emit `RsxNode::Component(ComponentNodeInner)` for user components and an
// `unwrap_components` walker to invoke `vtable.render` top-down.
//
// Manual vtable avoids `dyn Trait` / `Any` / downcast: the `#[component]`
// macro emits shims that `unsafe` cast the erased `NonNull<()>` back to the
// concrete props type (always valid — the shim is monomorphized per T).

/// Per-component-type static dispatch table. One `const` lives per `T` that
/// implements `ComponentTag`, emitted by the `#[component]` macro.
pub struct ComponentVTable {
    /// Invoke `T::render(*Box::from_raw(props), children)`. Consumes props.
    pub render: unsafe fn(NonNull<()>, Vec<RsxNode>) -> RsxNode,
    /// Drop `Box<T::Props>` when the containing `ComponentNodeInner` is
    /// dropped without being rendered.
    pub drop_props: unsafe fn(NonNull<()>),
    /// Deep-clone `Box<T::Props>` into a freshly boxed owned value,
    /// leaving the original intact. Used by the walker when a
    /// `RsxNode::Component` arrives with a shared `Rc` — one copy
    /// survives in the other holder, the clone is consumed by
    /// `vtable.render`. Requires `T::StrictProps: Clone`; the
    /// `#[component]` macro emits `#[derive(Clone)]` on the Props
    /// struct and the corresponding shim.
    pub clone_props: unsafe fn(NonNull<()>) -> NonNull<()>,
    /// Structural equality of two boxed `T::Props`. `None` opts out of
    /// memoization (each render re-invokes `render`). Emitted as
    /// `Some(_)` only when the user derives `PartialEq` on the props struct.
    pub props_eq: Option<unsafe fn(NonNull<()>, NonNull<()>) -> bool>,
    pub type_name: &'static str,
}

/// Deferred component description. Carries type-erased boxed props + a
/// vtable pointer; the concrete `T::Props` is recovered by the vtable's
/// shims. `children` is always `Vec<RsxNode>` — typed children is not
/// supported (see design memo).
///
/// Wrapped in `Rc<ComponentNodeInner>` as the `RsxNode::Component` variant.
/// Owns the boxed props — drops via `vtable.drop_props` on `Drop`.
pub struct ComponentNodeInner {
    pub identity: RsxNodeIdentity,
    pub type_id: TypeId,
    pub props: NonNull<()>,
    pub children: Vec<RsxNode>,
    pub key: Option<RsxKey>,
    pub vtable: &'static ComponentVTable,
    /// React parity P3: context stack captured at the moment this
    /// Component was constructed. Walker installs this before invoking
    /// `vtable.render` so `use_context` resolves providers that were
    /// on-stack at rsx! expansion time but have since been popped.
    pub context_snapshot: crate::ui::ContextSnapshot,
}

impl Drop for ComponentNodeInner {
    fn drop(&mut self) {
        // Safety: `props` was produced by `Box::into_raw(Box::new(T::Props))`
        // at construction, and the vtable's `drop_props` shim is the
        // monomorphized `Box<T::Props>` dropper for exactly that T.
        unsafe { (self.vtable.drop_props)(self.props) };
    }
}

/// Destructured form of a [`ComponentNodeInner`] prepared for the
/// `unwrap_components` walker. Obtaining a `RenderParts` transfers
/// ownership of the boxed props to the caller, so the caller becomes
/// responsible for either invoking `vtable.render` (which consumes them)
/// or calling `vtable.drop_props`.
pub struct ComponentRenderParts {
    pub identity: RsxNodeIdentity,
    pub type_id: TypeId,
    pub key: Option<RsxKey>,
    pub children: Vec<RsxNode>,
    pub props: NonNull<()>,
    pub vtable: &'static ComponentVTable,
    pub context_snapshot: crate::ui::ContextSnapshot,
}

impl ComponentNodeInner {
    /// Extract owned fields for the rendering walker. Suppresses the
    /// normal `Drop` impl (which would call `drop_props`) because the
    /// caller now owns the boxed props — typically to feed them into
    /// `vtable.render`, which takes ownership in turn.
    ///
    /// # Panics
    /// Panics if the `Rc` is shared. rsx-macro always constructs
    /// Component nodes with a fresh unique `Rc`; the walker pops one
    /// reference when descending, and no other code should hold one.
    pub fn into_render_parts(self: std::rc::Rc<Self>) -> ComponentRenderParts {
        match std::rc::Rc::try_unwrap(self) {
            Ok(inner) => {
                // Unique Rc — move props ownership out. Suppress Drop
                // via `ManuallyDrop` since the caller now owns `props`
                // and will consume it through `vtable.render`.
                let me = std::mem::ManuallyDrop::new(inner);
                // Safety: `me` is a `ManuallyDrop` wrapper we never read
                // again after this extraction; the duplicated
                // `Vec<RsxNode>` ownership from `ptr::read` is fine
                // because the source is never dropped.
                let children = unsafe { std::ptr::read(&me.children) };
                let context_snapshot = unsafe { std::ptr::read(&me.context_snapshot) };
                ComponentRenderParts {
                    identity: me.identity,
                    type_id: me.type_id,
                    key: me.key,
                    children,
                    props: me.props,
                    vtable: me.vtable,
                    context_snapshot,
                }
            }
            Err(rc) => {
                // Shared Rc — another holder exists (common when a
                // subtree is extracted into a variable and `.clone()`
                // ed into two positions, or when memo stores a prior
                // render). Deep-clone the boxed props via the vtable's
                // shim so this call site owns its copy; the other
                // holder keeps the original and will run `drop_props`
                // on its own drop.
                // Safety: `clone_props` was emitted by the `#[component]`
                // macro as a monomorphized shim over `<T::Props as Clone>`.
                let cloned_props = unsafe { (rc.vtable.clone_props)(rc.props) };
                // Snapshot cloning is cheap (`Rc<dyn Any>` clones).
                let context_snapshot = rc
                    .context_snapshot
                    .iter()
                    .map(|entry| crate::ui::ContextStackEntry {
                        type_id: entry.type_id,
                        stack: entry.stack.clone(),
                    })
                    .collect();
                ComponentRenderParts {
                    identity: rc.identity,
                    type_id: rc.type_id,
                    key: rc.key,
                    children: rc.children.clone(),
                    props: cloned_props,
                    vtable: rc.vtable,
                    context_snapshot,
                }
            }
        }
    }
}

/// Top-down walker: invoke `vtable.render` for every `RsxNode::Component`
/// subtree, producing a flat tree of Element/Text/Fragment/Component...
/// wait — output never contains `Component`. Post-walker output is
/// Element/Text/Fragment only.
///
/// Pushes a `render_component` frame before each component render and
/// pops it after, replicating the lifecycle that today's eager path
/// runs from inside `create_element`. The `unwrap_components` traversal
/// is intended to run inside the outermost `build_scope` so
/// `live_keys` / prune behaviour is preserved.
///
/// React parity P2a: skeleton only. No producer currently constructs
/// `RsxNode::Component` — this walker is covered by a unit test that
/// hand-builds a component node. Wired end-to-end in P2b.
#[cfg(test)]
mod p2a_walker_tests {
    use super::{ComponentNodeInner, ComponentVTable, RsxNode, unwrap_components};
    use crate::ui::{RsxNodeIdentity, build_scope};
    use std::any::TypeId;
    use std::cell::Cell;
    use std::ptr::NonNull;

    thread_local! {
        static RENDER_CALLS: Cell<u32> = const { Cell::new(0) };
        static RENDER_SUM: Cell<u32> = const { Cell::new(0) };
    }

    struct TestProps {
        value: u32,
    }

    unsafe fn test_render_shim(
        props: NonNull<()>,
        _children: Vec<RsxNode>,
    ) -> RsxNode {
        let boxed: Box<TestProps> = unsafe { Box::from_raw(props.as_ptr().cast()) };
        RENDER_CALLS.with(|c| c.set(c.get() + 1));
        RENDER_SUM.with(|c| c.set(c.get() + boxed.value));
        RsxNode::text(format!("r:{}", boxed.value))
    }

    unsafe fn test_drop_shim(props: NonNull<()>) {
        drop(unsafe { Box::from_raw(props.as_ptr().cast::<TestProps>()) });
    }

    unsafe fn test_clone_shim(props: NonNull<()>) -> NonNull<()> {
        let src: &TestProps = unsafe { &*props.as_ptr().cast::<TestProps>() };
        let cloned = TestProps { value: src.value };
        NonNull::new(Box::into_raw(Box::new(cloned)).cast()).unwrap()
    }

    static TEST_VTABLE: ComponentVTable = ComponentVTable {
        render: test_render_shim,
        drop_props: test_drop_shim,
        clone_props: test_clone_shim,
        props_eq: None,
        type_name: "TestComp",
    };

    fn make_component_node(value: u32) -> RsxNode {
        let props = Box::into_raw(Box::new(TestProps { value }));
        RsxNode::Component(std::rc::Rc::new(ComponentNodeInner {
            identity: RsxNodeIdentity::new("TestComp", None),
            type_id: TypeId::of::<TestProps>(),
            props: NonNull::new(props.cast()).unwrap(),
            children: Vec::new(),
            key: None,
            vtable: &TEST_VTABLE,
            context_snapshot: Vec::new(),
        }))
    }

    #[test]
    fn walker_invokes_vtable_render_on_component_node() {
        RENDER_CALLS.with(|c| c.set(0));
        RENDER_SUM.with(|c| c.set(0));
        let node = make_component_node(42);
        let out = build_scope(|| unwrap_components(node));
        RENDER_CALLS.with(|c| assert_eq!(c.get(), 1));
        RENDER_SUM.with(|c| assert_eq!(c.get(), 42));
        match out {
            RsxNode::Text(t) => assert_eq!(t.content, "r:42"),
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn walker_recurses_component_inside_element_children() {
        RENDER_CALLS.with(|c| c.set(0));
        RENDER_SUM.with(|c| c.set(0));
        // Hand-build: Element with one Component child.
        let child = make_component_node(7);
        let element = RsxNode::element("TestParent");
        let element = element.with_child(child);
        let out = build_scope(|| unwrap_components(element));
        RENDER_CALLS.with(|c| assert_eq!(c.get(), 1));
        RENDER_SUM.with(|c| assert_eq!(c.get(), 7));
        // Output root is Element; its child is the rendered Text.
        let RsxNode::Element(el) = out else {
            panic!("expected element root");
        };
        assert_eq!(el.children.len(), 1);
        match &el.children[0] {
            RsxNode::Text(t) => assert_eq!(t.content, "r:7"),
            other => panic!("expected text child, got {other:?}"),
        }
    }

    #[test]
    fn walker_unconstructed_component_drops_props_via_vtable() {
        // Node is dropped without walking — Drop impl must fire drop_props
        // exactly once to free the boxed TestProps.
        let node = make_component_node(99);
        drop(node);
        // No assertion beyond "this does not leak or double-free"; Miri
        // would catch either. RENDER_CALLS stays at its prior value —
        // drop_props does not increment render counters.
    }
}

/// React parity P2: rsx! entry point. Wraps [`build_scope`] and, at the
/// outermost invocation (depth 0 → 1), runs [`unwrap_components`] on the
/// produced tree so user-component render bodies fire top-down.
///
/// Nested `rsx_scope` calls (depth > 1) are pass-through — the outer
/// walker handles their `RsxNode::Component` children when it recurses.
///
/// Called from the `rsx!` macro expansion instead of `build_scope`.
pub fn rsx_scope(f: impl FnOnce() -> RsxNode) -> RsxNode {
    build_scope(|| {
        let tree = f();
        // After `build_scope` entered, `build_depth` is 1 for the outermost
        // invocation; nested rsx! calls see depth > 1.
        if current_build_depth() == 1 {
            unwrap_components(tree)
        } else {
            tree
        }
    })
}

pub fn unwrap_components(node: RsxNode) -> RsxNode {
    match node {
        RsxNode::Text(_) => node,
        RsxNode::Element(mut element_rc) => {
            // `Rc::make_mut` clones the inner only if the Rc is shared,
            // which is rare in the unwrap path (subtrees here were just
            // built by rsx-macro). We take children by value so the
            // recursive call owns its Component Rcs uniquely — critical
            // for `into_render_parts`'s `Rc::try_unwrap`.
            let inner = std::rc::Rc::make_mut(&mut element_rc);
            let old_children = std::mem::take(&mut inner.children);
            inner.children = old_children.into_iter().map(unwrap_components).collect();
            RsxNode::Element(element_rc)
        }
        RsxNode::Fragment(mut fragment_rc) => {
            let inner = std::rc::Rc::make_mut(&mut fragment_rc);
            let old_children = std::mem::take(&mut inner.children);
            inner.children = old_children.into_iter().map(unwrap_components).collect();
            RsxNode::Fragment(fragment_rc)
        }
        RsxNode::Provider(provider_rc) => {
            // Walker-ancestry: push (type_id, value) onto CONTEXT_STACK
            // for the duration of the child walk, then pop. Provider never
            // survives past the walker — returns the walked child directly.
            let provider = match std::rc::Rc::try_unwrap(provider_rc) {
                Ok(inner) => inner,
                Err(rc) => (*rc).clone(),
            };
            let walked_child = crate::ui::with_pushed_context_raw(
                provider.type_id,
                provider.value,
                || unwrap_components(provider.child),
            );
            walked_child
        }
        RsxNode::Component(inner) => {
            let parts = inner.into_render_parts();
            let ComponentRenderParts {
                identity,
                type_id,
                key,
                children,
                props,
                vtable,
                context_snapshot,
            } = parts;
            with_component_key(key, || {
                crate::ui::render_component_by_type_id(type_id, || {
                    crate::ui::with_installed_context_snapshot(&context_snapshot, || {
                    // Safety: `props` was produced by `Box::into_raw(Box::new(T::Props))`
                    // during Component construction, and `vtable.render` is the
                    // monomorphized shim that `Box::from_raw`s it back to the
                    // exact same T. `children` is owned here after
                    // `into_render_parts` — the shim consumes both.
                    let rendered = unsafe { (vtable.render)(props, children) };
                    let mut walked = unwrap_components(rendered);
                    walked.set_identity(identity);
                    // Mirror pre-P2 `build_tag_node` behaviour: stamp the
                    // outer component's `RsxTagDescriptor` onto the
                    // rendered root. Preserves `tag_descriptor == Outer`
                    // semantics consumers rely on (e.g. `<Window>` wraps
                    // `<WindowView>` — tree root's descriptor remains
                    // `Window`, not `WindowView`).
                    if let RsxNode::Element(el) = &mut walked {
                        std::rc::Rc::make_mut(el).tag_descriptor = Some(RsxTagDescriptor {
                            type_id,
                            type_name: identity.invocation_type,
                        });
                    }
                    walked
                    })
                })
            })
        }
    }
}

impl std::fmt::Debug for ComponentNodeInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentNodeInner")
            .field("type_name", &self.vtable.type_name)
            .field("key", &self.key)
            .field("children_len", &self.children.len())
            .finish()
    }
}

// Component nodes compare by raw prop-pointer identity + children equality.
// In P1 no producer constructs these, so this impl is exercised only by the
// derived `PartialEq` on `RsxNode`. Two boxed props allocations are unique
// by pointer, so this is effectively `Rc::ptr_eq` semantics at the enclosing
// `Rc<ComponentNodeInner>`.
impl PartialEq for ComponentNodeInner {
    fn eq(&self, other: &Self) -> bool {
        self.type_id == other.type_id
            && std::ptr::eq(self.vtable, other.vtable)
            && self.props == other.props
            && self.key == other.key
            && self.children == other.children
    }
}

pub(crate) mod sealed {
    pub trait Sealed {}
}

/// Marker trait for built-in host tags (Element/Text/TextArea/Image/Svg).
/// Sealed — only rfgui itself may implement. `#[component]`-generated
/// types implement [`ComponentTag`] instead.
pub trait HostTag: RsxTag + sealed::Sealed {}

/// Marker trait for user-authored components. Carries the static
/// [`ComponentVTable`] emitted by the `#[component]` macro (either the
/// fn-style authoring form or the `impl RsxTag` block form).
///
/// Not sealed — `#[component]` generates impls in downstream crates.
pub trait ComponentTag: RsxTag {
    const VTABLE: &'static ComponentVTable;
}

// ---------- React-style shared createElement ----------

pub trait RsxTag: 'static {
    type Props: Default;
    type StrictProps;
    const ACCEPTS_CHILDREN: bool;
    /// 軌 1 #13: host tag（Element/Text/TextArea/Image/Svg）skip
    /// `render_component` 的 frame push / live_keys 登記 / prune 旗標。
    /// User `#[component]` 保持 false（default）。
    const IS_HOST_TAG: bool = false;

    fn into_strict(props: Self::Props) -> Self::StrictProps;

    fn create_node(
        props: Self::StrictProps,
        children: Vec<RsxNode>,
        key: Option<RsxKey>,
    ) -> RsxNode;

    /// React parity P2: user-component implementations return
    /// `Some(&VTABLE)`; host implementations return `None`.
    /// `#[component]` macro auto-emits the override.
    fn component_vtable() -> Option<&'static ComponentVTable> {
        None
    }
}

/// Build a tag's props inside an isolated stack frame, then hand off to
/// `create_element`. Keeps large `Props` structs (e.g. `ElementPropSchema`)
/// out of the caller's frame — important on wasm where an unoptimized dev
/// build does not reuse stack slots across sibling RSX blocks and can blow
/// the 1 MB shadow stack on scenes with hundreds of elements.
#[inline(never)]
pub fn __rsx_create_element<T: RsxTag, F: FnOnce(&mut T::Props)>(
    setup: F,
    children: Vec<RsxNode>,
    key: Option<RsxKey>,
) -> RsxNode {
    let mut init = T::Props::default();
    setup(&mut init);
    create_element::<T>(init, children, key)
}

/// Two-path dispatch (React parity P2/P3/P5):
///
/// 1. **Host tag** (`T::IS_HOST_TAG == true`): build the concrete
///    `RsxNode::Element`/`Text`/... description inline. No hook frame,
///    no live-keys, no prune flag — host tags have no render body.
///
/// 2. **User component**: box `T::StrictProps`, snapshot the provider
///    stack, and wrap in `RsxNode::Component`. Defer — the
///    `unwrap_components` walker (invoked at the outermost `rsx_scope`)
///    pushes `render_component` / restores context and calls
///    `vtable.render` top-down, giving React-style parent-before-child
///    evaluation.
///
/// User components must declare their vtable via `#[component]` (either
/// the fn-style authoring form or the `impl RsxTag` block form). A
/// non-host `RsxTag` impl without `component_vtable()` panics with a
/// migration message — the pre-P5 fallback eager path was removed once
/// all in-repo components moved to the lazy path.
#[inline(never)]
pub fn create_element<T: RsxTag>(
    init: T::Props,
    children: Vec<RsxNode>,
    key: Option<RsxKey>,
) -> RsxNode {
    debug_assert!(T::ACCEPTS_CHILDREN || children.is_empty());
    if let Some(RsxKey::Global(global_key)) = key.clone() {
        register_global_key(global_key);
    }
    if T::IS_HOST_TAG {
        // 軌 1 #13: host tag 不進 state lifecycle — 不 push frame、
        // 不寫 live_keys、不設 components_rendered_in_build。純 node
        // construction。副作用：非主 render 路徑觸發的 rsx!（例如
        // TextArea on_render handler）不再誤觸 slot prune。
        with_component_key(key.clone(), || {
            let strict = T::into_strict(init);
            build_tag_node::<T>(strict, children, key)
        })
    } else {
        let vtable = T::component_vtable().unwrap_or_else(|| {
            panic!(
                "non-host RsxTag `{name}` has no ComponentTag vtable. \
                 Apply `#[rfgui::ui::component]` to the `impl RsxTag for {name}` \
                 block (or rewrite as `#[component] fn {name}(...)`) so the \
                 macro can emit the required vtable + ComponentTag impl.",
                name = std::any::type_name::<T>()
            )
        });
        let strict = T::into_strict(init);
        let props_raw = Box::into_raw(Box::new(strict));
        let identity = RsxNodeIdentity::new(std::any::type_name::<T>(), key);
        // P3: capture provider stack visible at rsx! expansion time so the
        // walker can re-install it when this Component later renders,
        // after the provider's closure has returned and popped its value.
        let context_snapshot = crate::ui::snapshot_context_stack();
        RsxNode::Component(std::rc::Rc::new(ComponentNodeInner {
            identity,
            type_id: std::any::TypeId::of::<T>(),
            props: NonNull::new(props_raw.cast())
                .expect("Box::into_raw returns non-null"),
            children,
            key,
            vtable,
            context_snapshot,
        }))
    }
}

#[inline]
fn build_tag_node<T: RsxTag>(
    strict: T::StrictProps,
    children: Vec<RsxNode>,
    key: Option<RsxKey>,
) -> RsxNode {
    // Only called from the host-tag branch of `create_element`. Host
    // `create_node` returns the concrete `RsxNode::Element` / `Text`
    // description directly — never a `Component` — so the P2 fallback
    // unwrap that used to live here (to eagerly resolve hand-written
    // wrappers) is no longer needed.
    let mut node = T::create_node(strict, children, key.clone());
    node.set_identity(RsxNodeIdentity::new(std::any::type_name::<T>(), key));
    if let RsxNode::Element(element) = &mut node {
        std::rc::Rc::make_mut(element).tag_descriptor = Some(RsxTagDescriptor::of::<T>());
    }
    node
}

#[doc(hidden)]
#[inline(always)]
pub fn __rsx_default_inner_option<T: Default>(_: &Option<T>) -> T {
    T::default()
}

#[doc(hidden)]
#[inline(always)]
pub fn __rsx_infer_inner_option<T>(_: &Option<T>) -> std::marker::PhantomData<T> {
    std::marker::PhantomData
}

#[doc(hidden)]
#[inline(always)]
pub fn __rsx_default_from_phantom<T: Default>(_: std::marker::PhantomData<T>) -> T {
    T::default()
}

impl From<GlobalKey> for RsxKey {
    fn from(value: GlobalKey) -> Self {
        Self::Global(value)
    }
}

