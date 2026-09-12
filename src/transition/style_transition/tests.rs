use super::{StyleField, StyleValue};
use crate::style::{Angle, BoxShadow, Color, Rotate, Transform, TransformOrigin};

#[test]
fn color_fields_delegate_interpolation_to_color_type() {
    let value = StyleField::BackgroundColor.interpolate_value(
        StyleValue::Color(Color::rgba(0, 0, 0, 0)),
        StyleValue::Color(Color::rgba(255, 255, 255, 255)),
        0.5,
    );
    assert_eq!(value, StyleValue::Color(Color::rgba(99, 99, 99, 128)));
}

#[test]
fn scalar_fields_delegate_interpolation_to_scalar_type() {
    let value = StyleField::Opacity.interpolate_value(
        StyleValue::Scalar(0.2),
        StyleValue::Scalar(0.8),
        0.25,
    );
    let StyleValue::Scalar(value) = value else {
        panic!("expected scalar value");
    };
    assert!((value - 0.35).abs() < 0.0001);
}

#[test]
fn field_default_values_match_property_kind() {
    assert_eq!(StyleField::Opacity.default_value(), StyleValue::Scalar(0.0));
    assert_eq!(
        StyleField::Color.default_value(),
        StyleValue::Color(Color::transparent())
    );
    assert_eq!(
        StyleField::Transform.default_value(),
        StyleValue::Transform(Transform::default())
    );
    assert_eq!(
        StyleField::TransformOrigin.default_value(),
        StyleValue::TransformOrigin(TransformOrigin::center())
    );
    assert_eq!(
        StyleField::BoxShadow.default_value(),
        StyleValue::BoxShadow(Vec::new())
    );
}

#[test]
fn transform_fields_delegate_interpolation_to_transform_type() {
    let value = StyleField::Transform.interpolate_value(
        StyleValue::Transform(Transform::new([Rotate::z(Angle::deg(0.0))])),
        StyleValue::Transform(Transform::new([Rotate::z(Angle::deg(180.0))])),
        0.5,
    );
    let StyleValue::TransformProgress { from, to, progress } = value else {
        panic!("expected transform progress value");
    };
    assert_eq!(from.as_slice().len(), 1);
    assert_eq!(to.as_slice().len(), 1);
    assert!((progress - 0.5).abs() < 0.0001);
}

#[test]
fn transform_origin_fields_delegate_to_progress_value() {
    let value = StyleField::TransformOrigin.interpolate_value(
        StyleValue::TransformOrigin(TransformOrigin::percent(50.0, 50.0)),
        StyleValue::TransformOrigin(TransformOrigin::px(10.0, 20.0)),
        0.25,
    );
    let StyleValue::TransformOriginProgress { progress, .. } = value else {
        panic!("expected transform-origin progress value");
    };
    assert!((progress - 0.25).abs() < 0.0001);
}

#[test]
fn box_shadow_field_delegates_to_shadow_list_interpolation() {
    let value = StyleField::BoxShadow.interpolate_value(
        StyleValue::BoxShadow(vec![BoxShadow::new().offset_x(0.0).blur(0.0)]),
        StyleValue::BoxShadow(vec![BoxShadow::new().offset_x(10.0).blur(8.0)]),
        0.5,
    );
    let StyleValue::BoxShadow(value) = value else {
        panic!("expected box-shadow value");
    };
    assert_eq!(value.len(), 1);
    assert_eq!(value[0].offset_x, 5.0);
    assert_eq!(value[0].blur, 4.0);
}
