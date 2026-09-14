use super::*;
use crate::view::paint::record_surface_dag_frame_artifact_cached;

#[test]
fn retained_metadata_requires_current_commands_and_failed_frames_release_blocks() {
    let (arena, root, properties, generations) = crate::view::paint::tests::prepared_leaf(
        0xfeed_a002,
        crate::style::Color::rgb(60, 80, 100),
        0.5,
        false,
    );
    let mut recording = RecordingCache::default();
    let _ = record_surface_dag_frame_artifact_cached(
        &arena,
        &[root],
        &properties,
        &generations,
        &mut recording,
    )
    .unwrap();
    let scope = recording.scopes[&root].0.clone();
    let metadata = recording.entries[&root].metadata.before_children[0].clone();
    let observation = || PaintCoverageItem::ArtifactChunk {
        order: Default::default(),
        chunk: metadata.clone(),
        owner_scope: scope.clone(),
        clip_snapshot: Arc::from([]),
        effect_snapshot: Arc::from([]),
        ops: None,
    };
    // Storage-only input: repeated observations exercise block reuse. Normal
    // recorder preflight independently enforces the unique chunk schedule.
    let source: Arc<[_]> = vec![observation(), observation()].into();
    let ops = recording.entries[&root]
        .shared_ops
        .iter()
        .cloned()
        .collect::<FxHashMap<_, _>>();
    let mut cache = MaterializedBlocks::default();
    let first = cache
        .materialize(source.clone().into(), &ops)
        .into_blocks()
        .pop()
        .unwrap();
    cache.finish(true);
    cache.begin();
    let same = cache
        .materialize(source.clone().into(), &ops)
        .into_blocks()
        .pop()
        .unwrap();
    assert!(Arc::ptr_eq(&first, &same));
    let reallocated: Arc<[_]> = source.iter().cloned().collect::<Vec<_>>().into();
    let same_values = cache.materialize_block(reallocated, &ops);
    assert!(
        Arc::ptr_eq(&first, &same_values),
        "complete identical observations retain the block"
    );
    let mut changed_metadata = source.to_vec();
    let PaintCoverageItem::ArtifactChunk { chunk, .. } = &mut changed_metadata[0] else {
        unreachable!();
    };
    chunk.content_revision.self_paint_revision += 1;
    let replaced = cache.materialize_block(changed_metadata.into(), &ops);
    assert!(
        !Arc::ptr_eq(&first, &replaced),
        "metadata changes must detach even when operations are identical"
    );
    let mut changed_order = source.to_vec();
    let PaintCoverageItem::ArtifactChunk { order, .. } = &mut changed_order[0] else {
        unreachable!();
    };
    order.root_index += 1;
    let replaced = cache.materialize_block(changed_order.into(), &ops);
    assert!(
        !Arc::ptr_eq(&first, &replaced),
        "paint order belongs to the immutable input"
    );

    let mut changed = ops.clone();
    let id = *changed.keys().next().unwrap();
    let replacement: Arc<[crate::view::paint::PaintOp]> = Arc::from([]);
    changed.insert(id, replacement.clone());
    let fresh = cache
        .materialize(source.clone().into(), &changed)
        .into_blocks()
        .pop()
        .unwrap();
    assert!(
        !Arc::ptr_eq(&first, &fresh),
        "same metadata must not conceal changed commands"
    );
    let actual = fresh
        .iter()
        .find_map(|item| match item {
            PaintCoverageItem::ArtifactChunk {
                chunk,
                ops: Some(ops),
                ..
            } if chunk.id == id => Some(ops),
            _ => None,
        })
        .unwrap();
    assert!(Arc::ptr_eq(actual, &replacement));
    cache.finish(false);
    assert!(cache.entries.is_empty());
    let retry = cache
        .materialize(source.into(), &ops)
        .into_blocks()
        .pop()
        .unwrap();
    assert!(!Arc::ptr_eq(&first, &retry));
    assert_eq!(format!("{first:?}"), format!("{retry:?}"));
}
