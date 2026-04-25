//! Layout primitives shared across components.
//!
//! See `docs/design/layout-functional-refactor.md` for the design rationale.
//! Free-function layout algorithms (axis solver, place pipelines) live in
//! submodules; the data types they exchange live in `types`.

pub(crate) mod flex_solver;
pub(crate) mod inline_fragment;
pub(crate) mod measure;
pub(crate) mod place;
mod types;

pub(crate) use types::{FlexLayoutInfo, LayoutState};
