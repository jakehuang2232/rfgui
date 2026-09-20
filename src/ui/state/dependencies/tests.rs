use super::*;
use crate::ui::{RsxNode, profile_ui_work};

struct Probe;
struct Owner;

fn memo(key: u64, renders: &Cell<usize>, render: impl FnOnce() -> RsxNode) -> RsxNode {
    with_component_key(Some(RsxKey::Local(key)), || {
        render_memoized_component::<Probe, _>((), |_| {
            renders.set(renders.get() + 1);
            render()
        })
    })
}

#[test]
fn separate_slots_of_one_owner_invalidate_only_their_readers() {
    let a_renders = Cell::new(0);
    let b_renders = Cell::new(0);
    let build = || {
        build_scope(|| {
            let (a, b) = render_component::<Owner, _>(|| (use_state(|| 0), use_state(|| 10)));
            let a_node = memo(1, &a_renders, || RsxNode::text(a.get().to_string()));
            let b_node = memo(2, &b_renders, || RsxNode::text(b.get().to_string()));
            (a, b, a_node, b_node)
        })
    };
    let first = build();
    assert_ne!(first.0.payload.target, first.1.payload.target);
    assert_eq!(
        first.0.payload.target,
        first.0.binding().snapshot().prop_payload.target
    );
    let (_, work) = profile_ui_work(|| batch_state_updates(|| first.0.set(1)));
    assert_eq!(work.memo_invalidation_visits, 1);
    assert_eq!(work.memo_invalidations, 1);
    let next = build();
    assert_eq!((a_renders.get(), b_renders.get()), (2, 1));
    assert_eq!(next.2, RsxNode::text("1"));
    assert!(RsxNode::ptr_eq(&first.3, &next.3));
    assert_eq!(first.0.get(), 0);
}

#[test]
fn globals_and_free_bindings_preserve_unrelated_memos() {
    #[derive(Clone, PartialEq)]
    struct Global(i32);
    let global = global_state(|| Global(0));
    let free = Binding::new(10);
    let counters = [Cell::new(0), Cell::new(0), Cell::new(0)];
    let build = || {
        build_scope(|| {
            memo(1, &counters[0], || {
                RsxNode::text(use_global_state::<Global>().get().0.to_string())
            });
            memo(2, &counters[1], || {
                RsxNode::text(free.snapshot().get().to_string())
            });
            memo(3, &counters[2], || RsxNode::text("static"));
        })
    };
    build();
    let (_, work) = profile_ui_work(|| batch_state_updates(|| global.binding().set(Global(1))));
    assert_eq!(work.memo_invalidation_visits, 1);
    build();
    assert_eq!(counters.each_ref().map(Cell::get), [2, 1, 1]);
    let (_, work) = profile_ui_work(|| batch_state_updates(|| free.set(11)));
    assert_eq!(work.memo_invalidation_visits, 1);
    build();
    assert_eq!(counters.each_ref().map(Cell::get), [2, 2, 1]);
    assert_eq!(global.payload.target, global.binding().prop_payload.target);
}

#[test]
fn a_batch_deduplicates_consumers_and_waits_for_all_commits() {
    let a = Binding::new(0);
    let b = Binding::new(0);
    let count = Cell::new(0);
    let build = || {
        build_scope(|| {
            memo(1, &count, || {
                RsxNode::text(format!("{}:{}", a.snapshot().get(), b.snapshot().get()))
            })
        })
    };
    build();
    let (_, work) = profile_ui_work(|| {
        batch_state_updates(|| {
            a.set(1);
            b.update(|n| {
                STORE.with(|store| assert!(store.borrow().dirty_memo_components.is_empty()));
                *n = 2;
            });
        })
    });
    assert_eq!(work.changed_targets, 2);
    assert_eq!(work.memo_invalidation_visits, 1);
    assert_eq!(work.memo_invalidations, 1);
    assert_eq!(build(), RsxNode::text("1:2"));
    assert_eq!(count.get(), 2);
}

#[test]
fn conditional_dependencies_are_replaced_after_render() {
    let choose_a = Binding::new(true);
    let a = Binding::new(1);
    let b = Binding::new(2);
    let count = Cell::new(0);
    let build = || {
        build_scope(|| {
            memo(1, &count, || {
                let value = if choose_a.snapshot().get() {
                    a.snapshot().get()
                } else {
                    b.snapshot().get()
                };
                RsxNode::text(value.to_string())
            })
        })
    };
    build();
    batch_state_updates(|| choose_a.set(false));
    assert_eq!(build(), RsxNode::text("2"));
    let (_, work) = profile_ui_work(|| batch_state_updates(|| a.set(3)));
    assert_eq!(work.memo_invalidation_visits, 0);
    build();
    assert_eq!(count.get(), 2);
    STORE.with(|store| {
        assert!(
            !store
                .borrow()
                .target_consumers
                .contains_key(&a.prop_payload.target)
        )
    });
    batch_state_updates(|| b.set(4));
    assert_eq!(build(), RsxNode::text("4"));
    assert_eq!(count.get(), 3);
}

#[test]
fn cached_child_dependencies_propagate_into_a_new_parent_render() {
    struct Parent;
    let binding = Binding::new(0);
    let parent = Cell::new(0);
    let child = Cell::new(0);
    let build = |props| {
        build_scope(|| {
            render_memoized_component::<Parent, _>(props, |_| {
                parent.set(parent.get() + 1);
                memo(1, &child, || {
                    RsxNode::text(binding.snapshot().get().to_string())
                })
            })
        })
    };
    build(0);
    build(1); // Parent props miss, child memo hit.
    assert_eq!((parent.get(), child.get()), (2, 1));
    let (_, work) = profile_ui_work(|| batch_state_updates(|| binding.set(1)));
    assert_eq!(work.memo_invalidation_visits, 2);
    assert_eq!(build(1), RsxNode::text("1"));
    assert_eq!((parent.get(), child.get()), (3, 2));
}

#[test]
fn unread_owned_state_invalidates_resolved_ancestors_but_not_siblings() {
    struct Parent;
    let state = RefCell::new(None);
    let parent = Cell::new(0);
    let sibling = Cell::new(0);
    let build = || {
        build_scope(|| {
            render_memoized_component::<Parent, _>((), |_| {
                parent.set(parent.get() + 1);
                render_component::<Owner, _>(|| *state.borrow_mut() = Some(use_state(|| 0)));
                memo(1, &sibling, || RsxNode::text("sibling"))
            })
        })
    };
    build();
    let (_, work) =
        profile_ui_work(|| batch_state_updates(|| state.borrow().as_ref().unwrap().set(1)));
    assert_eq!(work.memo_invalidation_visits, 1);
    build();
    assert_eq!((parent.get(), sibling.get()), (2, 1));
}

#[test]
fn unmount_removes_reverse_edges_and_remount_allocates_new_target() {
    let captured = RefCell::new(None);
    let free = Binding::new(1);
    let count = Cell::new(0);
    let build = || {
        build_scope(|| {
            memo(1, &count, || {
                let state = use_state(|| 0);
                *captured.borrow_mut() = Some(state);
                RsxNode::text(free.snapshot().get().to_string())
            })
        })
    };
    build();
    let old = captured.borrow().as_ref().unwrap().clone();
    build_scope(|| render_component::<Owner, _>(|| {}));
    STORE.with(|store| {
        let store = store.borrow();
        assert!(store.target_consumers.is_empty());
        assert!(store.component_memos.is_empty());
        assert!(store.dirty_memo_components.is_empty());
    });
    let (_, work) = profile_ui_work(|| batch_state_updates(|| free.set(2)));
    assert_eq!(work.memo_invalidation_visits, 0);
    build();
    let new = captured.borrow().as_ref().unwrap().clone();
    assert_ne!(old.payload.target, new.payload.target);
    take_state_dirty();
    let (_, work) = profile_ui_work(|| batch_state_updates(|| old.set(99)));
    assert_eq!(work.changed_targets, 0);
    assert_eq!(take_state_dirty(), UiDirtyState::NONE);
    build();
    assert_eq!(count.get(), 2);
    assert_eq!(new.get(), 0);
}

#[test]
fn no_op_and_redraw_only_batches_do_not_invalidate_memos() {
    let a = Binding::new(0);
    let redraw = Binding::new_with_dirty_state(0, UiDirtyState::REDRAW);
    let count = Cell::new(0);
    let build = || {
        build_scope(|| {
            memo(1, &count, || {
                RsxNode::text(format!(
                    "{}:{}",
                    a.snapshot().get(),
                    redraw.snapshot().get()
                ))
            })
        })
    };
    build();
    let (_, work) = profile_ui_work(|| {
        batch_state_updates(|| {
            a.set(1);
            a.set(0);
            redraw.set(1);
        })
    });
    assert_eq!(work.changed_targets, 1);
    assert_eq!(work.memo_invalidation_visits, 0);
    assert!(take_state_dirty().is_redraw_only());
    build();
    assert_eq!(count.get(), 1);
}

#[test]
fn updater_panic_still_invalidates_values_already_committed() {
    let a = Binding::new(0);
    let b = Binding::new(0);
    let count = Cell::new(0);
    let build =
        || build_scope(|| memo(1, &count, || RsxNode::text(a.snapshot().get().to_string())));
    build();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        batch_state_updates(|| {
            a.set(1);
            b.update(|_| panic!("updater"));
        });
    }));
    assert!(result.is_err());
    assert!(take_state_dirty().needs_rebuild());
    assert!(!FLUSHING_STATE.get());
    assert_eq!(build(), RsxNode::text("1"));
    assert_eq!(count.get(), 2);
}

#[test]
fn failed_render_never_revives_an_old_cache_entry() {
    let a = Binding::new(0);
    let count = Cell::new(0);
    let build = |fail: bool| {
        build_scope(|| {
            memo(1, &count, || {
                assert!(!fail, "render failed");
                RsxNode::text(a.snapshot().get().to_string())
            })
        })
    };
    build(false);
    batch_state_updates(|| a.set(1));
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build(true))).is_err());
    assert_eq!(build(false), RsxNode::text("1"));
    assert_eq!(count.get(), 3);
}

#[test]
fn binding_props_record_dependencies_without_a_value_read() {
    let binding = Binding::new(0);
    let count = Cell::new(0);
    let build = || {
        let current = binding.snapshot(); // Acquired outside the memo frame.
        build_scope(|| {
            memo(1, &count, || {
                let prop = current.into_prop_value();
                let recovered = Binding::<i32>::from_prop_value(prop).unwrap();
                assert_eq!(recovered.prop_payload.target, binding.prop_payload.target);
                RsxNode::text("binding prop")
            })
        })
    };
    build();
    let (_, work) = profile_ui_work(|| batch_state_updates(|| binding.set(1)));
    assert_eq!(work.memo_invalidation_visits, 1);
    build();
    assert_eq!(count.get(), 2);
}

#[test]
fn sparse_update_visits_one_consumer_among_many_cached_memos() {
    let bindings: Vec<_> = (0..256).map(|_| Binding::new(0)).collect();
    let counters: Vec<_> = (0..256).map(|_| Cell::new(0)).collect();
    let build = || {
        build_scope(|| {
            for (i, (binding, count)) in bindings.iter().zip(&counters).enumerate() {
                memo(i as u64, count, || {
                    RsxNode::text(binding.snapshot().get().to_string())
                });
            }
        })
    };
    build();
    let (_, work) = profile_ui_work(|| batch_state_updates(|| bindings[128].set(1)));
    assert_eq!(work.memo_invalidation_visits, 1);
    assert_eq!(work.memo_invalidations, 1);
    let (_, work) = profile_ui_work(build);
    assert_eq!(work.memo_hits, 255);
    assert_eq!(counters.iter().map(Cell::get).sum::<usize>(), 257);
}

#[test]
fn props_miss_replaces_dependencies_and_failed_props_miss_stays_dirty() {
    let a = Binding::new(1);
    let b = Binding::new(2);
    let count = Cell::new(0);
    let build = |read_a: bool, fail: bool| {
        build_scope(|| {
            render_memoized_component::<Probe, _>(read_a, |read_a| {
                count.set(count.get() + 1);
                assert!(!fail, "props miss panic");
                let source = if *read_a { &a } else { &b };
                RsxNode::text(source.snapshot().get().to_string())
            })
        })
    };
    build(true, false);
    build(false, false);
    let (_, work) = profile_ui_work(|| batch_state_updates(|| a.set(3)));
    assert_eq!(work.memo_invalidation_visits, 0);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build(true, true))).is_err());
    // Returning to the previously cached props must still render after failure.
    assert_eq!(build(false, false), RsxNode::text("2"));
    assert_eq!(count.get(), 4);
}

#[test]
fn pointer_state_invalidates_only_subscribers_and_their_cached_ancestors() {
    let subscriber = Cell::new(0);
    let other = Cell::new(0);
    let build = || {
        build_scope(|| {
            let node = memo(1, &subscriber, || {
                RsxNode::text(format!("{:?}", use_viewport_pointer_position()))
            });
            memo(2, &other, || RsxNode::text("static"));
            node
        })
    };
    build();
    VIEWPORT_POINTER_STATE.with(|state| state.borrow_mut().position = Some((5.0, 7.0)));
    let (_, work) = profile_ui_work(notify_viewport_pointer_state_changed);
    assert_eq!(work.memo_invalidation_visits, 1);
    assert_eq!(build(), RsxNode::text("Some((5.0, 7.0))"));
    assert_eq!((subscriber.get(), other.get()), (2, 1));
}

#[test]
fn retired_memo_props_drop_outside_the_state_store_borrow() {
    #[derive(Clone)]
    struct Props(u32, Rc<Cell<usize>>);
    impl PartialEq for Props {
        fn eq(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }
    impl Drop for Props {
        fn drop(&mut self) {
            STORE.with(|store| assert!(store.try_borrow().is_ok()));
            self.1.set(self.1.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let build = |version| {
        build_scope(|| {
            render_memoized_component::<Probe, _>(Props(version, drops.clone()), |_| {
                RsxNode::text("props")
            })
        })
    };
    build(0);
    build(1);
    assert_eq!(drops.get(), 1);
    build_scope(|| render_component::<Owner, _>(|| {}));
    assert_eq!(drops.get(), 2);
}
