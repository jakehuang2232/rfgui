#![allow(missing_docs)]

//! Component traits and helper APIs used by typed RSX components.

use std::any::TypeId;
use std::ptr::NonNull;

use crate::ui::{
    GlobalKey, RsxErrorBoundaryNode, RsxKey, RsxNode, RsxNodeIdentity, RsxTagDescriptor,
    build_scope, register_global_key, with_component_key,
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

impl_event_into_optional_prop!(
    crate::ui::PointerDownHandlerProp,
    crate::ui::PointerDownEvent
);
impl_event_into_optional_prop!(crate::ui::PointerUpHandlerProp, crate::ui::PointerUpEvent);
impl_event_into_optional_prop!(
    crate::ui::PointerMoveHandlerProp,
    crate::ui::PointerMoveEvent
);
impl_event_into_optional_prop!(
    crate::ui::PointerEnterHandlerProp,
    crate::ui::PointerEnterEvent
);
impl_event_into_optional_prop!(
    crate::ui::PointerLeaveHandlerProp,
    crate::ui::PointerLeaveEvent
);
impl_event_into_optional_prop!(crate::ui::ClickHandlerProp, crate::ui::ClickEvent);
impl_event_into_optional_prop!(
    crate::ui::ContextMenuHandlerProp,
    crate::ui::ContextMenuEvent
);
impl_event_into_optional_prop!(crate::ui::WheelHandlerProp, crate::ui::WheelEvent);
impl_event_into_optional_prop!(crate::ui::ImeCommitHandlerProp, crate::ui::ImeCommitEvent);
impl_event_into_optional_prop!(crate::ui::ImeEnabledHandlerProp, crate::ui::ImeEnabledEvent);
impl_event_into_optional_prop!(
    crate::ui::ImeDisabledHandlerProp,
    crate::ui::ImeDisabledEvent
);
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

impl<'a> IntoOptionalProp<crate::style::Color> for crate::style::HexColor<'a> {
    fn into_optional_prop(self) -> Option<crate::style::Color> {
        Some(crate::style::IntoColor::<crate::style::Color>::into_color(
            self,
        ))
    }
}

impl IntoOptionalProp<Box<dyn crate::style::ColorLike>> for &str {
    fn into_optional_prop(self) -> Option<Box<dyn crate::style::ColorLike>> {
        Some(Box::new(
            crate::style::IntoColor::<crate::style::Color>::into_color(self),
        ))
    }
}

impl IntoOptionalProp<Box<dyn crate::style::ColorLike>> for String {
    fn into_optional_prop(self) -> Option<Box<dyn crate::style::ColorLike>> {
        Some(Box::new(
            crate::style::IntoColor::<crate::style::Color>::into_color(self),
        ))
    }
}

impl IntoOptionalProp<Box<dyn crate::style::ColorLike>> for crate::style::Color {
    fn into_optional_prop(self) -> Option<Box<dyn crate::style::ColorLike>> {
        Some(Box::new(self))
    }
}

impl<'a> IntoOptionalProp<Box<dyn crate::style::ColorLike>> for crate::style::HexColor<'a> {
    fn into_optional_prop(self) -> Option<Box<dyn crate::style::ColorLike>> {
        Some(Box::new(
            crate::style::IntoColor::<crate::style::Color>::into_color(self),
        ))
    }
}

// Background accepts colors, gradients, and gradient builders.
// (Background → Option<Background> is handled by the blanket `impl<T> IntoOptionalProp<T> for T`.)
impl IntoOptionalProp<crate::style::Background> for crate::style::Gradient {
    fn into_optional_prop(self) -> Option<crate::style::Background> {
        Some(crate::style::Background::Gradient(self))
    }
}

impl IntoOptionalProp<crate::style::Background> for crate::style::LinearBuilder {
    fn into_optional_prop(self) -> Option<crate::style::Background> {
        Some(crate::style::Background::Gradient(self.build()))
    }
}

impl IntoOptionalProp<crate::style::Background> for crate::style::RadialBuilder {
    fn into_optional_prop(self) -> Option<crate::style::Background> {
        Some(crate::style::Background::Gradient(self.build()))
    }
}

impl IntoOptionalProp<crate::style::Background> for crate::style::ConicBuilder {
    fn into_optional_prop(self) -> Option<crate::style::Background> {
        Some(crate::style::Background::Gradient(self.build()))
    }
}

impl IntoOptionalProp<crate::style::Background> for Box<dyn crate::style::ColorLike> {
    fn into_optional_prop(self) -> Option<crate::style::Background> {
        Some(crate::style::Background::Color(self))
    }
}

impl IntoOptionalProp<crate::style::Background> for crate::style::Color {
    fn into_optional_prop(self) -> Option<crate::style::Background> {
        Some(crate::style::Background::Color(Box::new(self)))
    }
}

impl<'a> IntoOptionalProp<crate::style::Background> for crate::style::HexColor<'a> {
    fn into_optional_prop(self) -> Option<crate::style::Background> {
        Some(crate::style::Background::Color(Box::new(
            crate::style::IntoColor::<crate::style::Color>::into_color(self),
        )))
    }
}

impl IntoOptionalProp<crate::style::Background> for &str {
    fn into_optional_prop(self) -> Option<crate::style::Background> {
        Some(crate::style::Background::Color(Box::new(
            crate::style::IntoColor::<crate::style::Color>::into_color(self),
        )))
    }
}

impl IntoOptionalProp<crate::style::Background> for String {
    fn into_optional_prop(self) -> Option<crate::style::Background> {
        Some(crate::style::Background::Color(Box::new(
            crate::style::IntoColor::<crate::style::Color>::into_color(self),
        )))
    }
}

// Gradient (for background_image / border_image fields): accept builders too.
impl IntoOptionalProp<crate::style::Gradient> for crate::style::LinearBuilder {
    fn into_optional_prop(self) -> Option<crate::style::Gradient> {
        Some(self.build())
    }
}

impl IntoOptionalProp<crate::style::Gradient> for crate::style::RadialBuilder {
    fn into_optional_prop(self) -> Option<crate::style::Gradient> {
        Some(self.build())
    }
}

impl IntoOptionalProp<crate::style::Gradient> for crate::style::ConicBuilder {
    fn into_optional_prop(self) -> Option<crate::style::Gradient> {
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
            impl IntoOptionalProp<crate::style::Length> for $ty {
                fn into_optional_prop(self) -> Option<crate::style::Length> {
                    Some(crate::style::Length::px(self as f32))
                }
            }
        )*
    };
}

impl_numeric_into_optional_length!(i32, i64, u32, usize, f32, f64);

impl IntoOptionalProp<crate::style::BorderRadius> for crate::style::Length {
    fn into_optional_prop(self) -> Option<crate::style::BorderRadius> {
        Some(crate::style::BorderRadius::uniform(self))
    }
}

macro_rules! impl_numeric_into_optional_border_radius {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoOptionalProp<crate::style::BorderRadius> for $ty {
                fn into_optional_prop(self) -> Option<crate::style::BorderRadius> {
                    Some(crate::style::BorderRadius::uniform(crate::style::Length::px(self as f32)))
                }
            }
        )*
    };
}

impl_numeric_into_optional_border_radius!(i32, i64, u32, usize, f32, f64);

macro_rules! impl_numeric_into_optional_font_weight {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoOptionalProp<crate::style::FontWeight> for $ty {
                fn into_optional_prop(self) -> Option<crate::style::FontWeight> {
                    Some(crate::style::FontWeight::new((self as i64).max(0) as u16))
                }
            }
        )*
    };
}

impl_numeric_into_optional_font_weight!(i32, i64, u32, usize, u16);

macro_rules! impl_numeric_into_optional_opacity {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoOptionalProp<crate::style::Opacity> for $ty {
                fn into_optional_prop(self) -> Option<crate::style::Opacity> {
                    Some(crate::style::Opacity::new(self as f32))
                }
            }
        )*
    };
}

impl_numeric_into_optional_opacity!(i32, i64, u32, usize, f32, f64);

impl IntoOptionalProp<crate::style::Layout> for crate::style::FlexLayout {
    fn into_optional_prop(self) -> Option<crate::style::Layout> {
        Some(self.into())
    }
}

impl IntoOptionalProp<crate::style::Layout> for crate::style::FlowLayout {
    fn into_optional_prop(self) -> Option<crate::style::Layout> {
        Some(self.into())
    }
}

impl IntoOptionalProp<crate::style::Transitions> for crate::style::Transition {
    fn into_optional_prop(self) -> Option<crate::style::Transitions> {
        Some(self.into())
    }
}

impl IntoOptionalProp<crate::style::Transitions> for Vec<crate::style::Transition> {
    fn into_optional_prop(self) -> Option<crate::style::Transitions> {
        Some(self.into())
    }
}

impl<const N: usize> IntoOptionalProp<crate::style::Transitions> for [crate::style::Transition; N] {
    fn into_optional_prop(self) -> Option<crate::style::Transitions> {
        Some(self.into())
    }
}

impl IntoOptionalProp<crate::style::Animator> for crate::style::Animation {
    fn into_optional_prop(self) -> Option<crate::style::Animator> {
        Some(crate::style::Animator::new([self]))
    }
}

impl IntoOptionalProp<crate::style::Animator> for Vec<crate::style::Animation> {
    fn into_optional_prop(self) -> Option<crate::style::Animator> {
        Some(crate::style::Animator::from_vec(self))
    }
}

impl<const N: usize> IntoOptionalProp<crate::style::Animator> for [crate::style::Animation; N] {
    fn into_optional_prop(self) -> Option<crate::style::Animator> {
        Some(crate::style::Animator::new(self))
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

impl IntoOptionalProp<crate::style::FontSize> for f32 {
    fn into_optional_prop(self) -> Option<crate::style::FontSize> {
        Some(crate::style::FontSize::px(self))
    }
}

impl IntoOptionalProp<crate::style::FontSize> for f64 {
    fn into_optional_prop(self) -> Option<crate::style::FontSize> {
        Some(crate::style::FontSize::px(self as f32))
    }
}

impl IntoOptionalProp<crate::style::FontSize> for i32 {
    fn into_optional_prop(self) -> Option<crate::style::FontSize> {
        Some(crate::style::FontSize::px(self as f32))
    }
}

impl IntoOptionalProp<crate::style::FontSize> for i64 {
    fn into_optional_prop(self) -> Option<crate::style::FontSize> {
        Some(crate::style::FontSize::px(self as f32))
    }
}

impl IntoOptionalProp<crate::style::FontSize> for u32 {
    fn into_optional_prop(self) -> Option<crate::style::FontSize> {
        Some(crate::style::FontSize::px(self as f32))
    }
}

impl IntoOptionalProp<crate::style::FontSize> for usize {
    fn into_optional_prop(self) -> Option<crate::style::FontSize> {
        Some(crate::style::FontSize::px(self as f32))
    }
}

/// A user component's render. Props are borrowed: the component node keeps
/// them, so a memoized node can render again without copying its props, and
/// the next render compares against them. Clone a field to move it into the
/// output.
pub trait RsxComponent<Props>: Sized {
    fn render(props: &Props, children: Vec<RsxNode>) -> RsxNode;
}

/// `#[component]` copies a by-value parameter out of the borrowed props, so
/// its type must be `Copy`; take any other parameter by reference (`&T`).
#[doc(hidden)]
#[inline(always)]
pub fn copy_component_param<T: Copy>(value: &T) -> T {
    *value
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
    /// Invoke `T::render(&*props, children)`. Borrows props; the node
    /// keeps them.
    pub render: unsafe fn(NonNull<()>, Vec<RsxNode>) -> RsxNode,
    /// Drop `Box<T::Props>` when the containing `ComponentNodeInner` is
    /// dropped.
    pub drop_props: unsafe fn(NonNull<()>),
    /// Structural equality of two boxed `T::Props`. `None` opts out of
    /// memoization (each render re-invokes `render`). Generated comparators
    /// compare supported fields and conservatively miss for unknown types.
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
}

impl Drop for ComponentNodeInner {
    fn drop(&mut self) {
        // Safety: `props` was produced by `Box::into_raw(Box::new(T::Props))`
        // at construction, and the vtable's `drop_props` shim is the
        // monomorphized `Box<T::Props>` dropper for exactly that T.
        unsafe { (self.vtable.drop_props)(self.props) };
    }
}

#[cfg(test)]
mod p2a_walker_tests;

/// Runs one root render (React `root.render`): builds the root description,
/// renders its deferred user components top-down, and on exit retires the
/// state of every component that did not render. Call it only at the root,
/// outside any other root render.
///
/// `rsx!` only describes trees, so this is the sole place component render
/// bodies run from the root. The viewport wraps `App::build` in it; code
/// that drives a viewport directly calls it before `render_rsx`. A
/// description built anywhere else, such as in an event handler, stays
/// deferred until a later root render places it in the tree.
///
/// A render or mount that panics outside any error boundary (see
/// [`catch_render_node`](crate::ui::catch_render_node)) unmounts the whole
/// tree, as in React: the result renders nothing, and every component's
/// state retires.
pub fn render_root(build: impl FnOnce() -> RsxNode) -> RsxNode {
    let failed = std::rc::Rc::new(std::cell::Cell::new(false));
    let unmount: crate::ui::state::ErrorHandler = {
        let failed = failed.clone();
        std::rc::Rc::new(move |_| {
            failed.set(true);
            RsxNode::fragment(Vec::new())
        })
    };
    let root = build_scope(|| {
        crate::ui::state::catch_root_render(unmount.clone(), || unwrap_components(build()))
            .unwrap_or_else(|error| unmount(&error))
    });
    if failed.get() {
        eprintln!("[error] a component panicked outside any error boundary; unmounted the tree");
        return RsxNode::fragment(Vec::new());
    }
    root
}

/// Top-down walker: invokes `vtable.render` for every `RsxNode::Component`
/// and dissolves every `RsxNode::Provider` and `RsxNode::ErrorBoundary`, so
/// the output holds only Element/Text/Fragment nodes.
///
/// Each component render runs inside a `render_component` frame, so the
/// walk must happen inside a [`build_scope`] (normally [`render_root`])
/// that owns `live_keys` and pruning.
pub fn unwrap_components(node: RsxNode) -> RsxNode {
    let _profile = crate::ui::work_profile::scope(crate::ui::work_profile::Phase::Unwrap);
    crate::ui::work_profile::count(|p| p.unwrap_nodes += 1);
    match node {
        RsxNode::Text(_) => node,
        RsxNode::Element(mut element_rc) => {
            // `Rc::make_mut` clones the inner only if the Rc is shared,
            // which is rare in the unwrap path (subtrees here were just
            // built by rsx-macro). We take children by value so the
            // recursive call owns its Component Rcs uniquely and can move
            // their children out instead of cloning them.
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
            let walked_child =
                crate::ui::with_pushed_context_raw(provider.type_id, provider.value, || {
                    unwrap_components(provider.child)
                });
            walked_child
        }
        RsxNode::Component(inner) => memo::render_deferred(inner),
        RsxNode::ErrorBoundary(boundary_rc) => {
            let boundary = match std::rc::Rc::try_unwrap(boundary_rc) {
                Ok(inner) => inner,
                Err(rc) => (*rc).clone(),
            };
            let RsxErrorBoundaryNode {
                child, on_error, ..
            } = boundary;
            crate::ui::state::catch_render(on_error.clone(), || unwrap_components(child))
                .unwrap_or_else(|error| unwrap_components(on_error(&error)))
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

// Component nodes compare by type/vtable identity, raw prop-pointer identity,
// key, and children equality. Rendered components are unwrapped before host
// conversion, so equality remains tied to the authored component allocation
// rather than deep prop-value comparison.
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

/// Crate-internal style contract for built-in host tags.
///
/// `rsx-macro` must stay generic and does not inspect this type. The
/// view layer uses it to know which typed style prop schema and accepted
/// property set belong to each host.
#[allow(dead_code)]
pub(crate) trait HostStyleTag: HostTag {
    type StyleProp: crate::style::style_props::StylePropTrait;
}

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

    /// Phase 6b host-builder pointer. Built-in host tags and custom
    /// `view::HostBuilder` implementors set this to
    /// `Some(view::erased_host_builder::<Self>)`; ordinary `#[component]`
    /// user components leave it `None`.
    const HOST_BUILDER: Option<crate::ui::ErasedHostBuilder> = None;

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
/// 2. **User component**: box `T::StrictProps` and wrap in
///    `RsxNode::Component`. Defer — the `unwrap_components` walker
///    (driven by [`render_root`]) pushes `render_component` and calls
///    `vtable.render` top-down, giving React-style parent-before-child
///    evaluation. Creating the description never renders the component.
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
        RsxNode::Component(std::rc::Rc::new(ComponentNodeInner {
            identity,
            type_id: std::any::TypeId::of::<T>(),
            props: NonNull::new(props_raw.cast()).expect("Box::into_raw returns non-null"),
            children,
            key,
            vtable,
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
        std::rc::Rc::make_mut(element).tag_descriptor = Some(RsxTagDescriptor::for_tag::<T>());
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

mod memo;
#[doc(hidden)]
pub use memo::{MemoCompare, MemoCompareValue};
