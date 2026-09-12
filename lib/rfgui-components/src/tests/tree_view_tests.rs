use super::*;

#[test]
fn tree_view_renders_element_root() {
    let tree = rsx! {
        <TreeView nodes={sample_tree_nodes()} />
    };
    let RsxNode::Element(root) = tree else {
        panic!("TreeView should render element root");
    };
    assert_eq!(
        root.tag_descriptor,
        Some(RsxTagDescriptor::of::<TreeView>())
    );
}

#[test]
fn tree_view_collapsed_hides_child_labels() {
    let tree = rsx! {
        <TreeView nodes={sample_tree_nodes()} />
    };
    let mut texts = Vec::new();
    collect_text_nodes(&tree, &mut texts);
    assert!(
        texts.iter().any(|t| t == "Root"),
        "root label missing: {texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t == "Child"),
        "collapsed root should not render Child label: {texts:?}"
    );
}

#[test]
fn tree_view_default_expanded_shows_child_labels() {
    let tree = rsx! {
        <TreeView
            nodes={sample_tree_nodes()}
            default_expanded_items={vec![String::from("root")]}
        />
    };
    let mut texts = Vec::new();
    collect_text_nodes(&tree, &mut texts);
    assert!(texts.iter().any(|t| t == "Root"));
    assert!(
        texts.iter().any(|t| t == "Child"),
        "expanded root should render Child label: {texts:?}"
    );
}

#[test]
fn tree_view_click_toggles_expanded_and_selects() {
    let expanded = global_state(|| Vec::<String>::new());
    let selected = global_state(|| Option::<String>::None);

    let tree = rsx! {
        <TreeView
            nodes={sample_tree_nodes()}
            expanded_binding={expanded.binding()}
            selected_binding={selected.binding()}
        />
    };
    let mut arena = NodeArena::new();
    let roots = commit_rsx_tree_into(&mut arena, &tree);
    let mut viewport = rfgui::view::Viewport::new();
    click_once(&mut arena, roots[0], &mut viewport, 16.0, 14.0);

    assert_eq!(selected.get().as_deref(), Some("root"));
    assert_eq!(expanded.get(), vec![String::from("root")]);
}

/// Repro for the user-reported drag-reorder bug: dragging
/// `accordion.rs` after `tree_view.rs` in `layout/` mutates the
/// nodes prop. The incremental reconciler must reorder rows by
/// keyed identity so each row's nested label travels with it.
#[test]
fn tree_view_reorder_keeps_labels_aligned_with_rows() {
    fn folder_layout(children: Vec<TreeNode>) -> Vec<TreeNode> {
        vec![
            BranchNode::new("src", "src/")
                .with_children(vec![
                    BranchNode::new("layout", "layout/")
                        .with_children(children)
                        .into(),
                ])
                .into(),
        ]
    }

    let pre_drag = folder_layout(vec![
        LeafNode::new("accordion.rs", "accordion.rs").into(),
        LeafNode::new("tree_view.rs", "tree_view.rs").into(),
        LeafNode::new("window.rs", "window.rs").into(),
    ]);
    let post_drag = folder_layout(vec![
        LeafNode::new("tree_view.rs", "tree_view.rs").into(),
        LeafNode::new("accordion.rs", "accordion.rs").into(),
        LeafNode::new("window.rs", "window.rs").into(),
    ]);

    let make_tree = |nodes: Vec<TreeNode>| {
        rsx! {
            <TreeView
                nodes={nodes}
                default_expanded_items={vec![
                    String::from("src"),
                    String::from("layout"),
                ]}
            />
        }
    };

    let mut viewport = rfgui::view::Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&make_tree(pre_drag))
        .expect("cold render");
    viewport
        .render_rsx(&make_tree(post_drag))
        .expect("incremental reorder render");

    // Walk the arena and collect every Text host's content in
    // pre-order so labels show up in their displayed sibling order.
    fn collect(arena: &rfgui::view::NodeArena, key: rfgui::view::NodeKey, out: &mut Vec<String>) {
        if let Some(node) = arena.get(key) {
            if let Some(t) = node
                .element
                .as_any()
                .downcast_ref::<rfgui::view::base_component::Text>()
            {
                out.push(t.content().to_string());
            }
        }
        for child in arena.children_of(key) {
            collect(arena, child, out);
        }
    }
    let arena = viewport.node_arena();
    let mut labels = Vec::new();
    for &root in arena.roots() {
        collect(arena, root, &mut labels);
    }

    let layout_idx = labels
        .iter()
        .position(|s| s == "layout/")
        .expect("layout/ row label present");
    let leaf_window_idx = labels
        .iter()
        .position(|s| s == "window.rs")
        .expect("window.rs row label present");
    let leaves: Vec<&str> = labels[layout_idx + 1..=leaf_window_idx]
        .iter()
        .filter(|s| ["accordion.rs", "tree_view.rs", "window.rs"].contains(&s.as_str()))
        .map(|s| s.as_str())
        .collect();
    assert_eq!(
        leaves,
        vec!["tree_view.rs", "accordion.rs", "window.rs"],
        "after reorder, layout/ leaf labels must match new order; \
         stale labels = keyed-row reconcile bug",
    );
}
