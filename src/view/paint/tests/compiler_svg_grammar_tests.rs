use super::*;

#[test]
fn compiler_emits_typed_straight_srgb_svg_as_texture_composite() {
    let artifact = compiler_svg_test_artifact(false);
    assert_eq!(artifact.chunks[0].id.role, PaintChunkRole::SvgContent);
    assert_eq!(
        artifact.chunks[0].properties.legacy_boundary_dimensions(),
        PropertyTreeState::default(),
    );
    assert!(artifact.chunks[0].properties.layout_position.is_some());
    assert!(artifact.chunks[0].properties.visual_offset.is_some());

    let mut graph = compiled_whole_frame_graph(&artifact);
    let passes = graph.test_graphics_passes_mut::<crate::view::render_pass::TextureCompositePass>();
    assert_eq!(passes.len(), 1);
    let snapshot = passes[0].test_snapshot();
    let upload = snapshot
        .sampled_source
        .expect("SVG artifact must retain an owning sampled upload");
    assert!(matches!(
        upload.id,
        crate::view::sampled_texture::SampledTextureId::SvgRaster(_)
    ));
    assert_eq!(upload.format, wgpu::TextureFormat::Rgba8UnormSrgb);
    assert_eq!(
        upload.alpha_mode,
        crate::view::sampled_texture::SampledTextureAlphaMode::Straight
    );
    assert!(!snapshot.source_is_premultiplied);
    assert!(!snapshot.use_mask);
    assert!(snapshot.quad_position_bits.is_none());
    assert!(snapshot.mask_uv_bounds_bits.is_none());
    assert!(snapshot.explicit_scissor_rect.is_none());
    assert!(snapshot.effective_scissor_rect.is_none());
    assert!(snapshot.uv_bounds_bits.is_some());

    // The strict whole-graph adapter already knows TextureComposite; this
    // must remain a complete structural snapshot rather than an unchecked
    // pass hidden behind the common Image/SVG compiler implementation.
    let _ = strict_paint_snapshot(&mut graph, PaintParityConfig::default());
}
