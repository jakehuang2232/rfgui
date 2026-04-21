use crate::inputs::button::{ButtonColor, ButtonSize};
use crate::inputs::toggle_button::ToggleButton;
use crate::use_theme;
use rfgui::ui::{Binding, ClickEvent, Provider, RsxComponent, RsxNode, component, props, rsx, IntoOptionalProp, RsxTag, global_state};
use rfgui::view::Element;
use rfgui::{Border, ClipMode, Color, ColorLike, CrossSize, Layout, Length, Position};
use std::any::TypeId;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToggleOrientation {
    Horizontal,
    Vertical,
}

impl From<&str> for ToggleOrientation {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "horizontal" => ToggleOrientation::Horizontal,
            "vertical" => ToggleOrientation::Vertical,
            other => panic!(
                "rsx build error on <ToggleButtonGroup>. unknown orientation `{other}`"
            ),
        }
    }
}

impl From<String> for ToggleOrientation {
    fn from(value: String) -> Self {
        ToggleOrientation::from(value.as_str())
    }
}

impl IntoOptionalProp<ToggleOrientation> for &str {
    fn into_optional_prop(self) -> Option<ToggleOrientation> {
        Some(ToggleOrientation::from(self))
    }
}

impl IntoOptionalProp<ToggleOrientation> for String {
    fn into_optional_prop(self) -> Option<ToggleOrientation> {
        Some(ToggleOrientation::from(self))
    }
}

/// Context value published by [`ToggleButtonGroup`] to its descendant
/// [`super::ToggleButton`]s. Present-in-context = "I'm inside a group";
/// children override their own `selected`/`on_click`/`disabled` accordingly
/// and flatten their border / radius so the group wrapper owns the outline.
#[derive(Clone)]
pub struct ToggleButtonGroupContext {
    pub in_group: bool,
    pub value: Binding<Option<String>>,
    pub on_change: Option<ToggleGroupChangeHandler>,
    pub size: Option<ButtonSize>,
    pub color: Option<ButtonColor>,
    pub disabled: bool,
}

pub type ToggleGroupChangeHandler = Rc<dyn Fn(&mut ClickEvent, Option<String>)>;

pub struct ToggleButtonGroup;

#[derive(Clone)]
#[props]
pub struct ToggleButtonGroupProps {
    pub value: Option<Binding<Option<String>>>,
    pub on_change: Option<ToggleGroupChangeHandler>,
    pub orientation: Option<ToggleOrientation>,
    pub full_width: Option<bool>,
    pub size: Option<ButtonSize>,
    pub color: Option<ButtonColor>,
    pub disabled: Option<bool>,
}

impl RsxComponent<ToggleButtonGroupProps> for ToggleButtonGroup {
    fn render(props: ToggleButtonGroupProps, children: Vec<RsxNode>) -> RsxNode {
        rsx! {
            <ToggleButtonGroupView
                value={props.value}
                on_change={props.on_change}
                orientation={props.orientation}
                full_width={props.full_width}
                size={props.size}
                color={props.color}
                disabled={props.disabled}
            >
                {children}
            </ToggleButtonGroupView>
        }
    }
}

#[rfgui::ui::component]
impl RsxTag for ToggleButtonGroup {
    type Props = __ToggleButtonGroupPropsInit;
    type StrictProps = ToggleButtonGroupProps;
    const ACCEPTS_CHILDREN: bool = true;

    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<RsxNode>,
        _key: Option<rfgui::ui::RsxKey>,
    ) -> RsxNode {
        <Self as RsxComponent<ToggleButtonGroupProps>>::render(props, children)
    }
}

fn is_toggle_button(node: &RsxNode) -> bool {
    if let RsxNode::Component(inner) = node {
        inner.type_id == TypeId::of::<ToggleButton>()
    } else {
        false
    }
}

#[component]
fn ToggleButtonGroupView(
    value: Option<Binding<Option<String>>>,
    on_change: Option<ToggleGroupChangeHandler>,
    orientation: Option<ToggleOrientation>,
    full_width: Option<bool>,
    size: Option<ButtonSize>,
    color: Option<ButtonColor>,
    disabled: Option<bool>,
    children: Vec<RsxNode>,
) -> RsxNode {
    let theme = use_theme().0;
    let orientation = orientation.unwrap_or(ToggleOrientation::Horizontal);
    let full_width = full_width.unwrap_or(false);
    let disabled = disabled.unwrap_or(false);

    let binding = value.unwrap_or_else(|| {
        global_state::<Option<String>>(|| None).binding()
    });

    let ctx = ToggleButtonGroupContext {
        in_group: true,
        value: binding,
        on_change,
        size,
        color,
        disabled,
    };

    let border_color: Box<dyn ColorLike> = if disabled {
        theme.color.state.disabled.clone()
    } else {
        theme.color.border.clone()
    };

    // Walker-ancestry: flatten fragments + interleave dividers, then wrap
    // the rendered subtree in a Provider node. Walker pushes ctx onto
    // CONTEXT_STACK for the entire descent into `child`, so each
    // ToggleButton sees GroupContext via `use_context`. No per-node
    // snapshot rewrite needed.
    let mut flattened: Vec<RsxNode> = Vec::new();
    for child in children {
        flatten_into(child, &mut flattened);
    }
    let interleaved = interleave_dividers(flattened, orientation, border_color.as_ref());

    let layout = match orientation {
        ToggleOrientation::Horizontal => Layout::flow()
            .row()
            .no_wrap()
            .cross_size(CrossSize::Stretch),
        ToggleOrientation::Vertical => Layout::flow()
            .column()
            .no_wrap()
            .cross_size(CrossSize::Stretch),
    };
    let width = if full_width {
        Some(Length::percent(100.0))
    } else {
        None
    };

    let border = Border::uniform(Length::px(1.0), border_color.as_ref());
    let radius = theme.component.button.toggle_button_radius;

    rsx! {
        <Provider::<ToggleButtonGroupContext> value={ctx}>
            <Element style={{
                width: width,
                layout: layout,
                border: border,
                border_radius: radius,
                position: Position::static_().clip(ClipMode::Parent),
            }}>
                {interleaved}
            </Element>
        </Provider>
    }
}

fn flatten_into(node: RsxNode, out: &mut Vec<RsxNode>) {
    if let RsxNode::Fragment(frag) = &node {
        for child in frag.children.iter().cloned() {
            flatten_into(child, out);
        }
        return;
    }
    out.push(node);
}

fn interleave_dividers(
    nodes: Vec<RsxNode>,
    orientation: ToggleOrientation,
    color: &dyn ColorLike,
) -> Vec<RsxNode> {
    let [r, g, b, a] = color.to_rgba_u8();
    let divider_color = Color::rgba(r, g, b, a);

    let mut out: Vec<RsxNode> = Vec::with_capacity(nodes.len() * 2);
    let mut last_was_button = false;
    for node in nodes {
        let is_btn = is_toggle_button(&node);
        if is_btn && last_was_button {
            out.push(divider_node(orientation, divider_color));
        }
        out.push(node);
        last_was_button = is_btn;
    }
    out
}

fn divider_node(orientation: ToggleOrientation, color: Color) -> RsxNode {
    match orientation {
        ToggleOrientation::Horizontal => rsx! {
            <Element style={{
                width: Length::px(1.0),
                background: color,
            }} />
        },
        ToggleOrientation::Vertical => rsx! {
            <Element style={{
                height: Length::px(1.0),
                background: color,
            }} />
        },
    }
}
