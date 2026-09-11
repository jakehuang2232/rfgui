use super::*;

use super::artifact_intermediate_coverage_tests::{
    IntermediateSurfaceCoverage, validate_artifact_roundtrip_differences_are_partial_coverage_only,
    validate_artifact_roundtrip_differences_use_intermediate_partial_coverage,
};
use crate::view::viewport::{
    ArtifactSurfaceIntermediateReadbackForTest, AutoArtifactSurfaceEmissionForTest,
    emit_retained_auto_artifact_surface_for_test,
};

type ArtifactScrollFixture = fn() -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker);

pub(super) struct NativeArtifactTextThreadCacheCleanup;

impl Drop for NativeArtifactTextThreadCacheCleanup {
    fn drop(&mut self) {
        crate::view::render_pass::text_pass::clear_text_resources_cache();
    }
}

pub(super) fn zero_offset_single_scroll_content_fixture()
-> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    scroll_scene_gpu_fixture(
        ScrollSceneGpuCase {
            name: "diagnostic-artifact-single-scroll-content-offset-zero",
            offset_y: 0.0,
            content_height: 300.0,
            backing: ScrollSceneBackingKind::Single,
            max_dimension_2d: 8192,
            transition_local_y: 33.0,
        },
        GpuScrollbarCase::Hidden,
    )
}

pub(super) fn offset_single_scroll_content_fixture()
-> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    scroll_scene_gpu_fixture(
        ScrollSceneGpuCase {
            name: "artifact-single-scroll-content-offset-thirteen",
            offset_y: 13.0,
            content_height: 300.0,
            backing: ScrollSceneBackingKind::Single,
            max_dimension_2d: 8192,
            transition_local_y: 33.0,
        },
        GpuScrollbarCase::Hidden,
    )
}

pub(super) fn nested_multi_leaf_fixture()
-> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let (mut arena, outer, mut properties, mut generations) =
        nested_scroll_gpu_leaf_fixture(NestedScrollGpuLeafKind::Rect, 13.0, 9.0);
    let inner = arena.children_of(outer)[0];
    assert_eq!(arena.children_of(inner).len(), 1);

    let mut extra = Element::new_with_id(0xc3_c320, 62.0, 12.0, 28.0, 24.0);
    extra.set_background_color_value(Color::rgb(32, 208, 104));
    set_nested_scroll_gpu_position(&mut extra, 62.0, 12.0);
    let extra = arena.insert(Node::new(Box::new(extra)));
    arena.set_parent(extra, Some(inner));
    arena.push_child(inner, extra);
    arena
        .get_mut(extra)
        .expect("extra nested leaf")
        .element
        .clear_local_dirty_flags(DirtyFlags::ALL);
    arena.clear_arena_dirty_subtree(extra, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(outer);
    properties.sync(&arena, &[outer]);
    generations.sync(&arena, &[outer], &properties);
    assert_eq!(
        arena.children_of(inner).len(),
        2,
        "the Artifact nested rect gate must contain two ordinary direct leaves"
    );
    (arena, outer, properties, generations)
}

fn nested_inline_ifc_text_fixture() -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let (arena, outer, properties, generations) =
        crate::view::paint::nested_scroll_unready_text_fixture_for_test(
            crate::view::paint::NestedTextFallbackKind::InlineIfcOwned,
        );
    let inner = arena.children_of(outer)[0];
    let text = arena.children_of(inner)[0];
    let text_node = arena.get(text).expect("nested IFC-owned Text");
    let text_element = text_node
        .element
        .as_any()
        .downcast_ref::<Text>()
        .expect("nested payload is Text");
    assert!(
        text_element
            .inline_ifc_owned_paint_geometry_for_test()
            .is_some(),
        "the Artifact nested payload gate must contain IFC-owned Text geometry"
    );
    drop(text_node);
    (arena, outer, properties, generations)
}

fn legacy_artifact_scroll_fixture_graph(
    fixture: ArtifactScrollFixture,
    paint_offset: [f32; 2],
) -> Result<FrameGraph, String> {
    let (mut arena, root, _, _) = fixture();
    let (mut graph, mut ctx, target) = transformed_graph_prelude(1.0, None);
    ctx.set_paint_offset(paint_offset);
    arena
        .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
        .ok_or_else(|| "legacy Artifact scroll root disappeared".to_owned())?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

pub(super) fn production_artifact_scroll_fixture_graph(
    viewport: &mut Viewport,
    fixture: ArtifactScrollFixture,
) -> Result<(FrameGraph, AutoArtifactSurfaceEmissionForTest), String> {
    production_artifact_scroll_fixture_graph_with_paint_offset(viewport, fixture, [0.0, 0.0])
}

fn production_artifact_scroll_fixture_graph_with_paint_offset(
    viewport: &mut Viewport,
    fixture: ArtifactScrollFixture,
    paint_offset: [f32; 2],
) -> Result<(FrameGraph, AutoArtifactSurfaceEmissionForTest), String> {
    let (arena, root, properties, generations) = fixture();
    production_artifact_scroll_arena_graph(
        viewport,
        &arena,
        root,
        &properties,
        &generations,
        paint_offset,
    )
}

fn production_artifact_scroll_arena_graph(
    viewport: &mut Viewport,
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
    paint_offset: [f32; 2],
) -> Result<(FrameGraph, AutoArtifactSurfaceEmissionForTest), String> {
    let roots = [root];
    let (mut graph, mut ctx, target) = transformed_graph_prelude(1.0, None);
    ctx.set_paint_offset(paint_offset);
    let emission = emit_retained_auto_artifact_surface_for_test(
        viewport,
        &arena,
        &roots,
        &properties,
        &generations,
        &mut graph,
        &ctx,
    )?;
    add_present(&mut graph, &target)?;
    Ok((graph, emission))
}

fn relayout_and_sync_single_scroll_fixture(
    arena: &mut NodeArena,
    root: NodeKey,
) -> (PropertyTrees, PaintGenerationTracker) {
    crate::view::test_support::measure_and_place(
        arena,
        root,
        LayoutConstraints {
            max_width: WIDTH as f32,
            max_height: HEIGHT as f32,
            viewport_width: WIDTH as f32,
            viewport_height: HEIGHT as f32,
            percent_base_width: Some(WIDTH as f32),
            percent_base_height: Some(HEIGHT as f32),
        },
        LayoutPlacement {
            parent_x: 8.0,
            parent_y: 8.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: WIDTH as f32,
            available_height: HEIGHT as f32,
            viewport_width: WIDTH as f32,
            viewport_height: HEIGHT as f32,
            percent_base_width: Some(WIDTH as f32),
            percent_base_height: Some(HEIGHT as f32),
        },
    );
    let roots = [root];
    let mut properties = PropertyTrees::default();
    properties.sync(arena, &roots);
    assert!(
        properties.validation_errors.is_empty(),
        "natural scroll-offset fixture property errors: {:?}",
        properties.validation_errors
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(arena, &roots, &properties);
    assert!(generations.matches_live_snapshot(arena, &roots, &properties));
    (properties, generations)
}

fn legacy_immediate_scroll_oracle_pixels(
    gpu: &NativeGpu,
    arena: &mut NodeArena,
    root: NodeKey,
) -> Result<Vec<u8>, String> {
    let (mut graph, ctx, target) = transformed_graph_prelude(1.0, None);
    arena
        .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
        .ok_or_else(|| "legacy immediate scroll root disappeared".to_owned())?;
    add_present(&mut graph, &target)?;
    render(graph, gpu)
}

fn read_artifact_intermediate_surface(
    gpu: &NativeGpu,
    viewport: &Viewport,
    observation: ArtifactSurfaceIntermediateReadbackForTest,
) -> Result<Vec<u8>, String> {
    let padded_bytes_per_row = padded_bytes_per_row(observation.width);
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rfgui Artifact intermediate surface readback"),
        size: u64::from(padded_bytes_per_row) * u64::from(observation.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rfgui Artifact intermediate surface readback"),
        });
    viewport.encode_persistent_render_target_readback_for_test(
        observation.color_key,
        &mut encoder,
        &readback,
        padded_bytes_per_row,
        observation.width,
        observation.height,
    )?;
    let _submission = gpu.queue.submit(Some(encoder.finish()));

    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| format!("intermediate readback wait failed: {error:?}"))?;
    receiver
        .recv()
        .map_err(|error| format!("intermediate readback callback was lost: {error}"))?
        .map_err(|error| format!("intermediate readback map failed: {error:?}"))?;
    let mapped = readback
        .slice(..)
        .get_mapped_range()
        .map_err(|error| format!("failed to access intermediate readback: {error:?}"))?;
    let pixels = remove_row_padding(
        &mapped,
        observation.width,
        observation.height,
        padded_bytes_per_row,
    )?;
    drop(mapped);
    readback.unmap();
    Ok(pixels)
}

fn verify_artifact_scroll_fixture(
    gpu: &NativeGpu,
    adapter: &str,
    case: &str,
    fixture: ArtifactScrollFixture,
) -> Result<(usize, u64), String> {
    let legacy_graph =
        legacy_artifact_scroll_fixture_graph(fixture, ARTIFACT_HOST_PLACEMENT_OFFSET)?;
    let legacy_pixels = render(legacy_graph, gpu)?;
    let mut viewport = Viewport::new();

    let (cold_graph, cold) = production_artifact_scroll_fixture_graph_with_paint_offset(
        &mut viewport,
        fixture,
        ARTIFACT_HOST_PLACEMENT_OFFSET,
    )?;
    if cold.surface_count == 0
        || cold.aggregate_texture_bytes == 0
        || cold.actions.len() != cold.surface_count
        || cold
            .actions
            .iter()
            .any(|action| *action != RetainedSurfaceCompileAction::Reraster)
    {
        return Err(format!(
            "{case}: cold Artifact scroll frame must reraster non-empty surfaces on {adapter}: surfaces={}, bytes={}, actions={:?}",
            cold.surface_count, cold.aggregate_texture_bytes, cold.actions
        ));
    }
    let cold_pixels = render_on_viewport(cold_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(cold.frame_owner), true) {
        return Err(format!(
            "{case}: cold Artifact scroll transaction did not commit"
        ));
    }

    let (warm_graph, warm) = production_artifact_scroll_fixture_graph_with_paint_offset(
        &mut viewport,
        fixture,
        ARTIFACT_HOST_PLACEMENT_OFFSET,
    )?;
    if warm.surface_count != cold.surface_count
        || warm.aggregate_texture_bytes != cold.aggregate_texture_bytes
        || warm.actions.len() != warm.surface_count
        || warm
            .actions
            .iter()
            .any(|action| *action != RetainedSurfaceCompileAction::Reuse)
    {
        return Err(format!(
            "{case}: warm Artifact scroll frame must reuse every surface on {adapter}: cold={:?}/{}B, warm={:?}/{}B",
            cold.actions, cold.aggregate_texture_bytes, warm.actions, warm.aggregate_texture_bytes,
        ));
    }
    let warm_pixels = render_on_viewport(warm_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(warm.frame_owner), true) {
        return Err(format!(
            "{case}: warm Artifact scroll transaction did not commit"
        ));
    }

    super::native_nested_scroll_segment_tests::compare_nested_segment_pixels_within_one_lsb(
        &legacy_pixels,
        &cold_pixels,
        adapter,
        &format!("{case}/cold-vs-legacy"),
    )?;
    validate_artifact_roundtrip_differences_are_partial_coverage_only(
        &legacy_pixels,
        &cold_pixels,
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
// cargo test -q native_production_artifact_nested_scroll_multi_leaf_matches_legacy_within_one_lsb_and_reuses_real_pool -- --ignored --nocapture
fn native_production_artifact_nested_scroll_multi_leaf_matches_legacy_within_one_lsb_and_reuses_real_pool()
-> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let (surface_count, bytes) = verify_artifact_scroll_fixture(
        gpu,
        &adapter,
        "production-artifact-nested-scroll-multi-leaf",
        nested_multi_leaf_fixture,
    )?;
    eprintln!(
        "production Artifact nested multi-leaf scroll parity/reuse passed on {adapter}: surfaces={surface_count}, aggregate_color_depth_bytes={bytes}"
    );
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_production_artifact_nested_scroll_inline_ifc_text_matches_legacy_within_one_lsb_and_reuses_real_pool -- --ignored --nocapture
fn native_production_artifact_nested_scroll_inline_ifc_text_matches_legacy_within_one_lsb_and_reuses_real_pool()
-> Result<(), String> {
    let _thread_cache_cleanup = NativeArtifactTextThreadCacheCleanup;
    crate::view::render_pass::text_pass::clear_text_resources_cache();
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let (surface_count, bytes) = verify_artifact_scroll_fixture(
        gpu,
        &adapter,
        "production-artifact-nested-scroll-inline-ifc-text",
        nested_inline_ifc_text_fixture,
    )?;
    eprintln!(
        "production Artifact nested IFC-owned Text parity/reuse passed on {adapter}: surfaces={surface_count}, aggregate_color_depth_bytes={bytes}"
    );
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_production_artifact_scroll_offset_only_reuses_real_pool_and_matches_legacy_immediate -- --ignored --nocapture
fn native_production_artifact_scroll_offset_only_reuses_real_pool_and_matches_legacy_immediate()
-> Result<(), String> {
    const MOVED_OFFSET_Y: f32 = 13.0;

    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let (mut arena, root, _, _) = zero_offset_single_scroll_content_fixture();
    let (cold_properties, cold_generations) =
        relayout_and_sync_single_scroll_fixture(&mut arena, root);
    let mut viewport = Viewport::new();

    let (cold_graph, cold) = production_artifact_scroll_arena_graph(
        &mut viewport,
        &arena,
        root,
        &cold_properties,
        &cold_generations,
        [0.0, 0.0],
    )?;
    if cold.surface_count == 0
        || cold.aggregate_texture_bytes == 0
        || cold.actions.len() != cold.surface_count
        || cold
            .actions
            .iter()
            .any(|action| *action != RetainedSurfaceCompileAction::Reraster)
    {
        return Err(format!(
            "offset-zero cold frame must reraster non-empty Artifact surfaces on {adapter}: surfaces={}, bytes={}, actions={:?}",
            cold.surface_count, cold.aggregate_texture_bytes, cold.actions
        ));
    }
    let cold_pixels = render_on_viewport(cold_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(cold.frame_owner), true) {
        return Err("offset-zero cold Artifact transaction did not commit".to_owned());
    }

    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_scroll_offset((0.0, MOVED_OFFSET_Y));
    let (moved_properties, moved_generations) =
        relayout_and_sync_single_scroll_fixture(&mut arena, root);
    let moved_scroll = moved_properties
        .scrolls
        .get(&crate::view::compositor::property_tree::ScrollNodeId(root))
        .ok_or_else(|| "moved frame lost its ScrollContent snapshot".to_owned())?;
    if moved_scroll.offset.y.to_bits() != MOVED_OFFSET_Y.to_bits() {
        return Err(format!(
            "requested scroll offset was clamped or lost: requested={MOVED_OFFSET_Y}, actual={}",
            moved_scroll.offset.y
        ));
    }

    let (warm_graph, warm) = production_artifact_scroll_arena_graph(
        &mut viewport,
        &arena,
        root,
        &moved_properties,
        &moved_generations,
        [0.0, 0.0],
    )?;
    if warm.surface_count != cold.surface_count
        || warm.aggregate_texture_bytes != cold.aggregate_texture_bytes
        || warm.actions.len() != warm.surface_count
        || warm
            .actions
            .iter()
            .any(|action| *action != RetainedSurfaceCompileAction::Reuse)
    {
        return Err(format!(
            "offset-only warm frame must reuse the cold resident set on {adapter}: cold={:?}/{}B, warm={:?}/{}B",
            cold.actions, cold.aggregate_texture_bytes, warm.actions, warm.aggregate_texture_bytes,
        ));
    }
    let warm_pixels = render_on_viewport(warm_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    let [intermediate_observation] = warm.intermediate_surfaces.as_slice() else {
        return Err(format!(
            "offset-only gate requires exactly one readable Artifact intermediate surface on {adapter}: observations={:?}",
            warm.intermediate_surfaces
        ));
    };
    let intermediate_pixels =
        read_artifact_intermediate_surface(gpu, &viewport, *intermediate_observation)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(warm.frame_owner), true) {
        return Err("offset-only warm Artifact transaction did not commit".to_owned());
    }
    if cold_pixels == warm_pixels {
        return Err(format!(
            "offset-only gate rendered identical cold and warm frames on {adapter}; the fixture did not exercise visible scrolling"
        ));
    }

    // Before Artifact admission moved ahead of the scroll cascade, this exact
    // naturally re-laid-out shape exhausted the retained scroll candidates and
    // ended at the selector's immediate legacy painter. Build that named oracle
    // from an independent arena so the expected pixels do not reuse Artifact
    // preparation or the cold frame's retained resources.
    let (mut oracle_arena, oracle_root, _, _) = zero_offset_single_scroll_content_fixture();
    let _ = relayout_and_sync_single_scroll_fixture(&mut oracle_arena, oracle_root);
    crate::view::test_support::get_element_mut::<Element>(&oracle_arena, oracle_root)
        .set_scroll_offset((0.0, MOVED_OFFSET_Y));
    let _ = relayout_and_sync_single_scroll_fixture(&mut oracle_arena, oracle_root);
    let oracle_pixels = legacy_immediate_scroll_oracle_pixels(gpu, &mut oracle_arena, oracle_root)?;
    super::native_nested_scroll_segment_tests::compare_nested_segment_pixels_within_one_lsb(
        &oracle_pixels,
        &warm_pixels,
        &adapter,
        "production-artifact-scroll-offset-only/legacy-immediate-oracle",
    )?;
    validate_artifact_roundtrip_differences_use_intermediate_partial_coverage(
        &oracle_pixels,
        &warm_pixels,
        IntermediateSurfaceCoverage {
            pixels: &intermediate_pixels,
            width: intermediate_observation.width,
            height: intermediate_observation.height,
            source_physical_origin: intermediate_observation.source_physical_origin,
        },
        &adapter,
        "production-artifact-scroll-offset-only/legacy-immediate-oracle",
    )?;
    eprintln!(
        "production Artifact scroll-offset-only reuse passed on {adapter}: offset={MOVED_OFFSET_Y}, surfaces={}, aggregate_color_depth_bytes={}",
        warm.surface_count, warm.aggregate_texture_bytes
    );
    Ok(())
}
