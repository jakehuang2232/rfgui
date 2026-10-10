use super::*;

use crate::view::viewport::{
    AutoArtifactSurfaceEmissionForTest, emit_retained_auto_artifact_surface_for_test,
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
            offset_y: 0.0,
            content_height: 300.0,

            transition_local_y: 33.0,
        },
        GpuScrollbarCase::Hidden,
    )
}

pub(super) fn offset_single_scroll_content_fixture()
-> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    scroll_scene_gpu_fixture(
        ScrollSceneGpuCase {
            offset_y: 13.0,
            content_height: 300.0,

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
        nested_scroll_unready_text_fixture_for_test(NestedTextFallbackKind::InlineIfcOwned);
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

fn verify_artifact_scroll_fixture(
    gpu: &NativeGpu,
    adapter: &str,
    case: &str,
    fixture: ArtifactScrollFixture,
    expected: &dyn Fn(&[u8], &str) -> Result<(), String>,
) -> Result<(usize, u64), String> {
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

    expected(&cold_pixels, &format!("{case}/cold"))?;
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
// cargo test -q native_production_artifact_nested_scroll_multi_leaf_matches_geometry_and_reuses_real_pool -- --ignored --nocapture
fn native_production_artifact_nested_scroll_multi_leaf_matches_geometry_and_reuses_real_pool()
-> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    // The host placement [3.5, -2.25] snaps to [4, -2], so the outer
    // scrollport [14, 18] + [100, 80] meets the 67x64 target in
    // [14, 67) x [18, 64), painted in the hosts' background. The extra green
    // leaf moves to [66, 10] + [28, 24]: inside the target only its first
    // column shows, clipped to the scrollport's top, all of it in the edge's
    // coverage band. The pixel either side of every edge is skipped.
    let background = rgba8_unorm(Color::rgb(24, 48, 72));
    let expected = |pixels: &[u8], case: &str| {
        let near = |value: u32, edge: u32| value + 1 >= edge && value <= edge;
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                if near(x, 14) || near(y, 18) || near(x, 66) && y <= 34 || near(y, 34) && x >= 65 {
                    continue;
                }
                let want = if x < 14 || y < 18 {
                    [0, 0, 0, 0]
                } else {
                    background
                };
                let actual = pixel_at(pixels, x, y)?;
                if actual != want {
                    return Err(format!(
                        "{case}: ({x},{y}) is {actual:?}, expected {want:?} on {adapter}"
                    ));
                }
            }
        }
        let [r, g, b, _] = pixel_at(pixels, 66, 25)?;
        if g <= r.max(b) {
            return Err(format!(
                "{case}: the green leaf's visible column (66,25) is not green on {adapter}: {:?}",
                pixel_at(pixels, 66, 25)?
            ));
        }
        Ok(())
    };
    let (surface_count, bytes) = verify_artifact_scroll_fixture(
        gpu,
        &adapter,
        "production-artifact-nested-scroll-multi-leaf",
        nested_multi_leaf_fixture,
        &expected,
    )?;
    eprintln!(
        "production Artifact nested multi-leaf scroll geometry/reuse passed on {adapter}: surfaces={surface_count}, aggregate_color_depth_bytes={bytes}"
    );
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_production_artifact_nested_scroll_inline_ifc_text_paints_inside_its_scrollport_and_reuses_real_pool -- --ignored --nocapture
fn native_production_artifact_nested_scroll_inline_ifc_text_paints_inside_its_scrollport_and_reuses_real_pool()
-> Result<(), String> {
    let _thread_cache_cleanup = NativeArtifactTextThreadCacheCleanup;
    crate::view::render_pass::text_pass::clear_text_resources_cache();
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    // Glyph coverage has no geometric oracle. The snapped outer scrollport
    // [14, 18] + [100, 80] must clip everything; inside it each pixel is the
    // hosts' background, or that background blended toward the glyph color by
    // one coverage value, which the green and blue channels must agree on.
    // The fixture builds its IFC-owned geometry from the standalone Text's
    // shaped context, which Text shapes with a constant black brush (the
    // standalone bridge applies its color), so these glyphs paint black.
    let background = rgba8_unorm(Color::rgb(24, 48, 72));
    let glyph = [0, 0, 0, 255];
    let expected = |pixels: &[u8], case: &str| {
        let near = |value: u32, edge: u32| value + 1 >= edge && value <= edge;
        let mut inked = 0;
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                if near(x, 14) || near(y, 18) {
                    continue;
                }
                let actual = pixel_at(pixels, x, y)?;
                if x < 14 || y < 18 {
                    if actual != [0, 0, 0, 0] {
                        return Err(format!(
                            "{case}: ({x},{y}) outside the scrollport is {actual:?} on {adapter}"
                        ));
                    }
                    continue;
                }
                if actual == background {
                    continue;
                }
                let coverage = |channel: usize| {
                    (f32::from(actual[channel]) - f32::from(background[channel]))
                        / (f32::from(glyph[channel]) - f32::from(background[channel]))
                };
                let (green, blue) = (coverage(1), coverage(2));
                let red_bounded = actual[0] + 1 >= background[0].min(glyph[0])
                    && actual[0] <= background[0].max(glyph[0]) + 1;
                if actual[3] != 255
                    || !red_bounded
                    || !(-0.15..=1.15).contains(&green)
                    || !(-0.15..=1.15).contains(&blue)
                    // One code of the background's green 8 is 0.125 coverage.
                    || (green - blue).abs() > 0.2
                {
                    return Err(format!(
                        "{case}: ({x},{y}) is {actual:?}, not a blend of {background:?} toward {glyph:?} on {adapter}"
                    ));
                }
                inked += 1;
            }
        }
        if inked < 32 {
            return Err(format!("{case}: only {inked} glyph pixels on {adapter}"));
        }
        Ok(())
    };
    let (surface_count, bytes) = verify_artifact_scroll_fixture(
        gpu,
        &adapter,
        "production-artifact-nested-scroll-inline-ifc-text",
        nested_inline_ifc_text_fixture,
        &expected,
    )?;
    eprintln!(
        "production Artifact nested IFC-owned Text clip/coverage/reuse passed on {adapter}: surfaces={surface_count}, aggregate_color_depth_bytes={bytes}"
    );
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_production_artifact_scroll_offset_only_reuses_real_pool_and_scrolls_rigidly -- --ignored --nocapture
fn native_production_artifact_scroll_offset_only_reuses_real_pool_and_scrolls_rigidly()
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
    if warm.intermediate_surfaces.len() != 1 {
        return Err(format!(
            "offset-only gate requires exactly one Artifact intermediate surface on {adapter}: observations={:?}",
            warm.intermediate_surfaces
        ));
    }
    if !viewport.finish_retained_surface_transaction_for_frame(Some(warm.frame_owner), true) {
        return Err("offset-only warm Artifact transaction did not commit".to_owned());
    }
    if cold_pixels == warm_pixels {
        return Err(format!(
            "offset-only gate rendered identical cold and warm frames on {adapter}; the fixture did not exercise visible scrolling"
        ));
    }

    // The reused content moves rigidly with the scroll offset: inside the
    // scrollport every warm row equals the cold row MOVED_OFFSET_Y lower,
    // except the rows the scroll newly exposes, which the cold scrollport
    // clipped. Everything outside the scrollport is unchanged. The pixel
    // either side of a scrollport edge mixes edge coverage with whatever
    // content lies there, so it is skipped.
    let scrollport = arena.get(root).unwrap().element.box_model_snapshot();
    let [left, top, right, bottom] = [
        scrollport.x,
        scrollport.y,
        scrollport.x + scrollport.width,
        scrollport.y + scrollport.height,
    ]
    .map(|edge| edge as u32);
    let delta = MOVED_OFFSET_Y as u32;
    let near = |value: u32, edge: u32| value + 1 >= edge && value <= edge;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if near(x, left) || near(x, right) || near(y, top) || near(y, bottom) {
                continue;
            }
            let inside = x >= left && x < right && y >= top && y < bottom;
            // Newly exposed rows, and rows whose cold source lies in the
            // bottom edge's band.
            if inside && y + delta + 1 >= bottom {
                continue;
            }
            let cold_y = if inside { y + delta } else { y };
            let warm_pixel = pixel_at(&warm_pixels, x, y)?;
            let cold_pixel = pixel_at(&cold_pixels, x, cold_y)?;
            if warm_pixel != cold_pixel {
                return Err(format!(
                    "offset-only warm ({x},{y})={warm_pixel:?} is not cold ({x},{cold_y})={cold_pixel:?} on {adapter}"
                ));
            }
        }
    }
    eprintln!(
        "production Artifact scroll-offset-only reuse passed on {adapter}: offset={MOVED_OFFSET_Y}, surfaces={}, aggregate_color_depth_bytes={}",
        warm.surface_count, warm.aggregate_texture_bytes
    );
    Ok(())
}
