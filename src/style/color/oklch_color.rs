use crate::style::color::ColorLike;

#[derive(Clone)]
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
}

impl ColorLike for OklchColor {
    fn box_clone(&self) -> Box<dyn ColorLike> {
        Box::new(self.clone())
    }

    fn to_rgba_f32(&self) -> [f32; 4] {
        self.value
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
