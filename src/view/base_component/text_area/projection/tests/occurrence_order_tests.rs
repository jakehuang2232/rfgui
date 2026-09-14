use super::*;

fn install(arena: &mut NodeArena, root: NodeKey, keys: Vec<Option<u64>>) {
    arena.with_element_taken(root, |el, _| {
        let ta = el.as_any_mut().downcast_mut::<TextArea>().unwrap();
        ta.content = "abcdefgh".into();
        ta.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            for (index, key) in keys.iter().copied().enumerate() {
                render.range(index * 2..index * 2 + 1, move |_| {
                    let node = RsxNode::tagged(
                        "Element",
                        RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                    )
                    .with_prop(
                        "style",
                        ElementStylePropSchema {
                            width: Some(Length::px(20.0)),
                            height: Some(Length::px(15.0)),
                            ..Default::default()
                        },
                    );
                    match key {
                        Some(key) => node.with_key(RsxKey::Local(key)),
                        None => node,
                    }
                });
            }
        }));
        ta.mark_content_dirty();
    });
    relayout(arena, root);
}

fn projections(arena: &NodeArena, root: NodeKey) -> Vec<NodeKey> {
    arena
        .children_of(root)
        .into_iter()
        .filter(|key| {
            arena
                .get(*key)
                .unwrap()
                .element
                .as_any()
                .is::<TextAreaProjectionSegment>()
        })
        .collect()
}

#[test]
fn duplicate_unkeyed_projections_keep_occurrence_order_across_rebuilds() {
    let (mut arena, root) = fixture_with_keyed_projection("abXYZcd");
    install(&mut arena, root, vec![None; 3]);
    let before = projections(&arena, root);
    assert_eq!(before.len(), 3);
    let revision = arena.stable_id_index_revision();
    for _ in 0..6 {
        install(&mut arena, root, vec![None; 3]);
        assert_eq!(projections(&arena, root), before);
        assert_eq!(arena.stable_id_index_revision(), revision);
    }
    let node = arena.get(root).unwrap();
    let ta = node.element.as_any().downcast_ref::<TextArea>().unwrap();
    assert_eq!(
        ta.child_char_ranges,
        vec![0..1, 1..2, 2..3, 3..4, 4..5, 5..8]
    );
}

#[test]
fn duplicate_unkeyed_projection_insert_delete_matches_surviving_occurrences() {
    let (mut arena, root) = fixture_with_keyed_projection("abXYZcd");
    install(&mut arena, root, vec![None; 2]);
    let before = projections(&arena, root);
    install(&mut arena, root, vec![None; 3]);
    let expanded = projections(&arena, root);
    assert_eq!(&expanded[..2], before.as_slice());
    install(&mut arena, root, vec![None]);
    assert_eq!(projections(&arena, root), vec![before[0]]);
    assert!(!arena.contains_key(expanded[1]));
    assert!(!arena.contains_key(expanded[2]));
}

#[test]
fn explicit_projection_keys_follow_reordering_instead_of_occurrence_position() {
    let (mut arena, root) = fixture_with_keyed_projection("abXYZcd");
    install(&mut arena, root, vec![Some(1), Some(2), Some(3)]);
    let before = projections(&arena, root);
    install(&mut arena, root, vec![Some(3), Some(1), Some(2)]);
    assert_eq!(
        projections(&arena, root),
        vec![before[2], before[0], before[1]]
    );
}
