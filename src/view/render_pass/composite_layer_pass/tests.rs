use super::*;

#[test]
fn composite_rounded_points_zero_corner_keeps_fixed_topology() {
    let segments = 16;
    let pts = rounded_rect_points(0.0, 0.0, 100.0, 100.0, [0.0, 10.0, 10.0, 10.0], segments);
    assert_eq!(pts.len(), (segments * 4) as usize);
}

#[test]
fn composite_tessellate_asymmetric_radius_produces_geometry() {
    let (vertices, indices) = tessellate_composite_layer(
        [0.0, 0.0],
        [150.0, 150.0],
        [10.0, 32.0, 10.0, 135.0],
        1.0,
        800.0,
        600.0,
        800.0,
        600.0,
        [0.0, 0.0],
    );
    assert!(!vertices.is_empty());
    assert!(!indices.is_empty());
    assert_eq!(indices.len() % 3, 0);
}

#[test]
fn composite_append_ring_tolerates_mismatched_topology() {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let outer = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
    let inner = vec![[10.0, 10.0], [90.0, 10.0], [90.0, 90.0]];
    append_ring(
        &mut vertices,
        &mut indices,
        &outer,
        &inner,
        1.0,
        1.0,
        800.0,
        600.0,
        800.0,
        600.0,
        [0.0, 0.0],
    );
    assert!(!vertices.is_empty());
    assert!(!indices.is_empty());
}

#[test]
fn rectangular_layers_never_emit_coverage_outside_the_source_extent() {
    for position in [[16.0, 8.0], [16.125, 8.875]] {
        let (vertices, indices) = tessellate_composite_layer(
            position,
            [32.0, 16.0],
            [0.0; 4],
            0.625,
            128.0,
            128.0,
            128.0,
            128.0,
            [0.0; 2],
        );
        assert!(!indices.is_empty());
        for vertex in vertices {
            let x = (vertex.position[0] + 1.0) * 64.0;
            let y = (1.0 - vertex.position[1]) * 64.0;
            assert!(
                x >= position[0] && x <= position[0] + 32.0,
                "outside horizontal extent: {x}"
            );
            assert!(
                y >= position[1] && y <= position[1] + 16.0,
                "outside vertical extent: {y}"
            );
            assert_eq!(vertex.alpha, 0.625);
        }
    }
}
