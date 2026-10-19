//! Building blocks the showcase pages share.

use crate::rfgui::style::{Animator, Border, Layout, Length, Padding};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::use_theme;

/// `animator` while the Inspector's animation switch is on.
pub(crate) fn demo_animator(enabled: bool, animator: Animator) -> Option<Animator> {
    if !enabled {
        return None;
    }
    #[cfg(any(test, feature = "renderer-perf"))]
    if matches!(
        std::env::var("RFGUI_PERF_UPDATES").as_deref(),
        Ok("idle" | "particles")
    ) {
        return None;
    }
    Some(animator)
}

/// A card on a page: a title, what it shows, and the live demo below.
#[component]
pub fn Demo(title: &str, description: &str, children: Vec<RsxNode>) -> RsxNode {
    let (theme, _) = use_theme();
    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            layout: Layout::flow().column().no_wrap(),
            gap: theme.spacing.md,
            padding: Padding::uniform(theme.spacing.lg),
            background: theme.color.layer.surface.clone(),
            border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
            border_radius: theme.radius.lg,
        }}>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.xs,
            }}>
                <Text style={{
                    font_size: theme.typography.size.md,
                    font_weight: theme.typography.weight.medium,
                }}>{title}</Text>
                <Text style={{ color: theme.color.text.secondary.clone() }}>{description}</Text>
            </Element>
            {children}
        </Element>
    }
}

/// One sample inside a demo, captioned with what it shows.
#[component]
pub fn Sample(label: &str, children: Vec<RsxNode>) -> RsxNode {
    let (theme, _) = use_theme();
    rsx! {
        <Element style={{
            layout: Layout::flow().column().no_wrap(),
            gap: theme.spacing.sm,
            padding: Padding::uniform(theme.spacing.md),
            background: theme.color.layer.raised.clone(),
            border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
            border_radius: theme.radius.md,
        }}>
            <Text style={{
                color: theme.color.text.secondary.clone(),
                font_size: theme.typography.size.xs,
            }}>{label}</Text>
            {children}
        </Element>
    }
}

/// Samples side by side, wrapping onto new lines.
#[component]
pub fn SampleRow(children: Vec<RsxNode>) -> RsxNode {
    let (theme, _) = use_theme();
    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            layout: Layout::flow().row().wrap(),
            gap: theme.spacing.md,
        }}>
            {children}
        </Element>
    }
}

/// A secondary heading inside a demo.
#[component]
pub fn Caption(text: &str) -> RsxNode {
    let (theme, _) = use_theme();
    rsx! {
        <Text style={{ color: theme.color.text.secondary.clone() }}>{text}</Text>
    }
}
