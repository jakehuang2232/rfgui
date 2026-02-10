use crate::{Color, HexColor};
use crate::view::frame_graph::{FrameGraph, RenderPass, TextureDesc, InSlot};
use crate::view::frame_graph::texture_resource::TextureHandle;
use crate::view::render_pass::{ClearPass, CompositeLayerPass, DrawRectPass, LayerOut, LayerTag, TextPass};
use crate::view::render_pass::draw_rect_pass::{RenderTargetIn, RenderTargetOut, RenderTargetTag};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy, Debug)]
struct Size {
    width: f32,
    height: f32,
}

pub struct UiBuildContext {
    last_target: Option<RenderTargetOut>,
    color_target: Option<TextureHandle>,
    scissor_rect: Option<[u32; 4]>,
    clip_stack: Vec<Option<[u32; 4]>>,
}

impl UiBuildContext {
    pub fn new(viewport_width: u32, viewport_height: u32) -> Self {
        Self {
            last_target: None,
            color_target: None,
            scissor_rect: Some([0, 0, viewport_width, viewport_height]),
            clip_stack: Vec::new(),
        }
    }

    pub fn allocate_target(&mut self, graph: &mut FrameGraph) -> RenderTargetOut {
        self.next_target(graph)
    }

    pub fn set_last_target(&mut self, target: RenderTargetOut) {
        self.last_target = Some(target);
    }

    fn next_target(&mut self, graph: &mut FrameGraph) -> RenderTargetOut {
        graph.declare_texture::<RenderTargetTag>(TextureDesc::new(
            1,
            1,
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureDimension::D2,
        ))
    }

    fn allocate_layer(&mut self, graph: &mut FrameGraph) -> LayerOut {
        graph.declare_texture::<LayerTag>(TextureDesc::new(
            1,
            1,
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureDimension::D2,
        ))
    }

    fn color_target(&self) -> Option<TextureHandle> {
        self.color_target
    }

    fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        self.color_target = color_target;
    }

    fn push_clip(&mut self, scissor_rect: Option<[u32; 4]>) {
        self.clip_stack.push(self.scissor_rect);
        if let Some(scissor) = scissor_rect {
            self.scissor_rect = Some(match self.scissor_rect {
                Some(current) => intersect_scissor(current, scissor),
                None => scissor,
            });
        }
    }

    fn pop_clip(&mut self) {
        if let Some(scissor_rect) = self.clip_stack.pop() {
            self.scissor_rect = scissor_rect;
        }
    }

    fn scissor_rect(&self) -> Option<[u32; 4]> {
        self.scissor_rect
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

pub trait ElementTrait {
    fn calculate_layout(
        &mut self,
        available_width: f32,
        available_height: f32,
        parent_x: f32,
        parent_y: f32,
    );
    fn build(&mut self, graph: &mut FrameGraph, ctx: &mut UiBuildContext);
}

pub struct Element {
    position: Position,
    size: Size,
    layout_position: Position,
    layout_size: Size,
    should_render: bool,
    background_color: Box<dyn Color>,
    border_color: Box<dyn Color>,
    border_width: f32,
    border_radius: f32,
    opacity: f32,
    children: Vec<Box<dyn ElementTrait>>,
}

impl ElementTrait for Element {
    fn calculate_layout(
        &mut self,
        available_width: f32,
        available_height: f32,
        parent_x: f32,
        parent_y: f32,
    ) {
        let available_width = available_width.max(0.0);
        let available_height = available_height.max(0.0);
        self.layout_size = Size {
            width: self.size.width.max(0.0),
            height: self.size.height.max(0.0),
        };
        self.layout_position = Position {
            x: parent_x + self.position.x,
            y: parent_y + self.position.y,
        };

        let parent_left = parent_x;
        let parent_top = parent_y;
        let parent_right = parent_x + available_width;
        let parent_bottom = parent_y + available_height;
        let self_left = self.layout_position.x;
        let self_top = self.layout_position.y;
        let self_right = self.layout_position.x + self.layout_size.width;
        let self_bottom = self.layout_position.y + self.layout_size.height;
        self.should_render = self.layout_size.width > 0.0
            && self.layout_size.height > 0.0
            && self_right > parent_left
            && self_left < parent_right
            && self_bottom > parent_top
            && self_top < parent_bottom;
        if trace_layout_enabled() {
            eprintln!(
                "[layout] pos=({:.1},{:.1}) size=({:.1},{:.1}) parent=({:.1},{:.1},{:.1},{:.1}) should_render={}",
                self.layout_position.x,
                self.layout_position.y,
                self.layout_size.width,
                self.layout_size.height,
                parent_left,
                parent_top,
                parent_right,
                parent_bottom,
                self.should_render
            );
        }

        for child in &mut self.children {
            child.calculate_layout(
                self.layout_size.width,
                self.layout_size.height,
                self.layout_position.x,
                self.layout_position.y,
            );
        }
    }

    fn build(&mut self, graph: &mut FrameGraph, ctx: &mut UiBuildContext) {
        if trace_layout_enabled() {
            eprintln!(
                "[build ] pos=({:.1},{:.1}) size=({:.1},{:.1}) should_render={}",
                self.layout_position.x,
                self.layout_position.y,
                self.layout_size.width,
                self.layout_size.height,
                self.should_render
            );
        }
        if !self.should_render {
            return;
        }

        let max_r = (self.layout_size.width.min(self.layout_size.height)) * 0.5;
        let radius = self.border_radius.clamp(0.0, max_r);
        let use_layer = self.opacity < 1.0 || radius > 0.0;

        let previous_color_target = ctx.color_target();
        let layer = if use_layer {
            let layer = ctx.allocate_layer(graph);
            let Some(layer_handle) = layer.handle() else {
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

        let scissor = rect_to_scissor(
            self.layout_position.x,
            self.layout_position.y,
            self.layout_size.width.max(0.0),
            self.layout_size.height.max(0.0),
        );
        ctx.push_clip(Some(scissor));

        for child in &mut self.children {
            child.build(graph, ctx);
        }
        ctx.pop_clip();

        if let Some(layer) = layer {
            ctx.set_color_target(previous_color_target);
            let composite = CompositeLayerPass::new(
                [self.layout_position.x, self.layout_position.y],
                [self.layout_size.width, self.layout_size.height],
                radius,
                self.opacity.clamp(0.0, 1.0),
                layer,
            );
            ctx.push_pass(graph, composite);
        }
    }
}

impl Element {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Element {
            position: Position { x, y },
            size: Size { width, height },
            layout_position: Position { x, y },
            layout_size: Size {
                width: width.max(0.0),
                height: height.max(0.0),
            },
            should_render: true,
            background_color: Box::new(HexColor::new("#FFFFFF")),
            border_color: Box::new(HexColor::new("#000000")),
            border_width: 0.0,
            border_radius: 0.0,
            opacity: 1.0,
            children: Vec::new(),
        }
    }

    pub fn set_position(&mut self, x: f32, y: f32) {
        self.position = Position { x, y };
    }

    pub fn set_x(&mut self, x: f32) {
        self.position.x = x;
    }

    pub fn set_y(&mut self, y: f32) {
        self.position.y = y;
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        self.size = Size { width, height };
    }

    pub fn set_width(&mut self, width: f32) {
        self.size.width = width;
    }

    pub fn set_height(&mut self, height: f32) {
        self.size.height = height;
    }

    pub fn set_background_color<T: Color + 'static>(&mut self, color: T) {
        self.background_color = Box::new(color);
    }

    pub fn set_border_color<T: Color + 'static>(&mut self, color: T) {
        self.border_color = Box::new(color);
    }

    pub fn set_border_width(&mut self, width: f32) {
        self.border_width = width;
    }

    pub fn set_border_radius(&mut self, radius: f32) {
        self.border_radius = radius;
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity;
    }

    pub fn add_child(&mut self, child: Box<dyn ElementTrait>) {
        self.children.push(child);
    }

    fn build_self(&mut self, graph: &mut FrameGraph, ctx: &mut UiBuildContext, force_opaque: bool) {
        let fill_color = self.background_color.to_rgba_f32();
        let border_color = self.border_color.to_rgba_f32();
        let same_color = colors_close(fill_color, border_color);
        let opacity = if force_opaque { 1.0 } else { self.opacity };

        let max_bw = (self.layout_size.width.min(self.layout_size.height)) * 0.5;
        let bw = self.border_width.clamp(0.0, max_bw);
        let border_width = if bw > 0.0 && !same_color { bw } else { 0.0 };

        let max_r = (self.layout_size.width.min(self.layout_size.height)) * 0.5;
        let r = self.border_radius.clamp(0.0, max_r);
        let mut pass = DrawRectPass::new(
            [self.layout_position.x, self.layout_position.y],
            [self.layout_size.width, self.layout_size.height],
            fill_color,
            opacity,
        );
        pass.set_border_color(border_color);
        pass.set_border_width(border_width);
        pass.set_border_radius(r);
        self.push_pass(graph, ctx, pass);
    }

    fn push_pass<P: RenderTargetPass + RenderPass + 'static>(
        &mut self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        pass: P,
    ) {
        ctx.push_pass(graph, pass);
    }
}

impl Default for Element {
    fn default() -> Self {
        // Use a large default root size so rsx root without explicit size is still visible.
        Self::new(0.0, 0.0, 10_000.0, 10_000.0)
    }
}

pub(crate) trait RenderTargetPass {
    fn set_input(&mut self, input: RenderTargetIn);
    fn set_output(&mut self, output: RenderTargetOut);
    fn apply_clip(&mut self, _scissor_rect: Option<[u32; 4]>) {}
    fn set_color_target(&mut self, _color_target: Option<TextureHandle>) {}
}

impl RenderTargetPass for DrawRectPass {
    fn set_input(&mut self, input: RenderTargetIn) {
        DrawRectPass::set_input(self, input);
    }

    fn set_output(&mut self, output: RenderTargetOut) {
        DrawRectPass::set_output(self, output);
    }

    fn apply_clip(&mut self, scissor_rect: Option<[u32; 4]>) {
        DrawRectPass::set_scissor_rect(self, scissor_rect);
    }

    fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        DrawRectPass::set_color_target(self, color_target);
    }
}

impl RenderTargetPass for ClearPass {
    fn set_input(&mut self, input: RenderTargetIn) {
        ClearPass::set_input(self, input);
    }

    fn set_output(&mut self, output: RenderTargetOut) {
        ClearPass::set_output(self, output);
    }

    fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        ClearPass::set_color_target(self, color_target);
    }
}

impl RenderTargetPass for TextPass {
    fn set_input(&mut self, input: RenderTargetIn) {
        TextPass::set_input(self, input);
    }

    fn set_output(&mut self, output: RenderTargetOut) {
        TextPass::set_output(self, output);
    }

    fn apply_clip(&mut self, scissor_rect: Option<[u32; 4]>) {
        TextPass::set_scissor_rect(self, scissor_rect);
    }

    fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        TextPass::set_color_target(self, color_target);
    }
}

impl RenderTargetPass for CompositeLayerPass {
    fn set_input(&mut self, input: RenderTargetIn) {
        CompositeLayerPass::set_input(self, input);
    }

    fn set_output(&mut self, output: RenderTargetOut) {
        CompositeLayerPass::set_output(self, output);
    }

    fn apply_clip(&mut self, scissor_rect: Option<[u32; 4]>) {
        CompositeLayerPass::set_scissor_rect(self, scissor_rect);
    }

    fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        CompositeLayerPass::set_color_target(self, color_target);
    }
}

fn colors_close(a: [f32; 4], b: [f32; 4]) -> bool {
    let eps = 0.0001;
    (a[0] - b[0]).abs() < eps
        && (a[1] - b[1]).abs() < eps
        && (a[2] - b[2]).abs() < eps
        && (a[3] - b[3]).abs() < eps
}

fn rect_to_scissor(x: f32, y: f32, width: f32, height: f32) -> [u32; 4] {
    let x = x.max(0.0).floor() as u32;
    let y = y.max(0.0).floor() as u32;
    let width = width.max(0.0).ceil() as u32;
    let height = height.max(0.0).ceil() as u32;
    [x, y, width, height]
}

fn intersect_scissor(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
    let ax2 = a[0].saturating_add(a[2]);
    let ay2 = a[1].saturating_add(a[3]);
    let bx2 = b[0].saturating_add(b[2]);
    let by2 = b[1].saturating_add(b[3]);

    let x1 = a[0].max(b[0]);
    let y1 = a[1].max(b[1]);
    let x2 = ax2.min(bx2);
    let y2 = ay2.min(by2);

    let width = x2.saturating_sub(x1);
    let height = y2.saturating_sub(y1);
    [x1, y1, width, height]
}

fn trace_layout_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("RUST_GUI_TRACE_LAYOUT").is_ok())
}
