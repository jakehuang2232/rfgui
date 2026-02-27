use super::{ElementCore, Position, Size};
use crate::ColorLike;
use crate::render_pass::render_target::RenderTargetPass;
use crate::style::{
    AlignItems, AnchorName, BoxShadow, ClipMode, Collision, CollisionBoundary, Color,
    ComputedStyle, Cursor, Display, FlowDirection, FlowWrap, JustifyContent, Length, PositionMode,
    ScrollDirection, SizeValue, Style, TransitionProperty, TransitionTiming, compute_style,
};
use crate::transition::{
    LayoutField, LayoutTrackRequest, LayoutTransition as RuntimeLayoutTransition, ScrollAxis,
    StyleField, StyleTrackRequest, StyleTransition as RuntimeStyleTransition, StyleValue,
    TimeFunction, VisualField, VisualTrackRequest, VisualTransition as RuntimeVisualTransition,
};
use crate::ui::{
    BlurEvent, ClickEvent, FocusEvent, KeyDownEvent, KeyUpEvent, MouseButton as UiMouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent,
};
use crate::view::frame_graph::texture_resource::TextureHandle;
use crate::view::frame_graph::{FrameGraph, InSlot, RenderPass, TextureDesc};
use crate::view::render_pass::draw_rect_pass::{RenderTargetOut, RenderTargetTag};
use crate::view::render_pass::{
    ClearPass, CompositeLayerPass, DrawRectPass, LayerOut, LayerTag, ShadowMesh, ShadowParams,
    ShadowPass,
};
use crate::view::viewport::ViewportControl;
use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
struct EdgeInsets {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LayoutProposal {
    width: f32,
    height: f32,
    viewport_width: f32,
    viewport_height: f32,
    percent_base_width: Option<f32>,
    percent_base_height: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
struct LayoutFrame {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Clone, Copy, Debug)]
struct CornerRadii {
    top_left: f32,
    top_right: f32,
    bottom_right: f32,
    bottom_left: f32,
}

impl CornerRadii {
    const fn uniform(value: f32) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }

    const fn zero() -> Self {
        Self::uniform(0.0)
    }

    fn to_array(self) -> [f32; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
    }

    fn has_any_rounding(self) -> bool {
        self.top_left > 0.0
            || self.top_right > 0.0
            || self.bottom_right > 0.0
            || self.bottom_left > 0.0
    }

    fn max(self) -> f32 {
        self.top_left
            .max(self.top_right)
            .max(self.bottom_right)
            .max(self.bottom_left)
    }
}

#[derive(Clone)]
struct EdgeColors {
    left: Box<dyn ColorLike>,
    right: Box<dyn ColorLike>,
    top: Box<dyn ColorLike>,
    bottom: Box<dyn ColorLike>,
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Rect {
    fn contains(self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px <= self.x + self.width && py <= self.y + self.height
    }
}

#[derive(Clone, Copy, Debug)]
struct AnchorSnapshot {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Default)]
struct PlacementRuntime {
    depth: usize,
    viewport_width: f32,
    viewport_height: f32,
    anchors: std::collections::HashMap<String, AnchorSnapshot>,
    child_clip_stack: Vec<Rect>,
}

thread_local! {
    static PLACEMENT_RUNTIME: RefCell<PlacementRuntime> = RefCell::new(PlacementRuntime::default());
}

#[derive(Clone, Copy, Debug)]
enum ScrollbarAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug)]
struct ScrollbarDragState {
    axis: ScrollbarAxis,
    grab_offset: f32,
    reanchor_on_first_move: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct ScrollbarGeometry {
    vertical_track: Option<Rect>,
    vertical_thumb: Option<Rect>,
    horizontal_track: Option<Rect>,
    horizontal_thumb: Option<Rect>,
}

pub struct UiBuildContext {
    last_target: Option<RenderTargetOut>,
    color_target: Option<TextureHandle>,
    target_width: u32,
    target_height: u32,
    target_format: wgpu::TextureFormat,
    scissor_rect: Option<[u32; 4]>,
    deferred_node_ids: Vec<u64>,
}

impl UiBuildContext {
    pub fn new(
        viewport_width: u32,
        viewport_height: u32,
        viewport_format: wgpu::TextureFormat,
    ) -> Self {
        Self {
            last_target: None,
            color_target: None,
            target_width: viewport_width.max(1),
            target_height: viewport_height.max(1),
            target_format: viewport_format,
            scissor_rect: None,
            deferred_node_ids: Vec::new(),
        }
    }

    pub fn allocate_target(&mut self, graph: &mut FrameGraph) -> RenderTargetOut {
        self.next_target(graph)
    }

    pub fn set_last_target(&mut self, target: RenderTargetOut) {
        self.last_target = Some(target);
    }

    pub(crate) fn last_target(&self) -> Option<&RenderTargetOut> {
        self.last_target.as_ref()
    }

    fn next_target(&mut self, graph: &mut FrameGraph) -> RenderTargetOut {
        graph.declare_texture::<RenderTargetTag>(TextureDesc::new(
            self.target_width,
            self.target_height,
            self.target_format,
            wgpu::TextureDimension::D2,
        ))
    }

    fn allocate_layer(&mut self, graph: &mut FrameGraph) -> LayerOut {
        graph.declare_texture::<LayerTag>(TextureDesc::new(
            self.target_width,
            self.target_height,
            self.target_format,
            wgpu::TextureDimension::D2,
        ))
    }

    fn color_target(&self) -> Option<TextureHandle> {
        self.color_target
    }

    pub(crate) fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        self.color_target = color_target;
    }

    fn scissor_rect(&self) -> Option<[u32; 4]> {
        self.scissor_rect
    }

    pub(crate) fn push_scissor_rect(&mut self, scissor_rect: Option<[u32; 4]>) -> Option<[u32; 4]> {
        let previous = self.scissor_rect;
        self.scissor_rect = intersect_scissor_rects(self.scissor_rect, scissor_rect);
        previous
    }

    pub(crate) fn restore_scissor_rect(&mut self, previous: Option<[u32; 4]>) {
        self.scissor_rect = previous;
    }

    pub(crate) fn append_to_defer(&mut self, node_id: u64) {
        if !self.deferred_node_ids.contains(&node_id) {
            self.deferred_node_ids.push(node_id);
        }
    }

    pub(crate) fn take_deferred_node_ids(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.deferred_node_ids)
    }

    pub(crate) fn push_pass<P: RenderTargetPass + RenderPass + 'static>(
        &mut self,
        graph: &mut FrameGraph,
        mut pass: P,
    ) {
        pass.apply_clip(self.scissor_rect());
        pass.set_color_target(self.color_target());

        if let Some(prev) = self.last_target.as_ref() {
            if let Some(handle) = prev.handle() {
                pass.set_input(InSlot::with_handle(handle));
            }
        }
        let output = self.next_target(graph);
        let output_for_ctx = output.clone();
        pass.set_output(output);
        graph.add_pass(pass);
        self.last_target = Some(output_for_ctx);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutContext {
    pub width: f32,
    pub height: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub percent_base_width: Option<f32>,
    pub percent_base_height: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutConstraints {
    pub max_width: f32,
    pub max_height: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub percent_base_width: Option<f32>,
    pub percent_base_height: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutPlacement {
    pub parent_x: f32,
    pub parent_y: f32,
    pub visual_offset_x: f32,
    pub visual_offset_y: f32,
    pub available_width: f32,
    pub available_height: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub percent_base_width: Option<f32>,
    pub percent_base_height: Option<f32>,
}

impl LayoutConstraints {
    fn context(self) -> LayoutContext {
        LayoutContext {
            width: self.max_width,
            height: self.max_height,
            viewport_width: self.viewport_width,
            viewport_height: self.viewport_height,
            percent_base_width: self.percent_base_width,
            percent_base_height: self.percent_base_height,
        }
    }
}

impl LayoutPlacement {
    fn context(self) -> LayoutContext {
        LayoutContext {
            width: self.available_width,
            height: self.available_height,
            viewport_width: self.viewport_width,
            viewport_height: self.viewport_height,
            percent_base_width: self.percent_base_width,
            percent_base_height: self.percent_base_height,
        }
    }
}

pub trait Layoutable {
    fn measure(&mut self, constraints: LayoutConstraints);
    fn place(&mut self, placement: LayoutPlacement);
    fn measured_size(&self) -> (f32, f32);
    fn set_layout_width(&mut self, width: f32);
    fn set_layout_height(&mut self, height: f32);
    fn set_layout_offset(&mut self, _x: f32, _y: f32) {}
}

pub trait EventTarget {
    fn dispatch_mouse_down(
        &mut self,
        _event: &mut MouseDownEvent,
        _control: &mut ViewportControl<'_>,
    ) {
    }
    fn dispatch_mouse_up(&mut self, _event: &mut MouseUpEvent, _control: &mut ViewportControl<'_>) {
    }
    fn dispatch_mouse_move(
        &mut self,
        _event: &mut MouseMoveEvent,
        _control: &mut ViewportControl<'_>,
    ) {
    }
    fn dispatch_click(&mut self, _event: &mut ClickEvent, _control: &mut ViewportControl<'_>) {}
    fn dispatch_key_down(&mut self, _event: &mut KeyDownEvent, _control: &mut ViewportControl<'_>) {
    }
    fn dispatch_key_up(&mut self, _event: &mut KeyUpEvent, _control: &mut ViewportControl<'_>) {}
    fn dispatch_text_input(
        &mut self,
        _event: &mut crate::ui::TextInputEvent,
        _control: &mut ViewportControl<'_>,
    ) {
    }
    fn dispatch_ime_preedit(
        &mut self,
        _event: &mut crate::ui::ImePreeditEvent,
        _control: &mut ViewportControl<'_>,
    ) {
    }
    fn dispatch_focus(&mut self, _event: &mut FocusEvent, _control: &mut ViewportControl<'_>) {}
    fn dispatch_blur(&mut self, _event: &mut BlurEvent, _control: &mut ViewportControl<'_>) {}
    fn cancel_pointer_interaction(&mut self) -> bool {
        false
    }
    fn set_hovered(&mut self, _hovered: bool) -> bool {
        false
    }
    fn scroll_by(&mut self, _dx: f32, _dy: f32) -> bool {
        false
    }
    fn can_scroll_by(&self, _dx: f32, _dy: f32) -> bool {
        false
    }
    fn get_scroll_offset(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn set_scroll_offset(&mut self, _offset: (f32, f32)) {}
    fn ime_cursor_rect(&self) -> Option<(f32, f32, f32, f32)> {
        None
    }
    fn cursor(&self) -> Cursor {
        Cursor::Default
    }
    fn wants_animation_frame(&self) -> bool {
        false
    }
    fn take_style_transition_requests(&mut self) -> Vec<StyleTrackRequest> {
        Vec::new()
    }
    fn take_layout_transition_requests(&mut self) -> Vec<LayoutTrackRequest> {
        Vec::new()
    }
    fn take_visual_transition_requests(&mut self) -> Vec<VisualTrackRequest> {
        Vec::new()
    }
}

pub trait Renderable {
    fn build(&mut self, graph: &mut FrameGraph, ctx: &mut UiBuildContext);
}

pub trait ElementTrait: Layoutable + EventTarget + Renderable + std::any::Any {
    fn id(&self) -> u64;
    fn parent_id(&self) -> Option<u64>;
    fn set_parent_id(&mut self, parent_id: Option<u64>);
    fn box_model_snapshot(&self) -> BoxModelSnapshot;
    fn children(&self) -> Option<&[Box<dyn ElementTrait>]>;
    fn children_mut(&mut self) -> Option<&mut [Box<dyn ElementTrait>]>;

    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    fn snapshot_state(&self) -> Option<Box<dyn std::any::Any>> {
        None
    }

    fn restore_state(&mut self, _snapshot: &dyn std::any::Any) -> bool {
        false
    }

    fn intercepts_pointer_at(&self, _viewport_x: f32, _viewport_y: f32) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BoxModelSnapshot {
    pub node_id: u64,
    pub parent_id: Option<u64>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub border_radius: f32,
    pub should_render: bool,
}

type MouseDownHandler = Box<dyn FnMut(&mut MouseDownEvent, &mut ViewportControl<'_>)>;
type MouseUpHandler = Box<dyn FnMut(&mut MouseUpEvent, &mut ViewportControl<'_>)>;
type MouseMoveHandler = Box<dyn FnMut(&mut MouseMoveEvent, &mut ViewportControl<'_>)>;
type ClickHandler = Box<dyn FnMut(&mut ClickEvent, &mut ViewportControl<'_>)>;
type KeyDownHandler = Box<dyn FnMut(&mut KeyDownEvent, &mut ViewportControl<'_>)>;
type KeyUpHandler = Box<dyn FnMut(&mut KeyUpEvent, &mut ViewportControl<'_>)>;
type FocusHandler = Box<dyn FnMut(&mut FocusEvent, &mut ViewportControl<'_>)>;
type BlurHandler = Box<dyn FnMut(&mut BlurEvent, &mut ViewportControl<'_>)>;

#[derive(Clone, Debug)]
struct FlexLayoutInfo {
    lines: Vec<Vec<usize>>,
    line_main_sum: Vec<f32>,
    line_cross_max: Vec<f32>,
    total_main: f32,
    total_cross: f32,
    child_sizes: Vec<(f32, f32)>,
}

#[derive(Clone, Copy, Debug)]
struct ElementStyleSnapshot {
    opacity: f32,
    border_radius: f32,
    is_hovered: bool,
    background_color: Color,
    foreground_color: Color,
    border_top_color: Color,
    border_right_color: Color,
    border_bottom_color: Color,
    border_left_color: Color,
    transition_snapshot: Option<TransitionSnapshot>,
}

#[derive(Clone, Copy, Debug)]
struct TransitionSnapshot {
    has_layout_snapshot: bool,
    layout_transition_visual_offset_x: f32,
    layout_transition_visual_offset_y: f32,
    layout_transition_override_width: Option<f32>,
    layout_transition_override_height: Option<f32>,
    layout_transition_target_x: Option<f32>,
    layout_transition_target_y: Option<f32>,
    layout_transition_target_width: Option<f32>,
    layout_transition_target_height: Option<f32>,
    last_parent_layout_x: f32,
    last_parent_layout_y: f32,
}

pub struct Element {
    core: ElementCore,
    anchor_name: Option<AnchorName>,
    layout_flow_position: Position,
    layout_inner_position: Position,
    layout_flow_inner_position: Position,
    layout_inner_size: Size,
    parsed_style: Style,
    computed_style: ComputedStyle,
    padding: EdgeInsets,
    background_color: Box<dyn ColorLike>,
    border_colors: EdgeColors,
    border_widths: EdgeInsets,
    border_radii: CornerRadii,
    border_radius: f32,
    box_shadows: Vec<BoxShadow>,
    foreground_color: Color,
    opacity: f32,
    scroll_direction: ScrollDirection,
    scroll_offset: Position,
    content_size: Size,
    scrollbar_drag: Option<ScrollbarDragState>,
    last_scrollbar_interaction: Option<Instant>,
    scrollbar_shadow_blur_radius: f32,
    pending_style_transition_requests: Vec<StyleTrackRequest>,
    pending_layout_transition_requests: Vec<LayoutTrackRequest>,
    pending_visual_transition_requests: Vec<VisualTrackRequest>,
    has_style_snapshot: bool,
    has_layout_snapshot: bool,
    layout_transition_visual_offset_x: f32,
    layout_transition_visual_offset_y: f32,
    layout_transition_override_width: Option<f32>,
    layout_transition_override_height: Option<f32>,
    layout_transition_target_x: Option<f32>,
    layout_transition_target_y: Option<f32>,
    layout_transition_target_width: Option<f32>,
    layout_transition_target_height: Option<f32>,
    last_parent_layout_x: f32,
    last_parent_layout_y: f32,
    is_hovered: bool,
    mouse_down_handlers: Vec<MouseDownHandler>,
    mouse_up_handlers: Vec<MouseUpHandler>,
    mouse_move_handlers: Vec<MouseMoveHandler>,
    click_handlers: Vec<ClickHandler>,
    key_down_handlers: Vec<KeyDownHandler>,
    key_up_handlers: Vec<KeyUpHandler>,
    focus_handlers: Vec<FocusHandler>,
    blur_handlers: Vec<BlurHandler>,
    layout_dirty: bool,
    last_layout_proposal: Option<LayoutProposal>,
    flex_info: Option<FlexLayoutInfo>,
    has_absolute_descendant_for_hit_test: bool,
    children: Vec<Box<dyn ElementTrait>>,
}

impl ElementTrait for Element {
    fn id(&self) -> u64 {
        self.core.id
    }

    fn parent_id(&self) -> Option<u64> {
        self.core.parent_id
    }

    fn set_parent_id(&mut self, parent_id: Option<u64>) {
        self.core.parent_id = parent_id;
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.core.id,
            parent_id: self.core.parent_id,
            x: self.core.layout_position.x,
            y: self.core.layout_position.y,
            width: self.core.layout_size.width,
            height: self.core.layout_size.height,
            border_radius: self.border_radius,
            should_render: self.core.should_render,
        }
    }

    fn children(&self) -> Option<&[Box<dyn ElementTrait>]> {
        Some(&self.children)
    }

    fn children_mut(&mut self) -> Option<&mut [Box<dyn ElementTrait>]> {
        Some(&mut self.children)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn intercepts_pointer_at(&self, viewport_x: f32, viewport_y: f32) -> bool {
        let local_x = viewport_x - self.core.layout_position.x;
        let local_y = viewport_y - self.core.layout_position.y;
        self.is_scrollbar_hit(local_x, local_y)
    }

    fn snapshot_state(&self) -> Option<Box<dyn std::any::Any>> {
        let [bg_r, bg_g, bg_b, bg_a] = self.background_color.as_ref().to_rgba_u8();
        let [bt_r, bt_g, bt_b, bt_a] = self.border_colors.top.as_ref().to_rgba_u8();
        let [br_r, br_g, br_b, br_a] = self.border_colors.right.as_ref().to_rgba_u8();
        let [bb_r, bb_g, bb_b, bb_a] = self.border_colors.bottom.as_ref().to_rgba_u8();
        let [bl_r, bl_g, bl_b, bl_a] = self.border_colors.left.as_ref().to_rgba_u8();
        Some(Box::new(ElementStyleSnapshot {
            opacity: self.opacity,
            border_radius: self.border_radius,
            is_hovered: self.is_hovered,
            background_color: Color::rgba(bg_r, bg_g, bg_b, bg_a),
            foreground_color: self.foreground_color,
            border_top_color: Color::rgba(bt_r, bt_g, bt_b, bt_a),
            border_right_color: Color::rgba(br_r, br_g, br_b, br_a),
            border_bottom_color: Color::rgba(bb_r, bb_g, bb_b, bb_a),
            border_left_color: Color::rgba(bl_r, bl_g, bl_b, bl_a),
            transition_snapshot: Some(TransitionSnapshot {
                has_layout_snapshot: self.has_layout_snapshot,
                layout_transition_visual_offset_x: self.layout_transition_visual_offset_x,
                layout_transition_visual_offset_y: self.layout_transition_visual_offset_y,
                layout_transition_override_width: self.layout_transition_override_width,
                layout_transition_override_height: self.layout_transition_override_height,
                layout_transition_target_x: self.layout_transition_target_x,
                layout_transition_target_y: self.layout_transition_target_y,
                layout_transition_target_width: self.layout_transition_target_width,
                layout_transition_target_height: self.layout_transition_target_height,
                last_parent_layout_x: self.last_parent_layout_x,
                last_parent_layout_y: self.last_parent_layout_y,
            }),
        }))
    }

    fn restore_state(&mut self, snapshot: &dyn std::any::Any) -> bool {
        let Some(snapshot) = snapshot.downcast_ref::<ElementStyleSnapshot>() else {
            return false;
        };

        self.opacity = snapshot.opacity;
        self.border_radius = snapshot.border_radius;
        self.is_hovered = snapshot.is_hovered;
        self.background_color = Box::new(snapshot.background_color);
        self.foreground_color = snapshot.foreground_color;
        self.border_colors.top = Box::new(snapshot.border_top_color);
        self.border_colors.right = Box::new(snapshot.border_right_color);
        self.border_colors.bottom = Box::new(snapshot.border_bottom_color);
        self.border_colors.left = Box::new(snapshot.border_left_color);
        self.has_style_snapshot = true;
        if let Some(transition_snapshot) = snapshot.transition_snapshot {
            self.has_layout_snapshot = transition_snapshot.has_layout_snapshot;
            self.layout_transition_visual_offset_x =
                transition_snapshot.layout_transition_visual_offset_x;
            self.layout_transition_visual_offset_y =
                transition_snapshot.layout_transition_visual_offset_y;
            self.layout_transition_override_width =
                transition_snapshot.layout_transition_override_width;
            self.layout_transition_override_height =
                transition_snapshot.layout_transition_override_height;
            self.layout_transition_target_x = transition_snapshot.layout_transition_target_x;
            self.layout_transition_target_y = transition_snapshot.layout_transition_target_y;
            self.layout_transition_target_width =
                transition_snapshot.layout_transition_target_width;
            self.layout_transition_target_height =
                transition_snapshot.layout_transition_target_height;
            self.last_parent_layout_x = transition_snapshot.last_parent_layout_x;
            self.last_parent_layout_y = transition_snapshot.last_parent_layout_y;
        }

        // Recompute against current parsed_style so transitions can bridge from old -> new style.
        self.recompute_style();
        true
    }
}

impl EventTarget for Element {
    fn dispatch_mouse_down(
        &mut self,
        event: &mut MouseDownEvent,
        control: &mut ViewportControl<'_>,
    ) {
        if self.handle_scrollbar_mouse_down(event, control) {
            event.meta.request_keep_focus();
            event.meta.stop_propagation();
            return;
        }
        for handler in &mut self.mouse_down_handlers {
            handler(event, control);
        }
    }

    fn dispatch_mouse_up(&mut self, event: &mut MouseUpEvent, control: &mut ViewportControl<'_>) {
        if self.handle_scrollbar_mouse_up(event, control) {
            event.meta.stop_propagation();
            return;
        }
        for handler in &mut self.mouse_up_handlers {
            handler(event, control);
        }
    }

    fn dispatch_mouse_move(
        &mut self,
        event: &mut MouseMoveEvent,
        control: &mut ViewportControl<'_>,
    ) {
        if self.handle_scrollbar_mouse_move(event, control) {
            event.meta.stop_propagation();
            return;
        }
        for handler in &mut self.mouse_move_handlers {
            handler(event, control);
        }
    }

    fn dispatch_click(&mut self, event: &mut ClickEvent, control: &mut ViewportControl<'_>) {
        if self.is_scrollbar_hit(event.mouse.local_x, event.mouse.local_y) {
            event.meta.stop_propagation();
            return;
        }
        for handler in &mut self.click_handlers {
            handler(event, control);
        }
    }

    fn dispatch_key_down(&mut self, event: &mut KeyDownEvent, _control: &mut ViewportControl<'_>) {
        for handler in &mut self.key_down_handlers {
            handler(event, _control);
        }
    }

    fn dispatch_key_up(&mut self, event: &mut KeyUpEvent, _control: &mut ViewportControl<'_>) {
        for handler in &mut self.key_up_handlers {
            handler(event, _control);
        }
    }

    fn dispatch_focus(&mut self, event: &mut FocusEvent, _control: &mut ViewportControl<'_>) {
        for handler in &mut self.focus_handlers {
            handler(event, _control);
        }
    }

    fn dispatch_blur(&mut self, event: &mut BlurEvent, _control: &mut ViewportControl<'_>) {
        for handler in &mut self.blur_handlers {
            handler(event, _control);
        }
    }

    fn cancel_pointer_interaction(&mut self) -> bool {
        self.scrollbar_drag.take().is_some()
    }

    fn set_hovered(&mut self, hovered: bool) -> bool {
        if self.is_hovered == hovered {
            return false;
        }
        self.is_hovered = hovered;
        if hovered {
            self.note_scrollbar_interaction();
        }
        self.recompute_style();
        true
    }

    fn scroll_by(&mut self, dx: f32, dy: f32) -> bool {
        let can_scroll = !matches!(self.scroll_direction, ScrollDirection::None);
        if !can_scroll {
            return false;
        }
        let max_scroll_x = (self.content_size.width - self.layout_inner_size.width).max(0.0);
        let max_scroll_y = (self.content_size.height - self.layout_inner_size.height).max(0.0);
        let mut next_x = self.scroll_offset.x;
        let mut next_y = self.scroll_offset.y;
        match self.scroll_direction {
            ScrollDirection::Horizontal => {
                next_x = (next_x + dx).clamp(0.0, max_scroll_x);
            }
            ScrollDirection::Vertical => {
                next_y = (next_y + dy).clamp(0.0, max_scroll_y);
            }
            ScrollDirection::Both => {
                next_x = (next_x + dx).clamp(0.0, max_scroll_x);
                next_y = (next_y + dy).clamp(0.0, max_scroll_y);
            }
            ScrollDirection::None => {}
        }
        let changed =
            !approx_eq(next_x, self.scroll_offset.x) || !approx_eq(next_y, self.scroll_offset.y);
        self.scroll_offset.x = next_x;
        self.scroll_offset.y = next_y;
        if changed {
            self.note_scrollbar_interaction();
        }
        changed || can_scroll
    }

    fn can_scroll_by(&self, dx: f32, dy: f32) -> bool {
        let can_scroll = !matches!(self.scroll_direction, ScrollDirection::None);
        if !can_scroll {
            return false;
        }
        let max_scroll_x = (self.content_size.width - self.layout_inner_size.width).max(0.0);
        let max_scroll_y = (self.content_size.height - self.layout_inner_size.height).max(0.0);
        let mut next_x = self.scroll_offset.x;
        let mut next_y = self.scroll_offset.y;
        match self.scroll_direction {
            ScrollDirection::Horizontal => {
                next_x = (next_x + dx).clamp(0.0, max_scroll_x);
            }
            ScrollDirection::Vertical => {
                next_y = (next_y + dy).clamp(0.0, max_scroll_y);
            }
            ScrollDirection::Both => {
                next_x = (next_x + dx).clamp(0.0, max_scroll_x);
                next_y = (next_y + dy).clamp(0.0, max_scroll_y);
            }
            ScrollDirection::None => {}
        }
        let changed =
            !approx_eq(next_x, self.scroll_offset.x) || !approx_eq(next_y, self.scroll_offset.y);
        changed || can_scroll
    }

    fn get_scroll_offset(&self) -> (f32, f32) {
        (self.scroll_offset.x, self.scroll_offset.y)
    }

    fn set_scroll_offset(&mut self, offset: (f32, f32)) {
        self.scroll_offset.x = offset.0;
        self.scroll_offset.y = offset.1;
    }

    fn cursor(&self) -> Cursor {
        self.computed_style.cursor
    }

    fn take_style_transition_requests(&mut self) -> Vec<StyleTrackRequest> {
        std::mem::take(&mut self.pending_style_transition_requests)
    }

    fn take_layout_transition_requests(&mut self) -> Vec<LayoutTrackRequest> {
        std::mem::take(&mut self.pending_layout_transition_requests)
    }

    fn take_visual_transition_requests(&mut self) -> Vec<VisualTrackRequest> {
        std::mem::take(&mut self.pending_visual_transition_requests)
    }
}

impl Layoutable for Element {
    fn measure(&mut self, constraints: LayoutConstraints) {
        let context = constraints.context();
        let proposal = LayoutProposal {
            width: context.width,
            height: context.height,
            viewport_width: context.viewport_width,
            viewport_height: context.viewport_height,
            percent_base_width: context.percent_base_width,
            percent_base_height: context.percent_base_height,
        };

        if !self.layout_dirty && self.last_layout_proposal == Some(proposal) {
            return;
        }

        self.measure_self(proposal);
        self.apply_size_constraints(proposal, false);

        // We should always measure children because they might be Auto or use Percent units
        // that depend on our inner size.
        let is_flex = matches!(
            self.computed_style.display,
            Display::Flow { .. } | Display::InlineFlex
        );
        if is_flex {
            self.measure_flex_children(proposal);
        } else {
            let bw_l = resolve_px_or_zero(
                self.computed_style.border_widths.left,
                proposal.percent_base_width,
                proposal.viewport_width,
                proposal.viewport_height,
            );
            let bw_r = resolve_px_or_zero(
                self.computed_style.border_widths.right,
                proposal.percent_base_width,
                proposal.viewport_width,
                proposal.viewport_height,
            );
            let bw_t = resolve_px_or_zero(
                self.computed_style.border_widths.top,
                proposal.percent_base_height,
                proposal.viewport_width,
                proposal.viewport_height,
            );
            let bw_b = resolve_px_or_zero(
                self.computed_style.border_widths.bottom,
                proposal.percent_base_height,
                proposal.viewport_width,
                proposal.viewport_height,
            );

            let p_l = resolve_px_or_zero(
                self.computed_style.padding.left,
                proposal.percent_base_width,
                proposal.viewport_width,
                proposal.viewport_height,
            );
            let p_r = resolve_px_or_zero(
                self.computed_style.padding.right,
                proposal.percent_base_width,
                proposal.viewport_width,
                proposal.viewport_height,
            );
            let p_t = resolve_px_or_zero(
                self.computed_style.padding.top,
                proposal.percent_base_height,
                proposal.viewport_width,
                proposal.viewport_height,
            );
            let p_b = resolve_px_or_zero(
                self.computed_style.padding.bottom,
                proposal.percent_base_height,
                proposal.viewport_width,
                proposal.viewport_height,
            );

            let (layout_w, layout_h) = self.current_layout_transition_size();
            let measure_w = if self.computed_style.width == SizeValue::Auto
                && proposal.percent_base_width.is_some()
            {
                proposal.width.max(0.0)
            } else {
                layout_w
            };
            let measure_h = if self.computed_style.height == SizeValue::Auto
                && proposal.percent_base_height.is_some()
            {
                proposal.height.max(0.0)
            } else {
                layout_h
            };
            let inner_w = (measure_w - bw_l - bw_r - p_l - p_r).max(0.0);
            let inner_h = (measure_h - bw_t - bw_b - p_t - p_b).max(0.0);

            let (child_available_width, child_available_height) = match self.scroll_direction {
                ScrollDirection::None => (inner_w, inner_h),
                ScrollDirection::Vertical => (inner_w, 1_000_000.0),
                ScrollDirection::Horizontal => (1_000_000.0, inner_h),
                ScrollDirection::Both => (1_000_000.0, 1_000_000.0),
            };

            let child_percent_base_width = if self.width_is_known(proposal) {
                Some(inner_w)
            } else {
                None
            };
            let child_percent_base_height = if self.height_is_known(proposal) {
                Some(inner_h)
            } else {
                None
            };

            for child in &mut self.children {
                child.measure(LayoutConstraints {
                    max_width: child_available_width,
                    max_height: child_available_height,
                    viewport_width: proposal.viewport_width,
                    viewport_height: proposal.viewport_height,
                    percent_base_width: child_percent_base_width,
                    percent_base_height: child_percent_base_height,
                });
            }

            if self.computed_style.width == SizeValue::Auto
                || self.computed_style.height == SizeValue::Auto
            {
                self.update_size_from_measured_children();
            }
        }
        self.apply_size_constraints(proposal, true);

        self.last_layout_proposal = Some(proposal);
        self.layout_dirty = false;
    }

    fn place(&mut self, placement: LayoutPlacement) {
        self.begin_place_scope(placement);
        let context = placement.context();
        let proposal = LayoutProposal {
            width: context.width,
            height: context.height,
            viewport_width: context.viewport_width,
            viewport_height: context.viewport_height,
            percent_base_width: context.percent_base_width,
            percent_base_height: context.percent_base_height,
        };
        self.resolve_lengths_from_parent_inner(proposal);
        self.place_self(
            proposal,
            placement.parent_x,
            placement.parent_y,
            placement.visual_offset_x,
            placement.visual_offset_y,
        );
        self.register_anchor_snapshot();
        self.resolve_corner_radii_from_self_box(proposal);
        let max_bw = (self
            .core
            .layout_size
            .width
            .min(self.core.layout_size.height))
            * 0.5;
        let border_left = self.border_widths.left.clamp(0.0, max_bw);
        let border_right = self.border_widths.right.clamp(0.0, max_bw);
        let border_top = self.border_widths.top.clamp(0.0, max_bw);
        let border_bottom = self.border_widths.bottom.clamp(0.0, max_bw);
        let inset_left = border_left + self.padding.left.max(0.0);
        let inset_right = border_right + self.padding.right.max(0.0);
        let inset_top = border_top + self.padding.top.max(0.0);
        let inset_bottom = border_bottom + self.padding.bottom.max(0.0);
        self.layout_flow_inner_position = Position {
            x: self.layout_flow_position.x + inset_left,
            y: self.layout_flow_position.y + inset_top,
        };
        self.layout_inner_position = Position {
            x: self.core.layout_position.x + inset_left,
            y: self.core.layout_position.y + inset_top,
        };
        self.layout_inner_size = Size {
            width: (self.core.layout_size.width - inset_left - inset_right).max(0.0),
            height: (self.core.layout_size.height - inset_top - inset_bottom).max(0.0),
        };

        let child_percent_base_width = if self.width_is_known(proposal) {
            Some(self.layout_inner_size.width.max(0.0))
        } else {
            None
        };
        let child_percent_base_height = if self.height_is_known(proposal) {
            Some(self.layout_inner_size.height.max(0.0))
        } else {
            None
        };
        self.place_children(
            proposal.viewport_width,
            proposal.viewport_height,
            child_percent_base_width,
            child_percent_base_height,
        );
        self.end_place_scope();
    }

    fn measured_size(&self) -> (f32, f32) {
        self.current_layout_transition_size()
    }

    fn set_layout_width(&mut self, width: f32) {
        self.core.set_width(width);
    }

    fn set_layout_height(&mut self, height: f32) {
        self.core.set_height(height);
    }

    fn set_layout_offset(&mut self, x: f32, y: f32) {
        self.core.set_position(x, y);
    }
}

impl Renderable for Element {
    fn build(&mut self, graph: &mut FrameGraph, ctx: &mut UiBuildContext) {
        if trace_layout_enabled() {
            eprintln!(
                "[build ] pos=({:.1},{:.1}) size=({:.1},{:.1}) should_render={}",
                self.core.layout_position.x,
                self.core.layout_position.y,
                self.core.layout_size.width,
                self.core.layout_size.height,
                self.core.should_render
            );
        }
        if !self.core.should_render {
            if self.has_absolute_descendant_for_hit_test {
                self.collect_root_viewport_deferred_descendants(ctx);
            }
            return;
        }

        let previous_scissor_rect = self
            .absolute_viewport_clip_scissor_rect()
            .map(|scissor| ctx.push_scissor_rect(Some(scissor)));

        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.core.layout_size.width.max(0.0),
            self.core.layout_size.height.max(0.0),
        );
        self.border_radius = outer_radii.max();
        let max_bw = (self
            .core
            .layout_size
            .width
            .min(self.core.layout_size.height))
            * 0.5;
        // Rounded corners are already handled by DrawRectPass. Keep a layer only when
        // element-level opacity is needed, otherwise we can avoid an extra composite mask.
        let use_layer = self.opacity < 1.0;

        let previous_color_target = ctx.color_target();
        let layer = if use_layer {
            let layer = ctx.allocate_layer(graph);
            let Some(layer_handle) = layer.handle() else {
                if let Some(previous) = previous_scissor_rect {
                    ctx.restore_scissor_rect(previous);
                }
                return;
            };
            ctx.set_color_target(Some(layer_handle));
            let clear = ClearPass::new([0.0, 0.0, 0.0, 0.0]);
            self.push_pass(graph, ctx, clear);
            self.build_self(graph, ctx, true);
            Some(layer)
        } else {
            self.build_self(graph, ctx, false);
            None
        };

        let inset_left = self.border_widths.left.clamp(0.0, max_bw) + self.padding.left.max(0.0);
        let inset_right = self.border_widths.right.clamp(0.0, max_bw) + self.padding.right.max(0.0);
        let inset_top = self.border_widths.top.clamp(0.0, max_bw) + self.padding.top.max(0.0);
        let inset_bottom =
            self.border_widths.bottom.clamp(0.0, max_bw) + self.padding.bottom.max(0.0);
        let inner_clip_radii = normalize_corner_radii(
            inset_corner_radii(
                outer_radii,
                inset_left,
                inset_right,
                inset_top,
                inset_bottom,
            ),
            self.layout_inner_size.width.max(0.0),
            self.layout_inner_size.height.max(0.0),
        );

        let overflow_child_indices: Vec<usize> = (0..self.children.len())
            .filter(|&idx| self.child_renders_outside_inner_clip(idx))
            .collect();

        if self.layout_inner_size.width > 0.0 && self.layout_inner_size.height > 0.0 {
            let previous_color_target = ctx.color_target();
            let layer = ctx.allocate_layer(graph);
            let Some(layer_handle) = layer.handle() else {
                if let Some(previous) = previous_scissor_rect {
                    ctx.restore_scissor_rect(previous);
                }
                return;
            };
            ctx.set_color_target(Some(layer_handle));

            let clear = ClearPass::new([0.0, 0.0, 0.0, 0.0]);
            self.push_pass(graph, ctx, clear);

            for (idx, child) in self.children.iter_mut().enumerate() {
                if overflow_child_indices.contains(&idx) {
                    continue;
                }
                child.build(graph, ctx);
            }

            ctx.set_color_target(previous_color_target);
            let composite = CompositeLayerPass::new(
                [self.layout_inner_position.x, self.layout_inner_position.y],
                [self.layout_inner_size.width, self.layout_inner_size.height],
                inner_clip_radii.to_array(),
                1.0,
                layer,
            );
            ctx.push_pass(graph, composite);
        }

        for idx in overflow_child_indices {
            if let Some(child) = self.children.get_mut(idx) {
                if child
                    .as_any()
                    .downcast_ref::<Element>()
                    .is_some_and(Element::should_append_to_root_viewport_render)
                {
                    ctx.append_to_defer(child.id());
                    continue;
                }
                child.build(graph, ctx);
            }
        }
        self.render_scrollbars(graph, ctx);

        if let Some(layer) = layer {
            ctx.set_color_target(previous_color_target);
            // build_self() already rasterizes the element silhouette (including rounded corners).
            // Applying another rounded mask here can introduce visible edge artifacts under
            // nested opacity compositing, so keep this composite pass unmasked.
            let composite = CompositeLayerPass::new(
                [self.core.layout_position.x, self.core.layout_position.y],
                [self.core.layout_size.width, self.core.layout_size.height],
                [0.0; 4],
                self.opacity.clamp(0.0, 1.0),
                layer,
            );
            ctx.push_pass(graph, composite);
        }

        if let Some(previous) = previous_scissor_rect {
            ctx.restore_scissor_rect(previous);
        }
    }
}

impl Element {
    const SHOULD_RENDER_OVERSCAN_PX: f32 = 24.0;

    fn absolute_viewport_clip_scissor_rect(&self) -> Option<[u32; 4]> {
        if self.computed_style.position.mode() != PositionMode::Absolute {
            return None;
        }
        if self.computed_style.position.clip_mode() != ClipMode::Viewport {
            return None;
        }
        let (viewport_w, viewport_h) = self
            .viewport_size_from_runtime(self.core.layout_size.width, self.core.layout_size.height);
        rect_to_scissor_rect(Rect {
            x: 0.0,
            y: 0.0,
            width: viewport_w.max(0.0),
            height: viewport_h.max(0.0),
        })
    }

    fn current_layout_transition_size(&self) -> (f32, f32) {
        (
            self.layout_transition_override_width
                .unwrap_or(self.core.size.width)
                .max(0.0),
            self.layout_transition_override_height
                .unwrap_or(self.core.size.height)
                .max(0.0),
        )
    }

    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self::new_with_id(0, x, y, width, height)
    }

    pub fn new_with_id(id: u64, x: f32, y: f32, width: f32, height: f32) -> Self {
        let mut style = Style::new();
        style.insert(
            crate::style::PropertyId::Width,
            crate::style::ParsedValue::Length(Length::px(width)),
        );
        style.insert(
            crate::style::PropertyId::Height,
            crate::style::ParsedValue::Length(Length::px(height)),
        );

        let mut el = Element {
            core: if id == 0 {
                ElementCore::new(x, y, width, height)
            } else {
                ElementCore::new_with_id(id, x, y, width, height)
            },
            anchor_name: None,
            layout_flow_position: Position { x, y },
            layout_inner_position: Position { x, y },
            layout_flow_inner_position: Position { x, y },
            layout_inner_size: Size {
                width: width.max(0.0),
                height: height.max(0.0),
            },
            parsed_style: style,
            computed_style: ComputedStyle::default(),
            padding: EdgeInsets {
                left: 0.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
            },
            background_color: Box::new(Color::hex("#FFFFFF")),
            border_colors: EdgeColors {
                left: Box::new(Color::hex("#000000")),
                right: Box::new(Color::hex("#000000")),
                top: Box::new(Color::hex("#000000")),
                bottom: Box::new(Color::hex("#000000")),
            },
            border_widths: EdgeInsets {
                left: 0.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
            },
            border_radii: CornerRadii::zero(),
            border_radius: 0.0,
            box_shadows: Vec::new(),
            foreground_color: Color::rgb(0, 0, 0),
            opacity: 1.0,
            scroll_direction: ScrollDirection::None,
            scroll_offset: Position { x: 0.0, y: 0.0 },
            content_size: Size {
                width: 0.0,
                height: 0.0,
            },
            scrollbar_drag: None,
            last_scrollbar_interaction: None,
            scrollbar_shadow_blur_radius: 3.0,
            pending_style_transition_requests: Vec::new(),
            pending_layout_transition_requests: Vec::new(),
            pending_visual_transition_requests: Vec::new(),
            has_style_snapshot: false,
            has_layout_snapshot: false,
            layout_transition_visual_offset_x: 0.0,
            layout_transition_visual_offset_y: 0.0,
            layout_transition_override_width: None,
            layout_transition_override_height: None,
            layout_transition_target_x: None,
            layout_transition_target_y: None,
            layout_transition_target_width: None,
            layout_transition_target_height: None,
            last_parent_layout_x: x,
            last_parent_layout_y: y,
            is_hovered: false,
            mouse_down_handlers: Vec::new(),
            mouse_up_handlers: Vec::new(),
            mouse_move_handlers: Vec::new(),
            click_handlers: Vec::new(),
            key_down_handlers: Vec::new(),
            key_up_handlers: Vec::new(),
            focus_handlers: Vec::new(),
            blur_handlers: Vec::new(),
            layout_dirty: true,
            last_layout_proposal: None,
            flex_info: None,
            has_absolute_descendant_for_hit_test: false,
            children: Vec::new(),
        };
        el.recompute_style();
        // Initial mount should not animate from constructor defaults to first user style.
        el.has_style_snapshot = false;
        el
    }

    pub fn set_position(&mut self, x: f32, y: f32) {
        self.core.set_position(x, y);
    }

    pub fn set_anchor_name(&mut self, name: Option<AnchorName>) {
        self.anchor_name = name;
    }

    pub fn set_x(&mut self, x: f32) {
        self.core.set_x(x);
    }

    pub fn set_y(&mut self, y: f32) {
        self.core.set_y(y);
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        self.core.set_size(width, height);
        self.layout_dirty = true;
    }

    pub fn set_scrollbar_shadow_blur_radius(&mut self, radius: f32) {
        self.scrollbar_shadow_blur_radius = radius.max(0.0);
    }

    pub fn set_width(&mut self, width: f32) {
        self.core.set_width(width);
        self.layout_dirty = true;
    }

    pub fn set_height(&mut self, height: f32) {
        self.core.set_height(height);
        self.layout_dirty = true;
    }

    pub fn mark_layout_dirty(&mut self) {
        self.layout_dirty = true;
    }

    pub fn set_background_color<T: ColorLike + 'static>(&mut self, color: T) {
        self.background_color = Box::new(color);
    }

    pub fn set_background_color_value(&mut self, color: Color) {
        self.background_color = Box::new(color);
    }

    pub fn set_foreground_color(&mut self, color: Color) {
        self.foreground_color = color;
    }

    pub fn set_layout_transition_x(&mut self, value: f32) {
        self.layout_transition_visual_offset_x = value;
    }

    pub fn set_layout_transition_y(&mut self, value: f32) {
        self.layout_transition_visual_offset_y = value;
    }

    pub fn set_layout_transition_width(&mut self, value: f32) {
        self.layout_transition_override_width = Some(value.max(0.0));
        self.layout_dirty = true;
    }

    pub fn set_layout_transition_height(&mut self, value: f32) {
        self.layout_transition_override_height = Some(value.max(0.0));
        self.layout_dirty = true;
    }

    pub fn seed_layout_transition_snapshot(
        &mut self,
        layout_x: f32,
        layout_y: f32,
        layout_width: f32,
        layout_height: f32,
        parent_layout_x: f32,
        parent_layout_y: f32,
    ) {
        self.core.layout_position = Position {
            x: layout_x,
            y: layout_y,
        };
        self.layout_flow_position = Position {
            x: layout_x,
            y: layout_y,
        };
        self.core.layout_size = Size {
            width: layout_width.max(0.0),
            height: layout_height.max(0.0),
        };
        self.last_parent_layout_x = parent_layout_x;
        self.last_parent_layout_y = parent_layout_y;
        self.has_layout_snapshot = true;
    }

    pub fn set_border_top_color(&mut self, color: Color) {
        self.border_colors.top = Box::new(color);
    }

    pub fn set_border_right_color(&mut self, color: Color) {
        self.border_colors.right = Box::new(color);
    }

    pub fn set_border_bottom_color(&mut self, color: Color) {
        self.border_colors.bottom = Box::new(color);
    }

    pub fn set_border_left_color(&mut self, color: Color) {
        self.border_colors.left = Box::new(color);
    }

    pub fn set_border_radius(&mut self, radius: f32) {
        let radius = radius.max(0.0);
        self.border_radii = CornerRadii::uniform(radius);
        self.border_radius = radius;
        self.layout_dirty = true;
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity;
    }

    pub fn set_padding(&mut self, value: f32) {
        let value = value.max(0.0);
        self.padding = EdgeInsets {
            left: value,
            right: value,
            top: value,
            bottom: value,
        };
        self.layout_dirty = true;
    }

    pub fn set_padding_x(&mut self, value: f32) {
        let value = value.max(0.0);
        self.padding.left = value;
        self.padding.right = value;
        self.layout_dirty = true;
    }

    pub fn set_padding_y(&mut self, value: f32) {
        let value = value.max(0.0);
        self.padding.top = value;
        self.padding.bottom = value;
        self.layout_dirty = true;
    }

    pub fn set_padding_left(&mut self, value: f32) {
        self.padding.left = value.max(0.0);
        self.layout_dirty = true;
    }

    pub fn set_padding_right(&mut self, value: f32) {
        self.padding.right = value.max(0.0);
        self.layout_dirty = true;
    }

    pub fn set_padding_top(&mut self, value: f32) {
        self.padding.top = value.max(0.0);
        self.layout_dirty = true;
    }

    pub fn set_padding_bottom(&mut self, value: f32) {
        self.padding.bottom = value.max(0.0);
        self.layout_dirty = true;
    }

    pub fn apply_style(&mut self, style: Style) {
        self.parsed_style = self.parsed_style.clone() + style;
        self.recompute_style();
    }

    fn recompute_style(&mut self) {
        let prev_opacity = self.opacity;
        let prev_border_radius = self.border_radius;
        let prev_background_color = self.background_color.as_ref().to_rgba_u8();
        let prev_foreground_color = self.foreground_color;
        let prev_border_top_color = self.border_colors.top.as_ref().to_rgba_u8();
        let prev_border_right_color = self.border_colors.right.as_ref().to_rgba_u8();
        let prev_border_bottom_color = self.border_colors.bottom.as_ref().to_rgba_u8();
        let prev_border_left_color = self.border_colors.left.as_ref().to_rgba_u8();
        let had_snapshot = self.has_style_snapshot;
        let effective_style = if self.is_hovered {
            match self.parsed_style.hover() {
                Some(hover_style) => self.parsed_style.clone() + hover_style.clone(),
                None => self.parsed_style.clone(),
            }
        } else {
            self.parsed_style.clone()
        };
        self.computed_style = compute_style(&effective_style, None);
        self.sync_props_from_computed_style();
        if had_snapshot {
            self.collect_style_transition_requests(
                prev_opacity,
                prev_border_radius,
                Color::rgba(
                    prev_background_color[0],
                    prev_background_color[1],
                    prev_background_color[2],
                    prev_background_color[3],
                ),
                prev_foreground_color,
                Color::rgba(
                    prev_border_top_color[0],
                    prev_border_top_color[1],
                    prev_border_top_color[2],
                    prev_border_top_color[3],
                ),
                Color::rgba(
                    prev_border_right_color[0],
                    prev_border_right_color[1],
                    prev_border_right_color[2],
                    prev_border_right_color[3],
                ),
                Color::rgba(
                    prev_border_bottom_color[0],
                    prev_border_bottom_color[1],
                    prev_border_bottom_color[2],
                    prev_border_bottom_color[3],
                ),
                Color::rgba(
                    prev_border_left_color[0],
                    prev_border_left_color[1],
                    prev_border_left_color[2],
                    prev_border_left_color[3],
                ),
            );
        }
        self.has_style_snapshot = true;
        self.layout_dirty = true;
    }

    fn collect_style_transition_requests(
        &mut self,
        prev_opacity: f32,
        prev_border_radius: f32,
        prev_background_color: Color,
        prev_foreground_color: Color,
        prev_border_top_color: Color,
        prev_border_right_color: Color,
        prev_border_bottom_color: Color,
        prev_border_left_color: Color,
    ) {
        let next_opacity = self.opacity;
        let next_border_radius = self.border_radius;
        let [bg_r, bg_g, bg_b, bg_a] = self.background_color.as_ref().to_rgba_u8();
        let next_background_color = Color::rgba(bg_r, bg_g, bg_b, bg_a);
        let next_foreground_color = self.foreground_color;
        let [bt_r, bt_g, bt_b, bt_a] = self.border_colors.top.as_ref().to_rgba_u8();
        let [br_r, br_g, br_b, br_a] = self.border_colors.right.as_ref().to_rgba_u8();
        let [bb_r, bb_g, bb_b, bb_a] = self.border_colors.bottom.as_ref().to_rgba_u8();
        let [bl_r, bl_g, bl_b, bl_a] = self.border_colors.left.as_ref().to_rgba_u8();
        let next_border_top_color = Color::rgba(bt_r, bt_g, bt_b, bt_a);
        let next_border_right_color = Color::rgba(br_r, br_g, br_b, br_a);
        let next_border_bottom_color = Color::rgba(bb_r, bb_g, bb_b, bb_a);
        let next_border_left_color = Color::rgba(bl_r, bl_g, bl_b, bl_a);
        for transition in self.computed_style.transition.as_slice() {
            let runtime = RuntimeStyleTransition {
                duration_ms: transition.duration_ms,
                delay_ms: transition.delay_ms,
                timing: map_transition_timing(transition.timing),
            };
            match transition.property {
                TransitionProperty::All => {
                    if !approx_eq(prev_opacity, next_opacity) {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::Opacity,
                                from: StyleValue::Scalar(prev_opacity),
                                to: StyleValue::Scalar(next_opacity),
                                transition: runtime,
                            });
                    }
                    if !approx_eq(prev_border_radius, next_border_radius) {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderRadius,
                                from: StyleValue::Scalar(prev_border_radius),
                                to: StyleValue::Scalar(next_border_radius),
                                transition: runtime,
                            });
                    }
                    if prev_background_color != next_background_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BackgroundColor,
                                from: StyleValue::Color(prev_background_color),
                                to: StyleValue::Color(next_background_color),
                                transition: runtime,
                            });
                    }
                    if prev_foreground_color != next_foreground_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::Color,
                                from: StyleValue::Color(prev_foreground_color),
                                to: StyleValue::Color(next_foreground_color),
                                transition: runtime,
                            });
                    }
                    if prev_border_top_color != next_border_top_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderTopColor,
                                from: StyleValue::Color(prev_border_top_color),
                                to: StyleValue::Color(next_border_top_color),
                                transition: runtime,
                            });
                    }
                    if prev_border_right_color != next_border_right_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderRightColor,
                                from: StyleValue::Color(prev_border_right_color),
                                to: StyleValue::Color(next_border_right_color),
                                transition: runtime,
                            });
                    }
                    if prev_border_bottom_color != next_border_bottom_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderBottomColor,
                                from: StyleValue::Color(prev_border_bottom_color),
                                to: StyleValue::Color(next_border_bottom_color),
                                transition: runtime,
                            });
                    }
                    if prev_border_left_color != next_border_left_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderLeftColor,
                                from: StyleValue::Color(prev_border_left_color),
                                to: StyleValue::Color(next_border_left_color),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::Opacity => {
                    if !approx_eq(prev_opacity, next_opacity) {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::Opacity,
                                from: StyleValue::Scalar(prev_opacity),
                                to: StyleValue::Scalar(next_opacity),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::BorderRadius => {
                    if !approx_eq(prev_border_radius, next_border_radius) {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderRadius,
                                from: StyleValue::Scalar(prev_border_radius),
                                to: StyleValue::Scalar(next_border_radius),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::BackgroundColor => {
                    if prev_background_color != next_background_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BackgroundColor,
                                from: StyleValue::Color(prev_background_color),
                                to: StyleValue::Color(next_background_color),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::Color => {
                    if prev_foreground_color != next_foreground_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::Color,
                                from: StyleValue::Color(prev_foreground_color),
                                to: StyleValue::Color(next_foreground_color),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::BorderColor => {
                    if prev_border_top_color != next_border_top_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderTopColor,
                                from: StyleValue::Color(prev_border_top_color),
                                to: StyleValue::Color(next_border_top_color),
                                transition: runtime,
                            });
                    }
                    if prev_border_right_color != next_border_right_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderRightColor,
                                from: StyleValue::Color(prev_border_right_color),
                                to: StyleValue::Color(next_border_right_color),
                                transition: runtime,
                            });
                    }
                    if prev_border_bottom_color != next_border_bottom_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderBottomColor,
                                from: StyleValue::Color(prev_border_bottom_color),
                                to: StyleValue::Color(next_border_bottom_color),
                                transition: runtime,
                            });
                    }
                    if prev_border_left_color != next_border_left_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderLeftColor,
                                from: StyleValue::Color(prev_border_left_color),
                                to: StyleValue::Color(next_border_left_color),
                                transition: runtime,
                            });
                    }
                }
                _ => {}
            }
        }
    }

    fn note_scrollbar_interaction(&mut self) {
        self.last_scrollbar_interaction = Some(Instant::now());
    }

    fn max_scroll(&self) -> (f32, f32) {
        (
            (self.content_size.width - self.layout_inner_size.width).max(0.0),
            (self.content_size.height - self.layout_inner_size.height).max(0.0),
        )
    }

    fn local_inner_origin(&self) -> (f32, f32) {
        (
            self.layout_inner_position.x - self.core.layout_position.x,
            self.layout_inner_position.y - self.core.layout_position.y,
        )
    }

    fn scrollbar_visibility_alpha(&self) -> f32 {
        const HOLD: Duration = Duration::from_millis(900);
        const FADE: Duration = Duration::from_millis(350);
        if self.scrollbar_drag.is_some() {
            return 1.0;
        }
        let (max_x, max_y) = self.max_scroll();
        if max_x <= 0.0 && max_y <= 0.0 {
            return 0.0;
        }
        if self.is_hovered {
            return 1.0;
        }
        let Some(last) = self.last_scrollbar_interaction else {
            return 0.0;
        };
        let elapsed = last.elapsed();
        if elapsed <= HOLD {
            return 1.0;
        }
        let fade_elapsed = elapsed - HOLD;
        if fade_elapsed >= FADE {
            return 0.0;
        }
        1.0 - (fade_elapsed.as_secs_f32() / FADE.as_secs_f32())
    }

    fn scrollbar_geometry(&self, inner_x: f32, inner_y: f32) -> ScrollbarGeometry {
        const THICKNESS: f32 = 6.0;
        const MARGIN: f32 = 3.0;
        const MIN_THUMB: f32 = 24.0;

        let mut geometry = ScrollbarGeometry::default();
        let (max_scroll_x, max_scroll_y) = self.max_scroll();
        let can_scroll_x = matches!(
            self.scroll_direction,
            ScrollDirection::Horizontal | ScrollDirection::Both
        ) && max_scroll_x > 0.0;
        let can_scroll_y = matches!(
            self.scroll_direction,
            ScrollDirection::Vertical | ScrollDirection::Both
        ) && max_scroll_y > 0.0;

        let reserve_v = if can_scroll_y {
            THICKNESS + MARGIN
        } else {
            0.0
        };
        let reserve_h = if can_scroll_x {
            THICKNESS + MARGIN
        } else {
            0.0
        };

        if can_scroll_y {
            let track_x = inner_x + self.layout_inner_size.width - THICKNESS - MARGIN;
            let track_y = inner_y + MARGIN;
            let track_h = (self.layout_inner_size.height - MARGIN * 2.0 - reserve_h).max(0.0);
            if track_h > 0.0 {
                let track = Rect {
                    x: track_x,
                    y: track_y,
                    width: THICKNESS,
                    height: track_h,
                };
                let ratio = (self.layout_inner_size.height / self.content_size.height.max(1.0))
                    .clamp(0.0, 1.0);
                let thumb_h = (track_h * ratio).clamp(MIN_THUMB.min(track_h), track_h);
                let travel = (track_h - thumb_h).max(0.0);
                let thumb_offset = if max_scroll_y > 0.0 {
                    (self.scroll_offset.y / max_scroll_y).clamp(0.0, 1.0) * travel
                } else {
                    0.0
                };
                geometry.vertical_track = Some(track);
                geometry.vertical_thumb = Some(Rect {
                    x: track.x,
                    y: track.y + thumb_offset,
                    width: track.width,
                    height: thumb_h,
                });
            }
        }

        if can_scroll_x {
            let track_x = inner_x + MARGIN;
            let track_y = inner_y + self.layout_inner_size.height - THICKNESS - MARGIN;
            let track_w = (self.layout_inner_size.width - MARGIN * 2.0 - reserve_v).max(0.0);
            if track_w > 0.0 {
                let track = Rect {
                    x: track_x,
                    y: track_y,
                    width: track_w,
                    height: THICKNESS,
                };
                let ratio = (self.layout_inner_size.width / self.content_size.width.max(1.0))
                    .clamp(0.0, 1.0);
                let thumb_w = (track_w * ratio).clamp(MIN_THUMB.min(track_w), track_w);
                let travel = (track_w - thumb_w).max(0.0);
                let thumb_offset = if max_scroll_x > 0.0 {
                    (self.scroll_offset.x / max_scroll_x).clamp(0.0, 1.0) * travel
                } else {
                    0.0
                };
                geometry.horizontal_track = Some(track);
                geometry.horizontal_thumb = Some(Rect {
                    x: track.x + thumb_offset,
                    y: track.y,
                    width: thumb_w,
                    height: track.height,
                });
            }
        }

        geometry
    }

    fn update_scroll_from_drag(
        &mut self,
        axis: ScrollbarAxis,
        mouse_local_x: f32,
        mouse_local_y: f32,
        grab_offset: f32,
    ) -> bool {
        let Some(next_scroll) =
            self.scroll_value_from_drag(axis, mouse_local_x, mouse_local_y, grab_offset)
        else {
            return false;
        };
        let current_scroll = match axis {
            ScrollbarAxis::Vertical => self.scroll_offset.y,
            ScrollbarAxis::Horizontal => self.scroll_offset.x,
        };
        let changed = !approx_eq(next_scroll, current_scroll);
        match axis {
            ScrollbarAxis::Vertical => self.scroll_offset.y = next_scroll,
            ScrollbarAxis::Horizontal => self.scroll_offset.x = next_scroll,
        }
        changed
    }

    fn scroll_value_from_drag(
        &self,
        axis: ScrollbarAxis,
        mouse_local_x: f32,
        mouse_local_y: f32,
        grab_offset: f32,
    ) -> Option<f32> {
        let (inner_x, inner_y) = self.local_inner_origin();
        let geometry = self.scrollbar_geometry(inner_x, inner_y);
        let (track, thumb) = match axis {
            ScrollbarAxis::Vertical => (geometry.vertical_track, geometry.vertical_thumb),
            ScrollbarAxis::Horizontal => (geometry.horizontal_track, geometry.horizontal_thumb),
        };
        let (Some(track), Some(thumb)) = (track, thumb) else {
            return None;
        };
        let (mouse_axis, track_axis, track_len, thumb_len, max_scroll) = match axis {
            ScrollbarAxis::Vertical => (
                mouse_local_y,
                track.y,
                track.height,
                thumb.height,
                self.max_scroll().1,
            ),
            ScrollbarAxis::Horizontal => (
                mouse_local_x,
                track.x,
                track.width,
                thumb.width,
                self.max_scroll().0,
            ),
        };
        if track_len <= 0.0 || max_scroll <= 0.0 {
            return None;
        }
        let travel = (track_len - thumb_len).max(0.0);
        if travel <= 0.0 {
            return None;
        }
        let thumb_start = (mouse_axis - grab_offset).clamp(track_axis, track_axis + travel);
        let ratio = ((thumb_start - track_axis) / travel).clamp(0.0, 1.0);
        Some(ratio * max_scroll)
    }

    fn is_scrollbar_hit(&self, local_x: f32, local_y: f32) -> bool {
        if self.scrollbar_visibility_alpha() <= 0.0 {
            return false;
        }
        let (inner_x, inner_y) = self.local_inner_origin();
        let geometry = self.scrollbar_geometry(inner_x, inner_y);
        geometry
            .vertical_track
            .is_some_and(|track| track.contains(local_x, local_y))
            || geometry
                .vertical_thumb
                .is_some_and(|thumb| thumb.contains(local_x, local_y))
            || geometry
                .horizontal_track
                .is_some_and(|track| track.contains(local_x, local_y))
            || geometry
                .horizontal_thumb
                .is_some_and(|thumb| thumb.contains(local_x, local_y))
    }

    fn handle_scrollbar_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        control: &mut ViewportControl<'_>,
    ) -> bool {
        if event.mouse.button != Some(UiMouseButton::Left) {
            return false;
        }
        if !self.is_scrollbar_hit(event.mouse.local_x, event.mouse.local_y) {
            return false;
        }
        let local_x = event.mouse.local_x;
        let local_y = event.mouse.local_y;
        let (inner_x, inner_y) = self.local_inner_origin();
        let geometry = self.scrollbar_geometry(inner_x, inner_y);

        if let Some(thumb) = geometry.vertical_thumb {
            if thumb.contains(local_x, local_y) {
                control.cancel_scroll_track(self.core.id, ScrollAxis::Y);
                self.scrollbar_drag = Some(ScrollbarDragState {
                    axis: ScrollbarAxis::Vertical,
                    grab_offset: local_y - thumb.y,
                    reanchor_on_first_move: false,
                });
                control.set_pointer_capture(self.core.id);
                self.note_scrollbar_interaction();
                return true;
            }
        }
        if let Some(track) = geometry.vertical_track {
            if track.contains(local_x, local_y) {
                let grab = geometry
                    .vertical_thumb
                    .map(|thumb| thumb.height * 0.5)
                    .unwrap_or(0.0);
                if let Some(to) =
                    self.scroll_value_from_drag(ScrollbarAxis::Vertical, local_x, local_y, grab)
                {
                    let from = self.scroll_offset.y;
                    let _ = control.start_scroll_track(self.core.id, ScrollAxis::Y, from, to);
                }
                self.scrollbar_drag = Some(ScrollbarDragState {
                    axis: ScrollbarAxis::Vertical,
                    grab_offset: grab,
                    reanchor_on_first_move: true,
                });
                control.set_pointer_capture(self.core.id);
                self.note_scrollbar_interaction();
                return true;
            }
        }

        if let Some(thumb) = geometry.horizontal_thumb {
            if thumb.contains(local_x, local_y) {
                control.cancel_scroll_track(self.core.id, ScrollAxis::X);
                self.scrollbar_drag = Some(ScrollbarDragState {
                    axis: ScrollbarAxis::Horizontal,
                    grab_offset: local_x - thumb.x,
                    reanchor_on_first_move: false,
                });
                control.set_pointer_capture(self.core.id);
                self.note_scrollbar_interaction();
                return true;
            }
        }
        if let Some(track) = geometry.horizontal_track {
            if track.contains(local_x, local_y) {
                let grab = geometry
                    .horizontal_thumb
                    .map(|thumb| thumb.width * 0.5)
                    .unwrap_or(0.0);
                if let Some(to) =
                    self.scroll_value_from_drag(ScrollbarAxis::Horizontal, local_x, local_y, grab)
                {
                    let from = self.scroll_offset.x;
                    let _ = control.start_scroll_track(self.core.id, ScrollAxis::X, from, to);
                }
                self.scrollbar_drag = Some(ScrollbarDragState {
                    axis: ScrollbarAxis::Horizontal,
                    grab_offset: grab,
                    reanchor_on_first_move: true,
                });
                control.set_pointer_capture(self.core.id);
                self.note_scrollbar_interaction();
                return true;
            }
        }
        false
    }

    fn handle_scrollbar_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        control: &mut ViewportControl<'_>,
    ) -> bool {
        if let Some(drag) = self.scrollbar_drag {
            let mut drag = drag;
            match drag.axis {
                ScrollbarAxis::Vertical => control.cancel_scroll_track(self.core.id, ScrollAxis::Y),
                ScrollbarAxis::Horizontal => {
                    control.cancel_scroll_track(self.core.id, ScrollAxis::X)
                }
            }
            if drag.reanchor_on_first_move {
                let (inner_x, inner_y) = self.local_inner_origin();
                let geometry = self.scrollbar_geometry(inner_x, inner_y);
                drag.grab_offset = match drag.axis {
                    ScrollbarAxis::Vertical => geometry
                        .vertical_thumb
                        .map(|thumb| (event.mouse.local_y - thumb.y).clamp(0.0, thumb.height))
                        .unwrap_or(drag.grab_offset),
                    ScrollbarAxis::Horizontal => geometry
                        .horizontal_thumb
                        .map(|thumb| (event.mouse.local_x - thumb.x).clamp(0.0, thumb.width))
                        .unwrap_or(drag.grab_offset),
                };
                drag.reanchor_on_first_move = false;
                self.scrollbar_drag = Some(drag);
            }
            let changed = self.update_scroll_from_drag(
                drag.axis,
                event.mouse.local_x,
                event.mouse.local_y,
                drag.grab_offset,
            );
            if changed {
                self.note_scrollbar_interaction();
            }
            return true;
        }
        if self.scrollbar_visibility_alpha() <= 0.0 {
            return false;
        }
        let (inner_x, inner_y) = self.local_inner_origin();
        let geometry = self.scrollbar_geometry(inner_x, inner_y);
        let local_x = event.mouse.local_x;
        let local_y = event.mouse.local_y;
        if geometry
            .vertical_thumb
            .is_some_and(|thumb| thumb.contains(local_x, local_y))
            || geometry
                .horizontal_thumb
                .is_some_and(|thumb| thumb.contains(local_x, local_y))
        {
            self.note_scrollbar_interaction();
        }
        false
    }

    fn handle_scrollbar_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        control: &mut ViewportControl<'_>,
    ) -> bool {
        if event.mouse.button != Some(UiMouseButton::Left) {
            return false;
        }
        if self.scrollbar_drag.take().is_some() {
            control.release_pointer_capture(self.core.id);
            self.note_scrollbar_interaction();
            return true;
        }
        if self.is_scrollbar_hit(event.mouse.local_x, event.mouse.local_y) {
            self.note_scrollbar_interaction();
            return true;
        }
        false
    }

    fn render_scrollbar_shadow(
        &mut self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        rect: Rect,
        border_radius: f32,
        color: [f32; 4],
    ) {
        let mesh = ShadowMesh::rounded_rect(
            rect.x,
            rect.y,
            rect.width.max(0.0),
            rect.height.max(0.0),
            border_radius.max(0.0),
        );
        let pass = ShadowPass::new(
            mesh,
            ShadowParams {
                offset_x: 1.0,
                offset_y: 1.0,
                blur_radius: self.scrollbar_shadow_blur_radius.max(0.0),
                color,
                opacity: 1.0,
                spread: 0.0,
                clip_to_geometry: true,
            },
        );
        self.push_pass(graph, ctx, pass);
    }

    fn render_scrollbars(&mut self, graph: &mut FrameGraph, ctx: &mut UiBuildContext) {
        let alpha = self.scrollbar_visibility_alpha();
        if alpha <= 0.0 {
            return;
        }
        const TRACK_SHADOW_ALPHA: f32 = 0.5;
        const THUMB_SHADOW_ALPHA: f32 = 0.5;
        let geometry =
            self.scrollbar_geometry(self.layout_inner_position.x, self.layout_inner_position.y);
        let track_alpha = (0.35 * alpha).clamp(0.0, 1.0);
        let thumb_alpha = (0.58 * alpha).clamp(0.0, 1.0);
        let track_shadow_alpha = (TRACK_SHADOW_ALPHA * alpha).clamp(0.0, 1.0);
        let thumb_shadow_alpha = (THUMB_SHADOW_ALPHA * alpha).clamp(0.0, 1.0);
        let track_shadow_color = [0.0, 0.0, 0.0, track_shadow_alpha];
        let thumb_shadow_color = [0.0, 0.0, 0.0, thumb_shadow_alpha];
        let track_color = [0.95, 0.95, 0.95, track_alpha];
        let thumb_color = [0.95, 0.95, 0.95, thumb_alpha];
        if let Some(track) = geometry.vertical_track {
            self.render_scrollbar_shadow(
                graph,
                ctx,
                track,
                (track.width * 0.5).max(0.0),
                track_shadow_color,
            );

            let mut pass = DrawRectPass::new(
                [track.x, track.y],
                [track.width, track.height],
                track_color,
                1.0,
            );
            pass.set_border_width(0.0);
            pass.set_border_radius((track.width * 0.5).max(0.0));
            self.push_pass(graph, ctx, pass);
        }
        if let Some(track) = geometry.horizontal_track {
            self.render_scrollbar_shadow(
                graph,
                ctx,
                track,
                (track.height * 0.5).max(0.0),
                track_shadow_color,
            );

            let mut pass = DrawRectPass::new(
                [track.x, track.y],
                [track.width, track.height],
                track_color,
                1.0,
            );
            pass.set_border_width(0.0);
            pass.set_border_radius((track.height * 0.5).max(0.0));
            self.push_pass(graph, ctx, pass);
        }
        if let Some(thumb) = geometry.vertical_thumb {
            self.render_scrollbar_shadow(
                graph,
                ctx,
                thumb,
                (thumb.width * 0.5).max(0.0),
                thumb_shadow_color,
            );

            let mut pass = DrawRectPass::new(
                [thumb.x, thumb.y],
                [thumb.width, thumb.height],
                thumb_color,
                1.0,
            );
            pass.set_border_width(0.0);
            pass.set_border_radius((thumb.width * 0.5).max(0.0));
            self.push_pass(graph, ctx, pass);
        }
        if let Some(thumb) = geometry.horizontal_thumb {
            self.render_scrollbar_shadow(
                graph,
                ctx,
                thumb,
                (thumb.height * 0.5).max(0.0),
                thumb_shadow_color,
            );

            let mut pass = DrawRectPass::new(
                [thumb.x, thumb.y],
                [thumb.width, thumb.height],
                thumb_color,
                1.0,
            );
            pass.set_border_width(0.0);
            pass.set_border_radius((thumb.height * 0.5).max(0.0));
            self.push_pass(graph, ctx, pass);
        }
    }

    pub fn on_mouse_down<F>(&mut self, handler: F)
    where
        F: FnMut(&mut MouseDownEvent, &mut ViewportControl<'_>) + 'static,
    {
        self.mouse_down_handlers.push(Box::new(handler));
    }

    pub fn on_mouse_up<F>(&mut self, handler: F)
    where
        F: FnMut(&mut MouseUpEvent, &mut ViewportControl<'_>) + 'static,
    {
        self.mouse_up_handlers.push(Box::new(handler));
    }

    pub fn on_mouse_move<F>(&mut self, handler: F)
    where
        F: FnMut(&mut MouseMoveEvent, &mut ViewportControl<'_>) + 'static,
    {
        self.mouse_move_handlers.push(Box::new(handler));
    }

    pub fn on_click<F>(&mut self, handler: F)
    where
        F: FnMut(&mut ClickEvent, &mut ViewportControl<'_>) + 'static,
    {
        self.click_handlers.push(Box::new(handler));
    }

    pub fn on_key_down<F>(&mut self, handler: F)
    where
        F: FnMut(&mut KeyDownEvent, &mut ViewportControl<'_>) + 'static,
    {
        self.key_down_handlers.push(Box::new(handler));
    }

    pub fn on_key_up<F>(&mut self, handler: F)
    where
        F: FnMut(&mut KeyUpEvent, &mut ViewportControl<'_>) + 'static,
    {
        self.key_up_handlers.push(Box::new(handler));
    }

    pub fn on_focus<F>(&mut self, handler: F)
    where
        F: FnMut(&mut FocusEvent, &mut ViewportControl<'_>) + 'static,
    {
        self.focus_handlers.push(Box::new(handler));
    }

    pub fn on_blur<F>(&mut self, handler: F)
    where
        F: FnMut(&mut BlurEvent, &mut ViewportControl<'_>) + 'static,
    {
        self.blur_handlers.push(Box::new(handler));
    }

    pub fn id(&self) -> u64 {
        self.core.id
    }

    pub fn parent_id(&self) -> Option<u64> {
        self.core.parent_id
    }

    pub(crate) fn child_layout_origin(&self) -> (f32, f32) {
        (
            self.layout_flow_inner_position.x - self.scroll_offset.x,
            self.layout_flow_inner_position.y - self.scroll_offset.y,
        )
    }

    pub fn add_child(&mut self, child: Box<dyn ElementTrait>) {
        let mut child = child;
        if child.parent_id() != Some(self.core.id) {
            child.set_parent_id(Some(self.core.id));
        }
        if let Some(element) = child.as_any().downcast_ref::<Element>() {
            self.has_absolute_descendant_for_hit_test |= element
                .is_absolute_positioned_for_hit_test()
                || element.has_absolute_descendant_for_hit_test;
        }
        self.children.push(child);
        self.layout_dirty = true;
    }

    pub(crate) fn has_absolute_descendant_for_hit_test(&self) -> bool {
        self.has_absolute_descendant_for_hit_test
    }

    pub(crate) fn is_absolute_positioned_for_hit_test(&self) -> bool {
        self.computed_style.position.mode() == PositionMode::Absolute
    }

    pub(crate) fn clip_mode_for_hit_test(&self) -> ClipMode {
        self.computed_style.position.clip_mode()
    }

    pub(crate) fn has_anchor_name_for_hit_test(&self) -> bool {
        self.computed_style.position.anchor_name().is_some()
    }

    pub(crate) fn should_append_to_root_viewport_render(&self) -> bool {
        self.computed_style.position.mode() == PositionMode::Absolute
            && self.computed_style.position.clip_mode() == ClipMode::Viewport
    }

    fn collect_root_viewport_deferred_descendants(&self, ctx: &mut UiBuildContext) {
        for child in &self.children {
            let Some(element) = child.as_any().downcast_ref::<Element>() else {
                continue;
            };
            if element.should_append_to_root_viewport_render() {
                ctx.append_to_defer(element.id());
            }
            element.collect_root_viewport_deferred_descendants(ctx);
        }
    }

    fn measure_flex_children(&mut self, proposal: LayoutProposal) {
        let bw_l = resolve_px_or_zero(
            self.computed_style.border_widths.left,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let bw_r = resolve_px_or_zero(
            self.computed_style.border_widths.right,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let bw_t = resolve_px_or_zero(
            self.computed_style.border_widths.top,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let bw_b = resolve_px_or_zero(
            self.computed_style.border_widths.bottom,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );

        let p_l = resolve_px_or_zero(
            self.computed_style.padding.left,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let p_r = resolve_px_or_zero(
            self.computed_style.padding.right,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let p_t = resolve_px_or_zero(
            self.computed_style.padding.top,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let p_b = resolve_px_or_zero(
            self.computed_style.padding.bottom,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );

        let (layout_w, layout_h) = self.current_layout_transition_size();
        let measure_w = if self.computed_style.width == SizeValue::Auto
            && proposal.percent_base_width.is_some()
        {
            proposal.width.max(0.0)
        } else {
            layout_w
        };
        let measure_h = if self.computed_style.height == SizeValue::Auto
            && proposal.percent_base_height.is_some()
        {
            proposal.height.max(0.0)
        } else {
            layout_h
        };
        let inner_w = (measure_w - bw_l - bw_r - p_l - p_r).max(0.0);
        let inner_h = (measure_h - bw_t - bw_b - p_t - p_b).max(0.0);

        let (child_available_width, child_available_height) = match self.scroll_direction {
            ScrollDirection::None => (inner_w, inner_h),
            ScrollDirection::Vertical => (inner_w, 1_000_000.0),
            ScrollDirection::Horizontal => (1_000_000.0, inner_h),
            ScrollDirection::Both => (1_000_000.0, 1_000_000.0),
        };

        let child_percent_base_width = if self.width_is_known(proposal) {
            Some(inner_w)
        } else {
            None
        };
        let child_percent_base_height = if self.height_is_known(proposal) {
            Some(inner_h)
        } else {
            None
        };

        for child in &mut self.children {
            child.measure(LayoutConstraints {
                max_width: child_available_width,
                max_height: child_available_height,
                viewport_width: proposal.viewport_width,
                viewport_height: proposal.viewport_height,
                percent_base_width: child_percent_base_width,
                percent_base_height: child_percent_base_height,
            });
        }
        let info = self.compute_flex_info(
            inner_w,
            inner_h,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let is_row = matches!(self.computed_style.display_flow_direction(), FlowDirection::Row);

        if self.computed_style.width == SizeValue::Auto {
            let auto_width = if is_row {
                info.total_main
            } else {
                info.total_cross
            };
            self.core.set_width(auto_width + bw_l + bw_r + p_l + p_r);
        }
        if self.computed_style.height == SizeValue::Auto {
            let auto_height = if is_row {
                info.total_cross
            } else {
                info.total_main
            };
            self.core.set_height(auto_height + bw_t + bw_b + p_t + p_b);
        }

        self.content_size = Size {
            width: if is_row {
                info.total_main
            } else {
                info.total_cross
            },
            height: if is_row {
                info.total_cross
            } else {
                info.total_main
            },
        };
        self.flex_info = Some(info);
    }

    fn compute_flex_info(
        &self,
        inner_w: f32,
        inner_h: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> FlexLayoutInfo {
        let is_row = matches!(self.computed_style.display_flow_direction(), FlowDirection::Row);
        let wrap = matches!(self.computed_style.display_flow_wrap(), FlowWrap::Wrap);
        let main_limit = if is_row { inner_w } else { inner_h };
        let gap_base = if is_row { inner_w } else { inner_h };
        let gap = resolve_px(
            self.computed_style.gap,
            gap_base,
            viewport_width,
            viewport_height,
        );

        let mut child_sizes = vec![(0.0_f32, 0.0_f32); self.children.len()];
        for (idx, child) in self.children.iter().enumerate() {
            if self.child_is_absolute(idx) {
                continue;
            }
            let (w, h) = child.measured_size();
            let main = if is_row { w } else { h };
            let cross = if is_row { h } else { w };
            child_sizes[idx] = (main, cross);
        }

        let mut lines: Vec<Vec<usize>> = Vec::new();
        let mut line_main_sum: Vec<f32> = Vec::new();
        let mut line_cross_max: Vec<f32> = Vec::new();
        let mut current = Vec::new();
        let mut current_main = 0.0;
        let mut current_cross = 0.0;

        for (idx, (item_main, item_cross)) in child_sizes.iter().copied().enumerate() {
            if self.child_is_absolute(idx) {
                continue;
            }
            let next_main = if current.is_empty() {
                item_main
            } else {
                current_main + gap + item_main
            };
            if wrap && !current.is_empty() && next_main > main_limit {
                lines.push(current);
                line_main_sum.push(current_main);
                line_cross_max.push(current_cross);
                current = Vec::new();
                current_main = 0.0;
                current_cross = 0.0;
            }
            if current.is_empty() {
                current_main = item_main;
                current_cross = item_cross;
            } else {
                current_main += gap + item_main;
                current_cross = current_cross.max(item_cross);
            }
            current.push(idx);
        }
        if !current.is_empty() {
            lines.push(current);
            line_main_sum.push(current_main);
            line_cross_max.push(current_cross);
        }

        let total_main = line_main_sum.iter().fold(0.0f32, |a, &b| a.max(b));
        let total_cross = line_cross_max.iter().sum::<f32>()
            + gap * (line_cross_max.len().saturating_sub(1) as f32);

        FlexLayoutInfo {
            lines,
            line_main_sum,
            line_cross_max,
            total_main,
            total_cross,
            child_sizes,
        }
    }

    fn build_self(&mut self, graph: &mut FrameGraph, ctx: &mut UiBuildContext, force_opaque: bool) {
        let fill_color = self.background_color.as_ref().to_rgba_f32();
        let border_color = self.border_colors.top.as_ref().to_rgba_f32();
        let same_color = colors_close(fill_color, border_color);
        let opacity = if force_opaque { 1.0 } else { self.opacity };
        self.render_box_shadows(graph, ctx, opacity);

        let max_bw = (self
            .core
            .layout_size
            .width
            .min(self.core.layout_size.height))
            * 0.5;
        let left = self.border_widths.left.clamp(0.0, max_bw);
        let right = self.border_widths.right.clamp(0.0, max_bw);
        let top = self.border_widths.top.clamp(0.0, max_bw);
        let bottom = self.border_widths.bottom.clamp(0.0, max_bw);
        let draw_left = if same_color { 0.0 } else { left };
        let draw_right = if same_color { 0.0 } else { right };
        let draw_top = if same_color { 0.0 } else { top };
        let draw_bottom = if same_color { 0.0 } else { bottom };
        let uniform_color = colors_like_eq(
            self.border_colors.left.as_ref(),
            self.border_colors.right.as_ref(),
        ) && colors_like_eq(
            self.border_colors.left.as_ref(),
            self.border_colors.top.as_ref(),
        ) && colors_like_eq(
            self.border_colors.left.as_ref(),
            self.border_colors.bottom.as_ref(),
        );

        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.core.layout_size.width.max(0.0),
            self.core.layout_size.height.max(0.0),
        );
        let mut pass = DrawRectPass::new(
            [self.core.layout_position.x, self.core.layout_position.y],
            [self.core.layout_size.width, self.core.layout_size.height],
            fill_color,
            opacity,
        );
        if uniform_color {
            pass.set_border_color(border_color);
            pass.set_border_widths(draw_left, draw_right, draw_top, draw_bottom);
            pass.set_border_radii(outer_radii.to_array());
            self.push_pass(graph, ctx, pass);
            return;
        }
        if outer_radii.has_any_rounding() {
            pass.set_border_side_colors(
                self.border_colors.left.as_ref().to_rgba_f32(),
                self.border_colors.right.as_ref().to_rgba_f32(),
                self.border_colors.top.as_ref().to_rgba_f32(),
                self.border_colors.bottom.as_ref().to_rgba_f32(),
            );
            pass.set_border_widths(draw_left, draw_right, draw_top, draw_bottom);
            pass.set_border_radii(outer_radii.to_array());
            self.push_pass(graph, ctx, pass);
            return;
        }

        pass.set_border_width(0.0);
        pass.set_border_radii([0.0; 4]);
        self.push_pass(graph, ctx, pass);
        self.push_edge_border_passes(graph, ctx, left, right, top, bottom, opacity);
    }

    fn push_pass<P: RenderTargetPass + RenderPass + 'static>(
        &mut self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        pass: P,
    ) {
        ctx.push_pass(graph, pass);
    }

    fn sync_props_from_computed_style(&mut self) {
        self.background_color = Box::new(self.computed_style.background_color);
        self.foreground_color = self.computed_style.color;
        self.box_shadows = self.computed_style.box_shadow.clone();
        self.border_colors.left = Box::new(self.computed_style.border_colors.left);
        self.border_colors.right = Box::new(self.computed_style.border_colors.right);
        self.border_colors.top = Box::new(self.computed_style.border_colors.top);
        self.border_colors.bottom = Box::new(self.computed_style.border_colors.bottom);
        self.border_widths.left = resolve_px(
            self.computed_style.border_widths.left,
            self.core.size.width,
            0.0,
            0.0,
        );
        self.border_widths.right = resolve_px(
            self.computed_style.border_widths.right,
            self.core.size.width,
            0.0,
            0.0,
        );
        self.border_widths.top = resolve_px(
            self.computed_style.border_widths.top,
            self.core.size.height,
            0.0,
            0.0,
        );
        self.border_widths.bottom = resolve_px(
            self.computed_style.border_widths.bottom,
            self.core.size.height,
            0.0,
            0.0,
        );
        let radius_base = self.core.size.width.min(self.core.size.height).max(0.0);
        self.border_radii = CornerRadii {
            top_left: resolve_px(
                self.computed_style.border_radii.top_left,
                radius_base,
                0.0,
                0.0,
            ),
            top_right: resolve_px(
                self.computed_style.border_radii.top_right,
                radius_base,
                0.0,
                0.0,
            ),
            bottom_right: resolve_px(
                self.computed_style.border_radii.bottom_right,
                radius_base,
                0.0,
                0.0,
            ),
            bottom_left: resolve_px(
                self.computed_style.border_radii.bottom_left,
                radius_base,
                0.0,
                0.0,
            ),
        };
        self.border_radius = self.border_radii.max();
        self.opacity = self.computed_style.opacity.clamp(0.0, 1.0);
        self.scroll_direction = self.computed_style.scroll_direction;
        self.padding.left = resolve_px(
            self.computed_style.padding.left,
            self.core.size.width,
            0.0,
            0.0,
        );
        self.padding.right = resolve_px(
            self.computed_style.padding.right,
            self.core.size.width,
            0.0,
            0.0,
        );
        self.padding.top = resolve_px(
            self.computed_style.padding.top,
            self.core.size.height,
            0.0,
            0.0,
        );
        self.padding.bottom = resolve_px(
            self.computed_style.padding.bottom,
            self.core.size.height,
            0.0,
            0.0,
        );
    }

    fn push_edge_border_passes(
        &mut self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        left: f32,
        right: f32,
        top: f32,
        bottom: f32,
        opacity: f32,
    ) {
        let x = self.core.layout_position.x;
        let y = self.core.layout_position.y;
        let w = self.core.layout_size.width.max(0.0);
        let h = self.core.layout_size.height.max(0.0);
        if top > 0.0 {
            let mut pass = DrawRectPass::new(
                [x, y],
                [w, top.min(h)],
                self.border_colors.top.as_ref().to_rgba_f32(),
                opacity,
            );
            pass.set_border_width(0.0);
            pass.set_border_radius(0.0);
            self.push_pass(graph, ctx, pass);
        }
        if bottom > 0.0 {
            let bh = bottom.min(h);
            let mut pass = DrawRectPass::new(
                [x, y + (h - bh).max(0.0)],
                [w, bh],
                self.border_colors.bottom.as_ref().to_rgba_f32(),
                opacity,
            );
            pass.set_border_width(0.0);
            pass.set_border_radius(0.0);
            self.push_pass(graph, ctx, pass);
        }
        let middle_y = y + top.min(h);
        let middle_h = (h - top - bottom).max(0.0);
        if left > 0.0 && middle_h > 0.0 {
            let mut pass = DrawRectPass::new(
                [x, middle_y],
                [left.min(w), middle_h],
                self.border_colors.left.as_ref().to_rgba_f32(),
                opacity,
            );
            pass.set_border_width(0.0);
            pass.set_border_radius(0.0);
            self.push_pass(graph, ctx, pass);
        }
        if right > 0.0 && middle_h > 0.0 {
            let rw = right.min(w);
            let mut pass = DrawRectPass::new(
                [x + (w - rw).max(0.0), middle_y],
                [rw, middle_h],
                self.border_colors.right.as_ref().to_rgba_f32(),
                opacity,
            );
            pass.set_border_width(0.0);
            pass.set_border_radius(0.0);
            self.push_pass(graph, ctx, pass);
        }
    }

    fn push_inner_fill_pass(
        &mut self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        left: f32,
        right: f32,
        top: f32,
        bottom: f32,
        outer_radii: CornerRadii,
        opacity: f32,
    ) {
        let x = self.core.layout_position.x;
        let y = self.core.layout_position.y;
        let w = self.core.layout_size.width.max(0.0);
        let h = self.core.layout_size.height.max(0.0);
        let inner_x = x + left;
        let inner_y = y + top;
        let inner_w = (w - left - right).max(0.0);
        let inner_h = (h - top - bottom).max(0.0);
        if inner_w <= 0.0 || inner_h <= 0.0 {
            return;
        }
        let inner_radii = normalize_corner_radii(
            inset_corner_radii(outer_radii, left, right, top, bottom),
            inner_w,
            inner_h,
        );
        let mut pass = DrawRectPass::new(
            [inner_x, inner_y],
            [inner_w, inner_h],
            self.background_color.as_ref().to_rgba_f32(),
            opacity,
        );
        pass.set_border_width(0.0);
        pass.set_border_radii(inner_radii.to_array());
        self.push_pass(graph, ctx, pass);
    }

    fn render_box_shadows(
        &mut self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        opacity: f32,
    ) {
        if self.box_shadows.is_empty() {
            return;
        }
        let layout_x = self.core.layout_position.x;
        let layout_y = self.core.layout_position.y;
        let layout_w = self.core.layout_size.width.max(0.0);
        let layout_h = self.core.layout_size.height.max(0.0);
        if layout_w <= 0.0 || layout_h <= 0.0 {
            return;
        }
        let shadows = self.box_shadows.clone();
        for shadow in shadows {
            let spread = shadow.spread;
            let mesh = ShadowMesh::rounded_rect(
                layout_x - spread,
                layout_y - spread,
                layout_w + spread * 2.0,
                layout_h + spread * 2.0,
                (self.border_radius + spread).max(0.0),
            );
            let pass = ShadowPass::new(
                mesh,
                ShadowParams {
                    offset_x: shadow.offset_x,
                    offset_y: shadow.offset_y,
                    blur_radius: shadow.blur.max(0.0),
                    color: shadow.color.to_rgba_f32(),
                    opacity: opacity.clamp(0.0, 1.0),
                    spread: 0.0,
                    clip_to_geometry: false,
                },
            );
            self.push_pass(graph, ctx, pass);
        }
    }

    fn measure_self(&mut self, proposal: LayoutProposal) {
        if let SizeValue::Length(width) = self.computed_style.width {
            if let Some(resolved) = resolve_px_with_base(
                width,
                proposal.percent_base_width,
                proposal.viewport_width,
                proposal.viewport_height,
            ) {
                self.core.set_width(resolved);
            }
        }
        if let SizeValue::Length(height) = self.computed_style.height {
            if let Some(resolved) = resolve_px_with_base(
                height,
                proposal.percent_base_height,
                proposal.viewport_width,
                proposal.viewport_height,
            ) {
                self.core.set_height(resolved);
            }
        }
    }

    fn resolve_size_constraint(
        value: SizeValue,
        percent_base: Option<f32>,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Option<f32> {
        let SizeValue::Length(length) = value else {
            return None;
        };
        resolve_px_with_base(length, percent_base, viewport_width, viewport_height)
            .map(|v| v.max(0.0))
    }

    fn apply_size_constraints(&mut self, proposal: LayoutProposal, include_auto: bool) {
        let min_width = Self::resolve_size_constraint(
            self.computed_style.min_width,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        )
        .unwrap_or(0.0);
        let min_height = Self::resolve_size_constraint(
            self.computed_style.min_height,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        )
        .unwrap_or(0.0);

        let mut max_width = Self::resolve_size_constraint(
            self.computed_style.max_width,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let mut max_height = Self::resolve_size_constraint(
            self.computed_style.max_height,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );

        if let Some(value) = max_width {
            max_width = Some(value.max(min_width));
        }
        if let Some(value) = max_height {
            max_height = Some(value.max(min_height));
        }

        if include_auto || self.computed_style.width != SizeValue::Auto {
            let mut width = self.core.size.width.max(0.0).max(min_width);
            if let Some(max_width) = max_width {
                width = width.min(max_width);
            }
            self.core.set_width(width);
        }

        if include_auto || self.computed_style.height != SizeValue::Auto {
            let mut height = self.core.size.height.max(0.0).max(min_height);
            if let Some(max_height) = max_height {
                height = height.min(max_height);
            }
            self.core.set_height(height);
        }
    }

    fn width_is_known(&self, proposal: LayoutProposal) -> bool {
        match self.computed_style.width {
            SizeValue::Length(length) if length.needs_percent_base() => {
                proposal.percent_base_width.is_some()
            }
            SizeValue::Length(Length::Vw(_)) => true,
            SizeValue::Length(Length::Vh(_)) => true,
            SizeValue::Length(_) => true,
            SizeValue::Auto => proposal.percent_base_width.is_some(),
        }
    }

    fn height_is_known(&self, proposal: LayoutProposal) -> bool {
        match self.computed_style.height {
            SizeValue::Length(length) if length.needs_percent_base() => {
                proposal.percent_base_height.is_some()
            }
            SizeValue::Length(Length::Vw(_)) => true,
            SizeValue::Length(Length::Vh(_)) => true,
            SizeValue::Length(_) => true,
            SizeValue::Auto => proposal.percent_base_height.is_some(),
        }
    }

    fn resolve_lengths_from_parent_inner(&mut self, proposal: LayoutProposal) {
        self.border_widths.left = resolve_px_or_zero(
            self.computed_style.border_widths.left,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        self.border_widths.right = resolve_px_or_zero(
            self.computed_style.border_widths.right,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        self.border_widths.top = resolve_px_or_zero(
            self.computed_style.border_widths.top,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        self.border_widths.bottom = resolve_px_or_zero(
            self.computed_style.border_widths.bottom,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );

        if self
            .parsed_style
            .get(crate::style::PropertyId::PaddingLeft)
            .is_some()
        {
            self.padding.left = resolve_px_or_zero(
                self.computed_style.padding.left,
                proposal.percent_base_width,
                proposal.viewport_width,
                proposal.viewport_height,
            );
        }
        if self
            .parsed_style
            .get(crate::style::PropertyId::PaddingRight)
            .is_some()
        {
            self.padding.right = resolve_px_or_zero(
                self.computed_style.padding.right,
                proposal.percent_base_width,
                proposal.viewport_width,
                proposal.viewport_height,
            );
        }
        if self
            .parsed_style
            .get(crate::style::PropertyId::PaddingTop)
            .is_some()
        {
            self.padding.top = resolve_px_or_zero(
                self.computed_style.padding.top,
                proposal.percent_base_height,
                proposal.viewport_width,
                proposal.viewport_height,
            );
        }
        if self
            .parsed_style
            .get(crate::style::PropertyId::PaddingBottom)
            .is_some()
        {
            self.padding.bottom = resolve_px_or_zero(
                self.computed_style.padding.bottom,
                proposal.percent_base_height,
                proposal.viewport_width,
                proposal.viewport_height,
            );
        }
    }

    fn resolve_corner_radii_from_self_box(&mut self, proposal: LayoutProposal) {
        let radius_base = self
            .core
            .layout_size
            .width
            .min(self.core.layout_size.height)
            .max(0.0);
        self.border_radii = CornerRadii {
            top_left: resolve_px(
                self.computed_style.border_radii.top_left,
                radius_base,
                proposal.viewport_width,
                proposal.viewport_height,
            ),
            top_right: resolve_px(
                self.computed_style.border_radii.top_right,
                radius_base,
                proposal.viewport_width,
                proposal.viewport_height,
            ),
            bottom_right: resolve_px(
                self.computed_style.border_radii.bottom_right,
                radius_base,
                proposal.viewport_width,
                proposal.viewport_height,
            ),
            bottom_left: resolve_px(
                self.computed_style.border_radii.bottom_left,
                radius_base,
                proposal.viewport_width,
                proposal.viewport_height,
            ),
        };
        self.border_radius = self.border_radii.max();
    }

    fn update_content_size_from_children(&mut self) {
        if self.children.is_empty() {
            self.content_size = Size {
                width: 0.0,
                height: 0.0,
            };
            return;
        }
        let mut max_x = 0.0_f32;
        let mut max_y = 0.0_f32;
        for (idx, child) in self.children.iter().enumerate() {
            if self.child_is_absolute(idx) {
                continue;
            }
            let snapshot = child.box_model_snapshot();
            let (child_flow_x, child_flow_y) = child
                .as_any()
                .downcast_ref::<Element>()
                .map(|el| (el.layout_flow_position.x, el.layout_flow_position.y))
                .unwrap_or((snapshot.x, snapshot.y));
            let rel_x = child_flow_x - self.layout_flow_inner_position.x + self.scroll_offset.x;
            let rel_y = child_flow_y - self.layout_flow_inner_position.y + self.scroll_offset.y;
            max_x = max_x.max(rel_x + snapshot.width.max(0.0));
            max_y = max_y.max(rel_y + snapshot.height.max(0.0));
        }
        self.content_size = Size {
            width: max_x.max(0.0),
            height: max_y.max(0.0),
        };
    }

    fn clamp_scroll_offset(&mut self) {
        let max_x = (self.content_size.width - self.layout_inner_size.width).max(0.0);
        let max_y = (self.content_size.height - self.layout_inner_size.height).max(0.0);
        self.scroll_offset.x = self.scroll_offset.x.clamp(0.0, max_x);
        self.scroll_offset.y = self.scroll_offset.y.clamp(0.0, max_y);
    }

    fn child_layout_limits_for_proposal(&self, proposal: LayoutProposal) -> (f32, f32) {
        const SCROLL_EXPANDED_LIMIT: f32 = 1_000_000.0;

        let bw_l = resolve_px_or_zero(
            self.computed_style.border_widths.left,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let bw_r = resolve_px_or_zero(
            self.computed_style.border_widths.right,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let bw_t = resolve_px_or_zero(
            self.computed_style.border_widths.top,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let bw_b = resolve_px_or_zero(
            self.computed_style.border_widths.bottom,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );

        let p_l = resolve_px_or_zero(
            self.computed_style.padding.left,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let p_r = resolve_px_or_zero(
            self.computed_style.padding.right,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let p_t = resolve_px_or_zero(
            self.computed_style.padding.top,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let p_b = resolve_px_or_zero(
            self.computed_style.padding.bottom,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );

        let (layout_w, layout_h) = self.current_layout_transition_size();
        let inner_w = (layout_w - bw_l - bw_r - p_l - p_r).max(0.0);
        let inner_h = (layout_h - bw_t - bw_b - p_t - p_b).max(0.0);

        match self.scroll_direction {
            ScrollDirection::None => (inner_w, inner_h),
            ScrollDirection::Vertical => (inner_w, SCROLL_EXPANDED_LIMIT),
            ScrollDirection::Horizontal => (SCROLL_EXPANDED_LIMIT, inner_h),
            ScrollDirection::Both => (SCROLL_EXPANDED_LIMIT, SCROLL_EXPANDED_LIMIT),
        }
    }

    fn update_size_from_measured_children(&mut self) {
        let mut max_w = 0.0_f32;
        let mut max_h = 0.0_f32;
        for (idx, child) in self.children.iter().enumerate() {
            if self.child_is_absolute(idx) {
                continue;
            }
            let (w, h) = child.measured_size();
            max_w = max_w.max(w);
            max_h = max_h.max(h);
        }

        let proposal = self.last_layout_proposal.unwrap_or(LayoutProposal {
            width: 10_000.0,
            height: 10_000.0,
            viewport_width: 0.0,
            viewport_height: 0.0,
            percent_base_width: None,
            percent_base_height: None,
        });

        let bw_l = resolve_px_or_zero(
            self.computed_style.border_widths.left,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let bw_r = resolve_px_or_zero(
            self.computed_style.border_widths.right,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let bw_t = resolve_px_or_zero(
            self.computed_style.border_widths.top,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let bw_b = resolve_px_or_zero(
            self.computed_style.border_widths.bottom,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );

        let p_l = resolve_px_or_zero(
            self.computed_style.padding.left,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let p_r = resolve_px_or_zero(
            self.computed_style.padding.right,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let p_t = resolve_px_or_zero(
            self.computed_style.padding.top,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let p_b = resolve_px_or_zero(
            self.computed_style.padding.bottom,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );

        if self.computed_style.width == SizeValue::Auto {
            self.core.set_width(max_w + bw_l + bw_r + p_l + p_r);
        }
        if self.computed_style.height == SizeValue::Auto {
            self.core.set_height(max_h + bw_t + bw_b + p_t + p_b);
        }
    }

    fn place_self(
        &mut self,
        proposal: LayoutProposal,
        parent_x: f32,
        parent_y: f32,
        parent_visual_offset_x: f32,
        parent_visual_offset_y: f32,
    ) {
        let mut target_width = self.core.size.width.max(0.0);
        let mut target_height = self.core.size.height.max(0.0);
        let mut target_rel_x = self.core.position.x;
        let mut target_rel_y = self.core.position.y;
        let is_absolute = self.computed_style.position.mode() == PositionMode::Absolute;
        let mut absolute_clip_rect: Option<Rect> = None;
        if is_absolute {
            let fallback_anchor = AnchorSnapshot {
                x: parent_x,
                y: parent_y,
                width: proposal.width.max(0.0),
                height: proposal.height.max(0.0),
            };
            let anchor = self.resolve_anchor_snapshot(fallback_anchor);
            let left = self.computed_style.position.left_inset().and_then(|v| {
                resolve_signed_px_with_base(
                    v,
                    Some(anchor.width),
                    proposal.viewport_width,
                    proposal.viewport_height,
                )
            });
            let right = self.computed_style.position.right_inset().and_then(|v| {
                resolve_signed_px_with_base(
                    v,
                    Some(anchor.width),
                    proposal.viewport_width,
                    proposal.viewport_height,
                )
            });
            let top = self.computed_style.position.top_inset().and_then(|v| {
                resolve_signed_px_with_base(
                    v,
                    Some(anchor.height),
                    proposal.viewport_width,
                    proposal.viewport_height,
                )
            });
            let bottom = self.computed_style.position.bottom_inset().and_then(|v| {
                resolve_signed_px_with_base(
                    v,
                    Some(anchor.height),
                    proposal.viewport_width,
                    proposal.viewport_height,
                )
            });

            if let (Some(l), Some(r)) = (left, right) {
                target_width = (anchor.width - l - r).max(0.0);
            }
            if let (Some(t), Some(b)) = (top, bottom) {
                target_height = (anchor.height - t - b).max(0.0);
            }

            target_rel_x = if let Some(l) = left {
                (anchor.x - parent_x) + l
            } else if let Some(r) = right {
                (anchor.x - parent_x) + (anchor.width - r - target_width)
            } else {
                anchor.x - parent_x
            };
            target_rel_y = if let Some(t) = top {
                (anchor.y - parent_y) + t
            } else if let Some(b) = bottom {
                (anchor.y - parent_y) + (anchor.height - b - target_height)
            } else {
                anchor.y - parent_y
            };

            let mut abs_x = parent_x + target_rel_x;
            let mut abs_y = parent_y + target_rel_y;
            let boundary = match self.computed_style.position.collision_boundary() {
                CollisionBoundary::Parent => Rect {
                    x: parent_x,
                    y: parent_y,
                    width: proposal.width.max(0.0),
                    height: proposal.height.max(0.0),
                },
                CollisionBoundary::Viewport => {
                    let (vw, vh) = self.viewport_size_from_runtime(proposal.width, proposal.height);
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        width: vw,
                        height: vh,
                    }
                }
            };
            let clip_mode = self.computed_style.position.clip_mode();
            let has_anchor = self.computed_style.position.anchor_name().is_some();
            absolute_clip_rect = Some(match clip_mode {
                ClipMode::Parent => Rect {
                    x: parent_x + parent_visual_offset_x,
                    y: parent_y + parent_visual_offset_y,
                    width: proposal.width.max(0.0),
                    height: proposal.height.max(0.0),
                },
                ClipMode::Viewport => {
                    let (vw, vh) = self.viewport_size_from_runtime(proposal.width, proposal.height);
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        width: vw.max(0.0),
                        height: vh.max(0.0),
                    }
                }
                ClipMode::AnchorParent if has_anchor => Rect {
                    x: anchor.x,
                    y: anchor.y,
                    width: anchor.width.max(0.0),
                    height: anchor.height.max(0.0),
                },
                ClipMode::AnchorParent => Rect {
                    x: parent_x + parent_visual_offset_x,
                    y: parent_y + parent_visual_offset_y,
                    width: proposal.width.max(0.0),
                    height: proposal.height.max(0.0),
                },
            });
            apply_collision(
                self.computed_style.position.collision_mode(),
                boundary,
                &mut abs_x,
                &mut abs_y,
                target_width,
                target_height,
                anchor,
                left,
                right,
                top,
                bottom,
            );
            target_rel_x = abs_x - parent_x;
            target_rel_y = abs_y - parent_y;
        }
        let has_x_transition = self.computed_style.transition.as_slice().iter().any(|t| {
            matches!(
                t.property,
                TransitionProperty::Position
                    | TransitionProperty::PositionX
                    | TransitionProperty::X
            )
        });
        let has_y_transition = self.computed_style.transition.as_slice().iter().any(|t| {
            matches!(
                t.property,
                TransitionProperty::Position
                    | TransitionProperty::PositionY
                    | TransitionProperty::Y
            )
        });
        let has_width_transition = self.computed_style.transition.as_slice().iter().any(|t| {
            matches!(
                t.property,
                TransitionProperty::All | TransitionProperty::Width
            )
        });
        let has_height_transition = self.computed_style.transition.as_slice().iter().any(|t| {
            matches!(
                t.property,
                TransitionProperty::All | TransitionProperty::Height
            )
        });
        if !has_x_transition {
            self.layout_transition_visual_offset_x = 0.0;
            self.layout_transition_target_x = None;
        }
        if !has_y_transition {
            self.layout_transition_visual_offset_y = 0.0;
            self.layout_transition_target_y = None;
        }
        if !has_width_transition {
            self.layout_transition_override_width = None;
            self.layout_transition_target_width = None;
        }
        if !has_height_transition {
            self.layout_transition_override_height = None;
            self.layout_transition_target_height = None;
        }
        let current_visual_rel_x =
            (self.layout_flow_position.x - parent_x) + self.layout_transition_visual_offset_x;
        let current_visual_rel_y =
            (self.layout_flow_position.y - parent_y) + self.layout_transition_visual_offset_y;
        let prev_target_rel_x = self.layout_flow_position.x - self.last_parent_layout_x;
        let prev_target_rel_y = self.layout_flow_position.y - self.last_parent_layout_y;
        let current_offset_x = current_visual_rel_x - target_rel_x;
        let current_offset_y = current_visual_rel_y - target_rel_y;
        let prev_width = self.core.layout_size.width.max(0.0);
        let prev_height = self.core.layout_size.height.max(0.0);
        if self
            .layout_transition_target_x
            .is_some_and(|_| approx_eq(current_offset_x, 0.0))
        {
            self.layout_transition_target_x = None;
            self.layout_transition_visual_offset_x = 0.0;
        }
        if self
            .layout_transition_target_y
            .is_some_and(|_| approx_eq(current_offset_y, 0.0))
        {
            self.layout_transition_target_y = None;
            self.layout_transition_visual_offset_y = 0.0;
        }
        // If visual target changes while track is active, always rebase from current rendered
        // position and restart. This keeps the visual track start anchored to "where it is now".
        if self
            .layout_transition_target_x
            .is_some_and(|active| !approx_eq(active, target_rel_x))
        {
            self.layout_transition_visual_offset_x = current_offset_x;
            self.layout_transition_target_x = None;
        }
        if self
            .layout_transition_target_y
            .is_some_and(|active| !approx_eq(active, target_rel_y))
        {
            self.layout_transition_visual_offset_y = current_offset_y;
            self.layout_transition_target_y = None;
        }
        if self
            .layout_transition_target_width
            .is_some_and(|target| approx_eq(prev_width, target))
        {
            self.layout_transition_target_width = None;
            self.layout_transition_override_width = None;
        }
        if self
            .layout_transition_target_height
            .is_some_and(|target| approx_eq(prev_height, target))
        {
            self.layout_transition_target_height = None;
            self.layout_transition_override_height = None;
        }

        if self.has_layout_snapshot {
            self.collect_layout_transition_requests(
                current_offset_x,
                current_offset_y,
                prev_target_rel_x,
                prev_target_rel_y,
                prev_width,
                prev_height,
                target_rel_x,
                target_rel_y,
                target_width,
                target_height,
            );
        }
        self.has_layout_snapshot = true;

        let frame_rel_x = target_rel_x;
        let frame_rel_y = target_rel_y;
        let frame_width = self
            .layout_transition_override_width
            .unwrap_or(target_width);
        let frame_height = self
            .layout_transition_override_height
            .unwrap_or(target_height);
        self.layout_flow_position = Position {
            x: parent_x + frame_rel_x,
            y: parent_y + frame_rel_y,
        };
        let frame = LayoutFrame {
            x: self.layout_flow_position.x
                + parent_visual_offset_x
                + self.layout_transition_visual_offset_x,
            y: self.layout_flow_position.y
                + parent_visual_offset_y
                + self.layout_transition_visual_offset_y,
            width: frame_width,
            height: frame_height,
        };
        self.core.layout_position = Position {
            x: frame.x,
            y: frame.y,
        };
        self.core.layout_size = Size {
            width: frame.width,
            height: frame.height,
        };

        let parent_rect = Rect {
            x: parent_x + parent_visual_offset_x,
            y: parent_y + parent_visual_offset_y,
            width: proposal.width.max(0.0),
            height: proposal.height.max(0.0),
        };
        let cull_rect = if is_absolute {
            absolute_clip_rect.unwrap_or(parent_rect)
        } else {
            self.current_parent_child_clip_rect().unwrap_or(parent_rect)
        };
        let parent_left = cull_rect.x;
        let parent_top = cull_rect.y;
        let parent_right = cull_rect.x + cull_rect.width;
        let parent_bottom = cull_rect.y + cull_rect.height;
        let self_right = frame.x + frame.width;
        let self_bottom = frame.y + frame.height;

        self.core.should_render = frame.width > 0.0
            && frame.height > 0.0
            && self_right > parent_left
            && frame.x < parent_right
            && self_bottom > parent_top
            && frame.y < parent_bottom;
        self.last_parent_layout_x = parent_x;
        self.last_parent_layout_y = parent_y;
    }

    fn collect_layout_transition_requests(
        &mut self,
        prev_offset_x: f32,
        prev_offset_y: f32,
        prev_target_rel_x: f32,
        prev_target_rel_y: f32,
        prev_width: f32,
        prev_height: f32,
        target_rel_x: f32,
        target_rel_y: f32,
        target_width: f32,
        target_height: f32,
    ) {
        for transition in self.computed_style.transition.as_slice() {
            let runtime_layout = RuntimeLayoutTransition {
                duration_ms: transition.duration_ms,
                delay_ms: transition.delay_ms,
                timing: map_transition_timing(transition.timing),
            };
            let runtime_visual = RuntimeVisualTransition {
                duration_ms: transition.duration_ms,
                delay_ms: transition.delay_ms,
                timing: map_transition_timing(transition.timing),
            };
            match transition.property {
                TransitionProperty::All => {
                    let should_start_width = self
                        .layout_transition_target_width
                        .is_none_or(|active| !approx_eq(active, target_width));
                    if should_start_width && !approx_eq(prev_width, target_width) {
                        self.pending_layout_transition_requests
                            .push(LayoutTrackRequest {
                                target: self.core.id,
                                field: LayoutField::Width,
                                from: prev_width,
                                to: target_width,
                                transition: runtime_layout,
                            });
                        self.layout_transition_override_width = Some(prev_width.max(0.0));
                        self.layout_transition_target_width = Some(target_width);
                    }
                    let should_start_height = self
                        .layout_transition_target_height
                        .is_none_or(|active| !approx_eq(active, target_height));
                    if should_start_height && !approx_eq(prev_height, target_height) {
                        self.pending_layout_transition_requests
                            .push(LayoutTrackRequest {
                                target: self.core.id,
                                field: LayoutField::Height,
                                from: prev_height,
                                to: target_height,
                                transition: runtime_layout,
                            });
                        self.layout_transition_override_height = Some(prev_height.max(0.0));
                        self.layout_transition_target_height = Some(target_height);
                    }
                }
                TransitionProperty::Position => {
                    let should_start_x = self.layout_transition_target_x.is_none();
                    if should_start_x
                        && !approx_eq(prev_offset_x, 0.0)
                        && !approx_eq(prev_target_rel_x, target_rel_x)
                    {
                        self.pending_visual_transition_requests
                            .push(VisualTrackRequest {
                                target: self.core.id,
                                field: VisualField::X,
                                from: prev_offset_x,
                                to: 0.0,
                                transition: runtime_visual,
                            });
                        self.layout_transition_visual_offset_x = prev_offset_x;
                        self.layout_transition_target_x = Some(target_rel_x);
                    }
                    let should_start_y = self.layout_transition_target_y.is_none();
                    if should_start_y
                        && !approx_eq(prev_offset_y, 0.0)
                        && !approx_eq(prev_target_rel_y, target_rel_y)
                    {
                        self.pending_visual_transition_requests
                            .push(VisualTrackRequest {
                                target: self.core.id,
                                field: VisualField::Y,
                                from: prev_offset_y,
                                to: 0.0,
                                transition: runtime_visual,
                            });
                        self.layout_transition_visual_offset_y = prev_offset_y;
                        self.layout_transition_target_y = Some(target_rel_y);
                    }
                }
                TransitionProperty::Width => {
                    let should_start_width = self
                        .layout_transition_target_width
                        .is_none_or(|active| !approx_eq(active, target_width));
                    if should_start_width && !approx_eq(prev_width, target_width) {
                        self.pending_layout_transition_requests
                            .push(LayoutTrackRequest {
                                target: self.core.id,
                                field: LayoutField::Width,
                                from: prev_width,
                                to: target_width,
                                transition: runtime_layout,
                            });
                        self.layout_transition_override_width = Some(prev_width.max(0.0));
                        self.layout_transition_target_width = Some(target_width);
                    }
                }
                TransitionProperty::Height => {
                    let should_start_height = self
                        .layout_transition_target_height
                        .is_none_or(|active| !approx_eq(active, target_height));
                    if should_start_height && !approx_eq(prev_height, target_height) {
                        self.pending_layout_transition_requests
                            .push(LayoutTrackRequest {
                                target: self.core.id,
                                field: LayoutField::Height,
                                from: prev_height,
                                to: target_height,
                                transition: runtime_layout,
                            });
                        self.layout_transition_override_height = Some(prev_height.max(0.0));
                        self.layout_transition_target_height = Some(target_height);
                    }
                }
                TransitionProperty::X | TransitionProperty::PositionX => {
                    let should_start_x = self.layout_transition_target_x.is_none();
                    if should_start_x
                        && !approx_eq(prev_offset_x, 0.0)
                        && !approx_eq(prev_target_rel_x, target_rel_x)
                    {
                        self.pending_visual_transition_requests
                            .push(VisualTrackRequest {
                                target: self.core.id,
                                field: VisualField::X,
                                from: prev_offset_x,
                                to: 0.0,
                                transition: runtime_visual,
                            });
                        self.layout_transition_visual_offset_x = prev_offset_x;
                        self.layout_transition_target_x = Some(target_rel_x);
                    }
                }
                TransitionProperty::Y | TransitionProperty::PositionY => {
                    let should_start_y = self.layout_transition_target_y.is_none();
                    if should_start_y
                        && !approx_eq(prev_offset_y, 0.0)
                        && !approx_eq(prev_target_rel_y, target_rel_y)
                    {
                        self.pending_visual_transition_requests
                            .push(VisualTrackRequest {
                                target: self.core.id,
                                field: VisualField::Y,
                                from: prev_offset_y,
                                to: 0.0,
                                transition: runtime_visual,
                            });
                        self.layout_transition_visual_offset_y = prev_offset_y;
                        self.layout_transition_target_y = Some(target_rel_y);
                    }
                }
                _ => {}
            }
        }
    }

    fn child_layout_limits(&self) -> (f32, f32) {
        const SCROLL_EXPANDED_LIMIT: f32 = 1_000_000.0;
        match self.scroll_direction {
            ScrollDirection::None => (self.layout_inner_size.width, self.layout_inner_size.height),
            ScrollDirection::Vertical => (self.layout_inner_size.width, SCROLL_EXPANDED_LIMIT),
            ScrollDirection::Horizontal => (SCROLL_EXPANDED_LIMIT, self.layout_inner_size.height),
            ScrollDirection::Both => (SCROLL_EXPANDED_LIMIT, SCROLL_EXPANDED_LIMIT),
        }
    }

    fn begin_place_scope(&self, placement: LayoutPlacement) {
        PLACEMENT_RUNTIME.with(|runtime| {
            let mut runtime = runtime.borrow_mut();
            if runtime.depth == 0 {
                runtime.anchors.clear();
                runtime.child_clip_stack.clear();
                runtime.viewport_width = placement.viewport_width.max(0.0);
                runtime.viewport_height = placement.viewport_height.max(0.0);
            }
            runtime.depth += 1;
        });
    }

    fn end_place_scope(&self) {
        PLACEMENT_RUNTIME.with(|runtime| {
            let mut runtime = runtime.borrow_mut();
            if runtime.depth > 0 {
                runtime.depth -= 1;
            }
            if runtime.depth == 0 {
                runtime.anchors.clear();
                runtime.child_clip_stack.clear();
            }
        });
    }

    fn push_child_clip_scope(&self, rect: Rect) {
        PLACEMENT_RUNTIME.with(|runtime| {
            runtime.borrow_mut().child_clip_stack.push(rect);
        });
    }

    fn pop_child_clip_scope(&self) {
        PLACEMENT_RUNTIME.with(|runtime| {
            runtime.borrow_mut().child_clip_stack.pop();
        });
    }

    fn current_parent_child_clip_rect(&self) -> Option<Rect> {
        PLACEMENT_RUNTIME.with(|runtime| runtime.borrow().child_clip_stack.last().copied())
    }

    fn register_anchor_snapshot(&self) {
        let Some(anchor_name) = self.anchor_name.as_ref() else {
            return;
        };
        let snapshot = AnchorSnapshot {
            x: self.core.layout_position.x,
            y: self.core.layout_position.y,
            width: self.core.layout_size.width.max(0.0),
            height: self.core.layout_size.height.max(0.0),
        };
        PLACEMENT_RUNTIME.with(|runtime| {
            runtime
                .borrow_mut()
                .anchors
                .insert(anchor_name.as_str().to_string(), snapshot);
        });
    }

    fn resolve_anchor_snapshot(&self, fallback: AnchorSnapshot) -> AnchorSnapshot {
        let Some(anchor_name) = self.computed_style.position.anchor_name() else {
            return fallback;
        };
        PLACEMENT_RUNTIME.with(|runtime| {
            runtime
                .borrow()
                .anchors
                .get(anchor_name.as_str())
                .copied()
                .unwrap_or(fallback)
        })
    }

    fn viewport_size_from_runtime(&self, fallback_width: f32, fallback_height: f32) -> (f32, f32) {
        PLACEMENT_RUNTIME.with(|runtime| {
            let runtime = runtime.borrow();
            let width = if runtime.viewport_width > 0.0 {
                runtime.viewport_width
            } else {
                fallback_width.max(0.0)
            };
            let height = if runtime.viewport_height > 0.0 {
                runtime.viewport_height
            } else {
                fallback_height.max(0.0)
            };
            (width, height)
        })
    }

    fn child_is_absolute(&self, index: usize) -> bool {
        self.children
            .get(index)
            .and_then(|child| child.as_any().downcast_ref::<Element>())
            .is_some_and(|el| el.computed_style.position.mode() == PositionMode::Absolute)
    }

    fn child_renders_outside_inner_clip(&self, index: usize) -> bool {
        self.children
            .get(index)
            .and_then(|child| child.as_any().downcast_ref::<Element>())
            .is_some_and(|el| {
                if el.computed_style.position.mode() != PositionMode::Absolute {
                    return false;
                }
                match el.computed_style.position.clip_mode() {
                    ClipMode::Parent => false,
                    ClipMode::Viewport => true,
                    ClipMode::AnchorParent => el.computed_style.position.anchor_name().is_some(),
                }
            })
    }

    fn recompute_absolute_descendant_for_hit_test(&mut self) {
        self.has_absolute_descendant_for_hit_test = self.children.iter().any(|child| {
            child
                .as_any()
                .downcast_ref::<Element>()
                .is_some_and(|element| {
                    element.is_absolute_positioned_for_hit_test()
                        || element.has_absolute_descendant_for_hit_test
                })
        });
    }

    fn place_children(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        child_percent_base_width: Option<f32>,
        child_percent_base_height: Option<f32>,
    ) {
        let overscan = Self::SHOULD_RENDER_OVERSCAN_PX.max(0.0);
        self.push_child_clip_scope(Rect {
            x: self.layout_inner_position.x - overscan,
            y: self.layout_inner_position.y - overscan,
            width: (self.layout_inner_size.width + overscan * 2.0).max(0.0),
            height: (self.layout_inner_size.height + overscan * 2.0).max(0.0),
        });
        let (child_available_width, child_available_height) = self.child_layout_limits();
        let is_flex = matches!(
            self.computed_style.display,
            Display::Flow { .. } | Display::InlineFlex
        );
        if is_flex {
            self.place_flex_children(
                child_available_width,
                child_available_height,
                viewport_width,
                viewport_height,
                child_percent_base_width,
                child_percent_base_height,
            );
        } else {
            let origin_x = self.layout_flow_inner_position.x - self.scroll_offset.x;
            let origin_y = self.layout_flow_inner_position.y - self.scroll_offset.y;
            let visual_offset_x = self.core.layout_position.x - self.layout_flow_position.x;
            let visual_offset_y = self.core.layout_position.y - self.layout_flow_position.y;
            for idx in 0..self.children.len() {
                if self.child_is_absolute(idx) {
                    continue;
                }
                let child = &mut self.children[idx];
                child.place(LayoutPlacement {
                    parent_x: origin_x,
                    parent_y: origin_y,
                    visual_offset_x,
                    visual_offset_y,
                    available_width: child_available_width,
                    available_height: child_available_height,
                    viewport_width,
                    viewport_height,
                    percent_base_width: child_percent_base_width,
                    percent_base_height: child_percent_base_height,
                });
            }
            for idx in 0..self.children.len() {
                if !self.child_is_absolute(idx) {
                    continue;
                }
                let child = &mut self.children[idx];
                child.place(LayoutPlacement {
                    parent_x: origin_x,
                    parent_y: origin_y,
                    visual_offset_x,
                    visual_offset_y,
                    available_width: child_available_width,
                    available_height: child_available_height,
                    viewport_width,
                    viewport_height,
                    percent_base_width: child_percent_base_width,
                    percent_base_height: child_percent_base_height,
                });
            }
        }
        self.update_content_size_from_children();
        self.clamp_scroll_offset();
        self.recompute_absolute_descendant_for_hit_test();
        self.pop_child_clip_scope();
    }

    fn place_flex_children(
        &mut self,
        child_available_width: f32,
        child_available_height: f32,
        viewport_width: f32,
        viewport_height: f32,
        child_percent_base_width: Option<f32>,
        child_percent_base_height: Option<f32>,
    ) {
        if self.children.is_empty() {
            return;
        }

        let info = if let Some(info) = self.flex_info.take() {
            info
        } else {
            self.compute_flex_info(
                self.layout_inner_size.width,
                self.layout_inner_size.height,
                viewport_width,
                viewport_height,
            )
        };

        let is_row = matches!(self.computed_style.display_flow_direction(), FlowDirection::Row);
        let main_limit = if is_row {
            self.layout_inner_size.width
        } else {
            self.layout_inner_size.height
        };
        let cross_limit = if is_row {
            self.layout_inner_size.height
        } else {
            self.layout_inner_size.width
        };
        let gap_base = if is_row {
            self.layout_inner_size.width
        } else {
            self.layout_inner_size.height
        };
        let gap = resolve_px(
            self.computed_style.gap,
            gap_base,
            viewport_width,
            viewport_height,
        );
        let origin_x = self.layout_flow_inner_position.x - self.scroll_offset.x;
        let origin_y = self.layout_flow_inner_position.y - self.scroll_offset.y;
        let visual_offset_x = self.core.layout_position.x - self.layout_flow_position.x;
        let visual_offset_y = self.core.layout_position.y - self.layout_flow_position.y;

        let total_cross = info.total_cross;
        let mut cross_cursor =
            cross_start_offset(cross_limit, total_cross, self.computed_style.align_items);

        for (line_idx, line) in info.lines.iter().enumerate() {
            let line_main = info.line_main_sum[line_idx];
            let line_cross = info.line_cross_max[line_idx];
            let (mut main_cursor, distributed_gap) = main_axis_start_and_gap(
                main_limit,
                line_main,
                gap,
                line.len(),
                self.computed_style.display_flow_justify_content(),
            );

            for &child_idx in line {
                let (item_main, item_cross) = info.child_sizes[child_idx];
                let cross_offset =
                    cross_item_offset(line_cross, item_cross, self.computed_style.align_items);
                let (offset_x, offset_y) = if is_row {
                    (main_cursor, cross_cursor + cross_offset)
                } else {
                    (cross_cursor + cross_offset, main_cursor)
                };

                // Implement Stretch
                if self.computed_style.align_items == AlignItems::Stretch {
                    if is_row {
                        self.children[child_idx].set_layout_height(line_cross);
                    } else {
                        self.children[child_idx].set_layout_width(line_cross);
                    }
                }

                self.children[child_idx].set_layout_offset(offset_x, offset_y);
                self.children[child_idx].place(LayoutPlacement {
                    parent_x: origin_x,
                    parent_y: origin_y,
                    visual_offset_x,
                    visual_offset_y,
                    available_width: child_available_width,
                    available_height: child_available_height,
                    viewport_width,
                    viewport_height,
                    percent_base_width: child_percent_base_width,
                    percent_base_height: child_percent_base_height,
                });
                main_cursor += item_main + distributed_gap;
            }

            cross_cursor += line_cross + gap;
        }

        for idx in 0..self.children.len() {
            if !self.child_is_absolute(idx) {
                continue;
            }
            self.children[idx].place(LayoutPlacement {
                parent_x: origin_x,
                parent_y: origin_y,
                visual_offset_x,
                visual_offset_y,
                available_width: child_available_width,
                available_height: child_available_height,
                viewport_width,
                viewport_height,
                percent_base_width: child_percent_base_width,
                percent_base_height: child_percent_base_height,
            });
        }
    }
}

impl Default for Element {
    fn default() -> Self {
        // Use a large default root size so rsx root without explicit size is still visible.
        Self::new(0.0, 0.0, 10_000.0, 10_000.0)
    }
}

fn normalize_corner_radii(mut radii: CornerRadii, width: f32, height: f32) -> CornerRadii {
    radii.top_left = radii.top_left.max(0.0);
    radii.top_right = radii.top_right.max(0.0);
    radii.bottom_right = radii.bottom_right.max(0.0);
    radii.bottom_left = radii.bottom_left.max(0.0);
    let w = width.max(0.0);
    let h = height.max(0.0);
    if w <= 0.0 || h <= 0.0 {
        return CornerRadii::zero();
    }

    let top = radii.top_left + radii.top_right;
    let bottom = radii.bottom_left + radii.bottom_right;
    let left = radii.top_left + radii.bottom_left;
    let right = radii.top_right + radii.bottom_right;

    let mut scale = 1.0_f32;
    if top > w {
        scale = scale.min(w / top);
    }
    if bottom > w {
        scale = scale.min(w / bottom);
    }
    if left > h {
        scale = scale.min(h / left);
    }
    if right > h {
        scale = scale.min(h / right);
    }

    if scale < 1.0 {
        radii.top_left *= scale;
        radii.top_right *= scale;
        radii.bottom_right *= scale;
        radii.bottom_left *= scale;
    }

    radii
}

fn inset_corner_radii(
    radii: CornerRadii,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
) -> CornerRadii {
    CornerRadii {
        top_left: (radii.top_left - left.min(top)).max(0.0),
        top_right: (radii.top_right - right.min(top)).max(0.0),
        bottom_right: (radii.bottom_right - right.min(bottom)).max(0.0),
        bottom_left: (radii.bottom_left - left.min(bottom)).max(0.0),
    }
}

fn colors_close(a: [f32; 4], b: [f32; 4]) -> bool {
    let eps = 0.0001;
    (a[0] - b[0]).abs() < eps
        && (a[1] - b[1]).abs() < eps
        && (a[2] - b[2]).abs() < eps
        && (a[3] - b[3]).abs() < eps
}

fn colors_like_eq(a: &dyn ColorLike, b: &dyn ColorLike) -> bool {
    a.to_rgba_u8() == b.to_rgba_u8()
}

fn rect_to_scissor_rect(rect: Rect) -> Option<[u32; 4]> {
    let left = rect.x.floor().max(0.0) as i64;
    let top = rect.y.floor().max(0.0) as i64;
    let right = (rect.x + rect.width).ceil().max(0.0) as i64;
    let bottom = (rect.y + rect.height).ceil().max(0.0) as i64;
    if right <= left || bottom <= top {
        return None;
    }
    Some([
        left as u32,
        top as u32,
        (right - left) as u32,
        (bottom - top) as u32,
    ])
}

fn intersect_scissor_rects(a: Option<[u32; 4]>, b: Option<[u32; 4]>) -> Option<[u32; 4]> {
    match (a, b) {
        (None, None) => None,
        (Some(rect), None) | (None, Some(rect)) => Some(rect),
        (Some([ax, ay, aw, ah]), Some([bx, by, bw, bh])) => {
            let a_right = ax.saturating_add(aw);
            let a_bottom = ay.saturating_add(ah);
            let b_right = bx.saturating_add(bw);
            let b_bottom = by.saturating_add(bh);
            let left = ax.max(bx);
            let top = ay.max(by);
            let right = a_right.min(b_right);
            let bottom = a_bottom.min(b_bottom);
            if right <= left || bottom <= top {
                return None;
            }
            Some([left, top, right - left, bottom - top])
        }
    }
}

fn approx_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.0001
}

#[allow(clippy::too_many_arguments)]
fn apply_collision(
    collision: Collision,
    boundary: Rect,
    x: &mut f32,
    y: &mut f32,
    width: f32,
    height: f32,
    anchor: AnchorSnapshot,
    left: Option<f32>,
    right: Option<f32>,
    top: Option<f32>,
    bottom: Option<f32>,
) {
    if collision == Collision::None {
        return;
    }

    if matches!(collision, Collision::Flip | Collision::FlipFit) {
        if (*x < boundary.x || *x + width > boundary.x + boundary.width)
            && left.is_some()
            && right.is_none()
        {
            let l = left.unwrap_or(0.0);
            *x = anchor.x + anchor.width - l - width;
        } else if (*x < boundary.x || *x + width > boundary.x + boundary.width)
            && right.is_some()
            && left.is_none()
        {
            let r = right.unwrap_or(0.0);
            *x = anchor.x + r;
        }

        if (*y < boundary.y || *y + height > boundary.y + boundary.height)
            && top.is_some()
            && bottom.is_none()
        {
            let t = top.unwrap_or(0.0);
            *y = anchor.y + anchor.height - t - height;
        } else if (*y < boundary.y || *y + height > boundary.y + boundary.height)
            && bottom.is_some()
            && top.is_none()
        {
            let b = bottom.unwrap_or(0.0);
            *y = anchor.y + b;
        }
    }

    if matches!(collision, Collision::Fit | Collision::FlipFit) {
        let max_x = (boundary.x + boundary.width - width).max(boundary.x);
        let max_y = (boundary.y + boundary.height - height).max(boundary.y);
        *x = (*x).clamp(boundary.x, max_x);
        *y = (*y).clamp(boundary.y, max_y);
    }
}

fn main_axis_start_and_gap(
    main_limit: f32,
    occupied_main: f32,
    base_gap: f32,
    item_count: usize,
    justify: JustifyContent,
) -> (f32, f32) {
    let free = (main_limit - occupied_main).max(0.0);
    match justify {
        JustifyContent::Start => (0.0, base_gap),
        JustifyContent::Center => (free * 0.5, base_gap),
        JustifyContent::End => (free, base_gap),
        JustifyContent::SpaceBetween => {
            if item_count > 1 {
                (0.0, base_gap + free / ((item_count - 1) as f32))
            } else {
                (0.0, 0.0)
            }
        }
        JustifyContent::SpaceAround => {
            if item_count > 0 {
                let space = free / (item_count as f32);
                (space * 0.5, base_gap + space)
            } else {
                (0.0, base_gap)
            }
        }
        JustifyContent::SpaceEvenly => {
            if item_count > 0 {
                let space = free / ((item_count + 1) as f32);
                (space, base_gap + space)
            } else {
                (0.0, base_gap)
            }
        }
    }
}

fn cross_start_offset(limit: f32, occupied: f32, align: AlignItems) -> f32 {
    let free = (limit - occupied).max(0.0);
    match align {
        AlignItems::Start | AlignItems::Stretch => 0.0,
        AlignItems::Center => free * 0.5,
        AlignItems::End => free,
    }
}

fn cross_item_offset(line_cross: f32, item_cross: f32, align: AlignItems) -> f32 {
    let free = (line_cross - item_cross).max(0.0);
    match align {
        AlignItems::Start | AlignItems::Stretch => 0.0,
        AlignItems::Center => free * 0.5,
        AlignItems::End => free,
    }
}

fn trace_layout_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("RFGUI_TRACE_LAYOUT").is_ok())
}

fn resolve_px(length: Length, base: f32, viewport_width: f32, viewport_height: f32) -> f32 {
    length
        .resolve_with_base(Some(base), viewport_width, viewport_height)
        .unwrap_or(0.0)
        .max(0.0)
}

fn resolve_px_with_base(
    length: Length,
    base: Option<f32>,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<f32> {
    length
        .resolve_with_base(base, viewport_width, viewport_height)
        .map(|v| v.max(0.0))
}

fn resolve_signed_px_with_base(
    length: Length,
    base: Option<f32>,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<f32> {
    length.resolve_with_base(base, viewport_width, viewport_height)
}

fn resolve_px_or_zero(
    length: Length,
    base: Option<f32>,
    viewport_width: f32,
    viewport_height: f32,
) -> f32 {
    resolve_px_with_base(length, base, viewport_width, viewport_height).unwrap_or(0.0)
}

fn map_transition_timing(timing: TransitionTiming) -> TimeFunction {
    match timing {
        TransitionTiming::Linear => TimeFunction::Linear,
        TransitionTiming::EaseIn => TimeFunction::EaseIn,
        TransitionTiming::EaseOut => TimeFunction::EaseOut,
        TransitionTiming::EaseInOut => TimeFunction::EaseInOut,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Element, ElementTrait, EventTarget, LayoutConstraints, LayoutPlacement, Layoutable,
        Renderable, UiBuildContext, main_axis_start_and_gap, resolve_px_with_base,
        resolve_signed_px_with_base,
    };
    use crate::Display;
    use crate::style::{ParsedValue, PropertyId, Transition, TransitionProperty, Transitions};
    use crate::transition::{LayoutField, VisualField};
    use crate::view::frame_graph::FrameGraph;
    use crate::{
        AnchorName, Border, BoxShadow, ClipMode, Collision, CollisionBoundary, Color,
        JustifyContent, Length, Operator, Position, Style,
    };

    #[test]
    fn justify_content_space_evenly_distributes_free_space() {
        let (start, gap) = main_axis_start_and_gap(100.0, 40.0, 0.0, 3, JustifyContent::SpaceEvenly);
        assert!((start - 15.0).abs() < 0.001);
        assert!((gap - 15.0).abs() < 0.001);
    }

    #[test]
    fn child_layout_uses_parent_inner_box_with_padding() {
        let mut root = Element::new(10.0, 20.0, 200.0, 120.0);
        root.set_padding_left(8.0);
        root.set_padding_top(12.0);
        root.set_padding_right(16.0);
        root.set_padding_bottom(10.0);

        let child = Element::new(4.0, 6.0, 300.0, 300.0);
        root.add_child(Box::new(child));

        root.measure(crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        root.place(crate::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        let children = root.children().expect("element has children");
        let snapshot = children[0].box_model_snapshot();

        assert_eq!(snapshot.x, 22.0);
        assert_eq!(snapshot.y, 38.0);
        assert_eq!(snapshot.width, 300.0);
        assert_eq!(snapshot.height, 300.0);
    }

    #[test]
    fn content_box_subtracts_border_and_padding() {
        let mut root = Element::new(10.0, 20.0, 200.0, 120.0);
        let mut style = Style::new();
        style.set_border(Border::uniform(Length::px(5.0), &Color::hex("#000000")));
        root.apply_style(style);
        root.set_padding_left(8.0);
        root.set_padding_top(12.0);
        root.set_padding_right(16.0);
        root.set_padding_bottom(10.0);

        let child = Element::new(0.0, 0.0, 300.0, 300.0);
        root.add_child(Box::new(child));

        root.measure(crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        root.place(crate::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        let children = root.children().expect("element has children");
        let snapshot = children[0].box_model_snapshot();

        assert_eq!(snapshot.x, 23.0);
        assert_eq!(snapshot.y, 37.0);
        assert_eq!(snapshot.width, 300.0);
        assert_eq!(snapshot.height, 300.0);
    }

    #[test]
    fn percent_child_size_works_with_definite_containing_size() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 123.0, 77.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Width,
            ParsedValue::Length(Length::percent(50.0)),
        );
        child_style.insert(
            PropertyId::Height,
            ParsedValue::Length(Length::percent(50.0)),
        );
        child.apply_style(child_style);
        parent.add_child(Box::new(child));

        parent.measure(crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(crate::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        let snapshot_unknown = parent.children().expect("child")[0].box_model_snapshot();
        assert_eq!(snapshot_unknown.width, 400.0);
        assert_eq!(snapshot_unknown.height, 300.0);

        let mut known_parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut known_parent_style = Style::new();
        known_parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
        known_parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
        known_parent.apply_style(known_parent_style);

        let mut child2 = Element::new(0.0, 0.0, 123.0, 77.0);
        let mut child2_style = Style::new();
        child2_style.insert(
            PropertyId::Width,
            ParsedValue::Length(Length::percent(50.0)),
        );
        child2_style.insert(
            PropertyId::Height,
            ParsedValue::Length(Length::percent(50.0)),
        );
        child2.apply_style(child2_style);
        known_parent.add_child(Box::new(child2));

        known_parent.measure(crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        known_parent.place(crate::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        let snapshot_known = known_parent.children().expect("child")[0].box_model_snapshot();
        assert_eq!(snapshot_known.width, 120.0);
        assert_eq!(snapshot_known.height, 60.0);
    }

    #[test]
    fn calc_percent_and_px_resolves_against_parent_content_size() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 50.0, 40.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Width,
            ParsedValue::Length(Length::calc(
                Length::percent(100.0),
                Operator::subtract,
                Length::px(20.0),
            )),
        );
        child.apply_style(child_style);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });
        parent.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });

        let snapshot = parent.children().expect("child")[0].box_model_snapshot();
        assert_eq!(snapshot.width, 220.0);
    }

    #[test]
    fn calc_with_percent_resolves_when_containing_size_is_definite() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 77.0, 40.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Width,
            ParsedValue::Length(Length::calc(
                Length::percent(100.0),
                Operator::subtract,
                Length::px(20.0),
            )),
        );
        child.apply_style(child_style);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });
        parent.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });

        let snapshot = parent.children().expect("child")[0].box_model_snapshot();
        assert_eq!(snapshot.width, 780.0);
    }

    #[test]
    fn calc_with_percent_falls_back_to_auto_when_containing_size_is_indefinite() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 77.0, 40.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Width,
            ParsedValue::Length(Length::calc(
                Length::percent(100.0),
                Operator::subtract,
                Length::px(20.0),
            )),
        );
        child.apply_style(child_style);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: None,
            percent_base_height: None,
        });
        parent.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: None,
            percent_base_height: None,
        });

        let snapshot = parent.children().expect("child")[0].box_model_snapshot();
        assert_eq!(snapshot.width, 77.0);
    }

    #[test]
    fn calc_nested_with_multiply_and_add_is_supported() {
        let mut el = Element::new(0.0, 0.0, 10.0, 10.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::Width,
            ParsedValue::Length(Length::calc(
                Length::percent(100.0),
                Operator::plus,
                Length::calc(Length::px(10.0), Operator::multiply, 5),
            )),
        );
        el.apply_style(style);

        el.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });
        el.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });

        assert_eq!(el.box_model_snapshot().width, 850.0);
    }

    #[test]
    fn vh_child_size_resolves_against_viewport_height() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 10.0, 10.0);
        let mut child_style = Style::new();
        child_style.insert(PropertyId::Width, ParsedValue::Length(Length::vh(50.0)));
        child_style.insert(PropertyId::Height, ParsedValue::Length(Length::vh(50.0)));
        child.apply_style(child_style);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        let snapshot = parent.children().expect("child")[0].box_model_snapshot();
        assert_eq!(snapshot.width, 300.0);
        assert_eq!(snapshot.height, 300.0);
    }

    #[test]
    fn vw_child_size_resolves_against_viewport_width() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 10.0, 10.0);
        let mut child_style = Style::new();
        child_style.insert(PropertyId::Width, ParsedValue::Length(Length::vw(50.0)));
        child_style.insert(PropertyId::Height, ParsedValue::Length(Length::vw(50.0)));
        child.apply_style(child_style);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        let snapshot = parent.children().expect("child")[0].box_model_snapshot();
        assert_eq!(snapshot.width, 400.0);
        assert_eq!(snapshot.height, 400.0);
    }

    #[test]
    fn vh_falls_back_to_zero_when_viewport_is_unknown() {
        assert_eq!(
            resolve_px_with_base(Length::vh(50.0), None, 0.0, 0.0),
            Some(0.0)
        );
        assert_eq!(
            resolve_signed_px_with_base(Length::vh(-20.0), None, 0.0, 0.0),
            Some(0.0)
        );
        assert_eq!(
            resolve_px_with_base(Length::vw(50.0), None, 0.0, 0.0),
            Some(0.0)
        );
        assert_eq!(
            resolve_signed_px_with_base(Length::vw(-20.0), None, 0.0, 0.0),
            Some(0.0)
        );
    }

    #[test]
    fn absolute_child_does_not_affect_auto_parent_size() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);

        let normal_child = Element::new(0.0, 0.0, 80.0, 40.0);
        let mut absolute_child = Element::new(0.0, 0.0, 300.0, 200.0);
        let mut absolute_style = Style::new();
        absolute_style.insert(
            PropertyId::Position,
            ParsedValue::Position(Position::absolute()),
        );
        absolute_child.apply_style(absolute_style);

        parent.add_child(Box::new(normal_child));
        parent.add_child(Box::new(absolute_child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });

        let snapshot = parent.box_model_snapshot();
        assert_eq!(snapshot.width, 80.0);
        assert_eq!(snapshot.height, 40.0);
    }

    #[test]
    fn column_flow_auto_size_uses_cross_for_width_and_main_for_height() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Display,
            ParsedValue::Display(Display::flow().column().no_wrap()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);

        parent.add_child(Box::new(Element::new(0.0, 0.0, 80.0, 30.0)));
        parent.add_child(Box::new(Element::new(0.0, 0.0, 120.0, 10.0)));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });

        let snapshot = parent.box_model_snapshot();
        assert_eq!(snapshot.width, 120.0);
        assert_eq!(snapshot.height, 40.0);
    }

    #[test]
    fn absolute_defaults_to_parent_anchor_and_zero_insets() {
        let mut parent = Element::new(40.0, 60.0, 200.0, 120.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(Position::absolute()),
        );
        child.apply_style(child_style);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });

        let children = parent.children().expect("has child");
        let snapshot = children[0].box_model_snapshot();
        assert_eq!(snapshot.x, 40.0);
        assert_eq!(snapshot.y, 60.0);
    }

    #[test]
    fn absolute_stretch_with_left_right_top_bottom() {
        let mut parent = Element::new(10.0, 20.0, 200.0, 120.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(10.0))
                    .right(Length::px(20.0))
                    .top(Length::px(5.0))
                    .bottom(Length::px(15.0)),
            ),
        );
        child.apply_style(child_style);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });

        let children = parent.children().expect("has child");
        let snapshot = children[0].box_model_snapshot();
        assert_eq!(snapshot.x, 20.0);
        assert_eq!(snapshot.y, 25.0);
        assert_eq!(snapshot.width, 170.0);
        assert_eq!(snapshot.height, 100.0);
    }

    #[test]
    fn absolute_negative_insets_are_preserved() {
        let mut parent = Element::new(10.0, 20.0, 200.0, 120.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(-10.0))
                    .right(Length::px(20.0))
                    .top(Length::px(-5.0))
                    .bottom(Length::px(15.0)),
            ),
        );
        child.apply_style(child_style);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });

        let children = parent.children().expect("has child");
        let snapshot = children[0].box_model_snapshot();
        assert_eq!(snapshot.x, 0.0);
        assert_eq!(snapshot.y, 15.0);
        assert_eq!(snapshot.width, 190.0);
        assert_eq!(snapshot.height, 110.0);
    }

    #[test]
    fn absolute_collision_fit_viewport_clamps_into_view() {
        let mut el = Element::new(0.0, 0.0, 50.0, 30.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(390.0))
                    .top(Length::px(295.0))
                    .collision(Collision::Fit, CollisionBoundary::Viewport),
            ),
        );
        el.apply_style(style);

        el.measure(LayoutConstraints {
            max_width: 400.0,
            max_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        el.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        let snapshot = el.box_model_snapshot();
        assert_eq!(snapshot.x, 350.0);
        assert_eq!(snapshot.y, 270.0);
    }

    #[test]
    fn absolute_clip_viewport_allows_render_outside_parent_bounds() {
        let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(130.0))
                    .top(Length::px(10.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        child.apply_style(child_style);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 400.0,
            max_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        parent.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });

        let children = parent.children().expect("has child");
        let rendered = children[0]
            .as_any()
            .downcast_ref::<Element>()
            .expect("downcast child")
            .core
            .should_render;
        assert!(rendered);
    }

    #[test]
    fn viewport_clipped_absolute_descendant_is_deferred_even_if_parent_is_not_rendered() {
        let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(10.0))
                    .top(Length::px(10.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        child.apply_style(child_style);
        parent.add_child(Box::new(child));
        parent.core.should_render = false;

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(400, 300, wgpu::TextureFormat::Bgra8Unorm);
        parent.build(&mut graph, &mut ctx);

        let deferred = ctx.take_deferred_node_ids();
        let child_id = parent.children().expect("has child")[0].id();
        assert!(deferred.contains(&child_id));
    }

    #[test]
    fn absolute_clip_anchor_parent_falls_back_to_parent_without_anchor() {
        let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(130.0))
                    .top(Length::px(10.0))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        child.apply_style(child_style);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 400.0,
            max_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        parent.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });

        let children = parent.children().expect("has child");
        let rendered = children[0]
            .as_any()
            .downcast_ref::<Element>()
            .expect("downcast child")
            .core
            .should_render;
        assert!(!rendered);
    }

    #[test]
    fn absolute_clip_anchor_parent_uses_anchor_bounds() {
        let mut parent = Element::new(0.0, 0.0, 500.0, 200.0);
        let mut anchor = Element::new(300.0, 20.0, 40.0, 40.0);
        anchor.set_anchor_name(Some(AnchorName::new("menu_button")));

        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .anchor("menu_button")
                    .left(Length::px(50.0))
                    .top(Length::px(0.0))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        child.apply_style(child_style);
        parent.add_child(Box::new(anchor));
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 600.0,
            max_height: 300.0,
            viewport_width: 600.0,
            percent_base_width: Some(600.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        parent.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 600.0,
            available_height: 300.0,
            viewport_width: 600.0,
            percent_base_width: Some(600.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });

        let children = parent.children().expect("has children");
        let rendered = children[1]
            .as_any()
            .downcast_ref::<Element>()
            .expect("downcast child")
            .core
            .should_render;
        assert!(!rendered);
    }

    #[test]
    fn width_and_height_emit_layout_transition_requests() {
        let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::All,
                200,
            ))),
        );
        el.apply_style(style);

        el.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        el.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        let _ = el.take_visual_transition_requests();

        let mut next_style = Style::new();
        next_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(180.0)));
        next_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(90.0)));
        el.apply_style(next_style);
        el.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        el.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });

        let reqs = el.take_layout_transition_requests();
        assert!(reqs.iter().any(|r| r.field == LayoutField::Width));
        assert!(reqs.iter().any(|r| r.field == LayoutField::Height));
    }

    #[test]
    fn reflow_uses_current_rendered_position_as_layout_transition_start() {
        let mut el = Element::new(50.0, 0.0, 100.0, 40.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Position,
                200,
            ))),
        );
        el.apply_style(style);

        let constraints = LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        };
        let placement_at_100 = LayoutPlacement {
            parent_x: 100.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        };

        el.measure(constraints);
        el.place(placement_at_100);
        let _ = el.take_visual_transition_requests();

        // Simulate an in-flight visual offset frame: target rel-x=50, offset=30 => abs x = 180.
        el.set_layout_transition_x(30.0);
        el.place(placement_at_100);
        let _ = el.take_layout_transition_requests();

        // A reflow shifts parent origin and updates target x.
        el.set_position(120.0, 0.0);
        el.place(LayoutPlacement {
            parent_x: 130.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });

        let reqs = el.take_visual_transition_requests();
        let x_req = reqs
            .iter()
            .find(|r| r.field == VisualField::X)
            .expect("x transition request should exist");
        // current abs(180) - new parent_x(130) = 50, target rel-x=120 => offset = -70
        assert!((x_req.from + 70.0).abs() < 0.01);
        assert!((x_req.to - 0.0).abs() < 0.01);
    }

    #[test]
    fn reflow_uses_current_rendered_width_as_layout_transition_start() {
        let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Width,
                200,
            ))),
        );
        el.apply_style(style);

        let constraints = LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        };
        let placement = LayoutPlacement {
            parent_x: 100.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        };

        el.measure(constraints);
        el.place(placement);
        let _ = el.take_layout_transition_requests();

        // Simulate in-flight width frame.
        el.set_layout_transition_width(140.0);
        el.place(placement);
        let _ = el.take_layout_transition_requests();

        // Reflow updates target width while parent origin also changes.
        let mut next_style = Style::new();
        next_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
        el.apply_style(next_style);
        el.measure(constraints);
        el.place(LayoutPlacement {
            parent_x: 130.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });

        let reqs = el.take_layout_transition_requests();
        let w_req = reqs
            .iter()
            .find(|r| r.field == LayoutField::Width)
            .expect("width transition request should exist");
        assert!((w_req.from - 140.0).abs() < 0.01);
        assert!((w_req.to - 220.0).abs() < 0.01);
    }

    #[test]
    fn reflow_uses_current_rendered_height_as_layout_transition_start() {
        let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Height,
                200,
            ))),
        );
        el.apply_style(style);

        let constraints = LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        };
        let placement = LayoutPlacement {
            parent_x: 100.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        };

        el.measure(constraints);
        el.place(placement);
        let _ = el.take_layout_transition_requests();

        // Simulate in-flight height frame.
        el.set_layout_transition_height(70.0);
        el.place(placement);
        let _ = el.take_layout_transition_requests();

        // Reflow updates target height while parent origin also changes.
        let mut next_style = Style::new();
        next_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(160.0)));
        el.apply_style(next_style);
        el.measure(constraints);
        el.place(LayoutPlacement {
            parent_x: 130.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });

        let reqs = el.take_layout_transition_requests();
        let h_req = reqs
            .iter()
            .find(|r| r.field == LayoutField::Height)
            .expect("height transition request should exist");
        assert!((h_req.from - 70.0).abs() < 0.01);
        assert!((h_req.to - 160.0).abs() < 0.01);
    }

    #[test]
    fn snapshot_restore_keeps_layout_transition_inflight_state() {
        let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
        el.has_layout_snapshot = true;
        el.layout_transition_visual_offset_x = 12.5;
        el.layout_transition_visual_offset_y = -3.0;
        el.layout_transition_override_width = Some(140.0);
        el.layout_transition_override_height = Some(55.0);
        el.layout_transition_target_x = Some(30.0);
        el.layout_transition_target_y = Some(8.0);
        el.layout_transition_target_width = Some(180.0);
        el.layout_transition_target_height = Some(80.0);
        el.last_parent_layout_x = 21.0;
        el.last_parent_layout_y = 34.0;

        let snapshot = el.snapshot_state().expect("snapshot should exist");

        let mut restored = Element::new(0.0, 0.0, 100.0, 40.0);
        let ok = restored.restore_state(snapshot.as_ref());
        assert!(ok);

        assert!(restored.has_layout_snapshot);
        assert!((restored.layout_transition_visual_offset_x - 12.5).abs() < 0.001);
        assert!((restored.layout_transition_visual_offset_y + 3.0).abs() < 0.001);
        assert_eq!(restored.layout_transition_override_width, Some(140.0));
        assert_eq!(restored.layout_transition_override_height, Some(55.0));
        assert_eq!(restored.layout_transition_target_x, Some(30.0));
        assert_eq!(restored.layout_transition_target_y, Some(8.0));
        assert_eq!(restored.layout_transition_target_width, Some(180.0));
        assert_eq!(restored.layout_transition_target_height, Some(80.0));
        assert!((restored.last_parent_layout_x - 21.0).abs() < 0.001);
        assert!((restored.last_parent_layout_y - 34.0).abs() < 0.001);
    }

    #[test]
    fn snapshot_restore_preserves_hover_style_state() {
        let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#112233")),
        );
        let mut hover_style = Style::new();
        hover_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#aabbcc")),
        );
        style.set_hover(hover_style);
        el.apply_style(style);
        let _ = el.set_hovered(true);

        let snapshot = el.snapshot_state().expect("snapshot should exist");

        let mut restored = Element::new(0.0, 0.0, 100.0, 40.0);
        let mut restored_style = Style::new();
        restored_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#112233")),
        );
        let mut restored_hover_style = Style::new();
        restored_hover_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#aabbcc")),
        );
        restored_style.set_hover(restored_hover_style);
        restored.apply_style(restored_style);

        let ok = restored.restore_state(snapshot.as_ref());
        assert!(ok);
        assert!(restored.is_hovered);
        assert_eq!(restored.background_color.as_ref().to_rgba_u8(), [170, 187, 204, 255]);
    }

    #[test]
    fn min_and_max_size_clamp_explicit_width_and_height() {
        let mut el = Element::new(0.0, 0.0, 320.0, 20.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(320.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        style.insert(PropertyId::MinWidth, ParsedValue::Length(Length::px(120.0)));
        style.insert(PropertyId::MaxWidth, ParsedValue::Length(Length::px(180.0)));
        style.insert(PropertyId::MinHeight, ParsedValue::Length(Length::px(40.0)));
        style.insert(PropertyId::MaxHeight, ParsedValue::Length(Length::px(60.0)));
        el.apply_style(style);

        el.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });
        el.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });

        let snapshot = el.box_model_snapshot();
        assert_eq!(snapshot.width, 180.0);
        assert_eq!(snapshot.height, 40.0);
    }

    #[test]
    fn percent_min_and_max_size_resolve_against_parent_inner_size() {
        let mut parent = Element::new(0.0, 0.0, 300.0, 200.0);
        let mut child = Element::new(0.0, 0.0, 500.0, 10.0);
        let mut child_style = Style::new();
        child_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(500.0)));
        child_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(10.0)));
        child_style.insert(
            PropertyId::MinWidth,
            ParsedValue::Length(Length::percent(50.0)),
        );
        child_style.insert(
            PropertyId::MaxWidth,
            ParsedValue::Length(Length::percent(60.0)),
        );
        child_style.insert(
            PropertyId::MinHeight,
            ParsedValue::Length(Length::percent(40.0)),
        );
        child_style.insert(
            PropertyId::MaxHeight,
            ParsedValue::Length(Length::percent(45.0)),
        );
        child.apply_style(child_style);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });
        parent.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });

        let child_snapshot = parent.children().expect("child")[0].box_model_snapshot();
        assert_eq!(child_snapshot.width, 180.0);
        assert_eq!(child_snapshot.height, 80.0);
    }

    #[test]
    fn percent_min_and_max_size_do_not_apply_when_parent_size_is_unresolved() {
        let mut parent = Element::new(0.0, 0.0, 100.0, 100.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 20.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
        child_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        child_style.insert(
            PropertyId::MinWidth,
            ParsedValue::Length(Length::percent(60.0)),
        );
        child_style.insert(
            PropertyId::MinHeight,
            ParsedValue::Length(Length::percent(70.0)),
        );
        child_style.insert(
            PropertyId::MaxWidth,
            ParsedValue::Length(Length::percent(10.0)),
        );
        child_style.insert(
            PropertyId::MaxHeight,
            ParsedValue::Length(Length::percent(10.0)),
        );
        child.apply_style(child_style);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });
        parent.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });

        let child_snapshot = parent.children().expect("child")[0].box_model_snapshot();
        assert_eq!(child_snapshot.width, 20.0);
        assert_eq!(child_snapshot.height, 20.0);
    }

    #[test]
    fn min_greater_than_max_uses_min_as_effective_max() {
        let mut el = Element::new(0.0, 0.0, 10.0, 10.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(30.0)));
        style.insert(PropertyId::MinWidth, ParsedValue::Length(Length::px(120.0)));
        style.insert(PropertyId::MaxWidth, ParsedValue::Length(Length::px(90.0)));
        style.insert(PropertyId::MinHeight, ParsedValue::Length(Length::px(50.0)));
        style.insert(PropertyId::MaxHeight, ParsedValue::Length(Length::px(40.0)));
        el.apply_style(style);

        el.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });
        el.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });

        let snapshot = el.box_model_snapshot();
        assert_eq!(snapshot.width, 120.0);
        assert_eq!(snapshot.height, 50.0);
    }

    #[test]
    fn apply_style_syncs_box_shadow_into_element_state() {
        let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
        let mut style = Style::new();
        style.set_box_shadow(vec![
            BoxShadow::new()
                .color(Color::hex("#223344"))
                .offset_x(3.0)
                .offset_y(5.0)
                .blur(8.0)
                .spread(2.0),
            BoxShadow::new().offset(-1.0),
        ]);
        el.apply_style(style);

        assert_eq!(el.computed_style.box_shadow.len(), 2);
        assert_eq!(el.box_shadows.len(), 2);
        assert_eq!(el.box_shadows[0].offset_x, 3.0);
        assert_eq!(el.box_shadows[0].offset_y, 5.0);
        assert_eq!(el.box_shadows[0].blur, 8.0);
        assert_eq!(el.box_shadows[0].spread, 2.0);
        assert_eq!(el.box_shadows[1].offset_x, -1.0);
        assert_eq!(el.box_shadows[1].offset_y, -1.0);
    }
}
