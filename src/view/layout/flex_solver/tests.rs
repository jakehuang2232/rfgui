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
