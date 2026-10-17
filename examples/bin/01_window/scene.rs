use crate::inspector_panel::InspectorPanel;
use crate::page::Page;
use crate::pages::PageView;
use crate::rfgui::style::{Color, ColorLike, Layout, Length, ScrollDirection, flex};
use crate::rfgui::ui::{RsxNode, component, rsx, use_mount, use_state, use_viewport};
use crate::rfgui::view::Element;
use crate::rfgui_components::{Theme, use_theme};
use crate::sidebar::Sidebar;
use std::rc::Rc;

/// The sidebar and the selected page, with the Inspector floating above.
#[component]
pub fn MainScene(initial_page: Option<Page>) -> RsxNode {
    let page = use_state(|| initial_page.unwrap_or(Page::Welcome));
    let dark_mode = use_state(|| true);
    let animation_on = use_state(|| true);
    let inspector_open = use_state(|| false);

    let (theme, set_theme) = use_theme();
    let viewport = use_viewport();
    let background = theme.color.background.base.clone();
    use_mount(move || {
        viewport.set_clear_color(background.to_style_color().to_color());
    });
    let on_dark_mode = Rc::new(move |on: bool| {
        if on {
            set_theme(Theme::dark());
            viewport.set_clear_color(Color::rgb(40, 44, 52));
        } else {
            set_theme(Theme::light());
            viewport.set_clear_color(Color::rgb(240, 240, 240));
        }
    }) as Rc<dyn Fn(bool)>;

    let current = page.get();
    let shell = rsx! {
        <Element style={{
            width: Length::percent(100.0),
            height: Length::percent(100.0),
            layout: Layout::flex().row(),
            background: theme.color.background.base.clone(),
            color: theme.color.text.primary.clone(),
            font: theme.typography.font_family.clone(),
            font_size: theme.typography.size.sm,
        }}>
            <Sidebar
                page={page.binding()}
                dark_mode={dark_mode.binding()}
                on_dark_mode={on_dark_mode.clone()}
                inspector_open={inspector_open.binding()}
            />
            // Keyed by page, so each page opens scrolled to its top.
            <Element key={current.key()} style={{
                flex: flex().grow(1.0).shrink(1.0),
                min_width: Length::Zero,
                height: Length::percent(100.0),
                layout: Layout::flow().column().no_wrap(),
                scroll_direction: ScrollDirection::Vertical,
            }}>
                <PageView
                    page={current}
                    navigate={page.binding()}
                    animation_on={animation_on.binding()}
                />
            </Element>
        </Element>
    };
    RsxNode::fragment(vec![
        shell,
        rsx! {
            <InspectorPanel
                open={inspector_open.get()}
                dark_mode={dark_mode.binding()}
                on_dark_mode={on_dark_mode}
                animation_on={animation_on.binding()}
            />
        },
    ])
}
