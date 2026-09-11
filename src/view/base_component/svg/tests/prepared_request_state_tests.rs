use super::*;

#[test]
fn failed_active_svg_promotes_ready_replacement_before_selecting_slot_topology() {
    let mut svg = freeze_ready_svg(0xa308, unique_svg("error-pending-ready-topology"), 1.0);
    let old_key = svg.active_raster_key.unwrap();
    svg.prepare_frozen_paint(PaintResourcePreparationContext {
        frame_number: 3,
        device_scale: 2.0,
        now: Instant::now(),
    });
    let next_key = svg.pending_raster_key.unwrap();
    let next_request = svg.pending_raster_request.unwrap();
    set_svg_raster_error_for_test(old_key);
    set_svg_raster_ready_for_test(
        next_key,
        next_request.physical_width,
        next_request.physical_height,
    );
    let mut arena = new_test_arena();
    let owner = commit_element(&mut arena, Box::new(svg));
    arena.with_element_taken(owner, |element, arena| {
        let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
        svg.sync_arena(arena);
        assert_eq!(svg.active_slot, ActiveSlot::None);
        assert_eq!(svg.active_raster_key, Some(next_key));
        assert!(svg.pending_raster_key.is_none());
        svg.prepare_frozen_paint(PaintResourcePreparationContext {
            frame_number: 4,
            device_scale: 2.0,
            now: Instant::now(),
        });
        assert!(svg.frozen_paint.is_some());
        assert!(svg.has_frozen_ready_request_state());
    });
    let node = arena.get(owner).unwrap();
    let context = node
        .element
        .shadow_paint_recording_context(Default::default());
    let svg = node.element.as_any().downcast_ref::<Svg>().unwrap();
    assert!(
        svg.classify_shadow_paint(&arena, Some(owner), None, false, context)
            .is_ok()
    );
}

#[test]
fn changing_svg_raster_mode_preserves_the_ready_frame_until_replacement() {
    let mut svg = freeze_ready_svg(0xa309, unique_svg("pending-mode-change"), 1.0);
    let active_key = svg.active_raster_key;
    assert_eq!(
        svg.active_raster_request.unwrap().mode,
        SvgRasterMode::Uniform
    );
    svg.set_fit(crate::view::ImageFit::Fill);
    svg.prepare_frozen_paint(PaintResourcePreparationContext {
        frame_number: 3,
        device_scale: 1.0,
        now: Instant::now(),
    });
    assert_eq!(svg.active_slot, ActiveSlot::None);
    assert_eq!(svg.active_raster_key, active_key);
    assert_eq!(
        svg.pending_raster_request.unwrap().mode,
        SvgRasterMode::Fill
    );
    assert!(svg.frozen_paint.is_some());
    assert!(svg.has_frozen_ready_request_state());
    let mut arena = new_test_arena();
    let owner = commit_element(&mut arena, Box::new(svg));
    let node = arena.get(owner).unwrap();
    let context = node
        .element
        .shadow_paint_recording_context(Default::default());
    let svg = node.element.as_any().downcast_ref::<Svg>().unwrap();
    assert!(
        svg.classify_shadow_paint(&arena, Some(owner), None, false, context)
            .is_ok()
    );
}

#[test]
fn first_raster_request_records_loading_without_observing_late_completion() {
    let source = unique_svg("first-request-recorded-loading");
    let document = prime_svg_document_ready_for_test(&source, 80.0, 40.0);
    let mut svg = Svg::new_with_id(0xa301, source);
    layout_svg_element(&mut svg, 80.0, 40.0);
    let mut staging = new_test_arena();
    svg.sync_arena(&mut staging);
    assert!(svg.frozen_active_raster_key.is_none());
    svg.prepare_frozen_paint(PaintResourcePreparationContext {
        frame_number: 1,
        device_scale: 1.0,
        now: Instant::now(),
    });
    let key = svg.active_raster_key.unwrap();
    let request = svg.active_raster_request.unwrap();
    assert_eq!(svg.frozen_document_key, Some(document));
    assert_eq!(svg.frozen_active_raster_key, Some(key));
    assert!(matches!(
        svg.frozen_active_raster,
        Some(ImageSnapshot::Loading)
    ));
    assert_eq!(svg.active_slot, ActiveSlot::Loading);
    // Registry completion after preparation must not change the frame's state.
    let signature = svg.retained_paint_signature();
    set_svg_raster_ready_for_test(key, request.physical_width, request.physical_height);
    assert_eq!(signature, svg.retained_paint_signature());
    let mut arena = new_test_arena();
    let owner = commit_element(&mut arena, Box::new(svg));
    let revision = crate::view::paint::PaintContentRevision {
        self_paint_revision: 1,
        composite_revision: 1,
        topology_revision: 1,
    };
    let node = arena.get(owner).unwrap();
    let context = node
        .element
        .shadow_paint_recording_context(Default::default());
    assert!(
        node.element
            .record_shadow_paint_metadata(owner, Default::default(), revision, &arena, context)
            .is_some()
    );
    assert!(
        node.element
            .record_shadow_paint_artifact(owner, Default::default(), revision, &arena, context)
            .is_some()
    );
    drop(node);
    arena.with_element_taken(owner, |element, arena| {
        let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
        svg.sync_arena(arena);
        svg.prepare_frozen_paint(PaintResourcePreparationContext {
            frame_number: 2,
            device_scale: 1.0,
            now: Instant::now(),
        });
        assert_eq!(svg.active_slot, ActiveSlot::None);
        assert!(svg.frozen_paint.is_some());
        assert!(svg.has_frozen_ready_request_state());
    });
}

#[test]
fn prepared_svg_pending_resolution_is_valid_but_postprepare_drift_is_rejected() {
    for mutate in 0..9 {
        let mut svg = freeze_ready_svg(
            0xa310 + mutate,
            unique_svg(&format!("pending-witness-{mutate}")),
            1.0,
        );
        let previous_key = svg.active_raster_key;
        // Real preparation, not a hand-written authorization flag, creates
        // the pending request and freezes the still-renderable old upload.
        svg.prepare_frozen_paint(PaintResourcePreparationContext {
            frame_number: 3,
            device_scale: 2.0,
            now: Instant::now(),
        });
        assert_ne!(svg.active_raster_request, svg.frozen_desired_request);
        assert!(svg.pending_raster_key.is_some());
        assert_eq!(svg.active_raster_key, previous_key);
        assert!(!svg.frozen_request_is_exact);
        assert!(svg.has_frozen_ready_request_state());
        let mut arena = new_test_arena();
        let owner = commit_element(&mut arena, Box::new(svg));
        {
            let node = arena.get(owner).unwrap();
            let context = node
                .element
                .shadow_paint_recording_context(Default::default());
            let svg = node.element.as_any().downcast_ref::<Svg>().unwrap();
            assert!(
                svg.classify_shadow_paint(&arena, Some(owner), None, false, context)
                    .is_ok()
            );
        }
        arena.with_element_taken(owner, |element, _| {
            let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
            let bogus = Some(SvgRasterRequest::new(1024, 512, SvgRasterMode::Uniform));
            match mutate {
                0 => svg.active_raster_key = svg.active_raster_key.map(|key| key + 10000),
                1 => svg.active_raster_request = bogus,
                2 => svg.active_device_scale_bits = Some(9.0_f32.to_bits()),
                3 => svg.frozen_desired_request = bogus,
                4 => svg.pending_raster_key = svg.pending_raster_key.map(|key| key + 10000),
                5 => svg.pending_raster_request = bogus,
                6 => svg.pending_device_scale_bits = Some(9.0_f32.to_bits()),
                7 => svg.failed_raster_request = bogus,
                8 => svg.frozen_request_is_exact = true,
                _ => unreachable!(),
            }
        });
        assert_missing_prepared_svg_hooks(&arena, owner);
        // Restore lease-bearing keys before drop; the mutation tests validity,
        // not the registry's behavior for releasing an invented key.
        arena.with_element_taken(owner, |element, _| {
            let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
            let state = svg.frozen_request_state.unwrap();
            svg.active_raster_key = state.active_key;
            svg.pending_raster_key = state.pending_key;
        });
    }
}
