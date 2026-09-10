use super::*;
use crate::style::Opacity;
use crate::view::base_component::Svg;
use crate::view::svg_resource::{
    prime_svg_document_ready_for_test, set_svg_document_error_for_test,
    set_svg_document_loading_for_test,
};

fn effect_style(opacity: f32, blue: bool) -> Style {
    let mut style = sized_grid(20.0, 32.0);
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(Opacity::new(opacity)),
    );
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(if blue {
            Color::rgb(0, 0, 255)
        } else {
            Color::rgb(0, 255, 0)
        }),
    );
    style
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_resource_wrapper_effect_and_reuse() -> Result<(), String> {
    run_wrapper_effect(ViewportPaintRendererMode::RetainedAuto)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_legacy_resource_wrapper_effect_pixels() -> Result<(), String> {
    run_wrapper_effect(ViewportPaintRendererMode::Legacy)
}

fn run_wrapper_effect(mode: ViewportPaintRendererMode) -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU");
    for svg in [false, true] {
        for dpr in [1, 2] {
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let mut arena = NodeArena::new();
            let mut root = Element::new_with_id(0xc1_2200, 0.0, 0.0, 20.0, 32.0);
            root.apply_style(sized_grid(20.0, 32.0));
            let root = commit_element(&mut arena, Box::new(root));
            let (owner, image_handle, document) = if svg {
                let source = crate::view::SvgSource::Content(format!(
                    "<svg xmlns=\"http://www.w3.org/2000/svg\"><!-- c12 wrapper {mode:?} {dpr} --></svg>"
                ));
                let key = prime_svg_document_ready_for_test(&source, 20.0, 32.0);
                set_svg_document_loading_for_test(key);
                let mut host = Svg::new_with_id(0xc1_2201, source);
                host.apply_style(effect_style(0.5, false));
                (
                    commit_child(&mut arena, root, Box::new(host)),
                    None,
                    Some(key),
                )
            } else {
                let source = ImageSource::Rgba {
                    width: 1,
                    height: 1,
                    pixels: Arc::from([255, 0, 0, 255]),
                };
                let handle = acquire_image_resource(&source);
                set_image_loading_for_test(handle.asset_id());
                let mut host = Image::new_with_id(0xc1_2201, source);
                host.apply_style(effect_style(0.5, false));
                (
                    commit_child(&mut arena, root, Box::new(host)),
                    Some(handle),
                    None,
                )
            };
            viewport.install_single_viewport_scene_for_test(arena, root);
            // Ordered: cold Loading / warm / opacity only / warm / Error
            // with new background / warm. No manual dirty injection or pool reset.
            for frame in 0..6 {
                if frame == 2 || frame == 4 {
                    let style = effect_style(0.25, frame == 4);
                    let mut node = viewport.node_arena().get_mut(owner).unwrap();
                    if svg {
                        node.element
                            .as_any_mut()
                            .downcast_mut::<Svg>()
                            .unwrap()
                            .apply_style(style);
                    } else {
                        node.element
                            .as_any_mut()
                            .downcast_mut::<Image>()
                            .unwrap()
                            .apply_style(style);
                    }
                }
                if frame == 4 {
                    if let Some(key) = document {
                        set_svg_document_error_for_test(key);
                    } else {
                        set_image_error_for_test(
                            image_handle.as_ref().unwrap().asset_id(),
                            "c12 wrapper error",
                        );
                    }
                }
                begin_resource_frame(&mut viewport, gpu, dpr)?;
                // A frozen document Loading/Error has no raster acquisition:
                // every Auto frame must select Artifact, with no fallback.
                let observed = viewport.render_single_viewport_scene_for_test()?;
                let pixels =
                    read_submitted_texture(&observed.texture, gpu, [WIDTH * dpr, HEIGHT * dpr])?;
                let alpha = if frame < 2 { 128 } else { 64 };
                let expected = if frame < 4 {
                    [0, 255, 0, alpha]
                } else {
                    [0, 0, 255, alpha]
                };
                for (x, y, color) in [
                    (4, 4, expected),
                    (18, 30, expected),
                    (22, 4, [0; 4]),
                    (4, 34, [0; 4]),
                ] {
                    let i = ((y * dpr * WIDTH * dpr + x * dpr) * 4) as usize;
                    let actual: [u8; 4] = pixels[i..i + 4].try_into().unwrap();
                    assert!(
                        actual
                            .into_iter()
                            .zip(color)
                            .all(|(a, e)| a.abs_diff(e) <= 1),
                        "svg={svg} {mode:?} DPR {dpr} frame {frame} @({x},{y}): {actual:?} != {color:?}"
                    );
                }
                if mode == ViewportPaintRendererMode::RetainedAuto {
                    check_resource_retention(
                        &viewport,
                        &observed,
                        dpr,
                        if frame == 0 || frame == 4 {
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
    eprintln!("resource wrapper effect {mode:?} passed on {}", gpu.label());
    Ok(())
}

mod ancestor_slot_tests;
