#![allow(missing_docs)]

//! Stateful hooks and global state helpers used by typed RSX components.
use rustc_hash::{FxHashMap, FxHashSet};

use crate::time::{Duration, Instant};
use crate::ui::{FromPropValue, GlobalKey, IntoPropValue, PropValue, RsxKey, SharedPropValue};
use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;

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
    /// The component that owns this state slot (if any). Used by the memo
    /// cache to invalidate only that component's cached render when the slot
    /// changes. `None` means the state is not owned by a specific component
    /// (e.g. a free-standing `Binding` or a `GlobalState`) and conservatively
    /// flushes the entire memo cache on change.
    owner_component: Option<ComponentKey>,
}

#[derive(Clone)]
pub struct Binding<T: 'static> {
    prop_payload: Rc<BindingPropPayload<T>>,
}

impl<T: 'static> Binding<T> {
    pub fn new(initial: T) -> Self {
        Self::from_cell(Rc::new(RefCell::new(initial)), UiDirtyState::REBUILD)
    }

    pub fn new_with_dirty_state(initial: T, dirty_state: UiDirtyState) -> Self {
        Self::from_cell(Rc::new(RefCell::new(initial)), dirty_state)
    }

    pub(crate) fn from_cell(cell: Rc<RefCell<T>>, dirty_state: UiDirtyState) -> Self {
        Self::from_payload(Rc::new(BindingPropPayload {
            cell,
            dirty_state,
            owner_component: None,
        }))
    }

    fn from_payload(prop_payload: Rc<BindingPropPayload<T>>) -> Self {
        Self { prop_payload }
    }

    fn cell(&self) -> &Rc<RefCell<T>> {
        &self.prop_payload.cell
    }

    fn dirty_state(&self) -> UiDirtyState {
        self.prop_payload.dirty_state
    }
}

impl<T: Clone + PartialEq + 'static> Binding<T> {
    pub fn get(&self) -> T {
        self.cell().borrow().clone()
    }

    pub fn set(&self, value: T) {
        let mut current = self.cell().borrow_mut();
        if *current != value {
            *current = value;
            notify_state_changed(
                self.dirty_state(),
                self.prop_payload.owner_component.clone(),
            );
        }
    }

    pub fn update(&self, updater: impl FnOnce(&mut T)) {
        let mut current = self.cell().borrow_mut();
        let previous = current.clone();
        updater(&mut current);
        if *current != previous {
            notify_state_changed(
                self.dirty_state(),
                self.prop_payload.owner_component.clone(),
            );
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
        Rc::ptr_eq(&self.prop_payload, &other.prop_payload)
    }
}

#[derive(Clone)]
pub struct State<T: 'static> {
    payload: Rc<BindingPropPayload<T>>,
}

impl<T: Clone + PartialEq + 'static> State<T> {
    pub fn get(&self) -> T {
        self.payload.cell.borrow().clone()
    }

    pub fn set(&self, value: T) {
        let mut current = self.payload.cell.borrow_mut();
        if *current != value {
            *current = value;
            notify_state_changed(
                self.payload.dirty_state,
                self.payload.owner_component.clone(),
            );
        }
    }

    pub fn update(&self, updater: impl FnOnce(&mut T)) {
        let mut current = self.payload.cell.borrow_mut();
        let previous = current.clone();
        updater(&mut current);
        if *current != previous {
            notify_state_changed(
                self.payload.dirty_state,
                self.payload.owner_component.clone(),
            );
        }
    }

    pub fn binding(&self) -> Binding<T> {
        Binding::from_payload(self.payload.clone())
    }
}

impl<T: 'static> fmt::Debug for State<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State").finish()
    }
}

impl<T: 'static> PartialEq for State<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.payload, &other.payload)
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
    /// Components whose own state slots changed since their last render.
    /// A memo hit for a key in this set is forbidden — we must re-render.
    dirty_memo_components: FxHashSet<ComponentKey>,
}

/// A cached component render. `props` holds a type-erased clone of the last
/// props value, compared via the monomorphized `props_eq` function pointer.
struct MemoEntry {
    props: Box<dyn Any>,
    node: crate::ui::RsxNode,
    props_eq: fn(&dyn Any, &dyn Any) -> bool,
    live_keys: FxHashSet<ComponentKey>,
    live_global_keys: FxHashSet<GlobalKey>,
    live_timer_hooks: FxHashSet<TimerHookKey>,
}

/// A scope that captures which keys/hooks were registered during a render
/// inside a memoized component. Pushed by `render_memoized_component` and
/// popped once the render returns; the captured sets are stored in the
/// resulting [`MemoEntry`].
#[derive(Default)]
struct MemoFrame {
    live_keys: FxHashSet<ComponentKey>,
    live_global_keys: FxHashSet<GlobalKey>,
    live_timer_hooks: FxHashSet<TimerHookKey>,
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

struct TimerEntry {
    mode: TimerMode,
    enabled: bool,
    duration: Duration,
    next_fire_at: Instant,
    callback: Rc<RefCell<dyn FnMut()>>,
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

thread_local! {
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
    static PENDING_MOUNTS: RefCell<Vec<Box<dyn FnOnce()>>> = const { RefCell::new(Vec::new()) };
    /// Stack of in-progress memoized-component renders. Every registration of
    /// a `ComponentKey`, `GlobalKey`, or timer hook while this stack is
    /// non-empty is also recorded on the innermost frame so it can be
    /// reattached on a future memo hit.
    static MEMO_STACK: RefCell<Vec<MemoFrame>> = const { RefCell::new(Vec::new()) };
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

#[derive(Clone)]
pub struct GlobalState<T: 'static> {
    payload: Rc<BindingPropPayload<T>>,
}

impl<T: Clone + PartialEq + 'static> GlobalState<T> {
    pub fn get(&self) -> T {
        self.payload.cell.borrow().clone()
    }

    pub fn set(&self, value: T) {
        let mut current = self.payload.cell.borrow_mut();
        if *current != value {
            *current = value;
            notify_state_changed(
                self.payload.dirty_state,
                self.payload.owner_component.clone(),
            );
        }
    }

    pub fn update(&self, updater: impl FnOnce(&mut T)) {
        let mut current = self.payload.cell.borrow_mut();
        let previous = current.clone();
        updater(&mut current);
        if *current != previous {
            notify_state_changed(
                self.payload.dirty_state,
                self.payload.owner_component.clone(),
            );
        }
    }

    pub fn binding(&self) -> Binding<T> {
        Binding::from_payload(self.payload.clone())
    }
}

impl<T: 'static> fmt::Debug for GlobalState<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlobalState").finish()
    }
}

impl<T: 'static> PartialEq for GlobalState<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.payload, &other.payload)
    }
}

/// Current `build_depth` — the number of active `build_scope` frames.
/// Exposed for the React parity walker (`rsx_scope`) to detect the
/// outermost scope.
pub fn current_build_depth() -> usize {
    STORE.with(|store| store.borrow().build_depth)
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
            LIVE_MOUNT_HOOKS.with(|hooks| hooks.borrow_mut().clear());
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
            // Prune memo cache of components that did not render this build.
            store.memo_cache.retain(|k, _| live.contains(k));
            LIVE_TIMER_HOOKS.with(|hooks| {
                let live_hooks = hooks.borrow().clone();
                TIMER_STORE.with(|timers| {
                    timers
                        .borrow_mut()
                        .retain(|key, _| live_hooks.contains(key));
                });
            });
            // Prune mount entries for unmounted components first so their
            // cleanups (via MountEntry::Drop) run before the newly queued
            // mount callbacks for surviving components execute.
            LIVE_MOUNT_HOOKS.with(|hooks| {
                let live_hooks = hooks.borrow().clone();
                MOUNT_STORE.with(|mounts| {
                    mounts
                        .borrow_mut()
                        .retain(|key, _| live_hooks.contains(key));
                });
            });
            drain_pending_mounts();
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

/// Compute the `ComponentKey` for the next component invocation using the
/// same path algorithm as [`render_component`]. Advances parent/root cursors
/// as a side effect, so this must be called exactly once per component.
fn next_component_key<T: 'static>() -> ComponentKey {
    next_component_key_by_type_id(TypeId::of::<T>())
}

/// Type-id-driven variant for the React parity walker (P2). Identical path
/// algorithm to [`next_component_key`] but accepts a runtime `TypeId` so a
/// type-erased `ComponentNodeInner` can compute its own key during the
/// `unwrap_components` traversal.
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
    let key = next_component_key::<T>();
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
        let mut store = store.borrow_mut();
        let was_dirty = store.dirty_memo_components.remove(&key);
        if was_dirty {
            return None;
        }
        let entry = store.memo_cache.get(&key)?;
        let eq = (entry.props_eq)(&*entry.props, &props as &dyn Any);
        if !eq {
            return None;
        }
        Some((
            entry.node.clone(),
            entry.live_keys.clone(),
            entry.live_global_keys.clone(),
            entry.live_timer_hooks.clone(),
        ))
    });

    if let Some((node, lk, lgk, lth)) = cached_hit {
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
            }
        });
        return node;
    }

    // Miss — run the render closure under a fresh `MemoFrame` so we can
    // capture every descendant key that gets registered.
    MEMO_STACK.with(|stack| {
        stack.borrow_mut().push(MemoFrame::default());
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
    let node = crate::ui::unwrap_components(render(&props));

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
        }
    });

    STORE.with(|store| {
        store.borrow_mut().memo_cache.insert(
            key,
            MemoEntry {
                props: Box::new(props),
                node: node.clone(),
                props_eq: memo_props_eq::<P>,
                live_keys: frame.live_keys,
                live_global_keys: frame.live_global_keys,
                live_timer_hooks: frame.live_timer_hooks,
            },
        );
    });

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

    let mut init_opt = Some(init);
    let owner_key = key.clone();
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        let slots = store.slots.entry(key).or_default();
        if slots.len() <= slot_index {
            let value = (init_opt
                .take()
                .expect("use_state initializer should only run once"))();
            let payload: Rc<BindingPropPayload<T>> = Rc::new(BindingPropPayload {
                cell: Rc::new(RefCell::new(value)),
                dirty_state,
                owner_component: Some(owner_key.clone()),
            });
            slots.push(Box::new(payload));
        }
        let payload = slots[slot_index]
            .downcast_ref::<Rc<BindingPropPayload<T>>>()
            .unwrap_or_else(|| panic!("use_state slot type mismatch at index {}", slot_index))
            .clone();
        State { payload }
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
            let payload: Rc<BindingPropPayload<T>> = Rc::new(BindingPropPayload {
                cell: Rc::new(RefCell::new(value)),
                dirty_state: UiDirtyState::REBUILD,
                owner_component: None,
            });
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
    GlobalState {
        payload: global_payload_with_init(init),
    }
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
    GlobalState { payload }
}

#[cfg(test)]
mod tests {
    use super::{
        UiDirtyState, build_scope, next_timer_deadline, render_memoized_component,
        run_due_timers, take_state_dirty, use_interval, use_mount, use_state, use_timeout,
        with_component_key,
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

    // 軌 1 #13 regression: host tag `create_element` must not flip
    // `components_rendered_in_build`, so a `build_scope` that only builds
    // host tags (e.g. a TextArea `on_render` handler invoking `rsx!`
    // during layout) exits without pruning the main render's state slots.
    #[test]
    fn host_tag_only_build_scope_does_not_prune_user_state() {
        let state = build_scope(|| {
            crate::ui::render_component::<u32, _>(|| {
                let value = use_state(|| 0_i32);
                value.set(99);
                value
            })
        });
        assert_eq!(state.get(), 99);
        let _ = take_state_dirty();

        // Simulate handler-triggered `rsx!` producing only host tags.
        // Goes through the exact `create_element` path the handler's rsx! hits.
        let _ = build_scope(|| {
            crate::ui::create_element::<crate::view::Element>(
                crate::view::ElementPropSchema::default(),
                Vec::new(),
                None,
            )
        });

        // Re-render main component — state must survive.
        let after = build_scope(|| {
            crate::ui::render_component::<u32, _>(|| {
                let value = use_state(|| 0_i32);
                value.get()
            })
        });
        assert_eq!(after, 99);
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

    struct MemoProbeComponent;

    #[test]
    fn memoized_component_skips_render_when_props_equal() {
        let renders = Rc::new(Cell::new(0));

        let run = |props: i32| -> RsxNode {
            let counter = renders.clone();
            build_scope(|| {
                render_memoized_component::<MemoProbeComponent, _>(props, |_| {
                    counter.set(counter.get() + 1);
                    RsxNode::text("hit")
                })
            })
        };

        let first = run(1);
        assert_eq!(renders.get(), 1);

        // Same props → cached, render closure NOT invoked.
        let second = run(1);
        assert_eq!(renders.get(), 1);

        // Fast path returns the exact same `Rc` allocation, so the reconciler
        // bailout can short-circuit the entire subtree.
        assert!(
            RsxNode::ptr_eq(&first, &second),
            "memo hit should reuse the cached `Rc<RsxNode>`"
        );

        // Different props → render closure re-runs.
        let _ = run(2);
        assert_eq!(renders.get(), 2);
    }

    #[test]
    fn use_mount_runs_once_and_cleans_up_on_unmount() {
        let mounts = Rc::new(Cell::new(0));
        let cleanups = Rc::new(Cell::new(0));

        let build = |mounts: Rc<Cell<i32>>, cleanups: Rc<Cell<i32>>| {
            build_scope(|| {
                crate::ui::render_component::<u16, _>(|| {
                    let mounts = mounts.clone();
                    let cleanups = cleanups.clone();
                    use_mount(move || {
                        mounts.set(mounts.get() + 1);
                        move || cleanups.set(cleanups.get() + 1)
                    });
                })
            });
        };

        // Mount — callback fires once, no cleanup yet.
        build(mounts.clone(), cleanups.clone());
        assert_eq!(mounts.get(), 1);
        assert_eq!(cleanups.get(), 0);

        // Re-render — mount is a no-op.
        build(mounts.clone(), cleanups.clone());
        assert_eq!(mounts.get(), 1);
        assert_eq!(cleanups.get(), 0);

        // Unmount (a different component renders instead) — cleanup fires.
        build_scope(|| {
            crate::ui::render_component::<u32, _>(|| {});
        });
        assert_eq!(mounts.get(), 1);
        assert_eq!(cleanups.get(), 1);
    }

    #[test]
    fn memoized_component_reruns_when_its_own_state_changes() {
        let renders = Rc::new(Cell::new(0));
        let captured_state: Rc<std::cell::RefCell<Option<super::State<i32>>>> =
            Rc::new(std::cell::RefCell::new(None));

        let run = || -> RsxNode {
            let counter = renders.clone();
            let captured = captured_state.clone();
            build_scope(|| {
                render_memoized_component::<MemoProbeComponent, _>((), move |_| {
                    counter.set(counter.get() + 1);
                    let s = use_state(|| 0_i32);
                    *captured.borrow_mut() = Some(s);
                    RsxNode::text("hit")
                })
            })
        };

        let _ = run();
        assert_eq!(renders.get(), 1);

        // Same props, untouched state → cache hit, no re-render.
        let _ = run();
        assert_eq!(renders.get(), 1);

        // Mutating the component's own state must invalidate its memo entry.
        captured_state.borrow().as_ref().unwrap().set(7);
        let _ = take_state_dirty();
        let _ = run();
        assert_eq!(renders.get(), 2);
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

fn notify_state_changed(dirty_state: UiDirtyState, owner: Option<ComponentKey>) {
    STATE_DIRTY.with(|dirty| dirty.set(dirty.get().union(dirty_state)));
    if dirty_state.needs_rebuild() {
        STORE.with(|store| {
            let mut store = store.borrow_mut();
            match owner {
                Some(key) => {
                    // Targeted invalidation: only this component's memo entry
                    // is stale. Sibling/ancestor memos stay valid.
                    store.memo_cache.remove(&key);
                    store.dirty_memo_components.insert(key);
                }
                None => {
                    // Conservative flush: unowned state (global state, free
                    // bindings) could affect anything we have cached.
                    store.memo_cache.clear();
                    store.dirty_memo_components.clear();
                }
            }
        });
    }
    REDRAW_CALLBACK.with(|slot| {
        if let Some(callback) = slot.borrow().as_ref() {
            callback();
        }
    });
}

impl<T: Clone + PartialEq + 'static> IntoPropValue for Binding<T> {
    fn into_prop_value(self) -> PropValue {
        let erased: Rc<dyn Any> = self.prop_payload.clone();
        PropValue::Shared(SharedPropValue::new(erased))
    }
}

impl<T: Clone + PartialEq + 'static> FromPropValue for Binding<T> {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::Shared(shared) => {
                let erased = shared.value();
                if let Ok(payload) = Rc::downcast::<BindingPropPayload<T>>(erased.clone()) {
                    return Ok(Self::from_payload(payload));
                }
                let cell = Rc::downcast::<RefCell<T>>(erased)
                    .map_err(|_| "expected Binding value with matching type".to_string())?;
                Ok(Self::from_cell(cell, UiDirtyState::REBUILD))
            }
            _ => Err("expected Binding value".to_string()),
        }
    }
}
