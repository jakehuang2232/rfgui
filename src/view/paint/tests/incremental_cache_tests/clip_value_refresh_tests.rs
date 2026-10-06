use super::*;
use crate::view::paint::compiler::resolved_clips_full_and_refreshed_for_test;
use crate::view::test_support::get_element_mut;

fn raster_context() -> ArtifactSurfaceRasterContext {
    ArtifactSurfaceRasterContext::new(
        1.,
        wgpu::TextureFormat::Rgba8Unorm,
        [0., 0.],
        None,
        8192,
        128 * 1024 * 1024,
    )
    .unwrap()
}

fn sized(id: u64, width: f32, height: f32, scroll: bool) -> Element {
    let mut element = Element::new_with_id(id, 0.0, 0.0, width, height);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(21, 43, 65)),
    );
    if scroll {
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
    }
    element.apply_style(style);
    element
}

fn window_position(left: f32) -> Style {
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(left))
                .top(Length::px(10.0))
                .clip(ClipMode::Parent),
        ),
    );
    style
}

/// A parent-clipped absolute "window" holding a scroll container, as in a
/// desktop demo. Returns the arena, the scene root and the window.
fn window_scene() -> (NodeArena, NodeKey, NodeKey) {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(sized(0xfeed_9a00, 320.0, 240.0, false)),
    );
    let mut window = sized(0xfeed_9a01, 120.0, 90.0, false);
    window.apply_style(window_position(20.0));
    let window = commit_child(&mut arena, root, Box::new(window));
    let scroll = commit_child(
        &mut arena,
        window,
        Box::new(sized(0xfeed_9a02, 40.0, 32.0, true)),
    );
    let row = commit_child(
        &mut arena,
        scroll,
        Box::new(sized(0xfeed_9a03, 20.0, 120.0, false)),
    );
    // A nested scroller taller than the outer viewport: its contents clip
    // resolves by intersecting both scissors.
    let nested = commit_child(
        &mut arena,
        row,
        Box::new(sized(0xfeed_9a04, 16.0, 48.0, true)),
    );
    commit_child(
        &mut arena,
        nested,
        Box::new(sized(0xfeed_9a05, 10.0, 80.0, false)),
    );
    (arena, root, window)
}

fn moved_artifact(
    viewport: &mut crate::view::viewport::Viewport,
    arena: &mut NodeArena,
    root: NodeKey,
    window: NodeKey,
    left: f32,
) -> PaintArtifact {
    get_element_mut::<Element>(arena, window).apply_style(window_position(left));
    crate::view::viewport::layout_artifact_style_scene_for_test(
        viewport,
        arena,
        root,
        [320.0, 240.0],
    );
    let (properties, generations) = sync_identity(arena, &[root]);
    artifact(
        record_surface_dag_frame_artifact(
            arena,
            &[root],
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap(),
    )
}

/// Dragging a window keeps every clip relation. Relation, structure and
/// coverage proofs replay; only the clip scissors (including those copied
/// into scroll-content coverage) are folded again, matching a cold plan.
#[test]
fn moved_window_clips_reuse_relations_and_structure_with_current_scissors() {
    let (mut arena, root, window) = window_scene();
    let mut viewport = crate::view::viewport::Viewport::new();
    let context = raster_context();
    let mut cache = PlanningCache::default();
    let mut previous_clips = None;
    let mut previous_resolved = None;
    let mut latest = None;
    for (frame, left) in [20.0, 27.0, 41.5].into_iter().enumerate() {
        let input = moved_artifact(&mut viewport, &mut arena, root, window, left);
        assert!(!input.scroll_nodes.is_empty(), "fixture must scroll");
        if let Some(previous) = previous_clips.replace(input.clip_nodes.clone()) {
            assert_ne!(previous, input.clip_nodes, "fixture must move its clips");
        }
        // Relation replay folds current scissors exactly as full validation.
        let (full, refreshed) = resolved_clips_full_and_refreshed_for_test(&input);
        assert!(full.is_some(), "frame {frame}");
        assert_eq!(refreshed, full, "frame {frame}");
        if let Some(previous) = previous_resolved.replace(full.clone()) {
            assert_ne!(previous, full, "fixture must move resolved scissors");
        }
        let cached =
            prepare_artifact_surface_raster_plan_cached(input.clone(), context, &mut cache)
                .unwrap();
        let fresh = prepare_artifact_surface_raster_plan(input.clone(), context).unwrap();
        assert_eq!(format!("{cached:?}"), format!("{fresh:?}"), "frame {frame}");
        assert_eq!(
            cache.relation_hits,
            usize::from(frame != 0),
            "frame {frame}"
        );
        assert_eq!(
            cache.surface_structure_hits,
            usize::from(frame != 0),
            "frame {frame}"
        );
        latest = Some(input);
    }

    // A retired clip generation is rejected exactly as a cold plan rejects it.
    let mut retired = latest.unwrap();
    retired.clip_nodes[0].generation = 0;
    let fresh = prepare_artifact_surface_raster_plan(retired.clone(), context).unwrap_err();
    let cached =
        prepare_artifact_surface_raster_plan_cached(retired, context, &mut cache).unwrap_err();
    assert_eq!(format!("{cached:?}"), format!("{fresh:?}"));
    assert_eq!(cache.relation_hits, 0);
}

#[test]
fn changed_clip_relations_do_not_replay() {
    let (mut arena, root, window) = window_scene();
    let mut viewport = crate::view::viewport::Viewport::new();
    let context = raster_context();
    let mut cache = PlanningCache::default();
    let input = moved_artifact(&mut viewport, &mut arena, root, window, 20.0);
    prepare_artifact_surface_raster_plan_cached(input.clone(), context, &mut cache).unwrap();
    let mut changed = input;
    let clip = &mut changed.clip_nodes[0];
    clip.behavior = match clip.behavior {
        ClipBehavior::Replace => ClipBehavior::Intersect,
        ClipBehavior::Intersect => ClipBehavior::Replace,
    };
    let fresh = prepare_artifact_surface_raster_plan(changed.clone(), context);
    let cached = prepare_artifact_surface_raster_plan_cached(changed, context, &mut cache);
    assert_eq!(format!("{cached:?}"), format!("{fresh:?}"));
    assert_eq!(cache.relation_hits, 0);
    assert_eq!(cache.surface_structure_hits, 0);
}
