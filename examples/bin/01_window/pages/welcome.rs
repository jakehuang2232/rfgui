use crate::page::{PAGE_GROUPS, Page};
use crate::rfgui::style::{Align, Border, Cursor, Layout, Length, Padding, flex};
use crate::rfgui::ui::{Binding, RsxNode, component, rsx};
use crate::rfgui::view::{Element, Image, ImageFit, ImageSampling, Text};
use crate::rfgui_components::{Theme, use_theme};
use crate::showcase::Demo;
use crate::utils::output_image_source;

const HIGHLIGHTS: [(&str, &str); 3] = [
    (
        "Retained rendering",
        "Each frame records what changed and reuses the raster of everything else. Open the Inspector and turn on Debug RetainedAuto to watch it decide.",
    ),
    (
        "React-style components",
        "#[component] functions return rsx! trees, with hooks, context, keys and error boundaries.",
    ),
    (
        "Web-like layout and style",
        "Inline, flow and flex layout, typed styles, transitions, keyframe animations and 3D transforms.",
    ),
];

#[component]
pub fn WelcomePage(navigate: Binding<Page>) -> RsxNode {
    let (theme, _) = use_theme();
    let highlights = HIGHLIGHTS
        .iter()
        .enumerate()
        .map(|(index, (title, text))| {
            rsx! {
                <Element key={index} style={{
                    width: Length::px(260.0),
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                    padding: Padding::uniform(theme.spacing.md),
                    background: theme.color.layer.raised.clone(),
                    border_radius: theme.radius.md,
                }}>
                    <Text style={{ font_weight: theme.typography.weight.medium }}>{title.to_string()}</Text>
                    <Text style={{ color: theme.color.text.secondary.clone() }}>{text.to_string()}</Text>
                </Element>
            }
        })
        .collect::<Vec<_>>();
    let tours = PAGE_GROUPS
        .iter()
        .flat_map(|group| group.pages.iter().copied())
        .filter(|&page| page != Page::Welcome)
        .map(|page| tour_card(&theme, page, navigate.clone()))
        .collect::<Vec<_>>();

    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            layout: Layout::flex().row().align(Align::Center),
            gap: theme.spacing.xl,
            padding: Padding::uniform(theme.spacing.xl),
            background: theme.color.layer.surface.clone(),
            border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
            border_radius: theme.radius.lg,
        }}>
            <Image
                style={{ width: Length::px(96.0), height: Length::px(96.0) }}
                source={output_image_source("rfgui-logo.png")}
                sampling={ImageSampling::Linear}
                fit={ImageFit::Contain}
            />
            <Element style={{
                flex: flex().grow(1.0).shrink(1.0),
                min_width: Length::Zero,
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.sm,
            }}>
                <Text style={{
                    font_size: 32.0,
                    font_weight: theme.typography.weight.bold,
                }}>RFGUI</Text>
                <Text style={{ font_size: theme.typography.size.md }}>
                    {"A retained-mode GUI framework for Rust, built on wgpu and a frame graph of render passes."}
                </Text>
                <Text style={{ color: theme.color.text.secondary.clone() }}>
                    {"Everything in this window is drawn by RFGUI. Pick a page on the left, or a card below."}
                </Text>
            </Element>
        </Element>
        <Element style={{
            width: Length::percent(100.0),
            layout: Layout::flow().row().wrap(),
            gap: theme.spacing.md,
        }}>
            {highlights}
        </Element>
        <Demo
            title="Tour"
            description="Every page shows one area of the framework with live demos."
        >
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.md,
            }}>
                {tours}
            </Element>
        </Demo>
        <Demo
            title="Get started"
            description="Add RFGUI to a project, or run this showcase from a clone of the repository."
        >
            <Text>{"cargo add rfgui"}</Text>
            <Text>{"cargo run -r -p examples --bin 01_window"}</Text>
        </Demo>
    }
}

fn tour_card(theme: &Theme, page: Page, navigate: Binding<Page>) -> RsxNode {
    rsx! {
        <Element
            key={page.key()}
            style={{
                width: Length::px(260.0),
                layout: Layout::flex().row(),
                gap: theme.spacing.md,
                padding: Padding::uniform(theme.spacing.md),
                background: theme.color.layer.raised.clone(),
                border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
                border_radius: theme.radius.md,
                cursor: Cursor::Pointer,
                hover: {
                    border: Border::uniform(Length::px(1.0), theme.color.primary.base.as_ref()),
                },
            }}
            on_click={move |_| navigate.set(page)}
        >
            <Element style={{
                color: theme.color.primary.base.clone(),
                font_size: theme.typography.size.xl,
            }}>
                {page.icon()}
            </Element>
            <Element style={{
                flex: flex().grow(1.0).shrink(1.0),
                min_width: Length::Zero,
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.xs,
            }}>
                <Text style={{ font_weight: theme.typography.weight.medium }}>{page.title()}</Text>
                <Text style={{ color: theme.color.text.secondary.clone() }}>{page.summary()}</Text>
            </Element>
        </Element>
    }
}
