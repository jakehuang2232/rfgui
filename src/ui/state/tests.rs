use super::{
    UiDirtyState, build_scope, next_timer_deadline, render_memoized_component, run_due_timers,
    take_state_dirty, use_interval, use_mount, use_state, use_timeout, with_component_key,
};
use crate::time::{Duration, Instant};
use crate::ui::{GlobalKey, RsxKey, RsxNode};
use std::cell::Cell;
use std::rc::Rc;

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
    assert_eq!(state_before.get(), 7);
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
// host tags (e.g. a TextArea `on_render` handler invoking `rsx!`
// during layout) exits without pruning the main render's state slots.
#[test]
fn host_tag_only_build_scope_does_not_prune_user_state() {
    let state = build_scope(|| {
        crate::ui::render_component::<u32, _>(|| {
            let value = use_state(|| 0_i32);
            value.set(99);
            value
        })
    });
    assert_eq!(state.get(), 99);
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
    assert_eq!(first, 42);

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
