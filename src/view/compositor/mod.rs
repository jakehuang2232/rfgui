//! Retained compositor-side scene metadata.
//!
//! Property trees mirror already-resolved element state, while paint
//! generations track retained raster identity.

pub(crate) mod paint_generation;
pub(crate) mod property_tree;

pub(crate) use paint_generation::PaintGenerationTracker;
pub(crate) use property_tree::PropertyTrees;
