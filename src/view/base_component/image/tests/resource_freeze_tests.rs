use super::*;

#[test]
fn arena_sync_freezes_one_resource_generation_across_repeated_measure_and_identity_reads() {
    let mut image = Image::new_with_id(31, rgba_source(1, 1));
    let asset_id = image.source_handle.asset_id();
    let initial_signature = image.retained_paint_signature();
    crate::view::image_resource::replace_ready_image_for_test(
        asset_id,
        2,
        1,
        std::sync::Arc::from([1_u8; 8]),
    );
    assert_eq!(
        image.retained_paint_signature(),
        initial_signature,
        "retained identity must not observe registry state ahead of the frame freeze"
    );

    let mut arena = new_test_arena();
    image.clear_local_dirty_flags(DirtyFlags::ALL);
    image.sync_arena(&mut arena);
    let frozen_signature = image.retained_paint_signature();
    assert_ne!(frozen_signature, initial_signature);
    assert!(image.local_dirty_flags().contains(DirtyFlags::LAYOUT));

    crate::view::image_resource::replace_ready_image_for_test(
        asset_id,
        5,
        1,
        std::sync::Arc::from([2_u8; 20]),
    );
    image.measure(
        LayoutConstraints {
            max_width: 100.0,
            max_height: 100.0,
            viewport_width: 100.0,
            viewport_height: 100.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        &mut arena,
    );
    assert_eq!(image.measured_size(), (2.0, 1.0));
    assert_eq!(image.retained_paint_signature(), frozen_signature);

    image.measure(
        LayoutConstraints {
            max_width: 100.0,
            max_height: 100.0,
            viewport_width: 100.0,
            viewport_height: 100.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        &mut arena,
    );
    assert_eq!(image.measured_size(), (2.0, 1.0));
    assert_eq!(image.retained_paint_signature(), frozen_signature);

    image.sync_arena(&mut arena);
    image.measure(
        LayoutConstraints {
            max_width: 100.0,
            max_height: 100.0,
            viewport_width: 100.0,
            viewport_height: 100.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        &mut arena,
    );
    assert_eq!(image.measured_size(), (5.0, 1.0));
    assert_ne!(image.retained_paint_signature(), frozen_signature);
}

#[test]
fn path_ready_leaf_records_one_canonical_frozen_upload_after_handle_drop() {
    let pixels: std::sync::Arc<[u8]> = std::sync::Arc::from([17_u8; 24]);
    let (arena, root, asset_id, generation) =
        prepared_ready_image(0x9101, path_source("ready"), 3, 2, pixels.clone());
    {
        let mut node = arena.get_mut(root).unwrap();
        let image = node.element.as_any_mut().downcast_mut::<Image>().unwrap();
        image.set_fit(crate::view::ImageFit::Cover);
        image.set_sampling(crate::view::ImageSampling::Nearest);
    }
    let (metadata, artifact) = record_image_metadata_and_artifact(&arena, root);
    let crate::view::paint::PaintPayloadIdentity::Image(metadata_identity, decoration) =
        &metadata.payload_identity
    else {
        panic!("Path Ready metadata must use Image identity")
    };
    assert!(decoration.len() <= 2);
    assert_eq!(
        metadata_identity.sampled_texture_id,
        SampledTextureId::Image(asset_id)
    );
    assert_eq!(metadata_identity.generation, generation);
    assert_eq!((metadata_identity.width, metadata_identity.height), (3, 2));
    assert_eq!(metadata_identity.pixel_len, 24);
    assert_eq!(metadata_identity.pixel_ptr, pixels.as_ptr() as usize);
    assert_eq!(
        metadata_identity.sampling,
        crate::view::ImageSampling::Nearest
    );
    assert_eq!(
        metadata_identity.uv_bounds_bits,
        Some([0.5, 0.0, 2.0, 2.0].map(f32::to_bits)),
        "Cover fit mapping must be frozen into metadata identity"
    );

    let prepared = artifact
        .ops
        .iter()
        .find_map(|op| match op {
            crate::view::paint::PaintOp::PreparedImage(op) => Some(op),
            _ => None,
        })
        .expect("Path Ready full artifact upload");
    assert_eq!(
        crate::view::paint::PreparedImageIdentity::from_op(prepared),
        metadata_identity.clone()
    );
    assert!(std::sync::Arc::ptr_eq(&prepared.upload.pixels, &pixels));

    drop(arena);
    crate::view::image_resource::remove_image_entry_for_test(asset_id);
    assert!(prepared.upload.validate_rgba8().is_some());
    assert_eq!(prepared.upload.pixels.as_ref(), &[17_u8; 24]);
}

#[test]
fn path_source_swap_rejects_stale_frozen_snapshot_by_current_handle_identity() {
    let (arena, root, _old_asset_id, _) = prepared_ready_image(
        0x9102,
        path_source("swap-a"),
        2,
        2,
        std::sync::Arc::from([3_u8; 16]),
    );
    {
        let mut node = arena.get_mut(root).unwrap();
        let image = node.element.as_any_mut().downcast_mut::<Image>().unwrap();
        let stale = image.frozen_snapshot.clone().unwrap();
        image.set_source(path_source("swap-b"));
        image.frozen_snapshot = Some(stale);
        image.prepared_by_arena_sync = true;
    }
    let context = image_recording_context(&arena, root);
    let revision = crate::view::paint::PaintContentRevision {
        self_paint_revision: 1,
        composite_revision: 1,
        topology_revision: 1,
    };
    let node = arena.get(root).unwrap();
    assert_eq!(
        node.element
            .shadow_paint_recording_capability(&arena, false, context),
        ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedImage)
    );
    assert!(
        node.element
            .record_shadow_paint_metadata(root, Default::default(), revision, &arena, context,)
            .is_none()
    );
    assert!(
        node.element
            .record_shadow_paint_artifact(root, Default::default(), revision, &arena, context,)
            .is_none()
    );
}

#[test]
fn path_generation_drift_keeps_one_frame_freeze_then_advances_on_sync() {
    let old_pixels: std::sync::Arc<[u8]> = std::sync::Arc::from([5_u8; 16]);
    let (mut arena, root, asset_id, old_generation) =
        prepared_ready_image(0x9103, path_source("generation"), 2, 2, old_pixels.clone());
    let new_pixels: std::sync::Arc<[u8]> = std::sync::Arc::from([9_u8; 24]);
    let new_generation = crate::view::image_resource::replace_ready_image_for_test(
        asset_id,
        3,
        2,
        new_pixels.clone(),
    );
    let (metadata, artifact) = record_image_metadata_and_artifact(&arena, root);
    let crate::view::paint::PaintPayloadIdentity::Image(identity, _) =
        &metadata.payload_identity
    else {
        unreachable!()
    };
    assert_eq!(identity.generation, old_generation);
    assert_eq!(identity.pixel_ptr, old_pixels.as_ptr() as usize);
    let prepared = artifact
        .ops
        .iter()
        .find_map(|op| match op {
            crate::view::paint::PaintOp::PreparedImage(op) => Some(op),
            _ => None,
        })
        .unwrap();
    assert_eq!(prepared.upload.generation, old_generation);

    arena.with_element_taken(root, |element, arena| {
        element
            .as_any_mut()
            .downcast_mut::<Image>()
            .unwrap()
            .sync_arena(arena);
    });
    let (metadata, artifact) = record_image_metadata_and_artifact(&arena, root);
    let crate::view::paint::PaintPayloadIdentity::Image(identity, _) =
        &metadata.payload_identity
    else {
        unreachable!()
    };
    assert_eq!(identity.generation, new_generation);
    assert_eq!((identity.width, identity.height), (3, 2));
    assert_eq!(identity.pixel_ptr, new_pixels.as_ptr() as usize);
    let prepared = artifact
        .ops
        .iter()
        .find_map(|op| match op {
            crate::view::paint::PaintOp::PreparedImage(op) => Some(op),
            _ => None,
        })
        .unwrap();
    assert_eq!(prepared.upload.generation, new_generation);
}
