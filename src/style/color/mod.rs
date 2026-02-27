use once_cell::sync::Lazy;

mod hex_color;
mod oklch_color;

pub use hex_color::*;
pub use oklch_color::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn to_rgba_u8(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn hex(raw: &str) -> HexColor<'_> {
        HexColor::new(raw)
    }

    pub fn transparent() -> Self {
        Self::rgba(0, 0, 0, 0)
    }
}

pub trait ColorLike {
    fn box_clone(&self) -> Box<dyn ColorLike>;
    fn to_rgba_f32(&self) -> [f32; 4];
    fn to_rgba_u8(&self) -> [u8; 4] {
        let rgba_f32 = self.to_rgba_f32();
        [
            (linear_to_srgb_f32(rgba_f32[0].clamp(0.0, 1.0)) * 255.0).round() as u8,
            (linear_to_srgb_f32(rgba_f32[1].clamp(0.0, 1.0)) * 255.0).round() as u8,
            (linear_to_srgb_f32(rgba_f32[2].clamp(0.0, 1.0)) * 255.0).round() as u8,
            (rgba_f32[3].clamp(0.0, 1.0) * 255.0).round() as u8,
        ]
    }
    fn to_wgpu_color(&self) -> wgpu::Color {
        let rgba = self.to_rgba_f32();
        wgpu::Color {
            r: rgba[0] as f64,
            g: rgba[1] as f64,
            b: rgba[2] as f64,
            a: rgba[3] as f64,
        }
    }

    fn is_transparent(&self) -> bool {
        self.to_rgba_u8()[3] != 255
    }
}

impl Clone for Box<dyn ColorLike> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}

impl ColorLike for Box<dyn ColorLike> {
    fn box_clone(&self) -> Box<dyn ColorLike> {
        (**self).box_clone()
    }

    fn to_rgba_f32(&self) -> [f32; 4] {
        (**self).to_rgba_f32()
    }
}

impl ColorLike for Color {
    fn box_clone(&self) -> Box<dyn ColorLike> {
        Box::new(*self)
    }

    fn to_rgba_f32(&self) -> [f32; 4] {
        [
            srgb_to_linear(self.r),
            srgb_to_linear(self.g),
            srgb_to_linear(self.b),
            self.a as f32 / 255.0,
        ]
    }
}

impl Default for Box<dyn ColorLike> {
    fn default() -> Self {
        Box::new(Color::rgb(0, 0, 0))
    }
}

pub trait IntoColor<T> {
    fn into_color(self) -> T;
}

impl IntoColor<Color> for &str {
    fn into_color(self) -> Color {
        let [r, g, b, a] = Color::hex(self).to_rgba_u8();
        Color::rgba(r, g, b, a)
    }
}

impl IntoColor<Color> for String {
    fn into_color(self) -> Color {
        self.as_str().into_color()
    }
}

impl<'a> IntoColor<HexColor<'a>> for &'a str {
    fn into_color(self) -> HexColor<'a> {
        HexColor::new(self)
    }
}

impl IntoColor<HexColor<'static>> for String {
    fn into_color(self) -> HexColor<'static> {
        HexColor::new(self)
    }
}

impl<T> IntoColor<Color> for T
where
    T: ColorLike,
{
    fn into_color(self) -> Color {
        let [r, g, b, a] = self.to_rgba_u8();
        Color::rgba(r, g, b, a)
    }
}

static SRGB8_TO_LINEAR: Lazy<[f32; 256]> = Lazy::new(|| {
    let mut t = [0.0f32; 256];
    for i in 0..256 {
        let c = i as f32 / 255.0;
        t[i] = if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        };
    }
    t
});

pub fn srgb_to_linear(c: u8) -> f32 {
    SRGB8_TO_LINEAR[c as usize]
}

pub fn srgb_to_linear_f32(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb_f32(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}
