use crate::rfgui::style::{Layout, Length};
use crate::rfgui::ui::{PointerEnterHandlerProp, PointerLeaveHandlerProp, RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::material_symbol::FavoriteIcon;
use crate::rfgui_components::{
    Button, ButtonVariant, Tooltip, TooltipPlacement, use_theme, use_tooltip_ref,
};
use crate::showcase::{Caption, Demo};
use rfgui::style::Padding;

#[component]
pub fn Tooltips() -> RsxNode {
    let (theme, _) = use_theme();
    let standalone_ref = use_tooltip_ref();
    let on_enter = {
        let r = standalone_ref.clone();
        PointerEnterHandlerProp::new(move |_| r.show())
    };
    let on_leave = {
        let r = standalone_ref.clone();
        PointerLeaveHandlerProp::new(move |_| r.hide())
    };

    rsx! {
        <Demo
            title="Tooltip"
            description="Tooltips open on hover at any of twelve placements, flipping when they would leave the viewport. A standalone tooltip is shown and hidden through a handle and can hold any content."
        >
            <Caption text="Placement" />
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(rfgui::style::Align::Center),
                gap: theme.spacing.lg,
                padding: Padding::uniform(theme.spacing.md),
            }}>
                <Button
                    tooltip={Some(rsx! {
                        <Tooltip placement={TooltipPlacement::Top}>"Top placement"</Tooltip>
                    })}
                >Top</Button>
                <Button
                    tooltip={Some(rsx! {
                        <Tooltip placement={TooltipPlacement::Bottom}>"Bottom placement"</Tooltip>
                    })}
                >Bottom</Button>
                <Button
                    tooltip={Some(rsx! {
                        <Tooltip placement={TooltipPlacement::Left}>"Left placement"</Tooltip>
                    })}
                >Left</Button>
                <Button
                    tooltip={Some(rsx! {
                        <Tooltip placement={TooltipPlacement::Right}>"Right placement"</Tooltip>
                    })}
                >Right</Button>
            </Element>

            <Caption text="Start and end alignment" />
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(rfgui::style::Align::Center),
                gap: theme.spacing.lg,
                padding: Padding::uniform(theme.spacing.md),
            }}>
                <Button
                    variant={Some(ButtonVariant::Outlined)}
                    tooltip={Some(rsx! {
                        <Tooltip placement={TooltipPlacement::BottomStart}>"bottom-start"</Tooltip>
                    })}
                >BottomStart</Button>
                <Button
                    variant={Some(ButtonVariant::Outlined)}
                    tooltip={Some(rsx! {
                        <Tooltip placement={TooltipPlacement::BottomEnd}>"bottom-end"</Tooltip>
                    })}
                >BottomEnd</Button>
                <Button
                    variant={Some(ButtonVariant::Outlined)}
                    tooltip={Some(rsx! {
                        <Tooltip placement={TooltipPlacement::TopStart}>"top-start"</Tooltip>
                    })}
                >TopStart</Button>
                <Button
                    variant={Some(ButtonVariant::Outlined)}
                    tooltip={Some(rsx! {
                        <Tooltip placement={TooltipPlacement::TopEnd}>"top-end"</Tooltip>
                    })}
                >TopEnd</Button>
            </Element>

            <Caption text="Standalone, controlled through a handle, with rich content" />
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(rfgui::style::Align::Center),
                gap: theme.spacing.lg,
                padding: Padding::uniform(theme.spacing.md),
            }}>
                <Element
                    style={{
                        padding: rfgui::style::Padding::uniform(theme.spacing.sm),
                        border: rfgui::style::Border::uniform(Length::px(0.5), theme.color.border.as_ref()),
                        border_radius: theme.radius.sm,
                    }}
                    on_pointer_enter={on_enter}
                    on_pointer_leave={on_leave}
                >
                    <Text>Hover me</Text>
                    <Tooltip
                        handle={Some(standalone_ref.clone())}
                        placement={TooltipPlacement::Bottom}
                    >
                        <Element style={{
                            layout: Layout::flow().row().no_wrap().align(rfgui::style::Align::Center),
                            gap: theme.spacing.xs,
                        }}>
                            <FavoriteIcon style={{ font_size: theme.typography.size.xs }} />
                            <Text>"Rich tooltip with icon"</Text>
                        </Element>
                    </Tooltip>
                </Element>
            </Element>
        </Demo>
    }
}
