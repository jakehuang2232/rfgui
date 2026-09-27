use super::*;

fn graph(lifetime: ResourceLifetime) -> (FrameGraph, OutSlot<TextureResource, ()>) {
    let mut graph = FrameGraph::new();
    let texture =
        graph.declare_texture_internal(test_depth_stencil_texture_desc(), lifetime, Some(9));
    graph.add_graphics_pass(DepthStencilWritePass {
        target: texture.clone(),
    });
    graph.add_graphics_pass(DepthStencilReadPass {
        target: texture.clone(),
    });
    (graph, texture)
}

#[test]
fn intermediate_attachments_store_and_only_last_transient_use_discards() {
    for lifetime in [
        ResourceLifetime::Transient,
        ResourceLifetime::Imported,
        ResourceLifetime::Persistent,
    ] {
        let (mut graph, texture) = graph(lifetime);
        graph.compile().unwrap();
        let stores = &graph.compiled_graph().unwrap().depth_stencil_stores;
        let target = AttachmentTarget::Texture(texture.handle().unwrap());
        assert!(!stores.discard_after(target, &[0]));
        assert_eq!(
            stores.discard_after(target, &[1]),
            lifetime == ResourceLifetime::Transient
        );
    }
}

#[test]
fn exported_depth_stencil_remains_stored() {
    let (mut graph, texture) = graph(ResourceLifetime::Transient);
    graph
        .add_texture_sink(&texture, ExternalSinkKind::Readback)
        .unwrap();
    graph.compile().unwrap();
    assert!(
        !graph
            .compiled_graph()
            .unwrap()
            .depth_stencil_stores
            .discard_after(
                AttachmentTarget::Texture(texture.handle().unwrap()),
                &[0, 1],
            )
    );
}

#[test]
fn later_sampling_requires_stored_depth_stencil() {
    let (mut graph, texture) = graph(ResourceLifetime::Transient);
    graph.add_graphics_pass(ReadPass {
        input: InSlot::with_handle(texture.handle().unwrap()),
    });
    graph.compile().unwrap();
    assert!(
        !graph
            .compiled_graph()
            .unwrap()
            .depth_stencil_stores
            .discard_after(
                AttachmentTarget::Texture(texture.handle().unwrap()),
                &[0, 1],
            )
    );
}

#[test]
fn discard_does_not_split_a_mergeable_group() {
    let (mut graph, texture) = graph(ResourceLifetime::Transient);
    graph.add_graphics_pass(DepthStencilReadPass {
        target: texture.clone(),
    });
    for pass in &mut graph.passes {
        if let PassDetails::Graphics(graphics) = &mut pass.descriptor.details {
            graphics.merge_policy = GraphicsPassMergePolicy::Mergeable;
        }
    }
    graph.compile().unwrap();
    let compiled = graph.compiled_graph().unwrap();
    let group = compiled
        .execution_plan
        .steps
        .iter()
        .find_map(|step| match step {
            CompiledExecuteStep::GraphicsPassGroup(group) if group.pass_indices.contains(&2) => {
                Some(group)
            }
            _ => None,
        })
        .expect("two read-only logical passes still share one GPU pass");
    assert!(group.pass_indices.contains(&1));
    assert!(compiled.depth_stencil_stores.discard_after(
        AttachmentTarget::Texture(texture.handle().unwrap()),
        &group.pass_indices,
    ));
}
