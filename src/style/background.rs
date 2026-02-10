use crate::style::color::Color;

pub enum Background {
    Color(Box<dyn Color>),
}