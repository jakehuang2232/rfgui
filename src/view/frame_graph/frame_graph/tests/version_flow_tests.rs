use super::*;

#[test]
fn compile_orders_write_then_read_from_usage() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_texture::<()>(test_texture_desc());
    let writer = graph.add_graphics_pass(WritePass {
        output: texture.clone(),
    });
    let reader = graph.add_graphics_pass(ReadPass {
        input: InSlot::with_handle(
            texture
                .handle()
                .expect("declared texture should have handle"),
        ),
    });

    graph.compile().expect("compile should succeed");

    assert_eq!(graph.order, vec![writer.0, reader.0]);
}

#[test]
fn compile_orders_modify_chain_from_usage() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_texture::<()>(test_texture_desc());
    let writer = graph.add_graphics_pass(WritePass {
        output: texture.clone(),
    });
    let modify_a = graph.add_graphics_pass(ModifyPass {
        target: texture.clone(),
    });
    let modify_b = graph.add_graphics_pass(ModifyPass { target: texture });

    graph.compile().expect("compile should succeed");

    assert_eq!(graph.order, vec![writer.0, modify_a.0, modify_b.0]);
}

#[test]
fn compile_populates_version_metadata_for_write_then_read() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_texture::<()>(test_texture_desc());
    let writer = graph.add_graphics_pass(WritePass {
        output: texture.clone(),
    });
    let reader = graph.add_graphics_pass(ReadPass {
        input: InSlot::with_handle(
            texture
                .handle()
                .expect("declared texture should have handle"),
        ),
    });

    graph.compile().expect("compile should succeed");

    let writer_usage = graph.passes[writer.0]
        .usages
        .first()
        .copied()
        .expect("writer should have one usage");
    let reader_usage = graph.passes[reader.0]
        .usages
        .first()
        .copied()
        .expect("reader should have one usage");

    assert_eq!(writer_usage.read_version, None);
    assert_eq!(
        writer_usage.write_version,
        Some(ResourceVersionId::Texture(TextureVersionId(0)))
    );
    assert_eq!(
        reader_usage.read_version,
        Some(ResourceVersionId::Texture(TextureVersionId(0)))
    );
    assert_eq!(reader_usage.write_version, None);
}

#[test]
fn compile_populates_version_metadata_for_load_modify() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_texture::<()>(test_texture_desc());
    let writer = graph.add_graphics_pass(WritePass {
        output: texture.clone(),
    });
    let modify = graph.add_graphics_pass(ModifyPass { target: texture });

    graph.compile().expect("compile should succeed");

    let writer_usage = graph.passes[writer.0]
        .usages
        .first()
        .copied()
        .expect("writer should have one usage");
    let modify_usage = graph.passes[modify.0]
        .usages
        .first()
        .copied()
        .expect("modify should have one usage");

    assert_eq!(
        writer_usage.write_version,
        Some(ResourceVersionId::Texture(TextureVersionId(0)))
    );
    assert_eq!(
        modify_usage.read_version,
        Some(ResourceVersionId::Texture(TextureVersionId(0)))
    );
    assert_eq!(
        modify_usage.write_version,
        Some(ResourceVersionId::Texture(TextureVersionId(1)))
    );
}

#[test]
fn compiled_pass_exposes_versioned_inputs_outputs() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_texture::<()>(test_texture_desc());
    graph.add_graphics_pass(WritePass {
        output: texture.clone(),
    });
    graph.add_graphics_pass(ModifyPass {
        target: texture.clone(),
    });

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    assert_eq!(compiled.passes.len(), 2);
    assert_eq!(compiled.passes[0].input_versions.len(), 0);
    assert_eq!(compiled.passes[0].output_versions.len(), 1);
    assert_eq!(
        compiled.passes[0].output_versions[0].version,
        ResourceVersionId::Texture(TextureVersionId(0))
    );
    assert_eq!(compiled.passes[1].input_versions.len(), 1);
    assert_eq!(compiled.passes[1].output_versions.len(), 1);
    assert_eq!(
        compiled.passes[1].input_versions[0].version,
        ResourceVersionId::Texture(TextureVersionId(0))
    );
    assert_eq!(
        compiled.passes[1].output_versions[0].version,
        ResourceVersionId::Texture(TextureVersionId(1))
    );
}

#[test]
fn compile_orders_buffer_write_then_read_from_usage_api() {
    let mut graph = FrameGraph::new();
    let buffer =
        graph.declare_buffer_internal::<()>(test_buffer_desc(), ResourceLifetime::Transient, None);
    let buffer_handle = buffer.handle().expect("declared buffer should have handle");
    let writer = graph.add_graphics_pass(ExistingBufferWritePass {
        output: OutSlot::with_handle(buffer_handle),
    });
    let reader = graph.add_graphics_pass(BufferReadPass {
        input: OutSlot::with_handle(buffer_handle),
    });

    graph.compile().expect("compile should succeed");

    assert_eq!(graph.order, vec![writer.0, reader.0]);
}

#[test]
fn compile_culls_passes_outside_present_chain() {
    let mut graph = FrameGraph::new();
    let live_texture = graph.declare_texture::<()>(test_texture_desc());
    let dead_texture = graph.declare_texture::<()>(test_texture_desc());
    let live_writer = graph.add_graphics_pass(WritePass {
        output: live_texture.clone(),
    });
    let dead_writer = graph.add_graphics_pass(WritePass {
        output: dead_texture,
    });
    let present = graph.add_graphics_pass(make_present_pass(&live_texture));
    graph
        .add_pass_sink(present, ExternalSinkKind::SurfacePresent)
        .expect("pass sink should register");

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    assert_eq!(
        compiled.execution_plan.ordered_passes,
        vec![live_writer.0, present.0]
    );
    assert!(compiled.culled_passes.contains(&dead_writer.0));
}

#[test]
fn compile_discovers_live_passes_from_resource_sink() {
    let mut graph = FrameGraph::new();
    let live_texture = graph.declare_texture::<()>(test_texture_desc());
    let dead_texture = graph.declare_texture::<()>(test_texture_desc());
    let live_writer = graph.add_graphics_pass(WritePass {
        output: live_texture.clone(),
    });
    let dead_writer = graph.add_graphics_pass(WritePass {
        output: dead_texture,
    });
    graph
        .add_texture_sink(&live_texture, ExternalSinkKind::DebugCapture)
        .expect("resource sink should register");

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    assert_eq!(compiled.execution_plan.ordered_passes, vec![live_writer.0]);
    assert!(compiled.culled_passes.contains(&dead_writer.0));
    assert_eq!(
        compiled.external_sinks,
        vec![ExternalSink {
            id: ExternalSinkId(0),
            kind: ExternalSinkKind::DebugCapture,
            target: ExternalSinkTarget::Resource(ResourceHandle::Texture(
                live_texture
                    .handle()
                    .expect("live texture should have handle")
            )),
        }]
    );
}

#[test]
fn compile_allows_multiple_writers_on_same_resource() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_texture::<()>(test_texture_desc());
    let writer_a = graph.add_graphics_pass(WritePass {
        output: texture.clone(),
    });
    let writer_b = graph.add_graphics_pass(WritePass { output: texture });

    graph.compile().expect("compile should succeed");

    assert_eq!(graph.order, vec![writer_a.0, writer_b.0]);
}

#[test]
fn compile_culls_dead_overwritten_writer_from_version_flow() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_texture::<()>(test_texture_desc());
    let dead_writer = graph.add_graphics_pass(WritePass {
        output: texture.clone(),
    });
    let live_writer = graph.add_graphics_pass(WritePass {
        output: texture.clone(),
    });
    graph
        .add_texture_sink(&texture, ExternalSinkKind::ExportTexture)
        .expect("texture sink should be added");

    graph.compile().expect("compile should succeed");

    assert_eq!(graph.order, vec![live_writer.0]);
    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    assert_eq!(compiled.culled_passes, vec![dead_writer.0]);
}

#[test]
fn texture_sink_rejects_buffer_export_kind() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_texture::<()>(test_texture_desc());

    let err = graph
        .add_texture_sink(&texture, ExternalSinkKind::ExportBuffer)
        .expect_err("texture sink should reject buffer export kind");

    assert!(matches!(err, FrameGraphError::Validation(_)));
}

#[test]
fn buffer_sink_rejects_texture_export_kind() {
    let mut graph = FrameGraph::new();
    let buffer =
        graph.declare_buffer_internal::<()>(test_buffer_desc(), ResourceLifetime::Transient, None);

    let err = graph
        .add_buffer_sink(&buffer, ExternalSinkKind::ExportTexture)
        .expect_err("buffer sink should reject texture export kind");

    assert!(matches!(err, FrameGraphError::Validation(_)));
}

#[test]
fn compile_rejects_read_without_producer() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_texture::<()>(test_texture_desc());
    graph.add_graphics_pass(ReadPass {
        input: InSlot::with_handle(
            texture
                .handle()
                .expect("declared texture should have handle"),
        ),
    });

    let err = graph.compile().expect_err("compile should fail");
    assert!(matches!(err, FrameGraphError::MissingInput(_)));
}
