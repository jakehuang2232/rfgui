use crate::render_pass::render_target::RenderTargetPass;
use crate::view::frame_graph::{
    AttachmentLoadOp, AttachmentTarget, GraphicsColorAttachmentDescriptor,
    GraphicsDepthAspectDescriptor, GraphicsDepthStencilAttachmentDescriptor,
    GraphicsRecordContext, GraphicsStencilAspectDescriptor, PassBuilder,
};
use crate::view::render_pass::RenderPass;
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::{
    render_target_attachment_view, render_target_msaa_view, render_target_view,
};

pub struct ClearPass {
    depth_stencil_target: Option<AttachmentTarget>,
    params: ClearParams,
    output: ClearOutput,
}

pub struct ClearParams {
    pub color: [f32; 4],
}

impl ClearParams {
    pub fn new(color: [f32; 4]) -> Self {
        Self { color }
    }
}

#[derive(Default)]
pub struct ClearInput;

#[derive(Default)]
pub struct ClearOutput {
    pub render_target: RenderTargetOut,
}

impl ClearPass {
    pub fn new(params: ClearParams, input: ClearInput, output: ClearOutput) -> Self {
        let _ = input;
        Self {
            depth_stencil_target: None,
            params,
            output,
        }
    }
}

impl RenderPass for ClearPass {
    fn setup(&mut self, builder: &mut PassBuilder<'_>) {
        let color = [
            self.params.color[0] as f64,
            self.params.color[1] as f64,
            self.params.color[2] as f64,
            self.params.color[3] as f64,
        ];
        if let Some(target) = builder.texture_target(&self.output.render_target) {
            builder.declare_color_attachment(
                &self.output.render_target,
                GraphicsColorAttachmentDescriptor::clear(target, color),
            );
            if let Some(depth_stencil_target) = self.depth_stencil_target {
                builder.declare_depth_stencil_attachment(GraphicsDepthStencilAttachmentDescriptor {
                    target: depth_stencil_target,
                    depth: Some(GraphicsDepthAspectDescriptor::write(
                        AttachmentLoadOp::Clear,
                        Some(1.0),
                    )),
                    stencil: Some(GraphicsStencilAspectDescriptor::write(
                        AttachmentLoadOp::Clear,
                        Some(0),
                    )),
                });
            }
        } else {
            builder.declare_surface_color_attachment(GraphicsColorAttachmentDescriptor::clear(
                builder.surface_target(),
                color,
            ));
            builder.declare_depth_stencil_attachment(GraphicsDepthStencilAttachmentDescriptor {
                target: builder.surface_target(),
                depth: Some(GraphicsDepthAspectDescriptor::write(
                    AttachmentLoadOp::Clear,
                    Some(1.0),
                )),
                stencil: Some(GraphicsStencilAspectDescriptor::write(
                    AttachmentLoadOp::Clear,
                    Some(0),
                )),
            });
        }
    }

    fn record(&mut self, ctx: &mut GraphicsRecordContext<'_, '_, '_>) {
        let color = wgpu::Color {
            r: self.params.color[0] as f64,
            g: self.params.color[1] as f64,
            b: self.params.color[2] as f64,
            a: self.params.color[3] as f64,
        };

        let (offscreen_view, offscreen_msaa_view) = match self.output.render_target.handle() {
            Some(handle) => (
                render_target_view(ctx, handle),
                render_target_msaa_view(ctx, handle),
            ),
            None => (None, None),
        };
        let depth_stencil_view = match self.depth_stencil_target {
            Some(AttachmentTarget::Texture(handle)) => render_target_attachment_view(ctx, handle),
            Some(AttachmentTarget::Surface) | None => None,
        };
        let msaa_enabled = ctx.viewport.msaa_sample_count() > 1;
        let parts = match ctx.viewport.frame_parts() {
            Some(parts) => parts,
            None => return,
        };
        let surface_resolve = if msaa_enabled {
            parts.resolve_view
        } else {
            None
        };
        let (color_view, resolve_target) =
            match (offscreen_view.as_ref(), offscreen_msaa_view.as_ref()) {
                (Some(resolve_view), Some(msaa_view)) => (msaa_view, Some(resolve_view)),
                (Some(resolve_view), None) => (resolve_view, None),
                (None, _) => (parts.view, surface_resolve),
            };
        let depth_stencil_attachment = match self.depth_stencil_target {
            Some(AttachmentTarget::Surface) => {
                parts.depth_stencil_attachment(wgpu::LoadOp::Clear(1.0), wgpu::LoadOp::Clear(0))
            }
            Some(AttachmentTarget::Texture(_)) => depth_stencil_view.as_ref().map(|view| {
                wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: wgpu::StoreOp::Store,
                    }),
                }
            }),
            None => None,
        };
        let _pass = parts
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                    resolve_target,
                })],
                depth_stencil_attachment,
                ..Default::default()
            });
    }
}

impl RenderTargetPass for ClearPass {
    fn set_depth_stencil_target(&mut self, depth_stencil_target: Option<AttachmentTarget>) {
        self.depth_stencil_target = depth_stencil_target;
    }
}
