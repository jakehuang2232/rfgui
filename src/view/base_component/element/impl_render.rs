#[derive(Clone)]
pub(super) struct SelfDecorationPaintOps {
    fill: Option<crate::view::paint::DrawRectOp>,
    border: Option<crate::view::paint::DrawRectOp>,
}

/// Per-child "renders outside the inner clip" flags, in child order, and
/// whether the remaining children need the inner clip.
#[derive(Clone)]
pub(super) struct ChildClipClassification {
    outside_inner_clip: std::sync::Arc<[bool]>,
    clip_children: bool,
}

#[derive(Clone, Copy)]
struct SelfPaintRecordingGeometry {
    bounds: crate::view::base_component::Rect,
}

struct PreparedSelfPaintRecord {
    geometry: SelfPaintRecordingGeometry,
    shadows: std::sync::Arc<[crate::view::paint::PreparedShadowOp]>,
    decoration: SelfDecorationPaintOps,
    payload_identity: crate::view::paint::PaintPayloadIdentity,
}

impl PreparedSelfPaintRecord {
    /// The border box grown to cover the outer shadows.
    fn ink_bounds(&self) -> crate::view::base_component::Rect {
        crate::view::paint::bounds_with_shadow_ink(self.geometry.bounds, &self.shadows)
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
    pub(crate) fn inline_ifc_decoration_package_for_test(
        &mut self,
    ) -> Option<
        &mut crate::view::inline_formatting_context::InlineIfcElementDecorationDrawRectPackage,
    > {
        self.inline_ifc_rollout_packages
            .as_deref_mut()?
            .decoration_draw_rect
            .as_mut()
    }

    #[cfg(test)]
    pub(crate) fn set_should_paint_for_test(&mut self, should_paint: bool) {
        self.core.should_paint = should_paint;
    }

    fn requires_child_mask_surface(&self, arena: &crate::view::node_arena::NodeArena) -> bool {
        !self.children.is_empty() && self.child_clip_classification(arena).clip_children
    }

    /// Which children render outside the inner clip, and whether the rest
    /// must be clipped. A coverage walk asks from several hooks per owner and
    /// reads the arena immutably, so one walk computes it once per owner.
    pub(super) fn child_clip_classification(
        &self,
        arena: &crate::view::node_arena::NodeArena,
    ) -> ChildClipClassification {
        let memo_key =
            crate::view::paint::coverage_walk::active().map(|walk| (walk, arena.mutation_clock()));
        if let Some(key) = memo_key
            && let Some((cached_key, cached)) = self.child_clip_memo.borrow().as_ref()
            && *cached_key == key
        {
            return cached.clone();
        }
        let outside_inner_clip: std::sync::Arc<[bool]> = (0..self.children.len())
            .map(|idx| self.child_renders_outside_inner_clip(idx, arena))
            .collect();
        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.layout_state.layout_size.width.max(0.0),
            self.layout_state.layout_size.height.max(0.0),
        );
        let inner_radii = self.inner_clip_radii(outer_radii);
        let classification = ChildClipClassification {
            clip_children: self.should_clip_children(&outside_inner_clip, inner_radii, arena),
            outside_inner_clip,
        };
        if let Some(key) = memo_key {
            *self.child_clip_memo.borrow_mut() = Some((key, classification.clone()));
        }
        classification
    }

    pub(super) fn prepared_retained_child_mask_plan(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        recording_context: &crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::RetainedChildMaskPlan> {
        if (self.scroll_direction != ScrollDirection::None
            && !recording_context.authorizes_generic_scroll_host_root(self.stable_id()))
            || (self.is_fragmentable_inline_element() && self.inline_paint_fragments.len() > 1)
            || !self.requires_child_mask_surface(arena)
        {
            return None;
        }
        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.layout_state.layout_size.width.max(0.0),
            self.layout_state.layout_size.height.max(0.0),
        );
        let inner_radii = self.inner_clip_radii(outer_radii);
        let bounds = self.inner_clip_rect();
        // A mask wholly above or left of the viewport keeps an empty scissor:
        // it writes no stencil, so its in-scope children paint nothing.
        let logical_scissor = logical_scissor_for_clip_rect(bounds)?;
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
        // Capability above and ordered child classification below are live
        // observations, including changes without dirty notification. Only the
        // immutable mask payload and partition storage survive between calls.
        let outside_inner_clip = self.child_clip_classification(arena).outside_inner_clip;
        if let Some(previous) = self.child_mask_recording_inputs.borrow().as_ref()
            && previous.matches_live_inputs(
                bounds,
                logical_scissor,
                &op,
                self.children
                    .iter()
                    .copied()
                    .zip(outside_inner_clip.iter().copied()),
            )
        {
            crate::view::paint::work_profile::count("child_mask_input_replays", 1);
            return Some((**previous).clone());
        }
        let mut in_scope_children = Vec::new();
        let mut overflow_children = Vec::new();
        for (&child, &outside) in self.children.iter().zip(outside_inner_clip.iter()) {
            if outside {
                overflow_children.push(child);
            } else {
                in_scope_children.push(child);
            }
        }
        let plan = crate::view::paint::RetainedChildMaskPlan::new(
            bounds,
            logical_scissor,
            op,
            &self.children,
            in_scope_children,
            overflow_children,
        )?;
        *self.child_mask_recording_inputs.borrow_mut() = Some(Box::new(plan.clone()));
        Some(plan)
    }

    fn measure_flex_children(
        &mut self,
        proposal: LayoutProposal,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let insets = resolve_layout_insets(
            &self.box_lengths.border_widths,
            &self.box_lengths.padding,
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
            self.box_lengths.gap,
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
        self.flex_info = Some(Box::new(outputs.flex_info));
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

    pub(super) fn record_shadow_node_paint_artifact(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &crate::view::node_arena::NodeArena,
    ) -> Option<crate::view::paint::PaintArtifact> {
        use crate::view::paint::{PaintArtifact, PaintChunk, PaintOp};
        let prepared = self.prepared_self_paint_record()?;
        let mut metadata =
            self.record_shadow_node_paint_metadata(owner, properties, content_revision)?;
        metadata.bounds = prepared.ink_bounds();
        let mut ops = prepared
            .shadows
            .iter()
            .cloned()
            .map(PaintOp::PreparedShadow)
            .collect::<Vec<_>>();
        ops.extend(prepared.decoration.into_iter().map(PaintOp::DrawRect));
        metadata.payload_identity = prepared.payload_identity;
        Some(PaintArtifact {
            chunks: vec![PaintChunk {
                frame: crate::view::paint::PaintChunkFrame::Viewport,
                id: metadata.id,
                owner: metadata.owner,
                op_range: 0..ops.len(),
                bounds: metadata.bounds,
                properties: metadata.properties,
                content_revision: metadata.content_revision,
                payload_identity: metadata.payload_identity,
            }]
            .into(),
            ops: ops.into(),
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
                snap: crate::view::paint::PaintOwnerSnap::observe(self, arena, None),
            }],
        })
    }

    pub(super) fn record_shadow_node_paint_metadata(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
    ) -> Option<crate::view::paint::PaintChunkMetadata> {
        use crate::view::paint::{PaintChunkId, PaintChunkMetadata, PaintChunkRole};
        if !self.layout_state.should_render {
            return None;
        }
        let prepared = self.prepared_self_paint_record()?;
        Some(PaintChunkMetadata {
            frame: crate::view::paint::PaintChunkFrame::Viewport,
            id: PaintChunkId {
                owner,
                scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                slot: 0,
                role: PaintChunkRole::SelfDecoration,
            },
            owner,
            bounds: prepared.ink_bounds(),
            properties,
            content_revision,
            payload_identity: prepared.payload_identity,
        })
    }

    fn prepared_self_paint_record(&self) -> Option<PreparedSelfPaintRecord> {
        self.prepared_self_paint_from_inputs(self.self_paint_recording_geometry())
    }

    /// Produces the one geometry input shared by metadata-only and full
    /// artifact recording.
    fn self_paint_recording_geometry(&self) -> SelfPaintRecordingGeometry {
        SelfPaintRecordingGeometry {
            bounds: Rect {
                x: self.layout_state.layout_position.x,
                y: self.layout_state.layout_position.y,
                width: self.layout_state.layout_size.width.max(0.0),
                height: self.layout_state.layout_size.height.max(0.0),
            },
        }
    }

    pub(super) fn prepared_outer_shadow_ops(
        &self,
    ) -> Option<Vec<crate::view::paint::PreparedShadowOp>> {
        if !self.core.should_paint || self.box_shadows.is_empty() {
            return Some(Vec::new());
        }
        if let Some(prepared) = self.replay_prepared_outer_shadow_ops() {
            return Some(prepared);
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
        if fragment_rects.iter().any(|fragment| {
            [fragment.x, fragment.y, fragment.width, fragment.height]
                .iter()
                .any(|value| !value.is_finite())
                || fragment.width <= 0.0
                || fragment.height <= 0.0
        }) {
            return None;
        }
        let opacity = self.opacity.clamp(0.0, 1.0);
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
                let mesh = ShadowShape::rounded_rect_with_radii(
                    fragment.x - spread,
                    fragment.y - spread,
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
                        clip_to_geometry: shadow.inset,
                        caster: (!shadow.inset).then_some(crate::view::render_pass::ShadowCaster {
                            spread,
                            radii: outer_radii.to_array(),
                        }),
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
        self.inline_ifc_rollout_packages = packages.map(|packages| {
            Box::new(ElementInlineIfcRolloutPackages::from_inline_ifc_distributed(packages))
        });
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

    /// The decorations of an inline span the root's formatting context owns:
    /// one rect per installed fragment. `None` when it has nothing to draw.
    pub(super) fn prepared_inline_ifc_decoration_payload(
        &self,
    ) -> Option<PreparedElementInlineIfcDecorationPayload> {
        if !self.inline_ifc_owned_by_root
            || self
                .inline_ifc_rollout_packages
                .as_ref()
                .is_some_and(|packages| packages.atomic_placement.is_some())
        {
            return None;
        }
        if !self.core.should_paint {
            return Some(PreparedElementInlineIfcDecorationPayload {
                bounds: Rect {
                    x: self.layout_state.layout_position.x,
                    y: self.layout_state.layout_position.y,
                    width: self.layout_state.layout_size.width.max(0.0),
                    height: self.layout_state.layout_size.height.max(0.0),
                },
                shadows: Vec::new(),
                ops: Vec::new(),
            });
        }
        let shadows = self.prepared_outer_shadow_ops()?;
        let package = self
            .inline_ifc_rollout_packages
            .as_deref()?
            .decoration_draw_rect
            .as_ref()?;
        let mut ops = Vec::with_capacity(package.fragments.len());
        let mut left = f32::INFINITY;
        let mut top = f32::INFINITY;
        let mut right = f32::NEG_INFINITY;
        let mut bottom = f32::NEG_INFINITY;
        for fragment in &package.fragments {
            let mut prepared =
                self.inline_ifc_fragment_draw_rect_pass_metadata(fragment, [0.0, 0.0]);
            let opacity = fragment.metadata.opacity.clamp(0.0, 1.0);
            prepared.fill.opacity = opacity;
            if let Some(border) = &mut prepared.border {
                border.opacity = opacity;
            }
            let descriptor = crate::view::paint::PreparedInlineIfcDecorationDescriptor {
                source: fragment.source.0,
                line_index: fragment.line_index,
                range: fragment.range.clone(),
                style_key: fragment.style_key.brush,
                slice_insets: [
                    fragment.slice_insets.left,
                    fragment.slice_insets.right,
                    fragment.slice_insets.top,
                    fragment.slice_insets.bottom,
                ],
                is_first_for_source: fragment.is_first_for_source,
                is_last_for_source: fragment.is_last_for_source,
            };
            let Some(op) = crate::view::paint::PreparedInlineIfcDecorationOp::new(
                descriptor,
                prepared.fill,
                prepared.border,
            ) else {
                continue;
            };
            left = left.min(op.fill.position[0]);
            top = top.min(op.fill.position[1]);
            right = right.max(op.fill.position[0] + op.fill.size[0]);
            bottom = bottom.max(op.fill.position[1] + op.fill.size[1]);
            ops.push(op);
        }
        let width = right - left;
        let height = bottom - top;
        if !(width > 0.0 && height > 0.0) {
            return None;
        }
        Some(PreparedElementInlineIfcDecorationPayload {
            bounds: crate::view::paint::bounds_with_shadow_ink(
                Rect {
                    x: left,
                    y: top,
                    width,
                    height,
                },
                &shadows,
            ),
            shadows,
            ops,
        })
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
        self.box_lengths = BoxLengths::of(&self.computed_style);
        self.border_widths.left = resolve_px(
            self.box_lengths.border_widths.left,
            self.core.size.width,
            0.0,
            0.0,
        );
        self.border_widths.right = resolve_px(
            self.box_lengths.border_widths.right,
            self.core.size.width,
            0.0,
            0.0,
        );
        self.border_widths.top = resolve_px(
            self.box_lengths.border_widths.top,
            self.core.size.height,
            0.0,
            0.0,
        );
        self.border_widths.bottom = resolve_px(
            self.box_lengths.border_widths.bottom,
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
            self.box_lengths.padding.left,
            self.core.size.width,
            0.0,
            0.0,
        );
        self.padding.right = resolve_px(
            self.box_lengths.padding.right,
            self.core.size.width,
            0.0,
            0.0,
        );
        self.padding.top = resolve_px(
            self.box_lengths.padding.top,
            self.core.size.height,
            0.0,
            0.0,
        );
        self.padding.bottom = resolve_px(
            self.box_lengths.padding.bottom,
            self.core.size.height,
            0.0,
            0.0,
        );
    }

    fn update_resolved_transform(&mut self) {
        let matrix = self.compute_transform_matrix();
        self.resolved_inverse_transform = matrix.and_then(|matrix| {
            let det = matrix.determinant();
            if det.abs() <= 0.000_001 {
                None
            } else {
                Some(Box::new(matrix.inverse()))
            }
        });
        self.resolved_transform = matrix.map(Box::new);
        let culled = self.computed_style.backface_visibility == BackfaceVisibility::Hidden
            && matrix.is_some_and(is_back_facing);
        if culled != self.backface_culled {
            // An animated transform turning past edge-on changes what is
            // recorded, not just where it is composited.
            self.backface_culled = culled;
            self.mark_paint_dirty();
        }
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
}

#[cfg(test)]
mod paint_snap_tests;
