use crate::style::color::ColorLike;

pub enum Background {
    Color(Box<dyn ColorLike>),
}
