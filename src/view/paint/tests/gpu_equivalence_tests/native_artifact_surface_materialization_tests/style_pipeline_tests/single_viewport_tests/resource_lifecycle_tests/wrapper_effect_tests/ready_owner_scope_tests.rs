// C-1.2d: Ready payload and decoration share the owner's composition scope.
// Geometry and opaque-source-over-background determine expected pixels;
// neither renderer is an oracle. Keep failures visible until the renderer is
// fixed. SVG's initial acquisition frame is setup, not Ready acceptance.
// Both renderers pass eight accepted frames per case at DPR1/2. The sequence
// covers opacity, translation, and a non-transformed AnchorParent self clip;
// it does not prove arbitrary transform+clip combinations, inline ownership,
// native scrollbars, deferred viewport painting, or all fit/filter variants.
// Frames 3..=5 combine a real style translation with owner opacity 0.25 on
// a Ready leaf. The distinct nonempty-subtree case (translation plus ancestor
// opacity 0.5/0.25) is covered by ancestor_slot_tests' eight nine-frame gates.
// Neither fixture claims coverage for arbitrary rotation or scale.
use super::*;
use crate::style::{ClipMode, Position};
use crate::view::svg_resource::{SvgRasterMode, SvgRasterRequest, prime_svg_raster_ready_for_test};

fn ready_style(opacity: f32, tx: Option<f32>) -> Style {
    let mut style = effect_style(opacity, false);
    if let Some(tx) = tx {
        style.set_transform(Transform::new([Translate::xy(
            Length::px(tx),
            Length::px(4.0),
        )]));
    } else {
        // apply_style merges declarations; omission would retain the last
        // translation and make the later clip probes test the wrong pixels.
        style.set_transform(Transform::default());
    }
    style
}

fn run_ready_owner_scope(
    mode: ViewportPaintRendererMode,
    svg: bool,
    dpr: u32,
) -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware adapter");
    let mut viewport = Viewport::new();
    viewport.set_paint_renderer_mode(mode);
    let mut arena = NodeArena::new();
    let mut root = Element::new_with_id(0xc1_4100, 0.0, 0.0, WIDTH as f32, HEIGHT as f32);
    root.apply_style(sized_grid(WIDTH as f32, HEIGHT as f32));
    let root = commit_element(&mut arena, Box::new(root));
    // AnchorParent without an explicit anchor resolves to the grandparent.
    // Keep a distinct intermediary so the clipped phase tests that contract.
    let mut parent = Element::new_with_id(0xc1_4102, 0.0, 0.0, 20.0, 32.0);
    parent.apply_style(sized_grid(20.0, 32.0));
    let parent = commit_child(&mut arena, root, Box::new(parent));
    let owner = if svg {
        let source = crate::view::SvgSource::Content(format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><!-- ready scope {mode:?} {dpr} --></svg>"
        ));
        let doc = prime_svg_document_ready_for_test(&source, 20.0, 32.0);
        let width = (20 * dpr).div_ceil(32) * 32;
        let height = 32 * dpr;
        prime_svg_raster_ready_for_test(
            doc,
            SvgRasterRequest::new(width, height, SvgRasterMode::Fill),
            Arc::from([255, 0, 0, 255].repeat((width * height) as usize)),
        );
        let mut host = Svg::new_with_id(0xc1_4101, source);
        host.set_fit(crate::view::ImageFit::Fill);
        host.apply_style(ready_style(0.5, None));
        commit_child(&mut arena, parent, Box::new(host))
    } else {
        let source = ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: Arc::from([255, 0, 0, 255]),
        };
        let handle = acquire_image_resource(&source);
        // Establish Ready before the production freeze, independently of
        // decoder scheduling or a registry entry left by another fixture.
        replace_ready_image_for_test(handle.asset_id(), 1, 1, Arc::from([255, 0, 0, 255]));
        let mut host = Image::new_with_id(0xc1_4101, source);
        // Image defaults to Contain: a square source would occupy y=6..26
        // in this 20x32 box. Fill is the premise of the edge probes below.
        host.set_fit(crate::view::ImageFit::Fill);
        host.apply_style(ready_style(0.5, None));
        commit_child(&mut arena, parent, Box::new(host))
    };
    viewport.install_single_viewport_scene_for_test(arena, root);
    if svg {
        begin_resource_frame(&mut viewport, gpu, dpr)?;
        // Real post-layout acquisition cannot mutate the already frozen
        // resource snapshot. Only this setup frame may select Legacy.
        if mode == ViewportPaintRendererMode::RetainedAuto {
            viewport.render_single_viewport_selection_fallback_for_test(
                "artifact:[LegacyBoundary(MissingPreparedSvg)]",
            )?;
        } else {
            viewport.render_single_viewport_scene_for_test()?;
        }
    }
    // Ordered sequence: cold/warm, opacity only, add transform, move/warm,
    // then remove transform and add a 20x16 grandparent self clip / warm.
    // That last transition is compound, not evidence for one dirty cause.
    // Expected actions depend on this order; every frame uses real style/layout.
    for (frame, (opacity, tx)) in [
        (0.5, None),
        (0.5, None),
        (0.25, None),
        (0.25, Some(3.0)),
        (0.25, Some(11.0)),
        (0.25, Some(11.0)),
        (0.25, None),
        (0.25, None),
    ]
    .into_iter()
    .enumerate()
    {
        let clipped = frame >= 6;
        let mut style = ready_style(opacity, tx);
        if clipped {
            style.insert(
                PropertyId::Position,
                ParsedValue::Position(
                    Position::absolute()
                        .left(Length::px(0.0))
                        .top(Length::px(0.0))
                        .clip(ClipMode::AnchorParent),
                ),
            );
        }
        if frame == 6 {
            get_element_mut::<Element>(viewport.node_arena(), root)
                .apply_style(sized_grid(20.0, 16.0));
        }
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
        drop(node);
        begin_resource_frame(&mut viewport, gpu, dpr)?;
        let observed = viewport.render_single_viewport_scene_for_test()?;
        if clipped {
            assert!(
                viewport
                    .node_arena()
                    .get(owner)
                    .unwrap()
                    .element
                    .compositor_local_transform_snapshot()
                    .is_none(),
                "the clipped phase must remove the prior transform"
            );
            assert_eq!(
                viewport
                    .node_arena()
                    .get(owner)
                    .unwrap()
                    .element
                    .exact_retained_self_clip_scissor_rect(owner, viewport.node_arena(), false),
                Some([0, 0, 20, 16]),
                "resolve the intended grandparent clip before interpreting pixels"
            );
        }
        let pixels = read_submitted_texture(&observed.texture, gpu, [WIDTH * dpr, HEIGHT * dpr])?;
        let x = tx.unwrap_or(0.0) as u32;
        let y = if tx.is_some() { 4 } else { 0 };
        let alpha = if opacity == 0.5 { 128 } else { 64 };
        for (label, px, py, expected) in [
            (
                "opaque image replaces green background before group opacity",
                x + 4,
                y + 4,
                [255, 0, 0, alpha],
            ),
            ("inside image and clip", x + 4, y + 14, [255, 0, 0, alpha]),
            // x stays inside image; transparency in the clipped phase can
            // only establish the bottom clip, never an accidental side clip.
            (
                "bottom inside image, outside optional clip",
                x + 4,
                y + 30,
                if clipped { [0; 4] } else { [255, 0, 0, alpha] },
            ),
            ("right outside owner", x + 22, y + 4, [0; 4]),
            ("bottom outside owner", x + 4, y + 34, [0; 4]),
        ] {
            let i = ((py * dpr * WIDTH * dpr + px * dpr) * 4) as usize;
            let actual: [u8; 4] = pixels[i..i + 4].try_into().unwrap();
            assert!(
                actual
                    .into_iter()
                    .zip(expected)
                    .all(|(a, e)| a.abs_diff(e) <= 1),
                "{mode:?} svg={svg} DPR{dpr} frame{frame} {label} @({px},{py}): {actual:?} != {expected:?}"
            );
        }
        if mode == ViewportPaintRendererMode::RetainedAuto {
            check_resource_retention(
                &viewport,
                &observed,
                dpr,
                if frame == 0 || frame == 3 || frame == 6 {
                    RetainedSurfaceCompileAction::Reraster
                } else {
                    RetainedSurfaceCompileAction::Reuse
                },
            );
        } else {
            assert!(observed.legacy_selected);
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_ready_owner_scope_image_artifact_dpr1() -> Result<(), String> {
    run_ready_owner_scope(ViewportPaintRendererMode::RetainedAuto, false, 1)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_ready_owner_scope_image_artifact_dpr2() -> Result<(), String> {
    run_ready_owner_scope(ViewportPaintRendererMode::RetainedAuto, false, 2)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_ready_owner_scope_image_legacy_dpr1() -> Result<(), String> {
    run_ready_owner_scope(ViewportPaintRendererMode::Legacy, false, 1)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_ready_owner_scope_image_legacy_dpr2() -> Result<(), String> {
    run_ready_owner_scope(ViewportPaintRendererMode::Legacy, false, 2)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_ready_owner_scope_svg_artifact_dpr1() -> Result<(), String> {
    run_ready_owner_scope(ViewportPaintRendererMode::RetainedAuto, true, 1)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_ready_owner_scope_svg_artifact_dpr2() -> Result<(), String> {
    run_ready_owner_scope(ViewportPaintRendererMode::RetainedAuto, true, 2)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_ready_owner_scope_svg_legacy_dpr1() -> Result<(), String> {
    run_ready_owner_scope(ViewportPaintRendererMode::Legacy, true, 1)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_ready_owner_scope_svg_legacy_dpr2() -> Result<(), String> {
    run_ready_owner_scope(ViewportPaintRendererMode::Legacy, true, 2)
}
