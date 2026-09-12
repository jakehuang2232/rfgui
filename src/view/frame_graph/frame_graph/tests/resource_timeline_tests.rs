use super::*;

#[test]
fn compile_tracks_resource_lifetime_by_compiled_pass_index() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_texture::<()>(test_texture_desc());
    graph.add_graphics_pass(WritePass {
        output: texture.clone(),
    });
    graph.add_graphics_pass(ModifyPass {
        target: texture.clone(),
    });
    graph.add_graphics_pass(ReadPass {
        input: InSlot::with_handle(
            texture
                .handle()
                .expect("declared texture should have handle"),
        ),
    });

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    let resource = compiled
        .resources
        .iter()
        .find(|resource| resource.handle == ResourceHandle::Texture(texture.handle().unwrap()))
        .expect("compiled resource should exist");
    assert_eq!(resource.first_use_pass_index, 0);
    assert_eq!(resource.last_use_pass_index, 2);
}

#[test]
fn compile_tracks_resource_state_transitions_per_pass() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_texture::<()>(test_texture_desc());
    graph.add_graphics_pass(WritePass {
        output: texture.clone(),
    });
    graph.add_graphics_pass(ReadPass {
        input: InSlot::with_handle(
            texture
                .handle()
                .expect("declared texture should have handle"),
        ),
    });

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    let transitions = compiled
        .resource_transitions
        .iter()
        .filter(|transition| {
            transition.resource == ResourceHandle::Texture(texture.handle().unwrap())
        })
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(
        transitions,
        vec![
            CompiledResourceTransition {
                resource: ResourceHandle::Texture(texture.handle().unwrap()),
                pass_index: 0,
                execution_index: 0,
                before: ResourceState::Texture(TextureResourceState::Undefined),
                after: ResourceState::Texture(TextureResourceState::ColorAttachment),
            },
            CompiledResourceTransition {
                resource: ResourceHandle::Texture(texture.handle().unwrap()),
                pass_index: 1,
                execution_index: 1,
                before: ResourceState::Texture(TextureResourceState::ColorAttachment),
                after: ResourceState::Texture(TextureResourceState::Sampled),
            },
        ]
    );
    assert_eq!(compiled.passes[0].resource_transitions.len(), 1);
    assert_eq!(
        compiled.passes[0].resource_transitions[0],
        CompiledPassResourceTransition {
            resource: ResourceHandle::Texture(texture.handle().unwrap()),
            before: ResourceState::Texture(TextureResourceState::Undefined),
            after: ResourceState::Texture(TextureResourceState::ColorAttachment),
        }
    );
}

#[test]
fn compile_tracks_depth_stencil_state_in_timeline() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_texture::<()>(test_depth_stencil_texture_desc());
    graph.add_graphics_pass(DepthStencilWritePass {
        target: texture.clone(),
    });
    graph.add_graphics_pass(DepthStencilReadPass {
        target: texture.clone(),
    });

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    let timeline = compiled
        .resource_timelines
        .iter()
        .find(|timeline| timeline.resource == ResourceHandle::Texture(texture.handle().unwrap()))
        .expect("resource timeline should exist");
    assert_eq!(
        timeline.transitions,
        vec![
            CompiledResourceTransition {
                resource: ResourceHandle::Texture(texture.handle().unwrap()),
                pass_index: 0,
                execution_index: 0,
                before: ResourceState::Texture(TextureResourceState::Undefined),
                after: ResourceState::Texture(TextureResourceState::DepthStencilAttachment {
                    depth: Some(TextureAspectState::Write),
                    stencil: Some(TextureAspectState::Write),
                }),
            },
            CompiledResourceTransition {
                resource: ResourceHandle::Texture(texture.handle().unwrap()),
                pass_index: 1,
                execution_index: 1,
                before: ResourceState::Texture(TextureResourceState::DepthStencilAttachment {
                    depth: Some(TextureAspectState::Write),
                    stencil: Some(TextureAspectState::Write),
                },),
                after: ResourceState::Texture(TextureResourceState::DepthStencilAttachment {
                    depth: Some(TextureAspectState::Read),
                    stencil: Some(TextureAspectState::Read),
                }),
            },
        ]
    );
}
