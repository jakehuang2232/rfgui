use super::*;

// Synthetic complete accounting; these new phase values are not a
// retrospective measurement of the user's original frame.
fn complete_frame_timings() -> FrameTimings {
    FrameTimings {
        frame_number: 3043,
        total_ms: 17.697,
        begin_frame_ms: 0.085,
        layout_total_ms: 0.422,
        layout_ms: 0.422,
        prepare_paint_ms: 0.5,
        sync_properties_ms: 0.6,
        finish_render_ms: 0.168,
        build_graph_ms: 6.703,
        compile_ms: 3.645,
        execute_ms: 0.425,
        execute_profile_ms: Some(0.4),
        execute_pass_count: 696,
        end_frame_ms: 5.149,
        end_frame: EndFrameProfile {
            staging_finish_ms: 0.1,
            encoder_finish_ms: 0.2,
            submit_ms: 0.3,
            resource_cleanup_ms: 0.4,
            present_ms: 0.5,
            gpu_wait_ms: 3.649,
            gpu_waited: true,
            abort_cleanup_ms: 0.0,
        },
        ..Default::default()
    }
}

fn plain_trace(timings: &FrameTimings, opts: &ViewportDebugOptions) -> String {
    let trace = format_trace_render_tree(&Viewport::build_frame_trace_tree(timings, opts));
    let mut plain = String::new();
    let mut chars = trace.chars();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            for ch in chars.by_ref() {
                if ch == 'm' {
                    break;
                }
            }
        } else {
            plain.push(ch);
        }
    }
    plain
}

fn displayed_ms(line: &str) -> f64 {
    line.split_whitespace()
        .find_map(|word| word.strip_suffix("ms"))
        .expect("trace line must contain milliseconds")
        .parse()
        .unwrap()
}

fn top_level_lines(trace: &str) -> Vec<&str> {
    trace
        .lines()
        .filter(|line| line.starts_with("├─ ") || line.starts_with("└─ "))
        .collect()
}

#[test]
fn frame_timing_lists_measured_phases_without_a_residual_bucket() {
    let trace = plain_trace(&complete_frame_timings(), &ViewportDebugOptions::default());
    assert!(trace.starts_with("render_frame #3043 17.697ms (100.0%)"));
    assert!(!trace.contains("unattributed"));
    assert_frame_accounting(&complete_frame_timings());
    let child_sum: f64 = top_level_lines(&trace).into_iter().map(displayed_ms).sum();
    assert!((child_sum - 17.697).abs() < 1e-9, "{trace}");
}

#[test]
fn frame_timing_details_do_not_double_count_rsx_relayout_or_new_phases() {
    let timings = FrameTimings {
        frontend: FrontendProfile {
            rsx_build_ms: 2.5,
            ..Default::default()
        },
        layout_ms: 0.2,
        post_layout_transition_ms: 0.122,
        relayout_ms: 0.1,
        prepare_paint_ms: 0.5,
        sync_properties_ms: 0.6,
        finish_render_ms: 0.168,
        // Nested diagnostics are not additional top-level work.
        compile_children: vec![TraceRenderNode::new("compile detail", 3.0)],
        execute_ordered_passes: vec![("draw".into(), 0.4, 696)],
        ..complete_frame_timings()
    };
    let mut baseline = None;
    for flags in 0..8 {
        let opts = ViewportDebugOptions {
            trace_layout_detail: flags & 1 != 0,
            trace_compile_detail: flags & 2 != 0,
            trace_execute_detail: flags & 4 != 0,
            ..Default::default()
        };
        let trace = plain_trace(&timings, &opts);
        assert!(trace.starts_with("render_frame #3043 20.197ms (100.0%)"));
        for expected in [
            "rsx_build 2.500ms",
            "layout 0.422ms",
            "prepare_paint 0.500ms",
            "sync_properties 0.600ms",
            "finish_render 0.168ms",
        ] {
            assert!(trace.contains(expected), "missing {expected}: {trace}");
        }
        if flags != 0 {
            for expected in [
                "staging_finish 0.100ms",
                "encoder_finish 0.200ms",
                "queue_submit 0.300ms",
                "resource_cleanup 0.400ms",
                "present 0.500ms",
                "gpu_wait (waited=true) 3.649ms",
            ] {
                assert!(trace.contains(expected), "missing {expected}: {trace}");
            }
        }
        let lines: Vec<String> = top_level_lines(&trace)
            .into_iter()
            .map(str::to_owned)
            .collect();
        let child_sum: f64 = lines.iter().map(|line| displayed_ms(line)).sum();
        assert!((child_sum - 20.197).abs() < 1e-9, "{trace}");
        if let Some(baseline) = &baseline {
            assert_eq!(&lines, baseline, "detail flags changed phase accounting");
        } else {
            baseline = Some(lines);
        }
    }
}

#[test]
#[should_panic(expected = "frame phase sum differs from wall time")]
fn frame_timing_rejects_overlapping_intervals_instead_of_inventing_a_correction() {
    assert_frame_accounting(&FrameTimings {
        total_ms: 16.0,
        ..complete_frame_timings()
    });
}

#[test]
fn frame_timing_empty_frame_has_finite_zero_accounting() {
    let trace = plain_trace(&FrameTimings::default(), &ViewportDebugOptions::default());
    assert!(!trace.contains("unattributed"));
    assert_frame_accounting(&FrameTimings::default());
    assert!(!trace.contains("NaN"));
    assert!(!trace.contains("overlapping timers"));
}

#[test]
fn frame_timing_abort_is_cleanup_without_submit_or_gpu_wait() {
    let timings = FrameTimings {
        end_frame: EndFrameProfile {
            abort_cleanup_ms: 5.149,
            ..Default::default()
        },
        ..complete_frame_timings()
    };
    let trace = plain_trace(
        &timings,
        &ViewportDebugOptions {
            trace_compile_detail: true,
            ..Default::default()
        },
    );
    for expected in [
        "abort_cleanup 5.149ms",
        "queue_submit 0.000ms",
        "present 0.000ms",
        "gpu_wait (waited=false) 0.000ms",
    ] {
        assert!(trace.contains(expected), "missing {expected}: {trace}");
    }
    assert_frame_accounting(&timings);
}

// Called for every real production render_render_tree invocation in tests,
// including native terminal-failure and recovery frames. Nested diagnostics
// are intentionally excluded from this additive first-level partition.
pub(super) fn assert_frame_accounting(t: &FrameTimings) {
    let completion = &t.end_frame;
    let completion_phases = [
        completion.staging_finish_ms,
        completion.encoder_finish_ms,
        completion.submit_ms,
        completion.resource_cleanup_ms,
        completion.present_ms,
        completion.gpu_wait_ms,
        completion.abort_cleanup_ms,
    ];
    assert!(
        completion_phases
            .iter()
            .all(|ms| ms.is_finite() && *ms >= 0.0)
    );
    // The outer phase additionally includes dispatch, bookkeeping and drops.
    assert!(completion_phases.iter().sum::<f64>() <= t.end_frame_ms + 1e-9);
    if !completion.gpu_waited {
        assert_eq!(completion.gpu_wait_ms, 0.0);
    }
    let phases = [
        t.begin_frame_ms,
        t.layout_total_ms,
        t.prepare_paint_ms,
        t.sync_properties_ms,
        t.build_graph_ms,
        t.compile_ms,
        t.execute_ms,
        t.finish_render_ms,
        t.end_frame_ms,
    ];
    assert!(phases.iter().all(|ms| ms.is_finite() && *ms >= 0.0));
    let sum: f64 = phases.iter().sum();
    assert!(
        (sum - t.total_ms).abs() <= 1e-9 * t.total_ms.max(1.0),
        "frame phase sum differs from wall time: phases={phases:?}, sum={sum}, total={}",
        t.total_ms
    );
}

#[test]
fn frontend_intervals_are_additive_but_unwrap_is_nested_in_build() {
    let t = FrameTimings {
        frontend: FrontendProfile {
            state_flush_ms: 0.3,
            rsx_build_ms: 2.5,
            scene_update_ms: 1.7,
            work: crate::ui::UiWorkProfile {
                unwrap_ms: 2.0,
                reconcile_ms: 0.4,
                incremental_commit_ms: 0.5,
                ..Default::default()
            },
        },
        ..complete_frame_timings()
    };
    let trace = plain_trace(&t, &ViewportDebugOptions::default());
    assert!(trace.starts_with("render_frame #3043 22.197ms"), "{trace}");
    let child_sum: f64 = top_level_lines(&trace).into_iter().map(displayed_ms).sum();
    assert!((child_sum - 22.197).abs() < 1e-9, "{trace}");
    assert!(trace.contains("scene_update 1.700ms"));
    assert!(trace.contains("unwrap nodes=0 components=0 memo_hits=0 2.000ms"));
}
