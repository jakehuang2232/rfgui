use std::collections::HashMap;

use super::{
    ChannelId, ClaimMode, RunResult, StartTrackError, TimeFunction, TrackKey, TrackTarget,
    Transition, TransitionFrame, TransitionHost, TransitionPluginId, normalized_timeline_progress,
};

pub const CHANNEL_VISUAL_X: ChannelId = ChannelId(21_001);
pub const CHANNEL_VISUAL_Y: ChannelId = ChannelId(21_002);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VisualField {
    X,
    Y,
}

impl VisualField {
    pub const fn channel_id(self) -> ChannelId {
        match self {
            Self::X => CHANNEL_VISUAL_X,
            Self::Y => CHANNEL_VISUAL_Y,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisualTransition {
    pub duration_ms: u32,
    pub delay_ms: u32,
    pub timing: TimeFunction,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisualSample {
    pub target: TrackTarget,
    pub field: VisualField,
    pub value: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisualTrackRequest {
    pub target: TrackTarget,
    pub field: VisualField,
    pub from: f32,
    pub to: f32,
    pub transition: VisualTransition,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct VisualTrackState {
    from: f32,
    to: f32,
    elapsed_seconds: f32,
    transition: VisualTransition,
}

#[derive(Debug)]
pub struct VisualTransitionPlugin {
    plugin_id: TransitionPluginId,
    tracks: HashMap<TrackKey<TrackTarget>, VisualTrackState>,
    frame_samples: Vec<VisualSample>,
}

impl Default for VisualTransitionPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl VisualTransitionPlugin {
    pub const BUILTIN_PLUGIN_ID: TransitionPluginId = TransitionPluginId(4);

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

    pub fn start_visual_track(
        &mut self,
        host: &mut dyn TransitionHost<TrackTarget>,
        target: TrackTarget,
        field: VisualField,
        from: f32,
        to: f32,
        transition: VisualTransition,
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
            VisualTrackState {
                from,
                to,
                elapsed_seconds: 0.0,
                transition,
            },
        );
        Ok(())
    }

    pub fn take_samples(&mut self) -> Vec<VisualSample> {
        std::mem::take(&mut self.frame_samples)
    }
}

impl Transition<TrackTarget> for VisualTransitionPlugin {
    fn plugin_id(&self) -> TransitionPluginId {
        self.plugin_id
    }

    fn observed_channels(&self, _target: TrackTarget) -> Vec<ChannelId> {
        vec![CHANNEL_VISUAL_X, CHANNEL_VISUAL_Y]
    }

    fn start_track(
        &mut self,
        key: TrackKey<TrackTarget>,
        host: &mut dyn TransitionHost<TrackTarget>,
    ) -> Result<(), StartTrackError<TrackTarget>> {
        let field = match key.channel {
            CHANNEL_VISUAL_X => VisualField::X,
            CHANNEL_VISUAL_Y => VisualField::Y,
            _ => return Err(StartTrackError::ChannelNotRegistered(key.channel)),
        };
        self.start_visual_track(
            host,
            key.target,
            field,
            0.0,
            0.0,
            VisualTransition {
                duration_ms: 0,
                delay_ms: 0,
                timing: TimeFunction::EaseOut,
            },
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
                CHANNEL_VISUAL_X => VisualField::X,
                CHANNEL_VISUAL_Y => VisualField::Y,
                _ => continue,
            };
            self.frame_samples.push(VisualSample {
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
