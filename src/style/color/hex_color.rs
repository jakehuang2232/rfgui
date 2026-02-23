use crate::{srgb_to_linear_f32, style::color::ColorLike};
use std::borrow::Cow;

pub struct HexColor<'a> {
    raw: Cow<'a, str>,
    value: [f32; 4],
}

impl<'a> HexColor<'a> {
    pub fn new(hex: impl Into<Cow<'a, str>>) -> Self {
        let hex = hex.into();
        let bytes = hex.as_bytes();
        let is_valid = Self::validate(bytes);

        let value = if is_valid {
            match bytes.len() {
                4 => {
                    let r = hex_1_to_u8(bytes[1]);
                    let g = hex_1_to_u8(bytes[2]);
                    let b = hex_1_to_u8(bytes[3]);
                    [r * 17, g * 17, b * 17, 255]
                }
                5 => {
                    let r = hex_1_to_u8(bytes[1]);
                    let g = hex_1_to_u8(bytes[2]);
                    let b = hex_1_to_u8(bytes[3]);
                    let a = hex_1_to_u8(bytes[4]);
                    [r * 17, g * 17, b * 17, a * 17]
                }
                7 => {
                    let r = hex_2_to_u8(bytes[1], bytes[2]);
                    let g = hex_2_to_u8(bytes[3], bytes[4]);
                    let b = hex_2_to_u8(bytes[5], bytes[6]);
                    [r, g, b, 255]
                }
                9 => {
                    let r = hex_2_to_u8(bytes[1], bytes[2]);
                    let g = hex_2_to_u8(bytes[3], bytes[4]);
                    let b = hex_2_to_u8(bytes[5], bytes[6]);
                    let a = hex_2_to_u8(bytes[7], bytes[8]);
                    [r, g, b, a]
                }
                _ => unreachable!(),
            }
        } else {
            return HexColor {
                raw: hex,
                value: [0.0, 0.0, 0.0, 0.0],
            };
        };

        HexColor {
            raw: hex,
            value: [
                srgb_to_linear_f32(value[0] as f32 / 255.0),
                srgb_to_linear_f32(value[1] as f32 / 255.0),
                srgb_to_linear_f32(value[2] as f32 / 255.0),
                value[3] as f32 / 255.0,
            ],
        }
    }

    fn validate(bytes: &[u8]) -> bool {
        let length = bytes.len();

        if length == 0 || bytes[0] != b'#' {
            return false;
        }

        if length != 4 && length != 5 && length != 7 && length != 9 {
            return false;
        }

        for &c in &bytes[1..] {
            if !c.is_ascii_hexdigit() {
                return false;
            }
        }

        true
    }

    pub fn get_raw(&self) -> &str {
        &self.raw
    }
}

impl<'a> ColorLike for HexColor<'a> {
    fn box_clone(&self) -> Box<dyn ColorLike> {
        Box::new(HexColor::new(self.raw.to_string()))
    }

    fn to_rgba_f32(&self) -> [f32; 4] {
        self.value
    }
}

struct ColorNone {}

impl ColorLike for ColorNone {
    fn box_clone(&self) -> Box<dyn ColorLike> {
        Box::new(ColorNone {})
    }

    fn to_rgba_f32(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn to_rgba_u8(&self) -> [u8; 4] {
        [0, 0, 0, 0]
    }
}

fn hex_1_to_u8(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

fn hex_2_to_u8(c1: u8, c2: u8) -> u8 {
    (hex_1_to_u8(c1) << 4) | hex_1_to_u8(c2)
}
