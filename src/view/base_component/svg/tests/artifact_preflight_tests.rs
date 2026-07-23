use super::*;

#[test]
fn actual_svg_artifact_compiles_after_arena_drop_and_forced_registry_removal() {
    let mut svg = Svg::new_with_id(67, unique_svg("owning-artifact-drop"));
    wait_until_document_ready(svg.source_key);
    layout_svg_element(&mut svg, 80.0, 40.0);
    let mut sync_arena = new_test_arena();
    svg.sync_arena(&mut sync_arena);
    svg.prepare_frozen_paint(PaintResourcePreparationContext {
        frame_number: 1,
        device_scale: 1.0,
        now: Instant::now(),
    });
    let raster_key = svg
        .active_raster_key
        .expect("first prepare requests raster");
    let request = svg.active_raster_request.expect("request identity frozen");
    set_svg_raster_ready_for_test(raster_key, request.physical_width, request.physical_height);
    svg.sync_arena(&mut sync_arena);
    svg.prepare_frozen_paint(PaintResourcePreparationContext {
        frame_number: 2,
        device_scale: 1.0,
        now: Instant::now(),
    });
    assert!(svg.frozen_request_is_exact);
    let document_key = svg.source_key;

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(svg));
    let node = arena.get(root).unwrap();
    let artifact = node
        .element
        .record_shadow_paint_artifact(
            root,
            crate::view::compositor::property_tree::PropertyTreeState::default(),
            crate::view::paint::PaintContentRevision {
                self_paint_revision: 2,
                composite_revision: 2,
                topology_revision: 2,
            },
            &arena,
            crate::view::paint::PaintRecordingContext::default(),
        )
        .expect("actual frozen SVG hook should record");
    drop(node);
    drop(arena);
    remove_svg_raster_entry_for_test(raster_key);
    remove_svg_document_entry_for_test(document_key);

    let mut graph = crate::view::frame_graph::FrameGraph::new();
    let mut ctx = crate::view::base_component::UiBuildContext::new(
        80,
        40,
        wgpu::TextureFormat::Bgra8Unorm,
        1.0,
    );
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    let _ = crate::view::paint::compile_artifact(&artifact, &mut graph, ctx);
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len(),
        1
    );
}

#[test]
fn visible_child_and_nonexact_svg_fail_preflight_without_full_artifact_hook() {
    fn assert_missing_prepared_svg(
        arena: &crate::view::node_arena::NodeArena,
        root: crate::view::node_arena::NodeKey,
    ) {
        let mut properties = crate::view::compositor::PropertyTrees::default();
        properties.sync(arena, &[root]);
        let mut generations = crate::view::compositor::PaintGenerationTracker::default();
        generations.sync(arena, &[root], &properties);
        let preflight = crate::view::paint::record_coverage_manifest(
            arena,
            &[root],
            false,
            true,
            crate::view::paint::CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        assert!(
            matches!(
                preflight.items.as_slice(),
                [crate::view::paint::PaintCoverageItem::LegacyBoundary {
                    reason: crate::view::paint::LegacyPaintReason::MissingPreparedSvg
                        | crate::view::paint::LegacyPaintReason::MissingPreparedInlineRoot,
                    ..
                }]
            ),
            "unexpected SVG preflight: {:#?}",
            preflight.items
        );
        let _ = crate::view::paint::take_full_artifact_record_count();
        let full = crate::view::paint::record_coverage_manifest(
            arena,
            &[root],
            false,
            true,
            crate::view::paint::CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
        );
        assert!(matches!(
            full.items.as_slice(),
            [crate::view::paint::PaintCoverageItem::LegacyBoundary {
                reason: crate::view::paint::LegacyPaintReason::MissingPreparedSvg
                    | crate::view::paint::LegacyPaintReason::MissingPreparedInlineRoot,
                ..
            }]
        ));
        assert_eq!(crate::view::paint::take_full_artifact_record_count(), 0);
    }

    let mut child_arena = new_test_arena();
    let child_root = commit_element(
        &mut child_arena,
        Box::new(freeze_ready_svg(69, simple_svg(), 1.0)),
    );
    let _ = commit_child(
        &mut child_arena,
        child_root,
        Box::new(crate::view::base_component::Element::new_with_id(
            690, 0.0, 0.0, 4.0, 4.0,
        )),
    );
    assert_missing_prepared_svg(&child_arena, child_root);

    let mut nonexact_arena = new_test_arena();
    let mut nonexact = freeze_ready_svg(70, simple_svg(), 1.0);
    nonexact.frozen_request_is_exact = false;
    nonexact.pending_raster_request =
        Some(SvgRasterRequest::new(160, 80, SvgRasterMode::Uniform));
    let nonexact_root = commit_element(&mut nonexact_arena, Box::new(nonexact));
    assert_missing_prepared_svg(&nonexact_arena, nonexact_root);
}
