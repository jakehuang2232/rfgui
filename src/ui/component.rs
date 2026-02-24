use crate::ui::RsxNode;

pub trait RsxPropSchema {
    type PropsSchema;
}

pub trait RsxChildrenPolicy {
    const ACCEPTS_CHILDREN: bool;
}

pub trait OptionalDefault: Sized {
    fn optional_default() -> Self;
}

pub trait RsxComponent: Sized {
    type Props;

    fn render(props: Self::Props) -> RsxNode;
}

pub trait RsxPropsBuilder: Sized {
    type Builder;

    fn builder() -> Self::Builder;
    fn build(builder: Self::Builder) -> Result<Self, String>;
}

impl<T> RsxPropSchema for T
where
    T: RsxComponent,
{
    type PropsSchema = T::Props;
}
