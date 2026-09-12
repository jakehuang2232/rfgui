//! Regression tests for `commit_projection_segment`'s Provider unwrap.
//!
//! P5 wraps a projection's `RsxNode` in `<Provider<TextAreaImeContext>>`
//! when the caret falls inside that projection while preedit is active.
//! The descriptor walker rejects `RsxNode::Provider`, so the original
//! P5 commit silently dropped the segment in this case
//! (returning `None` and skipping it). These tests pin the unwrap path
//! that dissolves the Provider into a `CONTEXT_STACK` push for the
//! duration of the descriptor build.
use crate::style::Length;
use crate::ui::{RsxKey, RsxNode, RsxTagDescriptor};
use crate::view::ElementStylePropSchema;
use crate::view::base_component::text_area::inline_ifc::TextAreaUnifiedIfcSourceKind;
use crate::view::base_component::text_area::{TextAreaProjectionSegment, TextAreaTextRun};
use crate::view::base_component::{
    DirtyFlags, ElementTrait, LayoutConstraints, LayoutPlacement, Text, TextArea,
};
use crate::view::node_arena::{NodeArena, NodeKey};

fn fixture_with_caret_in_projection(
    ime_preedit: &str,
    ime_preedit_cursor: Option<(usize, usize)>,
) -> (NodeArena, NodeKey) {
    let mut text_area = TextArea::new();
    text_area.content = "abXYZcd".to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    // Caret 3 falls inside the projection range 2..5.
    text_area.cursor_char = 3;
    text_area.ime_preedit = ime_preedit.to_string();
    text_area.ime_preedit_cursor = ime_preedit_cursor;
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        render.range(2..5, |_text_area_node| {
            let style = ElementStylePropSchema {
                width: Some(Length::px(90.0)),
                height: Some(Length::px(42.0)),
                ..Default::default()
            };
            RsxNode::tagged(
                "Element",
                RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
            )
            .with_prop("style", style)
            .with_child(
                RsxNode::tagged(
                    "Text",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                )
                .with_child(RsxNode::text("XYZ")),
            )
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
            max_width: 300.0,
            max_height: 300.0,
            viewport_width: 300.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 300.0,
            available_height: 300.0,
            viewport_width: 300.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
    );
    (arena, root)
}

fn plain_textarea_with_preedit(
    content: &str,
    cursor_char: usize,
    ime_preedit: &str,
) -> (NodeArena, NodeKey) {
    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.multiline = true;
    text_area.cursor_char = cursor_char;
    text_area.ime_preedit = ime_preedit.to_string();
    text_area.ime_preedit_cursor = Some((ime_preedit.len(), ime_preedit.len()));

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
    relayout(&mut arena, root);
    (arena, root)
}

fn char_range_of(content: &str, needle: &str) -> std::ops::Range<usize> {
    let start_byte = content.find(needle).expect("needle exists");
    let end_byte = start_byte + needle.len();
    content[..start_byte].chars().count()..content[..end_byte].chars().count()
}

fn projection_chip_node(label: &str, width: f32) -> RsxNode {
    RsxNode::tagged(
        "Element",
        RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
    )
    .with_prop(
        "style",
        ElementStylePropSchema {
            width: Some(Length::px(width)),
            height: Some(Length::px(22.0)),
            ..Default::default()
        },
    )
    .with_child(
        RsxNode::tagged(
            "Text",
            RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
        )
        .with_child(RsxNode::text(label)),
    )
}

fn tall_projection_block_node(width: f32, height: f32) -> RsxNode {
    RsxNode::tagged(
        "Element",
        RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
    )
    .with_prop(
        "style",
        ElementStylePropSchema {
            width: Some(Length::px(width)),
            height: Some(Length::px(height)),
            ..Default::default()
        },
    )
}

fn auto_projection_chip_node(label: &str) -> RsxNode {
    RsxNode::tagged(
        "Element",
        RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
    )
    .with_prop(
        "style",
        ElementStylePropSchema {
            padding: Some(crate::style::Padding::uniform(Length::px(0.0)).x(Length::px(20.0))),
            font_size: Some(crate::style::FontSize::Px(24.0)),
            border: Some(crate::style::Border::uniform(
                Length::px(1.0),
                &crate::style::Color::hex("#42566f"),
            )),
            ..Default::default()
        },
    )
    .with_child(
        RsxNode::tagged(
            "Text",
            RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
        )
        .with_child(RsxNode::text(label)),
    )
}

fn assert_run_text_range(
    arena: &NodeArena,
    key: NodeKey,
    text: &str,
    range: std::ops::Range<usize>,
) {
    arena
        .with_element_taken_ref(key, |child, _| {
            let run = child
                .as_any()
                .downcast_ref::<crate::view::base_component::text_area::TextAreaTextRun>()
                .expect("TextAreaTextRun");
            assert_eq!(run.text, text);
            assert_eq!(run.char_range, range);
        })
        .expect("run exists");
}

fn assert_preedit_run(arena: &NodeArena, key: NodeKey, text: &str, range: std::ops::Range<usize>) {
    arena
        .with_element_taken_ref(key, |child, _| {
            let run = child
                .as_any()
                .downcast_ref::<crate::view::base_component::text_area::TextAreaTextRun>()
                .expect("TextAreaTextRun");
            assert_eq!(run.text, text);
            assert_eq!(run.char_range, range);
            assert!(run.is_preedit_run(), "expected transient preedit Run");
            assert!(run.inline_preedit.is_none());
        })
        .expect("run exists");
}

fn run_inline_preedit(
    arena: &NodeArena,
    key: NodeKey,
) -> Option<crate::view::base_component::text_area::run::InlinePreedit> {
    arena
        .with_element_taken_ref(key, |child, _| {
            child
                .as_any()
                .downcast_ref::<crate::view::base_component::text_area::TextAreaTextRun>()
                .and_then(|run| run.inline_preedit.clone())
        })
        .flatten()
}

// -----------------------------------------------------------------
// P6 regression tests — `rebuild_children_full` reconcile preserves
// matched-projection `NodeKey`s across rebuild instead of full
// teardown.
// -----------------------------------------------------------------

/// Standard test layout pass — re-runs measure/place at the same
/// constraints so a content edit that flagged `children_dirty`
/// drives `rebuild_children_if_dirty` again.
fn relayout(arena: &mut NodeArena, root: NodeKey) {
    crate::view::test_support::measure_and_place(
        arena,
        root,
        LayoutConstraints {
            max_width: 300.0,
            max_height: 300.0,
            viewport_width: 300.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 300.0,
            available_height: 300.0,
            viewport_width: 300.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
    );
}

fn fixture_with_keyed_projection(content: &str) -> (NodeArena, NodeKey) {
    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        render.range(2..5, |_text_area_node| {
            let style = ElementStylePropSchema {
                width: Some(Length::px(40.0)),
                height: Some(Length::px(20.0)),
                ..Default::default()
            };
            RsxNode::tagged(
                "Element",
                RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
            )
            .with_key(RsxKey::Local(0xC0AC_C0AC_0001))
            .with_prop("style", style)
            .with_child(
                RsxNode::tagged(
                    "Text",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                )
                .with_child(RsxNode::text("X")),
            )
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
    relayout(&mut arena, root);
    (arena, root)
}

fn first_text_descendant(arena: &NodeArena, root: NodeKey) -> NodeKey {
    let mut stack: Vec<NodeKey> = arena.children_of(root).into_iter().rev().collect();
    while let Some(key) = stack.pop() {
        if arena
            .get(key)
            .is_some_and(|node| node.element.as_any().is::<Text>())
        {
            return key;
        }
        for child in arena.children_of(key).into_iter().rev() {
            stack.push(child);
        }
    }
    panic!("expected Text descendant");
}

mod atomic_placement_tests;
mod preedit_tests;
mod reconciliation_tests;
mod selection_geometry_tests;
mod source_and_wrap_tests;
