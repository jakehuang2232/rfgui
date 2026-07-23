use super::*;

#[test]
fn resize_cooldown_only_keeps_safe_oversampled_shrink() {
    let mut svg = Svg::new_with_id(2, simple_svg());
    svg.active_raster_request = Some(SvgRasterRequest::new(128, 64, SvgRasterMode::Uniform));
    svg.active_device_scale_bits = Some(1.0_f32.to_bits());
    svg.last_raster_request_at = Some(Instant::now());
    assert!(svg.should_keep_existing_raster(
        SvgRasterRequest::new(96, 48, SvgRasterMode::Uniform),
        1.0,
        Instant::now()
    ));
    assert!(!svg.should_keep_existing_raster(
        SvgRasterRequest::new(160, 80, SvgRasterMode::Uniform),
        1.0,
        Instant::now()
    ));
}

#[test]
fn expired_cooldown_or_device_scale_change_requests_new_raster() {
    let mut svg = Svg::new_with_id(3, simple_svg());
    svg.active_raster_request = Some(SvgRasterRequest::new(128, 64, SvgRasterMode::Uniform));
    svg.active_device_scale_bits = Some(1.0_f32.to_bits());
    svg.last_raster_request_at = Some(Instant::now() - Duration::from_millis(200));
    assert!(!svg.should_keep_existing_raster(
        SvgRasterRequest::new(96, 48, SvgRasterMode::Uniform),
        1.0,
        Instant::now()
    ));
    svg.last_raster_request_at = Some(Instant::now());
    assert!(!svg.should_keep_existing_raster(
        SvgRasterRequest::new(96, 48, SvgRasterMode::Uniform),
        2.0,
        Instant::now()
    ));
}

#[test]
fn intrinsic_mapping_is_independent_of_bucket_backing_for_all_fit_modes() {
    let mut svg = Svg::new_with_id(4, simple_svg());
    let contain = svg
        .resolve_raster_plan(80.0, 40.0, 100.0, 100.0, 1.0)
        .unwrap();
    assert_eq!(contain.request.physical_width, 128);
    assert_eq!(contain.request.physical_height, 64);
    assert_eq!(contain.local_draw_bounds, [0.0, 25.0, 100.0, 50.0]);
    assert_eq!(contain.uv_bounds, [0.0, 0.0, 128.0, 64.0]);

    svg.set_fit(crate::view::ImageFit::Cover);
    let cover = svg
        .resolve_raster_plan(80.0, 40.0, 100.0, 100.0, 1.0)
        .unwrap();
    assert_eq!(
        (cover.request.physical_width, cover.request.physical_height),
        (224, 112)
    );
    assert_eq!(cover.local_draw_bounds, [0.0, 0.0, 100.0, 100.0]);
    assert_eq!(cover.uv_bounds, [56.0, 0.0, 112.0, 112.0]);

    svg.set_fit(crate::view::ImageFit::Fill);
    let fill = svg
        .resolve_raster_plan(80.0, 40.0, 100.0, 100.0, 1.0)
        .unwrap();
    assert_eq!(
        (fill.request.physical_width, fill.request.physical_height),
        (128, 128)
    );
    assert_eq!(fill.local_draw_bounds, [0.0, 0.0, 100.0, 100.0]);
    assert_eq!(fill.uv_bounds, [0.0, 0.0, 128.0, 128.0]);
}

#[test]
fn uniform_uv_uses_actual_resvg_scale_not_padded_backing_axis() {
    let svg = Svg::new_with_id(5, simple_svg());
    let wide = svg
        .resolve_raster_plan(101.0, 37.0, 101.0, 37.0, 1.0)
        .unwrap();
    assert_eq!(
        (wide.request.physical_width, wide.request.physical_height),
        (128, 47)
    );
    assert_eq!(wide.uv_bounds[2], 128.0);
    assert!(wide.uv_bounds[3] < 47.0);

    let tall = svg
        .resolve_raster_plan(37.0, 101.0, 37.0, 101.0, 1.0)
        .unwrap();
    assert_eq!(
        (tall.request.physical_width, tall.request.physical_height),
        (47, 128)
    );
    assert!(tall.uv_bounds[2] < 47.0);
    assert_eq!(tall.uv_bounds[3], 128.0);
}

#[test]
fn viewport_scale_changes_physical_extent_without_changing_logical_mapping() {
    let svg = Svg::new_with_id(6, simple_svg());
    assert!(
        svg.resolve_raster_plan(80.0, 40.0, 80.0, 40.0, f32::NAN)
            .is_none()
    );
    assert!(
        svg.resolve_raster_plan(80.0, 40.0, 80.0, 40.0, f32::INFINITY)
            .is_none()
    );
    assert!(
        svg.resolve_raster_plan(80.0, 40.0, 80.0, 40.0, 0.0)
            .is_none()
    );
    let one = svg
        .resolve_raster_plan(80.0, 40.0, 80.0, 40.0, 1.0)
        .unwrap();
    let two = svg
        .resolve_raster_plan(80.0, 40.0, 80.0, 40.0, 2.0)
        .unwrap();
    assert_eq!(one.local_draw_bounds, two.local_draw_bounds);
    assert_eq!(
        (one.request.physical_width, one.request.physical_height),
        (96, 48)
    );
    assert_eq!(
        (two.request.physical_width, two.request.physical_height),
        (160, 80)
    );
}

#[test]
fn prepared_svg_payload_owns_straight_srgb_upload_and_intrinsic_mapping() {
    let mut svg = Svg::new_with_id(61, simple_svg());
    let plan = svg
        .resolve_raster_plan(80.0, 40.0, 80.0, 40.0, 1.0)
        .unwrap();
    let image = ReadyImage {
        sampled_texture_id: SampledTextureId::SvgRaster(SvgRasterAssetId::for_test(61)),
        width: plan.request.physical_width,
        height: plan.request.physical_height,
        pixels: std::sync::Arc::from(vec![
            0_u8;
            (plan.request.physical_width * plan.request.physical_height * 4)
                as usize
        ]),
        generation: 1,
    };
    let upload = svg.upload_for_image(&image).unwrap();
    svg.frozen_paint = Some(FrozenSvgPaint {
        document_key: svg.source_key,
        raster_key: 0,
        device_scale_bits: 1.0_f32.to_bits(),
        plan,
        inner_origin: [10.0, 20.0],
        upload,
        opacity: 0.75,
    });
    let prepared = svg.prepared_svg_op([0.25, -0.5], 0.75).unwrap();
    assert_eq!(
        prepared.upload.alpha_mode,
        crate::view::sampled_texture::SampledTextureAlphaMode::Straight
    );
    assert!(!prepared.params.source_is_premultiplied);
    assert_eq!(prepared.params.bounds, [10.25, 19.5, 80.0, 40.0]);
    assert_eq!(prepared.params.uv_bounds, Some([0.0, 0.0, 96.0, 48.0]));
}

#[test]
fn postlayout_prepare_uses_final_bounds_once_and_refreshes_same_bucket_scale_identity() {
    let mut svg = freeze_ready_svg(62, simple_svg(), 1.0);
    let first = svg
        .resolve_raster_plan(80.0, 40.0, 80.0, 40.0, 1.0)
        .unwrap();
    let same_bucket = svg
        .resolve_raster_plan(80.0, 40.0, 80.0, 40.0, 1.01)
        .unwrap();
    assert_eq!(same_bucket.request, first.request);
    svg.prepare_frozen_paint(PaintResourcePreparationContext {
        frame_number: 10,
        device_scale: 1.01,
        now: Instant::now(),
    });
    assert!(
        svg.frozen_request_is_exact,
        "kind={:?} active={:?} desired={:?} scale={:?} pending={:?} paint={} slot={:?}",
        svg.source_kind,
        svg.active_raster_request,
        svg.frozen_desired_request,
        svg.active_device_scale_bits,
        svg.pending_raster_request,
        svg.frozen_paint.is_some(),
        svg.active_slot,
    );
    assert_eq!(svg.active_device_scale_bits, Some(1.01_f32.to_bits()));
    assert_eq!(svg.frozen_desired_request, Some(same_bucket.request));
    let frozen_bounds = svg.frozen_paint.as_ref().unwrap().plan.local_draw_bounds;

    layout_svg_element(&mut svg, 240.0, 120.0);
    svg.prepare_frozen_paint(PaintResourcePreparationContext {
        frame_number: 10,
        device_scale: 2.0,
        now: Instant::now(),
    });
    assert_eq!(
        svg.frozen_paint.as_ref().unwrap().plan.local_draw_bounds,
        frozen_bounds,
        "same-frame prepare must not refreeze after final layout"
    );
    assert_eq!(svg.frozen_desired_request, Some(same_bucket.request));
}

#[test]
fn ready_resource_with_loading_slot_topology_stays_unprepared_until_next_prelayout() {
    let mut svg = freeze_ready_svg(63, simple_svg(), 1.0);
    svg.active_slot = super::super::ActiveSlot::Loading;
    svg.prepared_frame_number = None;
    svg.prepare_frozen_paint(PaintResourcePreparationContext {
        frame_number: 2,
        device_scale: 1.0,
        now: Instant::now(),
    });
    assert!(svg.frozen_paint.is_none());
    assert!(!svg.frozen_request_is_exact);
}
