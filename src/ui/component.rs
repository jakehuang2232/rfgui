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

impl<T> RsxPropSchema for T
where
    T: RsxComponent,
{
    type PropsSchema = T::Props;
}
