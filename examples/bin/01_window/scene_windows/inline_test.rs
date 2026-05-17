use crate::rfgui::style::{
    Border, BorderRadius, Color, ColorLike, Layout, Length, Padding, VerticalAlign,
};
use crate::rfgui::ui::{RsxNode, component, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::{
    ButtonSize, NumberField, Theme, ToggleButton, ToggleButtonGroup, ToggleGroupChangeHandler,
};
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

fn demo_section(
    theme: &Theme,
    title: impl Into<String>,
    description: impl Into<String>,
    content: RsxNode,
) -> RsxNode {
    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            background: theme.color.layer.surface.clone(),
            border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
            border_radius: theme.radius.lg,
            padding: Padding::uniform(theme.spacing.md),
            layout: Layout::flow().column().no_wrap(),
            gap: theme.spacing.sm,
            color: theme.color.text.primary.clone(),
        }}>
            <Text>{title.into()}</Text>
            <Text>{description.into()}</Text>
            {content}
        </Element>
    }
}

#[component]
pub fn InlineTest(theme: Theme) -> RsxNode {
    let vertical_align = use_state(|| VerticalAlign::Baseline);
    let vertical_align_group = use_state(|| Some(String::from("baseline")));
    let line_height = use_state(|| 1.2_f64);

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
        <Element style={{
            width: Length::percent(100.0),
            layout: Layout::flow().column().no_wrap(),
            gap: theme.spacing.md,
            padding: Padding::uniform(theme.spacing.lg),
            color: theme.color.text.primary.clone(),
            font: theme.typography.font_family.clone(),
            font_size: theme.typography.size.sm,
            background: theme.color.background.base.clone(),
        }}>
            <Text>Inline Layout Test Window</Text>
            <Text>{"This window demonstrates Layout::Inline behavior: content participates in line boxes, wraps based on available width, and grows vertically as more lines are formed."}</Text>
            <Element style={{
                layout: Layout::flow().column().no_wrap(),
                width: Length::percent(100.0),
                gap: theme.spacing.sm,
                padding: Padding::uniform(theme.spacing.sm),
                background: theme.color.layer.surface.clone(),
                border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                border_radius: theme.radius.md,
            }}>
                <Element style={{
                    layout: Layout::flow().row().align(Center),
                    gap: theme.spacing.sm,
                }}>
                    <Text>vertical-align:</Text>
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
                <Element>
                    <NumberField
                        binding={line_height.binding()}
                        min={0.5_f64}
                        max={3.0_f64}
                        step={0.1_f64}
                        label={"line-height".to_string()}
                    />
                </Element>
            </Element>
            {demo_section(
                &theme,
                "Mixed Text / Element😀",
                "Text nodes and Elements participate in the same inline formatting context.",
                rsx! {
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
                            badge test test test test test test test
                        </Element>
                        <Text>then more text continues after the badge,</Text>
                        {inline_chip("note note note note note note note", Length::px(90.0), Length::px(50.0), Box::new(Color::hex("#065f46")), Box::new(Color::hex("#ecfdf5")), theme.radius.sm)}
                        <Text>{"最後接一段中文，確認混排時也能一起換行。"}</Text>
                    </Element>
                },
            )}
            {demo_section(
                &theme,
                "Inline Element Test",
                "Element children also participate in the same inline formatting context.",
                rsx! {
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
                            documentation files (the &quot;Software&quot;), to deal in the
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

                        <Element style={{
                            background: theme.color.primary.base.clone(),
                            color: theme.color.primary.on.clone(),
                            border_radius: BorderRadius::uniform(theme.radius.sm),
                            padding: Padding::uniform(Length::px(8.0)),
                        }}>
                            Permission is hereby granted, free of charge, to any
                            person obtaining a copy of this software and associated
                            documentation files (the &quot;Software&quot;), to deal in the
                            Software without restriction, including without
                            limitation the rights to use, copy, modify, merge,
                            publish, distribute, sublicense, and/or sell copies of
                            the Software, and to permit persons to whom the Software
                            is furnished to do so, subject to the following
                            conditions:
                        </Element>
                    </Element>
                },
            )}
        </Element>
    }
}
