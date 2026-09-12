use super::*;

use crate::style::Color;
use crate::style::{Length, ParsedValue, Position, PropertyId, ScrollDirection, Style};
use crate::ui::{
    ClickEvent, DataTransfer, DragEffect, DragOverEvent, EventMeta, Modifiers, NodeId,
    PointerButton, PointerButtons, PointerDownEvent, PointerEventData,
};
use crate::view::base_component::{Element, EventTarget, LayoutConstraints, LayoutPlacement};
use crate::view::test_support::{commit_child, commit_element, measure_and_place, new_test_arena};
use crate::view::{Viewport, ViewportControl};
use std::cell::Cell;
use std::rc::Rc;

fn constraints(w: f32, h: f32) -> LayoutConstraints {
    LayoutConstraints {
        max_width: w,
        max_height: h,
        viewport_width: w,
        percent_base_width: Some(w),
        percent_base_height: Some(h),
        viewport_height: h,
    }
}

fn placement(w: f32, h: f32) -> LayoutPlacement {
    LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: w,
        available_height: h,
        viewport_width: w,
        percent_base_width: Some(w),
        percent_base_height: Some(h),
        viewport_height: h,
    }
}

#[test]
fn drag_over_bubble_recomputes_local_pointer_for_current_target() {
    let observed_local = Rc::new(Cell::new(None::<(i32, i32)>));

    let root = Element::new(0.0, 0.0, 200.0, 120.0);
    let mut child = Element::new(0.0, 0.0, 100.0, 40.0);
    let child_observed = observed_local.clone();
    child.on_drag_over(move |event, _control| {
        child_observed.set(Some((
            event.pointer.local_x.round() as i32,
            event.pointer.local_y.round() as i32,
        )));
        event.accept(DragEffect::Move);
    });
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(20.0))
                .top(Length::px(30.0)),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(root));
    let child_key = commit_child(&mut arena, root_key, Box::new(child));

    measure_and_place(
        &mut arena,
        root_key,
        constraints(200.0, 120.0),
        placement(200.0, 120.0),
    );

    let mut viewport = Viewport::new();
    let mut control = ViewportControl::new(&mut viewport);
    let mut event = DragOverEvent {
        meta: EventMeta::new(child_key),
        pointer: PointerEventData {
            viewport_x: 25.0,
            viewport_y: 45.0,
            local_x: 0.0,
            local_y: 0.0,
            button: None,
            buttons: PointerButtons::default(),
            modifiers: Modifiers::default(),
            pointer_id: 0,
            pointer_type: crate::platform::input::PointerType::Mouse,
            pressure: 0.0,
            timestamp: crate::time::Instant::now(),
        },
        data: DataTransfer::new(),
        drop_effect: None,
    };

    assert!(dispatch_drag_over_bubble(
        &arena,
        root_key,
        child_key,
        &mut event,
        &mut control,
    ));
    assert_eq!(observed_local.get(), Some((5, 15)));
    assert_eq!(event.drop_effect, Some(DragEffect::Move));
}

#[test]
fn click_on_scrollbar_does_not_reach_click_handlers() {
    let mut root = Element::new(0.0, 0.0, 120.0, 120.0);
    let mut root_style = Style::new();
    root_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#101010")),
    );
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root.apply_style(root_style);

    let child_clicked = Rc::new(Cell::new(false));
    let mut child = Element::new(0.0, 0.0, 120.0, 360.0);
    child.set_background_color_value(Color::rgb(255, 0, 0));
    let child_clicked_flag = child_clicked.clone();
    child.on_click(move |_, _| child_clicked_flag.set(true));

    let root_clicked = Rc::new(Cell::new(false));
    let root_clicked_flag = root_clicked.clone();
    root.on_click(move |_, _| root_clicked_flag.set(true));

    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(root));
    let _child_key = commit_child(&mut arena, root_key, Box::new(child));

    measure_and_place(
        &mut arena,
        root_key,
        constraints(120.0, 120.0),
        placement(120.0, 120.0),
    );
    arena.with_element_taken(root_key, |el, _a| {
        if let Some(e) = el.as_any_mut().downcast_mut::<Element>() {
            let _ = e.set_hovered(true);
        }
    });

    let mut viewport = Viewport::new();
    let mut control = ViewportControl::new(&mut viewport);
    let mut click = ClickEvent {
        meta: EventMeta::new(NodeId::default()),
        pointer: PointerEventData {
            viewport_x: 115.0,
            viewport_y: 60.0,
            local_x: 0.0,
            local_y: 0.0,
            button: Some(PointerButton::Left),
            buttons: PointerButtons::default(),
            modifiers: Modifiers::default(),
            pointer_id: 0,
            pointer_type: crate::platform::input::PointerType::Mouse,
            pressure: 0.0,
            timestamp: crate::time::Instant::now(),
        },
        click_count: 1,
    };

    let handled = dispatch_click_from_hit_test(&mut arena, root_key, &mut click, &mut control);
    assert!(handled);
    assert!(!child_clicked.get());
    assert!(!root_clicked.get());
}

#[test]
fn mouse_down_on_scrollbar_requests_focus_keep() {
    let mut root = Element::new(0.0, 0.0, 120.0, 120.0);
    let mut root_style = Style::new();
    root_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#101010")),
    );
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root.apply_style(root_style);
    let mut child = Element::new(0.0, 0.0, 120.0, 360.0);
    child.set_background_color_value(Color::rgb(255, 0, 0));

    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(root));
    let _child_key = commit_child(&mut arena, root_key, Box::new(child));

    measure_and_place(
        &mut arena,
        root_key,
        constraints(120.0, 120.0),
        placement(120.0, 120.0),
    );
    arena.with_element_taken(root_key, |el, _a| {
        if let Some(e) = el.as_any_mut().downcast_mut::<Element>() {
            let _ = e.set_hovered(true);
        }
    });

    let mut viewport = Viewport::new();
    let meta = EventMeta::new(NodeId::default());
    let mut control = ViewportControl::new(&mut viewport);
    let mut down = PointerDownEvent {
        meta: meta.clone(),
        pointer: PointerEventData {
            viewport_x: 115.0,
            viewport_y: 60.0,
            local_x: 0.0,
            local_y: 0.0,
            button: Some(PointerButton::Left),
            buttons: PointerButtons::default(),
            modifiers: Modifiers::default(),
            pointer_id: 0,
            pointer_type: crate::platform::input::PointerType::Mouse,
            pressure: 0.0,
            timestamp: crate::time::Instant::now(),
        },
        viewport: meta.viewport(),
    };

    let handled =
        dispatch_pointer_down_from_hit_test(&mut arena, root_key, &mut down, &mut control);
    assert!(handled);
    assert!(down.meta.focus_change_suppressed());
}
