pub(super) struct SelfDecorationPaintOps {
    fill: Option<crate::view::paint::DrawRectOp>,
    border: Option<crate::view::paint::DrawRectOp>,
}

#[derive(Clone, Copy)]
struct SelfPaintRecordingGeometry {
    bounds: crate::view::base_component::Rect,
    context: crate::view::paint::PaintRecordingContext,
}

struct PreparedSelfPaintRecord {
    geometry: SelfPaintRecordingGeometry,
    shadows: Vec<crate::view::paint::PreparedShadowOp>,
    decoration: Vec<crate::view::paint::DrawRectOp>,
    payload_identity: crate::view::paint::PaintPayloadIdentity,
}

/// One frozen native texture operation owned by this Element's paint scope.
/// Bounds are prepared with zero paint offset and no owner transform. The
/// owner walk applies its snap once, before restoring self clip or compositing
/// its layer. This is a Legacy execution payload, not retained paint identity.
struct LegacyOwnerTexturePaint {
    params: crate::view::render_pass::TextureCompositeParams,
    upload: crate::view::sampled_texture::SampledTextureUpload,
}

/// Complete geometry contract for the legacy transformed-subtree surface.
///
/// Raster content stays in the untransformed logical `source_bounds`; the
/// resolved viewport transform is applied exactly once by the final texture
/// composite. Keeping this calculation in one pure snapshot prevents the
/// retained-paint rollout from independently re-deriving subtly different
/// quad, UV, paint-snap, or outer-scissor semantics.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TransformSurfaceGeometrySnapshot {
    pub(crate) source_bounds: crate::view::base_component::RetainedSurfaceBounds,
    pub(crate) visual_bounds: crate::view::base_component::RetainedSurfaceBounds,
    #[allow(dead_code)]
    // Frozen for the retained transform executor; legacy consumes the derived quad.
    pub(crate) viewport_transform: Mat4,
    pub(crate) quad_positions: [[f32; 2]; 4],
    pub(crate) uv_bounds: [f32; 4],
    pub(crate) outer_scissor_rect: Option<[u32; 4]>,
}

impl TransformSurfaceGeometrySnapshot {
    pub(crate) fn new(
        source_bounds: crate::view::base_component::RetainedSurfaceBounds,
        visual_bounds: crate::view::base_component::RetainedSurfaceBounds,
        viewport_transform: Mat4,
        outer_scissor_rect: Option<[u32; 4]>,
    ) -> Option<Self> {
        let canonical_bounds = |bounds: crate::view::base_component::RetainedSurfaceBounds| {
            bounds.x.is_finite()
                && bounds.y.is_finite()
                && bounds.width.is_finite()
                && bounds.height.is_finite()
                && bounds.width > 0.0
                && bounds.height > 0.0
        };
        if !canonical_bounds(source_bounds)
            || !canonical_bounds(visual_bounds)
            || viewport_transform
                .to_cols_array()
                .iter()
                .any(|value| !value.is_finite())
        {
            return None;
        }
        let corners = [
            Vec3::new(source_bounds.x, source_bounds.y + source_bounds.height, 0.0),
            Vec3::new(
                source_bounds.x + source_bounds.width,
                source_bounds.y + source_bounds.height,
                0.0,
            ),
            Vec3::new(source_bounds.x + source_bounds.width, source_bounds.y, 0.0),
            Vec3::new(source_bounds.x, source_bounds.y, 0.0),
        ];
        let dx = visual_bounds.x - source_bounds.x;
        let dy = visual_bounds.y - source_bounds.y;
        let mut quad_positions = [[0.0; 2]; 4];
        for (index, corner) in corners.into_iter().enumerate() {
            let transformed = viewport_transform * corner.extend(1.0);
            if !transformed.is_finite() || transformed.w.abs() <= 0.000_001 {
                return None;
            }
            let point = [
                transformed.x / transformed.w + dx,
                transformed.y / transformed.w + dy,
            ];
            if point.iter().any(|value| !value.is_finite()) {
                return None;
            }
            quad_positions[index] = point;
        }
        Some(Self {
            source_bounds,
            visual_bounds,
            viewport_transform,
            quad_positions,
            uv_bounds: [
                source_bounds.x,
                source_bounds.y,
                source_bounds.width,
                source_bounds.height,
            ],
            outer_scissor_rect,
        })
    }

    pub(crate) fn texture_composite_params(
        self,
    ) -> crate::view::render_pass::TextureCompositeParams {
        crate::view::render_pass::TextureCompositeParams {
            bounds: [
                self.visual_bounds.x,
                self.visual_bounds.y,
                self.visual_bounds.width,
                self.visual_bounds.height,
            ],
            quad_positions: Some(self.quad_positions),
            uv_bounds: Some(self.uv_bounds),
            mask_uv_bounds: None,
            use_mask: false,
            source_is_premultiplied: true,
            opacity: 1.0,
            scissor_rect: self.outer_scissor_rect,
        }
    }

    pub(crate) fn quad_aabb(self) -> Option<crate::view::base_component::RetainedSurfaceBounds> {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for [x, y] in self.quad_positions {
            if !x.is_finite() || !y.is_finite() {
                return None;
            }
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        let bounds = crate::view::base_component::RetainedSurfaceBounds {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
            corner_radii: [0.0; 4],
        };
        (bounds.x.is_finite()
            && bounds.y.is_finite()
            && bounds.width.is_finite()
            && bounds.height.is_finite()
            && bounds.width > 0.0
            && bounds.height > 0.0)
            .then_some(bounds)
    }

    #[allow(dead_code)] // C4A prepared-surface validation; production dispatch lands in C4B.
    pub(crate) fn matches_rebuilt_contract(self) -> bool {
        let Some(rebuilt) = Self::new(
            self.source_bounds,
            self.visual_bounds,
            self.viewport_transform,
            self.outer_scissor_rect,
        ) else {
            return false;
        };
        self.bitwise_eq(rebuilt)
    }

    pub(crate) fn bitwise_eq(self, other: Self) -> bool {
        let bounds_bits = |bounds: crate::view::base_component::RetainedSurfaceBounds| {
            (
                [bounds.x, bounds.y, bounds.width, bounds.height].map(f32::to_bits),
                bounds.corner_radii.map(f32::to_bits),
            )
        };
        bounds_bits(self.source_bounds) == bounds_bits(other.source_bounds)
            && bounds_bits(self.visual_bounds) == bounds_bits(other.visual_bounds)
            && self.viewport_transform.to_cols_array().map(f32::to_bits)
                == other.viewport_transform.to_cols_array().map(f32::to_bits)
            && self.quad_positions.map(|point| point.map(f32::to_bits))
                == other.quad_positions.map(|point| point.map(f32::to_bits))
            && self.uv_bounds.map(f32::to_bits) == other.uv_bounds.map(f32::to_bits)
            && self.outer_scissor_rect == other.outer_scissor_rect
    }
}

impl SelfDecorationPaintOps {
    const fn empty() -> Self {
        Self {
            fill: None,
            border: None,
        }
    }

    const fn fill(fill: crate::view::paint::DrawRectOp) -> Self {
        Self {
            fill: Some(fill),
            border: None,
        }
    }

    #[cfg(test)]
    fn test_len(&self) -> usize {
        usize::from(self.fill.is_some()) + usize::from(self.border.is_some())
    }
}

impl IntoIterator for SelfDecorationPaintOps {
    type Item = crate::view::paint::DrawRectOp;
    type IntoIter =
        std::iter::Flatten<std::array::IntoIter<Option<crate::view::paint::DrawRectOp>, 2>>;

    fn into_iter(self) -> Self::IntoIter {
        [self.fill, self.border].into_iter().flatten()
    }
}

impl Element {
    #[cfg(test)]
    pub(crate) fn set_scrollbar_shadow_blur_radius_for_test(&mut self, radius: f32) {
        self.scrollbar_shadow_blur_radius = radius;
    }

    #[cfg(test)]
    pub(crate) fn set_sampled_scrollbar_alpha_for_test(&mut self, alpha: f32) {
        self.scrollbar_interaction_pending = false;
        self.last_scrollbar_interaction = Some(crate::time::Instant::now());
        self.sampled_scrollbar_alpha = alpha;
    }

    #[cfg(test)]
    pub(crate) fn settle_scrollbar_hidden_for_test(&mut self) {
        self.scrollbar_interaction_pending = false;
        self.last_scrollbar_interaction = None;
        self.sampled_scrollbar_alpha = 0.0;
    }

    #[cfg(test)]
    pub(crate) fn set_scroll_direction_for_retained_test(&mut self, direction: ScrollDirection) {
        self.scroll_direction = direction;
    }

    #[cfg(test)]
    pub(crate) fn inline_ifc_decoration_package_for_test(
        &mut self,
    ) -> Option<
        &mut crate::view::inline_formatting_context::InlineIfcElementDecorationDrawRectPackage,
    > {
        self.inline_ifc_rollout_packages
            .decoration_draw_rect
            .as_mut()
    }

    #[cfg(test)]
    pub(crate) fn set_should_paint_for_test(&mut self, should_paint: bool) {
        self.core.should_paint = should_paint;
    }

    #[cfg(test)]
    pub(crate) fn install_empty_inline_ifc_atomic_package_for_test(&mut self) {
        let source = self
            .inline_ifc_rollout_packages
            .decoration_draw_rect
            .as_ref()
            .map(|package| package.source)
            .unwrap_or(crate::view::inline_formatting_context::InlineIfcSourceId(0));
        self.inline_ifc_rollout_packages.atomic_placement = Some(
            crate::view::inline_formatting_context::InlineIfcAtomicBoxPlacementPackage {
                source,
                placements: Vec::new(),
            },
        );
    }

    fn requires_child_mask_surface(&self, arena: &crate::view::node_arena::NodeArena) -> bool {
        if self.children.is_empty() {
            return false;
        }
        let overflow_child_indices: Vec<bool> = (0..self.children.len())
            .map(|idx| self.child_renders_outside_inner_clip(idx, arena))
            .collect();
        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.layout_state.layout_size.width.max(0.0),
            self.layout_state.layout_size.height.max(0.0),
        );
        let inner_radii = self.inner_clip_radii(outer_radii);
        self.should_clip_children(&overflow_child_indices, inner_radii, arena)
    }

    pub(super) fn prepared_retained_child_mask_plan(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::RetainedChildMaskPlan> {
        if (self.scroll_direction != ScrollDirection::None
            && !(recording_context.authorizes_frame_root_scroll_host_child_mask(self.stable_id())
                || recording_context.authorizes_generic_scroll_host_root(self.stable_id())))
            || self.inline_ifc_owned_by_root
            || (self.is_fragmentable_inline_element() && self.inline_paint_fragments.len() > 1)
            || !self.requires_child_mask_surface(arena)
            || recording_context
                .paint_offset
                .iter()
                .any(|value| !value.is_finite())
        {
            return None;
        }
        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.layout_state.layout_size.width.max(0.0),
            self.layout_state.layout_size.height.max(0.0),
        );
        let inner_radii = self.inner_clip_radii(outer_radii);
        let inner = self.inner_clip_rect();
        let bounds = Rect {
            x: inner.x + recording_context.paint_offset[0],
            y: inner.y + recording_context.paint_offset[1],
            width: inner.width,
            height: inner.height,
        };
        let logical_scissor = exact_logical_scissor_for_rect(bounds)?;
        let mut params = RectPassParams {
            position: [bounds.x, bounds.y],
            size: [bounds.width, bounds.height],
            fill_color: [0.0; 4],
            opacity: 1.0,
            ..Default::default()
        };
        params.set_border_width(0.0);
        params.set_border_radii(inner_radii.to_array());
        let op = crate::view::paint::DrawRectOp {
            params,
            mode: crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
        };
        let mut in_scope_children = Vec::new();
        let mut overflow_children = Vec::new();
        for (index, &child) in self.children.iter().enumerate() {
            if self.child_renders_outside_inner_clip(index, arena) {
                overflow_children.push(child);
            } else {
                in_scope_children.push(child);
            }
        }
        crate::view::paint::RetainedChildMaskPlan::new(
            bounds,
            logical_scissor,
            op,
            &self.children,
            in_scope_children,
            overflow_children,
        )
    }

    fn build_base_descendants_only(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        ctx: UiBuildContext,
        force_self_opaque: bool,
    ) -> BuildState {
        self.build_base_descendants_only_inner(graph, arena, ctx, force_self_opaque, true, None)
    }

    fn build_base_descendants_only_inner(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        mut ctx: UiBuildContext,
        force_self_opaque: bool,
        allow_layer: bool,
        owner_texture: Option<LegacyOwnerTexturePaint>,
    ) -> BuildState {
        let accumulated_render_transform =
            self.resolved_transform
                .map(|transform| match ctx.current_render_transform() {
                    Some(parent) => parent * transform,
                    None => transform,
                });
        ctx.set_current_render_transform(accumulated_render_transform);
        if !self.layout_state.should_render {
            // Viewport-clip descendants were already collected once at
            // frame start via `NodeArena::refresh_defer_render_nodes`.
            return ctx.into_state();
        }

        // A nonempty subtree contributes overlapping paint. Its local
        // opacity belongs to the final group composite, not each child op.
        // This also changes an existing transform layer: neutralize owner
        // paint inside that layer, then apply its opacity at composition.
        // A native Ready texture can overlap visible owner decoration even
        // without children. A bare texture has only one op, so its opacity
        // can stay on that op without an extra layer or quantization step.
        // Fragmented inline spans are painted by their IFC root; arena
        // children there are not an independently owned paint subtree.
        let group_opacity = !force_self_opaque
            && self.opacity < 1.0
            && (!self.children.is_empty() || (owner_texture.is_some() && self.core.should_paint))
            && !self.is_fragmentable_inline_element()
            && !self.inline_ifc_owned_by_root;
        if allow_layer && (self.resolved_transform.is_some() || group_opacity) {
            return self.build_legacy_layer_subtree(
                graph,
                arena,
                ctx,
                force_self_opaque,
                group_opacity,
                owner_texture,
            );
        }

        let parent_paint_projection = ctx.owner_paint_offset_projection();
        ctx.snap_owner_paint_offset([
            self.layout_state.layout_position.x,
            self.layout_state.layout_position.y,
        ]);

        let previous_scissor_rect = self.apply_self_clip_scissor(&mut ctx);

        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.layout_state.layout_size.width.max(0.0),
            self.layout_state.layout_size.height.max(0.0),
        );
        let inner_radii = self.inner_clip_radii(outer_radii);
        self.border_radius = outer_radii.max();
        let pipeline_state = self.build_render_pipeline(
            graph,
            arena,
            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
            force_self_opaque,
        );
        ctx.set_state(pipeline_state);

        let overflow_child_indices: Vec<bool> = (0..self.children.len())
            .map(|idx| self.child_renders_outside_inner_clip(idx, arena))
            .collect();

        let should_clip_children =
            self.should_clip_children(&overflow_child_indices, inner_radii, arena);
        let child_clip_scope = if should_clip_children {
            self.begin_child_clip_scope(graph, &mut ctx, inner_radii)
        } else {
            None
        };
        let should_render_children = !should_clip_children || child_clip_scope.is_some();

        // Viewport-clip descendants (`should_append_to_root_viewport_render`)
        // pin to the OS viewport and escape every ancestor scissor. The
        // canonical defer list was seeded once per frame from
        // `NodeArena::defer_render_nodes`, so skipping the children
        // loops below no longer drops viewport-anchored descendants.
        let inner_visible = self.has_visible_inner_render_area(&ctx);
        let render_children_passes = should_render_children && inner_visible;

        let child_keys: Vec<crate::view::node_arena::NodeKey> = self.children.clone();
        if render_children_passes {
            for (idx, child_key) in child_keys.iter().copied().enumerate() {
                if overflow_child_indices.get(idx).copied().unwrap_or(false) {
                    continue;
                }
                let viewport = ctx.viewport();
                let taken_state = ctx.state_clone();
                let ctx_in = UiBuildContext::from_parts(viewport.clone(), taken_state);
                let next_ctx = arena.with_element_taken(child_key, |child, arena| {
                    let ctx_local = ctx_in;
                    if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
                        let vp = ctx_local.viewport();
                        let next_state =
                            element.build_base_descendants_only(graph, arena, ctx_local, false);
                        UiBuildContext::from_parts(vp, next_state)
                    } else {
                        let vp = ctx_local.viewport();
                        let next_state = child.build(graph, arena, ctx_local);
                        UiBuildContext::from_parts(vp, next_state)
                    }
                });
                if let Some(c) = next_ctx {
                    ctx = c;
                }
            }
        }

        // End the parent's child clip scope (stencil + scissor + clip_id)
        // before rendering overflow children. Overflow children — Viewport-
        // and AnchorParent-clipped descendants — must paint outside the
        // immediate parent's inner clip, so the parent's stencil mask must
        // not be active when they build their render passes.
        self.end_child_clip_scope(graph, &mut ctx, child_clip_scope);

        if render_children_passes {
            for (idx, is_overflow) in overflow_child_indices.into_iter().enumerate() {
                if !is_overflow {
                    continue;
                }
                let Some(child_key) = child_keys.get(idx).copied() else {
                    continue;
                };
                let viewport = ctx.viewport();
                let taken_state = ctx.state_clone();
                let ctx_in = UiBuildContext::from_parts(viewport.clone(), taken_state);
                let next_ctx = arena.with_element_taken(child_key, |child, arena| {
                    let mut ctx_local = ctx_in;
                    if child.is_deferred_to_root_viewport_render() {
                        ctx_local.register_deferred(child_key, child.stable_id());
                        return ctx_local;
                    }
                    if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
                        let vp = ctx_local.viewport();
                        let next_state =
                            element.build_base_descendants_only(graph, arena, ctx_local, false);
                        UiBuildContext::from_parts(vp, next_state)
                    } else {
                        let vp = ctx_local.viewport();
                        let next_state = child.build(graph, arena, ctx_local);
                        UiBuildContext::from_parts(vp, next_state)
                    }
                });
                if let Some(c) = next_ctx {
                    ctx = c;
                }
            }
        }

        if let Some(mut texture) = owner_texture {
            // Preserve Legacy's native-after-children paint order, but emit
            // inside this owner's self clip and layer rather than after the
            // owner scope has been restored by build_base_only.
            let paint_offset = ctx.paint_offset();
            texture.params.bounds[0] += paint_offset[0];
            texture.params.bounds[1] += paint_offset[1];
            if force_self_opaque {
                texture.params.opacity = 1.0;
            }
            if let Some(target) = ctx.current_target() {
                graph.add_graphics_pass(crate::view::render_pass::TextureCompositePass::new(
                    texture.params,
                    crate::view::render_pass::TextureCompositeInput::from_sampled_texture(
                        texture.upload,
                        Default::default(),
                        ctx.graphics_pass_context(),
                    ),
                    crate::view::render_pass::TextureCompositeOutput {
                        render_target: target,
                    },
                ));
            }
        }

        if let Some(previous) = previous_scissor_rect {
            ctx.restore_scissor_rect(previous);
        }
        ctx.restore_owner_paint_offset_projection(parent_paint_projection);
        ctx.into_state()
    }

    fn measure_flex_children(
        &mut self,
        proposal: LayoutProposal,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let insets = resolve_layout_insets(
            &self.computed_style.border_widths,
            &self.computed_style.padding,
            proposal.percent_base_width,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );

        let sizes = self.resolve_layout_sizes(proposal);
        let measure_w = if self.computed_style.width == SizeValue::Auto
            && proposal.percent_base_width.is_some()
        {
            proposal.width.max(0.0)
        } else {
            sizes.axis_measure_constraint.width
        };
        let measure_h = if self.computed_style.height == SizeValue::Auto {
            proposal.height.max(0.0)
        } else {
            sizes.axis_measure_constraint.height
        };
        let inner_w = (measure_w - insets.horizontal()).max(0.0);
        let inner_h = (measure_h - insets.vertical()).max(0.0);

        let (child_available_width, child_available_height) = match self.scroll_direction {
            ScrollDirection::None => (inner_w, inner_h),
            ScrollDirection::Vertical => (inner_w, 1_000_000.0),
            ScrollDirection::Horizontal => (1_000_000.0, inner_h),
            ScrollDirection::Both => (1_000_000.0, 1_000_000.0),
        };

        let child_percent_base_width = if self.width_is_known(proposal) {
            Some(inner_w)
        } else {
            None
        };
        let child_percent_base_height = if self.height_is_known(proposal) {
            Some(inner_h)
        } else {
            None
        };

        let absolute_mask = self.compute_children_absolute_mask(arena);
        let is_row = matches!(
            self.computed_style.layout_axis_direction(),
            FlowDirection::Row
        );
        let is_real_flex = matches!(self.computed_style.layout, Layout::Flex { .. });
        let solver_wrap =
            !is_real_flex && matches!(self.computed_style.layout_flow_wrap(), FlowWrap::Wrap);
        let main_limit = if is_row { inner_w } else { inner_h };
        let solver_gap = resolve_px(
            self.computed_style.gap,
            if is_row { inner_w } else { inner_h },
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let outputs = crate::view::layout::measure::measure_axis(
            crate::view::layout::measure::MeasureAxisInputs {
                layout: self.computed_style.layout,
                children: &self.children,
                absolute_mask: &absolute_mask,
                is_row,
                is_real_flex,
                solver_wrap,
                solver_gap,
                main_limit,
                child_available_width,
                child_available_height,
                child_percent_base_width,
                child_percent_base_height,
                viewport_width: proposal.viewport_width,
                viewport_height: proposal.viewport_height,
            },
            arena,
        );

        if self.computed_style.width == SizeValue::Auto {
            let auto_width = if is_row {
                outputs.flex_info.total_main
            } else {
                outputs.flex_info.total_cross
            };
            self.core.set_width(auto_width + insets.horizontal());
        }
        if self.computed_style.height == SizeValue::Auto {
            let auto_height = if is_row {
                outputs.flex_info.total_cross
            } else {
                outputs.flex_info.total_main
            };
            self.core.set_height(auto_height + insets.vertical());
        }

        self.layout_state.content_size = outputs.content_size;
        self.flex_info = Some(outputs.flex_info);
    }

    fn build_render_pipeline(
        &mut self,
        graph: &mut FrameGraph,
        _arena: &mut crate::view::node_arena::NodeArena,
        mut ctx: UiBuildContext,
        force_opaque: bool,
    ) -> BuildState {
        if !self.core.should_paint {
            return ctx.into_state();
        }
        let opacity = if force_opaque { 1.0 } else { self.opacity };
        let shadow_state = self.render_box_shadows(
            graph,
            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
            opacity,
        );
        ctx.set_state(shadow_state);

        if self.inline_ifc_owned_by_root {
            return self.build_inline_ifc_draw_rect_package_render_pipeline(graph, ctx, opacity);
        }

        for op in self.self_decoration_paint_ops(opacity, ctx.paint_offset()) {
            let mut pass = DrawRectPass::new(
                op.params,
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            pass.set_render_mode(op.mode);
            ctx.emit_draw_rect_pass(graph, pass);
        }
        ctx.into_state()
    }

    pub(super) fn self_decoration_paint_ops(
        &self,
        opacity: f32,
        paint_offset: [f32; 2],
    ) -> SelfDecorationPaintOps {
        if !self.core.should_paint {
            return SelfDecorationPaintOps::empty();
        }
        let fill_color = self.background_color.as_ref().to_rgba_f32();
        let gradient_paint = self.computed_style.background_image.as_ref().map(|g| {
            resolve_gradient_paint(
                g,
                self.layout_state.layout_size.width.max(0.0),
                self.layout_state.layout_size.height.max(0.0),
            )
        });
        let border_gradient_paint = self.computed_style.border_image.as_ref().map(|g| {
            resolve_gradient_paint(
                g,
                self.layout_state.layout_size.width.max(0.0),
                self.layout_state.layout_size.height.max(0.0),
            )
        });
        let max_bw = (self
            .layout_state
            .layout_size
            .width
            .min(self.layout_state.layout_size.height))
            * 0.5;
        let left = self.border_widths.left.clamp(0.0, max_bw);
        let right = self.border_widths.right.clamp(0.0, max_bw);
        let top = self.border_widths.top.clamp(0.0, max_bw);
        let bottom = self.border_widths.bottom.clamp(0.0, max_bw);

        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.layout_state.layout_size.width.max(0.0),
            self.layout_state.layout_size.height.max(0.0),
        );
        let position = [
            self.layout_state.layout_position.x + paint_offset[0],
            self.layout_state.layout_position.y + paint_offset[1],
        ];
        let mut fill = RectPassParams {
            position,
            size: [
                self.layout_state.layout_size.width,
                self.layout_state.layout_size.height,
            ],
            fill_color,
            opacity,
            gradient: gradient_paint,
            ..Default::default()
        };
        fill.set_border_widths(left, right, top, bottom);
        fill.set_border_radii(outer_radii.to_array());
        let mut ops = SelfDecorationPaintOps::fill(crate::view::paint::DrawRectOp {
            params: fill,
            mode: RectRenderMode::FillOnly,
        });

        if left <= 0.0 && right <= 0.0 && top <= 0.0 && bottom <= 0.0 {
            return ops;
        }

        let mut border = RectPassParams {
            position,
            size: [
                self.layout_state.layout_size.width,
                self.layout_state.layout_size.height,
            ],
            fill_color: [0.0, 0.0, 0.0, 0.0],
            opacity,
            border_gradient: border_gradient_paint,
            ..Default::default()
        };
        border.set_border_side_colors(
            self.border_colors.left.as_ref().to_rgba_f32(),
            self.border_colors.right.as_ref().to_rgba_f32(),
            self.border_colors.top.as_ref().to_rgba_f32(),
            self.border_colors.bottom.as_ref().to_rgba_f32(),
        );
        border.set_border_widths(left, right, top, bottom);
        border.set_border_radii(outer_radii.to_array());
        ops.border = Some(crate::view::paint::DrawRectOp {
            params: border,
            mode: RectRenderMode::BorderOnly,
        });
        ops
    }

    #[allow(dead_code)] // Used by the staged artifact recorder before viewport authority rollout.
    pub(crate) fn record_safe_leaf_paint_artifact(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
    ) -> Result<crate::view::paint::PaintArtifact, crate::view::paint::LegacyPaintReason> {
        use crate::view::paint::{PaintArtifact, PaintChunk, PaintOp};

        let mut metadata =
            self.record_safe_leaf_paint_metadata(owner, properties, content_revision)?;

        // The root artifact path starts from the viewport paint origin. Match
        // Element's existing root-local pixel snap without mutating BuildState.
        let paint_offset = [
            round_layout_value(self.layout_state.layout_position.x)
                - self.layout_state.layout_position.x,
            round_layout_value(self.layout_state.layout_position.y)
                - self.layout_state.layout_position.y,
        ];
        let ops = self
            .self_decoration_paint_ops(self.opacity, paint_offset)
            .into_iter()
            .map(PaintOp::DrawRect)
            .collect::<Vec<_>>();
        metadata.payload_identity =
            crate::view::paint::PaintPayloadIdentity::prepared_shadows_with_decoration(
                std::iter::empty(),
                ops.iter().filter_map(|op| match op {
                    PaintOp::DrawRect(rect) => Some(rect),
                    _ => None,
                }),
            )
            .ok_or(crate::view::paint::LegacyPaintReason::StatefulPaint)?;
        let op_len = ops.len();
        Ok(PaintArtifact {
            target: Default::default(),
            chunks: vec![PaintChunk {
                id: metadata.id,
                owner: metadata.owner,
                op_range: 0..op_len,
                bounds: metadata.bounds,
                properties: metadata.properties,
                content_revision: metadata.content_revision,
                payload_identity: metadata.payload_identity,
            }],
            ops,
            clip_nodes: Vec::new(),
            effect_nodes: Vec::new(),
            transform_nodes: Vec::new(),
            layout_position_nodes: Vec::new(),
            visual_offset_nodes: Vec::new(),
            scroll_nodes: Vec::new(),
            owner_property_states: Vec::new(),
            owner_nodes: vec![crate::view::paint::PaintOwnerSnapshot {
                owner,
                parent: None,
            }],
        })
    }

    pub(super) fn record_shadow_node_paint_artifact(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &crate::view::node_arena::NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Result<crate::view::paint::PaintArtifact, crate::view::paint::LegacyPaintReason> {
        use crate::view::paint::{PaintArtifact, PaintChunk, PaintOp};
        let prepared = self.prepared_self_paint_record(owner, recording_context)?;
        let mut metadata = self.record_shadow_node_paint_metadata(
            owner,
            properties,
            content_revision,
            Some(arena),
            recording_context,
        )?;
        let mut ops = prepared
            .shadows
            .into_iter()
            .map(PaintOp::PreparedShadow)
            .collect::<Vec<_>>();
        ops.extend(prepared.decoration.into_iter().map(PaintOp::DrawRect));
        metadata.bounds = prepared.geometry.bounds;
        metadata.payload_identity = prepared.payload_identity;
        Ok(PaintArtifact {
            target: Default::default(),
            chunks: vec![PaintChunk {
                id: metadata.id,
                owner: metadata.owner,
                op_range: 0..ops.len(),
                bounds: metadata.bounds,
                properties: metadata.properties,
                content_revision: metadata.content_revision,
                payload_identity: metadata.payload_identity,
            }],
            ops,
            clip_nodes: Vec::new(),
            effect_nodes: Vec::new(),
            transform_nodes: Vec::new(),
            layout_position_nodes: Vec::new(),
            visual_offset_nodes: Vec::new(),
            scroll_nodes: Vec::new(),
            owner_property_states: Vec::new(),
            owner_nodes: vec![crate::view::paint::PaintOwnerSnapshot {
                owner,
                parent: None,
            }],
        })
    }

    #[allow(dead_code)] // Used by the staged artifact recorder before viewport authority rollout.
    pub(crate) fn record_safe_leaf_paint_metadata(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
    ) -> Result<crate::view::paint::PaintChunkMetadata, crate::view::paint::LegacyPaintReason> {
        use crate::view::paint::LegacyPaintReason;

        if !self.children.is_empty() {
            return Err(LegacyPaintReason::HasChildren);
        }
        if !self.box_shadows.is_empty() {
            return Err(LegacyPaintReason::BoxShadow);
        }
        self.record_shadow_node_paint_metadata(
            owner,
            properties,
            content_revision,
            None,
            crate::view::paint::PaintRecordingContext::default(),
        )
    }

    fn recording_context_authorizes_exact_self_clip(
        &self,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> bool {
        (self.anchor_parent_leaf_self_clip_scissor_rect().is_some()
            && recording_context.authorizes_self_clip_for(self.stable_id()))
            || self.absolute_clip_scissor_rect().is_some_and(|scissor| {
                recording_context.authorizes_subtree_self_clip_for(self.stable_id(), scissor)
            })
            || recording_context.authorizes_deferred_viewport_self_clip_for(self.stable_id())
    }

    pub(super) fn record_shadow_node_paint_metadata(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: Option<&crate::view::node_arena::NodeArena>,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Result<crate::view::paint::PaintChunkMetadata, crate::view::paint::LegacyPaintReason> {
        use crate::view::paint::{
            LegacyPaintReason, PaintChunkId, PaintChunkMetadata, PaintChunkRole,
        };
        if self.resolved_transform.is_some()
            && !recording_context.authorizes_transform_surface_root(self.stable_id())
        {
            return Err(LegacyPaintReason::Transform);
        }
        if self.inline_ifc_owned_by_root {
            return Err(LegacyPaintReason::InlineIfc);
        }
        if self.is_owning_inline_ifc_root_role()
            && arena.is_none_or(|arena| self.owning_inline_ifc_root_paint_witness(arena).is_err())
        {
            return Err(LegacyPaintReason::MissingPreparedInlineRoot);
        }
        if self.scroll_direction != ScrollDirection::None
            && !(recording_context.authorizes_baked_scroll_host_root(self.stable_id())
                || recording_context.authorizes_generic_scroll_host_root(self.stable_id()))
        {
            return Err(LegacyPaintReason::ScrollContainer);
        }
        if self.absolute_clip_scissor_rect().is_some() {
            if !self.recording_context_authorizes_exact_self_clip(recording_context) {
                return Err(LegacyPaintReason::SelfClip);
            }
        }
        if self.should_append_to_root_viewport_render()
            && !recording_context.authorizes_deferred_viewport_self_clip_for(self.stable_id())
        {
            return Err(LegacyPaintReason::Deferred);
        }
        // Layout-transition tracks install their sampled position and size in
        // `layout_state` before paint. The retained payload below consumes
        // that same frozen frame geometry; any clip topology still passes
        // through the ordinary SelfClip / ChildClip authority gates.
        if arena.is_some_and(|arena| self.requires_child_mask_surface(arena))
            && !recording_context.authorizes_baked_scroll_host_root(self.stable_id())
            && !recording_context.authorizes_generic_scroll_host_root(self.stable_id())
            && !recording_context.authorizes_descendant_contents_clip(self.stable_id())
            && arena.is_none_or(|arena| {
                self.prepared_retained_child_mask_plan(arena, recording_context)
                    .is_none()
            })
        {
            return Err(LegacyPaintReason::ChildClip);
        }
        if !self.layout_state.should_render {
            return Err(LegacyPaintReason::StatefulPaint);
        }

        let prepared = self.prepared_self_paint_record(owner, recording_context)?;

        Ok(PaintChunkMetadata {
            id: PaintChunkId {
                owner,
                scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                slot: 0,
                role: PaintChunkRole::SelfDecoration,
            },
            owner,
            bounds: prepared.geometry.bounds,
            properties,
            content_revision,
            payload_identity: prepared.payload_identity,
        })
    }

    fn prepared_self_paint_record(
        &self,
        owner: crate::view::node_arena::NodeKey,
        context: crate::view::paint::PaintRecordingContext,
    ) -> Result<PreparedSelfPaintRecord, crate::view::paint::LegacyPaintReason> {
        use crate::view::paint::LegacyPaintReason;

        let geometry = self.self_paint_recording_geometry(owner, context);
        let shadows = self
            .prepared_outer_shadow_ops(geometry.context)
            .ok_or(LegacyPaintReason::BoxShadow)?;
        let decoration = self
            .self_decoration_paint_ops(
                geometry.context.paint_opacity(self.opacity),
                geometry.context.paint_offset,
            )
            .into_iter()
            .collect::<Vec<_>>();
        let payload_identity =
            crate::view::paint::PaintPayloadIdentity::prepared_shadows_with_decoration(
                shadows.iter(),
                decoration.iter(),
            )
            .ok_or(LegacyPaintReason::StatefulPaint)?;
        Ok(PreparedSelfPaintRecord {
            geometry,
            shadows,
            decoration,
            payload_identity,
        })
    }

    /// Produces the one geometry input shared by metadata-only and full
    /// artifact recording. A detached scroll-content target is the sole case
    /// where the chunk bounds consume the recorder's full two-dimensional
    /// normalization offset; every prepared payload receives that same
    /// context, so the offset cannot be applied independently or twice.
    fn self_paint_recording_geometry(
        &self,
        owner: crate::view::node_arena::NodeKey,
        context: crate::view::paint::PaintRecordingContext,
    ) -> SelfPaintRecordingGeometry {
        let bounds_offset = if context.authorizes_scroll_content_local_owner(owner) {
            context.paint_offset
        } else {
            [0.0, 0.0]
        };
        SelfPaintRecordingGeometry {
            bounds: Rect {
                x: self.layout_state.layout_position.x + bounds_offset[0],
                y: self.layout_state.layout_position.y + bounds_offset[1],
                width: self.layout_state.layout_size.width.max(0.0),
                height: self.layout_state.layout_size.height.max(0.0),
            },
            context,
        }
    }

    pub(super) fn shadow_paint_blocker(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        deferred_phase_root: bool,
        authoritative_self_clip: bool,
        allow_outer_shadow_artifact: bool,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::base_component::element::ShadowPaintBlocker> {
        use crate::view::base_component::element::ShadowPaintBlocker;
        if !self.layout_state.should_render {
            return Some(ShadowPaintBlocker::StatefulPaint);
        }
        if self.resolved_transform.is_some()
            && !recording_context.authorizes_transform_surface_root(self.stable_id())
        {
            return Some(ShadowPaintBlocker::Transform);
        }
        let deferred_authorized = match recording_context.opacity_authority {
            crate::view::paint::PaintOpacityAuthority::NeutralRootEffect(effect) => {
                recording_context.authorizes_deferred_viewport_effect_for(self.stable_id(), effect)
            }
            crate::view::paint::PaintOpacityAuthority::Baked => {
                recording_context.authorizes_deferred_viewport_self_clip_for(self.stable_id())
            }
        };
        if self.should_append_to_root_viewport_render()
            && (!deferred_phase_root || !deferred_authorized)
        {
            return Some(ShadowPaintBlocker::Deferred);
        }
        if !self.box_shadows.is_empty()
            && (!allow_outer_shadow_artifact
                || self.prepared_outer_shadow_ops(recording_context).is_none())
        {
            return Some(ShadowPaintBlocker::BoxShadow);
        }
        if self.inline_ifc_owned_by_root {
            return Some(ShadowPaintBlocker::InlineIfc);
        }
        if self.is_owning_inline_ifc_root_role()
            && self.owning_inline_ifc_root_paint_witness(arena).is_err()
        {
            return Some(ShadowPaintBlocker::MissingPreparedInlineRoot);
        }
        if self.scroll_direction != ScrollDirection::None
            && !(recording_context.authorizes_baked_scroll_host_root(self.stable_id())
                || recording_context.authorizes_generic_scroll_host_root(self.stable_id()))
        {
            return Some(ShadowPaintBlocker::ScrollContainer);
        }
        if self.absolute_clip_scissor_rect().is_some()
            && (!authoritative_self_clip
                || !self.recording_context_authorizes_exact_self_clip(recording_context))
        {
            return Some(ShadowPaintBlocker::SelfClip);
        }
        // Active layout transitions are paintable once their sampled frame is
        // installed in `layout_state`. Retained recording reads exactly that
        // geometry; clip-dependent cases remain guarded below.
        if !self.children.is_empty() {
            let overflow_child_indices = (0..self.children.len())
                .map(|index| self.child_renders_outside_inner_clip(index, arena))
                .collect::<Vec<_>>();
            let outer_radii = normalize_corner_radii(
                self.border_radii,
                self.layout_state.layout_size.width.max(0.0),
                self.layout_state.layout_size.height.max(0.0),
            );
            let inner_radii = self.inner_clip_radii(outer_radii);
            if self.should_clip_children(&overflow_child_indices, inner_radii, arena)
                && !recording_context.authorizes_baked_scroll_host_root(self.stable_id())
                && !recording_context.authorizes_generic_scroll_host_root(self.stable_id())
                && !recording_context.authorizes_descendant_contents_clip(self.stable_id())
                && self
                    .prepared_retained_child_mask_plan(arena, recording_context)
                    .is_none()
            {
                return Some(ShadowPaintBlocker::ChildClip);
            }
        }
        None
    }

    pub(super) fn prepared_outer_shadow_ops(
        &self,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<Vec<crate::view::paint::PreparedShadowOp>> {
        if !self.core.should_paint || self.box_shadows.is_empty() {
            return Some(Vec::new());
        }
        let fragment_rects =
            if self.is_fragmentable_inline_element() && !self.inline_paint_fragments.is_empty() {
                self.inline_paint_fragments.clone()
            } else {
                vec![Rect {
                    x: self.layout_state.layout_position.x,
                    y: self.layout_state.layout_position.y,
                    width: self.layout_state.layout_size.width,
                    height: self.layout_state.layout_size.height,
                }]
            };
        if !recording_context
            .paint_offset
            .iter()
            .all(|value| value.is_finite())
            || fragment_rects.iter().any(|fragment| {
                [fragment.x, fragment.y, fragment.width, fragment.height]
                    .iter()
                    .any(|value| !value.is_finite())
                    || fragment.width <= 0.0
                    || fragment.height <= 0.0
            })
        {
            return None;
        }
        let opacity = recording_context.paint_opacity(self.opacity);
        let mut prepared = Vec::with_capacity(fragment_rects.len() * self.box_shadows.len());
        for fragment in fragment_rects {
            let outer_radii =
                normalize_corner_radii(self.border_radii, fragment.width, fragment.height);
            for shadow in &self.box_shadows {
                if !shadow.offset_x.is_finite()
                    || !shadow.offset_y.is_finite()
                    || !shadow.blur.is_finite()
                    || !shadow.spread.is_finite()
                {
                    return None;
                }
                let color = shadow.color.to_rgba_f32();
                if color
                    .iter()
                    .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
                {
                    return None;
                }
                let spread = shadow.spread;
                let shadow_radii = expand_corner_radii_for_spread(
                    outer_radii,
                    spread,
                    fragment.width,
                    fragment.height,
                );
                let mesh = ShadowMesh::rounded_rect_with_radii(
                    fragment.x - spread + recording_context.paint_offset[0],
                    fragment.y - spread + recording_context.paint_offset[1],
                    fragment.width + spread * 2.0,
                    fragment.height + spread * 2.0,
                    shadow_radii.to_array(),
                );
                prepared.push(crate::view::paint::PreparedShadowOp::new(
                    mesh,
                    ShadowParams {
                        offset_x: shadow.offset_x,
                        offset_y: shadow.offset_y,
                        blur_radius: shadow.blur.max(0.0),
                        color,
                        opacity,
                        spread: 0.0,
                        clip_to_geometry: shadow.inset,
                    },
                )?);
            }
        }
        Some(prepared)
    }

    fn install_inline_ifc_rollout_packages_from_candidate(
        &mut self,
        packages: Option<&InlineIfcDistributedElementPackages>,
    ) {
        self.inline_ifc_rollout_packages = packages
            .map(ElementInlineIfcRolloutPackages::from_inline_ifc_distributed)
            .unwrap_or_default();
    }

    fn inline_ifc_atomic_placement_metadata(
        &self,
    ) -> Option<ElementInlineIfcAtomicPlacementMetadata> {
        self.inline_ifc_rollout_packages
            .atomic_placement
            .as_ref()
            .filter(|package| !package.placements.is_empty())
            .cloned()
            .map(|package| ElementInlineIfcAtomicPlacementMetadata { package })
    }
    fn inline_ifc_fragment_draw_rect_pass_metadata(
        &self,
        fragment: &crate::view::inline_formatting_context::InlineIfcElementDecorationDrawRect,
        paint_offset: [f32; 2],
    ) -> ElementInlineIfcDrawRectPassMetadata {
        let metadata = fragment.metadata;
        let max_bw = metadata.size[0].min(metadata.size[1]) * 0.5;
        let left = if fragment.is_first_for_source {
            metadata.border_widths[0].clamp(0.0, max_bw)
        } else {
            0.0
        };
        let right = if fragment.is_last_for_source {
            metadata.border_widths[1].clamp(0.0, max_bw)
        } else {
            0.0
        };
        let top = metadata.border_widths[2].clamp(0.0, max_bw);
        let bottom = metadata.border_widths[3].clamp(0.0, max_bw);
        let mut fragment_radii = self.border_radii;
        if !fragment.is_first_for_source {
            fragment_radii.top_left = 0.0;
            fragment_radii.bottom_left = 0.0;
        }
        if !fragment.is_last_for_source {
            fragment_radii.top_right = 0.0;
            fragment_radii.bottom_right = 0.0;
        }
        let outer_radii =
            normalize_corner_radii(fragment_radii, metadata.size[0], metadata.size[1]);
        let mut fill = {
            let mut params = RectPassParams {
                position: [
                    metadata.position[0] + paint_offset[0],
                    metadata.position[1] + paint_offset[1],
                ],
                size: metadata.size,
                fill_color: metadata.fill_color,
                opacity: metadata.opacity,
                ..Default::default()
            };
            params.set_border_widths(left, right, top, bottom);
            params.set_border_radii(outer_radii.to_array());
            params
        };
        let border = if left <= 0.0 && right <= 0.0 && top <= 0.0 && bottom <= 0.0 {
            None
        } else {
            let mut params = RectPassParams {
                position: [
                    metadata.position[0] + paint_offset[0],
                    metadata.position[1] + paint_offset[1],
                ],
                size: metadata.size,
                fill_color: [0.0, 0.0, 0.0, 0.0],
                opacity: metadata.opacity,
                border_color: metadata.border_colors[0],
                ..Default::default()
            };
            params.set_border_side_colors(
                metadata.border_colors[0],
                metadata.border_colors[1],
                metadata.border_colors[2],
                metadata.border_colors[3],
            );
            params.set_border_widths(left, right, top, bottom);
            params.set_border_radii(outer_radii.to_array());
            Some(params)
        };
        if let Some(border) = &border {
            fill.border_color = border.border_color;
            fill.border_side_colors = border.border_side_colors;
            fill.use_border_side_colors = border.use_border_side_colors;
        }
        ElementInlineIfcDrawRectPassMetadata { fill, border }
    }

    pub(super) fn prepared_inline_ifc_decoration_payload(
        &self,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Result<PreparedElementInlineIfcDecorationPayload, ShadowPaintBlocker> {
        if recording_context.inside_text_area {
            return Err(ShadowPaintBlocker::TextAreaSelection);
        }
        if !self.inline_ifc_owned_by_root
            || self.inline_ifc_rollout_packages.atomic_placement.is_some()
        {
            return Err(ShadowPaintBlocker::InlineIfc);
        }
        if !recording_context
            .paint_offset
            .iter()
            .all(|value| value.is_finite())
        {
            return Err(ShadowPaintBlocker::MissingPreparedInlineDecoration);
        }

        let shell_bounds = Rect {
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self.layout_state.layout_size.width.max(0.0),
            height: self.layout_state.layout_size.height.max(0.0),
        };
        if !self.core.should_paint {
            if [
                shell_bounds.x,
                shell_bounds.y,
                shell_bounds.width,
                shell_bounds.height,
            ]
            .iter()
            .any(|value| !value.is_finite())
            {
                return Err(ShadowPaintBlocker::MissingPreparedInlineDecoration);
            }
            return Ok(PreparedElementInlineIfcDecorationPayload {
                bounds: shell_bounds,
                shadows: Vec::new(),
                ops: Vec::new(),
            });
        }

        let shadows = self
            .prepared_outer_shadow_ops(recording_context)
            .ok_or(ShadowPaintBlocker::BoxShadow)?;

        let package = self
            .inline_ifc_rollout_packages
            .decoration_draw_rect
            .as_ref()
            .ok_or(ShadowPaintBlocker::MissingPreparedInlineDecoration)?;
        let stable_id = self.stable_id();
        if stable_id == 0 || package.source.0 != stable_id {
            return Err(ShadowPaintBlocker::MissingPreparedInlineDecoration);
        }
        let expected_source = self.inline_ifc_decoration_package_source(package.source);
        let expected_style = expected_source.draw_rect_style;
        let expected_insets = [
            expected_source.slice_insets.left,
            expected_source.slice_insets.right,
            expected_source.slice_insets.top,
            expected_source.slice_insets.bottom,
        ];
        if package.fragments.is_empty()
            || package.fragments.len() != self.inline_paint_fragments.len()
            || package.style_key != expected_style.style_key
            || [
                package.slice_insets.left,
                package.slice_insets.right,
                package.slice_insets.top,
                package.slice_insets.bottom,
            ]
            .map(f32::to_bits)
                != expected_insets.map(f32::to_bits)
            || [
                package.slice_insets.left,
                package.slice_insets.right,
                package.slice_insets.top,
                package.slice_insets.bottom,
            ]
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(ShadowPaintBlocker::MissingPreparedInlineDecoration);
        }

        let package_insets = [
            package.slice_insets.left,
            package.slice_insets.right,
            package.slice_insets.top,
            package.slice_insets.bottom,
        ];
        let mut previous_order = None;
        let mut ops = Vec::with_capacity(package.fragments.len());
        let mut left = f32::INFINITY;
        let mut top = f32::INFINITY;
        let mut right = f32::NEG_INFINITY;
        let mut bottom = f32::NEG_INFINITY;
        let last_index = package.fragments.len() - 1;
        for (index, fragment) in package.fragments.iter().enumerate() {
            let metadata = fragment.metadata;
            let installed_rect = self.inline_paint_fragments[index];
            let order = (
                fragment.line_index,
                fragment.range.start,
                fragment.range.end,
            );
            let fragment_insets = [
                fragment.slice_insets.left,
                fragment.slice_insets.right,
                fragment.slice_insets.top,
                fragment.slice_insets.bottom,
            ];
            let package_matches = fragment.source == package.source
                && fragment.style_key == package.style_key
                && fragment_insets.map(f32::to_bits) == package_insets.map(f32::to_bits)
                && fragment.range.start < fragment.range.end
                && previous_order.is_none_or(|previous| previous < order)
                && fragment.is_first_for_source == (index == 0)
                && fragment.is_last_for_source == (index == last_index)
                && installed_rect.x.to_bits() == metadata.position[0].to_bits()
                && installed_rect.y.to_bits() == metadata.position[1].to_bits()
                && installed_rect.width.to_bits() == metadata.size[0].to_bits()
                && installed_rect.height.to_bits() == metadata.size[1].to_bits()
                && fragment.rect.width.to_bits() == metadata.size[0].to_bits()
                && fragment.rect.height.to_bits() == metadata.size[1].to_bits()
                && metadata.fill_color.map(f32::to_bits)
                    == expected_style.fill_color.map(f32::to_bits)
                && metadata.opacity.to_bits() == expected_style.opacity.to_bits()
                && metadata.border_widths.map(f32::to_bits)
                    == expected_style.border_widths.map(f32::to_bits)
                && metadata.border_colors.map(|color| color.map(f32::to_bits))
                    == expected_style
                        .border_colors
                        .map(|color| color.map(f32::to_bits));
            let colors_are_valid = metadata
                .fill_color
                .iter()
                .chain(metadata.border_colors.iter().flatten())
                .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel));
            if !package_matches
                || metadata.position.iter().any(|value| !value.is_finite())
                || metadata
                    .size
                    .iter()
                    .any(|value| !value.is_finite() || *value <= 0.0)
                || [
                    fragment.rect.x,
                    fragment.rect.y,
                    fragment.rect.width,
                    fragment.rect.height,
                ]
                .iter()
                .any(|value| !value.is_finite())
                || !metadata.opacity.is_finite()
                || !(0.0..=1.0).contains(&metadata.opacity)
                || metadata
                    .border_widths
                    .iter()
                    .any(|value| !value.is_finite() || *value < 0.0)
                || !colors_are_valid
            {
                return Err(ShadowPaintBlocker::MissingPreparedInlineDecoration);
            }
            previous_order = Some(order);

            let mut prepared = self.inline_ifc_fragment_draw_rect_pass_metadata(
                fragment,
                recording_context.paint_offset,
            );
            let opacity = recording_context.paint_opacity(metadata.opacity);
            prepared.fill.opacity = opacity;
            if let Some(border) = &mut prepared.border {
                border.opacity = opacity;
            }
            let descriptor = crate::view::paint::PreparedInlineIfcDecorationDescriptor {
                source: fragment.source.0,
                line_index: fragment.line_index,
                range: fragment.range.clone(),
                style_key: fragment.style_key.brush,
                slice_insets: fragment_insets,
                is_first_for_source: fragment.is_first_for_source,
                is_last_for_source: fragment.is_last_for_source,
            };
            let op = crate::view::paint::PreparedInlineIfcDecorationOp::new(
                descriptor,
                prepared.fill,
                prepared.border,
            )
            .ok_or(ShadowPaintBlocker::MissingPreparedInlineDecoration)?;
            left = left.min(op.fill.position[0]);
            top = top.min(op.fill.position[1]);
            right = right.max(op.fill.position[0] + op.fill.size[0]);
            bottom = bottom.max(op.fill.position[1] + op.fill.size[1]);
            ops.push(op);
        }

        let width = right - left;
        let height = bottom - top;
        if [left, top, right, bottom, width, height]
            .iter()
            .any(|value| !value.is_finite())
            || width <= 0.0
            || height <= 0.0
        {
            return Err(ShadowPaintBlocker::MissingPreparedInlineDecoration);
        }
        Ok(PreparedElementInlineIfcDecorationPayload {
            bounds: Rect {
                x: left,
                y: top,
                width,
                height,
            },
            shadows,
            ops,
        })
    }

    pub(super) fn inline_ifc_owned_shadow_paint_blocker(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        _deferred_phase_root: bool,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<ShadowPaintBlocker> {
        if !self.layout_state.should_render {
            return Some(ShadowPaintBlocker::StatefulPaint);
        }
        if self.resolved_transform.is_some() {
            return Some(ShadowPaintBlocker::Transform);
        }
        if self.should_append_to_root_viewport_render() {
            return Some(ShadowPaintBlocker::Deferred);
        }
        if self.scroll_direction != ScrollDirection::None {
            return Some(ShadowPaintBlocker::ScrollContainer);
        }
        if self.absolute_clip_scissor_rect().is_some() {
            return Some(ShadowPaintBlocker::SelfClip);
        }
        // Inline layout installs the sampled fragment rectangles and the
        // matching decoration package before paint. Retained recording
        // validates those two frozen inputs against each other below, so an
        // active size track is not itself a paint blocker. A stale or partial
        // package still fails closed as MissingPreparedInlineDecoration.
        if self.requires_child_mask_surface(arena) {
            return Some(ShadowPaintBlocker::ChildClip);
        }
        self.prepared_inline_ifc_decoration_payload(recording_context)
            .err()
    }

    fn build_inline_ifc_draw_rect_package_render_pipeline(
        &mut self,
        graph: &mut FrameGraph,
        mut ctx: UiBuildContext,
        _opacity: f32,
    ) -> BuildState {
        let _atomic_placement_count = self
            .inline_ifc_atomic_placement_metadata()
            .map(|metadata| metadata.package.placements.len())
            .unwrap_or(0);
        let Some(package) = self
            .inline_ifc_rollout_packages
            .decoration_draw_rect
            .as_ref()
            .cloned()
        else {
            return ctx.into_state();
        };
        for fragment in package.fragments {
            let pass_metadata =
                self.inline_ifc_fragment_draw_rect_pass_metadata(&fragment, ctx.paint_offset());
            let mut fill_pass = DrawRectPass::new(
                pass_metadata.fill,
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            fill_pass.set_render_mode(RectRenderMode::FillOnly);
            self.push_rect_pass_auto(graph, &mut ctx, fill_pass);

            let Some(border_params) = pass_metadata.border else {
                continue;
            };
            let mut border_pass = DrawRectPass::new(
                border_params,
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            border_pass.set_render_mode(RectRenderMode::BorderOnly);
            self.push_rect_pass_auto(graph, &mut ctx, border_pass);
        }
        ctx.into_state()
    }

    fn push_stencil_pass<P: GraphicsPass + DrawRectIoPass + 'static>(
        &mut self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        pass: P,
    ) {
        emit_draw_rect_io_pass(graph, ctx, pass);
    }

    fn push_rect_pass_auto(
        &mut self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        pass: DrawRectPass,
    ) {
        ctx.emit_draw_rect_pass(graph, pass);
    }

    fn sync_props_from_computed_style(&mut self) {
        self.background_color = Box::new(self.computed_style.background_color);
        self.foreground_color = self.computed_style.color;
        self.box_shadows = self.computed_style.box_shadow.clone();
        self.transform = self.computed_style.transform.clone();
        self.transform_origin = self.computed_style.transform_origin;
        self.border_colors.left = Box::new(self.computed_style.border_colors.left);
        self.border_colors.right = Box::new(self.computed_style.border_colors.right);
        self.border_colors.top = Box::new(self.computed_style.border_colors.top);
        self.border_colors.bottom = Box::new(self.computed_style.border_colors.bottom);
        self.border_widths.left = resolve_px(
            self.computed_style.border_widths.left,
            self.core.size.width,
            0.0,
            0.0,
        );
        self.border_widths.right = resolve_px(
            self.computed_style.border_widths.right,
            self.core.size.width,
            0.0,
            0.0,
        );
        self.border_widths.top = resolve_px(
            self.computed_style.border_widths.top,
            self.core.size.height,
            0.0,
            0.0,
        );
        self.border_widths.bottom = resolve_px(
            self.computed_style.border_widths.bottom,
            self.core.size.height,
            0.0,
            0.0,
        );
        let radius_base = self.core.size.width.min(self.core.size.height).max(0.0);
        self.border_radii = CornerRadii {
            top_left: resolve_px(
                self.computed_style.border_radii.top_left,
                radius_base,
                0.0,
                0.0,
            ),
            top_right: resolve_px(
                self.computed_style.border_radii.top_right,
                radius_base,
                0.0,
                0.0,
            ),
            bottom_right: resolve_px(
                self.computed_style.border_radii.bottom_right,
                radius_base,
                0.0,
                0.0,
            ),
            bottom_left: resolve_px(
                self.computed_style.border_radii.bottom_left,
                radius_base,
                0.0,
                0.0,
            ),
        };
        self.border_radius = self.border_radii.max();
        self.opacity = self.computed_style.opacity.clamp(0.0, 1.0);
        self.update_resolved_transform();
        self.scroll_direction = self.computed_style.scroll_direction;
        self.padding.left = resolve_px(
            self.computed_style.padding.left,
            self.core.size.width,
            0.0,
            0.0,
        );
        self.padding.right = resolve_px(
            self.computed_style.padding.right,
            self.core.size.width,
            0.0,
            0.0,
        );
        self.padding.top = resolve_px(
            self.computed_style.padding.top,
            self.core.size.height,
            0.0,
            0.0,
        );
        self.padding.bottom = resolve_px(
            self.computed_style.padding.bottom,
            self.core.size.height,
            0.0,
            0.0,
        );
    }

    fn update_resolved_transform(&mut self) {
        self.resolved_transform = self.compute_transform_matrix();
        self.resolved_inverse_transform = self.resolved_transform.and_then(|matrix| {
            let det = matrix.determinant();
            if det.abs() <= 0.000_001 {
                None
            } else {
                Some(matrix.inverse())
            }
        });
    }

    fn compute_local_transform_payload(&self) -> Option<(Mat4, Vec3)> {
        if self.transform.as_slice().is_empty() {
            return None;
        }
        let size = self.layout_state.layout_size;
        let origin = Vec3::new(
            resolve_signed_px_with_base(
                self.transform_origin.x(),
                Some(size.width.max(0.0)),
                0.0,
                0.0,
            )
            .unwrap_or(0.0),
            resolve_signed_px_with_base(
                self.transform_origin.y(),
                Some(size.height.max(0.0)),
                0.0,
                0.0,
            )
            .unwrap_or(0.0),
            self.transform_origin.z(),
        );
        let mut transform = Mat4::IDENTITY;
        for entry in self.transform.as_slice() {
            let next = match entry.kind() {
                TransformKind::Translate { x, y, z } => Mat4::from_translation(Vec3::new(
                    resolve_signed_px_with_base(x, Some(size.width.max(0.0)), 0.0, 0.0)
                        .unwrap_or(0.0),
                    resolve_signed_px_with_base(y, Some(size.height.max(0.0)), 0.0, 0.0)
                        .unwrap_or(0.0),
                    z,
                )),
                TransformKind::Scale { x, y, z } => Mat4::from_scale(Vec3::new(x, y, z)),
                TransformKind::Rotate { x, y, z } => {
                    Mat4::from_rotation_x(x.to_radians())
                        * Mat4::from_rotation_y(y.to_radians())
                        * Mat4::from_rotation_z(z.to_radians())
                }
                TransformKind::Perspective { depth } => css_perspective_matrix(depth.max(0.0001)),
                TransformKind::Matrix { matrix } => Mat4::from_cols_array(&matrix),
            };
            transform *= next;
        }
        Some((transform, origin))
    }

    fn compute_transform_matrix(&self) -> Option<Mat4> {
        let (transform, origin) = self.compute_local_transform_payload()?;
        let origin_world = Vec3::new(
            self.layout_state.layout_position.x + origin.x,
            self.layout_state.layout_position.y + origin.y,
            origin.z,
        );
        Some(compose_transform_about_origin(transform, origin_world))
    }

    fn render_box_shadows(
        &mut self,
        graph: &mut FrameGraph,
        mut ctx: UiBuildContext,
        opacity: f32,
    ) -> BuildState {
        if self.box_shadows.is_empty() {
            return ctx.into_state();
        }
        let fragment_rects =
            if self.is_fragmentable_inline_element() && !self.inline_paint_fragments.is_empty() {
                self.inline_paint_fragments.clone()
            } else {
                vec![Rect {
                    x: self.layout_state.layout_position.x,
                    y: self.layout_state.layout_position.y,
                    width: self.layout_state.layout_size.width.max(0.0),
                    height: self.layout_state.layout_size.height.max(0.0),
                }]
            };
        let shadows = self.box_shadows.clone();
        for fragment in fragment_rects {
            if fragment.width <= 0.0 || fragment.height <= 0.0 {
                continue;
            }
            let outer_radii =
                normalize_corner_radii(self.border_radii, fragment.width, fragment.height);
            for shadow in shadows.iter().cloned() {
                let spread = shadow.spread;
                let shadow_radii = expand_corner_radii_for_spread(
                    outer_radii,
                    spread,
                    fragment.width,
                    fragment.height,
                );
                let [shadow_x, shadow_y] =
                    ctx.paint_point(fragment.x - spread, fragment.y - spread);
                let mesh = ShadowMesh::rounded_rect_with_radii(
                    shadow_x,
                    shadow_y,
                    fragment.width + spread * 2.0,
                    fragment.height + spread * 2.0,
                    shadow_radii.to_array(),
                );
                let params = ShadowParams {
                    offset_x: shadow.offset_x,
                    offset_y: shadow.offset_y,
                    blur_radius: shadow.blur.max(0.0),
                    color: shadow.color.to_rgba_f32(),
                    opacity: opacity.clamp(0.0, 1.0),
                    spread: 0.0,
                    clip_to_geometry: shadow.inset,
                };
                let next_state = self.push_shadow_pass(
                    mesh,
                    params,
                    graph,
                    UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
                );
                ctx.set_state(next_state);
            }
        }
        ctx.into_state()
    }

    fn push_shadow_pass(
        &mut self,
        mesh: ShadowMesh,
        params: ShadowParams,
        graph: &mut FrameGraph,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        self.ensure_current_render_target(graph, &mut ctx);
        let output = ctx
            .current_target()
            .unwrap_or_else(|| ctx.allocate_target(graph));
        ctx.set_current_target(output);
        let built = build_shadow_module(
            graph,
            ShadowModuleSpec {
                mesh,
                params,
                viewport_width: ctx.viewport.target_width,
                viewport_height: ctx.viewport.target_height,
                scale_factor: ctx.viewport.scale_factor,
                pass_context: ctx.graphics_pass_context(),
                output,
            },
        );
        if built {
            ctx.set_current_target(output);
        }
        ctx.into_state()
    }

    fn ensure_current_render_target(
        &self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
    ) -> RenderTargetOut {
        if let Some(target) = ctx.current_target() {
            return target;
        }
        let target = ctx.allocate_target(graph);
        ctx.set_current_target(target);
        target
    }

    fn paint_snapped_own_composite_bounds(
        &self,
        bounds: crate::view::base_component::RetainedSurfaceBounds,
        paint_offset: [f32; 2],
    ) -> crate::view::base_component::RetainedSurfaceBounds {
        crate::view::viewport::scene_helpers::paint_snapped_retained_surface_bounds(
            self,
            bounds,
            paint_offset,
        )
    }

    pub(crate) fn transform_surface_geometry_snapshot(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        paint_offset: [f32; 2],
        outer_scissor_rect: Option<[u32; 4]>,
    ) -> Option<TransformSurfaceGeometrySnapshot> {
        self.transform_surface_geometry_snapshot_with_placement(
            arena,
            paint_offset,
            paint_offset,
            outer_scissor_rect,
        )
    }

    fn transform_surface_geometry_snapshot_with_placement(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        raster_paint_offset: [f32; 2],
        composite_paint_offset: [f32; 2],
        outer_scissor_rect: Option<[u32; 4]>,
    ) -> Option<TransformSurfaceGeometrySnapshot> {
        let viewport_transform = self.resolved_transform?;
        let source_bounds = self.legacy_transform_surface_bounds(arena, raster_paint_offset)?;
        let visual_bounds =
            self.paint_snapped_own_composite_bounds(source_bounds, composite_paint_offset);
        TransformSurfaceGeometrySnapshot::new(
            source_bounds,
            visual_bounds,
            viewport_transform,
            outer_scissor_rect,
        )
    }

    pub(crate) fn exact_transform_surface_geometry_snapshot(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        paint_offset: [f32; 2],
        outer_scissor_rect: Option<[u32; 4]>,
    ) -> Option<TransformSurfaceGeometrySnapshot> {
        let viewport_transform = self.resolved_transform?;
        let source_bounds = self.retained_transform_surface_bounds(arena, paint_offset)?;
        let visual_bounds = self.paint_snapped_own_composite_bounds(source_bounds, paint_offset);
        TransformSurfaceGeometrySnapshot::new(
            source_bounds,
            visual_bounds,
            viewport_transform,
            outer_scissor_rect,
        )
    }

    /// Builds the same exact transform composite geometry from a
    /// compiler-proven pre-transform raster union. Mixed property receivers
    /// use this after excluding detached boundary subtrees from their normal
    /// descendant bounds walk.
    pub(crate) fn exact_transform_receiver_geometry_snapshot_for_raster_bounds(
        &self,
        raster_bounds: crate::view::base_component::RetainedSurfaceBounds,
        paint_offset: [f32; 2],
        outer_scissor_rect: Option<[u32; 4]>,
    ) -> Option<TransformSurfaceGeometrySnapshot> {
        let viewport_transform = self.resolved_transform?;
        let visual_bounds = self.paint_snapped_own_composite_bounds(raster_bounds, paint_offset);
        TransformSurfaceGeometrySnapshot::new(
            raster_bounds,
            visual_bounds,
            viewport_transform,
            outer_scissor_rect,
        )
    }

    /// Counterpart for a compiler-proven artifact union whose coordinates
    /// already include the recorder's exact paint snap.  Applying the live
    /// Element snap again would cancel a later scroll projection for S->T.
    pub(crate) fn exact_transform_receiver_geometry_snapshot_for_presnapped_raster_bounds(
        &self,
        raster_bounds: crate::view::base_component::RetainedSurfaceBounds,
        outer_scissor_rect: Option<[u32; 4]>,
    ) -> Option<TransformSurfaceGeometrySnapshot> {
        let viewport_transform = self.resolved_transform?;
        TransformSurfaceGeometrySnapshot::new(
            raster_bounds,
            raster_bounds,
            viewport_transform,
            outer_scissor_rect,
        )
    }

    fn build_legacy_layer_subtree(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        mut ctx: UiBuildContext,
        force_self_opaque: bool,
        group_opacity: bool,
        owner_texture: Option<LegacyOwnerTexturePaint>,
    ) -> BuildState {
        let placement = ctx.owner_paint_offset_projection();
        let transformed = self.resolved_transform.is_some();
        let geometry = if transformed {
            self.transform_surface_geometry_snapshot_with_placement(
                arena,
                placement.host_neutral,
                placement.active,
                ctx.scissor_rect(),
            )
        } else {
            // Opacity alone keeps the caller's coordinate space and snap.
            // Include descendant output, even outside the owner's layout box.
            self.transform_subtree_raster_bounds(arena, ctx.paint_offset(), false)
                .and_then(|bounds| {
                    TransformSurfaceGeometrySnapshot::new(
                        bounds,
                        bounds,
                        Mat4::IDENTITY,
                        ctx.scissor_rect(),
                    )
                })
        };
        let Some(geometry) = geometry else {
            // Invalid layer geometry must never reach texture allocation
            // or a composite pass. Retaining the caller state is the legacy
            // fail-closed fallback for this frame.
            return ctx.into_state();
        };
        let source_bounds = geometry.source_bounds;
        // Texture descriptors have unsigned origins. Preserve every source
        // texel by rebasing negative raster coordinates, while the receiver
        // quad remains in the original viewport coordinate system.
        let scale = ctx.viewport().scale_factor().max(0.0001);
        let raster_shift = [
            -(source_bounds.x * scale).floor().min(0.0) / scale,
            -(source_bounds.y * scale).floor().min(0.0) / scale,
        ];
        let raster_bounds = crate::view::base_component::RetainedSurfaceBounds {
            x: source_bounds.x + raster_shift[0],
            y: source_bounds.y + raster_shift[1],
            ..source_bounds
        };
        let mut layer_ctx = UiBuildContext::from_parts(
            ctx.viewport(),
            ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
        );
        // With a transform, host placement belongs to the receiver edge.
        // Re-enter the detached target with the same zero-host owner snap chain used to
        // derive `source_bounds`; nested surfaces retain their parent-surface
        // fractional snap instead of being reset globally to zero.
        let raster_paint_offset = if transformed {
            placement.host_neutral
        } else {
            ctx.paint_offset()
        };
        layer_ctx.set_paint_offset([
            raster_paint_offset[0] + raster_shift[0],
            raster_paint_offset[1] + raster_shift[1],
        ]);
        layer_ctx.set_current_render_transform(ctx.current_render_transform());
        let layer_target = layer_ctx.allocate_persistent_target_with_key(
            graph,
            if transformed {
                crate::view::base_component::transformed_layer_stable_key(self.stable_id())
            } else {
                crate::view::base_component::isolation_layer_stable_key(self.stable_id())
            },
            raster_bounds,
        );
        layer_ctx.set_current_target(layer_target);
        graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: layer_ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: layer_target,
            },
        ));
        let layer_state = self.build_base_descendants_only_inner(
            graph,
            arena,
            layer_ctx,
            force_self_opaque || group_opacity,
            false,
            owner_texture,
        );
        ctx.state.merge_child_render_state(&layer_state);

        let parent_target = ctx.current_target().unwrap_or_else(|| {
            let target = ctx.allocate_target(graph);
            ctx.set_current_target(target);
            target
        });
        ctx.set_current_target(parent_target);
        graph.add_graphics_pass(crate::view::render_pass::TextureCompositePass::new(
            {
                let mut params = geometry.texture_composite_params();
                params.uv_bounds = Some([
                    raster_bounds.x,
                    raster_bounds.y,
                    raster_bounds.width,
                    raster_bounds.height,
                ]);
                if group_opacity {
                    // Only this owner's opacity is neutralized inside the
                    // layer. Child groups retain their own local opacity.
                    params.opacity = self.opacity;
                }
                params
            },
            crate::view::render_pass::TextureCompositeInput::from_render_target(
                crate::view::render_pass::TextureCompositeSourceIn::with_handle(
                    layer_target
                        .handle()
                        .expect("transformed layer target should exist"),
                ),
                Default::default(),
                ctx.graphics_pass_context(),
            ),
            crate::view::render_pass::TextureCompositeOutput {
                render_target: parent_target,
            },
        ));
        ctx.set_current_target(parent_target);
        ctx.into_state()
    }

    /// Execute an axis-aligned native texture prepared at zero paint offset
    /// inside the same owner scope as decoration and descendants. Preparation
    /// and resource freezing remain the caller's responsibility; owner layout
    /// bounds must enclose the texture (Image/SVG fit maps into the content box).
    pub(crate) fn build_base_with_texture(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        ctx: UiBuildContext,
        params: crate::view::render_pass::TextureCompositeParams,
        upload: crate::view::sampled_texture::SampledTextureUpload,
    ) -> BuildState {
        self.build_base_descendants_only_inner(
            graph,
            arena,
            ctx,
            false,
            true,
            Some(LegacyOwnerTexturePaint { params, upload }),
        )
    }

    pub(crate) fn build_base_only(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        ctx: UiBuildContext,
    ) -> BuildState {
        self.build_base_descendants_only(graph, arena, ctx, false)
    }
}

#[cfg(test)]
mod paint_snap_tests;
