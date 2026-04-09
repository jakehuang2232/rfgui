//! `rfgui` is a retained-mode GUI framework for Rust built around a typed style system,
//! an RSX authoring model, and a frame-graph-driven renderer.
//!
//! The crate re-exports commonly used style and view APIs at the crate root.

extern crate self as rfgui;

#[doc(hidden)]
#[macro_export]
macro_rules! __rfgui_style_entries {
    ($builder:ident, $kind:ident,) => {};
    ($builder:ident, $kind:ident, $key:ident : { $($inner:tt)* } , $($rest:tt)*) => {
        $crate::__rfgui_style_assign_nested!($builder, $kind, $key, { $($inner)* });
        $crate::__rfgui_style_entries!($builder, $kind, $($rest)*);
    };
    ($builder:ident, $kind:ident, $key:ident : { $($inner:tt)* }) => {
        $crate::__rfgui_style_assign_nested!($builder, $kind, $key, { $($inner)* });
    };
    ($builder:ident, $kind:ident, $key:ident : $value:expr , $($rest:tt)*) => {
        $crate::__rfgui_style_assign_expr!($builder, $kind, $key, $value);
        $crate::__rfgui_style_entries!($builder, $kind, $($rest)*);
    };
    ($builder:ident, $kind:ident, $key:ident : $value:expr) => {
        $crate::__rfgui_style_assign_expr!($builder, $kind, $key, $value);
    };
    ($builder:ident, $kind:ident, $key:ident , $($rest:tt)*) => {
        $crate::__rfgui_style_assign_expr!($builder, $kind, $key, $key);
        $crate::__rfgui_style_entries!($builder, $kind, $($rest)*);
    };
    ($builder:ident, $kind:ident, $key:ident) => {
        $crate::__rfgui_style_assign_expr!($builder, $kind, $key, $key);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __rfgui_style_assign_nested {
    ($builder:ident, element, hover, { $($inner:tt)* }) => {
        $builder.hover($crate::__rfgui_style_build_hover! { $($inner)* });
    };
    ($builder:ident, element, selection, { $($inner:tt)* }) => {
        $builder.selection($crate::__rfgui_style_build_selection! { $($inner)* });
    };
    ($builder:ident, hover, selection, { $($inner:tt)* }) => {
        $builder.selection($crate::__rfgui_style_build_selection! { $($inner)* });
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __rfgui_style_assign_expr {
    ($builder:ident, $kind:ident, position, $value:expr) => { $builder.position($value); };
    ($builder:ident, $kind:ident, width, $value:expr) => { $builder.width($value); };
    ($builder:ident, $kind:ident, height, $value:expr) => { $builder.height($value); };
    ($builder:ident, $kind:ident, min_width, $value:expr) => { $builder.min_width($value); };
    ($builder:ident, $kind:ident, max_width, $value:expr) => { $builder.max_width($value); };
    ($builder:ident, $kind:ident, min_height, $value:expr) => { $builder.min_height($value); };
    ($builder:ident, $kind:ident, max_height, $value:expr) => { $builder.max_height($value); };
    ($builder:ident, $kind:ident, layout, $value:expr) => { $builder.layout($value); };
    ($builder:ident, $kind:ident, cross_size, $value:expr) => { $builder.cross_size($value); };
    ($builder:ident, $kind:ident, align, $value:expr) => { $builder.align($value); };
    ($builder:ident, $kind:ident, flex, $value:expr) => { $builder.flex($value); };
    ($builder:ident, $kind:ident, gap, $value:expr) => { $builder.gap($value); };
    ($builder:ident, $kind:ident, scroll_direction, $value:expr) => { $builder.scroll_direction($value); };
    ($builder:ident, $kind:ident, cursor, $value:expr) => { $builder.cursor($value); };
    ($builder:ident, $kind:ident, color, $value:expr) => { $builder.color($value); };
    ($builder:ident, $kind:ident, border, $value:expr) => { $builder.border($value); };
    ($builder:ident, $kind:ident, background, $value:expr) => { $builder.background($value); };
    ($builder:ident, $kind:ident, background_color, $value:expr) => { $builder.background_color($value); };
    ($builder:ident, $kind:ident, font, $value:expr) => { $builder.font($value); };
    ($builder:ident, $kind:ident, font_size, $value:expr) => { $builder.font_size($value); };
    ($builder:ident, $kind:ident, font_weight, $value:expr) => { $builder.font_weight($value); };
    ($builder:ident, $kind:ident, text_wrap, $value:expr) => { $builder.text_wrap($value); };
    ($builder:ident, $kind:ident, border_radius, $value:expr) => { $builder.border_radius($value); };
    ($builder:ident, $kind:ident, opacity, $value:expr) => { $builder.opacity($value); };
    ($builder:ident, $kind:ident, box_shadow, $value:expr) => { $builder.box_shadow($value); };
    ($builder:ident, $kind:ident, padding, $value:expr) => { $builder.padding($value); };
    ($builder:ident, $kind:ident, transform, $value:expr) => { $builder.transform($value); };
    ($builder:ident, $kind:ident, transform_origin, $value:expr) => { $builder.transform_origin($value); };
    ($builder:ident, $kind:ident, transition, $value:expr) => { $builder.transition($value); };
    ($builder:ident, $kind:ident, animator, $value:expr) => { $builder.animator($value); };
    ($builder:ident, selection, background, $value:expr) => { $builder.background($value); };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __rfgui_style_build_hover {
    ($($tt:tt)*) => {{
        $crate::ui::build_typed_prop::<$crate::view::HoverElementStylePropSchema, _>(
            |__rfgui_style_builder| {
                $crate::__rfgui_style_entries!(__rfgui_style_builder, hover, $($tt)*);
            }
        )
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __rfgui_style_build_selection {
    ($($tt:tt)*) => {{
        $crate::ui::build_typed_prop::<$crate::view::SelectionStylePropSchema, _>(
            |__rfgui_style_builder| {
                $crate::__rfgui_style_entries!(__rfgui_style_builder, selection, $($tt)*);
            }
        )
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __rfgui_style_build_element {
    ($($tt:tt)*) => {{
        $crate::ui::build_typed_prop::<$crate::view::ElementStylePropSchema, _>(
            |__rfgui_style_builder| {
                $crate::__rfgui_style_entries!(__rfgui_style_builder, element, $($tt)*);
            }
        )
    }};
}

#[macro_export]
macro_rules! style {
    ($($tt:tt)*) => {{
        $crate::__rfgui_style_build_element! { $($tt)* }.to_style()
    }};
}

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
