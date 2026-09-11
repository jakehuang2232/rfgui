use super::*;
use crate::view::base_component::Svg;
use crate::view::node_arena::{Node, NodeKey};
use crate::view::svg_resource::{
    SvgRasterMode, SvgRasterRequest, prime_svg_document_ready_for_test,
    prime_svg_raster_ready_for_test, replace_svg_raster_ready_for_test,
    set_svg_raster_error_for_test, set_svg_raster_loading_for_test,
};

#[derive(Clone, Copy, Debug)]
enum Host {
    Image,
    Svg,
}

struct SlotScene {
    _image: Option<crate::view::image_resource::ImageHandle>,
    asset: Option<ImageAssetId>,
    raster: Option<u64>,
    extent: [u32; 2],
    root: NodeKey,
    owner: NodeKey,
    loading: NodeKey,
    error: NodeKey,
    first: NodeKey,
    second: NodeKey,
}

fn colored_style(width: f32, height: f32, rgb: [u8; 3], absolute: bool) -> Style {
    let mut style = sized_grid(width, height);
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(rgb[0], rgb[1], rgb[2])),
    );
    if absolute {
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                crate::style::Position::absolute()
                    .left(Length::px(0.0))
                    .top(Length::px(0.0)),
            ),
        );
    }
    style
}

fn colored_element(id: u64, width: f32, height: f32, rgb: [u8; 3], absolute: bool) -> Element {
    let mut element = Element::new_with_id(id, 0.0, 0.0, width, height);
    element.apply_style(colored_style(width, height, rgb, absolute));
    element
}

fn install_slots(
    viewport: &mut Viewport,
    host: Host,
    dpr: u32,
    mode: ViewportPaintRendererMode,
) -> SlotScene {
    let mut arena = NodeArena::new();
    let mut root = Element::new_with_id(0xc1_1100, 0.0, 0.0, 20.0, 16.0);
    let mut style = sized_grid(20.0, 16.0);
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root.apply_style(style);
    let root = commit_element(&mut arena, Box::new(root));
    let extent = [(20 * dpr).div_ceil(32) * 32, 32 * dpr];
    let (owner, handle, asset, raster) = match host {
        Host::Image => {
            let source = ImageSource::Rgba {
                width: 2,
                height: 2,
                pixels: Arc::from([255, 0, 0, 255].repeat(4)),
            };
            let handle = acquire_image_resource(&source);
            let asset = handle.asset_id();
            set_image_loading_for_test(asset);
            let mut image = Image::new_with_id(0xc1_1101, source);
            image.apply_style(colored_style(20.0, 32.0, [0, 255, 0], false));
            image.set_fit(crate::view::ImageFit::Fill);
            image.set_sampling(ImageSampling::Nearest);
            (
                commit_child(&mut arena, root, Box::new(image)),
                Some(handle),
                Some(asset),
                None,
            )
        }
        Host::Svg => {
            let source = crate::view::SvgSource::Content(format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\"><!-- slots {mode:?} {dpr} --></svg>"
            ));
            let doc = prime_svg_document_ready_for_test(&source, 20.0, 32.0);
            let request = SvgRasterRequest::new(extent[0], extent[1], SvgRasterMode::Fill);
            let (raster, _) = prime_svg_raster_ready_for_test(
                doc,
                request,
                Arc::from([255, 0, 0, 255].repeat((extent[0] * extent[1]) as usize)),
            );
            set_svg_raster_loading_for_test(raster);
            let mut svg = Svg::new_with_id(0xc1_1101, source);
            svg.apply_style(colored_style(20.0, 32.0, [0, 255, 0], false));
            svg.set_fit(crate::view::ImageFit::Fill);
            (
                commit_child(&mut arena, root, Box::new(svg)),
                None,
                None,
                Some(raster),
            )
        }
    };
    // Inactive slots have parent ownership but are absent from owner's children.
    // Production sync moves the selected roots into children (and empties that
    // slot's storage). A nonempty storage vector while active is malformed.
    let loading = arena.insert(Node::with_parent(
        Box::new(colored_element(0xc1_1110, 20.0, 32.0, [255, 255, 0], false)),
        Some(owner),
    ));
    let first = commit_child(
        &mut arena,
        loading,
        Box::new(colored_element(0xc1_1111, 12.0, 12.0, [255, 0, 0], true)),
    );
    let second = commit_child(
        &mut arena,
        loading,
        Box::new(colored_element(0xc1_1112, 12.0, 12.0, [0, 0, 255], true)),
    );
    let error = arena.insert(Node::with_parent(
        Box::new(colored_element(0xc1_1120, 20.0, 32.0, [255, 0, 255], false)),
        Some(owner),
    ));
    arena.with_element_taken(owner, |element, _| match host {
        Host::Image => {
            let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
            image.attach_loading_slot_cold(vec![loading]);
            image.attach_error_slot_cold(vec![error]);
        }
        Host::Svg => {
            let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
            svg.attach_loading_slot_cold(vec![loading]);
            svg.attach_error_slot_cold(vec![error]);
        }
    });
    viewport.install_single_viewport_scene_for_test(arena, root);
    SlotScene {
        _image: handle,
        asset,
        raster,
        extent,
        root,
        owner,
        loading,
        error,
        first,
        second,
    }
}

impl SlotScene {
    fn publish(&self, state: u8) {
        if let Some(asset) = self.asset {
            match state {
                0 => set_image_loading_for_test(asset),
                1 => {
                    replace_ready_image_for_test(
                        asset,
                        2,
                        2,
                        Arc::from([255, 0, 0, 255].repeat(4)),
                    );
                }
                2 => set_image_error_for_test(asset, "slot transition"),
                _ => unreachable!(),
            }
        } else {
            let raster = self.raster.unwrap();
            match state {
                0 => set_svg_raster_loading_for_test(raster),
                1 => {
                    replace_svg_raster_ready_for_test(
                        raster,
                        self.extent[0],
                        self.extent[1],
                        Arc::from(
                            [255, 0, 0, 255].repeat((self.extent[0] * self.extent[1]) as usize),
                        ),
                    );
                }
                2 => set_svg_raster_error_for_test(raster),
                _ => unreachable!(),
            }
        }
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_nonempty_resource_slots() -> Result<(), String> {
    run_slots(ViewportPaintRendererMode::RetainedAuto)
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_legacy_nonempty_resource_slots() -> Result<(), String> {
    run_slots(ViewportPaintRendererMode::Legacy)
}

fn run_slots(mode: ViewportPaintRendererMode) -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    for host in [Host::Image, Host::Svg] {
        for dpr in [1, 2] {
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let scene = install_slots(&mut viewport, host, dpr, mode);
            // Ordered sequence: loading bootstrap/warm, content edit/warm,
            // sibling reorder/warm, ready/warm, error/warm, ready/warm,
            // return to loading/warm. Expectations depend on this order.
            for frame in 0..15 {
                match frame {
                    3 => get_element_mut::<Element>(viewport.node_arena(), scene.second)
                        .apply_style(colored_style(12.0, 12.0, [0, 255, 255], true)),
                    5 => viewport.edit_scene_arena_for_test(|arena| {
                        // This direct topology edit explicitly injects dirty.
                        // Pixels/recording prove the new paint order, but this
                        // frame's Reraster does not prove that reordering alone
                        // triggers invalidation through production reconciliation.
                        arena.set_children(scene.loading, vec![scene.second, scene.first]);
                        get_element_mut::<Element>(arena, scene.loading).mark_layout_dirty();
                        arena.mark_dirty(
                            scene.loading,
                            crate::view::base_component::DirtyFlags::ALL,
                        );
                    }),
                    7 | 11 => scene.publish(1),
                    9 => scene.publish(2),
                    13 => scene.publish(0),
                    _ => {}
                }
                begin_resource_frame(&mut viewport, gpu, dpr)?;
                let observed = viewport.render_single_viewport_scene_for_test()?;
                let pixels =
                    read_submitted_texture(&observed.texture, gpu, [WIDTH * dpr, HEIGHT * dpr])?;
                let ready = matches!(frame, 7 | 8 | 11 | 12);
                let error = matches!(frame, 9 | 10);
                let inside = if ready {
                    [255, 0, 0, 255]
                } else if error {
                    [255, 0, 255, 255]
                } else if frame < 3 {
                    [0, 0, 255, 255]
                } else if frame < 5 {
                    [0, 255, 255, 255]
                } else {
                    [255, 0, 0, 255]
                };
                let outside_children = if ready {
                    [255, 0, 0, 255]
                } else if error {
                    [255, 0, 255, 255]
                } else {
                    [255, 255, 0, 255]
                };
                // Overlapping siblings make command order visible; (18,14)
                // observes the slot background independently of those siblings.
                for (x, y, expected) in [
                    (4, 4, inside),
                    (18, 14, outside_children),
                    (22, 4, [0; 4]),
                    (4, 18, [0; 4]),
                ] {
                    let i = ((y * dpr * WIDTH * dpr + x * dpr) * 4) as usize;
                    let actual: [u8; 4] = pixels[i..i + 4].try_into().unwrap();
                    assert!(
                        actual
                            .into_iter()
                            .zip(expected)
                            .all(|(a, e)| a.abs_diff(e) <= 1),
                        "{host:?} {mode:?} DPR {dpr} frame {frame} @({x},{y}): {actual:?} != {expected:?}"
                    );
                }
                let expected_children = if ready {
                    vec![]
                } else if error {
                    vec![scene.error]
                } else {
                    vec![scene.loading]
                };
                assert_eq!(
                    viewport.node_arena().children_of(scene.owner),
                    expected_children
                );
                check_recorded_slot_owners(&viewport, &scene, frame, ready, error);
                if mode == ViewportPaintRendererMode::RetainedAuto {
                    let reraster = matches!(frame, 0 | 3 | 5 | 7 | 9 | 11 | 13);
                    check_resource_retention(
                        &viewport,
                        &observed,
                        dpr,
                        if reraster {
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
    eprintln!("Nonempty slots {mode:?} passed on {}", gpu.label());
    Ok(())
}

fn check_recorded_slot_owners(
    viewport: &Viewport,
    scene: &SlotScene,
    frame: usize,
    ready: bool,
    error: bool,
) {
    use crate::view::paint::{
        FrameArtifactRecordOutcome, RendererMode, record_surface_dag_frame_artifact,
    };
    let arena = viewport.node_arena();
    let roots = [scene.root];
    let mut properties = PropertyTrees::default();
    properties.sync(arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(arena, &roots, &properties);
    // Independently re-derive recording from the frozen host state with fresh
    // property/generation trackers. This is NOT the submitted frame's artifact
    // and does not test recording defects dependent on its prior-frame state.
    // This entry verifies metadata/full equivalence before returning an artifact.
    // Check every resource-subtree owner; the outer scroll host separately owns
    // its boundary/overlay chunks. This detects an inactive subtree whose pixels
    // happened to be occluded by the active content. Owner order/count does not
    // prove empty slot storage: classification enforces that invariant before
    // coverage traverses children, without traversing inactive slot storage.
    let outcome = record_surface_dag_frame_artifact(
        arena,
        &roots,
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .expect("slot metadata/full recording must agree");
    let FrameArtifactRecordOutcome::Artifact {
        artifact,
        eligibility,
    } = outcome
    else {
        panic!("slot recording must not fall back");
    };
    assert!(eligibility.eligible);
    let expected = if ready {
        vec![scene.owner]
    } else if error {
        vec![scene.owner, scene.error]
    } else if frame < 5 {
        vec![scene.owner, scene.loading, scene.first, scene.second]
    } else {
        vec![scene.owner, scene.loading, scene.second, scene.first]
    };
    assert_eq!(
        artifact
            .chunks
            .iter()
            .filter(|chunk| chunk.owner != scene.root)
            .map(|chunk| chunk.owner)
            .collect::<Vec<_>>(),
        expected,
        "active slot commands must appear exactly once in canonical order"
    );
}
