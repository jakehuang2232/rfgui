use super::{ComponentNodeInner, ComponentVTable, RsxNode, unwrap_components};
use crate::ui::{RsxNodeIdentity, build_scope};
use std::any::TypeId;
use std::cell::Cell;
use std::ptr::NonNull;

thread_local! {
    static RENDER_CALLS: Cell<u32> = const { Cell::new(0) };
    static RENDER_SUM: Cell<u32> = const { Cell::new(0) };
}

struct TestProps {
    value: u32,
}

unsafe fn test_render_shim(props: NonNull<()>, _children: Vec<RsxNode>) -> RsxNode {
    let boxed: Box<TestProps> = unsafe { Box::from_raw(props.as_ptr().cast()) };
    RENDER_CALLS.with(|c| c.set(c.get() + 1));
    RENDER_SUM.with(|c| c.set(c.get() + boxed.value));
    RsxNode::text(format!("r:{}", boxed.value))
}

unsafe fn test_drop_shim(props: NonNull<()>) {
    drop(unsafe { Box::from_raw(props.as_ptr().cast::<TestProps>()) });
}

unsafe fn test_clone_shim(props: NonNull<()>) -> NonNull<()> {
    let src: &TestProps = unsafe { &*props.as_ptr().cast::<TestProps>() };
    let cloned = TestProps { value: src.value };
    NonNull::new(Box::into_raw(Box::new(cloned)).cast()).unwrap()
}

static TEST_VTABLE: ComponentVTable = ComponentVTable {
    render: test_render_shim,
    drop_props: test_drop_shim,
    clone_props: test_clone_shim,
    props_eq: None,
    type_name: "TestComp",
};

fn make_component_node(value: u32) -> RsxNode {
    let props = Box::into_raw(Box::new(TestProps { value }));
    RsxNode::Component(std::rc::Rc::new(ComponentNodeInner {
        identity: RsxNodeIdentity::new("TestComp", None),
        type_id: TypeId::of::<TestProps>(),
        props: NonNull::new(props.cast()).unwrap(),
        children: Vec::new(),
        key: None,
        vtable: &TEST_VTABLE,
    }))
}

#[test]
fn walker_invokes_vtable_render_on_component_node() {
    RENDER_CALLS.with(|c| c.set(0));
    RENDER_SUM.with(|c| c.set(0));
    let node = make_component_node(42);
    let out = build_scope(|| unwrap_components(node));
    RENDER_CALLS.with(|c| assert_eq!(c.get(), 1));
    RENDER_SUM.with(|c| assert_eq!(c.get(), 42));
    match out {
        RsxNode::Text(t) => assert_eq!(t.content, "r:42"),
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn walker_recurses_component_inside_element_children() {
    RENDER_CALLS.with(|c| c.set(0));
    RENDER_SUM.with(|c| c.set(0));
    // Hand-build: Element with one Component child.
    let child = make_component_node(7);
    let element = RsxNode::element("TestParent");
    let element = element.with_child(child);
    let out = build_scope(|| unwrap_components(element));
    RENDER_CALLS.with(|c| assert_eq!(c.get(), 1));
    RENDER_SUM.with(|c| assert_eq!(c.get(), 7));
    // Output root is Element; its child is the rendered Text.
    let RsxNode::Element(el) = out else {
        panic!("expected element root");
    };
    assert_eq!(el.children.len(), 1);
    match &el.children[0] {
        RsxNode::Text(t) => assert_eq!(t.content, "r:7"),
        other => panic!("expected text child, got {other:?}"),
    }
}

#[test]
fn walker_unconstructed_component_drops_props_via_vtable() {
    // Node is dropped without walking — Drop impl must fire drop_props
    // exactly once to free the boxed TestProps.
    let node = make_component_node(99);
    drop(node);
    // No assertion beyond "this does not leak or double-free"; Miri
    // would catch either. RENDER_CALLS stays at its prior value —
    // drop_props does not increment render counters.
}
