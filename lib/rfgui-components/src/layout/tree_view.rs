//! `TreeView` — data-driven hierarchical disclosure list.
//!
//! Inspired by MUI X `SimpleTreeView` / `TreeItem` (MIT-licensed source),
//! but reshaped around a `TreeNode<V>` data tree rather than `<TreeItem>`
//! composition. The data shape keeps per-item state (`expanded`, `selected`,
//! click handlers) wired up from one render scope, making the component
//! testable without a full viewport and making selection / expansion
//! semantics easy to reason about.
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

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::{ChevronRightIcon, MaterialSymbolIcon, use_theme};
use rfgui::ui::{
    Binding, ClickHandlerProp, DragEffect, RsxComponent, RsxNode, component, on_drag_end,
    on_drag_leave, on_drag_over, on_drag_start, on_drop, on_pointer_down, on_pointer_move,
    on_pointer_up, props, rsx, use_state,
};
use rfgui::view::{Element, Text};
use rfgui::{
    Align, Angle, Border, Color, ColorLike, Cursor, Layout, Length, Padding, Position, Rotate,
    TextWrap, Transform, Transition, TransitionProperty, flex,
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
// Drag & drop
// ---------------------------------------------------------------------------

/// Where a drop lands relative to the target row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropPosition {
    /// Insert as a sibling *above* the target.
    Before,
    /// Insert as a child of the target (only emitted when the target row
    /// has children or is the source's accepted container).
    Inside,
    /// Insert as a sibling *below* the target.
    After,
}

/// Payload passed to `on_move` when a drop completes.
#[derive(Clone, Debug)]
pub struct TreeMoveEvent<V> {
    pub source: V,
    pub target: V,
    pub position: DropPosition,
}

const DRAG_THRESHOLD_PX: f32 = 4.0;

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
    /// When set, rows become draggable. Fires after a successful drop
    /// with `{ source, target, position }`. The component does not
    /// mutate `nodes` itself — the host is responsible for computing
    /// the new tree and re-rendering.
    pub on_move: Option<Rc<dyn Fn(TreeMoveEvent<V>)>>,
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
                on_move={props.on_move}
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
    on_move: Option<Rc<dyn Fn(TreeMoveEvent<V>)>>,
) -> RsxNode {
    let theme = use_theme().0;

    let fallback_expanded = use_state(|| default_expanded_items.clone().unwrap_or_default());
    let expanded = expanded_binding.unwrap_or_else(|| fallback_expanded.binding());

    let fallback_selected = use_state(|| default_selected_item.clone());
    let selected = selected_binding.unwrap_or_else(|| fallback_selected.binding());

    // DnD state. `pending_drag` and `dragging` are non-reactive cells —
    // mutating them must NOT trigger a rebuild, otherwise the rebuild
    // mid-drag invalidates the pointer-down session before the threshold
    // check can fire `start_drag`. Only `drop_target` is reactive because
    // its value drives the indicator paint.
    let pending_drag: Rc<RefCell<Option<(V, f32, f32)>>> =
        use_state(|| Rc::new(RefCell::new(None::<(V, f32, f32)>))).get();
    let dragging: Rc<RefCell<Option<V>>> = use_state(|| Rc::new(RefCell::new(None::<V>))).get();
    let drop_target = use_state(|| None::<(V, DropPosition)>).binding();

    let expanded_set = expanded.get();
    let selected_value = selected.get();
    let drop_target_value = drop_target.get();
    let drag_enabled = on_move.is_some();

    let mut row_nodes: Vec<RsxNode> = Vec::new();
    for node in &nodes {
        emit_rows(
            node,
            0,
            &[],
            &expanded_set,
            selected_value.as_ref(),
            drop_target_value.as_ref(),
            &expanded,
            &selected,
            &pending_drag,
            &dragging,
            &drop_target,
            on_move.as_ref(),
            drag_enabled,
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
    ancestor_values: &[V],
    expanded_set: &[V],
    selected_value: Option<&V>,
    drop_target_value: Option<&(V, DropPosition)>,
    expanded_binding: &Binding<Vec<V>>,
    selected_binding: &Binding<Option<V>>,
    pending_drag_cell: &Rc<RefCell<Option<(V, f32, f32)>>>,
    dragging_cell: &Rc<RefCell<Option<V>>>,
    drop_target_binding: &Binding<Option<(V, DropPosition)>>,
    on_move: Option<&Rc<dyn Fn(TreeMoveEvent<V>)>>,
    drag_enabled: bool,
    out: &mut Vec<RsxNode>,
) {
    let is_expanded = expanded_set.iter().any(|x| x == &node.value);
    let is_selected = selected_value.map(|s| s == &node.value).unwrap_or(false);
    let row_drop_position = drop_target_value
        .filter(|(v, _)| v == &node.value)
        .map(|(_, p)| p.clone());

    out.push(render_row(
        node,
        depth,
        ancestor_values.to_vec(),
        is_expanded,
        is_selected,
        row_drop_position,
        expanded_binding.clone(),
        selected_binding.clone(),
        pending_drag_cell.clone(),
        dragging_cell.clone(),
        drop_target_binding.clone(),
        on_move.cloned(),
        drag_enabled,
    ));

    if is_expanded {
        let mut child_ancestors = ancestor_values.to_vec();
        child_ancestors.push(node.value.clone());
        for child in &node.children {
            emit_rows(
                child,
                depth + 1,
                &child_ancestors,
                expanded_set,
                selected_value,
                drop_target_value,
                expanded_binding,
                selected_binding,
                pending_drag_cell,
                dragging_cell,
                drop_target_binding,
                on_move,
                drag_enabled,
                out,
            );
        }
    }
}

fn render_row<V: Clone + PartialEq + std::hash::Hash + 'static>(
    node: &TreeNode<V>,
    depth: usize,
    ancestor_values: Vec<V>,
    is_expanded: bool,
    is_selected: bool,
    row_drop_position: Option<DropPosition>,
    expanded_binding: Binding<Vec<V>>,
    selected_binding: Binding<Option<V>>,
    pending_drag_cell: Rc<RefCell<Option<(V, f32, f32)>>>,
    dragging_cell: Rc<RefCell<Option<V>>>,
    drop_target_binding: Binding<Option<(V, DropPosition)>>,
    on_move: Option<Rc<dyn Fn(TreeMoveEvent<V>)>>,
    drag_enabled: bool,
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

    let row_pad_left =
        Length::px(TREE_ITEM_BASE_PAD_LEFT_PX + (depth as f32) * TREE_ITEM_INDENT_PX);

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
        node.expanded_icon.clone().or_else(|| node.icon.clone())
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

    // Drop indicator rendered as an absolutely-positioned overlay child so
    // it never affects the row's layout (border / box-shadow alternatives
    // either shift sibling content or are clipped by the parent's flex
    // axis). The overlay is added as the last child below.
    let drop_indicator: RsxNode = match &row_drop_position {
        Some(DropPosition::Before) => rsx! {
            <Element style={{
                position: Position::absolute()
                    .top(Length::Zero)
                    .left(Length::Zero)
                    .right(Length::Zero),
                height: Length::px(2.0),
                background: theme.color.primary.base.clone(),
            }} />
        },
        Some(DropPosition::After) => rsx! {
            <Element style={{
                position: Position::absolute()
                    .bottom(Length::Zero)
                    .left(Length::Zero)
                    .right(Length::Zero),
                height: Length::px(2.0),
                background: theme.color.primary.base.clone(),
            }} />
        },
        Some(DropPosition::Inside) => rsx! {
            <Element style={{
                position: Position::absolute()
                    .top(Length::Zero)
                    .left(Length::Zero)
                    .right(Length::Zero)
                    .bottom(Length::Zero),
                border: Border::uniform(Length::px(2.0), theme.color.primary.base.as_ref()),
            }} />
        },
        None => RsxNode::fragment(vec![]),
    };

    // --- DnD handlers -------------------------------------------------------
    // Attached unconditionally; they early-return when `drag_enabled` is
    // false so callers that don't pass `on_move` get a plain TreeView.
    //
    // `pending_drag_cell` and `dragging_cell` are non-reactive — mutating
    // them does NOT trigger a rebuild. This is critical: `pointer_down` →
    // `pointer_move` → `start_drag` is a continuous gesture; rebuilding
    // mid-gesture invalidates the pointer-down session and loses the drag.

    let pointer_down = {
        let pending = pending_drag_cell.clone();
        let value = value.clone();
        on_pointer_down(move |event| {
            if !drag_enabled || disabled {
                return;
            }
            *pending.borrow_mut() = Some((
                value.clone(),
                event.pointer.viewport_x,
                event.pointer.viewport_y,
            ));
        })
    };

    let pointer_move = {
        let pending = pending_drag_cell.clone();
        let value = value.clone();
        on_pointer_move(move |event| {
            if !drag_enabled {
                return;
            }
            let snapshot = pending.borrow().clone();
            let Some((pending_value, start_x, start_y)) = snapshot else {
                return;
            };
            if pending_value != value {
                return;
            }
            let dx = event.pointer.viewport_x - start_x;
            let dy = event.pointer.viewport_y - start_y;
            if (dx * dx + dy * dy).sqrt() < DRAG_THRESHOLD_PX {
                return;
            }
            *pending.borrow_mut() = None;
            let source_id = event.meta.target_id();
            event
                .viewport
                .start_drag(source_id, Vec::new(), DragEffect::Move);
        })
    };

    let pointer_up = {
        let pending = pending_drag_cell.clone();
        on_pointer_up(move |_event| {
            *pending.borrow_mut() = None;
        })
    };

    let drag_start = {
        let dragging = dragging_cell.clone();
        let value = value.clone();
        on_drag_start(move |_event| {
            *dragging.borrow_mut() = Some(value.clone());
        })
    };

    let drag_over = {
        let dragging = dragging_cell.clone();
        let drop_target = drop_target_binding.clone();
        let value = value.clone();
        let ancestor_values = ancestor_values.clone();
        on_drag_over(move |event| {
            if disabled {
                drop_target.set(None);
                return;
            }
            let source = dragging.borrow().clone();
            let Some(source) = source else {
                return;
            };
            if source == value {
                // Reject dropping onto self.
                drop_target.set(None);
                return;
            }
            if ancestor_values.iter().any(|ancestor| ancestor == &source) {
                // Reject dropping an ancestor into its own descendant.
                drop_target.set(None);
                return;
            }
            let y = event.pointer.local_y;
            let h = TREE_ITEM_ROW_HEIGHT_PX;
            // Directory rows split into 3 equal zones so the "drop as last
            // sibling" and "drop into the directory" zones are both
            // distinct and big enough to hit. Leaves only need 2 zones.
            let position = if has_children {
                if y < h * (1.0 / 3.0) {
                    DropPosition::Before
                } else if y > h * (2.0 / 3.0) {
                    DropPosition::After
                } else {
                    DropPosition::Inside
                }
            } else if y < h * 0.5 {
                DropPosition::Before
            } else {
                DropPosition::After
            };
            let next = Some((value.clone(), position));
            if drop_target.get() != next {
                drop_target.set(next);
            }
            event.accept(DragEffect::Move);
        })
    };

    let drag_leave = {
        let drop_target = drop_target_binding.clone();
        let value = value.clone();
        on_drag_leave(move |_event| {
            if drop_target
                .get()
                .as_ref()
                .is_some_and(|(target, _)| target == &value)
            {
                drop_target.set(None);
            }
        })
    };

    let drop_handler = {
        let dragging = dragging_cell.clone();
        let drop_target = drop_target_binding.clone();
        let pending = pending_drag_cell.clone();
        let on_move_cb = on_move.clone();
        let value = value.clone();
        on_drop(move |_event| {
            let source = dragging.borrow().clone();
            let target = drop_target.get();
            *dragging.borrow_mut() = None;
            *pending.borrow_mut() = None;
            drop_target.set(None);
            let (Some(src), Some((tgt, pos))) = (source, target) else {
                return;
            };
            if tgt != value || src == tgt {
                return;
            }
            if let Some(cb) = on_move_cb.as_ref() {
                cb(TreeMoveEvent {
                    source: src,
                    target: tgt,
                    position: pos,
                });
            }
        })
    };

    let drag_end = {
        let dragging = dragging_cell.clone();
        let drop_target = drop_target_binding.clone();
        let pending = pending_drag_cell.clone();
        on_drag_end(move |_event| {
            *dragging.borrow_mut() = None;
            *pending.borrow_mut() = None;
            drop_target.set(None);
        })
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
            on_pointer_down={pointer_down}
            on_pointer_move={pointer_move}
            on_pointer_up={pointer_up}
            on_drag_start={drag_start}
            on_drag_over={drag_over}
            on_drag_leave={drag_leave}
            on_drop={drop_handler}
            on_drag_end={drag_end}
        >
            {chevron_slot}
            {icon_slot}
            <Element style={{
                flex: flex().grow(1.0).shrink(1.0),
                min_width: Length::Zero,
                color: row_text_color.clone(),
                font_size: theme.typography.size.sm,
            }}>
                <Text style={{ text_wrap: TextWrap::NoWrap }}>{label}</Text>
            </Element>
            {drop_indicator}
        </Element>
    }
}
