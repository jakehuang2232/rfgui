use super::*;
use crate::style::{Color, ColorLike};
use crate::view::base_component::DirtyFlags;
use crate::view::test_support::{commit_element, get_element_mut};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone)]
struct LiveColor(Rc<Cell<[f32; 4]>>);
impl ColorLike for LiveColor {
    fn box_clone(&self) -> Box<dyn ColorLike> {
        Box::new(self.clone())
    }
    fn to_rgba_f32(&self) -> [f32; 4] {
        self.0.get()
    }
}

fn read(cache: &mut NativeSignatures, arena: &NodeArena, key: NodeKey) -> u64 {
    let node = arena.get(key).unwrap();
    let result = cache.observe(arena, key, node.element.as_ref());
    assert_eq!(result, node.element.retained_paint_signature());
    result
}

#[test]
fn signature_memo_keeps_live_color_direct_mutation_and_arena_identity_contracts() {
    let mut arena = NodeArena::new();
    let key = commit_element(&mut arena, Box::new(Element::new(0., 0., 20., 16.)));
    let mut cache = NativeSignatures::default();
    let first = read(&mut cache, &arena, key);
    assert_eq!(read(&mut cache, &arena, key), first);
    assert_eq!(cache.entries.len(), 1);
    get_element_mut::<Element>(&arena, key).set_background_color(Color::rgb(0, 255, 0));
    arena.clear_element_dirty_flags(key, DirtyFlags::ALL);
    assert_ne!(read(&mut cache, &arena, key), first);
    let color = LiveColor(Rc::new(Cell::new([1., 0., 0., 1.])));
    assert!(!color.is_immutable());
    get_element_mut::<Element>(&arena, key).set_background_color(color.clone());
    let red = read(&mut cache, &arena, key);
    color.0.set([0., 0., 1., 1.]);
    assert_ne!(read(&mut cache, &arena, key), red);
    assert!(cache.entries.is_empty());
    let mut other = NodeArena::new();
    let other_key = commit_element(&mut other, Box::new(Element::new(0., 0., 30., 16.)));
    assert_eq!(key, other_key);
    read(&mut cache, &other, other_key);
    assert_eq!(cache.entries.len(), 1);
    other.remove(other_key);
    cache.prune(&other);
    assert!(cache.entries.is_empty());
}
