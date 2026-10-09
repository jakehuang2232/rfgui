use crate::{Theme, use_theme};
use rfgui::style::FontSize;
use rfgui::ui::{RsxNode, component, profile_ui_work, render_root, rsx};
use rfgui::view::Element;

#[test]
fn theme_clones_share_identity_and_edits_copy_on_write() {
    let theme = Theme::dark();
    let shared = theme.clone();
    assert!(theme == shared, "a clone is the same theme");
    assert!(
        Theme::dark() != Theme::dark(),
        "separately built themes are distinct values"
    );

    let mut edited = shared.clone();
    edited.typography.size.md = FontSize::px(31.0);
    assert!(edited != theme, "an edit produces a new theme value");
    assert!(
        theme == shared,
        "editing a clone leaves the original untouched"
    );
    assert_ne!(theme.typography.size.md, FontSize::px(31.0));
    assert_eq!(edited.typography.size.md, FontSize::px(31.0));
}

#[component]
fn ThemedPanel(theme: Theme) -> RsxNode {
    rsx! { <Element style={{ font_size: theme.typography.size.md }} /> }
}

#[component]
fn ThemedPage(tick: i64) -> RsxNode {
    let theme = use_theme().0;
    rsx! {
        <Element>
            {tick.to_string()}
            <ThemedPanel theme={theme} />
        </Element>
    }
}

#[test]
fn unchanged_theme_prop_skips_child_render() {
    let _ = render_root(|| rsx! { <ThemedPage tick={0} /> });
    let (_, work) = profile_ui_work(|| render_root(|| rsx! { <ThemedPage tick={1} /> }));
    assert_eq!(
        work.component_renders, 1,
        "only the page re-renders; its themed child is reused"
    );
    assert_eq!(work.memo_hits, 1);

    let (_, set_theme) = use_theme();
    set_theme(Theme::light());
    let (_, work) = profile_ui_work(|| render_root(|| rsx! { <ThemedPage tick={1} /> }));
    assert_eq!(
        work.component_renders, 2,
        "a new theme re-renders the page and its themed child"
    );
}
