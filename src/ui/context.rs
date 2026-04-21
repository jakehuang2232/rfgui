//! Subtree-scoped context / provider primitive.
//!
//! `provide_context::<T>(value, || render_children())` pushes `value` onto a
//! TypeId-keyed stack for the lifetime of the closure; descendants call
//! `use_context::<T>()` to read the innermost value currently in scope.
//! Analogous to React's Context minus an explicit `<Provider>` node — the
//! provider component wraps its child-producing expression in the call.
//!
//! Values are `Clone + 'static`. Typical usage wraps mutable state in a
//! `Binding<T>` so consumers get both read access and change notification via
//! the existing binding dirty pipeline — context itself is purely a lookup
//! mechanism and does not own a dirty signal.

use rustc_hash::FxHashMap;
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    static CONTEXT_STACK: RefCell<FxHashMap<TypeId, Vec<Rc<dyn Any>>>> =
        RefCell::new(FxHashMap::default());
}

/// Provide `value` of type `T` to every component rendered inside `f`. The
/// value is popped automatically when `f` returns, or if `f` panics.
pub fn provide_context<T, R>(value: T, f: impl FnOnce() -> R) -> R
where
    T: Clone + 'static,
{
    let tid = TypeId::of::<T>();
    let boxed: Rc<dyn Any> = Rc::new(value);
    CONTEXT_STACK.with(|s| s.borrow_mut().entry(tid).or_default().push(boxed));

    struct Guard(TypeId);
    impl Drop for Guard {
        fn drop(&mut self) {
            CONTEXT_STACK.with(|s| {
                let mut map = s.borrow_mut();
                if let Some(stack) = map.get_mut(&self.0) {
                    stack.pop();
                    if stack.is_empty() {
                        map.remove(&self.0);
                    }
                }
            });
        }
    }

    let _guard = Guard(tid);
    f()
}

/// Read the innermost provided value of type `T`, or `None` if no ancestor
/// `provide_context::<T>` is currently in scope.
pub fn use_context<T: Clone + 'static>() -> Option<T> {
    let tid = TypeId::of::<T>();
    CONTEXT_STACK.with(|s| {
        s.borrow()
            .get(&tid)
            .and_then(|stack| stack.last().cloned())
            .and_then(|rc| rc.downcast_ref::<T>().cloned())
    })
}

/// Like [`use_context`] but panics with a clear message when no provider of
/// `T` is in scope. Prefer this at the root of a component that logically
/// requires the context — a missing provider is almost always a bug.
pub fn use_context_expect<T: Clone + 'static>() -> T {
    use_context::<T>().unwrap_or_else(|| {
        panic!(
            "use_context_expect::<{}>() called with no ancestor provide_context in scope",
            std::any::type_name::<T>()
        )
    })
}

// ---------- React parity P3: context snapshot for lazy components ----------
//
// Under the P2 lazy pipeline, `<Child/>` inside
// `provide_context(value, || rsx!{<Child/>})` no longer runs `Child::render`
// immediately — it just constructs a `RsxNode::Component` description. By
// the time the `unwrap_components` walker reaches that description and
// invokes `vtable.render`, the enclosing `provide_context` closure has
// already returned and popped its value off `CONTEXT_STACK`.
//
// Fix: at Component-node construction time, snapshot the current
// `CONTEXT_STACK` and carry it on the node. When the walker renders that
// component, it temporarily installs the snapshot so `use_context`
// resolves as if we were still inside the provider's closure. Nested
// `provide_context` calls inside the component's own body push on top of
// the snapshot normally.
//
// Snapshot shape: `Rc<Entry>` clones are cheap — we share value
// allocations with the live stack.

/// Per-TypeId entry in a captured context stack.
pub struct ContextStackEntry {
    pub type_id: TypeId,
    pub stack: Vec<Rc<dyn Any>>,
}

/// Full snapshot of `CONTEXT_STACK` at a point in time. Captured by
/// `create_element` when building a `RsxNode::Component` and re-installed
/// by the walker before invoking `vtable.render`.
pub type ContextSnapshot = Vec<ContextStackEntry>;

/// Snapshot the current `CONTEXT_STACK` state. Cheap: each entry's values
/// are `Rc<dyn Any>` so we clone refcounts, not payloads.
pub fn snapshot_context_stack() -> ContextSnapshot {
    CONTEXT_STACK.with(|s| {
        s.borrow()
            .iter()
            .map(|(tid, stack)| ContextStackEntry {
                type_id: *tid,
                stack: stack.clone(),
            })
            .collect()
    })
}

/// Push a raw, type-erased value onto `CONTEXT_STACK` for the duration
/// of `f`, then pop. Walker-ancestry helper: used by `unwrap_components`
/// when it encounters [`crate::ui::RsxNode::Provider`] so child subtree
/// renders see the provided value via `use_context::<T>()`. Pairs with
/// [`crate::ui::provide_context_node`] on the producer side.
///
/// The `value: Rc<dyn Any>` must point to an allocation whose concrete
/// type matches `type_id`; `use_context::<T>()` downcasts back to `T`.
pub fn with_pushed_context_raw<R>(
    type_id: TypeId,
    value: Rc<dyn Any>,
    f: impl FnOnce() -> R,
) -> R {
    CONTEXT_STACK.with(|s| s.borrow_mut().entry(type_id).or_default().push(value));

    struct Guard(TypeId);
    impl Drop for Guard {
        fn drop(&mut self) {
            CONTEXT_STACK.with(|s| {
                let mut map = s.borrow_mut();
                if let Some(stack) = map.get_mut(&self.0) {
                    stack.pop();
                    if stack.is_empty() {
                        map.remove(&self.0);
                    }
                }
            });
        }
    }
    let _guard = Guard(type_id);
    f()
}

/// Build a walker-ancestry provider node. The returned [`RsxNode::Provider`]
/// wraps `child` and publishes `value` under `TypeId::of::<T>()` for the
/// duration of the walker's descent into `child`. Prefer this over the
/// closure form [`provide_context`] when the provider is part of a
/// rendered subtree and you need children passed from outside to see the
/// value — the closure form captures at build time and does not reach
/// children built in an outer scope.
pub fn provide_context_node<T: Clone + 'static>(
    value: T,
    child: crate::ui::RsxNode,
) -> crate::ui::RsxNode {
    let boxed: Rc<dyn Any> = Rc::new(value);
    crate::ui::RsxNode::Provider(Rc::new(crate::ui::RsxProviderNode {
        identity: crate::ui::RsxNodeIdentity::new("Provider", None),
        type_id: TypeId::of::<T>(),
        value: boxed,
        child,
    }))
}

/// Install `snapshot` *underneath* the live walker-pushed entries for
/// the duration of `f`. Walker ancestry wins on conflict:
///
/// - Walker-pushed entries for a TypeId stay on top of the stack, so
///   `use_context::<T>()` (which reads `stack.last()`) still sees the
///   innermost walker Provider — e.g. a `<Provider<GroupCtx>>` wrapping
///   a lazy `Component` seen mid-walk.
/// - TypeIds absent from the walker stack fall back to the snapshot
///   entries captured at the Component's create-site, covering the
///   original P3 motivation (a provider that existed in define-site
///   scope but was popped before the lazy walker reached the child).
///
/// The prior implementation `mem::take`'d the live stack and replaced
/// it with snapshot entries — that regressed walker-push semantics:
/// any `<Provider>` pushed by the walker right before this call got
/// wiped, so descendants' `use_context` returned `None` despite the
/// Provider being their tree ancestor (e.g. `ToggleButtonGroup` →
/// `ToggleButton` lost `ToggleButtonGroupContext`).
pub fn with_installed_context_snapshot<R>(
    snapshot: &ContextSnapshot,
    f: impl FnOnce() -> R,
) -> R {
    // For each TypeId in the snapshot, prepend its stack under the
    // existing walker-pushed entries. Track how many snapshot entries
    // went in per TypeId so we can drain exactly those on exit (the
    // walker may also push/pop during `f`; we must not disturb those).
    let inserted: Vec<(TypeId, usize)> = CONTEXT_STACK.with(|s| {
        let mut map = s.borrow_mut();
        let mut inserted = Vec::with_capacity(snapshot.len());
        for entry in snapshot {
            if entry.stack.is_empty() {
                continue;
            }
            let cur = map.entry(entry.type_id).or_default();
            // Build [snapshot..., walker...]. Walker stays on top so
            // `last()` still resolves walker-pushed values first.
            let mut merged = entry.stack.clone();
            merged.append(cur);
            *cur = merged;
            inserted.push((entry.type_id, entry.stack.len()));
        }
        inserted
    });

    struct Guard(Vec<(TypeId, usize)>);
    impl Drop for Guard {
        fn drop(&mut self) {
            CONTEXT_STACK.with(|s| {
                let mut map = s.borrow_mut();
                for (tid, n) in self.0.drain(..) {
                    if let Some(stack) = map.get_mut(&tid) {
                        // Drain the first `n` entries (those we prepended).
                        stack.drain(0..n.min(stack.len()));
                        if stack.is_empty() {
                            map.remove(&tid);
                        }
                    }
                }
            });
        }
    }
    let _guard = Guard(inserted);
    f()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, PartialEq, Debug)]
    struct Theme(&'static str);

    #[derive(Clone, PartialEq, Debug)]
    struct GroupCtx {
        value: i32,
    }

    #[test]
    fn use_context_reads_provided_value() {
        let seen = provide_context(Theme("dark"), use_context::<Theme>);
        assert_eq!(seen, Some(Theme("dark")));
    }

    #[test]
    fn use_context_is_none_outside_provider() {
        assert_eq!(use_context::<Theme>(), None);
    }

    #[test]
    fn inner_provider_shadows_outer_and_restores_on_exit() {
        let (before, inner, after) = provide_context(Theme("light"), || {
            let before = use_context::<Theme>();
            let inner = provide_context(Theme("dark"), use_context::<Theme>);
            let after = use_context::<Theme>();
            (before, inner, after)
        });
        assert_eq!(before, Some(Theme("light")));
        assert_eq!(inner, Some(Theme("dark")));
        assert_eq!(after, Some(Theme("light")));
    }

    #[test]
    fn sibling_providers_do_not_leak() {
        provide_context(GroupCtx { value: 1 }, || {
            assert_eq!(use_context::<GroupCtx>(), Some(GroupCtx { value: 1 }));
        });
        assert_eq!(use_context::<GroupCtx>(), None);
        provide_context(GroupCtx { value: 2 }, || {
            assert_eq!(use_context::<GroupCtx>(), Some(GroupCtx { value: 2 }));
        });
    }

    #[test]
    fn stack_unwinds_on_panic() {
        let result = std::panic::catch_unwind(|| {
            provide_context(Theme("dark"), || -> () { panic!("boom") })
        });
        assert!(result.is_err());
        assert_eq!(use_context::<Theme>(), None);
    }

    #[test]
    fn use_context_expect_panics_without_provider() {
        let result = std::panic::catch_unwind(use_context_expect::<Theme>);
        assert!(result.is_err());
    }

    #[test]
    fn snapshot_install_preserves_walker_pushed_provider() {
        // Regression: ToggleButtonGroup-style walker-pushed Provider
        // was wiped by the old `mem::take`-based snapshot install,
        // making descendants' `use_context` return `None`.
        //
        // Simulate: walker pushes `GroupCtx { value: 42 }`, then a lazy
        // Component's snapshot (captured before walker entered the
        // Provider, so empty w.r.t. GroupCtx) is installed for its
        // render. Descendant `use_context::<GroupCtx>()` must still see
        // the walker-pushed value.
        let empty_snapshot: ContextSnapshot = Vec::new();
        let seen = with_pushed_context_raw(
            TypeId::of::<GroupCtx>(),
            Rc::new(GroupCtx { value: 42 }),
            || with_installed_context_snapshot(&empty_snapshot, use_context::<GroupCtx>),
        );
        assert_eq!(seen, Some(GroupCtx { value: 42 }));
    }

    #[test]
    fn snapshot_fills_in_typeids_walker_did_not_push() {
        // When walker has NOT pushed a given TypeId, the snapshot
        // entries should still be visible (original P3 motivation —
        // recover a provider that existed at create-site but was
        // popped before the lazy walker reached the child).
        let snap: ContextSnapshot = vec![ContextStackEntry {
            type_id: TypeId::of::<Theme>(),
            stack: vec![Rc::new(Theme("dark")) as Rc<dyn Any>],
        }];
        let seen = with_installed_context_snapshot(&snap, use_context::<Theme>);
        assert_eq!(seen, Some(Theme("dark")));
    }

    #[test]
    fn snapshot_install_cleanup_leaves_stack_pristine() {
        let snap: ContextSnapshot = vec![ContextStackEntry {
            type_id: TypeId::of::<Theme>(),
            stack: vec![Rc::new(Theme("dark")) as Rc<dyn Any>],
        }];
        with_installed_context_snapshot(&snap, || {
            assert!(use_context::<Theme>().is_some());
        });
        assert!(use_context::<Theme>().is_none());
    }

    #[test]
    fn walker_push_inside_installed_snapshot_stacks_on_top() {
        // Mixed ordering: install snapshot first, walker pushes on
        // top, use_context sees walker value. Exiting walker push
        // restores snapshot value.
        let snap: ContextSnapshot = vec![ContextStackEntry {
            type_id: TypeId::of::<GroupCtx>(),
            stack: vec![Rc::new(GroupCtx { value: 1 }) as Rc<dyn Any>],
        }];
        with_installed_context_snapshot(&snap, || {
            assert_eq!(use_context::<GroupCtx>(), Some(GroupCtx { value: 1 }));
            let inner = with_pushed_context_raw(
                TypeId::of::<GroupCtx>(),
                Rc::new(GroupCtx { value: 2 }),
                use_context::<GroupCtx>,
            );
            assert_eq!(inner, Some(GroupCtx { value: 2 }));
            // After walker pop, snapshot value visible again.
            assert_eq!(use_context::<GroupCtx>(), Some(GroupCtx { value: 1 }));
        });
    }
}
