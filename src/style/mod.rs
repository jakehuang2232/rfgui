mod color;
mod background;

pub use color::*;
pub use background::*;

pub enum Style {
    Background(Background),
}

pub struct Styles {
    styles: Vec<Style>,
}

pub struct Class {
    label: String,
    styles: Styles,
}