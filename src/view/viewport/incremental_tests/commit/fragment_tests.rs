use super::*;

use crate::ui::{State, component, use_mount, use_state};

thread_local! {
    static CONDITIONAL_UNMOUNT_MOUNTS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static CONDITIONAL_UNMOUNT_CLEANUPS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static CONDITIONAL_UNMOUNT_OBSERVED_STATE: std::cell::RefCell<Vec<u32>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static CONDITIONAL_UNMOUNT_STATE: std::cell::RefCell<Option<State<u32>>> =
        const { std::cell::RefCell::new(None) };
}

#[component]
fn ConditionalUnmountLifecycleProbe() -> RsxNode {
    let state = use_state(|| 7_u32);
    CONDITIONAL_UNMOUNT_OBSERVED_STATE.with(|observed| observed.borrow_mut().push(state.get()));
    CONDITIONAL_UNMOUNT_STATE.with(|captured| {
        *captured.borrow_mut() = Some(state);
    });
    use_mount(|| {
        CONDITIONAL_UNMOUNT_MOUNTS.with(|mounts| mounts.set(mounts.get() + 1));
        || {
            CONDITIONAL_UNMOUNT_CLEANUPS.with(|cleanups| cleanups.set(cleanups.get() + 1));
        }
    });
    host_el()
}

#[component]
fn ConditionalUnmountLifecycleBoundary(show: bool) -> RsxNode {
    if !show {
        return RsxNode::fragment(vec![]);
    }
    rsx! {
        <HostElement>
            <ConditionalUnmountLifecycleProbe />
        </HostElement>
    }
}

fn conditional_unmount_lifecycle_tree(show: bool) -> RsxNode {
    rsx! {
        <HostElement>
            <HostElement />
            <ConditionalUnmountLifecycleBoundary show={show} />
        </HostElement>
    }
}

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

/// A conditional component can preserve its authored identity while its
/// rendered shape changes from an Element to an empty Fragment. That is an
/// unmount of one child, not a reason to cold-rebuild every viewport root.
#[test]
fn incremental_commit_replace_node_with_empty_fragment_deletes_only_target() {
    use crate::ui::RsxNodeIdentity;

    let conditional_identity = RsxNodeIdentity::new("ConditionalChild", None);
    let mut shown = host_el();
    shown.set_identity(conditional_identity.clone());
    let first = host_el().with_child(shown).with_child(host_el());

    let mut hidden = RsxNode::fragment(vec![]);
    hidden.set_identity(conditional_identity);
    let second = host_el().with_child(hidden).with_child(host_el());

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&first).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let children_before = viewport.scene.node_arena.children_of(parent_key);
    let removed_child_key = children_before[0];
    let kept_child_key = children_before[1];

    viewport
        .render_rsx(&second)
        .expect("empty Fragment replacement must commit as a delete");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![parent_key],
        "parent root must survive instead of being cold rebuilt",
    );
    assert_eq!(
        viewport.scene.node_arena.children_of(parent_key),
        vec![kept_child_key],
        "only the conditional child should be removed",
    );
    assert!(
        viewport.scene.node_arena.get(removed_child_key).is_none(),
        "removed conditional subtree must not remain in the arena",
    );
}

#[test]
fn incremental_empty_fragment_unmount_cleans_effect_and_remounts_fresh_state() {
    CONDITIONAL_UNMOUNT_MOUNTS.with(|mounts| mounts.set(0));
    CONDITIONAL_UNMOUNT_CLEANUPS.with(|cleanups| cleanups.set(0));
    CONDITIONAL_UNMOUNT_OBSERVED_STATE.with(|observed| observed.borrow_mut().clear());
    CONDITIONAL_UNMOUNT_STATE.with(|captured| *captured.borrow_mut() = None);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    let shown = conditional_unmount_lifecycle_tree(true);
    viewport.render_rsx(&shown).expect("initial mount");
    let parent_key = viewport.scene.ui_root_keys[0];
    let children = viewport.scene.node_arena.children_of(parent_key);
    let kept_sibling_key = children[0];
    assert_eq!(children.len(), 2);
    assert_eq!(CONDITIONAL_UNMOUNT_MOUNTS.with(std::cell::Cell::get), 1);
    assert_eq!(CONDITIONAL_UNMOUNT_CLEANUPS.with(std::cell::Cell::get), 0);

    CONDITIONAL_UNMOUNT_STATE.with(|captured| {
        captured
            .borrow()
            .as_ref()
            .expect("mounted probe state")
            .set(41);
        *captured.borrow_mut() = None;
    });

    let hidden = conditional_unmount_lifecycle_tree(false);
    assert_eq!(
        CONDITIONAL_UNMOUNT_CLEANUPS.with(std::cell::Cell::get),
        1,
        "effect cleanup must run when the child component leaves the live build",
    );
    viewport.render_rsx(&hidden).expect("incremental unmount");
    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    assert_eq!(
        viewport.scene.node_arena.children_of(parent_key),
        vec![kept_sibling_key],
    );

    let remounted = conditional_unmount_lifecycle_tree(true);
    assert_eq!(CONDITIONAL_UNMOUNT_MOUNTS.with(std::cell::Cell::get), 2);
    assert_eq!(
        CONDITIONAL_UNMOUNT_OBSERVED_STATE.with(|observed| observed.borrow().clone()),
        vec![7, 7],
        "remount must allocate fresh state instead of reviving the prior value 41",
    );
    viewport
        .render_rsx(&remounted)
        .expect("incremental remount");
    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    assert_eq!(
        viewport.scene.node_arena.children_of(parent_key)[0],
        kept_sibling_key,
        "unrelated sibling must survive the unmount/remount cycle",
    );

    let hidden_again = conditional_unmount_lifecycle_tree(false);
    assert_eq!(CONDITIONAL_UNMOUNT_CLEANUPS.with(std::cell::Cell::get), 2);
    viewport
        .render_rsx(&hidden_again)
        .expect("final incremental unmount");
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
