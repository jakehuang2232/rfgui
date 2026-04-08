use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

fn highlight_ms(ms: f64) -> String {
    let color = if ms >= 16.7 {
        "\x1b[31m"
    } else if ms >= 8.3 {
        "\x1b[33m"
    } else {
        "\x1b[32m"
    };
    format!("{color}{ms:.3}ms\x1b[0m")
}

#[derive(Debug, Clone)]
pub(super) struct TraceRenderNode {
    name: String,
    elapsed_ms: f64,
    children: Vec<TraceRenderNode>,
}

impl TraceRenderNode {
    pub(super) fn new(name: impl Into<String>, elapsed_ms: f64) -> Self {
        Self {
            name: name.into(),
            elapsed_ms,
            children: Vec::new(),
        }
    }

    pub(super) fn with_children(
        name: impl Into<String>,
        elapsed_ms: f64,
        children: Vec<TraceRenderNode>,
    ) -> Self {
        Self {
            name: name.into(),
            elapsed_ms,
            children,
        }
    }
}

fn trace_render_percentage(parent_ms: f64, child_ms: f64) -> String {
    let pct = if parent_ms > f64::EPSILON {
        (child_ms / parent_ms) * 100.0
    } else {
        0.0
    };
    format!("{pct:.1}%")
}

const TRACE_RENDER_FIRST_LEVEL_ONLY_THRESHOLD_MS: f64 = 8.3;

fn trace_render_visible_depth(root: &TraceRenderNode) -> usize {
    if root.elapsed_ms < TRACE_RENDER_FIRST_LEVEL_ONLY_THRESHOLD_MS {
        1
    } else {
        usize::MAX
    }
}

pub(super) fn format_trace_render_tree(root: &TraceRenderNode) -> String {
    fn append_node(
        out: &mut String,
        node: &TraceRenderNode,
        parent_ms: Option<f64>,
        prefix: &str,
        is_last: bool,
        depth: usize,
        max_depth: usize,
    ) {
        if parent_ms.is_none() {
            out.push_str(&format!(
                "\x1b[1;36m{}\x1b[0m {} (100.0%)\n",
                node.name,
                highlight_ms(node.elapsed_ms)
            ));
        } else {
            let branch = if is_last { "└─ " } else { "├─ " };
            let parent_elapsed_ms = parent_ms.unwrap_or(node.elapsed_ms);
            out.push_str(&format!(
                "{prefix}{branch}{} {} ({})\n",
                node.name,
                highlight_ms(node.elapsed_ms),
                trace_render_percentage(parent_elapsed_ms, node.elapsed_ms)
            ));
        }

        let next_prefix = if parent_ms.is_none() {
            String::new()
        } else if is_last {
            format!("{prefix}   ")
        } else {
            format!("{prefix}│  ")
        };
        if depth >= max_depth {
            return;
        }
        let child_count = node.children.len();
        for (idx, child) in node.children.iter().enumerate() {
            append_node(
                out,
                child,
                Some(node.elapsed_ms),
                &next_prefix,
                idx + 1 == child_count,
                depth + 1,
                max_depth,
            );
        }
    }

    let mut output = String::new();
    append_node(
        &mut output,
        root,
        None,
        "",
        true,
        0,
        trace_render_visible_depth(root),
    );
    output.trim_end().to_string()
}

pub(super) fn build_execute_detail_trace_nodes(
    ordered_passes: Vec<(String, f64, usize)>,
) -> Vec<TraceRenderNode> {
    let mut out = Vec::new();
    let mut grouped_indices: HashMap<String, usize> = HashMap::new();

    for (name, elapsed_ms, count) in ordered_passes {
        if let Some((group, detail)) = name.split_once("::") {
            let index = if let Some(index) = grouped_indices.get(group).copied() {
                index
            } else {
                let index = out.len();
                out.push(TraceRenderNode::with_children(group, 0.0, Vec::new()));
                grouped_indices.insert(group.to_string(), index);
                index
            };
            out[index].elapsed_ms += elapsed_ms;
            out[index].children.push(TraceRenderNode::new(
                format!("{detail} (count={count})"),
                elapsed_ms,
            ));
        } else {
            out.push(TraceRenderNode::new(
                format!("{name} (count={count})"),
                elapsed_ms,
            ));
        }
    }

    out
}

pub(super) fn build_compile_trace_nodes(
    profile: &crate::view::frame_graph::CompileProfile,
    show_detail: bool,
) -> Vec<TraceRenderNode> {
    let graph = &profile.graph;
    let mut build_compiled_graph_children = vec![
        TraceRenderNode::new("build_version_producers", graph.build_version_producers_ms),
        TraceRenderNode::new(
            "latest_resource_versions",
            graph.latest_resource_versions_ms,
        ),
        TraceRenderNode::new(
            format!("discover_sink_passes (count={})", graph.sink_pass_count),
            graph.discover_sink_passes_ms,
        ),
        TraceRenderNode::new("discover_live_passes", graph.discover_live_passes_ms),
        TraceRenderNode::new(
            "build_live_dependency_graph",
            graph.build_live_dependency_graph_ms,
        ),
        TraceRenderNode::new("toposort_live_passes", graph.toposort_live_passes_ms),
        TraceRenderNode::new("build_execution_plan", graph.build_execution_plan_ms),
        TraceRenderNode::new(
            "build_resource_state_timelines",
            graph.build_resource_state_timelines_ms,
        ),
        TraceRenderNode::new(
            "build_compiled_resources",
            graph.build_compiled_resources_ms,
        ),
        TraceRenderNode::new(
            "assemble_compiled_passes",
            graph.assemble_compiled_passes_ms,
        ),
    ];

    if show_detail {
        build_compiled_graph_children.push(TraceRenderNode::with_children(
            format!(
                "pass_kinds (graphics={}, compute={}, transfer={})",
                graph.graphics_pass_count, graph.compute_pass_count, graph.transfer_pass_count,
            ),
            0.0,
            Vec::new(),
        ));
        build_compiled_graph_children.push(TraceRenderNode::with_children(
            format!(
                "dependency_graph (edges={}, max_in={}, max_out={})",
                graph.live_dependency_edge_count,
                graph
                    .top_indegree_passes
                    .first()
                    .map(|stat| stat.degree)
                    .unwrap_or(0),
                graph
                    .top_outdegree_passes
                    .first()
                    .map(|stat| stat.degree)
                    .unwrap_or(0),
            ),
            0.0,
            vec![
                TraceRenderNode::with_children(
                    "top_indegree",
                    0.0,
                    graph
                        .top_indegree_passes
                        .iter()
                        .map(|stat| {
                            TraceRenderNode::new(
                                format!(
                                    "{}#{} (indegree={})",
                                    stat.pass_name, stat.pass_index, stat.degree
                                ),
                                0.0,
                            )
                        })
                        .collect(),
                ),
                TraceRenderNode::with_children(
                    "top_outdegree",
                    0.0,
                    graph
                        .top_outdegree_passes
                        .iter()
                        .map(|stat| {
                            TraceRenderNode::new(
                                format!(
                                    "{}#{} (outdegree={})",
                                    stat.pass_name, stat.pass_index, stat.degree
                                ),
                                0.0,
                            )
                        })
                        .collect(),
                ),
            ],
        ));
        build_compiled_graph_children.push(TraceRenderNode::with_children(
            format!(
                "execution_steps (graphics={}, groups={}, max_group_size={})",
                graph.graphics_step_count,
                graph.graphics_group_count,
                graph.max_graphics_group_size,
            ),
            0.0,
            Vec::new(),
        ));
        build_compiled_graph_children.push(TraceRenderNode::with_children(
            "pass_name_top",
            0.0,
            graph
                .pass_name_counts
                .iter()
                .map(|stat| TraceRenderNode::new(format!("{} ({})", stat.label, stat.count), 0.0))
                .collect(),
        ));
        build_compiled_graph_children.push(TraceRenderNode::with_children(
            "resource_versions_top",
            0.0,
            graph
                .versioned_resource_counts
                .iter()
                .map(|stat| TraceRenderNode::new(format!("{} ({})", stat.label, stat.count), 0.0))
                .collect(),
        ));
    }

    vec![
        TraceRenderNode::new(
            format!("setup_passes (passes={})", profile.setup_pass_count),
            profile.setup_passes_ms,
        ),
        TraceRenderNode::new(
            "annotate_resource_versions",
            profile.annotate_resource_versions_ms,
        ),
        TraceRenderNode::with_children(
            format!(
                "build_compiled_graph (live={}, ordered={}, steps={}, resources={}, culled={})",
                graph.live_pass_count,
                graph.ordered_pass_count,
                graph.execution_step_count,
                graph.compiled_resource_count,
                graph.culled_pass_count,
            ),
            profile.build_compiled_graph_ms,
            build_compiled_graph_children,
        ),
        TraceRenderNode::new(
            format!("prepare_upload (passes={})", profile.prepare_pass_count),
            profile.prepare_upload_ms,
        ),
    ]
}

pub(super) fn build_layout_place_trace_nodes(
    profile: &crate::view::base_component::LayoutPlaceProfile,
) -> Vec<TraceRenderNode> {
    vec![
        TraceRenderNode::new(
            format!("place_self (nodes={})", profile.node_count),
            profile.place_self_ms,
        ),
        TraceRenderNode::new("place_children", profile.place_children_ms),
        TraceRenderNode::new("place_flex_children", profile.place_flex_children_ms),
        TraceRenderNode::new("place_layout_inline", profile.place_layout_inline_ms),
        TraceRenderNode::new("place_layout_flex", profile.place_layout_flex_ms),
        TraceRenderNode::new("place_layout_flow", profile.place_layout_flow_ms),
        TraceRenderNode::new(
            format!("child_place (calls={})", profile.child_place_calls),
            profile.non_axis_child_place_ms,
        ),
        TraceRenderNode::new(
            format!(
                "absolute_child_place (calls={})",
                profile.absolute_child_place_calls
            ),
            profile.absolute_child_place_ms,
        ),
        TraceRenderNode::new("update_content_size", profile.update_content_size_ms),
        TraceRenderNode::new("clamp_scroll", profile.clamp_scroll_ms),
        TraceRenderNode::new("recompute_hit_test", profile.recompute_hit_test_ms),
    ]
}

pub(super) fn format_promotion_trace(
    decisions: &[PromotionDecision],
    updates: &[PromotedLayerUpdate],
    base_threshold: i32,
) -> String {
    let promoted = decisions
        .iter()
        .filter(|decision| decision.should_promote)
        .collect::<Vec<_>>();
    let reraster_count = updates
        .iter()
        .filter(|update| {
            matches!(
                update.kind,
                crate::view::promotion::PromotedLayerUpdateKind::Reraster
            )
        })
        .count();
    let reuse_count = updates
        .iter()
        .filter(|update| {
            matches!(
                update.kind,
                crate::view::promotion::PromotedLayerUpdateKind::Reuse
            )
        })
        .count();
    format!(
        "[promotion] promoted={}/{} base_threshold={} updates(reuse={}, reraster={})",
        promoted.len(),
        decisions.len(),
        base_threshold,
        reuse_count,
        reraster_count
    )
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum DebugReusePathContext {
    Root,
    Child,
    Deferred,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DebugReusePathRecord {
    pub node_id: u64,
    pub context: DebugReusePathContext,
    pub requested: PromotedLayerUpdateKind,
    pub can_reuse: bool,
    pub actual: PromotedLayerUpdateKind,
    pub reason: Option<&'static str>,
    pub clip_rect: Option<[u32; 4]>,
}

fn debug_reuse_path_store() -> &'static Mutex<Vec<DebugReusePathRecord>> {
    static STORE: OnceLock<Mutex<Vec<DebugReusePathRecord>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Vec::new()))
}

fn debug_style_sample_store() -> &'static Mutex<Vec<String>> {
    static STORE: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Vec::new()))
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DebugStyleSampleRecord {
    pub target: u64,
    pub promoted_root: Option<u64>,
}

fn debug_style_sample_record_store() -> &'static Mutex<Vec<DebugStyleSampleRecord>> {
    static STORE: OnceLock<Mutex<Vec<DebugStyleSampleRecord>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Vec::new()))
}

fn debug_style_promotion_store() -> &'static Mutex<Vec<String>> {
    static STORE: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Vec::new()))
}

fn debug_style_request_store() -> &'static Mutex<Vec<String>> {
    static STORE: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Vec::new()))
}

pub(super) fn trace_promoted_build_frame_marker() {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    if !*ENABLED.get_or_init(|| std::env::var("RFGUI_TRACE_PROMOTED_BUILD").is_ok()) {
        return;
    }
    static FRAME_COUNTER: AtomicU64 = AtomicU64::new(0);
    let frame = FRAME_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
    eprintln!("\n========== [promoted-build frame {frame}] ==========");
}

pub(crate) fn begin_debug_reuse_path_frame() {
    debug_reuse_path_store().lock().unwrap().clear();
    debug_style_sample_store().lock().unwrap().clear();
    debug_style_sample_record_store().lock().unwrap().clear();
    debug_style_promotion_store().lock().unwrap().clear();
    debug_style_request_store().lock().unwrap().clear();
}

pub(crate) fn record_debug_reuse_path(record: DebugReusePathRecord) {
    debug_reuse_path_store().lock().unwrap().push(record);
}

pub(super) fn snapshot_debug_reuse_path() -> Vec<DebugReusePathRecord> {
    debug_reuse_path_store().lock().unwrap().clone()
}

pub(super) fn record_debug_style_sample(line: String) {
    debug_style_sample_store().lock().unwrap().push(line);
}

fn snapshot_debug_style_samples() -> Vec<String> {
    debug_style_sample_store().lock().unwrap().clone()
}

pub(super) fn record_debug_style_sample_record(record: DebugStyleSampleRecord) {
    debug_style_sample_record_store()
        .lock()
        .unwrap()
        .push(record);
}

pub(super) fn snapshot_debug_style_sample_records() -> Vec<DebugStyleSampleRecord> {
    debug_style_sample_record_store().lock().unwrap().clone()
}

pub(super) fn record_debug_style_promotion(line: String) {
    debug_style_promotion_store().lock().unwrap().push(line);
}

fn snapshot_debug_style_promotion() -> Vec<String> {
    debug_style_promotion_store().lock().unwrap().clone()
}

pub(super) fn record_debug_style_request(line: String) {
    debug_style_request_store().lock().unwrap().push(line);
}

fn snapshot_debug_style_requests() -> Vec<String> {
    debug_style_request_store().lock().unwrap().clone()
}

pub(super) fn format_reuse_path_trace() -> String {
    let mut records = snapshot_debug_reuse_path();
    if records.is_empty() {
        return "[reuse-path]\n  no promoted path activity".to_string();
    }

    records.sort_by_key(|record| (record.node_id, record.context as u8));
    let requested_reuse = records
        .iter()
        .filter(|record| matches!(record.requested, PromotedLayerUpdateKind::Reuse))
        .count();
    let actual_reuse = records
        .iter()
        .filter(|record| matches!(record.actual, PromotedLayerUpdateKind::Reuse))
        .count();
    let fallback_to_reraster = records
        .iter()
        .filter(|record| {
            matches!(record.requested, PromotedLayerUpdateKind::Reuse)
                && matches!(record.actual, PromotedLayerUpdateKind::Reraster)
        })
        .count();

    let mut lines = vec![
        "[reuse-path]".to_string(),
        format!(
            "  summary: nodes={} requested_reuse={} actual_reuse={} fallback_to_reraster={}",
            records.len(),
            requested_reuse,
            actual_reuse,
            fallback_to_reraster
        ),
    ];

    for record in records {
        let context = match record.context {
            DebugReusePathContext::Root => "root",
            DebugReusePathContext::Child => "child",
            DebugReusePathContext::Deferred => "deferred",
        };
        let requested = match record.requested {
            PromotedLayerUpdateKind::Reuse => "reuse",
            PromotedLayerUpdateKind::Reraster => "reraster",
        };
        let actual = match record.actual {
            PromotedLayerUpdateKind::Reuse => "reuse",
            PromotedLayerUpdateKind::Reraster => "reraster",
        };
        let reason = match record.reason {
            Some("absolute-viewport-clip-inline") => "absolute-viewport-clip-inline",
            Some("absolute-anchor-clip-inline") => "absolute-anchor-clip-inline",
            Some("child-scissor-clip-inline") => "child-scissor-clip-inline",
            Some("child-stencil-clip-inline") => "child-stencil-clip-inline",
            Some(other) => other,
            None => "-",
        };
        let clip_rect = record
            .clip_rect
            .map(|[x, y, w, h]| format!(" clip=[{x},{y},{w},{h}]"))
            .unwrap_or_default();
        lines.push(format!(
            "  - node={} context={} requested={} can_reuse={} actual={} reason={}{}",
            record.node_id, context, requested, record.can_reuse, actual, reason, clip_rect,
        ));
    }

    lines.join("\n")
}

pub(super) fn format_style_sample_trace() -> String {
    let lines = snapshot_debug_style_samples();
    if lines.is_empty() {
        return "[style-sample]\n  no style samples".to_string();
    }
    let mut out = vec![
        "[style-sample]".to_string(),
        format!("  summary: samples={}", lines.len()),
    ];
    out.extend(lines.into_iter().map(|line| format!("  - {line}")));
    out.join("\n")
}

pub(super) fn format_style_promotion_trace() -> String {
    let lines = snapshot_debug_style_promotion();
    if lines.is_empty() {
        return "[style-promotion]\n  no sampled promoted roots".to_string();
    }
    let mut out = vec![
        "[style-promotion]".to_string(),
        format!("  summary: roots={}", lines.len()),
    ];
    out.extend(lines.into_iter().map(|line| format!("  - {line}")));
    out.join("\n")
}

pub(super) fn format_style_request_trace() -> String {
    let lines = snapshot_debug_style_requests();
    if lines.is_empty() {
        return "[style-request]\n  no style requests".to_string();
    }
    let mut out = vec![
        "[style-request]".to_string(),
        format!("  summary: requests={}", lines.len()),
    ];
    out.extend(lines.into_iter().map(|line| format!("  - {line}")));
    out.join("\n")
}

pub(super) fn format_style_field(field: StyleField) -> &'static str {
    match field {
        StyleField::Opacity => "opacity",
        StyleField::BorderRadius => "border_radius",
        StyleField::BackgroundColor => "background_color",
        StyleField::Color => "foreground_color",
        StyleField::BorderTopColor => "border_top_color",
        StyleField::BorderRightColor => "border_right_color",
        StyleField::BorderBottomColor => "border_bottom_color",
        StyleField::BorderLeftColor => "border_left_color",
        StyleField::BoxShadow => "box_shadow",
        StyleField::Transform => "transform",
        StyleField::TransformOrigin => "transform_origin",
    }
}

pub(super) fn style_field_requires_relayout(field: StyleField) -> bool {
    match field {
        StyleField::Opacity
        | StyleField::BorderRadius
        | StyleField::BackgroundColor
        | StyleField::Color
        | StyleField::BorderTopColor
        | StyleField::BorderRightColor
        | StyleField::BorderBottomColor
        | StyleField::BorderLeftColor
        | StyleField::BoxShadow
        | StyleField::Transform
        | StyleField::TransformOrigin => false,
    }
}

pub(super) fn format_style_value(value: &StyleValue) -> String {
    match value {
        StyleValue::Scalar(value) => format!("{value:.3}"),
        StyleValue::Color(color) => {
            let [r, g, b, a] = color.to_rgba_u8();
            format!("rgba({r},{g},{b},{a})")
        }
        StyleValue::Transform(transform) => {
            format!("transform({} entries)", transform.as_slice().len())
        }
        StyleValue::TransformProgress { progress, .. } => {
            format!("transform(progress={progress:.3})")
        }
        StyleValue::BoxShadow(shadows) => format!("box-shadow({} layers)", shadows.len()),
        StyleValue::TransformOrigin(origin) => format!(
            "transform-origin(x={}, y={}, z={:.3})",
            origin.x().resolve_without_percent_base(0.0, 0.0),
            origin.y().resolve_without_percent_base(0.0, 0.0),
            origin.z()
        ),
        StyleValue::TransformOriginProgress { progress, .. } => {
            format!("transform-origin(progress={progress:.3})")
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PostLayoutTransitionResult {
    pub redraw_changed: bool,
    pub relayout_required: bool,
}

fn append_overlay_line_quad(
    vertices: &mut Vec<super::super::render_pass::debug_overlay_pass::DebugOverlayVertex>,
    indices: &mut Vec<u32>,
    p0: [f32; 2],
    p1: [f32; 2],
    thickness_px: f32,
    color: [f32; 4],
    screen_w: f32,
    screen_h: f32,
) {
    let dx = p1[0] - p0[0];
    let dy = p1[1] - p0[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 0.001 {
        return;
    }
    let nx = -dy / len;
    let ny = dx / len;
    let half = thickness_px * 0.5;
    let corners = [
        [p0[0] + nx * half, p0[1] + ny * half],
        [p1[0] + nx * half, p1[1] + ny * half],
        [p1[0] - nx * half, p1[1] - ny * half],
        [p0[0] - nx * half, p0[1] - ny * half],
    ];
    let base = vertices.len() as u32;
    for [x, y] in corners {
        let clip_x = (x / screen_w) * 2.0 - 1.0;
        let clip_y = 1.0 - (y / screen_h) * 2.0;
        vertices.push(
            super::super::render_pass::debug_overlay_pass::DebugOverlayVertex {
                position: [clip_x, clip_y],
                color,
            },
        );
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn append_overlay_rect_quad(
    vertices: &mut Vec<super::super::render_pass::debug_overlay_pass::DebugOverlayVertex>,
    indices: &mut Vec<u32>,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    color: [f32; 4],
    screen_w: f32,
    screen_h: f32,
) {
    if right <= left || bottom <= top {
        return;
    }
    let base = vertices.len() as u32;
    for [x, y] in [[left, top], [right, top], [right, bottom], [left, bottom]] {
        let clip_x = (x / screen_w) * 2.0 - 1.0;
        let clip_y = 1.0 - (y / screen_h) * 2.0;
        vertices.push(
            super::super::render_pass::debug_overlay_pass::DebugOverlayVertex {
                position: [clip_x, clip_y],
                color,
            },
        );
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn digit_segments(digit: char) -> Option<&'static [usize]> {
    match digit {
        '0' => Some(&[0, 1, 2, 4, 5, 6]),
        '1' => Some(&[2, 5]),
        '2' => Some(&[0, 2, 3, 4, 6]),
        '3' => Some(&[0, 2, 3, 5, 6]),
        '4' => Some(&[1, 2, 3, 5]),
        '5' => Some(&[0, 1, 3, 5, 6]),
        '6' => Some(&[0, 1, 3, 4, 5, 6]),
        '7' => Some(&[0, 2, 5]),
        '8' => Some(&[0, 1, 2, 3, 4, 5, 6]),
        '9' => Some(&[0, 1, 2, 3, 5, 6]),
        _ => None,
    }
}

fn append_overlay_digit(
    vertices: &mut Vec<super::super::render_pass::debug_overlay_pass::DebugOverlayVertex>,
    indices: &mut Vec<u32>,
    digit: char,
    x: f32,
    y: f32,
    scale: f32,
    color: [f32; 4],
    screen_w: f32,
    screen_h: f32,
) {
    let Some(segments) = digit_segments(digit) else {
        return;
    };
    let thickness = 1.5 * scale;
    let width = 7.0 * scale;
    let height = 12.0 * scale;
    let mid = y + height * 0.5;
    let horizontal_width = (width - thickness).max(thickness);
    let vertical_height = (height * 0.5 - thickness).max(thickness);
    let segment_rects = [
        (x, y, x + horizontal_width, y + thickness),
        (x, y, x + thickness, y + vertical_height),
        (x + width - thickness, y, x + width, y + vertical_height),
        (
            x,
            mid - thickness * 0.5,
            x + horizontal_width,
            mid + thickness * 0.5,
        ),
        (x, mid, x + thickness, y + height),
        (x + width - thickness, mid, x + width, y + height),
        (x, y + height - thickness, x + horizontal_width, y + height),
    ];

    for segment in segments {
        let (left, top, right, bottom) = segment_rects[*segment];
        append_overlay_rect_quad(
            vertices, indices, left, top, right, bottom, color, screen_w, screen_h,
        );
    }
}

pub(super) fn append_overlay_label_geometry(
    vertices: &mut Vec<super::super::render_pass::debug_overlay_pass::DebugOverlayVertex>,
    indices: &mut Vec<u32>,
    snapshot: &super::super::base_component::BoxModelSnapshot,
    label: &str,
    accent_color: [f32; 4],
    scale_factor: f32,
    screen_w: f32,
    screen_h: f32,
) {
    if label.is_empty() {
        return;
    }

    let scale = scale_factor.max(0.0001);
    let digit_scale = 0.8 * scale;
    let digit_width = 7.0 * digit_scale;
    let digit_height = 12.0 * digit_scale;
    let digit_gap = 1.5 * scale;
    let padding_x = 3.0 * scale;
    let padding_y = 2.5 * scale;
    let text_width = label.chars().count() as f32 * digit_width
        + label.chars().count().saturating_sub(1) as f32 * digit_gap;
    let label_width = text_width + padding_x * 2.0;
    let label_height = digit_height + padding_y * 2.0;
    let left = (snapshot.x * scale).max(0.0);
    let top = (snapshot.y * scale).sub(label_height).max(0.0);
    let right = (left + label_width).min(screen_w);
    let bottom = (top + label_height).min(screen_h);

    append_overlay_rect_quad(
        vertices,
        indices,
        left,
        top,
        right,
        bottom,
        [accent_color[0], accent_color[1], accent_color[2], 0.7],
        screen_w,
        screen_h,
    );

    let mut cursor_x = left + padding_x;
    let text_top = top + padding_y;
    for digit in label.chars() {
        append_overlay_digit(
            vertices,
            indices,
            digit,
            cursor_x,
            text_top,
            digit_scale,
            [0.0, 0.0, 0.0, 0.9],
            screen_w,
            screen_h,
        );
        cursor_x += digit_width + digit_gap;
    }
}

pub(super) fn build_reuse_overlay_geometry(
    snapshot: &super::super::base_component::BoxModelSnapshot,
    scale_factor: f32,
    screen_w: f32,
    screen_h: f32,
    color: [f32; 4],
    label: Option<&str>,
) -> (
    Vec<super::super::render_pass::debug_overlay_pass::DebugOverlayVertex>,
    Vec<u32>,
) {
    let scale = scale_factor.max(0.0001);
    let left = snapshot.x * scale;
    let top = snapshot.y * scale;
    let right = (snapshot.x + snapshot.width.max(0.0)) * scale;
    let bottom = (snapshot.y + snapshot.height.max(0.0)) * scale;
    if right <= left || bottom <= top {
        return (Vec::new(), Vec::new());
    }

    let corners = [[left, top], [right, top], [right, bottom], [left, bottom]];
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (u, v) in [(0_usize, 1_usize), (1, 2), (2, 3), (3, 0)] {
        append_overlay_line_quad(
            &mut vertices,
            &mut indices,
            corners[u],
            corners[v],
            2.0 * scale,
            color,
            screen_w,
            screen_h,
        );
    }
    if let Some(label) = label {
        append_overlay_label_geometry(
            &mut vertices,
            &mut indices,
            snapshot,
            label,
            color,
            scale,
            screen_w,
            screen_h,
        );
    }
    (vertices, indices)
}
