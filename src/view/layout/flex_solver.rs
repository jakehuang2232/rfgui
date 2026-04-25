//! Axis-layout solver: computes line breakdown for `Layout::Inline`,
//! `Layout::Flex`, `Layout::Flow`.
//!
//! Phase F2 of the layout functional refactor. Extracted from
//! `Element::compute_flex_info` (previously in
//! `view/base_component/element/impl_render.rs`). Pure free functions
//! over explicit inputs + `&mut NodeArena` (effect channel for child
//! `measure` recursion).

use crate::style::{Layout, SizeValue};
use crate::view::base_component::{
    FlexProps, InlineNodeSize, LayoutConstraints, resolve_px_with_base,
};
use crate::view::layout::types::{FlexLayoutInfo, FlexLineItem};
use crate::view::node_arena::{NodeArena, NodeKey};

/// Inputs to `compute_flex_info`.
///
/// Caller pre-computes:
/// - `absolute_mask`: parallel to `children`, true for absolute-positioned
///   children (skipped during line layout).
/// - `is_row` / `is_real_flex` / `wrap`: derived from `layout_kind`.
/// - `gap`: resolved px.
/// - `main_limit`: container's main-axis inner size.
pub(crate) struct FlexSolverInputs<'a> {
    pub layout_kind: Layout,
    pub children: &'a [NodeKey],
    pub absolute_mask: &'a [bool],
    pub is_row: bool,
    pub is_real_flex: bool,
    pub wrap: bool,
    pub gap: f32,
    pub main_limit: f32,
    pub child_available_width: f32,
    pub child_available_height: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub child_percent_base_width: Option<f32>,
    pub child_percent_base_height: Option<f32>,
}

/// Internal: per-item plan during flex grow/shrink distribution.
#[derive(Clone, Copy, Debug)]
struct FlexItemPlan {
    index: usize,
    flex_base_main: f32,
    hypothetical_main: f32,
    used_main: f32,
    min_main: f32,
    max_main: Option<f32>,
    grow: f32,
    shrink: f32,
    frozen: bool,
    cross: f32,
}

/// Compute the line breakdown for an axis-layout container.
///
/// Returns `FlexLayoutInfo` consumed by `place_flex_children`.
///
/// `arena: &mut NodeArena` is required because real flex (`is_real_flex
/// == true`) recursively calls `measure` on children with their resolved
/// main size.
pub(crate) fn compute_flex_info(
    inputs: FlexSolverInputs<'_>,
    arena: &mut NodeArena,
) -> FlexLayoutInfo {
    let FlexSolverInputs {
        layout_kind,
        children,
        absolute_mask,
        is_row,
        is_real_flex,
        wrap,
        gap,
        main_limit,
        child_available_width,
        child_available_height,
        viewport_width,
        viewport_height,
        child_percent_base_width,
        child_percent_base_height,
    } = inputs;

    let mut child_sizes = vec![(0.0_f32, 0.0_f32); children.len()];
    if is_real_flex {
        let mut items = build_flex_item_plans(
            children,
            absolute_mask,
            is_row,
            main_limit,
            viewport_width,
            viewport_height,
            arena,
        );
        let line = items.iter().map(|item| item.index).collect::<Vec<_>>();
        distribute_flex_line(&mut items, gap, main_limit);
        let gap_total = gap * (items.len().saturating_sub(1) as f32);
        let mut line_cross = 0.0_f32;
        let mut final_main_sum = 0.0_f32;

        for item in &mut items {
            let child_key = children[item.index];
            let measured = arena
                .with_element_taken(child_key, |child, arena| {
                    child.measure(
                        LayoutConstraints {
                            max_width: if is_row {
                                item.used_main
                            } else {
                                child_available_width
                            },
                            max_height: if is_row {
                                child_available_height
                            } else {
                                item.used_main
                            },
                            viewport_width,
                            viewport_height,
                            percent_base_width: child_percent_base_width,
                            percent_base_height: child_percent_base_height,
                        },
                        arena,
                    );
                    child.measured_size()
                })
                .unwrap_or((0.0, 0.0));
            let (measured_w, measured_h) = measured;
            item.cross = if is_row { measured_h } else { measured_w };
            child_sizes[item.index] = (item.used_main, item.cross);
            final_main_sum += item.used_main;
            line_cross = line_cross.max(item.cross);
        }

        let total_main = if line.is_empty() {
            0.0
        } else {
            final_main_sum + gap_total
        };
        let line_items = line
            .iter()
            .copied()
            .map(|child_index| FlexLineItem {
                child_index,
                node_index: 0,
                main: child_sizes[child_index].0,
                cross: child_sizes[child_index].1,
                main_offset: 0.0,
                cross_offset: 0.0,
            })
            .collect::<Vec<_>>();
        return FlexLayoutInfo {
            lines: if line_items.is_empty() {
                Vec::new()
            } else {
                vec![line_items]
            },
            line_main_sum: if total_main > 0.0 || line_cross > 0.0 {
                vec![total_main]
            } else {
                Vec::new()
            },
            line_cross_max: if total_main > 0.0 || line_cross > 0.0 {
                vec![line_cross]
            } else {
                Vec::new()
            },
            total_main,
            total_cross: line_cross,
        };
    }

    let mut inline_nodes: Vec<FlexLineItem> = Vec::new();
    for (child_index, child_key) in children.iter().enumerate() {
        if absolute_mask.get(child_index).copied().unwrap_or(false) {
            continue;
        }
        let node_sizes = {
            let Some(child_node) = arena.get(*child_key) else {
                continue;
            };
            if matches!(layout_kind, Layout::Inline) {
                child_node.element.get_inline_nodes_size(arena)
            } else {
                let (w, h) = child_node.element.measured_size();
                vec![InlineNodeSize { width: w, height: h }]
            }
        };
        if node_sizes.is_empty() {
            inline_nodes.push(FlexLineItem {
                child_index,
                node_index: 0,
                main: 0.0,
                cross: 0.0,
                main_offset: 0.0,
                cross_offset: 0.0,
            });
            continue;
        }
        for (node_index, node) in node_sizes.into_iter().enumerate() {
            let node_main = if is_row {
                node.width.max(0.0)
            } else {
                node.height.max(0.0)
            };
            let node_cross = if is_row {
                node.height.max(0.0)
            } else {
                node.width.max(0.0)
            };
            inline_nodes.push(FlexLineItem {
                child_index,
                node_index,
                main: node_main,
                cross: node_cross,
                main_offset: 0.0,
                cross_offset: 0.0,
            });
        }
    }

    let mut lines: Vec<Vec<FlexLineItem>> = Vec::new();
    let mut line_main_sum: Vec<f32> = Vec::new();
    let mut line_cross_max: Vec<f32> = Vec::new();
    let mut current = Vec::new();
    let mut current_main = 0.0;
    let mut current_cross = 0.0;

    for item in inline_nodes {
        let item_main = item.main;
        let item_cross = item.cross;
        let inserts_gap = current
            .last()
            .is_some_and(|prev: &FlexLineItem| prev.child_index != item.child_index);
        let next_main = if current.is_empty() {
            item_main
        } else if inserts_gap {
            current_main + gap + item_main
        } else {
            current_main + item_main
        };
        if wrap && !current.is_empty() && next_main > main_limit {
            lines.push(current);
            line_main_sum.push(current_main);
            line_cross_max.push(current_cross);
            current = Vec::new();
            current_main = 0.0;
            current_cross = 0.0;
        }
        if current.is_empty() {
            current_main = item_main;
            current_cross = item_cross;
        } else if current
            .last()
            .is_some_and(|prev: &FlexLineItem| prev.child_index != item.child_index)
        {
            current_main += gap + item_main;
            current_cross = current_cross.max(item_cross);
        } else {
            current_main += item_main;
            current_cross = current_cross.max(item_cross);
        }
        current.push(item);
    }
    if !current.is_empty() {
        lines.push(current);
        line_main_sum.push(current_main);
        line_cross_max.push(current_cross);
    }

    let total_main = line_main_sum.iter().fold(0.0f32, |a, &b| a.max(b));
    let total_cross = line_cross_max.iter().sum::<f32>()
        + gap * (line_cross_max.len().saturating_sub(1) as f32);

    FlexLayoutInfo {
        lines,
        line_main_sum,
        line_cross_max,
        total_main,
        total_cross,
    }
}

/// Build the `FlexItemPlan` list for a real-flex container.
fn build_flex_item_plans(
    children: &[NodeKey],
    absolute_mask: &[bool],
    is_row: bool,
    main_limit: f32,
    viewport_width: f32,
    viewport_height: f32,
    arena: &NodeArena,
) -> Vec<FlexItemPlan> {
    let mut items = Vec::new();
    for (idx, child_key) in children.iter().enumerate() {
        if absolute_mask.get(idx).copied().unwrap_or(false) {
            continue;
        }
        let Some(child_node) = arena.get(*child_key) else {
            continue;
        };
        let props = child_node.element.flex_props();
        let (measured_w, measured_h) = child_node.element.measured_size();
        drop(child_node);
        let measured_main = if is_row { measured_w } else { measured_h };
        let flex_base_main = resolve_flex_base_main_size(
            &props,
            measured_main,
            is_row,
            main_limit,
            viewport_width,
            viewport_height,
        );
        let min_main = if props.has_explicit_min_main(is_row) {
            resolve_flex_main_constraint(
                props.min_main(is_row),
                main_limit,
                viewport_width,
                viewport_height,
            )
            .unwrap_or(0.0)
        } else {
            props.auto_min_main(is_row).unwrap_or(0.0)
        };
        items.push(FlexItemPlan {
            index: idx,
            flex_base_main,
            hypothetical_main: flex_base_main,
            used_main: flex_base_main,
            min_main,
            max_main: resolve_flex_main_constraint(
                props.max_main(is_row),
                main_limit,
                viewport_width,
                viewport_height,
            ),
            grow: props.grow.max(0.0),
            shrink: props.shrink.max(0.0),
            frozen: false,
            cross: 0.0,
        });
        let item = items.last_mut().expect("just pushed");
        item.hypothetical_main = clamp_flex_main(item.flex_base_main, item.min_main, item.max_main);
        item.used_main = item.hypothetical_main;
    }
    items
}

/// Distribute remaining free space along a real-flex line via grow/shrink.
fn distribute_flex_line(items: &mut [FlexItemPlan], gap: f32, main_limit: f32) {
    for item in items.iter_mut() {
        item.used_main = item.hypothetical_main;
        item.frozen = false;
    }

    let gap_total = gap * (items.len().saturating_sub(1) as f32);
    loop {
        let free_space =
            main_limit - gap_total - items.iter().map(|item| item.used_main).sum::<f32>();
        if free_space.abs() <= 0.01 {
            break;
        }

        if free_space > 0.0 {
            let total_grow = items
                .iter()
                .filter(|item| !item.frozen)
                .map(|item| item.grow)
                .sum::<f32>();
            if total_grow <= 0.0 {
                break;
            }

            let mut froze_any = false;
            for item in items.iter_mut().filter(|item| !item.frozen) {
                let candidate = item.used_main + free_space * (item.grow / total_grow);
                let clamped = clamp_flex_main(candidate, item.min_main, item.max_main);
                item.used_main = clamped;
                if (clamped - candidate).abs() > 0.01 {
                    item.frozen = true;
                    froze_any = true;
                }
            }
            if !froze_any {
                break;
            }
            continue;
        }

        let total_shrink_weight = items
            .iter()
            .filter(|item| !item.frozen)
            .map(|item| item.shrink * item.flex_base_main)
            .sum::<f32>();
        if total_shrink_weight <= 0.0 {
            break;
        }

        let mut froze_any = false;
        for item in items.iter_mut().filter(|item| !item.frozen) {
            let shrink_weight = item.shrink * item.flex_base_main;
            let candidate = item.used_main + free_space * (shrink_weight / total_shrink_weight);
            let clamped = clamp_flex_main(candidate, item.min_main, item.max_main);
            item.used_main = clamped;
            if (clamped - candidate).abs() > 0.01 {
                item.frozen = true;
                froze_any = true;
            }
        }
        if !froze_any {
            break;
        }
    }
}

fn resolve_flex_base_main_size(
    props: &FlexProps,
    measured_main: f32,
    is_row: bool,
    main_limit: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> f32 {
    match props.basis {
        SizeValue::Length(length) => {
            resolve_px_with_base(length, Some(main_limit), viewport_width, viewport_height)
                .unwrap_or(measured_main)
        }
        SizeValue::Auto => match props.main_size(is_row) {
            SizeValue::Length(length) => {
                resolve_px_with_base(length, Some(main_limit), viewport_width, viewport_height)
                    .unwrap_or(0.0)
            }
            SizeValue::Auto => props.auto_base_main(is_row).unwrap_or(0.0),
        },
    }
    .max(0.0)
}

fn resolve_flex_main_constraint(
    value: SizeValue,
    main_limit: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<f32> {
    let SizeValue::Length(length) = value else {
        return None;
    };
    resolve_px_with_base(length, Some(main_limit), viewport_width, viewport_height)
        .map(|value| value.max(0.0))
}

fn clamp_flex_main(main: f32, min_main: f32, max_main: Option<f32>) -> f32 {
    let clamped = main.max(min_main);
    if let Some(max_main) = max_main {
        clamped.min(max_main.max(min_main))
    } else {
        clamped
    }
}

#[cfg(test)]
mod tests {
    //! Pure unit tests for the flex-solver helpers. Tests that exercise
    //! the full `compute_flex_info` need a `NodeArena` fixture and live
    //! in `view/base_component/element/tests.rs`.

    use super::*;

    fn plan(idx: usize, base: f32, min: f32, max: Option<f32>, grow: f32, shrink: f32) -> FlexItemPlan {
        FlexItemPlan {
            index: idx,
            flex_base_main: base,
            hypothetical_main: base,
            used_main: base,
            min_main: min,
            max_main: max,
            grow,
            shrink,
            frozen: false,
            cross: 0.0,
        }
    }

    #[test]
    fn clamp_flex_main_no_max_returns_min_floor() {
        assert!((clamp_flex_main(5.0, 10.0, None) - 10.0).abs() < 1e-6);
        assert!((clamp_flex_main(20.0, 10.0, None) - 20.0).abs() < 1e-6);
    }

    #[test]
    fn clamp_flex_main_max_below_min_collapses_to_min() {
        // Defensive: when max < min the function clamps to min, not max.
        assert!((clamp_flex_main(50.0, 30.0, Some(10.0)) - 30.0).abs() < 1e-6);
    }

    #[test]
    fn clamp_flex_main_clamps_within_range() {
        assert!((clamp_flex_main(50.0, 10.0, Some(40.0)) - 40.0).abs() < 1e-6);
        assert!((clamp_flex_main(5.0, 10.0, Some(40.0)) - 10.0).abs() < 1e-6);
        assert!((clamp_flex_main(20.0, 10.0, Some(40.0)) - 20.0).abs() < 1e-6);
    }

    #[test]
    fn distribute_flex_line_no_growth_when_no_grow_factor() {
        let mut items = vec![plan(0, 50.0, 0.0, None, 0.0, 1.0)];
        distribute_flex_line(&mut items, 0.0, 200.0);
        // Only one item, free space 150, but grow=0 so it stays at base.
        assert!((items[0].used_main - 50.0).abs() < 0.1);
    }

    #[test]
    fn distribute_flex_line_grows_proportionally() {
        // Two items, each 50, total 100. Limit 200, free 100.
        // grow = [1, 1] → each gets +50 → both end at 100.
        let mut items = vec![
            plan(0, 50.0, 0.0, None, 1.0, 1.0),
            plan(1, 50.0, 0.0, None, 1.0, 1.0),
        ];
        distribute_flex_line(&mut items, 0.0, 200.0);
        assert!((items[0].used_main - 100.0).abs() < 0.1);
        assert!((items[1].used_main - 100.0).abs() < 0.1);
    }

    #[test]
    fn distribute_flex_line_respects_max_when_growing() {
        // grow=1 each, but item 0 capped at 70.
        let mut items = vec![
            plan(0, 50.0, 0.0, Some(70.0), 1.0, 1.0),
            plan(1, 50.0, 0.0, None, 1.0, 1.0),
        ];
        distribute_flex_line(&mut items, 0.0, 200.0);
        assert!((items[0].used_main - 70.0).abs() < 0.1);
        // Item 1 absorbs remaining: 200 - 70 = 130.
        assert!((items[1].used_main - 130.0).abs() < 0.1);
    }

    #[test]
    fn distribute_flex_line_shrinks_when_overflowing() {
        // Two 100-wide items in 150 limit, total overflow = 50.
        // shrink=1 each → each loses ~25 → both ≈75.
        let mut items = vec![
            plan(0, 100.0, 0.0, None, 0.0, 1.0),
            plan(1, 100.0, 0.0, None, 0.0, 1.0),
        ];
        distribute_flex_line(&mut items, 0.0, 150.0);
        assert!((items[0].used_main - 75.0).abs() < 0.1);
        assert!((items[1].used_main - 75.0).abs() < 0.1);
    }

    #[test]
    fn distribute_flex_line_respects_min_when_shrinking() {
        // Item 0 has min=80; can't shrink below.
        let mut items = vec![
            plan(0, 100.0, 80.0, None, 0.0, 1.0),
            plan(1, 100.0, 0.0, None, 0.0, 1.0),
        ];
        distribute_flex_line(&mut items, 0.0, 150.0);
        assert!((items[0].used_main - 80.0).abs() < 0.1);
        // Item 1 absorbs remaining shrink: 150 - 80 = 70.
        assert!((items[1].used_main - 70.0).abs() < 0.1);
    }

    #[test]
    fn distribute_flex_line_accounts_for_gap_in_free_space() {
        // Two items at 50 each + gap 20 = 120 occupied. Limit 200, free 80.
        // grow=1 each → each +40 → both 90.
        let mut items = vec![
            plan(0, 50.0, 0.0, None, 1.0, 1.0),
            plan(1, 50.0, 0.0, None, 1.0, 1.0),
        ];
        distribute_flex_line(&mut items, 20.0, 200.0);
        assert!((items[0].used_main - 90.0).abs() < 0.1);
        assert!((items[1].used_main - 90.0).abs() < 0.1);
    }
}
