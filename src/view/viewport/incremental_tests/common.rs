use super::super::Viewport;
use crate::style::{
    ClipMode, Color, Cursor, Layout, Length, Padding, ParsedValue, Position, PropertyId,
    ScrollDirection, Style, Transition, TransitionProperty, Transitions, VerticalAlign,
};
use crate::ui::{Binding, DragEffect, RsxNode, RsxTagDescriptor, on_drag_over, on_drop, rsx};
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

pub(super) fn host_el() -> RsxNode {
    RsxNode::tagged("Element", RsxTagDescriptor::for_tag::<HostElement>())
}

pub(super) fn single_element(width_px: f32) -> RsxNode {
    rsx! {
        <HostElement style={{
            width: Length::px(width_px),
            height: Length::px(40.0),
        }} />
    }
}

pub(super) fn text_leaf(content: &str) -> RsxNode {
    RsxNode::text(content)
}

pub(super) fn inline_badge_vertical_align_tree(vertical_align: VerticalAlign) -> RsxNode {
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

pub(super) fn collect_text_contents(
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

pub(super) fn find_text_node(
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

pub(super) fn pointer_cursor_with_text_child_tree() -> RsxNode {
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

pub(super) fn overlapping_root_with_anchor_parent_resize_handle_tree() -> RsxNode {
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

pub(super) fn same_root_escape_descendant_under_later_sibling_tree() -> RsxNode {
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

pub(super) fn movable_root_with_anchor_parent_resize_handle_tree(left: f32) -> RsxNode {
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

pub(super) fn retained_window_accordion_button_tree() -> RsxNode {
    retained_window_accordion_button_tree_with_expanded(false)
}

pub(super) fn retained_window_accordion_button_tree_with_expanded(expanded: bool) -> RsxNode {
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

pub(super) fn retained_component_test_button_section_tree_with_expanded(expanded: bool) -> RsxNode {
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

pub(super) fn flex_base_only_axis_workload_tree() -> RsxNode {
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

pub(super) fn flex_nested_base_only_axis_workload_tree() -> RsxNode {
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

pub(super) fn flex_base_only_axis_workload_tree_with_gap(gap: f32) -> RsxNode {
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

pub(super) fn flex_base_only_column_axis_workload_tree() -> RsxNode {
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

pub(super) fn flex_with_text_area_descendant_tree() -> RsxNode {
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

pub(super) fn inline_text_axis_workload_tree() -> RsxNode {
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

pub(super) fn expanded_retained_accordion_section_style() -> Style {
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

pub(super) fn drag_drop_rerender_tree(hovering: bool, dropped: Binding<Vec<String>>) -> RsxNode {
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
pub(super) fn test_apply_ctx() -> crate::view::fiber_work::ApplyContext<'static> {
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
pub(super) fn run_layout_for_test(viewport: &mut Viewport, viewport_w: f32, viewport_h: f32) {
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

pub(super) fn run_layout_for_test_with_gate_profile(
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

pub(super) fn sample_clean_parent_relayout_for_placement_profile(
    tree: &RsxNode,
    viewport_w: f32,
    viewport_h: f32,
) -> crate::view::base_component::LayoutPlaceProfile {
    let mut viewport = Viewport::new();
    viewport.set_size(viewport_w as u32, viewport_h as u32);
    viewport.render_rsx(tree).expect("render workload tree");
    run_layout_for_test(&mut viewport, viewport_w, viewport_h);

    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, viewport_w, viewport_h);
    place_profile
}

pub(super) fn nested_box_model_tree() -> RsxNode {
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

pub(super) fn two_child_box_model_tree() -> RsxNode {
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

pub(super) fn two_child_grid_box_model_tree() -> RsxNode {
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

pub(super) fn nested_grid_box_model_tree() -> RsxNode {
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

pub(super) fn nested_grid_with_anchor_descendant_tree() -> RsxNode {
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

pub(super) fn nested_grid_with_absolute_descendant_tree() -> RsxNode {
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

pub(super) fn nested_grid_with_text_area_descendant_tree() -> RsxNode {
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

pub(super) fn scrollable_grid_box_model_tree() -> RsxNode {
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

pub(super) fn scrollable_box_model_tree() -> RsxNode {
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

pub(super) fn paint_dirty_on_click_tree() -> RsxNode {
    crate::view::host_builder_node::<PaintDirtyOnClickHost>("PaintDirtyOnClick")
}

pub(super) fn box_model_snapshot_for_node(
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

pub(super) fn mark_box_model_dirty_and_set_layout_width(
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

pub(super) fn mark_place_dirty_for_test(
    viewport: &mut Viewport,
    key: crate::view::node_arena::NodeKey,
) {
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

pub(super) fn mark_paint_dirty_for_test(
    viewport: &mut Viewport,
    key: crate::view::node_arena::NodeKey,
) {
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

pub(super) fn assert_same_box_model_snapshot(
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
