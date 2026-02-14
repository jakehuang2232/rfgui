use std::collections::HashMap;

use super::{
    ChannelId, ClaimMode, RunResult, StartTrackError, TimeFunction, TrackKey, TrackTarget,
    Transition, TransitionFrame, TransitionHost, TransitionPluginId, normalized_timeline_progress,
};

pub const CHANNEL_LAYOUT_X: ChannelId = ChannelId(20_001);
pub const CHANNEL_LAYOUT_Y: ChannelId = ChannelId(20_002);
pub const CHANNEL_LAYOUT_WIDTH: ChannelId = ChannelId(20_003);
pub const CHANNEL_LAYOUT_HEIGHT: ChannelId = ChannelId(20_004);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutField {
    X,
    Y,
    Width,
    Height,
}

impl LayoutField {
    pub const fn channel_id(self) -> ChannelId {
        match self {
            Self::X => CHANNEL_LAYOUT_X,
            Self::Y => CHANNEL_LAYOUT_Y,
            Self::Width => CHANNEL_LAYOUT_WIDTH,
            Self::Height => CHANNEL_LAYOUT_HEIGHT,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutTransition {
    pub duration_ms: u32,
    pub delay_ms: u32,
    pub timing: TimeFunction,
}

impl LayoutTransition {
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
pub struct LayoutSample {
    pub target: TrackTarget,
    pub field: LayoutField,
    pub value: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutTrackRequest {
    pub target: TrackTarget,
    pub field: LayoutField,
    pub from: f32,
    pub to: f32,
    pub transition: LayoutTransition,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LayoutTrackState {
    from: f32,
    to: f32,
    elapsed_seconds: f32,
    transition: LayoutTransition,
}

#[derive(Debug)]
pub struct LayoutTransitionPlugin {
    plugin_id: TransitionPluginId,
    tracks: HashMap<TrackKey<TrackTarget>, LayoutTrackState>,
    frame_samples: Vec<LayoutSample>,
}

impl Default for LayoutTransitionPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutTransitionPlugin {
    pub const BUILTIN_PLUGIN_ID: TransitionPluginId = TransitionPluginId(2);

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

    pub fn start_layout_track(
        &mut self,
        host: &mut dyn TransitionHost<TrackTarget>,
        target: TrackTarget,
        field: LayoutField,
        from: f32,
        to: f32,
        transition: LayoutTransition,
    ) -> Result<(), StartTrackError<TrackTarget>> {
        let key = TrackKey {
            target,
            channel: field.channel_id(),
        };
        if !host.is_channel_registered(key.channel) {
            return Err(StartTrackError::ChannelNotRegistered(key.channel));
        }
        if !host.claim_track(self.plugin_id, key, ClaimMode::Replace) {
            return Err(StartTrackError::ClaimRejected(key));
        }
        self.tracks.insert(
            key,
            LayoutTrackState {
                from,
                to,
                elapsed_seconds: 0.0,
                transition,
            },
        );
        Ok(())
    }

    pub fn take_samples(&mut self) -> Vec<LayoutSample> {
        std::mem::take(&mut self.frame_samples)
    }
}

impl Transition<TrackTarget> for LayoutTransitionPlugin {
    fn plugin_id(&self) -> TransitionPluginId {
        self.plugin_id
    }

    fn observed_channels(&self, _target: TrackTarget) -> Vec<ChannelId> {
        vec![
            CHANNEL_LAYOUT_X,
            CHANNEL_LAYOUT_Y,
            CHANNEL_LAYOUT_WIDTH,
            CHANNEL_LAYOUT_HEIGHT,
        ]
    }

    fn start_track(
        &mut self,
        key: TrackKey<TrackTarget>,
        host: &mut dyn TransitionHost<TrackTarget>,
    ) -> Result<(), StartTrackError<TrackTarget>> {
        let field = match key.channel {
            CHANNEL_LAYOUT_X => LayoutField::X,
            CHANNEL_LAYOUT_Y => LayoutField::Y,
            CHANNEL_LAYOUT_WIDTH => LayoutField::Width,
            CHANNEL_LAYOUT_HEIGHT => LayoutField::Height,
            _ => return Err(StartTrackError::ChannelNotRegistered(key.channel)),
        };
        self.start_layout_track(host, key.target, field, 0.0, 0.0, LayoutTransition::new(0))
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
            state.elapsed_seconds = (state.elapsed_seconds + frame.dt_seconds.max(0.0)).max(0.0);
            let delay = (state.transition.delay_ms as f32) * 0.001;
            let duration = (state.transition.duration_ms as f32) * 0.001;
            let Some(progress) =
                normalized_timeline_progress(state.elapsed_seconds, delay, duration)
            else {
                continue;
            };
            let eased = state.transition.timing.sample(progress);
            let value = state.from + (state.to - state.from) * eased;
            let field = match key.channel {
                CHANNEL_LAYOUT_X => LayoutField::X,
                CHANNEL_LAYOUT_Y => LayoutField::Y,
                CHANNEL_LAYOUT_WIDTH => LayoutField::Width,
                CHANNEL_LAYOUT_HEIGHT => LayoutField::Height,
                _ => continue,
            };
            self.frame_samples.push(LayoutSample {
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
            needs_layout: !self.frame_samples.is_empty(),
            needs_paint: false,
            keep_running: !self.tracks.is_empty(),
        }
    }
}
