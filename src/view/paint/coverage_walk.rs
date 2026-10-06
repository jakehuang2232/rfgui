//! Identifies one coverage walk. The walker reads the arena immutably for the
//! whole walk, so a host may memoize a pure observation of the arena within
//! it. Outside a walk there is no epoch, and hosts observe live state.
use std::cell::Cell;

thread_local! {
    static ACTIVE: Cell<Option<u64>> = const { Cell::new(None) };
    static NEXT: Cell<u64> = const { Cell::new(0) };
}

/// Restores the enclosing walk (if any) when the walk ends.
pub(crate) struct CoverageWalk(Option<u64>);

pub(crate) fn begin() -> CoverageWalk {
    let epoch = NEXT.get().wrapping_add(1);
    NEXT.set(epoch);
    CoverageWalk(ACTIVE.replace(Some(epoch)))
}

impl Drop for CoverageWalk {
    fn drop(&mut self) {
        ACTIVE.set(self.0);
    }
}

/// The epoch of the walk running on this thread.
pub(crate) fn active() -> Option<u64> {
    ACTIVE.get()
}

#[cfg(test)]
mod tests;
