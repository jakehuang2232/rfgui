#![allow(missing_docs)]

//! Background value types used by the typed style system.

use crate::style::color::ColorLike;

/// A typed background declaration.
pub enum Background {
    Color(Box<dyn ColorLike>),
}
