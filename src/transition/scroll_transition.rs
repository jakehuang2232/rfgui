use std::collections::HashMap;

use super::{
    ChannelId, ClaimMode, RunResult, StartTrackError, TimeFunction, TrackKey, TrackTarget,
    Transition, TransitionFrame, TransitionHost, TransitionPluginId, elapsed_seconds_from_frame,
    normalized_timeline_progress,
};

pub const CHANNEL_SCROLL_X: ChannelId = ChannelId(10_001);
pub const CHANNEL_SCROLL_Y: ChannelId = ChannelId(10_002);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScrollAxis {
    X,
    Y,
}

impl ScrollAxis {
    pub const fn channel_id(self) -> ChannelId {
        match self {
            Self::X => CHANNEL_SCROLL_X,
            Self::Y => CHANNEL_SCROLL_Y,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollTransition {
    pub duration_ms: u32,
    pub delay_ms: u32,
    pub timing: TimeFunction,
}

impl ScrollTransition {
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

    pub const fn linear(self) -> Self {
        self.timing(TimeFunction::Linear)
    }

    pub const fn ease_in(self) -> Self {
        self.timing(TimeFunction::EaseIn)
    }

    pub const fn ease_out(self) -> Self {
        self.timing(TimeFunction::EaseOut)
    }

    pub const fn ease_in_out(self) -> Self {
        self.timing(TimeFunction::EaseInOut)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollSample {
    pub target: TrackTarget,
    pub axis: ScrollAxis,
    pub value: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollTrackState {
    from: f32,
    to: f32,
    started_at_seconds: Option<f64>,
    transition: ScrollTransition,
}

#[derive(Debug)]
pub struct ScrollTransitionPlugin {
    plugin_id: TransitionPluginId,
    tracks: HashMap<TrackKey<TrackTarget>, ScrollTrackState>,
    frame_samples: Vec<ScrollSample>,
}

impl Default for ScrollTransitionPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollTransitionPlugin {
    pub const BUILTIN_PLUGIN_ID: TransitionPluginId = TransitionPluginId(1);

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

    pub fn start_scroll_track(
        &mut self,
        host: &mut dyn TransitionHost<TrackTarget>,
        target: TrackTarget,
        axis: ScrollAxis,
        from: f32,
        to: f32,
        transition: ScrollTransition,
    ) -> Result<(), StartTrackError<TrackTarget>> {
        let key = TrackKey {
            target,
            channel: axis.channel_id(),
        };
        if !host.is_channel_registered(key.channel) {
            return Err(StartTrackError::ChannelNotRegistered(key.channel));
        }
        if !host.claim_track(self.plugin_id, key, ClaimMode::Replace) {
            return Err(StartTrackError::ClaimRejected(key));
        }
        self.tracks.insert(
            key,
            ScrollTrackState {
                from,
                to,
                started_at_seconds: None,
                transition,
            },
        );
        Ok(())
    }

    pub fn take_samples(&mut self) -> Vec<ScrollSample> {
        std::mem::take(&mut self.frame_samples)
    }
}

impl Transition<TrackTarget> for ScrollTransitionPlugin {
    fn plugin_id(&self) -> TransitionPluginId {
        self.plugin_id
    }

    fn observed_channels(&self, _target: TrackTarget) -> Vec<ChannelId> {
        vec![CHANNEL_SCROLL_X, CHANNEL_SCROLL_Y]
    }

    fn start_track(
        &mut self,
        key: TrackKey<TrackTarget>,
        host: &mut dyn TransitionHost<TrackTarget>,
    ) -> Result<(), StartTrackError<TrackTarget>> {
        let axis = match key.channel {
            CHANNEL_SCROLL_X => ScrollAxis::X,
            CHANNEL_SCROLL_Y => ScrollAxis::Y,
            _ => return Err(StartTrackError::ChannelNotRegistered(key.channel)),
        };
        self.start_scroll_track(host, key.target, axis, 0.0, 0.0, ScrollTransition::new(0))
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

            if elapsed_seconds < delay {
                continue;
            }

            let Some(progress) = normalized_timeline_progress(elapsed_seconds, delay, duration)
            else {
                continue;
            };
            let eased = state.transition.timing.sample(progress);
            let value = state.from + (state.to - state.from) * eased;
            let axis = if key.channel == CHANNEL_SCROLL_X {
                ScrollAxis::X
            } else {
                ScrollAxis::Y
            };
            self.frame_samples.push(ScrollSample {
                target: key.target,
                axis,
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
