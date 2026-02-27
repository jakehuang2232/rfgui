use std::any::{Any, TypeId};
use std::collections::hash_map::DefaultHasher;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::ui::{FromPropValue, IntoPropValue, PropValue, SharedPropValue};

#[derive(Clone)]
pub struct Binding<T: 'static> {
    cell: Rc<RefCell<T>>,
}

impl<T: 'static> Binding<T> {
    pub fn new(initial: T) -> Self {
        Self {
            cell: Rc::new(RefCell::new(initial)),
        }
    }

    pub(crate) fn from_cell(cell: Rc<RefCell<T>>) -> Self {
        Self { cell }
    }
}

impl<T: Clone + 'static> Binding<T> {
    pub fn get(&self) -> T {
        self.cell.borrow().clone()
    }

    pub fn set(&self, value: T) {
        *self.cell.borrow_mut() = value;
        notify_state_changed();
    }

    pub fn update(&self, updater: impl FnOnce(&mut T)) {
        updater(&mut self.cell.borrow_mut());
        notify_state_changed();
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
}

impl<T: Clone + 'static> State<T> {
    pub fn get(&self) -> T {
        self.cell.borrow().clone()
    }

    pub fn set(&self, value: T) {
        *self.cell.borrow_mut() = value;
        notify_state_changed();
    }

    pub fn update(&self, updater: impl FnOnce(&mut T)) {
        updater(&mut self.cell.borrow_mut());
        notify_state_changed();
    }

    pub fn binding(&self) -> Binding<T> {
        Binding::from_cell(self.cell.clone())
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
    components_rendered_in_build: bool,
}

thread_local! {
    static STORE: RefCell<StateStore> = RefCell::new(StateStore::default());
    static GLOBAL_STORE: RefCell<HashMap<TypeId, Box<dyn Any>>> = RefCell::new(HashMap::new());
    static CONTEXT: RefCell<RenderContext> = RefCell::new(RenderContext::default());
    static COMPONENT_KEY_STACK: RefCell<Vec<Option<u64>>> = const { RefCell::new(Vec::new()) };
    static REDRAW_CALLBACK: RefCell<Option<Rc<dyn Fn()>>> = RefCell::new(None);
    static STATE_DIRTY: Cell<bool> = const { Cell::new(false) };
}

#[derive(Clone)]
pub struct GlobalState<T: 'static> {
    cell: Rc<RefCell<T>>,
}

impl<T: Clone + 'static> GlobalState<T> {
    pub fn get(&self) -> T {
        self.cell.borrow().clone()
    }

    pub fn set(&self, value: T) {
        *self.cell.borrow_mut() = value;
        notify_state_changed();
    }

    pub fn update(&self, updater: impl FnOnce(&mut T)) {
        updater(&mut self.cell.borrow_mut());
        notify_state_changed();
    }

    pub fn binding(&self) -> Binding<T> {
        Binding::from_cell(self.cell.clone())
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
            store.components_rendered_in_build = false;
        }
        store.build_depth += 1;
    });

    let out = f();

    STORE.with(|store| {
        let mut store = store.borrow_mut();
        store.build_depth = store.build_depth.saturating_sub(1);
        if store.build_depth == 0 && store.components_rendered_in_build {
            let live = store.live_keys.clone();
            store.slots.retain(|k, _| live.contains(k));
        }
    });

    out
}

pub fn component_key_token<T: ?Sized + Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

pub fn with_component_key<R>(key: Option<u64>, f: impl FnOnce() -> R) -> R {
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

fn current_component_key() -> Option<u64> {
    COMPONENT_KEY_STACK.with(|stack| stack.borrow().last().copied().flatten())
}

pub fn render_component<T: 'static, R>(f: impl FnOnce() -> R) -> R {
    const KEYED_PATH_MARKER: usize = usize::MAX;
    let path = CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        let component_key = current_component_key();
        if let Some(parent) = context.frames.last_mut() {
            let child_index = parent.child_cursor;
            parent.child_cursor += 1;
            let mut path = parent.path.clone();
            if let Some(key) = component_key {
                path.push(KEYED_PATH_MARKER);
                path.push(key as usize);
            } else {
                path.push(child_index);
            }
            path
        } else {
            STORE.with(|store| {
                let mut store = store.borrow_mut();
                let root_index = store.root_cursor;
                store.root_cursor += 1;
                if let Some(key) = component_key {
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

pub fn use_state<T: Clone + 'static>(init: impl FnOnce() -> T) -> State<T> {
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
        State { cell }
    })
}

fn global_cell_with_init<T: Clone + 'static>(init: impl FnOnce() -> T) -> Rc<RefCell<T>> {
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

fn global_cell<T: Clone + 'static>() -> Option<Rc<RefCell<T>>> {
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

pub fn global_state<T: Clone + 'static>(init: impl FnOnce() -> T) -> GlobalState<T> {
    GlobalState {
        cell: global_cell_with_init(init),
    }
}

#[allow(non_snake_case)]
pub fn globalState<T: Clone + 'static>(init: impl FnOnce() -> T) -> GlobalState<T> {
    global_state(init)
}

pub fn use_global_state<T: Clone + 'static>() -> GlobalState<T> {
    let cell = global_cell::<T>().unwrap_or_else(|| {
        panic!(
            "use_global_state::<{}>() called before global_state/globalState initialization",
            std::any::type_name::<T>()
        )
    });
    GlobalState { cell }
}

#[cfg(test)]
mod tests {
    use super::{build_scope, take_state_dirty, use_state, with_component_key};
    use crate::ui::RsxNode;

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

        let _ = build_scope(|| RsxNode::element("Element"));

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
            let a = with_component_key(Some(1), || {
                crate::ui::render_component::<u32, _>(|| {
                    let state = use_state(|| 10_i32);
                    state.get()
                })
            });
            let b = with_component_key(Some(2), || {
                crate::ui::render_component::<u32, _>(|| {
                    let state = use_state(|| 20_i32);
                    state.get()
                })
            });
            (a, b)
        });
        assert_eq!(first, (10, 20));

        let second = build_scope(|| {
            let b = with_component_key(Some(2), || {
                crate::ui::render_component::<u32, _>(|| {
                    let state = use_state(|| 999_i32);
                    state.get()
                })
            });
            let a = with_component_key(Some(1), || {
                crate::ui::render_component::<u32, _>(|| {
                    let state = use_state(|| 999_i32);
                    state.get()
                })
            });
            (b, a)
        });
        assert_eq!(second, (20, 10));
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

pub fn take_state_dirty() -> bool {
    STATE_DIRTY.with(|dirty| {
        let was_dirty = dirty.get();
        dirty.set(false);
        was_dirty
    })
}

fn notify_state_changed() {
    STATE_DIRTY.with(|dirty| dirty.set(true));
    REDRAW_CALLBACK.with(|slot| {
        if let Some(callback) = slot.borrow().as_ref() {
            callback();
        }
    });
}

impl<T: Clone + 'static> IntoPropValue for Binding<T> {
    fn into_prop_value(self) -> PropValue {
        let erased: Rc<dyn Any> = self.cell.clone();
        PropValue::Shared(SharedPropValue::new(erased))
    }
}

impl<T: Clone + 'static> FromPropValue for Binding<T> {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::Shared(shared) => {
                let erased = shared.value();
                let cell = Rc::downcast::<RefCell<T>>(erased)
                    .map_err(|_| "expected Binding value with matching type".to_string())?;
                Ok(Self { cell })
            }
            _ => Err("expected Binding value".to_string()),
        }
    }
}
