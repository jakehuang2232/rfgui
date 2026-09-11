use super::*;
use crate::view::test_support::{commit_child, commit_element, get_element_mut};

// Geometry is fixed independently of recorded ops: scrollport 40x80,
// content 20x240. The right-hand track has no content behind it. At (35,60)
// only its opaque-sampled 0.5 shadow and 0.35 fill overlap: alpha 0.675.
// Legacy and generic execution each face this absolute oracle.
fn fixture() -> (NodeArena, NodeKey) {
    let mut arena = new_test_arena();
    let make = |id, width, height, scroll| {
        let mut element = Element::new_with_id(id, 0.0, 0.0, width, height);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
        if scroll {
            style.insert(
                PropertyId::ScrollDirection,
                ParsedValue::ScrollDirection(crate::style::ScrollDirection::Vertical),
            );
        } else {
            style.insert(
                PropertyId::BackgroundColor,
                ParsedValue::color_like(Color::rgb(255, 0, 0)),
            );
        }
        element.apply_style(style);
        element
    };
    let root = commit_element(&mut arena, Box::new(make(0xc1_4400, 40.0, 80.0, true)));
    commit_child(
        &mut arena,
        root,
        Box::new(make(0xc1_4401, 20.0, 240.0, false)),
    );
    let mut layout = Viewport::new();
    crate::view::viewport::layout_artifact_style_scene_for_test(
        &mut layout,
        &mut arena,
        root,
        [64.0, 96.0],
    );
    let mut host = get_element_mut::<Element>(&arena, root);
    host.set_sampled_scrollbar_alpha_for_test(1.0);
    host.set_scrollbar_shadow_blur_radius(0.0);
    drop(host);
    (arena, root)
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_generic_and_legacy_frozen_scrollbar_absolute_alpha() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    for legacy in [false, true] {
        for dpr in [1_u32, 2] {
            let (mut arena, root) = fixture();
            let extent = [64 * dpr, 96 * dpr];
            let (mut graph, ctx, target) =
                transformed_graph_prelude_with_size(dpr as f32, None, extent);
            let mut viewport = Viewport::new();
            let owner = if legacy {
                arena
                    .with_element_taken(root, |element, arena| {
                        element.build(&mut graph, arena, ctx)
                    })
                    .unwrap();
                drop(arena);
                None
            } else {
                let (properties, generations) = sync_identity(&arena, &[root]);
                let FrameArtifactRecordOutcome::Artifact { artifact, .. } =
                    record_surface_dag_frame_artifact(
                        &arena,
                        &[root],
                        &properties,
                        &generations,
                        RendererMode::ForcedForTests,
                    )
                    .map_err(|error| format!("record: {error:?}"))?
                else {
                    panic!("fallback")
                };
                assert_eq!(
                    artifact.chunks.last().unwrap().id.role,
                    PaintChunkRole::ScrollbarOverlay
                );
                drop(arena);
                drop(properties);
                let context = ArtifactSurfaceRasterContext::new(
                    dpr as f32,
                    FORMAT,
                    ctx.paint_offset(),
                    None,
                    8192,
                    128 * 1024 * 1024,
                )
                .unwrap();
                let plan = prepare_artifact_surface_raster_plan(artifact, context)
                    .map_err(|error| format!("plan: {error:?}"))?;
                let plan = seal_prepared_artifact_surface_frame(plan)
                    .map_err(|error| format!("seal: {error:?}"))?;
                let owner = viewport.begin_retained_surface_frame_stage().unwrap();
                emit_prepared_artifact_surface_frame_from_pool(
                    &mut viewport,
                    owner,
                    plan,
                    &mut graph,
                    ctx,
                )
                .map_err(|error| format!("emit: {error:?}"))?;
                Some(owner)
            };
            add_present(&mut graph, &target)?;
            let pixels = render_on_viewport_with_size(
                graph,
                gpu,
                &mut viewport,
                dpr as f32,
                FORMAT,
                extent,
            )?;
            let pixel = |x, y| {
                let at = ((y * dpr * extent[0] + x * dpr) * 4) as usize;
                &pixels[at..at + 4]
            };
            assert!(
                pixel(35, 60)[3].abs_diff(172) <= 1,
                "track alpha: legacy={legacy}, DPR={dpr}, {:?}",
                pixel(35, 60)
            );
            assert_eq!(
                pixel(4, 60),
                [255, 0, 0, 255],
                "content remains inside its scrollport"
            );
            assert_eq!(
                pixel(4, 84),
                [0, 0, 0, 0],
                "content below scrollport is clipped"
            );
            if let Some(owner) = owner {
                assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
            }
        }
    }
    Ok(())
}
