use crate::rfgui::style::{Align, CrossSize, JustifyContent, Layout, Length, Padding};
use crate::rfgui::ui::{ClickEvent, RsxNode, component, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::material_symbol::{
    AlignHorizontalCenterIcon, AlignHorizontalLeftIcon, AlignHorizontalRightIcon, HeightIcon,
    OpenInFullIcon, SpaceBarIcon, UnfoldLessIcon, UnfoldMoreIcon, VerticalAlignCenterIcon,
    VerticalAlignTopIcon, ViewColumnIcon,
};
use crate::rfgui_components::{
    ButtonSize, ToggleButton, ToggleButtonGroup, ToggleGroupChangeHandler, use_theme,
};
use crate::showcase::Demo;
use std::rc::Rc;

/// Box sizes; the ones without a height take their content's, so cross
/// size stretch can grow them.
const BOXES: [(f32, Option<f32>); 5] = [
    (64.0, Some(40.0)),
    (96.0, None),
    (48.0, Some(104.0)),
    (120.0, None),
    (80.0, Some(64.0)),
];

#[component]
pub fn FlowAlignment() -> RsxNode {
    let (theme, _) = use_theme();
    let justify_content = use_state(|| JustifyContent::Start);
    let align = use_state(|| Align::Start);
    let cross_size = use_state(|| CrossSize::Fit);
    let justify_content_group = use_state(|| Some(String::from("start")));
    let align_group = use_state(|| Some(String::from("start")));
    let cross_size_group = use_state(|| Some(String::from("fit")));

    let justify_content_change = {
        let justify_content = justify_content.binding();
        let justify_content_group = justify_content_group.binding();
        Rc::new(move |_: &mut ClickEvent, value: Option<String>| {
            let selected = value.unwrap_or_else(|| match justify_content.get() {
                JustifyContent::Start => String::from("start"),
                JustifyContent::Center => String::from("center"),
                JustifyContent::End => String::from("end"),
                JustifyContent::SpaceBetween => String::from("space-between"),
                JustifyContent::SpaceAround => String::from("space-around"),
                JustifyContent::SpaceEvenly => String::from("space-evenly"),
            });
            let next = match selected.as_str() {
                "center" => JustifyContent::Center,
                "end" => JustifyContent::End,
                "space-between" => JustifyContent::SpaceBetween,
                "space-around" => JustifyContent::SpaceAround,
                "space-evenly" => JustifyContent::SpaceEvenly,
                _ => JustifyContent::Start,
            };
            justify_content.set(next);
            justify_content_group.set(Some(selected));
        }) as ToggleGroupChangeHandler
    };
    let align_change = {
        let align = align.binding();
        let align_group = align_group.binding();
        Rc::new(move |_: &mut ClickEvent, value: Option<String>| {
            let selected = value.unwrap_or_else(|| match align.get() {
                Align::Center => String::from("center"),
                Align::End => String::from("end"),
                _ => String::from("start"),
            });
            let next = match selected.as_str() {
                "center" => Align::Center,
                "end" => Align::End,
                _ => Align::Start,
            };
            align.set(next);
            align_group.set(Some(selected));
        }) as ToggleGroupChangeHandler
    };
    let cross_size_change = {
        let cross_size = cross_size.binding();
        let cross_size_group = cross_size_group.binding();
        Rc::new(move |_: &mut ClickEvent, value: Option<String>| {
            let selected = value.unwrap_or_else(|| match cross_size.get() {
                CrossSize::Stretch => String::from("stretch"),
                _ => String::from("fit"),
            });
            let next = match selected.as_str() {
                "stretch" => CrossSize::Stretch,
                _ => CrossSize::Fit,
            };
            cross_size.set(next);
            cross_size_group.set(Some(selected));
        }) as ToggleGroupChangeHandler
    };

    let boxes = BOXES
        .iter()
        .enumerate()
        .map(|(index, &(width, height))| {
            rsx! {
                <Element key={index} style={{
                    width: Length::px(width),
                    height: height.map(Length::px),
                    padding: Padding::uniform(theme.spacing.sm),
                    background: theme.color.primary.base.clone(),
                    color: theme.color.primary.on.clone(),
                    border_radius: theme.radius.sm,
                }}>
                    <Text>{format!("{}", index + 1)}</Text>
                </Element>
            }
        })
        .collect::<Vec<_>>();

    rsx! {
        <Demo
            title="Flow alignment"
            description="A flow row places its children in order and wraps them onto new lines. Justify content spreads them along a line; cross align and cross size place them across it."
        >
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(Align::Center),
                gap: theme.spacing.sm,
            }}>
                <Text>Justify content</Text>
                <ToggleButtonGroup
                    value={justify_content_group.binding()}
                    on_change={Some(justify_content_change)}
                    size={Some(ButtonSize::Small)}
                >
                    <ToggleButton value="start"><AlignHorizontalLeftIcon />Start</ToggleButton>
                    <ToggleButton value="center"><AlignHorizontalCenterIcon />Center</ToggleButton>
                    <ToggleButton value="end"><AlignHorizontalRightIcon />End</ToggleButton>
                    <ToggleButton value="space-between"><ViewColumnIcon />SpaceBetween</ToggleButton>
                    <ToggleButton value="space-around"><SpaceBarIcon />SpaceAround</ToggleButton>
                    <ToggleButton value="space-evenly"><UnfoldMoreIcon />SpaceEvenly</ToggleButton>
                </ToggleButtonGroup>
            </Element>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(Align::Center),
                gap: theme.spacing.sm,
            }}>
                <Text>Cross align</Text>
                <ToggleButtonGroup
                    value={align_group.binding()}
                    on_change={Some(align_change)}
                    size={Some(ButtonSize::Small)}
                >
                    <ToggleButton value="start"><VerticalAlignTopIcon />Start</ToggleButton>
                    <ToggleButton value="center"><VerticalAlignCenterIcon />Center</ToggleButton>
                    <ToggleButton value="end"><UnfoldLessIcon />End</ToggleButton>
                </ToggleButtonGroup>
                <Text>Cross size</Text>
                <ToggleButtonGroup
                    value={cross_size_group.binding()}
                    on_change={Some(cross_size_change)}
                    size={Some(ButtonSize::Small)}
                >
                    <ToggleButton value="fit"><HeightIcon />Fit</ToggleButton>
                    <ToggleButton value="stretch"><OpenInFullIcon />Stretch</ToggleButton>
                </ToggleButtonGroup>
            </Element>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow()
                    .row()
                    .wrap()
                    .justify_content(justify_content.get())
                    .align(align.get())
                    .cross_size(cross_size.get()),
                gap: theme.spacing.sm,
                padding: Padding::uniform(theme.spacing.md),
                background: theme.color.layer.raised.clone(),
                border_radius: theme.radius.md,
            }}>
                {boxes}
            </Element>
        </Demo>
    }
}
