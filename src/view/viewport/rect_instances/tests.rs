use super::*;
use crate::view::render_pass::draw_rect_pass::RECT_INSTANCE_STRIDE;

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
    .map_err(|error| format!("rect instance test requires a GPU adapter: {error:?}"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("rfgui rect instance stream test device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        experimental_features: wgpu::ExperimentalFeatures::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
    }))
    .map_err(|error| format!("failed to create rect instance test device: {error:?}"))?;
    Ok((instance, device, queue))
}

fn solid_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("rect instance test layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(RECT_INSTANCE_STRIDE),
            },
            count: None,
        }],
    })
}

#[test]
#[ignore = "requires a native GPU adapter"]
fn later_flushes_upload_only_the_tail_until_growth_replaces_the_buffer() -> Result<(), String> {
    let (_instance, device, queue) = request_gpu()?;
    let layout = solid_layout(&device);
    let instance = <RectInstance as bytemuck::Zeroable>::zeroed();
    let capacity = (RECT_INSTANCE_BUFFER_INITIAL_CAPACITY / RECT_INSTANCE_STRIDE) as usize;
    let mut viewport = Viewport::new();
    viewport.begin_offscreen_test_frame(
        device.clone(),
        queue.clone(),
        4,
        4,
        wgpu::TextureFormat::Rgba8Unorm,
    )?;
    let ((), work) = crate::ui::profile_ui_work(|| {
        assert_eq!(viewport.push_rect_instance(instance), 0);
        assert!(viewport.flush_rect_instance_uploads());
    });
    assert_eq!(work.rect_instance_uploads, 1);
    let (_, _, first) = viewport.rect_instance_stream_for_test();
    let first = first.expect("first flush allocates instance storage");
    assert_eq!(first.size(), RECT_INSTANCE_BUFFER_INITIAL_CAPACITY);
    let first_group = viewport
        .rect_bind_group(7, &layout, false)
        .expect("bind instance storage");

    // A second flush in the same frame copies only the new tail into the same
    // buffer, so bind groups resolved earlier stay valid.
    let ((), work) = crate::ui::profile_ui_work(|| {
        assert_eq!(viewport.push_rect_instance(instance), 1);
        assert!(viewport.flush_rect_instance_uploads());
        assert!(
            viewport.flush_rect_instance_uploads(),
            "nothing new to upload"
        );
    });
    assert_eq!(work.rect_instance_uploads, 1);
    let (staged, uploaded, buffer) = viewport.rect_instance_stream_for_test();
    assert_eq!((staged, uploaded), (2, 2));
    assert_eq!(buffer.as_ref(), Some(&first));
    assert_eq!(
        viewport.rect_bind_group(7, &layout, false).as_ref(),
        Some(&first_group)
    );

    // Outgrowing the buffer (by more than one doubling) replaces it and
    // re-uploads the whole frame; cached bind groups are rebuilt.
    let ((), work) = crate::ui::profile_ui_work(|| {
        for expected in 2..capacity * 5 {
            assert_eq!(viewport.push_rect_instance(instance), expected as u32);
        }
        assert!(viewport.flush_rect_instance_uploads());
    });
    assert_eq!(work.rect_instance_uploads, 1);
    let (staged, uploaded, grown) = viewport.rect_instance_stream_for_test();
    assert_eq!((staged, uploaded), (capacity * 5, capacity * 5));
    let grown = grown.expect("grown instance storage");
    assert_ne!(grown, first);
    assert_eq!(grown.size(), RECT_INSTANCE_BUFFER_INITIAL_CAPACITY * 8);
    let grown_group = viewport
        .rect_bind_group(7, &layout, false)
        .expect("bind grown instance storage");
    assert_ne!(grown_group, first_group);
    assert!(!viewport.has_gradient_stops_buffer_for_test());
    viewport.end_offscreen_test_frame()?;

    // The next frame restarts indices but keeps the grown buffer and its
    // bind groups.
    viewport.begin_offscreen_test_frame(
        device.clone(),
        queue.clone(),
        4,
        4,
        wgpu::TextureFormat::Rgba8Unorm,
    )?;
    let (staged, uploaded, buffer) = viewport.rect_instance_stream_for_test();
    assert_eq!((staged, uploaded), (0, 0));
    assert_eq!(buffer.as_ref(), Some(&grown));
    assert_eq!(viewport.push_rect_instance(instance), 0);
    assert!(viewport.flush_rect_instance_uploads());
    assert_eq!(
        viewport.rect_instance_stream_for_test().2.as_ref(),
        Some(&grown)
    );
    assert_eq!(
        viewport.rect_bind_group(7, &layout, false).as_ref(),
        Some(&grown_group)
    );
    viewport.end_offscreen_test_frame()?;
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| format!("GPU wait failed: {error:?}"))?;
    Ok(())
}
