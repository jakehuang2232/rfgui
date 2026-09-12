use super::ShadowMesh;

#[test]
fn rounded_rect_uniform_matches_per_corner_api() {
    let uniform = ShadowMesh::rounded_rect(10.0, 20.0, 120.0, 70.0, 14.0);
    let per_corner =
        ShadowMesh::rounded_rect_with_radii(10.0, 20.0, 120.0, 70.0, [14.0, 14.0, 14.0, 14.0]);
    assert_eq!(uniform.vertices, per_corner.vertices);
    assert_eq!(uniform.indices, per_corner.indices);
}

#[test]
fn rounded_rect_per_corner_uses_distinct_corner_radii() {
    let mesh = ShadowMesh::rounded_rect_with_radii(0.0, 0.0, 100.0, 60.0, [30.0, 10.0, 20.0, 5.0]);
    assert!(mesh.vertices.len() > 4);
    let first_ring = mesh.vertices[1];
    let last_ring = mesh.vertices[mesh.vertices.len() - 1];
    assert!((first_ring[0] - 90.0).abs() < 0.001);
    assert!((first_ring[1] - 0.0).abs() < 0.001);
    assert!((last_ring[0] - 30.0).abs() < 0.001);
    assert!((last_ring[1] - 0.0).abs() < 0.001);
}
