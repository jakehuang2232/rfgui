use crate::rfgui::ui::{RsxNode, component, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui::{Angle, Layout, Length, Padding, Rotate, Transform};
use crate::rfgui_components::{
    Button, ButtonColor, ButtonSize, ButtonVariant, Checkbox, CloseIcon, DeleteIcon, EditIcon,
    FavoriteIcon, FormatAlignCenterIcon, FormatAlignLeftIcon, FormatAlignRightIcon, FormatBoldIcon,
    FormatItalicIcon, FormatUnderlinedIcon, IconButton, NumberField, SaveIcon, Select, SendIcon,
    Slider, Switch, Theme, ToggleButton, ToggleButtonGroup, TreeNode, TreeView,
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

    let tree_expanded = use_state(|| vec![String::from("src"), String::from("layout")]);
    let tree_selected = use_state(|| Some(String::from("tree_view.rs")));
    let folder = |id: &str, label: &str, children: Vec<TreeNode>| {
        TreeNode::new(id, label)
            .with_icon("folder")
            .with_expanded_icon("folder_open")
            .with_children(children)
    };
    let file = |id: &str, label: &str, icon: &str| {
        TreeNode::new(id, label).with_icon(icon)
    };
    let tree_nodes = vec![
        folder(
            "src",
            "src/",
            vec![
                folder(
                    "inputs",
                    "inputs/",
                    vec![
                        file("button.rs", "button.rs", "code"),
                        file("checkbox.rs", "checkbox.rs", "code"),
                        file("select.rs", "select.rs", "code"),
                    ],
                ),
                folder(
                    "layout",
                    "layout/",
                    vec![
                        file("accordion.rs", "accordion.rs", "code"),
                        file("tree_view.rs", "tree_view.rs", "code"),
                        file("window.rs", "window.rs", "code"),
                    ],
                ),
                file("lib.rs", "lib.rs", "code"),
                file("theme.rs", "theme.rs", "palette"),
            ],
        ),
        folder(
            "examples",
            "examples/",
            vec![
                file("readme", "README.md", "description").with_disabled(true),
            ],
        ),
    ];

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
            <Accordion title="Tree View">
                <TreeView
                    nodes={tree_nodes}
                    expanded_binding={tree_expanded.binding()}
                    selected_binding={tree_selected.binding()}
                />
                <Text style={{ color: theme.color.text.secondary.clone() }}>
                    {format!(
                        "expanded={:?} selected={:?}",
                        tree_expanded.get(),
                        tree_selected.get()
                    )}
                </Text>
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
