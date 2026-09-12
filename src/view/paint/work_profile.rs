//! Opt-in exclusive CPU costs for retained graph construction.
//! `RFGUI_PROFILE_PAINT=1` enables native diagnostics; no timing reads occur
//! when disabled. This reports CPU wall time, never GPU execution duration.
use crate::time::Instant;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Default)]
struct Profile {
    active: bool,
    stack: Vec<Duration>,
    phases: BTreeMap<&'static str, (Duration, usize)>,
}
thread_local! {
    static PROFILE: RefCell<Profile> = RefCell::new(Profile::default());
}

#[inline]
fn enabled() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    let enabled = *{
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        ENABLED.get_or_init(|| std::env::var_os("RFGUI_PROFILE_PAINT").is_some_and(|v| v == "1"))
    };
    #[cfg(target_arch = "wasm32")]
    let enabled = false;
    enabled
}

pub(crate) fn begin() {
    if !enabled() {
        return;
    }
    PROFILE.with(|p| {
        let mut p = p.borrow_mut();
        p.active = true;
        p.stack.clear();
        p.phases.clear();
    });
}

pub(crate) struct Scope(Option<(&'static str, Instant)>);
pub(crate) fn scope(name: &'static str) -> Scope {
    // The disabled hot path avoids thread-local lookup and RefCell traffic.
    // Enabling remains fixed for the process, as it was at frame begin.
    if !enabled() {
        return Scope(None);
    }
    Scope(PROFILE.with(|p| {
        let mut p = p.borrow_mut();
        p.active.then(|| {
            p.stack.push(Duration::ZERO);
            (name, Instant::now())
        })
    }))
}
impl Drop for Scope {
    fn drop(&mut self) {
        let Some((name, start)) = self.0.take() else {
            return;
        };
        let elapsed = start.elapsed();
        PROFILE.with(|p| {
            let mut p = p.borrow_mut();
            p.complete_scope(name, elapsed);
        });
    }
}

pub(crate) fn finish(frame: u64, build_ms: f64) {
    if !enabled() {
        return;
    }
    PROFILE.with(|p| {
        let mut p = p.borrow_mut();
        if !p.active {
            return;
        }
        p.active = false;
        let phases = p
            .phases
            .iter()
            .map(|(name, (time, calls))| {
                format!("{name}={:.6}/{calls}", time.as_secs_f64() * 1000.0)
            })
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!("paint-cpu frame={frame} build_ms={build_ms:.6} {phases}");
    });
}

pub(crate) fn count(name: &'static str, count: usize) {
    if !enabled() {
        return;
    }
    PROFILE.with(|p| {
        let mut p = p.borrow_mut();
        if p.active {
            p.phases.entry(name).or_default().1 += count;
        }
    });
}

impl Profile {
    fn complete_scope(&mut self, name: &'static str, elapsed: Duration) {
        let children = self.stack.pop().expect("profile scopes are nested");
        if let Some(parent) = self.stack.last_mut() {
            *parent += elapsed;
        }
        let phase = self.phases.entry(name).or_default();
        phase.0 += elapsed.saturating_sub(children);
        phase.1 += 1;
    }
}

#[cfg(test)]
mod tests;
