use super::*;

/// Caret-in-projection + preedit active triggers the
/// `provide_context_node` wrap. With the unwrap fix the projection
/// segment commits as expected: 3 children (Run "ab" / projection
/// Element / Run "cd") and the projection holds an `<Element>` node,
/// not a missing slot.
#[test]
fn projection_commits_when_caret_inside_with_preedit() {
    let (arena, root) = fixture_with_caret_in_projection("\u{4E2D}", Some((1, 1)));

    let children = arena.children_of(root);
    assert_eq!(
        children.len(),
        3,
        "expected 3 children (Run / projection / Run); got {}",
        children.len(),
    );

    let projection_key = children[1];
    let is_segment = arena
        .with_element_taken_ref(projection_key, |el, _| {
            el.as_any().is::<TextAreaProjectionSegment>()
        })
        .unwrap_or(false);
    assert!(
        is_segment,
        "projection slot should hold a TextAreaProjectionSegment wrapper",
    );
    assert!(
        !arena.children_of(projection_key).is_empty(),
        "projection segment should have its descriptor children committed",
    );

    let text_key = first_text_descendant(&arena, projection_key);
    let text_content = arena
        .with_element_taken_ref(text_key, |el, _| {
            el.as_any()
                .downcast_ref::<Text>()
                .expect("projection Text")
                .content()
                .to_string()
        })
        .expect("text exists");
    assert_eq!(text_content, "X\u{4E2D}YZ");
}

/// Sanity baseline: same fixture without preedit (no Provider wrap).
/// Ensures the test isn't passing for an unrelated reason — both
/// shapes should produce the same 3-child arena layout.
#[test]
fn projection_commits_when_caret_inside_without_preedit() {
    let (arena, root) = fixture_with_caret_in_projection("", None);
    assert_eq!(arena.children_of(root).len(), 3);
}

#[test]
fn projection_reconcile_updates_text_with_preedit_context() {
    let (mut arena, root) = fixture_with_caret_in_projection("", None);
    let projection_key = arena.children_of(root)[1];
    let text_key_before = first_text_descendant(&arena, projection_key);

    arena.with_element_taken(root, |el, _| {
        let ta = el
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea root");
        ta.ime_preedit = "\u{4E2D}".to_string();
        ta.ime_preedit_cursor = Some((3, 3));
        ta.children_dirty = true;
        ta.dirty_flags = ta.dirty_flags.union(DirtyFlags::ALL);
    });
    relayout(&mut arena, root);

    let projection_key_after = arena.children_of(root)[1];
    let text_key_after = first_text_descendant(&arena, projection_key_after);
    assert_eq!(
        text_key_after, text_key_before,
        "projection Text should be reconciled in place",
    );
    let text_content = arena
        .with_element_taken_ref(text_key_after, |el, _| {
            el.as_any()
                .downcast_ref::<Text>()
                .expect("projection Text")
                .content()
                .to_string()
        })
        .expect("text exists");
    assert_eq!(text_content, "X\u{4E2D}YZ");
}

/// Caret inside a projection segment with preedit active should not
/// route preedit text onto adjacent Runs. The projection owns text
/// rendering via `TextAreaImeContext`.
#[test]
fn projection_preedit_does_not_route_to_adjacent_run_when_caret_in_projection() {
    let (arena, root) = fixture_with_caret_in_projection("\u{4E2D}\u{6587}", Some((2, 2)));

    let children = arena.children_of(root);
    assert_eq!(children.len(), 3, "expected Run / projection / Run");

    let preceding_pe = run_inline_preedit(&arena, children[0]);
    let following_pe = run_inline_preedit(&arena, children[2]);

    assert!(
        preceding_pe.is_none(),
        "Run before projection should not host the preedit; got {preceding_pe:?}",
    );
    assert!(
        following_pe.is_none(),
        "Run after projection should not host the preedit; got {following_pe:?}",
    );
}

#[test]
fn preedit_inserts_transient_run_on_middle_empty_paragraph() {
    let (arena, root) = plain_textarea_with_preedit("a\n\nb", 2, "\u{4E2D}");

    let children = arena.children_of(root);
    assert_eq!(
        children.len(),
        6,
        "expected Run / LineBreak / preedit Run / empty Run / LineBreak / Run"
    );
    assert_preedit_run(&arena, children[2], "\u{4E2D}", 2..2);
    assert_run_text_range(&arena, children[3], "", 2..2);
    assert!(run_inline_preedit(&arena, children[0]).is_none());
    assert!(run_inline_preedit(&arena, children[5]).is_none());
}

#[test]
fn preedit_inserts_transient_run_on_trailing_empty_paragraph() {
    let (arena, root) = plain_textarea_with_preedit("a\n", 2, "\u{4E2D}");

    let children = arena.children_of(root);
    assert_eq!(
        children.len(),
        4,
        "expected Run / LineBreak / preedit Run / trailing empty Run"
    );
    assert_preedit_run(&arena, children[2], "\u{4E2D}", 2..2);
    assert_run_text_range(&arena, children[3], "", 2..2);
    assert!(run_inline_preedit(&arena, children[0]).is_none());
}

#[test]
fn preedit_inserts_transient_run_in_empty_textarea() {
    let (arena, root) = plain_textarea_with_preedit("", 0, "\u{4E2D}");

    let children = arena.children_of(root);
    assert_eq!(
        children.len(),
        1,
        "empty TextArea should create a preedit Run"
    );
    assert_preedit_run(&arena, children[0], "\u{4E2D}", 0..0);
}
