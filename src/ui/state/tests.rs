use super::{
    UiDirtyState, build_scope, next_timer_deadline, render_memoized_component, run_due_timers,
    take_state_dirty, use_interval, use_mount, use_state, use_timeout, with_component_key,
};
use crate::time::{Duration, Instant};
use crate::ui::{GlobalKey, RsxKey, RsxNode};
use crate::view::Element;
use std::cell::Cell;
use std::rc::Rc;

mod timer_lifecycle_tests;

fn clear_test_timers() {
    build_scope(|| {
        crate::ui::render_component::<u8, _>(|| {});
    });
}

#[test]
fn non_component_scope_does_not_reset_use_state_slots() {
    let state_before = build_scope(|| {
        crate::ui::render_component::<u32, _>(|| {
            let value = use_state(|| 0_i32);
            value.set(7);
            value
        })
    });
    assert_eq!(state_before.get(), 0); // The captured render snapshot stays fixed.
    let _ = take_state_dirty();

    let _ = build_scope(|| {
        RsxNode::tagged(
            "Element",
            crate::ui::RsxTagDescriptor::for_tag::<crate::view::Element>(),
        )
    });

    let state_after = build_scope(|| {
        crate::ui::render_component::<u32, _>(|| {
            let value = use_state(|| 0_i32);
            value
        })
    });
    assert_eq!(state_after.get(), 7);
}

// 軌 1 #13 regression: host tag `create_element` must not flip
// `components_rendered_in_build`, so a `build_scope` that only builds
// host tags exits without pruning the main render's state slots.
#[test]
fn host_tag_only_build_scope_does_not_prune_user_state() {
    let state = build_scope(|| {
        crate::ui::render_component::<u32, _>(|| {
            let value = use_state(|| 0_i32);
            value.set(99);
            value
        })
    });
    assert_eq!(state.get(), 0);
    let _ = take_state_dirty();

    // Simulate handler-triggered `rsx!` producing only host tags.
    // Goes through the exact `create_element` path the handler's rsx! hits.
    let _ = build_scope(|| {
        crate::ui::create_element::<crate::view::Element>(
            crate::view::ElementPropSchema::default(),
            Vec::new(),
            None,
        )
    });

    // Re-render main component — state must survive.
    let after = build_scope(|| {
        crate::ui::render_component::<u32, _>(|| {
            let value = use_state(|| 0_i32);
            value.get()
        })
    });
    assert_eq!(after, 99);
}

thread_local! {
    static PASS_ROOT_COUNT: std::cell::RefCell<Option<super::State<i32>>> =
        const { std::cell::RefCell::new(None) };
    static DETACHED_LEAF_RENDERS: Cell<usize> = const { Cell::new(0) };
}

#[crate::ui::component]
fn PassRoot(children: Vec<RsxNode>) -> RsxNode {
    let count = use_state(|| 0_i32);
    PASS_ROOT_COUNT.with(|slot| *slot.borrow_mut() = Some(count.clone()));
    crate::ui::rsx! { <Element>{count.get().to_string()}{children}</Element> }
}

#[crate::ui::component]
fn DetachedLeaf() -> RsxNode {
    DETACHED_LEAF_RENDERS.with(|renders| renders.set(renders.get() + 1));
    let label = use_state(|| String::from("leaf"));
    RsxNode::text(label.get())
}

fn rendered_text(node: &RsxNode) -> String {
    match node {
        RsxNode::Text(text) => text.content.clone(),
        RsxNode::Element(element) => element.children.iter().map(rendered_text).collect(),
        RsxNode::Fragment(fragment) => fragment.children.iter().map(rendered_text).collect(),
        RsxNode::Component(_) | RsxNode::Provider(_) => panic!("unresolved node"),
    }
}

// An event handler or timer may describe a user component with `rsx!`
// outside any root render. Describing must neither render the component
// nor retire the state of the tree that the last pass rendered.
#[test]
fn rsx_outside_root_render_defers_component_and_keeps_tree_state() {
    let render = |children: Vec<RsxNode>| {
        crate::ui::render_root(|| crate::ui::rsx! { <PassRoot>{children}</PassRoot> })
    };
    assert_eq!(rendered_text(&render(Vec::new())), "0");
    let count = PASS_ROOT_COUNT.with(|slot| slot.borrow().clone().expect("root rendered"));

    let detached = crate::ui::batch_state_updates(|| {
        count.set(5);
        crate::ui::rsx! { <DetachedLeaf /> }
    });
    assert!(matches!(detached, RsxNode::Component(_)));
    assert_eq!(DETACHED_LEAF_RENDERS.with(Cell::get), 0);

    assert_eq!(rendered_text(&render(Vec::new())), "5");
    // The deferred description renders once a root render places it in the tree.
    assert_eq!(rendered_text(&render(vec![detached])), "5leaf");
    assert_eq!(DETACHED_LEAF_RENDERS.with(Cell::get), 1);
}

#[test]
fn keyed_component_keeps_state_when_order_changes() {
    let first = build_scope(|| {
        let a = with_component_key(Some(RsxKey::Local(1)), || {
            crate::ui::render_component::<u32, _>(|| {
                let state = use_state(|| 10_i32);
                state.get()
            })
        });
        let b = with_component_key(Some(RsxKey::Local(2)), || {
            crate::ui::render_component::<u32, _>(|| {
                let state = use_state(|| 20_i32);
                state.get()
            })
        });
        (a, b)
    });
    assert_eq!(first, (10, 20));

    let second = build_scope(|| {
        let b = with_component_key(Some(RsxKey::Local(2)), || {
            crate::ui::render_component::<u32, _>(|| {
                let state = use_state(|| 999_i32);
                state.get()
            })
        });
        let a = with_component_key(Some(RsxKey::Local(1)), || {
            crate::ui::render_component::<u32, _>(|| {
                let state = use_state(|| 999_i32);
                state.get()
            })
        });
        (b, a)
    });
    assert_eq!(second, (20, 10));
}

#[test]
fn global_key_component_keeps_state_when_parent_changes() {
    let global_key = GlobalKey::from("shared-child");

    let first = build_scope(|| {
        let left = crate::ui::render_component::<u8, _>(|| {
            with_component_key(Some(RsxKey::Global(global_key)), || {
                crate::ui::render_component::<u32, _>(|| {
                    let state = use_state(|| 5_i32);
                    state.set(42);
                    state.get()
                })
            })
        });
        let _right = crate::ui::render_component::<u16, _>(|| 0_i32);
        left
    });
    assert_eq!(first, 5);

    let second = build_scope(|| {
        let _left = crate::ui::render_component::<u8, _>(|| 0_i32);
        crate::ui::render_component::<u16, _>(|| {
            with_component_key(Some(RsxKey::Global(global_key)), || {
                crate::ui::render_component::<u32, _>(|| {
                    let state = use_state(|| 999_i32);
                    state.get()
                })
            })
        })
    });
    assert_eq!(second, 42);
}

#[test]
fn use_timeout_fires_once_and_disables_itself() {
    clear_test_timers();
    let fired = Rc::new(Cell::new(0));
    let fired_for_hook = fired.clone();

    build_scope(|| {
        crate::ui::render_component::<u32, _>(|| {
            use_timeout(true, Duration::from_millis(10), move || {
                fired_for_hook.set(fired_for_hook.get() + 1);
            });
        })
    });

    let deadline = next_timer_deadline().expect("timeout should schedule a deadline");
    run_due_timers(deadline);
    assert_eq!(fired.get(), 1);
    assert!(next_timer_deadline().is_none());

    run_due_timers(deadline + Duration::from_millis(10));
    assert_eq!(fired.get(), 1);
    clear_test_timers();
}

#[test]
fn use_interval_resets_when_reenabled_or_duration_changes() {
    clear_test_timers();
    let fired = Rc::new(Cell::new(0));
    let build = |enabled: bool, duration_ms: u64, fired: Rc<Cell<i32>>| {
        build_scope(|| {
            crate::ui::render_component::<u64, _>(|| {
                use_interval(enabled, Duration::from_millis(duration_ms), move || {
                    fired.set(fired.get() + 1);
                });
            })
        });
    };

    build(true, 20, fired.clone());
    let first_deadline = next_timer_deadline().expect("interval should schedule");
    run_due_timers(first_deadline);
    assert_eq!(fired.get(), 1);

    build(false, 20, fired.clone());
    assert!(next_timer_deadline().is_none());
    run_due_timers(Instant::now() + Duration::from_secs(1));
    assert_eq!(fired.get(), 1);

    build(true, 40, fired.clone());
    let reset_deadline = next_timer_deadline().expect("reenabled interval should reschedule");
    run_due_timers(reset_deadline);
    assert_eq!(fired.get(), 2);
    clear_test_timers();
}

#[test]
fn set_same_value_does_not_mark_dirty() {
    let state = build_scope(|| {
        crate::ui::render_component::<u32, _>(|| {
            let value = use_state(|| 7_i32);
            value.set(7);
            value
        })
    });
    assert_eq!(state.get(), 7);
    assert_eq!(take_state_dirty(), UiDirtyState::NONE);
}

#[test]
fn update_without_effective_change_does_not_mark_dirty() {
    let state = build_scope(|| {
        crate::ui::render_component::<u32, _>(|| {
            let value = use_state(|| String::from("unchanged"));
            value.update(|text| {
                text.push_str("");
            });
            value
        })
    });
    assert_eq!(state.get(), "unchanged");
    assert_eq!(take_state_dirty(), UiDirtyState::NONE);
}

struct MemoProbeComponent;

#[test]
fn memoized_component_skips_render_when_props_equal() {
    let renders = Rc::new(Cell::new(0));

    let run = |props: i32| -> RsxNode {
        let counter = renders.clone();
        build_scope(|| {
            render_memoized_component::<MemoProbeComponent, _>(props, |_| {
                counter.set(counter.get() + 1);
                RsxNode::text("hit")
            })
        })
    };

    let first = run(1);
    assert_eq!(renders.get(), 1);

    // Same props → cached, render closure NOT invoked.
    let second = run(1);
    assert_eq!(renders.get(), 1);

    // Fast path returns the exact same `Rc` allocation, so the reconciler
    // bailout can short-circuit the entire subtree.
    assert!(
        RsxNode::ptr_eq(&first, &second),
        "memo hit should reuse the cached `Rc<RsxNode>`"
    );

    // Different props → render closure re-runs.
    let _ = run(2);
    assert_eq!(renders.get(), 2);
}

#[test]
fn use_mount_runs_once_and_cleans_up_on_unmount() {
    let mounts = Rc::new(Cell::new(0));
    let cleanups = Rc::new(Cell::new(0));

    let build = |mounts: Rc<Cell<i32>>, cleanups: Rc<Cell<i32>>| {
        build_scope(|| {
            crate::ui::render_component::<u16, _>(|| {
                let mounts = mounts.clone();
                let cleanups = cleanups.clone();
                use_mount(move || {
                    mounts.set(mounts.get() + 1);
                    move || cleanups.set(cleanups.get() + 1)
                });
            })
        });
    };

    // Mount — callback fires once, no cleanup yet.
    build(mounts.clone(), cleanups.clone());
    assert_eq!(mounts.get(), 1);
    assert_eq!(cleanups.get(), 0);

    // Re-render — mount is a no-op.
    build(mounts.clone(), cleanups.clone());
    assert_eq!(mounts.get(), 1);
    assert_eq!(cleanups.get(), 0);

    // Unmount (a different component renders instead) — cleanup fires.
    build_scope(|| {
        crate::ui::render_component::<u32, _>(|| {});
    });
    assert_eq!(mounts.get(), 1);
    assert_eq!(cleanups.get(), 1);
}

#[test]
fn memoized_component_reruns_when_its_own_state_changes() {
    let renders = Rc::new(Cell::new(0));
    let captured_state: Rc<std::cell::RefCell<Option<super::State<i32>>>> =
        Rc::new(std::cell::RefCell::new(None));

    let run = || -> RsxNode {
        let counter = renders.clone();
        let captured = captured_state.clone();
        build_scope(|| {
            render_memoized_component::<MemoProbeComponent, _>((), move |_| {
                counter.set(counter.get() + 1);
                let s = use_state(|| 0_i32);
                *captured.borrow_mut() = Some(s);
                RsxNode::text("hit")
            })
        })
    };

    let _ = run();
    assert_eq!(renders.get(), 1);

    // Same props, untouched state → cache hit, no re-render.
    let _ = run();
    assert_eq!(renders.get(), 1);

    // Mutating the component's own state must invalidate its memo entry.
    captured_state.borrow().as_ref().unwrap().set(7);
    let _ = take_state_dirty();
    let _ = run();
    assert_eq!(renders.get(), 2);
}

#[test]
fn use_state_initializer_may_register_global_key() {
    // RSX built inside an initializer registers its GlobalKeys in the store,
    // so the initializer must not run under the store's mutable borrow.
    let calls = Rc::new(Cell::new(0));
    let render = || {
        build_scope(|| {
            crate::ui::render_component::<u16, _>(|| {
                use_state(|| {
                    calls.set(calls.get() + 1);
                    super::register_global_key(GlobalKey::from("use-state-initializer"));
                    calls.get()
                })
                .get()
            })
        })
    };
    assert_eq!(render(), 1);
    assert_eq!(render(), 1);
    assert_eq!(calls.get(), 1);
}
