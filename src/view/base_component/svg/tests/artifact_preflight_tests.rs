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
            &crate::view::paint::PaintRecordingContext::default(),
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
