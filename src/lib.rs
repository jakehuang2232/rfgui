//! `rfgui` is a retained-mode GUI framework for Rust built around a typed style system,
//! an RSX authoring model, and a frame-graph-driven renderer.
//!
//! The crate re-exports commonly used style and view APIs at the crate root.

extern crate self as rfgui;

#[doc(hidden)]
#[macro_export]
macro_rules! __rfgui_style_entries {
    ($target:ident, $kind:ident,) => {};
    ($target:ident, $kind:ident, $key:ident : { $($inner:tt)* } , $($rest:tt)*) => {
        $crate::__rfgui_style_assign_nested!($target, $kind, $key, { $($inner)* });
        $crate::__rfgui_style_entries!($target, $kind, $($rest)*);
    };
    ($target:ident, $kind:ident, $key:ident : { $($inner:tt)* }) => {
        $crate::__rfgui_style_assign_nested!($target, $kind, $key, { $($inner)* });
    };
    ($target:ident, $kind:ident, $key:ident : $value:expr , $($rest:tt)*) => {
        $target.$key = $crate::ui::IntoOptionalProp::into_optional_prop($value);
        $crate::__rfgui_style_entries!($target, $kind, $($rest)*);
    };
    ($target:ident, $kind:ident, $key:ident : $value:expr) => {
        $target.$key = $crate::ui::IntoOptionalProp::into_optional_prop($value);
    };
    ($target:ident, $kind:ident, $key:ident , $($rest:tt)*) => {
        $target.$key = $crate::ui::IntoOptionalProp::into_optional_prop($key);
        $crate::__rfgui_style_entries!($target, $kind, $($rest)*);
    };
    ($target:ident, $kind:ident, $key:ident) => {
        $target.$key = $crate::ui::IntoOptionalProp::into_optional_prop($key);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __rfgui_style_assign_nested {
    ($target:ident, element, hover, { $($inner:tt)* }) => {
        $target.hover = ::core::option::Option::Some(
            $crate::__rfgui_style_build_hover! { $($inner)* },
        );
    };
    ($target:ident, element, selection, { $($inner:tt)* }) => {
        $target.selection = ::core::option::Option::Some(
            $crate::__rfgui_style_build_selection! { $($inner)* },
        );
    };
    ($target:ident, hover, selection, { $($inner:tt)* }) => {
        $target.selection = ::core::option::Option::Some(
            $crate::__rfgui_style_build_selection! { $($inner)* },
        );
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __rfgui_style_build_hover {
    ($($tt:tt)*) => {{
        let mut __rfgui_style_target = <$crate::view::HoverElementStylePropSchema
            as ::core::default::Default>::default();
        $crate::__rfgui_style_entries!(__rfgui_style_target, hover, $($tt)*);
        __rfgui_style_target
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __rfgui_style_build_selection {
    ($($tt:tt)*) => {{
        let mut __rfgui_style_target = <$crate::view::SelectionStylePropSchema
            as ::core::default::Default>::default();
        $crate::__rfgui_style_entries!(__rfgui_style_target, selection, $($tt)*);
        __rfgui_style_target
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __rfgui_style_build_element {
    ($($tt:tt)*) => {{
        let mut __rfgui_style_target = <$crate::view::ElementStylePropSchema
            as ::core::default::Default>::default();
        $crate::__rfgui_style_entries!(__rfgui_style_target, element, $($tt)*);
        __rfgui_style_target
    }};
}

#[macro_export]
macro_rules! style {
    ($($tt:tt)*) => {{
        $crate::__rfgui_style_build_element! { $($tt)* }.to_style()
    }};
}

/// `App` trait + supporting types — contract between user code and host
/// runners. The engine itself never drives an event loop.
pub mod app;
/// Platform abstraction traits (surface target, clipboard, cursor sink, ...).
/// Phase 0 of the viewport-decoupling work. No backend code lives here.
pub mod platform;
mod style;
pub mod time {
    pub use std::time::Duration;
    #[cfg(not(target_arch = "wasm32"))]
    pub use std::time::Instant;
    #[cfg(target_arch = "wasm32")]
    pub use web_time::Instant;
}
/// Transition and animation primitives used by the retained UI runtime.
pub mod transition;
/// RSX authoring, component, state, event, and reconciliation APIs.
pub mod ui;
/// Viewport integration, built-in host tags, and low-level base components.
pub mod view;

pub use style::*;
pub use view::register_font_bytes;
pub use view::*;
