use crate::rfgui::view::{Element, Text};
use crate::rfgui::ui::{GlobalKey, RsxNode, component, on_click, rsx, use_state};
use crate::rfgui::{
    Border, Layout, Length, Padding, ScrollDirection, Transition, TransitionProperty,
};
use crate::rfgui_components::{Button, ButtonVariant, use_theme};

#[component]
pub fn GlobalKeyCounterCard(title: String) -> RsxNode {
    let count = use_state(|| 0_i32);
    let transition_alt = use_state(|| false);
    let theme = use_theme().get();
    let increment = {
        let count = count.clone();
        on_click(move |_| {
            count.update(|value| *value += 1);
        })
    };
    let toggle_transition = {
        let transition_alt = transition_alt.clone();
        on_click(move |_| {
            transition_alt.update(|value| *value = !*value);
        })
    };
    let scroll_lines = (1..=20)
        .map(|index| format!("Scrollable line {index}"))
        .collect::<Vec<_>>();

    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            background: theme.color.layer.raised.clone(),
            border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
            border_radius: theme.radius.md,
            padding: Padding::uniform(theme.spacing.sm),
            layout: Layout::flow().column().no_wrap(),
            gap: theme.spacing.xs,
            color: theme.color.text.primary.clone(),
        }}>
            <Text>{title}</Text>
            <Text>{format!("count={}", count.get())}</Text>
            <Button
                label="Count +1"
                variant={Some(ButtonVariant::Outlined)}
                on_click={increment}
            />
            <Element style={{
                width: Length::percent(100.0),
                height: Length::px(120.0),
                background: theme.color.layer.surface.clone(),
                border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
                border_radius: theme.radius.sm,
                padding: Padding::uniform(theme.spacing.xs),
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.xs,
                scroll_direction: ScrollDirection::Vertical,
                color: theme.color.text.primary.clone(),
                font_size: theme.typography.size.sm,
            }}>
                <Text>Scroll test: scroll here, then move the card.</Text>
                {scroll_lines.iter().enumerate().map(|(index, line)| rsx! {
                    <Text key={index}>{line.clone()}</Text>
                }).collect::<Vec<RsxNode>>()}
            </Element>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.xs,
            }}>
                <Text>Transition test: toggle, then move the card.</Text>
                <Button
                    label={if transition_alt.get() { "Transition -> base" } else { "Transition -> alt" }}
                    variant={Some(ButtonVariant::Text)}
                    on_click={toggle_transition}
                />
                <Element style={{
                    width: if transition_alt.get() {
                        Length::percent(100.0)
                    } else {
                        Length::px(88.0)
                    },
                    height: Length::px(40.0),
                    background: if transition_alt.get() {
                        theme.color.secondary.base.clone()
                    } else {
                        theme.color.primary.base.clone()
                    },
                    color: if transition_alt.get() {
                        theme.color.secondary.on.clone()
                    } else {
                        theme.color.primary.on.clone()
                    },
                    border_radius: theme.radius.sm,
                    padding: Padding::uniform(theme.spacing.sm),
                    transition: [
                        Transition::new(TransitionProperty::All, 10000)
                            .ease_in_out(),
                    ],
                }}>
                    <Text>{if transition_alt.get() { "Alt state" } else { "Base state" }}</Text>
                </Element>
            </Element>
        </Element>
    }
}

#[component]
pub fn GlobalKeyRenderTestBlock() -> RsxNode {
    let move_to_right = use_state(|| false);
    let theme = use_theme().get();
    let global_key = GlobalKey::from("example-01-window-render-test-global-key-card");
    let toggle_side = {
        let move_to_right = move_to_right.clone();
        on_click(move |_| {
            move_to_right.update(|value| *value = !*value);
        })
    };

    let card_node = rsx! {
        <GlobalKeyCounterCard
            key={global_key}
            title="GlobalKey movable card"
        />
    };

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
            <Text>GlobalKey render test</Text>
            <Text>
                Interact with the counter, scroll area, and transition block, then move the card
                left/right. Their state should stay the same.
            </Text>
            <Button
                label={if move_to_right.get() { "Move card to left column" } else { "Move card to right column" }}
                variant={Some(ButtonVariant::Contained)}
                on_click={toggle_side}
            />
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().no_wrap(),
                gap: theme.spacing.md,
            }}>
                <Element style={{
                    width: Length::percent(50.0),
                    min_height: Length::px(160.0),
                    background: theme.color.layer.raised.clone(),
                    border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
                    border_radius: theme.radius.md,
                    padding: Padding::uniform(theme.spacing.sm),
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.sm,
                }}>
                    <Text>Left column</Text>
                    {if !move_to_right.get() {
                        card_node.clone()
                    } else {
                        RsxNode::fragment(vec![])
                    }}
                </Element>
                <Element style={{
                    width: Length::percent(50.0),
                    min_height: Length::px(160.0),
                    background: theme.color.layer.raised.clone(),
                    border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
                    border_radius: theme.radius.md,
                    padding: Padding::uniform(theme.spacing.sm),
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.sm,
                }}>
                    <Text>Right column</Text>
                    {if move_to_right.get() {
                        card_node
                    } else {
                        RsxNode::fragment(vec![])
                    }}
                </Element>
            </Element>
        </Element>
    }
}
