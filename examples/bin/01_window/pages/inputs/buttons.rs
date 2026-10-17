use crate::rfgui::style::{Layout, Length};
use crate::rfgui::ui::{RsxNode, component, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::material_symbol::{
    DeleteIcon, EditIcon, FavoriteIcon, SaveIcon, SendIcon,
};
use crate::rfgui_components::{
    Button, ButtonColor, ButtonSize, ButtonVariant, IconButton, use_theme,
};
use crate::showcase::{Caption, Demo};

#[component]
pub fn Buttons() -> RsxNode {
    let (theme, _) = use_theme();
    let count = use_state(|| 0);
    let count_for_increment = count.clone();
    let count_increment = move |_event: &mut crate::rfgui::ui::ClickEvent| {
        count_for_increment.update(|value| *value += 1)
    };

    rsx! {
        <Demo
            title="Button and IconButton"
            description="Three variants, three sizes and six colors, with icons at either end. A repeat button fires while held."
        >
            <Caption text="Variant" />
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

            <Caption text="Size" />
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(rfgui::style::Align::Center),
                gap: theme.spacing.sm,
            }}>
                <Button size={Some(ButtonSize::Small)}>Small</Button>
                <Button size={Some(ButtonSize::Medium)}>Medium</Button>
                <Button size={Some(ButtonSize::Large)}>Large</Button>
            </Element>

            <Caption text="Color" />
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

            <Caption text="With icon" />
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

            <Caption text="Repeat and full width" />
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(rfgui::style::Align::Center),
                gap: theme.spacing.sm,
            }}>
                <Button repeat on_click={count_increment.clone()}>Hold to Repeat</Button>
                <Text>{format!("Count: {}", count.get())}</Text>
            </Element>
            <Button
                variant={Some(ButtonVariant::Contained)}
                full_width
            >Full Width</Button>

            <Caption text="IconButton" />
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(rfgui::style::Align::Center),
                gap: theme.spacing.sm,
            }}>
                <IconButton size={Some(ButtonSize::Small)}><EditIcon /></IconButton>
                <IconButton size={Some(ButtonSize::Medium)}><EditIcon /></IconButton>
                <IconButton size={Some(ButtonSize::Large)}><EditIcon /></IconButton>
                <IconButton color={Some(ButtonColor::Primary)}><FavoriteIcon /></IconButton>
                <IconButton color={Some(ButtonColor::Error)}><FavoriteIcon /></IconButton>
                <IconButton disabled><FavoriteIcon /></IconButton>
            </Element>
        </Demo>
    }
}
