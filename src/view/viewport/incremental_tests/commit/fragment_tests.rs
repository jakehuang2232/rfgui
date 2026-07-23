use super::*;

/// 軌 1 #5: a Fragment-shaped InsertChild expands to N descriptors
/// and commits as `FiberWork::CreateMany` — N consecutive
/// `arena_insert_child` calls. Parent NodeKey survives.
#[test]
fn incremental_commit_applies_fragment_insert_child_creates_many() {
    use crate::view::fiber_work::{DescriptorContext, apply_fiber_works, patch_to_fiber_work};

    // Seed: empty parent. NEW rsx mirror has the same parent +
    // a Fragment child (which itself holds N children) at index 0.
    let seed = host_el();
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&seed).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    assert_eq!(viewport.scene.node_arena.children_of(parent_key).len(), 0);

    // Synthetic patch: insert a Fragment containing two Element
    // children. The translator expands the Fragment into N=2
    // descriptors and emits CreateMany.
    let fragment = RsxNode::fragment(vec![host_el(), host_el()]);
    let new_root = host_el().with_child(fragment.clone());
    let patch = crate::ui::Patch::InsertChild {
        parent_path: vec![],
        index: 0,
        node: fragment,
    };
    let style = crate::style::Style::new();
    let ctx = DescriptorContext {
        new_rsx_root: &new_root,
        old_rsx_root: None,
        inherited_style: &style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    let work = patch_to_fiber_work(
        patch,
        viewport.scene.node_arena.stable_id_index(),
        &viewport.scene.node_arena,
        parent_key,
        Some(&ctx),
    )
    .expect("Fragment InsertChild must translate to CreateMany");
    assert!(work.is_committable(&viewport.scene.node_arena));
    apply_fiber_works(&mut viewport.scene.node_arena, test_apply_ctx(), vec![work])
        .expect("Fragment insert work applies");

    // Parent identity stable; two new children landed in order at
    // indices 0 and 1.
    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    let arena = &viewport.scene.node_arena;
    assert_eq!(arena.children_of(parent_key).len(), 2);
}

/// 軌 A #5 (extends 軌 1 #5): a Fragment new-node in `Patch::ReplaceNode`
/// expands to N descriptors at the replaced slot. The old child
/// subtree is removed and N new keys land in its place.
#[test]
fn incremental_commit_replace_node_with_fragment_expands_to_n_descriptors() {
    use crate::view::fiber_work::{DescriptorContext, apply_fiber_works, patch_to_fiber_work};

    // Seed: parent with two children, snapshot keys.
    let seed = host_el().with_child(host_el()).with_child(host_el());
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&seed).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let kept_child_key = viewport.scene.node_arena.children_of(parent_key)[1];

    // Replace child[0] with a Fragment containing 3 children → 3
    // descriptors. After apply, parent has 4 children: 3 new + 1
    // kept (kept_child_key is now at index 3).
    let fragment = RsxNode::fragment(vec![host_el(), host_el(), host_el()]);
    let new_root = host_el().with_child(fragment.clone()).with_child(host_el());
    let patch = crate::ui::Patch::ReplaceNode {
        path: vec![0],
        node: fragment,
    };
    let style = crate::style::Style::new();
    let ctx = DescriptorContext {
        new_rsx_root: &new_root,
        old_rsx_root: None,
        inherited_style: &style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    let work = patch_to_fiber_work(
        patch,
        viewport.scene.node_arena.stable_id_index(),
        &viewport.scene.node_arena,
        parent_key,
        Some(&ctx),
    )
    .expect("Fragment ReplaceNode must translate");
    assert!(work.is_committable(&viewport.scene.node_arena));
    apply_fiber_works(&mut viewport.scene.node_arena, test_apply_ctx(), vec![work])
        .expect("Fragment replace work applies");

    let arena = &viewport.scene.node_arena;
    let children = arena.children_of(parent_key);
    assert_eq!(children.len(), 4, "3 new + 1 kept");
    assert_eq!(children[3], kept_child_key, "kept sibling now at end");
}

/// Fragment root with N children → arena stores N roots. Re-rendering the
/// same tree must keep every arena root NodeKey stable (per-root reconcile
/// emits zero patches thanks to ptr_eq).
#[test]
fn incremental_commit_fragment_at_root_preserves_all_root_keys_across_identical_render() {
    let tree = RsxNode::fragment(vec![
        single_element(100.0),
        single_element(200.0),
        single_element(300.0),
    ]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&tree).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 3);
    let original = viewport.scene.ui_root_keys.clone();

    viewport.render_rsx(&tree).expect("identical re-render");
    assert_eq!(viewport.scene.ui_root_keys, original);
}

/// Fragment-at-root: changing one child's style prop must keep every
/// arena root NodeKey stable (UpdateElementProps routes via root_index,
/// doesn't rebuild siblings).
#[test]
fn incremental_commit_fragment_at_root_style_update_on_one_child_preserves_all_keys() {
    let first = RsxNode::fragment(vec![
        single_element(100.0),
        single_element(200.0),
        single_element(300.0),
    ]);
    // Only the middle child's width changes.
    let second = RsxNode::fragment(vec![
        single_element(100.0),
        single_element(250.0),
        single_element(300.0),
    ]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 3);
    let original = viewport.scene.ui_root_keys.clone();

    viewport
        .render_rsx(&second)
        .expect("fragment-root child style update must go incremental");

    assert_eq!(viewport.scene.ui_root_keys, original);
}

/// Fragment-at-root arity change (N → M, N != M) must go through the
/// `ReplaceAllRoots` path: arena root count matches the new arity.
/// NodeKeys are expected to be fresh (wholesale swap).
#[test]
fn incremental_commit_fragment_at_root_arity_change_replaces_all_roots() {
    let first = RsxNode::fragment(vec![single_element(100.0), single_element(200.0)]);
    let second = RsxNode::fragment(vec![
        single_element(100.0),
        single_element(200.0),
        single_element(300.0),
    ]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 2);

    viewport
        .render_rsx(&second)
        .expect("fragment-root arity change must commit via ReplaceAllRoots");

    assert_eq!(viewport.scene.ui_root_keys.len(), 3);
}

/// Single Element root → Fragment-at-root swap: identity/shape mismatch
/// triggers `ReplaceAllRoots`. Arena ends with N roots matching the new
/// Fragment's child count.
#[test]
fn incremental_commit_element_root_to_fragment_root_swaps_via_replace_all_roots() {
    let first = single_element(100.0);
    let second = RsxNode::fragment(vec![single_element(150.0), single_element(250.0)]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&first)
        .expect("cold render (single root)");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);

    viewport
        .render_rsx(&second)
        .expect("single-root → fragment-root swap must commit via ReplaceAllRoots");

    assert_eq!(viewport.scene.ui_root_keys.len(), 2);
}

// ---------------------------------------------------------------------------
// rsx_to_arena_path unit tests (Fragment path flattening)
// ---------------------------------------------------------------------------

#[test]
fn rsx_to_arena_path_flattens_mid_tree_fragment() {
    use crate::view::fiber_work::{ArenaPathResolution, rsx_to_arena_path};

    // Element { children: [A, Fragment([B]), C] }
    // B lives at rsx path [1, 0]; arena flattens Fragment, so B's
    // arena path is [1].
    let a = host_el();
    let b = host_el();
    let c = host_el();
    let root = host_el()
        .with_child(a)
        .with_child(RsxNode::fragment(vec![b]))
        .with_child(c);

    assert!(matches!(rsx_to_arena_path(&root, &[0]), ArenaPathResolution::Arena(p) if p == [0]));
    assert!(matches!(rsx_to_arena_path(&root, &[1, 0]), ArenaPathResolution::Arena(p) if p == [1]));
    assert!(matches!(rsx_to_arena_path(&root, &[2]), ArenaPathResolution::Arena(p) if p == [2]));
}

#[test]
fn rsx_to_arena_path_handles_nested_fragments() {
    use crate::view::fiber_work::{ArenaPathResolution, rsx_to_arena_path};

    // Element { children: [A, Fragment([Fragment([B]), C]), D] }
    let root = host_el()
        .with_child(host_el())
        .with_child(RsxNode::fragment(vec![
            RsxNode::fragment(vec![host_el()]),
            host_el(),
        ]))
        .with_child(host_el());

    assert!(matches!(rsx_to_arena_path(&root, &[0]), ArenaPathResolution::Arena(p) if p == [0]));
    assert!(
        matches!(rsx_to_arena_path(&root, &[1, 0, 0]), ArenaPathResolution::Arena(p) if p == [1])
    );
    assert!(matches!(rsx_to_arena_path(&root, &[1, 1]), ArenaPathResolution::Arena(p) if p == [2]));
    assert!(matches!(rsx_to_arena_path(&root, &[2]), ArenaPathResolution::Arena(p) if p == [3]));
}

// ---------------------------------------------------------------------------
// 軌 1 #8 Text::apply_style incremental
// ---------------------------------------------------------------------------
