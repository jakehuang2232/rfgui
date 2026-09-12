use super::*;

#[test]
fn compile_aliases_transient_textures_when_lifetimes_do_not_overlap() {
    let mut graph = FrameGraph::new();
    let texture_a = graph.declare_texture::<()>(test_texture_desc());
    let texture_b = graph.declare_texture::<()>(test_texture_desc());
    graph.add_graphics_pass(WritePass {
        output: texture_a.clone(),
    });
    graph.add_graphics_pass(WritePass {
        output: texture_b.clone(),
    });

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    let resource_a = compiled
        .resources
        .iter()
        .find(|resource| resource.handle == ResourceHandle::Texture(texture_a.handle().unwrap()))
        .expect("resource A should exist");
    let resource_b = compiled
        .resources
        .iter()
        .find(|resource| resource.handle == ResourceHandle::Texture(texture_b.handle().unwrap()))
        .expect("resource B should exist");
    assert_eq!(resource_a.allocation_id, resource_b.allocation_id);
    assert_eq!(compiled.allocation_plan.texture_allocations.len(), 1);
}

#[test]
fn compile_aliases_smaller_transient_texture_into_earlier_larger_slot() {
    let mut graph = FrameGraph::new();
    let large = graph.declare_texture::<()>(TextureDesc::new(
        200,
        100,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureDimension::D2,
    ));
    let small = graph.declare_texture::<()>(TextureDesc::new(
        100,
        50,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureDimension::D2,
    ));
    graph.add_graphics_pass(WritePass {
        output: large.clone(),
    });
    graph.add_graphics_pass(WritePass {
        output: small.clone(),
    });

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    let large_resource = compiled
        .resources
        .iter()
        .find(|resource| resource.handle == ResourceHandle::Texture(large.handle().unwrap()))
        .expect("large resource should exist");
    let small_resource = compiled
        .resources
        .iter()
        .find(|resource| resource.handle == ResourceHandle::Texture(small.handle().unwrap()))
        .expect("small resource should exist");
    assert_eq!(large_resource.allocation_id, small_resource.allocation_id);
    assert_eq!(compiled.allocation_plan.texture_allocations.len(), 1);
}

#[test]
fn compile_does_not_alias_larger_transient_texture_after_earlier_smaller_slot() {
    let mut graph = FrameGraph::new();
    let small = graph.declare_texture::<()>(TextureDesc::new(
        100,
        50,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureDimension::D2,
    ));
    let large = graph.declare_texture::<()>(TextureDesc::new(
        200,
        100,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureDimension::D2,
    ));
    graph.add_graphics_pass(WritePass {
        output: small.clone(),
    });
    graph.add_graphics_pass(WritePass {
        output: large.clone(),
    });

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    let small_resource = compiled
        .resources
        .iter()
        .find(|resource| resource.handle == ResourceHandle::Texture(small.handle().unwrap()))
        .expect("small resource should exist");
    let large_resource = compiled
        .resources
        .iter()
        .find(|resource| resource.handle == ResourceHandle::Texture(large.handle().unwrap()))
        .expect("large resource should exist");
    assert_ne!(small_resource.allocation_id, large_resource.allocation_id);
    assert_eq!(compiled.allocation_plan.texture_allocations.len(), 2);
}

#[test]
fn compile_marks_surface_as_external_owned() {
    let mut graph = FrameGraph::new();
    graph.add_graphics_pass(SurfacePass);

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    assert_eq!(
        compiled.allocation_plan.external_resources,
        vec![ExternalAllocationPlanEntry {
            resource: ExternalResource::Surface,
            owner: AllocationOwner::ExternalOwned,
        }]
    );
}

#[test]
fn compile_keeps_internal_persistent_resources_out_of_aliasing() {
    let mut graph = FrameGraph::new();
    graph.add_graphics_pass(PersistentInternalPass::default());

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    let resource = compiled
        .resources
        .iter()
        .find(|resource| resource.lifetime == ResourceLifetime::Persistent)
        .expect("persistent resource should exist");
    assert_eq!(
        resource.stable_key,
        Some(PersistentTextureKey::Generic(0xCAFE))
    );
    assert_eq!(resource.allocation_id, None);
    assert!(compiled.allocation_plan.texture_allocations.is_empty());
}

#[test]
fn compile_keeps_transient_buffers_on_distinct_allocations() {
    let mut graph = FrameGraph::new();
    graph.add_graphics_pass(BufferWritePass::default());
    graph.add_graphics_pass(BufferWritePass::default());

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    let buffer_resources = compiled
        .resources
        .iter()
        .filter(|resource| matches!(resource.handle, ResourceHandle::Buffer(_)))
        .collect::<Vec<_>>();
    assert_eq!(buffer_resources.len(), 2);
    assert_ne!(
        buffer_resources[0].allocation_id,
        buffer_resources[1].allocation_id
    );
    assert_eq!(compiled.allocation_plan.buffer_allocations.len(), 2);
}
