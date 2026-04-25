use crate::rfgui::ui::{PointerEnterHandlerProp, PointerLeaveHandlerProp, RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui::{Layout, Length};
use crate::rfgui_components::{
    Button, ButtonVariant, FavoriteIcon, Theme, Tooltip, TooltipPlacement, use_tooltip_ref,
};
use rfgui_components::Accordion;

#[component]
pub fn TooltipSection(theme: Theme) -> RsxNode {
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
        <Accordion title="Tooltip">
            <Text style={{ color: theme.color.text.secondary.clone() }}>Button with tooltip (placement)</Text>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(rfgui::Align::Center),
                gap: theme.spacing.lg,
                padding: rfgui::Padding::uniform(theme.spacing.md),
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

            <Text style={{ color: theme.color.text.secondary.clone() }}>Start / End variants</Text>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(rfgui::Align::Center),
                gap: theme.spacing.lg,
                padding: rfgui::Padding::uniform(theme.spacing.md),
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

            <Text style={{ color: theme.color.text.secondary.clone() }}>
                Standalone Tooltip controlled via ref (rich content)
            </Text>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(rfgui::Align::Center),
                gap: theme.spacing.lg,
                padding: rfgui::Padding::uniform(theme.spacing.md),
            }}>
                <Element
                    style={{
                        padding: rfgui::Padding::uniform(theme.spacing.sm),
                        border: rfgui::Border::uniform(Length::px(0.5), theme.color.border.as_ref()),
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
                            layout: Layout::flow().row().no_wrap().align(rfgui::Align::Center),
                            gap: theme.spacing.xs,
                        }}>
                            <FavoriteIcon style={{ font_size: theme.typography.size.xs }} />
                            <Text>"Rich tooltip with icon"</Text>
                        </Element>
                    </Tooltip>
                </Element>
            </Element>
        </Accordion>
    }
}
