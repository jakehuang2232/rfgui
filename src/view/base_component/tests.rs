use super::{hit_test, hit_test_roots, tick_animation_frames, tick_post_layout_animation_frames};
use crate::style::{Anchor, AnchorName, Color, Layout};
use crate::style::{
    Angle, ClipMode, Length, ParsedValue, Position, PropertyId, Rotate, ScrollDirection, Style,
    Transform, TransformOrigin, Translate,
};
use crate::ui::{
    ClickEvent, EventMeta, Modifiers, NodeId, PointerButton, PointerButtons, PointerEventData,
};
use crate::view::base_component::{
    BoxModelSnapshot, BuildState, DirtyFlags, Element, ElementTrait, EventTarget,
    LayoutConstraints, LayoutPlacement, Layoutable, PaintResourcePreparationContext, Renderable,
    UiBuildContext,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::test_support::{commit_child, commit_element, measure_and_place, new_test_arena};
use crate::view::viewport::dispatch::dispatch_click_from_hit_test;
use crate::view::{Viewport, ViewportControl};
use std::cell::Cell;
use std::rc::Rc;

struct AnimationTickProbe {
    id: u64,
    children: Vec<NodeKey>,
    ticks: Rc<Cell<u32>>,
    wants_checks: Rc<Cell<u32>>,
    tick_now: Rc<Cell<Option<crate::time::Instant>>>,
    post_tick_now: Rc<Cell<Option<crate::time::Instant>>>,
    resource_now: Rc<Cell<Option<crate::time::Instant>>>,
}

impl Layoutable for AnimationTickProbe {
    fn requires_arena_sync(&self) -> bool {
        true
    }
    fn prepare_paint_resources(&mut self, context: PaintResourcePreparationContext) {
        self.resource_now.set(Some(context.now));
    }
    fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
    fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
    fn measured_size(&self) -> (f32, f32) {
        (1.0, 1.0)
    }
    fn set_layout_width(&mut self, _width: f32) {}
    fn set_layout_height(&mut self, _height: f32) {}
}

impl EventTarget for AnimationTickProbe {
    fn wants_animation_frame(&self) -> bool {
        self.wants_checks.set(self.wants_checks.get() + 1);
        false
    }
}

impl Renderable for AnimationTickProbe {
    fn build(
        &mut self,
        _graph: &mut FrameGraph,
        _arena: &mut NodeArena,
        ctx: UiBuildContext,
    ) -> BuildState {
        ctx.into_state()
    }
}

impl ElementTrait for AnimationTickProbe {
    fn stable_id(&self) -> u64 {
        self.id
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
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

    fn tick_animation_frame(&mut self, now: crate::time::Instant) -> DirtyFlags {
        self.ticks.set(self.ticks.get() + 1);
        self.tick_now.set(Some(now));
        DirtyFlags::NONE
    }

    fn tick_post_layout_animation_frame(&mut self, now: crate::time::Instant) -> DirtyFlags {
        self.post_tick_now.set(Some(now));
        DirtyFlags::NONE
    }

    fn children(&self) -> &[NodeKey] {
        &self.children
    }

    fn sync_children_mirror(&mut self, children: &[NodeKey]) {
        self.children.clear();
        self.children.extend_from_slice(children);
    }
}

fn constraints(w: f32, h: f32) -> LayoutConstraints {
    LayoutConstraints {
        max_width: w,
        max_height: h,
        viewport_width: w,
        percent_base_width: Some(w),
        percent_base_height: Some(h),
        viewport_height: h,
    }
}

fn placement(w: f32, h: f32) -> LayoutPlacement {
    LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: w,
        available_height: h,
        viewport_width: w,
        percent_base_width: Some(w),
        percent_base_height: Some(h),
        viewport_height: h,
    }
}

fn absolute_diagnostic_element(position: Position, cursor: crate::style::Cursor) -> Element {
    let mut element = Element::new(0.0, 0.0, 20.0, 20.0);
    let mut style = Style::new();
    style.insert(PropertyId::Position, ParsedValue::Position(position));
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
    style.insert(PropertyId::Cursor, ParsedValue::Cursor(cursor));
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#ff00ff")),
    );
    element.apply_style(style);
    element
}

fn absolute_diagnostic_roots(
    position: Position,
) -> (
    crate::view::node_arena::NodeArena,
    [crate::view::node_arena::NodeKey; 2],
    crate::view::node_arena::NodeKey,
) {
    let mut lower_root = Element::new(0.0, 0.0, 80.0, 80.0);
    lower_root.set_background_color_value(Color::rgb(16, 16, 16));
    let popup = absolute_diagnostic_element(position, crate::style::Cursor::Crosshair);

    let mut higher_root = Element::new(90.0, 0.0, 80.0, 80.0);
    higher_root.set_background_color_value(Color::rgb(32, 32, 32));

    let mut arena = new_test_arena();
    let lower_key = commit_element(&mut arena, Box::new(lower_root));
    let popup_key = commit_child(&mut arena, lower_key, Box::new(popup));
    let higher_key = commit_element(&mut arena, Box::new(higher_root));
    let root_keys = [lower_key, higher_key];
    for &root_key in &root_keys {
        measure_and_place(
            &mut arena,
            root_key,
            constraints(220.0, 120.0),
            placement(220.0, 120.0),
        );
    }
    (arena, root_keys, popup_key)
}

mod absolute_hit_test_tests;
mod animation_tick_tests;
mod hit_test_tests;
