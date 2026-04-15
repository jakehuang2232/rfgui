use crate::rfgui::ui::{RsxNode, rsx, use_state, use_viewport};
use crate::rfgui::{Border, Color, Layout, Length, Padding};
use crate::rfgui_components::{Switch, Theme, use_theme};
use rfgui::view::Element;
use std::rc::Rc;

pub fn build(theme: &Theme) -> RsxNode {
    let dark_mode = use_state(|| true);
    let debug_geometry_overlay = use_state(|| false);
    let debug_render_time = use_state(|| false);
    let debug_compile_detail = use_state(|| false);
    let debug_reuse_path = use_state(|| false);
    let enable_layer_promotion = use_state(|| true);

    let show_reuse_legend = debug_reuse_path.get();

    // One viewport handle captured up front; cloned into every callback.
    let viewport = use_viewport();
    let (_, set_theme) = use_theme();

    let on_dark_mode = {
        let set_theme = set_theme.clone();
        Rc::new(move |on: bool| {
            if on {
                set_theme(Theme::dark());
                // Match the runner's startup clear color for dark theme
                // so the viewport background tracks the switch without a
                // second round trip through app config.
                viewport.set_clear_color(Color::rgb(40, 44, 52));
            } else {
                set_theme(Theme::light());
                viewport.set_clear_color(Color::rgb(240, 240, 240));
            }
        }) as Rc<dyn Fn(bool)>
    };
    let on_geometry_overlay = {
        let vp = viewport;
        Rc::new(move |on: bool| vp.set_debug_geometry_overlay(on)) as Rc<dyn Fn(bool)>
    };
    let on_render_time = {
        let vp = viewport;
        Rc::new(move |on: bool| vp.set_debug_trace_render_time(on)) as Rc<dyn Fn(bool)>
    };
    let on_compile_detail = {
        let vp = viewport;
        Rc::new(move |on: bool| vp.set_debug_trace_compile_detail(on)) as Rc<dyn Fn(bool)>
    };
    let on_reuse_path = {
        let vp = viewport;
        Rc::new(move |on: bool| vp.set_debug_trace_reuse_path(on)) as Rc<dyn Fn(bool)>
    };
    let on_layer_promotion = {
        let vp = viewport;
        Rc::new(move |on: bool| vp.set_promotion_enabled(on)) as Rc<dyn Fn(bool)>
    };

    rsx! {
        <Element style={{
            gap: theme.spacing.xs,
            layout: Layout::flow().column().no_wrap(),
            width: Length::percent(100.0),
        }}>
            <Switch
                label="Dark mode"
                binding={dark_mode.binding()}
                on_change={on_dark_mode}
            />
            <Switch
                label="Debug Geometry Overlay"
                binding={debug_geometry_overlay.binding()}
                on_change={on_geometry_overlay}
            />
            <Switch
                label="Debug Render Time"
                binding={debug_render_time.binding()}
                on_change={on_render_time}
            />
            <Switch
                label="Debug Compile Detail"
                binding={debug_compile_detail.binding()}
                on_change={on_compile_detail}
            />
            <Switch
                label="Debug Reuse Path"
                binding={debug_reuse_path.binding()}
                on_change={on_reuse_path}
            />
            {if show_reuse_legend {
                rsx! {
                    <Element style={{
                        layout: Layout::flow().column().no_wrap(),
                        gap: theme.spacing.xs,
                        padding: Padding::uniform(theme.spacing.xs),
                        border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                        width: Length::percent(100.0),
                    }}>
                        <Element>"Reuse Path Colors"</Element>
                        <Element style={{ layout: Layout::flow().row().no_wrap(), gap: theme.spacing.xs, width: Length::percent(100.0) }}>
                            <Element style={{
                                width: Length::px(12.0),
                                height: Length::px(12.0),
                                background: Color::hex("#26f25a"),
                            }} />
                            <Element>"Green: actual reuse"</Element>
                        </Element>
                        <Element style={{ layout: Layout::flow().row().no_wrap(), gap: theme.spacing.xs, width: Length::percent(100.0) }}>
                            <Element style={{
                                width: Length::px(12.0),
                                height: Length::px(12.0),
                                background: Color::hex("#ff731a"),
                            }} />
                            <Element>"Orange: actual reraster"</Element>
                        </Element>
                        <Element style={{ layout: Layout::flow().row().no_wrap(), gap: theme.spacing.xs, width: Length::percent(100.0) }}>
                            <Element style={{
                                width: Length::px(12.0),
                                height: Length::px(12.0),
                                background: Color::hex("#ffe526"),
                            }} />
                            <Element>"Yellow: child scissor clip inline"</Element>
                        </Element>
                        <Element style={{ layout: Layout::flow().row().no_wrap(), gap: theme.spacing.xs, width: Length::percent(100.0) }}>
                            <Element style={{
                                width: Length::px(12.0),
                                height: Length::px(12.0),
                                background: Color::hex("#ff8c26"),
                            }} />
                            <Element>"Deep orange: child stencil clip inline"</Element>
                        </Element>
                        <Element style={{ layout: Layout::flow().row().no_wrap(), gap: theme.spacing.xs, width: Length::percent(100.0) }}>
                            <Element style={{
                                width: Length::px(12.0),
                                height: Length::px(12.0),
                                background: Color::hex("#ff3333"),
                            }} />
                            <Element>"Red: absolute clip inline"</Element>
                        </Element>
                    </Element>
                }
            } else {
                rsx! { <Element /> }
            }}
            <Switch
                label="Enable Layer Promotion"
                binding={enable_layer_promotion.binding()}
                on_change={on_layer_promotion}
            />
        </Element>
    }
}
