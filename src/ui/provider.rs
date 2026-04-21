//! React-style `<Provider<T> value={...}>` component for walker-ancestry
//! context injection.
//!
//! Drop-in replacement for direct [`provide_context_node`] calls that lets
//! compound components use JSX-shaped syntax:
//!
//! ```ignore
//! rsx! {
//!     <Provider::<MyCtx> value={ctx}>
//!         <Child/>
//!     </Provider>
//! }
//! ```
//!
//! Walker pushes `(TypeId::of::<T>(), value)` onto `CONTEXT_STACK` for the
//! duration of the descent into `children`; `use_context::<T>()` inside
//! any descendant (including children the user passed in from an outer
//! rsx) reads the value.

use crate::ui::{
    RsxComponent, RsxFragmentNode, RsxNode, RsxNodeIdentity, provide_context_node,
};
use ::rfgui_rsx::props;
use std::marker::PhantomData;
use std::rc::Rc;

pub struct Provider<T>(PhantomData<T>);

#[derive(Clone)]
#[props]
pub struct ProviderProps<T> {
    pub value: T,
}

impl<T: Clone + 'static> RsxComponent<ProviderProps<T>> for Provider<T> {
    fn render(props: ProviderProps<T>, children: Vec<RsxNode>) -> RsxNode {
        let child = match children.len() {
            1 => children.into_iter().next().unwrap(),
            _ => RsxNode::Fragment(Rc::new(RsxFragmentNode {
                identity: RsxNodeIdentity::new("Provider::children", None),
                children,
            })),
        };
        provide_context_node(props.value, child)
    }
}

#[::rfgui_rsx::component]
impl<T> crate::ui::RsxTag for Provider<T>
where
    T: Clone + 'static,
{
    type Props = __ProviderPropsInit<T>;
    type StrictProps = ProviderProps<T>;
    const ACCEPTS_CHILDREN: bool = true;

    fn into_strict(p: Self::Props) -> Self::StrictProps {
        p.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<RsxNode>,
        _key: Option<crate::ui::RsxKey>,
    ) -> RsxNode {
        <Self as RsxComponent<ProviderProps<T>>>::render(props, children)
    }
}
