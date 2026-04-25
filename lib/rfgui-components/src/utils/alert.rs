use crate::{MaterialSymbolIcon, Theme, use_theme};
use rfgui::ui::{ClickHandlerProp, RsxComponent, RsxNode, component, props, rsx};
use rfgui::view::{Element, Text};
use rfgui::{
    Align, Border, Color, ColorLike, Cursor, JustifyContent, Layout, Length, Padding,
};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertSeverity {
    Info,
    Success,
    Warning,
    Error,
}

impl Default for AlertSeverity {
    fn default() -> Self {
        AlertSeverity::Info
    }
}

impl From<&str> for AlertSeverity {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "info" => AlertSeverity::Info,
            "success" => AlertSeverity::Success,
            "warning" => AlertSeverity::Warning,
            "error" => AlertSeverity::Error,
            other => panic!("rsx build error on <Alert>. unknown severity `{other}`"),
        }
    }
}

impl From<String> for AlertSeverity {
    fn from(value: String) -> Self {
        AlertSeverity::from(value.as_str())
    }
}

impl rfgui::ui::IntoOptionalProp<AlertSeverity> for &str {
    fn into_optional_prop(self) -> Option<AlertSeverity> {
        Some(AlertSeverity::from(self))
    }
}

impl rfgui::ui::IntoOptionalProp<AlertSeverity> for String {
    fn into_optional_prop(self) -> Option<AlertSeverity> {
        Some(AlertSeverity::from(self))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertVariant {
    Standard,
    Filled,
    Outlined,
}

impl Default for AlertVariant {
    fn default() -> Self {
        AlertVariant::Standard
    }
}

impl From<&str> for AlertVariant {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "standard" => AlertVariant::Standard,
            "filled" => AlertVariant::Filled,
            "outlined" => AlertVariant::Outlined,
            other => panic!("rsx build error on <Alert>. unknown variant `{other}`"),
        }
    }
}

impl From<String> for AlertVariant {
    fn from(value: String) -> Self {
        AlertVariant::from(value.as_str())
    }
}

impl rfgui::ui::IntoOptionalProp<AlertVariant> for &str {
    fn into_optional_prop(self) -> Option<AlertVariant> {
        Some(AlertVariant::from(self))
    }
}

impl rfgui::ui::IntoOptionalProp<AlertVariant> for String {
    fn into_optional_prop(self) -> Option<AlertVariant> {
        Some(AlertVariant::from(self))
    }
}

pub struct Alert;

#[derive(Clone)]
#[props]
pub struct AlertProps {
    pub severity: Option<AlertSeverity>,
    pub variant: Option<AlertVariant>,
    pub icon: Option<RsxNode>,
    pub action: Option<RsxNode>,
    pub on_close: Option<Rc<dyn Fn()>>,
}

impl RsxComponent<AlertProps> for Alert {
    fn render(props: AlertProps, children: Vec<RsxNode>) -> RsxNode {
        rsx! {
            <AlertView
                severity={props.severity.unwrap_or_default()}
                variant={props.variant.unwrap_or_default()}
                icon={props.icon}
                action={props.action}
                on_close={props.on_close}
            >
                {children}
            </AlertView>
        }
    }
}

#[rfgui::ui::component]
impl rfgui::ui::RsxTag for Alert {
    type Props = __AlertPropsInit;
    type StrictProps = AlertProps;
    const ACCEPTS_CHILDREN: bool = true;

    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<rfgui::ui::RsxNode>,
        _key: Option<rfgui::ui::RsxKey>,
    ) -> rfgui::ui::RsxNode {
        <Self as RsxComponent<AlertProps>>::render(props, children)
    }
}

fn severity_ligature(severity: AlertSeverity) -> &'static str {
    match severity {
        AlertSeverity::Info => "info",
        AlertSeverity::Success => "check_circle",
        AlertSeverity::Warning => "warning",
        AlertSeverity::Error => "error",
    }
}

fn severity_base_color(severity: AlertSeverity, theme: &Theme) -> Box<dyn ColorLike> {
    match severity {
        AlertSeverity::Info => theme.color.info.base.clone(),
        AlertSeverity::Success => theme.color.success.base.clone(),
        AlertSeverity::Warning => theme.color.warning.base.clone(),
        AlertSeverity::Error => theme.color.error.base.clone(),
    }
}

fn severity_on_color(severity: AlertSeverity, theme: &Theme) -> Box<dyn ColorLike> {
    match severity {
        AlertSeverity::Info => theme.color.info.on.clone(),
        AlertSeverity::Success => theme.color.success.on.clone(),
        AlertSeverity::Warning => theme.color.warning.on.clone(),
        AlertSeverity::Error => theme.color.error.on.clone(),
    }
}

fn tinted_background(severity: AlertSeverity, theme: &Theme) -> Box<dyn ColorLike> {
    let [r, g, b, _] = severity_base_color(severity, theme).to_rgba_u8();
    Box::new(Color::rgba(r, g, b, 36))
}

#[component]
fn AlertView(
    severity: AlertSeverity,
    variant: AlertVariant,
    icon: Option<RsxNode>,
    action: Option<RsxNode>,
    on_close: Option<Rc<dyn Fn()>>,
    children: Vec<RsxNode>,
) -> RsxNode {
    let theme = use_theme().0;
    let base = severity_base_color(severity, &theme);
    let on_base = severity_on_color(severity, &theme);

    let (background, foreground, border) = match variant {
        AlertVariant::Filled => (
            base.clone(),
            on_base.clone(),
            Border::uniform(Length::px(0.0), &Color::transparent()),
        ),
        AlertVariant::Outlined => (
            Box::new(Color::transparent()) as Box<dyn ColorLike>,
            base.clone(),
            Border::uniform(Length::px(1.0), base.as_ref()),
        ),
        AlertVariant::Standard => (
            tinted_background(severity, &theme),
            base.clone(),
            Border::uniform(Length::px(0.0), &Color::transparent()),
        ),
    };

    let icon_node = icon.unwrap_or_else(|| {
        rsx! {
            <MaterialSymbolIcon style={{
                font_size: theme.typography.size.md,
                color: foreground.clone(),
            }}>
                {severity_ligature(severity)}
            </MaterialSymbolIcon>
        }
    });

    let action_node = action.unwrap_or_else(|| RsxNode::fragment(vec![]));

    let close_button = if let Some(cb) = on_close {
        let click_handler = ClickHandlerProp::new(move |_| cb());
        let close_color = foreground.clone();
        let close_size = theme.typography.size.sm;
        let close_pad = theme.spacing.xs;
        let close_radius = theme.radius.sm;
        rsx! {
            <Element
                style={{
                    cursor: Cursor::Pointer,
                    padding: Padding::uniform(close_pad),
                    border_radius: close_radius,
                    layout: Layout::flow()
                        .row()
                        .no_wrap()
                        .align(Align::Center)
                        .justify_content(JustifyContent::Center),
                }}
                on_click={click_handler}
            >
                <MaterialSymbolIcon style={{
                    font_size: close_size,
                    color: close_color,
                }}>
                    {"close"}
                </MaterialSymbolIcon>
            </Element>
        }
    } else {
        RsxNode::fragment(vec![])
    };

    rsx! {
        <Element
            style={{
                padding: Padding::uniform(theme.spacing.sm).x(theme.spacing.md),
                background: background,
                color: foreground.clone(),
                border: border,
                border_radius: theme.radius.sm,
                font_size: theme.typography.size.sm,
                box_shadow: vec![theme.shadow.level_2.clone()],
                layout: Layout::flow()
                    .row()
                    .no_wrap()
                    .align(Align::Center)
                    .justify_content(JustifyContent::SpaceBetween),
                gap: theme.spacing.md,
                min_width: Length::px(240.0),
                max_width: Length::px(560.0),
            }}
        >
            <Element style={{
                layout: Layout::flow().row().no_wrap().align(Align::Center),
                gap: theme.spacing.sm,
            }}>
                {icon_node}
                <Text>{children}</Text>
            </Element>
            <Element style={{
                layout: Layout::flow().row().no_wrap().align(Align::Center),
                gap: theme.spacing.xs,
            }}>
                {action_node}
                {close_button}
            </Element>
        </Element>
    }
}
