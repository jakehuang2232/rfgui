use super::*;

use crate::style::{
    ClipMode, Layout, Length, Opacity, ParsedValue, Position, PropertyId, ScrollDirection, Style,
    Transform, TransformEntry, Translate,
};

use crate::view::base_component::text_area::{
    TextAreaLineBreak, TextAreaProjectionSegment, TextAreaTextRun,
};

use crate::view::base_component::{
    DirtyFlags, DirtyPassMask, Element, ElementTrait, Image, ScrollbarPaintStateWitness, Svg, Text,
    TextArea,
};

use crate::view::node_arena::Node;

use crate::view::{ImageSource, SvgSource};

use std::sync::Arc;

use crate::view::test_support::{commit_element, measure_and_place, new_test_arena};

struct NeutralCustomHost;

impl crate::view::base_component::Layoutable for NeutralCustomHost {
    fn measure(
        &mut self,
        _constraints: crate::view::base_component::LayoutConstraints,
        _arena: &mut NodeArena,
    ) {
    }

    fn place(
        &mut self,
        _placement: crate::view::base_component::LayoutPlacement,
        _arena: &mut NodeArena,
    ) {
    }

    fn measured_size(&self) -> (f32, f32) {
        (0.0, 0.0)
    }

    fn set_layout_width(&mut self, _width: f32) {}

    fn set_layout_height(&mut self, _height: f32) {}
}

impl crate::view::base_component::EventTarget for NeutralCustomHost {}

impl crate::view::base_component::Renderable for NeutralCustomHost {
    fn build(
        &mut self,
        _graph: &mut crate::view::frame_graph::FrameGraph,
        _arena: &mut NodeArena,
        ctx: crate::view::base_component::UiBuildContext,
    ) -> crate::view::base_component::BuildState {
        ctx.into_state()
    }
}

impl crate::view::base_component::ElementTrait for NeutralCustomHost {
    fn stable_id(&self) -> u64 {
        99
    }

    fn box_model_snapshot(&self) -> crate::view::base_component::BoxModelSnapshot {
        crate::view::base_component::BoxModelSnapshot {
            node_id: 99,
            parent_id: None,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            border_radius: 0.0,
            should_render: false,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

struct ContentsClipHost {
    id: u64,
    scissor: Option<[u32; 4]>,
    children: Vec<NodeKey>,
    declares_scroll: bool,
}

impl crate::view::base_component::Layoutable for ContentsClipHost {
    fn measure(
        &mut self,
        _constraints: crate::view::base_component::LayoutConstraints,
        _arena: &mut NodeArena,
    ) {
    }

    fn place(
        &mut self,
        _placement: crate::view::base_component::LayoutPlacement,
        _arena: &mut NodeArena,
    ) {
    }

    fn measured_size(&self) -> (f32, f32) {
        (1.0, 1.0)
    }

    fn set_layout_width(&mut self, _width: f32) {}
    fn set_layout_height(&mut self, _height: f32) {}
}

impl crate::view::base_component::EventTarget for ContentsClipHost {}

impl crate::view::base_component::Renderable for ContentsClipHost {
    fn build(
        &mut self,
        _graph: &mut crate::view::frame_graph::FrameGraph,
        _arena: &mut NodeArena,
        ctx: crate::view::base_component::UiBuildContext,
    ) -> crate::view::base_component::BuildState {
        ctx.into_state()
    }
}

impl crate::view::base_component::ElementTrait for ContentsClipHost {
    fn stable_id(&self) -> u64 {
        self.id
    }

    fn box_model_snapshot(&self) -> crate::view::base_component::BoxModelSnapshot {
        crate::view::base_component::BoxModelSnapshot {
            node_id: self.id,
            parent_id: None,
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            border_radius: 0.0,
            should_render: true,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn contents_logical_scissor(&self) -> Option<[u32; 4]> {
        self.scissor
    }

    fn retained_paint_properties(&self) -> crate::view::base_component::RetainedPaintProperties {
        crate::view::base_component::RetainedPaintProperties {
            is_scroll_container: self.declares_scroll,
            ..Default::default()
        }
    }

    fn children(&self) -> &[NodeKey] {
        &self.children
    }

    fn sync_children_mirror(&mut self, children: &[NodeKey]) {
        self.children.clear();
        self.children.extend_from_slice(children);
    }
}

fn insert_contents_clip_host(arena: &mut NodeArena, id: u64, scissor: Option<[u32; 4]>) -> NodeKey {
    arena.insert(Node::new(Box::new(ContentsClipHost {
        id,
        scissor,
        children: Vec::new(),
        declares_scroll: false,
    })))
}

fn insert_missing_scroll_contract_host(arena: &mut NodeArena, id: u64) -> NodeKey {
    arena.insert(Node::new(Box::new(ContentsClipHost {
        id,
        scissor: Some([1, 2, 30, 40]),
        children: Vec::new(),
        declares_scroll: true,
    })))
}

fn insert_element(arena: &mut NodeArena, id: u64) -> NodeKey {
    arena.insert(Node::new(Box::new(Element::new_with_id(
        id, 0.0, 0.0, 100.0, 100.0,
    ))))
}

fn append_child(arena: &mut NodeArena, parent: NodeKey, child: NodeKey) {
    arena.set_parent(child, Some(parent));
    arena.push_child(parent, child);
}

fn set_opacity(arena: &NodeArena, key: NodeKey, opacity: f32) {
    arena
        .get_mut(key)
        .expect("element exists")
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("Element")
        .set_opacity(opacity);
}

fn set_scroll_direction(arena: &NodeArena, key: NodeKey, direction: ScrollDirection) {
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(direction),
    );
    arena
        .get_mut(key)
        .expect("element exists")
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("Element")
        .apply_style(style);
}

fn install_scroll_layout_geometry(
    arena: &NodeArena,
    key: NodeKey,
    viewport: Rect,
    content_size: [f32; 2],
) {
    let mut node = arena.get_mut(key).expect("scroll element exists");
    let element = node
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("Element");
    element.layout_state.layout_position.x = viewport.x;
    element.layout_state.layout_position.y = viewport.y;
    element.layout_state.layout_size.width = viewport.width;
    element.layout_state.layout_size.height = viewport.height;
    element.layout_state.layout_inner_position.x = viewport.x;
    element.layout_state.layout_inner_position.y = viewport.y;
    element.layout_state.layout_inner_size.width = viewport.width;
    element.layout_state.layout_inner_size.height = viewport.height;
    element.layout_state.content_size = Size {
        width: content_size[0],
        height: content_size[1],
    };
}

fn make_vertical_scroll_fixture(
    arena: &mut NodeArena,
    root_id: u64,
    child_id: u64,
) -> (NodeKey, NodeKey) {
    let root = insert_element(arena, root_id);
    let child = insert_element(arena, child_id);
    append_child(arena, root, child);
    set_scroll_direction(arena, root, ScrollDirection::Vertical);
    install_scroll_layout_geometry(
        arena,
        root,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 80.0,
        },
        [100.0, 300.0],
    );
    clear_layout_dirty_for_subtree(arena, root);
    (root, child)
}

fn make_nested_vertical_scroll_fixture(
    arena: &mut NodeArena,
    outer_id: u64,
    inner_id: u64,
    leaf_id: u64,
) -> (NodeKey, NodeKey, NodeKey) {
    let outer = insert_element(arena, outer_id);
    let inner = insert_element(arena, inner_id);
    let leaf = insert_element(arena, leaf_id);
    append_child(arena, outer, inner);
    append_child(arena, inner, leaf);
    set_scroll_direction(arena, outer, ScrollDirection::Vertical);
    set_scroll_direction(arena, inner, ScrollDirection::Vertical);
    for owner in [outer, inner] {
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        arena
            .get_mut(owner)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .apply_style(style);
    }
    install_scroll_layout_geometry(
        arena,
        outer,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 80.0,
        },
        [100.0, 300.0],
    );
    install_scroll_layout_geometry(
        arena,
        inner,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 300.0,
        },
        [100.0, 600.0],
    );
    install_scroll_layout_geometry(
        arena,
        leaf,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 600.0,
        },
        [100.0, 600.0],
    );
    clear_layout_dirty_for_subtree(arena, outer);
    (outer, inner, leaf)
}

fn clear_layout_dirty_for_subtree(arena: &NodeArena, root: NodeKey) {
    fn walk(arena: &NodeArena, key: NodeKey, flags: DirtyFlags) {
        let children = arena
            .get(key)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();
        if let Some(mut node) = arena.get_mut(key) {
            node.element.clear_local_dirty_flags(flags);
        }
        for child in children {
            walk(arena, child, flags);
        }
    }
    let flags = DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT);
    walk(arena, root, flags);
    arena.refresh_subtree_dirty_cache(root);
}

fn set_transform(arena: &NodeArena, key: NodeKey, transform: Transform) {
    let mut style = Style::new();
    style.set_transform(transform);
    arena
        .get_mut(key)
        .expect("element exists")
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("Element")
        .apply_style(style);
}

fn translate_x(value: f32) -> Transform {
    Transform::new([Translate::x(Length::px(value))])
}

fn matrix_bits(matrix: Mat4) -> [u32; 16] {
    matrix.to_cols_array().map(f32::to_bits)
}

fn opacity_style(opacity: f32) -> Style {
    let mut style = Style::new();
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(Opacity::new(opacity)),
    );
    style
}

fn anchor_parent_clip_fixture(viewport_width: f32) -> (NodeArena, NodeKey) {
    let mut element = Element::new_with_id(701, 10.25, 12.75, 80.0, 40.0);
    element.set_background_color_value(crate::style::Color::rgb(220, 40, 30));
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(10.25))
                .top(Length::px(12.75))
                .clip(ClipMode::AnchorParent),
        ),
    );
    element.apply_style(style);
    let mut arena = new_test_arena();
    let key = commit_element(&mut arena, Box::new(element));
    measure_and_place(
        &mut arena,
        key,
        crate::view::base_component::LayoutConstraints {
            max_width: viewport_width,
            max_height: 240.0,
            viewport_width,
            viewport_height: 240.0,
            percent_base_width: Some(viewport_width),
            percent_base_height: Some(240.0),
        },
        crate::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: viewport_width,
            available_height: 240.0,
            viewport_width,
            viewport_height: 240.0,
            percent_base_width: Some(viewport_width),
            percent_base_height: Some(240.0),
        },
    );
    (arena, key)
}

fn nested_anchor_parent_fixture(anchor_first: bool) -> (NodeArena, NodeKey, NodeKey, NodeKey) {
    let mut arena = new_test_arena();
    let mut parent = Element::new_with_id(0x8d00, 0.0, 0.0, 320.0, 240.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    parent.apply_style(parent_style);
    let parent = commit_element(&mut arena, Box::new(parent));

    let child = |id, mode| {
        let mut child = Element::new_with_id(id, 0.0, 0.0, 40.0, 30.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(8.0))
                    .top(Length::px(9.0))
                    .clip(mode),
            ),
        );
        child.apply_style(style);
        child
    };
    let (normal, anchor) = if anchor_first {
        let anchor = arena.insert(Node::with_parent(
            Box::new(child(0x8d02, ClipMode::AnchorParent)),
            Some(parent),
        ));
        let normal = arena.insert(Node::with_parent(
            Box::new(child(0x8d01, ClipMode::Parent)),
            Some(parent),
        ));
        arena.set_children(parent, vec![anchor, normal]);
        (normal, anchor)
    } else {
        let normal = arena.insert(Node::with_parent(
            Box::new(child(0x8d01, ClipMode::Parent)),
            Some(parent),
        ));
        let anchor = arena.insert(Node::with_parent(
            Box::new(child(0x8d02, ClipMode::AnchorParent)),
            Some(parent),
        ));
        arena.set_children(parent, vec![normal, anchor]);
        (normal, anchor)
    };
    let constraints = crate::view::base_component::LayoutConstraints {
        max_width: 320.0,
        max_height: 240.0,
        viewport_width: 320.0,
        viewport_height: 240.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(240.0),
    };
    let placement = crate::view::base_component::LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 320.0,
        available_height: 240.0,
        viewport_width: 320.0,
        viewport_height: 240.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(240.0),
    };
    measure_and_place(&mut arena, parent, constraints, placement);
    (arena, parent, normal, anchor)
}

fn set_clip_mode(arena: &NodeArena, key: NodeKey, mode: ClipMode) {
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(10.25))
                .top(Length::px(12.75))
                .clip(mode),
        ),
    );
    arena
        .get_mut(key)
        .expect("element exists")
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("Element")
        .apply_style(style);
}

mod clip_scope_tests;
mod property_identity_tests;
mod scroll_geometry_tests;
