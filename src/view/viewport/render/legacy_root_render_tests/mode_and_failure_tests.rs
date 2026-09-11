use super::*;

#[test]
fn retained_auto_is_default_and_legacy_remains_explicit() {
    assert_eq!(
        Viewport::new().paint_renderer_mode(),
        ViewportPaintRendererMode::RetainedAuto
    );
    let (arena, roots) = prepared_transform_leaf();
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    assert_eq!(
        auto_authority_kind(&auto_decision(&arena, &roots, &ctx)),
        AutoAuthorityKind::Artifact
    );
    let mut legacy = Viewport::new();
    legacy.set_paint_renderer_mode(ViewportPaintRendererMode::Legacy);
    assert_eq!(
        legacy.paint_renderer_mode(),
        ViewportPaintRendererMode::Legacy
    );
}

#[test]
fn retained_auto_terminal_failure_outcome_is_typed_and_named_modes_do_not_arm() {
    assert_eq!(
        terminal_failure_stage(false, false),
        Some(RetainedAutoTerminalFailureStage::Compile)
    );
    assert_eq!(
        terminal_failure_stage(true, false),
        Some(RetainedAutoTerminalFailureStage::Execute)
    );
    assert_eq!(terminal_failure_stage(true, true), None);
    assert_eq!(frame_disposition(false, false), FrameDisposition::Abort);
    assert_eq!(frame_disposition(true, false), FrameDisposition::Abort);
    assert_eq!(
        frame_disposition(true, true),
        FrameDisposition::SubmitAndPresent
    );
    assert!(!should_store_compile_cache(false, false));
    assert!(!should_store_compile_cache(true, false));
    assert!(should_store_compile_cache(true, true));

    for mode in [ViewportPaintRendererMode::Legacy] {
        let mut viewport = Viewport::new();
        viewport.set_paint_renderer_mode(mode);
        viewport.take_redraw_request();
        assert!(
            !viewport.arm_retained_auto_terminal_failure(RetainedAutoTerminalFailureStage::Compile)
        );
        assert_eq!(viewport.retained_auto_terminal_failure, None);
        assert!(!viewport.take_redraw_request());
    }
}

#[test]
fn compile_terminal_failure_chooses_abort_completion() {
    let mut graph = FrameGraph::new();
    let desc = crate::view::frame_graph::TextureDesc::new(
        1,
        1,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureDimension::D2,
    );
    let duplicate_key = crate::view::frame_graph::PersistentTextureKey::Generic(0xab07);
    graph.declare_persistent_texture_internal::<()>(desc.clone(), duplicate_key);
    graph.declare_persistent_texture_internal::<()>(desc, duplicate_key);
    assert!(
        graph.compile().is_err(),
        "duplicate persistent keys are a compile-terminal fixture"
    );
    assert_eq!(
        terminal_failure_stage(false, false),
        Some(RetainedAutoTerminalFailureStage::Compile)
    );
    assert_eq!(frame_disposition(false, false), FrameDisposition::Abort);
}

#[test]
fn partial_execute_terminal_failure_chooses_abort_completion() {
    // `execute_profiled` reports this state after stopping at a failed
    // execute step; preceding steps may already have recorded commands.
    assert_eq!(
        terminal_failure_stage(true, false),
        Some(RetainedAutoTerminalFailureStage::Execute)
    );
    assert_eq!(frame_disposition(true, false), FrameDisposition::Abort);
}

#[test]
#[ignore = "requires a native GPU adapter"]
fn abort_frame_discards_encoder_resets_staging_and_next_frame_submits() -> Result<(), String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        flags: wgpu::InstanceFlags::empty(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .map_err(|error| format!("abort-frame test requires a GPU adapter: {error:?}"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("rfgui abort-frame test device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        experimental_features: wgpu::ExperimentalFeatures::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
    }))
    .map_err(|error| format!("failed to create abort-frame test device: {error:?}"))?;

    let mut viewport = Viewport::new();
    viewport.begin_offscreen_test_frame(
        device.clone(),
        queue.clone(),
        4,
        4,
        wgpu::TextureFormat::Rgba8Unorm,
    )?;
    assert!(viewport.frame.frame_state.is_some());
    assert!(
        viewport
            .upload_draw_rect_uniform(&[1, 2, 3, 4], 256, 256)
            .is_some(),
        "fixture must record a native staging-belt copy"
    );
    assert!(viewport.gpu.upload_staging_belt.is_some());

    let profile = viewport.complete_frame(FrameDisposition::Abort);
    assert!(viewport.frame.frame_state.is_none());
    assert_eq!(viewport.frame_completion_counts_for_test(), (0, 0, 1));
    assert!(!viewport.frame.frame_presented);
    assert!(viewport.gpu.upload_staging_belt.is_none());
    assert_eq!(profile.submit_ms, 0.0);
    assert_eq!(profile.present_ms, 0.0);

    viewport.begin_offscreen_test_frame(device, queue, 4, 4, wgpu::TextureFormat::Rgba8Unorm)?;
    assert!(
        viewport
            .upload_draw_rect_uniform(&[5, 6, 7, 8], 256, 256)
            .is_some(),
        "the frame after abort must lazily recreate the staging belt"
    );
    assert!(viewport.gpu.upload_staging_belt.is_some());
    viewport.end_offscreen_test_frame()?;
    assert!(viewport.frame.frame_state.is_none());
    assert_eq!(viewport.frame_completion_counts_for_test(), (1, 0, 1));
    Ok(())
}

#[test]
fn retained_auto_terminal_failure_latches_once_and_same_mode_setter_resets_it() {
    let mut viewport = Viewport::new();
    viewport.set_paint_renderer_mode(ViewportPaintRendererMode::Legacy);
    assert!(viewport.take_redraw_request());
    viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedAuto);
    assert!(viewport.take_redraw_request());

    let owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("fresh viewport owns the retained transaction stage");
    assert!(viewport.stage_retained_surface_clear());
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, None)
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test().1,
        None
    );

    assert!(viewport.arm_retained_auto_terminal_failure(RetainedAutoTerminalFailureStage::Compile));
    assert_eq!(
        viewport.paint_renderer_mode(),
        ViewportPaintRendererMode::RetainedAuto,
        "the public getter keeps the requested mode"
    );
    assert_eq!(
        viewport.retained_auto_terminal_failure,
        Some(RetainedAutoTerminalFailureStage::Compile)
    );
    assert!(viewport.take_redraw_request());

    assert!(
        !viewport.arm_retained_auto_terminal_failure(RetainedAutoTerminalFailureStage::Execute)
    );
    assert_eq!(
        viewport.retained_auto_terminal_failure,
        Some(RetainedAutoTerminalFailureStage::Compile),
        "the first terminal stage remains authoritative"
    );
    assert!(
        !viewport.take_redraw_request(),
        "an open breaker cannot spin redraws"
    );

    assert_eq!(terminal_failure_stage(true, true), None);
    assert_eq!(
        viewport.retained_auto_terminal_failure,
        Some(RetainedAutoTerminalFailureStage::Compile),
        "a successful Legacy recovery does not half-open automatically"
    );

    seed_empty_compile_cache(&mut viewport);
    assert!(viewport.frame.compile_cache.is_some());
    viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedAuto);
    assert_eq!(viewport.retained_auto_terminal_failure, None);
    assert!(
        viewport.frame.compile_cache.is_none(),
        "manual circuit reset must discard the failed-frame topology cache"
    );
    assert!(viewport.take_redraw_request());
    viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedAuto);
    assert!(
        !viewport.take_redraw_request(),
        "ordinary same-mode set stays idempotent"
    );

    assert!(viewport.arm_retained_auto_terminal_failure(RetainedAutoTerminalFailureStage::Execute));
    viewport.take_redraw_request();
    seed_empty_compile_cache(&mut viewport);
    assert!(viewport.frame.compile_cache.is_some());
    viewport.set_paint_renderer_mode(ViewportPaintRendererMode::Legacy);
    assert_eq!(viewport.retained_auto_terminal_failure, None);
    assert!(
        viewport.frame.compile_cache.is_none(),
        "paint mode switches must discard the prior mode's topology cache"
    );
    assert!(viewport.take_redraw_request());
}

#[test]
fn retained_auto_open_breaker_forces_auto_legacy_with_capture_invariant_telemetry() {
    let (arena, roots) = prepared_transform_leaf();
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    assert!(matches!(
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true),
        super::super::RetainedAutoDecision::Artifact { .. }
    ));

    for capture_trace in [false, true] {
        let Some(FramePaintSelection::Auto(super::super::RetainedAutoDecision::Legacy { trace })) =
            retained_auto_circuit_breaker_selection(
                Some(RetainedAutoTerminalFailureStage::Execute),
                capture_trace,
            )
        else {
            panic!("an open breaker must bypass retained planning as AutoLegacy")
        };
        assert_eq!(trace.capture_rejections, capture_trace);
        assert!(trace.rejections.is_empty());

        let selection = FramePaintSelection::AutoLegacy;
        let mut telemetry = PaintAuthorityTelemetry::from_selection(
            ViewportPaintRendererMode::RetainedAuto,
            &selection,
            Some((AutoAuthorityKind::Legacy, trace)),
        );
        telemetry.note_legacy_fallback(retained_auto_terminal_fallback_stage(
            RetainedAutoTerminalFailureStage::Execute,
        ));
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.authority_label, "retained-auto:legacy");
        assert_eq!(snapshot.selected, PaintAuthorityKind::Legacy);
        assert_eq!(
            snapshot.legacy_fallback_stage,
            Some(PaintAuthorityFallbackStage::Execute)
        );
    }
}

#[test]
fn viewport_paint_renderer_defaults_to_auto_and_supports_a_legacy_round_trip() {
    let mut viewport = Viewport::new();
    assert_eq!(
        viewport.paint_renderer_mode(),
        ViewportPaintRendererMode::RetainedAuto
    );
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        viewport.set_paint_renderer_mode(mode);
        assert_eq!(viewport.paint_renderer_mode(), mode);
    }
}
