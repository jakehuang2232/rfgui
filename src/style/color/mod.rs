use once_cell::sync::Lazy;

mod hex_color;

pub use hex_color::*;

pub trait Color {
    fn to_rgba_f32(&self) -> [f32; 4];
    fn to_rgba_u8(&self) -> [u8; 4] {
        let rgba_f32 = self.to_rgba_f32();
        [
            (rgba_f32[0] * 255.0).round() as u8,
            (rgba_f32[1] * 255.0).round() as u8,
            (rgba_f32[2] * 255.0).round() as u8,
            (rgba_f32[3] * 255.0).round() as u8,
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
