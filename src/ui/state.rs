#![allow(missing_docs)]

//! Stateful hooks and global state helpers used by typed RSX components.
use rustc_hash::{FxHashMap, FxHashSet};

use crate::time::{Duration, Instant};
use crate::ui::{
    EventMetaSnapshot, FromPropValue, GlobalKey, IntoPropValue, PointerButtons, PropValue, RsxKey,
    SharedPropValue, ViewportPointerDownEvent, ViewportPointerMoveEvent, ViewportPointerState,
    ViewportPointerUpEvent,
};
use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;

use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::{Rc, Weak};

mod dependencies;
use dependencies::{ChangedState, StateTargetId};
mod scheduler;
use scheduler::{PendingState, request_state_wakeup};
pub use scheduler::{batch_state_updates, flush_state_updates};
pub(crate) use scheduler::{begin_state_batch, begin_state_frame};

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

/// A shared update target. Handles share this allocation, while each render
/// retains its own immutable `Rc<T>` snapshot.
struct BindingPropPayload<T: 'static> {
    target: StateTargetId,
    value: RefCell<Rc<T>>,
    legacy_cell: Option<Rc<RefCell<T>>>,
    prop_view: RefCell<Weak<BindingPropView<T>>>,
    pending: RefCell<Vec<StateUpdate<T>>>,
    scheduled: Cell<bool>,
    alive: Rc<Cell<bool>>,
    dirty_state: UiDirtyState,
    owner_component: Option<ComponentKey>,
}

struct BindingPropView<T: 'static> {
    payload: Rc<BindingPropPayload<T>>,
    snapshot: Rc<T>,
}

enum StateUpdate<T> {
    Replace(T),
    Update(Box<dyn FnOnce(&mut T)>),
}

impl<T> BindingPropPayload<T> {
    fn new(
        value: T,
        dirty_state: UiDirtyState,
        owner_component: Option<ComponentKey>,
        alive: Rc<Cell<bool>>,
    ) -> Self {
        Self {
            target: StateTargetId::new(),
            value: RefCell::new(Rc::new(value)),
            legacy_cell: None,
            prop_view: RefCell::new(Weak::new()),
            pending: RefCell::new(Vec::new()),
            scheduled: Cell::new(false),
            alive,
            dirty_state,
            owner_component,
        }
    }
}

impl<T: Clone + PartialEq + 'static> BindingPropPayload<T> {
    fn enqueue(self: &Rc<Self>, update: StateUpdate<T>) {
        if !self.alive.get() {
            return;
        }
        // A replacement equal to committed state is only a no-op when no
        // earlier action is queued. set(1); set(0) must preserve the latter.
        if !FLUSHING_STATE.with(Cell::get)
            && self.pending.borrow().is_empty()
            && let StateUpdate::Replace(value) = &update
            && **self.value.borrow() == *value
        {
            return;
        }
        self.pending.borrow_mut().push(update);
        if !self.scheduled.replace(true) {
            PENDING_STATE.with(|queue| queue.borrow_mut().push(self.clone()));
        }
        request_state_wakeup();
    }
}

/// `get()` reads this handle's render snapshot, including inside an old
/// callback. `set` and `update` enqueue work against the shared target.
#[derive(Clone)]
pub struct Binding<T: 'static> {
    prop_payload: Rc<BindingPropPayload<T>>,
    snapshot: Rc<T>,
}

impl<T: 'static> Binding<T> {
    pub fn new(initial: T) -> Self {
        Self::new_with_dirty_state(initial, UiDirtyState::REBUILD)
    }

    pub fn new_with_dirty_state(initial: T, dirty_state: UiDirtyState) -> Self {
        Self::from_payload(Rc::new(BindingPropPayload::new(
            initial,
            dirty_state,
            None,
            Rc::new(Cell::new(true)),
        )))
    }

    fn from_payload(prop_payload: Rc<BindingPropPayload<T>>) -> Self {
        record_state_dependency(prop_payload.target);
        let snapshot = prop_payload.value.borrow().clone();
        Self {
            prop_payload,
            snapshot,
        }
    }

    /// Capture committed state for a new render. Cloning a handle preserves
    /// its old snapshot; refreshing is deliberately explicit.
    pub fn snapshot(&self) -> Self {
        Self::from_payload(self.prop_payload.clone())
    }
}

impl<T: Clone + PartialEq + 'static> Binding<T> {
    pub(crate) fn from_cell(cell: Rc<RefCell<T>>, dirty_state: UiDirtyState) -> Self {
        let mut payload = BindingPropPayload::new(
            cell.borrow().clone(),
            dirty_state,
            None,
            Rc::new(Cell::new(true)),
        );
        payload.legacy_cell = Some(cell);
        Self::from_payload(Rc::new(payload))
    }

    pub fn get(&self) -> T {
        record_state_dependency(self.prop_payload.target);
        (*self.snapshot).clone()
    }

    /// Imperative host bridge: read the last committed value, never pending
    /// actions. Use for native input sessions; render code should use `get`.
    pub fn get_committed(&self) -> T {
        record_state_dependency(self.prop_payload.target);
        (**self.prop_payload.value.borrow()).clone()
    }

    pub fn set(&self, value: T) {
        self.prop_payload.enqueue(StateUpdate::Replace(value));
    }

    /// Queue a pure updater. It receives the result of preceding actions;
    /// capture owned data (`move`) because execution is deferred.
    pub fn update(&self, updater: impl FnOnce(&mut T) + 'static) {
        self.prop_payload
            .enqueue(StateUpdate::Update(Box::new(updater)));
    }
}

impl<T: 'static> fmt::Debug for Binding<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Binding").finish()
    }
}

impl<T: 'static> PartialEq for Binding<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.prop_payload, &other.prop_payload)
            && Rc::ptr_eq(&self.snapshot, &other.snapshot)
    }
}

#[derive(Clone)]
pub struct State<T: 'static> {
    payload: Rc<BindingPropPayload<T>>,
    snapshot: Rc<T>,
}

impl<T: Clone + PartialEq + 'static> State<T> {
    pub fn get(&self) -> T {
        record_state_dependency(self.payload.target);
        (*self.snapshot).clone()
    }

    pub fn set(&self, value: T) {
        self.payload.enqueue(StateUpdate::Replace(value));
    }

    pub fn update(&self, updater: impl FnOnce(&mut T) + 'static) {
        self.payload.enqueue(StateUpdate::Update(Box::new(updater)));
    }

    pub fn binding(&self) -> Binding<T> {
        record_state_dependency(self.payload.target);
        Binding {
            prop_payload: self.payload.clone(),
            snapshot: self.snapshot.clone(),
        }
    }
}

impl<T: 'static> fmt::Debug for State<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State").finish()
    }
}

impl<T: 'static> PartialEq for State<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.payload, &other.payload) && Rc::ptr_eq(&self.snapshot, &other.snapshot)
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
    /// Monotonic index for keyed hooks (timers, mounts) whose identity is
    /// derived from `(component, hook_index)`. Does NOT track `use_state`
    /// slots — those use `state_cursor` instead so inserting a timer or
    /// mount hook between two `use_state` calls does not shift slot
    /// indices and corrupt the `StateStore::slots` vec.
    hook_cursor: usize,
    /// Monotonic index for `use_state` slots within this component frame.
    state_cursor: usize,
}

#[derive(Default)]
struct RenderContext {
    frames: Vec<Frame>,
}

#[derive(Default)]
struct StateStore {
    slots: FxHashMap<ComponentKey, Vec<Box<dyn Any>>>,
    lifetimes: FxHashMap<ComponentKey, Rc<Cell<bool>>>,
    build_depth: usize,
    root_cursor: usize,
    live_keys: FxHashSet<ComponentKey>,
    live_global_keys: FxHashSet<GlobalKey>,
    global_component_keys: FxHashMap<GlobalKey, ComponentKey>,
    active_build_global_keys: FxHashSet<GlobalKey>,
    components_rendered_in_build: bool,
    /// Component-memoization cache. Entries are keyed by `ComponentKey` and
    /// store the last props/output pair plus the set of descendant keys that
    /// were registered during that render, so we can keep them alive on a
    /// memo hit without re-entering the render function.
    memo_cache: FxHashMap<ComponentKey, MemoEntry>,
    /// Reverse indexes contain only cached memo consumers / resolved ancestors.
    target_consumers: FxHashMap<StateTargetId, FxHashSet<ComponentKey>>,
    component_memos: FxHashMap<ComponentKey, FxHashSet<ComponentKey>>,
    /// Memos invalidated since their last successful render.
    /// A memo hit for a key in this set is forbidden — we must re-render.
    dirty_memo_components: FxHashSet<ComponentKey>,
}

/// A cached component render. `props` holds a type-erased clone of the last
/// props value, compared via the monomorphized `props_eq` function pointer.
struct MemoEntry {
    context_dependencies: super::context::ContextDependencies,
    volatile: bool,
    props: Box<dyn Any>,
    node: crate::ui::RsxNode,
    props_eq: fn(&dyn Any, &dyn Any) -> bool,
    live_keys: FxHashSet<ComponentKey>,
    live_global_keys: FxHashSet<GlobalKey>,
    live_timer_hooks: FxHashSet<TimerHookKey>,
    live_mount_hooks: FxHashSet<MountHookKey>,
    state_dependencies: FxHashSet<StateTargetId>,
    live_viewport_pointer_hooks: FxHashSet<ViewportPointerHookKey>,
}

/// A scope that captures which keys/hooks were registered during a render
/// inside a memoized component. Pushed by `render_memoized_component` and
/// popped once the render returns; the captured sets are stored in the
/// resulting [`MemoEntry`].
#[derive(Default)]
struct MemoFrame {
    context_boundary: u64,
    context_dependencies: super::context::ContextDependencies,
    volatile: bool,
    live_keys: FxHashSet<ComponentKey>,
    live_global_keys: FxHashSet<GlobalKey>,
    live_timer_hooks: FxHashSet<TimerHookKey>,
    live_mount_hooks: FxHashSet<MountHookKey>,
    state_dependencies: FxHashSet<StateTargetId>,
    live_viewport_pointer_hooks: FxHashSet<ViewportPointerHookKey>,
}

fn memo_props_eq<P: PartialEq + 'static>(a: &dyn Any, b: &dyn Any) -> bool {
    match (a.downcast_ref::<P>(), b.downcast_ref::<P>()) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
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

type TimerCallback = Rc<RefCell<dyn FnMut()>>;

struct TimerEntry {
    mode: TimerMode,
    duration: Duration,
    timer: crate::time::timers::Timer,
    callback: Rc<RefCell<TimerCallback>>,
}

#[derive(Clone, Eq)]
struct MountHookKey {
    component: ComponentKey,
    hook_index: usize,
}

impl PartialEq for MountHookKey {
    fn eq(&self, other: &Self) -> bool {
        self.component == other.component && self.hook_index == other.hook_index
    }
}

impl Hash for MountHookKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.component.hash(state);
        self.hook_index.hash(state);
    }
}

struct MountEntry {
    cleanup: Option<Box<dyn FnOnce()>>,
}

impl Drop for MountEntry {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}

/// Result type returned from a `use_mount` closure. Returning `()` means no
/// cleanup; returning an `FnOnce() + 'static` closure registers it as cleanup
/// to run on component unmount.
pub trait MountCleanup {
    fn into_cleanup(self) -> Option<Box<dyn FnOnce()>>;
}

impl MountCleanup for () {
    fn into_cleanup(self) -> Option<Box<dyn FnOnce()>> {
        None
    }
}

impl<F> MountCleanup for F
where
    F: FnOnce() + 'static,
{
    fn into_cleanup(self) -> Option<Box<dyn FnOnce()>> {
        Some(Box::new(self))
    }
}

#[derive(Clone, Eq)]
struct ViewportPointerHookKey {
    component: ComponentKey,
    hook_index: usize,
}

impl PartialEq for ViewportPointerHookKey {
    fn eq(&self, other: &Self) -> bool {
        self.component == other.component && self.hook_index == other.hook_index
    }
}

impl Hash for ViewportPointerHookKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.component.hash(state);
        self.hook_index.hash(state);
    }
}

type ViewportPointerDownCallback = Rc<RefCell<dyn FnMut(&ViewportPointerDownEvent)>>;
type ViewportPointerMoveCallback = Rc<RefCell<dyn FnMut(&ViewportPointerMoveEvent)>>;
type ViewportPointerUpCallback = Rc<RefCell<dyn FnMut(&ViewportPointerUpEvent)>>;

thread_local! {
    static PENDING_STATE: RefCell<Vec<Rc<dyn PendingState>>> = const { RefCell::new(Vec::new()) };
    static BATCH_DEPTH: Cell<usize> = const { Cell::new(0) };
    static FRAME_DEPTH: Cell<usize> = const { Cell::new(0) };
    static FLUSHING_STATE: Cell<bool> = const { Cell::new(false) };
    static WAKE_REQUESTED: Cell<bool> = const { Cell::new(false) };
    static NOTIFYING_STATE: Cell<bool> = const { Cell::new(false) };
    static STORE: RefCell<StateStore> = RefCell::new(StateStore::default());
    static GLOBAL_STORE: RefCell<FxHashMap<TypeId, Box<dyn Any>>> = RefCell::new(FxHashMap::default());
    static CONTEXT: RefCell<RenderContext> = RefCell::new(RenderContext::default());
    static COMPONENT_KEY_STACK: RefCell<Vec<Option<RsxKey>>> = const { RefCell::new(Vec::new()) };
    static REDRAW_CALLBACK: RefCell<Option<Rc<dyn Fn()>>> = RefCell::new(None);
    static STATE_DIRTY: Cell<UiDirtyState> = const { Cell::new(UiDirtyState::NONE) };
    static TIMER_STORE: RefCell<FxHashMap<TimerHookKey, TimerEntry>> = RefCell::new(FxHashMap::default());
    static LIVE_TIMER_HOOKS: RefCell<FxHashSet<TimerHookKey>> = RefCell::new(FxHashSet::default());
    static MOUNT_STORE: RefCell<FxHashMap<MountHookKey, MountEntry>> = RefCell::new(FxHashMap::default());
    static LIVE_MOUNT_HOOKS: RefCell<FxHashSet<MountHookKey>> = RefCell::new(FxHashSet::default());
    static VIEWPORT_POINTER_DOWN_HOOKS: RefCell<FxHashMap<ViewportPointerHookKey, ViewportPointerDownCallback>> = RefCell::new(FxHashMap::default());
    static VIEWPORT_POINTER_MOVE_HOOKS: RefCell<FxHashMap<ViewportPointerHookKey, ViewportPointerMoveCallback>> = RefCell::new(FxHashMap::default());
    static VIEWPORT_POINTER_UP_HOOKS: RefCell<FxHashMap<ViewportPointerHookKey, ViewportPointerUpCallback>> = RefCell::new(FxHashMap::default());
    static VIEWPORT_POINTER_STATE_HOOKS: RefCell<FxHashSet<ViewportPointerHookKey>> = RefCell::new(FxHashSet::default());
    static LIVE_VIEWPORT_POINTER_HOOKS: RefCell<FxHashSet<ViewportPointerHookKey>> = RefCell::new(FxHashSet::default());
    static VIEWPORT_POINTER_STATE: RefCell<ViewportPointerState> = RefCell::new(ViewportPointerState::default());
    static PENDING_MOUNTS: RefCell<Vec<Box<dyn FnOnce()>>> = const { RefCell::new(Vec::new()) };
    /// Stack of in-progress memoized-component renders. Every registration of
    /// a `ComponentKey`, `GlobalKey`, or timer hook while this stack is
    /// non-empty is also recorded on the innermost frame so it can be
    /// reattached on a future memo hit.
    static MEMO_STACK: RefCell<Vec<MemoFrame>> = const { RefCell::new(Vec::new()) };
}

/// Reads inside a provider created by a memo belong to that memo's output,
/// not its external inputs. Record absence as well as present publications.
pub(crate) fn record_context_dependency(tid: TypeId, publication: Option<(&Rc<dyn Any>, u64)>) {
    MEMO_STACK.with(|stack| {
        for frame in stack.borrow_mut().iter_mut() {
            if publication.is_none_or(|(_, epoch)| epoch <= frame.context_boundary) {
                frame
                    .context_dependencies
                    .0
                    .insert(tid, publication.map(|(value, _)| value.clone()));
            }
        }
    });
}

pub(crate) fn record_volatile_render() {
    MEMO_STACK.with(|stack| {
        for frame in stack.borrow_mut().iter_mut() {
            frame.volatile = true;
        }
    });
}

fn record_state_dependency(target: StateTargetId) {
    MEMO_STACK.with(|stack| {
        if let Some(frame) = stack.borrow_mut().last_mut() {
            frame.state_dependencies.insert(target);
        }
    });
}

fn memo_stack_record_component_key(key: &ComponentKey) {
    MEMO_STACK.with(|s| {
        let mut stack = s.borrow_mut();
        if let Some(top) = stack.last_mut() {
            top.live_keys.insert(key.clone());
        }
    });
}

fn memo_stack_record_global_key(key: GlobalKey) {
    MEMO_STACK.with(|s| {
        let mut stack = s.borrow_mut();
        if let Some(top) = stack.last_mut() {
            top.live_global_keys.insert(key);
        }
    });
}

fn memo_stack_record_timer_hook(key: &TimerHookKey) {
    MEMO_STACK.with(|s| {
        let mut stack = s.borrow_mut();
        if let Some(top) = stack.last_mut() {
            top.live_timer_hooks.insert(key.clone());
        }
    });
}

fn memo_stack_record_viewport_pointer_hook(key: &ViewportPointerHookKey) {
    MEMO_STACK.with(|s| {
        let mut stack = s.borrow_mut();
        if let Some(top) = stack.last_mut() {
            top.live_viewport_pointer_hooks.insert(key.clone());
        }
    });
}

/// Global state uses the same queue and snapshot semantics as local state.
/// Reacquire via `use_global_state`, or `snapshot`, for the next render.
#[derive(Clone)]
pub struct GlobalState<T: 'static> {
    payload: Rc<BindingPropPayload<T>>,
    snapshot: Rc<T>,
}

impl<T: Clone + PartialEq + 'static> GlobalState<T> {
    fn from_payload(payload: Rc<BindingPropPayload<T>>) -> Self {
        record_state_dependency(payload.target);
        let snapshot = payload.value.borrow().clone();
        Self { payload, snapshot }
    }
    pub fn snapshot(&self) -> Self {
        Self::from_payload(self.payload.clone())
    }
    pub fn get(&self) -> T {
        record_state_dependency(self.payload.target);
        (*self.snapshot).clone()
    }
    pub fn set(&self, value: T) {
        self.payload.enqueue(StateUpdate::Replace(value));
    }
    pub fn update(&self, updater: impl FnOnce(&mut T) + 'static) {
        self.payload.enqueue(StateUpdate::Update(Box::new(updater)));
    }
    pub fn binding(&self) -> Binding<T> {
        record_state_dependency(self.payload.target);
        Binding {
            prop_payload: self.payload.clone(),
            snapshot: self.snapshot.clone(),
        }
    }
}

impl<T: 'static> fmt::Debug for GlobalState<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlobalState").finish()
    }
}

impl<T: 'static> PartialEq for GlobalState<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.payload, &other.payload) && Rc::ptr_eq(&self.snapshot, &other.snapshot)
    }
}

/// Current `build_depth` — the number of active `build_scope` frames.
/// Zero means no root render is running.
pub fn current_build_depth() -> usize {
    STORE.with(|store| store.borrow().build_depth)
}

fn shrink_map_if_sparse<K: Eq + Hash, V>(map: &mut FxHashMap<K, V>) {
    const MIN_CAPACITY_BEFORE_SHRINK: usize = 256;
    let target = map.len().saturating_mul(2).max(16);
    if map.capacity() > MIN_CAPACITY_BEFORE_SHRINK && map.capacity() > target.saturating_mul(2) {
        map.shrink_to(target);
    }
}

fn shrink_set_if_sparse<T: Eq + Hash>(set: &mut FxHashSet<T>) {
    const MIN_CAPACITY_BEFORE_SHRINK: usize = 256;
    let target = set.len().saturating_mul(2).max(16);
    if set.capacity() > MIN_CAPACITY_BEFORE_SHRINK && set.capacity() > target.saturating_mul(2) {
        set.shrink_to(target);
    }
}

/// The outermost scope is a root render: it resets live-key tracking on
/// entry and, if any component rendered, retires the state of every
/// component that did not. Nested scopes only add depth.
pub fn build_scope<R>(f: impl FnOnce() -> R) -> R {
    let _frame = begin_state_frame();
    struct UnwindGuard {
        depth: usize,
        contexts: usize,
        memos: usize,
    }
    impl Drop for UnwindGuard {
        fn drop(&mut self) {
            if std::thread::panicking() {
                STORE.with(|store| store.borrow_mut().build_depth = self.depth);
                CONTEXT.with(|context| context.borrow_mut().frames.truncate(self.contexts));
                MEMO_STACK.with(|stack| stack.borrow_mut().truncate(self.memos));
                PENDING_MOUNTS.with(|mounts| mounts.borrow_mut().clear());
            }
        }
    }
    let _unwind = UnwindGuard {
        depth: current_build_depth(),
        contexts: CONTEXT.with(|context| context.borrow().frames.len()),
        memos: MEMO_STACK.with(|stack| stack.borrow().len()),
    };
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        if store.build_depth == 0 {
            store.root_cursor = 0;
            store.live_keys.clear();
            store.live_global_keys.clear();
            store.active_build_global_keys.clear();
            store.components_rendered_in_build = false;
            LIVE_TIMER_HOOKS.with(|hooks| hooks.borrow_mut().clear());
            LIVE_MOUNT_HOOKS.with(|hooks| hooks.borrow_mut().clear());
            LIVE_VIEWPORT_POINTER_HOOKS.with(|hooks| hooks.borrow_mut().clear());
        }
        store.build_depth += 1;
    });

    let out = f();
    let mut retired_slots = Vec::new();
    let mut retired_mounts = Vec::new();
    let mut retired_memos = Vec::new();

    STORE.with(|store| {
        let mut store = store.borrow_mut();
        store.build_depth = store.build_depth.saturating_sub(1);
        if store.build_depth == 0 && store.components_rendered_in_build {
            let live = store.live_keys.clone();
            let live_global = store.live_global_keys.clone();
            store.lifetimes.retain(|key, alive| {
                let keep = live.contains(key);
                if !keep {
                    alive.set(false);
                }
                keep
            });
            let retired: Vec<_> = store
                .slots
                .keys()
                .filter(|key| !live.contains(*key))
                .cloned()
                .collect();
            for key in retired {
                if let Some(slots) = store.slots.remove(&key) {
                    retired_slots.extend(slots);
                }
            }
            store
                .global_component_keys
                .retain(|key, _| live_global.contains(key));
            // Prune memo cache of components that did not render this build.
            let retired: Vec<_> = store
                .memo_cache
                .keys()
                .filter(|key| !live.contains(*key))
                .cloned()
                .collect();
            for key in retired {
                if let Some(entry) = store.remove_memo(&key) {
                    retired_memos.push(entry);
                }
            }
            store.dirty_memo_components.retain(|key| live.contains(key));
            shrink_map_if_sparse(&mut store.target_consumers);
            shrink_map_if_sparse(&mut store.component_memos);
            shrink_set_if_sparse(&mut store.dirty_memo_components);
            shrink_map_if_sparse(&mut store.slots);
            shrink_map_if_sparse(&mut store.global_component_keys);
            shrink_map_if_sparse(&mut store.memo_cache);
            shrink_set_if_sparse(&mut store.live_keys);
            shrink_set_if_sparse(&mut store.live_global_keys);
            shrink_set_if_sparse(&mut store.active_build_global_keys);
            LIVE_TIMER_HOOKS.with(|hooks| {
                let live_hooks = hooks.borrow().clone();
                TIMER_STORE.with(|timers| {
                    let mut timers = timers.borrow_mut();
                    timers.retain(|key, _| live_hooks.contains(key));
                    shrink_map_if_sparse(&mut timers);
                });
            });
            // Prune mount entries for unmounted components first so their
            // cleanups (via MountEntry::Drop) run before the newly queued
            // mount callbacks for surviving components execute.
            LIVE_MOUNT_HOOKS.with(|hooks| {
                let live_hooks = hooks.borrow().clone();
                MOUNT_STORE.with(|mounts| {
                    let mut mounts = mounts.borrow_mut();
                    let retired: Vec<_> = mounts
                        .keys()
                        .filter(|key| !live_hooks.contains(*key))
                        .cloned()
                        .collect();
                    for key in retired {
                        if let Some(entry) = mounts.remove(&key) {
                            retired_mounts.push(entry);
                        }
                    }
                    shrink_map_if_sparse(&mut mounts);
                });
            });
            LIVE_VIEWPORT_POINTER_HOOKS.with(|hooks| {
                let live_hooks = hooks.borrow().clone();
                VIEWPORT_POINTER_DOWN_HOOKS.with(|store| {
                    let mut store = store.borrow_mut();
                    store.retain(|key, _| live_hooks.contains(key));
                    shrink_map_if_sparse(&mut store);
                });
                VIEWPORT_POINTER_MOVE_HOOKS.with(|store| {
                    let mut store = store.borrow_mut();
                    store.retain(|key, _| live_hooks.contains(key));
                    shrink_map_if_sparse(&mut store);
                });
                VIEWPORT_POINTER_UP_HOOKS.with(|store| {
                    let mut store = store.borrow_mut();
                    store.retain(|key, _| live_hooks.contains(key));
                    shrink_map_if_sparse(&mut store);
                });
                VIEWPORT_POINTER_STATE_HOOKS.with(|store| {
                    let mut store = store.borrow_mut();
                    store.retain(|key| live_hooks.contains(key));
                    shrink_set_if_sparse(&mut store);
                });
            });
        }
    });

    // User destructors and mount cleanups must run without a borrowed store.
    drop(retired_memos);
    drop(retired_slots);
    drop(retired_mounts);
    if current_build_depth() == 0 {
        drain_pending_mounts();
    }
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

/// Rejects a `GlobalKey` reused within one root render. A description built
/// outside one (event handler, timer) belongs to no build yet.
pub fn register_global_key(global_key: GlobalKey) {
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        if store.build_depth == 0 {
            return;
        }
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

/// Compute a component key from the current parent/root cursor exactly once
/// per invocation, for both the typed API and the type-erased walker.
fn next_component_key_by_type_id(type_id: TypeId) -> ComponentKey {
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
    ComponentKey { type_id, path }
}

pub fn render_component<T: 'static, R>(f: impl FnOnce() -> R) -> R {
    render_component_by_type_id(TypeId::of::<T>(), f)
}

/// Type-id-driven variant of [`render_component`] for the React parity
/// walker (P2). The `unwrap_components` walker holds a type-erased
/// `ComponentNodeInner` — the concrete `T` is lost, so the frame / live
/// keys / context push machinery is parameterized by `TypeId`.
pub fn render_component_by_type_id<R>(type_id: TypeId, f: impl FnOnce() -> R) -> R {
    let key = next_component_key_by_type_id(type_id);

    STORE.with(|store| {
        let mut store = store.borrow_mut();
        store.components_rendered_in_build = true;
        store.live_keys.insert(key.clone());
        if let Some(RsxKey::Global(global_key)) = current_rsx_key() {
            store.live_global_keys.insert(global_key);
            store.global_component_keys.insert(global_key, key.clone());
        }
    });
    memo_stack_record_component_key(&key);
    if let Some(RsxKey::Global(global_key)) = current_rsx_key() {
        memo_stack_record_global_key(global_key);
    }

    CONTEXT.with(|context| {
        context.borrow_mut().frames.push(Frame {
            key: key.clone(),
            path: key.path.clone(),
            child_cursor: 0,
            hook_cursor: 0,
            state_cursor: 0,
        });
    });

    let out = f();

    CONTEXT.with(|context| {
        let _ = context.borrow_mut().frames.pop();
    });

    out
}

/// Render a component with prop-based memoization.
///
/// Semantics (React `memo` equivalent):
/// 1. Compute the `ComponentKey` just like [`render_component`].
/// 2. If the component is NOT marked dirty (its own `use_state` slots are
///    unchanged since the last render) AND the cached props compare equal to
///    `props`, return a clone of the cached `RsxNode` and replay the set of
///    descendant component/global keys and timer hooks so the GC in
///    [`build_scope`] keeps them alive.
/// 3. Otherwise, push a `MemoFrame` and a component `Frame`, invoke `render`,
///    capture all keys registered underneath, store the new `MemoEntry` and
///    return the rendered node.
///
/// The caller is responsible for supplying a `Props` type that is
/// `PartialEq + Clone + 'static`. If two consecutive renders pass structurally
/// equal props, the render closure is skipped entirely and (combined with the
/// reconciler's `Rc::ptr_eq` bailout) the entire subtree is also bypassed
/// during diffing.
pub fn render_memoized_component<T, P>(
    props: P,
    render: impl FnOnce(&P) -> crate::ui::RsxNode,
) -> crate::ui::RsxNode
where
    T: 'static,
    P: PartialEq + Clone + 'static,
{
    render_memoized_component_by_type_id(TypeId::of::<T>(), props, |props| {
        crate::ui::unwrap_components(render(props))
    })
}

/// The walker supplies fully resolved output, including component identity.
pub(crate) fn render_memoized_component_by_type_id<P: PartialEq + 'static>(
    type_id: TypeId,
    props: P,
    render: impl FnOnce(&P) -> crate::ui::RsxNode,
) -> crate::ui::RsxNode {
    let key = next_component_key_by_type_id(type_id);
    let current_key = current_rsx_key();

    // Register this component as live regardless of memo hit / miss — it
    // executed during this build, so its slots must survive the GC sweep.
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        store.components_rendered_in_build = true;
        store.live_keys.insert(key.clone());
        if let Some(RsxKey::Global(global_key)) = current_key {
            store.live_global_keys.insert(global_key);
            store.global_component_keys.insert(global_key, key.clone());
        }
    });
    memo_stack_record_component_key(&key);
    if let Some(RsxKey::Global(global_key)) = current_key {
        memo_stack_record_global_key(global_key);
    }

    // Can we take the fast path? Only if: the component is NOT dirty AND the
    // cached props match the new props.
    let cached_hit = STORE.with(|store| {
        let store = store.borrow();
        if store.dirty_memo_components.contains(&key) {
            return None;
        }
        let entry = store.memo_cache.get(&key)?;
        if entry.volatile || !entry.context_dependencies.matches_current() {
            return None;
        }
        let eq = (entry.props_eq)(&*entry.props, &props as &dyn Any);
        if !eq {
            return None;
        }
        Some((
            entry.node.clone(),
            entry.live_keys.clone(),
            entry.live_global_keys.clone(),
            entry.live_timer_hooks.clone(),
            entry.live_mount_hooks.clone(),
            entry.state_dependencies.clone(),
            entry.live_viewport_pointer_hooks.clone(),
            entry.context_dependencies.clone(),
        ))
    });

    if let Some((node, lk, lgk, lth, lmh, deps, lvph, contexts)) = cached_hit {
        contexts.replay();
        crate::ui::work_profile::count(|p| p.memo_hits += 1);
        // Replay descendants — both into the thread-local live sets that
        // `build_scope` uses for GC, and into any enclosing memo frame.
        STORE.with(|store| {
            let mut store = store.borrow_mut();
            for k in &lk {
                store.live_keys.insert(k.clone());
            }
            for k in &lgk {
                store.live_global_keys.insert(*k);
            }
        });
        LIVE_TIMER_HOOKS.with(|hooks| {
            let mut hooks = hooks.borrow_mut();
            for k in &lth {
                hooks.insert(k.clone());
            }
        });
        LIVE_MOUNT_HOOKS.with(|hooks| hooks.borrow_mut().extend(lmh.iter().cloned()));
        LIVE_VIEWPORT_POINTER_HOOKS.with(|hooks| {
            let mut hooks = hooks.borrow_mut();
            for k in &lvph {
                hooks.insert(k.clone());
            }
        });
        MEMO_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            if let Some(top) = stack.last_mut() {
                for k in &lk {
                    top.live_keys.insert(k.clone());
                }
                for k in &lgk {
                    top.live_global_keys.insert(*k);
                }
                for k in &lth {
                    top.live_timer_hooks.insert(k.clone());
                }
                top.live_mount_hooks.extend(lmh.iter().cloned());
                top.state_dependencies.extend(deps.iter().cloned());
                for k in &lvph {
                    top.live_viewport_pointer_hooks.insert(k.clone());
                }
            }
        });
        return node;
    }

    // Keep a failed render dirty, including a props miss followed by a panic.
    // Only successful publication below can make this memo reusable again.
    STORE.with(|store| store.borrow_mut().dirty_memo_components.insert(key.clone()));

    // Miss — run the render closure under a fresh `MemoFrame` so we can
    // capture every descendant key that gets registered.
    MEMO_STACK.with(|stack| {
        stack.borrow_mut().push(MemoFrame {
            context_boundary: super::context::current_epoch(),
            ..Default::default()
        });
    });
    CONTEXT.with(|context| {
        context.borrow_mut().frames.push(Frame {
            key: key.clone(),
            path: key.path.clone(),
            child_cursor: 0,
            hook_cursor: 0,
            state_cursor: 0,
        });
    });

    // P2 (React parity): memo cache stores resolved trees (no
    // `RsxNode::Component` variants). If we cached lazy trees, the cache
    // hit would `Rc::clone` the Component node, sharing its `Rc` with the
    // cached copy — the walker later panics on `Rc::try_unwrap`. Unwrap
    // eagerly inside the memo frame so the component's render subtree
    // is fully flattened before caching and returning.
    let node = render(&props);

    CONTEXT.with(|context| {
        let _ = context.borrow_mut().frames.pop();
    });
    let frame = MEMO_STACK
        .with(|stack| stack.borrow_mut().pop())
        .unwrap_or_default();

    // Propagate the captured descendants into any enclosing memo frame so
    // a memo hit on an outer component keeps our subtree alive too.
    MEMO_STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        if let Some(top) = stack.last_mut() {
            for k in &frame.live_keys {
                top.live_keys.insert(k.clone());
            }
            for k in &frame.live_global_keys {
                top.live_global_keys.insert(*k);
            }
            for k in &frame.live_timer_hooks {
                top.live_timer_hooks.insert(k.clone());
            }
            top.live_mount_hooks
                .extend(frame.live_mount_hooks.iter().cloned());
            top.state_dependencies
                .extend(frame.state_dependencies.iter().cloned());
            for k in &frame.live_viewport_pointer_hooks {
                top.live_viewport_pointer_hooks.insert(k.clone());
            }
        }
    });

    let retired = STORE.with(|store| {
        store.borrow_mut().replace_memo(
            key,
            MemoEntry {
                context_dependencies: frame.context_dependencies,
                volatile: frame.volatile,
                props: Box::new(props),
                node: node.clone(),
                props_eq: memo_props_eq::<P>,
                live_keys: frame.live_keys,
                live_global_keys: frame.live_global_keys,
                live_timer_hooks: frame.live_timer_hooks,
                live_mount_hooks: frame.live_mount_hooks,
                state_dependencies: frame.state_dependencies,
                live_viewport_pointer_hooks: frame.live_viewport_pointer_hooks,
            },
        )
    });
    drop(retired);

    node
}

pub fn use_state<T: Clone + PartialEq + 'static>(init: impl FnOnce() -> T) -> State<T> {
    use_state_with_dirty_state(init, UiDirtyState::REBUILD)
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
        let index = frame.state_cursor;
        frame.state_cursor += 1;
        (frame.key.clone(), index)
    });

    let owner_key = key.clone();
    // Run the initializer without holding the store borrow: initial state
    // may hold RSX, and a `GlobalKey` in it registers itself in the store.
    let needs_init = STORE.with(|store| {
        store
            .borrow()
            .slots
            .get(&key)
            .is_none_or(|slots| slots.len() <= slot_index)
    });
    let initial = needs_init.then(init);
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        let alive = store
            .lifetimes
            .entry(key.clone())
            .or_insert_with(|| Rc::new(Cell::new(true)))
            .clone();
        let slots = store.slots.entry(key).or_default();
        if let Some(value) = initial {
            assert_eq!(
                slots.len(),
                slot_index,
                "use_state initializer must not call hooks"
            );
            let payload = Rc::new(BindingPropPayload::new(
                value,
                dirty_state,
                Some(owner_key.clone()),
                alive,
            ));
            slots.push(Box::new(payload));
        }
        let payload = slots[slot_index]
            .downcast_ref::<Rc<BindingPropPayload<T>>>()
            .unwrap_or_else(|| panic!("use_state slot type mismatch at index {}", slot_index))
            .clone();
        let snapshot = payload.value.borrow().clone();
        State { payload, snapshot }
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
    memo_stack_record_timer_hook(&key);

    TIMER_STORE.with(|timers| {
        let mut timers = timers.borrow_mut();
        let now = Instant::now();
        let interval = (mode == TimerMode::Interval).then_some(duration);
        let callback: TimerCallback = Rc::new(RefCell::new(callback));
        match timers.get_mut(&key) {
            Some(entry) => {
                let should_reset =
                    entry.mode != mode || entry.duration != duration || !entry.timer.is_scheduled();
                entry.mode = mode;
                entry.duration = duration;
                *entry.callback.borrow_mut() = callback;
                if !enabled {
                    entry.timer.cancel();
                } else if should_reset {
                    entry.timer.schedule(now + duration, interval);
                }
            }
            None => {
                let callback = Rc::new(RefCell::new(callback));
                let callback_for_timer = callback.clone();
                let timer = crate::time::timers::Timer::new(move || {
                    let callback = callback_for_timer.borrow().clone();
                    let _batch = begin_state_batch();
                    (callback.borrow_mut())();
                });
                if enabled {
                    timer.schedule(now + duration, interval);
                }
                timers.insert(
                    key,
                    TimerEntry {
                        mode,
                        duration,
                        timer,
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

fn next_viewport_pointer_hook_key(name: &str) -> ViewportPointerHookKey {
    let (component, hook_index) = CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        let frame = context
            .frames
            .last_mut()
            .unwrap_or_else(|| panic!("{name}() must be called inside #[component] render"));
        let index = frame.hook_cursor;
        frame.hook_cursor += 1;
        (frame.key.clone(), index)
    });

    let key = ViewportPointerHookKey {
        component,
        hook_index,
    };
    LIVE_VIEWPORT_POINTER_HOOKS.with(|hooks| {
        hooks.borrow_mut().insert(key.clone());
    });
    memo_stack_record_viewport_pointer_hook(&key);
    key
}

pub fn use_viewport_pointer_down<F>(handler: F)
where
    F: FnMut(&ViewportPointerDownEvent) + 'static,
{
    let key = next_viewport_pointer_hook_key("use_viewport_pointer_down");
    VIEWPORT_POINTER_DOWN_HOOKS.with(|store| {
        store
            .borrow_mut()
            .insert(key, Rc::new(RefCell::new(handler)));
    });
}

pub fn use_viewport_pointer_move<F>(handler: F)
where
    F: FnMut(&ViewportPointerMoveEvent) + 'static,
{
    let key = next_viewport_pointer_hook_key("use_viewport_pointer_move");
    VIEWPORT_POINTER_MOVE_HOOKS.with(|store| {
        store
            .borrow_mut()
            .insert(key, Rc::new(RefCell::new(handler)));
    });
}

pub fn use_viewport_pointer_up<F>(handler: F)
where
    F: FnMut(&ViewportPointerUpEvent) + 'static,
{
    let key = next_viewport_pointer_hook_key("use_viewport_pointer_up");
    VIEWPORT_POINTER_UP_HOOKS.with(|store| {
        store
            .borrow_mut()
            .insert(key, Rc::new(RefCell::new(handler)));
    });
}

pub fn use_viewport_pointer_state() -> ViewportPointerState {
    let key = next_viewport_pointer_hook_key("use_viewport_pointer_state");
    VIEWPORT_POINTER_STATE_HOOKS.with(|store| {
        store.borrow_mut().insert(key);
    });
    VIEWPORT_POINTER_STATE.with(|state| state.borrow().clone())
}

pub fn use_viewport_pointer_position() -> Option<(f32, f32)> {
    use_viewport_pointer_state().position
}

pub fn use_viewport_pointer_target() -> Option<EventMetaSnapshot> {
    use_viewport_pointer_state().target
}

pub fn use_viewport_pointer_buttons() -> PointerButtons {
    use_viewport_pointer_state().buttons
}

#[doc(hidden)]
pub fn has_viewport_pointer_hooks() -> bool {
    VIEWPORT_POINTER_DOWN_HOOKS.with(|down| {
        if !down.borrow().is_empty() {
            return true;
        }
        VIEWPORT_POINTER_MOVE_HOOKS.with(|move_hooks| {
            if !move_hooks.borrow().is_empty() {
                return true;
            }
            VIEWPORT_POINTER_UP_HOOKS.with(|up| !up.borrow().is_empty())
        })
    })
}

fn has_viewport_pointer_state_hooks() -> bool {
    VIEWPORT_POINTER_STATE_HOOKS.with(|hooks| !hooks.borrow().is_empty())
}

fn notify_viewport_pointer_state_changed() {
    if has_viewport_pointer_state_hooks() {
        STATE_DIRTY.with(|dirty| dirty.set(dirty.get().union(UiDirtyState::REBUILD)));
        let owners = VIEWPORT_POINTER_STATE_HOOKS.with(|hooks| {
            hooks
                .borrow()
                .iter()
                .map(|key| key.component.clone())
                .collect::<FxHashSet<_>>()
        });
        STORE.with(|store| {
            let mut store = store.borrow_mut();
            let mut affected = FxHashSet::default();
            for owner in owners {
                store.collect_owner_memos(&owner, &mut affected);
            }
            store.invalidate_memos(affected);
        });
        request_state_wakeup();
    }
}

#[doc(hidden)]
pub fn dispatch_viewport_pointer_down_hook(event: ViewportPointerDownEvent) {
    let _batch = begin_state_batch();
    let callbacks = VIEWPORT_POINTER_DOWN_HOOKS.with(|store| {
        store
            .borrow()
            .values()
            .cloned()
            .collect::<Vec<ViewportPointerDownCallback>>()
    });
    VIEWPORT_POINTER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.position = Some((event.pointer.viewport_x, event.pointer.viewport_y));
        state.buttons = event.pointer.buttons;
        state.target = Some(event.meta.clone());
        state.latest_down = Some(event.clone());
    });
    notify_viewport_pointer_state_changed();
    for callback in callbacks {
        (callback.borrow_mut())(&event);
    }
}

#[doc(hidden)]
pub fn dispatch_viewport_pointer_move_hook(event: ViewportPointerMoveEvent) {
    let _batch = begin_state_batch();
    let callbacks = VIEWPORT_POINTER_MOVE_HOOKS.with(|store| {
        store
            .borrow()
            .values()
            .cloned()
            .collect::<Vec<ViewportPointerMoveCallback>>()
    });
    VIEWPORT_POINTER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.position = Some((event.pointer.viewport_x, event.pointer.viewport_y));
        state.buttons = event.pointer.buttons;
        state.target = Some(event.meta.clone());
        state.latest_move = Some(event.clone());
    });
    notify_viewport_pointer_state_changed();
    for callback in callbacks {
        (callback.borrow_mut())(&event);
    }
}

#[doc(hidden)]
pub fn dispatch_viewport_pointer_up_hook(event: ViewportPointerUpEvent) {
    let _batch = begin_state_batch();
    let callbacks = VIEWPORT_POINTER_UP_HOOKS.with(|store| {
        store
            .borrow()
            .values()
            .cloned()
            .collect::<Vec<ViewportPointerUpCallback>>()
    });
    VIEWPORT_POINTER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.position = Some((event.pointer.viewport_x, event.pointer.viewport_y));
        state.buttons = event.pointer.buttons;
        state.target = Some(event.meta.clone());
        state.latest_up = Some(event.clone());
    });
    notify_viewport_pointer_state_changed();
    for callback in callbacks {
        (callback.borrow_mut())(&event);
    }
}

/// Run a mount callback exactly once when the component first renders. If
/// `mount` returns a closure, that closure is registered as cleanup and runs
/// when the component unmounts. Subsequent re-renders of the same component
/// are no-ops.
pub fn use_mount<F, R>(mount: F)
where
    F: FnOnce() -> R + 'static,
    R: MountCleanup + 'static,
{
    let (component, hook_index) = CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        let frame = context
            .frames
            .last_mut()
            .expect("use_mount() must be called inside #[component] render");
        let index = frame.hook_cursor;
        frame.hook_cursor += 1;
        (frame.key.clone(), index)
    });

    let key = MountHookKey {
        component,
        hook_index,
    };
    LIVE_MOUNT_HOOKS.with(|hooks| {
        hooks.borrow_mut().insert(key.clone());
    });
    MEMO_STACK.with(|stack| {
        if let Some(frame) = stack.borrow_mut().last_mut() {
            frame.live_mount_hooks.insert(key.clone());
        }
    });

    let is_first = MOUNT_STORE.with(|store| {
        let mut store = store.borrow_mut();
        if store.contains_key(&key) {
            false
        } else {
            store.insert(key.clone(), MountEntry { cleanup: None });
            true
        }
    });

    if !is_first {
        return;
    }

    let run_key = key;
    let runner: Box<dyn FnOnce()> = Box::new(move || {
        let new_cleanup = mount().into_cleanup();
        MOUNT_STORE.with(|store| {
            let mut store = store.borrow_mut();
            if let Some(entry) = store.get_mut(&run_key) {
                entry.cleanup = new_cleanup;
            } else if let Some(cleanup) = new_cleanup {
                // Entry was pruned before drain (component unmounted mid-build);
                // run cleanup immediately to honor symmetry.
                cleanup();
            }
        });
    });

    PENDING_MOUNTS.with(|pending| pending.borrow_mut().push(runner));
}

fn drain_pending_mounts() {
    loop {
        let batch: Vec<Box<dyn FnOnce()>> = PENDING_MOUNTS.with(|pending| {
            let mut pending = pending.borrow_mut();
            std::mem::take(&mut *pending)
        });
        if batch.is_empty() {
            break;
        }
        for runner in batch {
            runner();
        }
    }
}

/// Earliest component timer or retained viewport animation deadline on this
/// thread. Hosts can use it as their event-loop wake-up time.
pub fn next_timer_deadline() -> Option<Instant> {
    crate::time::timers::next_deadline()
}

/// Dispatch due work once, then let the host drain normal redraw requests.
/// Timers whose owners have unmounted or been dropped are canceled.
pub fn run_due_timers(now: Instant) {
    crate::time::timers::run_due(now);
}

pub(crate) fn request_timer_redraw() {
    request_state_wakeup();
}

fn global_payload_with_init<T: Clone + PartialEq + 'static>(
    init: impl FnOnce() -> T,
) -> Rc<BindingPropPayload<T>> {
    let mut init_opt = Some(init);
    GLOBAL_STORE.with(|store| {
        let mut store = store.borrow_mut();
        let type_id = TypeId::of::<T>();
        if !store.contains_key(&type_id) {
            let value = (init_opt
                .take()
                .expect("global_state initializer should only run once"))();
            let payload = Rc::new(BindingPropPayload::new(
                value,
                UiDirtyState::REBUILD,
                None,
                Rc::new(Cell::new(true)),
            ));
            store.insert(type_id, Box::new(payload));
        }
        store[&type_id]
            .downcast_ref::<Rc<BindingPropPayload<T>>>()
            .unwrap_or_else(|| panic!("global_state type mismatch for {:?}", type_id))
            .clone()
    })
}

fn global_payload<T: Clone + PartialEq + 'static>() -> Option<Rc<BindingPropPayload<T>>> {
    GLOBAL_STORE.with(|store| {
        let store = store.borrow();
        let type_id = TypeId::of::<T>();
        let value = store.get(&type_id)?;
        Some(
            value
                .downcast_ref::<Rc<BindingPropPayload<T>>>()
                .unwrap_or_else(|| panic!("global_state type mismatch for {:?}", type_id))
                .clone(),
        )
    })
}

pub fn global_state<T: Clone + PartialEq + 'static>(init: impl FnOnce() -> T) -> GlobalState<T> {
    GlobalState::from_payload(global_payload_with_init(init))
}

#[allow(non_snake_case)]
pub fn globalState<T: Clone + PartialEq + 'static>(init: impl FnOnce() -> T) -> GlobalState<T> {
    global_state(init)
}

pub fn use_global_state<T: Clone + PartialEq + 'static>() -> GlobalState<T> {
    let payload = global_payload::<T>().unwrap_or_else(|| {
        panic!(
            "use_global_state::<{}>() called before global_state/globalState initialization",
            std::any::type_name::<T>()
        )
    });
    GlobalState::from_payload(payload)
}

#[cfg(test)]
mod queue_tests;
#[cfg(test)]
mod tests;

pub fn set_redraw_callback<F>(callback: F)
where
    F: Fn() + 'static,
{
    REDRAW_CALLBACK.with(|slot| {
        *slot.borrow_mut() = Some(Rc::new(callback));
    });
    WAKE_REQUESTED.set(false);
    if peek_state_dirty().has_any() {
        request_state_wakeup();
    }
}

pub fn clear_redraw_callback() {
    REDRAW_CALLBACK.with(|slot| {
        *slot.borrow_mut() = None;
    });
    WAKE_REQUESTED.set(false);
}

pub fn peek_state_dirty() -> UiDirtyState {
    PENDING_STATE.with(|queue| {
        queue
            .borrow()
            .iter()
            .fold(STATE_DIRTY.with(Cell::get), |dirty, state| {
                dirty.union(state.dirty())
            })
    })
}

pub fn take_state_dirty() -> UiDirtyState {
    STATE_DIRTY.with(|dirty| {
        let was_dirty = dirty.get();
        dirty.set(UiDirtyState::NONE);
        was_dirty
    })
}

impl<T: Clone + PartialEq + 'static> IntoPropValue for Binding<T> {
    fn into_prop_value(self) -> PropValue {
        record_state_dependency(self.prop_payload.target);
        let cached = self.prop_payload.prop_view.borrow().upgrade();
        let view = cached
            .filter(|view| Rc::ptr_eq(&view.snapshot, &self.snapshot))
            .unwrap_or_else(|| {
                let view = Rc::new(BindingPropView {
                    payload: self.prop_payload.clone(),
                    snapshot: self.snapshot,
                });
                *self.prop_payload.prop_view.borrow_mut() = Rc::downgrade(&view);
                view
            });
        PropValue::Shared(SharedPropValue::new(view))
    }
}

impl<T: Clone + PartialEq + 'static> FromPropValue for Binding<T> {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::Shared(shared) => {
                let erased = shared.value();
                if let Ok(view) = Rc::downcast::<BindingPropView<T>>(erased.clone()) {
                    record_state_dependency(view.payload.target);
                    return Ok(Self {
                        prop_payload: view.payload.clone(),
                        snapshot: view.snapshot.clone(),
                    });
                }
                let cell = Rc::downcast::<RefCell<T>>(erased)
                    .map_err(|_| "expected Binding value with matching type".to_string())?;
                Ok(Self::from_cell(cell, UiDirtyState::REBUILD))
            }
            _ => Err("expected Binding value".to_string()),
        }
    }
}
