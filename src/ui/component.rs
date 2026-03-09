use crate::ui::{
    GlobalKey, IntoRsxNode, RsxKey, RsxNode, RsxNodeIdentity, register_global_key,
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

macro_rules! impl_into_rsx_children_tuple {
    ($($name:ident),+ $(,)?) => {
        impl<$($name),+> IntoRsxChildren for ($($name,)+)
        where
            $($name: IntoRsxNode,)+
        {
            #[allow(non_snake_case)]
            fn into_rsx_children(self) -> Vec<RsxNode> {
                let ($($name,)+) = self;
                vec![$($name.into_rsx_node(),)+]
            }
        }
    };
}

impl_into_rsx_children_tuple!(A);
impl_into_rsx_children_tuple!(A, B);
impl_into_rsx_children_tuple!(A, B, C);
impl_into_rsx_children_tuple!(A, B, C, D);
impl_into_rsx_children_tuple!(A, B, C, D, E);
impl_into_rsx_children_tuple!(A, B, C, D, E, F);
impl_into_rsx_children_tuple!(A, B, C, D, E, F, G);
impl_into_rsx_children_tuple!(A, B, C, D, E, F, G, H);
impl_into_rsx_children_tuple!(A, B, C, D, E, F, G, H, I);
impl_into_rsx_children_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_into_rsx_children_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_into_rsx_children_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);
impl_into_rsx_children_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M);
impl_into_rsx_children_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
impl_into_rsx_children_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
impl_into_rsx_children_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);

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

#[cfg(test)]
mod tests {
    use super::{GlobalKey, RsxChildrenPolicy, RsxComponent, create_element_with_key};
    use crate::ui::{Patch, RsxKey, RsxNode, reconcile};
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
}
