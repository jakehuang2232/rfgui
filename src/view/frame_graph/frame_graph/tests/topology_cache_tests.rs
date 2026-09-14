use super::*;

#[test]
fn topology_cache_rejects_equal_hash_with_different_typed_key_mapping() {
    let desc = test_texture_desc();
    let mut cached_graph = FrameGraph::new();
    let _ = cached_graph.declare_persistent_texture_internal::<()>(
        desc.clone(),
        PersistentTextureKey::retained(RetainedTextureRole::TransformedColor, u64::MAX),
    );
    let mut current_graph = FrameGraph::new();
    let _ = current_graph.declare_persistent_texture_internal::<()>(
        desc,
        PersistentTextureKey::retained(RetainedTextureRole::IsolationColor, u64::MAX),
    );

    // Inject the same fast hash to model an actual hash collision. Full
    // canonical equality must remain the correctness boundary.
    let cached = TopologyCacheKey {
        hash: current_graph.compute_topology_hash(),
        signature: cached_graph.topology_signature(),
    };
    assert!(!current_graph.resolve_topology_cache_key(Some(cached)).1);
}

#[test]
fn topology_cache_accepts_equal_hash_and_equal_canonical_signature() {
    let mut graph = FrameGraph::new();
    let _ = graph.declare_persistent_texture_internal::<()>(
        test_texture_desc(),
        PersistentTextureKey::retained(RetainedTextureRole::TransformedColor, u64::MAX),
    );
    let cached = graph.resolve_topology_cache_key(None).0;
    assert!(graph.resolve_topology_cache_key(Some(cached)).1);
}

fn signature_fixture() -> FrameGraph {
    let mut graph = FrameGraph::new();
    graph.add_graphics_pass(SurfacePass);
    graph.compile().unwrap();
    graph.declare_persistent_texture_internal::<()>(
        test_texture_desc().with_label("cache fixture"),
        PersistentTextureKey::Generic(17),
    );
    graph.declare_buffer_internal::<()>(
        BufferDesc {
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM,
            label: Some("uniform"),
        },
        ResourceLifetime::Persistent,
        Some(18),
    );
    graph
        .texture_attachment_pairs
        .insert(TextureHandle(0), AttachmentTarget::Surface);
    graph.add_external_sink(
        ExternalSinkKind::Readback,
        ExternalSinkTarget::Pass(PassHandle(0)),
    );
    graph.passes[0].usages.push(PassResourceUsage {
        resource: ResourceHandle::Buffer(BufferHandle(0)),
        usage: ResourceUsage::UniformRead,
        read_version: None,
        write_version: None,
    });
    graph.passes[0]
        .descriptor
        .graphics_mut()
        .depth_stencil_attachment = Some(GraphicsDepthStencilAttachmentDescriptor {
        target: AttachmentTarget::Surface,
        depth: Some(GraphicsDepthAspectDescriptor::write(
            AttachmentLoadOp::Clear,
            Some(1.0),
        )),
        stencil: Some(GraphicsStencilAspectDescriptor::write(
            AttachmentLoadOp::Clear,
            Some(0),
        )),
    });
    graph
}

#[test]
fn topology_live_comparison_preserves_every_signature_input() {
    // The original materialized signature remains an independent equality
    // oracle. Force equal hashes so each mutation reaches the full comparison.
    let cases: &[(&str, fn(&mut FrameGraph))] = &[
        ("pass count", |g| {
            g.passes.pop();
        }),
        ("pass name", |g| g.passes[0].descriptor.name = "changed"),
        ("pass kind", |g| {
            g.passes[0].descriptor.kind = PassKind::Compute
        }),
        ("pass details", |g| {
            g.passes[0].descriptor.details = PassDetails::Compute(ComputePassDescriptor)
        }),
        ("usages", |g| {
            g.passes[0].usages.clear();
        }),
        ("usage resource", |g| {
            g.passes[0].usages[0].resource = ResourceHandle::Texture(TextureHandle(0))
        }),
        ("usage access", |g| {
            g.passes[0].usages[0].usage = ResourceUsage::StorageRead
        }),
        ("color count", |g| {
            g.passes[0]
                .descriptor
                .graphics_mut()
                .color_attachments
                .clear()
        }),
        ("color target", |g| {
            g.passes[0].descriptor.graphics_mut().color_attachments[0].target =
                AttachmentTarget::Texture(TextureHandle(0))
        }),
        ("color load", |g| {
            g.passes[0].descriptor.graphics_mut().color_attachments[0].load_op =
                AttachmentLoadOp::Load
        }),
        ("color store", |g| {
            g.passes[0].descriptor.graphics_mut().color_attachments[0].store_op =
                AttachmentStoreOp::Discard
        }),
        ("color bits", |g| {
            g.passes[0].descriptor.graphics_mut().color_attachments[0]
                .clear_color
                .as_mut()
                .unwrap()[0] = -0.0
        }),
        ("depth attachment", |g| {
            g.passes[0]
                .descriptor
                .graphics_mut()
                .depth_stencil_attachment = None
        }),
        ("depth target", |g| {
            g.passes[0]
                .descriptor
                .graphics_mut()
                .depth_stencil_attachment
                .as_mut()
                .unwrap()
                .target = AttachmentTarget::Texture(TextureHandle(0))
        }),
        ("depth value", |g| {
            g.passes[0]
                .descriptor
                .graphics_mut()
                .depth_stencil_attachment
                .as_mut()
                .unwrap()
                .depth
                .as_mut()
                .unwrap()
                .clear_depth = Some(0.5)
        }),
        ("depth usage", |g| {
            g.passes[0]
                .descriptor
                .graphics_mut()
                .depth_stencil_attachment
                .as_mut()
                .unwrap()
                .depth
                .as_mut()
                .unwrap()
                .usage = ResourceUsage::DepthRead
        }),
        ("depth load", |g| {
            g.passes[0]
                .descriptor
                .graphics_mut()
                .depth_stencil_attachment
                .as_mut()
                .unwrap()
                .depth
                .as_mut()
                .unwrap()
                .load_op = AttachmentLoadOp::Load
        }),
        ("depth store", |g| {
            g.passes[0]
                .descriptor
                .graphics_mut()
                .depth_stencil_attachment
                .as_mut()
                .unwrap()
                .depth
                .as_mut()
                .unwrap()
                .store_op = AttachmentStoreOp::Discard
        }),
        ("stencil value", |g| {
            g.passes[0]
                .descriptor
                .graphics_mut()
                .depth_stencil_attachment
                .as_mut()
                .unwrap()
                .stencil
                .as_mut()
                .unwrap()
                .clear_stencil = Some(2)
        }),
        ("stencil usage", |g| {
            g.passes[0]
                .descriptor
                .graphics_mut()
                .depth_stencil_attachment
                .as_mut()
                .unwrap()
                .stencil
                .as_mut()
                .unwrap()
                .usage = ResourceUsage::StencilRead
        }),
        ("stencil load", |g| {
            g.passes[0]
                .descriptor
                .graphics_mut()
                .depth_stencil_attachment
                .as_mut()
                .unwrap()
                .stencil
                .as_mut()
                .unwrap()
                .load_op = AttachmentLoadOp::Load
        }),
        ("stencil store", |g| {
            g.passes[0]
                .descriptor
                .graphics_mut()
                .depth_stencil_attachment
                .as_mut()
                .unwrap()
                .stencil
                .as_mut()
                .unwrap()
                .store_op = AttachmentStoreOp::Discard
        }),
        ("samples", |g| {
            g.passes[0].descriptor.graphics_mut().sample_count = SampleCountPolicy::Fixed(4)
        }),
        ("viewport", |g| {
            g.passes[0].descriptor.graphics_mut().viewport_policy = ViewportPolicy::FixedToTarget
        }),
        ("scissor", |g| {
            g.passes[0].descriptor.graphics_mut().scissor_policy = ScissorPolicy::Disabled
        }),
        ("merge", |g| {
            g.passes[0].descriptor.graphics_mut().merge_policy = GraphicsPassMergePolicy::Mergeable
        }),
        ("requirements", |g| {
            g.passes[0]
                .descriptor
                .graphics_mut()
                .requirements
                .uses_depth = true
        }),
        ("sinks", |g| g.external_sinks.clear()),
        ("texture descriptor", |g| {
            g.textures[0] = g.textures[0].clone().with_size(31, 43)
        }),
        ("texture metadata", |g| {
            g.texture_metadata[0].lifetime = ResourceLifetime::Imported
        }),
        ("buffer size", |g| g.buffers[0].size += 16),
        ("buffer usage", |g| {
            g.buffers[0].usage = wgpu::BufferUsages::STORAGE
        }),
        ("buffer label", |g| g.buffers[0].label = Some("changed")),
        ("buffer metadata", |g| {
            g.buffer_metadata[0].stable_key = Some(PersistentTextureKey::Generic(19))
        }),
        ("attachment pair", |g| {
            g.texture_attachment_pairs.insert(
                TextureHandle(0),
                AttachmentTarget::Texture(TextureHandle(0)),
            );
        }),
        ("attachment pair count", |g| {
            g.texture_attachment_pairs.clear()
        }),
    ];
    for (name, change) in cases {
        let mut graph = signature_fixture();
        let old = graph.topology_signature();
        assert!(old.matches_live(&graph), "fixture: {name}");
        change(&mut graph);
        assert_ne!(
            old,
            graph.topology_signature(),
            "ineffective mutation: {name}"
        );
        let cached = TopologyCacheKey {
            hash: graph.compute_topology_hash(),
            signature: old,
        };
        let (new, hit) = graph.resolve_topology_cache_key(Some(cached));
        assert!(!hit, "incorrect reuse: {name}");
        assert_eq!(
            new.signature,
            graph.topology_signature(),
            "stale miss key: {name}"
        );
    }
}

#[test]
fn topology_live_comparison_keeps_nan_bits_and_ignores_map_insertion_order() {
    let mut graph = signature_fixture();
    graph.passes[0].descriptor.graphics_mut().color_attachments[0].clear_color =
        Some([f64::from_bits(0x7ff8_0000_0000_0042); 4]);
    graph.passes[0]
        .descriptor
        .graphics_mut()
        .depth_stencil_attachment
        .as_mut()
        .unwrap()
        .depth
        .as_mut()
        .unwrap()
        .clear_depth = Some(f32::from_bits(0x7fc0_0042));
    graph
        .texture_attachment_pairs
        .insert(TextureHandle(1), AttachmentTarget::Surface);
    let cached = graph.resolve_topology_cache_key(None).0;
    graph.texture_attachment_pairs.clear();
    for color in [1, 0] {
        graph
            .texture_attachment_pairs
            .insert(TextureHandle(color), AttachmentTarget::Surface);
    }
    let (cached, hit) = graph.resolve_topology_cache_key(Some(cached));
    assert!(
        hit,
        "equal NaN payloads must compare by bits, not float equality"
    );
    graph.passes[0]
        .descriptor
        .graphics_mut()
        .depth_stencil_attachment
        .as_mut()
        .unwrap()
        .depth
        .as_mut()
        .unwrap()
        .clear_depth = Some(f32::from_bits(0x7fc0_0043));
    assert!(!graph.resolve_topology_cache_key(Some(cached)).1);
}

#[test]
fn topology_live_comparison_preserves_compute_transfer_order_and_ignores_versions() {
    let mut graph = signature_fixture();
    graph.passes[0].descriptor = PassDescriptor::compute("compute");
    graph.add_graphics_pass(SurfacePass);
    graph.passes[1].descriptor = PassDescriptor::transfer("transfer");
    let key = graph.resolve_topology_cache_key(None).0;
    graph.passes[0].usages[0].read_version = Some(ResourceVersionId::Buffer(BufferVersionId(9)));
    let (key, hit) = graph.resolve_topology_cache_key(Some(key));
    assert!(
        hit,
        "annotated versions are compile outputs, not topology inputs"
    );
    graph.passes.swap(0, 1);
    let key = TopologyCacheKey {
        hash: graph.compute_topology_hash(),
        ..key
    };
    assert!(!graph.resolve_topology_cache_key(Some(key)).1);
}

#[test]
fn compile_cache_hit_reuses_signature_storage_and_miss_rebuilds_execution_plan() {
    let mut viewport = Viewport::new();
    let mut first = FrameGraph::new();
    first.add_graphics_pass(SurfacePass);
    let (cold, key) = first
        .compile_with_upload_cached(&mut viewport, None)
        .unwrap();
    assert!(!cold.topology_cache_hit);
    let signature_storage = key.signature.passes.as_ptr();
    let old_compiled = first.take_compiled_graph().unwrap();
    let mut warm = FrameGraph::new();
    warm.add_graphics_pass(SurfacePass);
    let (hit, key) = warm
        .compile_with_upload_cached(&mut viewport, Some((key, old_compiled)))
        .unwrap();
    assert!(hit.topology_cache_hit);
    assert_eq!(key.signature.passes.as_ptr(), signature_storage);
    assert_eq!(hit.annotate_resource_versions_ms, 0.0);
    assert_eq!(hit.build_compiled_graph_ms, 0.0);
    let old_compiled = warm.take_compiled_graph().unwrap();
    let mut changed = FrameGraph::new();
    changed.add_graphics_pass(SurfacePass);
    changed.add_graphics_pass(SurfacePass);
    let (miss, _) = changed
        .compile_with_upload_cached(&mut viewport, Some((key, old_compiled)))
        .unwrap();
    assert!(!miss.topology_cache_hit);
    assert_eq!(changed.order.len(), 2);
    for profile in [&cold, &hit, &miss] {
        assert!(profile.topology_cache_lookup_ms.is_finite());
        assert!(profile.topology_cache_lookup_ms >= 0.0);
        assert!(profile.topology_cache_lookup_ms <= profile.total_ms);
    }
}
