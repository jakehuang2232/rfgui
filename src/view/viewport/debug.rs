use super::*;

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
    let mut grouped_indices: FxHashMap<String, usize> = FxHashMap::default();

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

    let mut nodes = vec![
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
        TraceRenderNode::with_children(
            format!("prepare_upload (passes={})", profile.prepare_pass_count),
            profile.prepare_upload_ms,
            profile
                .prepare_by_pass_name
                .iter()
                .take(8)
                .map(|(name, count, ms)| {
                    TraceRenderNode::new(format!("{name} (passes={count})"), *ms)
                })
                .collect(),
        ),
    ];
    if profile.topology_cache_hit {
        nodes.insert(0, TraceRenderNode::new("topology_cache [HIT]", 0.0));
    }
    nodes
}

pub(super) fn build_text_measure_trace_nodes(
    profile: &crate::view::base_component::TextMeasureProfile,
) -> Vec<TraceRenderNode> {
    let entries: &[(&str, usize, Option<usize>, f64)] = &[
        (
            "text.first_wrapped_fragment",
            profile.first_wrapped_fragment_calls,
            Some(profile.first_wrapped_fragment_cache_hits),
            profile.first_wrapped_fragment_ms,
        ),
        (
            "text.wrapped_suffix_fragments",
            profile.wrapped_suffix_fragments_calls,
            Some(profile.wrapped_suffix_fragments_cache_hits),
            profile.wrapped_suffix_fragments_ms,
        ),
        (
            "text.relayout_from_base",
            profile.relayout_from_base_calls,
            Some(profile.relayout_from_base_cache_hits),
            profile.relayout_from_base_ms,
        ),
        (
            "text.ensure_shaped_base_buffer",
            profile.ensure_shaped_base_buffer_calls,
            Some(profile.ensure_shaped_base_buffer_cache_hits),
            profile.ensure_shaped_base_buffer_ms,
        ),
        (
            "text.measure_text_layout",
            profile.measure_text_layout_calls,
            Some(profile.measure_text_layout_cache_hits),
            profile.measure_text_layout_ms,
        ),
        (
            "text.trimmed_suffix_shape_line",
            profile.trimmed_suffix_shape_line_calls,
            Some(profile.trimmed_suffix_shape_line_cache_hits),
            profile.trimmed_suffix_shape_line_ms,
        ),
    ];
    entries
        .iter()
        .filter(|(_, calls, _, _)| *calls > 0)
        .map(|(name, calls, hits, ms)| {
            let label = match hits {
                Some(h) => format!("{name} (calls={calls}, hits={h})"),
                None => format!("{name} (calls={calls})"),
            };
            TraceRenderNode::new(label, *ms)
        })
        .collect()
}

pub(super) fn build_layout_place_trace_nodes(
    profile: &crate::view::base_component::LayoutPlaceProfile,
) -> Vec<TraceRenderNode> {
    let place_layout_total_ms = profile.place_layout_inline_ms
        + profile.place_layout_flex_ms
        + profile.place_layout_flow_ms;
    let place_flex_children_total_ms = profile.place_flex_children_ms + place_layout_total_ms;
    let place_children_total_ms = profile.place_children_ms
        + place_flex_children_total_ms
        + profile.non_axis_child_place_ms
        + profile.absolute_child_place_ms
        + profile.inline_ifc_root_install_ms
        + profile.update_content_size_ms
        + profile.clamp_scroll_ms
        + profile.recompute_hit_test_ms;
    let place_flex_children = TraceRenderNode::with_children(
        "place_flex_children",
        place_flex_children_total_ms,
        vec![
            TraceRenderNode::new("place_layout_inline", profile.place_layout_inline_ms),
            TraceRenderNode::new("place_layout_flex", profile.place_layout_flex_ms),
            TraceRenderNode::new("place_layout_flow", profile.place_layout_flow_ms),
        ],
    );
    let place_children = TraceRenderNode::with_children(
        "place_children",
        place_children_total_ms,
        vec![
            place_flex_children,
            TraceRenderNode::new(
                format!("child_place (calls={})", profile.child_place_calls),
                profile.non_axis_child_place_ms,
            ),
            TraceRenderNode::new(
                format!(
                    "skipped_child_place (calls={})",
                    profile.skipped_child_place_calls
                ),
                0.0,
            ),
            TraceRenderNode::new(
                format!(
                    "translated_subtree (roots={}, nodes={})",
                    profile.translated_subtree_roots, profile.translated_subtree_nodes
                ),
                0.0,
            ),
            TraceRenderNode::new(
                format!(
                    "placement_skip_failures (total={}, dirty_subtree={}, non_base_element={}, non_leaf={}, anchor_name={}, anchor_ref={}, absolute_descendant={}, runtime_state={}, placement_mismatch={}, placement_dirty_self={}, hit_test_clip_mismatch={}, anchor_parent_clip_mismatch={})",
                    profile.placement_skip_failures.total(),
                    profile.placement_skip_failures.dirty_subtree,
                    profile.placement_skip_failures.non_base_element,
                    profile.placement_skip_failures.non_leaf,
                    profile.placement_skip_failures.anchor_name,
                    profile.placement_skip_failures.anchor_ref,
                    profile.placement_skip_failures.absolute_descendant,
                    profile.placement_skip_failures.runtime_state,
                    profile.placement_skip_failures.placement_mismatch,
                    profile.placement_skip_failures.placement_dirty_self,
                    profile.placement_skip_failures.hit_test_clip_mismatch,
                    profile.placement_skip_failures.anchor_parent_clip_mismatch
                ),
                0.0,
            ),
            TraceRenderNode::new(
                format!(
                    "absolute_child_place (calls={})",
                    profile.absolute_child_place_calls
                ),
                profile.absolute_child_place_ms,
            ),
            TraceRenderNode::new(
                format!(
                    "inline_ifc_root_install (calls={}, reuse={})",
                    profile.inline_ifc_root_install_calls,
                    profile.inline_ifc_root_install_reuse_calls
                ),
                profile.inline_ifc_root_install_ms,
            ),
            TraceRenderNode::new("update_content_size", profile.update_content_size_ms),
            TraceRenderNode::new("clamp_scroll", profile.clamp_scroll_ms),
            TraceRenderNode::new("recompute_hit_test", profile.recompute_hit_test_ms),
        ],
    );
    vec![
        TraceRenderNode::new(
            format!("place_self (nodes={})", profile.node_count),
            profile.place_self_ms,
        ),
        place_children,
        TraceRenderNode::new(
            format!(
                "ifc_measure (cheap={}, shortcircuit={}, full={})",
                profile.ifc_measure_cheap,
                profile.ifc_measure_shortcircuit,
                profile.ifc_measure_full,
            ),
            0.0,
        ),
        TraceRenderNode::new(
            format!(
                "measure_ran (self_dirty={}, child_dirty={}, proposal_changed={} [size={}, viewport={}, percent_base={}, first={}])",
                profile.measure_ran_self_dirty,
                profile.measure_ran_child_dirty,
                profile.measure_ran_proposal_changed,
                profile.proposal_changed_size,
                profile.proposal_changed_viewport,
                profile.proposal_changed_percent_base,
                profile.proposal_changed_first,
            ),
            0.0,
        ),
        TraceRenderNode::new(
            format!(
                "axis_placement_eligibility (candidates={}, clean_subtree={}, dirty_subtree={}, potential_replay={}, inline={}, flex={}, flow={})",
                profile.axis_placement_eligibility.candidate_child_places,
                profile
                    .axis_placement_eligibility
                    .clean_subtree_child_places,
                profile
                    .axis_placement_eligibility
                    .dirty_subtree_child_places,
                profile
                    .axis_placement_eligibility
                    .potential_replay_child_places,
                profile.axis_placement_eligibility.inline_child_places,
                profile.axis_placement_eligibility.flex_child_places,
                profile.axis_placement_eligibility.flow_child_places,
            ),
            0.0,
        ),
        TraceRenderNode::new(
            format!(
                "axis_placement_potential_replay_by_layout (inline={}, flex={}, flow={})",
                profile
                    .axis_placement_eligibility
                    .inline_potential_replay_child_places,
                profile
                    .axis_placement_eligibility
                    .flex_potential_replay_child_places,
                profile
                    .axis_placement_eligibility
                    .flow_potential_replay_child_places,
            ),
            0.0,
        ),
        TraceRenderNode::new(
            format!(
                "axis_placement_blockers (total={}, dirty_subtree={}, non_base_element={}, non_leaf={}, anchor_name={}, anchor_ref={}, absolute_descendant={}, runtime_state={}, placement_mismatch={}, placement_dirty_self={}, hit_test_clip_mismatch={}, anchor_parent_clip_mismatch={})",
                profile.axis_placement_eligibility.blockers.total(),
                profile.axis_placement_eligibility.blockers.dirty_subtree,
                profile.axis_placement_eligibility.blockers.non_base_element,
                profile.axis_placement_eligibility.blockers.non_leaf,
                profile.axis_placement_eligibility.blockers.anchor_name,
                profile.axis_placement_eligibility.blockers.anchor_ref,
                profile
                    .axis_placement_eligibility
                    .blockers
                    .absolute_descendant,
                profile.axis_placement_eligibility.blockers.runtime_state,
                profile
                    .axis_placement_eligibility
                    .blockers
                    .placement_mismatch,
                profile
                    .axis_placement_eligibility
                    .blockers
                    .placement_dirty_self,
                profile
                    .axis_placement_eligibility
                    .blockers
                    .hit_test_clip_mismatch,
                profile
                    .axis_placement_eligibility
                    .blockers
                    .anchor_parent_clip_mismatch
            ),
            0.0,
        ),
    ]
}

#[cfg(test)]
pub(super) fn build_layout_traversal_trace_nodes(
    profile: &super::frame::LayoutTraversalProfile,
) -> Vec<TraceRenderNode> {
    vec![
        TraceRenderNode::new(
            "sync_registered_elements",
            profile.sync_registered_elements_ms,
        ),
        TraceRenderNode::new(
            format!(
                "dirty_refresh_before_measure (roots={})",
                profile.root_count
            ),
            profile.dirty_refresh_before_measure_ms,
        ),
        TraceRenderNode::new(
            format!("measure_roots (roots={})", profile.root_count),
            profile.measure_roots_ms,
        ),
        TraceRenderNode::new(
            format!(
                "measure_clean_child_candidates (clean={}, dirty={})",
                profile.measure_candidate_clean_children, profile.measure_dirty_children
            ),
            0.0,
        ),
        TraceRenderNode::new(
            format!("dirty_refresh_before_place (roots={})", profile.root_count),
            profile.dirty_refresh_before_place_ms,
        ),
        TraceRenderNode::new(
            format!("place_roots (roots={})", profile.root_count),
            profile.place_roots_ms,
        ),
        TraceRenderNode::new(
            format!(
                "placement_clean_child_candidates (clean={}, dirty={})",
                profile.placement_candidate_clean_children, profile.placement_dirty_children
            ),
            0.0,
        ),
        TraceRenderNode::new(
            format!(
                "skipped_child_place_calls (count={})",
                profile.skipped_child_place_calls
            ),
            0.0,
        ),
        TraceRenderNode::new(
            format!("collect_box_models (roots={})", profile.root_count),
            profile.collect_box_models_ms,
        ),
    ]
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

pub(super) fn build_debug_overlay_geometry(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_traversal_trace_marks_gate_counts_as_candidates() {
        let profile = super::super::frame::LayoutTraversalProfile {
            root_count: 1,
            measure_candidate_clean_children: 2,
            measure_dirty_children: 1,
            placement_candidate_clean_children: 3,
            placement_dirty_children: 0,
            skipped_child_place_calls: 2,
            ..Default::default()
        };
        let root = TraceRenderNode::with_children(
            "layout_traversal",
            0.0,
            build_layout_traversal_trace_nodes(&profile),
        );
        let trace = format_trace_render_tree(&root);

        assert!(trace.contains("measure_clean_child_candidates (clean=2, dirty=1)"));
        assert!(trace.contains("placement_clean_child_candidates (clean=3, dirty=0)"));
        assert!(trace.contains("skipped_child_place_calls (count=2)"));
    }

    #[test]
    fn layout_place_trace_nests_overlapping_place_timings() {
        let profile = crate::view::base_component::LayoutPlaceProfile {
            node_count: 4,
            place_self_ms: 1.0,
            place_children_ms: 10.0,
            place_flex_children_ms: 8.0,
            place_layout_flex_ms: 3.0,
            place_layout_flow_ms: 5.0,
            non_axis_child_place_ms: 7.0,
            absolute_child_place_ms: 2.0,
            child_place_calls: 6,
            absolute_child_place_calls: 2,
            update_content_size_ms: 1.0,
            clamp_scroll_ms: 0.5,
            recompute_hit_test_ms: 0.25,
            ..Default::default()
        };
        let root =
            TraceRenderNode::with_children("place", 40.0, build_layout_place_trace_nodes(&profile));
        let trace = format_trace_render_tree(&root);

        assert!(trace.contains("├─ place_children"));
        assert!(trace.contains("│  ├─ place_flex_children"));
        assert!(trace.contains("│  │  ├─ place_layout_inline"));
        assert!(trace.contains("│  │  ├─ place_layout_flex"));
        assert!(trace.contains("│  │  └─ place_layout_flow"));
        assert!(trace.contains("│  ├─ child_place (calls=6)"));
        assert!(trace.contains("│  ├─ absolute_child_place (calls=2)"));
        assert!(trace.contains("child_place (calls=6) \u{1b}[32m7.000ms\u{1b}[0m"));
        assert!(trace.contains("absolute_child_place (calls=2) \u{1b}[32m2.000ms\u{1b}[0m"));
    }
}
