use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use wgpu::{
    Instance, Queue, TextureUsages,
    rwh::{HasDisplayHandle, HasWindowHandle},
};

use crate::ui::{
    BlurEvent, ClickEvent, EventMeta, FocusEvent, KeyDownEvent, KeyEventData, KeyModifiers,
    ImePreeditEvent, KeyUpEvent, MouseButtons as UiMouseButtons, MouseDownEvent, MouseEventData,
    MouseMoveEvent, MouseUpEvent, RsxNode, TextInputEvent,
};
use crate::transition::{
    CHANNEL_LAYOUT_HEIGHT, CHANNEL_LAYOUT_WIDTH, CHANNEL_LAYOUT_X, CHANNEL_LAYOUT_Y,
    CHANNEL_SCROLL_X, CHANNEL_SCROLL_Y, CHANNEL_STYLE_BACKGROUND_COLOR,
    CHANNEL_STYLE_BORDER_BOTTOM_COLOR, CHANNEL_STYLE_BORDER_LEFT_COLOR,
    CHANNEL_STYLE_BORDER_RADIUS, CHANNEL_STYLE_BORDER_RIGHT_COLOR, CHANNEL_STYLE_BORDER_TOP_COLOR,
    CHANNEL_STYLE_COLOR, CHANNEL_STYLE_OPACITY, ChannelId, ClaimMode, LayoutTransitionPlugin,
    ScrollAxis, ScrollTransition, ScrollTransitionPlugin, StyleTransitionPlugin, TrackKey,
    TrackTarget, Transition, TransitionFrame, TransitionHost, TransitionPluginId,
};
use crate::{ColorLike, HexColor};

pub trait WindowHandle: HasWindowHandle + HasDisplayHandle {}
impl<T: HasWindowHandle + HasDisplayHandle> WindowHandle for T {}

pub type Window = Arc<dyn WindowHandle + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

#[derive(Debug, Clone, Default)]
struct InputState {
    focused_node_id: Option<u64>,
    selects: Vec<u64>,
    pointer_capture_node_id: Option<u64>,
    hovered_node_id: Option<u64>,
    mouse_position_viewport: Option<(f32, f32)>,
    pressed_mouse_buttons: HashSet<MouseButton>,
    pressed_keys: HashSet<String>,
}

pub struct ViewportControl<'a> {
    viewport: &'a mut Viewport,
}

impl<'a> ViewportControl<'a> {
    pub fn new(viewport: &'a mut Viewport) -> Self {
        Self { viewport }
    }

    pub fn request_redraw(&mut self) {
        self.viewport.request_redraw();
    }

    pub fn set_focus(&mut self, node_id: Option<u64>) {
        self.viewport.set_focused_node_id(node_id);
    }

    pub fn set_scroll_transition(&mut self, transition: ScrollTransition) {
        self.viewport.scroll_transition = transition;
    }

    pub fn set_selects(&mut self, selects: Vec<u64>) {
        self.viewport.set_selects(selects);
    }

    pub fn start_scroll_track(
        &mut self,
        target: TrackTarget,
        axis: ScrollAxis,
        from: f32,
        to: f32,
    ) -> bool {
        self.viewport.start_scroll_track(target, axis, from, to)
    }

    pub fn cancel_scroll_track(&mut self, target: TrackTarget, axis: ScrollAxis) {
        self.viewport.cancel_scroll_track(target, axis);
    }

    pub fn set_pointer_capture(&mut self, node_id: u64) {
        self.viewport.set_pointer_capture_node_id(Some(node_id));
    }

    pub fn release_pointer_capture(&mut self, node_id: u64) {
        if self.viewport.pointer_capture_node_id() == Some(node_id) {
            self.viewport.set_pointer_capture_node_id(None);
        }
    }
}

struct TransitionHostAdapter<'a> {
    registered_channels: &'a HashSet<ChannelId>,
    claims: &'a mut HashMap<TrackKey<TrackTarget>, TransitionPluginId>,
}

impl TransitionHost<TrackTarget> for TransitionHostAdapter<'_> {
    fn is_channel_registered(&self, channel: ChannelId) -> bool {
        self.registered_channels.contains(&channel)
    }

    fn claim_track(
        &mut self,
        plugin_id: TransitionPluginId,
        key: TrackKey<TrackTarget>,
        mode: ClaimMode,
    ) -> bool {
        if let Some(current) = self.claims.get(&key).copied() {
            if current == plugin_id {
                return true;
            }
            if matches!(mode, ClaimMode::Replace) {
                self.claims.insert(key, plugin_id);
                return true;
            }
            return false;
        }
        self.claims.insert(key, plugin_id);
        true
    }

    fn release_track_claim(&mut self, plugin_id: TransitionPluginId, key: TrackKey<TrackTarget>) {
        if self.claims.get(&key).copied() == Some(plugin_id) {
            self.claims.remove(&key);
        }
    }

    fn release_all_claims(&mut self, plugin_id: TransitionPluginId) {
        self.claims.retain(|_, owner| *owner != plugin_id);
    }
}

pub struct Viewport {
    clear_color: Box<dyn ColorLike>,
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
    redraw_requested: bool,
    frame_stats: FrameStats,
    frame_box_models: Vec<super::base_component::BoxModelSnapshot>,
    input_state: InputState,
    dispatched_focus_node_id: Option<u64>,
    ui_roots: Vec<Box<dyn super::base_component::ElementTrait>>,
    scroll_offsets: HashMap<u64, (f32, f32)>,
    last_rsx_root: Option<RsxNode>,
    transition_channels: HashSet<ChannelId>,
    transition_claims: HashMap<TrackKey<TrackTarget>, TransitionPluginId>,
    scroll_transition_plugin: ScrollTransitionPlugin,
    layout_transition_plugin: LayoutTransitionPlugin,
    style_transition_plugin: StyleTransitionPlugin,
    scroll_transition: ScrollTransition,
    last_transition_tick: Option<Instant>,
}

impl Viewport {
    fn cancel_pointer_interactions(roots: &mut [Box<dyn super::base_component::ElementTrait>]) -> bool {
        let mut changed = false;
        for root in roots.iter_mut() {
            changed |= super::base_component::cancel_pointer_interactions(root.as_mut());
        }
        changed
    }

    fn start_scroll_track(&mut self, target: TrackTarget, axis: ScrollAxis, from: f32, to: f32) -> bool {
        if (to - from).abs() <= 0.001 {
            return false;
        }
        let mut host = TransitionHostAdapter {
            registered_channels: &self.transition_channels,
            claims: &mut self.transition_claims,
        };
        if self
            .scroll_transition_plugin
            .start_scroll_track(&mut host, target, axis, from, to, self.scroll_transition)
            .is_err()
        {
            return false;
        }
        self.request_redraw();
        true
    }

    fn cancel_scroll_track(&mut self, target: TrackTarget, axis: ScrollAxis) {
        let key = TrackKey {
            target,
            channel: axis.channel_id(),
        };
        let mut host = TransitionHostAdapter {
            registered_channels: &self.transition_channels,
            claims: &mut self.transition_claims,
        };
        self.scroll_transition_plugin.cancel_track(key, &mut host);
    }

    fn apply_hover_target(
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
        target: Option<u64>,
    ) -> bool {
        let mut changed = false;
        for root in roots.iter_mut() {
            if super::base_component::update_hover_state(root.as_mut(), target) {
                changed = true;
            }
        }
        changed
    }

    fn save_scroll_states(
        roots: &[Box<dyn super::base_component::ElementTrait>],
        map: &mut HashMap<u64, (f32, f32)>,
    ) {
        for root in roots {
            let offset = root.get_scroll_offset();
            if offset != (0.0, 0.0) {
                map.insert(root.id(), offset);
            }
            if let Some(children) = root.children() {
                Self::save_scroll_states(children, map);
            }
        }
    }

    fn restore_scroll_states(
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
        map: &HashMap<u64, (f32, f32)>,
    ) {
        for root in roots {
            if let Some(offset) = map.get(&root.id()) {
                root.set_scroll_offset(*offset);
            }
            if let Some(children) = root.children_mut() {
                Self::restore_scroll_states(children, map);
            }
        }
    }

    fn apply_scroll_sample(
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
        target: TrackTarget,
        axis: ScrollAxis,
        value: f32,
    ) -> bool {
        for root in roots.iter_mut().rev() {
            if let Some((x, y)) = super::base_component::get_scroll_offset_by_id(root.as_ref(), target) {
                let next = match axis {
                    ScrollAxis::X => (value, y),
                    ScrollAxis::Y => (x, value),
                };
                return super::base_component::set_scroll_offset_by_id(root.as_mut(), target, next);
            }
        }
        false
    }

    fn run_transition_plugins(
        &mut self,
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
    ) -> bool {
        let now = Instant::now();
        let dt = self
            .last_transition_tick
            .map(|last| (now - last).as_secs_f32())
            .unwrap_or(0.0);
        self.last_transition_tick = Some(now);

        let mut style_requests = Vec::new();
        for root in roots.iter_mut() {
            super::base_component::take_style_transition_requests(root.as_mut(), &mut style_requests);
        }
        let mut layout_requests = Vec::new();
        for root in roots.iter_mut() {
            super::base_component::take_layout_transition_requests(root.as_mut(), &mut layout_requests);
        }
        if !style_requests.is_empty() {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transition_channels,
                claims: &mut self.transition_claims,
            };
            for request in style_requests {
                let _ = self.style_transition_plugin.start_style_track(
                    &mut host,
                    request.target,
                    request.field,
                    request.from,
                    request.to,
                    request.transition,
                );
            }
        }
        if !layout_requests.is_empty() {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transition_channels,
                claims: &mut self.transition_claims,
            };
            for request in layout_requests {
                let _ = self.layout_transition_plugin.start_layout_track(
                    &mut host,
                    request.target,
                    request.field,
                    request.from,
                    request.to,
                    request.transition,
                );
            }
        }

        let result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transition_channels,
                claims: &mut self.transition_claims,
            };
            self.scroll_transition_plugin
                .run_tracks(TransitionFrame { dt_seconds: dt }, &mut host)
        };
        let style_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transition_channels,
                claims: &mut self.transition_claims,
            };
            self.style_transition_plugin
                .run_tracks(TransitionFrame { dt_seconds: dt }, &mut host)
        };
        let layout_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transition_channels,
                claims: &mut self.transition_claims,
            };
            self.layout_transition_plugin
                .run_tracks(TransitionFrame { dt_seconds: dt }, &mut host)
        };
        let samples = self.scroll_transition_plugin.take_samples();
        let mut changed = false;
        for sample in samples {
            changed |= Self::apply_scroll_sample(roots, sample.target, sample.axis, sample.value);
        }
        let style_samples = self.style_transition_plugin.take_samples();
        for sample in style_samples {
            for root in roots.iter_mut().rev() {
                if super::base_component::set_style_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value,
                ) {
                    changed = true;
                    break;
                }
            }
        }
        let layout_samples = self.layout_transition_plugin.take_samples();
        for sample in layout_samples {
            for root in roots.iter_mut().rev() {
                if super::base_component::set_layout_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value,
                ) {
                    changed = true;
                    break;
                }
            }
        }

        if result.keep_running || style_result.keep_running || layout_result.keep_running {
            self.request_redraw();
        }
        changed || result.keep_running || style_result.keep_running || layout_result.keep_running
    }

    pub fn new() -> Self {
        Viewport {
            clear_color: Box::new(HexColor::new("#000000")),
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
            redraw_requested: false,
            frame_stats: FrameStats::new_from_env(),
            frame_box_models: Vec::new(),
            input_state: InputState::default(),
            dispatched_focus_node_id: None,
            ui_roots: Vec::new(),
            scroll_offsets: HashMap::new(),
            last_rsx_root: None,
            transition_channels: HashSet::from([
                CHANNEL_SCROLL_X,
                CHANNEL_SCROLL_Y,
                CHANNEL_LAYOUT_X,
                CHANNEL_LAYOUT_Y,
                CHANNEL_LAYOUT_WIDTH,
                CHANNEL_LAYOUT_HEIGHT,
                CHANNEL_STYLE_OPACITY,
                CHANNEL_STYLE_BORDER_RADIUS,
                CHANNEL_STYLE_BACKGROUND_COLOR,
                CHANNEL_STYLE_COLOR,
                CHANNEL_STYLE_BORDER_TOP_COLOR,
                CHANNEL_STYLE_BORDER_RIGHT_COLOR,
                CHANNEL_STYLE_BORDER_BOTTOM_COLOR,
                CHANNEL_STYLE_BORDER_LEFT_COLOR,
            ]),
            transition_claims: HashMap::new(),
            scroll_transition_plugin: ScrollTransitionPlugin::new(),
            layout_transition_plugin: LayoutTransitionPlugin::new(),
            style_transition_plugin: StyleTransitionPlugin::new(),
            scroll_transition: ScrollTransition::new(250).ease_out(),
            last_transition_tick: None,
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

    pub fn set_clear_color(&mut self, clear_color: Box<dyn ColorLike>) {
        self.clear_color = clear_color;
    }

    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor;
    }

    pub fn request_redraw(&mut self) {
        self.redraw_requested = true;
    }

    pub fn redraw_requested(&self) -> bool {
        self.redraw_requested
    }

    pub fn take_redraw_request(&mut self) -> bool {
        std::mem::take(&mut self.redraw_requested)
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
                    crate::view::render_pass::prewarm_text_pipeline(
                        device,
                        queue,
                        self.surface_config.format,
                    );
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

    fn render_render_tree(&mut self, roots: &mut [Box<dyn super::base_component::ElementTrait>]) {
        let frame_start = Instant::now();
        if !self.begin_frame() {
            return;
        }
        self.frame_box_models.clear();
        for root in roots.iter_mut() {
            root.measure(super::base_component::LayoutConstraints {
                max_width: self.surface_config.width as f32,
                max_height: self.surface_config.height as f32,
                percent_base_width: Some(self.surface_config.width as f32),
                percent_base_height: Some(self.surface_config.height as f32),
            });
            root.place(super::base_component::LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                available_width: self.surface_config.width as f32,
                available_height: self.surface_config.height as f32,
                percent_base_width: Some(self.surface_config.width as f32),
                percent_base_height: Some(self.surface_config.height as f32),
            });
            self.frame_box_models
                .extend(super::base_component::collect_box_models(root.as_ref()));
        }
        let mut graph = super::frame_graph::FrameGraph::new();
        let mut ctx = super::base_component::UiBuildContext::new(
            self.surface_config.width,
            self.surface_config.height,
            self.surface_config.format,
        );
        let mut clear_pass = super::frame_graph::ClearPass::new(self.clear_color.to_rgba_f32());
        let output = ctx.allocate_target(&mut graph);
        clear_pass.set_output(output);
        graph.add_pass(clear_pass);
        ctx.set_last_target(output);
        for root in roots.iter_mut() {
            root.build(&mut graph, &mut ctx);
        }
        if graph.compile().is_ok() {
            let _ = graph.execute(self);
        }
        self.end_frame();
        self.frame_stats.record_frame(frame_start.elapsed());
    }

    pub fn render_rsx(&mut self, root: &RsxNode) -> Result<(), String> {
        let needs_rebuild = self.last_rsx_root.as_ref() != Some(root);
        if needs_rebuild {
            // Clear and save current scroll states
            self.scroll_offsets.clear();
            Self::save_scroll_states(&self.ui_roots, &mut self.scroll_offsets);
            let layout_snapshots =
                super::base_component::collect_layout_transition_snapshots(&self.ui_roots);

            self.ui_roots = super::renderer_adapter::rsx_to_elements(root)?;
            self.last_rsx_root = Some(root.clone());

            // Restore scroll states into new elements
            Self::restore_scroll_states(&mut self.ui_roots, &self.scroll_offsets);
            super::base_component::seed_layout_transition_snapshots(
                &mut self.ui_roots,
                &layout_snapshots,
            );
        }
        self.sync_focus_dispatch();
        let mut roots = std::mem::take(&mut self.ui_roots);
        Self::apply_hover_target(&mut roots, self.input_state.hovered_node_id);
        let transition_changed_before_render = self.run_transition_plugins(&mut roots);
        if !roots.is_empty() {
            self.render_render_tree(&mut roots);
        }
        let transition_changed_after_render = self.run_transition_plugins(&mut roots);
        if transition_changed_before_render || transition_changed_after_render {
            self.request_redraw();
        }
        self.ui_roots = roots;
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
        self.frame_state
            .as_ref()
            .map(|frame| &frame.render_texture.texture)
    }

    pub fn frame_box_models(&self) -> &[super::base_component::BoxModelSnapshot] {
        &self.frame_box_models
    }

    pub fn set_focused_node_id(&mut self, node_id: Option<u64>) {
        self.input_state.focused_node_id = node_id;
    }

    pub fn focused_node_id(&self) -> Option<u64> {
        self.input_state.focused_node_id
    }

    pub fn set_pointer_capture_node_id(&mut self, node_id: Option<u64>) {
        self.input_state.pointer_capture_node_id = node_id;
    }

    pub fn pointer_capture_node_id(&self) -> Option<u64> {
        self.input_state.pointer_capture_node_id
    }

    pub fn set_selects(&mut self, selects: Vec<u64>) {
        self.input_state.selects = selects;
    }

    pub fn selects(&self) -> &[u64] {
        &self.input_state.selects
    }

    pub fn set_mouse_position_viewport(&mut self, x: f32, y: f32) {
        self.input_state.mouse_position_viewport = Some((x, y));
    }

    pub fn clear_mouse_position_viewport(&mut self) {
        self.input_state.mouse_position_viewport = None;
        self.input_state.pointer_capture_node_id = None;
        let hover_changed = Self::apply_hover_target(&mut self.ui_roots, None);
        let pointer_changed = Self::cancel_pointer_interactions(&mut self.ui_roots);
        if hover_changed || pointer_changed {
            self.request_redraw();
        }
    }

    pub fn mouse_position_viewport(&self) -> Option<(f32, f32)> {
        self.input_state.mouse_position_viewport
    }

    pub fn set_mouse_button_pressed(&mut self, button: MouseButton, pressed: bool) {
        if pressed {
            self.input_state.pressed_mouse_buttons.insert(button);
        } else {
            self.input_state.pressed_mouse_buttons.remove(&button);
        }
    }

    pub fn is_mouse_button_pressed(&self, button: MouseButton) -> bool {
        self.input_state.pressed_mouse_buttons.contains(&button)
    }

    pub fn pressed_mouse_buttons(&self) -> impl Iterator<Item = MouseButton> + '_ {
        self.input_state.pressed_mouse_buttons.iter().copied()
    }

    pub fn set_key_pressed(&mut self, key: impl Into<String>, pressed: bool) {
        let key = key.into();
        if pressed {
            self.input_state.pressed_keys.insert(key);
        } else {
            self.input_state.pressed_keys.remove(&key);
        }
    }

    pub fn is_key_pressed(&self, key: &str) -> bool {
        self.input_state.pressed_keys.contains(key)
    }

    pub fn pressed_keys(&self) -> impl Iterator<Item = &str> {
        self.input_state.pressed_keys.iter().map(String::as_str)
    }

    pub fn clear_input_state(&mut self) {
        self.set_focused_node_id(None);
        self.sync_focus_dispatch();
        self.input_state = InputState::default();
        self.dispatched_focus_node_id = None;
        let hover_changed = Self::apply_hover_target(&mut self.ui_roots, None);
        let pointer_changed = Self::cancel_pointer_interactions(&mut self.ui_roots);
        if hover_changed || pointer_changed {
            self.request_redraw();
        }
    }

    pub fn dispatch_mouse_down_event(&mut self, button: MouseButton) -> bool {
        let Some((x, y)) = self.mouse_position_viewport() else {
            return false;
        };
        let buttons = self.current_ui_mouse_buttons();
        let mut event = MouseDownEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(to_ui_mouse_button(button)),
                buttons,
                modifiers: current_key_modifiers(),
            },
        };
        let mut roots = std::mem::take(&mut self.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if super::base_component::dispatch_mouse_down_from_hit_test(
                    root.as_mut(),
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        self.ui_roots = roots;
        self.sync_focus_dispatch();
        if handled {
            self.request_redraw();
        } else if self.focused_node_id().is_some() {
            self.set_focused_node_id(None);
            self.sync_focus_dispatch();
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_mouse_up_event(&mut self, button: MouseButton) -> bool {
        let Some((x, y)) = self.mouse_position_viewport() else {
            self.input_state.pointer_capture_node_id = None;
            let changed = Self::cancel_pointer_interactions(&mut self.ui_roots);
            if changed {
                self.request_redraw();
            }
            return false;
        };
        let buttons = self.current_ui_mouse_buttons();
        let mut event = MouseUpEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(to_ui_mouse_button(button)),
                buttons,
                modifiers: current_key_modifiers(),
            },
        };
        let mut roots = std::mem::take(&mut self.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            if let Some(target_id) = control.viewport.pointer_capture_node_id() {
                for root in roots.iter_mut().rev() {
                    if super::base_component::dispatch_mouse_up_to_target(
                        root.as_mut(),
                        target_id,
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                        break;
                    }
                }
                control.viewport.set_pointer_capture_node_id(None);
            } else {
                for root in roots.iter_mut().rev() {
                    if super::base_component::dispatch_mouse_up_from_hit_test(
                        root.as_mut(),
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                        break;
                    }
                }
            }
        }
        self.ui_roots = roots;
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_mouse_move_event(&mut self) -> bool {
        let Some((x, y)) = self.mouse_position_viewport() else {
            return false;
        };
        let mut roots = std::mem::take(&mut self.ui_roots);
        let hover_target = roots
            .iter()
            .rev()
            .find_map(|root| super::base_component::hit_test(root.as_ref(), x, y));
        if self.input_state.hovered_node_id != hover_target {
            self.input_state.hovered_node_id = hover_target;
        }
        let hover_changed = Self::apply_hover_target(&mut roots, hover_target);
        let buttons = self.current_ui_mouse_buttons();
        let mut event = MouseMoveEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                button: None,
                buttons,
                modifiers: current_key_modifiers(),
            },
        };
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            if let Some(target_id) = control.viewport.pointer_capture_node_id() {
                for root in roots.iter_mut().rev() {
                    if super::base_component::dispatch_mouse_move_to_target(
                        root.as_mut(),
                        target_id,
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                        break;
                    }
                }
                if !handled {
                    control.viewport.set_pointer_capture_node_id(None);
                }
            } else {
                for root in roots.iter_mut().rev() {
                    if super::base_component::dispatch_mouse_move_from_hit_test(
                        root.as_mut(),
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                        break;
                    }
                }
            }
        }
        self.ui_roots = roots;
        if handled || hover_changed {
            self.request_redraw();
        }
        handled || hover_changed
    }

    pub fn dispatch_click_event(&mut self, button: MouseButton) -> bool {
        let Some((x, y)) = self.mouse_position_viewport() else {
            return false;
        };
        let buttons = self.current_ui_mouse_buttons();
        let mut event = ClickEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(to_ui_mouse_button(button)),
                buttons,
                modifiers: current_key_modifiers(),
            },
        };
        let mut roots = std::mem::take(&mut self.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if super::base_component::dispatch_click_from_hit_test(
                    root.as_mut(),
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        self.ui_roots = roots;
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_mouse_wheel_event(&mut self, delta_x: f32, delta_y: f32) -> bool {
        let Some((x, y)) = self.mouse_position_viewport() else {
            return false;
        };
        let mut pending_scroll_track: Option<(TrackTarget, (f32, f32), (f32, f32))> = None;
        for root in self.ui_roots.iter_mut().rev() {
            let Some(target_id) = super::base_component::find_scroll_handler_from_hit_test(
                root.as_ref(),
                x,
                y,
                delta_x,
                delta_y,
            ) else {
                continue;
            };
            let Some(from) = super::base_component::get_scroll_offset_by_id(root.as_ref(), target_id) else {
                continue;
            };
            let _ = super::base_component::dispatch_scroll_to_target(
                root.as_mut(),
                target_id,
                delta_x,
                delta_y,
            );
            let Some(to) = super::base_component::get_scroll_offset_by_id(root.as_ref(), target_id) else {
                continue;
            };
            let _ = super::base_component::set_scroll_offset_by_id(root.as_mut(), target_id, from);

            if (to.0 - from.0).abs() > 0.001 || (to.1 - from.1).abs() > 0.001 {
                pending_scroll_track = Some((target_id, from, to));
                break;
            }
        }
        let mut handled = false;
        if let Some((target_id, from, to)) = pending_scroll_track {
            let transition_spec = self.scroll_transition;
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transition_channels,
                claims: &mut self.transition_claims,
            };
            if (to.0 - from.0).abs() > 0.001 {
                let _ = self.scroll_transition_plugin.start_scroll_track(
                    &mut host,
                    target_id,
                    ScrollAxis::X,
                    from.0,
                    to.0,
                    transition_spec,
                );
            }
            if (to.1 - from.1).abs() > 0.001 {
                let _ = self.scroll_transition_plugin.start_scroll_track(
                    &mut host,
                    target_id,
                    ScrollAxis::Y,
                    from.1,
                    to.1,
                    transition_spec,
                );
            }
            handled = true;
        }
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_key_down_event(&mut self, key: String, code: String, repeat: bool) -> bool {
        let Some(target_id) = self.focused_node_id() else {
            return false;
        };
        let mut event = KeyDownEvent {
            meta: EventMeta::new(target_id),
            key: KeyEventData {
                key,
                code,
                repeat,
                modifiers: current_key_modifiers(),
            },
        };
        let mut roots = std::mem::take(&mut self.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if super::base_component::dispatch_key_down_bubble(
                    root.as_mut(),
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        self.ui_roots = roots;
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_key_up_event(&mut self, key: String, code: String, repeat: bool) -> bool {
        let Some(target_id) = self.focused_node_id() else {
            return false;
        };
        let mut event = KeyUpEvent {
            meta: EventMeta::new(target_id),
            key: KeyEventData {
                key,
                code,
                repeat,
                modifiers: current_key_modifiers(),
            },
        };
        let mut roots = std::mem::take(&mut self.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if super::base_component::dispatch_key_up_bubble(
                    root.as_mut(),
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        self.ui_roots = roots;
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_text_input_event(&mut self, text: String) -> bool {
        if text.is_empty() {
            return false;
        }
        let Some(target_id) = self.focused_node_id() else {
            return false;
        };
        let mut event = TextInputEvent {
            meta: EventMeta::new(target_id),
            text,
        };
        let mut roots = std::mem::take(&mut self.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if super::base_component::dispatch_text_input_bubble(
                    root.as_mut(),
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        self.ui_roots = roots;
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_ime_preedit_event(
        &mut self,
        text: String,
        cursor: Option<(usize, usize)>,
    ) -> bool {
        let Some(target_id) = self.focused_node_id() else {
            return false;
        };
        let mut event = ImePreeditEvent {
            meta: EventMeta::new(target_id),
            text,
            cursor,
        };
        let mut roots = std::mem::take(&mut self.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if super::base_component::dispatch_ime_preedit_bubble(
                    root.as_mut(),
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        self.ui_roots = roots;
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_focus_event(&mut self, target_id: u64) -> bool {
        let mut event = FocusEvent {
            meta: EventMeta::new(target_id),
        };
        let mut roots = std::mem::take(&mut self.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if super::base_component::dispatch_focus_bubble(
                    root.as_mut(),
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        self.ui_roots = roots;
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_blur_event(&mut self, target_id: u64) -> bool {
        let mut event = BlurEvent {
            meta: EventMeta::new(target_id),
        };
        let mut roots = std::mem::take(&mut self.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if super::base_component::dispatch_blur_bubble(
                    root.as_mut(),
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        self.ui_roots = roots;
        if handled {
            self.request_redraw();
        }
        handled
    }

    fn current_ui_mouse_buttons(&self) -> UiMouseButtons {
        UiMouseButtons {
            left: self.is_mouse_button_pressed(MouseButton::Left),
            right: self.is_mouse_button_pressed(MouseButton::Right),
            middle: self.is_mouse_button_pressed(MouseButton::Middle),
            back: self.is_mouse_button_pressed(MouseButton::Back),
            forward: self.is_mouse_button_pressed(MouseButton::Forward),
        }
    }

    pub fn focused_ime_cursor_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let target_id = self.focused_node_id()?;
        for root in self.ui_roots.iter().rev() {
            if let Some(rect) = super::base_component::get_ime_cursor_rect_by_id(
                root.as_ref(),
                target_id,
            ) {
                return Some(rect);
            }
        }
        None
    }

    fn sync_focus_dispatch(&mut self) {
        if self.ui_roots.is_empty() {
            return;
        }

        let desired = self.input_state.focused_node_id;
        let dispatched = self.dispatched_focus_node_id;
        if desired == dispatched {
            return;
        }

        if let Some(prev_id) = dispatched {
            let _ = self.dispatch_blur_event(prev_id);
        }
        if let Some(next_id) = desired {
            let _ = self.dispatch_focus_event(next_id);
        }

        self.dispatched_focus_node_id = desired;
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

        let encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

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

        self.queue
            .as_ref()
            .unwrap()
            .submit(Some(frame.encoder.finish()));
        frame.render_texture.present();
    }
}

fn to_ui_mouse_button(button: MouseButton) -> crate::ui::MouseButton {
    match button {
        MouseButton::Left => crate::ui::MouseButton::Left,
        MouseButton::Right => crate::ui::MouseButton::Right,
        MouseButton::Middle => crate::ui::MouseButton::Middle,
        MouseButton::Back => crate::ui::MouseButton::Back,
        MouseButton::Forward => crate::ui::MouseButton::Forward,
        MouseButton::Other(v) => crate::ui::MouseButton::Other(v),
    }
}

fn current_key_modifiers() -> KeyModifiers {
    KeyModifiers::default()
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

        eprintln!(
            "[perf ] fps={:.1} frame_avg={:.2}ms frames={}",
            fps, avg_ms, self.frames
        );

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
        self.depth_view
            .map(|view| wgpu::RenderPassDepthStencilAttachment {
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
