//! Style transitions of a Text, and the color it inherits from an ancestor
//! whose `color` animates. Text runs the same style tracks as an Element for
//! the fields it draws: color, opacity, transform and transform origin.

use crate::style::{Color, ColorLike, Transform, TransformOrigin};
use crate::transition::{ChannelId, StyleField, StyleTrackRequest, StyleTransition, StyleValue};
use crate::view::base_component::{
    DirtyFlags, map_transition_timing, mark_transition_requests_pending, push_transition_channels,
};

use super::Text;

/// The drawn values a style change transitions from.
pub(super) struct TextTransitionSnapshot {
    color: Color,
    opacity: f32,
    transform: Transform,
    transform_origin: TransformOrigin,
}

const FIELDS: [StyleField; 4] = [
    StyleField::Color,
    StyleField::Opacity,
    StyleField::Transform,
    StyleField::TransformOrigin,
];

impl TextTransitionSnapshot {
    fn value_for(&self, field: StyleField) -> StyleValue {
        match field {
            StyleField::Color => StyleValue::Color(self.color),
            StyleField::Opacity => StyleValue::Scalar(self.opacity),
            StyleField::Transform => StyleValue::Transform(self.transform.clone()),
            _ => StyleValue::TransformOrigin(self.transform_origin),
        }
    }
}

impl Text {
    pub(super) fn transition_snapshot(&self) -> TextTransitionSnapshot {
        TextTransitionSnapshot {
            color: self.drawn_color(),
            opacity: self.opacity,
            transform: self.transform.clone(),
            transform_origin: self.transform_origin,
        }
    }

    /// Queues a track for each drawn field the last style change moved and a
    /// transition of this Text covers. The last matching transition wins, as
    /// in CSS.
    pub(super) fn queue_style_transitions(&mut self, previous: &TextTransitionSnapshot) {
        let current = self.transition_snapshot();
        for field in FIELDS {
            let (from, to) = (previous.value_for(field), current.value_for(field));
            if from == to {
                continue;
            }
            let Some(transition) = self.transitions.as_slice().iter().rev().find(|transition| {
                let mut channels = Vec::new();
                push_transition_channels(transition.property, &mut channels);
                channels.contains(&field.channel_id())
            }) else {
                continue;
            };
            let transition = StyleTransition {
                duration_ms: transition.duration_ms,
                delay_ms: transition.delay_ms,
                timing: map_transition_timing(transition.timing),
            };
            mark_transition_requests_pending();
            self.style_transition_requests.push(StyleTrackRequest {
                target: self.node_id,
                field,
                from,
                to,
                transition,
            });
        }
    }

    pub(crate) fn take_queued_style_transition_requests(&mut self) -> Vec<StyleTrackRequest> {
        std::mem::take(&mut self.style_transition_requests)
    }

    pub(crate) fn active_transition_channels(&self) -> Vec<ChannelId> {
        let mut channels = Vec::new();
        for transition in self.transitions.as_slice() {
            push_transition_channels(transition.property, &mut channels);
        }
        channels.retain(|channel| FIELDS.iter().any(|field| field.channel_id() == *channel));
        channels.sort_unstable_by_key(|channel| channel.0);
        channels.dedup();
        channels
    }

    /// Writes one sampled value of a running track into the drawn field, as
    /// an Element's track writes its own. Returns whether the field applies.
    pub(crate) fn set_style_transition_sample(
        &mut self,
        field: StyleField,
        value: &StyleValue,
    ) -> bool {
        match (field, value) {
            (StyleField::Color, StyleValue::Color(color)) => {
                self.color = Box::new(*color);
                self.mark_drawn_color_dirty();
            }
            (StyleField::Opacity, StyleValue::Scalar(opacity)) => {
                self.opacity = *opacity;
                self.dirty_flags = self
                    .dirty_flags
                    .union(DirtyFlags::PAINT)
                    .union(DirtyFlags::COMPOSITE);
            }
            (StyleField::Transform, StyleValue::Transform(transform)) => {
                self.set_transform(transform.clone());
            }
            (StyleField::Transform, StyleValue::TransformProgress { from, to, progress }) => {
                let size = self.layout_state.layout_size;
                self.set_transform(crate::style::interpolate_transform_with_reference_box(
                    from,
                    to,
                    *progress,
                    glam::Vec2::new(size.width.max(0.0), size.height.max(0.0)),
                ));
            }
            (StyleField::TransformOrigin, StyleValue::TransformOrigin(origin)) => {
                self.set_transform_origin(*origin);
            }
            (
                StyleField::TransformOrigin,
                StyleValue::TransformOriginProgress { from, to, progress },
            ) => {
                let size = self.layout_state.layout_size;
                self.set_transform_origin(
                    crate::style::interpolate_transform_origin_with_reference_box(
                        *from,
                        *to,
                        *progress,
                        glam::Vec2::new(size.width.max(0.0), size.height.max(0.0)),
                    ),
                );
            }
            _ => return false,
        }
        true
    }

    /// The color an animating ancestor passes down, drawn instead of the
    /// inherited one while set. Text that sets its own color ignores it.
    pub(crate) fn set_inherited_color_override(&mut self, color: Option<Color>) {
        let color = color.filter(|_| !self.has_explicit_color());
        if self.inherited_color_override != color {
            self.inherited_color_override = color;
            self.mark_drawn_color_dirty();
        }
    }

    fn mark_drawn_color_dirty(&mut self) {
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::PAINT);
    }

    /// The color this Text draws with.
    pub(crate) fn drawn_color(&self) -> Color {
        self.inherited_color_override.unwrap_or_else(|| {
            let [r, g, b, a] = self.color.to_rgba_u8();
            Color::rgba(r, g, b, a)
        })
    }
}
