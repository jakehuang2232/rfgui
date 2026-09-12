use super::*;

fn scene() -> (NodeArena, NodeKey) {
    let mut arena = NodeArena::new();
    let key = arena.insert(Node::new(Box::new(Element::new_with_id(
        1, 0., 0., 20., 16.,
    ))));
    arena.set_roots(vec![key]);
    arena.clear_element_dirty_flags(key, DirtyFlags::ALL);
    let capture = arena.capture_render_changes();
    arena.commit_render_changes(capture);
    (arena, key)
}

#[test]
fn layout_consumption_preserves_render_causes_until_successful_acknowledgment() {
    let (arena, key) = scene();
    arena.mark_dirty(key, DirtyFlags::LAYOUT.union(DirtyFlags::RESOURCE));
    arena.refresh_subtree_dirty_cache(key);
    arena.clear_arena_dirty(key, DirtyFlags::ALL);
    assert!(
        arena
            .pending_render_changes(key)
            .contains(DirtyFlags::LAYOUT.union(DirtyFlags::RESOURCE))
    );
    let failed_attempt = arena.capture_render_changes();
    drop(failed_attempt);
    assert!(
        arena
            .pending_render_changes(key)
            .contains(DirtyFlags::RESOURCE)
    );
    let successful_attempt = arena.capture_render_changes();
    arena.commit_render_changes(successful_attempt);
    assert!(arena.pending_render_changes(key).is_empty());
}

#[test]
fn repeated_bit_after_capture_survives_even_when_local_dirty_is_cleared() {
    let (arena, key) = scene();
    arena.mark_dirty(key, DirtyFlags::RESOURCE);
    let capture = arena.capture_render_changes();
    arena.mark_dirty(key, DirtyFlags::RESOURCE);
    arena.clear_arena_dirty(key, DirtyFlags::ALL);
    arena.commit_render_changes(capture);
    assert!(
        arena
            .pending_render_changes(key)
            .contains(DirtyFlags::RESOURCE)
    );
    let retry = arena.capture_render_changes();
    arena.commit_render_changes(retry);
    assert!(arena.pending_render_changes(key).is_empty());
}

#[test]
fn mutable_access_survives_manual_dirty_clear_but_native_bookkeeping_does_not_invalidate() {
    let (arena, key) = scene();
    let first = arena.mutation_revision(key).unwrap();
    let capture = arena.capture_render_changes();
    {
        let mut node = arena.get_mut(key).unwrap();
        node.element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .set_width(40.);
        node.element.clear_local_dirty_flags(DirtyFlags::ALL);
    }
    assert_ne!(arena.mutation_revision(key), Some(first));
    arena.commit_render_changes(capture);
    assert!(
        arena
            .pending_render_changes(key)
            .contains(DirtyFlags::PAINT)
    );
    let current = arena.mutation_revision(key);
    arena.clear_element_dirty_flags(key, DirtyFlags::ALL);
    assert_eq!(arena.mutation_revision(key), current);
}

#[test]
fn captures_cannot_acknowledge_another_arena_or_a_reused_slot() {
    let (arena, key) = scene();
    let (mut other, other_key) = scene();
    assert_eq!(key, other_key);
    let capture = arena.capture_render_changes();
    other.mark_dirty(other_key, DirtyFlags::RESOURCE);
    other.commit_render_changes(capture);
    assert!(
        other
            .pending_render_changes(other_key)
            .contains(DirtyFlags::RESOURCE)
    );
    let capture = other.capture_render_changes();
    other.remove(other_key);
    let replacement = other.insert(Node::new(Box::new(Element::new_with_id(2, 0., 0., 2., 2.))));
    other.commit_render_changes(capture);
    assert!(
        other
            .pending_render_changes(replacement)
            .contains(DirtyFlags::ALL)
    );
}

#[test]
fn topology_mutations_and_taken_callbacks_invalidate_their_owners() {
    let (mut arena, key) = scene();
    let child = arena.insert(Node::new(Box::new(Element::new_with_id(2, 0., 0., 2., 2.))));
    arena.set_children(key, vec![child]);
    assert!(
        arena
            .pending_render_changes(key)
            .contains(DirtyFlags::RECORDING_TOPOLOGY)
    );
    let previous = arena.mutation_revision(key);
    arena.with_element_taken(key, |element, _| {
        element.clear_local_dirty_flags(DirtyFlags::ALL)
    });
    assert_ne!(arena.mutation_revision(key), previous);
    let previous = arena.mutation_revision(key);
    arena.with_element_taken_ref(key, |element, _| {
        element.clear_local_dirty_flags(DirtyFlags::ALL)
    });
    assert_ne!(arena.mutation_revision(key), previous);
}

#[test]
fn mutation_revision_exhaustion_never_mints_an_unchanged_proof() {
    let (arena, key) = scene();
    arena.mutation_clock.set(u64::MAX - 1);
    drop(arena.get_mut(key));
    assert_eq!(arena.mutation_revision(key), None);
    let capture = arena.capture_render_changes();
    arena.commit_render_changes(capture);
    assert!(!arena.pending_render_changes(key).is_empty());
}

#[test]
fn newer_content_work_does_not_keep_an_acknowledged_topology_cause_alive() {
    let (mut arena, key) = scene();
    arena.set_children(key, Vec::new());
    let capture = arena.capture_render_changes();
    drop(arena.get_mut(key)); // Later unclassified content observation.
    arena.commit_render_changes(capture);
    assert!(
        !arena
            .pending_render_changes(key)
            .intersects(DirtyFlags::RECORDING_TOPOLOGY)
    );
    assert!(
        arena
            .pending_render_changes(key)
            .contains(DirtyFlags::PAINT)
    );
}
