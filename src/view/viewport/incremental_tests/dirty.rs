#![allow(unused_imports)]

use super::super::Viewport;
use super::common::*;
use crate::style::{
    ClipMode, Color, Cursor, Layout, Length, Padding, ParsedValue, Position, PropertyId,
    ScrollDirection, Style, Transition, TransitionProperty, Transitions, VerticalAlign,
};
use crate::ui::{
    Binding, DragEffect, RsxNode, RsxTagDescriptor, global_state, on_drag_over, on_drop, rsx,
};
use crate::view::Element as HostElement;

#[test]
fn refresh_frame_box_models_collects_first_refresh_then_reuses_clean_root() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);

    viewport.refresh_frame_box_models();
    let first_stats = viewport.box_model_refresh_stats();
    assert_eq!(first_stats.collected_roots, 1);
    assert_eq!(first_stats.reused_roots, 0);
    assert_eq!(first_stats.collected_snapshots, 2);
    assert_eq!(viewport.frame_box_models().len(), 2);

    viewport.refresh_frame_box_models();
    let second_stats = viewport.box_model_refresh_stats();
    assert_eq!(second_stats.collected_roots, 0);
    assert_eq!(second_stats.reused_roots, 1);
    assert_eq!(second_stats.reused_snapshots, 2);
    assert_eq!(viewport.frame_box_models().len(), 2);
}

#[test]
fn refresh_frame_box_models_clean_skip_preserves_unrelated_paint_dirty() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    viewport.refresh_frame_box_models();

    let root_key = viewport.scene.ui_root_keys[0];
    let child_key = viewport.scene.node_arena.children_of(root_key)[0];
    viewport
        .scene
        .node_arena
        .mutate_element_with_invalidation(child_key, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>()
                .expect("child element")
                .mark_local_dirty(crate::view::base_component::DirtyPassMask::PAINT);
            cx.invalidate(crate::view::base_component::DirtyPassMask::PAINT);
        })
        .expect("child exists");

    viewport.refresh_frame_box_models();

    let stats = viewport.box_model_refresh_stats();
    assert_eq!(stats.collected_roots, 0);
    assert_eq!(stats.reused_roots, 1);
    let child = viewport
        .scene
        .node_arena
        .get(child_key)
        .expect("child exists");
    assert!(
        child
            .element
            .local_dirty_flags()
            .contains(crate::view::base_component::DirtyPassMask::PAINT)
    );
    drop(child);
    assert!(
        viewport
            .scene
            .node_arena
            .arena_local_dirty(child_key)
            .contains(crate::view::base_component::DirtyPassMask::PAINT)
    );
}

#[test]
fn refresh_frame_box_models_dirty_root_updates_cache_and_clears_arena_shadow_dirty() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    viewport.refresh_frame_box_models();

    let root_key = viewport.scene.ui_root_keys[0];
    mark_box_model_dirty_and_set_layout_width(&mut viewport, root_key, 222.0);

    viewport.refresh_frame_box_models();
    let stats = viewport.box_model_refresh_stats();
    assert_eq!(stats.collected_roots, 1);
    assert_eq!(stats.reused_roots, 0);
    assert_eq!(
        box_model_snapshot_for_node(&viewport, root_key).width,
        222.0
    );
    let box_model_flags = crate::view::base_component::DirtyPassMask::BOX_MODEL
        .union(crate::view::base_component::DirtyPassMask::HIT_TEST);
    assert!(
        !viewport
            .scene
            .node_arena
            .arena_local_dirty(root_key)
            .intersects(box_model_flags)
    );

    mark_box_model_dirty_and_set_layout_width(&mut viewport, root_key, 333.0);
    viewport.refresh_frame_box_models();
    assert_eq!(
        box_model_snapshot_for_node(&viewport, root_key).width,
        333.0
    );

    viewport.refresh_frame_box_models();
    let reuse_stats = viewport.box_model_refresh_stats();
    assert_eq!(reuse_stats.collected_roots, 0);
    assert_eq!(reuse_stats.reused_roots, 1);
    assert_eq!(
        box_model_snapshot_for_node(&viewport, root_key).width,
        333.0
    );
}

#[test]
fn layout_transition_field_update_invalidates_frame_box_model_cache() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    viewport.refresh_frame_box_models();

    let root_key = viewport.scene.ui_root_keys[0];
    let child_key = viewport.scene.node_arena.children_of(root_key)[0];
    let child_id = viewport
        .scene
        .node_arena
        .get(child_key)
        .expect("child exists")
        .element
        .stable_id();

    assert!(crate::view::base_component::set_layout_field_by_id(
        &mut viewport.scene.node_arena,
        root_key,
        child_id,
        crate::transition::LayoutField::Height,
        55.0,
    ));

    viewport.refresh_frame_box_models();

    let stats = viewport.box_model_refresh_stats();
    assert_eq!(
        stats.collected_roots, 1,
        "layout transition samples must invalidate cached frame box models",
    );
    assert_eq!(
        box_model_snapshot_for_node(&viewport, child_key).height,
        55.0,
        "frame box model cache must reflect the sampled transition height",
    );
}

#[test]
fn transition_runtime_reconcile_marks_arena_dirty_when_clearing_layout_state() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    viewport.refresh_frame_box_models();

    let root_key = viewport.scene.ui_root_keys[0];
    let child_key = viewport.scene.node_arena.children_of(root_key)[0];
    let child_id = viewport
        .scene
        .node_arena
        .get(child_key)
        .expect("child exists")
        .element
        .stable_id();

    assert!(crate::view::base_component::set_layout_field_by_id(
        &mut viewport.scene.node_arena,
        root_key,
        child_id,
        crate::transition::LayoutField::Width,
        72.0,
    ));
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );

    assert!(
        crate::view::base_component::reconcile_transition_runtime_state(
            &mut viewport.scene.node_arena,
            &[root_key],
            &rustc_hash::FxHashMap::default(),
        )
    );

    assert!(
        viewport
            .scene
            .node_arena
            .arena_local_dirty(child_key)
            .contains(crate::view::base_component::DirtyFlags::ALL),
        "clearing stale layout transition state should bubble the element's local dirty flags into arena dirty"
    );
    assert!(
        viewport
            .scene
            .node_arena
            .cached_subtree_dirty(root_key)
            .contains(crate::view::base_component::DirtyFlags::ALL)
    );

    viewport.refresh_frame_box_models();
    assert_eq!(
        viewport.box_model_refresh_stats().collected_roots,
        1,
        "runtime transition cleanup must invalidate cached frame box models"
    );
}

#[test]
fn visual_transition_field_update_marks_arena_runtime_dirty() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    viewport.refresh_frame_box_models();

    let root_key = viewport.scene.ui_root_keys[0];
    let child_key = viewport.scene.node_arena.children_of(root_key)[0];
    let child_id = viewport
        .scene
        .node_arena
        .get(child_key)
        .expect("child exists")
        .element
        .stable_id();

    assert!(crate::view::base_component::set_visual_field_by_id(
        &mut viewport.scene.node_arena,
        root_key,
        child_id,
        crate::transition::VisualField::X,
        12.0,
    ));

    assert!(
        viewport
            .scene
            .node_arena
            .arena_local_dirty(child_key)
            .contains(crate::view::base_component::DirtyPassMask::RUNTIME)
    );
    assert!(
        viewport
            .scene
            .node_arena
            .cached_subtree_dirty(root_key)
            .contains(crate::view::base_component::DirtyPassMask::RUNTIME)
    );

    viewport.refresh_frame_box_models();
    assert_eq!(
        viewport.box_model_refresh_stats().collected_roots,
        1,
        "visual transition samples must invalidate cached frame box models",
    );
}

#[test]
fn scroll_offset_update_marks_arena_runtime_dirty() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&scrollable_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 100.0, 50.0);
    viewport.refresh_frame_box_models();

    let root_key = viewport.scene.ui_root_keys[0];
    let root_id = viewport
        .scene
        .node_arena
        .get(root_key)
        .expect("root exists")
        .element
        .stable_id();

    assert!(crate::view::base_component::set_scroll_offset_by_id(
        &viewport.scene.node_arena,
        root_key,
        root_id,
        (24.0, 16.0),
    ));

    assert!(
        viewport
            .scene
            .node_arena
            .arena_local_dirty(root_key)
            .contains(crate::view::base_component::DirtyPassMask::RUNTIME)
    );

    viewport.refresh_frame_box_models();
    assert_eq!(
        viewport.box_model_refresh_stats().collected_roots,
        1,
        "scroll offset changes must invalidate cached frame box models",
    );
}

fn text_area_viewport(content: &str) -> (Viewport, crate::view::node_arena::NodeKey) {
    use crate::view::TextArea as HostTextArea;

    let tree = rsx! {
        <HostTextArea content={content.to_string()} />
    };

    let mut viewport = Viewport::new();
    viewport.set_size(320, 160);
    viewport.render_rsx(&tree).expect("render TextArea");
    run_layout_for_test(&mut viewport, 320.0, 160.0);
    let root_key = viewport.scene.ui_root_keys[0];
    viewport.refresh_frame_box_models();
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    (viewport, root_key)
}

#[test]
fn text_area_text_input_marks_arena_dirty_and_recollects_box_models() {
    use crate::view::base_component::DirtyFlags;

    let (mut viewport, root_key) = text_area_viewport("abc");
    viewport.set_focused_node_id(Some(root_key));

    assert!(viewport.dispatch_text_input_event("Z".to_string()));

    assert!(
        viewport
            .scene
            .node_arena
            .arena_local_dirty(root_key)
            .contains(DirtyFlags::ALL),
        "TextArea text input should mirror content dirty into arena local dirty"
    );
    assert!(
        viewport
            .scene
            .node_arena
            .cached_subtree_dirty(root_key)
            .contains(DirtyFlags::ALL),
        "TextArea text input should bubble content dirty into cached subtree dirty"
    );

    viewport.refresh_frame_box_models();
    assert_eq!(viewport.box_model_refresh_stats().collected_roots, 1);
    assert_eq!(viewport.box_model_refresh_stats().reused_roots, 0);
}

#[test]
fn text_area_focus_dirty_still_reuses_clean_box_model_cache() {
    use crate::view::base_component::{DirtyFlags, DirtyPassMask};

    let (mut viewport, root_key) = text_area_viewport("abc");

    assert!(viewport.dispatch_focus_event(root_key));

    let arena_dirty = viewport.scene.node_arena.arena_local_dirty(root_key);
    assert!(arena_dirty.intersects(DirtyFlags::PLACE));
    assert!(arena_dirty.intersects(DirtyFlags::PAINT));
    assert!(
        !arena_dirty.intersects(DirtyPassMask::BOX_MODEL.union(DirtyPassMask::HIT_TEST)),
        "focus/caret paint-place dirty should not become box-model dirty"
    );

    viewport.refresh_frame_box_models();
    assert_eq!(viewport.box_model_refresh_stats().collected_roots, 0);
    assert_eq!(viewport.box_model_refresh_stats().reused_roots, 1);
}

#[test]
fn generic_click_dispatch_marks_arena_paint_dirty_without_box_model_recollect() {
    use crate::view::base_component::{DirtyFlags, DirtyPassMask};

    let mut viewport = Viewport::new();
    viewport.set_size(120, 80);
    viewport
        .render_rsx(&paint_dirty_on_click_tree())
        .expect("render custom click host");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    viewport.refresh_frame_box_models();
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        DirtyFlags::ALL,
    );

    viewport.set_pointer_position_viewport(8.0, 8.0);
    assert!(viewport.dispatch_pointer_down_event(crate::view::viewport::PointerButton::Left));
    assert!(viewport.dispatch_pointer_up_event(crate::view::viewport::PointerButton::Left));
    assert!(viewport.dispatch_click_event(crate::view::viewport::PointerButton::Left));

    let arena_dirty = viewport.scene.node_arena.arena_local_dirty(root_key);
    assert!(arena_dirty.contains(DirtyPassMask::PAINT));
    assert!(
        !arena_dirty.intersects(DirtyPassMask::BOX_MODEL.union(DirtyPassMask::HIT_TEST)),
        "paint-only click mutation must not become box-model/hit-test dirty"
    );
    assert!(
        viewport
            .scene
            .node_arena
            .cached_subtree_dirty(root_key)
            .contains(DirtyPassMask::PAINT)
    );

    viewport.refresh_frame_box_models();
    assert_eq!(viewport.box_model_refresh_stats().collected_roots, 0);
    assert_eq!(viewport.box_model_refresh_stats().reused_roots, 1);
}

#[test]
fn refresh_frame_box_models_reuses_clean_root_and_recollects_dirty_root() {
    let tree = RsxNode::fragment(vec![nested_box_model_tree(), single_element(80.0)]);
    let mut viewport = Viewport::new();
    viewport.render_rsx(&tree).expect("cold render");
    run_layout_for_test(&mut viewport, 240.0, 120.0);
    viewport.refresh_frame_box_models();

    let clean_root = viewport.scene.ui_root_keys[0];
    let dirty_root = viewport.scene.ui_root_keys[1];
    let clean_before = box_model_snapshot_for_node(&viewport, clean_root);
    mark_box_model_dirty_and_set_layout_width(&mut viewport, dirty_root, 166.0);

    viewport.refresh_frame_box_models();

    let stats = viewport.box_model_refresh_stats();
    assert_eq!(stats.collected_roots, 1);
    assert_eq!(stats.reused_roots, 1);
    assert_eq!(
        box_model_snapshot_for_node(&viewport, clean_root).width,
        clean_before.width
    );
    assert_eq!(
        box_model_snapshot_for_node(&viewport, dirty_root).width,
        166.0
    );
    let box_model_flags = crate::view::base_component::DirtyPassMask::BOX_MODEL
        .union(crate::view::base_component::DirtyPassMask::HIT_TEST);
    assert!(
        !viewport
            .scene
            .node_arena
            .arena_local_dirty(dirty_root)
            .intersects(box_model_flags)
    );
}

#[test]
fn refresh_frame_box_models_clears_arena_shadow_dirty() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let child_key = viewport.scene.node_arena.children_of(root_key)[0];
    let flags = crate::view::base_component::DirtyFlags::BOX_MODEL
        .union(crate::view::base_component::DirtyFlags::HIT_TEST);

    assert!(
        crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
            &mut viewport.scene.node_arena,
            root_key,
            crate::view::base_component::DirtyFlags::ALL,
        )
    );
    viewport
        .scene
        .node_arena
        .mutate_element_with_invalidation(child_key, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>()
                .expect("child element")
                .mark_local_dirty(flags);
            cx.invalidate(flags);
        })
        .expect("child exists");

    assert!(
        viewport
            .scene
            .node_arena
            .arena_local_dirty(child_key)
            .contains(flags)
    );
    assert!(
        viewport
            .scene
            .node_arena
            .cached_subtree_dirty(root_key)
            .intersects(flags)
    );

    viewport.refresh_frame_box_models();

    let child = viewport
        .scene
        .node_arena
        .get(child_key)
        .expect("child exists");
    assert!(!child.element.local_dirty_flags().intersects(flags));
    drop(child);
    assert_eq!(
        viewport.scene.node_arena.arena_local_dirty(child_key),
        crate::view::base_component::DirtyFlags::NONE
    );
    assert!(
        !viewport
            .scene
            .node_arena
            .cached_subtree_dirty(root_key)
            .intersects(flags)
    );
    assert!(
        !viewport
            .scene
            .node_arena
            .cached_subtree_dirty(child_key)
            .intersects(flags)
    );
    assert_eq!(viewport.frame_box_models().len(), 2);
}

#[test]
fn layout_pass_clears_consumed_arena_layout_place_and_box_dirty() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let child_key = viewport.scene.node_arena.children_of(root_key)[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    viewport
        .scene
        .node_arena
        .mutate_element_with_invalidation(child_key, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>()
                .expect("child element")
                .mark_layout_dirty_with(cx);
        })
        .expect("child exists");

    assert!(
        viewport
            .scene
            .node_arena
            .cached_subtree_dirty(root_key)
            .contains(crate::view::base_component::DirtyFlags::ALL)
    );

    viewport.run_layout_pass();

    let consumed = crate::view::base_component::DirtyFlags::LAYOUT
        .union(crate::view::base_component::DirtyFlags::PLACE)
        .union(crate::view::base_component::DirtyFlags::BOX_MODEL)
        .union(crate::view::base_component::DirtyFlags::HIT_TEST);
    assert!(
        !viewport
            .scene
            .node_arena
            .arena_local_dirty(child_key)
            .intersects(consumed),
        "layout and box-model phases should not leave consumed arena dirty bits behind"
    );
    assert!(
        viewport
            .scene
            .node_arena
            .arena_local_dirty(child_key)
            .contains(crate::view::base_component::DirtyFlags::PAINT),
        "paint remains dirty until the render graph consumes it"
    );
}
