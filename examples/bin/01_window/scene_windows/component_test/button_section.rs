use crate::rfgui::ui::{RsxNode, component, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui::{Layout, Length};
use crate::rfgui_components::{
    Button, ButtonColor, ButtonSize, ButtonVariant, DeleteIcon, EditIcon, FavoriteIcon,
    FormatAlignCenterIcon, FormatAlignLeftIcon, FormatAlignRightIcon, FormatBoldIcon,
    FormatItalicIcon, FormatUnderlinedIcon, IconButton, SaveIcon, SendIcon, Theme, ToggleButton,
    ToggleButtonGroup,
};
use rfgui_components::Accordion;

#[component]
pub fn ButtonSection(theme: Theme) -> RsxNode {
    let count = use_state(|| 0);
    let count_for_increment = count.clone();
    let count_increment = move |_event: &mut crate::rfgui::ui::ClickEvent| {
        count_for_increment.update(|value| *value += 1)
    };

    let bold = use_state(|| false);
    let italic = use_state(|| true);
    let underline = use_state(|| false);
    let favorite = use_state(|| false);
    let align = use_state(|| Some(String::from("center")));

    let bold_toggle = {
        let bold = bold.clone();
        move |_: &mut crate::rfgui::ui::ClickEvent| bold.update(|v| *v = !*v)
    };
    let italic_toggle = {
        let italic = italic.clone();
        move |_: &mut crate::rfgui::ui::ClickEvent| italic.update(|v| *v = !*v)
    };
    let underline_toggle = {
        let underline = underline.clone();
        move |_: &mut crate::rfgui::ui::ClickEvent| underline.update(|v| *v = !*v)
    };
    let favorite_toggle = {
        let favorite = favorite.clone();
        move |_: &mut crate::rfgui::ui::ClickEvent| favorite.update(|v| *v = !*v)
    };

    rsx! {
        <Accordion title="Button">
            <Text style={{ color: theme.color.text.secondary.clone() }}>Variant</Text>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.sm,
            }}>
                <Button variant={Some(ButtonVariant::Contained)}>Contained</Button>
                <Button variant={Some(ButtonVariant::Outlined)}>Outlined</Button>
                <Button variant={Some(ButtonVariant::Text)}>Text</Button>
                <Button variant={Some(ButtonVariant::Contained)} disabled>Disabled</Button>
            </Element>

            <Text style={{ color: theme.color.text.secondary.clone() }}>Size</Text>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(rfgui::Align::Center),
                gap: theme.spacing.sm,
            }}>
                <Button size={Some(ButtonSize::Small)}>Small</Button>
                <Button size={Some(ButtonSize::Medium)}>Medium</Button>
                <Button size={Some(ButtonSize::Large)}>Large</Button>
            </Element>

            <Text style={{ color: theme.color.text.secondary.clone() }}>Color</Text>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.sm,
            }}>
                <Button color={Some(ButtonColor::Primary)}>Primary</Button>
                <Button color={Some(ButtonColor::Secondary)}>Secondary</Button>
                <Button color={Some(ButtonColor::Success)}>Success</Button>
                <Button color={Some(ButtonColor::Info)}>Info</Button>
                <Button color={Some(ButtonColor::Warning)}>Warning</Button>
                <Button color={Some(ButtonColor::Error)}>Error</Button>
            </Element>

            <Text style={{ color: theme.color.text.secondary.clone() }}>With icon</Text>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.sm,
            }}>
                <Button
                    start_icon={rsx! { <DeleteIcon style={{ font_size: theme.typography.size.sm }} /> }}
                    color={Some(ButtonColor::Error)}
                >Delete</Button>
                <Button
                    end_icon={rsx! { <SendIcon style={{ font_size: theme.typography.size.sm }} /> }}
                >Send</Button>
                <Button
                    variant={Some(ButtonVariant::Outlined)}
                    start_icon={rsx! { <SaveIcon style={{ font_size: theme.typography.size.sm }} /> }}
                    color={Some(ButtonColor::Success)}
                >Save</Button>
            </Element>

            <Text style={{ color: theme.color.text.secondary.clone() }}>Repeat / Full width</Text>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(rfgui::Align::Center),
                gap: theme.spacing.sm,
            }}>
                <Button repeat on_click={count_increment.clone()}>Hold to Repeat</Button>
                <Text>{format!("Count: {}", count.get())}</Text>
            </Element>
            <Button
                variant={Some(ButtonVariant::Contained)}
                full_width
                on_click={count_increment.clone()}
            >Full Width</Button>

            <Text style={{ color: theme.color.text.secondary.clone() }}>IconButton</Text>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(rfgui::Align::Center),
                gap: theme.spacing.sm,
            }}>
                <IconButton size={Some(ButtonSize::Small)}><EditIcon /></IconButton>
                <IconButton size={Some(ButtonSize::Medium)}><EditIcon /></IconButton>
                <IconButton size={Some(ButtonSize::Large)}><EditIcon /></IconButton>
                <IconButton color={Some(ButtonColor::Primary)}><FavoriteIcon /></IconButton>
                <IconButton color={Some(ButtonColor::Error)}><FavoriteIcon /></IconButton>
                <IconButton disabled><FavoriteIcon /></IconButton>
            </Element>

            <Text style={{ color: theme.color.text.secondary.clone() }}>ToggleButton</Text>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(rfgui::Align::Center),
                gap: theme.spacing.sm,
            }}>
                <ToggleButton
                    value="bold"
                    selected={bold.get()}
                    on_click={bold_toggle}
                ><FormatBoldIcon /></ToggleButton>
                <ToggleButton
                    value="italic"
                    selected={italic.get()}
                    on_click={italic_toggle}
                ><FormatItalicIcon /></ToggleButton>
                <ToggleButton
                    value="underline"
                    selected={underline.get()}
                    on_click={underline_toggle}
                ><FormatUnderlinedIcon /></ToggleButton>
                <ToggleButton
                    value="favorite"
                    selected={favorite.get()}
                    color={Some(ButtonColor::Error)}
                    on_click={favorite_toggle}
                >
                    <FavoriteIcon />
                    <Text>Favorite</Text>
                </ToggleButton>
                <ToggleButton value="disabled" disabled>Disabled</ToggleButton>
            </Element>
            <Text style={{ color: theme.color.text.secondary.clone() }}>
                {format!(
                    "bold={} italic={} underline={} favorite={}",
                    bold.get(), italic.get(), underline.get(), favorite.get()
                )}
            </Text>

            <Text style={{ color: theme.color.text.secondary.clone() }}>ToggleButtonGroup (exclusive, via context)</Text>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.md,
                align: rfgui::Align::Start,
            }}>
                <ToggleButtonGroup value={align.binding()}>
                    <ToggleButton value="left"><FormatAlignLeftIcon /></ToggleButton>
                    <ToggleButton value="center"><FormatAlignCenterIcon /></ToggleButton>
                    <ToggleButton value="right"><FormatAlignRightIcon /></ToggleButton>
                </ToggleButtonGroup>
                <ToggleButtonGroup
                    value={align.binding()}
                    orientation="vertical"
                >
                    <ToggleButton value="left"><FormatAlignLeftIcon /></ToggleButton>
                    <ToggleButton value="center"><FormatAlignCenterIcon /></ToggleButton>
                    <ToggleButton value="right"><FormatAlignRightIcon /></ToggleButton>
                </ToggleButtonGroup>
            </Element>
            <Text style={{ color: theme.color.text.secondary.clone() }}>
                {format!("align = {:?}", align.get())}
            </Text>
        </Accordion>
    }
}
