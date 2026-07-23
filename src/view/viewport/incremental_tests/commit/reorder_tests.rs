use super::*;

#[test]
fn incremental_commit_reorders_unkeyed_text_rows_without_duplicate_content() {
    use crate::view::Text as HostText;

    fn tree(labels: &[&str]) -> RsxNode {
        rsx! {
            <HostElement>
                {labels
                    .iter()
                    .map(|label| rsx! { <HostText>{(*label).to_string()}</HostText> })
                    .collect::<Vec<_>>()}
            </HostElement>
        }
    }

    let first = tree(&["window.rs", "accordion.rs", "tree_view.rs"]);
    let second = tree(&["accordion.rs", "window.rs", "tree_view.rs"]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    viewport
        .render_rsx(&second)
        .expect("text sibling reorder should render without duplicates");

    let mut labels = Vec::new();
    collect_text_contents(
        &viewport.scene.node_arena,
        viewport.scene.ui_root_keys[0],
        &mut labels,
    );
    assert_eq!(labels, vec!["accordion.rs", "window.rs", "tree_view.rs"]);
}

/// Regression for the TreeView drag-drop "duplicate row" bug.
///
/// Reconciler emits, for the same parent that's about to reorder via
/// keyed match, a per-child `RemoveChild + InsertChild` (because that
/// row's *internal* shape changed — e.g. a drop-indicator slot
/// switching from `Element` to `Fragment`). The InsertChild path uses
/// the OLD parent-relative index; after the keyed reorder happens
/// above it, walking NEW by that OLD index lands on a different keyed
/// sibling. The translator's `fallback_replace_node_patch` used to
/// blindly take `NEW[old_path]` as the replacement node, clobbering
/// the row at that arena slot with an unrelated row's contents → the
/// later MoveChild then duplicates that wrong content.
#[test]
fn keyed_row_internal_shape_change_plus_reorder_does_not_duplicate() {
    use crate::view::Text as HostText;

    fn row(label: &str, indicator: bool) -> RsxNode {
        let s = label.to_string();
        let inner = rsx! { <HostText>{s.clone()}</HostText> };
        let slot = if indicator {
            rsx! { <HostElement /> }
        } else {
            RsxNode::fragment(vec![])
        };
        rsx! {
            <HostElement key={s.clone()}>
                {inner}
                {slot}
            </HostElement>
        }
    }

    fn tree(rows: Vec<RsxNode>) -> RsxNode {
        rsx! { <HostElement>{rows}</HostElement> }
    }

    // Pre-drop snapshot the reconciler will diff against: order [A, B, C],
    // row "B" is showing the indicator slot as Element.
    let first = tree(vec![row("A", false), row("B", true), row("C", false)]);
    // Post-drop: order [B, A, C] (keyed reorder above), AND B's
    // indicator slot collapses back to Fragment.
    let second = tree(vec![row("B", false), row("A", false), row("C", false)]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    viewport
        .render_rsx(&second)
        .expect("reorder + shape-change must commit cleanly");

    let mut labels = Vec::new();
    collect_text_contents(
        &viewport.scene.node_arena,
        viewport.scene.ui_root_keys[0],
        &mut labels,
    );
    assert_eq!(
        labels,
        vec!["B", "A", "C"],
        "labels must follow keyed reorder; duplicates here mean \
         fallback ReplaceNode clobbered an arena slot whose OLD/NEW \
         identity diverged because of the surrounding keyed shuffle",
    );
}

// ---------------------------------------------------------------------------
// M4 #1: non-additive replace_style
// ---------------------------------------------------------------------------

/// Appending a child to a parent that already has one should commit
/// incrementally as a single `FiberWork::Create`: the existing
/// child's NodeKey survives (no full rebuild), the parent gains one
/// child, and the newly-authored child is parented to the same key.
#[test]
fn incremental_commit_inserts_appended_child_preserves_sibling_keys() {
    let child_a = host_el();

    // Parent with one child vs parent with two children, sharing child_a
    // as the stable first child (identity-matched by the reconciler).
    let parent_with_one = host_el().with_child(child_a.clone());
    let parent_with_two = host_el().with_child(child_a.clone()).with_child(host_el());

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&parent_with_one).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let parent_key = viewport.scene.ui_root_keys[0];
    let existing_child_key = {
        let arena = &viewport.scene.node_arena;
        let children = arena.children_of(parent_key);
        assert_eq!(children.len(), 1);
        children[0]
    };

    viewport
        .render_rsx(&parent_with_two)
        .expect("insert-child render should commit incrementally");

    // Parent identity survives — the incremental path didn't rebuild.
    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![parent_key],
        "parent NodeKey must survive InsertChild translation",
    );

    let arena = &viewport.scene.node_arena;
    let children = arena.children_of(parent_key);
    assert_eq!(
        children.len(),
        2,
        "InsertChild should grow the parent's child list by one",
    );
    assert_eq!(
        children[0], existing_child_key,
        "existing child NodeKey must be preserved at its original index",
    );
    // New child has a different key (arena slot) and lives under parent.
    assert_ne!(children[1], existing_child_key);
    let new_child_node = arena.get(children[1]).expect("new child node");
    assert_eq!(new_child_node.parent, Some(parent_key));
}

// ---------------------------------------------------------------------------
// M6 boundary: text-cascading style updates must fall back when
// descendants exist
// ---------------------------------------------------------------------------
