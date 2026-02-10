use std::sync::Arc;
use std::time::{Duration, Instant};

use wgpu::{
    Instance, Queue, TextureUsages,
    rwh::{HasDisplayHandle, HasWindowHandle},
};

use crate::{Color, HexColor};
use crate::ui::RsxNode;

pub trait WindowHandle: HasWindowHandle + HasDisplayHandle {}
impl<T: HasWindowHandle + HasDisplayHandle> WindowHandle for T {}

pub type Window = Arc<dyn WindowHandle + Send + Sync>;

pub struct Viewport {
    clear_color: HexColor<'static>,
    scale_factor: f32,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: wgpu::SurfaceConfiguration,
    device: Option<wgpu::Device>,
    instance: Option<Instance>,
    window: Option<Window>,
    queue: Option<Queue>,
    depth_texture: Option<wgpu::Texture>,
    depth_view: Option<wgpu::TextureView>,
    frame_state: Option<FrameState>,
    pending_size: Option<(u32, u32)>,
    needs_reconfigure: bool,
    frame_stats: FrameStats,
}

impl Viewport {
    pub fn new() -> Self {
        Viewport {
            clear_color: HexColor::new("#000000"),
            scale_factor: 1.0,
            surface: None,
            surface_config: wgpu::SurfaceConfiguration {
                usage: TextureUsages::RENDER_ATTACHMENT
                    | TextureUsages::COPY_SRC
                    | TextureUsages::COPY_DST,
                format: wgpu::TextureFormat::Bgra8Unorm,
                width: 1,
                height: 1,
                present_mode: wgpu::PresentMode::AutoVsync,
                desired_maximum_frame_latency: 1,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
            },
            device: None,
            instance: None,
            window: None,
            queue: None,
            depth_texture: None,
            depth_view: None,
            frame_state: None,
            pending_size: None,
            needs_reconfigure: false,
            frame_stats: FrameStats::new_from_env(),
        }
    }

    pub async fn set_window(&mut self, window: Window) {
        self.window = Some(window);
        if self.device.is_some() {
            self.create_surface().await;
        }
    }

    pub fn set_size(&mut self, mut width: u32, mut height: u32) {
        if width == 0 {
            width = 1;
        }
        if height == 0 {
            height = 1;
        }
        if self.surface_config.width == width
            && self.surface_config.height == height
            && self.pending_size.is_none()
        {
            return;
        }
        self.pending_size = Some((width, height));
        self.needs_reconfigure = true;
    }

    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor;
    }

    pub async fn create_surface(&mut self) {
        if let Some(window) = &self.window {
            let backends = wgpu::Backends::all();

            let instance = Instance::new(&wgpu::InstanceDescriptor {
                backends,
                flags: wgpu::InstanceFlags::empty(),
                memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
                backend_options: wgpu::BackendOptions::default(),
            });

            let mut adapters = instance.enumerate_adapters(backends).await;
            let adapter = adapters.remove(0);

            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    experimental_features: wgpu::ExperimentalFeatures::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                    trace: wgpu::Trace::Off,
                })
                .await
                .unwrap();

            let surface = instance.create_surface(window.clone()).unwrap();
            let caps = surface.get_capabilities(&adapter);
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or(caps.formats[0]);
            self.surface_config.format = format;
            self.surface_config.view_formats = vec![self.surface_config.format];
            if let Some((width, height)) = self.pending_size.take() {
                self.surface_config.width = width;
                self.surface_config.height = height;
            }

            surface.configure(&device, &self.surface_config);

            self.instance = Some(instance);
            self.surface = Some(surface);
            self.device = Some(device);
            self.queue = Some(queue);
            self.create_depth_texture();
            self.needs_reconfigure = false;
            if let Some(device) = self.device.as_ref() {
                if let Some(queue) = self.queue.as_ref() {
                    crate::view::render_pass::prewarm_text_pipeline(device, queue, self.surface_config.format);
                }
            }
        }
    }

    fn apply_pending_reconfigure(&mut self) -> bool {
        if !self.needs_reconfigure {
            return true;
        }
        if let Some((width, height)) = self.pending_size.take() {
            self.surface_config.width = width;
            self.surface_config.height = height;
        }
        let surface = match &self.surface {
            Some(surface) => surface,
            None => return false,
        };
        let device = match &self.device {
            Some(device) => device,
            None => return false,
        };
        surface.configure(device, &self.surface_config);
        self.create_depth_texture();
        self.needs_reconfigure = false;
        true
    }

    fn create_depth_texture(&mut self) {
        let device = match &self.device {
            Some(d) => d,
            None => return,
        };

        let size = wgpu::Extent3d {
            width: self.surface_config.width,
            height: self.surface_config.height,
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.depth_texture = Some(texture);
        self.depth_view = Some(view);
    }

    fn render_render_tree(&mut self, root: &mut dyn super::base_component::ElementTrait) {
        let frame_start = Instant::now();
        if !self.begin_frame() {
            return;
        }
        root.calculate_layout(
            self.surface_config.width as f32,
            self.surface_config.height as f32,
            0.0,
            0.0,
        );
        let mut graph = super::frame_graph::FrameGraph::new();
        let mut ctx = super::base_component::UiBuildContext::new(
            self.surface_config.width,
            self.surface_config.height,
        );
        let mut clear_pass = super::frame_graph::ClearPass::new(self.clear_color.to_rgba_f32());
        let output = ctx.allocate_target(&mut graph);
        clear_pass.set_output(output);
        graph.add_pass(clear_pass);
        ctx.set_last_target(output);
        root.build(&mut graph, &mut ctx);
        if graph.compile().is_ok() {
            let _ = graph.execute(self);
        }
        self.end_frame();
        self.frame_stats.record_frame(frame_start.elapsed());
    }

    pub fn render_rsx(&mut self, root: &RsxNode) -> Result<(), String> {
        let mut render_root = super::renderer_adapter::rsx_to_element(root)?;
        self.render_render_tree(render_root.as_mut());
        Ok(())
    }

    pub fn frame_parts(&mut self) -> Option<FrameParts<'_>> {
        let frame = self.frame_state.as_mut()?;
        Some(FrameParts {
            encoder: &mut frame.encoder,
            view: &frame.view,
            depth_view: frame.depth_view.as_ref(),
        })
    }


    pub fn device(&self) -> Option<&wgpu::Device> {
        self.device.as_ref()
    }

    pub fn queue(&self) -> Option<&Queue> {
        self.queue.as_ref()
    }

    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_config.format
    }

    pub fn surface_size(&self) -> (u32, u32) {
        (self.surface_config.width, self.surface_config.height)
    }

    pub fn frame_texture(&self) -> Option<&wgpu::Texture> {
        self.frame_state.as_ref().map(|frame| &frame.render_texture.texture)
    }
    
    fn begin_frame(&mut self) -> bool {
        if self.frame_state.is_some() {
            return true;
        }
        if !self.apply_pending_reconfigure() {
            return false;
        }

        let surface = match &self.surface {
            Some(s) => s,
            None => return false,
        };
        let device = match &self.device {
            Some(d) => d,
            None => return false,
        };

        let render_texture = match surface.get_current_texture() {
            Ok(texture) => texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                println!("[warn] surface lost, recreate render texture");
                surface.configure(device, &self.surface_config);
                match surface.get_current_texture() {
                    Ok(texture) => texture,
                    Err(_) => return false,
                }
            }
            Err(wgpu::SurfaceError::Timeout) => return false,
            Err(wgpu::SurfaceError::OutOfMemory) => return false,
            Err(wgpu::SurfaceError::Other) => return false,
        };

        let view = render_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        self.frame_state = Some(FrameState {
            render_texture,
            view,
            encoder,
            depth_view: self.depth_view.clone(),
        });
        true
    }

    fn end_frame(&mut self) {
        let frame = match self.frame_state.take() {
            Some(frame) => frame,
            None => return,
        };

        self.queue.as_ref().unwrap().submit(Some(frame.encoder.finish()));
        frame.render_texture.present();
    }
}

struct FrameStats {
    enabled: bool,
    last_report_at: Instant,
    frames: u32,
    total_frame_time: Duration,
}

impl FrameStats {
    fn new_from_env() -> Self {
        Self {
            enabled: std::env::var("RUST_GUI_TRACE_FPS").is_ok(),
            last_report_at: Instant::now(),
            frames: 0,
            total_frame_time: Duration::ZERO,
        }
    }

    fn record_frame(&mut self, frame_time: Duration) {
        if !self.enabled {
            return;
        }

        self.frames += 1;
        self.total_frame_time += frame_time;

        let elapsed = self.last_report_at.elapsed();
        if elapsed < Duration::from_secs(1) {
            return;
        }

        let secs = elapsed.as_secs_f64().max(f64::EPSILON);
        let fps = self.frames as f64 / secs;
        let avg_ms = if self.frames == 0 {
            0.0
        } else {
            (self.total_frame_time.as_secs_f64() * 1000.0) / self.frames as f64
        };

        eprintln!("[perf ] fps={:.1} frame_avg={:.2}ms frames={}", fps, avg_ms, self.frames);

        self.last_report_at = Instant::now();
        self.frames = 0;
        self.total_frame_time = Duration::ZERO;
    }
}

struct FrameState {
    render_texture: wgpu::SurfaceTexture,
    view: wgpu::TextureView,
    encoder: wgpu::CommandEncoder,
    depth_view: Option<wgpu::TextureView>,
}

pub struct FrameParts<'a> {
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub view: &'a wgpu::TextureView,
    pub depth_view: Option<&'a wgpu::TextureView>,
}

impl<'a> FrameParts<'a> {
    pub fn depth_stencil_attachment(
        &self,
        depth_load: wgpu::LoadOp<f32>,
        stencil_load: wgpu::LoadOp<u32>,
    ) -> Option<wgpu::RenderPassDepthStencilAttachment<'a>> {
        self.depth_view.map(|view| wgpu::RenderPassDepthStencilAttachment {
            view,
            depth_ops: Some(wgpu::Operations {
                load: depth_load,
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: Some(wgpu::Operations {
                load: stencil_load,
                store: wgpu::StoreOp::Store,
            }),
        })
    }
}
