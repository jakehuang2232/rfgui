use crate::ui::{
    ErrorBoundary, ErrorBoundaryHandle, ResetKey, RsxNode, State, component, render_root, rsx,
    take_state_dirty, use_context, use_mount, use_state,
};
use std::cell::{Cell, RefCell};

thread_local! {
    static NEXT_ID: Cell<u32> = const { Cell::new(0) };
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static HANDLE: RefCell<Option<ErrorBoundaryHandle>> = const { RefCell::new(None) };
}

fn log(entry: String) {
    LOG.with(|log| log.borrow_mut().push(entry));
}

/// The log since the last call, sorted: unmount order is unspecified.
fn take_log() -> Vec<String> {
    let mut log = LOG.with(|log| std::mem::take(&mut *log.borrow_mut()));
    log.sort();
    log
}

fn reset() {
    NEXT_ID.set(0);
    take_log();
}

#[derive(Clone, Copy, PartialEq)]
enum Fail {
    Nothing,
    Render,
    Mount,
}

/// Renders its name and the id its state got on mount, and logs mounts.
#[component]
fn Leaf(name: String, fail: Fail) -> RsxNode {
    let id: State<u32> = use_state(|| {
        NEXT_ID.set(NEXT_ID.get() + 1);
        NEXT_ID.get()
    });
    let id = id.get();
    let mounted = format!("{name}{id}");
    let failed_mount = format!("{name} mount failed");
    use_mount(move || {
        assert!(fail != Fail::Mount, "{failed_mount}");
        log(format!("mount {mounted}"));
        move || log(format!("unmount {mounted}"))
    });
    assert!(fail != Fail::Render, "{name} failed");
    RsxNode::text(format!("{name}{id} "))
}

#[component]
fn Nested(fail: Fail) -> RsxNode {
    rsx! { <Leaf name="inner" fail={fail} /> }
}

#[component]
fn ShowError() -> RsxNode {
    let handle = use_context::<ErrorBoundaryHandle>().expect("inside a boundary");
    let shown = format!("error: {} ", handle.error().message());
    HANDLE.with(|slot| *slot.borrow_mut() = Some(handle));
    RsxNode::text(shown)
}

/// The failing leaf sits below another component inside the boundary, and
/// a sibling renders after the boundary.
#[component]
fn Page(boundary: bool, fail: Fail, retry: u32) -> RsxNode {
    let outer = rsx! { <Leaf name="outer" fail={Fail::Nothing} /> };
    let nested = rsx! { <Nested fail={fail} /> };
    let guarded = if boundary {
        rsx! {
            <ErrorBoundary fallback={rsx! { <ShowError /> }} reset_key={ResetKey::new(&retry)}>
                {outer}
                {nested}
            </ErrorBoundary>
        }
    } else {
        RsxNode::fragment(vec![outer, nested])
    };
    rsx! {
        <crate::view::Element>
            {guarded}
            <Leaf name="sibling" fail={Fail::Nothing} />
        </crate::view::Element>
    }
}

fn text(node: &RsxNode) -> String {
    match node {
        RsxNode::Text(text) => text.content.clone(),
        RsxNode::Element(element) => element.children.iter().map(text).collect(),
        RsxNode::Fragment(fragment) => fragment.children.iter().map(text).collect(),
        RsxNode::Component(_) | RsxNode::Provider(_) | RsxNode::ErrorBoundary(_) => {
            panic!("unresolved node")
        }
    }
}

fn page(boundary: bool, fail: Fail, retry: u32) -> String {
    text(&render_root(
        || rsx! { <Page boundary={boundary} fail={fail} retry={retry} /> },
    ))
}

#[test]
fn boundary_unmounts_what_failed_and_holds_its_fallback_until_reset() {
    reset();
    assert_eq!(page(true, Fail::Nothing, 0), "outer1 inner2 sibling3 ");
    assert_eq!(
        take_log(),
        ["mount inner2", "mount outer1", "mount sibling3"]
    );

    // The sibling after the boundary keeps its state; everything under the
    // boundary unmounts, and the failed render queues no mount.
    assert_eq!(page(true, Fail::Render, 0), "error: inner failed sibling3 ");
    assert_eq!(take_log(), ["unmount inner2", "unmount outer1"]);
    assert_eq!(
        page(true, Fail::Nothing, 0),
        "error: inner failed sibling3 "
    );

    // A new reset key renders the children again, from fresh state.
    assert_eq!(page(true, Fail::Nothing, 1), "outer4 inner5 sibling3 ");
    assert_eq!(take_log(), ["mount inner5", "mount outer4"]);

    // So does the handle the fallback reads.
    assert_eq!(page(true, Fail::Render, 1), "error: inner failed sibling3 ");
    let handle = HANDLE
        .with(|slot| slot.borrow_mut().take())
        .expect("fallback rendered");
    handle.reset();
    assert_eq!(page(true, Fail::Nothing, 1), "outer6 inner7 sibling3 ");
}

#[test]
fn mount_panic_unmounts_the_boundary_children_from_the_next_frame() {
    reset();
    assert_eq!(page(true, Fail::Mount, 0), "outer1 inner2 sibling3 ");
    // The mount before the failed one is cleaned up, the sibling's runs.
    assert_eq!(
        take_log(),
        ["mount outer1", "mount sibling3", "unmount outer1"]
    );
    // The rebuild it asks for outlasts the frame taking its dirty state.
    take_state_dirty();
    assert!(take_state_dirty().needs_rebuild());
    assert_eq!(
        page(true, Fail::Nothing, 0),
        "error: inner mount failed sibling3 "
    );
    assert!(take_log().is_empty());
}

#[test]
fn panic_outside_any_boundary_unmounts_the_tree() {
    reset();
    assert_eq!(page(false, Fail::Nothing, 0), "outer1 inner2 sibling3 ");
    take_log();
    assert_eq!(page(false, Fail::Render, 0), "");
    assert_eq!(
        take_log(),
        ["unmount inner2", "unmount outer1", "unmount sibling3"]
    );

    // A failed mount unmounts the tree in the same render; the mounts after
    // it never run.
    assert_eq!(page(false, Fail::Mount, 0), "");
    assert_eq!(take_log(), ["mount outer4", "unmount outer4"]);
    // Like a render panic there, it asks for no rebuild that would retry.
    take_state_dirty();
    assert!(!take_state_dirty().needs_rebuild());
    assert_eq!(page(false, Fail::Nothing, 0), "outer7 inner8 sibling9 ");
}
