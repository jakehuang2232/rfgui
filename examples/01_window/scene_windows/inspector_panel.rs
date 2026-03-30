use crate::rfgui::ui::{Binding, RsxNode, rsx};
use crate::rfgui::{Border, Color, Layout, Length, Padding};
use crate::rfgui_components::{Button, ButtonVariant, Switch, Theme};
use crate::state::REQUEST_DUMP_FRAME_GRAPH_DOT;
use rfgui::view::Element;
use std::sync::atomic::Ordering;

pub struct InspectorPanelBindings {
    pub switch_on: Binding<bool>,
    pub debug_geometry_overlay: Binding<bool>,
    pub debug_render_time: Binding<bool>,
    pub debug_reuse_path: Binding<bool>,
    pub enable_layer_promotion: Binding<bool>,
}

pub fn build(theme: &Theme, bindings: InspectorPanelBindings) -> RsxNode {
    let debug_reuse_path_binding = bindings.debug_reuse_path.clone();
    let show_reuse_legend = debug_reuse_path_binding.get();
    rsx! {
        <Element style={{
            gap: theme.spacing.xs,
            layout: Layout::flow().column().no_wrap(),
            width: Length::percent(100.0),
        }}>
            <Switch
                label="Dark mode"
                binding={bindings.switch_on}
            />
            <Switch
                label="Debug Geometry Overlay"
                binding={bindings.debug_geometry_overlay}
            />
            <Switch
                label="Debug Render Time"
                binding={bindings.debug_render_time}
            />
            <Switch
                label="Debug Reuse Path"
                binding={debug_reuse_path_binding}
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
                rsx! { <Element style={{background: "#ff0000"}}/> }
            }}
            <Switch
                label="Enable Layer Promotion"
                binding={bindings.enable_layer_promotion}
            />
            <Button
                label="Dump FrameGraph DOT"
                variant={Some(ButtonVariant::Outlined)}
                on_click={move |_| {
                    REQUEST_DUMP_FRAME_GRAPH_DOT.store(true, Ordering::Release);
                }}
            />
        </Element>
    }
}
