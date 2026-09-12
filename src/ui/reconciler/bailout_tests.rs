use super::*;
use crate::ui::{RsxFragmentNode, RsxNodeIdentity};

fn element_with_children(children: Vec<RsxNode>) -> RsxNode {
    RsxNode::Fragment(Rc::new(RsxFragmentNode {
        identity: RsxNodeIdentity::new("Fragment", None),
        children,
    }))
}

#[test]
fn ptr_eq_node_is_skipped_without_patches() {
    let shared = RsxNode::text("hello");
    let old = element_with_children(vec![shared.clone(), RsxNode::text("b")]);
    // Reuse the exact same Rc for the first child; change the second.
    let new = element_with_children(vec![shared, RsxNode::text("B")]);

    let patches = reconcile(Some(&old), &new);
    // Only the second child should produce a patch; the first is bailed
    // out by the `Rc::ptr_eq` fast path in `reconcile_node`.
    assert_eq!(patches.len(), 1);
    assert!(matches!(patches[0], Patch::SetText { .. }));
}

#[test]
fn ptr_eq_whole_tree_yields_no_patches() {
    let tree = element_with_children(vec![RsxNode::text("a"), RsxNode::text("b")]);
    let patches = reconcile(Some(&tree), &tree.clone());
    assert!(patches.is_empty(), "got patches: {patches:?}");
}

#[test]
fn shared_props_rc_skips_prop_diff() {
    use crate::ui::{PropValue, RsxElementNode, RsxElementProps};

    // Build shared props: an `Rc<Vec<_>>` reused across two distinct
    // element allocations. The reconciler must take the `Rc::ptr_eq`
    // fast path and emit NO `UpdateElementProps` patch.
    let shared_props: RsxElementProps = Rc::new(vec![
        ("width", PropValue::I64(100)),
        ("color", PropValue::I64(1)),
    ]);

    let make = |children: Vec<RsxNode>| {
        RsxNode::Element(Rc::new(RsxElementNode {
            identity: RsxNodeIdentity::new("Element", None),
            tag: "Element",
            tag_descriptor: None,
            props: shared_props.clone(),
            children,
        }))
    };

    let old = make(vec![RsxNode::text("a")]);
    let new = make(vec![RsxNode::text("b")]);
    let patches = reconcile(Some(&old), &new);
    // Only SetText for the changed child; no UpdateElementProps.
    assert_eq!(patches.len(), 1, "got patches: {patches:?}");
    assert!(matches!(patches[0], Patch::SetText { .. }));
}
