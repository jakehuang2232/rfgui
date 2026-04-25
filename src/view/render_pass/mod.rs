use crate::view::frame_graph::{
    ComputePassBuilder, ComputeRecordContext, GraphicsPassBuilder, GraphicsRecordContext,
    PrepareContext, TransferPassBuilder, TransferRecordContext,
};
use crate::view::viewport::Viewport;
use wgpu::util::DeviceExt;

pub mod blur_module;
pub mod clear_pass;
pub mod composite_layer_pass;
pub mod debug_overlay_pass;
pub mod draw_rect_pass;
pub mod present_surface_pass;
mod rect_shader;
pub mod render_target;
pub mod shadow_module;
pub mod text_pass;
pub mod texture_composite_pass;
pub use clear_pass::ClearPass;
pub use draw_rect_pass::{DrawRectPass, OpaqueRectPass, RectRenderMode};
pub use shadow_module::{ShadowMesh, ShadowModuleSpec, ShadowParams, build_shadow_module};
pub use text_pass::{TextPass, prewarm_text_pipeline};
pub use texture_composite_pass::{
    TextureCompositeInput, TextureCompositeMaskIn, TextureCompositeOutput, TextureCompositeParams,
    TextureCompositePass, TextureCompositeSourceIn,
};

pub struct GraphicsCtx<'a, 'ctx, 'res, 'pass> {
    frame_resources: &'a mut GraphicsRecordContext<'ctx, 'res>,
    render_pass: &'a mut wgpu::RenderPass<'pass>,
}

impl<'a, 'ctx, 'res, 'pass> GraphicsCtx<'a, 'ctx, 'res, 'pass> {
    pub(crate) fn new(
        frame_resources: &'a mut GraphicsRecordContext<'ctx, 'res>,
        render_pass: &'a mut wgpu::RenderPass<'pass>,
    ) -> Self {
        Self {
            frame_resources,
            render_pass,
        }
    }

    pub fn frame_resources(&mut self) -> &mut GraphicsRecordContext<'ctx, 'res> {
        self.frame_resources
    }

    pub fn viewport(&mut self) -> &mut Viewport {
        self.frame_resources.viewport()
    }

    pub fn set_pipeline(&mut self, pipeline: &wgpu::RenderPipeline) {
        self.render_pass.set_pipeline(pipeline);
    }

    pub fn set_bind_group(
        &mut self,
        index: u32,
        bind_group: &wgpu::BindGroup,
        offsets: &[wgpu::DynamicOffset],
    ) {
        self.render_pass.set_bind_group(index, bind_group, offsets);
    }

    pub fn set_vertex_buffer(&mut self, slot: u32, buffer_slice: wgpu::BufferSlice<'_>) {
        self.render_pass.set_vertex_buffer(slot, buffer_slice);
    }

    pub fn set_index_buffer(
        &mut self,
        buffer_slice: wgpu::BufferSlice<'_>,
        index_format: wgpu::IndexFormat,
    ) {
        self.render_pass
            .set_index_buffer(buffer_slice, index_format);
    }

    pub fn set_scissor_rect(&mut self, x: u32, y: u32, width: u32, height: u32) {
        self.render_pass.set_scissor_rect(x, y, width, height);
    }

    pub fn set_stencil_reference(&mut self, reference: u32) {
        self.render_pass.set_stencil_reference(reference);
    }

    pub fn draw(&mut self, vertices: std::ops::Range<u32>, instances: std::ops::Range<u32>) {
        self.render_pass.draw(vertices, instances);
    }

    pub fn draw_indexed(
        &mut self,
        indices: std::ops::Range<u32>,
        base_vertex: i32,
        instances: std::ops::Range<u32>,
    ) {
        self.render_pass
            .draw_indexed(indices, base_vertex, instances);
    }
}

pub struct ComputeCtx<'a, 'ctx, 'res, 'pass> {
    frame_resources: &'a mut ComputeRecordContext<'ctx, 'res>,
    scope: BackendComputeScope<'a, 'pass>,
}

impl<'a, 'ctx, 'res, 'pass> ComputeCtx<'a, 'ctx, 'res, 'pass> {
    pub(crate) fn from_compute_pass(
        frame_resources: &'a mut ComputeRecordContext<'ctx, 'res>,
        compute_pass: &'a mut wgpu::ComputePass<'pass>,
    ) -> Self {
        Self {
            frame_resources,
            scope: BackendComputeScope::Compute(compute_pass),
        }
    }

    pub fn frame_resources(&mut self) -> &mut ComputeRecordContext<'ctx, 'res> {
        self.frame_resources
    }

    pub fn viewport(&mut self) -> &mut Viewport {
        self.frame_resources.viewport()
    }

    #[allow(dead_code)]
    pub(crate) fn raw_compute_pass(&mut self) -> Option<&mut wgpu::ComputePass<'pass>> {
        match &mut self.scope {
            BackendComputeScope::Compute(pass) => Some(*pass),
        }
    }
}

pub struct TransferCtx<'a, 'ctx, 'res> {
    frame_resources: &'a mut TransferRecordContext<'ctx, 'res>,
    encoder: &'a mut wgpu::CommandEncoder,
}

impl<'a, 'ctx, 'res> TransferCtx<'a, 'ctx, 'res> {
    pub(crate) fn new(
        frame_resources: &'a mut TransferRecordContext<'ctx, 'res>,
        encoder: &'a mut wgpu::CommandEncoder,
    ) -> Self {
        Self {
            frame_resources,
            encoder,
        }
    }

    pub fn frame_resources(&mut self) -> &mut TransferRecordContext<'ctx, 'res> {
        self.frame_resources
    }

    pub fn viewport(&mut self) -> &mut Viewport {
        self.frame_resources.viewport()
    }

    #[allow(dead_code)]
    pub(crate) fn command_encoder(&mut self) -> &mut wgpu::CommandEncoder {
        self.encoder
    }
}

pub trait GraphicsPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>);
    fn prepare(&mut self, _ctx: &mut PrepareContext<'_, '_>) {}
    fn execute(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>);
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

pub trait ComputePass {
    fn setup(&mut self, builder: &mut ComputePassBuilder<'_, '_>);
    fn prepare(&mut self, _ctx: &mut PrepareContext<'_, '_>) {}
    fn execute(&mut self, ctx: &mut ComputeCtx<'_, '_, '_, '_>);
}

pub trait TransferPass {
    fn setup(&mut self, builder: &mut TransferPassBuilder<'_, '_>);
    fn prepare(&mut self, _ctx: &mut PrepareContext<'_, '_>) {}
    fn execute(&mut self, ctx: &mut TransferCtx<'_, '_, '_>);
}

pub(crate) trait PassNodeDyn {
    fn setup(&mut self, builder: &mut crate::view::frame_graph::PassBuilderState<'_>);
    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>);
    fn execute_graphics(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>);
    fn execute_compute(&mut self, ctx: &mut ComputeCtx<'_, '_, '_, '_>);
    fn execute_transfer(&mut self, ctx: &mut TransferCtx<'_, '_, '_>);
    fn name(&self) -> &'static str;
}

enum BackendComputeScope<'a, 'pass> {
    Compute(&'a mut wgpu::ComputePass<'pass>),
}

// ---------------------------------------------------------------------------
// Transient buffer tracking (WebGPU / wasm32)
//
// On WebGPU, GPU buffer destruction relies on JS garbage collection, which
// does not know about GPU memory pressure.  Per-frame `create_buffer_init`
// calls therefore leak until GC decides to run — which in Firefox can mean
// tens of GB of GPU memory growth.
//
// We collect all per-frame ("transient") buffers in a thread-local Vec and
// explicitly call `buffer.destroy()` after `queue.submit()` each frame.
// Destroying a buffer that is in use by already-submitted GPU work is valid
// per the WebGPU specification.
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
std::thread_local! {
    static FRAME_TRANSIENT_BUFFERS: std::cell::RefCell<Vec<wgpu::Buffer>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Create a GPU buffer via `create_buffer_init`, and on wasm32 register it
/// for explicit destruction after the current frame's queue submission.
///
/// Use this for buffers that live only until the end of the current frame
/// (vertex/index/uniform buffers created inside render-pass `execute`).
/// Do **not** use this for buffers that are cached across frames.
pub(crate) fn create_transient_buffer(
    device: &wgpu::Device,
    desc: &wgpu::util::BufferInitDescriptor<'_>,
) -> wgpu::Buffer {
    let buffer = device.create_buffer_init(desc);
    #[cfg(target_arch = "wasm32")]
    FRAME_TRANSIENT_BUFFERS.with(|v| v.borrow_mut().push(buffer.clone()));
    buffer
}

/// Destroy all transient buffers that were created during the current frame.
/// Call once after `queue.submit()`.
#[cfg(target_arch = "wasm32")]
pub(crate) fn destroy_frame_transient_buffers() {
    FRAME_TRANSIENT_BUFFERS.with(|v| {
        for buffer in v.borrow_mut().drain(..) {
            buffer.destroy();
        }
    });
}

pub(crate) struct GraphicsPassWrapper<P: GraphicsPass> {
    pub pass: P,
}

impl<P: GraphicsPass + 'static> PassNodeDyn for GraphicsPassWrapper<P> {
    fn setup(&mut self, builder: &mut crate::view::frame_graph::PassBuilderState<'_>) {
        let mut builder = GraphicsPassBuilder::new(builder);
        self.pass.setup(&mut builder);
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        self.pass.prepare(ctx);
    }

    fn execute_graphics(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        self.pass.execute(ctx);
    }

    fn execute_compute(&mut self, _ctx: &mut ComputeCtx<'_, '_, '_, '_>) {
        unreachable!("graphics pass encoded through compute path");
    }

    fn execute_transfer(&mut self, _ctx: &mut TransferCtx<'_, '_, '_>) {
        unreachable!("graphics pass encoded through transfer path");
    }

    fn name(&self) -> &'static str {
        self.pass.name()
    }
}

pub(crate) struct ComputePassWrapper<P: ComputePass> {
    pub pass: P,
}

impl<P: ComputePass + 'static> PassNodeDyn for ComputePassWrapper<P> {
    fn setup(&mut self, builder: &mut crate::view::frame_graph::PassBuilderState<'_>) {
        let mut builder = ComputePassBuilder::new(builder);
        self.pass.setup(&mut builder);
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        self.pass.prepare(ctx);
    }

    fn execute_graphics(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        unreachable!("compute pass encoded through graphics path");
    }

    fn execute_compute(&mut self, ctx: &mut ComputeCtx<'_, '_, '_, '_>) {
        self.pass.execute(ctx);
    }

    fn execute_transfer(&mut self, _ctx: &mut TransferCtx<'_, '_, '_>) {
        unreachable!("compute pass encoded through transfer path");
    }

    fn name(&self) -> &'static str {
        std::any::type_name::<P>()
    }
}

pub(crate) struct TransferPassWrapper<P: TransferPass> {
    pub pass: P,
}

impl<P: TransferPass + 'static> PassNodeDyn for TransferPassWrapper<P> {
    fn setup(&mut self, builder: &mut crate::view::frame_graph::PassBuilderState<'_>) {
        let mut builder = TransferPassBuilder::new(builder);
        self.pass.setup(&mut builder);
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        self.pass.prepare(ctx);
    }

    fn execute_graphics(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        unreachable!("transfer pass encoded through graphics path");
    }

    fn execute_compute(&mut self, _ctx: &mut ComputeCtx<'_, '_, '_, '_>) {
        unreachable!("transfer pass encoded through compute path");
    }

    fn execute_transfer(&mut self, ctx: &mut TransferCtx<'_, '_, '_>) {
        self.pass.execute(ctx);
    }

    fn name(&self) -> &'static str {
        std::any::type_name::<P>()
    }
}
