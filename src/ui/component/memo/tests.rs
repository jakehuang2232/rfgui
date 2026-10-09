use super::*;
use crate::ui::{component, profile_ui_work, rsx};
use crate::view::Element;

#[component]
fn Row(value: usize) -> RsxNode {
    rsx! { <Element>{value.to_string()}</Element> }
}

#[test]
fn real_components_skip_255_of_256_unchanged_rows() {
    let build = |value| {
        render_pass(
            || rsx! { <Element>{(0..256).map(|i| rsx! { <Row key={i} value={if i == 128 { value } else { i }} /> }).collect::<Vec<_>>()}</Element> },
        )
    };
    let (_, first) = profile_ui_work(|| build(0));
    assert_eq!(first.component_renders, 256);
    let (_, second) = profile_ui_work(|| build(1));
    assert_eq!(second.component_renders, 1);
    assert_eq!(second.memo_hits, 255);
}

#[test]
fn comparison_falls_back_without_adding_a_partial_eq_bound() {
    struct Opaque;
    assert!((&&MemoCompare(&3)).memo_eq(&3));
    assert!(!(&&MemoCompare(&3)).memo_eq(&4));
    assert!(!(&&MemoCompare(&Opaque)).memo_eq(&Opaque));
}

use crate::ui::{
    Binding, State, batch_state_updates, provide_context_node, render_pass, use_context, use_mount,
    use_state, with_pushed_context_raw,
};
use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};

fn text(node: &RsxNode) -> String {
    match node {
        RsxNode::Text(t) => t.content.clone(),
        RsxNode::Element(e) => e.children.iter().map(text).collect(),
        RsxNode::Fragment(f) => f.children.iter().map(text).collect(),
        _ => panic!("unresolved node"),
    }
}

#[component]
fn Children(children: Vec<RsxNode>) -> RsxNode {
    rsx! {<Element>{children}</Element>}
}

#[test]
fn children_are_inputs_and_shared_resolved_output_is_reused() {
    let build = |child: RsxNode| render_pass(|| rsx! { <Children>{child}</Children> });
    let child = rsx! { <Element>"same"</Element> };
    let a = build(child.clone());
    let (b, work) = profile_ui_work(|| build(child));
    assert_eq!(work.component_renders, 0);
    assert_eq!(work.memo_hits, 1);
    assert!(RsxNode::ptr_eq(&a, &b));
    let (c, work) = profile_ui_work(|| build(RsxNode::text("changed")));
    assert_eq!(work.component_renders, 1);
    assert_eq!(text(&c), "changed");
}

#[component]
fn Callback(callback: Rc<dyn Fn() -> usize>) -> RsxNode {
    RsxNode::text(callback().to_string())
}
#[derive(Clone)]
struct Opaque(usize);
#[component]
fn Unknown(value: Opaque) -> RsxNode {
    RsxNode::text(value.0.to_string())
}
#[component]
fn Generic<T: Clone + PartialEq + ToString + 'static>(value: T) -> RsxNode {
    RsxNode::text(value.to_string())
}

#[test]
fn callback_and_opaque_props_never_reuse_stale_output() {
    let build = |v| {
        render_pass(
            || rsx! { <Element><Callback callback={Rc::new(move || v) as Rc<dyn Fn()->usize>}/><Unknown value={Opaque(v)}/></Element> },
        )
    };
    assert_eq!(text(&build(1)), "11");
    let (node, work) = profile_ui_work(|| build(2));
    assert_eq!(work.component_renders, 2);
    assert_eq!(text(&node), "22");
    let (_, work) = profile_ui_work(|| build(2));
    assert_eq!(work.component_renders, 2);
}

#[test]
fn generic_partial_eq_bounds_enable_comparison() {
    let build = || render_pass(|| rsx! { <Generic::<usize> value={7usize}/> });
    build();
    let (node, work) = profile_ui_work(build);
    assert_eq!(text(&node), "7");
    assert_eq!(work.memo_hits, 1);
}

thread_local! {
    static CAPTURED: RefCell<Option<State<usize>>> = const { RefCell::new(None) };
    static MOUNTS: Cell<usize> = const { Cell::new(0) };
    static CLEANUPS: Cell<usize> = const { Cell::new(0) };
    static EXTERNAL: Cell<usize> = const { Cell::new(0) };
}
#[component]
fn Stateful() -> RsxNode {
    let s = use_state(|| 0usize);
    CAPTURED.with(|slot| *slot.borrow_mut() = Some(s.clone()));
    use_mount(|| {
        MOUNTS.set(MOUNTS.get() + 1);
        || CLEANUPS.set(CLEANUPS.get() + 1)
    });
    rsx! { <Element>{s.get().to_string()}</Element> }
}
#[component]
fn Parent() -> RsxNode {
    rsx! { <Element><Stateful/><Row value={8}/></Element> }
}

#[test]
fn state_invalidates_cached_ancestors_preserves_siblings_and_hook_lifetimes() {
    let build = || render_pass(|| rsx! { <Parent/> });
    build();
    let old = CAPTURED.with(|slot| slot.borrow().as_ref().unwrap().clone());
    let (_, work) = profile_ui_work(build);
    assert_eq!(work.component_renders, 0);
    assert_eq!(MOUNTS.get(), 1);
    batch_state_updates(|| old.set(3));
    let (node, work) = profile_ui_work(build);
    assert_eq!(text(&node), "38");
    assert_eq!(work.component_renders, 2);
    assert_eq!(work.memo_hits, 1);
    assert_eq!(old.get(), 0);
    assert_eq!(MOUNTS.get(), 1);
    assert_eq!(CLEANUPS.get(), 0);
    let _ = render_pass(|| rsx! { <Row value={1}/> });
    assert_eq!(CLEANUPS.get(), 1);
    build();
    batch_state_updates(|| old.set(9));
    assert_eq!(text(&build()), "08");
    assert_eq!(MOUNTS.get(), 2);
}

#[component(no_memo)]
fn External() -> RsxNode {
    RsxNode::text(EXTERNAL.get().to_string())
}
#[component]
fn ExternalParent() -> RsxNode {
    rsx! { <Element><External/></Element> }
}

#[test]
fn explicit_opt_out_also_prevents_ancestor_bailout() {
    let build = || render_pass(|| rsx! { <ExternalParent/> });
    assert_eq!(text(&build()), "0");
    EXTERNAL.set(1);
    let (node, work) = profile_ui_work(build);
    assert_eq!(text(&node), "1");
    assert_eq!(work.component_renders, 2);
}

#[derive(Clone)]
struct Theme(&'static str);
#[component]
fn ContextReader() -> RsxNode {
    RsxNode::text(use_context::<Theme>().map_or("missing", |t| t.0))
}
#[component]
fn ContextParent(version: usize) -> RsxNode {
    let _ = version;
    rsx! { <Element><ContextReader/></Element> }
}

#[test]
fn context_publication_changes_invalidate_equal_props_and_missing_reads() {
    let build = |value: Option<Rc<dyn Any>>| {
        let render = || render_pass(|| rsx! { <ContextParent version={0}/> });
        match value {
            Some(v) => with_pushed_context_raw(TypeId::of::<Theme>(), v, render),
            None => render(),
        }
    };
    assert_eq!(text(&build(None)), "missing");
    let dark: Rc<dyn Any> = Rc::new(Theme("dark"));
    assert_eq!(text(&build(Some(dark.clone()))), "dark");
    let (_, work) = profile_ui_work(|| build(Some(dark.clone())));
    assert_eq!(work.component_renders, 0);
    assert_eq!(text(&build(Some(Rc::new(Theme("light"))))), "light");
    assert_eq!(text(&build(None)), "missing");
}

#[test]
fn cached_child_context_reads_are_replayed_when_parent_props_change() {
    let dark: Rc<dyn Any> = Rc::new(Theme("dark"));
    let build = |version, value| {
        with_pushed_context_raw(TypeId::of::<Theme>(), value, || {
            render_pass(|| rsx! { <ContextParent version={version}/> })
        })
    };
    build(0, dark.clone());
    let (_, work) = profile_ui_work(|| build(1, dark));
    assert_eq!(work.component_renders, 1);
    assert_eq!(work.memo_hits, 1);
    let (node, work) = profile_ui_work(|| build(1, Rc::new(Theme("light"))));
    assert_eq!(text(&node), "light");
    assert_eq!(work.component_renders, 2);
}

#[component]
fn InternalProvider() -> RsxNode {
    provide_context_node(Theme("inner"), rsx! { <ContextReader/> })
}

#[test]
fn internal_provider_shadowing_does_not_become_an_external_dependency() {
    let build =
        |outer| render_pass(|| provide_context_node(Theme(outer), rsx! { <InternalProvider/> }));
    assert_eq!(text(&build("outer1")), "inner");
    let (node, work) = profile_ui_work(|| build("outer2"));
    assert_eq!(text(&node), "inner");
    assert_eq!(work.component_renders, 0);
    assert_eq!(work.memo_hits, 1);
}

#[component]
fn ContextBinding() -> RsxNode {
    let b = use_context::<Binding<usize>>().unwrap();
    RsxNode::text(b.snapshot().get().to_string())
}

#[test]
fn a_binding_inside_an_unchanged_context_publication_still_tracks_its_target() {
    let binding = Binding::new(0usize);
    let publication: Rc<dyn Any> = Rc::new(binding.clone());
    let build = || {
        with_pushed_context_raw(TypeId::of::<Binding<usize>>(), publication.clone(), || {
            render_pass(|| rsx! { <ContextBinding/> })
        })
    };
    build();
    batch_state_updates(|| binding.set(2));
    assert_eq!(text(&build()), "2");
}

#[derive(Clone, PartialEq)]
struct Tracked {
    value: usize,
    lifetime: Rc<()>,
}
#[component]
fn OwnedProps(value: Tracked, fail: bool) -> RsxNode {
    assert!(!fail, "render panic");
    RsxNode::text(value.value.to_string())
}

#[test]
fn erased_props_are_released_on_hit_replacement_unmount_and_panic() {
    let lifetime = Rc::new(());
    let build = |value, fail| {
        render_pass(
            || rsx! { <OwnedProps value={Tracked { value, lifetime: lifetime.clone() }} fail={fail}/> },
        )
    };
    build(0, false);
    assert_eq!(Rc::strong_count(&lifetime), 2);
    build(0, false);
    assert_eq!(Rc::strong_count(&lifetime), 2);
    build(1, false);
    assert_eq!(Rc::strong_count(&lifetime), 2);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_pushed_context_raw(TypeId::of::<Theme>(), Rc::new(Theme("temporary")), || {
            build(2, true)
        })
    }));
    assert!(result.is_err());
    assert!(use_context::<Theme>().is_none());
    assert_eq!(Rc::strong_count(&lifetime), 2);
    let (node, work) = profile_ui_work(|| build(1, false));
    assert_eq!(work.component_renders, 1);
    assert_eq!(text(&node), "1");
    let _ = render_pass(|| rsx! { <Row value={3}/> });
    assert_eq!(Rc::strong_count(&lifetime), 1);
}

#[component]
fn ConditionalContext(read: bool) -> RsxNode {
    if read {
        RsxNode::text(use_context::<Theme>().unwrap().0)
    } else {
        RsxNode::text("constant")
    }
}

#[test]
fn context_dependencies_disappear_when_no_longer_read() {
    let build = |read, name| {
        with_pushed_context_raw(TypeId::of::<Theme>(), Rc::new(Theme(name)), || {
            render_pass(|| rsx! { <ConditionalContext read={read}/> })
        })
    };
    build(true, "one");
    build(false, "two");
    let (node, work) = profile_ui_work(|| build(false, "three"));
    assert_eq!(text(&node), "constant");
    assert_eq!(work.component_renders, 0);
}

struct Handwritten;
#[derive(Clone, Default, PartialEq)]
struct HandwrittenProps(usize);
impl crate::ui::RsxComponent<HandwrittenProps> for Handwritten {
    fn render(props: HandwrittenProps, _: Vec<RsxNode>) -> RsxNode {
        RsxNode::text(props.0.to_string())
    }
}
#[component]
impl crate::ui::RsxTag for Handwritten {
    type Props = HandwrittenProps;
    type StrictProps = HandwrittenProps;
    const ACCEPTS_CHILDREN: bool = false;
    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props
    }
    fn create_node(
        props: Self::StrictProps,
        children: Vec<RsxNode>,
        _: Option<crate::ui::RsxKey>,
    ) -> RsxNode {
        <Self as crate::ui::RsxComponent<HandwrittenProps>>::render(props, children)
    }
}

#[test]
fn impl_form_uses_declared_props_equality() {
    let build = || {
        render_pass(|| crate::ui::create_element::<Handwritten>(HandwrittenProps(7), vec![], None))
    };
    build();
    let (node, work) = profile_ui_work(build);
    assert_eq!(text(&node), "7");
    assert_eq!(work.memo_hits, 1);
}
