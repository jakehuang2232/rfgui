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

































fn nested_default_layout_tree(depth: usize, fanout: usize, idx: usize) -> RsxNode {
    use crate::view::Text as HostText;
    if depth == 0 {
        return rsx! {
            <HostElement style={{
                padding: Padding::uniform(Length::px(2.0)),
            }}>
                <HostText>{format!("leaf label {idx} with some words")}</HostText>
            </HostElement>
        };
    }
    let children = (0..fanout)
        .map(|i| nested_default_layout_tree(depth - 1, fanout, idx * fanout + i))
        .collect::<Vec<_>>();
    rsx! {
        <HostElement style={{
            padding: Padding::uniform(Length::px(4.0)),
        }}>{children}</HostElement>
    }
}

mod eligibility_tests;
mod cached_metadata_tests;
mod flex_replay_tests;
mod placement_skip_tests;
mod inline_ifc_tests;
mod microbench_tests;
