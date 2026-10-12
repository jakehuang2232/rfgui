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
    });
    let node = arena.get(owner).unwrap();
    let svg = node.element.as_any().downcast_ref::<Svg>().unwrap();
    assert!(svg.classify_shadow_paint().is_some());
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
    let mut arena = new_test_arena();
    let owner = commit_element(&mut arena, Box::new(svg));
    let node = arena.get(owner).unwrap();
    let svg = node.element.as_any().downcast_ref::<Svg>().unwrap();
    assert!(svg.classify_shadow_paint().is_some());
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
        .shadow_paint_recording_context(&Default::default());
    assert!(
        node.element
            .record_shadow_paint_metadata(owner, Default::default(), revision, &arena, &context)
            .is_some()
    );
    assert!(
        node.element
            .record_shadow_paint_artifact(owner, Default::default(), revision, &arena, &context)
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
    });
}
