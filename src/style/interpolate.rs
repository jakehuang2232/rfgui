#![allow(missing_docs)]

use crate::style::{
    Angle, BoxShadow, Color, ColorLike, Length, OklchColor, StyleColor, Transform, TransformEntry,
    TransformKind, TransformOrigin, linear_to_srgb_f32,
};
use glam::{EulerRot, Mat4, Quat, Vec2, Vec3, Vec4};

/// Typed interpolation semantics for style values.
pub trait Interpolate: Sized {
    fn interpolate(from: &Self, to: &Self, t: f32) -> Self;
}

impl Interpolate for f32 {
    fn interpolate(from: &Self, to: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        from + (to - from) * t
    }
}

impl Interpolate for Length {
    fn interpolate(from: &Self, to: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let from_weight = 1.0 - t;
        Length::calc(
            Length::calc(*from, crate::Operator::multiply, from_weight),
            crate::Operator::plus,
            Length::calc(*to, crate::Operator::multiply, t),
        )
    }
}

impl Interpolate for Angle {
    fn interpolate(from: &Self, to: &Self, t: f32) -> Self {
        let from = from.to_radians();
        let to = to.to_radians();
        Angle::rad(f32::interpolate(&from, &to, t))
    }
}

impl Interpolate for TransformOrigin {
    fn interpolate(from: &Self, to: &Self, t: f32) -> Self {
        TransformOrigin::new(
            Length::interpolate(&from.x(), &to.x(), t),
            Length::interpolate(&from.y(), &to.y(), t),
        )
        .with_z(f32::interpolate(&from.z(), &to.z(), t))
    }
}

pub fn interpolate_transform_origin_with_reference_box(
    from: TransformOrigin,
    to: TransformOrigin,
    t: f32,
    reference_box: Vec2,
) -> TransformOrigin {
    let resolve = |value: Length, base: f32| {
        value
            .resolve_with_base(Some(base.max(0.0)), 0.0, 0.0)
            .unwrap_or_else(|| value.resolve_without_percent_base(0.0, 0.0))
    };
    TransformOrigin::px(
        f32::interpolate(
            &resolve(from.x(), reference_box.x),
            &resolve(to.x(), reference_box.x),
            t,
        ),
        f32::interpolate(
            &resolve(from.y(), reference_box.y),
            &resolve(to.y(), reference_box.y),
            t,
        ),
    )
    .with_z(f32::interpolate(&from.z(), &to.z(), t))
}

impl Interpolate for TransformEntry {
    fn interpolate(from: &Self, to: &Self, t: f32) -> Self {
        match (from.kind(), to.kind()) {
            (
                TransformKind::Translate {
                    x: from_x,
                    y: from_y,
                    z: from_z,
                },
                TransformKind::Translate {
                    x: to_x,
                    y: to_y,
                    z: to_z,
                },
            ) => crate::Translate::xy(
                Length::interpolate(&from_x, &to_x, t),
                Length::interpolate(&from_y, &to_y, t),
            )
            .with_z(f32::interpolate(&from_z, &to_z, t)),
            (
                TransformKind::Scale {
                    x: from_x,
                    y: from_y,
                    z: from_z,
                },
                TransformKind::Scale {
                    x: to_x,
                    y: to_y,
                    z: to_z,
                },
            ) => crate::Scale::xy(
                f32::interpolate(&from_x, &to_x, t),
                f32::interpolate(&from_y, &to_y, t),
            )
            .with_z(f32::interpolate(&from_z, &to_z, t)),
            (
                TransformKind::Rotate {
                    x: from_x,
                    y: from_y,
                    z: from_z,
                },
                TransformKind::Rotate {
                    x: to_x,
                    y: to_y,
                    z: to_z,
                },
            ) => crate::Rotate::x(Angle::interpolate(&from_x, &to_x, t))
                .y(Angle::interpolate(&from_y, &to_y, t))
                .z(Angle::interpolate(&from_z, &to_z, t)),
            (
                TransformKind::Perspective { depth: from_depth },
                TransformKind::Perspective { depth: to_depth },
            ) => crate::Perspective::px(f32::interpolate(&from_depth, &to_depth, t)),
            (
                TransformKind::Matrix {
                    matrix: from_matrix,
                },
                TransformKind::Matrix { matrix: to_matrix },
            ) => {
                TransformEntry::from_matrix(interpolate_matrix_arrays(&from_matrix, &to_matrix, t))
            }
            _ => {
                if t.clamp(0.0, 1.0) < 0.5 {
                    *from
                } else {
                    *to
                }
            }
        }
    }
}

impl Interpolate for Vec<TransformEntry> {
    fn interpolate(from: &Self, to: &Self, t: f32) -> Self {
        interpolate_transform_entries_with_reference_box(from, to, t, Vec2::ZERO)
    }
}

impl Interpolate for Transform {
    fn interpolate(from: &Self, to: &Self, t: f32) -> Self {
        interpolate_transform_with_reference_box(from, to, t, Vec2::ZERO)
    }
}

pub fn interpolate_transform_with_reference_box(
    from: &Transform,
    to: &Transform,
    t: f32,
    reference_box: Vec2,
) -> Transform {
    let t = t.clamp(0.0, 1.0);
    Transform::from_vec(interpolate_transform_entries_with_reference_box(
        from.as_slice(),
        to.as_slice(),
        t,
        reference_box,
    ))
}

fn interpolate_transform_entries_with_reference_box(
    from: &[TransformEntry],
    to: &[TransformEntry],
    t: f32,
    reference_box: Vec2,
) -> Vec<TransformEntry> {
    let t = t.clamp(0.0, 1.0);
    if requires_matrix_fallback(from, to)
        && let Some(interpolated) =
            interpolate_transform_lists_via_matrix(from, to, t, reference_box)
    {
        return interpolated;
    }
    let len = from.len().max(to.len());
    let mut out = Vec::with_capacity(len);

    for index in 0..len {
        match (from.get(index), to.get(index)) {
            (Some(from_entry), Some(to_entry)) => {
                out.push(TransformEntry::interpolate(from_entry, to_entry, t));
            }
            (Some(from_entry), None) => {
                let identity = TransformEntry::identity_like(from_entry.kind());
                out.push(TransformEntry::interpolate(from_entry, &identity, t));
            }
            (None, Some(to_entry)) => {
                let identity = TransformEntry::identity_like(to_entry.kind());
                out.push(TransformEntry::interpolate(&identity, to_entry, t));
            }
            (None, None) => {}
        }
    }

    out
}

impl Interpolate for Color {
    fn interpolate(from: &Self, to: &Self, t: f32) -> Self {
        StyleColor::interpolate(&StyleColor::Srgb(*from), &StyleColor::Srgb(*to), t).to_color()
    }
}

impl Interpolate for StyleColor {
    fn interpolate(from: &Self, to: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        match (from, to) {
            (StyleColor::Oklch(from), StyleColor::Oklch(to)) => {
                StyleColor::Oklch(interpolate_oklch(from, to, t))
            }
            _ => StyleColor::Srgb(interpolate_oklab_colorlike(from, to, t)),
        }
    }
}

impl Interpolate for BoxShadow {
    fn interpolate(from: &Self, to: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        if from.inset != to.inset {
            return if t < 0.5 { *from } else { *to };
        }
        Self {
            color: StyleColor::interpolate(&from.color, &to.color, t),
            offset_x: f32::interpolate(&from.offset_x, &to.offset_x, t),
            offset_y: f32::interpolate(&from.offset_y, &to.offset_y, t),
            blur: f32::interpolate(&from.blur, &to.blur, t).max(0.0),
            spread: f32::interpolate(&from.spread, &to.spread, t),
            inset: from.inset,
        }
    }
}

fn interpolate_oklch(from: &OklchColor, to: &OklchColor, t: f32) -> OklchColor {
    let [fl, fc, fh, fa] = from.raw();
    let [tl, tc, th, ta] = to.raw();
    let delta_h = ((th - fh + 540.0) % 360.0) - 180.0;
    OklchColor::new(
        f32::interpolate(&fl, &tl, t),
        f32::interpolate(&fc, &tc, t),
        (fh + delta_h * t).rem_euclid(360.0),
        f32::interpolate(&fa, &ta, t),
    )
}

fn interpolate_oklab_colorlike(from: &dyn ColorLike, to: &dyn ColorLike, t: f32) -> Color {
    let [fl, fa, fb, falpha] = rgba_to_oklab(from.to_rgba_f32());
    let [tl, ta, tb, talpha] = rgba_to_oklab(to.to_rgba_f32());
    let l = f32::interpolate(&fl, &tl, t);
    let a = f32::interpolate(&fa, &ta, t);
    let b = f32::interpolate(&fb, &tb, t);
    let alpha = f32::interpolate(&falpha, &talpha, t);
    let [r, g, b] = oklab_to_linear_rgb(l, a, b);
    Color::rgba(
        linear_to_u8(r),
        linear_to_u8(g),
        linear_to_u8(b),
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

fn rgba_to_oklab(rgba: [f32; 4]) -> [f32; 4] {
    let l = 0.412_221_46 * rgba[0] + 0.536_332_55 * rgba[1] + 0.051_445_995 * rgba[2];
    let m = 0.211_903_5 * rgba[0] + 0.680_699_5 * rgba[1] + 0.107_396_96 * rgba[2];
    let s = 0.088_302_46 * rgba[0] + 0.281_718_85 * rgba[1] + 0.629_978_7 * rgba[2];
    let l_cbrt = l.max(0.0).cbrt();
    let m_cbrt = m.max(0.0).cbrt();
    let s_cbrt = s.max(0.0).cbrt();

    [
        0.210_454_26 * l_cbrt + 0.793_617_8 * m_cbrt - 0.004_072_047 * s_cbrt,
        1.977_998_5 * l_cbrt - 2.428_592_2 * m_cbrt + 0.450_593_7 * s_cbrt,
        0.025_904_037 * l_cbrt + 0.782_771_77 * m_cbrt - 0.808_675_77 * s_cbrt,
        rgba[3],
    ]
}

fn oklab_to_linear_rgb(l: f32, a: f32, b: f32) -> [f32; 3] {
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_35 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let l = l_.powi(3);
    let m = m_.powi(3);
    let s = s_.powi(3);

    [
        (4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s).clamp(0.0, 1.0),
        (-1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s).clamp(0.0, 1.0),
        (-0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s).clamp(0.0, 1.0),
    ]
}

fn linear_to_u8(value: f32) -> u8 {
    (linear_to_srgb_f32(value.clamp(0.0, 1.0)) * 255.0).round() as u8
}

fn requires_matrix_fallback(from: &[TransformEntry], to: &[TransformEntry]) -> bool {
    if from.len() != to.len() {
        return true;
    }
    from.iter().zip(to.iter()).any(|(left, right)| {
        std::mem::discriminant(&left.kind()) != std::mem::discriminant(&right.kind())
    })
}

fn interpolate_transform_lists_via_matrix(
    from: &[TransformEntry],
    to: &[TransformEntry],
    t: f32,
    reference_box: Vec2,
) -> Option<Vec<TransformEntry>> {
    let from_matrix = transform_list_to_matrix(from, reference_box);
    let to_matrix = transform_list_to_matrix(to, reference_box);
    if !is_affine_matrix(from_matrix) || !is_affine_matrix(to_matrix) {
        return Some(vec![TransformEntry::from_matrix(
            interpolate_matrix_arrays(&from_matrix.to_cols_array(), &to_matrix.to_cols_array(), t),
        )]);
    }

    let (from_scale, from_rotation, from_translation) = decompose_transform_matrix(from_matrix)?;
    let (to_scale, to_rotation, to_translation) = decompose_transform_matrix(to_matrix)?;

    let translation = from_translation.lerp(to_translation, t);
    let rotation = from_rotation.slerp(to_rotation, t.clamp(0.0, 1.0));
    let scale = from_scale.lerp(to_scale, t);

    Some(compose_canonical_transform_entries(
        translation,
        rotation,
        scale,
    ))
}

fn transform_list_to_matrix(entries: &[TransformEntry], reference_box: Vec2) -> Mat4 {
    let mut transform = Mat4::IDENTITY;
    for entry in entries {
        let next = match entry.kind() {
            TransformKind::Translate { x, y, z } => Mat4::from_translation(Vec3::new(
                resolve_transform_length(x, reference_box.x),
                resolve_transform_length(y, reference_box.y),
                z,
            )),
            TransformKind::Scale { x, y, z } => Mat4::from_scale(Vec3::new(x, y, z)),
            TransformKind::Rotate { x, y, z } => {
                Mat4::from_rotation_x(x.to_radians())
                    * Mat4::from_rotation_y(y.to_radians())
                    * Mat4::from_rotation_z(z.to_radians())
            }
            TransformKind::Perspective { depth } => css_perspective_matrix(depth.max(0.0001)),
            TransformKind::Matrix { matrix } => Mat4::from_cols_array(&matrix),
        };
        transform *= next;
    }
    transform
}

fn resolve_transform_length(length: Length, reference: f32) -> f32 {
    length
        .resolve_with_base(Some(reference.max(0.0)), 0.0, 0.0)
        .unwrap_or_else(|| length.resolve_without_percent_base(0.0, 0.0))
}

fn is_affine_matrix(matrix: Mat4) -> bool {
    matrix.x_axis.w.abs() <= 0.000_001
        && matrix.y_axis.w.abs() <= 0.000_001
        && matrix.z_axis.w.abs() <= 0.000_001
        && (matrix.w_axis.w - 1.0).abs() <= 0.000_001
}

fn decompose_transform_matrix(matrix: Mat4) -> Option<(Vec3, Quat, Vec3)> {
    if !matrix.is_finite() || !is_affine_matrix(matrix) {
        return None;
    }

    let translation = matrix.w_axis.truncate();
    let col0 = matrix.x_axis.truncate();
    let col1 = matrix.y_axis.truncate();
    let col2 = matrix.z_axis.truncate();
    let scale = Vec3::new(col0.length(), col1.length(), col2.length());
    if scale.x <= 0.000_001 || scale.y <= 0.000_001 || scale.z <= 0.000_001 {
        return None;
    }

    let rotation_matrix = Mat4::from_cols(
        (col0 / scale.x).extend(0.0),
        (col1 / scale.y).extend(0.0),
        (col2 / scale.z).extend(0.0),
        Vec4::new(0.0, 0.0, 0.0, 1.0),
    );
    let rotation = Quat::from_mat4(&rotation_matrix).normalize();
    Some((scale, rotation, translation))
}

fn compose_canonical_transform_entries(
    translation: Vec3,
    rotation: Quat,
    scale: Vec3,
) -> Vec<TransformEntry> {
    let mut entries = Vec::new();
    if translation.length_squared() > 0.000_001 {
        entries.push(
            crate::Translate::xy(Length::px(translation.x), Length::px(translation.y))
                .with_z(translation.z),
        );
    }

    let (rx, ry, rz) = rotation.to_euler(EulerRot::XYZ);
    if rx.abs() > 0.000_001 || ry.abs() > 0.000_001 || rz.abs() > 0.000_001 {
        entries.push(
            crate::Rotate::x(Angle::rad(rx))
                .y(Angle::rad(ry))
                .z(Angle::rad(rz)),
        );
    }

    if (scale.x - 1.0).abs() > 0.000_001
        || (scale.y - 1.0).abs() > 0.000_001
        || (scale.z - 1.0).abs() > 0.000_001
    {
        entries.push(crate::Scale::xy(scale.x, scale.y).with_z(scale.z));
    }

    entries
}

fn css_perspective_matrix(depth: f32) -> Mat4 {
    if depth.abs() <= 0.000_001 {
        return Mat4::IDENTITY;
    }
    Mat4::from_cols(
        Vec4::new(1.0, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 1.0, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 1.0, -1.0 / depth),
        Vec4::new(0.0, 0.0, 0.0, 1.0),
    )
}

fn interpolate_matrix_arrays(from: &[f32; 16], to: &[f32; 16], t: f32) -> [f32; 16] {
    let mut out = [0.0; 16];
    for index in 0..16 {
        out[index] = f32::interpolate(&from[index], &to[index], t);
    }
    out
}

impl Interpolate for Vec<BoxShadow> {
    fn interpolate(from: &Self, to: &Self, t: f32) -> Self {
        let len = from.len().max(to.len());
        (0..len)
            .map(|index| {
                let left_fallback;
                let right_fallback;
                let left = if let Some(left) = from.get(index) {
                    left
                } else {
                    right_fallback = BoxShadow::new()
                        .color(Color::transparent())
                        .inset(to.get(index).map(|shadow| shadow.inset).unwrap_or(false));
                    &right_fallback
                };
                let right = if let Some(right) = to.get(index) {
                    right
                } else {
                    left_fallback = BoxShadow::new()
                        .color(Color::transparent())
                        .inset(from.get(index).map(|shadow| shadow.inset).unwrap_or(false));
                    &left_fallback
                };
                BoxShadow::interpolate(left, right, t)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Interpolate, interpolate_transform_origin_with_reference_box,
        interpolate_transform_with_reference_box,
    };
    use crate::style::{
        Angle, BoxShadow, Color, ColorLike, Length, OklchColor, Rotate, Scale, StyleColor,
        Transform, TransformEntry, TransformKind, TransformOrigin, Translate,
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
        let from = Transform::new([crate::Perspective::px(200.0)]);
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
        let without_reference =
            interpolate_transform_with_reference_box(&from, &to, 0.5, Vec2::ZERO);

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
}
