//! Real production scroll culling, raster residency and independent pixels.
use rfgui::style::{Color, Layout, Length, Padding, ScrollDirection};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::viewport::{ViewportPaintRendererMode, set_scroll_offset_by_id};
use rfgui::view::{Element, Text, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

fn scene(changed: bool, rows: usize) -> RsxNode {
    rsx! {
        <Element style={{width:Length::px(400.), height:Length::px(400.), layout:Layout::flow().column(), scroll_direction:ScrollDirection::Both}}>
            {(0..rows).map(|i| rsx! {
                // Keep glyphs away from the left receiver edge: the two renderers
                // have a separately reproduced negative-glyph clipping discrepancy.
                // Rects still cross both edges; text crosses vertical edges.
                <Element style={{width:Length::px(600.), height:Length::px(24.), padding:Padding::uniform(Length::px(0.)).x(Length::px(128.)), background:if changed && i==0 {Color::rgb(210,30,80)} else {Color::rgb(20+(i%7)as u8*25,50,90)}}}>
                    <Text>{format!("Row {i}: immutable text")}</Text>
                </Element>
            }).collect::<Vec<_>>()}
        </Element>
    }
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
        let mut reference = Vec::new();
        for mode in [
            ViewportPaintRendererMode::Legacy,
            ViewportPaintRendererMode::RetainedAuto,
        ] {
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let now = Instant::now();
            let mut first_targets = None;
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
                    std::fs::write(dir.join(format!("{mode:?}-{dpr}-{frame}.rgba")), &pixels)
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
                                "escaped clip at {x},{y}: {mode:?} frame {frame}"
                            );
                        }
                    }
                }
                if mode == ViewportPaintRendererMode::Legacy {
                    reference.push(pixels);
                } else {
                    let diff = pixels
                        .iter()
                        .zip(&reference[frame])
                        .filter(|(a, b)| a != b)
                        .count();
                    if offset.0.fract() == 0. && offset.1.fract() == 0. {
                        assert_eq!(diff, 0, "pixel parity DPR {dpr} frame {frame}");
                    } else {
                        // Fractional composite sampling differs from
                        // immediate rendering. Compare the warmed cache against
                        // an actual cache-cleared raster for this fractional case.
                        viewport.release_render_resource_caches();
                        let cold = viewport.render_rsx_offscreen_for_test(
                            &tree,
                            gpu.device.clone(),
                            gpu.queue.clone(),
                            size,
                            dpr as f32,
                            now,
                        )?;
                        assert!(cold.rerasterizations > 0);
                        let cold_pixels = gpu.read(&cold.texture, size)?;
                        let cold_diff = pixels
                            .iter()
                            .zip(&cold_pixels)
                            .filter(|(a, b)| a != b)
                            .count();
                        assert_eq!(cold_diff, 0, "fractional warm/cold pixels DPR {dpr}");
                    }
                    if frame == 0 {
                        first_targets = Some(output.persistent_targets.clone());
                    }
                    if (1..=3).contains(&frame) {
                        assert_eq!(output.rerasterizations, 0, "DPR {dpr} frame {frame}");
                        assert!(output.reuses > 0);
                        assert_eq!(output.target_allocations, 0);
                        assert_eq!(Some(&output.persistent_targets), first_targets.as_ref());
                    }
                    if frame == 11 {
                        assert!(
                            output.rerasterizations > 0,
                            "paint edit must invalidate raster"
                        );
                    }
                }
            }
            assert_ne!(reference[0], reference[1], "scroll must change pixels");
            assert_ne!(reference[10], reference[11], "edit must change pixels");
        }
    }
    Ok(())
}
