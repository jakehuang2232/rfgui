#![allow(missing_docs)]

//! Keyframe animation runtime used by the typed `animator` style property.

use std::collections::HashMap;
use std::collections::HashSet;

use super::{
    LayoutField, LayoutSample, RunResult, StyleField, StyleSample, StyleValue, TimeFunction,
};
use crate::TransitionTiming;
use crate::style::{
    Animator, Direction, FillMode, ParsedValue, PlayState, PropertyId, Repeat, Style,
};

#[derive(Clone, Debug, PartialEq)]
pub struct AnimationRequest {
    pub target: u64,
    pub animator: Animator,
}

#[derive(Clone, Debug, PartialEq)]
struct CompiledKeyframe {
    progress: f32,
    style_values: HashMap<StyleField, StyleValue>,
    layout_values: HashMap<LayoutField, f32>,
}

#[derive(Clone, Debug, PartialEq)]
struct ActiveAnimation {
    keyframes: Vec<CompiledKeyframe>,
    duration_ms: u32,
    delay_ms: i32,
    timing: TimeFunction,
    repeat: Repeat,
    direction: Direction,
    fill_mode: FillMode,
    play_state: PlayState,
    started_at_seconds: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
struct ActiveAnimator {
    target: u64,
    animator: Animator,
    animations: Vec<ActiveAnimation>,
}

#[derive(Debug, Default)]
pub struct AnimationPlugin {
    animators: HashMap<u64, ActiveAnimator>,
    completed_animators: HashMap<u64, ActiveAnimator>,
    style_samples: Vec<StyleSample>,
    layout_samples: Vec<LayoutSample>,
}

impl AnimationPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start_animator(&mut self, request: AnimationRequest) {
        if request.animator.is_empty() {
            self.animators.remove(&request.target);
            self.completed_animators.remove(&request.target);
            return;
        }
        if self
            .animators
            .get(&request.target)
            .is_some_and(|active| active.animator == request.animator)
        {
            return;
        }
        if self
            .completed_animators
            .get(&request.target)
            .is_some_and(|completed| completed.animator == request.animator)
        {
            return;
        }
        self.completed_animators.remove(&request.target);
        let animations = request
            .animator
            .animations()
            .iter()
            .map(|animation| ActiveAnimation {
                keyframes: compile_keyframes(animation.keyframes()),
                duration_ms: request.animator.resolved_duration_ms(animation),
                delay_ms: request.animator.resolved_delay_ms(animation),
                timing: map_animation_timing(request.animator.resolved_timing(animation)),
                repeat: request.animator.resolved_repeat(animation),
                direction: request.animator.resolved_direction(animation),
                fill_mode: request.animator.resolved_fill_mode(animation),
                play_state: request.animator.resolved_play_state(animation),
                started_at_seconds: None,
            })
            .collect();
        self.animators.insert(
            request.target,
            ActiveAnimator {
                target: request.target,
                animator: request.animator,
                animations,
            },
        );
    }

    pub fn take_style_samples(&mut self) -> Vec<StyleSample> {
        std::mem::take(&mut self.style_samples)
    }

    pub fn take_layout_samples(&mut self) -> Vec<LayoutSample> {
        std::mem::take(&mut self.layout_samples)
    }

    pub fn prune_targets(&mut self, keep_targets: &HashSet<u64>) {
        self.animators
            .retain(|target, _| keep_targets.contains(target));
        self.completed_animators
            .retain(|target, _| keep_targets.contains(target));
    }

    pub fn active_targets(&self) -> HashSet<u64> {
        self.animators.keys().copied().collect()
    }

    pub fn active_promotion_hints(&self) -> HashMap<u64, AnimationPromotionHint> {
        self.animators
            .iter()
            .map(|(&target, animator)| (target, AnimationPromotionHint::from(animator)))
            .collect()
    }

    pub fn run_animations(&mut self, dt_seconds: f32, now_seconds: f64) -> RunResult {
        self.style_samples.clear();
        self.layout_samples.clear();

        let mut merged_style = HashMap::<(u64, StyleField), StyleValue>::new();
        let mut merged_layout = HashMap::<(u64, LayoutField), f32>::new();
        let mut keep_running = false;
        let mut finished_targets = Vec::new();

        for (&target, animator) in &mut self.animators {
            let mut target_keep_running = false;
            for animation in &mut animator.animations {
                let Some(sample_progress) =
                    sample_animation_progress(animation, dt_seconds, now_seconds)
                else {
                    continue;
                };
                target_keep_running |= sample_progress.keep_running;
                for (field, value) in sample_style_fields(animation, sample_progress.progress) {
                    merged_style.insert((target, field), value);
                }
                for (field, value) in sample_layout_fields(animation, sample_progress.progress) {
                    merged_layout.insert((target, field), value);
                }
            }
            if target_keep_running {
                keep_running = true;
            } else {
                finished_targets.push(target);
            }
        }

        for target in finished_targets {
            if let Some(animator) = self.animators.remove(&target) {
                self.completed_animators.insert(target, animator);
            }
        }

        for (&target, animator) in &self.completed_animators {
            for animation in &animator.animations {
                let Some(progress) = sample_completed_fill_progress(animation) else {
                    continue;
                };
                for (field, value) in sample_style_fields(animation, progress) {
                    merged_style.insert((target, field), value);
                }
                for (field, value) in sample_layout_fields(animation, progress) {
                    merged_layout.insert((target, field), value);
                }
            }
        }

        self.style_samples = merged_style
            .into_iter()
            .map(|((target, field), value)| StyleSample {
                target,
                field,
                value,
            })
            .collect();
        self.layout_samples = merged_layout
            .into_iter()
            .map(|((target, field), value)| LayoutSample {
                target,
                field,
                value,
            })
            .collect();

        RunResult {
            needs_layout: !self.layout_samples.is_empty(),
            needs_paint: !self.style_samples.is_empty(),
            keep_running,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnimationPromotionHint {
    pub composite_only: bool,
}

impl From<&ActiveAnimator> for AnimationPromotionHint {
    fn from(animator: &ActiveAnimator) -> Self {
        let mut saw_style_field = false;
        let mut saw_non_composite_style = false;
        let mut saw_layout_field = false;

        for animation in &animator.animations {
            for keyframe in &animation.keyframes {
                if !keyframe.layout_values.is_empty() {
                    saw_layout_field = true;
                }
                for field in keyframe.style_values.keys() {
                    saw_style_field = true;
                    if !matches!(
                        field,
                        StyleField::Opacity | StyleField::Transform | StyleField::TransformOrigin
                    ) {
                        saw_non_composite_style = true;
                    }
                }
            }
        }

        Self {
            composite_only: saw_style_field && !saw_non_composite_style && !saw_layout_field,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ProgressSample {
    progress: f32,
    keep_running: bool,
}

fn compile_keyframes(keyframes: &[crate::Keyframe]) -> Vec<CompiledKeyframe> {
    keyframes
        .iter()
        .map(|keyframe| CompiledKeyframe {
            progress: keyframe.progress(),
            style_values: compile_style_fields(keyframe.style()),
            layout_values: compile_layout_fields(keyframe.style()),
        })
        .collect()
}

fn compile_style_fields(style: &Style) -> HashMap<StyleField, StyleValue> {
    let mut out = HashMap::new();
    for declaration in style.declarations() {
        match declaration.property {
            PropertyId::Opacity => {
                if let ParsedValue::Opacity(value) = &declaration.value {
                    out.insert(StyleField::Opacity, StyleValue::Scalar(value.value()));
                }
            }
            PropertyId::BackgroundColor => {
                if let ParsedValue::Color(value) = &declaration.value {
                    out.insert(
                        StyleField::BackgroundColor,
                        StyleValue::Color(value.to_color()),
                    );
                }
            }
            PropertyId::Color => {
                if let ParsedValue::Color(value) = &declaration.value {
                    out.insert(StyleField::Color, StyleValue::Color(value.to_color()));
                }
            }
            PropertyId::BorderTopColor => {
                if let ParsedValue::Color(value) = &declaration.value {
                    out.insert(
                        StyleField::BorderTopColor,
                        StyleValue::Color(value.to_color()),
                    );
                }
            }
            PropertyId::BorderRightColor => {
                if let ParsedValue::Color(value) = &declaration.value {
                    out.insert(
                        StyleField::BorderRightColor,
                        StyleValue::Color(value.to_color()),
                    );
                }
            }
            PropertyId::BorderBottomColor => {
                if let ParsedValue::Color(value) = &declaration.value {
                    out.insert(
                        StyleField::BorderBottomColor,
                        StyleValue::Color(value.to_color()),
                    );
                }
            }
            PropertyId::BorderLeftColor => {
                if let ParsedValue::Color(value) = &declaration.value {
                    out.insert(
                        StyleField::BorderLeftColor,
                        StyleValue::Color(value.to_color()),
                    );
                }
            }
            PropertyId::BoxShadow => {
                if let ParsedValue::BoxShadow(value) = &declaration.value {
                    out.insert(StyleField::BoxShadow, StyleValue::BoxShadow(value.clone()));
                }
            }
            PropertyId::Transform => {
                if let ParsedValue::Transform(value) = &declaration.value {
                    out.insert(StyleField::Transform, StyleValue::Transform(value.clone()));
                }
            }
            PropertyId::TransformOrigin => {
                if let ParsedValue::TransformOrigin(value) = &declaration.value {
                    out.insert(
                        StyleField::TransformOrigin,
                        StyleValue::TransformOrigin(*value),
                    );
                }
            }
            PropertyId::BorderTopLeftRadius
            | PropertyId::BorderTopRightRadius
            | PropertyId::BorderBottomRightRadius
            | PropertyId::BorderBottomLeftRadius
            | PropertyId::BorderRadius => {
                if let ParsedValue::Length(value) = &declaration.value {
                    if let Some(radius) = resolve_numeric_length(*value) {
                        out.insert(
                            StyleField::BorderRadius,
                            StyleValue::Scalar(radius.max(0.0)),
                        );
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn compile_layout_fields(style: &Style) -> HashMap<LayoutField, f32> {
    let mut out = HashMap::new();
    for declaration in style.declarations() {
        match declaration.property {
            PropertyId::Width => {
                if let ParsedValue::Length(value) = &declaration.value
                    && let Some(width) = resolve_numeric_length(*value)
                {
                    out.insert(LayoutField::Width, width.max(0.0));
                }
            }
            PropertyId::Height => {
                if let ParsedValue::Length(value) = &declaration.value
                    && let Some(height) = resolve_numeric_length(*value)
                {
                    out.insert(LayoutField::Height, height.max(0.0));
                }
            }
            _ => {}
        }
    }
    out
}

fn resolve_numeric_length(length: crate::Length) -> Option<f32> {
    match length {
        crate::Length::Px(value) => Some(value),
        crate::Length::Zero => Some(0.0),
        _ => None,
    }
}

fn map_animation_timing(timing: TransitionTiming) -> TimeFunction {
    match timing {
        TransitionTiming::Linear => TimeFunction::Linear,
        TransitionTiming::EaseIn => TimeFunction::EaseIn,
        TransitionTiming::EaseOut => TimeFunction::EaseOut,
        TransitionTiming::EaseInOut => TimeFunction::EaseInOut,
    }
}

fn sample_animation_progress(
    animation: &mut ActiveAnimation,
    dt_seconds: f32,
    now_seconds: f64,
) -> Option<ProgressSample> {
    if animation.keyframes.is_empty() {
        return None;
    }
    if matches!(animation.play_state, PlayState::Paused) {
        return Some(ProgressSample {
            progress: boundary_progress(animation.direction, 0, false),
            keep_running: false,
        });
    }

    let dt = dt_seconds.max(0.0) as f64;
    let started = animation
        .started_at_seconds
        .get_or_insert_with(|| (now_seconds - dt).max(0.0));
    let elapsed_ms = ((now_seconds - *started) * 1000.0) as f32;
    let local_ms = elapsed_ms - animation.delay_ms as f32;
    let duration_ms = animation.duration_ms.max(0) as f32;

    if local_ms < 0.0 {
        return match animation.fill_mode {
            FillMode::Backwards | FillMode::Both => Some(ProgressSample {
                progress: boundary_progress(animation.direction, 0, false),
                keep_running: true,
            }),
            _ => None,
        };
    }

    let total_iterations = match animation.repeat {
        Repeat::Count(count) => count,
        Repeat::Infinite => u32::MAX,
    };
    if total_iterations == 0 {
        return None;
    }

    if duration_ms <= f32::EPSILON {
        let progress = boundary_progress(
            animation.direction,
            total_iterations.saturating_sub(1),
            true,
        );
        return Some(ProgressSample {
            progress,
            keep_running: false,
        });
    }

    let overall_progress = local_ms / duration_ms;
    let finite_end = match animation.repeat {
        Repeat::Count(count) => overall_progress >= count as f32,
        Repeat::Infinite => false,
    };

    if finite_end {
        return match animation.fill_mode {
            FillMode::Forwards | FillMode::Both => Some(ProgressSample {
                progress: boundary_progress(
                    animation.direction,
                    total_iterations.saturating_sub(1),
                    true,
                ),
                keep_running: false,
            }),
            _ => None,
        };
    }

    let iteration_index = overall_progress.floor().max(0.0) as u32;
    let iteration_progress = overall_progress.fract();
    let directed_progress = directed_progress(
        animation.direction,
        iteration_index,
        animation.timing.sample(iteration_progress),
    );
    Some(ProgressSample {
        progress: directed_progress,
        keep_running: true,
    })
}

fn sample_completed_fill_progress(animation: &ActiveAnimation) -> Option<f32> {
    match animation.fill_mode {
        FillMode::Forwards | FillMode::Both => Some(boundary_progress(
            animation.direction,
            completed_iteration_index(animation.repeat),
            true,
        )),
        FillMode::None | FillMode::Backwards => None,
    }
}

fn completed_iteration_index(repeat: Repeat) -> u32 {
    match repeat {
        Repeat::Count(count) => count.saturating_sub(1),
        Repeat::Infinite => 0,
    }
}

fn boundary_progress(direction: Direction, iteration_index: u32, at_end: bool) -> f32 {
    let raw = if at_end { 1.0 } else { 0.0 };
    directed_progress(direction, iteration_index, raw)
}

fn directed_progress(direction: Direction, iteration_index: u32, raw_progress: f32) -> f32 {
    let reverse = match direction {
        Direction::Normal => false,
        Direction::Reverse => true,
        Direction::Alternate => iteration_index % 2 == 1,
        Direction::AlternateReverse => iteration_index % 2 == 0,
    };
    if reverse {
        1.0 - raw_progress.clamp(0.0, 1.0)
    } else {
        raw_progress.clamp(0.0, 1.0)
    }
}

fn sample_style_fields(
    animation: &ActiveAnimation,
    progress: f32,
) -> Vec<(StyleField, StyleValue)> {
    const FIELDS: [StyleField; 11] = [
        StyleField::Opacity,
        StyleField::BorderRadius,
        StyleField::BackgroundColor,
        StyleField::Color,
        StyleField::BorderTopColor,
        StyleField::BorderRightColor,
        StyleField::BorderBottomColor,
        StyleField::BorderLeftColor,
        StyleField::BoxShadow,
        StyleField::Transform,
        StyleField::TransformOrigin,
    ];
    let mut out = Vec::new();
    for field in FIELDS {
        if let Some(value) = sample_style_field(animation, field, progress) {
            out.push((field, value));
        }
    }
    out
}

fn sample_style_field(
    animation: &ActiveAnimation,
    field: StyleField,
    progress: f32,
) -> Option<StyleValue> {
    let mut previous: Option<(f32, StyleValue)> = None;
    let mut next: Option<(f32, StyleValue)> = None;
    for keyframe in &animation.keyframes {
        let Some(value) = keyframe.style_values.get(&field).cloned() else {
            continue;
        };
        if keyframe.progress <= progress {
            previous = Some((keyframe.progress, value.clone()));
        }
        if keyframe.progress >= progress {
            next = Some((keyframe.progress, value));
            break;
        }
    }
    sample_segment_value(previous, next, progress, |from, to, t| {
        field.interpolate_value(from, to, t)
    })
}

fn sample_layout_fields(animation: &ActiveAnimation, progress: f32) -> Vec<(LayoutField, f32)> {
    [LayoutField::Width, LayoutField::Height]
        .into_iter()
        .filter_map(|field| {
            sample_layout_field(animation, field, progress).map(|value| (field, value))
        })
        .collect()
}

fn sample_layout_field(
    animation: &ActiveAnimation,
    field: LayoutField,
    progress: f32,
) -> Option<f32> {
    let mut previous: Option<(f32, f32)> = None;
    let mut next: Option<(f32, f32)> = None;
    for keyframe in &animation.keyframes {
        let Some(value) = keyframe.layout_values.get(&field).copied() else {
            continue;
        };
        if keyframe.progress <= progress {
            previous = Some((keyframe.progress, value));
        }
        if keyframe.progress >= progress {
            next = Some((keyframe.progress, value));
            break;
        }
    }
    sample_segment_value(previous, next, progress, |from, to, t| {
        from + (to - from) * t
    })
}

fn sample_segment_value<T, F>(
    previous: Option<(f32, T)>,
    next: Option<(f32, T)>,
    progress: f32,
    interpolate: F,
) -> Option<T>
where
    T: Clone,
    F: Fn(T, T, f32) -> T,
{
    match (previous, next) {
        (Some((_, value)), None) | (None, Some((_, value))) => Some(value),
        (Some((from_progress, from)), Some((to_progress, to))) => {
            if (to_progress - from_progress).abs() <= f32::EPSILON {
                return Some(to);
            }
            let segment_t =
                ((progress - from_progress) / (to_progress - from_progress)).clamp(0.0, 1.0);
            Some(interpolate(from, to, segment_t))
        }
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Animation, Animator, Color, Keyframe, Opacity, ParsedValue, PropertyId, Repeat, Style,
    };

    fn opacity_style(value: f32) -> Style {
        let mut style = Style::new();
        style.insert(
            PropertyId::Opacity,
            ParsedValue::Opacity(Opacity::new(value)),
        );
        style
    }

    #[test]
    fn animator_defaults_can_be_overridden_per_animation() {
        let base = Animator::new([Animation::new([
            Keyframe::new(0.0, opacity_style(0.0)),
            Keyframe::new(1.0, opacity_style(1.0)),
        ])
        .duration(900)])
        .duration(500)
        .repeat(Repeat::times(2));

        let animation = &base.animations()[0];
        assert_eq!(base.resolved_duration_ms(animation), 900);
        assert_eq!(base.resolved_repeat(animation), Repeat::times(2));
    }

    #[test]
    fn plugin_samples_interpolated_style_values() {
        let mut plugin = AnimationPlugin::new();
        plugin.start_animator(AnimationRequest {
            target: 7,
            animator: Animator::new([Animation::new([
                Keyframe::new(0.0, opacity_style(0.0)),
                Keyframe::new(1.0, opacity_style(1.0)),
            ])
            .duration(1000)]),
        });

        let result = plugin.run_animations(0.5, 0.5);
        assert!(result.keep_running);

        let samples = plugin.take_style_samples();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].target, 7);
        assert_eq!(samples[0].field, StyleField::Opacity);
        let StyleValue::Scalar(value) = samples[0].value.clone() else {
            panic!("expected scalar style sample");
        };
        assert!((value - 0.5).abs() < 0.0001);
    }

    #[test]
    fn keyframe_accepts_style_macro_shorthand() {
        let mut plugin = AnimationPlugin::new();
        plugin.start_animator(AnimationRequest {
            target: 8,
            animator: Animator::new([Animation::new([
                Keyframe::new(
                    0.0,
                    crate::style! {
                        color: Color::hex("#ff0000"),
                        opacity: 0.25,
                    },
                ),
                Keyframe::new(
                    1.0,
                    crate::style! {
                        color: Color::hex("#00ff00"),
                        opacity: 1.0,
                    },
                ),
            ])
            .duration(1000)]),
        });

        let result = plugin.run_animations(0.5, 0.5);
        assert!(result.keep_running);

        let samples = plugin.take_style_samples();
        assert!(
            samples
                .iter()
                .any(|sample| sample.field == StyleField::Color)
        );
        assert!(
            samples
                .iter()
                .any(|sample| sample.field == StyleField::Opacity)
        );
    }

    #[test]
    fn plugin_keeps_last_frame_with_forwards_fill() {
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::Color(Color::rgb(255, 0, 0).into()),
        );
        let mut plugin = AnimationPlugin::new();
        plugin.start_animator(AnimationRequest {
            target: 9,
            animator: Animator::new([Animation::new([Keyframe::new(1.0, style)])
                .duration(100)
                .fill_mode(FillMode::Forwards)]),
        });

        let _ = plugin.run_animations(0.2, 0.2);
        let samples = plugin.take_style_samples();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].field, StyleField::BackgroundColor);

        let replayed = plugin.run_animations(0.0, 0.2);
        assert!(!replayed.keep_running);
        let samples = plugin.take_style_samples();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].field, StyleField::BackgroundColor);
    }

    #[test]
    fn completed_animator_does_not_restart_on_identical_request() {
        let animator = Animator::new([Animation::new([
            Keyframe::new(0.0, opacity_style(0.0)),
            Keyframe::new(1.0, opacity_style(1.0)),
        ])
        .duration(100)]);
        let mut plugin = AnimationPlugin::new();
        plugin.start_animator(AnimationRequest {
            target: 21,
            animator: animator.clone(),
        });

        let first = plugin.run_animations(0.2, 0.2);
        assert!(!first.keep_running);
        assert!(plugin.take_style_samples().is_empty());

        plugin.start_animator(AnimationRequest {
            target: 21,
            animator,
        });
        let second = plugin.run_animations(0.0, 0.2);
        assert!(!second.keep_running);
        assert!(plugin.take_style_samples().is_empty());
    }

    #[test]
    fn prune_targets_clears_removed_node_state() {
        let animator = Animator::new([Animation::new([
            Keyframe::new(0.0, opacity_style(0.0)),
            Keyframe::new(1.0, opacity_style(1.0)),
        ])
        .duration(100)]);
        let mut plugin = AnimationPlugin::new();
        plugin.start_animator(AnimationRequest {
            target: 33,
            animator: animator.clone(),
        });
        let _ = plugin.run_animations(0.2, 0.2);

        let mut keep = std::collections::HashSet::new();
        keep.insert(34);
        plugin.prune_targets(&keep);

        plugin.start_animator(AnimationRequest {
            target: 33,
            animator,
        });
        let restarted = plugin.run_animations(0.0, 0.0);
        assert!(restarted.keep_running);
    }
}
