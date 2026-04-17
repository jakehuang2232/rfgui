use super::*;

pub(super) struct TransitionHostAdapter<'a> {
    pub(super) registered_channels: &'a FxHashSet<ChannelId>,
    pub(super) claims: &'a mut FxHashMap<TrackKey<TrackTarget>, TransitionPluginId>,
}

impl TransitionHost<TrackTarget> for TransitionHostAdapter<'_> {
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

impl Viewport {
    pub(super) fn invalidate_promoted_layer_reuse(&mut self) {
        self.compositor.promoted_base_signatures.clear();
        self.compositor.promoted_composition_signatures.clear();
        self.compositor.debug_previous_subtree_signatures.clear();
        self.compositor.promoted_layer_updates.clear();
        self.compositor.promoted_reuse_cooldown_frames = Self::PROMOTED_REUSE_COOLDOWN_FRAMES;
    }

    pub(super) fn is_style_driven_transition_channel(channel: ChannelId) -> bool {
        matches!(
            channel,
            CHANNEL_VISUAL_X
                | CHANNEL_VISUAL_Y
                | CHANNEL_LAYOUT_X
                | CHANNEL_LAYOUT_Y
                | CHANNEL_LAYOUT_WIDTH
                | CHANNEL_LAYOUT_HEIGHT
                | CHANNEL_STYLE_OPACITY
                | CHANNEL_STYLE_BORDER_RADIUS
                | CHANNEL_STYLE_BACKGROUND_COLOR
                | CHANNEL_STYLE_COLOR
                | CHANNEL_STYLE_BORDER_TOP_COLOR
                | CHANNEL_STYLE_BORDER_RIGHT_COLOR
                | CHANNEL_STYLE_BORDER_BOTTOM_COLOR
                | CHANNEL_STYLE_BORDER_LEFT_COLOR
                | CHANNEL_STYLE_TRANSFORM
                | CHANNEL_STYLE_TRANSFORM_ORIGIN
                | CHANNEL_STYLE_BOX_SHADOW
        )
    }

    pub(super) fn cancel_track_by_owner(&mut self, key: TrackKey<TrackTarget>) -> bool {
        let Some(owner) = self.transitions.transition_claims.get(&key).copied() else {
            return false;
        };
        let mut host = TransitionHostAdapter {
            registered_channels: &self.transitions.transition_channels,
            claims: &mut self.transitions.transition_claims,
        };
        if owner == ScrollTransitionPlugin::BUILTIN_PLUGIN_ID {
            self.transitions.scroll_transition_plugin.cancel_track(key, &mut host);
            return true;
        }
        if owner == LayoutTransitionPlugin::BUILTIN_PLUGIN_ID {
            self.transitions.layout_transition_plugin.cancel_track(key, &mut host);
            return true;
        }
        if owner == StyleTransitionPlugin::BUILTIN_PLUGIN_ID {
            self.transitions.style_transition_plugin.cancel_track(key, &mut host);
            return true;
        }
        if owner == VisualTransitionPlugin::BUILTIN_PLUGIN_ID {
            self.transitions.visual_transition_plugin.cancel_track(key, &mut host);
            return true;
        }
        self.transitions.transition_claims.remove(&key);
        false
    }

    pub(super) fn cancel_disallowed_transition_tracks(
        &mut self,
        roots: &[Box<dyn crate::view::base_component::ElementTrait>],
    ) -> bool {
        let allowlist = crate::view::base_component::collect_transition_track_allowlist(roots);
        let active_keys = self.transitions.transition_claims.keys().copied().collect::<Vec<_>>();
        let mut canceled = false;
        for key in active_keys {
            if !Self::is_style_driven_transition_channel(key.channel) {
                continue;
            }
            if allowlist.contains(&key) {
                continue;
            }
            canceled |= self.cancel_track_by_owner(key);
        }
        canceled
    }

    pub(super) fn present_mode_from_env() -> wgpu::PresentMode {
        let Ok(raw) = std::env::var("RFGUI_PRESENT_MODE") else {
            return wgpu::PresentMode::AutoVsync;
        };
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto_novsync" | "auto-no-vsync" | "no_vsync" | "no-vsync" | "novsync" => {
                wgpu::PresentMode::AutoNoVsync
            }
            "fifo" => wgpu::PresentMode::Fifo,
            "mailbox" => wgpu::PresentMode::Mailbox,
            "immediate" => wgpu::PresentMode::Immediate,
            _ => wgpu::PresentMode::AutoVsync,
        }
    }

    pub(super) fn alpha_mode_from_capabilities(
        alpha_modes: &[wgpu::CompositeAlphaMode],
    ) -> wgpu::CompositeAlphaMode {
        for preferred in [
            wgpu::CompositeAlphaMode::PostMultiplied,
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::Inherit,
            wgpu::CompositeAlphaMode::Auto,
            wgpu::CompositeAlphaMode::Opaque,
        ] {
            if alpha_modes.contains(&preferred) {
                return preferred;
            }
        }
        wgpu::CompositeAlphaMode::Auto
    }
    fn sync_layout_transition_claims(&mut self) {
        let active_keys = self
            .transitions
            .layout_transition_plugin
            .active_track_keys()
            .into_iter()
            .collect::<FxHashSet<_>>();
        self.transitions.transition_claims.retain(|key, owner| {
            if !matches!(
                key.channel,
                CHANNEL_LAYOUT_X | CHANNEL_LAYOUT_Y | CHANNEL_LAYOUT_WIDTH | CHANNEL_LAYOUT_HEIGHT
            ) {
                return true;
            }
            let _ = owner;
            active_keys.contains(key)
        });
    }

    fn trace_style_sample_apply(
        &self,
        roots: &[Box<dyn crate::view::base_component::ElementTrait>],
        target: u64,
        field: StyleField,
        value: StyleValue,
        applied: bool,
        before_signatures: Option<(u64, u64)>,
    ) {
        if !self.debug_options.trace_reuse_path {
            return;
        }
        let promoted_root = roots.iter().find_map(|root| {
            let root_id = root.id();
            if !self.compositor.promotion_state.promoted_node_ids.contains(&root_id) {
                return None;
            }
            if root_id == target
                || crate::view::base_component::subtree_contains_node(root.as_ref(), root_id, target)
            {
                Some(root_id)
            } else {
                None
            }
        });
        let state = roots.iter().rev().find_map(|root| {
            crate::view::base_component::get_debug_element_render_state_by_id(root.as_ref(), target)
        });
        let ancestry = roots
            .iter()
            .rev()
            .find_map(|root| crate::view::base_component::get_node_ancestry_ids(root.as_ref(), target));
        let after_signatures = roots.iter().rev().find_map(|root| {
            crate::view::base_component::get_debug_promotion_signatures_by_id(root.as_ref(), target)
        });
        let state_desc = match state {
            Some(state) => format!(
                "bg=rgba({},{},{},{}) fg=rgba({},{},{},{}) opacity={:.3} border_radius={:.3}",
                state.background_rgba[0],
                state.background_rgba[1],
                state.background_rgba[2],
                state.background_rgba[3],
                state.foreground_rgba[0],
                state.foreground_rgba[1],
                state.foreground_rgba[2],
                state.foreground_rgba[3],
                state.opacity,
                state.border_radius,
            ),
            None => "state=missing".to_string(),
        };
        let promoted_root_desc = promoted_root
            .map(|node_id| format!("promoted_root={node_id}"))
            .unwrap_or_else(|| "promoted_root=none".to_string());
        let signature_desc = match (before_signatures, after_signatures) {
            (Some((before_self, before_clip)), Some((after_self, after_clip))) => format!(
                "sig_self={}=>{} sig_clip={}=>{}",
                before_self, after_self, before_clip, after_clip
            ),
            (None, Some((after_self, after_clip))) => {
                format!(
                    "sig_self=missing=>{} sig_clip=missing=>{}",
                    after_self, after_clip
                )
            }
            (Some((before_self, before_clip)), None) => {
                format!(
                    "sig_self={}=>missing sig_clip={}=>missing",
                    before_self, before_clip
                )
            }
            (None, None) => "sig=missing".to_string(),
        };
        let ancestry_desc = ancestry
            .map(|path| {
                let joined = path
                    .into_iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join("->");
                format!("ancestry={joined}")
            })
            .unwrap_or_else(|| "ancestry=missing".to_string());
        record_debug_style_sample_record(DebugStyleSampleRecord {
            target,
            promoted_root,
        });
        record_debug_style_sample(format!(
            "node={} field={} sample={} applied={} {} {} {} {}",
            target,
            format_style_field(field),
            format_style_value(&value),
            applied,
            promoted_root_desc,
            ancestry_desc,
            signature_desc,
            state_desc,
        ));
    }

    pub(super) fn start_scroll_track(
        &mut self,
        target: TrackTarget,
        axis: ScrollAxis,
        from: f32,
        to: f32,
    ) -> bool {
        if (to - from).abs() <= 0.001 {
            return false;
        }
        let mut host = TransitionHostAdapter {
            registered_channels: &self.transitions.transition_channels,
            claims: &mut self.transitions.transition_claims,
        };
        if self
            .transitions
            .scroll_transition_plugin
            .start_scroll_track(&mut host, target, axis, from, to, self.transitions.scroll_transition)
            .is_err()
        {
            return false;
        }
        self.request_redraw();
        true
    }

    pub(super) fn cancel_scroll_track(&mut self, target: TrackTarget, axis: ScrollAxis) {
        let key = TrackKey {
            target,
            channel: axis.channel_id(),
        };
        let mut host = TransitionHostAdapter {
            registered_channels: &self.transitions.transition_channels,
            claims: &mut self.transitions.transition_claims,
        };
        self.transitions.scroll_transition_plugin.cancel_track(key, &mut host);
    }

    fn apply_scroll_sample(
        roots: &mut [Box<dyn crate::view::base_component::ElementTrait>],
        target: TrackTarget,
        axis: ScrollAxis,
        value: f32,
    ) -> bool {
        for root in roots.iter_mut().rev() {
            if let Some((x, y)) =
                crate::view::base_component::get_scroll_offset_by_id(root.as_ref(), target)
            {
                let next = match axis {
                    ScrollAxis::X => (value, y),
                    ScrollAxis::Y => (x, value),
                };
                return crate::view::base_component::set_scroll_offset_by_id(root.as_mut(), target, next);
            }
        }
        false
    }

    pub(super) fn transition_timing(&mut self) -> (f32, f64) {
        let now = Instant::now();
        let dt = self
            .transitions
            .last_transition_tick
            .map(|last| (now - last).as_secs_f32())
            .unwrap_or(0.0);
        self.transitions.last_transition_tick = Some(now);
        let epoch = self.transitions.transition_epoch.get_or_insert(now);
        let now_seconds = (now - *epoch).as_secs_f64();
        (dt, now_seconds)
    }

    pub(super) fn run_pre_layout_transitions(
        &mut self,
        roots: &mut [Box<dyn crate::view::base_component::ElementTrait>],
        dt: f32,
        now_seconds: f64,
    ) -> bool {
        let mut layout_requests = Vec::new();
        for root in roots.iter_mut() {
            crate::view::base_component::take_layout_transition_requests(
                root.as_mut(),
                &mut layout_requests,
            );
        }
        if !layout_requests.is_empty() {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            for request in layout_requests {
                let _ = self.transitions.layout_transition_plugin.start_layout_track(
                    &mut host,
                    request.target,
                    request.field,
                    request.from,
                    request.to,
                    request.transition,
                );
            }
        }
        let layout_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.layout_transition_plugin.run_tracks(
                TransitionFrame {
                    dt_seconds: dt,
                    now_seconds,
                },
                &mut host,
            )
        };
        self.sync_layout_transition_claims();
        let mut changed = false;
        let layout_samples = self.transitions.layout_transition_plugin.take_samples();
        for sample in layout_samples {
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::set_layout_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value.clone(),
                ) {
                    changed = true;
                    break;
                }
            }
        }
        if layout_result.keep_running {
            self.request_redraw();
        }
        changed || layout_result.keep_running
    }

    pub(super) fn run_post_layout_transitions(
        &mut self,
        roots: &mut [Box<dyn crate::view::base_component::ElementTrait>],
        dt: f32,
        now_seconds: f64,
    ) -> PostLayoutTransitionResult {
        let live_node_ids = crate::view::base_component::collect_node_id_allowlist(roots);
        self.transitions.animation_plugin.prune_targets(&live_node_ids);
        let mut animation_requests = Vec::new();
        for root in roots.iter_mut() {
            crate::view::base_component::take_animation_requests(root.as_mut(), &mut animation_requests);
        }
        let mut style_requests = Vec::new();
        for root in roots.iter_mut() {
            crate::view::base_component::take_style_transition_requests(
                root.as_mut(),
                &mut style_requests,
            );
        }
        let mut layout_requests = Vec::new();
        for root in roots.iter_mut() {
            crate::view::base_component::take_layout_transition_requests(
                root.as_mut(),
                &mut layout_requests,
            );
        }
        let mut visual_requests = Vec::new();
        for root in roots.iter_mut() {
            crate::view::base_component::take_visual_transition_requests(
                root.as_mut(),
                &mut visual_requests,
            );
        }
        for request in animation_requests {
            self.transitions.animation_plugin.start_animator(request);
        }
        if !style_requests.is_empty() {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            for request in style_requests {
                if self.debug_options.trace_reuse_path {
                    record_debug_style_request(format!(
                        "target={} field={} from={} to={} duration_ms={} delay_ms={}",
                        request.target,
                        format_style_field(request.field),
                        format_style_value(&request.from),
                        format_style_value(&request.to),
                        request.transition.duration_ms,
                        request.transition.delay_ms,
                    ));
                }
                let _ = self.transitions.style_transition_plugin.start_style_track(
                    &mut host,
                    request.target,
                    request.field,
                    request.from,
                    request.to,
                    request.transition,
                );
            }
        }
        if !layout_requests.is_empty() {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            for request in layout_requests {
                let _ = self.transitions.layout_transition_plugin.start_layout_track(
                    &mut host,
                    request.target,
                    request.field,
                    request.from,
                    request.to,
                    request.transition,
                );
            }
        }
        if !visual_requests.is_empty() {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            for request in visual_requests {
                let _ = self.transitions.visual_transition_plugin.start_visual_track(
                    &mut host,
                    request.target,
                    request.field,
                    request.from,
                    request.to,
                    request.transition,
                );
            }
        }

        let scroll_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.scroll_transition_plugin.run_tracks(
                TransitionFrame {
                    dt_seconds: dt,
                    now_seconds,
                },
                &mut host,
            )
        };
        let style_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.style_transition_plugin.run_tracks(
                TransitionFrame {
                    dt_seconds: dt,
                    now_seconds,
                },
                &mut host,
            )
        };
        let animation_result = self.transitions.animation_plugin.run_animations(dt, now_seconds);
        let visual_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.visual_transition_plugin.run_tracks(
                TransitionFrame {
                    dt_seconds: dt,
                    now_seconds,
                },
                &mut host,
            )
        };
        let layout_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.layout_transition_plugin.run_tracks(
                TransitionFrame {
                    dt_seconds: dt,
                    now_seconds,
                },
                &mut host,
            )
        };
        self.sync_layout_transition_claims();
        let samples = self.transitions.scroll_transition_plugin.take_samples();
        let mut redraw_changed = false;
        let mut relayout_required = false;
        for sample in samples {
            redraw_changed |=
                Self::apply_scroll_sample(roots, sample.target, sample.axis, sample.value);
        }
        let style_samples = self.transitions.style_transition_plugin.take_samples();
        for sample in style_samples {
            let before_signatures = roots.iter().rev().find_map(|root| {
                crate::view::base_component::get_debug_promotion_signatures_by_id(
                    root.as_ref(),
                    sample.target,
                )
            });
            let mut applied = false;
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::set_style_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value.clone(),
                ) {
                    redraw_changed = true;
                    if style_field_requires_relayout(sample.field) {
                        relayout_required = true;
                    }
                    applied = true;
                    break;
                }
            }
            self.trace_style_sample_apply(
                roots,
                sample.target,
                sample.field,
                sample.value,
                applied,
                before_signatures,
            );
        }
        let animation_style_samples = self.transitions.animation_plugin.take_style_samples();
        for sample in animation_style_samples {
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::set_style_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value.clone(),
                ) {
                    redraw_changed = true;
                    if style_field_requires_relayout(sample.field) {
                        relayout_required = true;
                    }
                    break;
                }
            }
        }
        let visual_samples = self.transitions.visual_transition_plugin.take_samples();
        for sample in visual_samples {
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::set_visual_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value.clone(),
                ) {
                    redraw_changed = true;
                    break;
                }
            }
        }
        let layout_samples = self.transitions.layout_transition_plugin.take_samples();
        for sample in layout_samples {
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::set_layout_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value,
                ) {
                    redraw_changed = true;
                    relayout_required = true;
                    break;
                }
            }
        }
        let animation_layout_samples = self.transitions.animation_plugin.take_layout_samples();
        for sample in animation_layout_samples {
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::set_layout_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value,
                ) {
                    redraw_changed = true;
                    relayout_required = true;
                    break;
                }
            }
        }
        if scroll_result.keep_running
            || style_result.keep_running
            || animation_result.keep_running
            || visual_result.keep_running
            || layout_result.keep_running
        {
            self.request_redraw();
            self.is_animating = true;
        }
        PostLayoutTransitionResult {
            redraw_changed: redraw_changed
                || scroll_result.keep_running
                || style_result.keep_running
                || animation_result.keep_running
                || visual_result.keep_running
                || layout_result.keep_running,
            relayout_required,
        }
    }

    pub(super) fn sync_inflight_transition_state(
        &mut self,
        roots: &mut [Box<dyn crate::view::base_component::ElementTrait>],
    ) -> bool {
        let live_node_ids = crate::view::base_component::collect_node_id_allowlist(roots);
        self.transitions.animation_plugin.prune_targets(&live_node_ids);
        let now = Instant::now();
        let epoch = self.transitions.transition_epoch.get_or_insert(now);
        let frame = TransitionFrame {
            dt_seconds: 0.0,
            now_seconds: (now - *epoch).as_secs_f64(),
        };
        let scroll_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.scroll_transition_plugin.run_tracks(frame, &mut host)
        };
        let style_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.style_transition_plugin.run_tracks(frame, &mut host)
        };
        let animation_result = self.transitions.animation_plugin.run_animations(0.0, frame.now_seconds);
        let visual_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.visual_transition_plugin.run_tracks(frame, &mut host)
        };
        let layout_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.layout_transition_plugin.run_tracks(frame, &mut host)
        };
        self.sync_layout_transition_claims();

        let mut changed = false;
        for sample in self.transitions.scroll_transition_plugin.take_samples() {
            changed |= Self::apply_scroll_sample(roots, sample.target, sample.axis, sample.value);
        }
        for sample in self.transitions.style_transition_plugin.take_samples() {
            let before_signatures = roots.iter().rev().find_map(|root| {
                crate::view::base_component::get_debug_promotion_signatures_by_id(
                    root.as_ref(),
                    sample.target,
                )
            });
            let mut applied = false;
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::set_style_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value.clone(),
                ) {
                    changed = true;
                    applied = true;
                    break;
                }
            }
            self.trace_style_sample_apply(
                roots,
                sample.target,
                sample.field,
                sample.value,
                applied,
                before_signatures,
            );
        }
        for sample in self.transitions.animation_plugin.take_style_samples() {
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::set_style_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value.clone(),
                ) {
                    changed = true;
                    break;
                }
            }
        }
        for sample in self.transitions.visual_transition_plugin.take_samples() {
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::set_visual_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value,
                ) {
                    changed = true;
                    break;
                }
            }
        }
        for sample in self.transitions.layout_transition_plugin.take_samples() {
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::set_layout_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value,
                ) {
                    changed = true;
                    break;
                }
            }
        }
        for sample in self.transitions.animation_plugin.take_layout_samples() {
            for root in roots.iter_mut().rev() {
                if crate::view::base_component::set_layout_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value,
                ) {
                    changed = true;
                    break;
                }
            }
        }

        changed
            || scroll_result.keep_running
            || style_result.keep_running
            || animation_result.keep_running
            || visual_result.keep_running
            || layout_result.keep_running
    }
}
