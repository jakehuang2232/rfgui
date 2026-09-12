use super::*;

#[test]
fn compiler_clears_first_transient_color_load_attachment() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_texture::<()>(test_texture_desc());
    let writer = graph.add_graphics_pass(ModifyPass {
        target: texture.clone(),
    });
    let present = graph.add_graphics_pass(make_present_pass(&texture));
    graph
        .add_pass_sink(present, ExternalSinkKind::SurfacePresent)
        .expect("sink registration should succeed");

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("graph should be compiled");
    let writer_pass = compiled
        .passes
        .iter()
        .find(|pass| pass.original_index == writer.0)
        .expect("writer pass should be live");
    let PassDetails::Graphics(graphics) = &writer_pass.descriptor.details else {
        panic!("expected graphics pass details");
    };
    assert_eq!(graphics.color_attachments.len(), 1);
    assert_eq!(
        graphics.color_attachments[0].load_op,
        AttachmentLoadOp::Clear
    );
    assert_eq!(
        graphics.color_attachments[0].clear_color,
        Some([0.0, 0.0, 0.0, 0.0])
    );
}

#[test]
fn compiler_keeps_first_persistent_color_load_attachment() {
    let mut graph = FrameGraph::new();
    let target = graph.declare_texture_internal::<()>(
        test_texture_desc(),
        ResourceLifetime::Persistent,
        Some(0xBEEF),
    );
    let writer = graph.add_graphics_pass(ModifyPass {
        target: target.clone(),
    });
    let present = graph.add_graphics_pass(make_present_pass(&target));
    graph
        .add_pass_sink(present, ExternalSinkKind::SurfacePresent)
        .expect("sink registration should succeed");

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("graph should be compiled");
    let writer_pass = compiled
        .passes
        .iter()
        .find(|pass| pass.original_index == writer.0)
        .expect("writer pass should be live");
    let PassDetails::Graphics(graphics) = &writer_pass.descriptor.details else {
        panic!("expected graphics pass details");
    };
    assert_eq!(graphics.color_attachments.len(), 1);
    assert_eq!(
        graphics.color_attachments[0].load_op,
        AttachmentLoadOp::Load
    );
    assert_eq!(graphics.color_attachments[0].clear_color, None);
}

#[test]
fn persistent_read_without_current_frame_producer_compiles() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_persistent_texture_internal::<()>(
        test_texture_desc(),
        PersistentTextureKey::retained(RetainedTextureRole::RootEffectColor, 17),
    );
    graph.add_graphics_pass(ReadPass {
        input: InSlot::with_handle(texture.handle().expect("persistent texture handle")),
    });

    graph
        .compile()
        .expect("resident persistent texture is a valid external input");
}

#[test]
fn root_effect_color_maps_to_distinct_depth_stencil_role() {
    let color =
        PersistentTextureKey::retained(RetainedTextureRole::RootEffectColor, 0xABCD_EF01_2345_6789);
    assert_eq!(
        color.depth_stencil(),
        Some(PersistentTextureKey::retained(
            RetainedTextureRole::RootEffectDepthStencil,
            0xABCD_EF01_2345_6789,
        ))
    );
    assert_eq!(color.depth_stencil().unwrap().depth_stencil(), None);
}

#[test]
fn declared_persistent_keys_include_resources_culled_from_compiled_graph() {
    let mut graph = FrameGraph::new();
    let persistent_key = 0xA11CE;
    let _unused_persistent = graph.declare_texture_internal::<()>(
        test_texture_desc(),
        ResourceLifetime::Persistent,
        Some(persistent_key),
    );
    let live = graph.declare_texture::<()>(test_texture_desc());
    graph.add_graphics_pass(WritePass {
        output: live.clone(),
    });
    let present = graph.add_graphics_pass(make_present_pass(&live));
    graph
        .add_pass_sink(present, ExternalSinkKind::SurfacePresent)
        .expect("sink registration should succeed");

    graph.compile().expect("compile should succeed");

    assert!(
        graph
            .declared_persistent_texture_keys()
            .any(|key| key == PersistentTextureKey::Generic(persistent_key)),
        "declaration liveness must not depend on executable pass usage"
    );
    let declared = graph
        .declared_persistent_textures()
        .find(|(key, _)| *key == PersistentTextureKey::Generic(persistent_key))
        .expect("culled persistent declaration should retain its descriptor");
    assert_eq!(declared.1, &test_texture_desc());
    assert!(
        !graph
            .compiled_graph()
            .expect("compiled graph")
            .texture_stable_keys
            .values()
            .any(|&key| key == PersistentTextureKey::Generic(persistent_key)),
        "fixture must prove the persistent resource was culled from the compiled graph"
    );
}

#[test]
fn duplicate_persistent_texture_key_fails_compile_before_resource_acquire() {
    let mut graph = FrameGraph::new();
    let desc = test_texture_desc();
    let _first = graph.declare_texture_internal::<()>(
        desc.clone(),
        ResourceLifetime::Persistent,
        Some(0xA11CE),
    );
    let _second =
        graph.declare_texture_internal::<()>(desc, ResourceLifetime::Persistent, Some(0xA11CE));

    let error = graph
        .compile()
        .expect_err("duplicate stable key must fail closed");
    assert!(matches!(
        error,
        FrameGraphError::Validation(message)
            if message.contains("duplicate persistent texture key")
    ));
}
