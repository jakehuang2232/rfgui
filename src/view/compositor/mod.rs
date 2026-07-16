//! Retained compositor-side scene metadata.
//!
//! The first slice is intentionally observational: property trees mirror
//! already-resolved element state, but do not drive rendering or promotion.

pub(crate) mod layer_tree;
pub(crate) mod layerizer;
pub(crate) mod paint_generation;
pub(crate) mod property_tree;
pub(crate) mod raster_cache;

pub(crate) use layer_tree::LayerTree;
pub(crate) use paint_generation::PaintGenerationTracker;
pub(crate) use property_tree::PropertyTrees;
pub(crate) use raster_cache::RasterCache;
