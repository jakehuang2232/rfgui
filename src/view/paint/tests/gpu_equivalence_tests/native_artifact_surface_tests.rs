use super::*;

use crate::view::viewport::{
    AutoArtifactSurfaceEmissionForTest, emit_retained_auto_artifact_surface_for_test,
};

fn nested_effect_fixture() -> (NodeArena, NodeKey) {
    let mut root = Element::new_with_id(0xc3_c210, 5.0, 5.0, 54.0, 48.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(24, 64, 116)),
    );
    root.apply_style(root_style);

    let mut child = Element::new_with_id(0xc3_c211, 12.0, 10.0, 32.0, 26.0);
    let mut child_style = Style::new();
    child_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    child_style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(crate::style::Opacity::new(0.625)),
    );
    child_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(224, 72, 36)),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root));
    commit_child(&mut arena, root, Box::new(child));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, root)
}

fn legacy_nested_effect_graph() -> Result<FrameGraph, String> {
    let (arena, root) = nested_effect_fixture();
    let roots = [root];
    let (properties, generations) = sync_identity(&arena, &roots);
    let (mut graph, ctx, target) = transformed_graph_prelude(1.0, None);
    // The pre-cutover production authority is PropertyScene, whose effect
    // group isolation is the semantic oracle. The immediate painter bakes
    // opacity per op and therefore is not an equivalent effect authority.
    let plan = crate::view::paint::plan_property_effect_scene_with_context(
        &arena,
        &roots,
        &properties,
        &generations,
        crate::view::paint::TransformSurfacePlanContext::new(
            ctx.paint_offset(),
            ctx.graphics_pass_context().logical_scissor_rect(),
        ),
    )
    .map_err(|error| format!("legacy property-effect planner rejected fixture: {error:?}"))?;
    let mut viewport = Viewport::new();
    crate::view::paint::build_retained_property_scene_with_forced_pool_for_test(
        &mut viewport,
        &plan,
        &mut graph,
        ctx,
    )
    .map_err(|error| format!("legacy property-effect executor rejected fixture: {error:?}"))?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn production_artifact_graph(
    viewport: &mut Viewport,
    fixture: fn() -> (NodeArena, NodeKey),
) -> Result<(FrameGraph, AutoArtifactSurfaceEmissionForTest), String> {
    let (arena, root) = fixture();
    let roots = [root];
    let (properties, generations) = sync_identity(&arena, &roots);
    let (mut graph, ctx, target) = transformed_graph_prelude(1.0, None);
    let trace = emit_retained_auto_artifact_surface_for_test(
        viewport,
        &arena,
        &roots,
        &properties,
        &generations,
        &mut graph,
        &ctx,
    )?;
    add_present(&mut graph, &target)?;
    Ok((graph, trace))
}

fn verify_cold_warm_artifact_surface(
    gpu: &NativeGpu,
    adapter: &str,
    case: &str,
    fixture: fn() -> (NodeArena, NodeKey),
    legacy: FrameGraph,
) -> Result<(usize, u64), String> {
    let legacy_pixels = render(legacy, gpu)?;
    let mut viewport = Viewport::new();

    let (cold_graph, cold) = production_artifact_graph(&mut viewport, fixture)?;
    if cold.actions.len() != cold.surface_count
        || cold
            .actions
            .iter()
            .any(|action| *action != RetainedSurfaceCompileAction::Reraster)
    {
        return Err(format!(
            "{case}: cold artifact frame must reraster every surface on {adapter}: surfaces={}, actions={:?}",
            cold.surface_count, cold.actions
        ));
    }
    let cold_pixels = render_on_viewport(cold_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(cold.frame_owner), true) {
        return Err(format!(
            "{case}: cold artifact transaction owner was not current"
        ));
    }

    let (warm_graph, warm) = production_artifact_graph(&mut viewport, fixture)?;
    if warm.surface_count != cold.surface_count
        || warm.aggregate_texture_bytes != cold.aggregate_texture_bytes
        || warm
            .actions
            .iter()
            .any(|action| *action != RetainedSurfaceCompileAction::Reuse)
    {
        return Err(format!(
            "{case}: warm artifact frame must reuse every surface on {adapter}: cold={:?}/{}B, warm={:?}/{}B",
            cold.actions, cold.aggregate_texture_bytes, warm.actions, warm.aggregate_texture_bytes,
        ));
    }
    let warm_pixels = render_on_viewport(warm_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(warm.frame_owner), true) {
        return Err(format!(
            "{case}: warm artifact transaction owner was not current"
        ));
    }

    compare_pixels(
        &legacy_pixels,
        &cold_pixels,
        [0, 0, WIDTH, HEIGHT],
        adapter,
        &format!("{case}/cold-vs-legacy"),
    )?;
    compare_pixels(
        &cold_pixels,
        &warm_pixels,
        [0, 0, WIDTH, HEIGHT],
        adapter,
        &format!("{case}/warm-reuse"),
    )?;
    Ok((cold.surface_count, cold.aggregate_texture_bytes))
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_production_artifact_transform_matches_legacy_and_reuses_real_pool -- --ignored --nocapture
fn native_production_artifact_transform_matches_legacy_and_reuses_real_pool() -> Result<(), String>
{
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let (surface_count, bytes) = verify_cold_warm_artifact_surface(
        gpu,
        &adapter,
        "production-artifact-transform",
        transformed_rect_fixture,
        legacy_transformed_rect_graph(1.0, None)?,
    )?;
    if surface_count == 0 || bytes == 0 {
        return Err(format!(
            "production artifact transform gate requires detached surfaces and descriptor bytes on {adapter}: surfaces={surface_count}, bytes={bytes}"
        ));
    }
    eprintln!(
        "production artifact transform parity/reuse passed on {adapter}: aggregate_color_depth_bytes={bytes}"
    );
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_production_artifact_effect_matches_legacy_and_reuses_real_pool -- --ignored --nocapture
fn native_production_artifact_effect_matches_legacy_and_reuses_real_pool() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let (surface_count, bytes) = verify_cold_warm_artifact_surface(
        gpu,
        &adapter,
        "production-artifact-effect",
        nested_effect_fixture,
        legacy_nested_effect_graph()?,
    )?;
    if surface_count == 0 || bytes == 0 {
        return Err(format!(
            "production artifact effect gate requires detached surfaces and descriptor bytes on {adapter}: surfaces={surface_count}, bytes={bytes}"
        ));
    }
    eprintln!(
        "production artifact effect parity/reuse passed on {adapter}: aggregate_color_depth_bytes={bytes}"
    );
    Ok(())
}
