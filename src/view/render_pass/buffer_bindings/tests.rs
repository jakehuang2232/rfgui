use super::*;
use wgpu::util::DeviceExt;

fn vertices(color: [f32; 4]) -> Vec<f32> {
    [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]]
        .into_iter()
        .flat_map(|position| {
            [
                position[0],
                position[1],
                color[0],
                color[1],
                color[2],
                color[3],
            ]
        })
        .collect()
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_buffer_bindings_preserve_ranges_formats_and_pass_scope() -> Result<(), String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .map_err(|error| error.to_string())?;
    assert!(matches!(
        adapter.get_info().device_type,
        wgpu::DeviceType::IntegratedGpu
            | wgpu::DeviceType::DiscreteGpu
            | wgpu::DeviceType::VirtualGpu
    ));
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
        .map_err(|error| error.to_string())?;
    let vertex_data = [
        vertices([1.0, 0.0, 0.0, 1.0]),
        vertices([0.0, 1.0, 0.0, 1.0]),
    ]
    .concat();
    let vertex = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("red then green vertices"),
        contents: bytemuck::cast_slice(&vertex_data),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let blue = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("different vertex identity"),
        contents: bytemuck::cast_slice(
            &[
                vertices([0.0, 0.0, 1.0, 1.0]),
                vertices([0.0, 0.0, 1.0, 1.0]),
            ]
            .concat(),
        ),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let mut index_data =
        bytemuck::cast_slice::<u16, u8>(&[0, 1, 2, 0, 2, 3, 0, 0, 0, 0, 0, 0]).to_vec();
    index_data.extend_from_slice(bytemuck::cast_slice::<u32, u8>(&[0, 1, 2, 0, 2, 3]));
    let index = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("u16 triangles, degenerate triangles, u32 triangles"),
        contents: &index_data,
        usage: wgpu::BufferUsages::INDEX,
    });
    let alternate_indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("different index identity"),
        contents: bytemuck::cast_slice::<u16, u8>(&[0, 0, 0, 0, 0, 0, 0, 1, 2, 0, 2, 3]),
        usage: wgpu::BufferUsages::INDEX,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(
            r#"
            struct Out { @builtin(position) position: vec4f, @location(0) color: vec4f }
            @vertex fn vs(@location(0) position: vec2f, @location(1) color: vec4f) -> Out {
                return Out(vec4f(position, 0.0, 1.0), color);
            }
            @fragment fn fs(input: Out) -> @location(0) vec4f { return input.color; }
        "#
            .into(),
        ),
    });
    let make_pipeline = || {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
    };
    let pipelines = [make_pipeline(), make_pipeline()];
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 64,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    let vertex_clone = vertex.clone();
    for _ in 0..2 {
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &attachments,
            ..Default::default()
        });
        let mut bindings = GraphicsBufferBindings::default();
        // Every new render pass needs bindings again, even with identical handles.
        assert!(bindings.set_vertex_buffer(&mut pass, 0, vertex.slice(0..96)));
        assert!(bindings.set_index_buffer(
            &mut pass,
            index.slice(0..12),
            wgpu::IndexFormat::Uint16
        ));
        // A cloned handle is the same resource; a different slot is not.
        assert!(!bindings.set_vertex_buffer(&mut pass, 0, vertex_clone.slice(0..96)));
        assert!(bindings.set_vertex_buffer(&mut pass, 1, vertex.slice(0..96)));
        assert!(!bindings.set_vertex_buffer(&mut pass, 1, vertex.slice(0..96)));
        // Format and length are independent parts of index binding identity.
        assert!(bindings.set_index_buffer(
            &mut pass,
            index.slice(0..24),
            wgpu::IndexFormat::Uint16
        ));
        assert!(bindings.set_index_buffer(
            &mut pass,
            index.slice(0..24),
            wgpu::IndexFormat::Uint32
        ));
        assert!(bindings.set_index_buffer(
            &mut pass,
            index.slice(0..24),
            wgpu::IndexFormat::Uint16
        ));
        // Scissored stripes make wrong vertex/index state visible separately.
        // Steps 3 and 5 change only buffer identity (identical offset/size),
        // yielding green->blue and clear->blue respectively. Comparing only
        // ranges would therefore fail pixels as well as command accounting.
        let steps = [
            (
                vertex.slice(0..96),
                index.slice(0..12),
                wgpu::IndexFormat::Uint16,
            ),
            (
                vertex.slice(0..96),
                index.slice(0..12),
                wgpu::IndexFormat::Uint16,
            ),
            (
                vertex.slice(96..192),
                index.slice(0..12),
                wgpu::IndexFormat::Uint16,
            ),
            (
                blue.slice(96..192),
                index.slice(0..12),
                wgpu::IndexFormat::Uint16,
            ),
            (
                blue.slice(96..192),
                index.slice(12..24),
                wgpu::IndexFormat::Uint16,
            ),
            (
                blue.slice(96..192),
                alternate_indices.slice(12..24),
                wgpu::IndexFormat::Uint16,
            ),
            (
                vertex.slice(..),
                index.slice(24..48),
                wgpu::IndexFormat::Uint32,
            ),
            (
                vertex.slice(0..96),
                index.slice(0..24),
                wgpu::IndexFormat::Uint16,
            ),
        ];
        for (stripe, (vertices, indices, format)) in steps.into_iter().enumerate() {
            pass.set_pipeline(&pipelines[stripe % 2]);
            let vertex_emitted = bindings.set_vertex_buffer(&mut pass, 0, vertices);
            let index_emitted = bindings.set_index_buffer(&mut pass, indices, format);
            assert_eq!(vertex_emitted, matches!(stripe, 2 | 3 | 6 | 7));
            assert_eq!(index_emitted, matches!(stripe, 0 | 4 | 5 | 6 | 7));
            assert!(!bindings.set_vertex_buffer(&mut pass, 0, vertices));
            assert!(!bindings.set_index_buffer(&mut pass, indices, format));
            pass.set_scissor_rect(stripe as u32 * 8, 0, 8, 8);
            pass.draw_indexed(0..6, 0, 0..1);
        }
    }
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256 * 8,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(8),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| error.to_string())?;
    receiver
        .recv()
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    let data = readback
        .slice(..)
        .get_mapped_range()
        .map_err(|error| error.to_string())?;
    let expected = [
        [255, 0, 0, 255],
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [0, 0, 0, 0],
        [0, 0, 255, 255],
        [255, 0, 0, 255],
        [255, 0, 0, 255],
    ];
    for row in data.chunks_exact(256) {
        for (x, pixel) in row.chunks_exact(4).enumerate() {
            assert_eq!(pixel, expected[x / 8], "stripe {}", x / 8);
        }
    }
    drop(data);
    readback.unmap();
    Ok(())
}
