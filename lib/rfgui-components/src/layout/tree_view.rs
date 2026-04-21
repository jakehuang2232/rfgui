//! `TreeView` — data-driven hierarchical disclosure list.
//!
//! Inspired by MUI X `SimpleTreeView` / `TreeItem` (MIT-licensed source),
//! but reshaped around a `TreeNode<V>` data tree rather than `<TreeItem>`
//! composition. The composition shape would require pushing context across
//! pre-built lazy `RsxNode::Component` children — and the walker's
//! `with_installed_context_snapshot` replaces the stack rather than merging
//! into it, so values pushed by an ancestor `<Provider>` do not reach
//! children that were constructed before the provider was walked.
//!
//! The data shape side-steps that limitation entirely: every row is rendered
//! by `TreeView` itself, so all the per-item state (`expanded`, `selected`,
//! click handlers) is wired up from one render scope.
//!
//! `V` is the per-node value type used for selection + the expanded-set.
//! Defaults to `String`, so the common path stays terse:
//!
//! ```ignore
//! let nodes = vec![
//!     TreeNode::new("root", "Root").with_children(vec![
//!         TreeNode::new("child-a", "Child A"),
//!         TreeNode::new("child-b", "Child B").with_children(vec![
//!             TreeNode::new("leaf", "Leaf"),
//!         ]),
//!     ]),
//! ];
//! rsx! {
//!     <TreeView
//!         nodes={nodes}
//!         default_expanded_items={vec![String::from("root")]}
//!     />
//! }
//! ```
//!
//! Use a custom `V` when string ids would lose type info — e.g. an enum:
//!
//! ```ignore
//! #[derive(Clone, PartialEq)]
//! enum NavTarget { Inbox, Drafts, Sent }
//! rsx! {
//!     <TreeView::<NavTarget>
//!         nodes={vec![TreeNode::new(NavTarget::Inbox, "Inbox")]}
//!     />
//! }
//! ```

use std::marker::PhantomData;

use crate::{ChevronRightIcon, MaterialSymbolIcon, use_theme};
use rfgui::ui::{
    Binding, ClickHandlerProp, RsxComponent, RsxNode, component, props, rsx, use_state,
};
use rfgui::view::{Element, Text};
use rfgui::{
    Align, Angle, Border, Color, ColorLike, Cursor, Layout, Length, Padding, Rotate, Transform,
    Transition, TransitionProperty, flex,
};

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

/// One row in a [`TreeView`].
///
/// `V` is the per-node value type — used as the identity in the expanded
/// set and as the selected value. Must be `Clone + PartialEq + 'static`.
///
/// `icon` / `expanded_icon` are Material Symbols ligatures (e.g. `"folder"`,
/// `"folder_open"`, `"description"`). When both are set, `expanded_icon`
/// shows while the row is expanded — handy for folder open/closed pairs.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeNode<V = String> {
    pub value: V,
    pub label: String,
    pub disabled: bool,
    pub icon: Option<String>,
    pub expanded_icon: Option<String>,
    pub children: Vec<TreeNode<V>>,
}

impl<V> TreeNode<V> {
    pub fn new(value: impl Into<V>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            disabled: false,
            icon: None,
            expanded_icon: None,
            children: Vec::new(),
        }
    }

    pub fn with_children(mut self, children: Vec<TreeNode<V>>) -> Self {
        self.children = children;
        self
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Material Symbols ligature for the resting / collapsed state.
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Material Symbols ligature shown when the row is expanded. Falls back
    /// to [`Self::with_icon`] when unset.
    pub fn with_expanded_icon(mut self, icon: impl Into<String>) -> Self {
        self.expanded_icon = Some(icon.into());
        self
    }
}

// ---------------------------------------------------------------------------
// TreeView
// ---------------------------------------------------------------------------

pub struct TreeView<V = String>(PhantomData<V>)
where
    V: 'static;

#[derive(Clone)]
#[props]
pub struct TreeViewProps<V: 'static> {
    pub nodes: Vec<TreeNode<V>>,
    /// Initial expanded `value` set when no `expanded_binding` is provided.
    pub default_expanded_items: Option<Vec<V>>,
    /// External binding that owns the expanded `value` set.
    pub expanded_binding: Option<Binding<Vec<V>>>,
    /// Initial selected `value` when no `selected_binding` is provided.
    pub default_selected_item: Option<V>,
    /// External binding that owns the currently selected `value`.
    pub selected_binding: Option<Binding<Option<V>>>,
}

impl<V> RsxComponent<TreeViewProps<V>> for TreeView<V>
where
    V: Clone + PartialEq + std::hash::Hash + 'static,
{
    fn render(props: TreeViewProps<V>, _children: Vec<RsxNode>) -> RsxNode {
        rsx! {
            <TreeViewView::<V>
                nodes={props.nodes}
                default_expanded_items={props.default_expanded_items}
                expanded_binding={props.expanded_binding}
                default_selected_item={props.default_selected_item}
                selected_binding={props.selected_binding}
            />
        }
    }
}

#[rfgui::ui::component]
impl<V> rfgui::ui::RsxTag for TreeView<V>
where
    V: Clone + PartialEq + std::hash::Hash + 'static,
{
    type Props = __TreeViewPropsInit<V>;
    type StrictProps = TreeViewProps<V>;
    const ACCEPTS_CHILDREN: bool = false;

    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<rfgui::ui::RsxNode>,
        _key: Option<rfgui::ui::RsxKey>,
    ) -> rfgui::ui::RsxNode {
        <Self as RsxComponent<TreeViewProps<V>>>::render(props, children)
    }
}

const TREE_ITEM_BASE_PAD_LEFT_PX: f32 = 8.0;
const TREE_ITEM_INDENT_PX: f32 = 16.0;
const TREE_ITEM_ROW_HEIGHT_PX: f32 = 28.0;
const TREE_ITEM_ICON_SLOT_PX: f32 = 18.0;

#[component]
fn TreeViewView<V: Clone + PartialEq + std::hash::Hash + 'static>(
    nodes: Vec<TreeNode<V>>,
    default_expanded_items: Option<Vec<V>>,
    expanded_binding: Option<Binding<Vec<V>>>,
    default_selected_item: Option<V>,
    selected_binding: Option<Binding<Option<V>>>,
) -> RsxNode {
    let theme = use_theme().0;

    let fallback_expanded = use_state(|| default_expanded_items.clone().unwrap_or_default());
    let expanded = expanded_binding.unwrap_or_else(|| fallback_expanded.binding());

    let fallback_selected = use_state(|| default_selected_item.clone());
    let selected = selected_binding.unwrap_or_else(|| fallback_selected.binding());

    let expanded_set = expanded.get();
    let selected_value = selected.get();

    let mut row_nodes: Vec<RsxNode> = Vec::new();
    for node in &nodes {
        emit_rows(
            node,
            0,
            &expanded_set,
            selected_value.as_ref(),
            &expanded,
            &selected,
            &mut row_nodes,
        );
    }

    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            layout: Layout::flow().column().no_wrap(),
            background: theme.color.layer.surface.clone(),
        }}>
            {row_nodes}
        </Element>
    }
}

fn emit_rows<V: Clone + PartialEq + std::hash::Hash + 'static>(
    node: &TreeNode<V>,
    depth: usize,
    expanded_set: &[V],
    selected_value: Option<&V>,
    expanded_binding: &Binding<Vec<V>>,
    selected_binding: &Binding<Option<V>>,
    out: &mut Vec<RsxNode>,
) {
    let is_expanded = expanded_set.iter().any(|x| x == &node.value);
    let is_selected = selected_value.map(|s| s == &node.value).unwrap_or(false);

    out.push(render_row(
        node,
        depth,
        is_expanded,
        is_selected,
        expanded_binding.clone(),
        selected_binding.clone(),
    ));

    if is_expanded {
        for child in &node.children {
            emit_rows(
                child,
                depth + 1,
                expanded_set,
                selected_value,
                expanded_binding,
                selected_binding,
                out,
            );
        }
    }
}

fn render_row<V: Clone + PartialEq + std::hash::Hash + 'static>(
    node: &TreeNode<V>,
    depth: usize,
    is_expanded: bool,
    is_selected: bool,
    expanded_binding: Binding<Vec<V>>,
    selected_binding: Binding<Option<V>>,
) -> RsxNode {
    let theme = use_theme().0;

    let value = node.value.clone();
    let label = node.label.clone();
    let disabled = node.disabled;
    let has_children = !node.children.is_empty();

    let value_for_click = value.clone();
    let click = ClickHandlerProp::new(move |_event| {
        if disabled {
            return;
        }
        selected_binding.set(Some(value_for_click.clone()));
        let mut next = expanded_binding.get();
        if let Some(pos) = next.iter().position(|x| x == &value_for_click) {
            next.remove(pos);
        } else {
            next.push(value_for_click.clone());
        }
        expanded_binding.set(next);
    });

    let row_pad_left = Length::px(TREE_ITEM_BASE_PAD_LEFT_PX + (depth as f32) * TREE_ITEM_INDENT_PX);

    let row_background: Box<dyn ColorLike> = if disabled {
        Box::new(Color::transparent())
    } else if is_selected {
        theme.color.state.active.clone()
    } else {
        Box::new(Color::transparent())
    };

    let row_hover_background: Box<dyn ColorLike> = if disabled {
        Box::new(Color::transparent())
    } else if is_selected {
        theme.color.state.active.clone()
    } else {
        theme.color.state.hover.clone()
    };

    let row_text_color = if disabled {
        theme.color.text.disabled.clone()
    } else {
        theme.color.text.primary.clone()
    };

    let chevron_color: Box<dyn ColorLike> = if disabled {
        theme.color.text.disabled.clone()
    } else {
        theme.color.text.secondary.clone()
    };

    let chevron_slot = if has_children {
        rsx! {
            <Element style={{
                width: Length::px(TREE_ITEM_ICON_SLOT_PX),
                height: Length::px(TREE_ITEM_ICON_SLOT_PX),
                layout: Layout::flex().align(Align::Center),
                color: chevron_color.clone(),
                transition: [
                    Transition::new(
                        TransitionProperty::Transform,
                        theme.motion.duration.fast,
                    )
                    .ease_in_out(),
                ],
                transform: if is_expanded {
                    Transform::new([Rotate::z(Angle::deg(90.0))])
                } else {
                    Transform::new([Rotate::z(Angle::deg(0.0))])
                },
            }}>
                <ChevronRightIcon style={{
                    font_size: theme.typography.size.md,
                    color: chevron_color.clone(),
                }} />
            </Element>
        }
    } else {
        rsx! {
            <Element style={{
                width: Length::px(TREE_ITEM_ICON_SLOT_PX),
                height: Length::px(TREE_ITEM_ICON_SLOT_PX),
            }} />
        }
    };

    // Resolve which Material Symbols ligature to render. `expanded_icon`
    // wins when expanded; otherwise fall back to `icon`.
    let active_icon_ligature: Option<String> = if is_expanded {
        node.expanded_icon
            .clone()
            .or_else(|| node.icon.clone())
    } else {
        node.icon.clone()
    };

    let icon_color: Box<dyn ColorLike> = if disabled {
        theme.color.text.disabled.clone()
    } else {
        theme.color.text.secondary.clone()
    };

    let icon_slot = match active_icon_ligature {
        Some(ligature) => rsx! {
            <Element style={{
                width: Length::px(TREE_ITEM_ICON_SLOT_PX),
                height: Length::px(TREE_ITEM_ICON_SLOT_PX),
                layout: Layout::flex().align(Align::Center),
            }}>
                <MaterialSymbolIcon style={{
                    font_size: theme.typography.size.md,
                    color: icon_color.clone(),
                }}>
                    {ligature}
                </MaterialSymbolIcon>
            </Element>
        },
        None => RsxNode::fragment(vec![]),
    };

    rsx! {
        <Element
            // React-style key: identity = node.value. Sibling reorder /
            // insert / delete keep row state aligned via reconciliation.
            key={value.clone()}
            style={{
                width: Length::percent(100.0),
                height: Length::px(TREE_ITEM_ROW_HEIGHT_PX),
                layout: Layout::flex().align(Align::Center),
                padding: Padding::uniform(Length::Zero)
                    .left(row_pad_left)
                    .right(theme.spacing.sm),
                gap: theme.spacing.sm,
                background: row_background,
                cursor: if disabled { Cursor::Default } else { Cursor::Pointer },
                border: Border::uniform(Length::Zero, theme.color.border.as_ref()),
                transition: [
                    Transition::new(
                        TransitionProperty::BackgroundColor,
                        theme.motion.duration.fast,
                    )
                    .ease_in_out(),
                ],
                hover: {
                    background: row_hover_background,
                },
            }}
            on_click={click}
        >
            {chevron_slot}
            {icon_slot}
            <Element style={{
                flex: flex().grow(1.0).shrink(1.0),
                color: row_text_color.clone(),
                font_size: theme.typography.size.sm,
            }}>
                <Text>{label}</Text>
            </Element>
        </Element>
    }
}
