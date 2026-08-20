use super::*;
use crate::view::paint::scroll_scene::{
    emit_prepared_nested_scroll_segment_scene,
    emit_prepared_nested_scroll_segment_text_direct_scene_for_test,
    plan_and_validate_nested_scroll_segment_scene, prepare_nested_scroll_segment_scene_from_pool,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SegmentLeafPaint {
    Baseline,
    ChangedImage,
}

fn changed_nested_segment_image_pixels() -> std::sync::Arc<[u8]> {
    let mut pixels = Vec::with_capacity(4 * 4 * 4);
    for y in 0..4 {
        for x in 0..4 {
            let rgba = if (x + y) % 2 == 0 {
                [32, 176, 224, 255]
            } else {
                [184, 48, 216, 255]
            };
            pixels.extend_from_slice(&rgba);
        }
    }
    std::sync::Arc::from(pixels)
}

struct DirectSegmentGraph {
    graph: FrameGraph,
    trace: RetainedPropertyScrollSceneBuildTrace,
    owner: crate::view::viewport::RetainedSurfaceFrameStageOwner,
    leaf_key: crate::view::frame_graph::PersistentTextureKey,
    leaf_desc: crate::view::frame_graph::TextureDesc,
}

struct DirectTextPrimitiveGraph {
    graph: FrameGraph,
    owner: crate::view::viewport::RetainedSurfaceFrameStageOwner,
}

fn nested_segment_gpu_fixture(
    kind: NestedScrollGpuLeafKind,
    outer_offset_y: f32,
    inner_offset_y: f32,
    paint: SegmentLeafPaint,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let (mut arena, root, mut properties, mut generations) =
        nested_scroll_gpu_leaf_fixture(kind, outer_offset_y, inner_offset_y);
    if paint != SegmentLeafPaint::Baseline {
        let inner = arena.children_of(root)[0];
        let leaf = arena.children_of(inner)[0];
        match paint {
            SegmentLeafPaint::Baseline => unreachable!(),
            SegmentLeafPaint::ChangedImage => {
                assert_eq!(kind, NestedScrollGpuLeafKind::Image);
                crate::view::test_support::get_element_mut::<Image>(&arena, leaf).set_source(
                    ImageSource::Rgba {
                        width: 4,
                        height: 4,
                        pixels: changed_nested_segment_image_pixels(),
                    },
                );
                arena.with_element_taken(leaf, |element, arena| element.sync_arena(arena));
                prepare_nested_scroll_gpu_leaf(&mut arena, leaf, 2);
                arena.with_element_taken(leaf, |element, _arena| {
                    element.clear_local_dirty_flags(crate::view::base_component::DirtyFlags::ALL);
                });
                arena.clear_arena_dirty_subtree(leaf, crate::view::base_component::DirtyFlags::ALL);
            }
        }
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
    }
    (arena, root, properties, generations)
}

#[test]
#[ignore = "requires native GPU adapter: nested-segment Image/SVG DPR1/DPR2 whole-frame per-channel <=1 LSB gate"]
// Run explicitly with:
// cargo test -q native_direct_nested_scroll_segment_image_svg_dpr1_dpr2_one_lsb_gate -- --ignored --nocapture
fn native_direct_nested_scroll_segment_image_svg_dpr1_dpr2_one_lsb_gate() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    for kind in [NestedScrollGpuLeafKind::Image, NestedScrollGpuLeafKind::Svg] {
        for scale_factor in [1.0, 2.0] {
            let outer_offset_y = 13.0;
            let inner_offset_y = 9.0;
            let legacy = render_legacy_nested_segment(
                gpu,
                kind,
                outer_offset_y,
                inner_offset_y,
                scale_factor,
                SegmentLeafPaint::Baseline,
            )?;
            let mut viewport = Viewport::new();
            let direct = direct_nested_segment_graph(
                &mut viewport,
                kind,
                outer_offset_y,
                inner_offset_y,
                scale_factor,
                SegmentLeafPaint::Baseline,
            )?;
            if (direct.trace.reraster_count, direct.trace.reuse_count) != (1, 0) {
                return Err(format!(
                    "cold direct nested-segment {} DPR{scale_factor} did not choose R: {:?}",
                    kind.label(),
                    direct.trace
                ));
            }
            let direct_pixels =
                render_on_viewport(direct.graph, gpu, &mut viewport, scale_factor, FORMAT)?;
            if !viewport.finish_retained_surface_transaction_for_frame(Some(direct.owner), true) {
                return Err(format!(
                    "cold direct nested-segment {} DPR{scale_factor} transaction did not commit",
                    kind.label()
                ));
            }
            if !viewport
                .has_compatible_persistent_render_target_pair(direct.leaf_key, &direct.leaf_desc)
            {
                return Err(format!(
                    "cold direct nested-segment {} DPR{scale_factor} did not establish leaf residency",
                    kind.label()
                ));
            }
            viewport.forget_retained_surface_pair_witness_for_test(direct.leaf_key);
            if !viewport
                .has_compatible_persistent_render_target_pair(direct.leaf_key, &direct.leaf_desc)
            {
                return Err(format!(
                    "direct nested-segment {} DPR{scale_factor} residency depended on test witness",
                    kind.label()
                ));
            }
            validate_nested_segment_non_clear_pixels(
                &legacy,
                &format!("legacy {} DPR{scale_factor}", kind.label()),
            )?;
            validate_nested_segment_non_clear_pixels(
                &direct_pixels,
                &format!("direct {} DPR{scale_factor}", kind.label()),
            )?;
            validate_nested_scroll_leaf_anchor(&legacy, kind)?;
            validate_nested_scroll_leaf_anchor(&direct_pixels, kind)?;
            compare_nested_segment_pixels_within_one_lsb(
                &legacy,
                &direct_pixels,
                &adapter,
                &format!("direct-nested-segment-{}/dpr-{scale_factor}", kind.label()),
            )?;
        }
    }
    eprintln!("direct nested-segment Image/SVG DPR1/DPR2 <=1 LSB parity passed on {adapter}");
    Ok(())
}

fn legacy_nested_segment_graph(
    kind: NestedScrollGpuLeafKind,
    outer_offset_y: f32,
    inner_offset_y: f32,
    scale_factor: f32,
    paint: SegmentLeafPaint,
) -> Result<FrameGraph, String> {
    let (mut arena, root, _, _) =
        nested_segment_gpu_fixture(kind, outer_offset_y, inner_offset_y, paint);
    let (mut graph, ctx, target) = transformed_graph_prelude(scale_factor, None);
    arena
        .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
        .ok_or_else(|| "legacy nested-segment root disappeared".to_string())?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn direct_nested_segment_graph(
    viewport: &mut Viewport,
    kind: NestedScrollGpuLeafKind,
    outer_offset_y: f32,
    inner_offset_y: f32,
    scale_factor: f32,
    paint: SegmentLeafPaint,
) -> Result<DirectSegmentGraph, String> {
    let (arena, root, properties, generations) =
        nested_segment_gpu_fixture(kind, outer_offset_y, inner_offset_y, paint);
    let scene = plan_and_validate_nested_scroll_segment_scene(
        &arena,
        &[root],
        &properties,
        &generations,
        scale_factor,
        [0.0; 2],
        None,
        FORMAT,
        ScrollSceneSingleTextureBudget::new(
            wgpu::Limits::default().max_texture_dimension_2d,
            128 * 1024 * 1024,
        )
        .expect("nested-segment GPU budget is non-zero"),
    )
    .map_err(|error| {
        format!(
            "direct nested-segment {} plan rejected: {error:?}",
            kind.label()
        )
    })?;
    let text_coordinates = (kind == NestedScrollGpuLeafKind::Text)
        .then(|| scene.text_coordinates_for_test())
        .flatten();
    if let Some((
        legacy_fragment_origin,
        legacy_first_glyph_final_paint_pos,
        direct_fragment_origin,
        direct_first_glyph_final_paint_pos,
        first_glyph_local_pos,
        composite_destination,
        leaf_texture_origin,
        leaf_source_bounds,
    )) = text_coordinates
    {
        eprintln!(
            "nested-segment Text coordinates: legacy.fragment.origin={legacy_fragment_origin:?}, legacy.first_glyph.final_paint_pos={legacy_first_glyph_final_paint_pos:?}, direct.fragment.origin={direct_fragment_origin:?}, direct.first_glyph.final_paint_pos={direct_first_glyph_final_paint_pos:?}, first_glyph.local_pos={first_glyph_local_pos:?}, render_transform=(legacy=None,direct_leaf=None), composite.destination={composite_destination:?}, leaf_texture_origin={leaf_texture_origin:?}, leaf_source_bounds={leaf_source_bounds:?}, dpr={scale_factor}"
        );
    }
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .ok_or_else(|| "nested-segment retained stage is unavailable".to_string())?;
    let mut graph = FrameGraph::new();
    let prepared = prepare_nested_scroll_segment_scene_from_pool(
        viewport,
        scene,
        &mut graph,
        UiBuildContext::new(WIDTH, HEIGHT, FORMAT, scale_factor),
        [0.0; 4],
        owner,
    )
    .map_err(|error| {
        format!(
            "direct nested-segment {} prepare rejected: {error:?}",
            kind.label()
        )
    })?;
    let outcome = emit_prepared_nested_scroll_segment_scene(prepared);
    let (state, trace) = outcome.into_parts();
    let target = state
        .current_target()
        .ok_or_else(|| "direct nested-segment did not produce a frame target".to_string())?;
    add_present(&mut graph, &target)?;

    let declared = graph
        .declared_persistent_textures()
        .map(|(key, desc)| (key, desc.clone()))
        .collect::<Vec<_>>();
    let colors = declared
        .iter()
        .filter(|(key, _)| key.depth_stencil().is_some())
        .cloned()
        .collect::<Vec<_>>();
    let [(leaf_key, leaf_desc)] = colors.as_slice() else {
        return Err(format!(
            "direct nested-segment must declare one leaf color pair: {declared:?}"
        ));
    };
    let depth_key = leaf_key
        .depth_stencil()
        .ok_or_else(|| "direct nested-segment leaf has no depth pair".to_string())?;
    if declared.len() != 2 || !declared.iter().any(|(key, _)| *key == depth_key) {
        return Err(format!(
            "direct nested-segment persistent pair is incomplete: {declared:?}"
        ));
    }
    Ok(DirectSegmentGraph {
        graph,
        trace,
        owner,
        leaf_key: *leaf_key,
        leaf_desc: leaf_desc.clone(),
    })
}

fn direct_nested_segment_text_primitive_graph(
    viewport: &mut Viewport,
    outer_offset_y: f32,
    inner_offset_y: f32,
    scale_factor: f32,
) -> Result<DirectTextPrimitiveGraph, String> {
    let (arena, root, properties, generations) = nested_segment_gpu_fixture(
        NestedScrollGpuLeafKind::Text,
        outer_offset_y,
        inner_offset_y,
        SegmentLeafPaint::Baseline,
    );
    let scene = plan_and_validate_nested_scroll_segment_scene(
        &arena,
        &[root],
        &properties,
        &generations,
        scale_factor,
        [0.0; 2],
        None,
        FORMAT,
        ScrollSceneSingleTextureBudget::new(
            wgpu::Limits::default().max_texture_dimension_2d,
            128 * 1024 * 1024,
        )
        .expect("nested Text direct-primitive GPU budget is non-zero"),
    )
    .map_err(|error| format!("nested Text direct-primitive plan rejected: {error:?}"))?;
    let expected_fragment_origin = scene.legacy_leaf_origin_for_test().ok_or_else(|| {
        "nested Text direct-primitive scene has no legacy leaf origin".to_string()
    })?;
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .ok_or_else(|| "nested Text direct-primitive frame stage is unavailable".to_string())?;
    let mut graph = FrameGraph::new();
    let prepared = prepare_nested_scroll_segment_scene_from_pool(
        viewport,
        scene,
        &mut graph,
        UiBuildContext::new(WIDTH, HEIGHT, FORMAT, scale_factor),
        [0.0; 4],
        owner,
    )
    .map_err(|error| format!("nested Text direct-primitive prepare rejected: {error:?}"))?;
    let state = emit_prepared_nested_scroll_segment_text_direct_scene_for_test(prepared)
        .map_err(str::to_string)?;
    let target = state
        .current_target()
        .ok_or_else(|| "nested Text direct primitive did not produce a frame target".to_string())?;
    if graph.declared_persistent_texture_keys().next().is_some() {
        return Err(
            "nested Text direct primitive must not declare persistent R1 or A0".to_string(),
        );
    }
    if viewport.retained_property_scroll_scene_stage_is_available() {
        return Err(
            "cold nested Text direct primitive did not stage its zero-resident transaction"
                .to_string(),
        );
    }
    let composites = graph
        .test_graphics_passes::<
            crate::view::render_pass::texture_composite_pass::TextureCompositePass,
        >()
        .len();
    let text_passes =
        graph.test_graphics_passes::<crate::view::render_pass::text_pass::TextPreparedInputPass>();
    if composites != 0 || text_passes.len() != 1 {
        return Err(format!(
            "nested Text direct primitive graph must contain one direct Text run and no leaf composite: text={}, composites={composites}",
            text_passes.len()
        ));
    }
    let snapshot = text_passes[0].test_snapshot();
    eprintln!("direct Text primitive pass: {snapshot:?}");
    if snapshot.fragments.as_slice()
        != [
            crate::view::render_pass::text_pass::TextPreparedFragmentTestSnapshot {
                origin_bits: expected_fragment_origin.map(f32::to_bits),
                size_bits: [100.0_f32.to_bits(), 600.0_f32.to_bits()],
            },
        ]
        || snapshot.pass_context.stencil_clip_id != Some(2)
        || !snapshot.pass_context.uses_depth_stencil
        || snapshot.pass_context.scissor_rect.is_none()
    {
        return Err(format!(
            "nested Text direct primitive did not preserve raw phase under the active mask/scissor: {snapshot:?}"
        ));
    }
    add_present(&mut graph, &target)?;
    Ok(DirectTextPrimitiveGraph { graph, owner })
}

fn render_legacy_nested_segment(
    gpu: &NativeGpu,
    kind: NestedScrollGpuLeafKind,
    outer_offset_y: f32,
    inner_offset_y: f32,
    scale_factor: f32,
    paint: SegmentLeafPaint,
) -> Result<Vec<u8>, String> {
    render_with_config(
        legacy_nested_segment_graph(kind, outer_offset_y, inner_offset_y, scale_factor, paint)?,
        gpu,
        scale_factor,
        FORMAT,
    )
}

fn validate_nested_segment_non_clear_pixels(pixels: &[u8], path: &str) -> Result<(), String> {
    let non_clear = pixels
        .chunks_exact(BYTES_PER_PIXEL as usize)
        .filter(|pixel| *pixel != [0, 0, 0, 0])
        .count();
    (non_clear > 0)
        .then_some(())
        .ok_or_else(|| format!("{path} nested-segment frame contains no non-clear pixels"))
}

fn compare_nested_segment_pixels_within_one_lsb(
    legacy: &[u8],
    direct: &[u8],
    adapter: &str,
    case: &str,
) -> Result<(), String> {
    if legacy.len() != direct.len() {
        return Err(format!(
            "{case}: pixel buffer lengths differ on {adapter}: legacy={}, direct={}",
            legacy.len(),
            direct.len()
        ));
    }
    let mut diff = PixelDiff::default();
    for pixel_index in 0..(WIDTH * HEIGHT) as usize {
        let x = pixel_index as u32 % WIDTH;
        let y = pixel_index as u32 / WIDTH;
        let offset = pixel_index * BYTES_PER_PIXEL as usize;
        let mut pixel_failed = false;
        for channel in 0..BYTES_PER_PIXEL as usize {
            let delta = legacy[offset + channel].abs_diff(direct[offset + channel]);
            diff.max_channel_delta = diff.max_channel_delta.max(delta);
            if delta > 1 {
                pixel_failed = true;
            }
        }
        if !pixel_failed {
            continue;
        }
        diff.mismatched_pixels += 1;
        diff.bounds = Some(match diff.bounds {
            None => [x, y, x, y],
            Some([left, top, right, bottom]) => {
                [left.min(x), top.min(y), right.max(x), bottom.max(y)]
            }
        });
    }
    if diff.mismatched_pixels == 0 {
        return Ok(());
    }
    Err(format!(
        "{case}: legacy/direct nested-segment pixel mismatch on {adapter}: mismatched_pixels={}, max_channel_delta={}, bounds={:?}, rule=whole-frame every-channel delta<=1 LSB",
        diff.mismatched_pixels, diff.max_channel_delta, diff.bounds
    ))
}

fn nested_segment_vertical_edge_histogram(
    pixels: &[u8],
    x: u32,
) -> Result<std::collections::BTreeMap<[u8; 4], usize>, String> {
    let mut histogram = std::collections::BTreeMap::new();
    for y in 20..HEIGHT {
        *histogram.entry(pixel_at(pixels, x, y)?).or_default() += 1;
    }
    Ok(histogram)
}

fn nested_text_pixel_diagnostics(pixels: &[u8]) -> Result<String, String> {
    let background = [24, 48, 72, 255];
    let mut bounds: Option<[u32; 4]> = None;
    let mut rows = std::collections::BTreeMap::<u32, usize>::new();
    let mut columns = std::collections::BTreeMap::<u32, usize>::new();
    let mut samples = Vec::new();
    for y in 20..HEIGHT {
        for x in 11..WIDTH {
            let pixel = pixel_at(pixels, x, y)?;
            if pixel == background {
                continue;
            }
            bounds = Some(match bounds {
                None => [x, y, x, y],
                Some([left, top, right, bottom]) => {
                    [left.min(x), top.min(y), right.max(x), bottom.max(y)]
                }
            });
            *rows.entry(y).or_default() += 1;
            *columns.entry(x).or_default() += 1;
            if samples.len() < 12 {
                samples.push((x, y, pixel));
            }
        }
    }
    Ok(format!(
        "text_like_bounds={bounds:?}, rows={rows:?}, columns={columns:?}, first_pixels={samples:?}"
    ))
}

#[test]
#[ignore = "requires native GPU adapter: nested-segment Rect DPR1/DPR2 whole-frame per-channel <=1 LSB gate"]
// Run explicitly with:
// cargo test -q native_direct_nested_scroll_segment_rect_matches_legacy_within_one_lsb_at_dpr1_dpr2 -- --ignored --nocapture
fn native_direct_nested_scroll_segment_rect_matches_legacy_within_one_lsb_at_dpr1_dpr2()
-> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let cases = [
        (NestedScrollGpuLeafKind::Rect, 13.0, 9.0, 1.0),
        (NestedScrollGpuLeafKind::Rect, 13.0, 9.0, 2.0),
    ];
    for (kind, outer_offset_y, inner_offset_y, scale_factor) in cases {
        let legacy = render_legacy_nested_segment(
            gpu,
            kind,
            outer_offset_y,
            inner_offset_y,
            scale_factor,
            SegmentLeafPaint::Baseline,
        )?;
        let mut viewport = Viewport::new();
        let direct = direct_nested_segment_graph(
            &mut viewport,
            kind,
            outer_offset_y,
            inner_offset_y,
            scale_factor,
            SegmentLeafPaint::Baseline,
        )?;
        if direct.trace.reraster_count != 1 || direct.trace.reuse_count != 0 {
            return Err(format!(
                "cold direct nested-segment {} DPR{scale_factor} did not choose R: {:?}",
                kind.label(),
                direct.trace
            ));
        }
        let direct_pixels =
            render_on_viewport(direct.graph, gpu, &mut viewport, scale_factor, FORMAT)?;
        if !viewport.finish_retained_surface_transaction_for_frame(Some(direct.owner), true) {
            return Err("cold direct nested-segment transaction did not commit".to_string());
        }
        if !viewport
            .has_compatible_persistent_render_target_pair(direct.leaf_key, &direct.leaf_desc)
        {
            return Err("cold direct nested-segment did not establish leaf residency".to_string());
        }
        viewport.forget_retained_surface_pair_witness_for_test(direct.leaf_key);
        if !viewport
            .has_compatible_persistent_render_target_pair(direct.leaf_key, &direct.leaf_desc)
        {
            return Err("direct nested-segment residency depended on test witness".to_string());
        }
        validate_nested_segment_non_clear_pixels(
            &legacy,
            &format!("legacy {} DPR{scale_factor}", kind.label()),
        )?;
        validate_nested_segment_non_clear_pixels(
            &direct_pixels,
            &format!("direct {} DPR{scale_factor}", kind.label()),
        )?;
        compare_nested_segment_pixels_within_one_lsb(
            &legacy,
            &direct_pixels,
            &adapter,
            &format!("direct-nested-segment-{}/dpr-{scale_factor}", kind.label()),
        )
        .map_err(|error| {
            let diagnostics = (|| {
                Some(format!(
                    "x10 legacy={:?} direct={:?}; x11 legacy={:?} direct={:?}; endpoints legacy={:?} direct={:?}",
                    nested_segment_vertical_edge_histogram(&legacy, 10).ok()?,
                    nested_segment_vertical_edge_histogram(&direct_pixels, 10).ok()?,
                    nested_segment_vertical_edge_histogram(&legacy, 11).ok()?,
                    nested_segment_vertical_edge_histogram(&direct_pixels, 11).ok()?,
                    [pixel_at(&legacy, 10, 20).ok()?, pixel_at(&legacy, 10, 63).ok()?],
                    [
                        pixel_at(&direct_pixels, 10, 20).ok()?,
                        pixel_at(&direct_pixels, 10, 63).ok()?
                    ],
                ))
            })()
            .unwrap_or_else(|| "edge diagnostics unavailable".to_string());
            format!("{error}; {diagnostics}")
        })?;
    }
    eprintln!("direct nested-segment Rect DPR1/DPR2 <=1 LSB parity passed on {adapter}");
    Ok(())
}

struct NativeTextThreadCacheCleanup;

impl Drop for NativeTextThreadCacheCleanup {
    fn drop(&mut self) {
        crate::view::render_pass::text_pass::clear_text_resources_cache();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NestedSegmentTextPixelExpectation {
    Exact,
    WholeFrameWithinOneLsb,
}

fn run_rgba8_text_dpr2_gate(
    outer_offset_y: f32,
    inner_offset_y: f32,
    case: &str,
    expectation: NestedSegmentTextPixelExpectation,
) -> Result<(), String> {
    let _thread_cache_cleanup = NativeTextThreadCacheCleanup;
    crate::view::render_pass::text_pass::clear_text_resources_cache();
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let kind = NestedScrollGpuLeafKind::Text;
    let scale_factor = 2.0;
    let legacy = render_legacy_nested_segment(
        gpu,
        kind,
        outer_offset_y,
        inner_offset_y,
        scale_factor,
        SegmentLeafPaint::Baseline,
    )?;
    let mut viewport = Viewport::new();
    let direct = direct_nested_segment_text_primitive_graph(
        &mut viewport,
        outer_offset_y,
        inner_offset_y,
        scale_factor,
    )?;
    let direct_pixels = render_on_viewport(direct.graph, gpu, &mut viewport, scale_factor, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(direct.owner), true) {
        return Err("cold direct nested-segment transaction did not commit".to_string());
    }
    validate_nested_segment_non_clear_pixels(&legacy, "legacy Text DPR2")?;
    validate_nested_segment_non_clear_pixels(&direct_pixels, "direct Text DPR2")?;
    match expectation {
        NestedSegmentTextPixelExpectation::Exact => compare_pixels(
            &legacy,
            &direct_pixels,
            [0, 0, WIDTH, HEIGHT],
            &adapter,
            case,
        ),
        NestedSegmentTextPixelExpectation::WholeFrameWithinOneLsb => {
            compare_nested_segment_pixels_within_one_lsb(&legacy, &direct_pixels, &adapter, case)
        }
    }
    .map_err(|error| {
        let diagnostics = (|| {
            Some(format!(
                "x10 legacy={:?} direct={:?}; x11 legacy={:?} direct={:?}",
                nested_segment_vertical_edge_histogram(&legacy, 10).ok()?,
                nested_segment_vertical_edge_histogram(&direct_pixels, 10).ok()?,
                nested_segment_vertical_edge_histogram(&legacy, 11).ok()?,
                nested_segment_vertical_edge_histogram(&direct_pixels, 11).ok()?,
            ))
        })()
        .unwrap_or_else(|| "edge diagnostics unavailable".to_string());
        format!("{error}; {diagnostics}")
    })
}

#[test]
#[ignore = "known red gate: RGBA8 physical-integer/logical-fractional Text DPR2 must remain exact"]
fn native_direct_nested_scroll_segment_rgba8_physical_integer_logical_fractional_text_dpr2_exact_gate()
-> Result<(), String> {
    run_rgba8_text_dpr2_gate(
        0.5,
        0.0,
        "direct-nested-segment-text/dpr-2-physical-integer-logical-fractional-rgba8",
        NestedSegmentTextPixelExpectation::Exact,
    )
}

#[test]
#[ignore = "known red gate: RGBA8 fractional-phase Text DPR2 must remain exact"]
fn native_direct_nested_scroll_segment_rgba8_fractional_phase_text_dpr2_exact_gate()
-> Result<(), String> {
    run_rgba8_text_dpr2_gate(
        0.5,
        0.25,
        "direct-nested-segment-text/dpr-2-fractional-phase-rgba8",
        NestedSegmentTextPixelExpectation::Exact,
    )
}

#[test]
#[ignore = "requires native GPU adapter: RGBA8 zero-offset Text DPR2 whole-frame per-channel <=1 LSB gate"]
fn native_direct_nested_scroll_segment_rgba8_zero_offset_text_dpr2_one_lsb_gate()
-> Result<(), String> {
    run_rgba8_text_dpr2_gate(
        0.0,
        0.0,
        "direct-nested-segment-text/dpr-2-zero-offset-rgba8",
        NestedSegmentTextPixelExpectation::WholeFrameWithinOneLsb,
    )
}

#[test]
#[ignore = "requires native GPU adapter: RGBA8 logical-integer Text DPR2 whole-frame per-channel <=1 LSB gate"]
fn native_direct_nested_scroll_segment_rgba8_logical_integer_text_dpr2_one_lsb_gate()
-> Result<(), String> {
    run_rgba8_text_dpr2_gate(
        1.0,
        0.0,
        "direct-nested-segment-text/dpr-2-logical-integer-rgba8",
        NestedSegmentTextPixelExpectation::WholeFrameWithinOneLsb,
    )
}

#[test]
#[ignore = "requires native GPU adapter: browser-style phase-sensitive Text direct primitive"]
// Run explicitly with:
// cargo test -q native_nested_scroll_segment_phase_sensitive_text_direct_primitive_dpr2 -- --ignored --nocapture
fn native_nested_scroll_segment_phase_sensitive_text_direct_primitive_dpr2() -> Result<(), String> {
    let _thread_cache_cleanup = NativeTextThreadCacheCleanup;
    crate::view::render_pass::text_pass::clear_text_resources_cache();
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let scale_factor = 2.0;
    let mut pixel_failures = Vec::new();
    for (outer_offset_y, inner_offset_y, case) in [
        (0.5, 0.0, "outer-half-inner-zero"),
        (0.5, 0.25, "outer-half-inner-quarter"),
    ] {
        let legacy_graph = legacy_nested_segment_graph(
            NestedScrollGpuLeafKind::Text,
            outer_offset_y,
            inner_offset_y,
            scale_factor,
            SegmentLeafPaint::Baseline,
        )?;
        let legacy_text_passes = legacy_graph
            .test_graphics_passes::<crate::view::render_pass::text_pass::TextPreparedInputPass>(
        );
        eprintln!(
            "legacy Text pass: {:?}",
            legacy_text_passes[0].test_snapshot()
        );
        let legacy = render_with_config(legacy_graph, gpu, scale_factor, FORMAT)?;
        let mut viewport = Viewport::new();
        let direct = direct_nested_segment_text_primitive_graph(
            &mut viewport,
            outer_offset_y,
            inner_offset_y,
            scale_factor,
        )?;
        let direct_pixels =
            render_on_viewport(direct.graph, gpu, &mut viewport, scale_factor, FORMAT)?;
        if viewport.retained_property_scroll_scene_stage_is_available() {
            return Err(format!(
                "{case}: cold direct Text run did not stage its zero-resident transaction"
            ));
        }
        if !viewport.finish_retained_surface_transaction_for_frame(Some(direct.owner), true) {
            return Err(format!("{case}: frame-stage owner did not finish"));
        }
        validate_nested_segment_non_clear_pixels(&legacy, &format!("legacy {case}"))?;
        validate_nested_segment_non_clear_pixels(&direct_pixels, &format!("direct {case}"))?;
        if let Err(error) = compare_nested_segment_pixels_within_one_lsb(
            &legacy,
            &direct_pixels,
            &adapter,
            &format!("nested-segment-text-direct-primitive/{case}/dpr-2"),
        ) {
            pixel_failures.push(format!(
                "{error}; legacy {}; direct {}",
                nested_text_pixel_diagnostics(&legacy)?,
                nested_text_pixel_diagnostics(&direct_pixels)?,
            ));
        }
    }
    if !pixel_failures.is_empty() {
        return Err(pixel_failures.join(" | "));
    }
    eprintln!("phase-sensitive nested Text direct primitive passed on {adapter}");
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter: real persistent pool parent-scroll U and leaf-paint R gate"]
// Run explicitly with:
// cargo test -q native_direct_nested_scroll_segment_real_pool_reuse_and_leaf_paint_r -- --ignored --nocapture
fn native_direct_nested_scroll_segment_real_pool_reuse_and_leaf_paint_r() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let scale_factor = 1.0;
    let mut viewport = Viewport::new();

    let cold = direct_nested_segment_graph(
        &mut viewport,
        NestedScrollGpuLeafKind::Image,
        13.0,
        9.0,
        scale_factor,
        SegmentLeafPaint::Baseline,
    )?;
    if (cold.trace.reraster_count, cold.trace.reuse_count) != (1, 0) {
        return Err(format!(
            "cold direct nested-segment did not choose R: {:?}",
            cold.trace
        ));
    }
    let cold_key = cold.leaf_key;
    let cold_desc = cold.leaf_desc.clone();
    let cold_composite = cold
        .graph
        .test_graphics_passes::<
            crate::view::render_pass::texture_composite_pass::TextureCompositePass,
        >()
        .last()
        .expect("cold direct segment has a leaf-to-frame composite")
        .test_snapshot();
    let cold_pixels = render_on_viewport(cold.graph, gpu, &mut viewport, scale_factor, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(cold.owner), true) {
        return Err("cold direct nested-segment transaction did not commit".to_string());
    }
    validate_nested_scroll_leaf_anchor(&cold_pixels, NestedScrollGpuLeafKind::Image)?;
    if !viewport.has_compatible_persistent_render_target_pair(cold_key, &cold_desc) {
        return Err("cold direct nested-segment did not establish leaf residency".to_string());
    }
    viewport.forget_retained_surface_pair_witness_for_test(cold_key);
    if !viewport.has_compatible_persistent_render_target_pair(cold_key, &cold_desc) {
        return Err("direct nested-segment reuse depended on test witness".to_string());
    }

    let moved = direct_nested_segment_graph(
        &mut viewport,
        NestedScrollGpuLeafKind::Image,
        14.0,
        9.0,
        scale_factor,
        SegmentLeafPaint::Baseline,
    )?;
    if moved.leaf_key != cold_key || moved.leaf_desc != cold_desc {
        return Err("parent scroll changed direct nested-segment leaf identity".to_string());
    }
    if (moved.trace.reraster_count, moved.trace.reuse_count) != (0, 1) {
        return Err(format!(
            "parent scroll did not reuse direct nested-segment leaf: {:?}",
            moved.trace
        ));
    }
    let moved_composite = moved
        .graph
        .test_graphics_passes::<
            crate::view::render_pass::texture_composite_pass::TextureCompositePass,
        >()
        .last()
        .expect("moved direct segment has a leaf-to-frame composite")
        .test_snapshot();
    eprintln!(
        "parent-scroll composite: cold bounds={:?} scissor={:?}; moved bounds={:?} scissor={:?}",
        cold_composite.bounds_bits,
        cold_composite.effective_scissor_rect,
        moved_composite.bounds_bits,
        moved_composite.effective_scissor_rect,
    );
    let moved_pixels = render_on_viewport(moved.graph, gpu, &mut viewport, scale_factor, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(moved.owner), true) {
        return Err("moved direct nested-segment transaction did not commit".to_string());
    }
    validate_nested_scroll_leaf_anchor(&moved_pixels, NestedScrollGpuLeafKind::Image)?;
    let moved_legacy = render_legacy_nested_segment(
        gpu,
        NestedScrollGpuLeafKind::Image,
        14.0,
        9.0,
        scale_factor,
        SegmentLeafPaint::Baseline,
    )?;
    compare_nested_segment_pixels_within_one_lsb(
        &moved_legacy,
        &moved_pixels,
        &adapter,
        "direct-nested-segment/parent-scroll-u",
    )?;
    if cold_composite.bounds_bits == moved_composite.bounds_bits
        && cold_composite.quad_position_bits == moved_composite.quad_position_bits
        && cold_composite.effective_scissor_rect == moved_composite.effective_scissor_rect
    {
        return Err("parent scroll reuse did not change composite context".to_string());
    }

    let changed = direct_nested_segment_graph(
        &mut viewport,
        NestedScrollGpuLeafKind::Image,
        14.0,
        9.0,
        scale_factor,
        SegmentLeafPaint::ChangedImage,
    )?;
    if changed.leaf_key != cold_key || changed.leaf_desc != cold_desc {
        return Err("leaf paint changed direct nested-segment resident slot".to_string());
    }
    if (changed.trace.reraster_count, changed.trace.reuse_count) != (1, 0) {
        return Err(format!(
            "leaf paint change did not reraster direct nested-segment leaf: {:?}",
            changed.trace
        ));
    }
    let changed_pixels =
        render_on_viewport(changed.graph, gpu, &mut viewport, scale_factor, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(changed.owner), true) {
        return Err("changed direct nested-segment transaction did not commit".to_string());
    }
    let changed_legacy = render_legacy_nested_segment(
        gpu,
        NestedScrollGpuLeafKind::Image,
        14.0,
        9.0,
        scale_factor,
        SegmentLeafPaint::ChangedImage,
    )?;
    compare_nested_segment_pixels_within_one_lsb(
        &changed_legacy,
        &changed_pixels,
        &adapter,
        "direct-nested-segment/leaf-paint-r",
    )?;
    if moved_pixels == changed_pixels {
        return Err("leaf paint reraster did not change composed pixels".to_string());
    }
    eprintln!("direct nested-segment real-pool reuse/R passed on {adapter}");
    Ok(())
}
