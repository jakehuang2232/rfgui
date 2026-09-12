use super::*;
use crate::ui::{RsxNode, RsxTagDescriptor};
use crate::view::TextArea as HostTextArea;
use crate::view::base_component::TextArea;
use crate::view::test_support::{commit_rsx_tree, measure_and_place};

fn host_text_area_node() -> RsxNode {
    RsxNode::tagged("TextArea", RsxTagDescriptor::for_tag::<HostTextArea>())
}

fn std_constraints() -> crate::view::base_component::LayoutConstraints {
    crate::view::base_component::LayoutConstraints {
        max_width: 800.0,
        max_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    }
}

fn std_placement() -> crate::view::base_component::LayoutPlacement {
    crate::view::base_component::LayoutPlacement {
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
    }
}

/// Same fixture as `build_map_for` but returns the live arena +
/// TextArea pointer so callers can poke the underlying Run for
/// affinity-aware position probes.
fn build_wrapped_textarea(
    content: &str,
    max_width: f32,
) -> (
    *const crate::view::base_component::TextArea,
    crate::view::node_arena::NodeArena,
) {
    let tree = host_text_area_node().with_prop("content", content);
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    let mut constraints = std_constraints();
    constraints.max_width = max_width;
    let mut placement = std_placement();
    placement.available_width = max_width;
    measure_and_place(&mut arena, root, constraints, placement);
    let ptr: *const crate::view::base_component::TextArea = arena
        .with_element_taken_ref(root, |el, _| {
            el.as_any()
                .downcast_ref::<crate::view::base_component::TextArea>()
                .unwrap() as *const _
        })
        .unwrap();
    (ptr, arena)
}

fn build_map_for(content: &str, max_width: f32) -> (std::rc::Rc<CaretNavigationMap>, usize) {
    let tree = host_text_area_node().with_prop("content", content);
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    let mut constraints = std_constraints();
    constraints.max_width = max_width;
    let mut placement = std_placement();
    placement.available_width = max_width;
    measure_and_place(&mut arena, root, constraints, placement);
    let text_area_ptr: *const TextArea = arena
        .with_element_taken_ref(root, |el, _| {
            el.as_any().downcast_ref::<TextArea>().unwrap() as *const TextArea
        })
        .unwrap();
    // SAFETY: arena is borrowed read-only for the duration of the
    // build call below. The pointer stays valid because `arena`
    // outlives this block and we never mutate it here.
    let text_area: &TextArea = unsafe { &*text_area_ptr };
    let map = CaretNavigationMap::build(text_area, &arena);
    let _ = &arena;
    let len = content.chars().count();
    (map, len)
}

// ---------------------------------------------------------------
// Projection-aware navigation map tests.
//
// Fixture builds a TextArea whose `on_render_handler` projects a
// single contiguous char range onto an `<Element>` (optionally
// containing a `<Text>` so glyph stops light up). Mirrors
// render.rs's projection_fixture style but with caret_map's
// assertions.
// ---------------------------------------------------------------

use crate::style::Length;
use crate::view::ElementStylePropSchema;
use crate::view::base_component::{ElementTrait, LayoutConstraints, LayoutPlacement};

struct ProjectionFixture {
    arena: crate::view::node_arena::NodeArena,
    root: crate::view::node_arena::NodeKey,
}

impl ProjectionFixture {
    fn map(&self) -> std::rc::Rc<CaretNavigationMap> {
        let ptr: *const TextArea = self
            .arena
            .with_element_taken_ref(self.root, |el, _| {
                el.as_any().downcast_ref::<TextArea>().unwrap() as *const TextArea
            })
            .unwrap();
        // SAFETY: arena is read-only for the duration of build().
        let text_area: &TextArea = unsafe { &*ptr };
        CaretNavigationMap::build(text_area, &self.arena)
    }
}

fn build_projection_fixture(
    content: &'static str,
    projection_range: std::ops::Range<usize>,
    inner_text: Option<&'static str>,
    projection_style: ElementStylePropSchema,
    max_width: f32,
) -> ProjectionFixture {
    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.cursor_char = 0;
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        let style = projection_style.clone();
        let inner = inner_text;
        render.range(projection_range.clone(), move |_node| {
            let element = RsxNode::tagged(
                "Element",
                RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
            )
            .with_prop("style", style.clone());
            if let Some(text) = inner {
                element.with_child(
                    RsxNode::tagged(
                        "Text",
                        RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                    )
                    .with_child(RsxNode::text(text)),
                )
            } else {
                element
            }
        });
    }));

    let mut arena = crate::view::test_support::new_test_arena();
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(text_area) as Box<dyn ElementTrait>,
    );
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea root")
            .set_self_node_key(root);
    });
    crate::view::test_support::measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width,
            max_height: 600.0,
            viewport_width: max_width,
            viewport_height: 600.0,
            percent_base_width: Some(max_width),
            percent_base_height: Some(600.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: max_width,
            available_height: 600.0,
            viewport_width: max_width,
            viewport_height: 600.0,
            percent_base_width: Some(max_width),
            percent_base_height: Some(600.0),
        },
    );
    ProjectionFixture { arena, root }
}

fn fixed_box_style() -> ElementStylePropSchema {
    ElementStylePropSchema {
        width: Some(Length::px(60.0)),
        height: Some(Length::px(28.0)),
        ..Default::default()
    }
}

mod pointer_and_empty_line_tests;
mod projection_navigation_tests;
mod wrapped_navigation_tests;
