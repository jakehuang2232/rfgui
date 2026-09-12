use super::{
    Interpolate, interpolate_transform_origin_with_reference_box,
    interpolate_transform_with_reference_box,
};
use crate::style::{
    Angle, BoxShadow, Color, ColorLike, Length, OklchColor, Rotate, Scale, StyleColor, Transform,
    TransformEntry, TransformKind, TransformOrigin, Translate,
};
use glam::Vec2;

#[test]
fn color_interpolation_uses_typed_api() {
    let value = Color::interpolate(
        &Color::rgba(0, 0, 0, 0),
        &Color::rgba(255, 255, 255, 255),
        0.5,
    );
    assert_eq!(value.to_rgba_u8(), [99, 99, 99, 128]);
}

#[test]
fn style_color_prefers_oklch_for_oklch_pairs() {
    let from = StyleColor::Oklch(OklchColor::new(0.3, 0.15, 20.0, 1.0));
    let to = StyleColor::Oklch(OklchColor::new(0.7, 0.05, 340.0, 0.5));
    let value = StyleColor::interpolate(&from, &to, 0.5);
    let StyleColor::Oklch(value) = value else {
        panic!("expected OKLCH result");
    };
    assert!((value.l() - 0.5).abs() < 0.0001);
    assert!((value.c() - 0.1).abs() < 0.0001);
    assert!((value.h() - 0.0).abs() < 0.0001);
    assert!((value.a() - 0.75).abs() < 0.0001);
}

#[test]
fn box_shadow_interpolates_each_field() {
    let from = BoxShadow::new()
        .color(Color::rgba(0, 0, 0, 0))
        .offset_x(0.0)
        .offset_y(2.0)
        .blur(4.0)
        .spread(0.0);
    let to = BoxShadow::new()
        .color(Color::rgba(255, 128, 64, 255))
        .offset_x(10.0)
        .offset_y(6.0)
        .blur(12.0)
        .spread(8.0);

    let value = BoxShadow::interpolate(&from, &to, 0.5);
    assert_eq!(value.offset_x, 5.0);
    assert_eq!(value.offset_y, 4.0);
    assert_eq!(value.blur, 8.0);
    assert_eq!(value.spread, 4.0);
    assert_eq!(value.color.to_rgba_u8(), [99, 46, 19, 128]);
    assert!(!value.inset);
}

#[test]
fn box_shadow_list_interpolation_pads_shorter_side_with_transparent_zero_shadow() {
    let from = vec![
        BoxShadow::new()
            .color(Color::rgba(0, 0, 0, 255))
            .offset_x(4.0)
            .offset_y(8.0)
            .blur(12.0)
            .spread(2.0),
    ];
    let to = vec![
        BoxShadow::new()
            .color(Color::rgba(255, 0, 0, 255))
            .offset_x(8.0)
            .offset_y(12.0)
            .blur(16.0)
            .spread(4.0),
        BoxShadow::new()
            .color(Color::rgba(0, 0, 255, 255))
            .offset_x(10.0)
            .offset_y(14.0)
            .blur(18.0)
            .spread(6.0),
    ];

    let value = Vec::<BoxShadow>::interpolate(&from, &to, 0.5);
    assert_eq!(value.len(), 2);
    assert_eq!(value[0].offset_x, 6.0);
    assert_eq!(value[0].offset_y, 10.0);
    assert_eq!(value[1].offset_x, 5.0);
    assert_eq!(value[1].offset_y, 7.0);
    assert_eq!(value[1].blur, 9.0);
    assert_eq!(value[1].spread, 3.0);
    assert_eq!(value[1].color.to_rgba_u8(), [0, 0, 99, 128]);
    assert!(!value[1].inset);
}

#[test]
fn box_shadow_list_padding_preserves_inset_for_missing_layer() {
    let from = Vec::new();
    let to = vec![BoxShadow::new().inset(true).offset_x(10.0).blur(6.0)];

    let value = Vec::<BoxShadow>::interpolate(&from, &to, 0.5);
    assert_eq!(value.len(), 1);
    assert!(value[0].inset);
    assert_eq!(value[0].offset_x, 5.0);
    assert_eq!(value[0].blur, 3.0);
}

#[test]
fn box_shadow_inset_mismatch_falls_back_to_discrete_interpolation() {
    let from = BoxShadow::new().inset(false).offset_x(2.0);
    let to = BoxShadow::new().inset(true).offset_x(20.0);

    let early = BoxShadow::interpolate(&from, &to, 0.25);
    let late = BoxShadow::interpolate(&from, &to, 0.75);

    assert_eq!(early, from);
    assert_eq!(late, to);
}

#[test]
fn transform_list_interpolation_pads_missing_entries_with_identity_of_matching_kind() {
    let from = vec![Translate::x(Length::px(10.0)), Scale::uniform(2.0)];
    let to = vec![Translate::x(Length::px(30.0))];

    let value = Vec::<TransformEntry>::interpolate(&from, &to, 0.5);
    assert_eq!(value.len(), 2);

    match value[0].kind() {
        TransformKind::Translate { x, y, z } => {
            assert!((x.resolve_without_percent_base(0.0, 0.0) - 20.0).abs() < 0.0001);
            assert!(y.resolve_without_percent_base(0.0, 0.0).abs() < 0.0001);
            assert_eq!(z, 0.0);
        }
        _ => panic!("expected translate"),
    }

    match value[1].kind() {
        TransformKind::Scale { x, y, z } => {
            assert!((x - 1.5).abs() < 0.0001);
            assert!((y - 1.5).abs() < 0.0001);
            assert!((z - 1.0).abs() < 0.0001);
        }
        _ => panic!("expected scale"),
    }
}

#[test]
fn transform_wrapper_interpolates_entry_lists() {
    let from = Transform::new([Rotate::z(Angle::deg(0.0))]);
    let to = Transform::new([Rotate::z(Angle::deg(180.0))]);

    let value = Transform::interpolate(&from, &to, 0.5);
    assert_eq!(value.as_slice().len(), 1);
    match value.as_slice()[0].kind() {
        TransformKind::Rotate { x, y, z } => {
            assert!(x.to_radians().abs() < 0.0001);
            assert!(y.to_radians().abs() < 0.0001);
            let radians = z.to_radians();
            assert!((radians.abs() - std::f32::consts::FRAC_PI_2).abs() < 0.0001);
        }
        _ => panic!("expected rotate"),
    }
}

#[test]
fn angle_interpolation_preserves_full_turn_delta() {
    let value = Angle::interpolate(&Angle::deg(0.0), &Angle::deg(360.0), 0.5);
    assert!((value.to_radians() - std::f32::consts::PI).abs() < 0.0001);
}

#[test]
fn transform_rotation_interpolation_preserves_full_turn_delta() {
    let from = Transform::new([Rotate::z(Angle::deg(0.0))]);
    let to = Transform::new([Rotate::z(Angle::deg(360.0))]);

    let value = Transform::interpolate(&from, &to, 0.5);
    assert_eq!(value.as_slice().len(), 1);
    match value.as_slice()[0].kind() {
        TransformKind::Rotate { x, y, z } => {
            assert!(x.to_radians().abs() < 0.0001);
            assert!(y.to_radians().abs() < 0.0001);
            assert!((z.to_radians() - std::f32::consts::PI).abs() < 0.0001);
        }
        _ => panic!("expected rotate"),
    }
}

#[test]
fn transform_mismatch_falls_back_to_continuous_matrix_interpolation() {
    let from = Transform::new([Translate::x(Length::px(10.0))]);
    let to = Transform::new([Scale::uniform(2.0)]);

    let value = Transform::interpolate(&from, &to, 0.5);
    assert!(!value.as_slice().is_empty());
    assert_ne!(value, from);
    assert_ne!(value, to);
}

#[test]
fn transform_order_mismatch_falls_back_to_matrix_decomposition() {
    let from = Transform::new([Translate::x(Length::px(20.0)), Rotate::z(Angle::deg(30.0))]);
    let to = Transform::new([Rotate::z(Angle::deg(30.0)), Translate::x(Length::px(20.0))]);

    let value = Transform::interpolate(&from, &to, 0.5);
    assert!(!value.as_slice().is_empty());
    assert_ne!(value, from);
    assert_ne!(value, to);
}

#[test]
fn perspective_mismatch_falls_back_to_matrix_entry_interpolation() {
    let from = Transform::new([crate::style::Perspective::px(200.0)]);
    let to = Transform::new([Rotate::z(Angle::deg(45.0))]);

    let value = Transform::interpolate(&from, &to, 0.5);
    assert_eq!(value.as_slice().len(), 1);
    match value.as_slice()[0].kind() {
        TransformKind::Matrix { .. } => {}
        _ => panic!("expected matrix fallback"),
    }
}

#[test]
fn matrix_fallback_resolves_percent_translate_against_reference_box() {
    let from = Transform::new([Translate::x(Length::percent(50.0))]);
    let to = Transform::new([Scale::uniform(2.0)]);

    let with_reference =
        interpolate_transform_with_reference_box(&from, &to, 0.5, Vec2::new(200.0, 100.0));
    let without_reference = interpolate_transform_with_reference_box(&from, &to, 0.5, Vec2::ZERO);

    assert_ne!(with_reference, without_reference);
}

#[test]
fn transform_origin_reference_box_interpolation_resolves_percent_lengths() {
    let from = TransformOrigin::percent(50.0, 50.0);
    let to = TransformOrigin::px(20.0, 10.0);

    let value =
        interpolate_transform_origin_with_reference_box(from, to, 0.5, Vec2::new(200.0, 100.0));

    assert!((value.x().resolve_without_percent_base(0.0, 0.0) - 60.0).abs() < 0.0001);
    assert!((value.y().resolve_without_percent_base(0.0, 0.0) - 30.0).abs() < 0.0001);
}
