use super::*;

use crate::view::viewport::{
    AutoArtifactSurfaceEmissionForTest, emit_retained_auto_artifact_surface_for_test,
};

pub(super) fn nested_effect_fixture() -> (NodeArena, NodeKey) {
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

    let mut overlap = Element::new_with_id(0xc3_c212, 0.0, 0.0, 20.0, 16.0);
    let mut overlap_style = Style::new();
    overlap_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    overlap_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(8.0))
                .top(Length::px(6.0)),
        ),
    );
    overlap_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(36, 184, 92)),
    );
    overlap.apply_style(overlap_style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root));
    let effect = commit_child(&mut arena, root, Box::new(child));
    commit_child(&mut arena, effect, Box::new(overlap));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, root)
}

fn property_scene_nested_effect_graph() -> Result<FrameGraph, String> {
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
    .map_err(|error| format!("PropertyScene planner rejected fixture: {error:?}"))?;
    let mut viewport = Viewport::new();
    crate::view::paint::build_retained_property_scene_with_forced_pool_for_test(
        &mut viewport,
        &plan,
        &mut graph,
        ctx,
    )
    .map_err(|error| format!("PropertyScene executor rejected fixture: {error:?}"))?;
    assert_property_scene_effect_fixture_is_overlap_sensitive(&graph)?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn assert_property_scene_effect_fixture_is_overlap_sensitive(
    graph: &FrameGraph,
) -> Result<(), String> {
    let effect_opacity_bits = 0.625_f32.to_bits();
    let layers = graph
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>(
        );
    let [effect_layer] = layers.as_slice() else {
        return Err(format!(
            "PropertyScene oracle must composite exactly one Effect layer, got {}",
            layers.len()
        ));
    };
    let opacity_bits = effect_layer.test_params().opacity.to_bits();
    if opacity_bits != effect_opacity_bits || opacity_bits == 1.0_f32.to_bits() {
        return Err("PropertyScene oracle must composite one non-neutral Effect layer".to_owned());
    }

    let rects = graph
        .test_rect_pass_snapshots()
        .into_iter()
        .filter(|rect| {
            rect.color_write_enabled
                && rect.opacity_bits == 1.0_f32.to_bits()
                && f32::from_bits(rect.fill_color_bits[3]) > 0.0
        })
        .collect::<Vec<_>>();
    let has_overlapping_effect_ops = rects.iter().enumerate().any(|(left_index, left)| {
        rects.iter().skip(left_index + 1).any(|right| {
            if left.output_target != right.output_target {
                return false;
            }
            let [left_x, left_y] = left.position_bits.map(f32::from_bits);
            let [left_width, left_height] = left.size_bits.map(f32::from_bits);
            let [right_x, right_y] = right.position_bits.map(f32::from_bits);
            let [right_width, right_height] = right.size_bits.map(f32::from_bits);
            let overlap_width =
                (left_x + left_width).min(right_x + right_width) - left_x.max(right_x);
            let overlap_height =
                (left_y + left_height).min(right_y + right_height) - left_y.max(right_y);
            overlap_width > 0.0 && overlap_height > 0.0
        })
    });
    has_overlapping_effect_ops
        .then_some(())
        .ok_or_else(|| {
            "PropertyScene oracle must raster at least two non-transparent overlapping Effect ops into one target"
                .to_owned()
        })
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
// cargo test -q native_production_artifact_effect_matches_property_scene_and_reuses_real_pool -- --ignored --nocapture
fn native_production_artifact_effect_matches_property_scene_and_reuses_real_pool()
-> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let (surface_count, bytes) = verify_cold_warm_artifact_surface(
        gpu,
        &adapter,
        "production-artifact-effect",
        nested_effect_fixture,
        property_scene_nested_effect_graph()?,
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
