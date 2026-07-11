//! `Layoutable` impl for Text.

use crate::style::TextWrap;
use crate::view::base_component::{
    DirtyFlags, FlexProps, LayoutConstraints, LayoutPlacement, Layoutable, Position, Size,
};
use crate::view::node_arena::NodeArena;

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

    fn measure(&mut self, constraints: LayoutConstraints, _arena: &mut NodeArena) {
        if !self.dirty_flags.intersects(DirtyFlags::LAYOUT)
            && self.last_layout_constraints == Some(constraints)
        {
            return;
        }
        self.layout_override_width = None;
        self.layout_override_height = None;
        let parent_width_is_constrained = constraints.percent_base_width.is_some();
        let allow_wrap = self.text_wrap == TextWrap::Wrap && parent_width_is_constrained;
        self.shaped_context = None;

        if !self.auto_width && !self.auto_height {
            let layout = self.relayout_from_base(Some(self.size.width.max(1.0)), allow_wrap);
            self.shaped_context = Some(layout.context);
            self.dirty_flags = self.dirty_flags.without(DirtyFlags::LAYOUT);
            self.last_layout_constraints = Some(constraints);
            return;
        }
        let mut intrinsic_layout = None;
        if self.auto_width {
            let next_intrinsic_layout = self.relayout_from_base(None, false);
            let intrinsic_width = next_intrinsic_layout.width;
            intrinsic_layout = Some(next_intrinsic_layout);
            let available = if parent_width_is_constrained {
                constraints.max_width.max(1.0)
            } else {
                f32::INFINITY
            };
            self.size.width = intrinsic_width.min(available).max(0.0);
        }
        if self.auto_height {
            let effective_width = if self.auto_width {
                self.size.width.max(1.0)
            } else {
                self.size.width.min(constraints.max_width.max(1.0)).max(1.0)
            };
            if let Some(layout) = intrinsic_layout.as_ref()
                && !allow_wrap
                && (effective_width - layout.width.max(1.0)).abs() <= 0.01
            {
                self.size.height = layout.height.max(1.0);
                self.shaped_context = Some(layout.context.clone());
            } else {
                let layout = self.relayout_from_base(Some(effective_width), allow_wrap);
                let measured_height = layout.height;
                self.size.height = measured_height.max(1.0);
                self.shaped_context = Some(layout.context);
            }
        } else {
            let final_width = if self.auto_width {
                self.size.width.max(1.0)
            } else {
                self.size.width.max(1.0)
            };
            if let Some(layout) = intrinsic_layout
                && !allow_wrap
                && (final_width - layout.width.max(1.0)).abs() <= 0.01
            {
                self.shaped_context = Some(layout.context);
            } else {
                let layout = self.relayout_from_base(Some(final_width), allow_wrap);
                self.shaped_context = Some(layout.context);
            }
        }
        self.dirty_flags = self.dirty_flags.without(DirtyFlags::LAYOUT);
        self.last_layout_constraints = Some(constraints);
    }

    fn place(&mut self, placement: LayoutPlacement, _arena: &mut NodeArena) {
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
        let self_bottom =
            self.layout_state.layout_position.y + self.layout_state.layout_size.height;
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
