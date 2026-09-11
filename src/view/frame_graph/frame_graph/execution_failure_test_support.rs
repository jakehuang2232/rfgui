use super::FrameGraphError;
use std::cell::Cell;

#[derive(Clone, Copy)]
struct Fault {
    after: usize,
    steps: usize,
    fired: bool,
}
thread_local! {
    static ARMED: Cell<Option<Fault>> = const { Cell::new(None) };
}

// Scoped and thread-local: concurrent tests cannot consume this fault, and
// early returns or panics cannot poison the next test on the same worker.
pub(crate) struct ExecutionFailureGuard;
pub(crate) fn arm() -> ExecutionFailureGuard {
    arm_after(1)
}
// usize::MAX observes a successful frame's actual execution-step count.
// Tests can then exercise every failure position without hard-coded ordinals.
pub(crate) fn arm_after(after: usize) -> ExecutionFailureGuard {
    assert!(after > 0);
    ARMED.with(|state| {
        assert!(state.get().is_none(), "execution fault already armed");
        state.set(Some(Fault {
            after,
            steps: 0,
            fired: false,
        }));
    });
    ExecutionFailureGuard
}
impl ExecutionFailureGuard {
    pub(crate) fn fired(&self) -> bool {
        ARMED.with(|state| state.get().is_some_and(|fault| fault.fired))
    }
    pub(crate) fn steps(&self) -> usize {
        ARMED.with(|state| state.get().expect("active guard").steps)
    }
}
impl Drop for ExecutionFailureGuard {
    fn drop(&mut self) {
        ARMED.with(|state| state.set(None));
    }
}
pub(super) fn after_step() -> Result<(), FrameGraphError> {
    ARMED.with(|state| {
        let Some(mut fault) = state.get() else {
            return Ok(());
        };
        fault.steps += 1;
        let fail = !fault.fired && fault.steps == fault.after;
        fault.fired |= fail;
        state.set(Some(fault));
        if fail {
            Err(FrameGraphError::Execution(
                "injected after successful execution step".into(),
            ))
        } else {
            Ok(())
        }
    })
}
