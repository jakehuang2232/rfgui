use super::Viewport;
use crate::view::ImageSampling;
use crate::view::frame_graph::FrameGraph;
use crate::view::render_pass::TextureCompositePass;
use crate::view::render_pass::texture_composite_pass::{
    TextureCompositeInput, TextureCompositeOutput, TextureCompositeParams,
    texture_composite_resources_cache_len,
};
use crate::view::sampled_texture::{
    ImageAssetId, SampledTextureAlphaMode, SampledTextureId, SampledTextureUpload, SvgRasterAssetId,
};
use std::sync::Arc;

fn request_gpu() -> Result<(wgpu::Instance, wgpu::Device, wgpu::Queue), String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        flags: wgpu::InstanceFlags::empty(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .map_err(|error| format!("sampled texture test requires a GPU adapter: {error:?}"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("rfgui sampled texture residency test device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        experimental_features: wgpu::ExperimentalFeatures::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
    }))
    .map_err(|error| format!("failed to create sampled texture test device: {error:?}"))?;
    Ok((instance, device, queue))
}

fn request_two_devices() -> Result<
    (
        wgpu::Instance,
        wgpu::Device,
        wgpu::Queue,
        wgpu::Device,
        wgpu::Queue,
    ),
    String,
> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        flags: wgpu::InstanceFlags::empty(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .map_err(|error| format!("two-device test requires a GPU adapter: {error:?}"))?;
    let descriptor = wgpu::DeviceDescriptor {
        label: Some("rfgui TextureComposite scope test device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        experimental_features: wgpu::ExperimentalFeatures::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
    };
    let (first_device, first_queue) = pollster::block_on(adapter.request_device(&descriptor))
        .map_err(|error| format!("failed to create first test device: {error:?}"))?;
    let (second_device, second_queue) = pollster::block_on(adapter.request_device(&descriptor))
        .map_err(|error| format!("failed to create second test device: {error:?}"))?;
    Ok((
        instance,
        first_device,
        first_queue,
        second_device,
        second_queue,
    ))
}

fn attach_gpu(viewport: &mut Viewport, device: &wgpu::Device, queue: &wgpu::Queue) {
    viewport.gpu.device = Some(device.clone());
    viewport.gpu.queue = Some(queue.clone());
}

fn upload(id: SampledTextureId, generation: u64) -> SampledTextureUpload {
    SampledTextureUpload {
        id,
        generation,
        width: 2,
        height: 2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        alpha_mode: SampledTextureAlphaMode::Straight,
        pixels: Arc::from([255_u8; 16]),
        sampling: ImageSampling::Linear,
    }
}

fn execute_texture_composite(
    viewport: &mut Viewport,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    sampled_source: SampledTextureUpload,
) -> Result<(), String> {
    viewport.begin_offscreen_test_frame(
        device.clone(),
        queue.clone(),
        4,
        4,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    )?;
    let mut graph = FrameGraph::new();
    graph.add_graphics_pass(TextureCompositePass::new(
        TextureCompositeParams {
            bounds: [0.0, 0.0, 2.0, 2.0],
            ..Default::default()
        },
        TextureCompositeInput::from_sampled_texture(
            sampled_source,
            Default::default(),
            Default::default(),
        ),
        TextureCompositeOutput::default(),
    ));
    graph
        .compile_with_upload(viewport)
        .map_err(|error| format!("TextureComposite graph compile failed: {error:?}"))?;
    graph
        .execute_profiled(viewport, false)
        .map_err(|error| format!("TextureComposite graph execute failed: {error:?}"))?;
    viewport.end_offscreen_test_frame()
}

#[test]
#[ignore = "requires a native GPU adapter"]
fn residency_is_per_viewport_generation_aware_and_recoverable() -> Result<(), String> {
    let (_instance, device, queue) = request_gpu()?;
    let id = SampledTextureId::Image(ImageAssetId::for_test(41));
    let first_generation = upload(id, 1);
    let mut first = Viewport::new();
    let mut second = Viewport::new();
    attach_gpu(&mut first, &device, &queue);
    attach_gpu(&mut second, &device, &queue);

    assert!(first.ensure_sampled_texture(&first_generation));
    assert_eq!(first.frame.sampled_texture_upload_count, 1);
    first.frame.frame_number += 1;
    assert!(first.ensure_sampled_texture(&first_generation));
    assert_eq!(first.frame.sampled_texture_upload_count, 1);

    assert!(second.ensure_sampled_texture(&first_generation));
    assert_eq!(second.frame.sampled_texture_upload_count, 1);

    let next_generation = upload(id, 2);
    assert!(first.ensure_sampled_texture(&next_generation));
    assert_eq!(first.frame.sampled_texture_upload_count, 2);

    first
        .frame
        .sampled_texture_cache
        .get_mut(&id)
        .unwrap()
        .byte_size = Viewport::SAMPLED_TEXTURE_PRESSURE_BYTES + 1;
    first.frame.frame_number += 1;
    first.evict_sampled_textures_under_pressure();
    assert!(!first.frame.sampled_texture_cache.contains_key(&id));
    assert!(first.ensure_sampled_texture(&next_generation));
    assert_eq!(first.frame.sampled_texture_upload_count, 3);

    first.release_render_resource_caches();
    assert!(first.ensure_sampled_texture(&next_generation));
    assert_eq!(first.frame.sampled_texture_upload_count, 4);
    Ok(())
}

#[test]
#[ignore = "requires a native GPU adapter"]
fn current_frame_is_pinned_and_equal_image_svg_ids_do_not_alias() -> Result<(), String> {
    let (_instance, device, queue) = request_gpu()?;
    let mut viewport = Viewport::new();
    attach_gpu(&mut viewport, &device, &queue);
    let image_id = SampledTextureId::Image(ImageAssetId::for_test(9));
    let svg_id = SampledTextureId::SvgRaster(SvgRasterAssetId::for_test(9));

    assert!(viewport.ensure_sampled_texture(&upload(image_id, 1)));
    viewport
        .frame
        .sampled_texture_cache
        .get_mut(&image_id)
        .unwrap()
        .byte_size = Viewport::SAMPLED_TEXTURE_PRESSURE_BYTES + 1;
    assert!(viewport.ensure_sampled_texture(&upload(image_id, 1)));
    assert!(viewport.frame.sampled_texture_cache.contains_key(&image_id));

    assert!(viewport.ensure_sampled_texture(&upload(svg_id, 1)));
    assert!(viewport.frame.sampled_texture_cache.contains_key(&image_id));
    assert!(viewport.frame.sampled_texture_cache.contains_key(&svg_id));
    assert_eq!(viewport.frame.sampled_texture_cache.len(), 2);
    Ok(())
}

#[test]
#[ignore = "requires two native GPU devices"]
fn texture_composite_resources_are_device_scoped_and_drop_reclaimed() -> Result<(), String> {
    let (_instance, first_device, first_queue, second_device, second_queue) =
        request_two_devices()?;
    let baseline = texture_composite_resources_cache_len();
    let mut first = Viewport::new();
    let mut second = Viewport::new();

    execute_texture_composite(
        &mut first,
        &first_device,
        &first_queue,
        upload(SampledTextureId::Image(ImageAssetId::for_test(71)), 1),
    )?;
    assert_eq!(texture_composite_resources_cache_len(), baseline + 1);
    execute_texture_composite(
        &mut second,
        &second_device,
        &second_queue,
        upload(SampledTextureId::Image(ImageAssetId::for_test(72)), 1),
    )?;
    assert_eq!(texture_composite_resources_cache_len(), baseline + 2);

    first.release_render_resource_caches();
    assert_eq!(texture_composite_resources_cache_len(), baseline + 1);
    drop(first);
    execute_texture_composite(
        &mut second,
        &second_device,
        &second_queue,
        upload(SampledTextureId::Image(ImageAssetId::for_test(72)), 1),
    )?;
    assert_eq!(texture_composite_resources_cache_len(), baseline + 1);

    drop(second);
    assert_eq!(texture_composite_resources_cache_len(), baseline);
    Ok(())
}
