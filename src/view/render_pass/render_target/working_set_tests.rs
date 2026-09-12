use super::*;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_transient_texture_working_set_survives_pressure_between_frames() -> Result<(), String> {
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
    // Exercise count and pixel pressure independently. These are soft cache
    // limits: a live frame already may require more than either threshold.
    for (count, extent) in [(68_u32, 8_u32), (4, 2048)] {
        let mut pool = OffscreenRenderTargetPool::new();
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Transient working set probes"),
            size: u64::from(count) * 256,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut first_textures: Vec<wgpu::Texture> = Vec::new();
        for frame in 0..3_u32 {
            let created_before = pool.next_entry_id;
            pool.begin_frame();
            let mut encoder = device.create_command_encoder(&Default::default());
            let mut textures = Vec::new();
            for i in 0..count {
                let desc = TextureDesc::new(
                    extent,
                    extent,
                    wgpu::TextureFormat::Rgba8Unorm,
                    wgpu::TextureDimension::D2,
                )
                .with_label(match i % 4 {
                    0 => "Shadow Layer / Blurred",
                    1 => "Shadow Layer",
                    2 => "Blur Intermediate / Downsample",
                    _ => "Blur Intermediate / Horizontal",
                });
                let bundle = pool.acquire(&device, AllocationId(i), desc, 1).unwrap();
                let texture = pool.entries[&pool.frame_bindings[&i]].texture.clone();
                assert!(
                    !textures.contains(&texture),
                    "live allocations must not alias"
                );
                let color = [i as u8, (frame * 50) as u8, 255, 255];
                {
                    let attachments = [Some(wgpu::RenderPassColorAttachment {
                        view: &bundle.view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: color[0] as f64 / 255.0,
                                g: color[1] as f64 / 255.0,
                                b: 1.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })];
                    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &attachments,
                        ..Default::default()
                    });
                }
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: &readback,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: u64::from(i) * 256,
                            bytes_per_row: Some(256),
                            rows_per_image: Some(1),
                        },
                    },
                    wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                );
                textures.push(texture);
            }
            queue.submit([encoder.finish()]);
            pool.finish_frame();
            let (tx, rx) = std::sync::mpsc::channel();
            readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                tx.send(r).unwrap();
            });
            device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(|e| e.to_string())?;
            rx.recv()
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            let data = readback
                .slice(..)
                .get_mapped_range()
                .map_err(|e| e.to_string())?;
            for i in 0..count {
                assert_eq!(
                    &data[i as usize * 256..i as usize * 256 + 4],
                    [i as u8, (frame * 50) as u8, 255, 255],
                    "fresh pixels, frame {frame}"
                );
            }
            drop(data);
            readback.unmap();
            let created = pool.next_entry_id - created_before;
            eprintln!(
                "transient pool count={count} extent={extent} frame={frame} created={created}"
            );
            if frame == 0 {
                first_textures = textures;
            } else {
                assert_eq!(
                    created, 0,
                    "unchanged working set must survive a frame boundary"
                );
                assert!(textures.iter().all(|t| first_textures.contains(t)));
            }
        }
        // Once the workload shrinks, unused entries must again obey BOTH
        // pressure limits. Deferring eviction must not disable reclamation.
        pool.begin_frame();
        let desc = TextureDesc::new(
            extent,
            extent,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureDimension::D2,
        )
        .with_label("Shadow Layer");
        pool.acquire(&device, AllocationId(0), desc, 1).unwrap();
        let live = pool.frame_bindings[&0];
        queue.submit([]);
        pool.finish_frame();
        assert!(pool.entries.contains_key(&live));
        assert!(pool.entries.len() <= OffscreenRenderTargetPool::MAX_ENTRIES);
        assert!(pool.total_pixels() <= OffscreenRenderTargetPool::MAX_TOTAL_PIXELS);
        for _ in 0..OffscreenRenderTargetPool::EVICT_UNUSED_AFTER_FRAMES {
            pool.begin_frame();
            pool.finish_frame();
        }
        assert!(
            pool.entries.is_empty(),
            "idle expiry must still release all backing"
        );
        pool.clear();
    }
    Ok(())
}
