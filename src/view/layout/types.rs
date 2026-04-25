//! Shared layout data types.
//!
//! Phase F0/F2 of the layout functional refactor (see
//! `docs/design/layout-functional-refactor.md`).
//! - F0: `LayoutState` — sub-struct aggregating layout-output fields.
//! - F2: `FlexLayoutInfo` / `FlexLineItem` — axis-layout solver output,
//!   shared between solver (`flex_solver`) and place pipelines.

use crate::view::base_component::{Position, Size};

/// Aggregated layout-output state for a component.
///
/// Owned by host components (currently only `Element`; later `TextArea`).
/// Written by layout free functions, read by render and event paths.
///
/// # Fields
///
/// - `layout_position` / `layout_size`: visual box (post-transform offsets
///   applied), used for hit-test and paint anchoring.
/// - `layout_inner_position` / `layout_inner_size`: content box (after
///   border + padding insets), used as origin for child layout.
/// - `layout_flow_position` / `layout_flow_inner_position`: flow-position
///   variants without transform offsets, used for layout transitions and
///   sibling flow.
/// - `content_size`: bounding box of placed children, drives auto-sizing
///   and scroll content metrics.
/// - `should_render`: whether the component contributes pixels to the
///   render tree.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LayoutState {
    pub layout_position: Position,
    pub layout_size: Size,
    pub layout_inner_position: Position,
    pub layout_inner_size: Size,
    pub layout_flow_position: Position,
    pub layout_flow_inner_position: Position,
    pub content_size: Size,
    pub should_render: bool,
}

impl LayoutState {
    /// Construct a `LayoutState` seeded from initial position/size.
    /// Inner / flow / content positions follow the visual box, and
    /// `should_render` defaults to `true`.
    pub(crate) fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        let position = Position { x, y };
        let size = Size {
            width: width.max(0.0),
            height: height.max(0.0),
        };
        Self {
            layout_position: position,
            layout_size: size,
            layout_inner_position: position,
            layout_inner_size: size,
            layout_flow_position: position,
            layout_flow_inner_position: position,
            content_size: Size {
                width: 0.0,
                height: 0.0,
            },
            should_render: true,
        }
    }
}

/// Output of the axis-layout solver (`flex_solver::compute_flex_info`).
///
/// One `FlexLayoutInfo` per axis-layout container (Inline / Flex / Flow).
/// Captures the line breakdown after wrap, gap distribution, and flex
/// grow/shrink resolution. Consumed by the place pipeline to position
/// children.
#[derive(Clone, Debug)]
pub(crate) struct FlexLayoutInfo {
    pub lines: Vec<Vec<FlexLineItem>>,
    pub line_main_sum: Vec<f32>,
    pub line_cross_max: Vec<f32>,
    pub total_main: f32,
    pub total_cross: f32,
}

/// One placeable item on a flex/inline line.
///
/// `child_index` indexes into the container's `children` vec.
/// `node_index` is the inline-fragment index within that child (always 0
/// for non-fragmentable inline children and for flex/flow items).
#[derive(Clone, Copy, Debug)]
pub(crate) struct FlexLineItem {
    pub child_index: usize,
    pub node_index: usize,
    pub main: f32,
    pub cross: f32,
    pub main_offset: f32,
    pub cross_offset: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_state_new_seeds_visual_box_and_zero_content_size() {
        let state = LayoutState::new(10.0, 20.0, 100.0, 50.0);
        assert!((state.layout_position.x - 10.0).abs() < 1e-6);
        assert!((state.layout_position.y - 20.0).abs() < 1e-6);
        assert!((state.layout_size.width - 100.0).abs() < 1e-6);
        assert!((state.layout_size.height - 50.0).abs() < 1e-6);
        assert!((state.layout_inner_position.x - 10.0).abs() < 1e-6);
        assert!((state.layout_flow_position.x - 10.0).abs() < 1e-6);
        // content_size starts at zero (children-driven), distinct from layout_size.
        assert!((state.content_size.width).abs() < 1e-6);
        assert!((state.content_size.height).abs() < 1e-6);
        assert!(state.should_render);
    }

    #[test]
    fn layout_state_new_clamps_negative_dimensions_to_zero() {
        let state = LayoutState::new(0.0, 0.0, -50.0, -10.0);
        assert!((state.layout_size.width).abs() < 1e-6);
        assert!((state.layout_size.height).abs() < 1e-6);
    }
}
