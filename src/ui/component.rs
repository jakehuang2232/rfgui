use crate::ui::{
    GlobalKey, IntoRsxNode, RsxKey, RsxNode, RsxNodeIdentity, register_global_key,
    render_component, with_component_key,
};
use std::collections::HashSet;
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
    T: IntoRsxNode,
{
    match value.into_rsx_node() {
        RsxNode::Fragment(fragment) => {
            validate_dynamic_rsx_children(&fragment.children);
            children.extend(fragment.children);
        }
        node => children.push(node),
    }
}

fn validate_dynamic_rsx_children(children: &[RsxNode]) {
    if children.len() <= 1 {
        return;
    }

    let mut seen_keys = HashSet::new();
    for child in children {
        let Some(key) = child.identity().key.clone() else {
            panic!("dynamic RSX children require `key` on each child root");
        };
        if !seen_keys.insert(key) {
            panic!("dynamic RSX children require unique sibling keys");
        }
    }
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
    use crate::ui::{Patch, RsxKey, RsxNode, component, reconcile, rsx};
    use std::marker::PhantomData;

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
    #[should_panic(expected = "dynamic RSX children require `key` on each child root")]
    fn dynamic_iterable_children_require_keys() {
        let _ = rsx! {
            <Element>
                {vec![rsx! { <Text>A</Text> }, rsx! { <Text>B</Text> }]}
            </Element>
        };
    }

    #[test]
    #[should_panic(expected = "dynamic RSX children require unique sibling keys")]
    fn dynamic_iterable_children_require_unique_keys() {
        let _ = rsx! {
            <Element>
                {vec![
                    rsx! { <Text key={1}>A</Text> },
                    rsx! { <Text key={1}>B</Text> },
                ]}
            </Element>
        };
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
}
