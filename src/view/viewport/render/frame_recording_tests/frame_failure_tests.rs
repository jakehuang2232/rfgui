use super::*;

#[test]
fn compile_or_execute_failure_aborts_the_frame() {
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
    assert_eq!(frame_disposition(false, false), FrameDisposition::Abort);
    // `execute_profiled` reports this state after stopping at a failed
    // execute step; preceding steps may already have recorded commands.
    assert_eq!(frame_disposition(true, false), FrameDisposition::Abort);
    assert!(!should_store_compile_cache(true, false));
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
    let instance =
        <crate::view::render_pass::draw_rect_pass::RectInstance as bytemuck::Zeroable>::zeroed();
    assert_eq!(
        viewport.push_rect_instance(instance),
        0,
        "fixture must stage a rect instance"
    );
    assert!(viewport.flush_rect_instance_uploads());
    let (_, _, uploaded_buffer) = viewport.rect_instance_stream_for_test();
    // Abort must discard both recorded copies and instances still waiting for
    // a prepare flush. Neither may be replayed into the next frame.
    assert_eq!(viewport.push_rect_instance(instance), 1);
    assert_eq!(viewport.rect_instance_stream_for_test().1, 1);
    assert!(viewport.gpu.upload_staging_belt.is_some());

    let profile = viewport.complete_frame(FrameDisposition::Abort);
    assert!(viewport.frame.frame_state.is_none());
    assert_eq!(viewport.frame_completion_counts_for_test(), (0, 0, 1));
    assert!(!viewport.frame.frame_presented);
    assert!(viewport.gpu.upload_staging_belt.is_none());
    assert_eq!(profile.submit_ms, 0.0);
    assert_eq!(profile.present_ms, 0.0);
    assert_eq!(profile.encoder_finish_ms, 0.0);
    assert_eq!(profile.gpu_wait_ms, 0.0);
    assert!(!profile.gpu_waited);
    assert!(profile.abort_cleanup_ms.is_finite() && profile.abort_cleanup_ms >= 0.0);

    viewport.begin_offscreen_test_frame(
        device.clone(),
        queue.clone(),
        4,
        4,
        wgpu::TextureFormat::Rgba8Unorm,
    )?;
    // The next frame restarts instance indices and re-uploads from zero into
    // the retained buffer; nothing staged before the abort survives.
    let (staged, uploaded, buffer) = viewport.rect_instance_stream_for_test();
    assert_eq!((staged, uploaded), (0, 0));
    assert_eq!(buffer, uploaded_buffer);
    assert_eq!(viewport.push_rect_instance(instance), 0);
    assert!(viewport.gpu.upload_staging_belt.is_none());
    assert!(
        viewport.flush_rect_instance_uploads(),
        "the frame after abort must lazily recreate the staging belt"
    );
    assert!(viewport.gpu.upload_staging_belt.is_some());
    assert_eq!(viewport.rect_instance_stream_for_test().1, 1);
    viewport.end_offscreen_test_frame()?;
    assert!(viewport.frame.frame_state.is_none());
    assert_eq!(viewport.frame_completion_counts_for_test(), (1, 0, 1));
    // Aborting must not count as an in-flight submission. The third real
    // submission reaches the existing bound, and its wait gets its own timer.
    for expected_wait in [false, true] {
        viewport.begin_offscreen_test_frame(
            device.clone(),
            queue.clone(),
            4,
            4,
            wgpu::TextureFormat::Rgba8Unorm,
        )?;
        let started_at = crate::time::Instant::now();
        let profile = viewport.complete_frame(FrameDisposition::SubmitAndPresent);
        let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
        let phases = [
            profile.staging_finish_ms,
            profile.encoder_finish_ms,
            profile.submit_ms,
            profile.resource_cleanup_ms,
            profile.present_ms,
            profile.gpu_wait_ms,
        ];
        assert!(phases.iter().all(|ms| ms.is_finite() && *ms >= 0.0));
        assert!(phases.iter().sum::<f64>() <= elapsed_ms);
        assert_eq!(profile.abort_cleanup_ms, 0.0);
        assert_eq!(profile.gpu_waited, expected_wait);
        if !expected_wait {
            assert_eq!(profile.gpu_wait_ms, 0.0);
        }
    }
    assert_eq!(viewport.frame_completion_counts_for_test(), (3, 0, 1));
    Ok(())
}
