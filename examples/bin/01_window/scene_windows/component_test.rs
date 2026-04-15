use crate::rfgui::ui::{RsxNode, component, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui::{Angle, Layout, Length, Padding, Rotate, Transform};
use crate::rfgui_components::{
    Button, ButtonVariant, Checkbox, CloseIcon, NumberField, Select, Slider, Switch, Theme,
};
use rfgui::Repeat::Infinite;
use rfgui::{Animation, Animator, FillMode, Keyframe, ScrollDirection};
use rfgui_components::Accordion;

fn select_value_identity(item: &String, _: usize) -> String {
    item.clone()
}

fn select_label(item: &String, _: usize) -> String {
    item.clone()
}

#[component]
pub fn ComponentTest(theme: Theme) -> RsxNode {
    let options = (1..=1000)
        .map(|index| format!("Option {index}"))
        .collect::<Vec<String>>();

    let checked = use_state(|| false);
    let selected = use_state(|| String::from("Option A"));
    let slider = use_state(|| 25.0_f64);
    let switch_state = use_state(|| true);
    let count = use_state(|| 0);
    let count_for_increment = count.clone();
    let count_increment = move |_event: &mut crate::rfgui::ui::ClickEvent| {
        count_for_increment.update(|value| *value += 1)
    };

    let int_number = use_state(|| 0);
    let float_number = use_state(|| 0.0);

    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            height: Length::percent(100.0),
            layout: Layout::flow().column().no_wrap(),
            gap: theme.spacing.sm,
            padding: Padding::uniform(theme.spacing.md),
            color: theme.color.text.primary.clone(),
            font_size: theme.typography.size.sm,
            scroll_direction: ScrollDirection::Vertical,
        }}>
            <Accordion title="Button">
                <Element style={{
                    width: Length::percent(100.0),
                    layout: Layout::flow().row().wrap(),
                    gap: theme.spacing.sm,
                }}>
                    <Button
                        label="Contained"
                        variant={Some(ButtonVariant::Contained)}
                    />
                    <Button
                        label="Outlined"
                        variant={Some(ButtonVariant::Outlined)}
                    />
                    <Button
                        label="Text"
                        variant={Some(ButtonVariant::Text)}
                    />
                </Element>
                <Element style={{
                            width: Length::percent(100.0),
                            layout: Layout::flow().row().wrap(),
                            gap: theme.spacing.sm,
                        }}>
                    <Button
                        label="Contained"
                        variant={Some(ButtonVariant::Contained)}
                        disabled
                    />
                    <Button
                        label="Outlined"
                        variant={Some(ButtonVariant::Outlined)}
                        disabled
                    />
                    <Button
                        label="Text"
                        variant={Some(ButtonVariant::Text)}
                        disabled
                    />
                </Element>
                <Element style={{
                    width: Length::percent(100.0),
                    layout: Layout::flow().row(),
                    gap: theme.spacing.sm,
                }}>
                    <Button
                        label="Click Me"
                        variant={Some(ButtonVariant::Contained)}
                        on_click={count_increment.clone()}
                    />
                    <Button
                        label="Hold to Repeat"
                        variant={Some(ButtonVariant::Contained)}
                        repeat
                        on_click={count_increment.clone()}
                    />
                    <Text>{format!("Count: {}", count.get())}</Text>
                </Element>
            </Accordion>
            <Accordion title="Number Field">
                <NumberField::<i32>
                    binding={int_number.binding()}
                    min=0
                    max=100
                    step=1
                    label="I32 Number"
                />
                <NumberField::<f64>
                    binding={float_number.binding()}
                    min=0.0
                    max=100.0
                    step=0.1
                    label="F32 Number"
                />
            </Accordion>
            <Accordion title="Material Symbols">
                <Element style={{
                    width: Length::percent(100.0),
                    layout: Layout::flow().row().wrap(),
                    gap: theme.spacing.md,
                    color: theme.color.text.primary.clone(),
                }}>
                    <Element style={{
                        layout: Layout::flow().column().no_wrap(),
                        gap: theme.spacing.xs,
                    }}>
                        <Text>Default</Text>
                        <CloseIcon />
                    </Element>
                    <Element style={{
                        layout: Layout::flow().column().no_wrap(),
                        gap: theme.spacing.xs,
                    }}>
                        <Text>Colored</Text>
                        <CloseIcon style={{
                            color: theme.color.secondary.base.clone(),
                            font_size: theme.typography.size.xl,
                        }} />
                    </Element>
                    <Element style={{
                        layout: Layout::flow().column().no_wrap(),
                        gap: theme.spacing.xs,
                    }}>
                        <Text>Rotating</Text>
                        <CloseIcon style={{
                            color: theme.color.primary.base.clone(),
                            animator: Animator::new([
                                Animation::new([
                                    Keyframe::new(0.0, rfgui::style! {
                                        transform: Transform::new([Rotate::z(Angle::deg(0.0))]),
                                    }),
                                    Keyframe::new(1.0, rfgui::style! {
                                        transform: Transform::new([Rotate::z(Angle::deg(360.0))]),
                                    }),
                                ]),
                            ]).fill_mode(FillMode::Forwards)
                            .repeat(Infinite)
                            .duration(theme.motion.duration.slow),
                        }} />
                    </Element>
                </Element>
            </Accordion>
            <Checkbox
                label="Enable flag"
                binding={checked.binding()}
            />
            <Switch
                label="Switch state"
                binding={switch_state.binding()}
            />

            <Select::<String, String>
                data={options}
                to_label={select_label as fn(&String, usize) -> String}
                to_value={select_value_identity as fn(&String, usize) -> String}
                value={selected.binding()}
            />
            <Slider
                binding={slider.binding()}
                min=0.0
                max=100.0
                label="Slider"
            />
            <Text>
                {format!(
                    "checked={} selected={} slider={:.0} switch={}",
                    checked.get(),
                    selected.get(),
                    slider.get(),
                    switch_state.get()
                )}
            </Text>
        </Element>
    }
}
