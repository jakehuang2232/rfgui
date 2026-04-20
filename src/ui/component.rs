#![allow(missing_docs)]

//! Component traits and helper APIs used by typed RSX components.

use crate::ui::{
    GlobalKey, RsxKey, RsxNode, RsxNodeIdentity, RsxTagDescriptor, register_global_key,
    render_component, with_component_key,
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

// ---------- React-style shared createElement ----------

pub trait RsxTag: 'static {
    type Props: Default;
    type StrictProps;
    const ACCEPTS_CHILDREN: bool;

    fn into_strict(props: Self::Props) -> Self::StrictProps;

    fn create_node(
        props: Self::StrictProps,
        children: Vec<RsxNode>,
        key: Option<RsxKey>,
    ) -> RsxNode;
}

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
    with_component_key(key.clone(), || {
        let strict = T::into_strict(init);
        render_component::<T, _>(|| {
            let mut node = T::create_node(strict, children, key.clone());
            node.set_identity(RsxNodeIdentity::new(std::any::type_name::<T>(), key));
            if let RsxNode::Element(element) = &mut node {
                std::rc::Rc::make_mut(element).tag_descriptor =
                    Some(RsxTagDescriptor::of::<T>());
            }
            node
        })
    })
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

