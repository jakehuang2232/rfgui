use super::*;

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
fn start_layout_track_retarget_uses_current_value_as_from() {
    let mut plugin = LayoutTransitionPlugin::new();
    let mut host = TestHost::with_channels(&[
        CHANNEL_LAYOUT_X,
        CHANNEL_LAYOUT_Y,
        CHANNEL_LAYOUT_WIDTH,
        CHANNEL_LAYOUT_HEIGHT,
    ]);

    let target = 7_u64;
    let field = LayoutField::X;

    plugin
        .start_layout_track(
            &mut host,
            target,
            field,
            0.0,
            100.0,
            LayoutTransition::new(1_000),
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
        .get(&TrackKey {
            target,
            channel: field.channel_id(),
        })
        .expect("track should exist after first frame")
        .current;
    plugin
        .start_layout_track(
            &mut host,
            target,
            field,
            10.0,
            20.0,
            LayoutTransition::new(250),
        )
        .expect("second track should retarget");

    let key = TrackKey {
        target,
        channel: field.channel_id(),
    };
    let state = plugin
        .tracks
        .get(&key)
        .copied()
        .expect("track should exist after retarget");
    assert_eq!(plugin.tracks.len(), 1);
    assert!((state.from - current_before).abs() <= 0.0001);
    assert_eq!(state.to, 20.0);
    assert_eq!(state.transition.duration_ms, 250);
    assert!(state.started_at_seconds.is_none());
}

#[test]
fn start_layout_track_keeps_existing_when_destination_unchanged() {
    let mut plugin = LayoutTransitionPlugin::new();
    let mut host = TestHost::with_channels(&[
        CHANNEL_LAYOUT_X,
        CHANNEL_LAYOUT_Y,
        CHANNEL_LAYOUT_WIDTH,
        CHANNEL_LAYOUT_HEIGHT,
    ]);

    let target = 7_u64;
    let field = LayoutField::X;

    plugin
        .start_layout_track(
            &mut host,
            target,
            field,
            1.0,
            100.0,
            LayoutTransition::new(1_000),
        )
        .expect("first track should start");
    plugin
        .start_layout_track(
            &mut host,
            target,
            field,
            50.0,
            100.0,
            LayoutTransition::new(250),
        )
        .expect("second track with same destination should be ignored");

    let key = TrackKey {
        target,
        channel: field.channel_id(),
    };
    let state = plugin
        .tracks
        .get(&key)
        .copied()
        .expect("track should exist");
    assert_eq!(plugin.tracks.len(), 1);
    assert_eq!(state.from, 1.0);
    assert_eq!(state.to, 100.0);
    assert_eq!(state.transition.duration_ms, 1_000);
}

#[test]
fn updating_x_track_does_not_restart_y_track_timeline() {
    let mut plugin = LayoutTransitionPlugin::new();
    let mut host = TestHost::with_channels(&[
        CHANNEL_LAYOUT_X,
        CHANNEL_LAYOUT_Y,
        CHANNEL_LAYOUT_WIDTH,
        CHANNEL_LAYOUT_HEIGHT,
    ]);
    let target = 42_u64;

    plugin
        .start_layout_track(
            &mut host,
            target,
            LayoutField::X,
            0.0,
            100.0,
            LayoutTransition::new(1_000),
        )
        .expect("x track should start");
    plugin
        .start_layout_track(
            &mut host,
            target,
            LayoutField::Y,
            0.0,
            200.0,
            LayoutTransition::new(1_000),
        )
        .expect("y track should start");

    plugin.run_tracks(
        TransitionFrame {
            dt_seconds: 0.016,
            now_seconds: 1.0,
        },
        &mut host,
    );

    let x_key = TrackKey {
        target,
        channel: CHANNEL_LAYOUT_X,
    };
    let y_key = TrackKey {
        target,
        channel: CHANNEL_LAYOUT_Y,
    };
    let y_started_before = plugin
        .tracks
        .get(&y_key)
        .expect("y track should exist")
        .started_at_seconds;
    assert!(y_started_before.is_some());

    plugin
        .start_layout_track(
            &mut host,
            target,
            LayoutField::X,
            50.0,
            120.0,
            LayoutTransition::new(1_000),
        )
        .expect("updating x track should succeed");

    let y_started_after = plugin
        .tracks
        .get(&y_key)
        .expect("y track should exist after x update")
        .started_at_seconds;
    assert_eq!(y_started_after, y_started_before);
    assert!(
        plugin
            .tracks
            .get(&x_key)
            .expect("x track should exist after update")
            .started_at_seconds
            .is_none()
    );
}
