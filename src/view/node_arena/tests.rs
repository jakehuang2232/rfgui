use super::*;
use crate::style::Color;
use crate::view::base_component::{
    BoxModelSnapshot, BuildState, DirtyFlags, Element, ElementTrait, EventTarget,
    LayoutConstraints, LayoutPlacement, Layoutable, Renderable, UiBuildContext,
};
use crate::view::frame_graph::FrameGraph;

struct TestElement {
    stable_id: u64,
    dirty_flags: DirtyFlags,
}

thread_local! {
    static RECORDED_BUILDS: std::cell::RefCell<Vec<&'static str>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

struct RecordingElement {
    stable_id: u64,
    label: &'static str,
    deferred: bool,
}

impl TestElement {
    fn new(stable_id: u64, dirty_flags: DirtyFlags) -> Self {
        Self {
            stable_id,
            dirty_flags,
        }
    }
}

impl RecordingElement {
    fn new(stable_id: u64, label: &'static str) -> Self {
        Self {
            stable_id,
            label,
            deferred: false,
        }
    }

    fn deferred(mut self) -> Self {
        self.deferred = true;
        self
    }
}

impl Layoutable for TestElement {
    fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
    fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
    fn measured_size(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn set_layout_width(&mut self, _width: f32) {}
    fn set_layout_height(&mut self, _height: f32) {}
}

impl EventTarget for TestElement {}

impl Renderable for TestElement {
    fn build(
        &mut self,
        _graph: &mut FrameGraph,
        _arena: &mut NodeArena,
        ctx: UiBuildContext,
    ) -> BuildState {
        ctx.into_state()
    }
}

impl ElementTrait for TestElement {
    fn stable_id(&self) -> u64 {
        self.stable_id
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.stable_id,
            parent_id: None,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            border_radius: 0.0,
            should_render: false,
        }
    }

    fn local_dirty_flags(&self) -> DirtyFlags {
        self.dirty_flags
    }

    fn clear_local_dirty_flags(&mut self, flags: DirtyFlags) {
        self.dirty_flags = self.dirty_flags.without(flags);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl Layoutable for RecordingElement {
    fn sync_arena(&mut self, _arena: &mut NodeArena) {
        RECORDED_BUILDS.with(|builds| builds.borrow_mut().push(self.label));
    }
    fn requires_arena_sync(&self) -> bool {
        true
    }
    fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
    fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
    fn measured_size(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn set_layout_width(&mut self, _width: f32) {}
    fn set_layout_height(&mut self, _height: f32) {}
}

impl EventTarget for RecordingElement {}

impl Renderable for RecordingElement {
    fn build(
        &mut self,
        _graph: &mut FrameGraph,
        _arena: &mut NodeArena,
        ctx: UiBuildContext,
    ) -> BuildState {
        RECORDED_BUILDS.with(|builds| builds.borrow_mut().push(self.label));
        ctx.into_state()
    }
}

impl ElementTrait for RecordingElement {
    fn stable_id(&self) -> u64 {
        self.stable_id
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.stable_id,
            parent_id: None,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            border_radius: 0.0,
            should_render: false,
        }
    }

    fn local_dirty_flags(&self) -> DirtyFlags {
        DirtyFlags::NONE
    }

    fn clear_local_dirty_flags(&mut self, _flags: DirtyFlags) {}

    fn is_deferred_to_root_viewport_render(&self) -> bool {
        self.deferred
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn insert_test_node(arena: &mut NodeArena, stable_id: u64, dirty_flags: DirtyFlags) -> NodeKey {
    arena.insert(Node::new(Box::new(TestElement::new(
        stable_id,
        dirty_flags,
    ))))
}

fn clean_element() -> Element {
    let mut element = Element::new(0.0, 0.0, 10.0, 10.0);
    element.clear_local_dirty_flags(DirtyFlags::ALL);
    element
}

fn link_child(arena: &mut NodeArena, parent: NodeKey, child: NodeKey) {
    arena.set_parent(child, Some(parent));
    arena.push_child(parent, child);
}

fn assert_element_and_arena_paint_dirty(arena: &NodeArena, root: NodeKey, child: NodeKey) {
    assert!(
        arena
            .get(child)
            .expect("child exists")
            .element
            .local_dirty_flags()
            .contains(DirtyFlags::PAINT)
    );
    assert!(arena.arena_local_dirty(child).contains(DirtyFlags::PAINT));
    assert!(
        arena
            .cached_subtree_dirty(child)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );
}

fn assert_cached_paint_clean(arena: &NodeArena, key: NodeKey) {
    assert!(
        !arena
            .cached_subtree_dirty(key)
            .intersects(DirtyFlags::PAINT)
    );
}

mod deferred_queue_tests;
mod dirty_propagation_tests;
mod invalidation_tests;
mod topology_tests;
