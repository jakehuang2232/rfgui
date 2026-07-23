use super::*;

#[test]
fn strict_structural_parity_covers_opaque_alpha_and_uniform_border() {
    for (opacity, expected_opaque) in [(1.0, true), (0.5, false)] {
        let snapshots = assert_whole_frame_structural_parity(
            || {
                let (arena, root, _, _) =
                    prepared_leaf(105, Color::rgb(220, 30, 40), opacity, true);
                (arena, vec![root])
            },
            PaintParityConfig::default(),
        );
        assert_eq!(snapshots.len(), 2, "fill and border must both be captured");
        assert_eq!(snapshots[0].opaque, expected_opaque);
    }
}

#[test]
fn strict_structural_parity_covers_asymmetric_border_radius_and_colors() {
    let snapshots = assert_whole_frame_structural_parity(
        prepared_asymmetric_border_tree,
        PaintParityConfig::default(),
    );
    assert_eq!(snapshots.len(), 2);
    assert!(snapshots[1].use_border_side_colors);
    assert_eq!(
        snapshots[1].border_width_bits,
        [5.0_f32, 3.0, 2.0, 4.0].map(f32::to_bits)
    );
    assert_eq!(
        snapshots[1].border_radius_bits,
        [[2.0_f32, 2.0], [6.0, 6.0], [10.0, 10.0], [14.0, 14.0]]
            .map(|radius| radius.map(f32::to_bits))
    );
}

#[test]
fn strict_structural_parity_covers_background_and_border_gradients() {
    let snapshots = assert_whole_frame_structural_parity(
        prepared_gradient_tree,
        PaintParityConfig::default(),
    );
    assert_eq!(snapshots.len(), 2);
    assert!(snapshots[0].gradient.is_some());
    assert!(snapshots[1].border_gradient.is_some());
}

#[test]
fn strict_structural_parity_covers_nested_multi_root_order() {
    let snapshots = assert_whole_frame_structural_parity(
        || {
            let (arena, roots, _) = prepared_plain_tree();
            (arena, roots)
        },
        PaintParityConfig::default(),
    );
    assert_eq!(snapshots.len(), 3);
    assert!(
        f32::from_bits(snapshots[0].fill_color_bits[0])
            > f32::from_bits(snapshots[0].fill_color_bits[1])
    );
    assert!(
        f32::from_bits(snapshots[1].fill_color_bits[1])
            > f32::from_bits(snapshots[1].fill_color_bits[0])
    );
    assert!(
        f32::from_bits(snapshots[2].fill_color_bits[2])
            > f32::from_bits(snapshots[2].fill_color_bits[0])
    );
}

#[test]
fn strict_structural_parity_covers_target_size_format_and_scale() {
    for config in [
        PaintParityConfig::default(),
        PaintParityConfig {
            width: 640,
            height: 480,
            format: wgpu::TextureFormat::Rgba8Unorm,
            scale_factor: 2.0,
            initial_scissor: None,
        },
    ] {
        let snapshots = assert_whole_frame_structural_parity(
            || {
                let (arena, root, _, _) =
                    prepared_leaf(106, Color::rgb(20, 40, 60), 1.0, false);
                (arena, vec![root])
            },
            config,
        );
        assert_eq!(snapshots.len(), 1);
        assert!(snapshots[0].opaque);
    }
}

#[test]
fn strict_snapshot_is_sensitive_to_scale_factor_alone() {
    let (arena, root, properties, generations) =
        prepared_leaf(108, Color::rgb(20, 40, 60), 1.0, false);
    let (artifact, _) = whole_frame_artifact(&arena, &[root], &properties, &generations);
    let base = PaintParityConfig::default();
    let scaled = PaintParityConfig {
        scale_factor: 2.0,
        ..base
    };
    let mut base_graph = compiled_whole_frame_graph_with_config(&artifact, base);
    let mut scaled_graph = compiled_whole_frame_graph_with_config(&artifact, scaled);
    let base_snapshot = strict_paint_snapshot(&mut base_graph, base);
    let scaled_snapshot = strict_paint_snapshot(&mut scaled_graph, scaled);

    assert_eq!(
        base_snapshot.graph, scaled_snapshot.graph,
        "scale is a viewport raster input, not FrameGraph topology"
    );
    assert_ne!(base_snapshot.viewport, scaled_snapshot.viewport);
    assert_ne!(base_snapshot, scaled_snapshot);
}

#[test]
fn strict_structural_parity_tracks_opacity_classification_transition() {
    let before = assert_whole_frame_structural_parity(
        || {
            let (arena, root, _, _) = prepared_leaf(107, Color::rgb(50, 70, 90), 1.0, false);
            (arena, vec![root])
        },
        PaintParityConfig::default(),
    );
    let after = assert_whole_frame_structural_parity(
        || {
            let (arena, root, _, _) = prepared_leaf(107, Color::rgb(50, 70, 90), 0.5, false);
            (arena, vec![root])
        },
        PaintParityConfig::default(),
    );
    assert!(before[0].opaque);
    assert!(!after[0].opaque);
}

#[test]
fn strict_structural_parity_covers_zero_opacity_without_partial_output() {
    let snapshots = assert_whole_frame_structural_parity(
        prepared_zero_opacity_tree,
        PaintParityConfig::default(),
    );
    assert_eq!(snapshots.len(), 1);
    assert!(f32::from_bits(snapshots[0].fill_color_bits[2]) > 0.9);
}
