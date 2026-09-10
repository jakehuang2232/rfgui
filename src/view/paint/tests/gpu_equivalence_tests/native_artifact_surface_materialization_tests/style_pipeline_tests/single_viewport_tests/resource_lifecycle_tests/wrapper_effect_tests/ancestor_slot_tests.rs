// C-1.2c acceptance gates: Artifact's four cases now pass all nine frames.
// Legacy's four cases still fail frame 0: alpha 255 instead of group alpha 64;
// its later-frame assertions remain unverified. Keep those red gates visible.
// Historical 7b3702b also rejected Artifact SelfClip. Its original fixture
// incorrectly assumed AnchorParent without an anchor clips to the parent;
// the intermediary below now establishes the intended grandparent bounds.
// Acceptance contract (applies equally to Artifact and Legacy):
// - Preserve the paint-order obligation exercised by
//   paint/tests/anchor_parent_clip_tests.rs: normal blue siblings paint before
//   overflow AnchorParent children, which occupy the late phase. Extending the
//   current leaf proof to subtrees must keep exact owner/arena mirrors and the
//   parent's normal-before-overflow partition, plus explicit self/descendant
//   clip scopes. Until a phase-aware traversal is proved, reject misordered
//   parents; do not silently record a different order or duplicate a subtree.
//   This fixture has one overflow child, so it does not replace that mixed-
//   sibling ordering test or prove a general phase-aware traversal.
// - Group opacity applies once after overlapping children are composed.
//   For two opaque rectangles in ONE opacity-0.5 group, both overlap and
//   non-overlap probes must have alpha 0.5. Applying 0.5 to each rectangle
//   instead yields overlap alpha 1 - (1 - 0.5)^2 = 0.75 (non-overlap 0.5).
//   This is a diagnostic example, not this fixture's numeric expectation:
//   its ancestor and wrapper each start at 0.5, so output alpha is 0.25,
//   approximately 64/255 after RGBA8 quantization, at both interior probes.
// - Expected coordinates/colors/alpha come from geometry and composition,
//   never from a Legacy readback. A mismatch measures a renderer defect.
//   Legacy gates may pass after a rendering fix; never redefine expected
//   pixels to match the current incorrect output. Legacy's later frames
//   remain unverified until its frame-zero failure is fixed.
use super::*;
use crate::style::{ClipMode, Position};
use crate::view::node_arena::{Node, NodeKey};

fn ancestor_style(tx: f32, opacity: f32) -> Style {
    let mut style = sized_grid(20.0, 16.0);
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(Opacity::new(opacity)),
    );
    style.set_transform(Transform::new([Translate::xy(
        Length::px(tx),
        Length::px(4.0),
    )]));
    style
}
fn host_style(opacity: f32) -> Style {
    let mut style = effect_style(opacity, false);
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(0.0))
                .top(Length::px(0.0))
                .clip(ClipMode::AnchorParent),
        ),
    );
    style
}
fn slot(arena: &mut NodeArena, owner: NodeKey, id: u64, loading: bool) -> NodeKey {
    let mut base = Element::new_with_id(id, 0.0, 0.0, 20.0, 32.0);
    let mut style = sized_grid(20.0, 32.0);
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(if loading {
            Color::rgb(255, 255, 0)
        } else {
            Color::rgb(255, 0, 255)
        }),
    );
    base.apply_style(style);
    let base = arena.insert(Node::with_parent(Box::new(base), Some(owner)));
    if loading {
        let mut child = Element::new_with_id(id + 1, 0.0, 0.0, 12.0, 12.0);
        let mut style = sized_grid(12.0, 12.0);
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(0, 0, 255)),
        );
        child.apply_style(style);
        commit_child(arena, base, Box::new(child));
    }
    base
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_ancestor_state_nonempty_slots_image_artifact_dpr1() -> Result<(), String>
{
    run_ancestor_slots(ViewportPaintRendererMode::RetainedAuto, false, 1)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_ancestor_state_nonempty_slots_image_artifact_dpr2() -> Result<(), String>
{
    run_ancestor_slots(ViewportPaintRendererMode::RetainedAuto, false, 2)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_ancestor_state_nonempty_slots_image_legacy_dpr1() -> Result<(), String> {
    run_ancestor_slots(ViewportPaintRendererMode::Legacy, false, 1)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_ancestor_state_nonempty_slots_image_legacy_dpr2() -> Result<(), String> {
    run_ancestor_slots(ViewportPaintRendererMode::Legacy, false, 2)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_ancestor_state_nonempty_slots_svg_artifact_dpr1() -> Result<(), String> {
    run_ancestor_slots(ViewportPaintRendererMode::RetainedAuto, true, 1)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_ancestor_state_nonempty_slots_svg_artifact_dpr2() -> Result<(), String> {
    run_ancestor_slots(ViewportPaintRendererMode::RetainedAuto, true, 2)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_ancestor_state_nonempty_slots_svg_legacy_dpr1() -> Result<(), String> {
    run_ancestor_slots(ViewportPaintRendererMode::Legacy, true, 1)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_ancestor_state_nonempty_slots_svg_legacy_dpr2() -> Result<(), String> {
    run_ancestor_slots(ViewportPaintRendererMode::Legacy, true, 2)
}

fn run_ancestor_slots(mode: ViewportPaintRendererMode, svg: bool, dpr: u32) -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU");
    let mut viewport = Viewport::new();
    viewport.set_paint_renderer_mode(mode);
    let mut arena = NodeArena::new();
    let mut root = Element::new_with_id(0xc1_2300, 0.0, 0.0, WIDTH as f32, HEIGHT as f32);
    root.apply_style(sized_grid(WIDTH as f32, HEIGHT as f32));
    let root = commit_element(&mut arena, Box::new(root));
    let mut ancestor = Element::new_with_id(0xc1_2301, 0.0, 0.0, 20.0, 16.0);
    ancestor.apply_style(ancestor_style(3.0, 0.5));
    let ancestor = commit_child(&mut arena, root, Box::new(ancestor));
    // Without an explicit anchor, AnchorParent uses the grandparent's box
    // (the established layout contract), not the immediate parent's box.
    // This transparent intermediary makes the 20x16 ancestor that clip owner.
    let mut parent = Element::new_with_id(0xc1_2303, 0.0, 0.0, 20.0, 32.0);
    parent.apply_style(sized_grid(20.0, 32.0));
    let parent = commit_child(&mut arena, ancestor, Box::new(parent));
    let (owner, handle, document) = if svg {
        let source = crate::view::SvgSource::Content(format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><!-- c12 ancestor slots {mode:?} {dpr} --></svg>"
        ));
        let key = prime_svg_document_ready_for_test(&source, 20.0, 32.0);
        set_svg_document_loading_for_test(key);
        let mut host = Svg::new_with_id(0xc1_2302, source);
        host.apply_style(host_style(0.5));
        (
            commit_child(&mut arena, parent, Box::new(host)),
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
        let mut host = Image::new_with_id(0xc1_2302, source);
        host.apply_style(host_style(0.5));
        (
            commit_child(&mut arena, parent, Box::new(host)),
            Some(handle),
            None,
        )
    };
    let loading = slot(&mut arena, owner, 0xc1_2310, true);
    let error = slot(&mut arena, owner, 0xc1_2320, false);
    arena.with_element_taken(owner, |element, _| {
        if svg {
            let host = element.as_any_mut().downcast_mut::<Svg>().unwrap();
            host.attach_loading_slot_cold(vec![loading]);
            host.attach_error_slot_cold(vec![error]);
        } else {
            let host = element.as_any_mut().downcast_mut::<Image>().unwrap();
            host.attach_loading_slot_cold(vec![loading]);
            host.attach_error_slot_cold(vec![error]);
        }
    });
    viewport.install_single_viewport_scene_for_test(arena, root);
    // Ordered sequence: cold/warm, ancestor placement, ancestor opacity,
    // local opacity, Error/warm, Loading/warm. One input changes per step.
    for frame in 0..9 {
        let tx = if frame < 2 { 3.0 } else { 11.0 };
        let ancestor_opacity = if frame < 3 { 0.5 } else { 0.25 };
        if frame == 2 || frame == 3 {
            get_element_mut::<Element>(viewport.node_arena(), ancestor)
                .apply_style(ancestor_style(tx, ancestor_opacity));
        }
        if frame == 4 {
            let mut node = viewport.node_arena().get_mut(owner).unwrap();
            if svg {
                node.element
                    .as_any_mut()
                    .downcast_mut::<Svg>()
                    .unwrap()
                    .apply_style(host_style(0.25));
            } else {
                node.element
                    .as_any_mut()
                    .downcast_mut::<Image>()
                    .unwrap()
                    .apply_style(host_style(0.25));
            }
        }
        if frame == 5 || frame == 7 {
            if let Some(key) = document {
                if frame == 5 {
                    set_svg_document_error_for_test(key);
                } else {
                    set_svg_document_loading_for_test(key);
                }
            } else {
                let asset = handle.as_ref().unwrap().asset_id();
                if frame == 5 {
                    set_image_error_for_test(asset, "ancestor slot error");
                } else {
                    set_image_loading_for_test(asset);
                }
            }
        }
        begin_resource_frame(&mut viewport, gpu, dpr)?;
        let observed = viewport.render_single_viewport_scene_for_test()?;
        assert_eq!(
            viewport
                .node_arena()
                .get(owner)
                .unwrap()
                .element
                .exact_generic_subtree_self_clip_scissor_rect(owner, viewport.node_arena(), false),
            Some([0, 0, 20, 16]),
            "fixture must resolve the intended grandparent clip before interpreting pixels",
        );
        let pixels = read_submitted_texture(&observed.texture, gpu, [WIDTH * dpr, HEIGHT * dpr])?;
        let alpha = if frame < 3 {
            64
        } else if frame < 4 {
            32
        } else {
            16
        };
        let is_error = frame == 5 || frame == 6;
        let inside = if is_error {
            [255, 0, 255, alpha]
        } else {
            [0, 0, 255, alpha]
        };
        let outside_child = if is_error {
            [255, 0, 255, alpha]
        } else {
            [255, 255, 0, alpha]
        };
        // Grandparent clip after transform is [tx,tx+20) x [4,20); content extends to y=36.
        // The bottom probe lies inside content in both axes and below
        // the parent, so transparency can only come from the clip.
        for (x, y, color) in [
            (tx as u32 + 4, 8, inside),
            (tx as u32 + 18, 18, outside_child),
            (tx as u32 + 4, 21, [0; 4]),
            (tx as u32 + 21, 8, [0; 4]),
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
            assert!(observed.artifact_selected);
            assert!(!observed.actions.is_empty());
            if matches!(frame, 1 | 6 | 8) {
                assert!(
                    observed
                        .actions
                        .iter()
                        .all(|action| *action == RetainedSurfaceCompileAction::Reuse),
                    "unchanged frame must reuse every raster"
                );
            }
            assert!(!observed.color_targets.is_empty());
            for (key, desc) in &observed.color_targets {
                assert!(viewport.has_compatible_persistent_render_target_pair(*key, desc));
            }
        } else {
            assert!(observed.legacy_selected);
        }
    }
    Ok(())
}
