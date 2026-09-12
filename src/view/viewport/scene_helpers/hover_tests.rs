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
