use super::*;

use crate::view::viewport::{
    AutoArtifactSurfaceEmissionForTest, emit_retained_auto_artifact_surface_for_test,
};

type ArtifactScrollFixture = fn() -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker);

struct NativeArtifactTextThreadCacheCleanup;

impl Drop for NativeArtifactTextThreadCacheCleanup {
    fn drop(&mut self) {
        crate::view::render_pass::text_pass::clear_text_resources_cache();
    }
}

pub(super) fn validate_artifact_roundtrip_differences_are_partial_coverage_only(
    legacy: &[u8],
    artifact: &[u8],
    adapter: &str,
    case: &str,
) -> Result<(), String> {
    if legacy.len() != artifact.len() {
        return Err(format!(
            "{case}: pixel buffer lengths differ on {adapter}: legacy={}, artifact={}",
            legacy.len(),
            artifact.len()
        ));
    }
    for (pixel_index, (legacy, artifact)) in legacy
        .chunks_exact(BYTES_PER_PIXEL as usize)
        .zip(artifact.chunks_exact(BYTES_PER_PIXEL as usize))
        .enumerate()
    {
        if legacy == artifact {
            continue;
        }
        let legacy_alpha = legacy[3];
        let artifact_alpha = artifact[3];
        if legacy_alpha != artifact_alpha || !(1..=254).contains(&legacy_alpha) {
            return Err(format!(
                "{case}: non-partial-coverage pixel differs on {adapter} at ({}, {}): legacy={legacy:?}, artifact={artifact:?}",
                pixel_index as u32 % WIDTH,
                pixel_index as u32 / WIDTH,
            ));
        }
    }
    Ok(())
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
) -> Result<FrameGraph, String> {
    let (mut arena, root, _, _) = fixture();
    let (mut graph, ctx, target) = transformed_graph_prelude(1.0, None);
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
    let (arena, root, properties, generations) = fixture();
    let roots = [root];
    let (mut graph, ctx, target) = transformed_graph_prelude(1.0, None);
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

fn verify_artifact_scroll_fixture(
    gpu: &NativeGpu,
    adapter: &str,
    case: &str,
    fixture: ArtifactScrollFixture,
) -> Result<(usize, u64), String> {
    let legacy_pixels = render(legacy_artifact_scroll_fixture_graph(fixture)?, gpu)?;
    let mut viewport = Viewport::new();

    let (cold_graph, cold) = production_artifact_scroll_fixture_graph(&mut viewport, fixture)?;
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

    let (warm_graph, warm) = production_artifact_scroll_fixture_graph(&mut viewport, fixture)?;
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
