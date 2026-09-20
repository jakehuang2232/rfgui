use super::*;
use crate::ui::profile_ui_work;
use std::{cell::Cell, rc::Rc};

fn tree() -> (NodeArena, NodeKey, NodeKey, NodeKey, NodeKey) {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(clean_element())));
    let left = arena.insert(Node::new(Box::new(clean_element())));
    let right = arena.insert(Node::new(Box::new(clean_element())));
    let leaf = arena.insert(Node::new(Box::new(clean_element())));
    link_child(&mut arena, root, left);
    link_child(&mut arena, root, right);
    link_child(&mut arena, left, leaf);
    (arena, root, left, right, leaf)
}

fn oracle(arena: &NodeArena, key: NodeKey) -> (DirtyFlags, PlacementEligibilityMetadata) {
    let n = arena.get(key).unwrap();
    let mut flags = n
        .element
        .local_dirty_flags()
        .union(arena.arena_local_dirty(key));
    let mut metadata = n.element.placement_eligibility_metadata();
    for &child in n.children() {
        let (f, m) = oracle(arena, child);
        flags = flags.union(f);
        metadata = metadata.union(m);
    }
    (flags, metadata)
}
fn assert_fresh(arena: &NodeArena, root: NodeKey) {
    let (flags, metadata) = oracle(arena, root);
    assert_eq!(arena.refresh_subtree_dirty_cache(root), flags);
    assert_eq!(arena.cached_placement_eligibility_metadata(root), metadata);
}

#[test]
fn unchanged_subtree_skips_observations_and_leaf_mutation_preserves_sibling() {
    let (arena, root, _, _, leaf) = tree();
    let (_, first) = profile_ui_work(|| assert_fresh(&arena, root));
    assert_eq!(first.dirty_observations, 4);
    let (_, clean) = profile_ui_work(|| assert_fresh(&arena, root));
    assert_eq!(
        (clean.dirty_observations, clean.dirty_subtree_reuses),
        (0, 1)
    );
    arena.mark_dirty(leaf, DirtyFlags::LAYOUT);
    let (_, changed) = profile_ui_work(|| assert_fresh(&arena, root));
    assert_eq!(
        (changed.dirty_observations, changed.dirty_subtree_reuses),
        (3, 1)
    );
    assert!(
        arena
            .cached_subtree_dirty(root)
            .contains(DirtyFlags::LAYOUT)
    );
}

#[test]
fn bookkeeping_clears_invalidate_dirty_proof_without_acknowledging_render_causes() {
    let (arena, root, _, _, leaf) = tree();
    arena.mark_dirty(leaf, DirtyFlags::LAYOUT.union(DirtyFlags::PAINT));
    assert_fresh(&arena, root);
    let revision = arena.subtree_mutation_revision(root);
    arena.clear_arena_dirty(leaf, DirtyFlags::LAYOUT);
    assert_fresh(&arena, root);
    assert!(
        !arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::LAYOUT)
    );
    arena.clear_arena_dirty_subtree(root, DirtyFlags::PAINT);
    assert_fresh(&arena, root);
    assert_eq!(arena.subtree_mutation_revision(root), revision);
    assert!(
        arena
            .pending_render_changes(leaf)
            .contains(DirtyFlags::PAINT)
    );
    arena.mark_dirty(leaf, DirtyFlags::PLACE);
    assert_fresh(&arena, root);
    arena.clear_cached_arena_dirty_subtree(root, DirtyFlags::PLACE);
    assert_fresh(&arena, root);
    assert!(
        !arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PLACE)
    );
}

#[test]
fn native_clear_and_mutable_host_access_refresh_local_flags_and_metadata() {
    let (arena, root, _, _, leaf) = tree();
    // Native hosts start dirty; mutable access and native bookkeeping clear
    // must both invalidate a previously installed proof.
    *arena.get_mut(leaf).unwrap().element = Box::new(Element::new(0., 0., 10., 10.));
    assert_fresh(&arena, root);
    let revision = arena.subtree_mutation_revision(root);
    arena.clear_element_dirty_flags(leaf, DirtyFlags::ALL);
    assert_fresh(&arena, root);
    assert_eq!(arena.subtree_mutation_revision(root), revision);
    assert!(arena.cached_subtree_dirty(root).is_empty());
    let mut n = arena.get_mut(leaf).unwrap();
    let e = n.element.as_any_mut().downcast_mut::<Element>().unwrap();
    e.set_layout_transition_x(10.);
    drop(n);
    assert_fresh(&arena, root);
    assert!(
        arena
            .cached_placement_eligibility_metadata(root)
            .contains_runtime_layout_state
    );
}

#[test]
fn topology_changes_reparent_removal_and_incoherent_links_never_hide_dirty() {
    let (mut arena, root, left, right, leaf) = tree();
    assert_fresh(&arena, root);
    arena.set_children(left, vec![]);
    arena.set_parent(leaf, Some(right));
    arena.set_children(right, vec![leaf]);
    arena.mark_dirty(leaf, DirtyFlags::LAYOUT);
    assert_fresh(&arena, root);
    arena.remove(leaf);
    arena.set_children(right, vec![]);
    assert_fresh(&arena, root);
    // Even malformed public wiring must not certify a skip relying on absent
    // parent links. The existing walker still observes the child each pass.
    let orphan = arena.insert(Node::new(Box::new(clean_element())));
    arena.set_children(left, vec![orphan]);
    assert_fresh(&arena, root);
    arena.mark_dirty(orphan, DirtyFlags::PLACE);
    assert_fresh(&arena, root);
    assert!(arena.cached_subtree_dirty(root).contains(DirtyFlags::PLACE));
}

#[test]
fn taken_host_restore_and_panic_do_not_leave_a_placeholder_proof() {
    let (mut arena, root, _, _, leaf) = tree();
    assert_fresh(&arena, root);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        arena.with_element_taken(leaf, |element, arena| {
            assert_fresh(arena, root);
            *element = Box::new(Element::new(0., 0., 20., 20.));
            panic!("restore host");
        });
    }));
    assert!(result.is_err());
    assert_fresh(&arena, root);
    assert!(!arena.cached_subtree_dirty(root).is_empty());
}

struct ExternalHost(Rc<Cell<DirtyFlags>>);
impl Layoutable for ExternalHost {
    fn measure(&mut self, _: LayoutConstraints, _: &mut NodeArena) {}
    fn place(&mut self, _: LayoutPlacement, _: &mut NodeArena) {}
    fn measured_size(&self) -> (f32, f32) {
        (0., 0.)
    }
    fn set_layout_width(&mut self, _: f32) {}
    fn set_layout_height(&mut self, _: f32) {}
}
impl EventTarget for ExternalHost {}
impl Renderable for ExternalHost {
    fn build(&mut self, _: &mut FrameGraph, _: &mut NodeArena, ctx: UiBuildContext) -> BuildState {
        ctx.into_state()
    }
}
impl ElementTrait for ExternalHost {
    fn stable_id(&self) -> u64 {
        999
    }
    fn local_dirty_flags(&self) -> DirtyFlags {
        self.0.get()
    }
    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        clean_element().box_model_snapshot()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[test]
fn unknown_shared_state_is_observed_while_known_siblings_still_reuse() {
    let (arena, root, _, _, leaf) = tree();
    let shared = Rc::new(Cell::new(DirtyFlags::NONE));
    *arena.get_mut(leaf).unwrap().element = Box::new(ExternalHost(shared.clone()));
    assert_fresh(&arena, root);
    let revision = arena.subtree_mutation_revision(root);
    shared.set(DirtyFlags::LAYOUT);
    let (_, p) = profile_ui_work(|| assert_fresh(&arena, root));
    assert_eq!(arena.subtree_mutation_revision(root), revision);
    assert_eq!((p.dirty_observations, p.dirty_subtree_reuses), (3, 1));
    shared.set(DirtyFlags::NONE);
    assert_fresh(&arena, root);
}

#[test]
fn revision_saturation_forces_observation() {
    let (arena, root, _, _, leaf) = tree();
    assert_fresh(&arena, root);
    arena.mutation_clock.set(u64::MAX);
    arena.mark_dirty(leaf, DirtyFlags::LAYOUT);
    let (_, p) = profile_ui_work(|| assert_fresh(&arena, root));
    assert_eq!(p.dirty_observations, 4);
    assert_eq!(p.dirty_subtree_reuses, 0);
}
