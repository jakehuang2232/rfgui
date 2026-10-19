use crate::page::{PAGE_GROUPS, Page};
use crate::rfgui::style::{
    Align, Border, Color, ColorLike, Cursor, Layout, Length, Padding, ScrollDirection, flex,
};
use crate::rfgui::ui::{Binding, RsxNode, component, rsx};
use crate::rfgui::view::{Element, Image, ImageFit, ImageSampling, Text};
use crate::rfgui_components::{Switch, Theme, use_theme};
use crate::utils::output_image_source;
use std::rc::Rc;

const WIDTH: f32 = 232.0;

/// The page navigation, with the theme and Inspector switches below it.
#[component]
pub fn Sidebar(
    page: &Binding<Page>,
    dark_mode: &Binding<bool>,
    on_dark_mode: &Rc<dyn Fn(bool)>,
    inspector_open: &Binding<bool>,
) -> RsxNode {
    let (theme, _) = use_theme();
    let current = page.get();
    let groups = PAGE_GROUPS
        .iter()
        .enumerate()
        .map(|(index, group)| {
            let items = group
                .pages
                .iter()
                .map(|&target| nav_item(&theme, target, current, page.clone()))
                .collect::<Vec<_>>();
            rsx! {
                <Element key={index} style={{
                    width: Length::percent(100.0),
                    layout: Layout::flow().column().no_wrap(),
                    gap: Length::px(2.0),
                }}>
                    <Element style={{
                        padding: Padding::uniform(Length::Zero).x(theme.spacing.md).y(theme.spacing.xs),
                        color: theme.color.text.secondary.clone(),
                        font_size: theme.typography.size.xs,
                        font_weight: theme.typography.weight.medium,
                    }}>
                        <Text>{group.title}</Text>
                    </Element>
                    {items}
                </Element>
            }
        })
        .collect::<Vec<_>>();

    rsx! {
        <Element style={{
            width: Length::px(WIDTH),
            height: Length::percent(100.0),
            flex: flex().shrink(0.0),
            layout: Layout::flex().column(),
            background: theme.color.layer.surface.clone(),
            border: Border::uniform(Length::Zero, theme.color.divider.as_ref())
                .right(Some(Length::px(1.0)), None),
        }}>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flex().row().align(Align::Center),
                gap: theme.spacing.sm,
                padding: Padding::uniform(theme.spacing.md).x(theme.spacing.lg),
            }}>
                <Image
                    style={{ width: Length::px(28.0), height: Length::px(28.0) }}
                    source={output_image_source("rfgui-logo.png")}
                    sampling={ImageSampling::Linear}
                    fit={ImageFit::Contain}
                />
                <Text style={{
                    font_size: theme.typography.size.lg,
                    font_weight: theme.typography.weight.bold,
                }}>RFGUI</Text>
            </Element>
            <Element style={{
                width: Length::percent(100.0),
                flex: flex().grow(1.0).shrink(1.0),
                min_height: Length::Zero,
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.sm,
                padding: Padding::uniform(Length::Zero).x(theme.spacing.sm),
                scroll_direction: ScrollDirection::Vertical,
            }}>
                {groups}
            </Element>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.sm,
                padding: Padding::uniform(theme.spacing.sm),
                border: Border::uniform(Length::Zero, theme.color.divider.as_ref())
                    .top(Some(Length::px(1.0)), None),
            }}>
                {nav_item(&theme, Page::Licenses, current, page.clone())}
                <Element style={{
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.sm,
                    padding: Padding::uniform(Length::Zero).x(theme.spacing.md).y(theme.spacing.xs),
                }}>
                    <Switch label="Dark mode" binding={dark_mode.clone()} on_change={on_dark_mode.clone()} />
                    <Switch label="Inspector" binding={inspector_open.clone()} />
                </Element>
            </Element>
        </Element>
    }
}

fn nav_item(theme: &Theme, target: Page, current: Page, page: Binding<Page>) -> RsxNode {
    let selected = target == current;
    let background: Box<dyn ColorLike> = if selected {
        theme.color.state.active.clone()
    } else {
        Box::new(Color::transparent())
    };
    let color = if selected {
        theme.color.primary.base.clone()
    } else {
        theme.color.text.primary.clone()
    };
    rsx! {
        <Element
            key={target.key()}
            style={{
                width: Length::percent(100.0),
                layout: Layout::flex().row().align(Align::Center),
                gap: theme.spacing.sm,
                padding: Padding::uniform(Length::px(5.0)).x(theme.spacing.md),
                border_radius: theme.radius.md,
                background: background,
                color: color,
                cursor: Cursor::Pointer,
                hover: {
                    background: theme.color.state.hover.clone(),
                },
            }}
            on_click={move |_| page.set(target)}
        >
            <Element style={{ font_size: theme.typography.size.lg }}>
                {target.icon()}
            </Element>
            <Text>{target.title()}</Text>
        </Element>
    }
}
