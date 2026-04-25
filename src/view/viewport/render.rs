use super::*;

impl Viewport {
    /// Run a single layout pass: measure → place → collect_box_models.
    /// Returns profiling data for the pass.
    fn run_layout_pass(&mut self) -> LayoutPassResult {
        self.compositor.frame_box_models.clear();
        crate::view::base_component::reset_text_measure_profile();

        // Take the arena out of the scene so we can pass it by &mut into
        // layout without aliasing the viewport; restore at the end.
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();

        let measure_started_at = Instant::now();
        let constraints = crate::view::base_component::LayoutConstraints {
            max_width: self.logical_width,
            max_height: self.logical_height,
            viewport_width: self.logical_width,
            viewport_height: self.logical_height,
            percent_base_width: Some(self.logical_width),
            percent_base_height: Some(self.logical_height),
        };
        // Flush deferred arena mutations (e.g. TextArea projection subtree
        // commits queued by imperative setters between frames). Must run
        // before measure so layout sees the current projection state.
        for &root_key in &root_keys {
            arena.sync_subtree(root_key);
        }
        // Refresh the per-node subtree-dirty cache once at the top of the
        // measure pass so every Element::measure / place can read
        // subtree_dirty_flags via an O(1) cache lookup instead of walking
        // its entire subtree (an O(N²) trap pre-cache).
        for &root_key in &root_keys {
            arena.refresh_subtree_dirty_cache(root_key);
        }
        for &root_key in &root_keys {
            arena.with_element_taken(root_key, |root, arena| {
                root.measure(constraints, arena);
            });
        }
        let measure_ms = measure_started_at.elapsed().as_secs_f64() * 1000.0;
        let text_measure_profile = crate::view::base_component::take_text_measure_profile();

        let place_started_at = Instant::now();
        crate::view::base_component::reset_layout_place_profile();
        let placement = crate::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: self.logical_width,
            available_height: self.logical_height,
            viewport_width: self.logical_width,
            viewport_height: self.logical_height,
            percent_base_width: Some(self.logical_width),
            percent_base_height: Some(self.logical_height),
        };
        // Measure mutated per-node dirty bits, so refresh the cache again
        // before place so `Element::place` can read it in O(1).
        for &root_key in &root_keys {
            arena.refresh_subtree_dirty_cache(root_key);
        }
        for &root_key in &root_keys {
            arena.with_element_taken(root_key, |root, arena| {
                root.place(placement, arena);
            });
        }
        let place_ms = place_started_at.elapsed().as_secs_f64() * 1000.0;
        let place_profile = crate::view::base_component::take_layout_place_profile();

        self.scene.node_arena = arena;
        let collect_started_at = Instant::now();
        self.refresh_frame_box_models();
        let collect_box_models_ms = collect_started_at.elapsed().as_secs_f64() * 1000.0;

        LayoutPassResult {
            measure_ms,
            place_ms,
            collect_box_models_ms,
            text_measure_profile,
            place_profile,
        }
    }

    fn push_debug_reuse_overlay_geometry(&mut self, reuse_records: &[DebugReusePathRecord]) {
        if !self.debug_options.trace_reuse_path {
            return;
        }
        let scale = self.scale_factor.max(0.0001);
        let screen_w = self.gpu.surface_config.width.max(1) as f32;
        let screen_h = self.gpu.surface_config.height.max(1) as f32;
        let arena = &self.scene.node_arena;
        let mut snapshots_by_id: FxHashMap<u64, crate::view::base_component::BoxModelSnapshot> =
            FxHashMap::default();
        for &root_key in &self.scene.ui_root_keys {
            for snapshot in crate::view::base_component::collect_box_models(root_key, arena) {
                snapshots_by_id.insert(snapshot.node_id, snapshot);
            }
        }
        let mut overlay_batches = Vec::new();
        let promoted_node_ids = self.compositor.promotion_state.promoted_node_ids.clone();
        for record in reuse_records {
            let Some(snapshot) = snapshots_by_id.get(&record.node_id).copied() else {
                continue;
            };
            if !snapshot.should_render {
                continue;
            }
            let color = match (record.actual, record.reason) {
                (PromotedLayerUpdateKind::Reuse, _) => [0.15, 0.95, 0.35, 0.95],
                (PromotedLayerUpdateKind::Reraster, Some("child-scissor-clip-inline")) => {
                    [1.0, 0.9, 0.15, 0.95]
                }
                (PromotedLayerUpdateKind::Reraster, Some("child-stencil-clip-inline")) => {
                    [1.0, 0.55, 0.15, 0.95]
                }
                (
                    PromotedLayerUpdateKind::Reraster,
                    Some("absolute-viewport-clip-inline" | "absolute-anchor-clip-inline"),
                ) => [1.0, 0.2, 0.2, 0.95],
                (PromotedLayerUpdateKind::Reraster, Some(reason))
                    if reason.ends_with("-inline") =>
                {
                    [1.0, 0.8, 0.35, 0.95]
                }
                (PromotedLayerUpdateKind::Reraster, _) => [1.0, 0.45, 0.1, 0.95],
            };
            let label = promoted_node_ids
                .contains(&record.node_id)
                .then(|| record.node_id.to_string());
            let (vertices, indices) = build_reuse_overlay_geometry(
                &snapshot,
                scale,
                screen_w,
                screen_h,
                color,
                label.as_deref(),
            );
            overlay_batches.push((vertices, indices));
        }
        for (vertices, indices) in overlay_batches {
            self.push_debug_overlay_geometry(&vertices, &indices);
        }
    }

    /// Build the hierarchical trace tree from collected frame timings.
    fn build_frame_trace_tree(&self, t: &FrameTimings) -> TraceRenderNode {
        let opts = &self.debug_options;
        let any_detail =
            opts.trace_layout_detail || opts.trace_compile_detail || opts.trace_execute_detail;
        let layout_with_transition_ms = t.layout_ms + t.post_layout_transition_ms + t.relayout_ms;

        // --- begin_frame (expand when any detail flag is on) ---
        let begin_frame = if any_detail {
            TraceRenderNode::with_children(
                "begin_frame",
                t.begin_frame_ms,
                vec![
                    TraceRenderNode::new("acquire_surface_texture", t.begin_frame_acquire_ms),
                    TraceRenderNode::new("create_surface_view", t.begin_frame_create_view_ms),
                    TraceRenderNode::new("create_command_encoder", t.begin_frame_create_encoder_ms),
                ],
            )
        } else {
            TraceRenderNode::new("begin_frame", t.begin_frame_ms)
        };

        // --- layout ---
        let layout = if opts.trace_layout_detail {
            let layout_measure_children =
                build_text_measure_trace_nodes(&t.layout_text_measure_profile);
            TraceRenderNode::with_children(
                "layout",
                layout_with_transition_ms,
                vec![
                    TraceRenderNode::with_children(
                        "measure",
                        t.layout_measure_ms,
                        layout_measure_children,
                    ),
                    TraceRenderNode::with_children(
                        "place",
                        t.layout_place_ms,
                        build_layout_place_trace_nodes(&t.layout_place_profile),
                    ),
                    TraceRenderNode::new("collect_box_models", t.layout_collect_box_models_ms),
                    TraceRenderNode::new("post_layout_transition", t.post_layout_transition_ms),
                    TraceRenderNode::with_children(
                        "relayout_after_transition",
                        t.relayout_ms,
                        vec![
                            TraceRenderNode::new("measure", t.relayout_measure_ms),
                            TraceRenderNode::with_children(
                                "place",
                                t.relayout_place_ms,
                                build_layout_place_trace_nodes(&t.relayout_place_profile),
                            ),
                            TraceRenderNode::new(
                                "collect_box_models",
                                t.relayout_collect_box_models_ms,
                            ),
                        ],
                    ),
                ],
            )
        } else {
            TraceRenderNode::new("layout", layout_with_transition_ms)
        };

        // --- compile ---
        let compile = if opts.trace_compile_detail {
            TraceRenderNode::with_children("compile", t.compile_ms, t.compile_children.clone())
        } else {
            TraceRenderNode::new("compile", t.compile_ms)
        };

        // --- execute ---
        let execute = if opts.trace_execute_detail {
            let mut execute_children = if t.execute_ordered_passes.is_empty() {
                vec![TraceRenderNode::new(
                    format!("passes ({})", t.execute_pass_count),
                    0.0,
                )]
            } else {
                build_execute_detail_trace_nodes(t.execute_ordered_passes.clone())
            };
            if !t.execute_detail_ordered_passes.is_empty() {
                let detail_total_ms: f64 = t
                    .execute_detail_ordered_passes
                    .iter()
                    .map(|(_, ms, _)| *ms)
                    .sum();
                let detail_children =
                    build_execute_detail_trace_nodes(t.execute_detail_ordered_passes.clone());
                execute_children.push(TraceRenderNode::with_children(
                    "execute_detail",
                    detail_total_ms,
                    detail_children,
                ));
            }
            TraceRenderNode::with_children(
                format!("execute (passes={})", t.execute_pass_count),
                t.execute_ms,
                execute_children,
            )
        } else {
            TraceRenderNode::new(
                format!("execute (passes={})", t.execute_pass_count),
                t.execute_ms,
            )
        };

        // --- end_frame (expand when any detail flag is on) ---
        let end_frame = if any_detail {
            TraceRenderNode::with_children(
                "end_frame",
                t.end_frame_ms,
                vec![
                    TraceRenderNode::new("queue_submit", t.end_frame_submit_ms),
                    TraceRenderNode::new("present", t.end_frame_present_ms),
                ],
            )
        } else {
            TraceRenderNode::new("end_frame", t.end_frame_ms)
        };

        TraceRenderNode::with_children(
            "render_frame",
            t.rsx_build_ms + t.total_ms,
            vec![
                TraceRenderNode::new("rsx_build", t.rsx_build_ms),
                begin_frame,
                layout,
                TraceRenderNode::new("update_promotion_state", t.update_promotion_ms),
                TraceRenderNode::new("build_graph", t.build_graph_ms),
                compile,
                execute,
                end_frame,
            ],
        )
    }

    fn render_render_tree(&mut self, dt: f32, now_seconds: f64) -> bool {
        let frame_start = Instant::now();
        set_debug_trace_enabled(self.debug_options.trace_reuse_path);
        trace_promoted_build_frame_marker();
        begin_debug_reuse_path_frame();
        let begin_frame_profile = match self.begin_frame() {
            Some(profile) => profile,
            None => {
                return false;
            }
        };

        let mut timings = FrameTimings {
            begin_frame_ms: begin_frame_profile.total_ms,
            begin_frame_acquire_ms: begin_frame_profile.acquire_ms,
            begin_frame_create_view_ms: begin_frame_profile.create_view_ms,
            begin_frame_create_encoder_ms: begin_frame_profile.create_encoder_ms,
            rsx_build_ms: self.frame.rsx_build_ms,
            ..Default::default()
        };

        // --- Layout ---
        crate::view::base_component::set_text_measure_profile_enabled(
            self.debug_options.trace_render_time,
        );
        let layout_started_at = Instant::now();
        let layout_result = self.run_layout_pass();
        timings.layout_measure_ms = layout_result.measure_ms;
        timings.layout_place_ms = layout_result.place_ms;
        timings.layout_collect_box_models_ms = layout_result.collect_box_models_ms;
        timings.layout_text_measure_profile = layout_result.text_measure_profile;
        timings.layout_place_profile = layout_result.place_profile;
        timings.layout_ms = layout_started_at.elapsed().as_secs_f64() * 1000.0;

        // After layout is resolved for this frame, immediately run visual/style/scroll transitions
        // so their updated endpoints are visible in the same frame.
        let post_layout_transition_started_at = Instant::now();
        let post_layout_transition = self.run_post_layout_transitions(dt, now_seconds);
        timings.post_layout_transition_ms =
            post_layout_transition_started_at.elapsed().as_secs_f64() * 1000.0;

        // --- Relayout after transition (if needed) ---
        let relayout_started_at = Instant::now();
        if post_layout_transition.relayout_required {
            let relayout_result = self.run_layout_pass();
            timings.relayout_measure_ms = relayout_result.measure_ms;
            timings.relayout_place_ms = relayout_result.place_ms;
            timings.relayout_collect_box_models_ms = relayout_result.collect_box_models_ms;
            timings.relayout_place_profile = relayout_result.place_profile;
        }
        timings.relayout_ms = relayout_started_at.elapsed().as_secs_f64() * 1000.0;

        // --- Promotion ---
        let update_promotion_started_at = Instant::now();
        self.update_promotion_state();
        timings.update_promotion_ms = update_promotion_started_at.elapsed().as_secs_f64() * 1000.0;

        // --- Build frame graph ---
        let build_graph_started_at = Instant::now();
        self.clear_debug_overlay_geometry();
        let mut graph = FrameGraph::new();
        let mut ctx = crate::view::base_component::UiBuildContext::new(
            self.gpu.surface_config.width,
            self.gpu.surface_config.height,
            self.offscreen_format(),
            self.scale_factor,
        );
        self.apply_promotion_runtime(&mut ctx);
        let clear_uses_premultiplied_alpha = matches!(
            self.gpu.surface_config.alpha_mode,
            wgpu::CompositeAlphaMode::PostMultiplied | wgpu::CompositeAlphaMode::PreMultiplied
        );
        let mut clear_rgba = self.clear_color.to_rgba_f32();
        if clear_uses_premultiplied_alpha {
            let a = clear_rgba[3].clamp(0.0, 1.0);
            clear_rgba[0] *= a;
            clear_rgba[1] *= a;
            clear_rgba[2] *= a;
            clear_rgba[3] = a;
        }

        let output = ctx.allocate_target(&mut graph);
        let output_handle = output.handle();
        ctx.set_current_target(output.clone());
        let clear_pass = crate::view::frame_graph::ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new(clear_rgba),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: output.clone(),
                ..Default::default()
            },
        );
        if let Some(handle) = output_handle {
            ctx.set_color_target(Some(handle));
        }
        graph.add_graphics_pass(clear_pass);
        ctx.set_current_target(output);
        // Take the arena out of the scene for the duration of the build
        // walk so the build chain can thread `&mut NodeArena` through
        // without fighting the outer `&mut self` borrow. Put it back
        // before returning (any early-return below restores it first).
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys_for_build = self.scene.ui_root_keys.clone();
        for &root_key in &root_keys_for_build {
            // Peek at root id and promotion status without holding the
            // element out (avoid aliasing arena).
            let Some(root_id) = arena.get(root_key).map(|n| n.element.stable_id()) else {
                continue;
            };
            if ctx.is_node_promoted(root_id) {
                let requested_update = ctx
                    .promoted_update_kind(root_id)
                    .unwrap_or(PromotedLayerUpdateKind::Reraster);
                // Try inline promotion rendering reason on Element first.
                let (inline_reason, inline_clip_rect): (Option<&'static str>, _) = {
                    let node = arena.get(root_key).unwrap();
                    if let Some(el) = node
                        .element
                        .as_any()
                        .downcast_ref::<crate::view::base_component::Element>()
                    {
                        (
                            el.inline_promotion_rendering_reason(&arena),
                            el.absolute_clip_scissor_rect(),
                        )
                    } else {
                        (None, None)
                    }
                };
                if let Some(reason) = inline_reason {
                    if reason != "child-scissor-clip-inline"
                        && reason != "child-stencil-clip-inline"
                    {
                        record_debug_reuse_path(DebugReusePathRecord {
                            node_id: root_id,
                            context: DebugReusePathContext::Root,
                            requested: requested_update,
                            can_reuse: false,
                            actual: PromotedLayerUpdateKind::Reraster,
                            reason: Some(reason),
                            clip_rect: inline_clip_rect,
                        });
                        let child_ctx = crate::view::base_component::UiBuildContext::from_parts(
                            ctx.viewport(),
                            ctx.state_clone(),
                        );
                        let next_state = arena
                            .with_element_taken(root_key, |root, arena| {
                                if let Some(element) =
                                    root.as_any_mut()
                                        .downcast_mut::<crate::view::base_component::Element>()
                                {
                                    element.build(&mut graph, arena, child_ctx)
                                } else {
                                    root.build(&mut graph, arena, child_ctx)
                                }
                            })
                            .unwrap();
                        ctx.set_state(next_state);
                        continue;
                    }
                }
                let update_kind = requested_update;
                let (can_reuse_subtree, composite_bounds) = {
                    let node = arena.get(root_key).unwrap();
                    let element = node.element.as_ref();
                    (
                        crate::view::base_component::can_reuse_promoted_subtree(
                            element, &ctx, &arena,
                        ),
                        element.promotion_composite_bounds(),
                    )
                };
                let can_reuse = matches!(
                    update_kind,
                    crate::view::promotion::PromotedLayerUpdateKind::Reuse
                ) && can_reuse_subtree;
                let mut root_ctx = crate::view::base_component::UiBuildContext::from_parts(
                    ctx.viewport(),
                    crate::view::base_component::BuildState::for_layer_subtree_with_ancestor_clip(
                        ctx.ancestor_clip_context(),
                    ),
                );
                let layer_target =
                    root_ctx.allocate_promoted_layer_target(&mut graph, root_id, composite_bounds);
                root_ctx.set_current_target(layer_target);
                let next_state = arena
                    .with_element_taken(root_key, |root, arena| {
                        if let Some(element) = root
                            .as_any_mut()
                            .downcast_mut::<crate::view::base_component::Element>()
                        {
                            element.build_promoted_layer(
                                &mut graph,
                                arena,
                                root_ctx,
                                update_kind,
                                can_reuse,
                                DebugReusePathContext::Root,
                            )
                        } else if can_reuse {
                            record_debug_reuse_path(DebugReusePathRecord {
                                node_id: root_id,
                                context: DebugReusePathContext::Root,
                                requested: update_kind,
                                can_reuse,
                                actual: PromotedLayerUpdateKind::Reuse,
                                reason: None,
                                clip_rect: None,
                            });
                            root_ctx.into_state()
                        } else {
                            record_debug_reuse_path(DebugReusePathRecord {
                                node_id: root_id,
                                context: DebugReusePathContext::Root,
                                requested: update_kind,
                                can_reuse,
                                actual: PromotedLayerUpdateKind::Reraster,
                                reason: if matches!(update_kind, PromotedLayerUpdateKind::Reuse) {
                                    Some("reuse-blocked")
                                } else {
                                    None
                                },
                                clip_rect: None,
                            });
                            graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
                                crate::view::render_pass::clear_pass::ClearParams::new([
                                    0.0, 0.0, 0.0, 0.0,
                                ]),
                                crate::view::render_pass::clear_pass::ClearInput {
                                    pass_context: root_ctx.graphics_pass_context(),
                                    clear_depth_stencil: true,
                                },
                                crate::view::render_pass::clear_pass::ClearOutput {
                                    render_target: layer_target,
                                },
                            ));
                            root.build(&mut graph, arena, root_ctx)
                        }
                    })
                    .unwrap();
                ctx.merge_child_state_side_effects(&next_state);
                let layer_target = next_state.current_target().unwrap_or(layer_target);
                // Composite the promoted root back into the parent target.
                {
                    let node = arena.get(root_key).unwrap();
                    self.composite_promoted_root(
                        &mut graph,
                        &mut ctx,
                        node.element.as_ref(),
                        layer_target,
                    );
                }
            } else {
                let child_ctx = crate::view::base_component::UiBuildContext::from_parts(
                    ctx.viewport(),
                    ctx.state_clone(),
                );
                let next_state = arena
                    .with_element_taken(root_key, |root, arena| {
                        root.build(&mut graph, arena, child_ctx)
                    })
                    .unwrap();
                ctx.set_state(next_state);
            }
        }
        let mut deferred_node_ids = ctx.take_deferred_node_ids();
        let mut deferred_index = 0usize;
        while deferred_index < deferred_node_ids.len() {
            let node_id = deferred_node_ids[deferred_index];
            deferred_index += 1;
            for &root_key in &root_keys_for_build {
                let handled = arena
                    .with_element_taken(root_key, |root, arena| {
                        crate::view::base_component::build_node_by_id(
                            root.as_mut(),
                            node_id,
                            &mut graph,
                            arena,
                            &mut ctx,
                        )
                    })
                    .unwrap_or(false);
                if handled {
                    break;
                }
            }
            let newly_deferred = ctx.take_deferred_node_ids();
            if !newly_deferred.is_empty() {
                deferred_node_ids.extend(newly_deferred);
            }
        }
        // Build walk is done — give the arena back to the scene.
        self.scene.node_arena = arena;
        let reuse_records = take_debug_reuse_path();
        self.push_debug_reuse_overlay_geometry(&reuse_records);
        let dependency_handle = ctx.current_target().and_then(|target| target.handle());
        if let Some(dep_handle) = dependency_handle {
            let present_pass =
                crate::view::render_pass::present_surface_pass::PresentSurfacePass::new(
                    crate::view::render_pass::present_surface_pass::PresentSurfaceParams,
                    crate::view::render_pass::present_surface_pass::PresentSurfaceInput {
                        source:
                            crate::view::render_pass::draw_rect_pass::RenderTargetIn::with_handle(
                                dep_handle,
                            ),
                        ..Default::default()
                    },
                    crate::view::render_pass::present_surface_pass::PresentSurfaceOutput::default(),
                );
            let present_handle = graph.add_graphics_pass(present_pass);
            graph
                .add_pass_sink(
                    present_handle,
                    crate::view::frame_graph::ExternalSinkKind::SurfacePresent,
                )
                .expect("surface present sink should register");
        }
        timings.build_graph_ms = build_graph_started_at.elapsed().as_secs_f64() * 1000.0;

        // --- Compile ---
        // Take the cache out (moves ownership) so we can pass self mutably to compile.
        // On cache hit the graph is reused in-place; on miss it is dropped. Either way
        // the returned compiled_graph is stored back for the next frame.
        let prior_cache = self
            .frame
            .compile_cache
            .take()
            .map(|c| (c.topology_hash, c.graph));
        let compiled = match graph.compile_with_upload_cached(self, prior_cache) {
            Ok((profile, topology_hash, compiled_graph)) => {
                timings.compile_ms = profile.total_ms;
                timings.compile_children =
                    build_compile_trace_nodes(&profile, self.debug_options.trace_compile_detail);
                self.frame.compile_cache = Some(CachedCompiledGraph {
                    topology_hash,
                    graph: compiled_graph,
                });
                true
            }
            Err(err) => {
                eprintln!("[warn] frame graph compile failed: {:?}", err);
                // compile_cache already cleared by take() above
                false
            }
        };

        // --- Execute ---
        if compiled {
            if let Ok(profile) = graph.execute_profiled(self) {
                timings.execute_ms = profile.total_ms;
                timings.execute_pass_count = profile.pass_count;
                timings.execute_ordered_passes = profile.ordered_passes;
                timings.execute_detail_ordered_passes = profile.detail_ordered;
            }
        }

        // --- End frame ---
        let end_frame_profile = self.end_frame();
        timings.end_frame_ms = end_frame_profile.total_ms;
        timings.end_frame_submit_ms = end_frame_profile.submit_ms;
        timings.end_frame_present_ms = end_frame_profile.present_ms;
        timings.total_ms = frame_start.elapsed().as_secs_f64() * 1000.0;

        // --- Trace output ---
        if self.debug_options.trace_render_time {
            let trace_root = self.build_frame_trace_tree(&timings);
            println!("{}", format_trace_render_tree(&trace_root));
            println!(
                "{}",
                format_promotion_trace(
                    &self.compositor.promotion_state.decisions,
                    &self.compositor.promoted_layer_updates,
                    self.compositor.promotion_config.base_threshold,
                )
            );
        }
        crate::view::base_component::set_text_measure_profile_enabled(false);
        if self.debug_options.trace_reuse_path {
            let mut reuse_records = reuse_records;
            println!("{}", format_reuse_path_trace(&mut reuse_records));
            println!("{}", format_style_request_trace());
            println!("{}", format_style_sample_trace());
            println!("{}", format_style_promotion_trace());
        }
        self.frame.frame_stats.record_frame(frame_start.elapsed());
        // Only persist the graph when compile succeeded; a failed compile
        // leaves the graph in an inconsistent state.
        self.frame.last_frame_graph = if compiled { Some(graph) } else { None };
        post_layout_transition.redraw_changed
    }

    pub fn render_rsx(&mut self, root: &RsxNode) -> Result<(), String> {
        let state_dirty = take_state_dirty();
        // Apply any viewport mutations that component event handlers
        // enqueued via `use_viewport()` during the previous tick. Must
        // run before dirty evaluation so toggles like trace_render_time
        // take effect on the upcoming frame.
        self.apply_pending_viewport_actions();
        // Reset the animation flag — transition plugins below will set
        // it back to true if any of them still want more frames.
        self.is_animating = false;
        let resource_dirty = crate::view::image_resource::take_image_redraw_dirty()
            || crate::view::svg_resource::take_svg_redraw_dirty();
        let root_changed = self.scene.last_rsx_root.as_ref() != Some(root);
        let mut needs_rebuild = state_dirty.needs_rebuild() || root_changed;
        if root_changed && self.try_apply_placement_updates(root)? {
            needs_rebuild = false;
        }
        // Phase A M2: dark-launched incremental Fiber-commit path.
        //
        // Only engaged when ALL of:
        //   - `set_use_incremental_commit(true)` was called,
        //   - a previous `last_rsx_root` exists (not a cold start),
        //   - the full-rebuild path below would otherwise run,
        //   - we have exactly one arena root (multi-root + fragment
        //     roots are M3 territory),
        //   - every reconcile patch translates into a FiberWork that is
        //     committable under M2's Delete/Move-only setter surface.
        //
        // Any failure leaves `needs_rebuild` untouched and falls
        // through to the legacy full-rebuild path. With the flag OFF
        // (the default) this whole block is a single `if false`
        // branch and behaviour is byte-identical to the prior
        // pipeline.
        if needs_rebuild
            && self.scene.use_incremental_commit
            && self.scene.last_rsx_root.is_some()
            && !self.scene.ui_root_keys.is_empty()
        {
            let previous_root = self.scene.last_rsx_root.as_ref().unwrap();
            // 軌 1 #4 Fragment-at-root: unpack Fragment root into its
            // children so `reconcile_multi` sees the same arity that
            // the arena stores (Fragment root → N arena roots).
            let old_roots = unpack_root_set(previous_root);
            let new_roots = unpack_root_set(root);
            let rooted_patches = crate::ui::reconcile_multi(Some(&old_roots), &new_roots);
            let descriptor_ctx = crate::view::fiber_work::DescriptorContext {
                new_rsx_root: root,
                // 軌 1 #6: pass the previous tree so the translator
                // can identity-validate parent_path walks for
                // InsertChild patches.
                old_rsx_root: Some(previous_root),
                inherited_style: &self.style,
                viewport_width: self.logical_width,
                viewport_height: self.logical_height,
            };
            let translated = crate::view::fiber_work::translate_rooted_patches_all_or_nothing(
                rooted_patches,
                self.scene.node_arena.stable_id_index(),
                &self.scene.node_arena,
                &self.scene.ui_root_keys,
                &old_roots,
                &new_roots,
                Some(&descriptor_ctx),
            );
            if let Some(works) = translated {
                let all_committable = works
                    .iter()
                    .all(|w| w.is_committable(&self.scene.node_arena));
                if all_committable {
                    let apply_ctx = crate::view::fiber_work::ApplyContext {
                        viewport_style: &self.style,
                        viewport_width: self.logical_width,
                        viewport_height: self.logical_height,
                    };
                    crate::view::fiber_work::apply_fiber_works(
                        &mut self.scene.node_arena,
                        apply_ctx,
                        works,
                    );
                    // Keep the arena roots view in lockstep: ReplaceRoot
                    // mints a new root NodeKey, so always refresh from
                    // the arena after a committed batch.
                    let refreshed_roots = self.scene.node_arena.roots().to_vec();
                    self.scene.ui_root_keys = refreshed_roots;
                    self.scene.last_rsx_root = Some(root.clone());
                    needs_rebuild = false;
                }
            }
        }
        if needs_rebuild {
            // Clear and save current scroll states
            self.scene.scroll_offsets.clear();
            Self::save_scroll_states(
                &self.scene.node_arena,
                &self.scene.ui_root_keys,
                &mut self.scene.scroll_offsets,
            );
            let layout_snapshots = crate::view::base_component::collect_layout_transition_snapshots(
                &self.scene.node_arena,
                &self.scene.ui_root_keys,
            );
            let (converted_descriptors, conversion_errors) =
                crate::view::renderer_adapter::rsx_to_descriptors_with_context(
                    root,
                    &self.style,
                    self.logical_width,
                    self.logical_height,
                );
            if !conversion_errors.is_empty() {
                eprintln!(
                    "[render_rsx] skipped {} invalid node(s):\n{}",
                    conversion_errors.len(),
                    conversion_errors.join("\n")
                );
            }
            if converted_descriptors.is_empty() {
                eprintln!("[render_rsx] no valid root nodes converted; keep previous render tree");
                self.scene.last_rsx_root = Some(root.clone());
                return Ok(());
            }
            // Approach-C: drop the previous arena subtree and commit the
            // freshly-built descriptor trees as new arena roots. `ui_roots`
            // (the legacy boxed mirror) stays empty — arena is the source
            // of truth; the still-legacy render/layout boxed traversal below
            // ignores it and walks the arena via root keys instead.
            for old_key in std::mem::take(&mut self.scene.ui_root_keys) {
                self.scene.node_arena.remove_subtree(old_key);
            }
            let mut new_root_keys = Vec::with_capacity(converted_descriptors.len());
            for desc in converted_descriptors {
                let key = crate::view::renderer_adapter::commit_descriptor_tree(
                    &mut self.scene.node_arena,
                    None,
                    desc,
                );
                new_root_keys.push(key);
            }
            self.scene.ui_root_keys = new_root_keys.clone();
            self.scene.node_arena.set_roots(new_root_keys);
            self.scene.last_rsx_root = Some(root.clone());

            // Restore scroll states into new elements
            Self::restore_scroll_states(
                &self.scene.node_arena,
                &self.scene.ui_root_keys,
                &self.scene.scroll_offsets,
            );
            {
                let mut arena = std::mem::take(&mut self.scene.node_arena);
                let root_keys = self.scene.ui_root_keys.clone();
                crate::view::base_component::seed_layout_transition_snapshots(
                    &mut arena,
                    &root_keys,
                    &layout_snapshots,
                );
                self.scene.node_arena = arena;
            }
            // Drop tracks for channels the rebuilt tree no longer declares
            // before applying in-flight samples — otherwise a removed
            // transition would re-stamp the stale interpolated value over
            // the freshly synced target.
            let _ = self.cancel_disallowed_transition_tracks();
            let has_inflight_transition = self.sync_inflight_transition_state();
            if has_inflight_transition {
                self.request_redraw();
            }
        }
        self.sync_focus_dispatch();
        let canceled_tracks = self.cancel_disallowed_transition_tracks();
        let reconciled_transition_state = {
            let mut arena = std::mem::take(&mut self.scene.node_arena);
            let root_keys = self.scene.ui_root_keys.clone();
            let result = crate::view::base_component::reconcile_transition_runtime_state(
                &mut arena,
                &root_keys,
                &active_channels_by_node(&self.transitions.transition_claims),
            );
            self.scene.node_arena = arena;
            result
        };
        let (dt, now_seconds) = self.transition_timing();
        let transition_changed_before_render = canceled_tracks
            || reconciled_transition_state
            || self.run_pre_layout_transitions(dt, now_seconds);
        let mut transition_changed_after_layout = false;
        if !self.scene.ui_root_keys.is_empty() {
            transition_changed_after_layout = self.render_render_tree(dt, now_seconds);
        }
        let next_hover_target = self.pointer_position_viewport().and_then(|(x, y)| {
            self.scene.ui_root_keys.iter().rev().find_map(|&root_key| {
                crate::view::base_component::hit_test(&self.scene.node_arena, root_key, x, y)
            })
        });
        let hover_changed = {
            let mut arena = std::mem::take(&mut self.scene.node_arena);
            let root_keys = self.scene.ui_root_keys.clone();
            let result = Self::sync_hover_visual_only(
                &mut arena,
                &root_keys,
                &mut self.input_state.hovered_node_id,
                next_hover_target,
            );
            self.scene.node_arena = arena;
            result
        };
        if resource_dirty
            || hover_changed
            || transition_changed_before_render
            || transition_changed_after_layout
        {
            self.request_redraw();
        }
        if self.scene.ui_root_keys.iter().any(|&root_key| {
            crate::view::base_component::has_animation_frame_request(
                &self.scene.node_arena,
                root_key,
            )
        }) {
            self.request_redraw();
        }
        if std::mem::take(&mut self.frame.frame_presented) {
            self.notify_cursor_handler();
        }
        Ok(())
    }

    /// Build RSX (if dirty) and render a frame in one call.
    ///
    /// Requires a live `App` set via `set_app`. Checks global dirty
    /// state, calls `App::build` when a rebuild is needed, then
    /// delegates to `render_rsx` for the GPU work.
    pub fn render_frame(
        &mut self,
        services: crate::platform::PlatformServices<'_>,
    ) -> super::RenderFrameResult {
        if self.app.is_none() {
            return super::RenderFrameResult::Ok;
        }

        if peek_state_dirty().needs_rebuild() {
            self.needs_rebuild = true;
        }

        if self.needs_rebuild || self.cached_rsx.is_none() {
            let build_start = Instant::now();
            let rsx = self.with_app(services, |app, ctx| app.build(ctx));
            self.frame.rsx_build_ms = build_start.elapsed().as_secs_f64() * 1000.0;
            self.cached_rsx = Some(rsx);
            self.needs_rebuild = false;
        } else {
            self.frame.rsx_build_ms = 0.0;
        }

        if let Some(rsx) = self.cached_rsx.clone() {
            let _ = self.render_rsx(&rsx);
        }

        if self.cached_rsx.is_some() && self.frame_box_models().is_empty() {
            super::RenderFrameResult::NeedsRetry
        } else {
            super::RenderFrameResult::Ok
        }
    }

    /// Forward an `AppEvent` to the held `App::on_event`.
    pub fn dispatch_app_event(
        &mut self,
        event: &crate::app::AppEvent,
        services: crate::platform::PlatformServices<'_>,
    ) {
        self.with_app(services, |app, ctx| app.on_event(event, ctx));
    }

    /// Call `App::on_ready` exactly once (subsequent calls are no-ops).
    pub fn app_on_ready(&mut self, services: crate::platform::PlatformServices<'_>) {
        if self.ready_dispatched {
            return;
        }
        self.ready_dispatched = true;
        self.with_app(services, |app, ctx| app.on_ready(ctx));
    }

    /// Call `App::on_shutdown`.
    pub fn app_on_shutdown(&mut self, services: crate::platform::PlatformServices<'_>) {
        if self.app.is_none() {
            return;
        }
        self.with_app(services, |app, ctx| app.on_shutdown(ctx));
    }

    /// Temporarily extract the App, build an AppContext, call the
    /// closure, then put the App back. This sidesteps the borrow-checker
    /// conflict between `&mut self` (for `ViewportControl`) and
    /// `&mut self.app`.
    ///
    /// The reborrowing of `services` fields breaks the invariant lifetime
    /// binding that `&'a mut` references carry, allowing the compiler to
    /// pick a shorter, block-scoped lifetime for the `AppContext`.
    fn with_app<R>(
        &mut self,
        services: crate::platform::PlatformServices<'_>,
        f: impl FnOnce(&mut dyn crate::app::App, &mut crate::app::AppContext<'_>) -> R,
    ) -> R {
        let mut app = self.app.take().expect("no app set");
        let result = {
            let mut ctx = crate::app::AppContext {
                viewport: super::ViewportControl::new(self),
                services: crate::platform::PlatformServices {
                    clipboard: &mut *services.clipboard,
                    cursor: &mut *services.cursor,
                    redraw: services.redraw,
                },
            };
            f(&mut *app, &mut ctx)
        };
        self.app = Some(app);
        result
    }

    /// Drain the thread-local queue populated by `ui::use_viewport()` and
    /// apply each action to this viewport. Called at the top of
    /// `render_rsx` so event handlers from the prior frame land
    /// before dirty flags are read.
    fn apply_pending_viewport_actions(&mut self) {
        let actions = crate::ui::drain_viewport_actions();
        if actions.is_empty() {
            return;
        }
        let mut promotion_dirty = false;
        for action in actions {
            match action {
                crate::ui::ViewportAction::SetDebugTraceFps(on) => {
                    self.debug_options.trace_fps = on;
                    self.frame.frame_stats.set_enabled(on);
                }
                crate::ui::ViewportAction::SetDebugTraceRenderTime(on) => {
                    self.debug_options.trace_render_time = on;
                }
                crate::ui::ViewportAction::SetDebugTraceLayoutDetail(on) => {
                    self.debug_options.trace_layout_detail = on;
                }
                crate::ui::ViewportAction::SetDebugTraceCompileDetail(on) => {
                    self.debug_options.trace_compile_detail = on;
                }
                crate::ui::ViewportAction::SetDebugTraceExecuteDetail(on) => {
                    self.debug_options.trace_execute_detail = on;
                }
                crate::ui::ViewportAction::SetDebugTraceReusePath(on) => {
                    self.debug_options.trace_reuse_path = on;
                }
                crate::ui::ViewportAction::SetDebugGeometryOverlay(on) => {
                    self.debug_options.geometry_overlay = on;
                }
                crate::ui::ViewportAction::SetPromotionEnabled(on) => {
                    let mut cfg = self.compositor.promotion_config.clone();
                    cfg.enabled = on;
                    // Scene that previously relied on the atomic threshold
                    // swap in 01_window gets the same behavior here: a
                    // large threshold effectively disables layer promotion
                    // even though the `enabled` flag remains true in
                    // other call paths.
                    cfg.base_threshold = if on {
                        ViewportPromotionConfig::default().base_threshold
                    } else {
                        1000
                    };
                    self.set_promotion_config(cfg);
                    promotion_dirty = true;
                }
                crate::ui::ViewportAction::SetClearColor(color) => {
                    self.set_clear_color(Box::new(color));
                }
                crate::ui::ViewportAction::RequestRedraw => self.request_redraw(),
            }
        }
        if promotion_dirty {
            self.invalidate_promoted_layer_reuse();
        }
    }

    fn begin_frame(&mut self) -> Option<BeginFrameProfile> {
        let total_started_at = Instant::now();
        // If a frame is already in progress (e.g. recursive render call),
        // return a zero-cost profile so the caller proceeds with the
        // existing encoder rather than skipping the frame entirely.
        if self.frame.frame_state.is_some() {
            return Some(BeginFrameProfile {
                total_ms: 0.0,
                acquire_ms: 0.0,
                create_view_ms: 0.0,
                create_encoder_ms: 0.0,
            });
        }
        if !self.apply_pending_reconfigure() {
            return None;
        }
        self.frame.offscreen_render_target_pool.begin_frame();
        self.frame.draw_rect_uniform_cursor = 0;
        self.frame.draw_rect_uniform_offset = 0;
        self.frame.gradient_stops_byte_cursor = 0;
        crate::view::render_pass::draw_rect_pass::begin_draw_rect_resources_frame();
        crate::view::render_pass::shadow_module::begin_shadow_resources_frame();
        crate::view::render_pass::text_pass::begin_text_resources_frame();

        let surface = match &self.gpu.surface {
            Some(s) => s,
            None => return None,
        };
        let device = match &self.gpu.device {
            Some(d) => d,
            None => return None,
        };

        let acquire_started_at = Instant::now();
        let render_texture = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                surface.configure(device, &self.gpu.surface_config);
                texture
            }
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                println!("[warn] surface lost, recreate render texture");
                surface.configure(device, &self.gpu.surface_config);
                match surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(texture)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
                    _ => return None,
                }
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return None,
        };
        let acquire_ms = acquire_started_at.elapsed().as_secs_f64() * 1000.0;

        let create_view_started_at = Instant::now();
        let surface_view = render_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor {
                format: Some(self.gpu.surface_target_format),
                ..Default::default()
            });
        let (view, resolve_view) = (surface_view, None);
        let create_view_ms = create_view_started_at.elapsed().as_secs_f64() * 1000.0;

        let create_encoder_started_at = Instant::now();
        let encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let create_encoder_ms = create_encoder_started_at.elapsed().as_secs_f64() * 1000.0;

        self.frame.frame_state = Some(FrameState {
            render_texture,
            view,
            resolve_view,
            encoder,
            depth_view: self.gpu.depth_view.clone(),
        });
        Some(BeginFrameProfile {
            total_ms: total_started_at.elapsed().as_secs_f64() * 1000.0,
            acquire_ms,
            create_view_ms,
            create_encoder_ms,
        })
    }

    fn end_frame(&mut self) -> EndFrameProfile {
        let total_started_at = Instant::now();
        let frame = match self.frame.frame_state.take() {
            Some(frame) => frame,
            None => {
                return EndFrameProfile {
                    total_ms: 0.0,
                    submit_ms: 0.0,
                    present_ms: 0.0,
                };
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(staging_belt) = self.gpu.upload_staging_belt.as_mut() {
            staging_belt.finish();
        }

        let submit_started_at = Instant::now();
        let queue = self.gpu.queue.as_ref().unwrap();
        queue.submit(Some(frame.encoder.finish()));
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(staging_belt) = self.gpu.upload_staging_belt.as_mut() {
            staging_belt.recall();
        }
        #[cfg(target_arch = "wasm32")]
        crate::view::render_pass::destroy_frame_transient_buffers();
        let submit_ms = submit_started_at.elapsed().as_secs_f64() * 1000.0;

        let present_started_at = Instant::now();
        frame.render_texture.present();
        let present_ms = present_started_at.elapsed().as_secs_f64() * 1000.0;
        self.frame.frame_presented = true;
        EndFrameProfile {
            total_ms: total_started_at.elapsed().as_secs_f64() * 1000.0,
            submit_ms,
            present_ms,
        }
    }
}

/// Flatten a Fragment-at-root into its children so multi-root reconcile
/// sees the same arity as the arena (Fragment root → N arena roots).
/// Non-Fragment roots pass through as a single-element slice.
fn unpack_root_set(root: &crate::ui::RsxNode) -> Vec<&crate::ui::RsxNode> {
    match root {
        crate::ui::RsxNode::Fragment(frag) => frag.children.iter().collect(),
        other => vec![other],
    }
}
