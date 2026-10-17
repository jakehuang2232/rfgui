use crate::rfgui::style::{
    Border, Layout, Length, Padding, ScrollDirection, Transition, TransitionProperty, flex,
};
use crate::rfgui::ui::{GlobalKey, RsxNode, component, on_click, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::{Button, ButtonVariant, use_theme};
use crate::showcase::Demo;

/// A card with state of every kind: a count, a scroll offset and a
/// running transition.
#[component]
fn MovableCard(title: String) -> RsxNode {
    let count = use_state(|| 0_i32);
    let transition_alt = use_state(|| false);
    let theme = use_theme().0;
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
                background: theme.color.background.base.clone(),
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

                    variant={Some(ButtonVariant::Outlined)}
                    on_click={increment}
    >Count +1</Button>
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
                    <Text>Scroll here, then move the card.</Text>
                    {scroll_lines.iter().enumerate().map(|(index, line)| rsx! {
                        <Text key={index}>{line.clone()}</Text>
                    }).collect::<Vec<RsxNode>>()}
                </Element>
                <Element style={{
                    width: Length::percent(100.0),
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                }}>
                    <Text>Start the ten second transition, then move the card.</Text>
                    <Button
                        variant={Some(ButtonVariant::Text)}
                        on_click={toggle_transition}
                    >{if transition_alt.get() { "Transition -> base" } else { "Transition -> alt" }}</Button>
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
pub fn GlobalKeyMove() -> RsxNode {
    let move_to_right = use_state(|| false);
    let theme = use_theme().0;
    let global_key = GlobalKey::from("showcase-movable-card");
    let toggle_side = {
        let move_to_right = move_to_right.clone();
        on_click(move |_| {
            move_to_right.update(|value| *value = !*value);
        })
    };

    let card_node = rsx! {
        <MovableCard
            key={global_key}
            title="A card with a global key"
        />
    };

    rsx! {
        <Demo
            title="State that moves with a component"
            description="Component state lives with the component, not its place in the tree. With a GlobalKey, a component keeps it when it moves to another parent: count, scroll and start the transition, then move the card."
        >
            <Button
                variant={Some(ButtonVariant::Contained)}
                on_click={toggle_side}
            >{if move_to_right.get() { "Move card to left column" } else { "Move card to right column" }}</Button>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flex().row(),
                gap: theme.spacing.md,
            }}>
                <Element style={{
                    flex: flex().grow(1.0).shrink(1.0),
                    min_width: Length::Zero,
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
                    flex: flex().grow(1.0).shrink(1.0),
                    min_width: Length::Zero,
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
        </Demo>
    }
}
