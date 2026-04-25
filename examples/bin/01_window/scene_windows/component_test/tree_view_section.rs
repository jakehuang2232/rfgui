use std::rc::Rc;

use crate::rfgui::ui::{RsxNode, component, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui::{Layout, Length, Padding};
use crate::rfgui_components::{
    Button, ButtonSize, ButtonVariant, DropPosition, Switch, Theme, TreeMoveEvent, TreeNode,
    TreeView,
};
use rfgui_components::Accordion;

fn default_tree_nodes() -> Vec<TreeNode> {
    let folder = |id: &str, label: &str, children: Vec<TreeNode>| {
        TreeNode::new(id, label)
            .with_icon("folder")
            .with_expanded_icon("folder_open")
            .with_children(children)
    };
    let file = |id: &str, label: &str, icon: &str| TreeNode::new(id, label).with_icon(icon);
    vec![
        folder(
            "src",
            "src/",
            vec![
                folder(
                    "inputs",
                    "inputs/",
                    vec![
                        file("button.rs", "button.rs", "code"),
                        file("checkbox.rs", "checkbox.rs", "code"),
                        file("select.rs", "select.rs", "code"),
                    ],
                ),
                folder(
                    "layout",
                    "layout/",
                    vec![
                        file("accordion.rs", "accordion.rs", "code"),
                        file("tree_view.rs", "tree_view.rs", "code"),
                        file("window.rs", "window.rs", "code"),
                    ],
                ),
                file("lib.rs", "lib.rs", "code"),
                file("theme.rs", "theme.rs", "palette"),
            ],
        ),
        folder(
            "examples",
            "examples/",
            vec![file("readme", "README.md", "description").with_disabled(true)],
        ),
    ]
}

fn collect_ids(nodes: &[TreeNode], out: &mut Vec<String>) {
    for node in nodes {
        out.push(node.value.clone());
        collect_ids(&node.children, out);
    }
}

fn collect_folder_ids(nodes: &[TreeNode], out: &mut Vec<String>) {
    for node in nodes {
        if !node.children.is_empty() {
            out.push(node.value.clone());
            collect_folder_ids(&node.children, out);
        }
    }
}

fn is_descendant_or_self(nodes: &[TreeNode], ancestor: &str, candidate: &str) -> bool {
    for node in nodes {
        if node.value == ancestor {
            if node.value == candidate {
                return true;
            }
            let mut ids = Vec::new();
            collect_ids(&node.children, &mut ids);
            return ids.iter().any(|id| id == candidate);
        }
        if is_descendant_or_self(&node.children, ancestor, candidate) {
            return true;
        }
    }
    false
}

fn remove_by_id(nodes: &mut Vec<TreeNode>, id: &str) -> Option<TreeNode> {
    if let Some(pos) = nodes.iter().position(|n| n.value == id) {
        return Some(nodes.remove(pos));
    }
    for node in nodes.iter_mut() {
        if let Some(removed) = remove_by_id(&mut node.children, id) {
            return Some(removed);
        }
    }
    None
}

fn insert_at(
    nodes: &mut Vec<TreeNode>,
    target: &str,
    position: DropPosition,
    subtree: TreeNode,
) -> Result<(), TreeNode> {
    if let Some(pos) = nodes.iter().position(|n| n.value == target) {
        match position {
            DropPosition::Before => nodes.insert(pos, subtree),
            DropPosition::After => nodes.insert(pos + 1, subtree),
            DropPosition::Inside => nodes[pos].children.push(subtree),
        }
        return Ok(());
    }
    let mut carry = subtree;
    for node in nodes.iter_mut() {
        match insert_at(&mut node.children, target, position.clone(), carry) {
            Ok(()) => return Ok(()),
            Err(returned) => carry = returned,
        }
    }
    Err(carry)
}

fn apply_tree_move(nodes: &mut Vec<TreeNode>, ev: &TreeMoveEvent<String>) -> bool {
    if ev.source == ev.target {
        return false;
    }
    if is_descendant_or_self(nodes, &ev.source, &ev.target) {
        return false;
    }
    let Some(subtree) = remove_by_id(nodes, &ev.source) else {
        return false;
    };
    insert_at(nodes, &ev.target, ev.position.clone(), subtree).is_ok()
}

#[component]
pub fn TreeViewSection(theme: Theme) -> RsxNode {
    let tree_expanded = use_state(|| vec![String::from("src"), String::from("layout")]);
    let tree_selected = use_state(|| Some(String::from("tree_view.rs")));
    let tree_nodes_state = use_state(default_tree_nodes);
    let tree_draggable = use_state(|| true);

    let tree_expand_all = {
        let tree_nodes_state = tree_nodes_state.clone();
        let tree_expanded = tree_expanded.clone();
        move |_: &mut crate::rfgui::ui::ClickEvent| {
            let mut ids = Vec::new();
            collect_folder_ids(&tree_nodes_state.get(), &mut ids);
            tree_expanded.set(ids);
        }
    };
    let tree_collapse_all = {
        let tree_expanded = tree_expanded.clone();
        move |_: &mut crate::rfgui::ui::ClickEvent| tree_expanded.set(Vec::new())
    };
    let tree_clear_selection = {
        let tree_selected = tree_selected.clone();
        move |_: &mut crate::rfgui::ui::ClickEvent| tree_selected.set(None)
    };
    let tree_reset = {
        let tree_nodes_state = tree_nodes_state.clone();
        let tree_expanded = tree_expanded.clone();
        let tree_selected = tree_selected.clone();
        move |_: &mut crate::rfgui::ui::ClickEvent| {
            tree_nodes_state.set(default_tree_nodes());
            tree_expanded.set(vec![String::from("src"), String::from("layout")]);
            tree_selected.set(Some(String::from("tree_view.rs")));
        }
    };
    let tree_on_move: Rc<dyn Fn(TreeMoveEvent<String>)> = {
        let tree_nodes_state = tree_nodes_state.clone();
        Rc::new(move |ev| {
            tree_nodes_state.update(|nodes| {
                apply_tree_move(nodes, &ev);
                fn dump(ns: &[TreeNode], depth: usize) {
                    for n in ns {
                        dump(&n.children, depth + 1);
                    }
                }
                dump(nodes, 0);
            });
        })
    };

    rsx! {
        <Accordion title="Tree View">
            <Element style={{
                layout: Layout::flow().row().no_wrap(),
                gap: theme.spacing.md,
                padding: Padding::uniform(Length::Zero).bottom(theme.spacing.sm),
            }}>
                <Switch label="Draggable" binding={tree_draggable.binding()} />
            </Element>
            <Element style={{
                layout: Layout::flow().row().no_wrap(),
                gap: theme.spacing.sm,
                padding: Padding::uniform(Length::Zero).bottom(theme.spacing.sm),
            }}>
                <Button size={Some(ButtonSize::Small)} on_click={tree_expand_all}>Expand All</Button>
                <Button size={Some(ButtonSize::Small)} on_click={tree_collapse_all}>Collapse All</Button>
                <Button size={Some(ButtonSize::Small)} on_click={tree_clear_selection}>Clear Selection</Button>
                <Button size={Some(ButtonSize::Small)} variant={Some(ButtonVariant::Outlined)} on_click={tree_reset}>Reset</Button>
            </Element>
            <TreeView
                nodes={tree_nodes_state.get()}
                expanded_binding={tree_expanded.binding()}
                selected_binding={tree_selected.binding()}
                on_move={if tree_draggable.get() { Some(tree_on_move) } else { None }}
            />
            <Text style={{ color: theme.color.text.secondary.clone() }}>
                {format!(
                    "expanded={:?} selected={:?}",
                    tree_expanded.get(),
                    tree_selected.get()
                )}
            </Text>
        </Accordion>
    }
}
