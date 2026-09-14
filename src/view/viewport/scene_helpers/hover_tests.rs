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
