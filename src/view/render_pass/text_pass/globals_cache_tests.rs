use super::*;

#[test]
#[ignore = "requires a native GPU adapter"]
fn native_text_globals_reuse_tracks_buffer_identity_and_releases_stale_entries() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        flags: wgpu::InstanceFlags::VALIDATION,
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("native GPU adapter required");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("text globals cache test"),
        ..Default::default()
    }))
    .expect("create GPU device");
    let screen = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("screen"),
        size: std::mem::size_of::<ScreenUniform>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let make_fragments = |count| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fragments"),
            size: (std::mem::size_of::<FragmentUniform>() * count) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    };
    let fragments = make_fragments(1);
    let mut resources = TextResources::default();
    resources.ensure_common(&device);
    let first = resources.globals_bind_group(&device, &screen, &fragments);
    resources.begin_frame();
    queue.write_buffer(
        &screen,
        0,
        bytemuck::bytes_of(&ScreenUniform {
            screen_size: [640.0, 480.0],
            _pad: [0.0, 0.0],
        }),
    );
    // Uploading new contents keeps the bindings to the same live resources.
    assert_eq!(
        first,
        resources.globals_bind_group(&device, &screen.clone(), &fragments.clone())
    );
    let replacement = make_fragments(1);
    fragments.destroy();
    assert_ne!(
        first,
        resources.globals_bind_group(&device, &screen, &replacement)
    );
    let resized = make_fragments(2);
    assert_ne!(
        first,
        resources.globals_bind_group(&device, &screen, &resized)
    );
    let replacement_screen = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: screen.size(),
        usage: screen.usage(),
        mapped_at_creation: false,
    });
    assert_ne!(
        first,
        resources.globals_bind_group(&device, &replacement_screen, &replacement)
    );

    for _ in 0..=MAX_TEXT_GLOBALS_UNUSED_FRAMES {
        resources.begin_frame();
    }
    assert!(resources.globals_cache.is_empty());

    let before_reset = resources.globals_bind_group(&device, &screen, &replacement);
    resources.destroy();
    assert!(resources.globals_cache.is_empty());
    resources.ensure_common(&device);
    assert_ne!(
        before_reset,
        resources.globals_bind_group(&device, &screen, &replacement)
    );

    // More unique live allocations than the cache budget still get bindings.
    for _ in 0..MAX_TEXT_GLOBALS_CACHE_ENTRIES {
        resources.globals_bind_group(&device, &screen, &make_fragments(1));
    }
    assert_eq!(
        resources.globals_cache.len(),
        MAX_TEXT_GLOBALS_CACHE_ENTRIES
    );
    let overflow = make_fragments(1);
    let uncached = resources.globals_bind_group(&device, &screen, &overflow);
    assert_ne!(
        uncached,
        resources.globals_bind_group(&device, &screen, &overflow)
    );
    assert_eq!(
        resources.globals_cache.len(),
        MAX_TEXT_GLOBALS_CACHE_ENTRIES
    );
    resources.destroy();
}
