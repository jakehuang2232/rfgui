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












fn projection_caret_probe_at(cursor_char: usize) {
    use crate::view::TextArea as HostTextArea;
    use crate::view::base_component::TextArea as TextAreaHost;

    let content = global_state(|| {
        String::from(
            "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line",
        )
    });
    // global_state persists across probe invocations in one process;
    // reset so every cursor position starts from the pristine fixture.
    content.binding().set(String::from(
        "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line",
    ));
    let projection_renderer = crate::ui::on_text_area_render(move |render| {
        let chars: Vec<char> = render.content().chars().collect();
        let mut ranges = Vec::new();
        let mut index = 0_usize;
        while index + 1 < chars.len() {
            if chars[index] == '{' && chars[index + 1] == '{' {
                let start = index;
                let mut cursor = index + 2;
                while cursor + 1 < chars.len() {
                    if chars[cursor] == '}' && chars[cursor + 1] == '}' {
                        ranges.push((start, cursor + 2));
                        index = cursor + 2;
                        break;
                    }
                    cursor += 1;
                }
                if cursor + 1 >= chars.len() {
                    break;
                }
                continue;
            }
            index += 1;
        }
        for (start, end) in ranges {
            let slice: String = chars[start..end].iter().collect();
            render.range(start..end, move |_node| {
                let slice = slice.clone();
                RsxNode::tagged(
                    "Element",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_prop(
                    "style",
                    crate::view::ElementStylePropSchema {
                        font_size: Some(crate::style::FontSize::Px(24.0)),
                        ..Default::default()
                    },
                )
                .with_child(
                    RsxNode::tagged(
                        "Text",
                        RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                    )
                    .with_child(RsxNode::text(slice)),
                )
            });
        }
    });

    let tree = {
        let binding = content.binding();
        let renderer = projection_renderer.clone();
        move || {
            let binding = binding.clone();
            let renderer = renderer.clone();
            rsx! {
                <HostTextArea
                    binding={binding}
                    auto_wrap={true}
                    multiline={true}
                    on_render={renderer}
                />
            }
        }
    };

    let mut viewport = Viewport::new();
    viewport.set_size(360, 200);
    viewport.render_rsx(&tree()).expect("render TextArea");
    run_layout_for_test(&mut viewport, 360.0, 200.0);
    viewport.refresh_frame_box_models();
    let root_key = viewport.scene.ui_root_keys[0];

    viewport.set_focused_node_id(Some(root_key));
    viewport
        .scene
        .node_arena
        .with_element_taken(root_key, |el, _| {
            let text_area = el
                .as_any_mut()
                .downcast_mut::<TextAreaHost>()
                .expect("TextArea root");
            text_area.cursor_char = cursor_char;
        });

    let caret_before = viewport
        .scene
        .node_arena
        .with_element_taken_ref(root_key, |el, arena| {
            el.as_any()
                .downcast_ref::<TextAreaHost>()
                .unwrap()
                .caret_screen_position(arena)
        })
        .expect("root")
        .expect("caret before typing");

    // Sequence-type three characters, one full frame each (rsx rebuild
    // with the binding round-trip + projection re-render, then layout).
    let mut previous = caret_before;
    for step in 0..3 {
        assert!(viewport.dispatch_text_input_event("X".to_string()));
        viewport
            .render_rsx(&tree())
            .expect("re-render after typing");
        run_layout_for_test(&mut viewport, 360.0, 200.0);
        viewport.refresh_frame_box_models();

        let caret_after = viewport
            .scene
            .node_arena
            .with_element_taken_ref(root_key, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextAreaHost>()
                    .unwrap()
                    .caret_screen_position(arena)
            })
            .expect("root")
            .expect("caret after typing");

        // Oracle: a from-scratch layout of the post-edit content with the
        // same cursor gives the ground-truth caret. Incremental frames
        // must land on the same position (wrap reflow included).
        let expected = {
            let post_content = content.binding().get();
            let cursor_now = viewport
                .scene
                .node_arena
                .with_element_taken_ref(root_key, |el, _| {
                    el.as_any()
                        .downcast_ref::<TextAreaHost>()
                        .unwrap()
                        .cursor_char
                })
                .expect("root");
            let fresh_state = global_state(|| post_content.clone());
            fresh_state.binding().set(post_content.clone());
            let fresh_renderer = projection_renderer.clone();
            let fresh_tree = {
                let binding = fresh_state.binding();
                rsx! {
                    <HostTextArea
                        binding={binding}
                        auto_wrap={true}
                        multiline={true}
                        on_render={fresh_renderer}
                    />
                }
            };
            let mut fresh = Viewport::new();
            fresh.set_size(360, 200);
            fresh.render_rsx(&fresh_tree).expect("render oracle");
            run_layout_for_test(&mut fresh, 360.0, 200.0);
            fresh.refresh_frame_box_models();
            let fresh_key = fresh.scene.ui_root_keys[0];
            fresh
                .scene
                .node_arena
                .with_element_taken(fresh_key, |el, _| {
                    el.as_any_mut()
                        .downcast_mut::<TextAreaHost>()
                        .unwrap()
                        .cursor_char = cursor_now;
                });
            fresh
                .scene
                .node_arena
                .with_element_taken_ref(fresh_key, |el, arena| {
                    el.as_any()
                        .downcast_ref::<TextAreaHost>()
                        .unwrap()
                        .caret_screen_position(arena)
                })
                .expect("oracle root")
                .expect("oracle caret")
        };
        let dx = (caret_after.0 - expected.0).abs();
        let dy = (caret_after.1 - expected.1).abs();
        assert!(
            dx < 1.0 && dy < 1.0,
            "cursor {cursor_char} step {step}: incremental caret must match a fresh layout: incremental={caret_after:?} fresh={expected:?}"
        );
        previous = caret_after;
    }

    // IME preedit at the same spot: the caret must track the preedit
    // cursor, not jump to another position. Skip positions at/inside a
    // projection chip — there the preedit is hosted by the projection and
    // anchors to the chip instead of the committed-text line.
    let cursor_now = viewport
        .scene
        .node_arena
        .with_element_taken_ref(root_key, |el, _| {
            el.as_any()
                .downcast_ref::<TextAreaHost>()
                .unwrap()
                .cursor_char
        })
        .expect("root");
    let near_chip = {
        let text = content.binding().get();
        let chars: Vec<char> = text.chars().collect();
        let probe = |index: usize| -> bool {
            index + 1 < chars.len()
                && ((chars[index] == '{' && chars[index + 1] == '{')
                    || (chars[index] == '}' && chars[index + 1] == '}'))
        };
        (cursor_now.saturating_sub(13)..(cursor_now + 2).min(chars.len())).any(probe)
    };
    if near_chip {
        return;
    }
    assert!(viewport.dispatch_ime_preedit_event("zh".to_string(), Some((2, 2))));
    viewport
        .render_rsx(&tree())
        .expect("re-render after preedit");
    run_layout_for_test(&mut viewport, 360.0, 200.0);
    viewport.refresh_frame_box_models();
    let caret_preedit = viewport
        .scene
        .node_arena
        .with_element_taken_ref(root_key, |el, arena| {
            el.as_any()
                .downcast_ref::<TextAreaHost>()
                .unwrap()
                .caret_screen_position(arena)
        })
        .expect("root")
        .expect("caret during preedit");
    let dy = (caret_preedit.1 - previous.1).abs();
    let dx = caret_preedit.0 - previous.0;
    assert!(
        dy < 9.0 && dx > 0.5 && dx < 60.0,
        "cursor {cursor_char}: preedit caret must sit after the preedit text: before={previous:?} preedit={caret_preedit:?}"
    );
}

mod hit_test_tests;
mod rerender_hit_test_tests;
mod projection_text_area_tests;
