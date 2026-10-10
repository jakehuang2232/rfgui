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

pub(super) fn production_artifact_graph(
    viewport: &mut Viewport,
    fixture: fn() -> (NodeArena, NodeKey),
    paint_offset: [f32; 2],
) -> Result<(FrameGraph, AutoArtifactSurfaceEmissionForTest), String> {
    let (arena, root) = fixture();
    let roots = [root];
    let (properties, generations) = sync_identity(&arena, &roots);
    let (mut graph, mut ctx, target) = transformed_graph_prelude(1.0, None);
    ctx.set_paint_offset(paint_offset);
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
    paint_offset: [f32; 2],
    geometry: &dyn Fn(&[u8], &str) -> Result<(), String>,
) -> Result<(usize, u64), String> {
    let mut viewport = Viewport::new();

    let (cold_graph, cold) = production_artifact_graph(&mut viewport, fixture, paint_offset)?;
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

    let (warm_graph, warm) = production_artifact_graph(&mut viewport, fixture, paint_offset)?;
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

    geometry(&cold_pixels, &format!("{case}/cold"))?;
    geometry(&warm_pixels, &format!("{case}/warm"))?;
    compare_pixels(
        &cold_pixels,
        &warm_pixels,
        [0, 0, WIDTH, HEIGHT],
        adapter,
        &format!("{case}/warm-reuse"),
    )?;
    Ok((cold.surface_count, cold.aggregate_texture_bytes))
}

/// sRGB bytes in linear premultiplied form, before any target quantizes them.
fn linear_opaque(rgb: [u8; 3]) -> [f32; 4] {
    let [r, g, b] = rgb.map(|byte| {
        let encoded = f32::from(byte) / 255.0;
        if encoded <= 0.04045 {
            encoded / 12.92
        } else {
            ((encoded + 0.055) / 1.055).powf(2.4)
        }
    });
    [r, g, b, 1.0]
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_production_artifact_transform_matches_geometry_and_reuses_real_pool -- --ignored --nocapture
fn native_production_artifact_transform_matches_geometry_and_reuses_real_pool() -> Result<(), String>
{
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    // The fixture box [10, 8, 28, 20] moves 6 pixels right by its transform
    // and by the host placement [3.5, -2.25], which snaps to [4, -2] at DPR 1.
    let fill = rgba8_unorm(Color::rgb(210, 55, 25));
    let geometry = |pixels: &[u8], case: &str| {
        assert_rect_geometry(
            pixels,
            [20, 6, 28, 20],
            |x, y| (x >= 23 && x < 45 && y >= 9 && y < 23).then_some(fill),
            &adapter,
            case,
        )?;
        // No pixel of the 2-pixel border is clear of both its edges, so only
        // check that it reads as the green border, not the red fill or empty.
        let [r, g, _, a] = pixel_at(pixels, 21, 15)?;
        if g < 100 || r > 40 || a < 128 {
            return Err(format!(
                "{case}: border band (21,15) is not the green border on {adapter}: {:?}",
                pixel_at(pixels, 21, 15)?
            ));
        }
        Ok(())
    };
    let (surface_count, bytes) = verify_cold_warm_artifact_surface(
        gpu,
        &adapter,
        "production-artifact-transform",
        transformed_rect_fixture,
        ARTIFACT_HOST_PLACEMENT_OFFSET,
        &geometry,
    )?;
    if surface_count == 0 || bytes == 0 {
        return Err(format!(
            "production artifact transform gate requires detached surfaces and descriptor bytes on {adapter}: surfaces={surface_count}, bytes={bytes}"
        ));
    }
    eprintln!(
        "production artifact transform geometry/reuse passed on {adapter}: aggregate_color_depth_bytes={bytes}"
    );
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_production_artifact_effect_matches_group_oracle_and_reuses_real_pool -- --ignored --nocapture
fn native_production_artifact_effect_matches_group_oracle_and_reuses_real_pool()
-> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    // After owner snapping the root is [9, 3] + [54, 48], the 0.625 group is
    // [21, 13] + [32, 26], and its green child is [29, 19] + [20, 16]. The
    // group rasters into an RGBA8 layer, then composites over the quantized
    // root background; quantize at both writes as the targets do.
    let background = quantize_premultiplied_rgba8(linear_opaque([24, 64, 116]));
    let group = |content: [u8; 3]| {
        let layer = quantize_premultiplied_rgba8(linear_opaque(content));
        premultiplied_to_readback_rgba8(quantize_premultiplied_rgba8(source_over(
            scale_premultiplied(layer, 0.625),
            background,
        )))
    };
    let root = premultiplied_to_readback_rgba8(background);
    let red = group([224, 72, 36]);
    let green = group([36, 184, 92]);
    let geometry = |pixels: &[u8], case: &str| {
        for (at, expected, region) in [
            // Immediately outside the group: no halo past its authored box.
            ([20, 20], root, "outside group left"),
            ([30, 12], root, "outside group top"),
            ([53, 20], root, "outside group right"),
            ([30, 39], root, "outside group bottom"),
            ([23, 15], red, "group without overlap"),
            ([51, 37], red, "group without overlap, far corner"),
            ([39, 27], green, "overlap, composed once"),
        ] {
            assert_pixel_near(
                pixels,
                at[0],
                at[1],
                expected,
                1,
                &format!("{case}/{region}"),
            )?;
        }
        Ok(())
    };
    let (surface_count, bytes) = verify_cold_warm_artifact_surface(
        gpu,
        &adapter,
        "production-artifact-effect",
        nested_effect_fixture,
        ARTIFACT_HOST_PLACEMENT_OFFSET,
        &geometry,
    )?;
    if surface_count == 0 || bytes == 0 {
        return Err(format!(
            "production artifact effect gate requires detached surfaces and descriptor bytes on {adapter}: surfaces={surface_count}, bytes={bytes}"
        ));
    }
    eprintln!(
        "production artifact effect oracle/reuse passed on {adapter}: aggregate_color_depth_bytes={bytes}"
    );
    Ok(())
}
