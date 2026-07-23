use super::*;

#[test]
fn document_intrinsic_transition_marks_layout_while_slot_remains_loading() {
    let mut svg = Svg::new_with_id(64, simple_svg());
    svg.frozen_document = Some(SvgDocumentSnapshot::Loading);
    svg.frozen_active_raster = None;
    svg.active_slot = super::super::ActiveSlot::Loading;
    wait_until_document_ready(svg.source_key);
    svg.clear_local_dirty_flags(DirtyFlags::ALL);
    let mut arena = new_test_arena();

    svg.sync_arena(&mut arena);

    assert_eq!(svg.active_slot, super::super::ActiveSlot::Loading);
    assert!(svg.local_dirty_flags().contains(DirtyFlags::LAYOUT));
    svg.measure(
        LayoutConstraints {
            max_width: 500.0,
            max_height: 500.0,
            viewport_width: 500.0,
            viewport_height: 500.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        &mut arena,
    );
    assert_eq!(svg.measured_size(), (80.0, 40.0));
}

#[test]
fn path_source_request_and_device_scale_drift_fail_closed() {
    let mut stale = freeze_ready_svg(0x6b01, SvgSource::Path("stale-source-a.svg".into()), 1.0);
    let stale_document_key = stale.source_key;
    let stale_raster_key = stale.active_raster_key.expect("stale raster key");
    let stale_request = stale.active_raster_request.expect("stale request");
    let stale_paint = stale.frozen_paint.clone();
    let next_source = SvgSource::Path("stale-source-b-m9b2.svg".into());
    let next_document_key = prime_svg_document_ready_for_test(&next_source, 80.0, 40.0);
    stale.set_source(next_source);
    stale.frozen_document_key = Some(stale_document_key);
    stale.frozen_document = Some(SvgDocumentSnapshot::Ready {
        intrinsic_width: 80.0,
        intrinsic_height: 40.0,
    });
    stale.active_raster_key = Some(stale_raster_key);
    stale.active_raster_request = Some(stale_request);
    stale.active_device_scale_bits = Some(1.0_f32.to_bits());
    stale.frozen_desired_request = Some(stale_request);
    stale.frozen_active_raster = stale_paint.as_ref().map(|paint| {
        ImageSnapshot::Ready(ReadyImage {
            sampled_texture_id: paint.upload.id,
            width: paint.upload.width,
            height: paint.upload.height,
            pixels: paint.upload.pixels.clone(),
            generation: paint.upload.generation,
        })
    });
    stale.frozen_paint = stale_paint;
    stale.frozen_request_is_exact = true;
    stale.active_slot = super::super::ActiveSlot::None;
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(stale));
    assert_eq!(
        arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_capability(&arena, false, Default::default()),
        ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedSvg)
    );
    drop(arena);
    remove_svg_raster_entry_for_test(stale_raster_key);
    remove_svg_document_entry_for_test(stale_document_key);
    remove_svg_document_entry_for_test(next_document_key);

    for (id, mutate) in [(0x6b02, 0_u8), (0x6b03, 1_u8), (0x6b04, 2_u8)] {
        let mut svg = freeze_ready_svg(
            id,
            SvgSource::Path(format!("authority-drift-{id}.svg").into()),
            1.0,
        );
        match mutate {
            0 => {
                svg.active_raster_request =
                    Some(SvgRasterRequest::new(160, 80, SvgRasterMode::Uniform));
            }
            1 => svg.active_device_scale_bits = Some(2.0_f32.to_bits()),
            2 => {
                svg.pending_raster_request =
                    Some(SvgRasterRequest::new(160, 80, SvgRasterMode::Uniform));
            }
            _ => unreachable!(),
        }
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(svg));
        assert_eq!(
            arena
                .get(root)
                .unwrap()
                .element
                .shadow_paint_recording_capability(&arena, false, Default::default()),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedSvg)
        );
    }
}

#[test]
fn normalized_equivalent_path_source_keeps_document_and_raster_state() {
    let relative = std::path::PathBuf::from("target/nonexistent-equivalent.svg");
    let absolute = std::env::current_dir().unwrap().join(&relative);
    let mut svg = Svg::new_with_id(8, SvgSource::Path(relative));
    let document_key = svg.source_key;
    let marker = SvgRasterRequest::new(96, 48, SvgRasterMode::Uniform);
    svg.active_raster_request = Some(marker);
    svg.clear_local_dirty_flags(crate::view::base_component::DirtyFlags::ALL);

    svg.set_source(SvgSource::Path(absolute));

    assert_eq!(svg.source_key, document_key);
    assert_eq!(svg.active_raster_request, Some(marker));
    assert!(svg.local_dirty_flags().is_empty());
}

#[test]
fn returning_to_active_request_cancels_pending_lease() {
    let mut svg = Svg::new_with_id(9, unique_svg("pending-cancel"));
    let active_request = SvgRasterRequest::new(96, 48, SvgRasterMode::Uniform);
    let pending_request = SvgRasterRequest::new(128, 64, SvgRasterMode::Uniform);
    let active_key = acquire_svg_raster(svg.source_key, active_request);
    let pending_key = acquire_svg_raster(svg.source_key, pending_request);
    svg.active_raster_key = Some(active_key);
    svg.active_raster_request = Some(active_request);
    svg.active_device_scale_bits = Some(1.0_f32.to_bits());
    svg.pending_raster_key = Some(pending_key);
    svg.pending_raster_request = Some(pending_request);
    svg.pending_device_scale_bits = Some(1.0_f32.to_bits());
    assert_eq!(svg_raster_ref_count_for_test(pending_key), Some(1));

    assert_eq!(
        svg.sync_raster_key(active_request, 1.0, Instant::now()),
        Some(active_key)
    );
    assert_eq!(svg.pending_raster_key, None);
    assert_eq!(svg_raster_ref_count_for_test(pending_key), Some(0));
}

#[test]
fn failed_pending_request_is_memoized_until_request_identity_changes() {
    let mut svg = Svg::new_with_id(10, unique_svg("failed-memo"));
    let active_request = SvgRasterRequest::new(96, 48, SvgRasterMode::Uniform);
    let failed_request = SvgRasterRequest::new(128, 64, SvgRasterMode::Uniform);
    let active_key = acquire_svg_raster(svg.source_key, active_request);
    let failed_key = acquire_svg_raster(svg.source_key, failed_request);
    svg.active_raster_key = Some(active_key);
    svg.active_raster_request = Some(active_request);
    svg.active_device_scale_bits = Some(1.0_f32.to_bits());
    svg.pending_raster_key = Some(failed_key);
    svg.pending_raster_request = Some(failed_request);
    svg.pending_device_scale_bits = Some(1.0_f32.to_bits());
    set_svg_raster_error_for_test(failed_key);

    assert_eq!(
        svg.sync_raster_key(failed_request, 1.0, Instant::now()),
        Some(active_key)
    );
    assert_eq!(svg.failed_raster_request, Some(failed_request));
    assert_eq!(svg_raster_ref_count_for_test(failed_key), Some(0));
    for _ in 0..3 {
        assert_eq!(
            svg.sync_raster_key(failed_request, 1.0, Instant::now()),
            Some(active_key)
        );
        assert_eq!(svg.pending_raster_key, None);
        assert_eq!(svg_raster_ref_count_for_test(failed_key), Some(0));
    }

    let changed_request = SvgRasterRequest::new(160, 80, SvgRasterMode::Uniform);
    assert_eq!(
        svg.sync_raster_key(changed_request, 1.0, Instant::now()),
        Some(active_key)
    );
    assert_eq!(svg.failed_raster_request, None);
    assert_eq!(svg.pending_raster_request, Some(changed_request));
}

#[test]
fn pending_readiness_is_invisible_until_next_prelayout_freeze_then_swaps() {
    let mut svg = Svg::new_with_id(11, unique_svg("raster-pending"));
    wait_until_document_ready(svg.source_key);
    let active_request = SvgRasterRequest::new(96, 48, SvgRasterMode::Uniform);
    let pending_request = SvgRasterRequest::new(128, 64, SvgRasterMode::Uniform);
    let active_key = acquire_svg_raster(svg.source_key, active_request);
    let pending_key = acquire_svg_raster(svg.source_key, pending_request);
    wait_until_raster_ready(active_key);
    wait_until_raster_ready(pending_key);
    set_svg_raster_loading_for_test(pending_key);
    svg.active_raster_key = Some(active_key);
    svg.active_raster_request = Some(active_request);
    svg.active_device_scale_bits = Some(1.0_f32.to_bits());
    svg.pending_raster_key = Some(pending_key);
    svg.pending_raster_request = Some(pending_request);
    svg.pending_device_scale_bits = Some(1.0_f32.to_bits());

    let mut arena = new_test_arena();
    svg.sync_arena(&mut arena);
    let loading = svg.retained_paint_signature();
    assert_eq!(loading, svg.retained_paint_signature());
    set_svg_raster_ready_for_test(
        pending_key,
        pending_request.physical_width,
        pending_request.physical_height,
    );
    let ready = svg.retained_paint_signature();
    assert_eq!(ready, loading);
    svg.sync_arena(&mut arena);
    let next_frame_ready = svg.retained_paint_signature();
    assert_ne!(next_frame_ready, loading);
    assert_eq!(
        svg.sync_raster_key(pending_request, 1.0, Instant::now()),
        Some(pending_key)
    );
    assert_eq!(svg.active_raster_request, Some(pending_request));
    assert_eq!(svg.pending_raster_key, None);
    assert_eq!(svg_raster_ref_count_for_test(active_key), Some(0));
}
