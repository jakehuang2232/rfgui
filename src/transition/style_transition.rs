#![allow(missing_docs)]

//! Style transition primitives for animating style-derived values.

use std::collections::HashMap;

use super::{
    ChannelId, ClaimMode, RunResult, StartTrackError, TimeFunction, TrackKey, TrackTarget,
    Transition, TransitionFrame, TransitionHost, TransitionPluginId, elapsed_seconds_from_frame,
    normalized_timeline_progress,
};
use crate::style::{Color, Interpolate};

pub const CHANNEL_STYLE_OPACITY: ChannelId = ChannelId(30_001);
pub const CHANNEL_STYLE_BORDER_RADIUS: ChannelId = ChannelId(30_002);
pub const CHANNEL_STYLE_BACKGROUND_COLOR: ChannelId = ChannelId(30_003);
pub const CHANNEL_STYLE_COLOR: ChannelId = ChannelId(30_004);
pub const CHANNEL_STYLE_BORDER_TOP_COLOR: ChannelId = ChannelId(30_005);
pub const CHANNEL_STYLE_BORDER_RIGHT_COLOR: ChannelId = ChannelId(30_006);
pub const CHANNEL_STYLE_BORDER_BOTTOM_COLOR: ChannelId = ChannelId(30_007);
pub const CHANNEL_STYLE_BORDER_LEFT_COLOR: ChannelId = ChannelId(30_008);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StyleField {
    Opacity,
    BorderRadius,
    BackgroundColor,
    Color,
    BorderTopColor,
    BorderRightColor,
    BorderBottomColor,
    BorderLeftColor,
}

impl StyleField {
    pub const fn channel_id(self) -> ChannelId {
        match self {
            Self::Opacity => CHANNEL_STYLE_OPACITY,
            Self::BorderRadius => CHANNEL_STYLE_BORDER_RADIUS,
            Self::BackgroundColor => CHANNEL_STYLE_BACKGROUND_COLOR,
            Self::Color => CHANNEL_STYLE_COLOR,
            Self::BorderTopColor => CHANNEL_STYLE_BORDER_TOP_COLOR,
            Self::BorderRightColor => CHANNEL_STYLE_BORDER_RIGHT_COLOR,
            Self::BorderBottomColor => CHANNEL_STYLE_BORDER_BOTTOM_COLOR,
            Self::BorderLeftColor => CHANNEL_STYLE_BORDER_LEFT_COLOR,
        }
    }

    pub const fn default_value(self) -> StyleValue {
        match self {
            Self::Opacity | Self::BorderRadius => StyleValue::Scalar(0.0),
            Self::BackgroundColor
            | Self::Color
            | Self::BorderTopColor
            | Self::BorderRightColor
            | Self::BorderBottomColor
            | Self::BorderLeftColor => StyleValue::Color(Color::rgba(0, 0, 0, 0)),
        }
    }

    pub fn interpolate_value(self, from: StyleValue, to: StyleValue, t: f32) -> StyleValue {
        match self {
            Self::Opacity | Self::BorderRadius => match (from, to) {
                (StyleValue::Scalar(from), StyleValue::Scalar(to)) => {
                    StyleValue::Scalar(f32::interpolate(&from, &to, t))
                }
                _ => to,
            },
            Self::BackgroundColor
            | Self::Color
            | Self::BorderTopColor
            | Self::BorderRightColor
            | Self::BorderBottomColor
            | Self::BorderLeftColor => match (from, to) {
                (StyleValue::Color(from), StyleValue::Color(to)) => {
                    StyleValue::Color(Color::interpolate(&from, &to, t))
                }
                _ => to,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StyleTransition {
    pub duration_ms: u32,
    pub delay_ms: u32,
    pub timing: TimeFunction,
}

impl StyleTransition {
    pub const fn new(duration_ms: u32) -> Self {
        Self {
            duration_ms,
            delay_ms: 0,
            timing: TimeFunction::EaseOut,
        }
    }

    pub const fn delay(mut self, delay_ms: u32) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    pub const fn timing(mut self, timing: TimeFunction) -> Self {
        self.timing = timing;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StyleValue {
    Scalar(f32),
    Color(Color),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StyleSample {
    pub target: TrackTarget,
    pub field: StyleField,
    pub value: StyleValue,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StyleTrackRequest {
    pub target: TrackTarget,
    pub field: StyleField,
    pub from: StyleValue,
    pub to: StyleValue,
    pub transition: StyleTransition,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct StyleTrackState {
    from: StyleValue,
    to: StyleValue,
    started_at_seconds: Option<f64>,
    transition: StyleTransition,
}

#[derive(Debug)]
pub struct StyleTransitionPlugin {
    plugin_id: TransitionPluginId,
    tracks: HashMap<TrackKey<TrackTarget>, StyleTrackState>,
    frame_samples: Vec<StyleSample>,
}

impl Default for StyleTransitionPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl StyleTransitionPlugin {
    pub const BUILTIN_PLUGIN_ID: TransitionPluginId = TransitionPluginId(3);

    pub fn new() -> Self {
        Self::with_plugin_id(Self::BUILTIN_PLUGIN_ID)
    }

    pub fn with_plugin_id(plugin_id: TransitionPluginId) -> Self {
        Self {
            plugin_id,
            tracks: HashMap::new(),
            frame_samples: Vec::new(),
        }
    }

    pub fn start_style_track(
        &mut self,
        host: &mut dyn TransitionHost<TrackTarget>,
        target: TrackTarget,
        field: StyleField,
        from: StyleValue,
        to: StyleValue,
        transition: StyleTransition,
    ) -> Result<(), StartTrackError<TrackTarget>> {
        let key = TrackKey {
            target,
            channel: field.channel_id(),
        };
        if let Some(existing) = self.tracks.get(&key) {
            if existing.to == to && existing.from == from {
                return Ok(());
            }
        }
        if !host.is_channel_registered(key.channel) {
            return Err(StartTrackError::ChannelNotRegistered(key.channel));
        }
        if !host.claim_track(self.plugin_id, key, ClaimMode::Replace) {
            return Err(StartTrackError::ClaimRejected(key));
        }
        self.tracks.insert(
            key,
            StyleTrackState {
                from,
                to,
                started_at_seconds: None,
                transition,
            },
        );
        Ok(())
    }

    pub fn take_samples(&mut self) -> Vec<StyleSample> {
        std::mem::take(&mut self.frame_samples)
    }
}

impl Transition<TrackTarget> for StyleTransitionPlugin {
    fn plugin_id(&self) -> TransitionPluginId {
        self.plugin_id
    }

    fn observed_channels(&self, _target: TrackTarget) -> Vec<ChannelId> {
        vec![
            CHANNEL_STYLE_OPACITY,
            CHANNEL_STYLE_BORDER_RADIUS,
            CHANNEL_STYLE_BACKGROUND_COLOR,
            CHANNEL_STYLE_COLOR,
            CHANNEL_STYLE_BORDER_TOP_COLOR,
            CHANNEL_STYLE_BORDER_RIGHT_COLOR,
            CHANNEL_STYLE_BORDER_BOTTOM_COLOR,
            CHANNEL_STYLE_BORDER_LEFT_COLOR,
        ]
    }

    fn start_track(
        &mut self,
        key: TrackKey<TrackTarget>,
        host: &mut dyn TransitionHost<TrackTarget>,
    ) -> Result<(), StartTrackError<TrackTarget>> {
        let field = match key.channel {
            CHANNEL_STYLE_OPACITY => StyleField::Opacity,
            CHANNEL_STYLE_BORDER_RADIUS => StyleField::BorderRadius,
            CHANNEL_STYLE_BACKGROUND_COLOR => StyleField::BackgroundColor,
            CHANNEL_STYLE_COLOR => StyleField::Color,
            CHANNEL_STYLE_BORDER_TOP_COLOR => StyleField::BorderTopColor,
            CHANNEL_STYLE_BORDER_RIGHT_COLOR => StyleField::BorderRightColor,
            CHANNEL_STYLE_BORDER_BOTTOM_COLOR => StyleField::BorderBottomColor,
            CHANNEL_STYLE_BORDER_LEFT_COLOR => StyleField::BorderLeftColor,
            _ => return Err(StartTrackError::ChannelNotRegistered(key.channel)),
        };
        self.start_style_track(
            host,
            key.target,
            field,
            field.default_value(),
            field.default_value(),
            StyleTransition::new(0),
        )
    }

    fn cancel_track(
        &mut self,
        key: TrackKey<TrackTarget>,
        host: &mut dyn TransitionHost<TrackTarget>,
    ) {
        self.tracks.remove(&key);
        host.release_track_claim(self.plugin_id, key);
    }

    fn run_tracks(
        &mut self,
        frame: TransitionFrame,
        host: &mut dyn TransitionHost<TrackTarget>,
    ) -> RunResult {
        self.frame_samples.clear();
        let mut finished = Vec::new();

        for (key, state) in &mut self.tracks {
            let elapsed_seconds = elapsed_seconds_from_frame(frame, &mut state.started_at_seconds);
            let delay = (state.transition.delay_ms as f32) * 0.001;
            let duration = (state.transition.duration_ms as f32) * 0.001;
            let Some(progress) = normalized_timeline_progress(elapsed_seconds, delay, duration)
            else {
                continue;
            };
            let eased = state.transition.timing.sample(progress);
            let field = match key.channel {
                CHANNEL_STYLE_OPACITY => StyleField::Opacity,
                CHANNEL_STYLE_BORDER_RADIUS => StyleField::BorderRadius,
                CHANNEL_STYLE_BACKGROUND_COLOR => StyleField::BackgroundColor,
                CHANNEL_STYLE_COLOR => StyleField::Color,
                CHANNEL_STYLE_BORDER_TOP_COLOR => StyleField::BorderTopColor,
                CHANNEL_STYLE_BORDER_RIGHT_COLOR => StyleField::BorderRightColor,
                CHANNEL_STYLE_BORDER_BOTTOM_COLOR => StyleField::BorderBottomColor,
                CHANNEL_STYLE_BORDER_LEFT_COLOR => StyleField::BorderLeftColor,
                _ => continue,
            };
            let value = field.interpolate_value(state.from, state.to, eased);
            self.frame_samples.push(StyleSample {
                target: key.target,
                field,
                value,
            });
            if progress >= 1.0 {
                finished.push(*key);
            }
        }

        for key in finished {
            self.tracks.remove(&key);
            host.release_track_claim(self.plugin_id, key);
        }

        RunResult {
            needs_layout: false,
            needs_paint: !self.frame_samples.is_empty(),
            keep_running: !self.tracks.is_empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{StyleField, StyleValue};
    use crate::style::Color;

    #[test]
    fn color_fields_delegate_interpolation_to_color_type() {
        let value = StyleField::BackgroundColor.interpolate_value(
            StyleValue::Color(Color::rgba(0, 0, 0, 0)),
            StyleValue::Color(Color::rgba(255, 255, 255, 255)),
            0.5,
        );
        assert_eq!(value, StyleValue::Color(Color::rgba(99, 99, 99, 128)));
    }

    #[test]
    fn scalar_fields_delegate_interpolation_to_scalar_type() {
        let value = StyleField::Opacity.interpolate_value(
            StyleValue::Scalar(0.2),
            StyleValue::Scalar(0.8),
            0.25,
        );
        let StyleValue::Scalar(value) = value else {
            panic!("expected scalar value");
        };
        assert!((value - 0.35).abs() < 0.0001);
    }

    #[test]
    fn field_default_values_match_property_kind() {
        assert_eq!(StyleField::Opacity.default_value(), StyleValue::Scalar(0.0));
        assert_eq!(
            StyleField::Color.default_value(),
            StyleValue::Color(Color::transparent())
        );
    }
}
