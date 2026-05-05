//! `Layoutable` impl for Text.

use crate::style::TextWrap;
use crate::time::Instant;
use crate::view::base_component::{
    DirtyFlags, FlexProps, InlineMeasureContext, InlineNodeSize, InlinePlacement, Layoutable,
    LayoutConstraints, LayoutPlacement, Position, Size,
};
use crate::view::node_arena::NodeArena;
use crate::view::text_layout::measure_buffer_size;

use super::cache::MeasuredTextLayout;
use super::profile::{record_text_measure_profile, text_measure_profile_enabled};
use super::Text;

impl Layoutable for Text {
    fn measured_size(&self) -> (f32, f32) {
        (self.size.width, self.size.height)
    }

    fn set_layout_width(&mut self, width: f32) {
        self.layout_override_width = Some(width.max(0.0));
    }

    fn set_layout_height(&mut self, height: f32) {
        self.layout_override_height = Some(height.max(0.0));
    }

    fn flex_props(&self) -> FlexProps {
        use crate::style::{Length, SizeValue};
        let (measured_w, measured_h) = self.measured_size();
        FlexProps {
            width: if self.auto_width {
                SizeValue::Auto
            } else {
                SizeValue::Length(Length::Px(self.size.width))
            },
            height: if self.auto_height {
                SizeValue::Auto
            } else {
                SizeValue::Length(Length::Px(self.size.height))
            },
            allows_cross_stretch_when_row: self.auto_height,
            allows_cross_stretch_when_col: self.auto_width,
            intrinsic_width: Some(measured_w),
            intrinsic_height: Some(measured_h),
            intrinsic_feeds_auto_min: true,
            intrinsic_feeds_auto_base: false,
            ..FlexProps::default()
        }
    }

    fn inline_relative_position(&self) -> (f32, f32) {
        (self.position.x, self.position.y)
    }

    fn set_layout_offset(&mut self, x: f32, y: f32) {
        self.position = Position { x, y };
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::RUNTIME);
    }

    fn measure_inline(
        &mut self,
        context: InlineMeasureContext,
        _arena: &mut NodeArena,
    ) {
        if !self.dirty_flags.intersects(DirtyFlags::LAYOUT)
            && self.last_inline_measure_context == Some(context)
        {
            return;
        }
        let started_at = text_measure_profile_enabled().then(Instant::now);
        self.inline_plan = None;
        self.last_inline_measure_context = Some(context);

        if self.content.is_empty() {
            self.size = Size {
                width: 0.0,
                height: 0.0,
            };
            self.render_size = self.size;
            self.dirty_flags = self.dirty_flags.without(DirtyFlags::LAYOUT).union(
                DirtyFlags::PLACE
                    .union(DirtyFlags::BOX_MODEL)
                    .union(DirtyFlags::HIT_TEST)
                    .union(DirtyFlags::PAINT),
            );
            if let Some(started_at) = started_at {
                let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
                record_text_measure_profile(|profile| {
                    profile.measure_inline_calls += 1;
                    profile.measure_inline_ms += elapsed_ms;
                });
            }
            return;
        }

        let plan = self.build_inline_plan(
            context.first_available_width.max(1.0),
            context.full_available_width.max(1.0),
        );
        self.inline_plan = Some(plan.clone());
        self.size = Size {
            width: plan.max_width.max(0.0),
            height: plan.max_height.max(0.0),
        };
        self.render_size = Size {
            width: plan.max_width.max(0.0),
            height: plan.max_height.max(0.0),
        };
        self.layout_buffer = None;
        self.dirty_flags = self.dirty_flags.without(DirtyFlags::LAYOUT).union(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST)
                .union(DirtyFlags::PAINT),
        );
        if let Some(started_at) = started_at {
            let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
            record_text_measure_profile(|profile| {
                profile.measure_inline_calls += 1;
                profile.measure_inline_ms += elapsed_ms;
            });
        }
    }

    fn get_inline_nodes_size(
        &self,
        _arena: &NodeArena,
    ) -> Vec<InlineNodeSize> {
        let runs = self
            .inline_plan
            .as_ref()
            .map(|plan| plan.runs.as_slice())
            .unwrap_or(&[]);
        let last = runs.len().saturating_sub(1);
        runs.iter()
            .enumerate()
            .map(|(idx, fragment)| InlineNodeSize {
                width: fragment.width,
                height: fragment.height,
                baseline: fragment.baseline,
                vertical_align: self.vertical_align,
                // Each fragment is one visual line of this Text already wrapped
                // by cosmic_text — trailing whitespace was stripped per CSS, so
                // packing two fragments side-by-side in the parent inline solver
                // would render adjacent words with no separator. Force a break
                // after every fragment except the last so subsequent inline
                // siblings can still continue on the last line.
                force_break_after: idx < last,
            })
            .collect()
    }

    fn place_inline(
        &mut self,
        placement: InlinePlacement,
        _arena: &mut NodeArena,
    ) {
        let Some(plan) = self.inline_plan.as_mut() else {
            return;
        };
        if placement.node_index == 0 {
            for fragment in &mut plan.runs {
                fragment.position = None;
            }
            self.layout_state.layout_position = Position {
                x: placement.x,
                y: placement.y,
            };
            self.layout_state.layout_size = Size {
                width: 0.0,
                height: 0.0,
            };
            self.layout_state.should_render = false;
        }

        let Some(fragment) = plan.runs.get_mut(placement.node_index) else {
            return;
        };
        fragment.position = Some(Position {
            x: placement.x,
            y: placement.y,
        });

        let left = placement.x;
        let top = placement.y;
        let right = placement.x + fragment.width.max(0.0);
        let bottom = placement.y + fragment.height.max(0.0);
        if self.layout_state.should_render {
            let current_right = self.layout_state.layout_position.x + self.layout_state.layout_size.width;
            let current_bottom = self.layout_state.layout_position.y + self.layout_state.layout_size.height;
            self.layout_state.layout_position.x = self.layout_state.layout_position.x.min(left);
            self.layout_state.layout_position.y = self.layout_state.layout_position.y.min(top);
            self.layout_state.layout_size.width = current_right.max(right) - self.layout_state.layout_position.x;
            self.layout_state.layout_size.height = current_bottom.max(bottom) - self.layout_state.layout_position.y;
        } else {
            self.layout_state.layout_position = Position { x: left, y: top };
            self.layout_state.layout_size = Size {
                width: (right - left).max(0.0),
                height: (bottom - top).max(0.0),
            };
        }
        self.layout_state.should_render = self.layout_state.layout_size.width > 0.0 && self.layout_state.layout_size.height > 0.0;
        self.dirty_flags = self.dirty_flags.without(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
        );
    }

    fn measure(
        &mut self,
        constraints: LayoutConstraints,
        _arena: &mut NodeArena,
    ) {
        self.inline_plan = None;
        self.last_inline_measure_context = None;
        self.layout_override_width = None;
        self.layout_override_height = None;
        let parent_width_is_constrained = constraints.percent_base_width.is_some();
        let next_allow_wrap = self.text_wrap == TextWrap::Wrap && parent_width_is_constrained;
        if self.allow_wrap != next_allow_wrap {
            self.allow_wrap = next_allow_wrap;
        }
        self.layout_buffer = None;

        if !self.auto_width && !self.auto_height {
            self.layout_buffer = Some(
                self.relayout_from_base(Some(self.size.width.max(1.0)), self.allow_wrap)
                    .buffer,
            );
            self.dirty_flags = self.dirty_flags.without(DirtyFlags::LAYOUT);
            return;
        }
        let mut intrinsic_layout: Option<MeasuredTextLayout> = None;
        if self.auto_width {
            let next_intrinsic_layout = match self.cached_intrinsic_layout.as_ref() {
                Some((revision, layout)) if *revision == self.measure_revision => layout.clone(),
                _ => {
                    let layout = self.relayout_from_base(None, false);
                    self.cached_intrinsic_layout = Some((self.measure_revision, layout.clone()));
                    layout
                }
            };
            let intrinsic_width = next_intrinsic_layout.width;
            intrinsic_layout = Some(next_intrinsic_layout);
            let available = if parent_width_is_constrained {
                constraints.max_width.max(1.0)
            } else {
                f32::INFINITY
            };
            self.size.width = intrinsic_width.min(available).max(0.0);
            self.render_size.width = intrinsic_width.min(available).max(0.0);
        }
        if self.auto_height {
            let effective_width = if self.auto_width {
                self.render_size.width.max(1.0)
            } else {
                self.size.width.min(constraints.max_width.max(1.0)).max(1.0)
            };
            if let Some(layout) = intrinsic_layout.as_ref()
                && !self.allow_wrap
                && (effective_width - layout.width.max(1.0)).abs() <= 0.01
            {
                self.cached_height_for_width =
                    Some((self.measure_revision, effective_width, layout.height));
                self.size.height = layout.height.max(1.0);
                self.render_size.height = layout.height.max(1.0);
                self.layout_buffer = Some(layout.buffer.clone());
            } else {
                let buffer = self
                    .relayout_from_base(Some(effective_width), self.allow_wrap)
                    .buffer;
                let (_, measured_height) = measure_buffer_size(&buffer);
                self.cached_height_for_width =
                    Some((self.measure_revision, effective_width, measured_height));
                self.size.height = measured_height.max(1.0);
                self.render_size.height = measured_height.max(1.0);
                self.layout_buffer = Some(buffer);
            }
        } else {
            let final_width = if self.auto_width {
                self.render_size.width.max(1.0)
            } else {
                self.size.width.max(1.0)
            };
            if let Some(layout) = intrinsic_layout
                && !self.allow_wrap
                && (final_width - layout.width.max(1.0)).abs() <= 0.01
            {
                self.layout_buffer = Some(layout.buffer);
            } else {
                self.layout_buffer = Some(
                    self.relayout_from_base(Some(final_width), self.allow_wrap)
                        .buffer,
                );
            }
        }
        self.dirty_flags = self.dirty_flags.without(DirtyFlags::LAYOUT);
    }

    fn place(
        &mut self,
        placement: LayoutPlacement,
        _arena: &mut NodeArena,
    ) {
        if !self.dirty_flags.intersects(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
        ) && self.last_layout_placement == Some(placement)
        {
            return;
        }
        let available_width = placement.available_width.max(0.0);
        let available_height = placement.available_height.max(0.0);
        let max_width = (available_width - self.position.x.max(0.0)).max(0.0);
        let max_height = (available_height - self.position.y.max(0.0)).max(0.0);
        let layout_width = self.layout_override_width.unwrap_or(self.size.width);
        let layout_height = self.layout_override_height.unwrap_or(self.size.height);
        self.layout_state.layout_size = Size {
            width: layout_width.max(0.0).min(max_width),
            height: layout_height.max(0.0).min(max_height),
        };
        self.layout_state.layout_position = Position {
            x: placement.parent_x + self.position.x + placement.visual_offset_x,
            y: placement.parent_y + self.position.y + placement.visual_offset_y,
        };

        let parent_left = placement.parent_x + placement.visual_offset_x;
        let parent_top = placement.parent_y + placement.visual_offset_y;
        let parent_right = parent_left + available_width;
        let parent_bottom = parent_top + available_height;
        let self_left = self.layout_state.layout_position.x;
        let self_top = self.layout_state.layout_position.y;
        let self_right = self.layout_state.layout_position.x + self.layout_state.layout_size.width;
        let self_bottom = self.layout_state.layout_position.y + self.layout_state.layout_size.height;
        self.layout_state.should_render = self.layout_state.layout_size.width > 0.0
            && self.layout_state.layout_size.height > 0.0
            && self_right > parent_left
            && self_left < parent_right
            && self_bottom > parent_top
            && self_top < parent_bottom;
        self.last_layout_placement = Some(placement);
        self.dirty_flags = self.dirty_flags.without(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
        );
    }
}
