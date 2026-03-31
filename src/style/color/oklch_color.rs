#![allow(missing_docs)]

//! OKLCH-based color values for perceptual color authoring.

use crate::style::color::ColorLike;

/// A color stored in the OKLCH color space with alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OklchColor {
    raw: [f32; 4],   // [l, c, h, a]
    value: [f32; 4], // linear RGBA
}

impl OklchColor {
    pub fn new(l: f32, c: f32, h: f32, a: f32) -> Self {
        let l = l.clamp(0.0, 1.0);
        let c = c.max(0.0);
        let h = h.rem_euclid(360.0);
        let a = a.clamp(0.0, 1.0);

        let value = oklch_to_linear_rgba(l, c, h, a);

        Self {
            raw: [l, c, h, a],
            value,
        }
    }

    pub fn raw(&self) -> [f32; 4] {
        self.raw
    }

    pub fn l(&self) -> f32 {
        self.raw[0]
    }
    pub fn c(&self) -> f32 {
        self.raw[1]
    }
    pub fn h(&self) -> f32 {
        self.raw[2]
    }
    pub fn a(&self) -> f32 {
        self.raw[3]
    }

    pub fn from_linear_rgba(value: [f32; 4]) -> Self {
        let [l, c, h] = linear_rgba_to_oklch(value[0], value[1], value[2]);
        Self::new(l, c, h, value[3])
    }
}

impl ColorLike for OklchColor {
    fn box_clone(&self) -> Box<dyn ColorLike> {
        Box::new(self.clone())
    }

    fn to_rgba_f32(&self) -> [f32; 4] {
        self.value
    }

    fn as_oklch(&self) -> Option<&OklchColor> {
        Some(self)
    }
}

// ---- conversion core ----

fn oklch_to_linear_rgba(l: f32, c: f32, h_deg: f32, a: f32) -> [f32; 4] {
    let h = h_deg.to_radians();
    let a_lab = c * h.cos();
    let b_lab = c * h.sin();

    let l_ = l + 0.3963377774 * a_lab + 0.2158037573 * b_lab;
    let m_ = l - 0.1055613458 * a_lab - 0.0638541728 * b_lab;
    let s_ = l - 0.0894841775 * a_lab - 1.2914855480 * b_lab;

    let l = l_.powi(3);
    let m = m_.powi(3);
    let s = s_.powi(3);

    let r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
    let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
    let b = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;

    [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0), a]
}

fn linear_rgba_to_oklch(r: f32, g: f32, b: f32) -> [f32; 3] {
    let l = 0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;

    let l_cbrt = l.max(0.0).cbrt();
    let m_cbrt = m.max(0.0).cbrt();
    let s_cbrt = s.max(0.0).cbrt();

    let l_ok = 0.210_454_26 * l_cbrt + 0.793_617_8 * m_cbrt - 0.004_072_047 * s_cbrt;
    let a_ok = 1.977_998_5 * l_cbrt - 2.428_592_2 * m_cbrt + 0.450_593_7 * s_cbrt;
    let b_ok = 0.025_904_037 * l_cbrt + 0.782_771_77 * m_cbrt - 0.808_675_77 * s_cbrt;

    let c = (a_ok * a_ok + b_ok * b_ok).sqrt();
    let h = b_ok.atan2(a_ok).to_degrees().rem_euclid(360.0);

    [l_ok.clamp(0.0, 1.0), c.max(0.0), h]
}
