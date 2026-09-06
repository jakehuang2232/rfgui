use super::*;
use crate::view::viewport::ViewportPaintRendererMode;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_artifact_layout_paint_and_pool_reuse() -> Result<(), String> {
    run_single_viewport_frames(ViewportPaintRendererMode::RetainedAuto)
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_legacy_layout_and_paint() -> Result<(), String> {
    run_single_viewport_frames(ViewportPaintRendererMode::Legacy)
}

fn run_single_viewport_frames(mode: ViewportPaintRendererMode) -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    for dpr in [1_u32, 2] {
        let mut viewport = Viewport::new();
        viewport.set_paint_renderer_mode(mode);
        let mut arena = NodeArena::new();
        let mut style = sized_grid(20.0, 16.0);
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(224, 36, 28)),
        );
        style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
        let mut element = Element::new_with_id(0xb4_8301, 0.0, 0.0, 20.0, 16.0);
        element.apply_style(style.clone());
        let root = commit_element(&mut arena, Box::new(element));
        // Scene ownership transfers once. Every subsequent mutation, layout,
        // property observation, paint, and pool operation uses this Viewport.
        viewport.install_single_viewport_scene_for_test(arena, root);
        let mut first_target = None;
        for (frame, (translation, scroll_y)) in
            StyleScene::TranslucentFill.states().into_iter().enumerate()
        {
            style.set_transform(Transform::new([Translate::xy(
                Length::px(translation[0]),
                Length::px(translation[1]),
            )]));
            get_element_mut::<Element>(viewport.node_arena(), root).apply_style(style.clone());
            viewport.begin_offscreen_test_frame(
                gpu.device.clone(),
                gpu.queue.clone(),
                WIDTH * dpr,
                HEIGHT * dpr,
                FORMAT,
            )?;
            // begin_offscreen_test_frame resets scale to 1 and logical size to
            // physical size every frame. Set DPR afterwards, inside this loop;
            // reversing the order silently invalidates the DPR 2 coverage.
            viewport.set_scale_factor(dpr as f32);
            assert_eq!(viewport.scale_factor(), dpr as f32);
            assert_eq!(viewport.logical_size(), (WIDTH as f32, HEIGHT as f32));
            let observed = viewport.render_single_viewport_scene_for_test()?;
            assert_eq!(observed.frame_number, frame as u64 + 1);
            assert!(viewport.node_arena().get(root).is_some());
            if mode == ViewportPaintRendererMode::RetainedAuto {
                assert!(
                    observed.artifact_selected,
                    "must not pass through an old retained authority"
                );
                let expected = if frame == 0 {
                    RetainedSurfaceCompileAction::Reraster
                } else {
                    RetainedSurfaceCompileAction::Reuse
                };
                assert_eq!(observed.actions, [expected]);
                assert_eq!(observed.texture_bytes, 20 * 16 * 12 * u64::from(dpr * dpr));
                assert_eq!(observed.color_targets.len(), 1);
                let target = &observed.color_targets[0];
                assert!(viewport.has_compatible_persistent_render_target_pair(target.0, &target.1));
                if let Some(first) = &first_target {
                    assert_eq!(target, first);
                } else {
                    first_target = Some(target.clone());
                }
            } else {
                assert!(observed.legacy_selected);
            }
            let pixels =
                read_submitted_texture(&observed.texture, gpu, [WIDTH * dpr, HEIGHT * dpr])?;
            validate_style_pixels(
                &pixels,
                StyleScene::TranslucentFill,
                translation,
                scroll_y,
                dpr,
                frame,
            )?;
        }
    }
    eprintln!(
        "single Viewport production frames {mode:?} passed on {}",
        gpu.label()
    );
    Ok(())
}

fn read_submitted_texture(
    texture: &wgpu::Texture,
    gpu: &NativeGpu,
    [width, height]: [u32; 2],
) -> Result<Vec<u8>, String> {
    let stride = padded_bytes_per_row(width);
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("single Viewport submitted pixels"),
        size: u64::from(stride) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    // This copy follows the production submission on the same queue.
    gpu.queue.submit(Some(encoder.finish()));
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| format!("GPU wait: {error:?}"))?;
    receiver
        .recv()
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    let mapped = readback
        .slice(..)
        .get_mapped_range()
        .map_err(|error| format!("readback: {error:?}"))?;
    let pixels = remove_row_padding(&mapped, width, height, stride)?;
    drop(mapped);
    readback.unmap();
    Ok(pixels)
}
