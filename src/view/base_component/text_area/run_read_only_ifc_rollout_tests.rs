use super::*;
use crate::view::base_component::text::{Text, TextReadOnlyIfcStagingMode};
use crate::view::base_component::{
    LayoutConstraints, LayoutPlacement, Layoutable, Renderable, UiBuildContext,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::node_arena::NodeArena;

#[test]
fn text_read_only_ifc_staging_stays_separate_from_text_area_prepared_default() {
    let mut run = TextAreaTextRun::new("text area run uses prepared default".to_string(), 0..35);
    let mut arena = NodeArena::new();
    run.measure(
        LayoutConstraints {
            max_width: 180.0,
            max_height: 120.0,
            viewport_width: 180.0,
            viewport_height: 120.0,
            percent_base_width: Some(180.0),
            percent_base_height: Some(120.0),
        },
        &mut arena,
    );
    run.place(
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 180.0,
            available_height: 120.0,
            viewport_width: 180.0,
            viewport_height: 120.0,
            percent_base_width: Some(180.0),
            percent_base_height: Some(120.0),
        },
        &mut arena,
    );

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(180, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    run.build(&mut graph, &mut arena, ctx);
    let pass_names = graph
        .pass_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name.to_string())
        .collect::<Vec<_>>();

    assert_eq!(pass_names.len(), 1);
    assert!(
        pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
        "TextAreaTextRun should use the prepared default independently from read-only Text staging, got {}",
        pass_names[0]
    );
}

fn place_run_for_inline_ifc_staging_test(
    run: &mut TextAreaTextRun,
    arena: &mut NodeArena,
    width: f32,
) {
    run.measure(
        LayoutConstraints {
            max_width: width,
            max_height: 120.0,
            viewport_width: width,
            viewport_height: 120.0,
            percent_base_width: Some(width),
            percent_base_height: Some(120.0),
        },
        arena,
    );
    run.place(
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: 120.0,
            viewport_width: width,
            viewport_height: 120.0,
            percent_base_width: Some(width),
            percent_base_height: Some(120.0),
        },
        arena,
    );
}

fn build_text_area_run_pass_names(
    run: &mut TextAreaTextRun,
    arena: &mut NodeArena,
    width: u32,
    height: u32,
) -> Vec<String> {
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(width, height, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    run.build(&mut graph, arena, ctx);
    graph
        .pass_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name.to_string())
        .collect::<Vec<_>>()
}

fn text_area_inline_ifc_ready_behavior_path_status() -> TextAreaEditableIfcBehaviorPathStatus {
    let mut run = TextAreaTextRun::new(
        "behavior path status observes readiness inputs".to_string(),
        0..63,
    );
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 18,
        preedit_text: "入力".to_string(),
        preedit_cursor: Some((1, 2)),
    }));
    run.set_preedit_run(true, Some((1, 3)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let mut payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose behavior path status metadata");
    payload
        .readiness
        .projection_diagnostic
        .projection_segment_count = 1;
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );
    let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcImeBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
        TextAreaEditableIfcCaretAffinityMetadataSourceInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    TextAreaEditableIfcBehaviorPathStatus::evaluate(
        TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
            &preflight,
            &audit,
            &ime_prewire,
            &caret_metadata_source,
            &projection_prewire,
            &scroll_prewire,
        ),
    )
}

fn text_area_inline_ifc_behavior_status_and_caret_metadata_source_for_readiness() -> (
    TextAreaEditableIfcBehaviorPathStatus,
    TextAreaEditableIfcCaretAffinityMetadataSource,
) {
    let mut run = TextAreaTextRun::new(
        "behavior readiness observes caret affinity candidate metadata".to_string(),
        0..63,
    );
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 12,
        preedit_text: "入力".to_string(),
        preedit_cursor: Some((1, 2)),
    }));
    run.set_preedit_run(true, Some((1, 3)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let mut payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose behavior readiness metadata");
    payload
        .readiness
        .projection_diagnostic
        .projection_segment_count = 1;
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );
    let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcImeBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
        TextAreaEditableIfcCaretAffinityMetadataSourceInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let behavior_status = TextAreaEditableIfcBehaviorPathStatus::evaluate(
        TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
            &preflight,
            &audit,
            &ime_prewire,
            &caret_metadata_source,
            &projection_prewire,
            &scroll_prewire,
        ),
    );

    (behavior_status, caret_metadata_source)
}

#[test]
fn text_area_inline_ifc_staging_payload_captures_diagnostic_without_render_enablement() {
    let mut run = TextAreaTextRun::new(
        "editable text area run diagnostic payload wraps".to_string(),
        4..52,
    );
    run.cascade_style(
        vec!["Arial".to_string()],
        16.0,
        1.3,
        crate::style::VerticalAlign::Baseline,
        500,
        crate::style::Color::rgba(33, 44, 55, 255),
        Cursor::Text,
        true,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 92.0);

    let payload = run
        .inline_ifc_staging_payload([7.0, 11.0], 9, 0.75)
        .expect("laid out TextAreaTextRun should expose P7 staging diagnostic payload");

    assert_eq!(payload.char_range, 4..52);
    assert!(!payload.render_enabled);
    assert_eq!(
        payload.fallback,
        TextAreaTextRunInlineIfcRenderFallback::LegacyTextPass
    );
    assert_eq!(payload.bridge_input.content, run.effective_text());
    assert_eq!(payload.bridge_input.origin, [7.0, 11.0]);
    assert_eq!(payload.bridge_input.fragment_index, 9);
    assert_eq!(payload.bridge_input.opacity, 0.75);
    assert_eq!(payload.bridge_input.width_constraint, Some(92.0));
    assert!(payload.bridge_input.allow_wrap);
    assert!(payload.diagnostic.bridge_glyph_count > 0);
    assert_eq!(
        payload.diagnostic.prepared_glyph_count,
        payload.prepared_input.glyphs.len()
    );
    assert_eq!(
        payload.diagnostic.staging_glyph_count,
        payload.text_pass_staging_input.glyphs.len()
    );
    assert_eq!(
        payload.diagnostic.batch_count,
        payload.prepared_input.batches.len()
    );
    assert_eq!(
        payload.text_pass_staging_input.glyphs.len(),
        payload.prepared_input.glyphs.len()
    );
    assert_eq!(payload.prepared_candidate.char_range, 4..52);
    assert_eq!(
        payload.prepared_candidate.fallback,
        TextAreaTextRunInlineIfcRenderFallback::LegacyTextPass
    );
    assert_eq!(payload.prepared_candidate.origin, [7.0, 11.0]);
    assert_eq!(
        payload.prepared_candidate.layout_size,
        payload.diagnostic.layout_size
    );
    assert_eq!(payload.prepared_candidate.width_constraint, Some(92.0));
    assert!(payload.prepared_candidate.allow_wrap);
    assert_eq!(payload.prepared_candidate.opacity, 0.75);
    assert_eq!(payload.prepared_candidate.fragment_index, 9);
    assert_eq!(
        payload.prepared_candidate.glyph_count,
        payload.bridge_package.glyphs.len()
    );
    assert_eq!(
        payload.prepared_candidate.prepared_glyph_count,
        payload.prepared_input.glyphs.len()
    );
    assert_eq!(
        payload.prepared_candidate.staging_glyph_count,
        payload.text_pass_staging_input.glyphs.len()
    );
    assert_eq!(
        payload.prepared_candidate.batch_count,
        payload.prepared_input.batches.len()
    );
    let first_candidate_glyph = payload
        .prepared_candidate
        .glyph_metadata
        .first()
        .expect("prepared candidate should expose glyph metadata");
    let first_prepared_glyph = payload
        .prepared_input
        .glyphs
        .first()
        .expect("prepared input should expose glyph metadata");
    assert_eq!(
        first_candidate_glyph.glyph_index,
        first_prepared_glyph.glyph_index
    );
    assert_eq!(
        first_candidate_glyph.batch_index,
        first_prepared_glyph.batch_index
    );
    assert_eq!(
        first_candidate_glyph.final_paint_pos,
        first_prepared_glyph.final_paint_pos
    );
    assert_eq!(
        first_candidate_glyph.local_pos,
        first_prepared_glyph.paint.local_pos
    );
    assert_eq!(
        first_candidate_glyph.font_data_id,
        first_prepared_glyph.raster.font_data_id
    );
    assert_eq!(
        first_candidate_glyph.font_index,
        first_prepared_glyph.raster.font_index
    );
    assert_eq!(
        first_candidate_glyph.font_size,
        first_prepared_glyph.raster.font_size
    );
    assert_eq!(
        first_candidate_glyph.normalized_coords_hash,
        first_prepared_glyph.raster.normalized_coords_hash
    );
    assert!(first_candidate_glyph.has_raster_key);
    assert!(payload.prepared_input.glyphs.iter().all(|glyph| {
        payload
            .text_pass_staging_input
            .glyphs
            .iter()
            .any(|staging| staging.final_paint_pos == glyph.final_paint_pos)
    }));
    assert!(payload.readiness.editable_text_area_run);
    assert!(!payload.readiness.projection_ifc_path_ready);
    assert!(!payload.readiness.ime_ifc_path_ready);
    assert!(!payload.readiness.caret_affinity_ifc_path_ready);
    assert!(!payload.readiness.scroll_follow_ifc_path_ready);
}

#[test]
fn text_area_inline_ifc_staging_payload_preserves_preedit_boundary_metadata() {
    let mut run = TextAreaTextRun::new("hello world".to_string(), 10..21);
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 5,
        preedit_text: " IME".to_string(),
        preedit_cursor: Some((1, 3)),
    }));
    run.set_preedit_run(true, Some((0, 2)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);

    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 2, 1.0)
        .expect("preedit TextAreaTextRun should still expose diagnostic payload");

    assert_eq!(payload.char_range, 10..21);
    assert_eq!(
        payload.diagnostic.content_len,
        "hello world".chars().count()
    );
    assert_eq!(
        payload.diagnostic.effective_content_len,
        "hello IME world".chars().count()
    );
    assert_eq!(payload.bridge_input.content, "hello IME world");
    assert!(payload.readiness.has_inline_preedit);
    assert!(payload.readiness.is_preedit_run);
    assert_eq!(payload.readiness.preedit_cursor, Some((0, 2)));
    assert_eq!(
        payload.readiness.caret_affinity_diagnostic.preedit_cursor,
        Some((0, 2))
    );
    assert!(
        payload
            .readiness
            .caret_affinity_diagnostic
            .caret_stop_snapshots
            .iter()
            .all(|snapshot| snapshot.local_char == 0),
        "preedit-run caret snapshot seeding keeps root insertion at the API boundary"
    );
    assert!(!payload.render_enabled);
}

#[test]
fn text_area_inline_ifc_default_prepared_render_uses_valid_candidate() {
    let mut run = TextAreaTextRun::new(
        "text area default renders prepared candidate".to_string(),
        0..44,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose prepared candidate");
    assert!(payload.prepared_candidate.prepared_glyph_count > 0);
    assert_eq!(
        payload.prepared_candidate.fallback,
        TextAreaTextRunInlineIfcRenderFallback::LegacyTextPass
    );

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    run.build(&mut graph, &mut arena, ctx);
    let pass_names = graph
        .pass_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name.to_string())
        .collect::<Vec<_>>();

    assert_eq!(pass_names.len(), 1);
    assert!(
        pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
        "TextAreaTextRun default should emit TextPreparedInputPass for a valid candidate, got {}",
        pass_names[0]
    );
}

#[test]
fn text_area_inline_ifc_default_render_mode_emits_prepared_input_pass() {
    let mut run = TextAreaTextRun::new(
        "default prepared render pass uses TextArea prepared candidate".to_string(),
        0..61,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose prepared candidate");
    assert!(payload.prepared_candidate.prepared_glyph_count > 0);
    assert_eq!(
        payload.prepared_candidate.fallback,
        TextAreaTextRunInlineIfcRenderFallback::LegacyTextPass
    );

    let pass_names = build_text_area_run_pass_names(&mut run, &mut arena, 180, 120);

    assert_eq!(pass_names.len(), 1);
    assert!(
        pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
        "default TextArea prepared render should emit TextPreparedInputPass, got {}",
        pass_names[0]
    );
}

#[test]
fn text_area_inline_ifc_default_prepared_render_falls_back_when_candidate_is_missing() {
    let mut run = TextAreaTextRun::new(
        "default prepared render falls back when candidate is missing".to_string(),
        0..62,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose a candidate shape before the test hook hides it");
    assert!(payload.prepared_candidate.prepared_glyph_count > 0);

    run.force_missing_inline_ifc_prepared_candidate_for_test();
    let pass_names = build_text_area_run_pass_names(&mut run, &mut arena, 180, 120);

    assert_eq!(pass_names.len(), 1);
    assert!(
        pass_names[0].ends_with("render_pass::text_pass::TextPass"),
        "default prepared render with missing candidate must fallback legacy TextPass, got {}",
        pass_names[0]
    );
}

#[test]
fn text_area_inline_ifc_default_prepared_render_does_not_synthesize_candidate_for_empty_or_unlaid_out_runs()
 {
    let unlaid_out = TextAreaTextRun::new("unlaid out".to_string(), 0..9);
    assert!(
        unlaid_out
            .prepared_render_payload([0.0, 0.0], 0, 1.0)
            .is_none()
    );

    let mut empty = TextAreaTextRun::new(String::new(), 0..0);
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut empty, &mut arena, 180.0);
    assert!(empty.prepared_render_payload([0.0, 0.0], 0, 1.0).is_none());
    assert!(build_text_area_run_pass_names(&mut empty, &mut arena, 180, 120).is_empty());
}

#[test]
fn text_area_inline_ifc_default_prepared_render_rejects_invalid_payload() {
    let mut run = TextAreaTextRun::new(
        "default prepared render rejects invalid opacity".to_string(),
        0..48,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);

    assert!(run.prepared_render_payload([0.0, 0.0], 0, 0.0).is_none());
}

#[test]
fn text_area_inline_ifc_staging_payload_stays_separate_from_read_only_text_prepared_path() {
    let mut run = TextAreaTextRun::new("text area adapter stays diagnostic".to_string(), 0..34);
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let run_payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 3, 1.0)
        .expect("TextAreaTextRun should expose diagnostic payload");

    let mut text = Text::new(0.0, 0.0, 180.0, 120.0, "read-only text uses prepared path");
    text.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::RenderPreparedInput);
    let text_staging = text
        .text_read_only_ifc_prepared_staging_input([0.0, 0.0], 3)
        .expect("read-only Text should expose render-enabled prepared staging input");

    assert!(!run_payload.render_enabled);
    assert_eq!(
        run_payload.text_pass_staging_input.glyphs.len(),
        run_payload.prepared_input.glyphs.len()
    );
    assert!(!text_staging.glyphs.is_empty());
    assert_ne!(
        run_payload.bridge_input.content, "read-only text uses prepared path",
        "TextArea adapter payload must remain independent from read-only Text prepared path"
    );
}

#[test]
fn text_area_inline_ifc_staging_payload_is_probe_only_for_empty_or_unlaid_out_runs() {
    let run = TextAreaTextRun::new("unlaid out".to_string(), 0..9);
    assert!(
        run.inline_ifc_staging_payload([0.0, 0.0], 0, 1.0).is_none(),
        "adapter should not synthesize IFC payload before TextArea run layout exists"
    );

    let mut empty = TextAreaTextRun::new(String::new(), 0..0);
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut empty, &mut arena, 180.0);
    assert!(
        empty
            .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
            .is_none(),
        "empty TextAreaTextRun should stay legacy/no-op"
    );
}

#[test]
fn text_area_inline_ifc_evaluation_input_aggregates_staging_payload_diagnostic() {
    let mut run = TextAreaTextRun::new("hello world".to_string(), 10..21);
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 5,
        preedit_text: " IME".to_string(),
        preedit_cursor: Some((1, 3)),
    }));
    run.set_preedit_run(true, Some((0, 2)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([4.0, 6.0], 5, 1.0)
        .expect("laid out TextAreaTextRun should provide staging payload");

    let input = TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload.clone()]);
    assert_eq!(input.run_inputs.len(), 1);
    assert_eq!(input.run_inputs[0].char_range, 10..21);
    assert!(input.run_inputs[0].has_inline_preedit);
    assert!(input.run_inputs[0].is_preedit_run);
    assert_eq!(input.run_inputs[0].preedit_cursor, Some((0, 2)));
    assert_eq!(
        input.run_inputs[0].diagnostic.effective_content_len,
        "hello IME world".chars().count()
    );
    assert_eq!(
        input.run_inputs[0].diagnostic.staging_glyph_count,
        payload.text_pass_staging_input.glyphs.len()
    );
    assert!(input.legacy_fallback_confirmed);
    assert!(input.read_only_text_path_separated);

    let preflight = TextAreaInlineIfcEvaluationPreflight::evaluate(input);
    assert_eq!(
        preflight.state(),
        TextAreaInlineIfcEvaluationPreflightState::Blocked
    );
    assert_eq!(preflight.run_inputs().len(), 1);
    assert!(!preflight.render_enabled());
    assert!(!preflight.layout_enabled());
    assert!(
        !preflight.allows_text_area_editable_behavior_path_switch(),
        "evaluation input is diagnostic-only and must not authorize TextArea rollout"
    );
}

#[test]
fn text_area_inline_ifc_evaluation_preflight_blocks_current_metadata_paths() {
    let mut run = TextAreaTextRun::new(
        "editable run evaluation metadata remains unwired".to_string(),
        0..48,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should provide staging payload");
    let preflight = TextAreaInlineIfcEvaluationPreflight::evaluate(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );

    assert_eq!(
        preflight.state(),
        TextAreaInlineIfcEvaluationPreflightState::Blocked
    );
    for reason in [
        TextAreaInlineIfcEvaluationPreflightBlockedReason::ProjectionPathUnwired,
        TextAreaInlineIfcEvaluationPreflightBlockedReason::ImePathUnwired,
        TextAreaInlineIfcEvaluationPreflightBlockedReason::CaretAffinityPathUnwired,
        TextAreaInlineIfcEvaluationPreflightBlockedReason::ScrollFollowPathUnwired,
    ] {
        assert!(
            preflight.blocked_reasons().contains(&reason),
            "current P7 TextArea evaluation preflight must block on {reason:?}"
        );
    }
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcEvaluationPreflightBlockedReason::MissingRunPayload),
        "laid out run payload should satisfy the run payload requirement"
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcEvaluationPreflightBlockedReason::LegacyFallbackMissing),
        "TextArea staging payload confirms correctness fallback to legacy TextPass"
    );
    assert!(
        !preflight.blocked_reasons().contains(
            &TextAreaInlineIfcEvaluationPreflightBlockedReason::ReadOnlyTextPathSeparationMissing
        ),
        "TextArea staging payload remains separated from read-only Text prepared path"
    );
}

#[test]
fn text_area_inline_ifc_evaluation_preflight_blocks_missing_or_unlaid_out_run_payload() {
    let run = TextAreaTextRun::new("unlaid out evaluation".to_string(), 0..21);
    let payloads = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .into_iter()
        .collect::<Vec<_>>();
    let preflight = TextAreaInlineIfcEvaluationPreflight::evaluate(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(payloads),
    );

    assert_eq!(
        preflight.state(),
        TextAreaInlineIfcEvaluationPreflightState::Blocked
    );
    assert!(
        preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcEvaluationPreflightBlockedReason::MissingRunPayload)
    );
    assert!(
        preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcEvaluationPreflightBlockedReason::LegacyFallbackMissing)
    );
    assert!(preflight.run_inputs().is_empty());
    assert!(!preflight.render_enabled());
    assert!(!preflight.layout_enabled());
}

#[test]
fn text_area_inline_ifc_evaluation_preflight_requires_read_only_path_separation() {
    let mut run = TextAreaTextRun::new("text area evaluation stays separated".to_string(), 0..36);
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should provide staging payload");
    let mut input = TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]);
    input.read_only_text_path_separated = false;

    let preflight = TextAreaInlineIfcEvaluationPreflight::evaluate(input);

    assert_eq!(
        preflight.state(),
        TextAreaInlineIfcEvaluationPreflightState::Blocked
    );
    assert!(preflight.blocked_reasons().contains(
        &TextAreaInlineIfcEvaluationPreflightBlockedReason::ReadOnlyTextPathSeparationMissing
    ));
    assert!(!preflight.render_enabled());
    assert!(
        !preflight.allows_text_area_editable_behavior_path_switch(),
        "missing read-only Text separation blocks evaluation and still never authorizes rollout"
    );
}

#[test]
fn text_area_inline_ifc_metadata_bridge_builds_from_evaluation_input() {
    let mut run = TextAreaTextRun::new("bridge keeps run metadata".to_string(), 20..45);
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 6,
        preedit_text: " IME".to_string(),
        preedit_cursor: Some((1, 2)),
    }));
    run.set_preedit_run(true, Some((0, 1)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([2.0, 3.0], 4, 1.0)
        .expect("laid out run should expose staging payload for metadata bridge");
    let input = TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload.clone()]);

    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(input);

    assert_eq!(bridge.run_metadata.len(), 1);
    assert_eq!(bridge.run_metadata[0].char_range, 20..45);
    assert!(bridge.run_metadata[0].has_inline_preedit);
    assert!(bridge.run_metadata[0].is_preedit_run);
    assert_eq!(bridge.run_metadata[0].preedit_cursor, Some((0, 1)));
    assert_eq!(
        bridge.run_metadata[0].diagnostic.staging_glyph_count,
        payload.text_pass_staging_input.glyphs.len()
    );
    let ime_diagnostic = bridge
        .ime_metadata_diagnostic
        .as_ref()
        .expect("laid out run metadata should expose IME bridge diagnostic");
    assert_eq!(ime_diagnostic.run_count, 1);
    assert!(ime_diagnostic.has_inline_preedit);
    assert!(ime_diagnostic.has_preedit_run);
    assert_eq!(ime_diagnostic.preedit_cursor_count, 1);
    assert_eq!(
        ime_diagnostic.effective_content_len,
        "bridge IME keeps run metadata".chars().count()
    );
    assert_eq!(
        bridge.projection_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedNoProjectionSegments
    );
    let projection_diagnostic = bridge
        .projection_metadata_diagnostic
        .as_ref()
        .expect("laid out run metadata should expose projection bridge diagnostic");
    assert_eq!(projection_diagnostic.run_count, 1);
    assert_eq!(projection_diagnostic.char_range_count, 1);
    assert_eq!(projection_diagnostic.char_span, 25);
    assert_eq!(projection_diagnostic.projection_segment_count, 0);
    assert_eq!(projection_diagnostic.inline_preedit_run_count, 1);
    assert_eq!(projection_diagnostic.preedit_run_count, 1);
    assert_eq!(
        bridge.ime_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::Observed
    );
    assert_eq!(
        bridge.caret_affinity_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
    );
    let caret_diagnostic = bridge
        .caret_affinity_metadata_diagnostic
        .as_ref()
        .expect("laid out run metadata should expose caret affinity bridge diagnostic");
    assert_eq!(caret_diagnostic.run_count, 1);
    assert!(caret_diagnostic.visual_line_count >= 1);
    assert!(caret_diagnostic.caret_stop_count >= 1);
    assert_eq!(caret_diagnostic.preedit_cursor_count, 1);
    assert_eq!(
        bridge.scroll_follow_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::Observed
    );
    let scroll_diagnostic = bridge
        .scroll_follow_metadata_diagnostic
        .as_ref()
        .expect("laid out run metadata should expose scroll-follow bridge diagnostic");
    assert_eq!(scroll_diagnostic.run_count, 1);
    assert_eq!(scroll_diagnostic.layout_size_count, 1);
    assert_eq!(scroll_diagnostic.char_span, 25);
    assert!(scroll_diagnostic.visual_line_count >= 1);
    assert!(scroll_diagnostic.caret_stop_count >= 1);
    assert!(bridge.legacy_fallback_confirmed);
    assert!(bridge.read_only_text_path_separated);
}

#[test]
fn text_area_inline_ifc_metadata_bridge_observes_ime_preedit_from_run_payload() {
    let mut run = TextAreaTextRun::new("hello world".to_string(), 10..21);
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 5,
        preedit_text: " IME".to_string(),
        preedit_cursor: Some((1, 3)),
    }));
    run.set_preedit_run(true, Some((0, 2)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([1.0, 2.0], 7, 1.0)
        .expect("preedit run should expose metadata bridge payload");

    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

    assert_eq!(
        preflight.bridge_input().ime_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::Observed
    );
    let ime_diagnostic = preflight
        .bridge_input()
        .ime_metadata_diagnostic
        .as_ref()
        .expect("IME metadata observation should preserve aggregate diagnostic");
    assert!(ime_diagnostic.has_inline_preedit);
    assert!(ime_diagnostic.has_preedit_run);
    assert_eq!(ime_diagnostic.preedit_cursor_count, 1);
    assert_eq!(
        ime_diagnostic.effective_content_len,
        "hello IME world".chars().count()
    );
    assert_eq!(preflight.bridge_input().run_metadata[0].char_range, 10..21);
    assert!(preflight.bridge_input().run_metadata[0].has_inline_preedit);
    assert!(preflight.bridge_input().run_metadata[0].is_preedit_run);
    assert_eq!(
        preflight.bridge_input().run_metadata[0].preedit_cursor,
        Some((0, 2))
    );
    assert_eq!(
        preflight.state(),
        TextAreaInlineIfcMetadataBridgePreflightState::ReadyForDiagnosticEvaluation
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ImeMetadataUnwired)
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ProjectionMetadataUnwired),
        "laid out run projection metadata should now be observed diagnostically"
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ScrollFollowMetadataUnwired),
        "laid out run scroll-follow metadata should now be observed diagnostically"
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::CaretAffinityMetadataUnwired),
        "laid out run caret metadata should now be observed diagnostically"
    );
    assert!(!preflight.render_enabled());
    assert!(!preflight.layout_enabled());
    assert!(!preflight.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_metadata_bridge_observes_no_preedit_without_synthesizing_one() {
    let mut run = TextAreaTextRun::new("plain editable metadata".to_string(), 0..23);
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("plain laid out run should expose metadata bridge payload");

    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

    assert_eq!(
        preflight.bridge_input().ime_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedNoPreedit
    );
    let ime_diagnostic = preflight
        .bridge_input()
        .ime_metadata_diagnostic
        .as_ref()
        .expect("plain run should still expose a no-preedit IME observation");
    assert!(!ime_diagnostic.has_inline_preedit);
    assert!(!ime_diagnostic.has_preedit_run);
    assert_eq!(ime_diagnostic.preedit_cursor_count, 0);
    assert_eq!(
        ime_diagnostic.effective_content_len,
        "plain editable metadata".chars().count()
    );
    assert!(!preflight.bridge_input().run_metadata[0].has_inline_preedit);
    assert!(!preflight.bridge_input().run_metadata[0].is_preedit_run);
    assert_eq!(
        preflight.bridge_input().run_metadata[0].preedit_cursor,
        None
    );
    assert_eq!(
        preflight.state(),
        TextAreaInlineIfcMetadataBridgePreflightState::ReadyForDiagnosticEvaluation
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ImeMetadataUnwired)
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ProjectionMetadataUnwired),
        "plain laid out run projection metadata should now be observed diagnostically"
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ScrollFollowMetadataUnwired),
        "no-preedit run scroll-follow metadata should now be observed diagnostically"
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::CaretAffinityMetadataUnwired),
        "plain laid out run caret metadata should now be observed diagnostically"
    );
    assert!(!preflight.render_enabled());
    assert!(!preflight.layout_enabled());
    assert!(!preflight.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_metadata_bridge_observes_projection_metadata_from_laid_out_run() {
    let mut run = TextAreaTextRun::new(
        "projection metadata source is a plain laid out run".to_string(),
        40..92,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 140.0);
    let payload = run
        .inline_ifc_staging_payload([5.0, 6.0], 9, 1.0)
        .expect("laid out run should expose projection metadata bridge payload");

    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

    assert_eq!(
        preflight.bridge_input().projection_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedNoProjectionSegments
    );
    let projection_diagnostic = preflight
        .bridge_input()
        .projection_metadata_diagnostic
        .as_ref()
        .expect("projection metadata observation should preserve aggregate diagnostic");
    assert_eq!(projection_diagnostic.run_count, 1);
    assert_eq!(projection_diagnostic.char_range_count, 1);
    assert_eq!(projection_diagnostic.char_span, 52);
    assert_eq!(
        projection_diagnostic.effective_content_len,
        "projection metadata source is a plain laid out run"
            .chars()
            .count()
    );
    assert_eq!(projection_diagnostic.projection_segment_count, 0);
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ProjectionMetadataUnwired),
        "projection metadata source should be observed even when no projection segments exist"
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ScrollFollowMetadataUnwired),
        "projection metadata observation should preserve scroll-follow diagnostic metadata"
    );
    assert!(!preflight.render_enabled());
    assert!(!preflight.layout_enabled());
    assert!(!preflight.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_metadata_bridge_observes_caret_affinity_stops_from_laid_out_run() {
    let mut run = TextAreaTextRun::new(
        "caret affinity metadata observes laid out stops".to_string(),
        30..77,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 120.0);
    let payload = run
        .inline_ifc_staging_payload([3.0, 4.0], 2, 1.0)
        .expect("laid out run should expose caret metadata bridge payload");

    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

    assert_eq!(
        preflight.bridge_input().caret_affinity_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
    );
    let caret_diagnostic = preflight
        .bridge_input()
        .caret_affinity_metadata_diagnostic
        .as_ref()
        .expect("caret metadata observation should preserve aggregate diagnostic");
    assert_eq!(caret_diagnostic.run_count, 1);
    assert!(caret_diagnostic.visual_line_count >= 1);
    assert!(caret_diagnostic.caret_stop_count >= 1);
    assert!(
        caret_diagnostic.multi_stop_line_count >= 1,
        "non-empty laid out text should expose at least one visual line with caret stops"
    );
    assert_eq!(caret_diagnostic.preedit_cursor_count, 0);
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::CaretAffinityMetadataUnwired),
        "caret stops are now observed as diagnostic metadata"
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ProjectionMetadataUnwired),
        "caret/projection metadata observation should no longer report projection unwired"
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ScrollFollowMetadataUnwired),
        "caret/projection metadata observation should preserve scroll-follow diagnostic metadata"
    );
    assert!(!preflight.render_enabled());
    assert!(!preflight.layout_enabled());
    assert!(!preflight.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_metadata_bridge_keeps_preedit_cursor_in_caret_metadata() {
    let mut run = TextAreaTextRun::new("preedit cursor stays observable".to_string(), 5..35);
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 7,
        preedit_text: "編輯".to_string(),
        preedit_cursor: Some((3, 6)),
    }));
    run.set_preedit_run(true, Some((1, 2)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 3, 1.0)
        .expect("preedit run should expose caret metadata bridge payload");

    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

    assert_eq!(
        preflight.bridge_input().ime_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::Observed
    );
    assert_eq!(
        preflight.bridge_input().caret_affinity_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
    );
    assert_eq!(
        preflight.bridge_input().run_metadata[0].preedit_cursor,
        Some((1, 2))
    );
    assert_eq!(
        preflight.bridge_input().run_metadata[0]
            .caret_affinity_diagnostic
            .preedit_cursor,
        Some((1, 2))
    );
    assert_eq!(
        preflight
            .bridge_input()
            .caret_affinity_metadata_diagnostic
            .as_ref()
            .expect("preedit run should preserve caret metadata diagnostic")
            .preedit_cursor_count,
        1
    );
    assert_eq!(
        preflight.state(),
        TextAreaInlineIfcMetadataBridgePreflightState::ReadyForDiagnosticEvaluation
    );
    assert!(!preflight.render_enabled());
    assert!(!preflight.layout_enabled());
    assert!(!preflight.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_metadata_bridge_does_not_fake_affinity_slots_when_missing() {
    let mut run = TextAreaTextRun::new("caret metadata can be absent".to_string(), 0..28);
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
    let mut payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose metadata bridge payload");
    payload
        .readiness
        .caret_affinity_diagnostic
        .visual_line_count = 0;
    payload.readiness.caret_affinity_diagnostic.caret_stop_count = 0;
    payload
        .readiness
        .caret_affinity_diagnostic
        .multi_stop_line_count = 0;

    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

    assert_eq!(
        preflight.bridge_input().caret_affinity_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedNoAffinitySlots
    );
    let caret_diagnostic = preflight
        .bridge_input()
        .caret_affinity_metadata_diagnostic
        .as_ref()
        .expect("missing slots should still be an explicit diagnostic observation");
    assert_eq!(caret_diagnostic.caret_stop_count, 0);
    assert_eq!(caret_diagnostic.visual_line_count, 0);
    assert_eq!(
        preflight.state(),
        TextAreaInlineIfcMetadataBridgePreflightState::ReadyForDiagnosticEvaluation
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::CaretAffinityMetadataUnwired),
        "missing slots should not be mislabeled as an unwired metadata source"
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ProjectionMetadataUnwired),
        "missing caret slots must not hide observed projection metadata"
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ScrollFollowMetadataUnwired)
    );
    assert!(!preflight.render_enabled());
    assert!(!preflight.layout_enabled());
    assert!(!preflight.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_metadata_bridge_observes_scroll_follow_metadata_from_laid_out_run() {
    let mut run = TextAreaTextRun::new(
        "metadata bridge current state remains diagnostic only".to_string(),
        0..53,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose metadata bridge payload");
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );

    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

    assert_eq!(
        preflight.state(),
        TextAreaInlineIfcMetadataBridgePreflightState::ReadyForDiagnosticEvaluation
    );
    assert_eq!(
        preflight.bridge_input().scroll_follow_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::Observed
    );
    let scroll_diagnostic = preflight
        .bridge_input()
        .scroll_follow_metadata_diagnostic
        .as_ref()
        .expect("laid out run should expose scroll-follow aggregate diagnostic");
    assert_eq!(scroll_diagnostic.run_count, 1);
    assert_eq!(scroll_diagnostic.layout_size_count, 1);
    assert_eq!(scroll_diagnostic.char_span, 53);
    assert_eq!(
        scroll_diagnostic.effective_content_len,
        "metadata bridge current state remains diagnostic only"
            .chars()
            .count()
    );
    assert!(scroll_diagnostic.visual_line_count >= 1);
    assert!(scroll_diagnostic.caret_stop_count >= 1);
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ProjectionMetadataUnwired),
        "laid out run projection metadata should be observed without marking projection behavior ready"
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ScrollFollowMetadataUnwired),
        "scroll-follow metadata source should be observed without marking behavior ready"
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::CaretAffinityMetadataUnwired),
        "laid out run caret metadata should be observed without marking caret behavior ready"
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::MissingRunMetadata),
        "laid out run should satisfy metadata bridge run diagnostics"
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ImeMetadataUnwired),
        "laid out run metadata now observes the IME/preedit bridge path"
    );
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::LegacyFallbackUnconfirmed),
        "TextArea bridge confirms correctness fallback to legacy TextPass"
    );
    assert!(
        !preflight.blocked_reasons().contains(
            &TextAreaInlineIfcMetadataBridgeBlockedReason::ReadOnlyTextPathSeparationUnconfirmed
        ),
        "TextArea bridge remains separated from read-only Text prepared path"
    );
    assert!(!preflight.render_enabled());
    assert!(!preflight.layout_enabled());
    assert!(!preflight.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_metadata_bridge_does_not_fake_scroll_follow_source_when_missing() {
    let mut run = TextAreaTextRun::new("scroll metadata can be absent".to_string(), 0..29);
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
    let mut payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose metadata bridge payload");
    payload.readiness.scroll_follow_diagnostic.layout_size = [0.0, 0.0];
    payload.readiness.scroll_follow_diagnostic.visual_line_count = 0;
    payload.readiness.scroll_follow_diagnostic.caret_stop_count = 0;

    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

    assert_eq!(
        preflight.bridge_input().scroll_follow_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedNoScrollSource
    );
    let scroll_diagnostic = preflight
        .bridge_input()
        .scroll_follow_metadata_diagnostic
        .as_ref()
        .expect("missing scroll source should still be an explicit diagnostic observation");
    assert_eq!(scroll_diagnostic.layout_size_count, 0);
    assert_eq!(scroll_diagnostic.visual_line_count, 0);
    assert_eq!(scroll_diagnostic.caret_stop_count, 0);
    assert!(
        !preflight
            .blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::ScrollFollowMetadataUnwired),
        "missing scroll source should not be mislabeled as an unwired metadata source"
    );
    assert!(!preflight.render_enabled());
    assert!(!preflight.layout_enabled());
    assert!(!preflight.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_metadata_bridge_observed_placeholders_still_do_not_enable_rollout() {
    let mut run = TextAreaTextRun::new(
        "observed placeholders are still rollout blocked".to_string(),
        0..47,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose metadata bridge payload");
    let mut bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    bridge.projection_metadata_status = TextAreaInlineIfcMetadataBridgeStatus::Observed;
    bridge.ime_metadata_status = TextAreaInlineIfcMetadataBridgeStatus::Observed;
    bridge.caret_affinity_metadata_status = TextAreaInlineIfcMetadataBridgeStatus::Observed;
    bridge.scroll_follow_metadata_status = TextAreaInlineIfcMetadataBridgeStatus::Observed;

    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

    assert_eq!(
        preflight.state(),
        TextAreaInlineIfcMetadataBridgePreflightState::ReadyForDiagnosticEvaluation
    );
    assert!(preflight.blocked_reasons().is_empty());
    assert_eq!(preflight.bridge_input().run_metadata.len(), 1);
    assert!(
        !preflight.render_enabled(),
        "metadata bridge observation must not enable TextArea prepared rendering"
    );
    assert!(
        !preflight.layout_enabled(),
        "metadata bridge observation must not switch TextArea layout"
    );
    assert!(
        !preflight.allows_text_area_editable_behavior_path_switch(),
        "observed placeholder metadata is only diagnostic and must not authorize default behavior path switch"
    );
}

#[test]
fn text_area_inline_ifc_readiness_audit_keeps_observed_metadata_behavior_blocked() {
    let mut run = TextAreaTextRun::new(
        "readiness audit sees metadata but behavior remains blocked".to_string(),
        0..57,
    );
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 8,
        preedit_text: " IME".to_string(),
        preedit_cursor: Some((1, 2)),
    }));
    run.set_preedit_run(true, Some((0, 1)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose metadata bridge payload");
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );

    assert_eq!(
        audit.metadata_observation_readiness(),
        TextAreaEditableIfcMetadataObservationReadiness::Ready
    );
    assert_eq!(
        audit.state(),
        TextAreaEditableIfcReadinessAuditState::Blocked
    );
    assert!(audit.metadata_blocked_reasons().is_empty());
    for reason in [
        TextAreaEditableIfcBehaviorPathBlockedReason::ProjectionPathUnwired,
        TextAreaEditableIfcBehaviorPathBlockedReason::ImePathUnwired,
        TextAreaEditableIfcBehaviorPathBlockedReason::CaretAffinityPathUnwired,
        TextAreaEditableIfcBehaviorPathBlockedReason::ScrollFollowPathUnwired,
    ] {
        assert!(
            audit.behavior_path_blocked_reasons().contains(&reason),
            "TextArea editable IFC audit must keep behavior path blocked on {reason:?}"
        );
    }
    assert_eq!(
        audit.recommendation(),
        TextAreaEditableIfcBehaviorPathRecommendation::KeepLegacyEditableBehaviorPath
    );
    assert!(!audit.render_enabled());
    assert!(!audit.layout_enabled());
    assert!(!audit.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_readiness_audit_records_no_source_metadata_without_behavior_ready() {
    let mut run = TextAreaTextRun::new(
        "audit no source metadata remains blocked".to_string(),
        0..40,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 160.0);
    let mut payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose metadata bridge payload");
    payload
        .readiness
        .caret_affinity_diagnostic
        .visual_line_count = 0;
    payload.readiness.caret_affinity_diagnostic.caret_stop_count = 0;
    payload.readiness.scroll_follow_diagnostic.layout_size = [0.0, 0.0];
    payload.readiness.scroll_follow_diagnostic.visual_line_count = 0;
    payload.readiness.scroll_follow_diagnostic.caret_stop_count = 0;
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );

    assert_eq!(
        audit.input().caret_affinity_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedNoAffinitySlots
    );
    assert_eq!(
        audit.input().scroll_follow_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedNoScrollSource
    );
    assert_eq!(
        audit.metadata_observation_readiness(),
        TextAreaEditableIfcMetadataObservationReadiness::Ready
    );
    assert!(
        audit
            .behavior_path_blocked_reasons()
            .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::CaretAffinityPathUnwired),
        "no affinity slots are metadata-observed but behavior path is still unwired"
    );
    assert!(
        audit
            .behavior_path_blocked_reasons()
            .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::ScrollFollowPathUnwired),
        "no scroll source is metadata-observed but behavior path is still unwired"
    );
    assert!(!audit.render_enabled());
    assert!(!audit.layout_enabled());
    assert!(!audit.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_readiness_audit_records_missing_metadata_as_incomplete() {
    let run = TextAreaTextRun::new("unlaid out audit".to_string(), 0..16);
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(
            run.inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
                .into_iter()
                .collect(),
        ),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);

    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );

    assert_eq!(
        audit.metadata_observation_readiness(),
        TextAreaEditableIfcMetadataObservationReadiness::Incomplete
    );
    assert!(
        audit
            .metadata_blocked_reasons()
            .contains(&TextAreaInlineIfcMetadataBridgeBlockedReason::MissingRunMetadata)
    );
    assert_eq!(audit.input().run_metadata_count, 0);
    assert!(!audit.input().projection_metadata_diagnostic_present);
    assert!(!audit.input().ime_metadata_diagnostic_present);
    assert!(!audit.input().caret_affinity_metadata_diagnostic_present);
    assert!(!audit.input().scroll_follow_metadata_diagnostic_present);
    assert!(!audit.render_enabled());
    assert!(!audit.layout_enabled());
    assert!(!audit.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_ime_behavior_prewire_observes_preedit_metadata_and_cursor() {
    let mut run = TextAreaTextRun::new("ime behavior path reads preedit".to_string(), 0..31);
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 4,
        preedit_text: "入力".to_string(),
        preedit_cursor: Some((2, 6)),
    }));
    run.set_preedit_run(true, Some((1, 3)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out preedit run should expose IME metadata source metadata");
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );

    let prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcImeBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        prewire.state(),
        TextAreaEditableIfcImeBehaviorPathPrewireState::DiagnosticPrewired
    );
    assert!(prewire.blocked_reasons().is_empty());
    assert!(prewire.diagnostic_prewired());
    let diagnostic = prewire
        .diagnostic()
        .expect("preedit IME metadata source should preserve diagnostic metadata");
    assert_eq!(
        diagnostic.ime_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::Observed
    );
    assert_eq!(diagnostic.run_count, 1);
    assert!(diagnostic.has_inline_preedit);
    assert!(diagnostic.has_preedit_run);
    assert_eq!(diagnostic.preedit_cursor_count, 1);
    assert_eq!(diagnostic.preedit_cursors, vec![(1, 3)]);
    assert_eq!(
        diagnostic.effective_content_len,
        "ime 入力behavior path reads preedit".chars().count()
    );
    assert!(
        prewire
            .input()
            .readiness_behavior_blocked_reasons
            .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::ImePathUnwired),
        "#45 metadata source must not rewrite the #44 behavior-path audit"
    );
    assert!(!prewire.ime_behavior_path_ready());
    assert!(!prewire.render_enabled());
    assert!(!prewire.layout_enabled());
    assert!(!prewire.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_ime_behavior_prewire_observes_no_preedit_without_faking_ready() {
    let mut run = TextAreaTextRun::new("plain IME metadata source".to_string(), 0..26);
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out no-preedit run should expose IME metadata source metadata");
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );

    let prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcImeBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        prewire.state(),
        TextAreaEditableIfcImeBehaviorPathPrewireState::ObservedNoPreedit
    );
    assert!(prewire.diagnostic_prewired());
    let diagnostic = prewire
        .diagnostic()
        .expect("no-preedit IME metadata source should keep explicit diagnostic metadata");
    assert_eq!(
        diagnostic.ime_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedNoPreedit
    );
    assert!(!diagnostic.has_inline_preedit);
    assert!(!diagnostic.has_preedit_run);
    assert_eq!(diagnostic.preedit_cursor_count, 0);
    assert!(diagnostic.preedit_cursors.is_empty());
    assert!(!prewire.ime_behavior_path_ready());
    assert!(!prewire.render_enabled());
    assert!(!prewire.layout_enabled());
    assert!(!prewire.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_ime_behavior_prewire_blocks_missing_metadata_source() {
    let run = TextAreaTextRun::new("unlaid out IME prewire".to_string(), 0..22);
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(
            run.inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
                .into_iter()
                .collect(),
        ),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );

    let prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcImeBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        prewire.state(),
        TextAreaEditableIfcImeBehaviorPathPrewireState::Blocked
    );
    for reason in [
        TextAreaEditableIfcImeBehaviorPathPrewireBlockedReason::MissingRunMetadata,
        TextAreaEditableIfcImeBehaviorPathPrewireBlockedReason::ImeMetadataUnwired,
        TextAreaEditableIfcImeBehaviorPathPrewireBlockedReason::MissingImeMetadataDiagnostic,
    ] {
        assert!(
            prewire.blocked_reasons().contains(&reason),
            "missing IME prewire metadata should block on {reason:?}"
        );
    }
    assert!(!prewire.diagnostic_prewired());
    assert!(prewire.diagnostic().is_none());
    assert!(!prewire.ime_behavior_path_ready());
    assert!(!prewire.render_enabled());
    assert!(!prewire.layout_enabled());
    assert!(!prewire.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_ime_behavior_prewire_keeps_readiness_audit_diagnostic_only() {
    let mut run = TextAreaTextRun::new("IME prewire does not authorize rollout".to_string(), 0..39);
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 3,
        preedit_text: "IME".to_string(),
        preedit_cursor: Some((0, 3)),
    }));
    run.set_preedit_run(true, Some((0, 1)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out preedit run should expose IME prewire metadata");
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );
    let prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcImeBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        audit.state(),
        TextAreaEditableIfcReadinessAuditState::Blocked
    );
    assert!(
        audit
            .behavior_path_blocked_reasons()
            .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::ImePathUnwired),
        "readiness audit must keep IME behavior path blocked after metadata source"
    );
    assert_eq!(
        audit.recommendation(),
        TextAreaEditableIfcBehaviorPathRecommendation::KeepLegacyEditableBehaviorPath
    );
    assert!(prewire.diagnostic_prewired());
    assert!(!audit.render_enabled());
    assert!(!audit.layout_enabled());
    assert!(!audit.allows_text_area_editable_behavior_path_switch());
    assert!(!prewire.render_enabled());
    assert!(!prewire.layout_enabled());
    assert!(!prewire.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_caret_affinity_metadata_source_observes_caret_stop_metadata() {
    let mut run = TextAreaTextRun::new(
        "caret affinity behavior path reads visual stops".to_string(),
        0..47,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose caret affinity metadata source metadata");
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );

    let metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
        TextAreaEditableIfcCaretAffinityMetadataSourceInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        metadata_source.state(),
        TextAreaEditableIfcCaretAffinityMetadataSourceState::MetadataObserved
    );
    assert!(metadata_source.blocked_reasons().is_empty());
    assert!(metadata_source.metadata_observed());
    let diagnostic = metadata_source
        .diagnostic()
        .expect("caret affinity metadata source should preserve diagnostic metadata");
    assert_eq!(
        diagnostic.caret_affinity_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
    );
    assert_eq!(diagnostic.run_count, 1);
    assert!(diagnostic.visual_line_count > 0);
    assert!(diagnostic.caret_stop_count > 0);
    assert!(
        diagnostic.multi_stop_line_count > 0,
        "laid out text should expose at least one line with multiple affinity stops"
    );
    assert_eq!(diagnostic.preedit_cursor_count, 0);
    assert_eq!(diagnostic.per_run_caret_diagnostics.len(), 1);
    assert_eq!(
        diagnostic.per_run_caret_diagnostics[0].caret_stop_count,
        diagnostic.caret_stop_count
    );
    assert!(
        metadata_source
            .input()
            .readiness_behavior_blocked_reasons
            .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::CaretAffinityPathUnwired),
        "#46 metadata source must not rewrite the #44 behavior-path audit"
    );
    assert!(!metadata_source.caret_affinity_behavior_path_ready());
    assert!(!metadata_source.render_enabled());
    assert!(!metadata_source.layout_enabled());
    assert!(!metadata_source.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_caret_affinity_metadata_source_observes_no_affinity_slots() {
    let mut run = TextAreaTextRun::new(
        "caret affinity no slots remains diagnostic".to_string(),
        0..43,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let mut payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose caret affinity metadata");
    payload
        .readiness
        .caret_affinity_diagnostic
        .visual_line_count = 0;
    payload.readiness.caret_affinity_diagnostic.caret_stop_count = 0;
    payload
        .readiness
        .caret_affinity_diagnostic
        .multi_stop_line_count = 0;
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );

    let metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
        TextAreaEditableIfcCaretAffinityMetadataSourceInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        metadata_source.state(),
        TextAreaEditableIfcCaretAffinityMetadataSourceState::ObservedNoAffinitySlots
    );
    assert!(metadata_source.blocked_reasons().is_empty());
    assert!(metadata_source.metadata_observed());
    let diagnostic = metadata_source
        .diagnostic()
        .expect("no-affinity-slots source should keep explicit diagnostic metadata");
    assert_eq!(
        diagnostic.caret_affinity_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedNoAffinitySlots
    );
    assert_eq!(diagnostic.visual_line_count, 0);
    assert_eq!(diagnostic.caret_stop_count, 0);
    assert_eq!(diagnostic.multi_stop_line_count, 0);
    assert!(!metadata_source.caret_affinity_behavior_path_ready());
    assert!(!metadata_source.render_enabled());
    assert!(!metadata_source.layout_enabled());
    assert!(!metadata_source.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_caret_affinity_metadata_source_keeps_readiness_audit_diagnostic_only() {
    let mut run = TextAreaTextRun::new(
        "caret affinity metadata source does not authorize rollout".to_string(),
        0..50,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose caret affinity metadata source metadata");
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );
    let metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
        TextAreaEditableIfcCaretAffinityMetadataSourceInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        audit.state(),
        TextAreaEditableIfcReadinessAuditState::Blocked
    );
    assert!(
        audit
            .behavior_path_blocked_reasons()
            .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::CaretAffinityPathUnwired),
        "readiness audit must keep caret affinity behavior path blocked after metadata source"
    );
    assert_eq!(
        audit.recommendation(),
        TextAreaEditableIfcBehaviorPathRecommendation::KeepLegacyEditableBehaviorPath
    );
    assert!(metadata_source.metadata_observed());
    assert!(!audit.render_enabled());
    assert!(!audit.layout_enabled());
    assert!(!audit.allows_text_area_editable_behavior_path_switch());
    assert!(!metadata_source.caret_affinity_behavior_path_ready());
    assert!(!metadata_source.render_enabled());
    assert!(!metadata_source.layout_enabled());
    assert!(!metadata_source.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_caret_affinity_metadata_source_preserves_ime_preedit_metadata() {
    let mut run = TextAreaTextRun::new(
        "caret affinity metadata source leaves IME metadata intact".to_string(),
        0..47,
    );
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 6,
        preedit_text: "入力".to_string(),
        preedit_cursor: Some((1, 3)),
    }));
    run.set_preedit_run(true, Some((1, 2)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out preedit run should expose caret affinity metadata source metadata");
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );
    let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
        TextAreaEditableIfcCaretAffinityMetadataSourceInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcImeBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        caret_metadata_source.state(),
        TextAreaEditableIfcCaretAffinityMetadataSourceState::MetadataObserved
    );
    let caret_diagnostic = caret_metadata_source
        .diagnostic()
        .expect("caret metadata source should preserve per-run caret diagnostic");
    assert_eq!(caret_diagnostic.preedit_cursor_count, 1);
    assert_eq!(
        caret_diagnostic.per_run_caret_diagnostics[0].preedit_cursor,
        Some((1, 2))
    );
    assert_eq!(
        ime_prewire.state(),
        TextAreaEditableIfcImeBehaviorPathPrewireState::DiagnosticPrewired
    );
    let ime_diagnostic = ime_prewire
        .diagnostic()
        .expect("IME prewire should still preserve preedit diagnostic metadata");
    assert_eq!(ime_diagnostic.preedit_cursor_count, 1);
    assert_eq!(ime_diagnostic.preedit_cursors, vec![(1, 2)]);
    assert!(!caret_metadata_source.caret_affinity_behavior_path_ready());
    assert!(!ime_prewire.ime_behavior_path_ready());
    assert!(!caret_metadata_source.allows_text_area_editable_behavior_path_switch());
    assert!(!ime_prewire.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_projection_behavior_prewire_observes_projection_segment_metadata() {
    let mut run = TextAreaTextRun::new(
        "projection behavior path reads projected segment metadata".to_string(),
        10..66,
    );
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 10,
        preedit_text: "候補".to_string(),
        preedit_cursor: Some((1, 2)),
    }));
    run.set_preedit_run(true, Some((0, 1)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let mut payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose projection metadata source metadata");
    payload
        .readiness
        .projection_diagnostic
        .projection_segment_count = 2;
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );

    let prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        prewire.state(),
        TextAreaEditableIfcProjectionBehaviorPathPrewireState::DiagnosticPrewired
    );
    assert!(prewire.blocked_reasons().is_empty());
    assert!(prewire.diagnostic_prewired());
    let diagnostic = prewire
        .diagnostic()
        .expect("projection metadata source should preserve diagnostic metadata");
    assert_eq!(
        diagnostic.projection_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::Observed
    );
    assert_eq!(diagnostic.run_count, 1);
    assert_eq!(diagnostic.char_range_count, 1);
    assert_eq!(diagnostic.char_span, 56);
    assert_eq!(
        diagnostic.effective_content_len,
        "projection 候補behavior path reads projected segment metadata"
            .chars()
            .count()
    );
    assert_eq!(diagnostic.inline_preedit_run_count, 1);
    assert_eq!(diagnostic.preedit_run_count, 1);
    assert_eq!(diagnostic.projection_segment_count, 2);
    assert_eq!(diagnostic.per_run_projection_diagnostics.len(), 1);
    assert_eq!(
        diagnostic.per_run_projection_diagnostics[0].projection_segment_count,
        2
    );
    assert!(
        prewire
            .input()
            .readiness_behavior_blocked_reasons
            .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::ProjectionPathUnwired),
        "#47 metadata source must not rewrite the #44 behavior-path audit"
    );
    assert!(!prewire.projection_behavior_path_ready());
    assert!(!prewire.render_enabled());
    assert!(!prewire.layout_enabled());
    assert!(!prewire.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_projection_behavior_prewire_observes_no_projection_segments() {
    let mut run = TextAreaTextRun::new(
        "projection prewire no segment remains diagnostic".to_string(),
        0..47,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose projection metadata");
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );

    let prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        prewire.state(),
        TextAreaEditableIfcProjectionBehaviorPathPrewireState::ObservedNoProjectionSegments
    );
    assert!(prewire.blocked_reasons().is_empty());
    assert!(prewire.diagnostic_prewired());
    let diagnostic = prewire
        .diagnostic()
        .expect("no-projection-segments prewire should keep explicit diagnostic metadata");
    assert_eq!(
        diagnostic.projection_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedNoProjectionSegments
    );
    assert_eq!(diagnostic.projection_segment_count, 0);
    assert_eq!(diagnostic.per_run_projection_diagnostics.len(), 1);
    assert_eq!(
        diagnostic.per_run_projection_diagnostics[0].projection_segment_count,
        0
    );
    assert!(!prewire.projection_behavior_path_ready());
    assert!(!prewire.render_enabled());
    assert!(!prewire.layout_enabled());
    assert!(!prewire.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_projection_behavior_prewire_blocks_missing_metadata_source() {
    let run = TextAreaTextRun::new("unlaid out projection prewire".to_string(), 0..30);
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(
            run.inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
                .into_iter()
                .collect(),
        ),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );

    let prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        prewire.state(),
        TextAreaEditableIfcProjectionBehaviorPathPrewireState::Blocked
    );
    for reason in [
        TextAreaEditableIfcProjectionBehaviorPathPrewireBlockedReason::MissingRunMetadata,
        TextAreaEditableIfcProjectionBehaviorPathPrewireBlockedReason::
            ProjectionMetadataUnwired,
        TextAreaEditableIfcProjectionBehaviorPathPrewireBlockedReason::
            MissingProjectionMetadataDiagnostic,
    ] {
        assert!(
            prewire.blocked_reasons().contains(&reason),
            "missing projection prewire metadata should block on {reason:?}"
        );
    }
    assert!(!prewire.diagnostic_prewired());
    assert!(prewire.diagnostic().is_none());
    assert!(!prewire.projection_behavior_path_ready());
    assert!(!prewire.render_enabled());
    assert!(!prewire.layout_enabled());
    assert!(!prewire.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_projection_behavior_prewire_keeps_readiness_audit_diagnostic_only() {
    let mut run = TextAreaTextRun::new(
        "projection prewire does not authorize rollout".to_string(),
        0..45,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let mut payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose projection prewire metadata");
    payload
        .readiness
        .projection_diagnostic
        .projection_segment_count = 1;
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );
    let prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        audit.state(),
        TextAreaEditableIfcReadinessAuditState::Blocked
    );
    assert!(
        audit
            .behavior_path_blocked_reasons()
            .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::ProjectionPathUnwired),
        "readiness audit must keep projection behavior path blocked after metadata source"
    );
    assert_eq!(
        audit.recommendation(),
        TextAreaEditableIfcBehaviorPathRecommendation::KeepLegacyEditableBehaviorPath
    );
    assert!(prewire.diagnostic_prewired());
    assert!(!audit.render_enabled());
    assert!(!audit.layout_enabled());
    assert!(!audit.allows_text_area_editable_behavior_path_switch());
    assert!(!prewire.projection_behavior_path_ready());
    assert!(!prewire.render_enabled());
    assert!(!prewire.layout_enabled());
    assert!(!prewire.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_projection_behavior_prewire_preserves_ime_and_caret_metadata_sources() {
    let mut run = TextAreaTextRun::new(
        "projection prewire leaves IME and caret metadata intact".to_string(),
        0..54,
    );
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 11,
        preedit_text: "入力".to_string(),
        preedit_cursor: Some((1, 2)),
    }));
    run.set_preedit_run(true, Some((1, 3)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let mut payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out preedit run should expose projection prewire metadata");
    payload
        .readiness
        .projection_diagnostic
        .projection_segment_count = 1;
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );
    let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcImeBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
        TextAreaEditableIfcCaretAffinityMetadataSourceInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        projection_prewire.state(),
        TextAreaEditableIfcProjectionBehaviorPathPrewireState::DiagnosticPrewired
    );
    assert_eq!(
        ime_prewire.state(),
        TextAreaEditableIfcImeBehaviorPathPrewireState::DiagnosticPrewired
    );
    assert_eq!(
        caret_metadata_source.state(),
        TextAreaEditableIfcCaretAffinityMetadataSourceState::MetadataObserved
    );
    assert_eq!(
        ime_prewire
            .diagnostic()
            .expect("IME prewire should still preserve preedit metadata")
            .preedit_cursors,
        vec![(1, 3)]
    );
    assert_eq!(
        caret_metadata_source
            .diagnostic()
            .expect("caret metadata source should still preserve caret metadata")
            .preedit_cursor_count,
        1
    );
    assert!(!projection_prewire.projection_behavior_path_ready());
    assert!(!ime_prewire.ime_behavior_path_ready());
    assert!(!caret_metadata_source.caret_affinity_behavior_path_ready());
    assert!(!projection_prewire.allows_text_area_editable_behavior_path_switch());
    assert!(!ime_prewire.allows_text_area_editable_behavior_path_switch());
    assert!(!caret_metadata_source.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_scroll_follow_behavior_prewire_observes_scroll_metadata() {
    let mut run = TextAreaTextRun::new(
        "scroll follow behavior path reads layout and caret metadata".to_string(),
        4..61,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose scroll-follow metadata source metadata");
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );

    let prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        prewire.state(),
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::DiagnosticPrewired
    );
    assert!(prewire.blocked_reasons().is_empty());
    assert!(prewire.diagnostic_prewired());
    let diagnostic = prewire
        .diagnostic()
        .expect("scroll-follow metadata source should preserve diagnostic metadata");
    assert_eq!(
        diagnostic.scroll_follow_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::Observed
    );
    assert_eq!(diagnostic.run_count, 1);
    assert_eq!(diagnostic.layout_size_count, 1);
    assert_eq!(diagnostic.char_span, 57);
    assert_eq!(
        diagnostic.effective_content_len,
        "scroll follow behavior path reads layout and caret metadata"
            .chars()
            .count()
    );
    assert!(diagnostic.visual_line_count >= 1);
    assert!(diagnostic.caret_stop_count >= 1);
    assert_eq!(diagnostic.per_run_scroll_follow_diagnostics.len(), 1);
    assert!(
        diagnostic.per_run_scroll_follow_diagnostics[0].layout_size[0] > 0.0
            && diagnostic.per_run_scroll_follow_diagnostics[0].layout_size[1] > 0.0
    );
    assert!(
        prewire
            .input()
            .readiness_behavior_blocked_reasons
            .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::ScrollFollowPathUnwired),
        "#48 metadata source must not rewrite the #44 behavior-path audit"
    );
    assert!(!prewire.scroll_follow_behavior_path_ready());
    assert!(!prewire.render_enabled());
    assert!(!prewire.layout_enabled());
    assert!(!prewire.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_scroll_follow_behavior_prewire_observes_no_scroll_source() {
    let mut run = TextAreaTextRun::new(
        "scroll follow prewire no source remains diagnostic".to_string(),
        0..51,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let mut payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose scroll-follow metadata");
    payload.readiness.scroll_follow_diagnostic.layout_size = [0.0, 0.0];
    payload.readiness.scroll_follow_diagnostic.visual_line_count = 0;
    payload.readiness.scroll_follow_diagnostic.caret_stop_count = 0;
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );

    let prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        prewire.state(),
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::ObservedNoScrollSource
    );
    assert!(prewire.blocked_reasons().is_empty());
    assert!(prewire.diagnostic_prewired());
    let diagnostic = prewire
        .diagnostic()
        .expect("no-scroll-source prewire should keep explicit diagnostic metadata");
    assert_eq!(
        diagnostic.scroll_follow_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedNoScrollSource
    );
    assert_eq!(diagnostic.layout_size_count, 0);
    assert_eq!(diagnostic.visual_line_count, 0);
    assert_eq!(diagnostic.caret_stop_count, 0);
    assert_eq!(diagnostic.per_run_scroll_follow_diagnostics.len(), 1);
    assert_eq!(
        diagnostic.per_run_scroll_follow_diagnostics[0].layout_size,
        [0.0, 0.0]
    );
    assert!(!prewire.scroll_follow_behavior_path_ready());
    assert!(!prewire.render_enabled());
    assert!(!prewire.layout_enabled());
    assert!(!prewire.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_scroll_follow_behavior_prewire_blocks_missing_metadata_source() {
    let run = TextAreaTextRun::new("unlaid out scroll-follow prewire".to_string(), 0..34);
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(
            run.inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
                .into_iter()
                .collect(),
        ),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );

    let prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        prewire.state(),
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::Blocked
    );
    for reason in [
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireBlockedReason::MissingRunMetadata,
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireBlockedReason::
            ScrollFollowMetadataUnwired,
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireBlockedReason::
            MissingScrollFollowMetadataDiagnostic,
    ] {
        assert!(
            prewire.blocked_reasons().contains(&reason),
            "missing scroll-follow prewire metadata should block on {reason:?}"
        );
    }
    assert!(!prewire.diagnostic_prewired());
    assert!(prewire.diagnostic().is_none());
    assert!(!prewire.scroll_follow_behavior_path_ready());
    assert!(!prewire.render_enabled());
    assert!(!prewire.layout_enabled());
    assert!(!prewire.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_scroll_follow_behavior_prewire_keeps_readiness_audit_diagnostic_only() {
    let mut run = TextAreaTextRun::new(
        "scroll follow prewire does not authorize rollout".to_string(),
        0..48,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose scroll-follow prewire metadata");
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );
    let prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        audit.state(),
        TextAreaEditableIfcReadinessAuditState::Blocked
    );
    assert!(
        audit
            .behavior_path_blocked_reasons()
            .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::ScrollFollowPathUnwired),
        "readiness audit must keep scroll-follow behavior path blocked after metadata source"
    );
    assert_eq!(
        audit.recommendation(),
        TextAreaEditableIfcBehaviorPathRecommendation::KeepLegacyEditableBehaviorPath
    );
    assert!(prewire.diagnostic_prewired());
    assert!(!audit.render_enabled());
    assert!(!audit.layout_enabled());
    assert!(!audit.allows_text_area_editable_behavior_path_switch());
    assert!(!prewire.scroll_follow_behavior_path_ready());
    assert!(!prewire.render_enabled());
    assert!(!prewire.layout_enabled());
    assert!(!prewire.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_scroll_follow_behavior_prewire_preserves_prior_prewires() {
    let mut run = TextAreaTextRun::new(
        "scroll follow prewire leaves IME caret and projection intact".to_string(),
        0..60,
    );
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 14,
        preedit_text: "入力".to_string(),
        preedit_cursor: Some((1, 2)),
    }));
    run.set_preedit_run(true, Some((1, 3)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let mut payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out preedit run should expose scroll-follow prewire metadata");
    payload
        .readiness
        .projection_diagnostic
        .projection_segment_count = 1;
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );
    let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcImeBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
        TextAreaEditableIfcCaretAffinityMetadataSourceInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    assert_eq!(
        scroll_prewire.state(),
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::DiagnosticPrewired
    );
    assert_eq!(
        projection_prewire.state(),
        TextAreaEditableIfcProjectionBehaviorPathPrewireState::DiagnosticPrewired
    );
    assert_eq!(
        ime_prewire.state(),
        TextAreaEditableIfcImeBehaviorPathPrewireState::DiagnosticPrewired
    );
    assert_eq!(
        caret_metadata_source.state(),
        TextAreaEditableIfcCaretAffinityMetadataSourceState::MetadataObserved
    );
    assert_eq!(
        ime_prewire
            .diagnostic()
            .expect("IME prewire should still preserve preedit metadata")
            .preedit_cursors,
        vec![(1, 3)]
    );
    assert_eq!(
        caret_metadata_source
            .diagnostic()
            .expect("caret metadata source should still preserve caret metadata")
            .preedit_cursor_count,
        1
    );
    assert_eq!(
        projection_prewire
            .diagnostic()
            .expect("projection prewire should still preserve projection metadata")
            .projection_segment_count,
        1
    );
    assert!(!scroll_prewire.scroll_follow_behavior_path_ready());
    assert!(!projection_prewire.projection_behavior_path_ready());
    assert!(!ime_prewire.ime_behavior_path_ready());
    assert!(!caret_metadata_source.caret_affinity_behavior_path_ready());
    assert!(!scroll_prewire.allows_text_area_editable_behavior_path_switch());
    assert!(!projection_prewire.allows_text_area_editable_behavior_path_switch());
    assert!(!ime_prewire.allows_text_area_editable_behavior_path_switch());
    assert!(!caret_metadata_source.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_behavior_path_status_allows_behavior_status_observation_only() {
    let mut run = TextAreaTextRun::new(
        "behavior path status observes all metadata source diagnostics".to_string(),
        0..61,
    );
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 18,
        preedit_text: "入力".to_string(),
        preedit_cursor: Some((1, 2)),
    }));
    run.set_preedit_run(true, Some((1, 3)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let mut payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose behavior path status metadata");
    payload
        .readiness
        .projection_diagnostic
        .projection_segment_count = 1;
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );
    let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcImeBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
        TextAreaEditableIfcCaretAffinityMetadataSourceInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    let diagnostic = TextAreaEditableIfcBehaviorPathStatus::evaluate(
        TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
            &preflight,
            &audit,
            &ime_prewire,
            &caret_metadata_source,
            &projection_prewire,
            &scroll_prewire,
        ),
    );

    assert_eq!(
        diagnostic.state(),
        TextAreaEditableIfcBehaviorPathStatusState::ReadyForStatusObservation
    );
    assert!(diagnostic.blocked_reasons().is_empty());
    assert_eq!(
        diagnostic.recommendation(),
        TextAreaEditableIfcBehaviorPathStatusRecommendation::KeepLegacyEditableBehaviorPath
    );
    assert!(!diagnostic.render_enabled());
    assert!(!diagnostic.layout_enabled());
    assert!(!diagnostic.allows_text_area_editable_behavior_path_switch());
    assert_eq!(
        diagnostic.report().metadata_bridge_state,
        TextAreaInlineIfcMetadataBridgePreflightState::ReadyForDiagnosticEvaluation
    );
    assert_eq!(
        diagnostic.report().audit_state,
        TextAreaEditableIfcReadinessAuditState::Blocked
    );
    assert_eq!(
        diagnostic.report().audit_recommendation,
        TextAreaEditableIfcBehaviorPathRecommendation::KeepLegacyEditableBehaviorPath
    );
    assert_eq!(
        diagnostic.report().ime_prewire_state,
        TextAreaEditableIfcImeBehaviorPathPrewireState::DiagnosticPrewired
    );
    assert_eq!(
        diagnostic.report().caret_affinity_metadata_source_state,
        TextAreaEditableIfcCaretAffinityMetadataSourceState::MetadataObserved
    );
    assert_eq!(
        diagnostic.report().projection_prewire_state,
        TextAreaEditableIfcProjectionBehaviorPathPrewireState::DiagnosticPrewired
    );
    assert_eq!(
        diagnostic.report().scroll_follow_prewire_state,
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::DiagnosticPrewired
    );
    assert!(diagnostic.report().ime_diagnostic_prewired);
    assert!(diagnostic.report().caret_affinity_metadata_observed);
    assert!(diagnostic.report().projection_diagnostic_prewired);
    assert!(diagnostic.report().scroll_follow_diagnostic_prewired);
    assert!(!diagnostic.report().ime_behavior_path_ready);
    assert!(!diagnostic.report().caret_affinity_behavior_path_ready);
    assert!(!diagnostic.report().projection_behavior_path_ready);
    assert!(!diagnostic.report().scroll_follow_behavior_path_ready);
    for reason in [
        TextAreaEditableIfcBehaviorPathBlockedReason::ProjectionPathUnwired,
        TextAreaEditableIfcBehaviorPathBlockedReason::ImePathUnwired,
        TextAreaEditableIfcBehaviorPathBlockedReason::CaretAffinityPathUnwired,
        TextAreaEditableIfcBehaviorPathBlockedReason::ScrollFollowPathUnwired,
    ] {
        assert!(
            diagnostic
                .report()
                .behavior_path_blocked_reasons
                .contains(&reason),
            "behavior path status must preserve #44 behavior-path blocked reason {reason:?}"
        );
    }
    assert!(!ime_prewire.ime_behavior_path_ready());
    assert!(!caret_metadata_source.caret_affinity_behavior_path_ready());
    assert!(!projection_prewire.projection_behavior_path_ready());
    assert!(!scroll_prewire.scroll_follow_behavior_path_ready());
}

#[test]
fn text_area_inline_ifc_behavior_path_status_blocks_missing_prewire_source() {
    let run = TextAreaTextRun::new("unlaid out behavior path status".to_string(), 0..30);
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(
            run.inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
                .into_iter()
                .collect(),
        ),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );
    let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcImeBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
        TextAreaEditableIfcCaretAffinityMetadataSourceInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );

    let gate = TextAreaEditableIfcBehaviorPathStatus::evaluate(
        TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
            &preflight,
            &audit,
            &ime_prewire,
            &caret_metadata_source,
            &projection_prewire,
            &scroll_prewire,
        ),
    );

    assert_eq!(
        gate.state(),
        TextAreaEditableIfcBehaviorPathStatusState::Blocked
    );
    for reason in [
        TextAreaEditableIfcBehaviorPathStatusBlockedReason::MetadataBridgeBlocked,
        TextAreaEditableIfcBehaviorPathStatusBlockedReason::MetadataObservationIncomplete,
        TextAreaEditableIfcBehaviorPathStatusBlockedReason::ImePrewireBlocked,
        TextAreaEditableIfcBehaviorPathStatusBlockedReason::ImeDiagnosticNotObserved,
        TextAreaEditableIfcBehaviorPathStatusBlockedReason::CaretAffinityMetadataSourceBlocked,
        TextAreaEditableIfcBehaviorPathStatusBlockedReason::CaretAffinityMetadataNotObserved,
        TextAreaEditableIfcBehaviorPathStatusBlockedReason::ProjectionPrewireBlocked,
        TextAreaEditableIfcBehaviorPathStatusBlockedReason::ProjectionDiagnosticNotObserved,
        TextAreaEditableIfcBehaviorPathStatusBlockedReason::ScrollFollowPrewireBlocked,
        TextAreaEditableIfcBehaviorPathStatusBlockedReason::ScrollFollowDiagnosticNotObserved,
    ] {
        assert!(
            gate.blocked_reasons().contains(&reason),
            "missing behavior path status inputs should block on {reason:?}"
        );
    }
    assert!(!gate.render_enabled());
    assert!(!gate.layout_enabled());
    assert!(!gate.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_behavior_path_status_blocks_one_incomplete_prewire() {
    let mut run = TextAreaTextRun::new(
        "behavior path status keeps one missing prewire blocked".to_string(),
        0..54,
    );
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 12,
        preedit_text: "入力".to_string(),
        preedit_cursor: Some((1, 2)),
    }));
    run.set_preedit_run(true, Some((1, 3)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let mut payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose behavior path status metadata");
    payload
        .readiness
        .projection_diagnostic
        .projection_segment_count = 1;
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );
    let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcImeBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
        TextAreaEditableIfcCaretAffinityMetadataSourceInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let mut input =
        TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
            &preflight,
            &audit,
            &ime_prewire,
            &caret_metadata_source,
            &projection_prewire,
            &scroll_prewire,
        );
    input.ime_prewire_state = TextAreaEditableIfcImeBehaviorPathPrewireState::Blocked;
    input.ime_prewire_blocked_reasons =
        vec![TextAreaEditableIfcImeBehaviorPathPrewireBlockedReason::ImeMetadataUnwired];
    input.ime_diagnostic_prewired = false;

    let gate = TextAreaEditableIfcBehaviorPathStatus::evaluate(input);

    assert_eq!(
        gate.state(),
        TextAreaEditableIfcBehaviorPathStatusState::Blocked
    );
    assert!(
        gate.blocked_reasons()
            .contains(&TextAreaEditableIfcBehaviorPathStatusBlockedReason::ImePrewireBlocked)
    );
    assert!(
        gate.blocked_reasons().contains(
            &TextAreaEditableIfcBehaviorPathStatusBlockedReason::ImeDiagnosticNotObserved
        )
    );
    assert!(
        gate.report()
            .behavior_path_blocked_reasons
            .contains(&TextAreaEditableIfcBehaviorPathBlockedReason::ImePathUnwired),
        "behavior path status must keep #44 behavior path blocked reasons while blocking one prewire"
    );
    assert!(!gate.report().ime_behavior_path_ready);
    assert!(!gate.report().caret_affinity_behavior_path_ready);
    assert!(!gate.report().projection_behavior_path_ready);
    assert!(!gate.report().scroll_follow_behavior_path_ready);
    assert!(!gate.render_enabled());
    assert!(!gate.layout_enabled());
    assert!(!gate.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_behavior_path_status_uses_prepared_default_render() {
    let mut run = TextAreaTextRun::new(
        "behavior path status still renders through legacy text pass".to_string(),
        0..56,
    );
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 11,
        preedit_text: "入力".to_string(),
        preedit_cursor: Some((1, 2)),
    }));
    run.set_preedit_run(true, Some((1, 3)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let mut payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose behavior path status metadata");
    payload
        .readiness
        .projection_diagnostic
        .projection_segment_count = 1;
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );
    let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcImeBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
        TextAreaEditableIfcCaretAffinityMetadataSourceInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let gate = TextAreaEditableIfcBehaviorPathStatus::evaluate(
        TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
            &preflight,
            &audit,
            &ime_prewire,
            &caret_metadata_source,
            &projection_prewire,
            &scroll_prewire,
        ),
    );
    assert_eq!(
        gate.state(),
        TextAreaEditableIfcBehaviorPathStatusState::ReadyForStatusObservation
    );
    assert!(!gate.render_enabled());

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(180, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    run.build(&mut graph, &mut arena, ctx);
    let pass_names = graph
        .pass_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name.to_string())
        .collect::<Vec<_>>();

    assert_eq!(pass_names.len(), 1);
    assert!(
        pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
        "TextArea behavior path status does not own render switching; TextArea default render should emit TextPreparedInputPass for a valid candidate, got {}",
        pass_names[0]
    );
}

#[test]
fn text_area_inline_ifc_behavior_path_status_reports_blocked_switch() {
    let run = TextAreaTextRun::new("blocked behavior path status".to_string(), 0..32);
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(
            run.inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
                .into_iter()
                .collect(),
        ),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );
    let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcImeBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
        TextAreaEditableIfcCaretAffinityMetadataSourceInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let diagnostic = TextAreaEditableIfcBehaviorPathStatus::evaluate(
        TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
            &preflight,
            &audit,
            &ime_prewire,
            &caret_metadata_source,
            &projection_prewire,
            &scroll_prewire,
        ),
    );

    assert_eq!(
        diagnostic.state(),
        TextAreaEditableIfcBehaviorPathStatusState::Blocked
    );
    assert_eq!(
        diagnostic.recommendation(),
        TextAreaEditableIfcBehaviorPathStatusRecommendation::KeepLegacyEditableBehaviorPath
    );
    assert!(
        diagnostic.behavior_path_switch_blocked_reasons().contains(
            &TextAreaEditableIfcBehaviorPathStatusBlockedReason::BehaviorPathsStillNotReady
        )
    );
    assert!(diagnostic.report().prepared_render_default_observed);
    assert!(!diagnostic.render_enabled());
    assert!(!diagnostic.layout_enabled());
    assert!(!diagnostic.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_behavior_path_status_blocks_diagnostic_only_switch() {
    let diagnostic = text_area_inline_ifc_ready_behavior_path_status();

    assert_eq!(
        diagnostic.state(),
        TextAreaEditableIfcBehaviorPathStatusState::ReadyForStatusObservation
    );
    assert!(diagnostic.blocked_reasons().is_empty());
    assert!(
        diagnostic
            .behavior_path_switch_blocked_reasons()
            .contains(&TextAreaEditableIfcBehaviorPathStatusBlockedReason::StatusObservationOnly)
    );
    assert!(
        diagnostic.behavior_path_switch_blocked_reasons().contains(
            &TextAreaEditableIfcBehaviorPathStatusBlockedReason::BehaviorPathsStillNotReady
        )
    );
    assert!(!diagnostic.report().ime_behavior_path_ready);
    assert!(!diagnostic.report().caret_affinity_behavior_path_ready);
    assert!(!diagnostic.report().projection_behavior_path_ready);
    assert!(!diagnostic.report().scroll_follow_behavior_path_ready);
    assert!(diagnostic.report().prepared_render_default_observed);
    assert!(!diagnostic.render_enabled());
    assert!(!diagnostic.layout_enabled());
    assert!(!diagnostic.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_behavior_path_status_preserves_layout_with_prepared_render() {
    let diagnostic = text_area_inline_ifc_ready_behavior_path_status();
    let mut run = TextAreaTextRun::new(
        "behavior path status keeps layout unchanged".to_string(),
        0..63,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let before_size = run.layout_state.layout_size;
    let before_fragments = run.inline_fragment_positions();

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(180, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    run.build(&mut graph, &mut arena, ctx);
    let pass_names = graph
        .pass_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name.to_string())
        .collect::<Vec<_>>();

    assert_eq!(pass_names.len(), 1);
    assert!(
        pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
        "TextArea behavior path status does not own render switching; TextArea default render should emit TextPreparedInputPass for a valid candidate, got {}",
        pass_names[0]
    );
    assert_eq!(before_size.width, run.layout_state.layout_size.width);
    assert_eq!(before_size.height, run.layout_state.layout_size.height);
    assert_eq!(before_fragments, run.inline_fragment_positions());
    assert!(!diagnostic.render_enabled());
    assert!(!diagnostic.layout_enabled());
    assert!(!diagnostic.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_behavior_readiness_report_lists_all_blocked_paths() {
    let (behavior_status, caret_metadata_source) =
        text_area_inline_ifc_behavior_status_and_caret_metadata_source_for_readiness();

    let ready_report = behavior_status.readiness_report(&caret_metadata_source);

    assert_eq!(
        behavior_status.state(),
        TextAreaEditableIfcBehaviorPathStatusState::ReadyForStatusObservation
    );
    assert!(
        behavior_status
            .behavior_path_switch_blocked_reasons()
            .contains(
                &TextAreaEditableIfcBehaviorPathStatusBlockedReason::BehaviorPathsStillNotReady
            )
    );
    for reason in [
        TextAreaEditableIfcBehaviorPathReadinessBlockedReason::ImeBehaviorPathNotReady,
        TextAreaEditableIfcBehaviorPathReadinessBlockedReason::CaretAffinityBehaviorPathNotReady,
        TextAreaEditableIfcBehaviorPathReadinessBlockedReason::ProjectionBehaviorPathNotReady,
        TextAreaEditableIfcBehaviorPathReadinessBlockedReason::ScrollFollowBehaviorPathNotReady,
    ] {
        assert!(
            ready_report.blocked_reasons.contains(&reason),
            "readiness report must split BehaviorPathsStillNotReady into {reason:?}"
        );
    }
    assert_eq!(
        ready_report.ime_readiness_state,
        TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
    );
    assert_eq!(
        ready_report.projection_readiness_state,
        TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
    );
    assert_eq!(
        ready_report.scroll_follow_readiness_state,
        TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
    );
}

#[test]
fn text_area_inline_ifc_behavior_readiness_report_observes_caret_candidate_only() {
    let (behavior_status, caret_metadata_source) =
        text_area_inline_ifc_behavior_status_and_caret_metadata_source_for_readiness();

    let ready_report = behavior_status.readiness_report(&caret_metadata_source);

    assert_eq!(
        caret_metadata_source.state(),
        TextAreaEditableIfcCaretAffinityMetadataSourceState::MetadataObserved
    );
    assert!(caret_metadata_source.metadata_observed());
    assert_eq!(
        ready_report.caret_affinity_readiness_state,
        TextAreaEditableIfcBehaviorPathReadinessState::CaretAffinityCandidateObserved
    );
    assert_eq!(
        ready_report.caret_affinity_candidate,
        TextAreaEditableIfcBehaviorPathReadinessCandidate::CaretAffinityCandidateObserved
    );
    assert_eq!(
        ready_report.caret_affinity_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
    );
    assert!(ready_report.caret_affinity_caret_stop_count > 0);
    assert!(!ready_report.caret_affinity_behavior_path_ready);
    assert!(!behavior_status.report().caret_affinity_behavior_path_ready);
    assert!(!caret_metadata_source.caret_affinity_behavior_path_ready());
    assert!(!ready_report.ime_behavior_path_ready);
    assert!(!ready_report.projection_behavior_path_ready);
    assert!(!ready_report.scroll_follow_behavior_path_ready);
    assert!(!behavior_status.render_enabled());
    assert!(!behavior_status.layout_enabled());
    assert!(!behavior_status.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_behavior_readiness_report_preserves_layout_with_prepared_default() {
    let (behavior_status, caret_metadata_source) =
        text_area_inline_ifc_behavior_status_and_caret_metadata_source_for_readiness();
    let ready_report = behavior_status.readiness_report(&caret_metadata_source);
    let mut run = TextAreaTextRun::new(
        "behavior readiness preserves layout with prepared default render".to_string(),
        0..62,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let before_size = run.layout_state.layout_size;
    let before_fragments = run.inline_fragment_positions();

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(180, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    run.build(&mut graph, &mut arena, ctx);
    let pass_names = graph
        .pass_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name.to_string())
        .collect::<Vec<_>>();

    assert_eq!(pass_names.len(), 1);
    assert!(
        pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
        "TextArea behavior readiness does not own render switching; TextArea default render should emit TextPreparedInputPass for a valid candidate, got {}",
        pass_names[0]
    );
    assert_eq!(before_size.width, run.layout_state.layout_size.width);
    assert_eq!(before_size.height, run.layout_state.layout_size.height);
    assert_eq!(before_fragments, run.inline_fragment_positions());
    assert!(!ready_report.caret_affinity_behavior_path_ready);
    assert!(!behavior_status.render_enabled());
    assert!(!behavior_status.layout_enabled());
    assert!(!behavior_status.allows_text_area_editable_behavior_path_switch());
}

fn text_area_inline_ifc_caret_affinity_equivalence_fixture() -> (
    TextAreaEditableIfcBehaviorPathStatus,
    TextAreaEditableIfcCaretAffinityMetadataSource,
    TextAreaEditableIfcCaretAffinityEquivalenceAudit,
) {
    let (behavior_status, caret_metadata_source) =
        text_area_inline_ifc_behavior_status_and_caret_metadata_source_for_readiness();
    let audit = TextAreaEditableIfcCaretAffinityEquivalenceAudit::evaluate(
        TextAreaEditableIfcCaretAffinityEquivalenceAuditInput::
            from_behavior_status_and_caret_affinity_metadata_source(
                &behavior_status,
                &caret_metadata_source,
            ),
    );

    (behavior_status, caret_metadata_source, audit)
}

fn text_area_inline_ifc_all_equivalence_fixture() -> (
    TextAreaEditableIfcBehaviorPathStatus,
    TextAreaEditableIfcCaretAffinityEquivalenceAudit,
    TextAreaEditableIfcProjectionEquivalenceAudit,
    TextAreaEditableIfcScrollFollowEquivalenceAudit,
    TextAreaEditableIfcImeEquivalenceAudit,
) {
    let mut run = TextAreaTextRun::new(
        "observation equivalence status observes IME caret projection scroll".to_string(),
        0..67,
    );
    run.set_inline_preedit(Some(InlinePreedit {
        insert_at_local: 12,
        preedit_text: "入力".to_string(),
        preedit_cursor: Some((1, 2)),
    }));
    run.set_preedit_run(true, Some((1, 3)));
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let mut payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose all equivalence metadata");
    payload
        .readiness
        .projection_diagnostic
        .projection_segment_count = 2;
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );
    let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcImeBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
        TextAreaEditableIfcCaretAffinityMetadataSourceInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&audit, &preflight),
    );
    let behavior_status = TextAreaEditableIfcBehaviorPathStatus::evaluate(
        TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
            &preflight,
            &audit,
            &ime_prewire,
            &caret_metadata_source,
            &projection_prewire,
            &scroll_prewire,
        ),
    );
    let caret_audit = TextAreaEditableIfcCaretAffinityEquivalenceAudit::evaluate(
        TextAreaEditableIfcCaretAffinityEquivalenceAuditInput::
            from_behavior_status_and_caret_affinity_metadata_source(
                &behavior_status,
                &caret_metadata_source,
            ),
    );
    let projection_audit = TextAreaEditableIfcProjectionEquivalenceAudit::evaluate(
        TextAreaEditableIfcProjectionEquivalenceAuditInput::
            from_behavior_status_and_projection_prewire(&behavior_status, &projection_prewire),
    );
    let scroll_audit = TextAreaEditableIfcScrollFollowEquivalenceAudit::evaluate(
        TextAreaEditableIfcScrollFollowEquivalenceAuditInput::
            from_behavior_status_and_scroll_follow_prewire(&behavior_status, &scroll_prewire),
    );
    let ime_audit = TextAreaEditableIfcImeEquivalenceAudit::evaluate(
        TextAreaEditableIfcImeEquivalenceAuditInput::from_behavior_status_and_ime_prewire(
            &behavior_status,
            &ime_prewire,
        ),
    );

    (
        behavior_status,
        caret_audit,
        projection_audit,
        scroll_audit,
        ime_audit,
    )
}

#[test]
fn text_area_inline_ifc_caret_affinity_equivalence_audit_observes_observation_only_candidate() {
    let (behavior_status, caret_metadata_source, audit) =
        text_area_inline_ifc_caret_affinity_equivalence_fixture();
    let metadata_source_diagnostic = caret_metadata_source
        .diagnostic()
        .expect("fixture should expose caret metadata source diagnostic");

    assert_eq!(
        audit.state(),
        TextAreaEditableIfcCaretAffinityEquivalenceAuditState::ReadyForObservationOnly
    );
    assert!(audit.blocked_reasons().is_empty());
    assert_eq!(
        audit.diagnostic().equivalent_candidate,
        TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate::EquivalentCandidateObserved
    );
    assert_eq!(
        audit.recommendation(),
        TextAreaEditableIfcCaretAffinityEquivalenceAuditRecommendation::ObservationOnlyNoOp
    );
    assert_eq!(
        audit.diagnostic().caret_affinity_candidate,
        TextAreaEditableIfcBehaviorPathReadinessCandidate::CaretAffinityCandidateObserved
    );
    assert_eq!(
        audit.diagnostic().caret_affinity_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
    );
    assert_eq!(
        audit.diagnostic().visual_line_count,
        metadata_source_diagnostic.visual_line_count
    );
    assert_eq!(
        audit.diagnostic().caret_stop_count,
        metadata_source_diagnostic.caret_stop_count
    );
    assert_eq!(
        audit.diagnostic().multi_stop_line_count,
        metadata_source_diagnostic.multi_stop_line_count
    );
    assert_eq!(
        audit.diagnostic().preedit_cursor_count,
        metadata_source_diagnostic.preedit_cursor_count
    );
    assert!(!audit.caret_affinity_behavior_path_ready());
    assert!(!audit.diagnostic().caret_affinity_behavior_path_ready);
    assert!(
        !behavior_status
            .readiness_report(&caret_metadata_source)
            .caret_affinity_behavior_path_ready
    );
    assert!(!behavior_status.report().caret_affinity_behavior_path_ready);
    assert!(!audit.render_enabled());
    assert!(!audit.layout_enabled());
    assert!(!audit.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_caret_affinity_behavior_input_uses_equivalent_metadata_only() {
    let (behavior_status, caret_metadata_source, audit) =
        text_area_inline_ifc_caret_affinity_equivalence_fixture();
    let behavior_input =
        TextAreaEditableIfcCaretAffinityBehaviorInput::from_equivalence_audit(&audit);

    assert!(behavior_input.equivalent_candidate_observed);
    assert_eq!(
        behavior_input.caret_affinity_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
    );
    assert_eq!(
        behavior_input.visual_line_count,
        audit.diagnostic().visual_line_count
    );
    assert_eq!(
        behavior_input.caret_stop_count,
        audit.diagnostic().caret_stop_count
    );
    assert_eq!(
        behavior_input.multi_stop_line_count,
        audit.diagnostic().multi_stop_line_count
    );
    assert_eq!(
        behavior_input.preedit_cursor_count,
        audit.diagnostic().preedit_cursor_count
    );
    assert_eq!(
        behavior_input.per_run_caret_diagnostics,
        audit.diagnostic().per_run_caret_diagnostics
    );
    assert!(!behavior_input.caret_affinity_behavior_path_ready());
    assert!(!behavior_input.render_enabled());
    assert!(!behavior_input.layout_enabled());
    assert!(!behavior_input.allows_text_area_editable_behavior_path_switch());
    assert!(
        !behavior_status
            .readiness_report(&caret_metadata_source)
            .caret_affinity_behavior_path_ready
    );
    assert!(!behavior_status.report().ime_behavior_path_ready);
    assert!(!behavior_status.report().projection_behavior_path_ready);
    assert!(!behavior_status.report().scroll_follow_behavior_path_ready);
}

#[test]
fn text_area_inline_ifc_caret_affinity_behavior_evaluation_reads_behavior_input_metadata() {
    let (behavior_status, caret_metadata_source, audit) =
        text_area_inline_ifc_caret_affinity_equivalence_fixture();
    let behavior_input =
        TextAreaEditableIfcCaretAffinityBehaviorInput::from_equivalence_audit(&audit);
    let evaluation =
        TextAreaEditableIfcCaretAffinityBehaviorEvaluation::evaluate(behavior_input.clone());

    assert_eq!(
        evaluation.state(),
        TextAreaEditableIfcCaretAffinityBehaviorEvaluationState::InputObserved
    );
    assert!(evaluation.blocked_reasons().is_empty());
    assert_eq!(evaluation.input(), &behavior_input);
    assert!(evaluation.diagnostic().equivalent_candidate_observed);
    assert!(evaluation.diagnostic().input_observed);
    assert_eq!(
        evaluation.diagnostic().caret_affinity_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::ObservedCaretStops
    );
    assert_eq!(
        evaluation.diagnostic().visual_line_count,
        behavior_input.visual_line_count
    );
    assert_eq!(
        evaluation.diagnostic().caret_stop_count,
        behavior_input.caret_stop_count
    );
    assert_eq!(
        evaluation.diagnostic().multi_stop_line_count,
        behavior_input.multi_stop_line_count
    );
    assert_eq!(
        evaluation.diagnostic().preedit_cursor_count,
        behavior_input.preedit_cursor_count
    );
    assert_eq!(
        evaluation.diagnostic().per_run_caret_diagnostics,
        behavior_input.per_run_caret_diagnostics
    );
    assert!(!evaluation.caret_affinity_behavior_path_ready());
    assert!(!evaluation.diagnostic().caret_affinity_behavior_path_ready);
    assert!(!evaluation.render_enabled());
    assert!(!evaluation.layout_enabled());
    assert!(!evaluation.allows_text_area_editable_behavior_path_switch());
    assert!(
        !behavior_status
            .readiness_report(&caret_metadata_source)
            .caret_affinity_behavior_path_ready
    );
    assert!(!behavior_status.report().ime_behavior_path_ready);
    assert!(!behavior_status.report().projection_behavior_path_ready);
    assert!(!behavior_status.report().scroll_follow_behavior_path_ready);
}

#[test]
fn text_area_inline_ifc_caret_affinity_read_only_lookup_reads_behavior_evaluation() {
    let (behavior_status, caret_metadata_source, audit) =
        text_area_inline_ifc_caret_affinity_equivalence_fixture();
    let behavior_input =
        TextAreaEditableIfcCaretAffinityBehaviorInput::from_equivalence_audit(&audit);
    let evaluation =
        TextAreaEditableIfcCaretAffinityBehaviorEvaluation::evaluate(behavior_input.clone());
    let adapter = TextAreaEditableIfcCaretAffinityReadOnlyLookupAdapter::from_behavior_evaluation(
        &evaluation,
    );
    let lookup = adapter
        .lookup()
        .expect("observed behavior evaluation should expose read-only lookup");

    assert_eq!(
        adapter.state(),
        TextAreaEditableIfcCaretAffinityReadOnlyLookupState::ReadOnlyLookupObserved
    );
    assert!(adapter.blocked_reasons().is_empty());
    assert_eq!(lookup.visual_line_count, behavior_input.visual_line_count);
    assert_eq!(lookup.caret_stop_count, behavior_input.caret_stop_count);
    assert_eq!(
        lookup.multi_stop_line_count,
        behavior_input.multi_stop_line_count
    );
    assert_eq!(
        lookup.preedit_cursor_count,
        behavior_input.preedit_cursor_count
    );
    assert_eq!(
        lookup.per_run_caret_diagnostics,
        behavior_input.per_run_caret_diagnostics
    );
    assert!(!adapter.caret_affinity_behavior_path_ready());
    assert!(!adapter.render_enabled());
    assert!(!adapter.layout_enabled());
    assert!(!adapter.allows_text_area_editable_behavior_path_switch());
    assert!(
        !behavior_status
            .readiness_report(&caret_metadata_source)
            .caret_affinity_behavior_path_ready
    );
    assert!(!behavior_status.report().ime_behavior_path_ready);
    assert!(!behavior_status.report().projection_behavior_path_ready);
    assert!(!behavior_status.report().scroll_follow_behavior_path_ready);
}

#[test]
fn text_area_inline_ifc_caret_affinity_helper_reads_read_only_lookup_metadata() {
    let (behavior_status, caret_metadata_source, audit) =
        text_area_inline_ifc_caret_affinity_equivalence_fixture();
    let behavior_input =
        TextAreaEditableIfcCaretAffinityBehaviorInput::from_equivalence_audit(&audit);
    let evaluation =
        TextAreaEditableIfcCaretAffinityBehaviorEvaluation::evaluate(behavior_input.clone());
    let adapter = TextAreaEditableIfcCaretAffinityReadOnlyLookupAdapter::from_behavior_evaluation(
        &evaluation,
    );
    let helper = adapter
        .behavior_helper()
        .expect("observed read-only lookup should expose caret affinity helper");

    assert_eq!(
        helper.line_summary(),
        (
            behavior_input.visual_line_count,
            behavior_input.multi_stop_line_count
        )
    );
    assert_eq!(
        helper.stop_summary(),
        (
            behavior_input.caret_stop_count,
            behavior_input.multi_stop_line_count
        )
    );
    assert_eq!(
        helper.preedit_cursor_metadata(),
        (
            behavior_input.preedit_cursor_count,
            behavior_input
                .per_run_caret_diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.has_preedit_cursor)
                .map(|diagnostic| diagnostic.preedit_cursor)
                .collect::<Vec<_>>()
        )
    );
    assert_eq!(
        helper.per_run_caret_diagnostics(),
        behavior_input.per_run_caret_diagnostics.as_slice()
    );
    assert_eq!(
        helper.caret_stop_snapshots(),
        collect_text_area_editable_ifc_caret_affinity_stop_snapshots(
            &behavior_input.per_run_caret_diagnostics
        )
        .as_slice()
    );
    assert_eq!(
        helper.placement_navigation_summary(),
        TextAreaEditableIfcCaretAffinityPlacementNavigationSummary {
            visual_line_count: behavior_input.visual_line_count,
            caret_stop_count: behavior_input.caret_stop_count,
            multi_stop_line_count: behavior_input.multi_stop_line_count,
            per_run_visual_line_counts: behavior_input
                .per_run_caret_diagnostics
                .iter()
                .map(|diagnostic| diagnostic.visual_line_count)
                .collect(),
            per_run_caret_stop_counts: behavior_input
                .per_run_caret_diagnostics
                .iter()
                .map(|diagnostic| diagnostic.caret_stop_count)
                .collect(),
            per_run_multi_stop_line_counts: behavior_input
                .per_run_caret_diagnostics
                .iter()
                .map(|diagnostic| diagnostic.multi_stop_line_count)
                .collect(),
            has_affinity_slots: behavior_input.multi_stop_line_count > 0,
            preedit_cursor_count: behavior_input.preedit_cursor_count,
            preedit_cursors: behavior_input
                .per_run_caret_diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.has_preedit_cursor)
                .map(|diagnostic| diagnostic.preedit_cursor)
                .collect(),
            caret_stop_snapshot_count: behavior_input.caret_stop_count,
            run_local_char_indices_available: true,
            run_local_geometry_available: true,
        }
    );
    assert!(!helper.caret_affinity_behavior_path_ready());
    assert!(!helper.render_enabled());
    assert!(!helper.layout_enabled());
    assert!(!helper.allows_text_area_editable_behavior_path_switch());
    assert!(!adapter.caret_affinity_behavior_path_ready());
    assert!(!adapter.render_enabled());
    assert!(!adapter.layout_enabled());
    assert!(!adapter.allows_text_area_editable_behavior_path_switch());
    assert!(
        !behavior_status
            .readiness_report(&caret_metadata_source)
            .caret_affinity_behavior_path_ready
    );
    assert!(!behavior_status.report().ime_behavior_path_ready);
    assert!(!behavior_status.report().projection_behavior_path_ready);
    assert!(!behavior_status.report().scroll_follow_behavior_path_ready);
}

#[test]
fn text_area_inline_ifc_caret_affinity_helper_provides_ifc_caret_placement() {
    let mut run = TextAreaTextRun::new(
        "caret helper provides visible caret placement from ifc adapter".to_string(),
        0..62,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 140.0);
    let seed_caret_lines = run.seed_caret_stops_for_ifc_snapshot();
    let seed_visual_line_count = seed_caret_lines.len();
    let seed_caret_stop_count = seed_caret_lines
        .iter()
        .map(|line| line.stops.len())
        .sum::<usize>();
    let seed_multi_stop_line_count = seed_caret_lines
        .iter()
        .filter(|line| line.stops.len() > 1)
        .count();
    let seed_caret_stop_snapshots = seed_caret_lines
        .iter()
        .enumerate()
        .flat_map(|(visual_line_index, line)| {
            line.stops
                .iter()
                .enumerate()
                .map(
                    move |(stop_index, stop)| TextAreaEditableIfcCaretAffinityStopSnapshot {
                        run_index: 0,
                        visual_line_index,
                        stop_index,
                        local_char: stop.local_char,
                        local_x: stop.local_x,
                        local_y_top: stop.local_y_top,
                        height: stop.height,
                    },
                )
        })
        .collect::<Vec<_>>();
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose read-only caret metadata");
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    let readiness_audit = TextAreaEditableIfcReadinessAudit::evaluate(
        TextAreaEditableIfcReadinessAuditInput::from_metadata_bridge_preflight(&preflight),
    );
    let ime_prewire = TextAreaEditableIfcImeBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcImeBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&readiness_audit, &preflight),
    );
    let caret_metadata_source = TextAreaEditableIfcCaretAffinityMetadataSource::evaluate(
        TextAreaEditableIfcCaretAffinityMetadataSourceInput::
            from_readiness_audit_and_metadata_bridge_preflight(&readiness_audit, &preflight),
    );
    let projection_prewire = TextAreaEditableIfcProjectionBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcProjectionBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&readiness_audit, &preflight),
    );
    let scroll_prewire = TextAreaEditableIfcScrollFollowBehaviorPathPrewire::evaluate(
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireInput::
            from_readiness_audit_and_metadata_bridge_preflight(&readiness_audit, &preflight),
    );
    let behavior_status = TextAreaEditableIfcBehaviorPathStatus::evaluate(
        TextAreaEditableIfcBehaviorPathStatusInput::from_bridge_audit_and_behavior_prewires(
            &preflight,
            &readiness_audit,
            &ime_prewire,
            &caret_metadata_source,
            &projection_prewire,
            &scroll_prewire,
        ),
    );
    let audit = TextAreaEditableIfcCaretAffinityEquivalenceAudit::evaluate(
        TextAreaEditableIfcCaretAffinityEquivalenceAuditInput::
            from_behavior_status_and_caret_affinity_metadata_source(
                &behavior_status,
                &caret_metadata_source,
            ),
    );
    let behavior_input =
        TextAreaEditableIfcCaretAffinityBehaviorInput::from_equivalence_audit(&audit);
    let evaluation =
        TextAreaEditableIfcCaretAffinityBehaviorEvaluation::evaluate(behavior_input.clone());
    let adapter = TextAreaEditableIfcCaretAffinityReadOnlyLookupAdapter::from_behavior_evaluation(
        &evaluation,
    );
    let helper = adapter
        .behavior_helper()
        .expect("observed read-only lookup should expose caret affinity helper");
    let placement_adapter = adapter
        .placement_read_only_adapter()
        .expect("observed lookup should expose read-only placement adapter");
    let summary = helper.placement_navigation_summary();
    let snapshots = helper.caret_stop_snapshots();

    assert_eq!(summary.visual_line_count, seed_visual_line_count);
    assert_eq!(summary.caret_stop_count, seed_caret_stop_count);
    assert_eq!(summary.multi_stop_line_count, seed_multi_stop_line_count);
    assert_eq!(
        summary.per_run_visual_line_counts,
        vec![seed_visual_line_count]
    );
    assert_eq!(
        summary.per_run_caret_stop_counts,
        vec![seed_caret_stop_count]
    );
    assert_eq!(
        summary.per_run_multi_stop_line_counts,
        vec![seed_multi_stop_line_count]
    );
    assert!(summary.has_affinity_slots);
    assert_eq!(summary.preedit_cursor_count, 0);
    assert!(summary.preedit_cursors.is_empty());
    assert_eq!(summary.caret_stop_snapshot_count, seed_caret_stop_count);
    assert!(
        summary.run_local_char_indices_available,
        "read-only lookup should carry per-stop local_char"
    );
    assert!(
        summary.run_local_geometry_available,
        "read-only lookup should carry per-stop x/y/height geometry"
    );
    assert_eq!(snapshots, seed_caret_stop_snapshots.as_slice());
    assert_eq!(
        snapshots
            .iter()
            .map(|snapshot| snapshot.local_char)
            .collect::<Vec<_>>(),
        seed_caret_stop_snapshots
            .iter()
            .map(|snapshot| snapshot.local_char)
            .collect::<Vec<_>>()
    );
    assert!(
        snapshots.iter().all(|snapshot| snapshot.height > 0.0),
        "read-only snapshot should preserve usable caret geometry"
    );
    let first_snapshot = seed_caret_stop_snapshots
        .first()
        .expect("IFC snapshot seed caret stops should expose at least one snapshot");
    assert_eq!(
        helper.stop_geometry_summary(
            first_snapshot.run_index,
            first_snapshot.visual_line_index,
            first_snapshot.stop_index,
        ),
        Some(TextAreaEditableIfcCaretAffinityStopGeometrySummary {
            run_index: first_snapshot.run_index,
            visual_line_index: first_snapshot.visual_line_index,
            stop_index: first_snapshot.stop_index,
            local_char: first_snapshot.local_char,
            local_x: first_snapshot.local_x,
            local_y_top: first_snapshot.local_y_top,
            height: first_snapshot.height,
        })
    );
    let expected_local_char_candidates = seed_caret_stop_snapshots
        .iter()
        .filter(|snapshot| {
            snapshot.run_index == first_snapshot.run_index
                && snapshot.local_char == first_snapshot.local_char
        })
        .enumerate()
        .map(|(candidate_index, snapshot)| {
            let affinity = if seed_caret_stop_snapshots
                .iter()
                .filter(|candidate| {
                    candidate.run_index == first_snapshot.run_index
                        && candidate.local_char == first_snapshot.local_char
                })
                .count()
                > 1
                && candidate_index == 0
            {
                super::super::caret_map::CaretAffinity::Upstream
            } else {
                super::super::caret_map::CaretAffinity::Downstream
            };
            TextAreaEditableIfcCaretAffinityLocalCharCandidate {
                run_index: snapshot.run_index,
                local_char: snapshot.local_char,
                affinity,
                visual_line_index: snapshot.visual_line_index,
                stop_index: snapshot.stop_index,
                local_x: snapshot.local_x,
                local_y_top: snapshot.local_y_top,
                height: snapshot.height,
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        helper.local_char_candidates(first_snapshot.run_index, first_snapshot.local_char),
        expected_local_char_candidates
    );
    assert!(
        helper
            .local_char_candidates(first_snapshot.run_index, usize::MAX)
            .is_empty(),
        "missing local_char must not fabricate read-only candidates"
    );
    let downstream_candidate = helper
        .local_char_candidate_with_affinity(
            first_snapshot.run_index,
            first_snapshot.local_char,
            super::super::caret_map::CaretAffinity::Downstream,
        )
        .expect("read-only lookup should expose downstream-like local_char candidate");
    let downstream_geometry = run
        .local_char_to_screen_position_with_affinity(
            first_snapshot.local_char,
            super::super::caret_map::CaretAffinity::Downstream,
        )
        .expect("IFC caret placement should expose downstream geometry");
    assert_eq!(
        (
            downstream_candidate.local_x,
            downstream_candidate.local_y_top,
            downstream_candidate.height,
        ),
        downstream_geometry
    );
    assert_eq!(
        run.caret_affinity_placement_position_from_ifc(
            first_snapshot.local_char,
            super::super::caret_map::CaretAffinity::Downstream,
        ),
        Some(downstream_geometry),
        "visible caret placement should be sourced from the IFC placement adapter"
    );
    assert_eq!(
        placement_adapter.local_char_candidate_with_affinity(
            first_snapshot.run_index,
            first_snapshot.local_char,
            super::super::caret_map::CaretAffinity::Downstream,
        ),
        Some(TextAreaEditableIfcCaretAffinityPlacementCandidate {
            run_index: downstream_candidate.run_index,
            local_char: downstream_candidate.local_char,
            affinity: downstream_candidate.affinity,
            local_x: downstream_candidate.local_x,
            local_y_top: downstream_candidate.local_y_top,
            height: downstream_candidate.height,
        })
    );
    assert_eq!(
        placement_adapter.local_char_to_run_local_position_with_affinity(
            first_snapshot.run_index,
            first_snapshot.local_char,
            super::super::caret_map::CaretAffinity::Downstream,
        ),
        Some(downstream_geometry)
    );
    let visible_caret_lines = run.caret_stops();
    assert_eq!(visible_caret_lines.len(), summary.visual_line_count);
    assert_eq!(
        visible_caret_lines
            .iter()
            .map(|line| line.stops.len())
            .sum::<usize>(),
        summary.caret_stop_count
    );
    assert_eq!(
        visible_caret_lines
            .iter()
            .enumerate()
            .flat_map(|(visual_line_index, line)| {
                line.stops
                    .iter()
                    .enumerate()
                    .map(
                        move |(stop_index, stop)| TextAreaEditableIfcCaretAffinityStopSnapshot {
                            run_index: 0,
                            visual_line_index,
                            stop_index,
                            local_char: stop.local_char,
                            local_x: stop.local_x,
                            local_y_top: stop.local_y_top,
                            height: stop.height,
                        },
                    )
            })
            .collect::<Vec<_>>(),
        snapshots,
        "visible caret_stops should be reconstructed from read-only IFC snapshots"
    );
    assert_eq!(
        visible_caret_lines
            .iter()
            .map(|line| (line.local_y_top, line.local_y_bottom))
            .collect::<Vec<_>>(),
        seed_caret_lines
            .iter()
            .map(|line| (line.local_y_top, line.local_y_bottom))
            .collect::<Vec<_>>(),
        "IFC snapshot-backed caret_stops should preserve visible line geometry"
    );
    let hit_test_pair = snapshots
        .windows(2)
        .find(|pair| {
            pair[0].run_index == 0
                && pair[1].run_index == 0
                && pair[0].visual_line_index == pair[1].visual_line_index
                && pair[1].local_x > pair[0].local_x
        })
        .expect("fixture should expose adjacent IFC caret stops on one visual line");
    let left_snapshot = hit_test_pair[0].clone();
    let right_snapshot = hit_test_pair[1].clone();
    let midpoint = left_snapshot.local_x + (right_snapshot.local_x - left_snapshot.local_x) / 2.0;
    let left_hit_x = left_snapshot.local_x + (midpoint - left_snapshot.local_x) / 2.0;
    let right_hit_x = midpoint + (right_snapshot.local_x - midpoint) / 2.0;
    let hit_y = left_snapshot.local_y_top + left_snapshot.height / 2.0;
    assert_eq!(
        run.screen_position_to_local_char_from_ifc(left_hit_x, hit_y),
        Some(left_snapshot.local_char),
        "IFC helper hit-test should choose the nearest snapshot stop before the midpoint"
    );
    assert_eq!(
        run.screen_position_to_local_char(left_hit_x, hit_y),
        Some(left_snapshot.local_char),
        "visible hit-test should be sourced from the IFC snapshot helper"
    );
    assert_eq!(
        run.screen_position_to_local_char_from_ifc(right_hit_x, hit_y),
        Some(right_snapshot.local_char),
        "IFC helper hit-test should choose the nearest snapshot stop after the midpoint"
    );
    assert_eq!(
        run.screen_position_to_local_char(right_hit_x, hit_y),
        Some(right_snapshot.local_char),
        "visible hit-test should stay aligned with IFC snapshot-derived local chars"
    );
    assert!(
        helper
            .local_char_candidate_with_affinity(
                first_snapshot.run_index,
                usize::MAX,
                super::super::caret_map::CaretAffinity::Downstream,
            )
            .is_none(),
        "missing local_char must not expose affinity candidate"
    );
    assert_eq!(
        run.caret_affinity_placement_position_from_ifc(
            usize::MAX,
            super::super::caret_map::CaretAffinity::Downstream,
        ),
        None,
        "missing adapter candidate must not expose IFC placement"
    );
    assert_eq!(
        run.local_char_to_screen_position_with_affinity(
            usize::MAX,
            super::super::caret_map::CaretAffinity::Downstream,
        ),
        None,
        "laid-out visible caret placement should require an IFC adapter candidate"
    );
    assert!(!helper.caret_affinity_behavior_path_ready());
    assert!(!adapter.render_enabled());
    assert!(!adapter.layout_enabled());
    assert!(!adapter.allows_text_area_editable_behavior_path_switch());
    assert!(
        !behavior_status
            .readiness_report(&caret_metadata_source)
            .caret_affinity_behavior_path_ready
    );
    assert!(!behavior_status.report().ime_behavior_path_ready);
    assert!(!behavior_status.report().projection_behavior_path_ready);
    assert!(!behavior_status.report().scroll_follow_behavior_path_ready);
}

#[test]
fn text_area_inline_ifc_caret_affinity_behavior_evaluation_blocks_incomplete_input() {
    let (_behavior_status, _caret_metadata_source, audit) =
        text_area_inline_ifc_caret_affinity_equivalence_fixture();
    let mut behavior_input =
        TextAreaEditableIfcCaretAffinityBehaviorInput::from_equivalence_audit(&audit);
    behavior_input.equivalent_candidate_observed = false;
    behavior_input.caret_affinity_metadata_status = TextAreaInlineIfcMetadataBridgeStatus::Unwired;
    behavior_input.caret_stop_count = 0;
    behavior_input.multi_stop_line_count = 0;
    behavior_input.preedit_cursor_count += 1;

    let evaluation = TextAreaEditableIfcCaretAffinityBehaviorEvaluation::evaluate(behavior_input);

    assert_eq!(
        evaluation.state(),
        TextAreaEditableIfcCaretAffinityBehaviorEvaluationState::Blocked
    );
    for reason in [
        TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::
            EquivalentCandidateMissing,
        TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::
            CaretAffinityMetadataStatusNotObservedCaretStops,
        TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::MissingCaretStops,
        TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::MissingAffinitySlots,
        TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::
            CaretMetadataShapeMismatch,
        TextAreaEditableIfcCaretAffinityBehaviorEvaluationBlockedReason::
            PreeditCursorMetadataIncomplete,
    ] {
        assert!(
            evaluation.blocked_reasons().contains(&reason),
            "incomplete behavior input should block on {reason:?}"
        );
    }
    assert!(!evaluation.diagnostic().input_observed);
    assert!(!evaluation.caret_affinity_behavior_path_ready());
    assert!(!evaluation.render_enabled());
    assert!(!evaluation.layout_enabled());
    assert!(!evaluation.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_caret_affinity_read_only_lookup_blocks_incomplete_evaluation() {
    let (_behavior_status, _caret_metadata_source, audit) =
        text_area_inline_ifc_caret_affinity_equivalence_fixture();
    let mut behavior_input =
        TextAreaEditableIfcCaretAffinityBehaviorInput::from_equivalence_audit(&audit);
    behavior_input.equivalent_candidate_observed = false;
    behavior_input.caret_affinity_metadata_status = TextAreaInlineIfcMetadataBridgeStatus::Unwired;
    behavior_input.visual_line_count = 0;
    behavior_input.caret_stop_count = 0;
    behavior_input.multi_stop_line_count = 0;
    behavior_input.preedit_cursor_count += 1;

    let evaluation = TextAreaEditableIfcCaretAffinityBehaviorEvaluation::evaluate(behavior_input);
    let adapter = TextAreaEditableIfcCaretAffinityReadOnlyLookupAdapter::from_behavior_evaluation(
        &evaluation,
    );

    assert_eq!(
        adapter.state(),
        TextAreaEditableIfcCaretAffinityReadOnlyLookupState::Blocked
    );
    assert_eq!(
        adapter.blocked_reasons(),
        &[TextAreaEditableIfcCaretAffinityReadOnlyLookupBlockedReason::
            BehaviorEvaluationNotObserved]
    );
    assert!(adapter.lookup().is_none());
    assert!(adapter.behavior_helper().is_none());
    assert!(adapter.placement_read_only_adapter().is_none());
    assert!(!adapter.caret_affinity_behavior_path_ready());
    assert!(!adapter.render_enabled());
    assert!(!adapter.layout_enabled());
    assert!(!adapter.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_caret_affinity_equivalence_audit_blocks_missing_or_incomplete_sources() {
    let (behavior_status, caret_metadata_source, _audit) =
        text_area_inline_ifc_caret_affinity_equivalence_fixture();
    let base_input = TextAreaEditableIfcCaretAffinityEquivalenceAuditInput::
        from_behavior_status_and_caret_affinity_metadata_source(&behavior_status, &caret_metadata_source);

    let mut missing_stops_input = base_input.clone();
    missing_stops_input.visual_line_count = 0;
    missing_stops_input.caret_stop_count = 0;
    missing_stops_input.per_run_caret_diagnostics.clear();
    let missing_stops =
        TextAreaEditableIfcCaretAffinityEquivalenceAudit::evaluate(missing_stops_input);
    assert_eq!(
        missing_stops.state(),
        TextAreaEditableIfcCaretAffinityEquivalenceAuditState::Blocked
    );
    assert!(missing_stops.blocked_reasons().contains(
        &TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::MissingCaretStops
    ));
    assert!(missing_stops.blocked_reasons().contains(
        &TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::
            CaretAffinityMetadataSourceMissing
    ));
    assert_eq!(
        missing_stops.diagnostic().equivalent_candidate,
        TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate::None
    );

    let mut no_affinity_slots_input = base_input.clone();
    no_affinity_slots_input.multi_stop_line_count = 0;
    for diagnostic in &mut no_affinity_slots_input.per_run_caret_diagnostics {
        diagnostic.multi_stop_line_count = 0;
    }
    let no_affinity_slots =
        TextAreaEditableIfcCaretAffinityEquivalenceAudit::evaluate(no_affinity_slots_input);
    assert_eq!(
        no_affinity_slots.state(),
        TextAreaEditableIfcCaretAffinityEquivalenceAuditState::Blocked
    );
    assert!(no_affinity_slots.blocked_reasons().contains(
        &TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::MissingAffinitySlots
    ));
    assert_eq!(
        no_affinity_slots.diagnostic().equivalent_candidate,
        TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate::None
    );

    let mut missing_metadata_source_input = base_input;
    missing_metadata_source_input.caret_affinity_metadata_source_state =
        TextAreaEditableIfcCaretAffinityMetadataSourceState::Blocked;
    missing_metadata_source_input.caret_affinity_metadata_observed = false;
    missing_metadata_source_input.caret_affinity_metadata_status =
        TextAreaInlineIfcMetadataBridgeStatus::Unwired;
    missing_metadata_source_input
        .per_run_caret_diagnostics
        .clear();
    let missing_metadata_source =
        TextAreaEditableIfcCaretAffinityEquivalenceAudit::evaluate(missing_metadata_source_input);
    assert_eq!(
        missing_metadata_source.state(),
        TextAreaEditableIfcCaretAffinityEquivalenceAuditState::Blocked
    );
    for reason in [
        TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::
            CaretAffinityMetadataSourceMissing,
        TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::
            CaretAffinityMetadataSourceNotObserved,
        TextAreaEditableIfcCaretAffinityEquivalenceAuditBlockedReason::
            CaretAffinityMetadataStatusNotObservedCaretStops,
    ] {
        assert!(
            missing_metadata_source.blocked_reasons().contains(&reason),
            "missing metadata source should block on {reason:?}"
        );
    }
    assert_eq!(
        missing_metadata_source.diagnostic().equivalent_candidate,
        TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate::None
    );
}

#[test]
fn text_area_inline_ifc_caret_affinity_equivalence_audit_keeps_behavior_path_switch_blocked() {
    let (behavior_status, caret_metadata_source, audit) =
        text_area_inline_ifc_caret_affinity_equivalence_fixture();

    assert_eq!(
        behavior_status.state(),
        TextAreaEditableIfcBehaviorPathStatusState::ReadyForStatusObservation
    );
    assert!(
        behavior_status
            .behavior_path_switch_blocked_reasons()
            .contains(
                &TextAreaEditableIfcBehaviorPathStatusBlockedReason::BehaviorPathsStillNotReady
            )
    );
    assert!(!behavior_status.render_enabled());
    assert!(!behavior_status.layout_enabled());
    assert!(!behavior_status.allows_text_area_editable_behavior_path_switch());
    assert!(!behavior_status.report().caret_affinity_behavior_path_ready);
    assert!(!caret_metadata_source.caret_affinity_behavior_path_ready());
    assert!(
        !behavior_status
            .readiness_report(&caret_metadata_source)
            .caret_affinity_behavior_path_ready
    );
    assert!(!audit.caret_affinity_behavior_path_ready());
    assert!(!audit.diagnostic().caret_affinity_behavior_path_ready);
    assert!(!audit.render_enabled());
    assert!(!audit.layout_enabled());
    assert!(!audit.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_caret_affinity_equivalence_audit_keeps_other_paths_blocked() {
    let (_decision, caret_metadata_source, audit) =
        text_area_inline_ifc_caret_affinity_equivalence_fixture();
    let ready_report = _decision.readiness_report(&caret_metadata_source);

    assert_eq!(
        audit.diagnostic().ime_readiness_state,
        TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
    );
    assert_eq!(
        audit.diagnostic().projection_readiness_state,
        TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
    );
    assert_eq!(
        audit.diagnostic().scroll_follow_readiness_state,
        TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
    );
    assert!(!audit.diagnostic().ime_behavior_path_ready);
    assert!(!audit.diagnostic().projection_behavior_path_ready);
    assert!(!audit.diagnostic().scroll_follow_behavior_path_ready);
    assert_eq!(
        ready_report.ime_readiness_state,
        TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
    );
    assert_eq!(
        ready_report.projection_readiness_state,
        TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
    );
    assert_eq!(
        ready_report.scroll_follow_readiness_state,
        TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
    );
}

#[test]
fn text_area_inline_ifc_caret_affinity_equivalence_audit_preserves_layout_with_prepared_default() {
    let (_decision, _caret_metadata_source, audit) =
        text_area_inline_ifc_caret_affinity_equivalence_fixture();
    let mut run = TextAreaTextRun::new(
        "caret equivalence audit keeps legacy layout and render unchanged".to_string(),
        0..66,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let before_size = run.layout_state.layout_size;
    let before_fragments = run.inline_fragment_positions();

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(180, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    run.build(&mut graph, &mut arena, ctx);
    let pass_names = graph
        .pass_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name.to_string())
        .collect::<Vec<_>>();

    assert_eq!(pass_names.len(), 1);
    assert!(
        pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
        "TextArea caret equivalence audit does not own render switching; TextArea default render should emit TextPreparedInputPass for a valid candidate, got {}",
        pass_names[0]
    );
    assert_eq!(before_size.width, run.layout_state.layout_size.width);
    assert_eq!(before_size.height, run.layout_state.layout_size.height);
    assert_eq!(before_fragments, run.inline_fragment_positions());
    assert!(!audit.render_enabled());
    assert!(!audit.layout_enabled());
    assert!(!audit.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_caret_affinity_equivalence_keeps_production_not_ready() {
    let (behavior_status, caret_audit, _projection, _scroll, _ime) =
        text_area_inline_ifc_all_equivalence_fixture();

    assert_eq!(
        caret_audit.state(),
        TextAreaEditableIfcCaretAffinityEquivalenceAuditState::ReadyForObservationOnly
    );
    assert!(caret_audit.blocked_reasons().is_empty());
    assert_eq!(
        caret_audit.diagnostic().equivalent_candidate,
        TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate::EquivalentCandidateObserved
    );
    assert_eq!(
        caret_audit.diagnostic().caret_affinity_readiness_state,
        TextAreaEditableIfcBehaviorPathReadinessState::CaretAffinityCandidateObserved
    );
    assert_eq!(
        caret_audit.diagnostic().projection_readiness_state,
        TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
    );
    assert_eq!(
        caret_audit.diagnostic().scroll_follow_readiness_state,
        TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
    );
    assert_eq!(
        caret_audit.diagnostic().ime_readiness_state,
        TextAreaEditableIfcBehaviorPathReadinessState::BlockedNotReady
    );
    assert!(!caret_audit.caret_affinity_behavior_path_ready());
    assert!(!caret_audit.diagnostic().caret_affinity_behavior_path_ready);
    assert_eq!(
        behavior_status.state(),
        TextAreaEditableIfcBehaviorPathStatusState::ReadyForStatusObservation
    );
    assert!(
        behavior_status
            .behavior_path_switch_blocked_reasons()
            .contains(
                &TextAreaEditableIfcBehaviorPathStatusBlockedReason::BehaviorPathsStillNotReady
            )
    );
    assert!(!caret_audit.render_enabled());
    assert!(!caret_audit.layout_enabled());
    assert!(!caret_audit.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_projection_equivalence_audit_observes_and_blocks_missing_sources() {
    let (_decision, _caret_audit, projection, _scroll, _ime) =
        text_area_inline_ifc_all_equivalence_fixture();

    assert_eq!(
        projection.state(),
        TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly
    );
    assert!(projection.blocked_reasons().is_empty());
    assert_eq!(
        projection.diagnostic().equivalent_candidate,
        TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::EquivalentCandidateObserved
    );
    assert_eq!(
        projection.diagnostic().projection_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::Observed
    );
    assert_eq!(projection.diagnostic().projection_segment_count, 2);
    assert_eq!(projection.diagnostic().run_count, 1);
    assert_eq!(projection.diagnostic().char_range_count, 1);
    assert_eq!(
        projection.diagnostic().per_run_projection_diagnostics.len(),
        1
    );
    assert!(!projection.projection_behavior_path_ready());
    assert!(!projection.render_enabled());
    assert!(!projection.layout_enabled());
    assert!(!projection.allows_text_area_editable_behavior_path_switch());

    let mut no_segments_input = projection.input().clone();
    no_segments_input.projection_prewire_state =
        TextAreaEditableIfcProjectionBehaviorPathPrewireState::ObservedNoProjectionSegments;
    no_segments_input.projection_metadata_status =
        TextAreaInlineIfcMetadataBridgeStatus::ObservedNoProjectionSegments;
    let diagnostic = no_segments_input
        .projection_metadata_diagnostic
        .as_mut()
        .expect("fixture should expose projection diagnostic");
    diagnostic.projection_metadata_status =
        TextAreaInlineIfcMetadataBridgeStatus::ObservedNoProjectionSegments;
    diagnostic.projection_segment_count = 0;
    for per_run in &mut diagnostic.per_run_projection_diagnostics {
        per_run.projection_segment_count = 0;
    }
    let no_segments = TextAreaEditableIfcProjectionEquivalenceAudit::evaluate(no_segments_input);
    assert_eq!(
        no_segments.state(),
        TextAreaEditableIfcBehaviorEquivalenceAuditState::Blocked
    );
    assert!(no_segments.blocked_reasons().contains(
        &TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::MissingProjectionSegments
    ));
    assert!(no_segments.blocked_reasons().contains(
        &TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
            ProjectionMetadataStatusNotObserved
    ));
    assert_eq!(
        no_segments.diagnostic().equivalent_candidate,
        TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::None
    );

    let mut missing_input = projection.input().clone();
    missing_input.projection_prewire_state =
        TextAreaEditableIfcProjectionBehaviorPathPrewireState::Blocked;
    missing_input.projection_diagnostic_prewired = false;
    missing_input.projection_metadata_status = TextAreaInlineIfcMetadataBridgeStatus::Unwired;
    missing_input.projection_metadata_diagnostic = None;
    let missing = TextAreaEditableIfcProjectionEquivalenceAudit::evaluate(missing_input);
    assert_eq!(
        missing.state(),
        TextAreaEditableIfcBehaviorEquivalenceAuditState::Blocked
    );
    for reason in [
        TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
            MissingProjectionMetadataDiagnostic,
        TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
            ProjectionPrewireMissing,
        TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
            ProjectionMetadataSourceNotObserved,
        TextAreaEditableIfcProjectionEquivalenceAuditBlockedReason::
            ProjectionMetadataStatusNotObserved,
    ] {
        assert!(
            missing.blocked_reasons().contains(&reason),
            "missing projection audit source should block on {reason:?}"
        );
    }
}

#[test]
fn text_area_inline_ifc_scroll_follow_equivalence_audit_observes_and_blocks_missing_sources() {
    let (_decision, _caret_audit, _projection, scroll, _ime) =
        text_area_inline_ifc_all_equivalence_fixture();

    assert_eq!(
        scroll.state(),
        TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly
    );
    assert!(scroll.blocked_reasons().is_empty());
    assert_eq!(
        scroll.diagnostic().equivalent_candidate,
        TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::EquivalentCandidateObserved
    );
    assert_eq!(
        scroll.diagnostic().scroll_follow_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::Observed
    );
    assert_eq!(scroll.diagnostic().layout_size_count, 1);
    assert!(scroll.diagnostic().visual_line_count > 0);
    assert!(scroll.diagnostic().caret_stop_count > 0);
    assert!(!scroll.scroll_follow_behavior_path_ready());
    assert!(!scroll.render_enabled());
    assert!(!scroll.layout_enabled());
    assert!(!scroll.allows_text_area_editable_behavior_path_switch());

    let mut no_source_input = scroll.input().clone();
    no_source_input.scroll_follow_prewire_state =
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::ObservedNoScrollSource;
    no_source_input.scroll_follow_metadata_status =
        TextAreaInlineIfcMetadataBridgeStatus::ObservedNoScrollSource;
    let diagnostic = no_source_input
        .scroll_follow_metadata_diagnostic
        .as_mut()
        .expect("fixture should expose scroll diagnostic");
    diagnostic.scroll_follow_metadata_status =
        TextAreaInlineIfcMetadataBridgeStatus::ObservedNoScrollSource;
    diagnostic.layout_size_count = 0;
    diagnostic.visual_line_count = 0;
    diagnostic.caret_stop_count = 0;
    for per_run in &mut diagnostic.per_run_scroll_follow_diagnostics {
        per_run.layout_size = [0.0, 0.0];
        per_run.visual_line_count = 0;
        per_run.caret_stop_count = 0;
    }
    let no_source = TextAreaEditableIfcScrollFollowEquivalenceAudit::evaluate(no_source_input);
    assert_eq!(
        no_source.state(),
        TextAreaEditableIfcBehaviorEquivalenceAuditState::Blocked
    );
    assert!(no_source.blocked_reasons().contains(
        &TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason::MissingScrollSource
    ));
    assert!(no_source.blocked_reasons().contains(
        &TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason::
            ScrollFollowMetadataStatusNotObserved
    ));

    let mut missing_input = scroll.input().clone();
    missing_input.scroll_follow_prewire_state =
        TextAreaEditableIfcScrollFollowBehaviorPathPrewireState::Blocked;
    missing_input.scroll_follow_diagnostic_prewired = false;
    missing_input.scroll_follow_metadata_status = TextAreaInlineIfcMetadataBridgeStatus::Unwired;
    missing_input.scroll_follow_metadata_diagnostic = None;
    let missing = TextAreaEditableIfcScrollFollowEquivalenceAudit::evaluate(missing_input);
    assert_eq!(
        missing.state(),
        TextAreaEditableIfcBehaviorEquivalenceAuditState::Blocked
    );
    assert!(missing.blocked_reasons().contains(
        &TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason::
            MissingScrollFollowMetadataDiagnostic
    ));
    assert!(missing.blocked_reasons().contains(
        &TextAreaEditableIfcScrollFollowEquivalenceAuditBlockedReason::ScrollFollowPrewireMissing
    ));
}

#[test]
fn text_area_inline_ifc_ime_equivalence_audit_observes_preedit_and_blocks_no_preedit() {
    let (_decision, _caret_audit, _projection, _scroll, ime) =
        text_area_inline_ifc_all_equivalence_fixture();

    assert_eq!(
        ime.state(),
        TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly
    );
    assert!(ime.blocked_reasons().is_empty());
    assert_eq!(
        ime.diagnostic().equivalent_candidate,
        TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::EquivalentCandidateObserved
    );
    assert_eq!(
        ime.diagnostic().ime_metadata_status,
        TextAreaInlineIfcMetadataBridgeStatus::Observed
    );
    assert!(ime.diagnostic().has_inline_preedit);
    assert!(ime.diagnostic().has_preedit_run);
    assert_eq!(ime.diagnostic().preedit_cursor_count, 1);
    assert_eq!(ime.diagnostic().preedit_cursors, vec![(1, 3)]);
    assert!(!ime.ime_behavior_path_ready());
    assert!(!ime.render_enabled());
    assert!(!ime.layout_enabled());
    assert!(!ime.allows_text_area_editable_behavior_path_switch());

    let mut no_preedit_input = ime.input().clone();
    no_preedit_input.ime_prewire_state =
        TextAreaEditableIfcImeBehaviorPathPrewireState::ObservedNoPreedit;
    no_preedit_input.ime_metadata_status = TextAreaInlineIfcMetadataBridgeStatus::ObservedNoPreedit;
    let diagnostic = no_preedit_input
        .ime_metadata_diagnostic
        .as_mut()
        .expect("fixture should expose IME diagnostic");
    diagnostic.ime_metadata_status = TextAreaInlineIfcMetadataBridgeStatus::ObservedNoPreedit;
    diagnostic.has_inline_preedit = false;
    diagnostic.has_preedit_run = false;
    diagnostic.preedit_cursor_count = 0;
    diagnostic.preedit_cursors.clear();
    let no_preedit = TextAreaEditableIfcImeEquivalenceAudit::evaluate(no_preedit_input);
    assert_eq!(
        no_preedit.state(),
        TextAreaEditableIfcBehaviorEquivalenceAuditState::Blocked
    );
    assert!(
        no_preedit
            .blocked_reasons()
            .contains(&TextAreaEditableIfcImeEquivalenceAuditBlockedReason::NoPreeditCase),
        "no-preedit metadata must not be promoted to IME observation-ready"
    );
    assert!(no_preedit.blocked_reasons().contains(
        &TextAreaEditableIfcImeEquivalenceAuditBlockedReason::ImeMetadataStatusNotObserved
    ));
    assert_eq!(
        no_preedit.diagnostic().equivalent_candidate,
        TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::None
    );
}

#[test]
fn text_area_inline_ifc_equivalence_audits_keep_status_observation_only_surface() {
    let (behavior_status, caret, projection, scroll, ime) =
        text_area_inline_ifc_all_equivalence_fixture();

    assert_eq!(
        caret.state(),
        TextAreaEditableIfcCaretAffinityEquivalenceAuditState::ReadyForObservationOnly
    );
    assert_eq!(
        caret.diagnostic().equivalent_candidate,
        TextAreaEditableIfcCaretAffinityEquivalenceAuditCandidate::EquivalentCandidateObserved
    );
    assert_eq!(
        projection.state(),
        TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly
    );
    assert_eq!(
        scroll.state(),
        TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly
    );
    assert_eq!(
        ime.state(),
        TextAreaEditableIfcBehaviorEquivalenceAuditState::ReadyForObservationOnly
    );
    assert_eq!(
        projection.diagnostic().equivalent_candidate,
        TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::EquivalentCandidateObserved
    );
    assert_eq!(
        scroll.diagnostic().equivalent_candidate,
        TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::EquivalentCandidateObserved
    );
    assert_eq!(
        ime.diagnostic().equivalent_candidate,
        TextAreaEditableIfcBehaviorEquivalenceAuditCandidate::EquivalentCandidateObserved
    );
    assert!(!ime.ime_behavior_path_ready());
    assert!(!caret.caret_affinity_behavior_path_ready());
    assert!(!projection.projection_behavior_path_ready());
    assert!(!scroll.scroll_follow_behavior_path_ready());
    assert!(!ime.render_enabled());
    assert!(!caret.render_enabled());
    assert!(!projection.render_enabled());
    assert!(!scroll.render_enabled());
    assert!(!ime.layout_enabled());
    assert!(!caret.layout_enabled());
    assert!(!projection.layout_enabled());
    assert!(!scroll.layout_enabled());
    assert!(!ime.allows_text_area_editable_behavior_path_switch());
    assert!(!caret.allows_text_area_editable_behavior_path_switch());
    assert!(!projection.allows_text_area_editable_behavior_path_switch());
    assert!(!scroll.allows_text_area_editable_behavior_path_switch());
    assert_eq!(
        behavior_status.state(),
        TextAreaEditableIfcBehaviorPathStatusState::ReadyForStatusObservation
    );
    assert!(
        behavior_status
            .behavior_path_switch_blocked_reasons()
            .contains(
                &TextAreaEditableIfcBehaviorPathStatusBlockedReason::BehaviorPathsStillNotReady
            )
    );
    assert!(!behavior_status.render_enabled());
    assert!(!behavior_status.layout_enabled());
    assert!(!behavior_status.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_behavior_path_status_preserves_layout_with_prepared_default() {
    let (behavior_status, _caret_audit, _projection, _scroll, _ime) =
        text_area_inline_ifc_all_equivalence_fixture();
    let mut run = TextAreaTextRun::new(
        "behavior path audit keeps layout unchanged with prepared default".to_string(),
        0..72,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let before_size = run.layout_state.layout_size;
    let before_fragments = run.inline_fragment_positions();

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(180, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    run.build(&mut graph, &mut arena, ctx);
    let pass_names = graph
        .pass_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name.to_string())
        .collect::<Vec<_>>();

    assert_eq!(pass_names.len(), 1);
    assert!(
        pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
        "TextArea behavior-path status does not own render switching; TextArea default render should emit TextPreparedInputPass for a valid candidate, got {}",
        pass_names[0]
    );
    assert_eq!(before_size.width, run.layout_state.layout_size.width);
    assert_eq!(before_size.height, run.layout_state.layout_size.height);
    assert_eq!(before_fragments, run.inline_fragment_positions());
    assert!(behavior_status.report().prepared_render_default_observed);
    assert!(!behavior_status.render_enabled());
    assert!(!behavior_status.layout_enabled());
    assert!(!behavior_status.allows_text_area_editable_behavior_path_switch());
}

#[test]
fn text_area_inline_ifc_metadata_bridge_uses_default_prepared_render_graph() {
    let mut run = TextAreaTextRun::new(
        "metadata bridge uses default prepared text pass".to_string(),
        0..53,
    );
    let mut arena = NodeArena::new();
    place_run_for_inline_ifc_staging_test(&mut run, &mut arena, 180.0);
    let payload = run
        .inline_ifc_staging_payload([0.0, 0.0], 0, 1.0)
        .expect("laid out run should expose metadata bridge payload");
    let bridge = TextAreaInlineIfcMetadataBridgeInput::from_evaluation_input(
        TextAreaInlineIfcEvaluationInput::from_staging_payloads(vec![payload]),
    );
    let preflight = TextAreaInlineIfcMetadataBridgePreflight::evaluate(bridge);
    assert!(!preflight.render_enabled());

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(180, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    run.build(&mut graph, &mut arena, ctx);
    let pass_names = graph
        .pass_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name.to_string())
        .collect::<Vec<_>>();

    assert_eq!(pass_names.len(), 1);
    assert!(
        pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
        "TextArea metadata bridge does not own render switching; TextArea default render should emit TextPreparedInputPass for a valid candidate, got {}",
        pass_names[0]
    );
}
