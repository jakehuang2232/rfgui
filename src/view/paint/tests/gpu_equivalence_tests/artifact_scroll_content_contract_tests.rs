use super::*;

#[test]
fn production_artifact_scroll_content_composite_uses_the_scrollport_clip() {
    let mut viewport = Viewport::new();
    let (artifact, _) =
        super::native_artifact_scroll_content_tests::production_artifact_scroll_fixture_graph(
            &mut viewport,
            super::native_artifact_scroll_content_tests::zero_offset_single_scroll_content_fixture,
        )
        .expect("Artifact offset-zero scroll graph");
    let layers = artifact
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>(
        );
    let [layer] = layers.as_slice() else {
        panic!("one ScrollContent surface must emit one layer composite")
    };
    assert_eq!(
        layer.test_snapshot().effective_scissor_rect,
        Some([8, 8, 48, 40]),
        "the detached ScrollContent surface must retain its consumed contents clip as the final scrollport scissor"
    );
}

#[test]
fn single_scroll_content_sampling_origin_preserves_the_visible_source_offset() {
    let mut viewport = Viewport::new();
    let (artifact, _) =
        super::native_artifact_scroll_content_tests::production_artifact_scroll_fixture_graph(
            &mut viewport,
            super::native_artifact_scroll_content_tests::offset_single_scroll_content_fixture,
        )
        .expect("Artifact offset-thirteen scroll graph");
    let layers = artifact
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>(
        );
    let [layer] = layers.as_slice() else {
        panic!("one ScrollContent surface must emit one layer composite")
    };
    let destination_y = layer.test_params().rect_pos[1];
    let source_physical_origin_y = layer
        .test_source_physical_origin_bits()
        .expect("Artifact ScrollContent owns a sealed sampling origin")[1];
    let source_physical_origin_y = f32::from_bits(source_physical_origin_y);
    assert_eq!(destination_y.to_bits(), (-5.0_f32).to_bits());
    assert_eq!(source_physical_origin_y.to_bits(), (-5.0_f32).to_bits());
    assert_eq!(
        (8.0 - source_physical_origin_y).to_bits(),
        13.0_f32.to_bits(),
        "the scrollport's visible top must sample source y=13"
    );
}

#[test]
fn nested_scroll_content_projects_child_destination_and_sampling_origin_in_parent_space() {
    let mut viewport = Viewport::new();
    let (artifact, _) =
        super::native_artifact_scroll_content_tests::production_artifact_scroll_fixture_graph(
            &mut viewport,
            super::native_artifact_scroll_content_tests::nested_multi_leaf_fixture,
        )
        .expect("Artifact nested multi-leaf scroll graph");
    let layers = artifact
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>(
        );
    assert_eq!(layers.len(), 2, "nested ScrollContent owns two composites");

    let mut placement = layers
        .iter()
        .map(|layer| {
            (
                layer.test_params().rect_pos,
                layer
                    .test_source_physical_origin_bits()
                    .expect("Artifact ScrollContent owns a sealed sampling origin")
                    .map(f32::from_bits),
            )
        })
        .collect::<Vec<_>>();
    placement.sort_by(|left, right| {
        left.0[1]
            .total_cmp(&right.0[1])
            .then_with(|| left.0[0].total_cmp(&right.0[0]))
    });
    assert_eq!(
        placement,
        vec![([10.0, -2.0], [10.0, -2.0]), ([0.0, 0.0], [0.0, 0.0])],
        "the parent base translation must project the nested destination before its raw-bounds union, and each ScrollContent sampler must use the finalized destination rather than the pre-normalization raster origin"
    );
}
