use super::*;

use crate::ui::{Modifiers, PointerButtons, PointerEventData};
use crate::view::base_component::Element;
use crate::view::test_support::{commit_child, commit_element, new_test_arena};

use std::cell::RefCell;
use std::rc::Rc;

fn test_pointer_data() -> PointerEventData {
    PointerEventData {
        viewport_x: 0.0,
        viewport_y: 0.0,
        local_x: 0.0,
        local_y: 0.0,
        button: None,
        buttons: PointerButtons::default(),
        modifiers: Modifiers::default(),
        pointer_id: 0,
        pointer_type: crate::platform::input::PointerType::Mouse,
        pressure: 0.0,
        timestamp: crate::time::Instant::now(),
    }
}

#[test]
fn hover_transition_dispatches_enter_leave_on_changed_ancestors_only() {
    let order = Rc::new(RefCell::new(Vec::new()));

    let mut root = Element::new(0.0, 0.0, 120.0, 120.0);
    let root_order = order.clone();
    root.on_pointer_enter(move |_event| root_order.borrow_mut().push("root-enter"));
    let root_order = order.clone();
    root.on_pointer_leave(move |_event| root_order.borrow_mut().push("root-leave"));

    let mut parent = Element::new(0.0, 0.0, 120.0, 120.0);
    let parent_order = order.clone();
    parent.on_pointer_enter(move |_event| parent_order.borrow_mut().push("parent-enter"));
    let parent_order = order.clone();
    parent.on_pointer_leave(move |_event| parent_order.borrow_mut().push("parent-leave"));

    let mut child = Element::new(0.0, 0.0, 60.0, 60.0);
    let child_order = order.clone();
    child.on_pointer_enter(move |_event| child_order.borrow_mut().push("child-enter"));
    let child_order = order.clone();
    child.on_pointer_leave(move |_event| child_order.borrow_mut().push("child-leave"));

    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(root));
    let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
    let child_key = commit_child(&mut arena, parent_key, Box::new(child));

    let roots = [root_key];

    assert!(dispatch_hover_transition(
        &mut arena,
        &roots,
        None,
        Some(child_key),
        test_pointer_data()
    ));
    assert_eq!(
        order.borrow().as_slice(),
        &["root-enter", "parent-enter", "child-enter"]
    );

    order.borrow_mut().clear();
    assert!(dispatch_hover_transition(
        &mut arena,
        &roots,
        Some(child_key),
        Some(parent_key),
        test_pointer_data(),
    ));
    assert_eq!(order.borrow().as_slice(), &["child-leave"]);

    order.borrow_mut().clear();
    assert!(dispatch_hover_transition(
        &mut arena,
        &roots,
        Some(parent_key),
        None,
        test_pointer_data(),
    ));
    assert_eq!(order.borrow().as_slice(), &["parent-leave", "root-leave"]);

    order.borrow_mut().clear();
    assert!(!dispatch_hover_transition(
        &mut arena,
        &roots,
        Some(root_key),
        Some(root_key),
        test_pointer_data(),
    ));
    assert!(order.borrow().is_empty());
}

#[test]
fn unchanged_hover_does_not_record_paint_mutations() {
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(Element::new(0.0, 0.0, 100.0, 100.0)));
    let a = commit_child(
        &mut arena,
        root,
        Box::new(Element::new(0.0, 0.0, 30.0, 30.0)),
    );
    let b = commit_child(
        &mut arena,
        root,
        Box::new(Element::new(40.0, 0.0, 30.0, 30.0)),
    );
    for target in [None, Some(a), Some(b), None] {
        update_hover_state(&arena, root, target);
        let before: Vec<_> = [root, a, b].map(|k| arena.mutation_revision(k)).into();
        assert!(!update_hover_state(&arena, root, target));
        assert_eq!(before, [root, a, b].map(|k| arena.mutation_revision(k)));
        assert!(
            !arena
                .get(root)
                .unwrap()
                .element
                .hover_update_needed(target.is_some())
        );
        assert!(
            !arena
                .get(a)
                .unwrap()
                .element
                .hover_update_needed(target == Some(a))
        );
        assert!(
            !arena
                .get(b)
                .unwrap()
                .element
                .hover_update_needed(target == Some(b))
        );
    }
}

#[test]
fn same_target_resynchronizes_replaced_ancestor_hover_state() {
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(Element::new(0.0, 0.0, 100.0, 100.0)));
    let child = commit_child(
        &mut arena,
        root,
        Box::new(Element::new(0.0, 0.0, 20.0, 20.0)),
    );
    assert!(update_hover_state(&arena, root, Some(child)));
    // Rebuilding a native host can reset its state while retaining the target.
    arena.mutate_element_ref_with_invalidation(root, |element, _| {
        element.set_hovered(false);
    });
    assert!(update_hover_state(&arena, root, Some(child)));
    assert!(!arena.get(root).unwrap().element.hover_update_needed(true));
    let replacement = commit_child(
        &mut arena,
        root,
        Box::new(Element::new(30.0, 0.0, 20.0, 20.0)),
    );
    assert!(update_hover_state(&arena, root, Some(replacement)));
    assert!(!arena.get(child).unwrap().element.hover_update_needed(false));
    assert!(
        !arena
            .get(replacement)
            .unwrap()
            .element
            .hover_update_needed(true)
    );
}

struct UnknownHoverHost {
    calls: Rc<std::cell::Cell<usize>>,
}
impl crate::view::base_component::Layoutable for UnknownHoverHost {
    fn measure(
        &mut self,
        _: crate::view::base_component::LayoutConstraints,
        _: &mut crate::view::node_arena::NodeArena,
    ) {
    }
    fn place(
        &mut self,
        _: crate::view::base_component::LayoutPlacement,
        _: &mut crate::view::node_arena::NodeArena,
    ) {
    }
    fn measured_size(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn set_layout_width(&mut self, _: f32) {}
    fn set_layout_height(&mut self, _: f32) {}
}
impl crate::view::base_component::EventTarget for UnknownHoverHost {
    fn set_hovered(&mut self, _: bool) -> bool {
        self.calls.set(self.calls.get() + 1);
        false
    }
}
impl crate::view::base_component::Renderable for UnknownHoverHost {
    fn build(
        &mut self,
        _: &mut crate::view::frame_graph::FrameGraph,
        _: &mut crate::view::node_arena::NodeArena,
        ctx: crate::view::base_component::UiBuildContext,
    ) -> crate::view::base_component::BuildState {
        ctx.into_state()
    }
}
impl crate::view::base_component::ElementTrait for UnknownHoverHost {
    fn dirty_observation_is_tracked(&self) -> bool {
        true
    }
    fn clear_local_dirty_flags(&mut self, _: DirtyFlags) {
        self.calls.set(self.calls.get() + 1);
    }
    fn stable_id(&self) -> u64 {
        42
    }
    fn box_model_snapshot(&self) -> crate::view::base_component::BoxModelSnapshot {
        crate::view::base_component::BoxModelSnapshot {
            node_id: 42,
            parent_id: None,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            border_radius: 0.0,
            should_render: false,
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[test]
fn unknown_hover_host_keeps_setter_side_effects_and_mutation_tracking() {
    let calls = Rc::new(std::cell::Cell::new(0));
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(UnknownHoverHost {
            calls: calls.clone(),
        }),
    );
    update_hover_state(&arena, root, None);
    let revision = arena.mutation_revision(root);
    update_hover_state(&arena, root, None);
    assert_eq!(calls.get(), 2);
    assert_ne!(arena.mutation_revision(root), revision);
}

#[test]
fn warm_hover_visits_only_changed_paths_in_a_wide_tree() {
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(Element::new(0., 0., 100., 100.)));
    let children: Vec<_> = (0..1000)
        .map(|_| commit_child(&mut arena, root, Box::new(Element::new(0., 0., 10., 10.))))
        .collect();
    let (_, cold) =
        crate::ui::profile_ui_work(|| update_hover_state(&arena, root, Some(children[0])));
    assert_eq!(cold.hover_observations, 1001);
    let (_, warm) =
        crate::ui::profile_ui_work(|| update_hover_state(&arena, root, Some(children[0])));
    assert_eq!(warm.hover_observations, 1);
    let (_, moved) =
        crate::ui::profile_ui_work(|| update_hover_state(&arena, root, Some(children[999])));
    assert_eq!(moved.hover_observations, 3);
    for &key in &children {
        assert!(
            !arena
                .get(key)
                .unwrap()
                .element
                .hover_update_needed(key == children[999])
        );
    }
    let (_, leave) = crate::ui::profile_ui_work(|| update_hover_state(&arena, root, None));
    assert_eq!(leave.hover_observations, 2);
    assert!(!arena.get(root).unwrap().element.hover_update_needed(false));
}

#[test]
fn hover_cache_resynchronizes_reparented_and_incoherent_paths() {
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(Element::new(0., 0., 100., 100.)));
    let left = commit_child(&mut arena, root, Box::new(Element::new(0., 0., 40., 40.)));
    let right = commit_child(&mut arena, root, Box::new(Element::new(50., 0., 40., 40.)));
    let leaf = commit_child(&mut arena, left, Box::new(Element::new(0., 0., 10., 10.)));
    update_hover_state(&arena, root, Some(leaf));
    arena.set_children(left, vec![]);
    arena.set_children(right, vec![leaf]);
    arena.set_parent(leaf, Some(right));
    assert!(update_hover_state(&arena, root, Some(leaf)));
    assert!(!arena.get(left).unwrap().element.hover_update_needed(false));
    assert!(!arena.get(right).unwrap().element.hover_update_needed(true));
    arena.set_parent(leaf, None);
    update_hover_state(&arena, root, None);
    assert!(update_hover_state(&arena, root, Some(leaf)));
    assert!(!arena.get(leaf).unwrap().element.hover_update_needed(true));
    assert!(!arena.get(right).unwrap().element.hover_update_needed(true));
}

#[test]
fn cyclic_hover_parent_path_terminates() {
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(Element::new(0., 0., 10., 10.)));
    arena.set_parent(root, Some(root));
    assert!(hover_path_for_target(&arena, &[root], Some(root)).is_empty());
}

#[test]
fn tracked_custom_dirty_getter_does_not_suppress_clear_hook_side_effects() {
    let calls = Rc::new(std::cell::Cell::new(0));
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(UnknownHoverHost {
            calls: calls.clone(),
        }),
    );
    clear_subtree_dirty_flags_with_arena_dirty(&mut arena, root, DirtyFlags::ALL);
    let revision = arena.mutation_revision(root);
    clear_subtree_dirty_flags_with_arena_dirty(&mut arena, root, DirtyFlags::ALL);
    assert_eq!(calls.get(), 2);
    assert_ne!(revision, arena.mutation_revision(root));
}

#[test]
fn pointer_dispatch_reuses_unchanged_hover_subtrees_after_event_mutations() {
    fn branch(
        arena: &mut crate::view::node_arena::NodeArena,
        parent: crate::view::node_arena::NodeKey,
        depth: usize,
        leaves: &mut Vec<crate::view::node_arena::NodeKey>,
    ) {
        for _ in 0..10 {
            let child = commit_child(arena, parent, Box::new(Element::new(0., 0., 10., 10.)));
            if depth == 1 {
                leaves.push(child);
            } else {
                branch(arena, child, depth - 1, leaves);
            }
        }
    }
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(Element::new(0., 0., 100., 100.)));
    let mut leaves = Vec::new();
    branch(&mut arena, root, 3, &mut leaves);
    assert_eq!(arena.len(), 1111);
    let mut hovered = None;
    Viewport::sync_hover_target(
        &arena,
        &[root],
        &mut hovered,
        Some(leaves[0]),
        test_pointer_data(),
    );
    let (_, profile) = crate::ui::profile_ui_work(|| {
        Viewport::sync_hover_target(
            &arena,
            &[root],
            &mut hovered,
            Some(leaves[999]),
            test_pointer_data(),
        )
    });
    // Enter/leave hooks invalidate their owners, so those branches are checked
    // live. The other 1060 nodes need no hover observation.
    assert_eq!(profile.hover_observations, 51);
    for (index, &leaf) in leaves.iter().enumerate() {
        assert!(
            !arena
                .get(leaf)
                .unwrap()
                .element
                .hover_update_needed(index == 999)
        );
    }
}
