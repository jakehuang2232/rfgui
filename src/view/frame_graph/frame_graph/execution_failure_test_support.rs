use super::FrameGraphError;
use std::cell::Cell;

thread_local! {
    static ARMED: Cell<Option<bool>> = const { Cell::new(None) };
}

// Scoped and thread-local: concurrent tests cannot consume this fault, and
// early returns or panics cannot poison the next test on the same worker.
pub(crate) struct ExecutionFailureGuard;
pub(crate) fn arm() -> ExecutionFailureGuard {
    ARMED.with(|state| {
        assert!(state.get().is_none(), "execution fault already armed");
        state.set(Some(false));
    });
    ExecutionFailureGuard
}
impl ExecutionFailureGuard {
    pub(crate) fn fired(&self) -> bool {
        ARMED.with(|state| state.get() == Some(true))
    }
}
impl Drop for ExecutionFailureGuard {
    fn drop(&mut self) {
        ARMED.with(|state| state.set(None));
    }
}
pub(super) fn after_step() -> Result<(), FrameGraphError> {
    ARMED.with(|state| {
        if state.get() == Some(false) {
            state.set(Some(true));
            Err(FrameGraphError::Execution(
                "injected after successful execution step".into(),
            ))
        } else {
            Ok(())
        }
    })
}
