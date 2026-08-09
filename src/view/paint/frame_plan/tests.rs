use super::*;
use std::any::Any;
use std::sync::Arc;

use slotmap::Key;

use crate::style::{
    AnchorName, Angle, BoxShadow, Color, Layout, Length, Opacity, ParsedValue, Position, PropertyId,
    Rotate, Scale, ScrollDirection, Style, Transform, Transition, TransitionProperty, Transitions,
};
use crate::view::base_component::{
    BoxModelSnapshot, BuildState, DirtyPassMask, ElementTrait, EventTarget, Image,
    LayoutConstraints, LayoutPlacement, Layoutable, Rect, Renderable, Size, Svg, Text,
    UiBuildContext,
};
use crate::view::frame_graph::{FrameGraph, FramePassTestPayload};
use crate::view::node_arena::Node;
use crate::view::paint::tests::exact_isolation_fixture;
use crate::view::paint::{
    PaintBakedScrollHostWitness, PaintChunk, PaintChunkId, PaintChunkRole, PaintContentRevision,
    PaintNodePhase, PaintPayloadIdentity, PaintPropertyScope, PaintScrollContentWitness,
    PlannedBoundary, PlannedBoundaryKind, RETAINED_CHILD_MASK_SLOT, RetainedSurfaceCompileAction,
};
use crate::view::test_support::{commit_child, commit_element, measure_and_place, new_test_arena};
use crate::view::viewport::Viewport;
use crate::view::{ImageSource, SvgSource};

struct UnknownHost {
    id: u64,
    width: f32,
    height: f32,
}

impl Layoutable for UnknownHost {
    fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
    fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
    fn measured_size(&self) -> (f32, f32) {
        (self.width, self.height)
    }
    fn set_layout_width(&mut self, width: f32) {
        self.width = width;
    }
    fn set_layout_height(&mut self, height: f32) {
        self.height = height;
    }
}

impl EventTarget for UnknownHost {}

impl Renderable for UnknownHost {
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        _arena: &mut NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        let mut pass = crate::view::render_pass::draw_rect_pass::DrawRectPass::new(
            crate::view::render_pass::draw_rect_pass::RectPassParams {
                position: [0.0, 0.0],
                size: [self.width, self.height],
                fill_color: [0.2, 0.4, 0.6, 0.5],
                opacity: 1.0,
                ..Default::default()
            },
            Default::default(),
            Default::default(),
        );
        pass.set_render_mode(crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly);
        ctx.emit_draw_rect_pass(graph, pass);
        ctx.into_state()
    }
}

impl ElementTrait for UnknownHost {
    fn stable_id(&self) -> u64 {
        self.id
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.id,
            parent_id: None,
            x: 0.0,
            y: 0.0,
            width: self.width,
            height: self.height,
            border_radius: 0.0,
            should_render: true,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn exact_transform_fixture_at_origin_with_ids(
    root_id: u64,
    child_id: u64,
    root_x: f32,
    root_y: f32,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let mut root = Element::new_with_id(root_id, root_x, root_y, 40.0, 24.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(20, 40, 80)),
    );
    root_style.set_transform(Transform::new([Rotate::z(Angle::deg(12.0))]));
    root.apply_style(root_style);

    let mut child = Element::new_with_id(child_id, 0.0, 0.0, 18.0, 10.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(180, 60, 20)),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root));
    commit_child(&mut arena, root, Box::new(child));
    measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 160.0,
            max_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
        LayoutPlacement {
            parent_x: root_x,
            parent_y: root_y,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 160.0,
            available_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
    );
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, properties, generations)
}

fn exact_transform_fixture() -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    exact_transform_fixture_at_origin_with_ids(0xc1_0001, 0xc1_0002, 4.25, 3.5)
}

fn nested_exact_transform_fixture() -> (
    NodeArena,
    NodeKey,
    NodeKey,
    NodeKey,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let styled_element = |id, x, y, width, height, color| {
        let mut element = Element::new_with_id(id, x, y, width, height);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
        element.apply_style(style);
        element
    };

    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(styled_element(
            0xc5_a100,
            0.25,
            0.25,
            40.0,
            24.0,
            Color::rgb(20, 40, 80),
        )),
    );
    let before = commit_child(
        &mut arena,
        root,
        Box::new(styled_element(
            0xc5_a101,
            1.25,
            1.25,
            2.0,
            2.0,
            Color::rgb(40, 120, 80),
        )),
    );
    let child = commit_child(
        &mut arena,
        root,
        Box::new(styled_element(
            0xc5_a102,
            4.25,
            1.5,
            18.0,
            10.0,
            Color::rgb(180, 60, 20),
        )),
    );
    let descendant = commit_child(
        &mut arena,
        child,
        Box::new(styled_element(
            0xc5_a103,
            5.0,
            1.75,
            1.0,
            1.0,
            Color::rgb(200, 160, 20),
        )),
    );
    let after = commit_child(
        &mut arena,
        root,
        Box::new(styled_element(
            0xc5_a104,
            2.25,
            5.25,
            2.0,
            2.0,
            Color::rgb(100, 80, 180),
        )),
    );

    measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 160.0,
            max_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
        LayoutPlacement {
            parent_x: 0.25,
            parent_y: 0.25,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 160.0,
            available_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
    );

    let parent_matrix = glam::Mat4::from_translation(glam::Vec3::new(100.0, 0.0, 0.0));
    let child_matrix = glam::Mat4::from_cols_array(&[
        0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 30.0, 0.0, 0.0, 1.0,
    ]);
    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_resolved_transform_for_test(Some(parent_matrix));
    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_resolved_transform_for_test(Some(child_matrix));

    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (
        arena,
        root,
        before,
        child,
        descendant,
        after,
        properties,
        generations,
    )
}

struct GeneralPropertySceneFixture {
    arena: NodeArena,
    roots: Vec<NodeKey>,
    outer: NodeKey,
    inner_a: NodeKey,
    deep: NodeKey,
    inner_b: NodeKey,
    second_root: NodeKey,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

#[derive(Clone, Copy)]
enum ScrollInterleaveFixtureShape {
    FrameRootScroll,
    TransformScroll,
    EffectScroll,
    TransformEffectScroll,
    EffectTransformScroll,
    EffectNeutralTransformNeutralScroll,
    ScrollTransform,
    CoLocatedTransformScroll,
    /// A scroll host whose ancestor path already contains a scroll host.
    ///
    /// This is the shape every retained authority rejects in the example
    /// scene. See `docs/design/nested-scroll-property-interleave.md`.
    NestedScroll,
}

fn property_scroll_interleave_fixture(
    shape: ScrollInterleaveFixtureShape,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let mut arena = NodeArena::new();
    let wrapper = |id| Element::new_with_id(id, 0.0, 0.0, 120.0, 90.0);
    let root_element = if matches!(
        shape,
        ScrollInterleaveFixtureShape::TransformEffectScroll
            | ScrollInterleaveFixtureShape::EffectTransformScroll
            | ScrollInterleaveFixtureShape::EffectNeutralTransformNeutralScroll
    ) {
        Element::new_with_id(0xb4_0001, 0.0, 0.0, 168.0, 112.0)
    } else {
        wrapper(0xb4_0001)
    };
    let root = arena.insert(Node::new(Box::new(root_element)));
    let (scroll, content) = match shape {
        ScrollInterleaveFixtureShape::FrameRootScroll
        | ScrollInterleaveFixtureShape::CoLocatedTransformScroll => {
            let content = arena.insert(Node::new(Box::new(Element::new_with_id(
                0xb4_0010, 0.0, -20.0, 120.0, 240.0,
            ))));
            arena.set_parent(content, Some(root));
            arena.push_child(root, content);
            (root, content)
        }
        ScrollInterleaveFixtureShape::TransformScroll
        | ScrollInterleaveFixtureShape::EffectScroll => {
            let scroll = arena.insert(Node::new(Box::new(wrapper(0xb4_0002))));
            let content = arena.insert(Node::new(Box::new(Element::new_with_id(
                0xb4_0010, 0.0, -20.0, 120.0, 240.0,
            ))));
            arena.set_parent(scroll, Some(root));
            arena.push_child(root, scroll);
            arena.set_parent(content, Some(scroll));
            arena.push_child(scroll, content);
            (scroll, content)
        }
        ScrollInterleaveFixtureShape::TransformEffectScroll => {
            let effect = arena.insert(Node::new(Box::new(wrapper(0xb4_0002))));
            let scroll = arena.insert(Node::new(Box::new(wrapper(0xb4_0003))));
            let content = arena.insert(Node::new(Box::new(Element::new_with_id(
                0xb4_0010, 0.0, -20.0, 120.0, 240.0,
            ))));
            arena.set_parent(effect, Some(root));
            arena.push_child(root, effect);
            arena.set_parent(scroll, Some(effect));
            arena.push_child(effect, scroll);
            arena.set_parent(content, Some(scroll));
            arena.push_child(scroll, content);
            crate::view::test_support::get_element_mut::<Element>(&arena, effect).set_opacity(0.5);
            crate::view::test_support::get_element_mut::<Element>(&arena, effect)
                .set_background_color_value(Color::rgb(32, 64, 96));
            let mut effect_style = Style::new();
            effect_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            crate::view::test_support::get_element_mut::<Element>(&arena, effect)
                .apply_style(effect_style);
            crate::view::test_support::get_element_mut::<Element>(&arena, effect).set_opacity(0.5);
            crate::view::test_support::get_element_mut::<Element>(&arena, effect)
                .set_background_color_value(Color::rgb(32, 64, 96));
            (scroll, content)
        }
        ScrollInterleaveFixtureShape::EffectTransformScroll => {
            let transform = arena.insert(Node::new(Box::new(wrapper(0xb4_0002))));
            let scroll = arena.insert(Node::new(Box::new(wrapper(0xb4_0003))));
            let content = arena.insert(Node::new(Box::new(Element::new_with_id(
                0xb4_0010, 0.0, -20.0, 120.0, 240.0,
            ))));
            arena.set_parent(transform, Some(root));
            arena.push_child(root, transform);
            arena.set_parent(scroll, Some(transform));
            arena.push_child(transform, scroll);
            arena.set_parent(content, Some(scroll));
            arena.push_child(scroll, content);
            crate::view::test_support::get_element_mut::<Element>(&arena, transform)
                .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                    glam::Vec3::new(7.0, 3.0, 0.0),
                )));
            let mut transform_style = Style::new();
            transform_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            crate::view::test_support::get_element_mut::<Element>(&arena, transform)
                .apply_style(transform_style);
            crate::view::test_support::get_element_mut::<Element>(&arena, transform)
                .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                    glam::Vec3::new(7.0, 3.0, 0.0),
                )));
            (scroll, content)
        }
        ScrollInterleaveFixtureShape::EffectNeutralTransformNeutralScroll => {
            let outer_before = arena.insert(Node::new(Box::new(Element::new_with_id(
                0xb4_0020, 2.0, 2.0, 8.0, 8.0,
            ))));
            let outer_wrapper = arena.insert(Node::new(Box::new(wrapper(0xb4_0021))));
            let wrapper_before = arena.insert(Node::new(Box::new(Element::new_with_id(
                0xb4_0022, 4.0, 4.0, 8.0, 8.0,
            ))));
            let transform = arena.insert(Node::new(Box::new(wrapper(0xb4_0002))));
            let wrapper_after = arena.insert(Node::new(Box::new(Element::new_with_id(
                0xb4_0023, 14.0, 4.0, 8.0, 8.0,
            ))));
            let outer_after = arena.insert(Node::new(Box::new(Element::new_with_id(
                0xb4_0024, 24.0, 2.0, 8.0, 8.0,
            ))));
            for child in [outer_before, outer_wrapper, outer_after] {
                arena.set_parent(child, Some(root));
                arena.push_child(root, child);
            }
            for child in [wrapper_before, transform, wrapper_after] {
                arena.set_parent(child, Some(outer_wrapper));
                arena.push_child(outer_wrapper, child);
            }
            let inner_before = arena.insert(Node::new(Box::new(Element::new_with_id(
                0xb4_0025, 2.0, 16.0, 8.0, 8.0,
            ))));
            let inner_wrapper = arena.insert(Node::new(Box::new(wrapper(0xb4_0026))));
            let inner_after = arena.insert(Node::new(Box::new(Element::new_with_id(
                0xb4_0027, 24.0, 16.0, 8.0, 8.0,
            ))));
            for child in [inner_before, inner_wrapper, inner_after] {
                arena.set_parent(child, Some(transform));
                arena.push_child(transform, child);
            }
            let scroll = arena.insert(Node::new(Box::new(wrapper(0xb4_0003))));
            arena.set_parent(scroll, Some(inner_wrapper));
            arena.push_child(inner_wrapper, scroll);
            let content = arena.insert(Node::new(Box::new(Element::new_with_id(
                0xb4_0010, 0.0, -20.0, 120.0, 240.0,
            ))));
            arena.set_parent(content, Some(scroll));
            arena.push_child(scroll, content);
            for owner in [
                outer_before,
                outer_wrapper,
                wrapper_before,
                wrapper_after,
                outer_after,
                inner_before,
                inner_wrapper,
                inner_after,
            ] {
                let mut element =
                    crate::view::test_support::get_element_mut::<Element>(&arena, owner);
                let mut neutral_style = Style::new();
                neutral_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
                element.apply_style(neutral_style);
                element.set_background_color_value(Color::rgb(12, 24, 36));
            }
            crate::view::test_support::get_element_mut::<Element>(&arena, transform)
                .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                    glam::Vec3::new(7.0, 3.0, 0.0),
                )));
            let mut transform_style = Style::new();
            transform_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            crate::view::test_support::get_element_mut::<Element>(&arena, transform)
                .apply_style(transform_style);
            crate::view::test_support::get_element_mut::<Element>(&arena, transform)
                .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                    glam::Vec3::new(7.0, 3.0, 0.0),
                )));
            (scroll, content)
        }
        ScrollInterleaveFixtureShape::NestedScroll => {
            let outer = arena.insert(Node::new(Box::new(wrapper(0xb4_0002))));
            let inner = arena.insert(Node::new(Box::new(wrapper(0xb4_0003))));
            let content = arena.insert(Node::new(Box::new(Element::new_with_id(
                0xb4_0010, 0.0, -20.0, 120.0, 240.0,
            ))));
            arena.set_parent(outer, Some(root));
            arena.push_child(root, outer);
            arena.set_parent(inner, Some(outer));
            arena.push_child(outer, inner);
            arena.set_parent(content, Some(inner));
            arena.push_child(inner, content);
            // The shared tail below turns `inner` into a scroll host. Make the
            // outer wrapper one too, so `inner` plans with a scroll ancestor
            // already on its path.
            let mut outer_style = Style::new();
            outer_style.insert(
                PropertyId::ScrollDirection,
                ParsedValue::ScrollDirection(ScrollDirection::Vertical),
            );
            outer_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            {
                let mut element =
                    crate::view::test_support::get_element_mut::<Element>(&arena, outer);
                element.apply_style(outer_style);
                element.layout_state.content_size = Size {
                    width: 120.0,
                    height: 240.0,
                };
                element.set_scroll_offset((0.0, 10.0));
                element
                    .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
            }
            (inner, content)
        }
        ScrollInterleaveFixtureShape::ScrollTransform => {
            let transform = arena.insert(Node::new(Box::new(Element::new_with_id(
                0xb4_0002, 0.0, -20.0, 120.0, 240.0,
            ))));
            arena.set_parent(transform, Some(root));
            arena.push_child(root, transform);
            crate::view::test_support::get_element_mut::<Element>(&arena, transform)
                .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                    glam::Vec3::new(3.0, 0.0, 0.0),
                )));
            (root, transform)
        }
    };
    match shape {
        ScrollInterleaveFixtureShape::TransformScroll
        | ScrollInterleaveFixtureShape::TransformEffectScroll => {
            crate::view::test_support::get_element_mut::<Element>(&arena, root)
                .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                    glam::Vec3::new(7.0, 0.0, 0.0),
                )));
            if matches!(shape, ScrollInterleaveFixtureShape::TransformEffectScroll) {
                crate::view::test_support::get_element_mut::<Element>(&arena, root)
                    .set_background_color_value(Color::rgb(16, 32, 48));
                let mut root_style = Style::new();
                root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
                crate::view::test_support::get_element_mut::<Element>(&arena, root)
                    .apply_style(root_style);
                crate::view::test_support::get_element_mut::<Element>(&arena, root)
                    .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                        glam::Vec3::new(7.0, 0.0, 0.0),
                    )));
                crate::view::test_support::get_element_mut::<Element>(&arena, root)
                    .set_background_color_value(Color::rgb(16, 32, 48));
            }
        }
        ScrollInterleaveFixtureShape::EffectScroll => {
            crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.5);
        }
        ScrollInterleaveFixtureShape::EffectTransformScroll
        | ScrollInterleaveFixtureShape::EffectNeutralTransformNeutralScroll => {
            let mut root_style = Style::new();
            root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            crate::view::test_support::get_element_mut::<Element>(&arena, root)
                .apply_style(root_style);
            crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.5);
            crate::view::test_support::get_element_mut::<Element>(&arena, root)
                .set_background_color_value(Color::rgb(16, 32, 48));
        }
        ScrollInterleaveFixtureShape::CoLocatedTransformScroll => {
            crate::view::test_support::get_element_mut::<Element>(&arena, root)
                .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                    glam::Vec3::new(7.0, 0.0, 0.0),
                )));
        }
        ScrollInterleaveFixtureShape::FrameRootScroll
        | ScrollInterleaveFixtureShape::NestedScroll
        | ScrollInterleaveFixtureShape::ScrollTransform => {}
    }
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut scroll = crate::view::test_support::get_element_mut::<Element>(&arena, scroll);
        scroll.apply_style(style);
        scroll.layout_state.content_size = Size {
            width: 120.0,
            height: 240.0,
        };
        scroll.set_scroll_offset((0.0, 20.0));
        scroll.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    if matches!(
        shape,
        ScrollInterleaveFixtureShape::CoLocatedTransformScroll
    ) {
        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                7.0, 0.0, 0.0,
            ))));
    }
    crate::view::test_support::get_element_mut::<Element>(&arena, content)
        .set_background_color_value(Color::rgb(24, 48, 72));
    arena
        .get_mut(content)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(
        properties.validation_errors.is_empty(),
        "{:?}",
        properties.validation_errors
    );
    if matches!(shape, ScrollInterleaveFixtureShape::TransformEffectScroll) {
        let children = arena.children_of(root);
        let [effect] = children.as_slice() else {
            panic!("T->E->S fixture owns one direct effect child")
        };
        assert!(properties.transforms.contains_key(&TransformNodeId(root)));
        assert!(properties.effects.contains_key(&EffectNodeId(*effect)));
    }
    if matches!(shape, ScrollInterleaveFixtureShape::ScrollTransform) {
        assert!(
            properties
                .transforms
                .contains_key(&TransformNodeId(content))
        );
    }
    if matches!(
        shape,
        ScrollInterleaveFixtureShape::CoLocatedTransformScroll
    ) {
        assert!(properties.transforms.contains_key(&TransformNodeId(root)));
    }
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, properties, generations)
}

/// Independently authored C1 input artifact for topology-only fixtures.
///
/// One structural zero-op chunk is emitted for every reachable owner in arena
/// DFS order, including transparent property owners that the paint recorder
/// intentionally omits. Property states and every snapshot payload still come
/// from the real synced `PropertyTrees`; no planner DAG fact is an input.
fn stage_c_classification_artifact_fixture(
    arena: &NodeArena,
    roots: &[NodeKey],
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    let mut artifact = PaintArtifact::default();
    let mut seen = FxHashSet::default();
    let mut stack = roots
        .iter()
        .rev()
        .copied()
        .map(|root| (root, None))
        .collect::<Vec<_>>();
    while let Some((owner, parent)) = stack.pop() {
        if !seen.insert(owner) {
            return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(owner)]);
        }
        let node = arena
            .get(owner)
            .ok_or_else(|| vec![FrameArtifactFallbackReason::PropertyBoundary(owner)])?;
        let state = properties
            .node_state_for(owner)
            .ok_or_else(|| vec![FrameArtifactFallbackReason::PropertyBoundary(owner)])?;
        let generation = generations
            .local_generations_for(owner)
            .ok_or_else(|| vec![FrameArtifactFallbackReason::PropertyBoundary(owner)])?;
        artifact.owner_nodes.push(PaintOwnerSnapshot { owner, parent });
        artifact.chunks.push(PaintChunk {
            id: PaintChunkId {
                owner,
                scope: PaintPropertyScope::SelfPaint,
                phase: PaintNodePhase::BeforeChildren,
                slot: 0,
                role: PaintChunkRole::SelfDecoration,
            },
            owner,
            op_range: 0..0,
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
            properties: state.paint,
            content_revision: PaintContentRevision {
                self_paint_revision: generation.self_paint_revision,
                composite_revision: generation.composite_revision,
                topology_revision: generation.topology_revision,
            },
            payload_identity: PaintPayloadIdentity::None,
        });
        stack.extend(
            node.element
                .children()
                .iter()
                .rev()
                .copied()
                .map(|child| (child, Some(owner))),
        );
    }

    let mut clips = FxHashMap::default();
    let mut effects = FxHashMap::default();
    for chunk in &artifact.chunks {
        for snapshot in properties
            .clip_snapshot_for(chunk.properties.clip)
            .ok_or_else(|| vec![FrameArtifactFallbackReason::PropertyBoundary(chunk.owner)])?
        {
            if let Some(existing) = clips.get(&snapshot.id) {
                if *existing != snapshot {
                    return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
                        chunk.owner,
                    )]);
                }
            } else {
                clips.insert(snapshot.id, snapshot);
                artifact.clip_nodes.push(snapshot);
            }
        }
        for snapshot in properties
            .effect_snapshot_for(chunk.properties.effect)
            .ok_or_else(|| vec![FrameArtifactFallbackReason::PropertyBoundary(chunk.owner)])?
        {
            if let Some(existing) = effects.get(&snapshot.id) {
                if *existing != snapshot {
                    return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
                        chunk.owner,
                    )]);
                }
            } else {
                effects.insert(snapshot.id, snapshot);
                artifact.effect_nodes.push(snapshot);
            }
        }
    }
    super::super::frame_recorder::populate_referenced_property_snapshots_for_test(
        &mut artifact,
        properties,
    )?;
    Ok(artifact)
}

fn stage_c_owner_is_within(arena: &NodeArena, owner: NodeKey, target: NodeKey) -> bool {
    let mut cursor = Some(owner);
    while let Some(owner) = cursor {
        if owner == target {
            return true;
        }
        cursor = arena.parent_of(owner);
    }
    false
}

fn assert_stage_c_classified_cursors_match_artifact_traversal(
    arena: &NodeArena,
    artifact: &PaintArtifact,
    events: &[crate::view::paint::ClassifiedTransitionEvent],
) {
    for event in events {
        let expected_chunk = artifact
            .chunks
            .iter()
            .position(|chunk| stage_c_owner_is_within(arena, chunk.owner, event.target()))
            .expect("every classified target owns an artifact chunk subtree");
        assert_eq!(event.cursor().chunk_index(), expected_chunk);
        assert_eq!(
            event.cursor().op_index(),
            artifact.chunks[expected_chunk].op_range.start,
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum StageCScrollCapabilityCase {
    DepthFour,
    NestedScroll,
    BranchingSiblings,
    HeterogeneousRoots,
    CoLocatedProperties,
    ClipCrossingScroll,
    InterleavedSiblingOrder,
    NamedAnchor,
    LayoutPositionReferenceScroll,
    ScrollOffsetDelta,
    OverlayPhase,
}

const STAGE_C_SCROLL_CAPABILITY_CASES: [StageCScrollCapabilityCase; 11] = [
    StageCScrollCapabilityCase::DepthFour,
    StageCScrollCapabilityCase::NestedScroll,
    StageCScrollCapabilityCase::BranchingSiblings,
    StageCScrollCapabilityCase::HeterogeneousRoots,
    StageCScrollCapabilityCase::CoLocatedProperties,
    StageCScrollCapabilityCase::ClipCrossingScroll,
    StageCScrollCapabilityCase::InterleavedSiblingOrder,
    StageCScrollCapabilityCase::NamedAnchor,
    StageCScrollCapabilityCase::LayoutPositionReferenceScroll,
    StageCScrollCapabilityCase::ScrollOffsetDelta,
    StageCScrollCapabilityCase::OverlayPhase,
];

impl StageCScrollCapabilityCase {
    const fn label(self) -> &'static str {
        match self {
            Self::DepthFour => "depth-four",
            Self::NestedScroll => "nested-scroll",
            Self::BranchingSiblings => "branching-siblings",
            Self::HeterogeneousRoots => "heterogeneous-roots",
            Self::CoLocatedProperties => "co-located-properties",
            Self::ClipCrossingScroll => "clip-crossing-scroll",
            Self::InterleavedSiblingOrder => "interleaved-sibling-order",
            Self::NamedAnchor => "named-anchor",
            Self::LayoutPositionReferenceScroll => "layout-position-reference-scroll",
            Self::ScrollOffsetDelta => "scroll-offset-delta",
            Self::OverlayPhase => "overlay-phase",
        }
    }
}

fn stage_c_depth_four_scroll_fixture() -> (
    NodeArena,
    Vec<NodeKey>,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (mut arena, roots, _, _) = native_scroll_forest_plan_fixture();
    let wrapper = arena
        .find_by_stable_id(0x12f0_06)
        .expect("C0c depth-four wrapper");
    let fourth = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_15, 10.0, 20.0, 600.0, 600.0,
    ))));
    arena.set_parent(fourth, Some(wrapper));
    arena.push_child(wrapper, fourth);
    let mut scroll_style = Style::new();
    scroll_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Both),
    );
    scroll_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, fourth);
        element.apply_style(scroll_style);
        element.layout_state.layout_position.x = 10.0;
        element.layout_state.layout_position.y = 20.0;
        element.layout_state.layout_size = Size {
            width: 600.0,
            height: 600.0,
        };
        element.layout_state.layout_inner_position = element.layout_state.layout_position;
        element.layout_state.layout_inner_size = element.layout_state.layout_size;
        element.layout_state.content_size = Size {
            width: 900.0,
            height: 900.0,
        };
        element.set_scroll_offset((15.0, 25.0));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    let leaf = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_16, -5.0, -5.0, 900.0, 900.0,
    ))));
    arena.set_parent(leaf, Some(fourth));
    arena.push_child(fourth, leaf);
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, leaf);
        element.layout_state.layout_position.x = -5.0;
        element.layout_state.layout_position.y = -5.0;
        element.layout_state.layout_size = Size {
            width: 900.0,
            height: 900.0,
        };
        element.layout_state.layout_inner_position = element.layout_state.layout_position;
        element.layout_state.layout_inner_size = element.layout_state.layout_size;
        element.layout_state.content_size = element.layout_state.layout_size;
        element.set_background_color_value(Color::rgb(18, 36, 54));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    for root in &roots {
        arena.refresh_subtree_dirty_cache(*root);
    }
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    (arena, roots, wrapper, fourth, properties, generations)
}

fn stage_c_named_anchor_classification_fixture() -> (
    NodeArena,
    NodeKey,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut root_element = Element::new_with_id(0xc1c0_0100, 0.0, 0.0, 400.0, 300.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_element.apply_style(root_style);

    let mut anchor_element = Element::new_with_id(0xc1c0_0101, 0.0, 0.0, 40.0, 20.0);
    let mut anchor_style = Style::new();
    anchor_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(120.0))
                .top(Length::px(30.0)),
        ),
    );
    anchor_element.apply_style(anchor_style);
    anchor_element.set_anchor_name(Some(AnchorName::new("stage-c-classifier-anchor")));

    let mut child_element = Element::new_with_id(0xc1c0_0102, 0.0, 0.0, 30.0, 20.0);
    let mut child_style = Style::new();
    child_style.set_transform(Transform::new([Scale::uniform(1.1)]));
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .anchor("stage-c-classifier-anchor")
                .left(Length::px(7.0))
                .top(Length::px(9.0)),
        ),
    );
    child_element.apply_style(child_style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root_element));
    let anchor = commit_child(&mut arena, root, Box::new(anchor_element));
    let child = commit_child(&mut arena, root, Box::new(child_element));
    let constraints = LayoutConstraints {
        max_width: 400.0,
        max_height: 300.0,
        viewport_width: 400.0,
        viewport_height: 300.0,
        percent_base_width: Some(400.0),
        percent_base_height: Some(300.0),
    };
    let placement = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 400.0,
        available_height: 300.0,
        viewport_width: 400.0,
        viewport_height: 300.0,
        percent_base_width: Some(400.0),
        percent_base_height: Some(300.0),
    };
    measure_and_place(&mut arena, root, constraints, placement);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(
        properties.spatial_validation_errors.is_empty(),
        "C1c named-anchor fixture: {:?}",
        properties.spatial_validation_errors,
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, anchor, child, properties, generations)
}

fn stage_c_layout_position_reference_scroll_fixture() -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0xc0c0_0100, 0.0, 0.0, 100.0, 80.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
    root_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(80.0)));
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root.apply_style(root_style);
    let root = commit_element(&mut arena, Box::new(root));

    let mut child = Element::new_with_id(0xc0c0_0101, 0.0, 0.0, 100.0, 240.0);
    let mut child_style = Style::new();
    child_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
    child_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(240.0)));
    child_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    child.apply_style(child_style);
    let child = commit_child(&mut arena, root, Box::new(child));

    let constraints = LayoutConstraints {
        max_width: 100.0,
        max_height: 80.0,
        viewport_width: 100.0,
        viewport_height: 80.0,
        percent_base_width: Some(100.0),
        percent_base_height: Some(80.0),
    };
    let placement = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 100.0,
        available_height: 80.0,
        viewport_width: 100.0,
        viewport_height: 80.0,
        percent_base_width: Some(100.0),
        percent_base_height: Some(80.0),
    };
    measure_and_place(&mut arena, root, constraints, placement);
    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_scroll_offset((0.0, 20.0));
    measure_and_place(&mut arena, root, constraints, placement);
    assert!(
        crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
            &mut arena,
            root,
            DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT),
        ),
        "C0c reference-scroll fixture remains reachable",
    );
    let geometry_dirty = DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT);
    assert!(!arena.subtree_dirty_intersects(root, geometry_dirty));
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(
        properties.validation_errors.is_empty(),
        "C0c reference-scroll fixture: {:?}",
        properties.validation_errors,
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, child, properties, generations)
}

fn apply_stage_c_scroll_offset_delta(
    arena: &NodeArena,
    scroll: NodeKey,
    properties: &mut PropertyTrees,
    generations: &mut PaintGenerationTracker,
    delta_y: f32,
) -> glam::Vec2 {
    let baseline_offset = properties
        .scroll_snapshot_for(ScrollNodeId(scroll))
        .expect("C0c scroll snapshot")
        .offset;
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(arena, scroll);
        element.set_scroll_offset((baseline_offset.x, baseline_offset.y + delta_y));
        // Production `element/event_target.rs::set_scroll_offset` marks
        // placement dirty, and `element/impl_layout.rs::place_children`
        // derives each child origin as
        // `layout_flow_inner_position - scroll_offset`. This exact-admission
        // fixture mirrors only that production delta so both C0c and C1c
        // consume one shared input mutation.
        element.clear_local_dirty_flags(DirtyPassMask::PLACEMENT);
    }
    let content_root = arena.children_of(scroll)[0];
    let mut pending = vec![content_root];
    while let Some(owner) = pending.pop() {
        pending.extend(arena.children_of(owner));
        let mut element = crate::view::test_support::get_element_mut::<Element>(arena, owner);
        element.layout_state.layout_position.y -= delta_y;
        element.layout_state.layout_inner_position.y -= delta_y;
        element.clear_local_dirty_flags(DirtyPassMask::PLACEMENT);
    }
    arena.refresh_subtree_dirty_cache(scroll);
    properties.sync(arena, &[scroll]);
    generations.sync(arena, &[scroll], properties);
    baseline_offset
}

fn stage_c_visible_overlay_fixture() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (arena, roots, mut properties, mut generations) = native_scroll_forest_plan_fixture();
    let scroll = arena
        .find_by_stable_id(0x12f0_01)
        .expect("C0c visible overlay scroll host");
    crate::view::test_support::get_element_mut::<Element>(&arena, scroll)
        .set_sampled_scrollbar_alpha_for_test(1.0);
    for root in &roots {
        arena.refresh_subtree_dirty_cache(*root);
    }
    properties.sync(&arena, &roots);
    generations.sync(&arena, &roots, &properties);
    (arena, roots, properties, generations)
}

/// Two scroll hosts on one ancestor path, plus the inner host's key.
pub(crate) fn nested_scroll_fixture() -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::NestedScroll);
    let outer = arena.children_of(root)[0];
    let inner = arena.children_of(outer)[0];
    (arena, root, inner, properties, generations)
}

pub(crate) fn same_owner_transform_scroll_fixture()
-> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::CoLocatedTransformScroll)
}

pub(crate) fn same_owner_effect_scroll_fixture()
-> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let (arena, root, _, _) = same_owner_transform_scroll_fixture();
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        element.set_resolved_transform_for_test(None);
        element.set_opacity(0.625);
        element.set_background_color_value(Color::rgb(32, 64, 96));
    }
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, properties, generations)
}

pub(crate) fn same_owner_transform_effect_scroll_roles_fixture()
-> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let (arena, root, _, _) = same_owner_transform_scroll_fixture();
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        element.set_opacity(0.625);
        element.set_background_color_value(Color::rgb(32, 64, 96));
    }
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, properties, generations)
}

pub(crate) fn scroll_content_effect_interleave_fixture(
    outer_transform: bool,
    neutral_wrapper: bool,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let shape = if outer_transform {
        ScrollInterleaveFixtureShape::TransformScroll
    } else {
        ScrollInterleaveFixtureShape::FrameRootScroll
    };
    let (mut arena, root, _, _) = property_scroll_interleave_fixture(shape);
    if outer_transform {
        let mut root_style = Style::new();
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        let mut root_element = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        root_element.apply_style(root_style);
        root_element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
            glam::Vec3::new(7.0, 0.0, 0.0),
        )));
        root_element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    let scroll = if outer_transform {
        arena.children_of(root)[0]
    } else {
        root
    };
    let content = arena.children_of(scroll)[0];
    let effect_parent = if neutral_wrapper {
        let wrapper = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xb4_3011, 0.0, -20.0, 120.0, 240.0,
        ))));
        arena.set_parent(wrapper, Some(content));
        arena.push_child(content, wrapper);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        let mut wrapper_element =
            crate::view::test_support::get_element_mut::<Element>(&arena, wrapper);
        wrapper_element.apply_style(wrapper_style);
        wrapper_element.set_background_color_value(Color::rgb(12, 24, 36));
        wrapper_element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        wrapper
    } else {
        content
    };
    let effect = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_3020, 8.0, -12.0, 72.0, 48.0,
    ))));
    let leaf = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_3021, 12.0, -8.0, 48.0, 24.0,
    ))));
    arena.set_parent(effect, Some(effect_parent));
    arena.push_child(effect_parent, effect);
    arena.set_parent(leaf, Some(effect));
    arena.push_child(effect, leaf);
    let mut content_style = Style::new();
    content_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    crate::view::test_support::get_element_mut::<Element>(&arena, content)
        .apply_style(content_style);
    crate::view::test_support::get_element_mut::<Element>(&arena, content)
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    for owner in [effect, leaf] {
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, owner);
        element.apply_style(style);
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    {
        let mut effect_element =
            crate::view::test_support::get_element_mut::<Element>(&arena, effect);
        effect_element.set_opacity(0.625);
        effect_element.set_background_color_value(Color::rgb(32, 64, 96));
    }
    crate::view::test_support::get_element_mut::<Element>(&arena, leaf)
        .set_background_color_value(Color::rgb(24, 48, 72));
    assert!(
        crate::view::test_support::get_element::<Element>(&arena, content)
            .exact_retained_scroll_content_wrapper_recording_offset([0.0, 20.0])
            .is_some(),
        "exact content receiver offset oracle"
    );
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(
        properties.validation_errors.is_empty(),
        "{:?}",
        properties.validation_errors
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, properties, generations)
}

pub(crate) fn nested_scroll_plan_fixture() -> (
    NodeArena,
    NodeKey,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    fn install_geometry(arena: &NodeArena, key: NodeKey, rect: Rect, content: Size) {
        let mut element = crate::view::test_support::get_element_mut::<Element>(arena, key);
        element.layout_state.layout_position.x = rect.x;
        element.layout_state.layout_position.y = rect.y;
        element.layout_state.layout_size = Size {
            width: rect.width,
            height: rect.height,
        };
        element.layout_state.layout_inner_position.x = rect.x;
        element.layout_state.layout_inner_position.y = rect.y;
        element.layout_state.layout_inner_size = Size {
            width: rect.width,
            height: rect.height,
        };
        element.layout_state.content_size = content;
        element.set_background_color_value(Color::rgb(24, 48, 72));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }

    let mut arena = NodeArena::new();
    let outer = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x1251_00, 10.0, 20.0, 100.0, 80.0,
    ))));
    let inner = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x1251_01, 10.0, 20.0, 100.0, 300.0,
    ))));
    let leaf = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x1251_02, 10.0, 20.0, 100.0, 600.0,
    ))));
    arena.set_parent(inner, Some(outer));
    arena.push_child(outer, inner);
    arena.set_parent(leaf, Some(inner));
    arena.push_child(inner, leaf);
    for owner in [outer, inner] {
        let mut style = Style::new();
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        crate::view::test_support::get_element_mut::<Element>(&arena, owner).apply_style(style);
    }
    install_geometry(
        &arena,
        outer,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 80.0,
        },
        Size {
            width: 100.0,
            height: 300.0,
        },
    );
    install_geometry(
        &arena,
        inner,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 300.0,
        },
        Size {
            width: 100.0,
            height: 600.0,
        },
    );
    install_geometry(
        &arena,
        leaf,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 600.0,
        },
        Size {
            width: 100.0,
            height: 600.0,
        },
    );
    arena.refresh_subtree_dirty_cache(outer);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[outer]);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[outer], &properties);
    (arena, outer, inner, leaf, properties, generations)
}

pub(crate) fn native_scroll_forest_plan_fixture() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    native_scroll_forest_plan_fixture_with_s2_offset(30.0)
}

fn native_scroll_forest_plan_fixture_with_s2_offset(
    s2_offset_x: f32,
) -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    assert!(s2_offset_x.is_finite() && (0.0..=300.0).contains(&s2_offset_x));
    fn install_geometry(arena: &NodeArena, key: NodeKey, rect: Rect, content: Size) {
        let mut element = crate::view::test_support::get_element_mut::<Element>(arena, key);
        element.layout_state.layout_position.x = rect.x;
        element.layout_state.layout_position.y = rect.y;
        element.layout_state.layout_size = Size {
            width: rect.width,
            height: rect.height,
        };
        element.layout_state.layout_inner_position.x = rect.x;
        element.layout_state.layout_inner_position.y = rect.y;
        element.layout_state.layout_inner_size = Size {
            width: rect.width,
            height: rect.height,
        };
        element.layout_state.content_size = content;
        element.set_background_color_value(Color::rgb(24, 48, 72));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    fn attach(arena: &mut NodeArena, parent: NodeKey, child: NodeKey) {
        arena.set_parent(child, Some(parent));
        arena.push_child(parent, child);
    }
    fn scroll_style(axis: ScrollDirection) -> Style {
        let mut style = Style::new();
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(axis),
        );
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style
    }
    fn neutral_style() -> Style {
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style
    }

    let mut arena = NodeArena::new();
    let root0 = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_00, 0.0, 0.0, 700.0, 700.0,
    ))));
    let s0 = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_01, 10.0, 20.0, 100.0, 80.0,
    ))));
    let wrapper0 = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_02, 10.0, 20.0, 100.0, 300.0,
    ))));
    let s1 = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_03, 10.0, 20.0, 100.0, 300.0,
    ))));
    let wrapper1 = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_04, 10.0, 20.0, 300.0, 300.0,
    ))));
    let s2 = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_05, 10.0, 20.0, 300.0, 300.0,
    ))));
    let leaf0 = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_06, 10.0, 20.0, 600.0, 600.0,
    ))));
    let between = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_07, 330.0, 20.0, 20.0, 20.0,
    ))));
    let s2b = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_08, 400.0, 20.0, 100.0, 100.0,
    ))));
    let leaf0b = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_09, 390.0, 10.0, 200.0, 200.0,
    ))));
    attach(&mut arena, root0, s0);
    attach(&mut arena, s0, wrapper0);
    attach(&mut arena, wrapper0, s1);
    attach(&mut arena, s1, wrapper1);
    attach(&mut arena, wrapper1, s2);
    attach(&mut arena, s2, leaf0);
    attach(&mut arena, wrapper1, between);
    attach(&mut arena, wrapper1, s2b);
    attach(&mut arena, s2b, leaf0b);

    let root1 = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_10, 0.0, 0.0, 400.0, 220.0,
    ))));
    let s3 = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_11, 0.0, 0.0, 80.0, 80.0,
    ))));
    let leaf1 = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_12, 0.0, 0.0, 160.0, 80.0,
    ))));
    let s4 = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_13, 200.0, 0.0, 80.0, 80.0,
    ))));
    let leaf2 = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x12f0_14, 200.0, 0.0, 160.0, 160.0,
    ))));
    attach(&mut arena, root1, s3);
    attach(&mut arena, s3, leaf1);
    attach(&mut arena, root1, s4);
    attach(&mut arena, s4, leaf2);

    for wrapper in [root0, wrapper0, wrapper1, root1] {
        crate::view::test_support::get_element_mut::<Element>(&arena, wrapper)
            .apply_style(neutral_style());
    }

    crate::view::test_support::get_element_mut::<Element>(&arena, s0)
        .apply_style(scroll_style(ScrollDirection::Vertical));
    let mut rounded_horizontal = scroll_style(ScrollDirection::Horizontal);
    rounded_horizontal.insert(
        PropertyId::BorderRadius,
        ParsedValue::Length(crate::style::Length::px(8.0)),
    );
    crate::view::test_support::get_element_mut::<Element>(&arena, s1)
        .apply_style(rounded_horizontal);
    crate::view::test_support::get_element_mut::<Element>(&arena, s2)
        .apply_style(scroll_style(ScrollDirection::Both));
    crate::view::test_support::get_element_mut::<Element>(&arena, s3)
        .apply_style(scroll_style(ScrollDirection::Horizontal));
    crate::view::test_support::get_element_mut::<Element>(&arena, s4)
        .apply_style(scroll_style(ScrollDirection::Both));
    crate::view::test_support::get_element_mut::<Element>(&arena, s2b)
        .apply_style(scroll_style(ScrollDirection::Both));
    crate::view::test_support::get_element_mut::<Element>(&arena, s0)
        .set_scroll_offset((0.0, 24.0));
    crate::view::test_support::get_element_mut::<Element>(&arena, s1)
        .set_scroll_offset((40.0, 0.0));
    crate::view::test_support::get_element_mut::<Element>(&arena, s2)
        .set_scroll_offset((s2_offset_x, 50.0));
    crate::view::test_support::get_element_mut::<Element>(&arena, s3)
        .set_scroll_offset((20.0, 0.0));
    crate::view::test_support::get_element_mut::<Element>(&arena, s4)
        .set_scroll_offset((20.0, 30.0));
    crate::view::test_support::get_element_mut::<Element>(&arena, s2b)
        .set_scroll_offset((10.0, 10.0));

    install_geometry(
        &arena,
        root0,
        Rect {
            x: 0.0,
            y: 0.0,
            width: 700.0,
            height: 700.0,
        },
        Size {
            width: 700.0,
            height: 700.0,
        },
    );
    install_geometry(
        &arena,
        s0,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 80.0,
        },
        Size {
            width: 100.0,
            height: 300.0,
        },
    );
    install_geometry(
        &arena,
        wrapper0,
        Rect {
            x: 10.0,
            y: -4.0,
            width: 100.0,
            height: 300.0,
        },
        Size {
            width: 100.0,
            height: 300.0,
        },
    );
    install_geometry(
        &arena,
        s1,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 300.0,
        },
        Size {
            width: 300.0,
            height: 300.0,
        },
    );
    install_geometry(
        &arena,
        wrapper1,
        Rect {
            x: -30.0,
            y: 20.0,
            width: 300.0,
            height: 300.0,
        },
        Size {
            width: 300.0,
            height: 300.0,
        },
    );
    install_geometry(
        &arena,
        s2,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 300.0,
            height: 300.0,
        },
        Size {
            width: 600.0,
            height: 600.0,
        },
    );
    install_geometry(
        &arena,
        leaf0,
        Rect {
            x: 10.0 - s2_offset_x,
            y: -30.0,
            width: 600.0,
            height: 600.0,
        },
        Size {
            width: 600.0,
            height: 600.0,
        },
    );
    install_geometry(
        &arena,
        between,
        Rect {
            x: 330.0,
            y: 20.0,
            width: 20.0,
            height: 20.0,
        },
        Size {
            width: 20.0,
            height: 20.0,
        },
    );
    install_geometry(
        &arena,
        s2b,
        Rect {
            x: 400.0,
            y: 20.0,
            width: 100.0,
            height: 100.0,
        },
        Size {
            width: 200.0,
            height: 200.0,
        },
    );
    install_geometry(
        &arena,
        leaf0b,
        Rect {
            x: 390.0,
            y: 10.0,
            width: 200.0,
            height: 200.0,
        },
        Size {
            width: 200.0,
            height: 200.0,
        },
    );
    for (key, rect, content) in [
        (
            root1,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 400.0,
                height: 220.0,
            },
            Size {
                width: 400.0,
                height: 220.0,
            },
        ),
        (
            s3,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 80.0,
            },
            Size {
                width: 160.0,
                height: 80.0,
            },
        ),
        (
            leaf1,
            Rect {
                x: -20.0,
                y: 0.0,
                width: 160.0,
                height: 80.0,
            },
            Size {
                width: 160.0,
                height: 80.0,
            },
        ),
        (
            s4,
            Rect {
                x: 200.0,
                y: 0.0,
                width: 80.0,
                height: 80.0,
            },
            Size {
                width: 160.0,
                height: 160.0,
            },
        ),
        (
            leaf2,
            Rect {
                x: 180.0,
                y: -30.0,
                width: 160.0,
                height: 160.0,
            },
            Size {
                width: 160.0,
                height: 160.0,
            },
        ),
    ] {
        install_geometry(&arena, key, rect, content);
    }
    for root in [root0, root1] {
        arena.refresh_subtree_dirty_cache(root);
    }
    let roots = vec![root0, root1];
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    for boundary in [s0, s1, s2, s2b, s3, s4] {
        let element = crate::view::test_support::get_element::<Element>(&arena, boundary);
        assert!(
            element
                .exact_retained_scroll_forest_host_admission(boundary, &arena, 1.0)
                .is_some(),
            "forest host admission {boundary:?}"
        );
    }
    (arena, roots, properties, generations)
}

fn direct_scroll_transform_transaction_from_fixture_for_test(
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
) -> super::super::scroll_scene::ValidatedDirectScrollTransformTransaction {
    let scaffold = super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
        arena,
        &[root],
        properties,
        generations,
        1.0,
        [0.0, 0.0],
        None,
    )
    .unwrap();
    let geometry = super::super::scroll_scene::plan_direct_scroll_transform_geometry(
        &arena,
        scaffold,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(u32::MAX, u64::MAX)
            .unwrap(),
    )
    .unwrap();
    super::super::scroll_scene::compile_direct_scroll_transform_transaction(geometry).unwrap()
}

fn exact_direct_scroll_transform_transaction_for_test()
-> super::super::scroll_scene::ValidatedDirectScrollTransformTransaction {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
    direct_scroll_transform_transaction_from_fixture_for_test(
        &arena,
        root,
        &properties,
        &generations,
    )
}

fn general_property_scene_fixture() -> GeneralPropertySceneFixture {
    let (mut arena, outer, _before, inner_a, deep, inner_b, _, _) =
        nested_exact_transform_fixture();
    crate::view::test_support::get_element_mut::<Element>(&arena, deep)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            4.0, 0.0, 0.0,
        ))));
    crate::view::test_support::get_element_mut::<Element>(&arena, inner_b)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            5.0, 0.0, 0.0,
        ))));

    let neutral_root = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xd1_0001, 130.0, 10.0, 8.0, 8.0)),
    );
    let second_root = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xd1_0020, 145.0, 10.0, 12.0, 10.0)),
    );
    let trailing_root = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xd1_0030, 166.0, 10.0, 8.0, 8.0)),
    );
    let constraints = LayoutConstraints {
        max_width: 220.0,
        max_height: 140.0,
        viewport_width: 220.0,
        viewport_height: 140.0,
        percent_base_width: Some(220.0),
        percent_base_height: Some(140.0),
    };
    for (root, x, y) in [
        (neutral_root, 130.0, 10.0),
        (second_root, 145.0, 10.0),
        (trailing_root, 166.0, 10.0),
    ] {
        measure_and_place(
            &mut arena,
            root,
            constraints,
            LayoutPlacement {
                parent_x: x,
                parent_y: y,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 220.0,
                available_height: 140.0,
                viewport_width: 220.0,
                viewport_height: 140.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(140.0),
            },
        );
    }
    crate::view::test_support::get_element_mut::<Element>(&arena, second_root)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            6.0, 0.0, 0.0,
        ))));
    let roots = vec![neutral_root, outer, second_root, trailing_root];
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    GeneralPropertySceneFixture {
        arena,
        roots,
        outer,
        inner_a,
        deep,
        inner_b,
        second_root,
        properties,
        generations,
    }
}

fn property_surface_mut(
    steps: &mut [PaintPlanStep],
    owner: NodeKey,
) -> Option<&mut RetainedSurfacePlan> {
    for step in steps {
        let PaintPlanStep::RetainedSurface(surface) = step else {
            continue;
        };
        if surface.boundary_root() == owner {
            return Some(surface);
        }
        if let Some(found) = property_surface_mut(&mut surface.raster_steps, owner) {
            return Some(found);
        }
    }
    None
}

fn exact_transform_child_isolation_fixture() -> (
    NodeArena,
    NodeKey,
    NodeKey,
    NodeKey,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (arena, root, before, child, descendant, after, _, _) = nested_exact_transform_fixture();
    {
        let mut child_element =
            crate::view::test_support::get_element_mut::<Element>(&arena, child);
        child_element.set_resolved_transform_for_test(None);
        child_element.set_opacity(0.5);
    }
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (
        arena,
        root,
        before,
        child,
        descendant,
        after,
        properties,
        generations,
    )
}

fn planning_only_nested_effect_fixture() -> (
    NodeArena,
    NodeKey,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (arena, root, _before, child, grandchild, _after, _, _) = nested_exact_transform_fixture();
    {
        let mut root_element = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        root_element.set_resolved_transform_for_test(None);
        root_element.set_opacity(0.5);
    }
    {
        let mut child_element =
            crate::view::test_support::get_element_mut::<Element>(&arena, child);
        child_element.set_resolved_transform_for_test(None);
        child_element.set_opacity(0.0);
    }
    crate::view::test_support::get_element_mut::<Element>(&arena, grandchild).set_opacity(0.75);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, child, grandchild, properties, generations)
}

#[cfg(not(target_arch = "wasm32"))]
fn native_nested_effect_fixture(
    host: &str,
    state: &str,
    opacity: f32,
    transform: bool,
    deferred: bool,
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='18' height='14'><rect width='18' height='14' fill='#38bdf8'/></svg>";

    let mut parent = Element::new_with_id(0xc1_1200, 0.0, 0.0, 64.0, 40.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    parent_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(15, 23, 42)),
    );
    parent.apply_style(parent_style);

    let mut native_style = Style::new();
    native_style.insert(
        PropertyId::Width,
        ParsedValue::Length(crate::style::Length::px(18.0)),
    );
    native_style.insert(
        PropertyId::Height,
        ParsedValue::Length(crate::style::Length::px(14.0)),
    );
    native_style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(Opacity::new(opacity)),
    );
    native_style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(Transition::new(
            TransitionProperty::Width,
            200,
        ))),
    );
    if transform {
        native_style.set_transform(Transform::new([Rotate::z(Angle::deg(9.0))]));
    }
    if deferred {
        native_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                crate::style::Position::absolute()
                    .left(crate::style::Length::px(4.0))
                    .top(crate::style::Length::px(5.0))
                    .clip(crate::style::ClipMode::Viewport),
            ),
        );
    }
    let stable_id = 0xc1_1201;
    let child: Box<dyn ElementTrait> = match host {
        "text" => {
            let mut text = Text::new_with_id(stable_id, 0.0, 0.0, 18.0, 14.0, "native");
            text.set_opacity(opacity);
            Box::new(text)
        }
        "image" => {
            let source = if state == "ready" {
                ImageSource::Rgba {
                    width: 1,
                    height: 1,
                    pixels: Arc::from([64_u8, 160, 255, 255]),
                }
            } else {
                ImageSource::Path(format!("nested-effect-{state}.png").into())
            };
            let mut image = Image::new_with_id(stable_id, source);
            image.apply_style(native_style);
            match state {
                "loading" => image.set_resource_loading_for_test(),
                "error" => image.set_resource_error_for_test(),
                _ => {}
            }
            Box::new(image)
        }
        "svg" => {
            let source = if state == "ready" {
                SvgSource::Content(SVG.into())
            } else {
                SvgSource::Path(format!("nested-effect-{state}.svg").into())
            };
            let mut svg = Svg::new_with_id(stable_id, source);
            svg.apply_style(native_style);
            match state {
                "loading" => svg.set_document_loading_for_transform_test(),
                "error" => svg.set_document_error_for_transform_test(),
                _ => {}
            }
            Box::new(svg)
        }
        _ => panic!("unknown native effect host"),
    };

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(parent));
    let child = commit_child(&mut arena, root, child);
    arena
        .with_element_taken(child, |element, arena| element.sync_arena(arena))
        .expect("freeze native nested-effect resource state");
    measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 160.0,
            max_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
        LayoutPlacement {
            parent_x: 3.0,
            parent_y: 4.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 160.0,
            available_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
    );
    if matches!(host, "image" | "svg") {
        let mut node = arena
            .get_mut(child)
            .expect("native effect transition child");
        if host == "image" {
            node.element
                .as_any_mut()
                .downcast_mut::<Image>()
                .expect("Image host")
                .set_layout_transition_width_for_test(20.0);
        } else {
            node.element
                .as_any_mut()
                .downcast_mut::<Svg>()
                .expect("Svg host")
                .set_layout_transition_width_for_test(20.0);
        }
        drop(node);
        measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 160.0,
                max_height: 120.0,
                viewport_width: 160.0,
                viewport_height: 120.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 3.0,
                parent_y: 4.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 160.0,
                available_height: 120.0,
                viewport_width: 160.0,
                viewport_height: 120.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(120.0),
            },
        );
        assert!(
            arena
                .get(child)
                .unwrap()
                .element
                .retained_sampled_layout_transition_snapshot()
                .is_some(),
            "{host}/{state} must install one exact sampled effect transition"
        );
    }
    if host == "svg" && state == "ready" {
        arena
            .get_mut(child)
            .expect("svg child")
            .element
            .as_any_mut()
            .downcast_mut::<Svg>()
            .expect("Svg host")
            .prepare_content_paint_for_test(SVG, (18.0, 14.0), 1.0)
            .expect("prepare exact SVG paint");
    }
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, child, properties, generations)
}

fn deferred_element_effect_fixture(
    opacity: f32,
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut root = Element::new_with_id(0xc1_1300, 0.0, 0.0, 80.0, 60.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(15, 23, 42)),
    );
    root.apply_style(root_style);

    let mut deferred = Element::new_with_id(0xc1_1301, 0.0, 0.0, 24.0, 18.0);
    let mut deferred_style = Style::new();
    deferred_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    deferred_style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(Opacity::new(opacity)),
    );
    deferred_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            crate::style::Position::absolute()
                .left(crate::style::Length::px(4.0))
                .top(crate::style::Length::px(5.0))
                .clip(crate::style::ClipMode::Viewport),
        ),
    );
    deferred_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(30, 120, 220)),
    );
    deferred.apply_style(deferred_style);

    let mut nested = Element::new_with_id(0xc1_1302, 1.0, 1.0, 6.0, 5.0);
    let mut nested_style = Style::new();
    nested_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    nested_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(250, 180, 30)),
    );
    nested.apply_style(nested_style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root));
    let deferred = commit_child(&mut arena, root, Box::new(deferred));
    let nested = commit_child(&mut arena, deferred, Box::new(nested));
    measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 160.0,
            max_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 160.0,
            available_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
    );
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, deferred, nested, properties, generations)
}

fn nested_opaque_cursor_fixture(
    parent_before_opaque: usize,
    child_opaque: usize,
    parent_after_opaque: usize,
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    assert!(child_opaque > 0);
    let element = |id: u64, width: f32, height: f32, opaque: bool| {
        let mut element = Element::new_with_id(id, 0.0, 0.0, width, height);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        if opaque {
            style.insert(
                PropertyId::BackgroundColor,
                ParsedValue::color_like(Color::rgb(40, 100, 180)),
            );
        }
        element.apply_style(style);
        element
    };

    let mut next_id = 0xc5_a200_u64;
    let mut take_id = || {
        let id = next_id;
        next_id += 1;
        id
    };
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(element(take_id(), 80.0, 60.0, parent_before_opaque > 0)),
    );
    for _ in 1..parent_before_opaque {
        commit_child(
            &mut arena,
            root,
            Box::new(element(take_id(), 4.0, 4.0, true)),
        );
    }
    let child = commit_child(
        &mut arena,
        root,
        Box::new(element(take_id(), 20.0, 16.0, true)),
    );
    for _ in 1..child_opaque {
        commit_child(
            &mut arena,
            child,
            Box::new(element(take_id(), 3.0, 3.0, true)),
        );
    }
    for _ in 0..parent_after_opaque {
        commit_child(
            &mut arena,
            root,
            Box::new(element(take_id(), 4.0, 4.0, true)),
        );
    }

    measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 160.0,
            max_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 160.0,
            available_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
    );
    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            10.0, 0.0, 0.0,
        ))));
    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            20.0, 0.0, 0.0,
        ))));
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, child, properties, generations)
}

fn only_surface(plan: &FramePaintPlan) -> &RetainedSurfacePlan {
    let [PaintPlanStep::RetainedSurface(surface)] = plan.steps.as_slice() else {
        panic!("fixture must contain one retained surface")
    };
    surface
}

fn only_surface_mut(plan: &mut FramePaintPlan) -> &mut RetainedSurfacePlan {
    let [PaintPlanStep::RetainedSurface(surface)] = plan.steps.as_mut_slice() else {
        panic!("fixture must contain one retained surface")
    };
    surface
}

fn only_span(surface: &RetainedSurfacePlan) -> &ArtifactSpanPlan {
    let [PaintPlanStep::ArtifactSpan(span)] = surface.raster_steps.as_slice() else {
        panic!("fixture surface must contain one artifact span")
    };
    span
}

fn only_span_mut(surface: &mut RetainedSurfacePlan) -> &mut ArtifactSpanPlan {
    let [PaintPlanStep::ArtifactSpan(span)] = surface.raster_steps.as_mut_slice() else {
        panic!("fixture surface must contain one artifact span")
    };
    span
}

fn nested_surface_mut(plan: &mut FramePaintPlan) -> &mut RetainedSurfacePlan {
    let parent = only_surface_mut(plan);
    parent
        .raster_steps
        .iter_mut()
        .find_map(|step| match step {
            PaintPlanStep::RetainedSurface(surface) => Some(surface.as_mut()),
            PaintPlanStep::ArtifactSpan(_) => None,
        })
        .expect("fixture contains one nested surface")
}

fn isolation_plan_mut(plan: &mut FramePaintPlan) -> &mut IsolationSurfacePlan {
    match &mut only_surface_mut(plan).kind {
        SurfaceKind::Isolation(plan) => plan,
        SurfaceKind::Transform(_)
        | SurfaceKind::NestedIsolation(_)
        | SurfaceKind::ScrollHost(_) => {
            panic!("fixture must contain root isolation surface")
        }
    }
}

fn assert_forced_rejection_has_zero_graph_mutation(
    plan: &FramePaintPlan,
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
    expected: super::super::ForcedTransformSurfaceError,
) {
    let before = graph.build_state_snapshot_for_test();
    let mut viewport = Viewport::new();
    let viewport_before = viewport.retained_surface_transaction_shape_for_test();
    let error = match super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        plan,
        graph,
        ctx,
    ) {
        Ok(_) => panic!("tampered forced plan must reject before emit"),
        Err(error) => error,
    };
    assert_eq!(error, expected);
    assert_eq!(graph.build_state_snapshot_for_test(), before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        viewport_before,
        "prepare rejection cannot stage or commit any retained-surface transaction"
    );
}

fn commit_forced_nested_plan(viewport: &mut Viewport, plan: &FramePaintPlan) {
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let outer = ctx.allocate_target(&mut graph);
    ctx.set_current_target(outer);
    super::super::execute_forced_transform_surface_for_test(viewport, plan, &mut graph, ctx)
        .expect("baseline nested R/R execution");
    viewport.finish_retained_surface_transaction(true);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (2, None)
    );
}

fn parent_context_with_clear(
    graph: &mut FrameGraph,
    width: u32,
    height: u32,
    scale: f32,
) -> (
    UiBuildContext,
    crate::view::render_pass::draw_rect_pass::RenderTargetOut,
) {
    let mut ctx = UiBuildContext::new(width, height, wgpu::TextureFormat::Bgra8Unorm, scale);
    let parent = ctx.allocate_target(graph);
    ctx.set_current_target(parent);
    graph.add_graphics_pass(crate::view::render_pass::ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: parent,
        },
    ));
    (ctx, parent)
}

fn parent_context_without_clear(
    graph: &mut FrameGraph,
    width: u32,
    height: u32,
    scale: f32,
) -> UiBuildContext {
    let mut ctx = UiBuildContext::new(width, height, wgpu::TextureFormat::Bgra8Unorm, scale);
    let parent = ctx.allocate_target(graph);
    ctx.set_current_target(parent);
    ctx
}

fn execute_forced_plan_graph(viewport: &mut Viewport, plan: &FramePaintPlan) -> FrameGraph {
    let mut graph = FrameGraph::new();
    let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
    super::super::execute_forced_transform_surface_for_test(viewport, plan, &mut graph, ctx)
        .expect("forced exact retained plan");
    graph
}

fn retained_surface_stamp(
    surface: &RetainedSurfacePlan,
    artifact: &PaintArtifact,
) -> Option<super::super::RetainedSurfaceRasterStamp> {
    let scale = 2.0_f32;
    let color_key = surface.persistent_color_key;
    let color = crate::view::base_component::texture_desc_for_logical_bounds(
        surface.geometry().source_bounds,
        scale,
        None,
        wgpu::TextureFormat::Bgra8Unorm,
    );
    let (color, depth) =
        crate::view::base_component::persistent_target_texture_descriptors(color, color_key);
    super::super::validated_retained_surface_raster_stamp(
        artifact,
        surface.boundary_root,
        surface.stable_id,
        surface.transform(),
        super::super::RetainedSurfaceRasterInputs {
            color,
            depth,
            scale_factor_bits: scale.to_bits(),
            source_bounds_bits: [
                surface.geometry().source_bounds.x.to_bits(),
                surface.geometry().source_bounds.y.to_bits(),
                surface.geometry().source_bounds.width.to_bits(),
                surface.geometry().source_bounds.height.to_bits(),
            ],
        },
        surface.aggregate_opaque_order_span.clone(),
    )
}

mod deferred_effect_tests;
mod direct_scroll_transform_prepare_tests;
mod direct_scroll_transform_seal_tests;
mod forced_executor_rejection_tests;
mod forced_nested_surface_tests;
mod forced_rect_executor_tests;
mod inherited_transform_authorization_tests;
mod isolation_tree_tests;
mod legacy_graph_equivalence_tests;
mod mixed_effect_tree_tests;
mod native_media_transform_tests;
mod native_scroll_forest_pool_tests;
mod native_scroll_forest_seal_tests;
mod planner_rejection_tests;
mod property_boundary_dag_tests;
mod property_boundary_forest_branching_tests;
mod property_boundary_forest_linear_tests;
mod property_boundary_forest_multi_root_tests;
mod property_boundary_forest_plain_root_tests;
mod property_boundary_forest_tests;
mod property_boundary_program_forest_tests;
mod property_effect_scaffold_tests;
mod property_effect_scene_tests;
mod property_scene_tests;
mod property_scroll_interleave_tests;
mod same_owner_effect_tests;
mod same_owner_transform_effect_scroll_tests;
mod stage_c_retained_semantic_baseline_tests;
mod stage_c_scroll_capability_corpus_tests;
mod stage_c_transition_capability_tests;
mod transform_isolation_tests;
