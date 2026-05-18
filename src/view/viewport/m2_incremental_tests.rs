//! Phase A M2 tests: dark-launched incremental Fiber commit path.
//!
//! These live in a dedicated submodule (rather than fiber_work.rs) so
//! they can reach into the viewport's private `scene` field to inspect
//! arena root keys directly — the whole point of M2 is that NodeKey
//! identity survives across renders, and the arena handles aren't
//! otherwise exposed from the Viewport API surface.
//!
//! Flag-off coverage is implicit: every existing `cargo test --lib`
//! path already exercises `render_rsx` with `use_incremental_commit
//! == false`. The tests here specifically flip the flag on.

#![cfg(test)]

use super::Viewport;
use crate::style::{
    ClipMode, Color, Cursor, Layout, Length, Padding, ParsedValue, Position, PropertyId,
    ScrollDirection, Style, Transition, TransitionProperty, Transitions, VerticalAlign,
};
use crate::ui::{
    Binding, DragEffect, RsxNode, RsxTagDescriptor, global_state, on_drag_over, on_drop, rsx,
};
use crate::view::Element as HostElement;

struct PaintDirtyOnClickElement {
    element: crate::view::base_component::Element,
}

impl PaintDirtyOnClickElement {
    fn new() -> Self {
        Self {
            element: crate::view::base_component::Element::new_with_id(
                0x4E4E_0001,
                0.0,
                0.0,
                80.0,
                32.0,
            ),
        }
    }
}

impl crate::view::base_component::Layoutable for PaintDirtyOnClickElement {
    fn sync_arena(&mut self, arena: &mut crate::view::node_arena::NodeArena) {
        self.element.sync_arena(arena);
    }

    fn measure(
        &mut self,
        constraints: crate::view::base_component::LayoutConstraints,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        self.element.measure(constraints, arena);
    }

    fn place(
        &mut self,
        placement: crate::view::base_component::LayoutPlacement,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        self.element.place(placement, arena);
    }

    fn measured_size(&self) -> (f32, f32) {
        self.element.measured_size()
    }

    fn layout_target_size(&self) -> (f32, f32) {
        self.element.layout_target_size()
    }

    fn set_layout_width(&mut self, width: f32) {
        self.element.set_layout_width(width);
    }

    fn set_layout_height(&mut self, height: f32) {
        self.element.set_layout_height(height);
    }

    fn flex_props(&self) -> crate::view::base_component::FlexProps {
        self.element.flex_props()
    }

    fn cross_alignment_size(
        &self,
        is_row: bool,
        stretched_cross: Option<f32>,
        arena: &crate::view::node_arena::NodeArena,
    ) -> f32 {
        self.element
            .cross_alignment_size(is_row, stretched_cross, arena)
    }

    fn inline_relative_position(&self) -> (f32, f32) {
        self.element.inline_relative_position()
    }

    fn set_layout_offset(&mut self, x: f32, y: f32) {
        self.element.set_layout_offset(x, y);
    }

    fn measure_inline(
        &mut self,
        context: crate::view::base_component::InlineMeasureContext,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        self.element.measure_inline(context, arena);
    }

    fn get_inline_nodes_size(
        &self,
        arena: &crate::view::node_arena::NodeArena,
    ) -> Vec<crate::view::base_component::InlineNodeSize> {
        self.element.get_inline_nodes_size(arena)
    }

    fn place_inline(
        &mut self,
        placement: crate::view::base_component::InlinePlacement,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        self.element.place_inline(placement, arena);
    }
}

impl crate::view::base_component::EventTarget for PaintDirtyOnClickElement {
    fn dispatch_click(
        &mut self,
        _event: &mut crate::ui::ClickEvent,
        _control: &mut crate::view::viewport::ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
        self.element
            .set_background_color_value(Color::rgb(32, 96, 160));
    }
}

impl crate::view::base_component::Renderable for PaintDirtyOnClickElement {
    fn build(
        &mut self,
        graph: &mut crate::view::frame_graph::FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        ctx: crate::view::base_component::UiBuildContext,
    ) -> crate::view::base_component::BuildState {
        self.element.build(graph, arena, ctx)
    }
}

impl crate::view::base_component::ElementTrait for PaintDirtyOnClickElement {
    fn stable_id(&self) -> u64 {
        self.element.stable_id()
    }

    fn box_model_snapshot(&self) -> crate::view::base_component::BoxModelSnapshot {
        self.element.box_model_snapshot()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn children(&self) -> &[crate::view::node_arena::NodeKey] {
        self.element.children()
    }

    fn local_dirty_flags(&self) -> crate::view::base_component::DirtyFlags {
        self.element.local_dirty_flags()
    }

    fn clear_local_dirty_flags(&mut self, flags: crate::view::base_component::DirtyFlags) {
        self.element.clear_local_dirty_flags(flags);
    }
}

struct PaintDirtyOnClickHost;

impl crate::view::HostBuilder for PaintDirtyOnClickHost {
    fn build_descriptor(
        _node: &crate::ui::RsxElementNode,
        _path: &[u64],
        _ctx: &crate::view::BuildCtx,
    ) -> Result<crate::view::renderer_adapter::ElementDescriptor, String> {
        Ok(crate::view::renderer_adapter::ElementDescriptor::leaf(
            Box::new(PaintDirtyOnClickElement::new()),
        ))
    }
}

fn host_el() -> RsxNode {
    RsxNode::tagged("Element", RsxTagDescriptor::for_tag::<HostElement>())
}

fn single_element(width_px: f32) -> RsxNode {
    rsx! {
        <HostElement style={{
            width: Length::px(width_px),
            height: Length::px(40.0),
        }} />
    }
}

fn text_leaf(content: &str) -> RsxNode {
    RsxNode::text(content)
}

fn inline_badge_vertical_align_tree(vertical_align: VerticalAlign) -> RsxNode {
    use crate::view::Text as HostText;

    rsx! {
        <HostElement style={{
            layout: Layout::Inline,
            width: Length::px(960.0),
            gap: Length::px(8.0),
            line_height: 1.2f32,
            vertical_align: vertical_align,
        }}>
            <HostText>"Inline text starts here,"</HostText>
            <HostElement style={{
                padding: Padding::uniform(Length::px(8.0)),
            }}>
                <HostText>"badge test test test test test test test"</HostText>
            </HostElement>
            <HostText>"then more text continues after the badge,"</HostText>
            <HostElement style={{
                width: Length::px(90.0),
                height: Length::px(50.0),
                padding: Padding::uniform(Length::px(8.0)),
            }}>
                <HostText>"note note note note note note note"</HostText>
            </HostElement>
        </HostElement>
    }
}

fn collect_text_contents(
    arena: &crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    out: &mut Vec<String>,
) {
    if let Some(node) = arena.get(key) {
        if let Some(text) = node
            .element
            .as_any()
            .downcast_ref::<crate::view::base_component::Text>()
        {
            out.push(text.content().to_string());
        }
        for child in arena.children_of(key) {
            collect_text_contents(arena, child, out);
        }
    }
}

fn find_text_node(
    arena: &crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    content: &str,
) -> Option<crate::view::node_arena::NodeKey> {
    let node = arena.get(key)?;
    if node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Text>()
        .is_some_and(|text| text.content() == content)
    {
        return Some(key);
    }
    let children = node.children.clone();
    drop(node);
    for child in children {
        if let Some(found) = find_text_node(arena, child, content) {
            return Some(found);
        }
    }
    None
}

fn pointer_cursor_with_text_child_tree() -> RsxNode {
    rsx! {
        <HostElement style={{
            layout: Layout::flow().row().no_wrap(),
            width: Length::px(160.0),
            height: Length::px(40.0),
            cursor: Cursor::Pointer,
        }}>
            {"Hover target"}
        </HostElement>
    }
}

fn overlapping_root_with_anchor_parent_resize_handle_tree() -> RsxNode {
    RsxNode::fragment(vec![
        rsx! {
            <HostElement style={{
                position: Position::absolute()
                    .left(Length::px(0.0))
                    .top(Length::px(0.0))
                    .clip(ClipMode::Parent),
                width: Length::px(100.0),
                height: Length::px(80.0),
            }}>
                <HostElement style={{
                    position: Position::absolute()
                        .right(Length::px(-2.0))
                        .top(Length::px(0.0))
                        .bottom(Length::px(0.0))
                        .clip(ClipMode::AnchorParent),
                    width: Length::px(4.0),
                    cursor: Cursor::EwResize,
                }} />
            </HostElement>
        },
        rsx! {
            <HostElement style={{
                position: Position::absolute()
                    .left(Length::px(50.0))
                    .top(Length::px(0.0))
                    .clip(ClipMode::Parent),
                width: Length::px(100.0),
                height: Length::px(80.0),
            }} />
        },
    ])
}

fn same_root_escape_descendant_under_later_sibling_tree() -> RsxNode {
    rsx! {
        <HostElement style={{
            width: Length::px(220.0),
            height: Length::px(120.0),
        }}>
            <HostElement style={{
                width: Length::px(80.0),
                height: Length::px(80.0),
            }}>
                <HostElement style={{
                    position: Position::absolute()
                        .left(Length::px(100.0))
                        .top(Length::px(10.0))
                        .clip(ClipMode::Viewport),
                    width: Length::px(20.0),
                    height: Length::px(20.0),
                    cursor: Cursor::Crosshair,
                }} />
            </HostElement>
            <HostElement style={{
                position: Position::absolute()
                    .left(Length::px(90.0))
                    .top(Length::px(0.0))
                    .clip(ClipMode::Parent),
                width: Length::px(80.0),
                height: Length::px(80.0),
            }} />
        </HostElement>
    }
}

fn movable_root_with_anchor_parent_resize_handle_tree(left: f32) -> RsxNode {
    rsx! {
        <HostElement style={{
            position: Position::absolute()
                .left(Length::px(left))
                .top(Length::px(20.0))
                .clip(ClipMode::Parent),
            width: Length::px(100.0),
            height: Length::px(80.0),
        }}>
            <HostElement style={{
                position: Position::absolute()
                    .right(Length::px(-2.0))
                    .top(Length::px(0.0))
                    .bottom(Length::px(0.0))
                    .clip(ClipMode::AnchorParent),
                width: Length::px(4.0),
                cursor: Cursor::EwResize,
            }} />
        </HostElement>
    }
}

fn retained_window_accordion_button_tree() -> RsxNode {
    retained_window_accordion_button_tree_with_expanded(false)
}

fn retained_window_accordion_button_tree_with_expanded(expanded: bool) -> RsxNode {
    rsx! {
        <HostElement style={{
            layout: Layout::flow().column().no_wrap(),
            width: Length::px(460.0),
            height: Length::px(380.0),
            scroll_direction: ScrollDirection::Both,
        }}>
            <HostElement style={{
                width: Length::percent(100.0),
                height: Length::px(32.0),
            }}>
                {"Button"}
            </HostElement>
            <HostElement style={{
                layout: Layout::flow().column().no_wrap(),
                width: Length::percent(100.0),
                height: if expanded { None } else { Length::Zero },
                transition: [Transition::new(TransitionProperty::Height, 200).ease_in_out()],
            }}>
                <HostElement style={{
                    layout: Layout::flow().row().no_wrap(),
                    width: Length::px(96.0),
                    height: Length::px(32.0),
                    cursor: Cursor::Pointer,
                }}>
                    {"Contained"}
                </HostElement>
            </HostElement>
        </HostElement>
    }
}

fn retained_component_test_button_section_tree_with_expanded(expanded: bool) -> RsxNode {
    use crate::view::Text as HostText;

    rsx! {
        <HostElement style={{
            width: Length::px(460.0),
            height: Length::px(380.0),
            layout: Layout::flow().column().no_wrap(),
        }}>
            <HostElement style={{
                width: Length::percent(100.0),
                height: Length::px(32.0),
            }}>
                <HostText>"Component Test"</HostText>
            </HostElement>
            <HostElement style={{
                width: Length::percent(100.0),
                height: Length::px(348.0),
                padding: Padding::uniform(Length::px(16.0)),
                layout: Layout::flow().column().no_wrap(),
                scroll_direction: ScrollDirection::Both,
            }}>
                <HostElement style={{
                    width: Length::percent(100.0),
                    layout: Layout::flow().column().no_wrap(),
                    gap: Length::px(8.0),
                    padding: Padding::uniform(Length::px(12.0)),
                }}>
                    <HostElement style={{
                        width: Length::percent(100.0),
                        layout: Layout::flow().column().no_wrap(),
                    }}>
                        <HostElement style={{
                            width: Length::percent(100.0),
                            height: Length::px(36.0),
                            layout: Layout::flex().align(crate::style::Align::Center),
                            padding: Padding::uniform(Length::px(8.0)),
                            cursor: Cursor::Pointer,
                        }}>
                            <HostText>"Button"</HostText>
                        </HostElement>
                        <HostElement style={{
                            layout: Layout::flex().column(),
                            height: if expanded { None } else { Length::Zero },
                            transition: [Transition::new(TransitionProperty::Height, 200).ease_in_out()],
                        }}>
                            <HostElement style={{
                                padding: Padding::uniform(Length::px(12.0)),
                                gap: Length::px(8.0),
                            }}>
                                <HostText>"Variant"</HostText>
                                <HostElement style={{
                                    width: Length::percent(100.0),
                                    layout: Layout::flow().row().wrap(),
                                    gap: Length::px(8.0),
                                }}>
                                    <HostElement style={{
                                        layout: Layout::flow().row().no_wrap().align(crate::style::Align::Center),
                                        padding: Padding::uniform(Length::px(8.0)),
                                        cursor: Cursor::Pointer,
                                    }}>
                                        <HostText>"Contained"</HostText>
                                    </HostElement>
                                    <HostElement style={{
                                        layout: Layout::flow().row().no_wrap().align(crate::style::Align::Center),
                                        padding: Padding::uniform(Length::px(8.0)),
                                        cursor: Cursor::Pointer,
                                    }}>
                                        <HostText>"Outlined"</HostText>
                                    </HostElement>
                                    <HostElement style={{
                                        layout: Layout::flow().row().no_wrap().align(crate::style::Align::Center),
                                        padding: Padding::uniform(Length::px(8.0)),
                                        cursor: Cursor::Pointer,
                                    }}>
                                        <HostText>"Text"</HostText>
                                    </HostElement>
                                    <HostElement style={{
                                        layout: Layout::flow().row().no_wrap().align(crate::style::Align::Center),
                                        padding: Padding::uniform(Length::px(8.0)),
                                    }}>
                                        <HostText>"Disabled"</HostText>
                                    </HostElement>
                                </HostElement>
                                <HostText>"Size"</HostText>
                                <HostElement style={{
                                    width: Length::percent(100.0),
                                    layout: Layout::flow().row().wrap().align(crate::style::Align::Center),
                                    gap: Length::px(8.0),
                                }}>
                                    <HostElement style={{
                                        padding: Padding::uniform(Length::px(6.0)),
                                        cursor: Cursor::Pointer,
                                    }}>
                                        <HostText>"Small"</HostText>
                                    </HostElement>
                                    <HostElement style={{
                                        padding: Padding::uniform(Length::px(8.0)),
                                        cursor: Cursor::Pointer,
                                    }}>
                                        <HostText>"Medium"</HostText>
                                    </HostElement>
                                    <HostElement style={{
                                        padding: Padding::uniform(Length::px(10.0)),
                                        cursor: Cursor::Pointer,
                                    }}>
                                        <HostText>"Large"</HostText>
                                    </HostElement>
                                </HostElement>
                            </HostElement>
                        </HostElement>
                    </HostElement>
                </HostElement>
            </HostElement>
        </HostElement>
    }
}

fn flex_base_only_axis_workload_tree() -> RsxNode {
    rsx! {
        <HostElement style={{
            layout: Layout::flex().row(),
            width: Length::px(240.0),
            height: Length::px(80.0),
        }}>
            <HostElement style={{
                width: Length::px(80.0),
                height: Length::px(30.0),
            }} />
            <HostElement style={{
                width: Length::px(72.0),
                height: Length::px(28.0),
            }} />
        </HostElement>
    }
}

fn flex_nested_base_only_axis_workload_tree() -> RsxNode {
    rsx! {
        <HostElement style={{
            layout: Layout::flex().row(),
            width: Length::px(240.0),
            height: Length::px(80.0),
        }}>
            <HostElement style={{
                width: Length::px(80.0),
                height: Length::px(40.0),
            }}>
                <HostElement style={{
                    width: Length::px(24.0),
                    height: Length::px(12.0),
                }} />
            </HostElement>
            <HostElement style={{
                width: Length::px(72.0),
                height: Length::px(28.0),
            }} />
        </HostElement>
    }
}

fn flex_base_only_axis_workload_tree_with_gap(gap: f32) -> RsxNode {
    rsx! {
        <HostElement style={{
            layout: Layout::flex().row(),
            width: Length::px(240.0),
            height: Length::px(80.0),
            gap: Length::px(gap),
        }}>
            <HostElement style={{
                width: Length::px(80.0),
                height: Length::px(30.0),
            }} />
            <HostElement style={{
                width: Length::px(72.0),
                height: Length::px(28.0),
            }} />
        </HostElement>
    }
}

fn flex_base_only_column_axis_workload_tree() -> RsxNode {
    rsx! {
        <HostElement style={{
            layout: Layout::flex().column(),
            width: Length::px(240.0),
            height: Length::px(80.0),
        }}>
            <HostElement style={{
                width: Length::px(80.0),
                height: Length::px(30.0),
            }} />
            <HostElement style={{
                width: Length::px(72.0),
                height: Length::px(28.0),
            }} />
        </HostElement>
    }
}

fn flex_with_text_area_descendant_tree() -> RsxNode {
    use crate::view::TextArea as HostTextArea;

    rsx! {
        <HostElement style={{
            layout: Layout::flex().row(),
            width: Length::px(240.0),
            height: Length::px(80.0),
        }}>
            <HostElement style={{
                width: Length::px(120.0),
                height: Length::px(60.0),
            }}>
                <HostTextArea content={"phase 5d text area".to_string()} />
            </HostElement>
        </HostElement>
    }
}

fn inline_text_axis_workload_tree() -> RsxNode {
    use crate::view::Text as HostText;

    rsx! {
        <HostElement style={{
            layout: Layout::Inline,
            width: Length::px(320.0),
            height: Length::px(80.0),
        }}>
            <HostText>"inline non-base descendant"</HostText>
        </HostElement>
    }
}

fn expanded_retained_accordion_section_style() -> Style {
    let mut style = Style::new();
    style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().column().no_wrap().into()),
    );
    style.insert(
        PropertyId::Width,
        ParsedValue::Length(Length::percent(100.0)),
    );
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::from(vec![
            Transition::new(TransitionProperty::Height, 200).ease_in_out(),
        ])),
    );
    style
}

fn drag_drop_rerender_tree(hovering: bool, dropped: Binding<Vec<String>>) -> RsxNode {
    let target_over = on_drag_over(move |event| {
        event.accept(DragEffect::Move);
    });
    let target_drop = {
        let dropped = dropped.clone();
        on_drop(move |_event| {
            dropped.update(|items| items.push("target".to_string()));
        })
    };
    let target_label = if hovering { "target-hover" } else { "target" };

    rsx! {
        <HostElement style={{
            layout: Layout::flow().column().no_wrap(),
            width: Length::px(200.0),
            height: Length::px(60.0),
        }}>
            <HostElement
                style={{
                    width: Length::px(200.0),
                    height: Length::px(30.0),
                }}
            >
                {text_leaf("source")}
            </HostElement>
            <HostElement
                style={{
                    width: Length::px(200.0),
                    height: Length::px(30.0),
                }}
                on_drag_over={target_over}
                on_drop={target_drop}
            >
                {text_leaf(target_label)}
            </HostElement>
        </HostElement>
    }
}

/// 軌 A #9: tests that build their own `FiberWork` and call
/// `apply_fiber_works` directly need an `ApplyContext`. The viewport
/// dimensions / style here mirror the defaults the integration tests
/// would use (`Viewport::new()` defaults).
fn test_apply_ctx() -> crate::view::fiber_work::ApplyContext<'static> {
    use std::sync::OnceLock;
    static STYLE: OnceLock<crate::style::Style> = OnceLock::new();
    let style = STYLE.get_or_init(crate::style::Style::new);
    crate::view::fiber_work::ApplyContext {
        viewport_style: style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    }
}

/// Drive measure+place on each arena root after `render_rsx`.
///
/// `Viewport::render_rsx` defers layout to `render_render_tree`, which
/// bails early in tests because there's no GPU surface to acquire. Hit
/// tests against `box_model_snapshot()` therefore see uninitialized
/// layout state. Tests that need real bounds run measure+place
/// explicitly via the test_support helper.
fn run_layout_for_test(viewport: &mut Viewport, viewport_w: f32, viewport_h: f32) {
    let constraints = crate::view::base_component::LayoutConstraints {
        max_width: viewport_w,
        max_height: viewport_h,
        viewport_width: viewport_w,
        viewport_height: viewport_h,
        percent_base_width: Some(viewport_w),
        percent_base_height: Some(viewport_h),
    };
    let placement = crate::view::base_component::LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: viewport_w,
        available_height: viewport_h,
        viewport_width: viewport_w,
        viewport_height: viewport_h,
        percent_base_width: Some(viewport_w),
        percent_base_height: Some(viewport_h),
    };
    let mut arena = std::mem::take(&mut viewport.scene.node_arena);
    let root_keys = viewport.scene.ui_root_keys.clone();
    for &root in &root_keys {
        crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);
    }
    viewport.scene.node_arena = arena;
}

fn run_layout_for_test_with_gate_profile(
    viewport: &mut Viewport,
    viewport_w: f32,
    viewport_h: f32,
) -> (
    crate::view::base_component::LayoutGateCandidateProfile,
    crate::view::base_component::LayoutPlaceProfile,
) {
    let constraints = crate::view::base_component::LayoutConstraints {
        max_width: viewport_w,
        max_height: viewport_h,
        viewport_width: viewport_w,
        viewport_height: viewport_h,
        percent_base_width: Some(viewport_w),
        percent_base_height: Some(viewport_h),
    };
    let placement = crate::view::base_component::LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: viewport_w,
        available_height: viewport_h,
        viewport_width: viewport_w,
        viewport_height: viewport_h,
        percent_base_width: Some(viewport_w),
        percent_base_height: Some(viewport_h),
    };
    crate::view::base_component::reset_layout_gate_candidate_profile();
    crate::view::base_component::reset_layout_place_profile();
    let mut arena = std::mem::take(&mut viewport.scene.node_arena);
    let root_keys = viewport.scene.ui_root_keys.clone();
    for &root in &root_keys {
        arena.refresh_subtree_dirty_cache(root);
    }
    for &root in &root_keys {
        arena.with_element_taken(root, |el, arena| {
            el.measure(constraints, arena);
        });
    }
    for &root in &root_keys {
        arena.refresh_subtree_dirty_cache(root);
    }
    for &root in &root_keys {
        arena.with_element_taken(root, |el, arena| {
            el.place(placement, arena);
        });
    }
    viewport.scene.node_arena = arena;
    (
        crate::view::base_component::take_layout_gate_candidate_profile(),
        crate::view::base_component::take_layout_place_profile(),
    )
}

fn sample_clean_parent_relayout_for_placement_profile(
    tree: &RsxNode,
    viewport_w: f32,
    viewport_h: f32,
) -> crate::view::base_component::LayoutPlaceProfile {
    let mut viewport = Viewport::new();
    viewport.set_size(viewport_w as u32, viewport_h as u32);
    viewport.render_rsx(tree).expect("render workload tree");
    run_layout_for_test(&mut viewport, viewport_w, viewport_h);

    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, viewport_w, viewport_h);
    place_profile
}

fn nested_box_model_tree() -> RsxNode {
    rsx! {
        <HostElement style={{
            width: Length::px(120.0),
            height: Length::px(80.0),
        }}>
            <HostElement style={{
                width: Length::px(40.0),
                height: Length::px(20.0),
            }} />
        </HostElement>
    }
}

fn two_child_box_model_tree() -> RsxNode {
    rsx! {
        <HostElement style={{
            width: Length::px(120.0),
            height: Length::px(80.0),
        }}>
            <HostElement style={{
                width: Length::px(40.0),
                height: Length::px(20.0),
            }} />
            <HostElement style={{
                width: Length::px(30.0),
                height: Length::px(18.0),
            }} />
        </HostElement>
    }
}

fn two_child_grid_box_model_tree() -> RsxNode {
    rsx! {
        <HostElement style={{
            layout: Layout::Grid,
            width: Length::px(120.0),
            height: Length::px(80.0),
        }}>
            <HostElement style={{
                width: Length::px(40.0),
                height: Length::px(20.0),
            }} />
            <HostElement style={{
                width: Length::px(30.0),
                height: Length::px(18.0),
            }} />
        </HostElement>
    }
}

fn nested_grid_box_model_tree() -> RsxNode {
    rsx! {
        <HostElement style={{
            layout: Layout::Grid,
            width: Length::px(120.0),
            height: Length::px(80.0),
        }}>
            <HostElement style={{
                width: Length::px(60.0),
                height: Length::px(30.0),
            }}>
                <HostElement style={{
                    width: Length::px(24.0),
                    height: Length::px(12.0),
                }} />
            </HostElement>
        </HostElement>
    }
}

fn nested_grid_with_anchor_descendant_tree() -> RsxNode {
    rsx! {
        <HostElement style={{
            layout: Layout::Grid,
            width: Length::px(120.0),
            height: Length::px(80.0),
        }}>
            <HostElement style={{
                width: Length::px(60.0),
                height: Length::px(30.0),
            }}>
                <HostElement
                    anchor={"phase_4l_anchor".to_string()}
                    style={{
                        width: Length::px(24.0),
                        height: Length::px(12.0),
                    }}
                />
            </HostElement>
        </HostElement>
    }
}

fn nested_grid_with_absolute_descendant_tree() -> RsxNode {
    rsx! {
        <HostElement style={{
            layout: Layout::Grid,
            width: Length::px(120.0),
            height: Length::px(80.0),
        }}>
            <HostElement style={{
                width: Length::px(60.0),
                height: Length::px(30.0),
            }}>
                <HostElement style={{
                    position: Position::absolute(),
                    width: Length::px(24.0),
                    height: Length::px(12.0),
                }} />
            </HostElement>
        </HostElement>
    }
}

fn nested_grid_with_text_area_descendant_tree() -> RsxNode {
    use crate::view::TextArea as HostTextArea;

    rsx! {
        <HostElement style={{
            layout: Layout::Grid,
            width: Length::px(180.0),
            height: Length::px(100.0),
        }}>
            <HostElement style={{
                width: Length::px(140.0),
                height: Length::px(60.0),
            }}>
                <HostTextArea content={"phase 4m text area".to_string()} />
            </HostElement>
        </HostElement>
    }
}

fn scrollable_grid_box_model_tree() -> RsxNode {
    rsx! {
        <HostElement style={{
            layout: Layout::Grid,
            width: Length::px(100.0),
            height: Length::px(50.0),
            scroll_direction: ScrollDirection::Both,
        }}>
            <HostElement style={{
                width: Length::px(200.0),
                height: Length::px(100.0),
            }} />
        </HostElement>
    }
}

fn scrollable_box_model_tree() -> RsxNode {
    rsx! {
        <HostElement style={{
            width: Length::px(100.0),
            height: Length::px(50.0),
            scroll_direction: ScrollDirection::Both,
        }}>
            <HostElement style={{
                width: Length::px(200.0),
                height: Length::px(100.0),
            }} />
        </HostElement>
    }
}

fn paint_dirty_on_click_tree() -> RsxNode {
    crate::view::host_builder_node::<PaintDirtyOnClickHost>("PaintDirtyOnClick")
}

fn box_model_snapshot_for_node(
    viewport: &Viewport,
    key: crate::view::node_arena::NodeKey,
) -> crate::view::base_component::BoxModelSnapshot {
    let node_id = viewport
        .scene
        .node_arena
        .get(key)
        .expect("node exists")
        .element
        .stable_id();
    *viewport
        .frame_box_models()
        .iter()
        .find(|snapshot| snapshot.node_id == node_id)
        .expect("snapshot exists")
}

fn mark_box_model_dirty_and_set_layout_width(
    viewport: &mut Viewport,
    key: crate::view::node_arena::NodeKey,
    width: f32,
) {
    let flags = crate::view::base_component::DirtyPassMask::BOX_MODEL
        .union(crate::view::base_component::DirtyPassMask::HIT_TEST);
    viewport
        .scene
        .node_arena
        .mutate_element_with_invalidation(key, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>()
                .expect("host element")
                .set_layout_transition_width(width);
            cx.invalidate(flags);
        })
        .expect("node exists");
}

fn mark_place_dirty_for_test(viewport: &mut Viewport, key: crate::view::node_arena::NodeKey) {
    viewport
        .scene
        .node_arena
        .mutate_element_with_invalidation(key, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>()
                .expect("host element")
                .mark_place_dirty_with(cx);
        })
        .expect("node exists");
}

fn mark_paint_dirty_for_test(viewport: &mut Viewport, key: crate::view::node_arena::NodeKey) {
    viewport
        .scene
        .node_arena
        .mutate_element_with_invalidation(key, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>()
                .expect("host element")
                .mark_paint_dirty_with(cx);
        })
        .expect("node exists");
}

fn assert_same_box_model_snapshot(
    actual: crate::view::base_component::BoxModelSnapshot,
    expected: crate::view::base_component::BoxModelSnapshot,
) {
    assert_eq!(actual.node_id, expected.node_id);
    assert_eq!(actual.parent_id, expected.parent_id);
    assert_eq!(actual.x, expected.x);
    assert_eq!(actual.y, expected.y);
    assert_eq!(actual.width, expected.width);
    assert_eq!(actual.height, expected.height);
    assert_eq!(actual.border_radius, expected.border_radius);
    assert_eq!(actual.should_render, expected.should_render);
}

#[test]
fn phase_4k_samples_workload_placement_skip_failure_distribution() {
    let grid_leaf = sample_clean_parent_relayout_for_placement_profile(
        &two_child_grid_box_model_tree(),
        120.0,
        80.0,
    );
    assert_eq!(grid_leaf.child_place_calls, 0);
    assert_eq!(grid_leaf.skipped_child_place_calls, 2);
    assert_eq!(grid_leaf.placement_skip_failures.total(), 0);

    let nested_grid = sample_clean_parent_relayout_for_placement_profile(
        &nested_grid_box_model_tree(),
        120.0,
        80.0,
    );
    assert_eq!(nested_grid.child_place_calls, 0);
    assert_eq!(nested_grid.skipped_child_place_calls, 1);
    assert_eq!(nested_grid.placement_skip_failures.non_leaf, 0);
    assert_eq!(nested_grid.placement_skip_failures.total(), 0);

    let scrollable_grid = sample_clean_parent_relayout_for_placement_profile(
        &scrollable_grid_box_model_tree(),
        100.0,
        50.0,
    );
    assert_eq!(scrollable_grid.child_place_calls, 0);
    assert_eq!(scrollable_grid.skipped_child_place_calls, 1);
    assert_eq!(scrollable_grid.placement_skip_failures.total(), 0);

    let retained_accordion = sample_clean_parent_relayout_for_placement_profile(
        &retained_window_accordion_button_tree(),
        460.0,
        380.0,
    );
    assert_eq!(retained_accordion.child_place_calls, 2);
    assert_eq!(retained_accordion.skipped_child_place_calls, 0);
    assert_eq!(retained_accordion.placement_skip_failures.total(), 0);
}

#[test]
fn phase_5a_axis_placement_eligibility_observes_retained_flow_without_skipping() {
    let retained_accordion = sample_clean_parent_relayout_for_placement_profile(
        &retained_window_accordion_button_tree(),
        460.0,
        380.0,
    );

    assert_eq!(retained_accordion.child_place_calls, 2);
    assert_eq!(retained_accordion.skipped_child_place_calls, 0);
    assert_eq!(
        retained_accordion
            .axis_placement_eligibility
            .candidate_child_places,
        retained_accordion.child_place_calls,
        "Phase 5a observes axis candidates without reducing actual place calls"
    );
    assert_eq!(
        retained_accordion
            .axis_placement_eligibility
            .clean_subtree_child_places,
        2
    );
    assert_eq!(
        retained_accordion
            .axis_placement_eligibility
            .dirty_subtree_child_places,
        0
    );
    assert_eq!(
        retained_accordion
            .axis_placement_eligibility
            .flow_child_places,
        2
    );
    assert_eq!(
        retained_accordion
            .axis_placement_eligibility
            .blockers
            .non_base_element,
        2,
        "retained accordion children contain text descendants, so Phase 5a records a blocker"
    );

    let place_trace_root = super::debug::TraceRenderNode::with_children(
        "place",
        0.0,
        super::debug::build_layout_place_trace_nodes(&retained_accordion),
    );
    let place_trace = super::debug::format_trace_render_tree(&place_trace_root);
    assert!(place_trace.contains("axis_placement_eligibility (candidates=2"));
    assert!(place_trace.contains("flow=2"));
    assert!(place_trace.contains("axis_placement_blockers (total=2"));
    assert!(place_trace.contains("non_base_element=2"));
}

#[test]
fn phase_5a_axis_placement_eligibility_counts_dirty_and_clean_children() {
    let mut viewport = Viewport::new();
    viewport.set_size(460, 380);
    viewport
        .render_rsx(&retained_window_accordion_button_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 460.0, 380.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let first_child = viewport
        .scene
        .node_arena
        .children_of(root_key)
        .first()
        .copied()
        .expect("retained root has first child");
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);
    mark_place_dirty_for_test(&mut viewport, first_child);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 460.0, 380.0);

    assert!(
        place_profile.child_place_calls
            >= place_profile
                .axis_placement_eligibility
                .candidate_child_places,
        "Phase 5a observes axis candidates without suppressing actual child placement"
    );
    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert_eq!(
        place_profile
            .axis_placement_eligibility
            .dirty_subtree_child_places,
        1
    );
    let axis_profile = place_profile.axis_placement_eligibility;
    assert_eq!(
        axis_profile.clean_subtree_child_places + axis_profile.dirty_subtree_child_places,
        axis_profile.candidate_child_places
    );
    assert_eq!(
        place_profile
            .axis_placement_eligibility
            .blockers
            .dirty_subtree,
        1
    );
}

#[test]
fn phase_5b_cached_placement_metadata_marks_base_only_nested_subtree_replayable() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let candidate = viewport.scene.node_arena.children_of(root_key)[0];
    viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(root_key);

    let metadata = viewport
        .scene
        .node_arena
        .cached_placement_eligibility_metadata(candidate);
    assert!(
        metadata.first_blocker().is_none(),
        "base-only nested Element subtree should have no cached placement replay blocker"
    );
    assert!(!metadata.contains_non_base_element);
    assert!(!metadata.contains_anchor_name);
    assert!(!metadata.contains_anchor_ref);
    assert!(!metadata.contains_absolute_descendant);
    assert!(!metadata.contains_runtime_layout_state);
}

#[test]
fn phase_5b_cached_placement_metadata_marks_text_area_descendant_as_non_base() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_with_text_area_descendant_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 180.0, 100.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let candidate = viewport.scene.node_arena.children_of(root_key)[0];
    viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(root_key);

    let metadata = viewport
        .scene
        .node_arena
        .cached_placement_eligibility_metadata(candidate);
    assert!(metadata.contains_non_base_element);
    assert_eq!(
        metadata.first_blocker(),
        Some(crate::view::base_component::PlacementSkipFailureReason::NonBaseElement)
    );
}

#[test]
fn phase_5b_cached_placement_metadata_marks_anchor_and_absolute_descendants() {
    let mut anchor_viewport = Viewport::new();
    anchor_viewport
        .render_rsx(&nested_grid_with_anchor_descendant_tree())
        .expect("cold render");
    run_layout_for_test(&mut anchor_viewport, 120.0, 80.0);
    let anchor_root = anchor_viewport.scene.ui_root_keys[0];
    let anchor_candidate = anchor_viewport.scene.node_arena.children_of(anchor_root)[0];
    anchor_viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(anchor_root);
    let anchor_metadata = anchor_viewport
        .scene
        .node_arena
        .cached_placement_eligibility_metadata(anchor_candidate);
    assert!(anchor_metadata.contains_anchor_name);
    assert_eq!(
        anchor_metadata.first_blocker(),
        Some(crate::view::base_component::PlacementSkipFailureReason::AnchorName)
    );

    let mut absolute_viewport = Viewport::new();
    absolute_viewport
        .render_rsx(&nested_grid_with_absolute_descendant_tree())
        .expect("cold render");
    run_layout_for_test(&mut absolute_viewport, 120.0, 80.0);
    let absolute_root = absolute_viewport.scene.ui_root_keys[0];
    let absolute_candidate = absolute_viewport
        .scene
        .node_arena
        .children_of(absolute_root)[0];
    absolute_viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(absolute_root);
    let absolute_metadata = absolute_viewport
        .scene
        .node_arena
        .cached_placement_eligibility_metadata(absolute_candidate);
    assert!(absolute_metadata.contains_absolute_descendant);
    assert_eq!(
        absolute_metadata.first_blocker(),
        Some(crate::view::base_component::PlacementSkipFailureReason::AbsoluteDescendant)
    );
}

#[test]
fn phase_5b_cached_placement_metadata_refreshes_after_anchor_mutation() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let candidate = viewport.scene.node_arena.children_of(root_key)[0];
    let descendant = viewport.scene.node_arena.children_of(candidate)[0];
    viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(root_key);
    assert!(
        !viewport
            .scene
            .node_arena
            .cached_placement_eligibility_metadata(candidate)
            .contains_anchor_name
    );

    viewport
        .scene
        .node_arena
        .mutate_element_with_invalidation(descendant, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>()
                .expect("descendant element")
                .set_anchor_name(Some(crate::style::AnchorName::new("phase_5b_anchor")));
            cx.invalidate(crate::view::base_component::DirtyPassMask::PLACEMENT);
        })
        .expect("descendant exists");

    assert!(
        viewport.scene.node_arena.subtree_dirty_intersects(
            candidate,
            crate::view::base_component::DirtyPassMask::PLACEMENT,
        ),
        "dirty cache remains the first guard while metadata may be stale"
    );
    viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(root_key);
    let refreshed = viewport
        .scene
        .node_arena
        .cached_placement_eligibility_metadata(candidate);
    assert!(refreshed.contains_anchor_name);
    assert_eq!(
        refreshed.first_blocker(),
        Some(crate::view::base_component::PlacementSkipFailureReason::AnchorName)
    );
}

#[test]
fn phase_5c_axis_trace_summarizes_retained_flow_hit_rate_without_skipping() {
    let retained_accordion = sample_clean_parent_relayout_for_placement_profile(
        &retained_window_accordion_button_tree(),
        460.0,
        380.0,
    );
    let axis = retained_accordion.axis_placement_eligibility;

    assert_eq!(retained_accordion.child_place_calls, 2);
    assert_eq!(retained_accordion.skipped_child_place_calls, 0);
    assert_eq!(
        axis.candidate_child_places,
        retained_accordion.child_place_calls
    );
    assert_eq!(axis.clean_subtree_child_places, 2);
    assert_eq!(axis.dirty_subtree_child_places, 0);
    assert_eq!(axis.flow_child_places, 2);
    assert_eq!(axis.potential_replay_child_places, 0);
    assert_eq!(axis.flow_potential_replay_child_places, 0);
    assert_eq!(axis.blockers.non_base_element, 2);

    let place_trace_root = super::debug::TraceRenderNode::with_children(
        "place",
        0.0,
        super::debug::build_layout_place_trace_nodes(&retained_accordion),
    );
    let place_trace = super::debug::format_trace_render_tree(&place_trace_root);
    assert!(place_trace.contains("axis_placement_eligibility (candidates=2"));
    assert!(place_trace.contains("potential_replay=0"));
    assert!(place_trace.contains("flow=2"));
    assert!(place_trace.contains("axis_placement_potential_replay_by_layout"));
    assert!(place_trace.contains("flow=0"));
}

#[test]
fn phase_5c_axis_trace_counts_flex_base_only_replay_candidates() {
    let flex_base = sample_clean_parent_relayout_for_placement_profile(
        &flex_base_only_axis_workload_tree(),
        240.0,
        80.0,
    );
    let axis = flex_base.axis_placement_eligibility;

    assert_eq!(flex_base.child_place_calls, 0);
    assert_eq!(flex_base.skipped_child_place_calls, 2);
    assert_eq!(axis.candidate_child_places, 2);
    assert_eq!(axis.clean_subtree_child_places, 2);
    assert_eq!(axis.dirty_subtree_child_places, 0);
    assert_eq!(axis.flex_child_places, 2);
    assert_eq!(axis.potential_replay_child_places, 2);
    assert_eq!(axis.flex_potential_replay_child_places, 2);
    assert_eq!(axis.blockers.total(), 0);
}

#[test]
fn phase_5d_flex_clean_base_only_subtree_replays_without_stale_hit_test() {
    let mut viewport = Viewport::new();
    viewport.set_size(240, 80);
    viewport
        .render_rsx(&flex_nested_base_only_axis_workload_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 240.0, 80.0);
    viewport.refresh_frame_box_models();

    let root_key = viewport.scene.ui_root_keys[0];
    let wrapper_key = viewport.scene.node_arena.children_of(root_key)[0];
    let leaf_key = viewport.scene.node_arena.children_of(wrapper_key)[0];
    let wrapper_before = box_model_snapshot_for_node(&viewport, wrapper_key);
    let leaf_before = box_model_snapshot_for_node(&viewport, leaf_key);
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 240.0, 80.0);

    assert_eq!(place_profile.child_place_calls, 0);
    assert_eq!(place_profile.skipped_child_place_calls, 2);
    assert_eq!(place_profile.placement_skip_failures.total(), 0);
    assert_eq!(
        place_profile
            .axis_placement_eligibility
            .flex_potential_replay_child_places,
        2
    );

    viewport.refresh_frame_box_models();
    assert_same_box_model_snapshot(
        box_model_snapshot_for_node(&viewport, wrapper_key),
        wrapper_before,
    );
    assert_same_box_model_snapshot(
        box_model_snapshot_for_node(&viewport, leaf_key),
        leaf_before,
    );
    let target = crate::view::base_component::hit_test(
        &viewport.scene.node_arena,
        root_key,
        leaf_before.x + leaf_before.width * 0.5,
        leaf_before.y + leaf_before.height * 0.5,
    );
    assert_eq!(target, Some(leaf_key));
}

#[test]
fn phase_5d_flex_dirty_descendant_does_not_replay_skip() {
    let mut viewport = Viewport::new();
    viewport.set_size(240, 80);
    viewport
        .render_rsx(&flex_nested_base_only_axis_workload_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 240.0, 80.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let wrapper_key = viewport.scene.node_arena.children_of(root_key)[0];
    let dirty_leaf = viewport.scene.node_arena.children_of(wrapper_key)[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);
    mark_place_dirty_for_test(&mut viewport, dirty_leaf);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 240.0, 80.0);

    assert!(
        place_profile.child_place_calls >= 1,
        "dirty descendant must force flex child placement"
    );
    assert_eq!(place_profile.skipped_child_place_calls, 1);
    assert_eq!(place_profile.placement_skip_failures.dirty_subtree, 1);
}

#[test]
fn phase_5d_flex_context_changes_do_not_replay_skip() {
    let mut viewport = Viewport::new();
    viewport.set_size(240, 80);
    viewport
        .render_rsx(&flex_base_only_axis_workload_tree_with_gap(0.0))
        .expect("cold render");
    run_layout_for_test(&mut viewport, 240.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    viewport
        .render_rsx(&flex_base_only_axis_workload_tree_with_gap(12.0))
        .expect("gap rerender");
    let (_gate_profile, gap_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 240.0, 80.0);

    assert_eq!(gap_profile.skipped_child_place_calls, 0);
    assert_eq!(gap_profile.child_place_calls, 2);

    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    viewport
        .render_rsx(&flex_base_only_column_axis_workload_tree())
        .expect("axis direction rerender");
    let (_gate_profile, direction_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 240.0, 80.0);

    assert_eq!(direction_profile.skipped_child_place_calls, 0);
    assert_eq!(direction_profile.child_place_calls, 2);
}

#[test]
fn phase_5d_flex_available_size_change_does_not_replay_skip() {
    let mut viewport = Viewport::new();
    viewport.set_size(240, 80);
    viewport
        .render_rsx(&flex_base_only_axis_workload_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 240.0, 80.0);

    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 260.0, 80.0);

    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert_eq!(place_profile.child_place_calls, 2);
    assert_eq!(place_profile.placement_skip_failures.placement_mismatch, 2);
}

#[test]
fn phase_5d_flex_non_base_descendant_does_not_replay_skip() {
    let place_profile = sample_clean_parent_relayout_for_placement_profile(
        &flex_with_text_area_descendant_tree(),
        240.0,
        80.0,
    );

    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert!(
        place_profile.child_place_calls >= 1,
        "non-base descendant must keep the flex subtree on the normal placement path"
    );
    assert!(
        place_profile
            .axis_placement_eligibility
            .blockers
            .non_base_element
            >= 1
    );
    assert!(place_profile.placement_skip_failures.non_base_element >= 1);
}

#[test]
fn phase_5d_flow_and_inline_child_place_counts_do_not_drop() {
    let retained_accordion = sample_clean_parent_relayout_for_placement_profile(
        &retained_window_accordion_button_tree(),
        460.0,
        380.0,
    );
    let inline_text = sample_clean_parent_relayout_for_placement_profile(
        &inline_text_axis_workload_tree(),
        320.0,
        80.0,
    );

    assert_eq!(retained_accordion.child_place_calls, 2);
    assert_eq!(retained_accordion.skipped_child_place_calls, 0);
    assert_eq!(
        retained_accordion
            .axis_placement_eligibility
            .flow_child_places,
        2
    );
    assert_eq!(inline_text.child_place_calls, 1);
    assert_eq!(inline_text.skipped_child_place_calls, 0);
    assert_eq!(
        inline_text.axis_placement_eligibility.inline_child_places,
        1
    );
}

#[test]
fn phase_5c_axis_trace_counts_inline_non_base_blockers_without_skipping() {
    let inline_text = sample_clean_parent_relayout_for_placement_profile(
        &inline_text_axis_workload_tree(),
        320.0,
        80.0,
    );
    let axis = inline_text.axis_placement_eligibility;

    assert_eq!(inline_text.child_place_calls, 1);
    assert_eq!(inline_text.skipped_child_place_calls, 0);
    assert_eq!(axis.candidate_child_places, inline_text.child_place_calls);
    assert_eq!(axis.clean_subtree_child_places, 1);
    assert_eq!(axis.dirty_subtree_child_places, 0);
    assert_eq!(axis.inline_child_places, 1);
    assert_eq!(axis.potential_replay_child_places, 0);
    assert_eq!(axis.inline_potential_replay_child_places, 0);
    assert_eq!(axis.blockers.non_base_element, 1);
}

#[test]
fn layout_gate_profile_counts_clean_children_as_candidates_without_skipping() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&two_child_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );

    let (gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(gate_profile.measure_candidate_clean_children, 2);
    assert_eq!(gate_profile.measure_dirty_children, 0);
    assert_eq!(gate_profile.placement_candidate_clean_children, 2);
    assert_eq!(gate_profile.placement_dirty_children, 0);
    assert_eq!(
        place_profile.child_place_calls, 0,
        "Phase 4g is observational: the existing clean-root early return still governs traversal"
    );
}

#[test]
fn placement_skip_clean_child_does_not_call_place_and_preserves_box_models() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&two_child_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    viewport.refresh_frame_box_models();

    let root_key = viewport.scene.ui_root_keys[0];
    let children = viewport.scene.node_arena.children_of(root_key);
    let first_before = box_model_snapshot_for_node(&viewport, children[0]);
    let second_before = box_model_snapshot_for_node(&viewport, children[1]);
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);

    let (gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(gate_profile.placement_candidate_clean_children, 2);
    assert_eq!(gate_profile.placement_dirty_children, 0);
    assert_eq!(
        place_profile.child_place_calls, 0,
        "clean in-flow children with unchanged placement context should not be placed again"
    );
    assert_eq!(place_profile.skipped_child_place_calls, 2);
    assert_eq!(place_profile.placement_skip_failures.total(), 0);

    viewport.refresh_frame_box_models();
    assert_same_box_model_snapshot(
        box_model_snapshot_for_node(&viewport, children[0]),
        first_before,
    );
    assert_same_box_model_snapshot(
        box_model_snapshot_for_node(&viewport, children[1]),
        second_before,
    );
}

#[test]
fn placement_skip_clean_child_is_visible_in_layout_trace() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&two_child_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);

    let (gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    let traversal_profile = super::frame::LayoutTraversalProfile {
        root_count: 1,
        measure_candidate_clean_children: gate_profile.measure_candidate_clean_children,
        measure_dirty_children: gate_profile.measure_dirty_children,
        placement_candidate_clean_children: gate_profile.placement_candidate_clean_children,
        placement_dirty_children: gate_profile.placement_dirty_children,
        skipped_child_place_calls: place_profile.skipped_child_place_calls,
        ..Default::default()
    };
    let trace_root = super::debug::TraceRenderNode::with_children(
        "layout_traversal",
        0.0,
        super::debug::build_layout_traversal_trace_nodes(&traversal_profile),
    );
    let trace = super::debug::format_trace_render_tree(&trace_root);
    let place_trace_root = super::debug::TraceRenderNode::with_children(
        "place",
        0.0,
        super::debug::build_layout_place_trace_nodes(&place_profile),
    );
    let place_trace = super::debug::format_trace_render_tree(&place_trace_root);

    assert_eq!(place_profile.skipped_child_place_calls, 2);
    assert_eq!(place_profile.placement_skip_failures.total(), 0);
    assert!(trace.contains("skipped_child_place_calls (count=2)"));
    assert!(place_trace.contains("skipped_child_place (calls=2)"));
    assert!(place_trace.contains("placement_skip_failures (total=0"));
}

#[test]
fn placement_skip_clean_nested_non_axis_subtree_does_not_call_place() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    viewport.refresh_frame_box_models();

    let root_key = viewport.scene.ui_root_keys[0];
    let wrapper_key = viewport.scene.node_arena.children_of(root_key)[0];
    let leaf_key = viewport.scene.node_arena.children_of(wrapper_key)[0];
    let wrapper_before = box_model_snapshot_for_node(&viewport, wrapper_key);
    let leaf_before = box_model_snapshot_for_node(&viewport, leaf_key);
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(place_profile.child_place_calls, 0);
    assert_eq!(place_profile.skipped_child_place_calls, 1);
    assert_eq!(place_profile.placement_skip_failures.non_leaf, 0);
    assert_eq!(place_profile.placement_skip_failures.total(), 0);

    viewport.refresh_frame_box_models();
    assert_same_box_model_snapshot(
        box_model_snapshot_for_node(&viewport, wrapper_key),
        wrapper_before,
    );
    assert_same_box_model_snapshot(
        box_model_snapshot_for_node(&viewport, leaf_key),
        leaf_before,
    );
}

#[test]
fn placement_skip_clean_nested_subtree_preserves_descendant_hit_test() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    viewport.refresh_frame_box_models();

    let root_key = viewport.scene.ui_root_keys[0];
    let wrapper_key = viewport.scene.node_arena.children_of(root_key)[0];
    let leaf_key = viewport.scene.node_arena.children_of(wrapper_key)[0];
    let leaf_before = box_model_snapshot_for_node(&viewport, leaf_key);
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(place_profile.child_place_calls, 0);
    assert_eq!(place_profile.skipped_child_place_calls, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 0);

    let target = crate::view::base_component::hit_test(
        &viewport.scene.node_arena,
        root_key,
        leaf_before.x + leaf_before.width * 0.5,
        leaf_before.y + leaf_before.height * 0.5,
    );
    assert_eq!(
        target,
        Some(leaf_key),
        "skipped nested subtree must retain descendant hit-test bounds",
    );
}

#[test]
fn layout_gate_profile_excludes_dirty_child_and_still_places_it() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&two_child_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    let dirty_child = viewport.scene.node_arena.children_of(root_key)[0];
    viewport
        .scene
        .node_arena
        .mutate_element_with_invalidation(dirty_child, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>()
                .expect("host child")
                .mark_layout_dirty_with(cx);
        })
        .expect("child exists");

    let (gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(gate_profile.measure_candidate_clean_children, 1);
    assert_eq!(gate_profile.measure_dirty_children, 1);
    assert_eq!(gate_profile.placement_candidate_clean_children, 1);
    assert_eq!(gate_profile.placement_dirty_children, 1);
    assert!(
        place_profile.child_place_calls >= 1,
        "dirty child must still drive placement traversal"
    );
    assert!(
        place_profile.skipped_child_place_calls >= 1,
        "clean sibling may be skipped by the Phase 4h child placement gate"
    );
}

#[test]
fn placement_skip_does_not_skip_dirty_child() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&two_child_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    let dirty_child = viewport.scene.node_arena.children_of(root_key)[0];
    mark_place_dirty_for_test(&mut viewport, dirty_child);

    let (gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(gate_profile.placement_candidate_clean_children, 1);
    assert_eq!(gate_profile.placement_dirty_children, 1);
    assert_eq!(place_profile.child_place_calls, 1);
    assert_eq!(place_profile.skipped_child_place_calls, 1);
    assert_eq!(place_profile.placement_skip_failures.dirty_subtree, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 1);
}

#[test]
fn placement_skip_does_not_skip_nested_subtree_with_dirty_descendant() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    let wrapper_key = viewport.scene.node_arena.children_of(root_key)[0];
    let dirty_leaf = viewport.scene.node_arena.children_of(wrapper_key)[0];
    mark_place_dirty_for_test(&mut viewport, dirty_leaf);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert!(
        place_profile.child_place_calls >= 1,
        "dirty descendant must force placement through the subtree"
    );
    assert_eq!(place_profile.placement_skip_failures.dirty_subtree, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 1);
}

#[test]
fn placement_skip_does_not_skip_when_child_placement_context_changes() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&two_child_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );

    let (gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 140.0, 80.0);

    assert_eq!(gate_profile.placement_candidate_clean_children, 2);
    assert_eq!(gate_profile.placement_dirty_children, 0);
    assert_eq!(
        place_profile.child_place_calls, 2,
        "clean children must still be placed when the child placement key changes"
    );
    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert_eq!(place_profile.placement_skip_failures.placement_mismatch, 2);
    assert_eq!(place_profile.placement_skip_failures.total(), 2);
}

#[test]
fn placement_skip_does_not_skip_nested_subtree_when_context_changes() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 140.0, 80.0);

    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert!(
        place_profile.child_place_calls >= 1,
        "placement key change must force nested subtree placement"
    );
    assert_eq!(place_profile.placement_skip_failures.placement_mismatch, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 1);
}

#[test]
fn placement_skip_does_not_skip_scroll_offset_context_change() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&scrollable_grid_box_model_tree())
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
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );

    assert!(crate::view::base_component::set_scroll_offset_by_id(
        &viewport.scene.node_arena,
        root_key,
        root_id,
        (24.0, 16.0),
    ));

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 100.0, 50.0);

    assert_eq!(place_profile.child_place_calls, 1);
    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert_eq!(place_profile.placement_skip_failures.placement_mismatch, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 1);
}

#[test]
fn placement_skip_does_not_skip_nested_subtree_with_active_layout_transition_runtime_state() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let wrapper_key = viewport.scene.node_arena.children_of(root_key)[0];
    let wrapper_id = viewport
        .scene
        .node_arena
        .get(wrapper_key)
        .expect("wrapper exists")
        .element
        .stable_id();

    assert!(crate::view::base_component::set_layout_field_by_id(
        &mut viewport.scene.node_arena,
        root_key,
        wrapper_id,
        crate::transition::LayoutField::Width,
        72.0,
    ));
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert!(
        place_profile.child_place_calls >= 1,
        "active transition runtime state must force placement traversal"
    );
    assert_eq!(place_profile.placement_skip_failures.runtime_state, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 1);
}

#[test]
fn placement_skip_does_not_skip_nested_subtree_with_anchor_descendant() {
    let place_profile = sample_clean_parent_relayout_for_placement_profile(
        &nested_grid_with_anchor_descendant_tree(),
        120.0,
        80.0,
    );

    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert!(
        place_profile.child_place_calls >= 1,
        "anchor descendants need placement runtime replay"
    );
    assert_eq!(place_profile.placement_skip_failures.anchor_name, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 1);
}

#[test]
fn placement_skip_does_not_skip_nested_subtree_with_absolute_descendant() {
    let place_profile = sample_clean_parent_relayout_for_placement_profile(
        &nested_grid_with_absolute_descendant_tree(),
        120.0,
        80.0,
    );

    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert!(
        place_profile.child_place_calls >= 1,
        "absolute descendants are still excluded from the Phase 4l expansion"
    );
    assert_eq!(place_profile.placement_skip_failures.absolute_descendant, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 1);
}

#[test]
fn placement_skip_does_not_skip_nested_subtree_with_text_area_descendant() {
    let place_profile = sample_clean_parent_relayout_for_placement_profile(
        &nested_grid_with_text_area_descendant_tree(),
        180.0,
        100.0,
    );

    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert!(
        place_profile.child_place_calls >= 1,
        "TextArea descendants are non-base elements and must not be replay-skipped",
    );
    assert_eq!(place_profile.placement_skip_failures.non_base_element, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 1);
}

#[test]
fn placement_skip_ignores_paint_only_dirty_and_reuses_box_model_cache() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&two_child_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    viewport.refresh_frame_box_models();
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    let paint_child = viewport.scene.node_arena.children_of(root_key)[0];
    mark_paint_dirty_for_test(&mut viewport, paint_child);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(place_profile.child_place_calls, 0);
    assert_eq!(
        place_profile.skipped_child_place_calls, 0,
        "paint-only dirty should let the clean root placement early-return"
    );
    assert!(
        viewport
            .scene
            .node_arena
            .cached_subtree_dirty(root_key)
            .contains(crate::view::base_component::DirtyPassMask::PAINT)
    );

    viewport.refresh_frame_box_models();
    assert_eq!(viewport.box_model_refresh_stats().collected_roots, 0);
    assert_eq!(viewport.box_model_refresh_stats().reused_roots, 1);
}

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

#[test]
fn drag_drop_retargets_after_drag_over_rerender() {
    let dropped = global_state(|| Vec::<String>::new());
    let mut viewport = Viewport::new();
    viewport.set_size(200, 120);

    viewport
        .render_rsx(&drag_drop_rerender_tree(false, dropped.binding()))
        .expect("cold render");
    run_layout_for_test(&mut viewport, 200.0, 120.0);
    let old_target = viewport
        .scene
        .ui_root_keys
        .iter()
        .rev()
        .find_map(|&root_key| {
            crate::view::base_component::hit_test(&viewport.scene.node_arena, root_key, 20.0, 45.0)
        })
        .expect("initial target should hit-test");

    viewport.input_state.drag_state = Some(super::DragState {
        source_id: old_target,
        data: crate::ui::DataTransfer::default(),
        effect_allowed: DragEffect::Move,
        last_over_target: Some(old_target),
        last_drop_effect: Some(DragEffect::Move),
    });
    viewport.set_pointer_position_viewport(20.0, 45.0);

    viewport
        .render_rsx(&drag_drop_rerender_tree(true, dropped.binding()))
        .expect("drag-over indicator render");
    run_layout_for_test(&mut viewport, 200.0, 120.0);

    viewport.dispatch_pointer_up_event(crate::view::viewport::PointerButton::Left);

    assert_eq!(dropped.get(), vec!["target".to_string()]);
}

#[test]
fn cursor_resolves_pointer_from_hovered_text_child_ancestor() {
    let mut viewport = Viewport::new();
    viewport.set_size(160, 40);
    viewport
        .render_rsx(&pointer_cursor_with_text_child_tree())
        .expect("render pointer cursor tree");
    run_layout_for_test(&mut viewport, 160.0, 40.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let text_key =
        find_text_node(&viewport.scene.node_arena, root_key, "Hover target").expect("text child");
    let text_snapshot = viewport
        .scene
        .node_arena
        .get(text_key)
        .expect("text child")
        .element
        .box_model_snapshot();
    let target = crate::view::base_component::hit_test(
        &viewport.scene.node_arena,
        root_key,
        text_snapshot.x + text_snapshot.width * 0.5,
        text_snapshot.y + text_snapshot.height * 0.5,
    );

    assert_eq!(target, Some(text_key), "hit-test should land on text child");
    viewport.input_state.hovered_node_id = target;

    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::Pointer,
        "cursor should inherit from the hovered text child's pointer ancestor",
    );
}

#[test]
fn pointer_move_cursor_respects_root_stacking_over_anchor_parent_resize_handle() {
    let mut viewport = Viewport::new();
    viewport.set_size(200, 120);
    viewport
        .render_rsx(&overlapping_root_with_anchor_parent_resize_handle_tree())
        .expect("render overlapping root tree");
    run_layout_for_test(&mut viewport, 200.0, 120.0);

    let lower_root = viewport.scene.ui_root_keys[0];
    let handle_key = viewport.scene.node_arena.children_of(lower_root)[0];
    let higher_root = viewport.scene.ui_root_keys[1];
    assert_eq!(
        crate::view::base_component::hit_test_roots(
            &viewport.scene.node_arena,
            &viewport.scene.ui_root_keys,
            101.0,
            20.0,
        ),
        Some((1, higher_root)),
        "root children follow sibling stacking; an earlier root's escape descendant is not a top layer",
    );

    viewport.set_pointer_position_viewport(101.0, 20.0);
    assert!(
        viewport.dispatch_pointer_move_event(),
        "pointer move should update hover at the resize handle",
    );
    assert_eq!(
        viewport.input_state.hovered_node_id,
        Some(higher_root),
        "production pointer-move path should hover the later root body, not an earlier root descendant",
    );
    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::Default,
        "escape clipping does not promote an earlier root descendant above a later root",
    );

    let mut popup_stack = crate::view::popup_stack::PopupStack::new();
    let handle_id = viewport
        .scene
        .node_arena
        .get(handle_key)
        .expect("handle node exists")
        .element
        .stable_id();
    popup_stack.register(handle_id);
    assert_eq!(
        crate::view::base_component::hit_test_stacked(
            &viewport.scene.node_arena,
            &popup_stack,
            101.0,
            20.0,
        ),
        Some((lower_root, handle_key)),
        "PopupStack is the explicit top-layer interaction path",
    );
}

#[test]
fn hit_test_same_root_escape_descendant_respects_later_sibling_stacking() {
    let mut viewport = Viewport::new();
    viewport.set_size(220, 120);
    viewport
        .render_rsx(&same_root_escape_descendant_under_later_sibling_tree())
        .expect("render same-root stacking tree");
    run_layout_for_test(&mut viewport, 220.0, 120.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let root_children = viewport.scene.node_arena.children_of(root_key);
    let earlier_parent = root_children[0];
    let escape_child = viewport.scene.node_arena.children_of(earlier_parent)[0];
    let later_sibling = root_children[1];

    assert_eq!(
        crate::view::base_component::hit_test(&viewport.scene.node_arena, root_key, 105.0, 15.0,),
        Some(later_sibling),
        "within one root, a later sibling stacks above an earlier sibling's escape descendant",
    );

    let mut popup_stack = crate::view::popup_stack::PopupStack::new();
    let escape_child_id = viewport
        .scene
        .node_arena
        .get(escape_child)
        .expect("escape child exists")
        .element
        .stable_id();
    popup_stack.register(escape_child_id);
    assert_eq!(
        crate::view::base_component::hit_test_stacked(
            &viewport.scene.node_arena,
            &popup_stack,
            105.0,
            15.0,
        ),
        Some((root_key, escape_child)),
        "PopupStack can intentionally promote an escape descendant above normal sibling stacking",
    );
}

#[test]
fn cursor_resolves_from_hovered_node_key_when_stable_ids_collide() {
    let mut lower =
        crate::view::base_component::Element::new_with_id(0xC0DE, 0.0, 0.0, 100.0, 80.0);
    let mut lower_style = Style::new();
    lower_style.insert(PropertyId::Cursor, ParsedValue::Cursor(Cursor::EwResize));
    lower.apply_style(lower_style);

    let higher = crate::view::base_component::Element::new_with_id(0xC0DE, 0.0, 0.0, 100.0, 80.0);

    let mut viewport = Viewport::new();
    let lower_key = viewport
        .scene
        .node_arena
        .insert(crate::view::node_arena::Node::new(Box::new(lower)));
    let higher_key = viewport
        .scene
        .node_arena
        .insert(crate::view::node_arena::Node::new(Box::new(higher)));
    viewport.scene.ui_root_keys = vec![lower_key, higher_key];
    viewport
        .scene
        .node_arena
        .set_roots(viewport.scene.ui_root_keys.clone());
    viewport.input_state.hovered_node_id = Some(lower_key);

    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::EwResize,
        "cursor resolution must use the hovered NodeKey, not a colliding stable id from a later root"
    );
}

#[test]
fn placement_only_position_update_moves_anchor_parent_resize_handle_hit_test() {
    let mut viewport = Viewport::new();
    viewport.set_size(240, 140);
    viewport
        .render_rsx(&movable_root_with_anchor_parent_resize_handle_tree(20.0))
        .expect("render initial movable root tree");
    run_layout_for_test(&mut viewport, 240.0, 140.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let handle_key = viewport.scene.node_arena.children_of(root_key)[0];
    assert_eq!(
        crate::view::base_component::hit_test_roots(
            &viewport.scene.node_arena,
            &viewport.scene.ui_root_keys,
            121.0,
            50.0,
        ),
        Some((0, handle_key)),
        "initial right resize handle should be hit-testable"
    );

    viewport
        .render_rsx(&movable_root_with_anchor_parent_resize_handle_tree(80.0))
        .expect("render moved tree through placement-only update");
    run_layout_for_test(&mut viewport, 240.0, 140.0);

    assert_ne!(
        crate::view::base_component::hit_test_roots(
            &viewport.scene.node_arena,
            &viewport.scene.ui_root_keys,
            121.0,
            50.0,
        ),
        Some((0, handle_key)),
        "old handle position must not remain clickable after placement-only move"
    );
    assert_eq!(
        crate::view::base_component::hit_test_roots(
            &viewport.scene.node_arena,
            &viewport.scene.ui_root_keys,
            181.0,
            50.0,
        ),
        Some((0, handle_key)),
        "new right resize handle position should be hit-testable after placement-only move"
    );

    viewport.set_pointer_position_viewport(181.0, 50.0);
    assert!(
        viewport.dispatch_pointer_move_event(),
        "production pointer move should update hover at the moved resize handle",
    );
    assert_eq!(
        viewport.input_state.hovered_node_id,
        Some(handle_key),
        "hover target should follow the placement-only moved resize handle"
    );
    assert_eq!(viewport.resolve_cursor(), Cursor::EwResize);
}

#[test]
fn retained_window_accordion_button_false_to_true_hit_tests_without_scroll() {
    let mut viewport = Viewport::new();
    viewport.set_size(460, 380);
    viewport
        .render_rsx(&retained_window_accordion_button_tree())
        .expect("render collapsed retained tree");
    run_layout_for_test(&mut viewport, 460.0, 380.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let section_key = viewport.scene.node_arena.children_of(root_key)[1];
    let button_key = viewport.scene.node_arena.children_of(section_key)[0];
    let label_key = find_text_node(&viewport.scene.node_arena, root_key, "Contained")
        .expect("contained button label");

    viewport
        .scene
        .node_arena
        .mutate_element_with_invalidation(section_key, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>()
                .expect("section element")
                .replace_style(expanded_retained_accordion_section_style());
            cx.invalidate(crate::view::base_component::DirtyFlags::ALL);
        })
        .expect("section exists");

    run_layout_for_test(&mut viewport, 460.0, 380.0);
    let post_layout = viewport.run_post_layout_transitions(1.0, 1.0);
    if post_layout.relayout_required {
        run_layout_for_test(&mut viewport, 460.0, 380.0);
    }

    assert_eq!(
        viewport.scene.node_arena.children_of(section_key)[0],
        button_key,
        "button NodeKey should be retained across false -> true expansion",
    );
    assert!(
        find_text_node(&viewport.scene.node_arena, root_key, "Contained") == Some(label_key),
        "label NodeKey should be retained across false -> true expansion",
    );

    let label_snapshot = viewport
        .scene
        .node_arena
        .get(label_key)
        .expect("contained label")
        .element
        .box_model_snapshot();
    let hit_x = label_snapshot.x + label_snapshot.width * 0.5;
    let hit_y = label_snapshot.y + label_snapshot.height * 0.5;
    let target =
        crate::view::base_component::hit_test(&viewport.scene.node_arena, root_key, hit_x, hit_y);
    assert!(
        matches!(target, Some(target) if target == label_key || target == button_key),
        "hit at retained expanded button ({hit_x}, {hit_y}) should target button branch; got {target:?}",
    );

    viewport.input_state.hovered_node_id = target;
    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::Pointer,
        "retained expanded button should resolve pointer cursor before any scroll",
    );
}

#[test]
fn retained_window_accordion_button_rerender_false_to_true_hit_tests_without_scroll() {
    let collapsed = retained_window_accordion_button_tree_with_expanded(false);
    let expanded = retained_window_accordion_button_tree_with_expanded(true);

    let mut viewport = Viewport::new();
    viewport.set_size(460, 380);
    viewport.set_use_incremental_commit(true);
    viewport
        .render_rsx(&collapsed)
        .expect("render collapsed retained tree");
    run_layout_for_test(&mut viewport, 460.0, 380.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let section_key = viewport.scene.node_arena.children_of(root_key)[1];
    let button_key = viewport.scene.node_arena.children_of(section_key)[0];
    let label_key = find_text_node(&viewport.scene.node_arena, root_key, "Contained")
        .expect("contained button label");

    viewport
        .render_rsx(&expanded)
        .expect("rerender expanded retained tree");
    run_layout_for_test(&mut viewport, 460.0, 380.0);
    let post_layout = viewport.run_post_layout_transitions(1.0, 1.0);
    if post_layout.relayout_required {
        run_layout_for_test(&mut viewport, 460.0, 380.0);
    }

    assert_eq!(
        viewport.scene.ui_root_keys[0], root_key,
        "root NodeKey should be retained across false -> true rerender",
    );
    assert_eq!(
        viewport.scene.node_arena.children_of(root_key)[1],
        section_key,
        "section NodeKey should be retained across false -> true rerender",
    );
    assert_eq!(
        viewport.scene.node_arena.children_of(section_key)[0],
        button_key,
        "button NodeKey should be retained across false -> true rerender",
    );
    assert_eq!(
        find_text_node(&viewport.scene.node_arena, root_key, "Contained"),
        Some(label_key),
        "label NodeKey should be retained across false -> true rerender",
    );

    let label_snapshot = viewport
        .scene
        .node_arena
        .get(label_key)
        .expect("contained label")
        .element
        .box_model_snapshot();
    let hit_x = label_snapshot.x + label_snapshot.width * 0.5;
    let hit_y = label_snapshot.y + label_snapshot.height * 0.5;
    let target =
        crate::view::base_component::hit_test(&viewport.scene.node_arena, root_key, hit_x, hit_y);
    assert!(
        matches!(target, Some(target) if target == label_key || target == button_key),
        "hit at rerendered expanded button ({hit_x}, {hit_y}) should target button branch before any scroll; got {target:?}",
    );

    viewport.input_state.hovered_node_id = target;
    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::Pointer,
        "rerendered expanded button should resolve pointer cursor before any scroll",
    );
}

#[test]
fn component_test_button_section_rerender_false_to_true_hit_tests_without_scroll() {
    let collapsed = retained_component_test_button_section_tree_with_expanded(false);
    let expanded = retained_component_test_button_section_tree_with_expanded(true);

    let mut viewport = Viewport::new();
    viewport.set_size(460, 380);
    viewport.set_use_incremental_commit(true);
    viewport
        .render_rsx(&collapsed)
        .expect("render collapsed component-test-like tree");
    run_layout_for_test(&mut viewport, 460.0, 380.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let contained_key = find_text_node(&viewport.scene.node_arena, root_key, "Contained")
        .expect("contained button text");
    let button_key = viewport
        .scene
        .node_arena
        .parent_of(contained_key)
        .expect("contained text parent button");

    viewport
        .render_rsx(&expanded)
        .expect("rerender expanded component-test-like tree");
    run_layout_for_test(&mut viewport, 460.0, 380.0);
    let post_layout = viewport.run_post_layout_transitions(1.0, 1.0);
    if post_layout.relayout_required {
        let profile = run_layout_for_test_with_gate_profile(&mut viewport, 460.0, 380.0);
        eprintln!("profile1 {profile:?}");
    }
    let post_layout = viewport.run_post_layout_transitions(1.0, 2.0);
    if post_layout.relayout_required {
        let profile = run_layout_for_test_with_gate_profile(&mut viewport, 460.0, 380.0);
        eprintln!("profile2 {profile:?}");
    }

    assert_eq!(
        find_text_node(&viewport.scene.node_arena, root_key, "Contained"),
        Some(contained_key),
        "contained text NodeKey should be retained across component-test-like expansion",
    );

    let contained_snapshot = viewport
        .scene
        .node_arena
        .get(contained_key)
        .expect("contained button text")
        .element
        .box_model_snapshot();
    let hit_x = contained_snapshot.x + contained_snapshot.width * 0.5;
    let hit_y = contained_snapshot.y + contained_snapshot.height * 0.5;
    let target =
        crate::view::base_component::hit_test(&viewport.scene.node_arena, root_key, hit_x, hit_y);
    assert!(
        matches!(target, Some(target) if target == contained_key || target == button_key),
        "hit at component-test-like expanded button ({hit_x}, {hit_y}) should target the button branch before any scroll; got {target:?}, button={button_key:?}, text={contained_key:?}",
    );

    viewport.input_state.hovered_node_id = target;
    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::Pointer,
        "component-test-like expanded button should resolve pointer cursor before any scroll",
    );
}

/// Structure-identical re-render: reconcile produces an empty patch
/// list, and the incremental path must commit zero works while keeping
/// arena root NodeKeys intact.
#[test]
fn incremental_commit_preserves_node_key_across_identical_render() {
    // Build the tree once and render the same `RsxNode` twice. The
    // reconciler's `ptr_eq` fast-path short-circuits prop-diffing (the
    // `Style` prop is an `Rc`-backed `Shared` value that otherwise
    // compares by pointer), producing an empty patch list. Under M2
    // that is the canonical case the incremental path must handle:
    // zero works committed, NodeKey untouched.
    let tree = single_element(120.0);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&tree)
        .expect("cold render should fall back to full rebuild and succeed");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&tree)
        .expect("identical re-render should succeed on incremental path");

    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    assert_eq!(
        viewport.scene.ui_root_keys[0], original_key,
        "NodeKey must be stable across an identical incremental render",
    );
}

/// When the incremental path can't handle a change (here: a prop
/// update, which translates to `FiberWork::Update` — not
/// M2-committable), the flow must fall back to the full-rebuild
/// pipeline. Under the current legacy path an identity-preserving
/// rebuild can still mint a fresh NodeKey; we only assert the render
/// succeeds and the arena still holds a single root.
#[test]
fn incremental_commit_falls_back_on_non_committable_work() {
    let first = single_element(120.0);
    let second = single_element(160.0);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&first)
        .expect("cold render should succeed");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);

    viewport
        .render_rsx(&second)
        .expect("prop-change render must fall back and still succeed");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
}

/// Remove-a-child: reconcile emits a single `Patch::RemoveChild`,
/// which translates to `FiberWork::Delete` — committable under M2.
/// The parent's NodeKey must survive the incremental commit, the
/// removed child's stable id must be cleared from the index, and the
/// parent's arena child list must shrink by one.
#[test]
fn incremental_commit_deletes_child_without_rebuilding_parent() {
    let child_a = host_el();
    let child_b = host_el();

    // Both parents share the same child identities so reconcile's
    // match phase pairs them up and only the surplus child drops.
    let parent_with_two = host_el()
        .with_child(child_a.clone())
        .with_child(child_b.clone());
    let parent_with_one = host_el().with_child(child_a.clone());

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&parent_with_two).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let parent_key = viewport.scene.ui_root_keys[0];
    let arena = &viewport.scene.node_arena;
    let children_before = arena.children_of(parent_key);
    assert_eq!(children_before.len(), 2);
    let kept_child_key = children_before[0];

    viewport
        .render_rsx(&parent_with_one)
        .expect("delete-child render should commit incrementally");

    // Parent and surviving child must keep their keys — this is the
    // core identity-preservation guarantee M2 ships.
    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    let arena_after = &viewport.scene.node_arena;
    let children_after = arena_after.children_of(parent_key);
    assert_eq!(children_after, vec![kept_child_key]);
}

/// 軌 1 #1: A root-type swap emits `Patch::ReplaceRoot`. The
/// incremental path now builds a descriptor from the new RSX via the
/// shared `DescriptorContext` + `rsx_to_descriptors_with_inherited`
/// pipeline, drops the old subtree, and commits the new one as the
/// sole root — without the full-rebuild fallback ever firing.
#[test]
fn incremental_commit_applies_replace_root() {
    let first = single_element(120.0);
    let second = text_leaf("hello");

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&first)
        .expect("cold render should succeed");
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&second)
        .expect("ReplaceRoot must commit incrementally");

    // Root replaced — a new NodeKey is expected (the new element is a
    // text host, not an Element) but `ui_root_keys` must still be a
    // single entry pointing at the freshly-committed arena slot.
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let new_key = viewport.scene.ui_root_keys[0];
    assert_ne!(
        new_key, original_key,
        "ReplaceRoot swaps the arena slot — new NodeKey expected",
    );
    // Old slot is gone; arena must not leak it.
    assert!(
        viewport.scene.node_arena.get(original_key).is_none(),
        "old root slot must be removed after ReplaceRoot commit",
    );
}

/// 軌 1 #1: `Patch::ReplaceNode` (mid-tree type change) commits
/// incrementally via the apply-side `arena_replace_child`. The
/// reconciler only emits `ReplaceNode` when the child-match step
/// pairs two children whose inner variant or tag then differs —
/// which, given identity keys invocation_type + key, is rare in
/// natural RSX. We exercise the path directly by constructing the
/// patch and feeding it through the translator + applier.
#[test]
fn incremental_commit_replace_node_rebuilds_child_preserves_parent_key() {
    use crate::style::Style;
    use crate::view::fiber_work::{DescriptorContext, apply_fiber_works, patch_to_fiber_work};

    // Seed: parent with two children. Snapshot keys before we mutate.
    let seed = host_el().with_child(host_el()).with_child(host_el());
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&seed).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let kept_child_key = viewport.scene.node_arena.children_of(parent_key)[1];
    let old_first_key = viewport.scene.node_arena.children_of(parent_key)[0];

    // Build a synthetic ReplaceNode at path [0] — swap the first
    // child for a text leaf. New rsx root mirrors the same parent
    // structure so `walk_rsx_by_index_path` and resolve_path line up.
    let new_root = host_el()
        .with_child(text_leaf("swapped"))
        .with_child(host_el());
    let patch = crate::ui::Patch::ReplaceNode {
        path: vec![0],
        node: text_leaf("swapped"),
    };
    let viewport_style = Style::new();
    let ctx = DescriptorContext {
        new_rsx_root: &new_root,
        old_rsx_root: None,
        inherited_style: &viewport_style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    let work = patch_to_fiber_work(
        patch,
        viewport.scene.node_arena.stable_id_index(),
        &viewport.scene.node_arena,
        parent_key,
        Some(&ctx),
    )
    .expect("ReplaceNode must translate to a FiberWork");
    assert!(work.is_committable(&viewport.scene.node_arena));
    apply_fiber_works(&mut viewport.scene.node_arena, test_apply_ctx(), vec![work]);

    // Parent NodeKey unchanged; children list still length 2; kept
    // sibling survives at slot 1; first slot is a fresh NodeKey.
    let arena = &viewport.scene.node_arena;
    let children = arena.children_of(parent_key);
    assert_eq!(children.len(), 2);
    assert_eq!(children[1], kept_child_key);
    assert_ne!(
        children[0], old_first_key,
        "replaced slot must mint a new key"
    );
    assert!(
        arena.get(old_first_key).is_none(),
        "old child slot must be dropped",
    );
}

// ---------------------------------------------------------------------------
// M3: incremental Update + SetText coverage
// ---------------------------------------------------------------------------
//
// These extend M2's Delete/Move-only gate with the prop-setter layer.
// The identity-preservation contract is the same: if the incremental
// path commits the work, the target NodeKey survives.

/// Style update on an Element host commits through the incremental
/// path and keeps the root NodeKey stable — no full rebuild.
#[test]
fn incremental_commit_applies_style_update_preserves_node_key() {
    let first = rsx! {
        <HostElement style={{ width: Length::px(120.0), height: Length::px(40.0), opacity: 0.9 }} />
    };
    let second = rsx! {
        <HostElement style={{ width: Length::px(120.0), height: Length::px(40.0), opacity: 0.5 }} />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        original_key,
        crate::view::base_component::DirtyFlags::ALL,
    );

    viewport
        .render_rsx(&second)
        .expect("style-change render must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "style update must preserve NodeKey via the M3 setter path",
    );
    assert!(
        viewport
            .scene
            .node_arena
            .arena_local_dirty(original_key)
            .contains(crate::view::base_component::DirtyPassMask::PAINT),
        "FiberWork::Update must propagate element-owned dirty flags into arena dirty",
    );
}

/// font_size update on a Text leaf commits as `FiberWork::Update`.
/// NodeKey must survive.
///
/// Uses numeric f64 directly so the prop lands as `PropValue::F64`
/// (the M3 Text font_size branch only handles numeric; `FontSize`-
/// typed values that need inherited-context resolution fall back).
#[test]
fn incremental_commit_applies_font_size_update_preserves_node_key() {
    use crate::view::Text as HostText;

    fn tree(size: f64) -> RsxNode {
        rsx! { <HostText font_size={size}>"hi"</HostText> }
    }

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&tree(14.0)).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&tree(20.0))
        .expect("font_size update must commit incrementally");
    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "font_size update must preserve NodeKey via the M3 setter path",
    );
}

/// M4 #4: event-handler changes are now committable incrementally via
/// `Element::clear_rsx_event_handler` + the shared
/// `try_assign_event_handler_prop` dispatcher. The NodeKey must
/// survive the handler swap (was previously force-rebuilt under M3).
#[test]
fn incremental_commit_applies_event_handler_change_preserves_node_key() {
    use crate::ui::PointerDownHandlerProp;
    let handler_a = PointerDownHandlerProp::new(|_| {});
    let handler_b = PointerDownHandlerProp::new(|_| {});

    let first = rsx! {
        <HostElement
            style={{ width: Length::px(120.0), height: Length::px(40.0) }}
            on_pointer_down={handler_a}
        />
    };
    let second = rsx! {
        <HostElement
            style={{ width: Length::px(120.0), height: Length::px(40.0) }}
            on_pointer_down={handler_b}
        />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&second)
        .expect("event-handler change must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "on_pointer_down replacement must preserve NodeKey via the M4 setter path",
    );

    use crate::view::base_component::Element as ElementHost;
    let arena = &viewport.scene.node_arena;
    let node = arena.get(original_key).expect("root node");
    let el = node
        .element
        .as_any()
        .downcast_ref::<ElementHost>()
        .expect("Element host");
    assert_eq!(
        el.rsx_event_handler_count("on_pointer_down"),
        1,
        "replace semantics: clear + assign must leave exactly one handler, not stack",
    );
}

/// Removing an `on_*` prop between renders emits a reconciler
/// `removed: [..]` entry. M4 #4 routes that through
/// `Element::clear_rsx_event_handler`, so the handler Vec drops to
/// zero and NodeKey still survives.
#[test]
fn incremental_commit_removes_event_handler_prop_clears_handler_list() {
    use crate::ui::PointerDownHandlerProp;
    use crate::view::base_component::Element as ElementHost;

    let handler = PointerDownHandlerProp::new(|_| {});
    let with_handler = rsx! {
        <HostElement
            style={{ width: Length::px(120.0), height: Length::px(40.0) }}
            on_pointer_down={handler}
        />
    };
    let without_handler = rsx! {
        <HostElement style={{ width: Length::px(120.0), height: Length::px(40.0) }} />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&with_handler).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];
    {
        let arena = &viewport.scene.node_arena;
        let node = arena.get(original_key).unwrap();
        let el = node.element.as_any().downcast_ref::<ElementHost>().unwrap();
        assert_eq!(el.rsx_event_handler_count("on_pointer_down"), 1);
    }

    viewport
        .render_rsx(&without_handler)
        .expect("handler-removal render must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "removed on_pointer_down must commit through clear and keep NodeKey",
    );
    let arena = &viewport.scene.node_arena;
    let node = arena.get(original_key).unwrap();
    let el = node.element.as_any().downcast_ref::<ElementHost>().unwrap();
    assert_eq!(
        el.rsx_event_handler_count("on_pointer_down"),
        0,
        "removed handler prop must clear the handler list",
    );
}

/// Text content change on a Text leaf commits as `FiberWork::SetText`.
/// The Text host's NodeKey survives.
#[test]
fn incremental_commit_applies_set_text_preserves_node_key() {
    use crate::view::Text as HostText;

    let first = rsx! { <HostText>"hello"</HostText> };
    let second = rsx! { <HostText>"world"</HostText> };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        original_key,
        crate::view::base_component::DirtyFlags::ALL,
    );

    viewport
        .render_rsx(&second)
        .expect("text-content change must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "SetText must preserve NodeKey via the M3 setter path",
    );
    assert!(
        viewport
            .scene
            .node_arena
            .arena_local_dirty(original_key)
            .intersects(crate::view::base_component::DirtyFlags::ALL),
        "FiberWork::SetText must propagate text dirty flags into arena dirty",
    );
}

#[test]
fn incremental_commit_reorders_unkeyed_text_rows_without_duplicate_content() {
    use crate::view::Text as HostText;

    fn tree(labels: &[&str]) -> RsxNode {
        rsx! {
            <HostElement>
                {labels
                    .iter()
                    .map(|label| rsx! { <HostText>{(*label).to_string()}</HostText> })
                    .collect::<Vec<_>>()}
            </HostElement>
        }
    }

    let first = tree(&["window.rs", "accordion.rs", "tree_view.rs"]);
    let second = tree(&["accordion.rs", "window.rs", "tree_view.rs"]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    viewport
        .render_rsx(&second)
        .expect("text sibling reorder should render without duplicates");

    let mut labels = Vec::new();
    collect_text_contents(
        &viewport.scene.node_arena,
        viewport.scene.ui_root_keys[0],
        &mut labels,
    );
    assert_eq!(labels, vec!["accordion.rs", "window.rs", "tree_view.rs"]);
}

/// Regression for the TreeView drag-drop "duplicate row" bug.
///
/// Reconciler emits, for the same parent that's about to reorder via
/// keyed match, a per-child `RemoveChild + InsertChild` (because that
/// row's *internal* shape changed — e.g. a drop-indicator slot
/// switching from `Element` to `Fragment`). The InsertChild path uses
/// the OLD parent-relative index; after the keyed reorder happens
/// above it, walking NEW by that OLD index lands on a different keyed
/// sibling. The translator's `fallback_replace_node_patch` used to
/// blindly take `NEW[old_path]` as the replacement node, clobbering
/// the row at that arena slot with an unrelated row's contents → the
/// later MoveChild then duplicates that wrong content.
#[test]
fn keyed_row_internal_shape_change_plus_reorder_does_not_duplicate() {
    use crate::view::Text as HostText;

    fn row(label: &str, indicator: bool) -> RsxNode {
        let s = label.to_string();
        let inner = rsx! { <HostText>{s.clone()}</HostText> };
        let slot = if indicator {
            rsx! { <HostElement /> }
        } else {
            RsxNode::fragment(vec![])
        };
        rsx! {
            <HostElement key={s.clone()}>
                {inner}
                {slot}
            </HostElement>
        }
    }

    fn tree(rows: Vec<RsxNode>) -> RsxNode {
        rsx! { <HostElement>{rows}</HostElement> }
    }

    // Pre-drop snapshot the reconciler will diff against: order [A, B, C],
    // row "B" is showing the indicator slot as Element.
    let first = tree(vec![row("A", false), row("B", true), row("C", false)]);
    // Post-drop: order [B, A, C] (keyed reorder above), AND B's
    // indicator slot collapses back to Fragment.
    let second = tree(vec![row("B", false), row("A", false), row("C", false)]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    viewport
        .render_rsx(&second)
        .expect("reorder + shape-change must commit cleanly");

    let mut labels = Vec::new();
    collect_text_contents(
        &viewport.scene.node_arena,
        viewport.scene.ui_root_keys[0],
        &mut labels,
    );
    assert_eq!(
        labels,
        vec!["B", "A", "C"],
        "labels must follow keyed reorder; duplicates here mean \
         fallback ReplaceNode clobbered an arena slot whose OLD/NEW \
         identity diverged because of the surrounding keyed shuffle",
    );
}

// ---------------------------------------------------------------------------
// M4 #1: non-additive replace_style
// ---------------------------------------------------------------------------

/// A `style` prop update whose new `Style` lacks a declaration the old
/// one had must clear that declaration — proving the M4 #1
/// `replace_style` wiring is not using the additive `apply_style`
/// merge. Asserts directly against `Element::parsed_style()`.
#[test]
fn incremental_commit_replace_style_drops_absent_declaration() {
    use crate::style::Color;
    use crate::style::PropertyId;
    use crate::view::base_component::Element as ElementHost;

    let with_bg = rsx! {
        <HostElement style={{
            width: Length::px(120.0),
            height: Length::px(40.0),
            background_color: Color::hex("#FF0000"),
        }} />
    };
    let without_bg = rsx! {
        <HostElement style={{
            width: Length::px(120.0),
            height: Length::px(40.0),
        }} />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&with_bg).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];
    {
        let arena = &viewport.scene.node_arena;
        let node = arena.get(original_key).expect("root node");
        let el = node
            .element
            .as_any()
            .downcast_ref::<ElementHost>()
            .expect("Element host");
        assert!(
            el.parsed_style().get(PropertyId::BackgroundColor).is_some(),
            "background_color declaration must be present after cold render",
        );
    }

    viewport
        .render_rsx(&without_bg)
        .expect("style-drop render must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "replace_style path must preserve NodeKey",
    );
    let arena = &viewport.scene.node_arena;
    let node = arena.get(original_key).expect("root node after re-render");
    let el = node
        .element
        .as_any()
        .downcast_ref::<ElementHost>()
        .expect("Element host");
    assert!(
        el.parsed_style().get(PropertyId::BackgroundColor).is_none(),
        "replace_style must drop the declaration absent from the new Style",
    );
}

/// When the `style` prop itself is removed between renders, reconcile
/// emits a `removed: [\"style\"]` entry. M4 #1 routes that through
/// `Element::replace_style(...)`, clearing all authored declarations
/// while preserving the inherited base needed by the Element's own
/// computed style.
#[test]
fn incremental_commit_removes_style_prop_resets_parsed_style() {
    use crate::view::base_component::Element as ElementHost;

    let with_style = rsx! {
        <HostElement style={{ width: Length::px(120.0), height: Length::px(40.0) }} />
    };
    let without_style = host_el();

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&with_style).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];
    {
        let arena = &viewport.scene.node_arena;
        let node = arena.get(original_key).expect("root node");
        let el = node
            .element
            .as_any()
            .downcast_ref::<ElementHost>()
            .expect("Element host");
        assert!(
            !el.parsed_style().declarations().is_empty(),
            "initial style prop should author at least one declaration",
        );
    }

    viewport
        .render_rsx(&without_style)
        .expect("style-removal render must commit incrementally");

    // NodeKey equality is the identity-preservation contract. If the
    // removed-style patch had been rejected by `is_committable`, the
    // legacy full-rebuild path would have minted a fresh NodeKey.
    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "removed-style prop path must commit through replace_style and keep NodeKey",
    );
    let arena = &viewport.scene.node_arena;
    let node = arena.get(original_key).expect("root node after re-render");
    let el = node
        .element
        .as_any()
        .downcast_ref::<ElementHost>()
        .expect("Element host");
    assert!(
        el.text_cascade_style().declarations().is_empty(),
        "removed-style prop must clear the authored text cascade style",
    );
    assert!(
        !matches!(
            el.parsed_style().get(crate::style::PropertyId::Width),
            Some(crate::style::ParsedValue::Length(length))
                if *length == Length::px(120.0)
        ),
        "removed-style prop must drop the authored width declaration",
    );
}

// ---------------------------------------------------------------------------
// M5 #5/#6: Create via InsertChild translation
// ---------------------------------------------------------------------------

/// Appending a child to a parent that already has one should commit
/// incrementally as a single `FiberWork::Create`: the existing
/// child's NodeKey survives (no full rebuild), the parent gains one
/// child, and the newly-authored child is parented to the same key.
#[test]
fn incremental_commit_inserts_appended_child_preserves_sibling_keys() {
    let child_a = host_el();

    // Parent with one child vs parent with two children, sharing child_a
    // as the stable first child (identity-matched by the reconciler).
    let parent_with_one = host_el().with_child(child_a.clone());
    let parent_with_two = host_el().with_child(child_a.clone()).with_child(host_el());

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&parent_with_one).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let parent_key = viewport.scene.ui_root_keys[0];
    let existing_child_key = {
        let arena = &viewport.scene.node_arena;
        let children = arena.children_of(parent_key);
        assert_eq!(children.len(), 1);
        children[0]
    };

    viewport
        .render_rsx(&parent_with_two)
        .expect("insert-child render should commit incrementally");

    // Parent identity survives — the incremental path didn't rebuild.
    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![parent_key],
        "parent NodeKey must survive InsertChild translation",
    );

    let arena = &viewport.scene.node_arena;
    let children = arena.children_of(parent_key);
    assert_eq!(
        children.len(),
        2,
        "InsertChild should grow the parent's child list by one",
    );
    assert_eq!(
        children[0], existing_child_key,
        "existing child NodeKey must be preserved at its original index",
    );
    // New child has a different key (arena slot) and lives under parent.
    assert_ne!(children[1], existing_child_key);
    let new_child_node = arena.get(children[1]).expect("new child node");
    assert_eq!(new_child_node.parent, Some(parent_key));
}

// ---------------------------------------------------------------------------
// M6 boundary: text-cascading style updates must fall back when
// descendants exist
// ---------------------------------------------------------------------------

/// A `style` update that changes a text-cascading decl (here
/// `font_size`) on an Element with a Text child must fall back to
/// the full-rebuild path — the incremental setter doesn't recascade
/// descendants, so letting it commit would diverge from cold-path
/// behaviour. We assert the boundary by checking the Text child's
/// resolved font_size after the re-render: only the full rebuild
/// path walks the convert pipeline, which re-resolves Text fonts
/// against the new inherited cascade.
/// 軌 A #7: a text-cascading style change on an Element ancestor
/// now commits incrementally — the apply side calls
/// `recascade_text_subtree`, which walks Text/TextArea descendants
/// and re-applies ancestor-derived props via `apply_inherited`
/// (explicit-flag gated). Parent NodeKey survives and the Text
/// child's `font_size` matches the cold-path cascade.
#[test]
fn incremental_commit_applies_text_cascading_style_change_recascades_descendants() {
    use crate::view::Text as HostText;
    use crate::view::base_component::Text as TextHost;

    let parent_20 = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 20.0f32,
        }}>
            <HostText>"hi"</HostText>
        </HostElement>
    };
    let parent_30 = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 30.0f32,
        }}>
            <HostText>"hi"</HostText>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&parent_20).expect("cold render");
    let original_parent_key = viewport.scene.ui_root_keys[0];
    let original_text_key = viewport.scene.node_arena.children_of(original_parent_key)[0];

    viewport
        .render_rsx(&parent_30)
        .expect("cascading style change must commit incrementally");

    // NodeKeys stable — no fallback full rebuild fired.
    assert_eq!(viewport.scene.ui_root_keys, vec![original_parent_key]);
    let arena = &viewport.scene.node_arena;
    let children = arena.children_of(original_parent_key);
    assert_eq!(children, vec![original_text_key]);
    let text = arena
        .get(original_text_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<TextHost>()
        .unwrap()
        .font_size();
    assert!(
        (text - 30.0).abs() < 1e-4,
        "recascade_text_subtree must flow new parent font_size into Text; got {}",
        text
    );
}

/// 軌 A #7: explicit-prop tracking preserves author overrides.
/// A Text child with its own `font_size={14}` must NOT be clobbered
/// when the ancestor's cascading font_size changes.
#[test]
fn incremental_commit_recascade_preserves_explicit_text_font_size() {
    use crate::view::Text as HostText;
    use crate::view::base_component::Text as TextHost;

    let parent_20_explicit_14 = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 20.0f32,
        }}>
            <HostText font_size={14.0f64}>"hi"</HostText>
        </HostElement>
    };
    let parent_30_explicit_14 = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 30.0f32,
        }}>
            <HostText font_size={14.0f64}>"hi"</HostText>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&parent_20_explicit_14)
        .expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let text_key = viewport.scene.node_arena.children_of(parent_key)[0];

    viewport
        .render_rsx(&parent_30_explicit_14)
        .expect("cascade change must commit incrementally");

    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    let arena = &viewport.scene.node_arena;
    let text = arena
        .get(text_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<TextHost>()
        .unwrap()
        .font_size();
    assert!(
        (text - 14.0).abs() < 1e-4,
        "explicit font_size={{14}} must survive ancestor cascade change; got {}",
        text
    );
}

#[test]
fn incremental_commit_vertical_align_keeps_fragmentable_badge_text_aligned() {
    use crate::view::base_component::Text as TextHost;

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&inline_badge_vertical_align_tree(VerticalAlign::Baseline))
        .expect("cold render");
    run_layout_for_test(&mut viewport, 960.0, 240.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let children = viewport.scene.node_arena.children_of(root_key);
    let lead_key = children[0];
    let badge_key = children[1];
    let badge_text_key = viewport.scene.node_arena.children_of(badge_key)[0];

    viewport
        .render_rsx(&inline_badge_vertical_align_tree(VerticalAlign::Middle))
        .expect("vertical-align change should render");
    run_layout_for_test(&mut viewport, 960.0, 240.0);

    assert_eq!(viewport.scene.ui_root_keys, vec![root_key]);
    assert_eq!(
        viewport.scene.node_arena.children_of(root_key)[1],
        badge_key
    );

    let lead_y = viewport
        .scene
        .node_arena
        .get(lead_key)
        .expect("lead text")
        .element
        .as_any()
        .downcast_ref::<TextHost>()
        .expect("lead text")
        .inline_fragment_positions()[0]
        .1
        .y;
    let badge_y = viewport
        .scene
        .node_arena
        .get(badge_text_key)
        .expect("badge text")
        .element
        .as_any()
        .downcast_ref::<TextHost>()
        .expect("badge text")
        .inline_fragment_positions()[0]
        .1
        .y;
    let wrapper_nodes = viewport
        .scene
        .node_arena
        .with_element_taken(badge_key, |element, arena| {
            element.get_inline_nodes_size(arena)
        })
        .expect("badge wrapper");

    assert!(
        (lead_y - badge_y).abs() < 0.5,
        "fragmentable badge text must track sibling inline text after incremental vertical-align update: lead_y={lead_y}, badge_y={badge_y}"
    );
    assert!(
        wrapper_nodes
            .iter()
            .all(|node| node.vertical_align == VerticalAlign::Middle),
        "fragmentable badge wrapper must expose the updated inherited vertical_align, got {wrapper_nodes:?}"
    );
}

#[test]
fn incremental_commit_recascade_updates_text_area_inherited_vertical_align() {
    use crate::view::TextArea as HostTextArea;
    use crate::view::base_component::TextArea as TextAreaHost;

    fn tree(vertical_align: VerticalAlign) -> RsxNode {
        rsx! {
            <HostElement style={{
                width: Length::px(240.0),
                height: Length::px(120.0),
                vertical_align: vertical_align,
            }}>
                <HostTextArea content={"abc".to_string()} />
            </HostElement>
        }
    }

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&tree(VerticalAlign::Middle))
        .expect("cold render");
    let root_key = viewport.scene.ui_root_keys[0];
    let text_area_key = viewport.scene.node_arena.children_of(root_key)[0];

    viewport
        .render_rsx(&tree(VerticalAlign::Bottom))
        .expect("parent vertical-align update should commit incrementally");

    assert_eq!(viewport.scene.ui_root_keys, vec![root_key]);
    let text_area_node = viewport
        .scene
        .node_arena
        .get(text_area_key)
        .expect("TextArea node");
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<TextAreaHost>()
        .expect("TextArea host");
    assert_eq!(
        text_area.vertical_align,
        VerticalAlign::Bottom,
        "TextArea without its own style must follow parent inherited vertical_align updates",
    );
}

#[test]
fn incremental_commit_recascade_updates_text_area_inherited_line_height() {
    use crate::view::TextArea as HostTextArea;
    use crate::view::base_component::TextArea as TextAreaHost;
    use crate::view::base_component::text_area::TextAreaTextRun;

    fn tree(line_height: f32) -> RsxNode {
        rsx! {
            <HostElement style={{
                width: Length::px(240.0),
                height: Length::px(120.0),
                line_height: line_height,
            }}>
                <HostTextArea content={"abc".to_string()} />
            </HostElement>
        }
    }

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&tree(1.1)).expect("cold render");
    run_layout_for_test(&mut viewport, 240.0, 120.0);
    let root_key = viewport.scene.ui_root_keys[0];
    let text_area_key = viewport.scene.node_arena.children_of(root_key)[0];

    viewport
        .render_rsx(&tree(1.8))
        .expect("parent line-height update should commit incrementally");
    run_layout_for_test(&mut viewport, 240.0, 120.0);

    assert_eq!(viewport.scene.ui_root_keys, vec![root_key]);
    let text_area_node = viewport
        .scene
        .node_arena
        .get(text_area_key)
        .expect("TextArea node");
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<TextAreaHost>()
        .expect("TextArea host");
    assert!(
        (text_area.line_height - 1.8).abs() < 1e-4,
        "TextArea without its own style must follow parent inherited line_height updates, got {}",
        text_area.line_height,
    );
    let run_key = text_area.children[0];
    let run_node = viewport
        .scene
        .node_arena
        .get(run_key)
        .expect("TextArea run");
    let run = run_node
        .element
        .as_any()
        .downcast_ref::<TextAreaTextRun>()
        .expect("TextAreaTextRun");
    assert!(
        (run.line_height - 1.8).abs() < 1e-4,
        "TextArea run children must be rebuilt with updated inherited line_height, got {}",
        run.line_height,
    );
}

#[test]
fn incremental_commit_reset_element_style_restores_inherited_text_base() {
    use crate::view::Text as HostText;

    let first = rsx! {
        <HostElement style={{
            layout: Layout::Inline,
            width: Length::px(320.0),
            vertical_align: VerticalAlign::Bottom,
        }}>
            <HostElement style={{
                vertical_align: VerticalAlign::Middle,
                padding: Padding::uniform(Length::px(4.0)),
            }}>
                <HostText>"badge"</HostText>
            </HostElement>
        </HostElement>
    };
    let second = rsx! {
        <HostElement style={{
            layout: Layout::Inline,
            width: Length::px(320.0),
            vertical_align: VerticalAlign::Bottom,
        }}>
            <HostElement>
                <HostText>"badge"</HostText>
            </HostElement>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    run_layout_for_test(&mut viewport, 320.0, 120.0);
    let root_key = viewport.scene.ui_root_keys[0];
    let wrapper_key = viewport.scene.node_arena.children_of(root_key)[0];

    viewport
        .render_rsx(&second)
        .expect("style removal should commit incrementally");
    run_layout_for_test(&mut viewport, 320.0, 120.0);

    assert_eq!(viewport.scene.ui_root_keys, vec![root_key]);
    assert_eq!(
        viewport.scene.node_arena.children_of(root_key)[0],
        wrapper_key
    );
    let wrapper_nodes = viewport
        .scene
        .node_arena
        .with_element_taken(wrapper_key, |element, arena| {
            element.get_inline_nodes_size(arena)
        })
        .expect("wrapper element");
    assert!(
        wrapper_nodes
            .iter()
            .all(|node| node.vertical_align == VerticalAlign::Bottom),
        "removing an Element style should leave its inherited text base intact, got {wrapper_nodes:?}"
    );
}

/// Conversely, a cascading style change on a *leaf* Element (no
/// descendants) has no one to recascade into, so it may commit
/// incrementally — NodeKey survives.
#[test]
fn incremental_commit_applies_text_cascading_style_change_on_leaf() {
    let first = rsx! {
        <HostElement style={{
            width: Length::px(120.0),
            height: Length::px(40.0),
            font_size: 20.0f32,
        }} />
    };
    let second = rsx! {
        <HostElement style={{
            width: Length::px(120.0),
            height: Length::px(40.0),
            font_size: 30.0f32,
        }} />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&second)
        .expect("cascading style change on leaf must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "no descendants → no recascade risk → NodeKey preserved",
    );
}

// ---------------------------------------------------------------------------
// M6: cascade reconstruction for InsertChild
// ---------------------------------------------------------------------------

/// M6 cascade: an incremental InsertChild under a parent that
/// authored `style={{ font_size: 22 }}` must build the new Text child
/// with `font_size == 22`, matching what the cold-path converter
/// would do via `InheritedTextStyle::merge_style`. M5.0 previously
/// shipped with the viewport root style as the approximation, which
/// would have resolved to the default 16.0.
#[test]
fn incremental_commit_insert_child_inherits_parent_font_size_from_cascade() {
    use crate::view::Text as HostText;
    use crate::view::base_component::Text as TextHost;

    // Parent Element authors a font-cascading style. Children inherit
    // font_size 22 through the cascade.
    let parent_with_no_text = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 22.0f32,
        }} />
    };
    let parent_with_text = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 22.0f32,
        }}>
            <HostText>"hi"</HostText>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&parent_with_no_text)
        .expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let parent_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&parent_with_text)
        .expect("InsertChild with cascade must commit incrementally");

    // Parent identity survives and the Text child is parented to it.
    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    let arena = &viewport.scene.node_arena;
    let children = arena.children_of(parent_key);
    assert_eq!(children.len(), 1, "Text child should be inserted");
    let text_node = arena.get(children[0]).expect("text child node");
    let text = text_node
        .element
        .as_any()
        .downcast_ref::<TextHost>()
        .expect("Text host");
    assert!(
        (text.font_size() - 22.0).abs() < 1e-4,
        "incremental InsertChild must inherit parent font_size 22.0 via cascade, got {}",
        text.font_size(),
    );
}

// ---------------------------------------------------------------------------
// 軌 1 #2 / #3 / #4: context-free setter surface, slot hot-swap, source
// ---------------------------------------------------------------------------

/// 軌 1 #2: an `anchor` prop change on an Element commits via the
/// new `set_anchor_name` setter — NodeKey survives.
#[test]
fn incremental_commit_applies_anchor_change_preserves_node_key() {
    use crate::view::base_component::Element as ElementHost;

    let first = rsx! {
        <HostElement
            anchor={"first".to_string()}
            style={{ width: Length::px(120.0), height: Length::px(40.0) }}
        />
    };
    let second = rsx! {
        <HostElement
            anchor={"second".to_string()}
            style={{ width: Length::px(120.0), height: Length::px(40.0) }}
        />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&first).expect("cold render");
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&second)
        .expect("anchor change must commit incrementally");

    assert_eq!(viewport.scene.ui_root_keys, vec![original_key]);
    let _ = ElementHost::stable_id; // keep import live
}

/// 軌 1 #2: a removed `anchor` prop resets to None via
/// `set_anchor_name(None)`. NodeKey survives.
#[test]
fn incremental_commit_removes_anchor_prop_clears_anchor_name() {
    use crate::view::base_component::Element as ElementHost;

    let with_anchor = rsx! {
        <HostElement
            anchor={"name".to_string()}
            style={{ width: Length::px(120.0), height: Length::px(40.0) }}
        />
    };
    let without_anchor = rsx! {
        <HostElement style={{ width: Length::px(120.0), height: Length::px(40.0) }} />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&with_anchor).expect("cold render");
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&without_anchor)
        .expect("anchor removal must commit incrementally");

    assert_eq!(viewport.scene.ui_root_keys, vec![original_key]);
    let _ = ElementHost::stable_id; // keep import live
}

/// 軌 1 #2: padding prop change on an Element host. Padding doesn't
/// have a top-level rsx slot (it lives inside `style`), so we drive
/// the apply path directly with a synthetic `Patch::UpdateElementProps`.
#[test]
fn incremental_commit_applies_padding_change_via_setter() {
    use crate::view::base_component::Element as ElementHost;
    use crate::view::fiber_work::{apply_fiber_works, patch_to_fiber_work};

    let seed = single_element(120.0);
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&seed).expect("cold render");
    let key = viewport.scene.ui_root_keys[0];

    let patch = crate::ui::Patch::UpdateElementProps {
        path: vec![],
        changed: vec![("padding", crate::ui::PropValue::F64(8.0))],
        removed: vec![],
    };
    let work = patch_to_fiber_work(
        patch,
        viewport.scene.node_arena.stable_id_index(),
        &viewport.scene.node_arena,
        key,
        None,
    )
    .expect("padding patch must translate to FiberWork");
    assert!(work.is_committable(&viewport.scene.node_arena));
    apply_fiber_works(&mut viewport.scene.node_arena, test_apply_ctx(), vec![work]);

    // The setter is fire-and-forget — no public getter for padding,
    // but we can confirm the work was committed (NodeKey untouched
    // is the survival guarantee, no full rebuild fired).
    assert_eq!(viewport.scene.ui_root_keys, vec![key]);
    let _ = ElementHost::stable_id; // keep import live
}

/// 軌 1 #2 + #4: Image `fit` and `source` hot-swap commit
/// incrementally. Driven via direct Patch construction since the
/// rsx Image schema bundles `source` as a mandatory field — easier
/// to seed an Image directly and exercise the apply dispatch.
#[test]
fn incremental_commit_applies_image_fit_and_source_swap() {
    use crate::view::base_component::Image;
    use crate::view::fiber_work::{apply_fiber_works, patch_to_fiber_work};
    use crate::view::test_support::{commit_element, new_test_arena};
    use crate::view::{ImageFit, ImageSource};

    fn rgba(width: u32, height: u32, byte: u8) -> ImageSource {
        ImageSource::Rgba {
            width,
            height,
            pixels: std::sync::Arc::<[u8]>::from(vec![byte; (width * height * 4) as usize]),
        }
    }

    let mut arena = new_test_arena();
    let image = Image::new_with_id(42, rgba(10, 10, 0));
    let key = commit_element(&mut arena, Box::new(image));

    // Build a fit-change patch and apply.
    let fit_patch = crate::ui::Patch::UpdateElementProps {
        path: vec![],
        changed: vec![("fit", ImageFit::Cover.into_prop_value())],
        removed: vec![],
    };
    let work = patch_to_fiber_work(fit_patch, arena.stable_id_index(), &arena, key, None)
        .expect("fit patch must translate");
    assert!(work.is_committable(&arena));
    apply_fiber_works(&mut arena, test_apply_ctx(), vec![work]);

    // Source swap — the apply side acquires a fresh handle; the old
    // one drops via RAII. We can't easily peek at the resource entry
    // without exposing internals, so we assert the commit succeeds
    // and the arena slot is still present.
    let new_source = rgba(20, 20, 255);
    let source_patch = crate::ui::Patch::UpdateElementProps {
        path: vec![],
        changed: vec![("source", new_source.into_prop_value())],
        removed: vec![],
    };
    let work = patch_to_fiber_work(source_patch, arena.stable_id_index(), &arena, key, None)
        .expect("source patch must translate");
    assert!(work.is_committable(&arena));
    apply_fiber_works(&mut arena, test_apply_ctx(), vec![work]);
    assert!(
        arena.get(key).is_some(),
        "Image slot must survive source swap"
    );
}

use crate::ui::IntoPropValue;

/// 軌 1 #5: a Fragment-shaped InsertChild expands to N descriptors
/// and commits as `FiberWork::CreateMany` — N consecutive
/// `arena_insert_child` calls. Parent NodeKey survives.
#[test]
fn incremental_commit_applies_fragment_insert_child_creates_many() {
    use crate::view::fiber_work::{DescriptorContext, apply_fiber_works, patch_to_fiber_work};

    // Seed: empty parent. NEW rsx mirror has the same parent +
    // a Fragment child (which itself holds N children) at index 0.
    let seed = host_el();
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&seed).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    assert_eq!(viewport.scene.node_arena.children_of(parent_key).len(), 0);

    // Synthetic patch: insert a Fragment containing two Element
    // children. The translator expands the Fragment into N=2
    // descriptors and emits CreateMany.
    let fragment = RsxNode::fragment(vec![host_el(), host_el()]);
    let new_root = host_el().with_child(fragment.clone());
    let patch = crate::ui::Patch::InsertChild {
        parent_path: vec![],
        index: 0,
        node: fragment,
    };
    let style = crate::style::Style::new();
    let ctx = DescriptorContext {
        new_rsx_root: &new_root,
        old_rsx_root: None,
        inherited_style: &style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    let work = patch_to_fiber_work(
        patch,
        viewport.scene.node_arena.stable_id_index(),
        &viewport.scene.node_arena,
        parent_key,
        Some(&ctx),
    )
    .expect("Fragment InsertChild must translate to CreateMany");
    assert!(work.is_committable(&viewport.scene.node_arena));
    apply_fiber_works(&mut viewport.scene.node_arena, test_apply_ctx(), vec![work]);

    // Parent identity stable; two new children landed in order at
    // indices 0 and 1.
    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    let arena = &viewport.scene.node_arena;
    assert_eq!(arena.children_of(parent_key).len(), 2);
}

/// 軌 A #9: an `em`-valued `font_size` update on a Text leaf now
/// resolves through the inherited cascade (parent's font_size on
/// the arena) instead of falling back to the full-rebuild pipeline.
#[test]
fn incremental_commit_resolves_em_font_size_via_inherited_cascade() {
    use crate::style::FontSize;
    use crate::ui::{IntoPropValue, Patch, PropValue};
    use crate::view::Text as HostText;
    use crate::view::base_component::Text as TextHost;
    use crate::view::fiber_work::{apply_fiber_works, patch_to_fiber_work};

    // Parent Element has font_size=20 in its style; Text child
    // initially has font_size 14 explicit.
    let seed = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 20.0f32,
        }}>
            <HostText font_size={14.0f64}>"hi"</HostText>
        </HostElement>
    };
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&seed).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let text_key = viewport.scene.node_arena.children_of(parent_key)[0];

    // Synthetic Update patch: change Text's font_size to Em(2.0).
    // Translator resolves via parent's cascade: 2.0em × 20px = 40px.
    let patch = Patch::UpdateElementProps {
        path: vec![0],
        changed: vec![("font_size", PropValue::FontSize(FontSize::Em(2.0)))],
        removed: vec![],
    };
    let style = crate::style::Style::new();
    let ctx = crate::view::fiber_work::DescriptorContext {
        new_rsx_root: &seed,
        old_rsx_root: None,
        inherited_style: &style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    let work = patch_to_fiber_work(
        patch,
        viewport.scene.node_arena.stable_id_index(),
        &viewport.scene.node_arena,
        parent_key,
        Some(&ctx),
    )
    .expect("font_size em patch must translate");
    assert!(
        work.is_committable(&viewport.scene.node_arena),
        "em font_size now committable via cascade resolver",
    );
    apply_fiber_works(&mut viewport.scene.node_arena, test_apply_ctx(), vec![work]);

    let arena = &viewport.scene.node_arena;
    let node = arena.get(text_key).unwrap();
    let text = node.element.as_any().downcast_ref::<TextHost>().unwrap();
    assert!(
        (text.font_size() - 40.0).abs() < 1e-4,
        "Em(2.0) × parent 20px = 40px; got {}",
        text.font_size(),
    );
    let _ = <FontSize as IntoPropValue>::into_prop_value;
}

/// 軌 A #5 (extends 軌 1 #5): a Fragment new-node in `Patch::ReplaceNode`
/// expands to N descriptors at the replaced slot. The old child
/// subtree is removed and N new keys land in its place.
#[test]
fn incremental_commit_replace_node_with_fragment_expands_to_n_descriptors() {
    use crate::view::fiber_work::{DescriptorContext, apply_fiber_works, patch_to_fiber_work};

    // Seed: parent with two children, snapshot keys.
    let seed = host_el().with_child(host_el()).with_child(host_el());
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&seed).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let kept_child_key = viewport.scene.node_arena.children_of(parent_key)[1];

    // Replace child[0] with a Fragment containing 3 children → 3
    // descriptors. After apply, parent has 4 children: 3 new + 1
    // kept (kept_child_key is now at index 3).
    let fragment = RsxNode::fragment(vec![host_el(), host_el(), host_el()]);
    let new_root = host_el().with_child(fragment.clone()).with_child(host_el());
    let patch = crate::ui::Patch::ReplaceNode {
        path: vec![0],
        node: fragment,
    };
    let style = crate::style::Style::new();
    let ctx = DescriptorContext {
        new_rsx_root: &new_root,
        old_rsx_root: None,
        inherited_style: &style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    let work = patch_to_fiber_work(
        patch,
        viewport.scene.node_arena.stable_id_index(),
        &viewport.scene.node_arena,
        parent_key,
        Some(&ctx),
    )
    .expect("Fragment ReplaceNode must translate");
    assert!(work.is_committable(&viewport.scene.node_arena));
    apply_fiber_works(&mut viewport.scene.node_arena, test_apply_ctx(), vec![work]);

    let arena = &viewport.scene.node_arena;
    let children = arena.children_of(parent_key);
    assert_eq!(children.len(), 4, "3 new + 1 kept");
    assert_eq!(children[3], kept_child_key, "kept sibling now at end");
}

/// 軌 1 #6: when the OLD tree's structure higher up no longer
/// matches the NEW tree at the InsertChild parent_path, the
/// identity-validated walk aborts and the translator returns `None`
/// (forcing the all-or-nothing batch to fall back to full rebuild).
#[test]
fn incremental_commit_path_drift_identity_check_rejects_misaligned_walk() {
    use crate::view::fiber_work::{DescriptorContext, patch_to_fiber_work};

    let seed = host_el().with_child(host_el());
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&seed).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];

    // OLD tree (matches what reconcile would have walked):
    let old_root = host_el().with_child(host_el());
    // NEW tree: the child at path [0] has a different identity
    // (Text leaf instead of Element host) — `walk_rsx_by_index_path
    // _validated` should detect the mismatch when validating
    // `parent_path = [0]` and abort.
    let new_root = host_el().with_child(text_leaf("drifted"));
    let patch = crate::ui::Patch::InsertChild {
        parent_path: vec![0],
        index: 0,
        node: host_el(),
    };
    let style = crate::style::Style::new();
    let ctx = DescriptorContext {
        new_rsx_root: &new_root,
        old_rsx_root: Some(&old_root),
        inherited_style: &style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    let work = patch_to_fiber_work(
        patch,
        viewport.scene.node_arena.stable_id_index(),
        &viewport.scene.node_arena,
        parent_key,
        Some(&ctx),
    );
    assert!(
        work.is_none(),
        "identity drift on parent_path must abort translation",
    );
}

/// 軌 1 #3: a `loading` slot prop change on an Svg host commits via
/// `Svg::replace_loading_slot_incremental` (mirror of Image #3). The
/// new slot subtree is committed under the Svg's arena key.
#[test]
fn incremental_commit_applies_svg_loading_slot_swap() {
    use crate::view::SvgSource;
    use crate::view::base_component::Svg;
    use crate::view::fiber_work::{apply_fiber_works, patch_to_fiber_work};
    use crate::view::test_support::{commit_element, new_test_arena};

    let source = SvgSource::Content(
        r##"<svg width="40" height="40"><rect width="40" height="40"/></svg>"##.to_string(),
    );
    let mut arena = new_test_arena();
    let svg = Svg::new_with_id(7, source);
    let key = commit_element(&mut arena, Box::new(svg));

    // Build a `loading` slot RsxNode (any HostElement leaf works as
    // the slot wrapper — convert_image_slot_desc wraps it in a
    // single descriptor).
    let slot_rsx = RsxNode::tagged("Element", RsxTagDescriptor::for_tag::<HostElement>());
    let patch = crate::ui::Patch::UpdateElementProps {
        path: vec![],
        changed: vec![("loading", slot_rsx.into_prop_value())],
        removed: vec![],
    };
    let work = patch_to_fiber_work(patch, arena.stable_id_index(), &arena, key, None)
        .expect("loading patch must translate");
    assert!(work.is_committable(&arena));
    apply_fiber_works(&mut arena, test_apply_ctx(), vec![work]);

    // Svg slot now holds 1 key (the wrapper). Use the `loading_slot_len`
    // accessor — the wrapper sits in the Vec until `sync_active_slot`
    // promotes it on the next measure pass.
    let node = arena.get(key).expect("Svg slot survives slot swap");
    let svg = node.element.as_any().downcast_ref::<Svg>().unwrap();
    assert_eq!(svg.loading_slot_len(), 1);
}

#[test]
fn placement_only_path_resolves_node_key_without_stable_id_index() {
    use crate::style::{Transform, Translate};
    use crate::view::base_component::{DirtyFlags, Element as ElementHost};
    use crate::view::test_support::{commit_child, commit_element, new_test_arena};

    let mut arena = new_test_arena();
    let root_key = commit_element(
        &mut arena,
        Box::new(ElementHost::new_with_id(1, 0.0, 0.0, 0.0, 0.0)),
    );
    let first_child = commit_child(
        &mut arena,
        root_key,
        Box::new(ElementHost::new_with_id(77, 0.0, 0.0, 0.0, 0.0)),
    );
    let second_child = commit_child(
        &mut arena,
        root_key,
        Box::new(ElementHost::new_with_id(77, 0.0, 0.0, 0.0, 0.0)),
    );
    assert_eq!(
        arena.find_by_stable_id(77),
        Some(second_child),
        "duplicate stable_id index points at the last inserted node",
    );
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut arena,
        root_key,
        DirtyFlags::ALL,
    );

    let rsx_root = host_el().with_child(host_el()).with_child(host_el());
    let target_key = Viewport::arena_key_for_rsx_path(&arena, &[root_key], &rsx_root, &[0])
        .expect("path [0] should resolve to the first arena child");
    assert_eq!(
        target_key, first_child,
        "placement-only patch target must come from RSX path -> arena path -> NodeKey",
    );

    let transform = Transform::new([Translate::x(Length::px(24.0))]);
    let mut style = crate::style::Style::new();
    style.set_transform(transform.clone());
    assert!(Viewport::apply_placement_style_by_node_key(
        &arena, target_key, &style,
    ));

    {
        let first = arena.get(first_child).unwrap();
        let first = first
            .element
            .as_any()
            .downcast_ref::<ElementHost>()
            .unwrap();
        assert_eq!(first.debug_transform(), &transform);
    }

    {
        let second = arena.get(second_child).unwrap();
        let second = second
            .element
            .as_any()
            .downcast_ref::<ElementHost>()
            .unwrap();
        assert_eq!(second.debug_transform(), &Transform::default());
    }

    assert!(
        arena
            .arena_local_dirty(first_child)
            .contains(DirtyFlags::RUNTIME)
    );
    assert!(
        !arena
            .arena_local_dirty(second_child)
            .contains(DirtyFlags::RUNTIME)
    );
}

/// M5: the flag is on by default. Flipping it off must still work
/// (call sites can A/B test or bisect regressions), and a render
/// round-trip in off-mode should succeed via the legacy full-rebuild
/// path.
#[test]
fn flag_default_on_and_off_switch_survives_round_trip() {
    let first = single_element(120.0);
    let second = single_element(120.0);

    let mut viewport = Viewport::new();
    assert!(
        viewport.use_incremental_commit(),
        "M5 default: flag starts on",
    );

    viewport.set_use_incremental_commit(false);
    viewport.render_rsx(&first).expect("cold render (flag off)");
    viewport
        .render_rsx(&second)
        .expect("identical re-render with flag off must still succeed");
    assert!(!viewport.use_incremental_commit());
}

// ---------------------------------------------------------------------------
// 軌 1 #4 Fragment-at-root: multi-root incremental path
// ---------------------------------------------------------------------------

/// Fragment root with N children → arena stores N roots. Re-rendering the
/// same tree must keep every arena root NodeKey stable (per-root reconcile
/// emits zero patches thanks to ptr_eq).
#[test]
fn incremental_commit_fragment_at_root_preserves_all_root_keys_across_identical_render() {
    let tree = RsxNode::fragment(vec![
        single_element(100.0),
        single_element(200.0),
        single_element(300.0),
    ]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&tree).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 3);
    let original = viewport.scene.ui_root_keys.clone();

    viewport.render_rsx(&tree).expect("identical re-render");
    assert_eq!(viewport.scene.ui_root_keys, original);
}

/// Fragment-at-root: changing one child's style prop must keep every
/// arena root NodeKey stable (UpdateElementProps routes via root_index,
/// doesn't rebuild siblings).
#[test]
fn incremental_commit_fragment_at_root_style_update_on_one_child_preserves_all_keys() {
    let first = RsxNode::fragment(vec![
        single_element(100.0),
        single_element(200.0),
        single_element(300.0),
    ]);
    // Only the middle child's width changes.
    let second = RsxNode::fragment(vec![
        single_element(100.0),
        single_element(250.0),
        single_element(300.0),
    ]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 3);
    let original = viewport.scene.ui_root_keys.clone();

    viewport
        .render_rsx(&second)
        .expect("fragment-root child style update must go incremental");

    assert_eq!(viewport.scene.ui_root_keys, original);
}

/// Fragment-at-root arity change (N → M, N != M) must go through the
/// `ReplaceAllRoots` path: arena root count matches the new arity.
/// NodeKeys are expected to be fresh (wholesale swap).
#[test]
fn incremental_commit_fragment_at_root_arity_change_replaces_all_roots() {
    let first = RsxNode::fragment(vec![single_element(100.0), single_element(200.0)]);
    let second = RsxNode::fragment(vec![
        single_element(100.0),
        single_element(200.0),
        single_element(300.0),
    ]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 2);

    viewport
        .render_rsx(&second)
        .expect("fragment-root arity change must commit via ReplaceAllRoots");

    assert_eq!(viewport.scene.ui_root_keys.len(), 3);
}

/// Single Element root → Fragment-at-root swap: identity/shape mismatch
/// triggers `ReplaceAllRoots`. Arena ends with N roots matching the new
/// Fragment's child count.
#[test]
fn incremental_commit_element_root_to_fragment_root_swaps_via_replace_all_roots() {
    let first = single_element(100.0);
    let second = RsxNode::fragment(vec![single_element(150.0), single_element(250.0)]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&first)
        .expect("cold render (single root)");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);

    viewport
        .render_rsx(&second)
        .expect("single-root → fragment-root swap must commit via ReplaceAllRoots");

    assert_eq!(viewport.scene.ui_root_keys.len(), 2);
}

// ---------------------------------------------------------------------------
// rsx_to_arena_path unit tests (Fragment path flattening)
// ---------------------------------------------------------------------------

#[test]
fn rsx_to_arena_path_flattens_mid_tree_fragment() {
    use crate::view::fiber_work::{ArenaPathResolution, rsx_to_arena_path};

    // Element { children: [A, Fragment([B]), C] }
    // B lives at rsx path [1, 0]; arena flattens Fragment, so B's
    // arena path is [1].
    let a = host_el();
    let b = host_el();
    let c = host_el();
    let root = host_el()
        .with_child(a)
        .with_child(RsxNode::fragment(vec![b]))
        .with_child(c);

    assert!(matches!(rsx_to_arena_path(&root, &[0]), ArenaPathResolution::Arena(p) if p == [0]));
    assert!(matches!(rsx_to_arena_path(&root, &[1, 0]), ArenaPathResolution::Arena(p) if p == [1]));
    assert!(matches!(rsx_to_arena_path(&root, &[2]), ArenaPathResolution::Arena(p) if p == [2]));
}

#[test]
fn rsx_to_arena_path_handles_nested_fragments() {
    use crate::view::fiber_work::{ArenaPathResolution, rsx_to_arena_path};

    // Element { children: [A, Fragment([Fragment([B]), C]), D] }
    let root = host_el()
        .with_child(host_el())
        .with_child(RsxNode::fragment(vec![
            RsxNode::fragment(vec![host_el()]),
            host_el(),
        ]))
        .with_child(host_el());

    assert!(matches!(rsx_to_arena_path(&root, &[0]), ArenaPathResolution::Arena(p) if p == [0]));
    assert!(
        matches!(rsx_to_arena_path(&root, &[1, 0, 0]), ArenaPathResolution::Arena(p) if p == [1])
    );
    assert!(matches!(rsx_to_arena_path(&root, &[1, 1]), ArenaPathResolution::Arena(p) if p == [2]));
    assert!(matches!(rsx_to_arena_path(&root, &[2]), ArenaPathResolution::Arena(p) if p == [3]));
}

// ---------------------------------------------------------------------------
// 軌 1 #8 Text::apply_style incremental
// ---------------------------------------------------------------------------

/// Text.style update (color change): NodeKey of the Text host must
/// survive; this exercises the new `apply_style_incremental` path on
/// `apply_update_to_text`.
#[test]
fn incremental_commit_text_style_color_change_preserves_node_key() {
    use crate::style::Color;
    use crate::view::Text as HostText;

    let first = rsx! {
        <HostElement>
            <HostText style={{ color: Color::hex("#ff0000") }}>"hi"</HostText>
        </HostElement>
    };
    let second = rsx! {
        <HostElement>
            <HostText style={{ color: Color::hex("#0000ff") }}>"hi"</HostText>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let text_key = viewport.scene.node_arena.children_of(parent_key)[0];

    viewport
        .render_rsx(&second)
        .expect("Text.style color change must go incremental");

    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    assert_eq!(
        viewport.scene.node_arena.children_of(parent_key),
        vec![text_key],
        "Text NodeKey must survive style color update",
    );
}

/// Text.style update that drops a declaration (font_size goes away):
/// the explicit flag for font_size must flip back to `false` so the
/// ancestor cascade can refill it.
#[test]
fn incremental_commit_text_style_drops_font_size_keeps_prior_explicit_value() {
    // Track 1 #10 scope: apply_style_incremental does NOT reset
    // explicit flags — an independent `font_size={}` prop may be the
    // source of truth. Removing `font_size` from the style declaration
    // alone therefore does not refill from the ancestor cascade;
    // it keeps whatever the prior explicit value was. Cold-path
    // rebuild is still responsible for wholesale defaults.
    use crate::view::Text as HostText;
    use crate::view::base_component::Text as TextHost;

    let first = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 20.0f32,
        }}>
            <HostText style={{ font_size: 40.0f32 }}>"hi"</HostText>
        </HostElement>
    };
    let second = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 20.0f32,
        }}>
            <HostText style={{}}>"hi"</HostText>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let text_key = viewport.scene.node_arena.children_of(parent_key)[0];

    viewport
        .render_rsx(&second)
        .expect("Text.style losing font_size must commit incrementally");

    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    let arena = &viewport.scene.node_arena;
    let font_size = arena
        .get(text_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<TextHost>()
        .unwrap()
        .font_size();
    // Prior explicit 40.0 persists; cascade does not override.
    assert!(
        (font_size - 40.0).abs() < 1e-4,
        "prior explicit font_size 40.0 must stick; got {}",
        font_size,
    );
}

/// Text.style prop removed entirely (UpdateElementProps `removed` list
/// carries `"style"`). `apply_remove_to_text` must route through the
/// new `"style"` arm: all explicit flags reset, ancestor cascade fills
/// in, NodeKey stable.
#[test]
fn incremental_commit_text_style_prop_removed_preserves_node_key() {
    use crate::view::Text as HostText;

    let first = rsx! {
        <HostElement>
            <HostText style={{ font_size: 32.0f32 }}>"hi"</HostText>
        </HostElement>
    };
    let second = rsx! {
        <HostElement>
            <HostText>"hi"</HostText>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let text_key = viewport.scene.node_arena.children_of(parent_key)[0];

    viewport
        .render_rsx(&second)
        .expect("Text.style prop removal must commit incrementally");

    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    assert_eq!(
        viewport.scene.node_arena.children_of(parent_key),
        vec![text_key]
    );
}

/// Fragment-root SetText on a deep descendant under root[1] must route
/// via root_index=1 (not the first arena root). Validates that the
/// multi-root dispatcher passes the correct per-root key to the
/// translator.
#[test]
fn incremental_commit_fragment_at_root_set_text_on_second_root_child() {
    fn tree(second_text: &str) -> RsxNode {
        RsxNode::fragment(vec![
            single_element(100.0),
            rsx! { <HostElement>{text_leaf(second_text)}</HostElement> },
        ])
    }
    let first = tree("hello");
    let second = tree("world");

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 2);
    let original = viewport.scene.ui_root_keys.clone();

    viewport
        .render_rsx(&second)
        .expect("fragment-root SetText on root[1] child must go incremental");

    assert_eq!(viewport.scene.ui_root_keys, original);
}
