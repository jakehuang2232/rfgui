use crate::rfgui::style::{Border, Layout, Length, Padding};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::use_theme;
use crate::showcase::{Caption, Demo};

#[component]
pub fn Typography() -> RsxNode {
    let (theme, _) = use_theme();
    let size = &theme.typography.size;
    let weight = &theme.typography.weight;
    rsx! {
        <Demo
            title="Typography"
            description="Text is shaped and laid out per script, so Latin, CJK and emoji share a line and each breaks where its script allows."
        >
            <Caption text="Sizes" />
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.lg,
            }}>
                <Text style={{ font_size: size.xs }}>Extra small</Text>
                <Text style={{ font_size: size.sm }}>Small</Text>
                <Text style={{ font_size: size.md }}>Medium</Text>
                <Text style={{ font_size: size.lg }}>Large</Text>
                <Text style={{ font_size: size.xl }}>Extra large</Text>
            </Element>
            <Caption text="Weights and colors" />
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.lg,
                font_size: size.md,
            }}>
                <Text style={{ font_weight: weight.regular }}>Regular</Text>
                <Text style={{ font_weight: weight.medium }}>Medium</Text>
                <Text style={{ font_weight: weight.bold }}>Bold</Text>
                <Text style={{ color: theme.color.primary.base.clone() }}>Primary</Text>
                <Text style={{ color: theme.color.secondary.base.clone() }}>Secondary</Text>
                <Text style={{ color: theme.color.text.secondary.clone() }}>Muted</Text>
            </Element>
            <Caption text="Mixed scripts wrap together" />
            <Element style={{
                width: Length::percent(100.0),
                padding: Padding::uniform(theme.spacing.md),
                background: theme.color.layer.raised.clone(),
                border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
                border_radius: theme.radius.md,
                font_size: size.md,
                line_height: 1.5,
            }}>
                <Text>{"English words wrap at spaces, 中文在字與字之間換行，日本語も同じです。Emoji such as 🚀 🦀 ✨ sit on the same baseline, and the whole paragraph reflows as the window narrows."}</Text>
            </Element>
        </Demo>
    }
}
