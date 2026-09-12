use super::*;

fn allocations(exact: bool, second_size: [u32; 2]) -> (AllocationId, AllocationId) {
    let mut graph = FrameGraph::new();
    for size in [[48, 120], second_size] {
        let desc = TextureDesc::new(
            size[0],
            size[1],
            wgpu::TextureFormat::Depth32FloatStencil8,
            wgpu::TextureDimension::D2,
        );
        graph.textures.push(if exact {
            desc.with_exact_extent()
        } else {
            desc
        });
    }
    let resources = (0..2)
        .map(|index| CompiledResource {
            handle: ResourceHandle::Texture(TextureHandle(index)),
            stable_key: None,
            kind: ResourceKind::Texture,
            allocation_class: AllocationClass::Texture,
            lifetime: ResourceLifetime::Transient,
            first_use_pass_index: index as usize,
            last_use_pass_index: index as usize,
            producer_passes: Vec::new(),
            consumer_passes: Vec::new(),
            allocation_id: None,
            allocation_owner: AllocationOwner::AllocatorManaged,
        })
        .collect::<Vec<_>>();
    let (_, ids, _) = build_allocation_plan(&resources, &[], &graph);
    (ids[&TextureHandle(0)], ids[&TextureHandle(1)])
}

#[test]
fn exact_attachments_share_only_identical_extents() {
    let (first, smaller) = allocations(true, [20, 16]);
    assert_ne!(
        first, smaller,
        "depth must not inherit a larger attachment extent"
    );
    let (first, same) = allocations(true, [48, 120]);
    assert_eq!(
        first, same,
        "disjoint exact attachments can still share storage"
    );
    let (first, smaller) = allocations(false, [20, 16]);
    assert_eq!(
        first, smaller,
        "ordinary scratch allocation keeps larger-fit reuse"
    );
}
