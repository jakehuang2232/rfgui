use crate::rfgui::style::{Align, Layout, Length};
use crate::rfgui::ui::{ClickEvent, RsxNode, component, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::material_symbol::{
    FavoriteIcon, FormatAlignCenterIcon, FormatAlignLeftIcon, FormatAlignRightIcon, FormatBoldIcon,
    FormatItalicIcon, FormatUnderlinedIcon,
};
use crate::rfgui_components::{ButtonColor, ToggleButton, ToggleButtonGroup, use_theme};
use crate::showcase::{Caption, Demo};

#[component]
pub fn ToggleButtons() -> RsxNode {
    let (theme, _) = use_theme();
    let bold = use_state(|| false);
    let italic = use_state(|| true);
    let underline = use_state(|| false);
    let favorite = use_state(|| false);
    let align = use_state(|| Some(String::from("center")));

    let bold_toggle = {
        let bold = bold.clone();
        move |_: &mut ClickEvent| bold.update(|v| *v = !*v)
    };
    let italic_toggle = {
        let italic = italic.clone();
        move |_: &mut ClickEvent| italic.update(|v| *v = !*v)
    };
    let underline_toggle = {
        let underline = underline.clone();
        move |_: &mut ClickEvent| underline.update(|v| *v = !*v)
    };
    let favorite_toggle = {
        let favorite = favorite.clone();
        move |_: &mut ClickEvent| favorite.update(|v| *v = !*v)
    };

    rsx! {
        <Demo
            title="ToggleButton and ToggleButtonGroup"
            description="Toggle buttons hold their own selection. A group makes them exclusive, sharing its value with its buttons through context, in either orientation."
        >
            <Caption text="Independent toggles" />
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(Align::Center),
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
            <Caption text={format!(
                "bold={} italic={} underline={} favorite={}",
                bold.get(), italic.get(), underline.get(), favorite.get()
            )} />
            <Caption text="An exclusive group, horizontal and vertical" />
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.md,
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
            <Caption text={format!("align = {:?}", align.get())} />
        </Demo>
    }
}
