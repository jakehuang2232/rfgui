use super::*;

#[test]
fn deadlines_rearm_cancel_and_drop_without_catchup_callbacks() {
    let now = Instant::now();
    let count = Rc::new(Cell::new(0));
    let fired = count.clone();
    let timer = Timer::new(move || fired.set(fired.get() + 1));
    timer.schedule(now, None);
    run_due(now);
    run_due(now);
    assert_eq!(count.get(), 1);
    assert_eq!(next_deadline(), None);
    let interval = Duration::from_millis(10);
    timer.schedule(now, Some(interval));
    let late = now + Duration::from_secs(1);
    run_due(late);
    assert_eq!(count.get(), 2);
    assert_eq!(next_deadline(), Some(late + interval));
    timer.cancel();
    run_due(late + interval);
    assert_eq!(count.get(), 2);
    timer.schedule(now, None);
    drop(timer);
    assert_eq!(next_deadline(), None);
    run_due(late);
    assert_eq!(count.get(), 2);
}

#[test]
fn callbacks_can_drop_or_rearm_other_due_registrations() {
    let now = Instant::now();
    let other = Rc::new(RefCell::new(None::<Timer>));
    let slot = other.clone();
    let first = Timer::new(move || {
        slot.borrow_mut().take();
    });
    let second = Timer::new(|| panic!("dropped owner must not fire from the due snapshot"));
    first.schedule(now, None);
    second.schedule(now, None);
    other.replace(Some(second));
    run_due(now);
    assert_eq!(next_deadline(), None);

    let other = Rc::new(RefCell::new(None::<Timer>));
    let slot = other.clone();
    let future = now + Duration::from_secs(1);
    let first = Timer::new(move || slot.borrow().as_ref().unwrap().schedule(future, None));
    let count = Rc::new(Cell::new(0));
    let fired = count.clone();
    let second = Timer::new(move || fired.set(fired.get() + 1));
    first.schedule(now, None);
    second.schedule(now, None);
    other.replace(Some(second));
    run_due(now);
    assert_eq!(count.get(), 0);
    run_due(future);
    assert_eq!(count.get(), 1);
}

#[test]
fn recursive_dispatch_does_not_repeat_a_callback() {
    let now = Instant::now();
    let count = Rc::new(Cell::new(0));
    let fired = count.clone();
    let timer = Timer::new(move || {
        fired.set(fired.get() + 1);
        run_due(now);
    });
    timer.schedule(now, Some(Duration::ZERO));
    let other_count = Rc::new(Cell::new(0));
    let fired = other_count.clone();
    let other = Timer::new(move || fired.set(fired.get() + 1));
    other.schedule(now, Some(Duration::ZERO));
    run_due(now);
    assert_eq!(count.get(), 1);
    assert_eq!(other_count.get(), 1);
}
