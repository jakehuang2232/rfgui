use crate::ui::{
    GlobalKey, RsxKey, RsxNode, RsxNodeIdentity, register_global_key, render_component,
    with_component_key,
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

impl<'a> IntoOptionalProp<crate::Color> for crate::HexColor<'a> {
    fn into_optional_prop(self) -> Option<crate::Color> {
        Some(crate::IntoColor::<crate::Color>::into_color(self))
    }
}

impl IntoOptionalProp<String> for &str {
    fn into_optional_prop(self) -> Option<String> {
        Some(self.to_string())
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

pub trait RsxPropsBuilder: Sized {
    type Builder;

    fn builder() -> Self::Builder;
    fn build(builder: Self::Builder) -> Result<Self, String>;
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

pub fn create_element<T, P, C>(element_type: PhantomData<T>, props: P, children: C) -> RsxNode
where
    T: RsxChildrenPolicy + RsxComponent<P> + 'static,
    C: IntoRsxChildren,
{
    create_element_with_key(element_type, props, children, None)
}

pub fn create_element_with_key<T, P, C>(
    _element_type: PhantomData<T>,
    props: P,
    children: C,
    key: Option<RsxKey>,
) -> RsxNode
where
    T: RsxChildrenPolicy + RsxComponent<P> + 'static,
    C: IntoRsxChildren,
{
    let children = children.into_rsx_children();
    debug_assert!(T::ACCEPTS_CHILDREN || children.is_empty());
    if let Some(RsxKey::Global(global_key)) = key {
        register_global_key(global_key);
    }
    with_component_key(key.clone(), || {
        let mut node = render_component::<T, _>(|| T::render(props, children));
        node.set_identity(RsxNodeIdentity::new(std::any::type_name::<T>(), key));
        node
    })
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
    use super::{GlobalKey, RsxChildrenPolicy, RsxComponent, create_element_with_key};
    use crate::style::{Color, FontSize, FontWeight, Length, ParsedValue, PropertyId};
    use crate::ui::host::{Element, Text};
    use crate::ui::{
        ClickEvent, EventMeta, KeyDownEvent, KeyEventData, MouseButton, MouseButtons,
        MouseEventData, Patch, PropValue, RsxKey, RsxNode, component, reconcile, rsx,
    };
    use std::cell::Cell;
    use std::marker::PhantomData;
    use std::rc::Rc;

    struct Button;
    struct ElementLike;

    impl RsxChildrenPolicy for Button {
        const ACCEPTS_CHILDREN: bool = false;
    }

    impl RsxChildrenPolicy for ElementLike {
        const ACCEPTS_CHILDREN: bool = false;
    }

    impl RsxComponent<()> for Button {
        fn render(_: (), _: Vec<RsxNode>) -> RsxNode {
            RsxNode::element("Element")
        }
    }

    impl RsxComponent<()> for ElementLike {
        fn render(_: (), _: Vec<RsxNode>) -> RsxNode {
            RsxNode::element("Element")
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

    #[test]
    #[should_panic(expected = "duplicate GlobalKey detected in the same build")]
    fn duplicate_global_key_panics_in_same_build() {
        crate::ui::build_scope(|| {
            let global_key = GlobalKey::from("dup");
            let _ = create_element_with_key::<Button, _, _>(
                PhantomData,
                (),
                (),
                Some(RsxKey::Global(global_key)),
            );
            let _ = create_element_with_key::<Button, _, _>(
                PhantomData,
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
            create_element_with_key::<Button, _, _>(
                PhantomData,
                (),
                (),
                Some(RsxKey::Global(global_key)),
            )
        });
        let new = crate::ui::build_scope(|| {
            create_element_with_key::<ElementLike, _, _>(
                PhantomData,
                (),
                (),
                Some(RsxKey::Global(global_key)),
            )
        });

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
            <crate::ui::host::Element
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
        let style = node
            .props
            .iter()
            .find_map(|(key, value)| match (key.as_str(), value) {
                ("style", crate::ui::PropValue::Style(style)) => Some(style),
                _ => None,
            })
            .expect("missing style prop");
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
    fn numeric_font_size_style_values_coerce_to_px() {
        let node = rsx! {
            <crate::ui::host::Element
                style={{
                    font_size: 14,
                }}
            />
        };
        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        let style = node
            .props
            .iter()
            .find_map(|(key, value)| match (key.as_str(), value) {
                ("style", crate::ui::PropValue::Style(style)) => Some(style),
                _ => None,
            })
            .expect("missing style prop");
        assert_eq!(
            style.get(PropertyId::FontSize),
            Some(&ParsedValue::FontSize(FontSize::px(14.0)))
        );
    }

    #[test]
    fn typed_color_style_values_coerce_via_style_field_helper() {
        let node = rsx! {
            <crate::ui::host::Element
                style={{
                    color: Color::hex("#112233"),
                    background: Color::rgba(1, 2, 3, 255),
                }}
            />
        };
        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        let style = node
            .props
            .iter()
            .find_map(|(key, value)| match (key.as_str(), value) {
                ("style", crate::ui::PropValue::Style(style)) => Some(style),
                _ => None,
            })
            .expect("missing style prop");
        assert_eq!(
            style.get(PropertyId::Color),
            Some(&ParsedValue::Color(Color::rgba(17, 34, 51, 255)))
        );
        assert_eq!(
            style.get(PropertyId::BackgroundColor),
            Some(&ParsedValue::Color(Color::rgba(1, 2, 3, 255)))
        );
    }

    #[test]
    fn numeric_font_weight_style_values_coerce_via_style_field_helper() {
        let node = rsx! {
            <crate::ui::host::Element
                style={{
                    font_weight: 700,
                }}
            />
        };
        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        let style = node
            .props
            .iter()
            .find_map(|(key, value)| match (key.as_str(), value) {
                ("style", crate::ui::PropValue::Style(style)) => Some(style),
                _ => None,
            })
            .expect("missing style prop");
        assert_eq!(
            style.get(PropertyId::FontWeight),
            Some(&ParsedValue::FontWeight(FontWeight::new(700)))
        );
    }

    #[test]
    fn text_wrap_style_values_use_typed_helper() {
        let node = rsx! {
            <crate::ui::host::Element
                style={{
                    text_wrap: crate::TextWrap::NoWrap,
                }}
            />
        };
        let RsxNode::Element(node) = node else {
            panic!("expected element node");
        };
        let style = node
            .props
            .iter()
            .find_map(|(key, value)| match (key.as_str(), value) {
                ("style", crate::ui::PropValue::Style(style)) => Some(style),
                _ => None,
            })
            .expect("missing style prop");
        assert_eq!(
            style.get(PropertyId::TextWrap),
            Some(&ParsedValue::TextWrap(crate::TextWrap::NoWrap))
        );
    }

    #[test]
    fn selection_style_object_uses_typed_background() {
        let node = rsx! {
            <crate::ui::host::Element
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
        let style = node
            .props
            .iter()
            .find_map(|(key, value)| match (key.as_str(), value) {
                ("style", crate::ui::PropValue::Style(style)) => Some(style),
                _ => None,
            })
            .expect("missing style prop");
        let selection = style.selection().expect("missing selection style");
        assert_eq!(
            selection.background_color(),
            Some(Color::rgba(255, 255, 255, 255))
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
            <crate::ui::host::TextArea
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
            <crate::ui::host::TextArea
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
            <crate::ui::host::TextArea
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
                button: Some(MouseButton::Left),
                buttons: MouseButtons::default(),
                modifiers: crate::ui::KeyModifiers::default(),
            },
        };
        handler.call(&mut event);
        assert!(called.get());
    }
}
