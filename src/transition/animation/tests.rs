use super::*;
use crate::style::{
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

    let _ = plugin.run_animations(0.0);
    let result = plugin.run_animations(0.5);
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

    let result = plugin.run_animations(0.5);
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

    let _ = plugin.run_animations(0.0);
    let _ = plugin.run_animations(0.2);
    let samples = plugin.take_style_samples();
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].field, StyleField::BackgroundColor);

    let replayed = plugin.run_animations(0.2);
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

    let _ = plugin.run_animations(0.0);
    let first = plugin.run_animations(0.2);
    assert!(!first.keep_running);
    assert!(plugin.take_style_samples().is_empty());

    plugin.start_animator(AnimationRequest {
        target: 21,
        animator,
    });
    let second = plugin.run_animations(0.2);
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
    let _ = plugin.run_animations(0.0);
    let _ = plugin.run_animations(0.2);

    let mut keep = FxHashSet::default();
    keep.insert(34);
    plugin.prune_targets(&keep);

    plugin.start_animator(AnimationRequest {
        target: 33,
        animator,
    });
    let restarted = plugin.run_animations(0.2);
    assert!(restarted.keep_running);
}

fn fade_in(delay_ms: i32, fill_mode: FillMode) -> Animator {
    Animator::new([Animation::new([
        Keyframe::new(0.0, opacity_style(0.0)),
        Keyframe::new(1.0, opacity_style(1.0)),
    ])
    .duration(500)
    .delay(delay_ms)
    .fill_mode(fill_mode)
    .linear()])
}

fn released_targets(plugin: &mut AnimationPlugin) -> Vec<u64> {
    let mut targets: Vec<_> = plugin
        .take_released_style_fields()
        .into_iter()
        .map(|(target, field)| {
            assert_eq!(field, StyleField::Opacity);
            target
        })
        .collect();
    targets.sort_unstable();
    targets
}

#[test]
fn fields_go_back_to_the_element_once_no_animation_writes_them() {
    let mut plugin = AnimationPlugin::new();
    for (target, delay_ms) in [(1, 0), (2, 300), (3, 0)] {
        plugin.start_animator(AnimationRequest {
            target,
            animator: fade_in(delay_ms, FillMode::Backwards),
        });
    }
    let _ = plugin.run_animations(0.0);
    let _ = plugin.run_animations(0.1);
    assert_eq!(plugin.take_style_samples().len(), 3);
    assert!(released_targets(&mut plugin).is_empty());

    plugin.start_animator(AnimationRequest {
        target: 3,
        animator: Animator::from_vec(Vec::new()),
    });
    let _ = plugin.run_animations(0.116);
    assert_eq!(released_targets(&mut plugin), [3]);

    // The next frame comes long after both ends: one fade was midway, the
    // other still in its delay.
    let late = plugin.run_animations(2.0);
    assert!(!late.keep_running);
    assert!(plugin.take_style_samples().is_empty());
    assert_eq!(released_targets(&mut plugin), [1, 2]);
}

#[test]
fn a_delay_without_backwards_fill_waits_from_the_first_sample() {
    let mut plugin = AnimationPlugin::new();
    plugin.start_animator(AnimationRequest {
        target: 5,
        animator: fade_in(300, FillMode::None),
    });

    // First sampled long after the clock started.
    let waiting = plugin.run_animations(5.0);
    assert!(waiting.keep_running);
    assert!(plugin.take_style_samples().is_empty());

    let _ = plugin.run_animations(5.4);
    let samples = plugin.take_style_samples();
    assert_eq!(samples.len(), 1);
    let StyleValue::Scalar(value) = samples[0].value.clone() else {
        panic!("expected scalar style sample");
    };
    assert!((value - 0.2).abs() < 0.0001);
}
