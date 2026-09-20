use super::*;

struct Counter;
fn counter() -> State<i32> {
    build_scope(|| render_component::<Counter, _>(|| use_state(|| 0)))
}

#[test]
fn old_state_and_binding_callbacks_keep_their_render_snapshot() {
    let state = counter();
    let binding = state.binding();
    let old = state.clone();
    let callback = move || old.get();
    batch_state_updates(|| {
        state.set(5);
        assert_eq!(state.get(), 0);
        assert_eq!(binding.get(), 0);
        assert_eq!(binding.get_committed(), 0);
    });
    assert_eq!(counter().get(), 5);
    assert_eq!(callback(), 0);
    assert_eq!(binding.get(), 0);
    assert_eq!(binding.snapshot().get(), 5);
}

#[test]
fn replacements_and_functional_updates_reduce_in_order() {
    let state = counter();
    batch_state_updates(|| {
        state.set(state.get() + 1);
        state.set(state.get() + 1);
        state.update(|n| *n += 1);
        state.update(|n| *n *= 3);
    });
    assert_eq!(counter().get(), 6);
    batch_state_updates(|| {
        state.update(|n| *n += 1);
        state.set(42);
    });
    assert_eq!(counter().get(), 42);
}

#[test]
fn standalone_binding_and_global_state_use_the_same_queue() {
    #[derive(Clone, PartialEq)]
    struct Global(i32);
    let binding = Binding::new(1);
    let global = global_state(|| Global(1));
    let global_binding = global.binding();
    batch_state_updates(|| {
        binding.update(|n| *n += 2);
        global.update(|n| n.0 += 2);
        global_binding.update(|n| n.0 *= 4);
        assert_eq!(global.get().0, 1);
        assert_eq!(binding.get(), 1);
    });
    assert_eq!(binding.snapshot().get(), 3);
    assert_eq!(use_global_state::<Global>().get().0, 12);
    assert_eq!(global.get().0, 1);
    assert_eq!(global_binding.get().0, 1);
}

#[test]
fn nested_event_batches_notify_once_after_all_slots_are_committed() {
    let a = Binding::new(0);
    let b = Binding::new(0);
    let notices = Rc::new(RefCell::new(Vec::new()));
    set_redraw_callback({
        let a = a.clone();
        let b = b.clone();
        let notices = notices.clone();
        move || {
            notices
                .borrow_mut()
                .push((a.get_committed(), b.get_committed()))
        }
    });
    batch_state_updates(|| {
        a.set(1);
        batch_state_updates(|| {
            a.update(|n| *n += 1);
            b.set(3);
        });
        assert!(notices.borrow().is_empty());
    });
    clear_redraw_callback();
    assert_eq!(*notices.borrow(), vec![(2, 3)]);
}

#[test]
fn no_effective_change_preserves_snapshot_and_does_not_notify() {
    let binding = Binding::new(0);
    let notices = Rc::new(Cell::new(0));
    set_redraw_callback({
        let notices = notices.clone();
        move || notices.set(notices.get() + 1)
    });
    batch_state_updates(|| {
        binding.set(1);
        binding.set(0);
        binding.update(|n| *n += 0);
    });
    assert_eq!(take_state_dirty(), UiDirtyState::NONE);
    assert_eq!(binding, binding.snapshot());
    assert_eq!(notices.get(), 0);
    clear_redraw_callback();
}

#[test]
fn redraw_only_queue_preserves_its_dirty_contract() {
    let binding = Binding::new_with_dirty_state(0, UiDirtyState::REDRAW);
    binding.set(1);
    assert!(peek_state_dirty().is_redraw_only());
    flush_state_updates();
    assert!(take_state_dirty().is_redraw_only());
    assert_eq!(binding.snapshot().get(), 1);
}

#[test]
fn unmounted_setter_does_not_dirty_or_change_a_remounted_slot() {
    let stale = counter();
    build_scope(|| render_component::<bool, _>(|| ()));
    let fresh = counter();
    let _ = take_state_dirty();
    stale.set(50);
    stale
        .binding()
        .update(|_| panic!("unmounted updater must not run"));
    assert!(!peek_state_dirty().has_any());
    batch_state_updates(|| fresh.update(|n| *n += 1));
    assert_eq!(counter().get(), 1);
    assert_eq!(stale.get(), 0);
}

#[test]
fn queued_updates_are_not_applied_inside_a_render_or_nested_scope() {
    let binding = Binding::new(0);
    binding.set(1);
    build_scope(|| {
        assert_eq!(binding.snapshot().get(), 1);
        binding.set(2);
        build_scope(|| {
            flush_state_updates();
            assert_eq!(binding.snapshot().get(), 1);
        });
    });
    assert_eq!(binding.snapshot().get(), 1);
    assert!(peek_state_dirty().needs_rebuild());
    build_scope(|| assert_eq!(binding.snapshot().get(), 2));
}

#[test]
fn updaters_run_without_borrowing_the_store_or_value() {
    let state = counter();
    let binding = state.binding();
    state.update(move |n| {
        assert_eq!(current_build_depth(), 0);
        assert_eq!(binding.get_committed(), 0);
        *n += 1;
    });
    assert_eq!(counter().get(), 1);
}

#[test]
fn updates_enqueued_during_flush_survive_for_the_next_batch() {
    let a = Binding::new(0);
    let b = Binding::new(0);
    let later = b.clone();
    a.update(move |n| {
        *n = 1;
        later.set(0);
    });
    b.set(5);
    flush_state_updates();
    assert_eq!(b.snapshot().get(), 5);
    flush_state_updates();
    assert_eq!(b.snapshot().get(), 0);
}

#[test]
fn redraw_callback_can_read_build_and_replace_itself() {
    let binding = Binding::new(0);
    let seen = Rc::new(Cell::new(0));
    set_redraw_callback({
        let binding = binding.clone();
        let seen = seen.clone();
        move || {
            clear_redraw_callback();
            build_scope(|| seen.set(binding.snapshot().get()));
        }
    });
    binding.set(9);
    assert_eq!(seen.get(), 9);
    assert_eq!(binding.get(), 0);
}

#[test]
fn mount_and_cleanup_can_enqueue_updates_without_reentrant_borrows() {
    let binding = Binding::new(0);
    let mount_binding = binding.clone();
    build_scope(|| {
        render_component::<Counter, _>(|| {
            use_mount(move || {
                mount_binding.set(1);
                move || {
                    assert_eq!(current_build_depth(), 0);
                    mount_binding.set(2);
                }
            });
        })
    });
    assert_eq!(binding.snapshot().get(), 0);
    build_scope(|| render_component::<bool, _>(|| ()));
    assert_eq!(binding.snapshot().get(), 1);
    flush_state_updates();
    assert_eq!(binding.snapshot().get(), 2);
}

#[test]
fn descendant_state_invalidates_ancestor_memo_but_preserves_sibling() {
    struct Parent;
    struct Sibling;
    let saved = Rc::new(RefCell::new(None::<State<i32>>));
    let renders = Cell::new(0);
    let sibling_renders = Cell::new(0);
    let run = || {
        build_scope(|| {
            render_memoized_component::<Parent, _>((), |_| {
                renders.set(renders.get() + 1);
                render_component::<Counter, _>(|| {
                    let value = use_state(|| 0);
                    *saved.borrow_mut() = Some(value.clone());
                    crate::ui::RsxNode::text(value.get().to_string())
                })
            });
            render_memoized_component::<Sibling, _>((), |_| {
                sibling_renders.set(sibling_renders.get() + 1);
                crate::ui::RsxNode::text("sibling")
            });
        })
    };
    run();
    run();
    saved.borrow().as_ref().unwrap().set(7);
    run();
    assert_eq!(renders.get(), 2);
    assert_eq!(sibling_renders.get(), 1);
    assert_eq!(saved.borrow().as_ref().unwrap().get(), 7);
}

#[test]
fn memo_hit_keeps_mount_alive() {
    let mounts = Rc::new(Cell::new(0));
    let cleanups = Rc::new(Cell::new(0));
    let run = || {
        build_scope(|| {
            render_memoized_component::<Counter, _>((), |_| {
                let mounts = mounts.clone();
                let cleanups = cleanups.clone();
                use_mount(move || {
                    mounts.set(mounts.get() + 1);
                    move || cleanups.set(cleanups.get() + 1)
                });
                crate::ui::RsxNode::text("mounted")
            })
        })
    };
    run();
    run();
    assert_eq!(mounts.get(), 1);
    assert_eq!(cleanups.get(), 0);
    build_scope(|| render_component::<bool, _>(|| ()));
    assert_eq!(cleanups.get(), 1);
}

#[test]
fn caught_render_panic_does_not_poison_the_next_build() {
    let result = std::panic::catch_unwind(|| {
        build_scope(|| {
            render_component::<Counter, _>(|| {
                use_state(|| 0);
                panic!("render failed");
            })
        });
    });
    assert!(result.is_err());
    assert_eq!(current_build_depth(), 0);
    let value = counter();
    batch_state_updates(|| value.set(3));
    assert_eq!(counter().get(), 3);
}

#[test]
fn binding_props_share_identity_per_snapshot_and_preserve_old_values() {
    let binding = Binding::new(1);
    let old_prop = binding.clone().into_prop_value();
    assert_eq!(old_prop, binding.clone().into_prop_value());
    batch_state_updates(|| binding.set(2));
    let new_prop = binding.snapshot().into_prop_value();
    assert_ne!(old_prop, new_prop);
    assert_eq!(new_prop, binding.snapshot().into_prop_value());
    assert_eq!(Binding::<i32>::from_prop_value(old_prop).unwrap().get(), 1);
    assert_eq!(Binding::<i32>::from_prop_value(new_prop).unwrap().get(), 2);
}

#[test]
fn sibling_memo_reading_a_shared_binding_is_invalidated() {
    struct Consumer;
    let saved = RefCell::new(None::<State<i32>>);
    let seen = Cell::new(0);
    let renders = Cell::new(0);
    let run = || {
        build_scope(|| {
            render_component::<Counter, _>(|| {
                *saved.borrow_mut() = Some(use_state(|| 0));
            });
            render_memoized_component::<Consumer, _>((), |_| {
                let value = saved.borrow().as_ref().unwrap().binding();
                seen.set(value.get());
                renders.set(renders.get() + 1);
                crate::ui::RsxNode::text("consumer")
            });
        })
    };
    run();
    run();
    saved.borrow().as_ref().unwrap().set(6);
    run();
    assert_eq!(seen.get(), 6);
    assert_eq!(renders.get(), 2);
}
