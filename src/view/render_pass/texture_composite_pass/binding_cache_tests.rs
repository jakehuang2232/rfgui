use super::*;

#[test]
#[ignore = "requires a native GPU adapter"]
fn native_composite_bindings_follow_resource_identity_and_expire() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let source = texture.create_view(&Default::default());
    let replacement_view = texture.create_view(&Default::default());
    let make_uniform = || {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    };
    let uniform = make_uniform();
    let mut resources = create_resources(&device, 1, wgpu::TextureFormat::Rgba8Unorm, 1);
    let first = resources.bind_group(&device, &source, &source, &uniform, false);
    queue.write_buffer(&uniform, 0, &[0; 16]);
    let (second, work) = crate::ui::profile_ui_work(|| {
        resources.bind_group(&device, &source.clone(), &source, &uniform, false)
    });
    assert_eq!(first, second);
    assert_eq!(work.composite_bind_group_creations, 0);
    assert_ne!(
        first,
        resources.bind_group(&device, &replacement_view, &source, &uniform, false)
    );
    assert_ne!(
        first,
        resources.bind_group(&device, &source, &replacement_view, &uniform, false)
    );
    assert_ne!(
        first,
        resources.bind_group(&device, &source, &source, &uniform, true)
    );
    assert_ne!(
        first,
        resources.bind_group(&device, &source, &source, &make_uniform(), false)
    );
    for _ in 0..=MAX_COMPOSITE_BIND_GROUP_UNUSED_FRAMES {
        resources.begin_frame();
    }
    assert!(resources.bind_groups.is_empty());
    assert_ne!(
        first,
        resources.bind_group(&device, &source, &source, &uniform, false)
    );
    let mut other_scope = create_resources(&device, 2, wgpu::TextureFormat::Rgba8Unorm, 1);
    assert_ne!(
        first,
        other_scope.bind_group(&device, &source, &source, &uniform, false)
    );
}
