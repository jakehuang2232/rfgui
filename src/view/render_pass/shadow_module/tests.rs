use super::{ShadowShape, TemplateAxis, TemplatePlan};

#[test]
fn rounded_rect_uniform_matches_per_corner_api() {
    let uniform = ShadowShape::rounded_rect(10.0, 20.0, 120.0, 70.0, 14.0);
    let per_corner =
        ShadowShape::rounded_rect_with_radii(10.0, 20.0, 120.0, 70.0, [14.0, 14.0, 14.0, 14.0]);
    assert_eq!(uniform, per_corner);
    assert_eq!(uniform.fill_mesh(), per_corner.fill_mesh());
}

#[test]
fn rounded_rect_per_corner_uses_distinct_corner_radii() {
    let shape =
        ShadowShape::rounded_rect_with_radii(0.0, 0.0, 100.0, 60.0, [30.0, 10.0, 20.0, 5.0]);
    let (vertices, _) = shape.fill_mesh();
    assert!(vertices.len() > 4);
    let first_ring = vertices[1];
    let last_ring = vertices[vertices.len() - 1];
    assert!((first_ring[0] - 90.0).abs() < 0.001);
    assert!((first_ring[1] - 0.0).abs() < 0.001);
    assert!((last_ring[0] - 30.0).abs() < 0.001);
    assert!((last_ring[1] - 0.0).abs() < 0.001);
}

#[test]
fn oversized_radii_are_normalized_to_the_rect() {
    let shape = ShadowShape::rounded_rect(0.0, 0.0, 40.0, 20.0, 50.0);
    assert_eq!(shape.radii, [10.0; 4]);
    assert!(!ShadowShape::rounded_rect(0.0, 0.0, 0.0, 20.0, 2.0).is_valid());
}

/// Every destination layer pixel reads the template texel holding the same
/// distances to both shape edges, or a texel of the constant middle band.
#[test]
fn template_axis_maps_the_layer_onto_a_stretched_template() {
    let axis = TemplateAxis::new(100.25, 400.0, 12.0, 30, 30.0, 4).unwrap();
    assert_eq!(axis.origin, 70);
    assert_eq!(axis.shape_offset, 30.25);
    assert_eq!(axis.layer, 461);
    assert!(axis.stretch > 0);
    assert_eq!(
        axis.stretch % 4,
        0,
        "stretch keeps the blur downsample grid"
    );
    assert_eq!(axis.template % 4, 0);
    assert_eq!(axis.shape_len, 400.0 - axis.stretch as f32);
    assert!(axis.template < axis.layer);
    let template_right_edge = axis.shape_offset + axis.shape_len;
    let influence = 12.0 + 30.0;
    assert!(axis.split as f32 >= axis.shape_offset + influence);
    assert!((axis.split + 1) as f32 <= template_right_edge - influence);
    // The right-hand template region equals the layer shifted by `stretch`.
    let last_layer_pixel = axis.layer - 1;
    assert!(last_layer_pixel - axis.stretch < axis.template);
}

#[test]
fn small_shapes_use_the_whole_layer_as_template() {
    let axis = TemplateAxis::new(5.5, 20.0, 4.0, 6, 9.0, 1).unwrap();
    assert_eq!(axis.stretch, 0);
    assert!(axis.template >= axis.layer);
    assert_eq!(axis.split, axis.template, "no texel is repeated");
}

#[test]
fn template_key_ignores_whole_pixel_translation_and_stretched_length() {
    let plan = |x: f32, y: f32, width: f32| {
        TemplatePlan::new(ShadowShape::rounded_rect(x, y, width, 300.0, 16.0), 24.0).unwrap()
    };
    let base = plan(40.5, 60.0, 500.0);
    assert_eq!(
        base.key,
        plan(41.5, 75.0, 500.0).key,
        "integer moves reuse it"
    );
    assert_eq!(
        base.key,
        plan(40.5, 60.0, 700.0).key,
        "wider shapes reuse it"
    );
    assert_ne!(
        base.key,
        plan(40.75, 60.0, 500.0).key,
        "sub-pixel phase differs"
    );
    assert_ne!(
        base.key,
        TemplatePlan::new(
            ShadowShape::rounded_rect(40.5, 60.0, 500.0, 300.0, 16.0),
            20.0
        )
        .unwrap()
        .key
    );
}

fn spec(x: f32) -> super::ShadowModuleSpec {
    super::ShadowModuleSpec {
        shape: ShadowShape::rounded_rect(x, 30.0, 300.0, 200.0, 12.0),
        params: super::ShadowParams {
            blur_radius: 10.0,
            ..Default::default()
        },
        viewport_width: 1600,
        viewport_height: 1200,
        scale_factor: 2.0,
        pass_context: Default::default(),
        output: Default::default(),
    }
}

fn pass_names(graph: &crate::view::frame_graph::FrameGraph) -> Vec<&'static str> {
    graph
        .pass_descriptors()
        .iter()
        .map(|descriptor| descriptor.name.rsplit("::").next().unwrap())
        .collect()
}

#[test]
fn resident_template_is_read_without_being_produced() {
    use crate::view::frame_graph::FrameGraph;
    let mut cold = FrameGraph::new();
    assert!(super::build_shadow_module(&mut cold, spec(20.0)));
    let names = pass_names(&cold);
    assert_eq!(names.first(), Some(&"ShadowFillPass"));
    assert!(names.contains(&"BlurStagePass"));
    assert_eq!(names.last(), Some(&"TextureCompositePass"));
    let resident = cold.shadow_templates.declared.keys().copied().collect();

    let mut warm = FrameGraph::new();
    warm.set_resident_shadow_templates(resident);
    assert!(super::build_shadow_module(&mut warm, spec(23.0)));
    assert_eq!(pass_names(&warm), vec!["TextureCompositePass"]);
}

#[test]
fn shadows_sharing_a_template_declare_and_produce_it_once() {
    use crate::view::frame_graph::FrameGraph;
    let mut graph = FrameGraph::new();
    assert!(super::build_shadow_module(&mut graph, spec(20.0)));
    let produced = graph.pass_descriptors().len();
    assert!(super::build_shadow_module(&mut graph, spec(400.0)));
    assert_eq!(graph.shadow_templates.declared.len(), 1);
    assert_eq!(
        pass_names(&graph)[produced..],
        ["TextureCompositePass"],
        "the second shadow only draws"
    );
    assert_eq!(graph.declared_persistent_texture_keys().count(), 1);
}
