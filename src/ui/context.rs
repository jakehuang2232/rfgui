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
}
