use super::*;

impl Viewport {
    /// Run a single layout pass: measure → place → collect_box_models.
    /// Returns profiling data for the pass.
    fn run_layout_pass(
        &mut self,
        roots: &mut [Box<dyn crate::view::base_component::ElementTrait>],
    ) -> LayoutPassResult {
        self.compositor.frame_box_models.clear();
        crate::view::base_component::reset_text_measure_profile();

        let measure_started_at = Instant::now();
        for root in roots.iter_mut() {
            root.measure(crate::view::base_component::LayoutConstraints {
                max_width: self.logical_width,
                max_height: self.logical_height,
                viewport_width: self.logical_width,
                viewport_height: self.logical_height,
                percent_base_width: Some(self.logical_width),
                percent_base_height: Some(self.logical_height),
            });
        }
        let measure_ms = measure_started_at.elapsed().as_secs_f64() * 1000.0;
        let text_measure_profile = crate::view::base_component::take_text_measure_profile();

        let place_started_at = Instant::now();
        crate::view::base_component::reset_layout_place_profile();
        for root in roots.iter_mut() {
            root.place(crate::view::base_component::LayoutPlacement {
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
            });
        }
        let place_ms = place_started_at.elapsed().as_secs_f64() * 1000.0;
        let place_profile = crate::view::base_component::take_layout_place_profile();

        let collect_started_at = Instant::now();
        self.refresh_frame_box_models(roots);
        let collect_box_models_ms = collect_started_at.elapsed().as_secs_f64() * 1000.0;

        LayoutPassResult {
            measure_ms,
            place_ms,
            collect_box_models_ms,
            text_measure_profile,
            place_profile,
        }
    }

    fn push_debug_reuse_overlay_geometry(&mut self) {
        if !self.debug_options.trace_reuse_path {
            return;
        }
        let scale = self.scale_factor.max(0.0001);
        let screen_w = self.gpu.surface_config.width.max(1) as f32;
        let screen_h = self.gpu.surface_config.height.max(1) as f32;
        let snapshots_by_id = self
            .compositor
            .frame_box_models
            .iter()
            .map(|snapshot| (snapshot.node_id, *snapshot))
            .collect::<HashMap<_, _>>();
        let mut overlay_batches = Vec::new();
        let promoted_node_ids = self.compositor.promotion_state.promoted_node_ids.clone();
        for record in snapshot_debug_reuse_path() {
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
        let any_detail = opts.trace_layout_detail
            || opts.trace_compile_detail
            || opts.trace_execute_detail;
        let layout_with_transition_ms =
            t.layout_ms + t.post_layout_transition_ms + t.relayout_ms;

        // --- begin_frame (expand when any detail flag is on) ---
        let begin_frame = if any_detail {
            TraceRenderNode::with_children(
                "begin_frame",
                t.begin_frame_ms,
                vec![
                    TraceRenderNode::new("acquire_surface_texture", t.begin_frame_acquire_ms),
                    TraceRenderNode::new("create_surface_view", t.begin_frame_create_view_ms),
                    TraceRenderNode::new(
                        "create_command_encoder",
                        t.begin_frame_create_encoder_ms,
                    ),
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
                    TraceRenderNode::new(
                        "collect_box_models",
                        t.layout_collect_box_models_ms,
                    ),
                    TraceRenderNode::new(
                        "post_layout_transition",
                        t.post_layout_transition_ms,
                    ),
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
            TraceRenderNode::with_children(
                "compile",
                t.compile_ms,
                t.compile_children.clone(),
            )
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

    fn render_render_tree(
        &mut self,
        roots: &mut [Box<dyn crate::view::base_component::ElementTrait>],
        dt: f32,
        now_seconds: f64,
    ) -> bool {
        let frame_start = Instant::now();
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
        let layout_result = self.run_layout_pass(roots);
        timings.layout_measure_ms = layout_result.measure_ms;
        timings.layout_place_ms = layout_result.place_ms;
        timings.layout_collect_box_models_ms = layout_result.collect_box_models_ms;
        timings.layout_text_measure_profile = layout_result.text_measure_profile;
        timings.layout_place_profile = layout_result.place_profile;
        timings.layout_ms = layout_started_at.elapsed().as_secs_f64() * 1000.0;

        // After layout is resolved for this frame, immediately run visual/style/scroll transitions
        // so their updated endpoints are visible in the same frame.
        let post_layout_transition_started_at = Instant::now();
        let post_layout_transition = self.run_post_layout_transitions(roots, dt, now_seconds);
        timings.post_layout_transition_ms =
            post_layout_transition_started_at.elapsed().as_secs_f64() * 1000.0;

        // --- Relayout after transition (if needed) ---
        let relayout_started_at = Instant::now();
        if post_layout_transition.relayout_required {
            let relayout_result = self.run_layout_pass(roots);
            timings.relayout_measure_ms = relayout_result.measure_ms;
            timings.relayout_place_ms = relayout_result.place_ms;
            timings.relayout_collect_box_models_ms = relayout_result.collect_box_models_ms;
            timings.relayout_place_profile = relayout_result.place_profile;
        }
        timings.relayout_ms = relayout_started_at.elapsed().as_secs_f64() * 1000.0;

        // --- Promotion ---
        let update_promotion_started_at = Instant::now();
        self.update_promotion_state(roots);
        timings.update_promotion_ms =
            update_promotion_started_at.elapsed().as_secs_f64() * 1000.0;

        // --- Build frame graph ---
        let build_graph_started_at = Instant::now();
        self.clear_debug_overlay_geometry();
        let mut graph = FrameGraph::new();
        let mut ctx = crate::view::base_component::UiBuildContext::new(
            self.gpu.surface_config.width,
            self.gpu.surface_config.height,
            self.gpu.surface_config.format,
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
        for root in roots.iter_mut() {
            if ctx.is_node_promoted(root.id()) {
                let root_id = root.id();
                let requested_update = ctx
                    .promoted_update_kind(root_id)
                    .unwrap_or(PromotedLayerUpdateKind::Reraster);
                if let Some(element) = root
                    .as_any_mut()
                    .downcast_mut::<crate::view::base_component::Element>()
                {
                    if let Some(reason) = element.inline_promotion_rendering_reason() {
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
                                clip_rect: element.absolute_clip_scissor_rect(),
                            });
                            let next_state = element.build(
                                &mut graph,
                                crate::view::base_component::UiBuildContext::from_parts(
                                    ctx.viewport(),
                                    ctx.state_clone(),
                                ),
                            );
                            ctx.set_state(next_state);
                            continue;
                        }
                    }
                }
                let update_kind = requested_update;
                let can_reuse_subtree =
                    crate::view::base_component::can_reuse_promoted_subtree(root.as_ref(), &ctx);
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
                let layer_target = root_ctx.allocate_promoted_layer_target(
                    &mut graph,
                    root_id,
                    root.promotion_composite_bounds(),
                );
                root_ctx.set_current_target(layer_target);
                let next_state = if let Some(element) =
                    root.as_any_mut()
                        .downcast_mut::<crate::view::base_component::Element>()
                {
                    element.build_promoted_layer(
                        &mut graph,
                        root_ctx,
                        update_kind,
                        can_reuse,
                        DebugReusePathContext::Root,
                    )
                } else if can_reuse {
                    record_debug_reuse_path(DebugReusePathRecord {
                        node_id: root.id(),
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
                        node_id: root.id(),
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
                        crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
                        crate::view::render_pass::clear_pass::ClearInput {
                            pass_context: root_ctx.graphics_pass_context(),
                            clear_depth_stencil: true,
                        },
                        crate::view::render_pass::clear_pass::ClearOutput {
                            render_target: layer_target,
                        },
                    ));
                    root.build(&mut graph, root_ctx)
                };
                ctx.merge_child_state_side_effects(&next_state);
                let layer_target = next_state.current_target().unwrap_or(layer_target);
                self.composite_promoted_root(&mut graph, &mut ctx, root.as_ref(), layer_target);
            } else {
                let next_state = root.build(
                    &mut graph,
                    crate::view::base_component::UiBuildContext::from_parts(
                        ctx.viewport(),
                        ctx.state_clone(),
                    ),
                );
                ctx.set_state(next_state);
            }
        }
        let mut deferred_node_ids = ctx.take_deferred_node_ids();
        let mut deferred_index = 0usize;
        while deferred_index < deferred_node_ids.len() {
            let node_id = deferred_node_ids[deferred_index];
            deferred_index += 1;
            for root in roots.iter_mut() {
                if crate::view::base_component::build_node_by_id(
                    root.as_mut(),
                    node_id,
                    &mut graph,
                    &mut ctx,
                ) {
                    break;
                }
            }
            let newly_deferred = ctx.take_deferred_node_ids();
            if !newly_deferred.is_empty() {
                deferred_node_ids.extend(newly_deferred);
            }
        }
        self.push_debug_reuse_overlay_geometry();
        let dependency_handle = ctx.current_target().and_then(|target| target.handle());
        if let Some(dep_handle) = dependency_handle {
            let present_pass = crate::view::render_pass::present_surface_pass::PresentSurfacePass::new(
                crate::view::render_pass::present_surface_pass::PresentSurfaceParams,
                crate::view::render_pass::present_surface_pass::PresentSurfaceInput {
                    source: crate::view::render_pass::draw_rect_pass::RenderTargetIn::with_handle(
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
            println!("{}", format_reuse_path_trace());
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
        if root_changed
            && state_dirty.is_redraw_only()
            && self.try_apply_redraw_only_transform_updates(root)?
        {
            needs_rebuild = false;
        }
        if needs_rebuild {
            // Clear and save current scroll states
            self.scene.scroll_offsets.clear();
            Self::save_scroll_states(&self.scene.ui_roots, &mut self.scene.scroll_offsets);
            self.scene.element_snapshots.clear();
            Self::save_element_snapshots(&self.scene.ui_roots, &mut self.scene.element_snapshots);
            let layout_snapshots =
                crate::view::base_component::collect_layout_transition_snapshots(&self.scene.ui_roots);
            let (converted_roots, conversion_errors) =
                crate::view::renderer_adapter::rsx_to_elements_lossy_with_context(
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
            if converted_roots.is_empty() {
                eprintln!("[render_rsx] no valid root nodes converted; keep previous render tree");
                self.scene.last_rsx_root = Some(root.clone());
                return Ok(());
            }
            self.scene.ui_roots = converted_roots;
            self.scene.last_rsx_root = Some(root.clone());

            // Restore scroll states into new elements
            Self::restore_scroll_states(&mut self.scene.ui_roots, &self.scene.scroll_offsets);
            Self::restore_element_snapshots(&mut self.scene.ui_roots, &self.scene.element_snapshots);
            crate::view::base_component::seed_layout_transition_snapshots(
                &mut self.scene.ui_roots,
                &layout_snapshots,
            );
            let mut rebuilt_roots = std::mem::take(&mut self.scene.ui_roots);
            let has_inflight_transition =
                self.sync_inflight_transition_state(&mut rebuilt_roots);
            self.scene.ui_roots = rebuilt_roots;
            if has_inflight_transition {
                self.request_redraw();
            }
        }
        self.sync_focus_dispatch();
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let canceled_tracks = self.cancel_disallowed_transition_tracks(&roots);
        let reconciled_transition_state = crate::view::base_component::reconcile_transition_runtime_state(
            &mut roots,
            &active_channels_by_node(&self.transitions.transition_claims),
        );
        let (dt, now_seconds) = self.transition_timing();
        let transition_changed_before_render = canceled_tracks
            || reconciled_transition_state
            || self.run_pre_layout_transitions(&mut roots, dt, now_seconds);
        let mut transition_changed_after_layout = false;
        if !roots.is_empty() {
            transition_changed_after_layout = self.render_render_tree(&mut roots, dt, now_seconds);
        }
        let next_hover_target = self.mouse_position_viewport().and_then(|(x, y)| {
            roots
                .iter()
                .rev()
                .find_map(|root| crate::view::base_component::hit_test(root.as_ref(), x, y))
        });
        let hover_changed = Self::sync_hover_visual_only(
            &mut roots,
            &mut self.input_state.hovered_node_id,
            next_hover_target,
        );
        if resource_dirty
            || hover_changed
            || transition_changed_before_render
            || transition_changed_after_layout
        {
            self.request_redraw();
        }
        if roots
            .iter()
            .any(|root| crate::view::base_component::has_animation_frame_request(root.as_ref()))
        {
            self.request_redraw();
        }
        self.scene.ui_roots = roots;
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
        crate::view::render_pass::draw_rect_pass::begin_draw_rect_resources_frame();
        crate::view::render_pass::shadow_module::begin_shadow_resources_frame();

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
            .create_view(&wgpu::TextureViewDescriptor::default());
        let (view, resolve_view) = if self.gpu.msaa_sample_count > 1 {
            let Some(msaa_view) = self.gpu.surface_msaa_view.as_ref() else {
                return None;
            };
            (msaa_view.clone(), Some(surface_view))
        } else {
            (surface_view, None)
        };
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
        if let Some(staging_belt) = self.gpu.upload_staging_belt.as_mut() {
            staging_belt.finish();
        }

        let submit_started_at = Instant::now();
        let queue = self.gpu.queue.as_ref().unwrap();
        queue.submit(Some(frame.encoder.finish()));
        if let Some(staging_belt) = self.gpu.upload_staging_belt.as_mut() {
            staging_belt.recall();
        }
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
