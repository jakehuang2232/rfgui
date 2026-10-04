//! Instanced rectangle runs within one wgpu render pass.
//!
//! Each draw-rect logical pass contributes one instance of the frame's
//! instance array. Consecutive logical passes whose instances are contiguous
//! and share pipeline, scissor and stencil reference extend one pending run;
//! the run is recorded as a single instanced draw before any other command
//! reaches the render pass, and when the render pass ends. A single draw
//! rasterizes and blends its instances in order, so ordering semantics
//! (alpha blending, stencil increment/decrement) are unchanged.
//!
//! Like `GraphicsBufferBindings`, this state never survives a render pass.

struct PendingRectRun {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    scissor: [u32; 4],
    stencil_reference: Option<u32>,
    first_instance: u32,
    instance_count: u32,
}

#[derive(Default)]
pub(crate) struct RectDrawBatch {
    pending: Option<PendingRectRun>,
}

impl RectDrawBatch {
    /// Appends `instance` to the pending run when it continues that run with
    /// identical recording state.
    pub(crate) fn try_extend(
        &mut self,
        pipeline: &wgpu::RenderPipeline,
        scissor: [u32; 4],
        stencil_reference: Option<u32>,
        instance: u32,
    ) -> bool {
        let Some(run) = self.pending.as_mut() else {
            return false;
        };
        let continues = run.first_instance.checked_add(run.instance_count) == Some(instance)
            && run.scissor == scissor
            && run.stencil_reference == stencil_reference
            && run.pipeline == *pipeline;
        if continues {
            run.instance_count += 1;
        }
        continues
    }

    /// Starts a new run. Any pending run must have been flushed first.
    pub(crate) fn start(
        &mut self,
        pipeline: wgpu::RenderPipeline,
        bind_group: wgpu::BindGroup,
        scissor: [u32; 4],
        stencil_reference: Option<u32>,
        instance: u32,
    ) {
        debug_assert!(self.pending.is_none(), "flush before starting a rect run");
        self.pending = Some(PendingRectRun {
            pipeline,
            bind_group,
            scissor,
            stencil_reference,
            first_instance: instance,
            instance_count: 1,
        });
    }

    /// Records the pending run, if any, as one instanced draw.
    pub(crate) fn flush(&mut self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(run) = self.pending.take() else {
            return;
        };
        pass.set_pipeline(&run.pipeline);
        pass.set_bind_group(0, &run.bind_group, &[]);
        if let Some(reference) = run.stencil_reference {
            pass.set_stencil_reference(reference);
        }
        let [x, y, width, height] = run.scissor;
        pass.set_scissor_rect(x, y, width, height);
        // The shader emits the six quad corners procedurally per instance.
        pass.draw(
            0..6,
            run.first_instance..run.first_instance + run.instance_count,
        );
        crate::ui::work_profile::count(|p| {
            p.rect_draw_calls += 1;
            p.rect_instances += run.instance_count as usize;
        });
    }
}
