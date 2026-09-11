use super::*;

mod style_pipeline_tests;
mod text_area_recording_tests;
mod recording_overlay_tests;

// Admission gap for the selector-transfer batch: both the direct S->T fixture
// and the T->E fixture below install resolved transforms through a test hook.
// These gates prove materialization execution, not the production style-to-
// property pipeline's exact translation matrix. Validate that pipeline before
// using these gates as production Transform admission evidence.

use crate::view::paint::{
    ArtifactSurfaceRasterContext, FrameArtifactRecordOutcome, RendererMode,
    RetainedSurfaceCompileAction, SurfaceMaterializationOutcome,
    emit_prepared_artifact_surface_frame_from_pool, prepare_artifact_surface_raster_plan,
    record_surface_dag_frame_artifact, seal_prepared_artifact_surface_frame,
    take_last_production_actions_for_test,
};
fn materialized_direct_scroll_transform_graph(
    viewport: &mut Viewport,
    case: DirectScrollTransformGpuCase,
    dpr: f32,
) -> Result<
    (
        FrameGraph,
        crate::view::viewport::RetainedSurfaceFrameStageOwner,
        Vec<RetainedSurfaceCompileAction>,
        u64,
    ),
    String,
> {
    let (arena, root, properties, generations) = direct_scroll_transform_gpu_fixture(case);
    let roots = [root];
    let FrameArtifactRecordOutcome::Artifact { artifact, .. } = record_surface_dag_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .map_err(|error| format!("materialized S->T record rejected: {:?}", error.reasons))?
    else {
        return Err("materialized S->T record fell back".to_owned());
    };
    materialized_two_boundary_graph(viewport, artifact, dpr)
}

fn materialized_two_boundary_graph(
    viewport: &mut Viewport,
    artifact: crate::view::paint::PaintArtifact,
    dpr: f32,
) -> Result<
    (
        FrameGraph,
        crate::view::viewport::RetainedSurfaceFrameStageOwner,
        Vec<RetainedSurfaceCompileAction>,
        u64,
    ),
    String,
> {
    let (mut graph, ctx, target) =
        transformed_graph_prelude_with_size(dpr, None, [WIDTH * dpr as u32, HEIGHT * dpr as u32]);
    let raster_context = ArtifactSurfaceRasterContext::new(
        dpr,
        FORMAT,
        ctx.paint_offset(),
        ctx.graphics_pass_context().logical_scissor_rect(),
        wgpu::Limits::default().max_texture_dimension_2d,
        128 * 1024 * 1024,
    )
    .ok_or_else(|| "materialized S->T raster context is invalid".to_owned())?;
    let plan = prepare_artifact_surface_raster_plan(artifact, raster_context)
        .map_err(|error| format!("materialized S->T plan rejected: {error:?}"))?;
    if plan.nodes().len() != 1
        || plan.materialization_decisions().len() != 2
        || plan
            .materialization_decisions()
            .iter()
            .filter(|decision| {
                decision.outcome() == SurfaceMaterializationOutcome::EliminatedPassThrough
            })
            .count()
            != 1
    {
        return Err(format!(
            "materialized S->T target projection drifted: nodes={}, decisions={:?}",
            plan.nodes().len(),
            plan.materialization_decisions()
        ));
    }
    let aggregate_texture_bytes = plan.nodes().iter().try_fold(0_u64, |total, node| {
        let target = node.target();
        let color = crate::view::raster_cost::texture_desc_payload_bytes(&target.color).bytes;
        let depth = crate::view::raster_cost::texture_desc_payload_bytes(&target.depth).bytes;
        total.checked_add(color)?.checked_add(depth)
    });
    let aggregate_texture_bytes = aggregate_texture_bytes
        .ok_or_else(|| "materialized S->T descriptor bytes overflowed".to_owned())?;
    let frame = seal_prepared_artifact_surface_frame(plan)
        .map_err(|error| format!("materialized S->T seal rejected: {error:?}"))?;
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .ok_or_else(|| "materialized S->T retained stage is unavailable".to_owned())?;
    let _ = take_last_production_actions_for_test();
    emit_prepared_artifact_surface_frame_from_pool(viewport, owner, frame, &mut graph, ctx)
        .map_err(|error| format!("materialized S->T execution rejected: {error:?}"))?;
    let actions = take_last_production_actions_for_test();
    add_present(&mut graph, &target)?;
    Ok((graph, owner, actions, aggregate_texture_bytes))
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_materialized_direct_scroll_transform_matches_the_pre_cutover_pixels_and_reuses_one_pair()
-> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    let adapter = gpu.label();
    let mut viewport = Viewport::new();

    let (cold_graph, cold_owner, cold_actions, cold_bytes) =
        materialized_direct_scroll_transform_graph(
            &mut viewport,
            DirectScrollTransformGpuCase::BASELINE,
            1.0,
        )?;
    // This hardware fixture is 48x120 at DPR 1, so one RGBA8+depth pair is
    // 48 * 120 * (4 + 8) = 69,120 bytes. The 100x300 CPU contract separately
    // freezes the 360,000-byte planning example.
    if cold_actions != [RetainedSurfaceCompileAction::Reraster] || cold_bytes != 69_120 {
        return Err(format!(
            "materialized S->T cold accounting drifted on {adapter}: actions={cold_actions:?}, bytes={cold_bytes}"
        ));
    }
    let cold_pixels = render_on_viewport(cold_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true) {
        return Err("materialized S->T cold transaction did not commit".to_owned());
    }
    let cold_oracle = render(
        legacy_direct_scroll_transform_graph(DirectScrollTransformGpuCase::BASELINE)?,
        gpu,
    )?;
    validate_direct_scroll_transform_gradient_coverage(
        &cold_pixels,
        DirectScrollTransformGpuCase::BASELINE,
        "materialized cold",
        &adapter,
    )?;
    compare_pixels(
        &cold_oracle,
        &cold_pixels,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "materialized-direct-s-t/cold",
    )?;

    let (warm_graph, warm_owner, warm_actions, warm_bytes) =
        materialized_direct_scroll_transform_graph(
            &mut viewport,
            DirectScrollTransformGpuCase::SCROLL_ONLY,
            1.0,
        )?;
    if warm_actions != [RetainedSurfaceCompileAction::Reuse] || warm_bytes != cold_bytes {
        return Err(format!(
            "materialized S->T warm accounting drifted on {adapter}: actions={warm_actions:?}, cold_bytes={cold_bytes}, warm_bytes={warm_bytes}"
        ));
    }
    let warm_pixels = render_on_viewport(warm_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true) {
        return Err("materialized S->T warm transaction did not commit".to_owned());
    }
    let warm_oracle = render(
        legacy_direct_scroll_transform_graph(DirectScrollTransformGpuCase::SCROLL_ONLY)?,
        gpu,
    )?;
    validate_direct_scroll_transform_gradient_coverage(
        &warm_pixels,
        DirectScrollTransformGpuCase::SCROLL_ONLY,
        "materialized warm offset",
        &adapter,
    )?;
    compare_pixels(
        &warm_oracle,
        &warm_pixels,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "materialized-direct-s-t/warm-offset",
    )?;
    eprintln!(
        "materialized direct scroll/transform one-pair gate passed on {adapter}: bytes={warm_bytes}"
    );
    Ok(())
}

// This gate deliberately has no legacy graph/oracle dependency. All probes and
// resource expectations come from the fixture's authored geometry and colors.
// FORMAT is linear RGBA8: authored sRGB bytes are independently converted by
// round(255 * ((c/255 + .055)/1.055)^2.4) (all nonzero channels exceed .04045).
// Thus red [224,36,28] -> [190,4,3], blue [24,72,224] -> [2,17,190].
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_materialization_absolute_pixels_and_reuse_at_both_dprs() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    for dpr in [1_u32, 2] {
        let mut viewport = Viewport::new();
        for (case, action, red_y, blue_y) in [
            (
                DirectScrollTransformGpuCase::BASELINE,
                RetainedSurfaceCompileAction::Reraster,
                12,
                20,
            ),
            (
                DirectScrollTransformGpuCase::SCROLL_ONLY,
                RetainedSurfaceCompileAction::Reuse,
                4,
                12,
            ),
        ] {
            let (graph, owner, actions, bytes) =
                materialized_direct_scroll_transform_graph(&mut viewport, case, dpr as f32)?;
            let expected_bytes = 48 * 120 * 12 * u64::from(dpr * dpr);
            if actions != [action] || bytes != expected_bytes {
                return Err(format!(
                    "{} DPR {dpr}: actions={actions:?}, bytes={bytes}, expected={expected_bytes}",
                    case.label
                ));
            }
            let pixels = render_on_viewport_with_size(
                graph,
                gpu,
                &mut viewport,
                dpr as f32,
                FORMAT,
                [WIDTH * dpr, HEIGHT * dpr],
            )?;
            if !viewport.finish_retained_surface_transaction_for_frame(Some(owner), true) {
                return Err("absolute materialization transaction did not commit".into());
            }
            for (name, x, y, expected) in [
                ("red interior", 7, red_y, [190_u8, 4, 3, 255]),
                ("blue interior", 7, blue_y, [2, 17, 190, 255]),
                ("before translated content", 1, 12, [0, 0, 0, 0]),
                ("beyond scrollport right", 49, 12, [0, 0, 0, 0]),
                ("beyond scrollport bottom", 7, 41, [0, 0, 0, 0]),
            ] {
                let index = ((y * dpr * WIDTH * dpr + x * dpr) * 4) as usize;
                let actual: [u8; 4] = pixels[index..index + 4].try_into().unwrap();
                if actual
                    .into_iter()
                    .zip(expected)
                    .any(|(a, e)| a.abs_diff(e) > 1)
                {
                    return Err(format!(
                        "{} DPR {dpr} {name} @({},{}): {actual:?}, expected {expected:?} on {}",
                        case.label,
                        x * dpr,
                        y * dpr,
                        gpu.label()
                    ));
                }
            }
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_materialization_translation_effect_pixels_and_reuse() -> Result<(), String> {
    use crate::style::{Layout, ParsedValue, PropertyId, Style};
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    for dpr in [1_u32, 2] {
        let mut viewport = Viewport::new();
        for (tx, action) in [
            (9.0, RetainedSurfaceCompileAction::Reraster),
            (17.0, RetainedSurfaceCompileAction::Reuse),
        ] {
            let mut root = Element::new_with_id(0xb4_7d01, 0.0, 0.0, 20.0, 16.0);
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            root.apply_style(style.clone());
            root.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                glam::Vec3::new(tx, 4.0, 0.0),
            )));
            root.set_background_color_value(Color::rgb(224, 36, 28));
            root.set_opacity(0.5);
            root.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
            let mut arena = NodeArena::new();
            let root = arena.insert(Node::new(Box::new(root)));
            arena.refresh_subtree_dirty_cache(root);
            let mut properties = PropertyTrees::default();
            properties.sync(&arena, &[root]);
            let mut generations = PaintGenerationTracker::default();
            generations.sync(&arena, &[root], &properties);
            let FrameArtifactRecordOutcome::Artifact { artifact, .. } =
                record_surface_dag_frame_artifact(
                    &arena,
                    &[root],
                    &properties,
                    &generations,
                    RendererMode::ForcedForTests,
                )
                .map_err(|error| format!("T->E artifact: {error:?}"))?
            else {
                return Err("T->E fallback".into());
            };
            let (graph, owner, actions, bytes) =
                materialized_two_boundary_graph(&mut viewport, artifact, dpr as f32)?;
            if actions != [action] || bytes != 20 * 16 * 12 * u64::from(dpr * dpr) {
                return Err(format!(
                    "T->E DPR {dpr}, tx {tx}: {actions:?}, {bytes} bytes"
                ));
            }
            let pixels = render_on_viewport_with_size(
                graph,
                gpu,
                &mut viewport,
                dpr as f32,
                FORMAT,
                [WIDTH * dpr, HEIGHT * dpr],
            )?;
            if !viewport.finish_retained_surface_transaction_for_frame(Some(owner), true) {
                return Err("T->E commit failed".into());
            }
            for (x, y, expected) in [
                // Presentation returns straight alpha; opacity halves alpha,
                // while RGB remains linear red (one-LSB intermediate rounding).
                (tx as u32 + 3, 7, [190_u8, 4, 3, 128]),
                (tx as u32 - 2, 7, [0, 0, 0, 0]),
                (tx as u32 + 22, 7, [0, 0, 0, 0]),
            ] {
                let index = ((y * dpr * WIDTH * dpr + x * dpr) * 4) as usize;
                let actual: [u8; 4] = pixels[index..index + 4].try_into().unwrap();
                if actual
                    .into_iter()
                    .zip(expected)
                    .any(|(a, e)| a.abs_diff(e) > 1)
                {
                    return Err(format!(
                        "T->E DPR {dpr} tx {tx} @({x},{y}): {actual:?}, expected {expected:?}"
                    ));
                }
            }
        }
    }
    Ok(())
}
