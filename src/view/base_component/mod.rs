use crate::ui::{
    BlurEvent, ClickEvent, FocusEvent, KeyDownEvent, KeyUpEvent, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, TextInputEvent, ImePreeditEvent,
};
use crate::transition::{
    LayoutField, LayoutTrackRequest, StyleField, StyleTrackRequest, StyleValue, VisualField,
    VisualTrackRequest,
};
use crate::view::viewport::ViewportControl;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

mod core;
mod element;
mod text;
mod text_area;

pub(crate) use core::*;
pub use element::*;
pub use text::*;
pub use text_area::*;

fn next_ui_node_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn collect_box_models(root: &dyn ElementTrait) -> Vec<BoxModelSnapshot> {
    fn walk(node: &dyn ElementTrait, out: &mut Vec<BoxModelSnapshot>) {
        out.push(node.box_model_snapshot());
        if let Some(children) = node.children() {
            for child in children {
                walk(child.as_ref(), out);
            }
        }
    }

    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

#[derive(Debug, Clone, Copy)]
pub struct LayoutTransitionSnapshotSeed {
    pub layout_x: f32,
    pub layout_y: f32,
    pub layout_width: f32,
    pub layout_height: f32,
    pub parent_layout_x: f32,
    pub parent_layout_y: f32,
}

pub fn collect_layout_transition_snapshots(
    roots: &[Box<dyn ElementTrait>],
) -> HashMap<u64, LayoutTransitionSnapshotSeed> {
    let mut out = HashMap::new();

    fn walk(
        node: &dyn ElementTrait,
        parent_layout_x: f32,
        parent_layout_y: f32,
        out: &mut HashMap<u64, LayoutTransitionSnapshotSeed>,
    ) {
        let snapshot = node.box_model_snapshot();
        out.insert(
            node.id(),
            LayoutTransitionSnapshotSeed {
                layout_x: snapshot.x,
                layout_y: snapshot.y,
                layout_width: snapshot.width,
                layout_height: snapshot.height,
                parent_layout_x,
                parent_layout_y,
            },
        );

        let (next_parent_x, next_parent_y) = node
            .as_any()
            .downcast_ref::<Element>()
            .map(Element::child_layout_origin)
            .unwrap_or((snapshot.x, snapshot.y));

        if let Some(children) = node.children() {
            for child in children {
                walk(child.as_ref(), next_parent_x, next_parent_y, out);
            }
        }
    }

    for root in roots {
        walk(root.as_ref(), 0.0, 0.0, &mut out);
    }

    out
}

pub fn seed_layout_transition_snapshots(
    roots: &mut [Box<dyn ElementTrait>],
    snapshots: &HashMap<u64, LayoutTransitionSnapshotSeed>,
) {
    fn apply(node: &mut dyn ElementTrait, snapshots: &HashMap<u64, LayoutTransitionSnapshotSeed>) {
        if let Some(seed) = snapshots.get(&node.id()) {
            if let Some(element) = node.as_any_mut().downcast_mut::<Element>() {
                element.seed_layout_transition_snapshot(
                    seed.layout_x,
                    seed.layout_y,
                    seed.layout_width,
                    seed.layout_height,
                    seed.parent_layout_x,
                    seed.parent_layout_y,
                );
            }
        }
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut() {
                apply(child.as_mut(), snapshots);
            }
        }
    }

    for root in roots.iter_mut() {
        apply(root.as_mut(), snapshots);
    }
}

pub fn hit_test(root: &dyn ElementTrait, viewport_x: f32, viewport_y: f32) -> Option<u64> {
    fn find(node: &dyn ElementTrait, x: f32, y: f32) -> Option<u64> {
        let snapshot = node.box_model_snapshot();
        if !snapshot.should_render || !point_in_box_model(&snapshot, x, y) {
            return None;
        }

        if let Some(children) = node.children() {
            for child in children.iter().rev() {
                if let Some(id) = find(child.as_ref(), x, y) {
                    return Some(id);
                }
            }
        }

        Some(snapshot.node_id)
    }

    find(root, viewport_x, viewport_y)
}

pub fn dispatch_mouse_down_from_hit_test(
    root: &mut dyn ElementTrait,
    event: &mut MouseDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_id) = hit_test(root, event.mouse.viewport_x, event.mouse.viewport_y) else {
        return false;
    };
    event.meta.set_target_id(target_id);
    dispatch_mouse_down_bubble(root, target_id, event, control)
}

pub fn dispatch_mouse_up_from_hit_test(
    root: &mut dyn ElementTrait,
    event: &mut MouseUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_id) = hit_test(root, event.mouse.viewport_x, event.mouse.viewport_y) else {
        return false;
    };
    event.meta.set_target_id(target_id);
    dispatch_mouse_up_bubble(root, target_id, event, control)
}

pub fn dispatch_mouse_up_to_target(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut MouseUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    event.meta.set_target_id(target_id);
    dispatch_mouse_up_bubble(root, target_id, event, control)
}

pub fn dispatch_mouse_move_from_hit_test(
    root: &mut dyn ElementTrait,
    event: &mut MouseMoveEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_id) = hit_test(root, event.mouse.viewport_x, event.mouse.viewport_y) else {
        return false;
    };
    event.meta.set_target_id(target_id);
    dispatch_mouse_move_bubble(root, target_id, event, control)
}

pub fn dispatch_mouse_move_to_target(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut MouseMoveEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    event.meta.set_target_id(target_id);
    dispatch_mouse_move_bubble(root, target_id, event, control)
}

pub fn dispatch_click_from_hit_test(
    root: &mut dyn ElementTrait,
    event: &mut ClickEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let Some(target_id) = hit_test(root, event.mouse.viewport_x, event.mouse.viewport_y) else {
        return false;
    };
    event.meta.set_target_id(target_id);
    dispatch_click_bubble(root, target_id, event, control)
}

pub fn dispatch_scroll_from_hit_test(
    root: &mut dyn ElementTrait,
    viewport_x: f32,
    viewport_y: f32,
    delta_x: f32,
    delta_y: f32,
) -> bool {
    let Some(target_id) = hit_test(root, viewport_x, viewport_y) else {
        return false;
    };
    dispatch_scroll_bubble(root, target_id, delta_x, delta_y)
}

pub fn find_scroll_handler_from_hit_test(
    root: &dyn ElementTrait,
    viewport_x: f32,
    viewport_y: f32,
    delta_x: f32,
    delta_y: f32,
) -> Option<u64> {
    let target_id = hit_test(root, viewport_x, viewport_y)?;
    find_scroll_handler_bubble(root, target_id, delta_x, delta_y)
}

pub fn dispatch_scroll_to_target(
    root: &mut dyn ElementTrait,
    target_id: u64,
    delta_x: f32,
    delta_y: f32,
) -> bool {
    dispatch_scroll_bubble(root, target_id, delta_x, delta_y)
}

pub fn get_scroll_offset_by_id(root: &dyn ElementTrait, node_id: u64) -> Option<(f32, f32)> {
    if root.id() == node_id {
        return Some(root.get_scroll_offset());
    }
    if let Some(children) = root.children() {
        for child in children {
            if let Some(offset) = get_scroll_offset_by_id(child.as_ref(), node_id) {
                return Some(offset);
            }
        }
    }
    None
}

pub fn set_scroll_offset_by_id(
    root: &mut dyn ElementTrait,
    node_id: u64,
    offset: (f32, f32),
) -> bool {
    if root.id() == node_id {
        root.set_scroll_offset(offset);
        return true;
    }
    if let Some(children) = root.children_mut() {
        for child in children.iter_mut() {
            if set_scroll_offset_by_id(child.as_mut(), node_id, offset) {
                return true;
            }
        }
    }
    false
}

pub fn take_style_transition_requests(
    root: &mut dyn ElementTrait,
    out: &mut Vec<StyleTrackRequest>,
) {
    if let Some(children) = root.children_mut() {
        for child in children.iter_mut().rev() {
            take_style_transition_requests(child.as_mut(), out);
        }
    }
    out.extend(root.take_style_transition_requests());
}

pub fn take_layout_transition_requests(
    root: &mut dyn ElementTrait,
    out: &mut Vec<LayoutTrackRequest>,
) {
    if let Some(children) = root.children_mut() {
        for child in children.iter_mut().rev() {
            take_layout_transition_requests(child.as_mut(), out);
        }
    }
    out.extend(root.take_layout_transition_requests());
}

pub fn take_visual_transition_requests(
    root: &mut dyn ElementTrait,
    out: &mut Vec<VisualTrackRequest>,
) {
    if let Some(children) = root.children_mut() {
        for child in children.iter_mut().rev() {
            take_visual_transition_requests(child.as_mut(), out);
        }
    }
    out.extend(root.take_visual_transition_requests());
}

pub fn set_style_field_by_id(
    root: &mut dyn ElementTrait,
    node_id: u64,
    field: StyleField,
    value: StyleValue,
) -> bool {
    if root.id() == node_id {
        if let Some(element) = root.as_any_mut().downcast_mut::<Element>() {
            match field {
                StyleField::Opacity => {
                    if let StyleValue::Scalar(value) = value {
                        element.set_opacity(value);
                    } else {
                        return false;
                    }
                }
                StyleField::BorderRadius => {
                    if let StyleValue::Scalar(value) = value {
                        element.set_border_radius(value);
                    } else {
                        return false;
                    }
                }
                StyleField::BackgroundColor => {
                    if let StyleValue::Color(color) = value {
                        element.set_background_color_value(color);
                    } else {
                        return false;
                    }
                }
                StyleField::Color => {
                    if let StyleValue::Color(color) = value {
                        element.set_foreground_color(color);
                    } else {
                        return false;
                    }
                }
                StyleField::BorderTopColor => {
                    if let StyleValue::Color(color) = value {
                        element.set_border_top_color(color);
                    } else {
                        return false;
                    }
                }
                StyleField::BorderRightColor => {
                    if let StyleValue::Color(color) = value {
                        element.set_border_right_color(color);
                    } else {
                        return false;
                    }
                }
                StyleField::BorderBottomColor => {
                    if let StyleValue::Color(color) = value {
                        element.set_border_bottom_color(color);
                    } else {
                        return false;
                    }
                }
                StyleField::BorderLeftColor => {
                    if let StyleValue::Color(color) = value {
                        element.set_border_left_color(color);
                    } else {
                        return false;
                    }
                }
            }
            return true;
        }
        return false;
    }
    if let Some(children) = root.children_mut() {
        for child in children.iter_mut() {
            if set_style_field_by_id(child.as_mut(), node_id, field, value) {
                return true;
            }
        }
    }
    false
}

pub fn set_layout_field_by_id(
    root: &mut dyn ElementTrait,
    node_id: u64,
    field: LayoutField,
    value: f32,
) -> bool {
    if root.id() == node_id {
        if let Some(element) = root.as_any_mut().downcast_mut::<Element>() {
            match field {
                LayoutField::Width => element.set_layout_transition_width(value),
                LayoutField::Height => element.set_layout_transition_height(value),
                LayoutField::X | LayoutField::Y => return false,
            }
            return true;
        }
        return false;
    }
    if let Some(children) = root.children_mut() {
        for child in children.iter_mut() {
            if set_layout_field_by_id(child.as_mut(), node_id, field, value) {
                return true;
            }
        }
    }
    false
}

pub fn set_visual_field_by_id(
    root: &mut dyn ElementTrait,
    node_id: u64,
    field: VisualField,
    value: f32,
) -> bool {
    if root.id() == node_id {
        if let Some(element) = root.as_any_mut().downcast_mut::<Element>() {
            match field {
                VisualField::X => element.set_layout_transition_x(value),
                VisualField::Y => element.set_layout_transition_y(value),
            }
            return true;
        }
        return false;
    }
    if let Some(children) = root.children_mut() {
        for child in children.iter_mut() {
            if set_visual_field_by_id(child.as_mut(), node_id, field, value) {
                return true;
            }
        }
    }
    false
}

pub fn update_hover_state(root: &mut dyn ElementTrait, target_id: Option<u64>) -> bool {
    fn walk(node: &mut dyn ElementTrait, target_id: Option<u64>) -> (bool, bool) {
        let self_id = node.id();
        let mut contains_target = target_id == Some(self_id);
        let mut changed = false;

        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                let (child_contains_target, child_changed) = walk(child.as_mut(), target_id);
                contains_target |= child_contains_target;
                changed |= child_changed;
            }
        }

        changed |= node.set_hovered(contains_target);
        (contains_target, changed)
    }

    walk(root, target_id).1
}

pub fn cancel_pointer_interactions(root: &mut dyn ElementTrait) -> bool {
    fn walk(node: &mut dyn ElementTrait) -> bool {
        let mut changed = node.cancel_pointer_interaction();
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                changed |= walk(child.as_mut());
            }
        }
        changed
    }

    walk(root)
}

pub fn dispatch_key_down_bubble(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut KeyDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_key_down_impl(root, target_id, event, control)
}

pub fn dispatch_key_up_bubble(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut KeyUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_key_up_impl(root, target_id, event, control)
}

pub fn dispatch_text_input_bubble(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut TextInputEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_text_input_impl(root, target_id, event, control)
}

pub fn dispatch_ime_preedit_bubble(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut ImePreeditEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_ime_preedit_impl(root, target_id, event, control)
}

pub fn dispatch_focus_bubble(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut FocusEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_focus_impl(root, target_id, event, control)
}

pub fn dispatch_blur_bubble(
    root: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut BlurEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    dispatch_blur_impl(root, target_id, event, control)
}

fn dispatch_mouse_down_bubble(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut MouseDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let snapshot = node.box_model_snapshot();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_mouse_down_bubble(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    event.mouse.local_x = event.mouse.viewport_x - snapshot.x;
    event.mouse.local_y = event.mouse.viewport_y - snapshot.y;
    node.dispatch_mouse_down(event, control);
    true
}

fn dispatch_mouse_up_bubble(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut MouseUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let snapshot = node.box_model_snapshot();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_mouse_up_bubble(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    event.mouse.local_x = event.mouse.viewport_x - snapshot.x;
    event.mouse.local_y = event.mouse.viewport_y - snapshot.y;
    node.dispatch_mouse_up(event, control);
    true
}

fn dispatch_mouse_move_bubble(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut MouseMoveEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let snapshot = node.box_model_snapshot();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_mouse_move_bubble(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    event.mouse.local_x = event.mouse.viewport_x - snapshot.x;
    event.mouse.local_y = event.mouse.viewport_y - snapshot.y;
    node.dispatch_mouse_move(event, control);
    true
}

fn dispatch_click_bubble(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut ClickEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let snapshot = node.box_model_snapshot();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_click_bubble(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    event.mouse.local_x = event.mouse.viewport_x - snapshot.x;
    event.mouse.local_y = event.mouse.viewport_y - snapshot.y;
    node.dispatch_click(event, control);
    true
}

fn dispatch_scroll_bubble(node: &mut dyn ElementTrait, target_id: u64, dx: f32, dy: f32) -> bool {
    fn walk(node: &mut dyn ElementTrait, target_id: u64, dx: f32, dy: f32) -> (bool, bool) {
        let node_id = node.id();
        if node_id == target_id {
            let handled = node.scroll_by(dx, dy);
            return (true, handled);
        }

        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                let (found, handled) = walk(child.as_mut(), target_id, dx, dy);
                if !found {
                    continue;
                }
                if handled {
                    return (true, true);
                }
                let self_handled = node.scroll_by(dx, dy);
                return (true, self_handled);
            }
        }

        (false, false)
    }

    walk(node, target_id, dx, dy).1
}

fn find_scroll_handler_bubble(
    node: &dyn ElementTrait,
    target_id: u64,
    dx: f32,
    dy: f32,
) -> Option<u64> {
    fn walk(node: &dyn ElementTrait, target_id: u64, dx: f32, dy: f32) -> (bool, Option<u64>) {
        let node_id = node.id();
        if node_id == target_id {
            if node.can_scroll_by(dx, dy) {
                return (true, Some(node_id));
            }
            return (true, None);
        }

        if let Some(children) = node.children() {
            for child in children.iter().rev() {
                let (found, handled) = walk(child.as_ref(), target_id, dx, dy);
                if !found {
                    continue;
                }
                if handled.is_some() {
                    return (true, handled);
                }
                if node.can_scroll_by(dx, dy) {
                    return (true, Some(node_id));
                }
                return (true, None);
            }
        }

        (false, None)
    }

    walk(node, target_id, dx, dy).1
}

fn dispatch_key_down_impl(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut KeyDownEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_key_down_impl(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    node.dispatch_key_down(event, control);
    true
}

fn dispatch_key_up_impl(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut KeyUpEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_key_up_impl(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    node.dispatch_key_up(event, control);
    true
}

fn dispatch_focus_impl(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut FocusEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_focus_impl(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    node.dispatch_focus(event, control);
    true
}

fn dispatch_text_input_impl(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut TextInputEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_text_input_impl(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    node.dispatch_text_input(event, control);
    true
}

fn dispatch_ime_preedit_impl(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut ImePreeditEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_ime_preedit_impl(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    node.dispatch_ime_preedit(event, control);
    true
}

fn dispatch_blur_impl(
    node: &mut dyn ElementTrait,
    target_id: u64,
    event: &mut BlurEvent,
    control: &mut ViewportControl<'_>,
) -> bool {
    let node_id = node.id();
    let mut found = node_id == target_id;

    if !found {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if dispatch_blur_impl(child.as_mut(), target_id, event, control) {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || event.meta.propagation_stopped() {
        return found;
    }

    event.meta.set_current_target_id(node_id);
    node.dispatch_blur(event, control);
    true
}

fn point_in_box_model(snapshot: &BoxModelSnapshot, x: f32, y: f32) -> bool {
    if snapshot.width <= 0.0 || snapshot.height <= 0.0 {
        return false;
    }

    let left = snapshot.x;
    let top = snapshot.y;
    let right = left + snapshot.width;
    let bottom = top + snapshot.height;
    if x < left || x > right || y < top || y > bottom {
        return false;
    }

    let r = snapshot
        .border_radius
        .max(0.0)
        .min(snapshot.width * 0.5)
        .min(snapshot.height * 0.5);
    if r <= 0.0 {
        return true;
    }

    let tl = (left + r, top + r);
    let tr = (right - r, top + r);
    let bl = (left + r, bottom - r);
    let br = (right - r, bottom - r);

    if x < tl.0 && y < tl.1 {
        return distance_sq(x, y, tl.0, tl.1) <= r * r;
    }
    if x > tr.0 && y < tr.1 {
        return distance_sq(x, y, tr.0, tr.1) <= r * r;
    }
    if x < bl.0 && y > bl.1 {
        return distance_sq(x, y, bl.0, bl.1) <= r * r;
    }
    if x > br.0 && y > br.1 {
        return distance_sq(x, y, br.0, br.1) <= r * r;
    }

    true
}

fn distance_sq(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x1 - x2;
    let dy = y1 - y2;
    dx * dx + dy * dy
}

pub fn get_ime_cursor_rect_by_id(
    root: &dyn ElementTrait,
    node_id: u64,
) -> Option<(f32, f32, f32, f32)> {
    if root.id() == node_id {
        return root.ime_cursor_rect();
    }
    if let Some(children) = root.children() {
        for child in children {
            if let Some(rect) = get_ime_cursor_rect_by_id(child.as_ref(), node_id) {
                return Some(rect);
            }
        }
    }
    None
}
