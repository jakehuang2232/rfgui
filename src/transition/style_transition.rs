#![allow(missing_docs)]

//! Style transition primitives for animating style-derived values.
use rustc_hash::FxHashMap;

use super::{
    ChannelId, ClaimMode, RunResult, StartTrackError, TimeFunction, TrackKey, TrackTarget,
    Transition, TransitionFrame, TransitionHost, TransitionPluginId, elapsed_seconds_from_frame,
    normalized_timeline_progress,
};
use crate::style::{BoxShadow, Color, Interpolate, Length, Transform, TransformOrigin};

pub const CHANNEL_STYLE_OPACITY: ChannelId = ChannelId(30_001);
pub const CHANNEL_STYLE_BORDER_RADIUS: ChannelId = ChannelId(30_002);
pub const CHANNEL_STYLE_BACKGROUND_COLOR: ChannelId = ChannelId(30_003);
pub const CHANNEL_STYLE_COLOR: ChannelId = ChannelId(30_004);
pub const CHANNEL_STYLE_BORDER_TOP_COLOR: ChannelId = ChannelId(30_005);
pub const CHANNEL_STYLE_BORDER_RIGHT_COLOR: ChannelId = ChannelId(30_006);
pub const CHANNEL_STYLE_BORDER_BOTTOM_COLOR: ChannelId = ChannelId(30_007);
pub const CHANNEL_STYLE_BORDER_LEFT_COLOR: ChannelId = ChannelId(30_008);
pub const CHANNEL_STYLE_TRANSFORM: ChannelId = ChannelId(30_009);
pub const CHANNEL_STYLE_TRANSFORM_ORIGIN: ChannelId = ChannelId(30_010);
pub const CHANNEL_STYLE_BOX_SHADOW: ChannelId = ChannelId(30_011);
pub const CHANNEL_STYLE_PADDING_TOP: ChannelId = ChannelId(30_012);
pub const CHANNEL_STYLE_PADDING_RIGHT: ChannelId = ChannelId(30_013);
pub const CHANNEL_STYLE_PADDING_BOTTOM: ChannelId = ChannelId(30_014);
pub const CHANNEL_STYLE_PADDING_LEFT: ChannelId = ChannelId(30_015);
pub const CHANNEL_STYLE_BORDER_TOP_WIDTH: ChannelId = ChannelId(30_016);
pub const CHANNEL_STYLE_BORDER_RIGHT_WIDTH: ChannelId = ChannelId(30_017);
pub const CHANNEL_STYLE_BORDER_BOTTOM_WIDTH: ChannelId = ChannelId(30_018);
pub const CHANNEL_STYLE_BORDER_LEFT_WIDTH: ChannelId = ChannelId(30_019);
pub const CHANNEL_STYLE_GAP: ChannelId = ChannelId(30_020);

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
    Transform,
    TransformOrigin,
    BoxShadow,
    PaddingTop,
    PaddingRight,
    PaddingBottom,
    PaddingLeft,
    BorderTopWidth,
    BorderRightWidth,
    BorderBottomWidth,
    BorderLeftWidth,
    Gap,
}

impl StyleField {
    pub const ALL: [Self; 20] = [
        Self::Opacity,
        Self::BorderRadius,
        Self::BackgroundColor,
        Self::Color,
        Self::BorderTopColor,
        Self::BorderRightColor,
        Self::BorderBottomColor,
        Self::BorderLeftColor,
        Self::Transform,
        Self::TransformOrigin,
        Self::BoxShadow,
        Self::PaddingTop,
        Self::PaddingRight,
        Self::PaddingBottom,
        Self::PaddingLeft,
        Self::BorderTopWidth,
        Self::BorderRightWidth,
        Self::BorderBottomWidth,
        Self::BorderLeftWidth,
        Self::Gap,
    ];

    pub fn from_channel_id(channel: ChannelId) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|field| field.channel_id() == channel)
    }

    /// Padding, border widths and gap: lengths layout resolves, so a sample
    /// needs a relayout.
    pub const fn is_box_length(self) -> bool {
        matches!(
            self,
            Self::PaddingTop
                | Self::PaddingRight
                | Self::PaddingBottom
                | Self::PaddingLeft
                | Self::BorderTopWidth
                | Self::BorderRightWidth
                | Self::BorderBottomWidth
                | Self::BorderLeftWidth
                | Self::Gap
        )
    }

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
            Self::Transform => CHANNEL_STYLE_TRANSFORM,
            Self::TransformOrigin => CHANNEL_STYLE_TRANSFORM_ORIGIN,
            Self::BoxShadow => CHANNEL_STYLE_BOX_SHADOW,
            Self::PaddingTop => CHANNEL_STYLE_PADDING_TOP,
            Self::PaddingRight => CHANNEL_STYLE_PADDING_RIGHT,
            Self::PaddingBottom => CHANNEL_STYLE_PADDING_BOTTOM,
            Self::PaddingLeft => CHANNEL_STYLE_PADDING_LEFT,
            Self::BorderTopWidth => CHANNEL_STYLE_BORDER_TOP_WIDTH,
            Self::BorderRightWidth => CHANNEL_STYLE_BORDER_RIGHT_WIDTH,
            Self::BorderBottomWidth => CHANNEL_STYLE_BORDER_BOTTOM_WIDTH,
            Self::BorderLeftWidth => CHANNEL_STYLE_BORDER_LEFT_WIDTH,
            Self::Gap => CHANNEL_STYLE_GAP,
        }
    }

    pub fn default_value(self) -> StyleValue {
        match self {
            Self::Opacity | Self::BorderRadius => StyleValue::Scalar(0.0),
            Self::BackgroundColor
            | Self::Color
            | Self::BorderTopColor
            | Self::BorderRightColor
            | Self::BorderBottomColor
            | Self::BorderLeftColor => StyleValue::Color(Color::rgba(0, 0, 0, 0)),
            Self::Transform => StyleValue::Transform(Transform::default()),
            Self::TransformOrigin => StyleValue::TransformOrigin(TransformOrigin::center()),
            Self::BoxShadow => StyleValue::BoxShadow(Vec::new()),
            Self::PaddingTop
            | Self::PaddingRight
            | Self::PaddingBottom
            | Self::PaddingLeft
            | Self::BorderTopWidth
            | Self::BorderRightWidth
            | Self::BorderBottomWidth
            | Self::BorderLeftWidth
            | Self::Gap => StyleValue::Length(Length::Zero),
        }
    }

    pub fn interpolate_value(self, from: StyleValue, to: StyleValue, t: f32) -> StyleValue {
        match self {
            Self::Opacity | Self::BorderRadius => match (from, to) {
                (StyleValue::Scalar(from), StyleValue::Scalar(to)) => {
                    StyleValue::Scalar(f32::interpolate(&from, &to, t))
                }
                (_, to) => to,
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
                (_, to) => to,
            },
            Self::Transform => match (from, to) {
                (StyleValue::Transform(from), StyleValue::Transform(to)) => {
                    StyleValue::TransformProgress {
                        from,
                        to,
                        progress: t.clamp(0.0, 1.0),
                    }
                }
                (StyleValue::TransformProgress { from, to, .. }, StyleValue::Transform(_))
                | (StyleValue::Transform(_), StyleValue::TransformProgress { from, to, .. })
                | (
                    StyleValue::TransformProgress { from, to, .. },
                    StyleValue::TransformProgress { .. },
                ) => StyleValue::TransformProgress {
                    from,
                    to,
                    progress: t.clamp(0.0, 1.0),
                },
                (_, to) => to,
            },
            Self::TransformOrigin => match (from, to) {
                (StyleValue::TransformOrigin(from), StyleValue::TransformOrigin(to)) => {
                    StyleValue::TransformOriginProgress {
                        from,
                        to,
                        progress: t.clamp(0.0, 1.0),
                    }
                }
                (
                    StyleValue::TransformOriginProgress { from, to, .. },
                    StyleValue::TransformOrigin(_),
                )
                | (
                    StyleValue::TransformOrigin(_),
                    StyleValue::TransformOriginProgress { from, to, .. },
                )
                | (
                    StyleValue::TransformOriginProgress { from, to, .. },
                    StyleValue::TransformOriginProgress { .. },
                ) => StyleValue::TransformOriginProgress {
                    from,
                    to,
                    progress: t.clamp(0.0, 1.0),
                },
                (_, to) => to,
            },
            Self::BoxShadow => match (from, to) {
                (StyleValue::BoxShadow(from), StyleValue::BoxShadow(to)) => {
                    StyleValue::BoxShadow(Vec::<BoxShadow>::interpolate(&from, &to, t))
                }
                (_, to) => to,
            },
            // A mix of a px and a percent stays a `calc` that layout resolves
            // against the current base, as in CSS. The end returns the target
            // itself, so the finished value equals the computed one.
            Self::PaddingTop
            | Self::PaddingRight
            | Self::PaddingBottom
            | Self::PaddingLeft
            | Self::BorderTopWidth
            | Self::BorderRightWidth
            | Self::BorderBottomWidth
            | Self::BorderLeftWidth
            | Self::Gap => match (from, to) {
                (StyleValue::Length(_), to) if t >= 1.0 => to,
                (StyleValue::Length(from), StyleValue::Length(to)) => {
                    StyleValue::Length(Length::interpolate(&from, &to, t))
                }
                (_, to) => to,
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

#[derive(Clone, Debug, PartialEq)]
pub enum StyleValue {
    Scalar(f32),
    Color(Color),
    Transform(Transform),
    TransformProgress {
        from: Transform,
        to: Transform,
        progress: f32,
    },
    TransformOrigin(TransformOrigin),
    TransformOriginProgress {
        from: TransformOrigin,
        to: TransformOrigin,
        progress: f32,
    },
    BoxShadow(Vec<BoxShadow>),
    Length(Length),
}

#[derive(Clone, Debug, PartialEq)]
pub struct StyleSample {
    pub target: TrackTarget,
    pub field: StyleField,
    pub value: StyleValue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StyleTrackRequest {
    pub target: TrackTarget,
    pub field: StyleField,
    pub from: StyleValue,
    pub to: StyleValue,
    pub transition: StyleTransition,
}

#[derive(Clone, Debug, PartialEq)]
struct StyleTrackState {
    from: StyleValue,
    to: StyleValue,
    started_at_seconds: Option<f64>,
    transition: StyleTransition,
}

#[derive(Debug)]
pub struct StyleTransitionPlugin {
    plugin_id: TransitionPluginId,
    tracks: FxHashMap<TrackKey<TrackTarget>, StyleTrackState>,
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
            tracks: FxHashMap::default(),
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
            // Same target -> keep the running track. A rebuild restores the
            // interpolated value into `from`, but the in-flight track already
            // owns the timeline; replacing it would restart from progress 0.
            if existing.to == to {
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
        StyleField::ALL.map(StyleField::channel_id).to_vec()
    }

    fn start_track(
        &mut self,
        key: TrackKey<TrackTarget>,
        host: &mut dyn TransitionHost<TrackTarget>,
    ) -> Result<(), StartTrackError<TrackTarget>> {
        let Some(field) = StyleField::from_channel_id(key.channel) else {
            return Err(StartTrackError::ChannelNotRegistered(key.channel));
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
            let Some(field) = StyleField::from_channel_id(key.channel) else {
                continue;
            };
            let value = field.interpolate_value(state.from.clone(), state.to.clone(), eased);
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
mod tests;
