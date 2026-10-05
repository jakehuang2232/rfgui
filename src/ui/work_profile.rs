//! Opt-in CPU observations for state/build/reconcile and downstream layout work.
//! Timers are inclusive per phase, but recursive entries of the same phase
//! count only once. Unwrap includes component renders and is nested in build;
//! it must never be added to build time. Disabled scopes never read a clock.
use crate::time::Instant;
use std::cell::{Cell, RefCell};

/// Work performed inside one explicit observation scope. Durations are CPU
/// wall time, not GPU time; counters describe actual calls, not scene census.
#[derive(Clone, Copy, Debug, Default)]
pub struct UiWorkProfile {
    /// Host dirty/placement observations performed by the layout refresh.
    pub dirty_observations: usize,
    /// Subtrees reused by the layout dirty refresh without visiting descendants.
    pub dirty_subtree_reuses: usize,
    /// Native host measure calls (including calls that take an internal bailout).
    pub measure_calls: usize,
    /// Native child measure calls avoided by the exact geometry reuse contract.
    pub measure_reuses: usize,
    /// Native host place calls (including calls that take an internal bailout).
    pub place_calls: usize,
    /// Host box-model snapshots read during collection.
    pub box_model_reads: usize,
    /// Box-model snapshots copied from the existing root cache.
    pub box_model_reused_snapshots: usize,
    /// Host dirty-clear hooks visited (including already-clean hosts).
    pub dirty_clear_visits: usize,
    /// Nodes observed by the render-change capture after layout/resources.
    pub render_change_observations: usize,
    /// Nodes inspected while synchronizing hover visuals, including cache hits.
    pub hover_observations: usize,
    /// Rect instance-buffer upload copies; at most one per prepare flush,
    /// covering only instances staged since the previous flush.
    pub rect_instance_uploads: usize,
    /// Instanced rect draw calls recorded (one per contiguous run).
    pub rect_draw_calls: usize,
    /// Rect instances covered by those draw calls.
    pub rect_instances: usize,
    /// Shadow templates produced; a reused template costs no production.
    pub shadow_template_builds: usize,
    /// Glyphs inspected for immutable prepared-input validation/hash misses.
    pub text_input_glyph_observations: usize,
    /// Texture/layer composite bind groups actually created on cache misses.
    pub composite_bind_group_creations: usize,
    /// Surface presentation bindings and immutable uniforms created on cache misses.
    pub present_bind_group_creations: usize,
    /// Logical graphics passes actually recorded, excluding an aborted surface pass.
    pub graphics_passes_recorded: usize,
    /// Component scheduling hooks observed while aggregating animation requests.
    pub animation_request_observations: usize,
    /// Queue evaluation and state publication, including invalidation.
    pub state_flush_ms: f64,
    /// Outermost component expansion; nested within App build.
    pub unwrap_ms: f64,
    /// Tree diff calls, excluding patch translation.
    pub reconcile_ms: f64,
    /// Placement validation and direct application.
    pub placement_ms: f64,
    /// Patch to FiberWork translation.
    pub translate_ms: f64,
    /// Incremental application including scroll transfer.
    pub incremental_commit_ms: f64,
    /// Descriptor conversion and cold arena replacement.
    pub cold_commit_ms: f64,
    /// Detached state target queues.
    pub state_targets: usize,
    /// Actions detached from those queues.
    pub state_actions: usize,
    /// Targets whose final value changed.
    pub changed_targets: usize,
    /// Distinct affected memo entries visited by invalidation, per batch.
    pub memo_invalidation_visits: usize,
    /// Memo entries transitioned from reusable to dirty.
    pub memo_invalidations: usize,
    /// Component vtable render invocations.
    pub component_renders: usize,
    /// Memoized render functions skipped.
    pub memo_hits: usize,
    /// Nodes visited by component expansion.
    pub unwrap_nodes: usize,
    /// Reconcile entry point calls.
    pub reconcile_calls: usize,
    /// Node pairs visited by reconciliation.
    pub reconciled_nodes: usize,
    /// Node pairs skipped through shared pointer identity.
    pub shared_subtree_hits: usize,
    /// Patches generated for viewport commit.
    pub patches: usize,
    /// Work units produced by the viewport translator.
    pub fiber_works: usize,
    /// Nodes visited while saving scroll for incremental replacement.
    pub scroll_save_nodes: usize,
    /// Saved scroll owners found in the current arena index.
    pub scroll_restore_nodes: usize,
}

#[derive(Clone, Copy)]
pub(crate) enum Phase {
    StateFlush,
    Unwrap,
    Reconcile,
    Placement,
    Translate,
    IncrementalCommit,
    ColdCommit,
}
struct Active {
    profile: UiWorkProfile,
    depth: [usize; 7],
}
thread_local! {
    static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
    static ENABLED: Cell<bool> = const { Cell::new(false) };
}

pub(crate) struct Capture {
    owner: bool,
}
pub(crate) fn capture(enabled: bool) -> Capture {
    let owner = enabled
        && ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            if active.is_some() {
                return false;
            }
            *active = Some(Active {
                profile: UiWorkProfile::default(),
                depth: [0; 7],
            });
            true
        });
    if owner {
        ENABLED.set(true);
    }
    Capture { owner }
}
impl Drop for Capture {
    fn drop(&mut self) {
        if self.owner {
            ENABLED.set(false);
            ACTIVE.with(|active| *active.borrow_mut() = None);
        }
    }
}
pub(crate) fn snapshot() -> UiWorkProfile {
    ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .map_or_else(UiWorkProfile::default, |a| a.profile)
    })
}
/// Observe one logical operation (for example build plus render). A nested
/// observation shares the enclosing scope and returns its cumulative snapshot.
/// The scope is released on unwinding as well as on normal return.
pub fn profile_ui_work<R>(f: impl FnOnce() -> R) -> (R, UiWorkProfile) {
    let _capture = capture(true);
    let result = f();
    (result, snapshot())
}
#[inline]
pub(crate) fn count(f: impl FnOnce(&mut UiWorkProfile)) {
    if !ENABLED.get() {
        return;
    }
    ACTIVE.with(|active| {
        if let Some(active) = active.borrow_mut().as_mut() {
            f(&mut active.profile);
        }
    });
}
pub(crate) struct Scope {
    phase: Phase,
    start: Option<Instant>,
    entered: bool,
}
#[inline]
pub(crate) fn scope(phase: Phase) -> Scope {
    if !ENABLED.get() {
        return Scope {
            phase,
            start: None,
            entered: false,
        };
    }
    let (entered, outer) = ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let Some(active) = active.as_mut() else {
            return (false, false);
        };
        let depth = &mut active.depth[phase as usize];
        let outer = *depth == 0;
        *depth += 1;
        (true, outer)
    });
    Scope {
        phase,
        start: outer.then(Instant::now),
        entered,
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        if !self.entered {
            return;
        }
        let ms = self
            .start
            .map(|start| start.elapsed().as_secs_f64() * 1000.0);
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            let Some(active) = active.as_mut() else {
                return;
            };
            active.depth[self.phase as usize] -= 1;
            if let Some(ms) = ms {
                let p = &mut active.profile;
                let total = match self.phase {
                    Phase::StateFlush => &mut p.state_flush_ms,
                    Phase::Unwrap => &mut p.unwrap_ms,
                    Phase::Reconcile => &mut p.reconcile_ms,
                    Phase::Placement => &mut p.placement_ms,
                    Phase::Translate => &mut p.translate_ms,
                    Phase::IncrementalCommit => &mut p.incremental_commit_ms,
                    Phase::ColdCommit => &mut p.cold_commit_ms,
                };
                *total += ms;
            }
        });
    }
}

#[cfg(test)]
mod tests;

impl UiWorkProfile {
    pub(crate) fn since(self, before: Self) -> Self {
        Self {
            dirty_observations: self
                .dirty_observations
                .saturating_sub(before.dirty_observations),
            dirty_subtree_reuses: self
                .dirty_subtree_reuses
                .saturating_sub(before.dirty_subtree_reuses),
            measure_calls: self.measure_calls.saturating_sub(before.measure_calls),
            place_calls: self.place_calls.saturating_sub(before.place_calls),
            box_model_reads: self.box_model_reads.saturating_sub(before.box_model_reads),
            box_model_reused_snapshots: self
                .box_model_reused_snapshots
                .saturating_sub(before.box_model_reused_snapshots),
            dirty_clear_visits: self
                .dirty_clear_visits
                .saturating_sub(before.dirty_clear_visits),
            render_change_observations: self
                .render_change_observations
                .saturating_sub(before.render_change_observations),
            hover_observations: self
                .hover_observations
                .saturating_sub(before.hover_observations),
            rect_instance_uploads: self
                .rect_instance_uploads
                .saturating_sub(before.rect_instance_uploads),
            rect_draw_calls: self.rect_draw_calls.saturating_sub(before.rect_draw_calls),
            shadow_template_builds: self
                .shadow_template_builds
                .saturating_sub(before.shadow_template_builds),
            rect_instances: self.rect_instances.saturating_sub(before.rect_instances),
            text_input_glyph_observations: self
                .text_input_glyph_observations
                .saturating_sub(before.text_input_glyph_observations),
            composite_bind_group_creations: self
                .composite_bind_group_creations
                .saturating_sub(before.composite_bind_group_creations),
            graphics_passes_recorded: self
                .graphics_passes_recorded
                .saturating_sub(before.graphics_passes_recorded),
            present_bind_group_creations: self
                .present_bind_group_creations
                .saturating_sub(before.present_bind_group_creations),
            animation_request_observations: self
                .animation_request_observations
                .saturating_sub(before.animation_request_observations),
            measure_reuses: self.measure_reuses.saturating_sub(before.measure_reuses),
            state_flush_ms: self.state_flush_ms - before.state_flush_ms,
            unwrap_ms: self.unwrap_ms - before.unwrap_ms,
            reconcile_ms: self.reconcile_ms - before.reconcile_ms,
            placement_ms: self.placement_ms - before.placement_ms,
            translate_ms: self.translate_ms - before.translate_ms,
            incremental_commit_ms: self.incremental_commit_ms - before.incremental_commit_ms,
            cold_commit_ms: self.cold_commit_ms - before.cold_commit_ms,
            state_targets: self.state_targets.saturating_sub(before.state_targets),
            state_actions: self.state_actions.saturating_sub(before.state_actions),
            changed_targets: self.changed_targets.saturating_sub(before.changed_targets),
            memo_invalidation_visits: self
                .memo_invalidation_visits
                .saturating_sub(before.memo_invalidation_visits),
            memo_invalidations: self
                .memo_invalidations
                .saturating_sub(before.memo_invalidations),
            component_renders: self
                .component_renders
                .saturating_sub(before.component_renders),
            memo_hits: self.memo_hits.saturating_sub(before.memo_hits),
            unwrap_nodes: self.unwrap_nodes.saturating_sub(before.unwrap_nodes),
            reconcile_calls: self.reconcile_calls.saturating_sub(before.reconcile_calls),
            reconciled_nodes: self
                .reconciled_nodes
                .saturating_sub(before.reconciled_nodes),
            shared_subtree_hits: self
                .shared_subtree_hits
                .saturating_sub(before.shared_subtree_hits),
            patches: self.patches.saturating_sub(before.patches),
            fiber_works: self.fiber_works.saturating_sub(before.fiber_works),
            scroll_save_nodes: self
                .scroll_save_nodes
                .saturating_sub(before.scroll_save_nodes),
            scroll_restore_nodes: self
                .scroll_restore_nodes
                .saturating_sub(before.scroll_restore_nodes),
        }
    }
}
