use super::*;

fn two_clipped_roots() -> (PaintArtifact, ClipNodeId, ClipNodeId) {
    let (arena, roots, _) = prepared_plain_tree();
    let (properties, generations) = sync_identity(&arena, &roots);
    let mut input = artifact(
        record_surface_dag_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap(),
    );
    let parent: std::collections::HashMap<_, _> = input
        .owner_nodes
        .iter()
        .map(|n| (n.owner, n.parent))
        .collect();
    let clip_ids = roots
        .iter()
        .map(|&owner| ClipNodeId {
            owner,
            role: ClipNodeRole::SelfClip,
        })
        .collect::<Vec<_>>();
    let clip_for = |mut owner| {
        while let Some(Some(next)) = parent.get(&owner) {
            owner = *next;
        }
        *clip_ids.iter().find(|clip| clip.owner == owner).unwrap()
    };
    for &id in &clip_ids {
        input.clip_nodes.push(ClipNodeSnapshot {
            id,
            owner: id.owner,
            parent: None,
            logical_scissor: [0, 0, 320, 240],
            behavior: ClipBehavior::Replace,
            generation: 1,
        });
    }
    for chunk in &mut input.chunks {
        chunk.properties.clip = Some(clip_for(chunk.owner));
    }
    for owner in &mut input.owner_property_states {
        owner.paint.clip = Some(clip_for(owner.owner));
        owner.descendants.clip = owner.paint.clip;
    }
    (input, clip_ids[0], clip_ids[1])
}

#[test]
fn local_clip_edit_reprepares_only_spans_that_consume_it() {
    let (baseline, first, second) = two_clipped_roots();
    let context = ArtifactSurfaceRasterContext::new(
        1.,
        wgpu::TextureFormat::Rgba8Unorm,
        [0., 0.],
        None,
        8192,
        128 * 1024 * 1024,
    )
    .unwrap();
    let mut cache = PlanningCache::default();
    for frame in 0..4 {
        let mut input = baseline.clone();
        if frame >= 2 {
            let clip = input
                .clip_nodes
                .iter_mut()
                .find(|clip| clip.id == first)
                .unwrap();
            clip.logical_scissor = [0, 0, 12, 8];
            clip.generation = 2;
        }
        if frame == 3 {
            let clip = input
                .clip_nodes
                .iter_mut()
                .find(|clip| clip.id == second)
                .unwrap();
            clip.logical_scissor = [0, 0, 7, 6];
            clip.generation = 2;
        }
        let cached =
            prepare_artifact_surface_raster_plan_cached(input.clone(), context, &mut cache)
                .unwrap();
        let fresh = prepare_artifact_surface_raster_plan(input, context).unwrap();
        assert_eq!(format!("{cached:?}"), format!("{fresh:?}"), "frame {frame}");
        assert_eq!(
            cache.raster_span_hits(),
            match frame {
                0 => 0,
                1 => 2,
                _ => 1,
            },
            "frame {frame}"
        );
    }
    let mut invalid = baseline;
    invalid.clip_nodes[0].parent = Some(first);
    assert!(
        prepare_artifact_surface_raster_plan_cached(invalid, context, &mut cache).is_err(),
        "dependency reuse cannot hide an invalid current clip graph"
    );
}
