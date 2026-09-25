//! Thread-local deadlines shared by component hooks and retained viewports.
//! Owners keep a registration alive; dropping it cancels pending work.
use super::{Duration, Instant};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

struct Entry {
    deadline: Cell<Option<Instant>>,
    interval: Cell<Option<Duration>>,
    revision: Cell<u64>,
    callback: RefCell<Box<dyn FnMut()>>,
}

thread_local! {
    static ENTRIES: RefCell<BTreeMap<u64, Weak<Entry>>> = RefCell::new(BTreeMap::new());
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

pub(crate) struct Timer {
    id: u64,
    entry: Rc<Entry>,
}

impl Timer {
    pub(crate) fn new(callback: impl FnMut() + 'static) -> Self {
        let id = NEXT_ID.with(|next| {
            let id = next.get().checked_add(1).expect("timer identity exhausted");
            next.set(id);
            id
        });
        let entry = Rc::new(Entry {
            deadline: Cell::new(None),
            interval: Cell::new(None),
            revision: Cell::new(0),
            callback: RefCell::new(Box::new(callback)),
        });
        ENTRIES.with(|entries| entries.borrow_mut().insert(id, Rc::downgrade(&entry)));
        Self { id, entry }
    }

    pub(crate) fn schedule(&self, deadline: Instant, interval: Option<Duration>) {
        self.cancel();
        self.entry.interval.set(interval);
        self.entry.deadline.set(Some(deadline));
    }

    pub(crate) fn cancel(&self) {
        self.entry.deadline.set(None);
        self.entry
            .revision
            .set(self.entry.revision.get().wrapping_add(1));
    }

    pub(crate) fn is_scheduled(&self) -> bool {
        self.entry.deadline.get().is_some()
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.cancel();
        // Owners can also be destroyed during thread-local teardown.
        let _ = ENTRIES.try_with(|entries| entries.borrow_mut().remove(&self.id));
    }
}

pub(crate) fn next_deadline() -> Option<Instant> {
    ENTRIES.with(|entries| {
        entries
            .borrow()
            .values()
            .filter_map(Weak::upgrade)
            .filter_map(|entry| entry.deadline.get())
            .min()
    })
}

pub(crate) fn run_due(now: Instant) {
    let due: Vec<_> = ENTRIES.with(|entries| {
        entries
            .borrow()
            .values()
            .filter_map(Weak::upgrade)
            .filter(|entry| entry.deadline.get().is_some_and(|at| at <= now))
            .map(|entry| {
                let revision = entry.revision.get();
                (entry, revision)
            })
            .collect()
    });
    // Callbacks can drop, cancel, or rearm other owners. Never hold the
    // registry borrow across callbacks, or dispatch a superseded registration.
    for (entry, revision) in due {
        if entry.revision.get() != revision || entry.deadline.get().is_none() {
            continue;
        }
        let Ok(mut callback) = entry.callback.try_borrow_mut() else {
            continue; // Recursive dispatch must not reenter the same callback.
        };
        entry.revision.set(revision.wrapping_add(1));
        entry
            .deadline
            .set(entry.interval.get().map(|interval| now + interval));
        callback();
    }
}

#[cfg(test)]
mod tests;
