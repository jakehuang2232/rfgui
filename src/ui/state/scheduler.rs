use super::*;

pub(super) trait PendingState {
    fn dirty(&self) -> UiDirtyState;
    // Detach all queues before running any user updater. Writes made by an
    // updater belong to the next batch, even when they target another slot.
    fn prepare(self: Rc<Self>) -> Box<dyn FnOnce() -> Option<ChangedState>>;
}

impl<T: Clone + PartialEq + 'static> PendingState for BindingPropPayload<T> {
    fn dirty(&self) -> UiDirtyState {
        if self.alive.get() {
            self.dirty_state
        } else {
            UiDirtyState::NONE
        }
    }

    fn prepare(self: Rc<Self>) -> Box<dyn FnOnce() -> Option<ChangedState>> {
        self.scheduled.set(false);
        let actions = std::mem::take(&mut *self.pending.borrow_mut());
        crate::ui::work_profile::count(|p| {
            p.state_targets += 1;
            p.state_actions += actions.len();
        });
        Box::new(move || {
            if !self.alive.get() {
                return None;
            }
            let previous = self.value.borrow().clone();
            let mut next = (*previous).clone();
            for action in actions {
                match action {
                    StateUpdate::Replace(value) => next = value,
                    StateUpdate::Update(update) => update(&mut next),
                }
            }
            if next != *previous {
                crate::ui::work_profile::count(|p| p.changed_targets += 1);
                if let Some(cell) = &self.legacy_cell {
                    *cell.borrow_mut() = next.clone();
                }
                *self.value.borrow_mut() = Rc::new(next);
                return Some(ChangedState {
                    target: self.target,
                    dirty: self.dirty_state,
                    owner: self.owner_component.clone(),
                });
            }
            None
        })
    }
}

pub(super) fn request_state_wakeup() {
    if BATCH_DEPTH.with(Cell::get) != 0
        || FRAME_DEPTH.with(Cell::get) != 0
        || FLUSHING_STATE.with(Cell::get)
        || NOTIFYING_STATE.with(Cell::get)
    {
        return;
    }
    if WAKE_REQUESTED.replace(true) {
        return;
    }
    let callback = REDRAW_CALLBACK.with(|slot| slot.borrow().clone());
    if let Some(callback) = callback {
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                NOTIFYING_STATE.set(false);
            }
        }
        NOTIFYING_STATE.set(true);
        let _guard = Guard;
        callback();
    }
}

/// Settle one batch of queued updates before constructing a new render.
/// No-op during a render or event batch. Updaters must be pure; they run in
/// order against preceding queued values, outside any state-store borrow.
pub fn flush_state_updates() {
    if BATCH_DEPTH.with(Cell::get) != 0
        || FRAME_DEPTH.with(Cell::get) != 0
        || FLUSHING_STATE.replace(true)
    {
        return;
    }
    let _profile = crate::ui::work_profile::scope(crate::ui::work_profile::Phase::StateFlush);
    struct Guard {
        changes: Vec<ChangedState>,
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            // Publish invalidation once after all commits. On unwind, values
            // already committed must still invalidate their old memo output.
            dependencies::publish_changes(&self.changes);
            FLUSHING_STATE.set(false);
        }
    }
    let mut guard = Guard {
        changes: Vec::new(),
    };
    WAKE_REQUESTED.set(false);
    let pending = PENDING_STATE.with(|queue| std::mem::take(&mut *queue.borrow_mut()));
    let commits: Vec<_> = pending.into_iter().map(|state| state.prepare()).collect();
    for commit in commits {
        if let Some(change) = commit() {
            guard.changes.push(change);
        }
    }
}

/// Group updates from one logical event. Nesting preserves the outer event
/// boundary, including bubbling handlers and native input mutations.
pub fn batch_state_updates<R>(f: impl FnOnce() -> R) -> R {
    let _batch = begin_state_batch();
    f()
}

pub(crate) struct StateBatch;

pub(crate) fn begin_state_batch() -> StateBatch {
    if BATCH_DEPTH.with(Cell::get) == 0 {
        flush_state_updates();
    }
    BATCH_DEPTH.set(BATCH_DEPTH.with(Cell::get) + 1);
    StateBatch
}

impl Drop for StateBatch {
    fn drop(&mut self) {
        BATCH_DEPTH.set(BATCH_DEPTH.with(Cell::get) - 1);
        if BATCH_DEPTH.with(Cell::get) == 0 && !std::thread::panicking() {
            flush_state_updates();
            if peek_state_dirty().has_any() {
                request_state_wakeup();
            }
        }
    }
}

pub(crate) struct StateFrame;

pub(crate) fn begin_state_frame() -> StateFrame {
    if FRAME_DEPTH.with(Cell::get) == 0 {
        flush_state_updates();
    }
    FRAME_DEPTH.set(FRAME_DEPTH.with(Cell::get) + 1);
    StateFrame
}

impl Drop for StateFrame {
    fn drop(&mut self) {
        FRAME_DEPTH.set(FRAME_DEPTH.with(Cell::get) - 1);
        if FRAME_DEPTH.with(Cell::get) == 0
            && !std::thread::panicking()
            && peek_state_dirty().has_any()
        {
            request_state_wakeup();
        }
    }
}
