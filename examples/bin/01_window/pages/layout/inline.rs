use crate::rfgui::style::{
    Border, BorderRadius, Color, ColorLike, Layout, Length, Padding, VerticalAlign,
};
use crate::rfgui::ui::{RsxNode, component, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::{
    ButtonSize, NumberField, ToggleButton, ToggleButtonGroup, ToggleGroupChangeHandler, use_theme,
};
use crate::showcase::{Caption, Demo};
use rfgui::style::Align::Center;
use std::rc::Rc;

fn inline_chip(
    label: impl Into<String>,
    width: Length,
    height: Length,
    background: Box<dyn ColorLike>,
    text_color: Box<dyn ColorLike>,
    radius: Length,
) -> RsxNode {
    rsx! {
        <Element style={{
            width: width,
            height: height,
            background: background,
            color: text_color,
            border_radius: BorderRadius::uniform(radius),
            padding: Padding::uniform(Length::px(8.0)),
        }}>
            <Text>{label.into()}</Text>
        </Element>
    }
}

#[component]
pub fn InlineFormatting() -> RsxNode {
    let (theme, _) = use_theme();
    let vertical_align = use_state(|| VerticalAlign::Baseline);
    let vertical_align_group = use_state(|| Some(String::from("baseline")));
    let line_height = use_state(|| 1.8_f64);

    let vertical_align_change = {
        let vertical_align = vertical_align.binding();
        let vertical_align_group = vertical_align_group.binding();
        Rc::new(
            move |_: &mut crate::rfgui::ui::ClickEvent, value: Option<String>| {
                let selected = value.unwrap_or_else(|| match vertical_align.get() {
                    VerticalAlign::Top => String::from("top"),
                    VerticalAlign::Middle => String::from("middle"),
                    VerticalAlign::Bottom => String::from("bottom"),
                    _ => String::from("baseline"),
                });
                let next = match selected.as_str() {
                    "top" => VerticalAlign::Top,
                    "middle" => VerticalAlign::Middle,
                    "bottom" => VerticalAlign::Bottom,
                    _ => VerticalAlign::Baseline,
                };
                vertical_align.set(next);
                vertical_align_group.set(Some(selected));
            },
        ) as ToggleGroupChangeHandler
    };

    let va = vertical_align.get();
    let lh = line_height.get();

    rsx! {
        <Demo
            title="Inline formatting"
            description="Layout::Inline, the default, runs text and elements together in lines that wrap at the container width, as text does on a web page. An inline element keeps its padding on the line of the text it surrounds."
        >
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(Center),
                gap: theme.spacing.md,
            }}>
                <Element style={{
                    layout: Layout::flow().row().align(Center),
                    gap: theme.spacing.sm,
                }}>
                    <Text>vertical-align</Text>
                    <ToggleButtonGroup
                        value={vertical_align_group.binding()}
                        on_change={Some(vertical_align_change)}
                        size={Some(ButtonSize::Small)}
                    >
                        <ToggleButton value="baseline">Baseline</ToggleButton>
                        <ToggleButton value="top">Top</ToggleButton>
                        <ToggleButton value="middle">Middle</ToggleButton>
                        <ToggleButton value="bottom">Bottom</ToggleButton>
                    </ToggleButtonGroup>
                </Element>
                <Element style={{ width: Length::px(220.0) }}>
                    <NumberField
                        binding={line_height.binding()}
                        min={0.5_f64}
                        max={3.0_f64}
                        step={0.1_f64}
                        label={"line-height".to_string()}
                    />
                </Element>
            </Element>
            <Caption text="Text and elements in one line box 😀" />
            <Element style={{
                width: Length::percent(100.0),
                background: theme.color.layer.raised.clone(),
                border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
                border_radius: theme.radius.md,
                padding: Padding::uniform(theme.spacing.sm),
                layout: Layout::Inline,
                gap: theme.spacing.xs,
                line_height: lh,
                vertical_align: va,
            }}>
                Inline text starts here,
                <Element style={{
                    background: theme.color.primary.base.clone(),
                    color: theme.color.primary.on.clone(),
                    border_radius: BorderRadius::uniform(theme.radius.sm),
                    padding: Padding::uniform(Length::px(8.0)),
                }}>
                    a badge that wraps with the words around it
                </Element>
                <Text>then more text continues after the badge,</Text>
                {inline_chip("fixed size", Length::px(90.0), Length::px(50.0), Box::new(Color::hex("#065f46")), Box::new(Color::hex("#ecfdf5")), theme.radius.sm)}
                <Text>{"and the line ends in Chinese, which breaks between characters: 最後接一段中文，確認混排時也能一起換行。"}</Text>
            </Element>
            <Caption text="Inline elements nested in inline elements" />
            <Element style={{
                width: Length::percent(100.0),
                background: theme.color.layer.raised.clone(),
                border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
                border_radius: theme.radius.md,
                padding: Padding::uniform(theme.spacing.sm),
                layout: Layout::Inline,
                gap: theme.spacing.xs,
                line_height: lh,
                vertical_align: va,
            }}>
                <Element style={{
                    background: theme.color.secondary.base.clone(),
                    color: theme.color.primary.on.clone(),
                    border_radius: BorderRadius::uniform(theme.radius.sm),
                    padding: Padding::uniform(Length::px(8.0)),
                }}>
                    Permission is hereby granted, free of charge, to any
                    person obtaining a copy of this software and associated
                    documentation files (the Software), to deal in the
                    Software without
                    <Element style={{
                        background: theme.color.primary.base.clone(),
                        color: theme.color.primary.on.clone(),
                        border_radius: BorderRadius::uniform(theme.radius.sm),
                        padding: Padding::uniform(Length::px(8.0)),
                    }}>
                        restriction, including without
                        limitation the rights to use, copy, modify, merge,
                        publish, distribute, sublicense,
                    </Element>
                    and/or sell copies of
                    the Software, and to permit persons to whom the Software
                    is furnished to do so, subject to the following
                    conditions
                </Element>
            </Element>
        </Demo>
    }
}
