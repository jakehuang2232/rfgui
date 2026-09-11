//! Full-frame resource lifecycle: the selector must never need Legacy for an
//! Auto frame. Resource delivery is controlled; no prepared paint is injected.
use super::*;
use crate::view::base_component::Svg;
use crate::view::svg_resource::{
    SvgRasterMode, SvgRasterRequest, prime_svg_document_ready_for_test,
    prime_svg_raster_ready_for_test, replace_svg_raster_ready_for_test,
    set_svg_raster_error_for_test, set_svg_raster_loading_for_test,
};
use crate::view::test_support::get_element_mut;

#[derive(Clone, Copy)]
enum Delivery {
    Image(ImageAssetId),
    Svg(u64, [u32; 2]),
}
impl Delivery {
    fn ready(self, rgba: [u8; 4]) {
        match self {
            Self::Image(asset) => replace_ready_image_for_test(asset, 1, 1, Arc::from(rgba)),
            Self::Svg(key, [w, h]) => replace_svg_raster_ready_for_test(
                key,
                w,
                h,
                Arc::from(rgba.repeat((w * h) as usize)),
            ),
        };
    }
    fn loading(self) {
        match self {
            Self::Image(asset) => set_image_loading_for_test(asset),
            Self::Svg(key, _) => set_svg_raster_loading_for_test(key),
        }
    }
    fn error(self) {
        match self {
            Self::Image(asset) => set_image_error_for_test(asset, "controlled source error"),
            Self::Svg(key, _) => set_svg_raster_error_for_test(key),
        }
    }
}
enum Source {
    Image(ImageSource, crate::view::image_resource::ImageHandle),
    Svg(crate::view::SvgSource, u64),
}
fn raster(document: u64, dpr: u32, rgba: [u8; 4]) -> Delivery {
    let size = [(20 * dpr).div_ceil(32) * 32, 32 * dpr];
    let (key, _) = prime_svg_raster_ready_for_test(
        document,
        SvgRasterRequest::new(size[0], size[1], SvgRasterMode::Fill),
        Arc::from(rgba.repeat((size[0] * size[1]) as usize)),
    );
    Delivery::Svg(key, size)
}
fn source(svg: bool, label: &str, dpr: u32, rgba: [u8; 4]) -> (Source, Delivery) {
    if svg {
        let source = crate::view::SvgSource::Content(format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><!-- source transition {label} --></svg>"
        ));
        let document = prime_svg_document_ready_for_test(&source, 20.0, 32.0);
        (Source::Svg(source, document), raster(document, dpr, rgba))
    } else {
        let source = ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: Arc::from(rgba),
        };
        let handle = acquire_image_resource(&source);
        let delivery = Delivery::Image(handle.asset_id());
        delivery.ready(rgba);
        (Source::Image(source, handle), delivery)
    }
}
fn set_source(viewport: &Viewport, owner: NodeKey, source: &Source) {
    match source {
        Source::Image(source, lease) => {
            let _ = lease.asset_id(); // Keep the inline resource alive through handoff.
            get_element_mut::<Image>(viewport.node_arena(), owner).set_source(source.clone());
        }
        Source::Svg(source, _) => {
            get_element_mut::<Svg>(viewport.node_arena(), owner).set_source(source.clone());
        }
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_resource_sources_stay_retained_through_preparation() -> Result<(), String>
{
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    let mut frames = 0;
    for svg in [false, true] {
        for mode in [
            ViewportPaintRendererMode::RetainedAuto,
            ViewportPaintRendererMode::Legacy,
        ] {
            for initial_dpr in [1_u32, 2] {
                let label = format!("svg={svg} {mode:?} initial DPR={initial_dpr}");
                let (mut current, mut delivery) =
                    source(svg, &format!("{label} A"), initial_dpr, [255, 0, 0, 255]);
                let mut arena = NodeArena::new();
                let mut root = Element::new_with_id(0xa320, 0.0, 0.0, 20.0, 16.0);
                let mut root_style = sized_grid(20.0, 16.0);
                root_style.insert(
                    PropertyId::ScrollDirection,
                    ParsedValue::ScrollDirection(ScrollDirection::Vertical),
                );
                root.apply_style(root_style);
                let root = commit_element(&mut arena, Box::new(root));
                let mut host_style = sized_grid(20.0, 32.0);
                host_style.insert(
                    PropertyId::BackgroundColor,
                    ParsedValue::color_like(Color::rgb(0, 255, 0)),
                );
                let host: Box<dyn ElementTrait> = match &current {
                    Source::Image(source, _) => {
                        let mut host = Image::new_with_id(0xa321, source.clone());
                        host.apply_style(host_style);
                        host.set_fit(crate::view::ImageFit::Fill);
                        host.set_sampling(ImageSampling::Nearest);
                        Box::new(host)
                    }
                    Source::Svg(source, _) => {
                        let mut host = Svg::new_with_id(0xa321, source.clone());
                        host.apply_style(host_style);
                        host.set_fit(crate::view::ImageFit::Fill);
                        Box::new(host)
                    }
                };
                let owner = commit_child(&mut arena, root, host);
                let mut viewport = Viewport::new();
                viewport.set_paint_renderer_mode(mode);
                viewport.install_single_viewport_scene_for_test(arena, root);
                let mut pending = None;
                let mut previous_targets = None;
                // Ordered states: first request, ready, source B/loading, warm,
                // completion, late completion, next freeze, error, source C,
                // ready, warm, DPR/pending resolution, warm, active error, replacement, warm,
                // fit-mode/pending request, completion, warm.
                // All mutations precede the production frame except the named
                // late completion. No dirty flags or paint snapshots are injected.
                for frame in 0..19 {
                    let dpr = if frame >= 11 {
                        3 - initial_dpr
                    } else {
                        initial_dpr
                    };
                    match frame {
                        2 | 8 => {
                            let (next, next_delivery) =
                                source(svg, &format!("{label} {frame}"), dpr, [0, 0, 255, 255]);
                            if frame == 2 {
                                next_delivery.loading();
                            }
                            set_source(&viewport, owner, &next);
                            current = next;
                            delivery = next_delivery;
                        }
                        4 => delivery.ready([0, 0, 255, 255]),
                        7 => delivery.error(),
                        11 => {
                            if let Source::Svg(_, document) = &current {
                                let next = raster(*document, dpr, [0, 0, 255, 255]);
                                next.loading();
                                pending = Some(next);
                            }
                        }
                        13 if svg => delivery.error(),
                        14 | 17 => {
                            if let Some(next) = pending {
                                next.ready([0, 0, 255, 255]);
                            }
                        }
                        16 => {
                            if let Source::Svg(_, document) = &current {
                                // Intrinsic and destination aspect ratios match:
                                // switching Fill -> Contain preserves the visible
                                // geometry while requiring a different raster mode.
                                let (w, h) =
                                    crate::view::svg_resource::quantize_svg_uniform_raster_size(
                                        20.0, 32.0, dpr as f32,
                                    );
                                let (key, _) = prime_svg_raster_ready_for_test(
                                    *document,
                                    SvgRasterRequest::new(w, h, SvgRasterMode::Uniform),
                                    Arc::from([0, 0, 255, 255].repeat((w * h) as usize)),
                                );
                                let next = Delivery::Svg(key, [w, h]);
                                next.loading();
                                pending = Some(next);
                                get_element_mut::<Svg>(viewport.node_arena(), owner)
                                    .set_fit(crate::view::ImageFit::Contain);
                            }
                        }
                        _ => {}
                    }
                    begin_resource_frame(&mut viewport, gpu, dpr)?;
                    let observed = if frame == 5 {
                        viewport.render_single_viewport_after_freeze_for_test(move |_| {
                            delivery.ready([255, 0, 0, 255])
                        })?
                    } else {
                        viewport.render_single_viewport_scene_for_test()?
                    };
                    let pixels = read_submitted_texture(
                        &observed.texture,
                        gpu,
                        [WIDTH * dpr, HEIGHT * dpr],
                    )?;
                    let expected = match frame {
                        0 if svg => [0, 255, 0, 255],
                        0 | 1 | 6 => [255, 0, 0, 255],
                        2 | 3 | 7 => [0, 255, 0, 255],
                        8 | 13 if svg => [0, 255, 0, 255],
                        _ => [0, 0, 255, 255],
                    };
                    check_resource_pixels(
                        &pixels,
                        dpr,
                        expected,
                        &format!("{label} frame={frame}"),
                    )?;
                    eprintln!(
                        "source transition {label} frame={frame} selected_artifact={} actions={:?}",
                        observed.artifact_selected, observed.actions
                    );
                    if mode == ViewportPaintRendererMode::RetainedAuto {
                        // SVG Error -> new-source Loading paints the same
                        // green wrapper, with no sampled op or active children.
                        // Changing an unpainted resource key must not invalidate
                        // those identical raster pixels.
                        let reuse = matches!(frame, 3 | 5 | 10 | 12 | 15 | 16 | 18)
                            || (svg && frame == 8)
                            || (!svg && matches!(frame, 1 | 9 | 13 | 14 | 17));
                        check_resource_retention(
                            &viewport,
                            &observed,
                            dpr,
                            if reuse {
                                RetainedSurfaceCompileAction::Reuse
                            } else {
                                RetainedSurfaceCompileAction::Reraster
                            },
                        );
                        if reuse {
                            assert_eq!(
                                previous_targets.as_ref(),
                                Some(&observed.color_targets),
                                "{label} frame={frame}"
                            );
                        }
                        previous_targets = Some(observed.color_targets);
                    } else {
                        assert!(observed.legacy_selected);
                    }
                    frames += 1;
                }
            }
        }
    }
    assert_eq!(frames, 152);
    eprintln!(
        "source transition acceptance: {frames} frames on {}",
        gpu.label()
    );
    Ok(())
}
