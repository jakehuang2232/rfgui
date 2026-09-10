use super::*;
use crate::view::base_component::Svg;
use crate::view::svg_resource::{
    SvgRasterMode, SvgRasterRequest, prime_svg_document_ready_for_test,
    prime_svg_raster_ready_for_test, replace_svg_raster_ready_for_test,
    set_svg_raster_loading_for_test,
};

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_svg_raster_generation_and_freeze() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for dpr in [1_u32, 2] {
            let source = crate::view::SvgSource::Content(format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\"><!-- c4 {mode:?} {dpr} --></svg>"
            ));
            let doc = prime_svg_document_ready_for_test(&source, 20.0, 32.0);
            let width = (20 * dpr).div_ceil(32) * 32;
            let height = 32 * dpr;
            let request = SvgRasterRequest::new(width, height, SvgRasterMode::Fill);
            let red: Arc<[u8]> = Arc::from([255, 0, 0, 255].repeat((width * height) as usize));
            let blue: Arc<[u8]> = Arc::from([0, 0, 255, 255].repeat((width * height) as usize));
            let (raster, _) = prime_svg_raster_ready_for_test(doc, request, red.clone());
            set_svg_raster_loading_for_test(raster);
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let mut arena = NodeArena::new();
            let mut root = Element::new_with_id(0xc4_7101, 0.0, 0.0, 20.0, 16.0);
            let mut style = sized_grid(20.0, 16.0);
            style.insert(
                PropertyId::BackgroundColor,
                ParsedValue::color_like(Color::rgba(0, 0, 0, 0)),
            );
            style.insert(
                PropertyId::ScrollDirection,
                ParsedValue::ScrollDirection(ScrollDirection::Vertical),
            );
            root.apply_style(style);
            let root = commit_element(&mut arena, Box::new(root));
            let mut svg = Svg::new_with_id(0xc4_7102, source);
            let mut style = sized_grid(20.0, 32.0);
            style.insert(
                PropertyId::BackgroundColor,
                ParsedValue::color_like(Color::rgb(0, 255, 0)),
            );
            svg.apply_style(style);
            svg.set_fit(crate::view::ImageFit::Fill);
            commit_child(&mut arena, root, Box::new(svg));
            viewport.install_single_viewport_scene_for_test(arena, root);
            for frame in 0..8 {
                if frame == 2 {
                    let red = red.clone();
                    std::thread::spawn(move || {
                        replace_svg_raster_ready_for_test(raster, width, height, red)
                    })
                    .join()
                    .expect("SVG completion");
                }
                begin_resource_frame(&mut viewport, gpu, dpr)?;
                let observed = if frame == 0 && mode == ViewportPaintRendererMode::RetainedAuto {
                    // The first post-layout raster acquisition cannot alter
                    // the pre-layout resource freeze. Production explicitly
                    // falls back for this one unprepared frame; next-frame
                    // sync reconciles the key and enables artifact recording.
                    viewport.render_single_viewport_selection_fallback_for_test(
                        "artifact:[LegacyBoundary(MissingPreparedSvg)]",
                    )?
                } else if frame == 3 {
                    let blue = blue.clone();
                    viewport.render_single_viewport_after_freeze_for_test(move |_| {
                        std::thread::spawn(move || {
                            replace_svg_raster_ready_for_test(raster, width, height, blue)
                        })
                        .join()
                        .expect("late SVG completion");
                    })?
                } else {
                    viewport.render_single_viewport_scene_for_test()?
                };
                let pixels =
                    read_submitted_texture(&observed.texture, gpu, [WIDTH * dpr, HEIGHT * dpr])?;
                let expected = if frame < 2 {
                    [0, 255, 0, 255]
                } else if frame < 4 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                };
                check_resource_pixels(
                    &pixels,
                    dpr,
                    expected,
                    &format!("SVG {mode:?} frame {frame}"),
                )?;
                if mode == ViewportPaintRendererMode::RetainedAuto && frame != 0 {
                    check_resource_retention(
                        &viewport,
                        &observed,
                        dpr,
                        if frame == 1 || frame == 2 || frame == 4 {
                            RetainedSurfaceCompileAction::Reraster
                        } else {
                            RetainedSurfaceCompileAction::Reuse
                        },
                    );
                } else {
                    assert!(observed.legacy_selected);
                }
            }
        }
    }
    eprintln!("SVG resource freeze passed on {}", gpu.label());
    Ok(())
}
