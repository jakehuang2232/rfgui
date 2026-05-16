use crate::rfgui::style::{Border, BorderRadius, Color, Layout, Length, Padding};
use crate::rfgui::ui::{RsxNode, component, on_text_area_render, rsx, use_state};
use crate::rfgui::view::{Element, Text, TextArea};
use crate::rfgui_components::{Switch, Theme};

fn projection_token_ranges(content: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = content.chars().collect();
    let mut ranges = Vec::new();
    let mut index = 0_usize;
    while index + 1 < chars.len() {
        if chars[index] == '{' && chars[index + 1] == '{' {
            let start = index;
            let mut cursor = index + 2;
            while cursor + 1 < chars.len() {
                if chars[cursor] == '}' && chars[cursor + 1] == '}' {
                    ranges.push((start, cursor + 2));
                    index = cursor + 2;
                    break;
                }
                cursor += 1;
            }
            if cursor + 1 >= chars.len() {
                break;
            }
            continue;
        }
        index += 1;
    }
    ranges
}

#[component]
pub fn TextareaTest(theme: Theme) -> RsxNode {
    let content = use_state(|| {
        String::from(
            "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line",
        )
    });
    let multiline = use_state(|| true);
    let projection = use_state(|| true);
    let fixed_width = use_state(|| true);
    let fixed_height = use_state(|| true);
    let auto_wrap = use_state(|| true);

    let multiline_value = multiline.get();
    let projection_value = projection.get();
    let fixed_width_value = fixed_width.get();
    let fixed_height_value = fixed_height.get();
    let auto_wrap_value = auto_wrap.get();

    let badge_background = Color::hex("#233241");
    let badge_border = Color::hex("#42566f");
    let badge_text = Color::hex("#9fbdd9");
    let textarea_width = Length::px(360.0);
    let textarea_height = Length::px(176.0);

    let projection_renderer = projection_value.then(|| {
        let bg = badge_background.clone();
        let bd = badge_border.clone();
        let tx = badge_text.clone();
        on_text_area_render(
            move |render: &mut rfgui::view::base_component::TextAreaRenderString| {
                let content_chars: Vec<char> = render.content().chars().collect();
                for (start, end) in projection_token_ranges(render.content()) {
                    let badge_background = bg.clone();
                    let badge_border = bd.clone();
                    let badge_text = tx.clone();
                    let slice: String = content_chars[start..end].iter().collect();
                    render.range(start..end, move |_text_area_node| {
                        // let slice = slice.clone();
                        // let hover = use_state(|| false);
                        // let hover_clone = hover.clone();

                        rsx! {
                            <Element style={{
                                background: badge_background.clone(),
                                border: Border::uniform(Length::px(1.0), &badge_border),
                                border_radius: BorderRadius::uniform(Length::px(4.0)),
                                padding: Padding::uniform(Length::px(0.0)).x(Length::px(20.0)),
                                color: badge_text.clone(),
                                font_size: theme.typography.size.xl,
                                // opacity: hover.get().then_some(0.8).unwrap_or(1.0),
                            }}>
                            // }} on_pointer_enter={move |_| {hover.set(true)}} on_pointer_leave={move |_| {hover_clone.set(false)}}>
                                <Text>{slice}</Text>
                            </Element>
                        }
                    });
                }
            },
        )
    });

    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            background: theme.color.layer.surface.clone(),
            layout: Layout::flow().column().no_wrap(),
            gap: theme.spacing.sm,
            padding: Padding::uniform(theme.spacing.md),
            color: theme.color.text.primary.clone(),
            font_size: theme.typography.size.sm,
        }}>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.md,
            }}>
                <Switch label="Multiline" binding={multiline.binding()} />
                <Switch label="Projection" binding={projection.binding()} />
                <Switch label="Fixed width" binding={fixed_width.binding()} />
                <Switch label="Fixed height" binding={fixed_height.binding()} />
                <Switch label="Auto wrap" binding={auto_wrap.binding()} />
            </Element>
            <Text>{format!("chars={} lines={} width={} height={} projection={} wrap={}",
                content.get().chars().count(),
                content.get().lines().count(),
                if fixed_width_value { "fixed" } else { "auto" },
                if fixed_height_value { "fixed" } else { "auto" },
                projection_value,
                auto_wrap_value,
            )}</Text>
            // Design A1: TextArea is not-IS-A Element. Box model
            // (width / height / background / border / border-radius /
            // padding) lives on a wrapping <Element>. TextArea's own
            // style is text-side only (color / font / font-size).
            <Element style={{
                width: fixed_width_value.then_some(textarea_width),
                height: fixed_height_value.then_some(textarea_height),
                background: theme.color.background.base.clone(),
                border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
                border_radius: theme.radius.md,
                padding: Padding::uniform(Length::px(8.0)),
            }}>
                <TextArea
                    style={{
                        color: theme.color.text.primary.clone(),
                    }}
                    font_size=14
                    multiline={multiline_value}
                    auto_wrap={auto_wrap_value}
                    placeholder="Type text here..."
                    binding={content.binding()}
                    on_render={projection_renderer}
                />
            </Element>
        </Element>
    }
}
