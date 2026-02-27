use crate::ui::RsxNode;

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

pub trait RsxComponent<Props>: Sized {
    fn render(props: Props) -> RsxNode;
}

pub trait RsxPropsBuilder: Sized {
    type Builder;

    fn builder() -> Self::Builder;
    fn build(builder: Self::Builder) -> Result<Self, String>;
}
