use super::*;

/// Structure-identical re-render: reconcile produces an empty patch
/// list, and the incremental path must commit zero works while keeping
/// arena root NodeKeys intact.
#[test]
fn incremental_commit_preserves_node_key_across_identical_render() {
    // Build the tree once and render the same `RsxNode` twice. The
    // reconciler's `ptr_eq` fast-path short-circuits prop-diffing (the
    // `Style` prop is an `Rc`-backed `Shared` value that otherwise
    // compares by pointer), producing an empty patch list. Under M2
    // that is the canonical case the incremental path must handle:
    // zero works committed, NodeKey untouched.
    let tree = single_element(120.0);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&tree)
        .expect("cold render should fall back to full rebuild and succeed");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&tree)
        .expect("identical re-render should succeed on incremental path");

    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    assert_eq!(
        viewport.scene.ui_root_keys[0], original_key,
        "NodeKey must be stable across an identical incremental render",
    );
}

/// When the incremental path can't handle a change (here: a prop
/// update, which translates to `FiberWork::Update` — not
/// M2-committable), the flow must fall back to the full-rebuild
/// pipeline. Under the current legacy path an identity-preserving
/// rebuild can still mint a fresh NodeKey; we only assert the render
/// succeeds and the arena still holds a single root.
#[test]
fn incremental_commit_falls_back_on_non_committable_work() {
    let first = single_element(120.0);
    let second = single_element(160.0);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&first)
        .expect("cold render should succeed");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);

    viewport
        .render_rsx(&second)
        .expect("prop-change render must fall back and still succeed");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
}

/// Remove-a-child: reconcile emits a single `Patch::RemoveChild`,
/// which translates to `FiberWork::Delete` — committable under M2.
/// The parent's NodeKey must survive the incremental commit, the
/// removed child's stable id must be cleared from the index, and the
/// parent's arena child list must shrink by one.
#[test]
fn incremental_commit_deletes_child_without_rebuilding_parent() {
    let child_a = host_el();
    let child_b = host_el();

    // Both parents share the same child identities so reconcile's
    // match phase pairs them up and only the surplus child drops.
    let parent_with_two = host_el()
        .with_child(child_a.clone())
        .with_child(child_b.clone());
    let parent_with_one = host_el().with_child(child_a.clone());

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&parent_with_two).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let parent_key = viewport.scene.ui_root_keys[0];
    let arena = &viewport.scene.node_arena;
    let children_before = arena.children_of(parent_key);
    assert_eq!(children_before.len(), 2);
    let kept_child_key = children_before[0];

    viewport
        .render_rsx(&parent_with_one)
        .expect("delete-child render should commit incrementally");

    // Parent and surviving child must keep their keys — this is the
    // core identity-preservation guarantee M2 ships.
    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    let arena_after = &viewport.scene.node_arena;
    let children_after = arena_after.children_of(parent_key);
    assert_eq!(children_after, vec![kept_child_key]);
}

/// 軌 1 #1: A root-type swap emits `Patch::ReplaceRoot`. The
/// incremental path now builds a descriptor from the new RSX via the
/// shared `DescriptorContext` + `rsx_to_descriptors_with_inherited`
/// pipeline, drops the old subtree, and commits the new one as the
/// sole root — without the full-rebuild fallback ever firing.
#[test]
fn incremental_commit_applies_replace_root() {
    let first = single_element(120.0);
    let second = text_leaf("hello");

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&first)
        .expect("cold render should succeed");
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&second)
        .expect("ReplaceRoot must commit incrementally");

    // Root replaced — a new NodeKey is expected (the new element is a
    // text host, not an Element) but `ui_root_keys` must still be a
    // single entry pointing at the freshly-committed arena slot.
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let new_key = viewport.scene.ui_root_keys[0];
    assert_ne!(
        new_key, original_key,
        "ReplaceRoot swaps the arena slot — new NodeKey expected",
    );
    // Old slot is gone; arena must not leak it.
    assert!(
        viewport.scene.node_arena.get(original_key).is_none(),
        "old root slot must be removed after ReplaceRoot commit",
    );
}

/// 軌 1 #1: `Patch::ReplaceNode` (mid-tree type change) commits
/// incrementally via the apply-side `arena_replace_child`. The
/// reconciler only emits `ReplaceNode` when the child-match step
/// pairs two children whose inner variant or tag then differs —
/// which, given identity keys invocation_type + key, is rare in
/// natural RSX. We exercise the path directly by constructing the
/// patch and feeding it through the translator + applier.
#[test]
fn incremental_commit_replace_node_rebuilds_child_preserves_parent_key() {
    use crate::style::Style;
    use crate::view::fiber_work::{DescriptorContext, apply_fiber_works, patch_to_fiber_work};

    // Seed: parent with two children. Snapshot keys before we mutate.
    let seed = host_el().with_child(host_el()).with_child(host_el());
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&seed).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let kept_child_key = viewport.scene.node_arena.children_of(parent_key)[1];
    let old_first_key = viewport.scene.node_arena.children_of(parent_key)[0];

    // Build a synthetic ReplaceNode at path [0] — swap the first
    // child for a text leaf. New rsx root mirrors the same parent
    // structure so `walk_rsx_by_index_path` and resolve_path line up.
    let new_root = host_el()
        .with_child(text_leaf("swapped"))
        .with_child(host_el());
    let patch = crate::ui::Patch::ReplaceNode {
        path: vec![0],
        node: text_leaf("swapped"),
    };
    let viewport_style = Style::new();
    let ctx = DescriptorContext {
        new_rsx_root: &new_root,
        old_rsx_root: None,
        inherited_style: &viewport_style,
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
    .expect("ReplaceNode must translate to a FiberWork");
    assert!(work.is_committable(&viewport.scene.node_arena));
    apply_fiber_works(&mut viewport.scene.node_arena, test_apply_ctx(), vec![work])
        .expect("ReplaceNode work applies");

    // Parent NodeKey unchanged; children list still length 2; kept
    // sibling survives at slot 1; first slot is a fresh NodeKey.
    let arena = &viewport.scene.node_arena;
    let children = arena.children_of(parent_key);
    assert_eq!(children.len(), 2);
    assert_eq!(children[1], kept_child_key);
    assert_ne!(
        children[0], old_first_key,
        "replaced slot must mint a new key"
    );
    assert!(
        arena.get(old_first_key).is_none(),
        "old child slot must be dropped",
    );
}

// ---------------------------------------------------------------------------
// M3: incremental Update + SetText coverage
// ---------------------------------------------------------------------------
//
// These extend M2's Delete/Move-only gate with the prop-setter layer.
// The identity-preservation contract is the same: if the incremental
// path commits the work, the target NodeKey survives.

/// 軌 1 #6: when the OLD tree's structure higher up no longer
/// matches the NEW tree at the InsertChild parent_path, the
/// identity-validated walk aborts and the translator returns `None`
/// (forcing the all-or-nothing batch to fall back to full rebuild).
#[test]
fn incremental_commit_path_drift_identity_check_rejects_misaligned_walk() {
    use crate::view::fiber_work::{DescriptorContext, patch_to_fiber_work};

    let seed = host_el().with_child(host_el());
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&seed).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];

    // OLD tree (matches what reconcile would have walked):
    let old_root = host_el().with_child(host_el());
    // NEW tree: the child at path [0] has a different identity
    // (Text leaf instead of Element host) — `walk_rsx_by_index_path
    // _validated` should detect the mismatch when validating
    // `parent_path = [0]` and abort.
    let new_root = host_el().with_child(text_leaf("drifted"));
    let patch = crate::ui::Patch::InsertChild {
        parent_path: vec![0],
        index: 0,
        node: host_el(),
    };
    let style = crate::style::Style::new();
    let ctx = DescriptorContext {
        new_rsx_root: &new_root,
        old_rsx_root: Some(&old_root),
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
    );
    assert!(
        work.is_none(),
        "identity drift on parent_path must abort translation",
    );
}

#[test]
fn placement_only_path_resolves_node_key_without_stable_id_index() {
    use crate::style::{Transform, Translate};
    use crate::view::base_component::{DirtyFlags, Element as ElementHost};
    use crate::view::test_support::{commit_child, commit_element, new_test_arena};

    let mut arena = new_test_arena();
    let root_key = commit_element(
        &mut arena,
        Box::new(ElementHost::new_with_id(1, 0.0, 0.0, 0.0, 0.0)),
    );
    let first_child = commit_child(
        &mut arena,
        root_key,
        Box::new(ElementHost::new_with_id(77, 0.0, 0.0, 0.0, 0.0)),
    );
    let second_child = commit_child(
        &mut arena,
        root_key,
        Box::new(ElementHost::new_with_id(77, 0.0, 0.0, 0.0, 0.0)),
    );
    assert_eq!(
        arena.find_by_stable_id(77),
        Some(second_child),
        "duplicate stable_id index points at the last inserted node",
    );
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut arena,
        root_key,
        DirtyFlags::ALL,
    );

    let rsx_root = host_el().with_child(host_el()).with_child(host_el());
    let target_key = Viewport::arena_key_for_rsx_path(&arena, &[root_key], &rsx_root, &[0])
        .expect("path [0] should resolve to the first arena child");
    assert_eq!(
        target_key, first_child,
        "placement-only patch target must come from RSX path -> arena path -> NodeKey",
    );

    let transform = Transform::new([Translate::x(Length::px(24.0))]);
    let mut style = crate::style::Style::new();
    style.set_transform(transform.clone());
    assert!(Viewport::apply_placement_style_by_node_key(
        &arena, target_key, &style,
    ));

    {
        let first = arena.get(first_child).unwrap();
        let first = first
            .element
            .as_any()
            .downcast_ref::<ElementHost>()
            .unwrap();
        assert_eq!(first.debug_transform(), &transform);
    }

    {
        let second = arena.get(second_child).unwrap();
        let second = second
            .element
            .as_any()
            .downcast_ref::<ElementHost>()
            .unwrap();
        assert_eq!(second.debug_transform(), &Transform::default());
    }

    assert!(
        arena
            .arena_local_dirty(first_child)
            .contains(DirtyFlags::RUNTIME)
    );
    assert!(
        !arena
            .arena_local_dirty(second_child)
            .contains(DirtyFlags::RUNTIME)
    );
}
