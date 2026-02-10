use crate::ui::{RsxNode, RsxProps};

pub trait FromRsxProps: Sized {
    const ACCEPTS_CHILDREN: bool;

    fn from_rsx_props(props: RsxProps, children: Vec<RsxNode>) -> Result<Self, String>;
}

pub trait RsxTag {
    fn rsx_render(props: RsxProps, children: Vec<RsxNode>) -> Result<RsxNode, String>;
}

pub trait RsxPropSchema {
    type PropsSchema;
}

pub trait RsxChildrenPolicy {
    const ACCEPTS_CHILDREN: bool;
}

pub trait RsxComponent: Sized {
    type Props: FromRsxProps;

    fn render(props: Self::Props) -> RsxNode;
}

impl<T> RsxTag for T
where
    T: RsxComponent,
{
    fn rsx_render(props: RsxProps, children: Vec<RsxNode>) -> Result<RsxNode, String> {
        let parsed = T::Props::from_rsx_props(props, children)?;
        Ok(T::render(parsed))
    }
}

impl<T> RsxPropSchema for T
where
    T: RsxComponent,
{
    type PropsSchema = T::Props;
}

impl<T> RsxChildrenPolicy for T
where
    T: RsxComponent,
{
    const ACCEPTS_CHILDREN: bool = <T::Props as FromRsxProps>::ACCEPTS_CHILDREN;
}
