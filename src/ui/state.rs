#![allow(missing_docs)]

//! Stateful hooks and global state helpers used by typed RSX components.

use crate::time::{Duration, Instant};
use crate::ui::{FromPropValue, GlobalKey, IntoPropValue, PropValue, RsxKey, SharedPropValue};
use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiDirtyState(u8);

impl Default for UiDirtyState {
    fn default() -> Self {
        Self::NONE
    }
}

impl UiDirtyState {
    pub const NONE: Self = Self(0);
    pub const REDRAW: Self = Self(1 << 0);
    pub const REBUILD: Self = Self(1 << 1);

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn has_any(self) -> bool {
        self.0 != 0
    }

    pub const fn needs_redraw(self) -> bool {
        self.0 & (Self::REDRAW.0 | Self::REBUILD.0) != 0
    }

    pub const fn needs_rebuild(self) -> bool {
        self.0 & Self::REBUILD.0 != 0
    }

    pub const fn is_redraw_only(self) -> bool {
        self.needs_redraw() && !self.needs_rebuild()
    }
}

#[derive(Clone)]
struct BindingPropPayload<T: 'static> {
    cell: Rc<RefCell<T>>,
    dirty_state: UiDirtyState,
}

#[derive(Clone)]
pub struct Binding<T: 'static> {
    cell: Rc<RefCell<T>>,
    dirty_state: UiDirtyState,
}

impl<T: 'static> Binding<T> {
    pub fn new(initial: T) -> Self {
        Self {
            cell: Rc::new(RefCell::new(initial)),
            dirty_state: UiDirtyState::REBUILD,
        }
    }

    pub fn new_with_dirty_state(initial: T, dirty_state: UiDirtyState) -> Self {
        Self {
            cell: Rc::new(RefCell::new(initial)),
            dirty_state,
        }
    }

    pub(crate) fn from_cell(cell: Rc<RefCell<T>>, dirty_state: UiDirtyState) -> Self {
        Self { cell, dirty_state }
    }
}

impl<T: Clone + PartialEq + 'static> Binding<T> {
    pub fn get(&self) -> T {
        self.cell.borrow().clone()
    }

    pub fn set(&self, value: T) {
        let mut current = self.cell.borrow_mut();
        if *current != value {
            *current = value;
            notify_state_changed(self.dirty_state);
        }
    }

    pub fn update(&self, updater: impl FnOnce(&mut T)) {
        let mut current = self.cell.borrow_mut();
        let previous = current.clone();
        updater(&mut current);
        if *current != previous {
            notify_state_changed(self.dirty_state);
        }
    }
}

impl<T: 'static> fmt::Debug for Binding<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Binding").finish()
    }
}

impl<T: 'static> PartialEq for Binding<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.cell, &other.cell)
    }
}

#[derive(Clone)]
pub struct State<T: 'static> {
    cell: Rc<RefCell<T>>,
    dirty_state: UiDirtyState,
}

impl<T: Clone + PartialEq + 'static> State<T> {
    pub fn get(&self) -> T {
        self.cell.borrow().clone()
    }

    pub fn set(&self, value: T) {
        let mut current = self.cell.borrow_mut();
        if *current != value {
            *current = value;
            notify_state_changed(self.dirty_state);
        }
    }

    pub fn update(&self, updater: impl FnOnce(&mut T)) {
        let mut current = self.cell.borrow_mut();
        let previous = current.clone();
        updater(&mut current);
        if *current != previous {
            notify_state_changed(self.dirty_state);
        }
    }

    pub fn binding(&self) -> Binding<T> {
        Binding::from_cell(self.cell.clone(), self.dirty_state)
    }
}

impl<T: 'static> fmt::Debug for State<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State").finish()
    }
}

impl<T: 'static> PartialEq for State<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.cell, &other.cell)
    }
}

#[derive(Clone, Eq)]
struct ComponentKey {
    type_id: TypeId,
    path: Vec<usize>,
}

impl PartialEq for ComponentKey {
    fn eq(&self, other: &Self) -> bool {
        self.type_id == other.type_id && self.path == other.path
    }
}

impl Hash for ComponentKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_id.hash(state);
        self.path.hash(state);
    }
}

struct Frame {
    key: ComponentKey,
    path: Vec<usize>,
    child_cursor: usize,
    hook_cursor: usize,
}

#[derive(Default)]
struct RenderContext {
    frames: Vec<Frame>,
}

#[derive(Default)]
struct StateStore {
    slots: HashMap<ComponentKey, Vec<Box<dyn Any>>>,
    build_depth: usize,
    root_cursor: usize,
    live_keys: HashSet<ComponentKey>,
    live_global_keys: HashSet<GlobalKey>,
    global_component_keys: HashMap<GlobalKey, ComponentKey>,
    active_build_global_keys: HashSet<GlobalKey>,
    components_rendered_in_build: bool,
}

#[derive(Clone, Eq)]
struct TimerHookKey {
    component: ComponentKey,
    hook_index: usize,
}

impl PartialEq for TimerHookKey {
    fn eq(&self, other: &Self) -> bool {
        self.component == other.component && self.hook_index == other.hook_index
    }
}

impl Hash for TimerHookKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.component.hash(state);
        self.hook_index.hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimerMode {
    Timeout,
    Interval,
}

struct TimerEntry {
    mode: TimerMode,
    enabled: bool,
    duration: Duration,
    next_fire_at: Instant,
    callback: Rc<RefCell<dyn FnMut()>>,
}

thread_local! {
    static STORE: RefCell<StateStore> = RefCell::new(StateStore::default());
    static GLOBAL_STORE: RefCell<HashMap<TypeId, Box<dyn Any>>> = RefCell::new(HashMap::new());
    static CONTEXT: RefCell<RenderContext> = RefCell::new(RenderContext::default());
    static COMPONENT_KEY_STACK: RefCell<Vec<Option<RsxKey>>> = const { RefCell::new(Vec::new()) };
    static REDRAW_CALLBACK: RefCell<Option<Rc<dyn Fn()>>> = RefCell::new(None);
    static STATE_DIRTY: Cell<UiDirtyState> = const { Cell::new(UiDirtyState::NONE) };
    static TIMER_STORE: RefCell<HashMap<TimerHookKey, TimerEntry>> = RefCell::new(HashMap::new());
    static LIVE_TIMER_HOOKS: RefCell<HashSet<TimerHookKey>> = RefCell::new(HashSet::new());
}

#[derive(Clone)]
pub struct GlobalState<T: 'static> {
    cell: Rc<RefCell<T>>,
    dirty_state: UiDirtyState,
}

impl<T: Clone + PartialEq + 'static> GlobalState<T> {
    pub fn get(&self) -> T {
        self.cell.borrow().clone()
    }

    pub fn set(&self, value: T) {
        let mut current = self.cell.borrow_mut();
        if *current != value {
            *current = value;
            notify_state_changed(self.dirty_state);
        }
    }

    pub fn update(&self, updater: impl FnOnce(&mut T)) {
        let mut current = self.cell.borrow_mut();
        let previous = current.clone();
        updater(&mut current);
        if *current != previous {
            notify_state_changed(self.dirty_state);
        }
    }

    pub fn binding(&self) -> Binding<T> {
        Binding::from_cell(self.cell.clone(), self.dirty_state)
    }
}

impl<T: 'static> fmt::Debug for GlobalState<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlobalState").finish()
    }
}

impl<T: 'static> PartialEq for GlobalState<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.cell, &other.cell)
    }
}

pub fn build_scope<R>(f: impl FnOnce() -> R) -> R {
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        if store.build_depth == 0 {
            store.root_cursor = 0;
            store.live_keys.clear();
            store.live_global_keys.clear();
            store.active_build_global_keys.clear();
            store.components_rendered_in_build = false;
            LIVE_TIMER_HOOKS.with(|hooks| hooks.borrow_mut().clear());
        }
        store.build_depth += 1;
    });

    let out = f();

    STORE.with(|store| {
        let mut store = store.borrow_mut();
        store.build_depth = store.build_depth.saturating_sub(1);
        if store.build_depth == 0 && store.components_rendered_in_build {
            let live = store.live_keys.clone();
            let live_global = store.live_global_keys.clone();
            store.slots.retain(|k, _| live.contains(k));
            store
                .global_component_keys
                .retain(|key, _| live_global.contains(key));
            LIVE_TIMER_HOOKS.with(|hooks| {
                let live_hooks = hooks.borrow().clone();
                TIMER_STORE.with(|timers| {
                    timers
                        .borrow_mut()
                        .retain(|key, _| live_hooks.contains(key));
                });
            });
        }
    });

    out
}

pub fn component_key_token<T: ?Sized + Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

pub fn classify_component_key<T: Hash + Any>(value: &T) -> RsxKey {
    let any = value as &dyn Any;
    if let Some(global_key) = any.downcast_ref::<GlobalKey>() {
        return RsxKey::Global(*global_key);
    }
    RsxKey::Local(component_key_token(value))
}

pub fn register_global_key(global_key: GlobalKey) {
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        if !store.active_build_global_keys.insert(global_key) {
            panic!("duplicate GlobalKey detected in the same build");
        }
    });
}

pub fn with_component_key<R>(key: Option<RsxKey>, f: impl FnOnce() -> R) -> R {
    struct StackGuard;
    impl Drop for StackGuard {
        fn drop(&mut self) {
            COMPONENT_KEY_STACK.with(|stack| {
                let _ = stack.borrow_mut().pop();
            });
        }
    }

    COMPONENT_KEY_STACK.with(|stack| {
        stack.borrow_mut().push(key);
    });
    let _guard = StackGuard;
    f()
}

fn current_rsx_key() -> Option<RsxKey> {
    COMPONENT_KEY_STACK.with(|stack| stack.borrow().last().cloned().flatten())
}

pub fn render_component<T: 'static, R>(f: impl FnOnce() -> R) -> R {
    const KEYED_PATH_MARKER: usize = usize::MAX;
    const GLOBAL_KEYED_PATH_MARKER: usize = usize::MAX - 1;
    let path = CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        let component_key = current_rsx_key();
        if let Some(parent) = context.frames.last_mut() {
            let child_index = parent.child_cursor;
            parent.child_cursor += 1;
            if let Some(RsxKey::Global(global_key)) = component_key {
                vec![GLOBAL_KEYED_PATH_MARKER, global_key.id() as usize]
            } else {
                let mut path = parent.path.clone();
                if let Some(RsxKey::Local(key)) = component_key {
                    path.push(KEYED_PATH_MARKER);
                    path.push(key as usize);
                } else {
                    path.push(child_index);
                }
                path
            }
        } else {
            STORE.with(|store| {
                let mut store = store.borrow_mut();
                let root_index = store.root_cursor;
                store.root_cursor += 1;
                if let Some(RsxKey::Global(global_key)) = component_key {
                    vec![GLOBAL_KEYED_PATH_MARKER, global_key.id() as usize]
                } else if let Some(RsxKey::Local(key)) = component_key {
                    vec![KEYED_PATH_MARKER, key as usize]
                } else {
                    vec![root_index]
                }
            })
        }
    });

    let key = ComponentKey {
        type_id: TypeId::of::<T>(),
        path,
    };

    STORE.with(|store| {
        let mut store = store.borrow_mut();
        store.components_rendered_in_build = true;
        store.live_keys.insert(key.clone());
        if let Some(RsxKey::Global(global_key)) = current_rsx_key() {
            store.live_global_keys.insert(global_key);
            store.global_component_keys.insert(global_key, key.clone());
        }
    });

    CONTEXT.with(|context| {
        context.borrow_mut().frames.push(Frame {
            key: key.clone(),
            path: key.path.clone(),
            child_cursor: 0,
            hook_cursor: 0,
        });
    });

    let out = f();

    CONTEXT.with(|context| {
        let _ = context.borrow_mut().frames.pop();
    });

    out
}

pub fn use_state<T: Clone + PartialEq + 'static>(init: impl FnOnce() -> T) -> State<T> {
    use_state_with_dirty_state(init, UiDirtyState::REBUILD)
}

pub fn use_redraw_state<T: Clone + PartialEq + 'static>(init: impl FnOnce() -> T) -> State<T> {
    use_state_with_dirty_state(init, UiDirtyState::REDRAW)
}

pub fn use_state_with_dirty_state<T: Clone + PartialEq + 'static>(
    init: impl FnOnce() -> T,
    dirty_state: UiDirtyState,
) -> State<T> {
    let (key, slot_index) = CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        let frame = context
            .frames
            .last_mut()
            .expect("use_state() must be called inside #[component] render");
        let index = frame.hook_cursor;
        frame.hook_cursor += 1;
        (frame.key.clone(), index)
    });

    let mut init_opt = Some(init);
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        let slots = store.slots.entry(key).or_default();
        if slots.len() <= slot_index {
            let value = (init_opt
                .take()
                .expect("use_state initializer should only run once"))();
            slots.push(Box::new(Rc::new(RefCell::new(value))));
        }
        let cell = slots[slot_index]
            .downcast_ref::<Rc<RefCell<T>>>()
            .unwrap_or_else(|| panic!("use_state slot type mismatch at index {}", slot_index))
            .clone();
        State { cell, dirty_state }
    })
}

fn use_timer<F>(mode: TimerMode, enabled: bool, duration: Duration, callback: F)
where
    F: FnMut() + 'static,
{
    let (component, hook_index) = CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        let frame = context
            .frames
            .last_mut()
            .expect("timer hooks must be called inside #[component] render");
        let index = frame.hook_cursor;
        frame.hook_cursor += 1;
        (frame.key.clone(), index)
    });

    let key = TimerHookKey {
        component,
        hook_index,
    };
    LIVE_TIMER_HOOKS.with(|hooks| {
        hooks.borrow_mut().insert(key.clone());
    });

    TIMER_STORE.with(|timers| {
        let mut timers = timers.borrow_mut();
        let now = Instant::now();
        let callback: Rc<RefCell<dyn FnMut()>> = Rc::new(RefCell::new(callback));
        match timers.get_mut(&key) {
            Some(entry) => {
                let should_reset =
                    entry.mode != mode || entry.duration != duration || (!entry.enabled && enabled);
                entry.mode = mode;
                entry.duration = duration;
                entry.enabled = enabled;
                entry.callback = callback;
                if should_reset {
                    entry.next_fire_at = now + duration;
                }
            }
            None => {
                timers.insert(
                    key,
                    TimerEntry {
                        mode,
                        enabled,
                        duration,
                        next_fire_at: now + duration,
                        callback,
                    },
                );
            }
        }
    });
}

pub fn use_timeout<F>(enabled: bool, delay: Duration, callback: F)
where
    F: FnMut() + 'static,
{
    use_timer(TimerMode::Timeout, enabled, delay, callback);
}

pub fn use_interval<F>(enabled: bool, interval: Duration, callback: F)
where
    F: FnMut() + 'static,
{
    use_timer(TimerMode::Interval, enabled, interval, callback);
}

pub fn next_timer_deadline() -> Option<Instant> {
    TIMER_STORE.with(|timers| {
        timers
            .borrow()
            .values()
            .filter(|entry| entry.enabled)
            .map(|entry| entry.next_fire_at)
            .min()
    })
}

pub fn run_due_timers(now: Instant) {
    let mut due_callbacks: Vec<Rc<RefCell<dyn FnMut()>>> = Vec::new();
    TIMER_STORE.with(|timers| {
        let mut timers = timers.borrow_mut();
        for entry in timers.values_mut() {
            if !entry.enabled || entry.next_fire_at > now {
                continue;
            }
            due_callbacks.push(entry.callback.clone());
            match entry.mode {
                TimerMode::Timeout => {
                    entry.enabled = false;
                }
                TimerMode::Interval => {
                    entry.next_fire_at = now + entry.duration;
                }
            }
        }
    });

    for callback in due_callbacks {
        (callback.borrow_mut())();
    }
}

fn global_cell_with_init<T: Clone + PartialEq + 'static>(
    init: impl FnOnce() -> T,
) -> Rc<RefCell<T>> {
    let mut init_opt = Some(init);
    GLOBAL_STORE.with(|store| {
        let mut store = store.borrow_mut();
        let type_id = TypeId::of::<T>();
        if !store.contains_key(&type_id) {
            let value = (init_opt
                .take()
                .expect("global_state initializer should only run once"))();
            store.insert(type_id, Box::new(Rc::new(RefCell::new(value))));
        }
        store[&type_id]
            .downcast_ref::<Rc<RefCell<T>>>()
            .unwrap_or_else(|| panic!("global_state type mismatch for {:?}", type_id))
            .clone()
    })
}

fn global_cell<T: Clone + PartialEq + 'static>() -> Option<Rc<RefCell<T>>> {
    GLOBAL_STORE.with(|store| {
        let store = store.borrow();
        let type_id = TypeId::of::<T>();
        let value = store.get(&type_id)?;
        Some(
            value
                .downcast_ref::<Rc<RefCell<T>>>()
                .unwrap_or_else(|| panic!("global_state type mismatch for {:?}", type_id))
                .clone(),
        )
    })
}

pub fn global_state<T: Clone + PartialEq + 'static>(init: impl FnOnce() -> T) -> GlobalState<T> {
    GlobalState {
        cell: global_cell_with_init(init),
        dirty_state: UiDirtyState::REBUILD,
    }
}

#[allow(non_snake_case)]
pub fn globalState<T: Clone + PartialEq + 'static>(init: impl FnOnce() -> T) -> GlobalState<T> {
    global_state(init)
}

pub fn use_global_state<T: Clone + PartialEq + 'static>() -> GlobalState<T> {
    let cell = global_cell::<T>().unwrap_or_else(|| {
        panic!(
            "use_global_state::<{}>() called before global_state/globalState initialization",
            std::any::type_name::<T>()
        )
    });
    GlobalState {
        cell,
        dirty_state: UiDirtyState::REBUILD,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        UiDirtyState, build_scope, next_timer_deadline, run_due_timers, take_state_dirty,
        use_interval, use_redraw_state, use_state, use_timeout, with_component_key,
    };
    use crate::time::{Duration, Instant};
    use crate::ui::{GlobalKey, RsxKey, RsxNode};
    use std::cell::Cell;
    use std::rc::Rc;

    fn clear_test_timers() {
        build_scope(|| {
            crate::ui::render_component::<u8, _>(|| {});
        });
    }

    #[test]
    fn non_component_scope_does_not_reset_use_state_slots() {
        let state_before = build_scope(|| {
            crate::ui::render_component::<u32, _>(|| {
                let value = use_state(|| 0_i32);
                value.set(7);
                value
            })
        });
        assert_eq!(state_before.get(), 7);
        let _ = take_state_dirty();

        let _ = build_scope(|| {
            RsxNode::tagged(
                "Element",
                crate::ui::RsxTagDescriptor::of::<crate::view::Element>(),
            )
        });

        let state_after = build_scope(|| {
            crate::ui::render_component::<u32, _>(|| {
                let value = use_state(|| 0_i32);
                value
            })
        });
        assert_eq!(state_after.get(), 7);
    }

    #[test]
    fn keyed_component_keeps_state_when_order_changes() {
        let first = build_scope(|| {
            let a = with_component_key(Some(RsxKey::Local(1)), || {
                crate::ui::render_component::<u32, _>(|| {
                    let state = use_state(|| 10_i32);
                    state.get()
                })
            });
            let b = with_component_key(Some(RsxKey::Local(2)), || {
                crate::ui::render_component::<u32, _>(|| {
                    let state = use_state(|| 20_i32);
                    state.get()
                })
            });
            (a, b)
        });
        assert_eq!(first, (10, 20));

        let second = build_scope(|| {
            let b = with_component_key(Some(RsxKey::Local(2)), || {
                crate::ui::render_component::<u32, _>(|| {
                    let state = use_state(|| 999_i32);
                    state.get()
                })
            });
            let a = with_component_key(Some(RsxKey::Local(1)), || {
                crate::ui::render_component::<u32, _>(|| {
                    let state = use_state(|| 999_i32);
                    state.get()
                })
            });
            (b, a)
        });
        assert_eq!(second, (20, 10));
    }

    #[test]
    fn global_key_component_keeps_state_when_parent_changes() {
        let global_key = GlobalKey::from("shared-child");

        let first = build_scope(|| {
            let left = crate::ui::render_component::<u8, _>(|| {
                with_component_key(Some(RsxKey::Global(global_key)), || {
                    crate::ui::render_component::<u32, _>(|| {
                        let state = use_state(|| 5_i32);
                        state.set(42);
                        state.get()
                    })
                })
            });
            let _right = crate::ui::render_component::<u16, _>(|| 0_i32);
            left
        });
        assert_eq!(first, 42);

        let second = build_scope(|| {
            let _left = crate::ui::render_component::<u8, _>(|| 0_i32);
            crate::ui::render_component::<u16, _>(|| {
                with_component_key(Some(RsxKey::Global(global_key)), || {
                    crate::ui::render_component::<u32, _>(|| {
                        let state = use_state(|| 999_i32);
                        state.get()
                    })
                })
            })
        });
        assert_eq!(second, 42);
    }

    #[test]
    fn use_timeout_fires_once_and_disables_itself() {
        clear_test_timers();
        let fired = Rc::new(Cell::new(0));
        let fired_for_hook = fired.clone();

        build_scope(|| {
            crate::ui::render_component::<u32, _>(|| {
                use_timeout(true, Duration::from_millis(10), move || {
                    fired_for_hook.set(fired_for_hook.get() + 1);
                });
            })
        });

        let deadline = next_timer_deadline().expect("timeout should schedule a deadline");
        run_due_timers(deadline);
        assert_eq!(fired.get(), 1);
        assert!(next_timer_deadline().is_none());

        run_due_timers(deadline + Duration::from_millis(10));
        assert_eq!(fired.get(), 1);
        clear_test_timers();
    }

    #[test]
    fn use_interval_resets_when_reenabled_or_duration_changes() {
        clear_test_timers();
        let fired = Rc::new(Cell::new(0));
        let build = |enabled: bool, duration_ms: u64, fired: Rc<Cell<i32>>| {
            build_scope(|| {
                crate::ui::render_component::<u64, _>(|| {
                    use_interval(enabled, Duration::from_millis(duration_ms), move || {
                        fired.set(fired.get() + 1);
                    });
                })
            });
        };

        build(true, 20, fired.clone());
        let first_deadline = next_timer_deadline().expect("interval should schedule");
        run_due_timers(first_deadline);
        assert_eq!(fired.get(), 1);

        build(false, 20, fired.clone());
        assert!(next_timer_deadline().is_none());
        run_due_timers(Instant::now() + Duration::from_secs(1));
        assert_eq!(fired.get(), 1);

        build(true, 40, fired.clone());
        let reset_deadline = next_timer_deadline().expect("reenabled interval should reschedule");
        run_due_timers(reset_deadline);
        assert_eq!(fired.get(), 2);
        clear_test_timers();
    }

    #[test]
    fn redraw_state_marks_redraw_without_rebuild() {
        let redraw = build_scope(|| {
            crate::ui::render_component::<u32, _>(|| {
                let value = use_redraw_state(|| 0_i32);
                value.set(1);
                value
            })
        });
        assert_eq!(redraw.get(), 1);
        assert_eq!(take_state_dirty(), UiDirtyState::REDRAW);
    }

    #[test]
    fn set_same_value_does_not_mark_dirty() {
        let state = build_scope(|| {
            crate::ui::render_component::<u32, _>(|| {
                let value = use_state(|| 7_i32);
                value.set(7);
                value
            })
        });
        assert_eq!(state.get(), 7);
        assert_eq!(take_state_dirty(), UiDirtyState::NONE);
    }

    #[test]
    fn update_without_effective_change_does_not_mark_dirty() {
        let state = build_scope(|| {
            crate::ui::render_component::<u32, _>(|| {
                let value = use_state(|| String::from("unchanged"));
                value.update(|text| {
                    text.push_str("");
                });
                value
            })
        });
        assert_eq!(state.get(), "unchanged");
        assert_eq!(take_state_dirty(), UiDirtyState::NONE);
    }
}

pub fn set_redraw_callback<F>(callback: F)
where
    F: Fn() + 'static,
{
    REDRAW_CALLBACK.with(|slot| {
        *slot.borrow_mut() = Some(Rc::new(callback));
    });
}

pub fn clear_redraw_callback() {
    REDRAW_CALLBACK.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

pub fn peek_state_dirty() -> UiDirtyState {
    STATE_DIRTY.with(Cell::get)
}

pub fn take_state_dirty() -> UiDirtyState {
    STATE_DIRTY.with(|dirty| {
        let was_dirty = dirty.get();
        dirty.set(UiDirtyState::NONE);
        was_dirty
    })
}

fn notify_state_changed(dirty_state: UiDirtyState) {
    STATE_DIRTY.with(|dirty| dirty.set(dirty.get().union(dirty_state)));
    REDRAW_CALLBACK.with(|slot| {
        if let Some(callback) = slot.borrow().as_ref() {
            callback();
        }
    });
}

impl<T: Clone + PartialEq + 'static> IntoPropValue for Binding<T> {
    fn into_prop_value(self) -> PropValue {
        let erased: Rc<dyn Any> = Rc::new(BindingPropPayload {
            cell: self.cell.clone(),
            dirty_state: self.dirty_state,
        });
        PropValue::Shared(SharedPropValue::new(erased))
    }
}

impl<T: Clone + PartialEq + 'static> FromPropValue for Binding<T> {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::Shared(shared) => {
                let erased = shared.value();
                if let Ok(payload) = Rc::downcast::<BindingPropPayload<T>>(erased.clone()) {
                    return Ok(Self {
                        cell: payload.cell.clone(),
                        dirty_state: payload.dirty_state,
                    });
                }
                let cell = Rc::downcast::<RefCell<T>>(erased)
                    .map_err(|_| "expected Binding value with matching type".to_string())?;
                Ok(Self {
                    cell,
                    dirty_state: UiDirtyState::REBUILD,
                })
            }
            _ => Err("expected Binding value".to_string()),
        }
    }
}
