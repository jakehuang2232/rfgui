use super::*;

#[test]
fn unmount_during_dispatch_cancels_other_due_hooks() {
    clear_test_timers();
    build_scope(|| {
        crate::ui::render_component::<u32, _>(|| {
            use_timeout(true, Duration::ZERO, || {
                clear_test_timers();
            });
            use_timeout(true, Duration::ZERO, || {
                panic!("unmounted hook must not fire")
            });
        });
    });
    run_due_timers(Instant::now() + Duration::from_secs(1));
    assert_eq!(next_timer_deadline(), None);
}

#[test]
fn callback_can_rebuild_and_rearm_its_own_hook() {
    clear_test_timers();
    let count = Rc::new(Cell::new(0));
    let fired = count.clone();
    build_scope(|| {
        crate::ui::render_component::<u32, _>(|| {
            use_timeout(true, Duration::ZERO, move || {
                fired.set(fired.get() + 1);
                let fired = fired.clone();
                build_scope(|| {
                    crate::ui::render_component::<u32, _>(|| {
                        use_timeout(true, Duration::from_secs(1), move || {
                            fired.set(fired.get() + 1);
                        });
                    });
                });
            });
        });
    });
    run_due_timers(next_timer_deadline().unwrap());
    assert_eq!(count.get(), 1);
    run_due_timers(next_timer_deadline().expect("hook was rearmed during its callback"));
    assert_eq!(count.get(), 2);
    clear_test_timers();
}
