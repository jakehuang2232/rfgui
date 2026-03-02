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
    fn render(props: Props) -> RsxNode;
}

pub trait RsxPropsBuilder: Sized {
    type Builder;

    fn builder() -> Self::Builder;
    fn build(builder: Self::Builder) -> Result<Self, String>;
}
