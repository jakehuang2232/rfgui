use super::*;

fn transition(duration_ms: u32) -> VisualTransition {
    VisualTransition {
        duration_ms,
        delay_ms: 0,
        timing: TimeFunction::EaseOut,
    }
}

struct TestHost {
    registered_channels: FxHashSet<ChannelId>,
    claims: FxHashMap<TrackKey<TrackTarget>, TransitionPluginId>,
}

impl TestHost {
    fn with_channels(channels: &[ChannelId]) -> Self {
        Self {
            registered_channels: channels.iter().copied().collect(),
            claims: FxHashMap::default(),
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

    fn release_track_claim(&mut self, plugin_id: TransitionPluginId, key: TrackKey<TrackTarget>) {
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
        .start_visual_track(&mut host, target, field, -5.0, 0.0, transition(1_000))
        .expect("first track should start");
    plugin
        .start_visual_track(&mut host, target, field, -100.0, 0.0, transition(250))
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
        .start_visual_track(&mut host, target, field, 0.0, 100.0, transition(1_000))
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
        .start_visual_track(&mut host, target, field, 10.0, 20.0, transition(500))
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

#[test]
fn start_visual_track_clears_finished_track_when_destination_unchanged() {
    let mut plugin = VisualTransitionPlugin::new();
    let mut host = TestHost::with_channels(&[CHANNEL_VISUAL_X, CHANNEL_VISUAL_Y]);
    let target = 9_u64;
    let field = VisualField::Y;
    let key = TrackKey {
        target,
        channel: field.channel_id(),
    };

    plugin
        .start_visual_track(&mut host, target, field, -12.0, 0.0, transition(100))
        .expect("track should start");
    plugin.run_tracks(
        TransitionFrame {
            dt_seconds: 0.2,
            now_seconds: 1.0,
        },
        &mut host,
    );
    assert!(
        !plugin.tracks.contains_key(&key),
        "finished track should be removed"
    );

    plugin
        .start_visual_track(&mut host, target, field, -99.0, 0.0, transition(100))
        .expect("same destination after finish should be accepted");
    let state = plugin
        .tracks
        .get(&key)
        .copied()
        .expect("track should be recreated after prior finish");
    assert_eq!(state.from, -99.0);
    assert_eq!(state.to, 0.0);
}
