use crate::use_theme;
use rfgui::ui::{Binding, RsxComponent, RsxNode, component, props, rsx, use_state};
use rfgui::view::Element;
use rfgui::{
    ClipMode, Collision, CollisionBoundary, Layout, Length, Operator, Padding, Position, Transform,
    Translate,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TooltipPlacement {
    Top,
    TopStart,
    TopEnd,
    Bottom,
    BottomStart,
    BottomEnd,
    Left,
    LeftStart,
    LeftEnd,
    Right,
    RightStart,
    RightEnd,
}

impl Default for TooltipPlacement {
    fn default() -> Self {
        TooltipPlacement::Bottom
    }
}

impl From<&str> for TooltipPlacement {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "top" => TooltipPlacement::Top,
            "top-start" => TooltipPlacement::TopStart,
            "top-end" => TooltipPlacement::TopEnd,
            "bottom" => TooltipPlacement::Bottom,
            "bottom-start" => TooltipPlacement::BottomStart,
            "bottom-end" => TooltipPlacement::BottomEnd,
            "left" => TooltipPlacement::Left,
            "left-start" => TooltipPlacement::LeftStart,
            "left-end" => TooltipPlacement::LeftEnd,
            "right" => TooltipPlacement::Right,
            "right-start" => TooltipPlacement::RightStart,
            "right-end" => TooltipPlacement::RightEnd,
            other => panic!("rsx build error on <Tooltip>. unknown placement `{other}`"),
        }
    }
}

impl From<String> for TooltipPlacement {
    fn from(value: String) -> Self {
        TooltipPlacement::from(value.as_str())
    }
}

impl rfgui::ui::IntoOptionalProp<TooltipPlacement> for &str {
    fn into_optional_prop(self) -> Option<TooltipPlacement> {
        Some(TooltipPlacement::from(self))
    }
}

impl rfgui::ui::IntoOptionalProp<TooltipPlacement> for String {
    fn into_optional_prop(self) -> Option<TooltipPlacement> {
        Some(TooltipPlacement::from(self))
    }
}

/// Imperative handle for controlling a `<Tooltip>` from outside its subtree.
///
/// Created via [`use_tooltip_ref`]. Pass into `<Tooltip handle={...}>` and call
/// `show()` / `hide()` from arbitrary event handlers (e.g. on a sibling
/// trigger element's `on_pointer_enter`).
///
/// When a Tooltip has no `handle`, it renders unconditionally — useful for
/// host components (like `Button`) that already gate visibility via
/// conditional mount/unmount.
#[derive(Clone, PartialEq)]
pub struct TooltipRef {
    open: Binding<bool>,
}

impl TooltipRef {
    pub fn show(&self) {
        self.open.set(true);
    }

    pub fn hide(&self) {
        self.open.set(false);
    }

    pub fn toggle(&self) {
        self.open.update(|v| *v = !*v);
    }

    pub fn visible(&self) -> bool {
        self.open.get()
    }
}

pub fn use_tooltip_ref() -> TooltipRef {
    let open = use_state(|| false);
    TooltipRef {
        open: open.binding(),
    }
}

pub struct Tooltip;

#[derive(Clone)]
#[props]
pub struct TooltipProps {
    pub handle: Option<TooltipRef>,
    pub placement: Option<TooltipPlacement>,
    pub arrow: Option<bool>,
}

impl RsxComponent<TooltipProps> for Tooltip {
    fn render(props: TooltipProps, children: Vec<RsxNode>) -> RsxNode {
        rsx! {
            <TooltipView
                handle={props.handle}
                placement={props.placement.unwrap_or_default()}
                arrow={props.arrow.unwrap_or(false)}
            >
                {children}
            </TooltipView>
        }
    }
}

#[rfgui::ui::component]
impl rfgui::ui::RsxTag for Tooltip {
    type Props = __TooltipPropsInit;
    type StrictProps = TooltipProps;
    const ACCEPTS_CHILDREN: bool = true;

    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<rfgui::ui::RsxNode>,
        _key: Option<rfgui::ui::RsxKey>,
    ) -> rfgui::ui::RsxNode {
        <Self as RsxComponent<TooltipProps>>::render(props, children)
    }
}

fn placement_position(placement: TooltipPlacement, gap: Length) -> Position {
    use TooltipPlacement::*;
    let base = Position::absolute()
        .collision(Collision::FlipFit, CollisionBoundary::Viewport)
        .clip(ClipMode::Viewport);
    let gap_plus_full = Length::calc(Length::percent(100.0), Operator::plus, gap);
    match placement {
        Top => base.bottom(gap_plus_full).left(Length::percent(50.0)),
        TopStart => base.bottom(gap_plus_full).left(Length::px(0.0)),
        TopEnd => base.bottom(gap_plus_full).right(Length::px(0.0)),
        Bottom => base.top(gap_plus_full).left(Length::percent(50.0)),
        BottomStart => base.top(gap_plus_full).left(Length::px(0.0)),
        BottomEnd => base.top(gap_plus_full).right(Length::px(0.0)),
        Left => base.right(gap_plus_full).top(Length::percent(50.0)),
        LeftStart => base.right(gap_plus_full).top(Length::px(0.0)),
        LeftEnd => base.right(gap_plus_full).bottom(Length::px(0.0)),
        Right => base.left(gap_plus_full).top(Length::percent(50.0)),
        RightStart => base.left(gap_plus_full).top(Length::px(0.0)),
        RightEnd => base.left(gap_plus_full).bottom(Length::px(0.0)),
    }
}

fn placement_centering_translate(placement: TooltipPlacement) -> Option<Transform> {
    use TooltipPlacement::*;
    match placement {
        Top | Bottom => Some(Transform::new([Translate::xy(
            Length::percent(-50.0),
            Length::px(0.0),
        )])),
        Left | Right => Some(Transform::new([Translate::xy(
            Length::px(0.0),
            Length::percent(-50.0),
        )])),
        _ => None,
    }
}

#[component]
fn TooltipView(
    handle: Option<TooltipRef>,
    placement: TooltipPlacement,
    arrow: bool,
    children: Vec<RsxNode>,
) -> RsxNode {
    // arrow: accepted but not yet implemented.
    let _ = arrow;

    let visible = handle.as_ref().map(|h| h.visible()).unwrap_or(true);
    if !visible {
        return RsxNode::fragment(vec![]);
    }

    let theme = use_theme().0;
    let gap = Length::px(6.0);
    let position = placement_position(placement, gap);
    let centering = placement_centering_translate(placement);

    rsx! {
        <Element
            style={{
                position: position,
                padding: Padding::uniform(theme.spacing.xs).x(theme.spacing.sm),
                background: theme.color.layer.inverse.clone(),
                border_radius: theme.radius.sm,
                color: theme.color.layer.on_inverse.clone(),
                font_size: theme.typography.size.xs,
                transform: centering,
                layout: Layout::flow().row().no_wrap(),
            }}
        >
            {children}
        </Element>
    }
}
