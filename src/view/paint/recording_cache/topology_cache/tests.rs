use super::*;
use crate::view::base_component::*;
use crate::view::node_arena::Node;
use std::cell::Cell;
use std::rc::Rc;
struct External {
    id: Rc<Cell<u64>>,
    deferred: Rc<Cell<bool>>,
}
impl Layoutable for External {
    fn measure(&mut self, _: LayoutConstraints, _: &mut NodeArena) {}
    fn place(&mut self, _: LayoutPlacement, _: &mut NodeArena) {}
    fn measured_size(&self) -> (f32, f32) {
        (10., 10.)
    }
    fn set_layout_width(&mut self, _: f32) {}
    fn set_layout_height(&mut self, _: f32) {}
}
impl EventTarget for External {}
impl Renderable for External {
    fn build(
        &mut self,
        _: &mut crate::view::frame_graph::FrameGraph,
        _: &mut NodeArena,
        ctx: UiBuildContext,
    ) -> BuildState {
        ctx.into_state()
    }
}
impl ElementTrait for External {
    fn stable_id(&self) -> u64 {
        self.id.get()
    }
    fn is_deferred_to_root_viewport_render(&self) -> bool {
        self.deferred.get()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.id.get(),
            parent_id: None,
            x: 0.,
            y: 0.,
            width: 10.,
            height: 10.,
            border_radius: 0.,
            should_render: true,
        }
    }
}
#[test]
fn opaque_topology_getters_are_observed_without_arena_mutation() {
    for change_id in [true, false] {
        let mut arena = NodeArena::new();
        let id = Rc::new(Cell::new(71));
        let deferred = Rc::new(Cell::new(false));
        let root = arena.insert(Node::new(Box::new(External {
            id: id.clone(),
            deferred: deferred.clone(),
        })));
        let snapshot = Arc::new(TopologySnapshot {
            owner_parents: Arc::new([(root, None)].into_iter().collect()),
            covered: Arc::new([root].into_iter().collect()),
            deferred_roots: vec![],
            deferred: FxHashSet::default(),
        });
        let mut cache = TopologyCache::default();
        cache.remember(
            &arena,
            &[root],
            snapshot.clone(),
            &[(71, root)].into_iter().collect(),
            &[(root, vec![])].into_iter().collect(),
        );
        assert!(Arc::ptr_eq(
            &snapshot,
            &cache.replay(&arena, &[root]).unwrap()
        ));
        let clock = arena.mutation_clock();
        if change_id {
            id.set(72);
        } else {
            deferred.set(true);
        }
        assert_eq!(arena.mutation_clock(), clock);
        assert!(cache.replay(&arena, &[root]).is_none());
    }
}

#[test]
fn boundary_census_rejects_incoherent_edges_and_rechecks_live_owners() {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(81, 0., 0., 10., 10.))));
    let child = arena.insert(Node::new(Box::new(Element::new_with_id(82, 0., 0., 10., 10.))));
    arena.set_children(root, vec![child]);
    let mut cache = TopologyCache::default();
    let snapshot = Arc::new(TopologySnapshot {
        owner_parents: Arc::new([(root, None), (child, Some(root))].into_iter().collect()),
        covered: Arc::new([root, child].into_iter().collect()),
        deferred_roots: vec![],
        deferred: FxHashSet::default(),
    });
    let stable = [(81, root), (82, child)].into_iter().collect();
    let children = [(root, vec![child]), (child, vec![])].into_iter().collect();
    cache.remember(&arena, &[root], snapshot.clone(), &stable, &children);
    assert!(
        cache.boundary_nodes(&arena, &[root]).is_none(),
        "missing child back edge"
    );
    arena.set_parent(child, Some(root));
    cache.remember(&arena, &[root], snapshot.clone(), &stable, &children);
    assert_eq!(cache.boundary_nodes(&arena, &[root]), Some(vec![]));
    arena.set_arena_children_without_mirror_for_test(root, vec![]);
    assert!(
        cache.boundary_nodes(&arena, &[root]).is_none(),
        "changed mirror"
    );
    cache.remember(&arena, &[root], snapshot, &stable, &children);
    assert!(
        cache.boundary_nodes(&arena, &[root]).is_none(),
        "cold mismatch is not certified"
    );
}

#[test]
fn boundary_census_does_not_hide_opaque_deferred_observations() {
    let mut arena = NodeArena::new();
    let deferred = Rc::new(Cell::new(false));
    let root = arena.insert(Node::new(Box::new(External {
        id: Rc::new(Cell::new(91)),
        deferred: deferred.clone(),
    })));
    let mut cache = TopologyCache::default();
    cache.remember(
        &arena,
        &[root],
        Arc::new(TopologySnapshot {
            owner_parents: Arc::new([(root, None)].into_iter().collect()),
            covered: Arc::new([root].into_iter().collect()),
            deferred_roots: vec![],
            deferred: FxHashSet::default(),
        }),
        &[(91, root)].into_iter().collect(),
        &[(root, vec![])].into_iter().collect(),
    );
    assert_eq!(cache.boundary_nodes(&arena, &[root]), Some(vec![root]));
    deferred.set(true);
    assert!(cache.boundary_nodes(&arena, &[root]).is_none());
    cache.finish(false);
    assert!(cache.boundary_nodes(&arena, &[root]).is_none());
}

/// A translated subtree is mutated without changing topology: its census
/// still replays, while a changed edge on any mutated owner rejects it.
#[test]
fn mutated_owners_with_unchanged_topology_keep_the_census() {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        91, 0., 0., 10., 10.,
    ))));
    let child = arena.insert(Node::new(Box::new(Element::new_with_id(
        92, 0., 0., 10., 10.,
    ))));
    arena.set_children(root, vec![child]);
    arena.set_parent(child, Some(root));
    let snapshot = Arc::new(TopologySnapshot {
        owner_parents: Arc::new([(root, None), (child, Some(root))].into_iter().collect()),
        covered: Arc::new([root, child].into_iter().collect()),
        deferred_roots: vec![],
        deferred: FxHashSet::default(),
    });
    let mut cache = TopologyCache::default();
    cache.remember(
        &arena,
        &[root],
        snapshot.clone(),
        &[(91, root), (92, child)].into_iter().collect(),
        &[(root, vec![child]), (child, vec![])].into_iter().collect(),
    );
    for key in [root, child] {
        arena
            .get_mut(key)
            .unwrap()
            .element
            .translate_in_place(4.0, 2.0);
    }
    assert!(Arc::ptr_eq(
        &snapshot,
        &cache
            .replay(&arena, &[root])
            .expect("geometry-only mutation")
    ));
    arena.set_parent(child, None);
    assert!(
        cache.replay(&arena, &[root]).is_none(),
        "changed parent edge"
    );
}
