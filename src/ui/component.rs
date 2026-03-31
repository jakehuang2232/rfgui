#![allow(missing_docs)]

//! Component traits and helper APIs used by typed RSX components.

use crate::ui::{
    GlobalKey, RsxKey, RsxNode, RsxNodeIdentity, RsxTagDescriptor, register_global_key,
    render_component, with_component_key,
};
use std::marker::PhantomData;

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

impl_event_into_optional_prop!(crate::ui::MouseDownHandlerProp, crate::ui::MouseDownEvent);
impl_event_into_optional_prop!(crate::ui::MouseUpHandlerProp, crate::ui::MouseUpEvent);
impl_event_into_optional_prop!(crate::ui::MouseMoveHandlerProp, crate::ui::MouseMoveEvent);
impl_event_into_optional_prop!(crate::ui::MouseEnterHandlerProp, crate::ui::MouseEnterEvent);
impl_event_into_optional_prop!(crate::ui::MouseLeaveHandlerProp, crate::ui::MouseLeaveEvent);
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
impl_no_arg_event_into_optional_prop!(
    crate::ui::MouseDownHandlerProp,
    crate::ui::into_mouse_down_handler
);
impl_no_arg_event_into_optional_prop!(
    crate::ui::MouseUpHandlerProp,
    crate::ui::into_mouse_up_handler
);
impl_no_arg_event_into_optional_prop!(
    crate::ui::MouseMoveHandlerProp,
    crate::ui::into_mouse_move_handler
);
impl_no_arg_event_into_optional_prop!(
    crate::ui::MouseEnterHandlerProp,
    crate::ui::into_mouse_enter_handler
);
impl_no_arg_event_into_optional_prop!(
    crate::ui::MouseLeaveHandlerProp,
    crate::ui::into_mouse_leave_handler
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

pub trait RsxTag<Props>: 'static
where
    Props: RsxPropsBuilder,
{
    const ACCEPTS_CHILDREN: bool = true;

    fn create_node(props: Props, children: Vec<RsxNode>, key: Option<RsxKey>) -> RsxNode;
}

pub trait RsxPropsBuilder: Sized {
    type Builder;

    fn builder() -> Self::Builder;
    fn build(builder: Self::Builder) -> Result<Self, String>;
}

pub trait RsxStyleSchema {
    type SelectionSchema;
}

pub trait RsxPropsStyleSchema {
    type StyleSchema: RsxStyleSchema;
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

#[deprecated(
    note = "use create_tag_element::<T>(props, children); legacy bridge for old host/component path"
)]
pub fn create_element<T, P, C>(_element_type: PhantomData<T>, props: P, children: C) -> RsxNode
where
    T: RsxChildrenPolicy + RsxComponent<P> + 'static,
    P: RsxPropsBuilder,
    C: IntoRsxChildren,
{
    create_tag_element::<T, P, C>(props, children)
}

pub fn create_tag_element<T, P, C>(props: P, children: C) -> RsxNode
where
    T: RsxTag<P>,
    P: RsxPropsBuilder,
    C: IntoRsxChildren,
{
    create_tag_element_with_key::<T, P, C>(props, children, None)
}

pub fn create_tag_element_with_key<T, P, C>(props: P, children: C, key: Option<RsxKey>) -> RsxNode
where
    T: RsxTag<P>,
    P: RsxPropsBuilder,
    C: IntoRsxChildren,
{
    let children = children.into_rsx_children();
    debug_assert!(T::ACCEPTS_CHILDREN || children.is_empty());
    if let Some(RsxKey::Global(global_key)) = key.clone() {
        register_global_key(global_key);
    }
    with_component_key(key.clone(), || {
        let mut node = T::create_node(props, children, key.clone());
        node.set_identity(RsxNodeIdentity::new(std::any::type_name::<T>(), key));
        if let RsxNode::Element(element) = &mut node {
            element.tag_descriptor = Some(RsxTagDescriptor::of::<T>());
        }
        node
    })
}

#[deprecated(
    note = "use create_tag_element_with_key::<T>(props, children, key); legacy bridge for old host/component path"
)]
pub fn create_element_with_key<T, P, C>(
    _element_type: PhantomData<T>,
    props: P,
    children: C,
    key: Option<RsxKey>,
) -> RsxNode
where
    T: RsxChildrenPolicy + RsxComponent<P> + 'static,
    P: RsxPropsBuilder,
    C: IntoRsxChildren,
{
    create_tag_element_with_key::<T, P, C>(props, children, key)
}

impl<T, P> RsxTag<P> for T
where
    T: RsxChildrenPolicy + RsxComponent<P> + 'static,
    P: RsxPropsBuilder,
{
    const ACCEPTS_CHILDREN: bool = T::ACCEPTS_CHILDREN;

    fn create_node(props: P, children: Vec<RsxNode>, _key: Option<RsxKey>) -> RsxNode {
        render_component::<T, _>(|| T::render(props, children))
    }
}

impl From<GlobalKey> for RsxKey {
    fn from(value: GlobalKey) -> Self {
        Self::Global(value)
    }
}

pub fn build_typed_prop<T, F>(build: F) -> T
where
    T: RsxPropsBuilder,
    F: FnOnce(&mut T::Builder),
{
    let mut builder = T::builder();
    build(&mut builder);
    T::build(builder).expect("rsx build error on typed prop object")
}

pub fn build_typed_prop_for<T, F>(_: PhantomData<T>, build: F) -> T
where
    T: RsxPropsBuilder,
    F: FnOnce(&mut T::Builder),
{
    build_typed_prop(build)
}

pub fn boolean_prop_shorthand<T>(_: PhantomData<T>) -> bool
where
    T: BooleanPropMarker,
{
    true
}

#[cfg(test)]
mod tests {
    macro_rules! style {
        ($($tokens:tt)*) => {{
            let _ = stringify!($($tokens)*);
            crate::view::ElementStylePropSchema {
                width: Some(crate::Length::px(42.0)),
                ..crate::ui::build_typed_prop::<crate::view::ElementStylePropSchema, _>(|_| {})
            }
        }};
    }

    use super::{
        GlobalKey, RsxChildrenPolicy, RsxComponent, RsxPropsBuilder, RsxPropsStyleSchema, RsxTag,
        create_tag_element, create_tag_element_with_key,
    };
    use crate::style::{Color, FontSize, FontWeight, Length, ParsedValue, PropertyId};
    use crate::ui::{
        ClickEvent, EventMeta, KeyDownEvent, KeyEventData, MouseButton, MouseButtons,
        MouseEventData, Patch, PropValue, RsxKey, RsxNode, component, props, reconcile, rsx,
    };
    use crate::view::{Element, Text};
    use std::cell::Cell;
    use std::rc::Rc;

    struct Button;
    struct ElementLike;
    struct AnotherElementLike;

    impl RsxChildrenPolicy for Button {
        const ACCEPTS_CHILDREN: bool = false;
    }

    impl RsxChildrenPolicy for ElementLike {
        const ACCEPTS_CHILDREN: bool = false;
    }

    impl RsxChildrenPolicy for AnotherElementLike {
        const ACCEPTS_CHILDREN: bool = false;
    }

    impl RsxPropsBuilder for () {
        type Builder = ();

        fn builder() -> Self::Builder {}

        fn build(_builder: Self::Builder) -> Result<Self, String> {
            Ok(())
        }
    }

    impl RsxComponent<()> for Button {
        fn render(_: (), _: Vec<RsxNode>) -> RsxNode {
            RsxNode::tagged(
                "Element",
                crate::ui::RsxTagDescriptor::of::<crate::view::Element>(),
            )
        }
    }

    impl RsxComponent<()> for ElementLike {
        fn render(_: (), _: Vec<RsxNode>) -> RsxNode {
            RsxNode::tagged(
                "Element",
                crate::ui::RsxTagDescriptor::of::<crate::view::Element>(),
            )
        }
    }

    impl RsxComponent<()> for AnotherElementLike {
        fn render(_: (), _: Vec<RsxNode>) -> RsxNode {
            RsxNode::tagged(
                "Element",
                crate::ui::RsxTagDescriptor::of::<crate::view::Element>(),
            )
        }
    }

    #[component]
    fn BoolFlag(flag: bool) -> RsxNode {
        if flag {
            RsxNode::element("BoolTrue")
        } else {
            RsxNode::element("BoolFalse")
        }
    }

    #[component]
    fn OptionalFlag(flag: Option<bool>) -> RsxNode {
        if flag == Some(true) {
            RsxNode::element("OptionTrue")
        } else {
            RsxNode::element("OptionFalse")
        }
    }

    #[component]
    fn PassThrough(children: Vec<RsxNode>) -> RsxNode {
        rsx! {
            <Element>
                {children}
            </Element>
        }
    }

    #[props]
    struct NestedStyleSlot {
        style: Option<crate::view::ElementStylePropSchema>,
    }

    impl RsxPropsStyleSchema for NestedStyleSlot {
        type StyleSchema = crate::view::ElementStylePropSchema;
    }

    #[component]
    fn NestedStyleProbe(nested: Option<NestedStyleSlot>) -> RsxNode {
        let style = nested.and_then(|slot| slot.style);
        rsx! {
            <crate::view::Element style={style} />
        }
    }

    fn extract_element_style(node: &crate::ui::RsxElementNode) -> crate::Style {
        node.props
            .iter()
            .find_map(|(key, value)| match (key.as_str(), value) {
                ("style", crate::ui::PropValue::Shared(shared)) => shared
                    .value()
                    .downcast::<crate::view::ElementStylePropSchema>()
                    .ok()
                    .map(|style| style.to_style()),
                _ => None,
            })
            .expect("missing style prop")
    }

    #[test]
    #[should_panic(expected = "duplicate GlobalKey detected in the same build")]
    fn duplicate_global_key_panics_in_same_build() {
        crate::ui::build_scope(|| {
            let global_key = GlobalKey::from("dup");
            let _ = create_tag_element_with_key::<Button, _, _>(
                (),
                (),
                Some(RsxKey::Global(global_key)),
            );
            let _ = create_tag_element_with_key::<Button, _, _>(
                (),
                (),
                Some(RsxKey::Global(global_key)),
            );
        });
    }

    #[test]
    fn same_global_key_but_different_invocation_type_replaces_node() {
        let global_key = GlobalKey::from("shared");
        let old = crate::ui::build_scope(|| {
            create_tag_element_with_key::<Button, _, _>((), (), Some(RsxKey::Global(global_key)))
        });
        let new = crate::ui::build_scope(|| {
            create_tag_element_with_key::<ElementLike, _, _>(
                (),
                (),
                Some(RsxKey::Global(global_key)),
            )
        });

        let patches = reconcile(Some(&old), &new);
        assert!(matches!(patches.as_slice(), [Patch::ReplaceRoot(_)]));
    }

    #[test]
    fn create_tag_element_with_key_bridges_existing_rsx_component_types() {
        let node = crate::ui::build_scope(|| {
            create_tag_element_with_key::<Button, _, _>((), (), Some(RsxKey::Local(7)))
        });

        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        assert_eq!(node.identity.key, Some(RsxKey::Local(7)));
        assert_eq!(
            node.tag_descriptor,
            Some(crate::ui::RsxTagDescriptor::of::<Button>())
        );
    }

    #[test]
    fn component_macro_types_implement_rsx_tag() {
        fn accepts_tag<T, P>()
        where
            T: RsxTag<P>,
            P: RsxPropsBuilder,
        {
        }

        accepts_tag::<BoolFlag, BoolFlagProps>();
        accepts_tag::<OptionalFlag, OptionalFlagProps>();
        accepts_tag::<PassThrough, PassThroughProps>();
    }

    #[test]
    fn tag_descriptor_distinguishes_same_string_tag_from_different_tag_types() {
        let old = crate::ui::build_scope(|| create_tag_element::<ElementLike, _, _>((), ()));
        let new = crate::ui::build_scope(|| create_tag_element::<AnotherElementLike, _, _>((), ()));

        let patches = reconcile(Some(&old), &new);
        assert!(matches!(patches.as_slice(), [Patch::ReplaceRoot(_)]));
    }

    #[test]
    fn rsx_supports_more_than_sixteen_direct_children() {
        let node = rsx! {
            <Element>
                <Text>A</Text>
                <Text>B</Text>
                <Text>C</Text>
                <Text>D</Text>
                <Text>E</Text>
                <Text>F</Text>
                <Text>G</Text>
                <Text>H</Text>
                <Text>I</Text>
                <Text>J</Text>
                <Text>K</Text>
                <Text>L</Text>
                <Text>M</Text>
                <Text>N</Text>
                <Text>O</Text>
                <Text>P</Text>
                <Text>Q</Text>
            </Element>
        };

        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        assert_eq!(node.children.len(), 17);
    }

    #[test]
    fn dynamic_iterable_children_without_keys_do_not_panic() {
        let node = rsx! {
            <Element>
                {vec![rsx! { <Text>A</Text> }, rsx! { <Text>B</Text> }]}
            </Element>
        };

        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        assert_eq!(node.children.len(), 2);
        assert!(
            node.children
                .iter()
                .all(|child| child.identity().key.is_none())
        );
    }

    #[test]
    fn dynamic_iterable_children_with_duplicate_keys_do_not_panic() {
        let node = rsx! {
            <Element>
                {vec![
                    rsx! { <Text key={1}>A</Text> },
                    rsx! { <Text key={1}>B</Text> },
                ]}
            </Element>
        };

        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        assert_eq!(node.children.len(), 2);
    }

    #[test]
    fn dynamic_iterable_children_with_keys_are_supported() {
        let node = rsx! {
            <Element>
                {vec![
                    rsx! { <Text key={1}>A</Text> },
                    rsx! { <Text key={2}>B</Text> },
                ]}
            </Element>
        };

        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        assert_eq!(node.children.len(), 2);
        assert!(
            node.children
                .iter()
                .all(|child| child.identity().key.is_some())
        );
    }

    #[test]
    fn passthrough_children_do_not_require_keys() {
        let node = rsx! {
            <PassThrough>
                <Text>A</Text>
                <Text>B</Text>
            </PassThrough>
        };

        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        assert_eq!(node.children.len(), 2);
    }

    #[test]
    fn boolean_shorthand_sets_required_bool_prop_true() {
        let node = rsx! { <BoolFlag flag /> };
        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        assert_eq!(node.tag, "BoolTrue");
    }

    #[test]
    fn boolean_shorthand_sets_optional_bool_prop_true() {
        let node = rsx! { <OptionalFlag flag /> };
        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        assert_eq!(node.tag, "OptionTrue");
    }

    #[test]
    fn numeric_length_style_values_coerce_to_px() {
        let node = rsx! {
            <crate::view::Element
                style={{
                    width: 10,
                    height: 12.5,
                    gap: 8,
                }}
            />
        };
        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        let style = extract_element_style(&node);
        assert_eq!(
            style.get(PropertyId::Width),
            Some(&ParsedValue::Length(Length::px(10.0)))
        );
        assert_eq!(
            style.get(PropertyId::Height),
            Some(&ParsedValue::Length(Length::px(12.5)))
        );
        assert_eq!(
            style.get(PropertyId::Gap),
            Some(&ParsedValue::Length(Length::px(8.0)))
        );
    }

    #[test]
    fn prop_macro_style_syntax_expands_to_typed_style_value() {
        let node = rsx! {
            <crate::view::Element style! { width: Length::px(10.0) } />
        };
        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        let style = extract_element_style(&node);
        assert_eq!(
            style.get(PropertyId::Width),
            Some(&ParsedValue::Length(Length::px(42.0)))
        );
    }

    #[test]
    fn numeric_font_size_style_values_coerce_to_px() {
        let node = rsx! {
            <crate::view::Element
                style={{
                    font_size: 14,
                }}
            />
        };
        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        let style = extract_element_style(&node);
        assert_eq!(
            style.get(PropertyId::FontSize),
            Some(&ParsedValue::FontSize(FontSize::px(14.0)))
        );
    }

    #[test]
    fn typed_color_style_values_coerce_via_style_field_helper() {
        let node = rsx! {
            <crate::view::Element
                style={{
                    color: Color::hex("#112233"),
                    background: Color::rgba(1, 2, 3, 255),
                }}
            />
        };
        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        let style = extract_element_style(&node);
        assert_eq!(
            style.get(PropertyId::Color),
            Some(&ParsedValue::Color(Color::rgba(17, 34, 51, 255).into()))
        );
        assert_eq!(
            style.get(PropertyId::BackgroundColor),
            Some(&ParsedValue::Color(Color::rgba(1, 2, 3, 255).into()))
        );
    }

    #[test]
    fn numeric_font_weight_style_values_coerce_via_style_field_helper() {
        let node = rsx! {
            <crate::view::Element
                style={{
                    font_weight: 700,
                }}
            />
        };
        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        let style = extract_element_style(&node);
        assert_eq!(
            style.get(PropertyId::FontWeight),
            Some(&ParsedValue::FontWeight(FontWeight::new(700)))
        );
    }

    #[test]
    fn text_wrap_style_values_use_typed_helper() {
        let node = rsx! {
            <crate::view::Element
                style={{
                    text_wrap: crate::TextWrap::NoWrap,
                }}
            />
        };
        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        let style = extract_element_style(&node);
        assert_eq!(
            style.get(PropertyId::TextWrap),
            Some(&ParsedValue::TextWrap(crate::TextWrap::NoWrap))
        );
    }

    #[test]
    fn selection_style_object_uses_typed_background() {
        let node = rsx! {
            <crate::view::Element
                style={{
                    selection: {
                        background: Color::hex("#ffffff"),
                    },
                }}
            />
        };
        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        let style = extract_element_style(&node);
        let selection = style.selection().expect("missing selection style");
        assert_eq!(
            selection.background_color(),
            Some(&Color::rgba(255, 255, 255, 255).into())
        );
    }

    #[test]
    fn transform_style_values_use_typed_helper() {
        let node = rsx! {
            <crate::view::Element
                style={{
                    transform: crate::Transform::new([
                        crate::Translate::x(crate::Length::px(10.0)),
                        crate::Rotate::z(crate::Angle::deg(12.0)),
                    ]),
                    transform_origin: crate::TransformOrigin::percent(50.0, 50.0),
                }}
            />
        };
        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        let style = extract_element_style(&node);
        assert!(matches!(
            style.get(PropertyId::Transform),
            Some(ParsedValue::Transform(_))
        ));
        assert!(matches!(
            style.get(PropertyId::TransformOrigin),
            Some(ParsedValue::TransformOrigin(_))
        ));
    }

    #[test]
    fn nested_object_prop_supports_nested_style_schema_hooks() {
        let node = rsx! {
            <NestedStyleProbe
                nested={{
                    style: {
                        width: Length::px(16.0),
                        selection: {
                            background: Color::hex("#abcdef"),
                        },
                    },
                }}
            />
        };
        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        let style = extract_element_style(&node);
        assert_eq!(
            style.get(PropertyId::Width),
            Some(&ParsedValue::Length(Length::px(16.0)))
        );
        let selection = style.selection().expect("missing selection style");
        assert_eq!(
            selection.background_color(),
            Some(&Color::rgba(171, 205, 239, 255).into())
        );
    }

    #[test]
    fn event_props_accept_bare_closures_for_all_handler_types() {
        let node = rsx! {
            <Element
                on_mouse_down={move |_event| {}}
                on_mouse_up={move |_event| {}}
                on_mouse_move={move |_event| {}}
                on_mouse_enter={move |_event| {}}
                on_mouse_leave={move |_event| {}}
                on_click={move |_event| {}}
                on_key_down={move |_event| {}}
                on_key_up={move |_event| {}}
                on_focus={move |_event| {}}
                on_blur={move |_event| {}}
            />
        };

        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        assert_eq!(node.props.len(), 10);
        assert!(node.props.iter().any(|(key, _)| key == "on_click"));
        assert!(node.props.iter().any(|(key, _)| key == "on_key_down"));
    }

    #[test]
    fn text_area_on_change_accepts_typed_event_closure() {
        let node = rsx! {
            <crate::view::TextArea
                on_change={move |event: &mut crate::ui::TextChangeEvent| event.meta.stop_propagation()}
            />
        };

        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        assert!(node.props.iter().any(|(key, _)| key == "on_change"));
    }

    #[test]
    fn text_area_on_focus_accepts_target_selection_methods() {
        let node = rsx! {
            <crate::view::TextArea
                on_focus={move |event| event.target.select_all()}
            />
        };

        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        assert!(node.props.iter().any(|(key, _)| key == "on_focus"));
    }

    #[test]
    fn text_area_on_blur_accepts_typed_event_closure() {
        let node = rsx! {
            <crate::view::TextArea
                on_blur={move |event: &mut crate::ui::BlurEvent| event.meta.stop_propagation()}
            />
        };

        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        assert!(node.props.iter().any(|(key, _)| key == "on_blur"));
    }

    #[test]
    fn click_event_props_accept_zero_arg_closures() {
        let called = Rc::new(Cell::new(false));
        let called_for_handler = called.clone();
        let node = rsx! {
            <Element on_click={move || called_for_handler.set(true)} />
        };

        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        let Some((_, PropValue::OnClick(handler))) =
            node.props.iter().find(|(key, _)| key == "on_click")
        else {
            panic!("missing on_click prop");
        };

        let mut event = ClickEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: 0.0,
                viewport_y: 0.0,
                local_x: 0.0,
                local_y: 0.0,
                current_target_width: 0.0,
                current_target_height: 0.0,
                button: Some(MouseButton::Left),
                buttons: MouseButtons::default(),
                modifiers: crate::ui::KeyModifiers::default(),
            },
        };
        handler.call(&mut event);
        assert!(called.get());
    }

    #[test]
    fn key_event_props_accept_bare_closures_with_event_argument() {
        let node = rsx! {
            <Element on_key_down={move |event| event.meta.stop_propagation()} />
        };

        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        let Some((_, PropValue::OnKeyDown(handler))) =
            node.props.iter().find(|(key, _)| key == "on_key_down")
        else {
            panic!("missing on_key_down prop");
        };

        let mut event = KeyDownEvent {
            meta: EventMeta::new(0),
            key: KeyEventData {
                key: "Enter".to_string(),
                code: "Enter".to_string(),
                repeat: false,
                modifiers: crate::ui::KeyModifiers::default(),
            },
        };
        handler.call(&mut event);
        assert!(event.meta.propagation_stopped());
    }

    #[test]
    fn event_prop_variables_accept_typed_event_closures() {
        let called = Rc::new(Cell::new(false));
        let called_for_handler = called.clone();
        let handler = move |_event: &mut ClickEvent| {
            called_for_handler.set(true);
        };

        let node = rsx! {
            <Element on_click={handler} />
        };

        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        let Some((_, PropValue::OnClick(handler))) =
            node.props.iter().find(|(key, _)| key == "on_click")
        else {
            panic!("missing on_click prop");
        };

        let mut event = ClickEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: 0.0,
                viewport_y: 0.0,
                local_x: 0.0,
                local_y: 0.0,
                current_target_width: 0.0,
                current_target_height: 0.0,
                button: Some(MouseButton::Left),
                buttons: MouseButtons::default(),
                modifiers: crate::ui::KeyModifiers::default(),
            },
        };
        handler.call(&mut event);
        assert!(called.get());
    }
}
