//! Real production scroll culling, raster residency and independent pixels.
use rfgui::style::{Angle, Color, Layout, Length, Rotate, ScrollDirection, Transform};
use rfgui::time::{Duration, Instant};
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::viewport::{get_scroll_offset_by_id, set_scroll_offset_by_id};
use rfgui::view::{Element, Text, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

/// Rows in a scroll container; the first row also holds a rotated box, a
/// transformed surface inside the scrolled content.
fn scene(changed: bool, rows: usize) -> RsxNode {
    rsx! {
        <Element style={{width:Length::px(400.), height:Length::px(400.), layout:Layout::flow().column(), scroll_direction:ScrollDirection::Both}}>
            {(0..rows).map(|i| rsx! {
                <Element style={{width:Length::px(600.), height:Length::px(24.), background:if changed && i==0 {Color::rgb(210,30,80)} else {Color::rgb(20+(i%7)as u8*25,50,90)}}}>
                    <Text>{format!("Row {i}: immutable text")}</Text>
                    {(i == 0).then(|| rsx! {
                        <Element style={{width:Length::px(16.), height:Length::px(20.), background:Color::rgb(240,200,40), transform:Transform::new([Rotate::z(Angle::deg(90.))])}} />
                    }).into_iter().collect::<Vec<_>>()}
                </Element>
            }).collect::<Vec<_>>()}
        </Element>
    }
}

/// `tree` scrolled to `offset` in a fresh viewport, rasterized with every
/// render cache released, so nothing from an earlier frame can be reused.
fn cold_pixels(
    gpu: &gpu::Gpu,
    tree: &RsxNode,
    offset: (f32, f32),
    size: [u32; 2],
    dpr: f32,
    now: Instant,
) -> Result<Vec<u8>, String> {
    let mut viewport = Viewport::new();
    let render = |viewport: &mut Viewport| {
        viewport.render_rsx_offscreen_for_test(
            tree,
            gpu.device.clone(),
            gpu.queue.clone(),
            size,
            dpr,
            now,
        )
    };
    // Scrolling needs the laid-out container. Content that does not
    // overflow clamps the offset away, as it does for the warm viewport.
    render(&mut viewport)?;
    let arena = viewport.node_arena();
    let root = arena.roots()[0];
    let id = arena.get(root).unwrap().element.stable_id();
    set_scroll_offset_by_id(arena, root, id, offset);
    viewport.release_render_resource_caches();
    let cold = render(&mut viewport)?;
    assert!(cold.rerasterizations > 0);
    gpu.read(&cold.texture, size)
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn scroll_interest_reuses_rasters_and_refreshes_exposed_or_changed_content() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    // Small moves, cull-window boundaries, far jumps, edits, removal/clamping
    // and regrowth all go through the same production path.
    let cases = [
        ((0., 0.), false, 512),
        ((12., 12.), false, 512),
        ((0., 0.), false, 512),
        ((12., 12.), false, 512),
        ((24., 24.), false, 512),
        ((96., 96.), false, 512),
        ((108., 108.), false, 512),
        ((108.5, 108.5), false, 512),
        ((0., 4096.), false, 512),
        ((12., 4108.), false, 512),
        ((0., 0.), false, 512),
        ((0., 0.), true, 512),
        ((0., 4096.), true, 512),
        ((0., 4096.), true, 8),
        ((0., 12.), true, 512),
    ];
    for dpr in [1, 2] {
        let size = [440 * dpr, 440 * dpr];
        let mut viewport = Viewport::new();
        let now = Instant::now();
        let mut first_targets = None;
        let mut frames = Vec::new();
        for (frame, &(offset, changed, rows)) in cases.iter().enumerate() {
            if frame > 0 {
                let arena = viewport.node_arena();
                let root = arena.roots()[0];
                let id = arena.get(root).unwrap().element.stable_id();
                assert!(set_scroll_offset_by_id(arena, root, id, offset));
            }
            let tree = scene(changed, rows);
            let output = viewport.render_rsx_offscreen_for_test(
                &tree,
                gpu.device.clone(),
                gpu.queue.clone(),
                size,
                dpr as f32,
                now,
            )?;
            let pixels = gpu.read(&output.texture, size)?;
            if let Some(dir) = std::env::var_os("RFGUI_BENCH_PIXELS") {
                let dir = std::path::PathBuf::from(dir);
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                std::fs::write(dir.join(format!("-{dpr}-{frame}.rgba")), &pixels)
                    .map_err(|e| e.to_string())?;
            }
            assert!(pixels.chunks_exact(4).any(|p| p[3] > 0));
            // Expanded paint interest must never expand the receiver clip.
            for y in 0..size[1] {
                for x in 0..size[0] {
                    if x >= 400 * dpr || y >= 400 * dpr {
                        assert_eq!(
                            pixels[((y * size[0] + x) * 4 + 3) as usize],
                            0,
                            "escaped clip at {x},{y}: frame {frame}"
                        );
                    }
                }
            }
            // Whatever this frame reused must paint what a cold raster of
            // the same state paints.
            let cold = cold_pixels(&gpu, &tree, offset, size, dpr as f32, now)?;
            let diff = pixels.iter().zip(&cold).filter(|(a, b)| a != b).count();
            assert_eq!(diff, 0, "warm/cold pixels DPR {dpr} frame {frame}");
            frames.push(pixels);
            if frame == 0 {
                first_targets = Some(output.persistent_targets.clone());
            }
            if (1..=3).contains(&frame) {
                assert_eq!(output.rerasterizations, 0, "DPR {dpr} frame {frame}");
                assert!(output.reuses > 0);
                assert_eq!(output.target_allocations, 0);
                // A reused content raster leaves the rotated box's raster in
                // it unused: only targets the first frame had stay resident.
                let first = first_targets.as_ref().unwrap();
                assert!(
                    output
                        .persistent_targets
                        .iter()
                        .all(|target| first.contains(target))
                );
            }
            if frame == 11 {
                assert!(
                    output.rerasterizations > 0,
                    "paint edit must invalidate raster"
                );
            }
        }
        assert_ne!(frames[0], frames[1], "scroll must change pixels");
        assert_ne!(frames[10], frames[11], "edit must change pixels");
    }
    Ok(())
}

/// Smooth scrolling samples fractional offsets, and a wheel delta can be a
/// fraction of a pixel. The content moves by whole device pixels, so every
/// frame reuses its rasters, and the fractions still add up.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn fractional_scrolls_move_by_device_pixels_and_reuse_rasters() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let dpr = 2;
    let size = [440 * dpr, 440 * dpr];
    let tree = scene(false, 512);
    let mut viewport = Viewport::new();
    let start = Instant::now();
    let mut frame = 0;
    let mut render = |viewport: &mut Viewport| {
        frame += 1;
        viewport.render_rsx_offscreen_for_test(
            &tree,
            gpu.device.clone(),
            gpu.queue.clone(),
            size,
            dpr as f32,
            start + Duration::from_millis(frame * 16),
        )
    };
    render(&mut viewport)?;
    viewport.set_pointer_position_viewport(200., 200.);
    viewport.dispatch_pointer_move_event();
    let mut fractional_samples = 0;
    // One wheel step, then five a fifth of a pixel each; every one runs
    // its smooth scroll to the end.
    for delta in [10., 0.2, 0.2, 0.2, 0.2, 0.2] {
        assert!(viewport.dispatch_pointer_wheel_event(0., delta));
        for _ in 0..20 {
            let output = render(&mut viewport)?;
            assert_eq!(output.rerasterizations, 0, "frame {frame}");
            assert!(output.reuses > 0);
            let arena = viewport.node_arena();
            let root = arena.roots()[0];
            let id = arena.get(root).unwrap().element.stable_id();
            let (_, requested) = get_scroll_offset_by_id(arena, root, id).unwrap();
            if (requested * dpr as f32).fract() != 0. {
                fractional_samples += 1;
            }
            // In tree order: the container, then its first row.
            let first_row = &viewport.frame_box_models()[1];
            assert_eq!(
                (first_row.y * dpr as f32).fract(),
                0.,
                "frame {frame} drew at {}",
                first_row.y
            );
        }
    }
    assert!(fractional_samples > 0);
    let arena = viewport.node_arena();
    let root = arena.roots()[0];
    let id = arena.get(root).unwrap().element.stable_id();
    let (_, offset) = get_scroll_offset_by_id(arena, root, id).unwrap();
    assert!((offset - 11.).abs() < 1e-4, "scrolled to {offset}");
    Ok(())
}
