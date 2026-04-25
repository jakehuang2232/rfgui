//! Subtree-scoped context / provider primitive.
//!
//! Context is published exclusively via `RsxNode::Provider` nodes built
//! by [`provide_context_node`] (typically surfaced as `<Provider<T>>` in
//! rsx). The `unwrap_components` walker pushes `(TypeId, value)` onto a
//! TypeId-keyed thread-local stack for the duration of the provider's
//! subtree walk; descendants call [`use_context`] to read the innermost
//! value currently in scope.
//!
//! Single source of truth: walker tree position. The older closure form
//! (`provide_context(val, || render)`) and the `ContextSnapshot` lazy-
//! recovery mechanism have been removed — providers now always appear as
//! nodes in the rsx tree, so walker ancestry resolves correctly without
//! snapshotting state captured at rsx! expansion time.
//!
//! Values are `Clone + 'static`. Typical usage wraps mutable state in a
//! `Binding<T>` so consumers get both read access and change notification
//! via the existing binding dirty pipeline — context itself is purely a
//! lookup mechanism and does not own a dirty signal.

use rustc_hash::FxHashMap;
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    static CONTEXT_STACK: RefCell<FxHashMap<TypeId, Vec<Rc<dyn Any>>>> =
        RefCell::new(FxHashMap::default());
}

/// Read the innermost provided value of type `T`, or `None` if no ancestor
/// provider of `T` is currently in scope.
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
            "use_context_expect::<{}>() called with no ancestor provider in scope",
            std::any::type_name::<T>()
        )
    })
}

/// Push a raw, type-erased value onto `CONTEXT_STACK` for the duration
/// of `f`, then pop. Walker-ancestry helper: used by `unwrap_components`
/// when it encounters [`crate::ui::RsxNode::Provider`] so child subtree
/// renders see the provided value via `use_context::<T>()`. Pairs with
/// [`provide_context_node`] on the producer side.
///
/// The `value: Rc<dyn Any>` must point to an allocation whose concrete
/// type matches `type_id`; `use_context::<T>()` downcasts back to `T`.
pub fn with_pushed_context_raw<R>(type_id: TypeId, value: Rc<dyn Any>, f: impl FnOnce() -> R) -> R {
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
/// duration of the walker's descent into `child`. Typically surfaced as
/// `<Provider<T> value={...}>...</Provider>` in rsx.
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
    fn use_context_none_without_push() {
        assert_eq!(use_context::<Theme>(), None);
    }

    #[test]
    fn with_pushed_context_raw_makes_value_visible() {
        let seen = with_pushed_context_raw(
            TypeId::of::<Theme>(),
            Rc::new(Theme("dark")),
            use_context::<Theme>,
        );
        assert_eq!(seen, Some(Theme("dark")));
    }

    #[test]
    fn nested_push_shadows_outer() {
        let (outer_visible, inner_visible, after_pop) =
            with_pushed_context_raw(TypeId::of::<Theme>(), Rc::new(Theme("light")), || {
                let outer = use_context::<Theme>();
                let inner = with_pushed_context_raw(
                    TypeId::of::<Theme>(),
                    Rc::new(Theme("dark")),
                    use_context::<Theme>,
                );
                let restored = use_context::<Theme>();
                (outer, inner, restored)
            });
        assert_eq!(outer_visible, Some(Theme("light")));
        assert_eq!(inner_visible, Some(Theme("dark")));
        assert_eq!(after_pop, Some(Theme("light")));
    }

    #[test]
    fn sibling_pushes_do_not_leak() {
        with_pushed_context_raw(
            TypeId::of::<GroupCtx>(),
            Rc::new(GroupCtx { value: 1 }),
            || {
                assert_eq!(use_context::<GroupCtx>(), Some(GroupCtx { value: 1 }));
            },
        );
        assert_eq!(use_context::<GroupCtx>(), None);
        with_pushed_context_raw(
            TypeId::of::<GroupCtx>(),
            Rc::new(GroupCtx { value: 2 }),
            || {
                assert_eq!(use_context::<GroupCtx>(), Some(GroupCtx { value: 2 }));
            },
        );
        assert_eq!(use_context::<GroupCtx>(), None);
    }

    #[test]
    fn stack_unwinds_on_panic() {
        let result = std::panic::catch_unwind(|| {
            with_pushed_context_raw(TypeId::of::<Theme>(), Rc::new(Theme("dark")), || -> () {
                panic!("boom")
            })
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
