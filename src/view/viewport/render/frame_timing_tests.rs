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
        rsx_build_ms: 2.5,
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

// Called for every real production render_render_tree invocation in tests,
// including native terminal-failure and recovery frames. Nested diagnostics
// are intentionally excluded from this additive first-level partition.
pub(super) fn assert_frame_accounting(t: &FrameTimings) {
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
