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
