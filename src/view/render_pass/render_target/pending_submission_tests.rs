use super::*;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_persistent_retirement_preserves_unsubmitted_attachments() -> Result<(), String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .map_err(|e| e.to_string())?;
    assert!(matches!(
        adapter.get_info().device_type,
        wgpu::DeviceType::IntegratedGpu
            | wgpu::DeviceType::DiscreteGpu
            | wgpu::DeviceType::VirtualGpu
    ));
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
        .map_err(|e| e.to_string())?;
    for resize in [false, true] {
        let mut pool = OffscreenRenderTargetPool::new();
        pool.begin_frame();
        let key = PersistentTextureKey::Generic(472);
        let desc = TextureDesc::new(
            8,
            8,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureDimension::D2,
        );
        let bundle = pool
            .acquire_persistent(&device, key, desc.clone(), 1)
            .unwrap();
        let entry = pool
            .entries
            .get(&pool.persistent_bindings[&key].entry_id)
            .unwrap();
        let texture = entry.texture.clone();
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let colors = [Some(wgpu::RenderPassColorAttachment {
                view: &bundle.view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::RED),
                    store: wgpu::StoreOp::Store,
                },
            })];
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &colors,
                ..Default::default()
            });
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 256 * 8,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(8),
                },
            },
            wgpu::Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
        if resize {
            let new_desc = TextureDesc::new(
                16,
                8,
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureDimension::D2,
            );
            pool.acquire_persistent(&device, key, new_desc.clone(), 1)
                .unwrap();
            assert!(!pool.has_compatible_persistent(key, &desc, 1));
            assert!(pool.has_compatible_persistent(key, &new_desc, 1));
        } else {
            assert!(pool.release_persistent_pair(key));
            assert!(!pool.has_compatible_persistent(key, &desc, 1));
            assert!(!pool.release_persistent_pair(key));
        }
        assert_eq!(pool.retired_frame_entries.len(), 1);
        // Releasing residency before submission must not destroy attachments
        // already encoded. Both targeted release and descriptor replacement
        // used to invalidate this command buffer at Queue::submit.
        queue.submit([encoder.finish()]);
        // The next frame may start before GPU completion. Submission is the
        // lifetime boundary; no device-wide wait is needed to retire backing.
        pool.begin_frame();
        assert!(
            pool.retired_frame_entries.is_empty(),
            "retired backing survives only until next frame"
        );
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                tx.send(result).unwrap();
            });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| e.to_string())?;
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let data = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|e| e.to_string())?;
        for row in data.chunks_exact(256) {
            for pixel in row[..32].chunks_exact(4) {
                assert_eq!(pixel, [255, 0, 0, 255]);
            }
        }
        drop(data);
        buffer.unmap();
        pool.clear();
    }
    Ok(())
}
