#![allow(missing_docs)]

use crate::style::{BoxShadow, Color, ColorLike, OklchColor, StyleColor, linear_to_srgb_f32};

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
        Self {
            color: StyleColor::interpolate(&from.color, &to.color, t),
            offset_x: f32::interpolate(&from.offset_x, &to.offset_x, t),
            offset_y: f32::interpolate(&from.offset_y, &to.offset_y, t),
            blur: f32::interpolate(&from.blur, &to.blur, t).max(0.0),
            spread: f32::interpolate(&from.spread, &to.spread, t),
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

impl Interpolate for Vec<BoxShadow> {
    fn interpolate(from: &Self, to: &Self, t: f32) -> Self {
        let len = from.len().max(to.len());
        let default_shadow = BoxShadow::new().color(Color::transparent());
        (0..len)
            .map(|index| {
                let left = from.get(index).unwrap_or(&default_shadow);
                let right = to.get(index).unwrap_or(&default_shadow);
                BoxShadow::interpolate(left, right, t)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::Interpolate;
    use crate::style::{BoxShadow, Color, ColorLike, OklchColor, StyleColor};

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
    }
}
