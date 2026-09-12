use super::*;

/// After `set_content_from_external`, the projection Element's
/// `NodeKey` should be the same instance as before — proving that
/// `reconcile_existing_subtree` reused the slot rather than the
/// rebuild tearing it down. The Run keys may legitimately change
/// (Run reuse is a queue, but for this single-projection layout
/// the Run count is stable so they should also be reused).
#[test]
fn projection_node_key_preserved_across_outer_edit() {
    let (mut arena, root) = fixture_with_keyed_projection("abXYZcd");
    let kids_before = arena.children_of(root);
    assert_eq!(kids_before.len(), 3, "Run / projection / Run");
    let proj_key_before = kids_before[1];
    let projection_text_before = first_text_descendant(&arena, proj_key_before);

    // Outer edit: append "!". Projection range stays 2..5.
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea root")
            .set_content_from_external("abXYZcd!".to_string());
    });
    relayout(&mut arena, root);

    let kids_after = arena.children_of(root);
    assert_eq!(kids_after.len(), 3);
    assert_eq!(
        kids_after[1], proj_key_before,
        "projection NodeKey should survive outer edit",
    );
    let projection_text_after = first_text_descendant(&arena, kids_after[1]);
    assert_eq!(
        projection_text_after, projection_text_before,
        "projection inner Text NodeKey should also survive",
    );
}

/// Run NodeKeys should also survive a rebuild when the segment
/// shape (Plain/Projection counts) is unchanged. This is the
/// in-place plain-Run reuse path inside the full-rebuild flow.
#[test]
fn run_node_keys_preserved_across_outer_edit() {
    let (mut arena, root) = fixture_with_keyed_projection("abXYZcd");
    let kids_before = arena.children_of(root);
    let run_a_before = kids_before[0];
    let run_b_before = kids_before[2];

    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea root")
            .set_content_from_external("abXYZcde".to_string());
    });
    relayout(&mut arena, root);

    let kids_after = arena.children_of(root);
    assert_eq!(kids_after.len(), 3);
    assert_eq!(kids_after[0], run_a_before, "leading Run reused");
    assert_eq!(kids_after[2], run_b_before, "trailing Run reused");
}

/// Identity-mismatched projection (different `key=`) forces a
/// fresh commit — the old projection NodeKey must NOT survive.
#[test]
fn projection_node_key_changes_when_key_mismatches() {
    // First fixture has key="counter".
    let (mut arena, root) = fixture_with_keyed_projection("abXYZcd");
    let kids_before = arena.children_of(root);
    let proj_key_before = kids_before[1];

    // Swap handler to one with key="other".
    arena.with_element_taken(root, |el, _| {
        let ta = el
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea root");
        ta.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
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
                .with_key(RsxKey::Local(0xC0AC_C0AC_0002))
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
        ta.mark_content_dirty();
    });
    relayout(&mut arena, root);

    let kids_after = arena.children_of(root);
    assert_eq!(kids_after.len(), 3);
    assert_ne!(
        kids_after[1], proj_key_before,
        "key=counter → key=other: projection NodeKey must NOT survive",
    );
}

/// Multi-paragraph plain content (no projections) with a paragraph
/// count change. The full-rebuild path's Run reuse queue should
/// keep the leading paragraphs' Run `NodeKey`s stable across the
/// edit and only mint a fresh Run for the appended paragraph.
/// Pins the M6 promise that the M3 reconcile path covers
/// multi-paragraph plain content efficiently — no separate fast
/// path needed.
#[test]
fn multi_paragraph_plain_reuses_existing_runs() {
    let mut text_area = TextArea::new();
    text_area.content = "line one\nline two".to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.multiline = true;

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

    let kids_before = arena.children_of(root);
    assert_eq!(
        kids_before.len(),
        3,
        "two paragraphs → two Runs plus one LineBreak"
    );
    let run_a_before = kids_before[0];
    let break_before = kids_before[1];
    let run_b_before = kids_before[2];

    // Append a third paragraph.
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea root")
            .set_content_from_external("line one\nline two\nline three".to_string());
    });
    relayout(&mut arena, root);

    let kids_after = arena.children_of(root);
    assert_eq!(kids_after.len(), 5);
    assert_eq!(kids_after[0], run_a_before, "para 0 Run reused");
    assert_eq!(kids_after[1], break_before, "line break reused");
    assert_eq!(kids_after[2], run_b_before, "para 1 Run reused");
}

/// Two keyed projections in reverse order: identity-keyed match
/// must follow `key=` rather than position, so each projection's
/// NodeKey tracks its key across the swap.
#[test]
fn keyed_projection_reorder_preserves_state() {
    // Build a fixture with two keyed projections in order [a, b].
    let mut text_area = TextArea::new();
    text_area.content = "X1Y2Z".to_string(); // ranges 1..2 and 3..4
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    let order: std::rc::Rc<std::cell::Cell<bool>> = std::rc::Rc::new(std::cell::Cell::new(false));
    let order_for_handler = order.clone();
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        let swapped = order_for_handler.get();
        let key_a = RsxKey::Local(0xA);
        let key_b = RsxKey::Local(0xB);
        let key_first = if swapped { key_b } else { key_a };
        let key_second = if swapped { key_a } else { key_b };
        render.range(1..2, move |_text_area_node| {
            RsxNode::tagged(
                "Element",
                RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
            )
            .with_key(key_first)
            .with_prop(
                "style",
                ElementStylePropSchema {
                    width: Some(Length::px(20.0)),
                    height: Some(Length::px(20.0)),
                    ..Default::default()
                },
            )
        });
        render.range(3..4, move |_text_area_node| {
            RsxNode::tagged(
                "Element",
                RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
            )
            .with_key(key_second)
            .with_prop(
                "style",
                ElementStylePropSchema {
                    width: Some(Length::px(20.0)),
                    height: Some(Length::px(20.0)),
                    ..Default::default()
                },
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

    let kids_before = arena.children_of(root);
    // Layout: Run "X" / projA / Run "Y" / projB / Run "Z".
    assert_eq!(kids_before.len(), 5);
    let key_a_before = kids_before[1];
    let key_b_before = kids_before[3];

    // Swap.
    order.set(true);
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea root")
            .mark_content_dirty();
    });
    relayout(&mut arena, root);

    let kids_after = arena.children_of(root);
    assert_eq!(kids_after.len(), 5);
    // Position 1 used to be key=a; now it's key=b. Identity-keyed
    // match relocates the key=b NodeKey from old position 3 to
    // new position 1.
    assert_eq!(
        kids_after[1], key_b_before,
        "key=b projection migrated to position 1",
    );
    assert_eq!(
        kids_after[3], key_a_before,
        "key=a projection migrated to position 3",
    );
}
