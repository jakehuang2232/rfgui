use std::collections::HashMap;

use super::{
    ChannelId, ClaimMode, RunResult, StartTrackError, TimeFunction, TrackKey, TrackTarget,
    Transition, TransitionFrame, TransitionHost, TransitionPluginId, elapsed_seconds_from_frame,
    normalized_timeline_progress,
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
    current: f32,
    started_at_seconds: Option<f64>,
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
        let mut next_from = from;
        if let Some(existing) = self.tracks.get(&key) {
            let same_to = (existing.to - to).abs() <= 0.0001;
            if same_to {
                return Ok(());
            }
            next_from = existing.current;
        }
        if !host.is_channel_registered(key.channel) {
            return Err(StartTrackError::ChannelNotRegistered(key.channel));
        }
        if !host.claim_track(self.plugin_id, key, ClaimMode::Replace) {
            return Err(StartTrackError::ClaimRejected(key));
        }
        self.tracks.insert(
            key,
            VisualTrackState {
                from: next_from,
                to,
                current: next_from,
                started_at_seconds: None,
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
            let elapsed_seconds = elapsed_seconds_from_frame(frame, &mut state.started_at_seconds);
            let delay = (state.transition.delay_ms as f32) * 0.001;
            let duration = (state.transition.duration_ms as f32) * 0.001;
            let Some(progress) = normalized_timeline_progress(elapsed_seconds, delay, duration)
            else {
                continue;
            };
            let eased = state.transition.timing.sample(progress);
            let value = state.from + (state.to - state.from) * eased;
            state.current = value;
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
                state.current = state.to;
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
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn transition(duration_ms: u32) -> VisualTransition {
        VisualTransition {
            duration_ms,
            delay_ms: 0,
            timing: TimeFunction::EaseOut,
        }
    }

    struct TestHost {
        registered_channels: HashSet<ChannelId>,
        claims: HashMap<TrackKey<TrackTarget>, TransitionPluginId>,
    }

    impl TestHost {
        fn with_channels(channels: &[ChannelId]) -> Self {
            Self {
                registered_channels: channels.iter().copied().collect(),
                claims: HashMap::new(),
            }
        }
    }

    impl TransitionHost<TrackTarget> for TestHost {
        fn is_channel_registered(&self, channel: ChannelId) -> bool {
            self.registered_channels.contains(&channel)
        }

        fn claim_track(
            &mut self,
            plugin_id: TransitionPluginId,
            key: TrackKey<TrackTarget>,
            mode: ClaimMode,
        ) -> bool {
            if let Some(current) = self.claims.get(&key).copied() {
                if current == plugin_id {
                    return true;
                }
                if matches!(mode, ClaimMode::Replace) {
                    self.claims.insert(key, plugin_id);
                    return true;
                }
                return false;
            }
            self.claims.insert(key, plugin_id);
            true
        }

        fn release_track_claim(
            &mut self,
            plugin_id: TransitionPluginId,
            key: TrackKey<TrackTarget>,
        ) {
            if self.claims.get(&key).copied() == Some(plugin_id) {
                self.claims.remove(&key);
            }
        }

        fn release_all_claims(&mut self, plugin_id: TransitionPluginId) {
            self.claims.retain(|_, owner| *owner != plugin_id);
        }
    }

    #[test]
    fn start_visual_track_keeps_existing_when_destination_unchanged() {
        let mut plugin = VisualTransitionPlugin::new();
        let mut host = TestHost::with_channels(&[CHANNEL_VISUAL_X, CHANNEL_VISUAL_Y]);
        let target = 7_u64;
        let field = VisualField::Y;

        plugin
            .start_visual_track(
                &mut host,
                target,
                field,
                -5.0,
                0.0,
                transition(1_000),
            )
            .expect("first track should start");
        plugin
            .start_visual_track(
                &mut host,
                target,
                field,
                -100.0,
                0.0,
                transition(250),
            )
            .expect("same destination should be ignored");

        let key = TrackKey {
            target,
            channel: field.channel_id(),
        };
        let state = plugin
            .tracks
            .get(&key)
            .copied()
            .expect("track should exist");
        assert_eq!(state.from, -5.0);
        assert_eq!(state.to, 0.0);
        assert_eq!(state.transition.duration_ms, 1_000);
    }

    #[test]
    fn start_visual_track_retarget_uses_current_value_as_from() {
        let mut plugin = VisualTransitionPlugin::new();
        let mut host = TestHost::with_channels(&[CHANNEL_VISUAL_X, CHANNEL_VISUAL_Y]);
        let target = 42_u64;
        let field = VisualField::X;
        let key = TrackKey {
            target,
            channel: field.channel_id(),
        };

        plugin
            .start_visual_track(
                &mut host,
                target,
                field,
                0.0,
                100.0,
                transition(1_000),
            )
            .expect("first track should start");
        plugin.run_tracks(
            TransitionFrame {
                dt_seconds: 0.016,
                now_seconds: 1.0,
            },
            &mut host,
        );
        let current_before = plugin
            .tracks
            .get(&key)
            .expect("track should exist after first frame")
            .current;

        plugin
            .start_visual_track(
                &mut host,
                target,
                field,
                10.0,
                20.0,
                transition(500),
            )
            .expect("second track should retarget");

        let state = plugin
            .tracks
            .get(&key)
            .copied()
            .expect("track should exist after retarget");
        assert!((state.from - current_before).abs() <= 0.0001);
        assert_eq!(state.to, 20.0);
        assert_eq!(state.transition.duration_ms, 500);
        assert!(state.started_at_seconds.is_none());
    }
}
