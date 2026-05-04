use crate::use_theme;
use rfgui::style::{Align, Anchor, ClipMode, JustifyContent, Layout, Length, Padding, Position};
use rfgui::ui::{RsxComponent, RsxKey, RsxNode, component, props, rsx, use_timeout};
use rfgui::view::Element;
use std::rc::Rc;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnackbarVertical {
    Top,
    Bottom,
}

impl Default for SnackbarVertical {
    fn default() -> Self {
        SnackbarVertical::Bottom
    }
}

impl From<&str> for SnackbarVertical {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "top" => SnackbarVertical::Top,
            "bottom" => SnackbarVertical::Bottom,
            other => panic!("rsx build error on <Snackbar>. unknown vertical `{other}`"),
        }
    }
}

impl From<String> for SnackbarVertical {
    fn from(value: String) -> Self {
        SnackbarVertical::from(value.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnackbarHorizontal {
    Left,
    Right,
}

impl Default for SnackbarHorizontal {
    fn default() -> Self {
        SnackbarHorizontal::Left
    }
}

impl From<&str> for SnackbarHorizontal {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "left" => SnackbarHorizontal::Left,
            "right" => SnackbarHorizontal::Right,
            other => panic!("rsx build error on <Snackbar>. unknown horizontal `{other}`"),
        }
    }
}

impl From<String> for SnackbarHorizontal {
    fn from(value: String) -> Self {
        SnackbarHorizontal::from(value.as_str())
    }
}

/// MUI-style `anchorOrigin`. Picks which corner / edge of the viewport the
/// snackbar pins to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SnackbarOrigin {
    pub vertical: SnackbarVertical,
    pub horizontal: SnackbarHorizontal,
}

impl SnackbarOrigin {
    pub const fn new(vertical: SnackbarVertical, horizontal: SnackbarHorizontal) -> Self {
        Self {
            vertical,
            horizontal,
        }
    }
}

/// Reason supplied to `on_close`. Mirrors the subset of MUI close reasons
/// rfgui currently surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnackbarCloseReason {
    Timeout,
    Manual,
}

pub struct Snackbar;

#[derive(Clone)]
#[props]
pub struct SnackbarProps {
    pub open: bool,
    pub message: Option<RsxNode>,
    pub action: Option<RsxNode>,
    pub auto_hide_duration: Option<Duration>,
    pub anchor_origin: Option<SnackbarOrigin>,
    pub on_close: Option<Rc<dyn Fn(SnackbarCloseReason)>>,
}

impl RsxComponent<SnackbarProps> for Snackbar {
    fn render(props: SnackbarProps, children: Vec<RsxNode>) -> RsxNode {
        rsx! {
            <SnackbarView
                open={props.open}
                message={props.message}
                action={props.action}
                auto_hide_duration={props.auto_hide_duration}
                anchor_origin={props.anchor_origin.unwrap_or_default()}
                on_close={props.on_close}
            >
                {children}
            </SnackbarView>
        }
    }
}

#[rfgui::ui::component]
impl rfgui::ui::RsxTag for Snackbar {
    type Props = __SnackbarPropsInit;
    type StrictProps = SnackbarProps;
    const ACCEPTS_CHILDREN: bool = true;

    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<RsxNode>,
        _key: Option<RsxKey>,
    ) -> RsxNode {
        <Self as RsxComponent<SnackbarProps>>::render(props, children)
    }
}

fn placement_position(origin: SnackbarOrigin, gap: Length) -> Position {
    let base = Position::absolute()
        .anchor(Anchor::Viewport)
        .clip(ClipMode::Viewport);
    let with_v = match origin.vertical {
        SnackbarVertical::Top => base.top(gap),
        SnackbarVertical::Bottom => base.bottom(gap),
    };
    match origin.horizontal {
        SnackbarHorizontal::Left => with_v.left(gap),
        SnackbarHorizontal::Right => with_v.right(gap),
    }
}

fn horizontal_justify(origin: SnackbarOrigin) -> JustifyContent {
    match origin.horizontal {
        SnackbarHorizontal::Left => JustifyContent::Start,
        SnackbarHorizontal::Right => JustifyContent::End,
    }
}

#[component]
fn SnackbarView(
    open: bool,
    message: Option<RsxNode>,
    action: Option<RsxNode>,
    auto_hide_duration: Option<Duration>,
    anchor_origin: SnackbarOrigin,
    on_close: Option<Rc<dyn Fn(SnackbarCloseReason)>>,
    children: Vec<RsxNode>,
) -> RsxNode {
    let timer_enabled = open && auto_hide_duration.is_some();
    let timer_duration = auto_hide_duration.unwrap_or(Duration::ZERO);
    let timeout_cb = on_close.clone();
    use_timeout(timer_enabled, timer_duration, move || {
        if let Some(cb) = timeout_cb.as_ref() {
            cb(SnackbarCloseReason::Timeout);
        }
    });

    if !open {
        return RsxNode::fragment(vec![]);
    }

    let theme = use_theme().0;
    let position = placement_position(anchor_origin, theme.spacing.lg);
    let justify = horizontal_justify(anchor_origin);

    let body: Vec<RsxNode> = if children.is_empty() {
        vec![rsx! {
            <SnackbarContent message={message} action={action} />
        }]
    } else {
        children
    };

    rsx! {
        <Element
            style={{
                position: position,
                layout: Layout::flow()
                    .row()
                    .no_wrap()
                    .align(Align::Center)
                    .justify_content(justify),
            }}
        >
            {body}
        </Element>
    }
}

pub struct SnackbarContent;

#[derive(Clone)]
#[props]
pub struct SnackbarContentProps {
    pub message: Option<RsxNode>,
    pub action: Option<RsxNode>,
}

impl RsxComponent<SnackbarContentProps> for SnackbarContent {
    fn render(props: SnackbarContentProps, _children: Vec<RsxNode>) -> RsxNode {
        rsx! {
            <SnackbarContentView message={props.message} action={props.action} />
        }
    }
}

#[rfgui::ui::component]
impl rfgui::ui::RsxTag for SnackbarContent {
    type Props = __SnackbarContentPropsInit;
    type StrictProps = SnackbarContentProps;
    const ACCEPTS_CHILDREN: bool = false;

    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<rfgui::ui::RsxNode>,
        _key: Option<rfgui::ui::RsxKey>,
    ) -> rfgui::ui::RsxNode {
        <Self as RsxComponent<SnackbarContentProps>>::render(props, children)
    }
}

#[component]
fn SnackbarContentView(message: Option<RsxNode>, action: Option<RsxNode>) -> RsxNode {
    let theme = use_theme().0;
    let message_node = message.unwrap_or_else(|| RsxNode::fragment(vec![]));
    let action_node = action.unwrap_or_else(|| RsxNode::fragment(vec![]));

    rsx! {
        <Element
            style={{
                padding: Padding::uniform(theme.spacing.sm).x(theme.spacing.md),
                background: theme.color.layer.inverse.clone(),
                color: theme.color.layer.on_inverse.clone(),
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
            {message_node}
            {action_node}
        </Element>
    }
}

/// Convenience: wrap a `Binding<bool>` into a `SnackbarProps::on_close`
/// handler that flips the binding to `false`. Drops the close reason. Use
/// when the simplest behaviour ("auto-hide → just close") is enough.
pub fn snackbar_close_binding(open: rfgui::ui::Binding<bool>) -> Rc<dyn Fn(SnackbarCloseReason)> {
    Rc::new(move |_reason: SnackbarCloseReason| {
        open.set(false);
    })
}
