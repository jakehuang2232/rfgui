//! Transition primitives shared by style, layout, visual, and scroll animations.

use std::fmt;

mod layout_transition;
mod scroll_transition;
mod style_transition;
mod time_function;
mod visual_transition;
pub use layout_transition::*;
pub use scroll_transition::*;
pub use style_transition::*;
pub use time_function::*;
pub use visual_transition::*;

/// The default target identifier type used by built-in transition helpers.
pub type TrackTarget = u64;

/// Identifies a transition channel within a host runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChannelId(pub u32);

/// A `(target, channel)` pair used to address an individual transition track.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TrackKey<TargetType> {
    pub target: TargetType,
    pub channel: ChannelId,
}

/// Identifies a transition plugin implementation within a host runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct TransitionPluginId(pub u32);

/// Controls how a transition claims an existing track.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimMode {
    IfUnclaimed,
    Replace,
}

/// Frame timing information passed into transition execution.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransitionFrame {
    pub dt_seconds: f32,
    pub now_seconds: f64,
}

pub(crate) fn elapsed_seconds_from_frame(
    frame: TransitionFrame,
    started_at_seconds: &mut Option<f64>,
) -> f32 {
    let dt = frame.dt_seconds.max(0.0) as f64;
    let now = frame.now_seconds.max(0.0);
    let start = started_at_seconds.get_or_insert_with(|| (now - dt).max(0.0));
    (now - *start).max(0.0) as f32
}

/// The aggregate result of running a batch of transition tracks for one frame.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunResult {
    pub needs_layout: bool,
    pub needs_paint: bool,
    pub keep_running: bool,
}

#[allow(missing_docs)]
impl RunResult {
    pub const fn none() -> Self {
        Self {
            needs_layout: false,
            needs_paint: false,
            keep_running: false,
        }
    }

    pub const fn merge(self, rhs: Self) -> Self {
        Self {
            needs_layout: self.needs_layout || rhs.needs_layout,
            needs_paint: self.needs_paint || rhs.needs_paint,
            keep_running: self.keep_running || rhs.keep_running,
        }
    }
}

/// Errors returned when a transition track cannot be started.
#[allow(missing_docs)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartTrackError<TargetType> {
    ChannelNotRegistered(ChannelId),
    ClaimRejected(TrackKey<TargetType>),
    InvalidInput(&'static str),
}

impl<TargetType: fmt::Debug> fmt::Display for StartTrackError<TargetType> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ChannelNotRegistered(channel) => {
                write!(f, "channel not registered: {}", channel.0)
            }
            Self::ClaimRejected(key) => write!(
                f,
                "track claim rejected: target={:?}, channel={}",
                key.target, key.channel.0
            ),
            Self::InvalidInput(message) => write!(f, "invalid track input: {message}"),
        }
    }
}

impl<TargetType: fmt::Debug> std::error::Error for StartTrackError<TargetType> {}

/// Host callbacks required by transition plugins.
#[allow(missing_docs)]
pub trait TransitionHost<TargetType> {
    fn is_channel_registered(&self, channel: ChannelId) -> bool;

    fn claim_track(
        &mut self,
        plugin_id: TransitionPluginId,
        key: TrackKey<TargetType>,
        mode: ClaimMode,
    ) -> bool;

    fn release_track_claim(&mut self, plugin_id: TransitionPluginId, key: TrackKey<TargetType>);

    fn release_all_claims(&mut self, plugin_id: TransitionPluginId);
}

/// A transition plugin that manages one or more tracks for a target type.
#[allow(missing_docs)]
pub trait Transition<TargetType: Copy> {
    fn plugin_id(&self) -> TransitionPluginId;

    fn observed_channels(&self, target: TargetType) -> Vec<ChannelId>;

    fn start_track(
        &mut self,
        key: TrackKey<TargetType>,
        host: &mut dyn TransitionHost<TargetType>,
    ) -> Result<(), StartTrackError<TargetType>>;

    fn cancel_track(
        &mut self,
        key: TrackKey<TargetType>,
        host: &mut dyn TransitionHost<TargetType>,
    );

    fn run_tracks(
        &mut self,
        frame: TransitionFrame,
        host: &mut dyn TransitionHost<TargetType>,
    ) -> RunResult;
}
