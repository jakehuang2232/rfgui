use crate::rfgui::ui::{Binding, RsxNode, rsx};
use crate::rfgui::{Layout, Length};
use crate::rfgui_components::{Button, ButtonVariant, Switch, Theme};
use crate::state::REQUEST_DUMP_FRAME_GRAPH_DOT;
use std::sync::atomic::Ordering;
use rfgui::ui::host::Element;

pub struct InspectorPanelBindings {
    pub switch_on: Binding<bool>,
    pub debug_geometry_overlay: Binding<bool>,
    pub debug_render_time: Binding<bool>,
}

pub fn build(theme: &Theme, bindings: InspectorPanelBindings) -> RsxNode {
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
