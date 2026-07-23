// TODO(session3-todo3): port tests to arena API. 5000+ lines of tests
// using legacy Box-tree add_child, single-arg measure/place/build,
// children().expect(...). Gated pending port.
//
// Prior agent attempts: imports already updated to the arena API (super
// exports, test_support helpers). Remaining work: ~117 `add_child`, ~300
// `measure`/`place` call-sites, ~69 `children()` accessors, 5 `build`
// calls to rewrite via `commit_child` / `with_element_taken` /
// `arena.children_of`. ~404 rustc errors when un-gated.

use super::*;
use super::super::core::Position as LayoutPosition;
use super::{
    DirtyFlags, Element, ElementTrait, EventTarget, LayoutConstraints, LayoutPlacement,
    Layoutable, ScrollbarAxis, ScrollbarDragState, Size, UiBuildContext,
    expand_corner_radii_for_spread, main_axis_start_and_gap, normalize_corner_radii,
    resolve_px_with_base, resolve_signed_px_with_base,
};
use crate::style::Layout;
use crate::style::{
    Align, AnchorName, Border, BorderRadius, BoxShadow, ClipMode, Collision,
    CollisionBoundary, Color, ComputedStyle, CrossSize, JustifyContent, Length, Opacity,
    Operator, Origin, Position, ScrollDirection, Style, Transform, TransformOrigin,
    Translate, VerticalAlign,
};
use crate::style::{ParsedValue, PropertyId, Transition, TransitionProperty, Transitions};
use crate::transition::{LayoutField, VisualField};
use crate::view::base_component::ComputedStyleConsumer;
use crate::view::base_component::Text;
use crate::view::frame_graph::FrameGraph;
use crate::view::inline_formatting_context::{
    InlineFormattingContext, InlineIfcAtomicMeasureConstraints, InlineIfcDecorationBoxInsets,
    InlineIfcElementDecorationDrawRectStyle, InlineIfcInput, InlineIfcItem,
    InlineIfcMeasuredAtomicBox, InlineIfcSize, InlineIfcSourceId, InlineIfcStyle,
};
use crate::view::test_support::{
    child_key, child_snapshot, commit_child, commit_element, measure_and_place, new_test_arena,
    nth_child_snapshot,
};
use crate::view::viewport::transitions_tick::set_style_field_by_id;
use glam::{Mat4, Vec3};



fn drain_deferred(
    ctx: &mut UiBuildContext,
) -> Vec<crate::view::base_component::DeferredRenderNode> {
    std::iter::from_fn(|| ctx.next_deferred()).collect()
}




































































































fn clean_bridge_element(width: f32, height: f32) -> Element {
    let mut element = Element::new(0.0, 0.0, width, height);
    element.clear_local_dirty_flags(DirtyFlags::ALL);
    element.mark_paint_dirty();
    element
}

fn mark_arena_paint_dirty_for_subtree(
    arena: &crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
) {
    arena.mark_dirty(key, DirtyFlags::PAINT);
    for child in arena.children_of(key) {
        mark_arena_paint_dirty_for_subtree(arena, child);
    }
}




fn clean_style_sample_arena() -> (
    crate::view::node_arena::NodeArena,
    crate::view::node_arena::NodeKey,
    crate::view::node_arena::NodeKey,
    u64,
) {
    let mut arena = new_test_arena();
    let mut root = Element::new(0.0, 0.0, 200.0, 150.0);
    root.clear_local_dirty_flags(DirtyFlags::ALL);
    let root_key = commit_element(&mut arena, Box::new(root));

    let mut child = Element::new(0.0, 0.0, 80.0, 40.0);
    child.clear_local_dirty_flags(DirtyFlags::ALL);
    let child_id = child.stable_id();
    let child_key = commit_child(&mut arena, root_key, Box::new(child));

    assert!(
        crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
            &mut arena,
            root_key,
            DirtyFlags::ALL,
        )
    );
    arena.refresh_subtree_dirty_cache(root_key);
    assert_eq!(arena.arena_local_dirty(child_key), DirtyFlags::NONE);
    assert!(
        !arena
            .cached_subtree_dirty(root_key)
            .intersects(DirtyFlags::PAINT)
    );

    (arena, root_key, child_key, child_id)
}

fn assert_style_sample_paint_dirty(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    child_key: crate::view::node_arena::NodeKey,
) {
    assert_style_sample_dirty_flags(arena, root_key, child_key, DirtyFlags::PAINT);
}

fn assert_style_sample_dirty_flags(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    child_key: crate::view::node_arena::NodeKey,
    flags: DirtyFlags,
) {
    let child = crate::view::test_support::get_element::<Element>(arena, child_key);
    assert!(child.local_dirty_flags().contains(flags));
    assert!(arena.arena_local_dirty(child_key).contains(flags));
    assert!(arena.cached_subtree_dirty(child_key).contains(flags));
    assert!(arena.cached_subtree_dirty(root_key).contains(flags));
}

fn style_sample_place_dirty_flags() -> DirtyFlags {
    DirtyFlags::PLACE
        .union(DirtyFlags::BOX_MODEL)
        .union(DirtyFlags::HIT_TEST)
        .union(DirtyFlags::PAINT)
}






















#[derive(Clone, Debug)]
struct InlineElementIfcDemoSpec {
    name: &'static str,
    max_width: f32,
    include_atomic_box: bool,
}




fn place_grandparent_parent_child(
    parent_box: (f32, f32, f32, f32),
    child_anchor: crate::style::Anchor,
    child_left: f32,
    child_top: f32,
) -> (
    crate::view::node_arena::NodeArena,
    crate::view::node_arena::NodeKey,
) {
    // grandparent (root) > parent (absolute @ parent_box) > child (absolute, anchor=...)
    let grandparent = Element::new(0.0, 0.0, 800.0, 600.0);
    let mut parent = Element::new(0.0, 0.0, parent_box.2, parent_box.3);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(parent_box.0))
                .top(Length::px(parent_box.1)),
        ),
    );
    parent.apply_style(parent_style);
    let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .anchor(child_anchor)
                .left(Length::px(child_left))
                .top(Length::px(child_top)),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let gp_key = commit_element(&mut arena, Box::new(grandparent));
    let parent_key = commit_child(&mut arena, gp_key, Box::new(parent));
    let child_key = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        gp_key,
        LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        },
    );
    (arena, child_key)
}












/// Helper: build a parent inline container holding two pure
/// elements of differing heights. `va` is applied to each child
/// directly (the runtime style cascade for Element-to-Element
/// inheritance is not wired through the test apply_style path —
/// `compute_style` with parent context is exercised in its own
/// unit tests). Returns the placed y-offset of each element.
fn place_two_pure_elements_with_va(
    va: VerticalAlign,
    first_w: f32,
    first_h: f32,
    second_w: f32,
    second_h: f32,
) -> (f32, f32) {
    let mut arena = new_test_arena();
    let mut parent = Element::new(0.0, 0.0, 200.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
    parent.apply_style(parent_style);
    let parent_key = commit_element(&mut arena, Box::new(parent));

    let mut first = Element::new(0.0, 0.0, first_w, first_h);
    let mut first_style = Style::new();
    first_style.insert(PropertyId::VerticalAlign, ParsedValue::VerticalAlign(va));
    first.apply_style(first_style);
    commit_child(&mut arena, parent_key, Box::new(first));

    let mut second = Element::new(0.0, 0.0, second_w, second_h);
    let mut second_style = Style::new();
    second_style.insert(PropertyId::VerticalAlign, ParsedValue::VerticalAlign(va));
    second.apply_style(second_style);
    commit_child(&mut arena, parent_key, Box::new(second));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 200.0,
            max_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        },
    );

    let first = nth_child_snapshot(&arena, parent_key, 0);
    let second = nth_child_snapshot(&arena, parent_key, 1);
    (first.y, second.y)
}








/// Padded fragmentable inline wrapper sharing an outer line with
/// non-padded text siblings: per CSS, the wrapper's vertical
/// padding paints OUTSIDE the line box, so the painted box top
/// extends above the line top by `padding-top`. The wrapper's
/// inner text fragment.position.y must still match its non-padded
/// siblings' fragment.position.y on the same line. Mirrors the
/// inline-test demo's "Mixed Text / Element" scene where a padded
/// badge flows inline alongside `<Text>` siblings.
/// D7: fragmentable inline element shares its own `vertical-align`
/// across all outer fragments. Inner line items keep their own
/// values.
// ---- Regression: projected atomic content must wrap from residue to a fresh line ----



mod box_model_tests;
mod length_resolution_tests;
mod flow_layout_tests;
mod flex_layout_tests;
mod absolute_positioning_tests;
mod absolute_clip_tests;
mod anchor_resolution_tests;
mod viewport_anchored_tests;
mod viewport_anchored_snackbar_tests;
mod hover_and_style_sync_tests;
mod layout_transition_tests;
mod transition_measure_tests;
mod transition_clip_tests;
mod min_max_size_tests;
mod child_clip_scope_tests;
mod scroll_container_tests;
mod render_state_tests;
mod dirty_flag_tests;
mod style_sample_tests;
mod inline_layout_tests;
mod inline_ifc_package_tests;
mod vertical_align_tests;
mod persistent_target_key_tests;

