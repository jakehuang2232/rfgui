//! A render pass whose attachments do not fit together is left out, and the
//! rest of the frame still draws.
use super::*;
use crate::view::frame_graph::{
    AttachmentLoadOp, AttachmentTarget, GraphicsColorAttachmentOps, GraphicsPassBuilder,
    TextureDesc,
};
use crate::view::render_pass::{GraphicsCtx, GraphicsPass};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Draws into the single-sample target with a four-sample depth attachment,
/// and records whether it ran.
struct MismatchedPass {
    target: RenderTargetOut,
    depth: AttachmentTarget,
    ran: Arc<AtomicBool>,
}

impl GraphicsPass for MismatchedPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.write_color(&self.target, GraphicsColorAttachmentOps::load());
        builder.write_depth(self.depth, AttachmentLoadOp::Clear(1.0));
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        self.ran.store(true, Ordering::Relaxed);
    }
}

fn rect(x: f32, color: [f32; 4]) -> DrawRectPass {
    DrawRectPass::new(
        RectPassParams {
            position: [x, 8.0],
            size: [16.0, 16.0],
            fill_color: color,
            opacity: 1.0,
            ..Default::default()
        },
        DrawRectInput::default(),
        DrawRectOutput::default(),
    )
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_pass_whose_attachments_do_not_fit_is_left_out() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native graphics context");
    let mut viewport = Viewport::new();
    for frame in 0..2 {
        let (mut graph, mut ctx, target) = transformed_graph_prelude(1.0, None);
        let depth = graph
            .declare_texture::<()>(
                TextureDesc::new(
                    WIDTH,
                    HEIGHT,
                    wgpu::TextureFormat::Depth24PlusStencil8,
                    wgpu::TextureDimension::D2,
                )
                .with_sample_count(4),
            )
            .handle()
            .ok_or("depth texture has no handle")?;
        let ran = Arc::new(AtomicBool::new(false));
        ctx.emit_draw_rect_pass(&mut graph, rect(8.0, [0.0, 1.0, 0.0, 1.0]));
        graph.add_graphics_pass(MismatchedPass {
            target,
            depth: AttachmentTarget::Texture(depth),
            ran: ran.clone(),
        });
        ctx.emit_draw_rect_pass(&mut graph, rect(40.0, [0.0, 0.0, 1.0, 1.0]));
        add_present(&mut graph, &target)?;
        let pixels =
            render_on_viewport_with_size(graph, gpu, &mut viewport, 1.0, FORMAT, [WIDTH, HEIGHT])?;
        assert!(!ran.load(Ordering::Relaxed), "frame {frame}: the pass ran");
        // The passes before and after it still draw.
        for (x, expected) in [(14, [0, 255, 0, 255]), (46, [0, 0, 255, 255])] {
            let at = ((14 * WIDTH + x) * 4) as usize;
            assert_eq!(&pixels[at..at + 4], expected, "frame {frame}, x {x}");
        }
    }
    assert_eq!(
        viewport.skipped_render_passes_for_test(),
        1,
        "reported once"
    );
    Ok(())
}
