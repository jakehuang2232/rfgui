use super::*;

#[test]
fn unchanged_sync_preserves_effect_identity_and_generation() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 1);
    set_opacity(&arena, root, 0.5);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let first = trees.effects[&EffectNodeId(root)];

    trees.sync(&arena, &[root]);

    assert_eq!(trees.states[&root].paint.effect, Some(EffectNodeId(root)));
    assert_eq!(
        trees.effects[&EffectNodeId(root)].generation,
        first.generation
    );
    assert_eq!(trees.changes_for(root), PropertyChangeFlags::NONE);
}

#[test]
fn transform_state_applies_to_self_and_descendants_without_parent_multiplication() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 0x8d00);
    let child = insert_element(&mut arena, 0x8d01);
    append_child(&mut arena, root, child);
    set_transform(&arena, root, translate_x(12.0));
    set_transform(&arena, child, translate_x(7.0));
    let root_source = arena
        .get(root)
        .unwrap()
        .element
        .compositor_local_transform_snapshot()
        .unwrap()
        .to_cols_array()
        .map(f32::to_bits);
    let child_source = arena
        .get(child)
        .unwrap()
        .element
        .compositor_local_transform_snapshot()
        .unwrap()
        .to_cols_array()
        .map(f32::to_bits);

    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);

    let root_id = TransformNodeId(root);
    let child_id = TransformNodeId(child);
    assert_eq!(trees.states[&root].paint.transform, Some(root_id));
    assert_eq!(trees.states[&root].descendants.transform, Some(root_id));
    assert_eq!(trees.states[&child].paint.transform, Some(child_id));
    assert_eq!(trees.states[&child].descendants.transform, Some(child_id));
    assert_eq!(trees.transforms[&root_id].parent, None);
    assert_eq!(trees.transforms[&child_id].parent, Some(root_id));
    assert_eq!(
        matrix_bits(trees.transforms[&root_id].local_matrix),
        root_source
    );
    assert_eq!(
        matrix_bits(trees.transforms[&child_id].local_matrix),
        child_source,
        "authored local matrices must not be parent-multiplied during sync",
    );
}

#[test]
fn transform_generation_is_bitwise_stable_and_matrix_change_is_not_topology() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 0x8d10);
    set_transform(&arena, root, translate_x(4.0));
    let id = TransformNodeId(root);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let first = trees.transforms[&id].generation;

    trees.sync(&arena, &[root]);
    assert_eq!(trees.transforms[&id].generation, first);
    assert_eq!(trees.changes_for(root), PropertyChangeFlags::NONE);

    set_transform(&arena, root, translate_x(5.0));
    trees.sync(&arena, &[root]);
    assert_eq!(trees.transforms[&id].generation, first + 1);
    let changes = trees.changes_for(root);
    assert!(changes.contains(PropertyChangeFlags::TRANSFORM));
    assert!(!changes.contains(PropertyChangeFlags::TOPOLOGY));
    assert!(!changes.contains(PropertyChangeFlags::CLIP));
    assert!(!changes.contains(PropertyChangeFlags::EFFECT));
    assert!(!changes.contains(PropertyChangeFlags::SCROLL));
}

#[test]
fn transform_reparent_preserves_id_and_updates_parent_topology() {
    let mut arena = NodeArena::new();
    let left = insert_element(&mut arena, 0x8d20);
    let right = insert_element(&mut arena, 0x8d21);
    let child = insert_element(&mut arena, 0x8d22);
    set_transform(&arena, left, translate_x(1.0));
    set_transform(&arena, right, translate_x(2.0));
    set_transform(&arena, child, translate_x(3.0));
    append_child(&mut arena, left, child);
    let child_id = TransformNodeId(child);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[left, right]);
    let first_generation = trees.transforms[&child_id].generation;

    arena.set_children(left, Vec::new());
    arena.set_parent(child, Some(right));
    arena.push_child(right, child);
    trees.sync(&arena, &[left, right]);

    assert_eq!(
        trees.transforms[&child_id].parent,
        Some(TransformNodeId(right))
    );
    assert_eq!(trees.transforms[&child_id].generation, first_generation + 1);
    let changes = trees.changes_for(child);
    assert!(changes.contains(PropertyChangeFlags::TRANSFORM));
    assert!(changes.contains(PropertyChangeFlags::TOPOLOGY));
}

#[test]
fn transform_tombstone_is_monotonic_across_remove_reinsert_and_inactive_roots() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 0x8d30);
    set_transform(&arena, root, translate_x(1.0));
    let id = TransformNodeId(root);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let first = trees.transforms[&id].generation;

    trees.sync(&arena, &[]);
    assert!(!trees.transforms.contains_key(&id));
    assert!(trees.transform_generations.contains_key(&id));
    trees.sync(&arena, &[root]);
    assert!(trees.transforms[&id].generation > first);

    set_transform(&arena, root, Transform::default());
    trees.sync(&arena, &[root]);
    let removed_generation = trees.transform_generations[&id];
    assert!(!trees.transforms.contains_key(&id));
    set_transform(&arena, root, translate_x(1.0));
    trees.sync(&arena, &[root]);
    assert!(trees.transforms[&id].generation > removed_generation);

    arena.remove(root);
    trees.sync(&arena, &[root]);
    assert!(!trees.transform_generations.contains_key(&id));
}

#[test]
fn non_finite_transform_remains_a_property_boundary_and_reports_validation() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 0x8d40);
    let mut matrix = Mat4::IDENTITY.to_cols_array();
    matrix[5] = f32::NAN;
    set_transform(
        &arena,
        root,
        Transform::new([TransformEntry::from_matrix(matrix)]),
    );
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);

    assert_eq!(
        trees.states[&root].paint.transform,
        Some(TransformNodeId(root)),
        "invalid numeric payloads must not collapse to neutral",
    );
    assert_eq!(
        trees.validation_errors,
        vec![PropertyTreeValidationError::NonFiniteTransform(root)]
    );
}

#[test]
fn image_and_svg_delegate_their_element_transform_snapshot() {
    let mut arena = NodeArena::new();
    let mut image = Image::new_with_id(
        0x8d50,
        crate::view::ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: Arc::from([255, 255, 255, 255]),
        },
    );
    let mut image_style = Style::new();
    image_style.set_transform(translate_x(8.0));
    image.apply_style(image_style);
    let image_key = arena.insert(Node::new(Box::new(image)));

    let mut svg = Svg::new_with_id(
        0x8d51,
        crate::view::SvgSource::Content("<svg xmlns=\"http://www.w3.org/2000/svg\"/>".into()),
    );
    let mut svg_style = Style::new();
    svg_style.set_transform(translate_x(9.0));
    svg.apply_style(svg_style);
    let svg_key = arena.insert(Node::new(Box::new(svg)));

    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[image_key, svg_key]);

    for key in [image_key, svg_key] {
        assert_eq!(
            trees.states[&key].paint.transform,
            Some(TransformNodeId(key))
        );
        let expected = arena
            .get(key)
            .unwrap()
            .element
            .compositor_local_transform_snapshot()
            .unwrap()
            .to_cols_array()
            .map(f32::to_bits);
        assert_eq!(
            matrix_bits(trees.transforms[&TransformNodeId(key)].local_matrix),
            expected
        );
    }
}

#[test]
fn built_in_non_wrapper_hosts_are_explicitly_transform_neutral() {
    // These hosts do not own an `Element` and their prop/style schemas do
    // not accept CSS transforms. Keeping the inventory here prevents a new
    // transform-capable built-in from silently inheriting the neutral
    // default instead of delegating an authoritative snapshot.
    let hosts: Vec<Box<dyn ElementTrait>> = vec![
        Box::new(Text::new(0.0, 0.0, 10.0, 10.0, "text")),
        Box::new(TextArea::new()),
        Box::new(TextAreaProjectionSegment::new()),
        Box::new(TextAreaTextRun::new("run".to_string(), 0..3)),
        Box::new(TextAreaLineBreak::new(3..4)),
    ];

    for host in hosts {
        assert!(
            host.compositor_local_transform_snapshot().is_none(),
            "{} unexpectedly became transform-capable without an explicit delegate",
            host.element_type_name(),
        );
    }
}

#[test]
fn effect_snapshot_owns_complete_leaf_to_root_chain_bit_exactly() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 10);
    let child = insert_element(&mut arena, 11);
    append_child(&mut arena, root, child);
    set_opacity(&arena, root, 0.5);
    set_opacity(&arena, child, 0.25);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);

    let snapshots = trees
        .effect_snapshot_for(Some(EffectNodeId(child)))
        .expect("complete effect chain must snapshot");
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0].id, EffectNodeId(child));
    assert_eq!(snapshots[0].owner, child);
    assert_eq!(snapshots[0].parent, Some(EffectNodeId(root)));
    assert_eq!(snapshots[0].opacity.to_bits(), 0.25_f32.to_bits());
    assert!(snapshots[0].generation > 0);
    assert_eq!(snapshots[1].id, EffectNodeId(root));
    assert_eq!(snapshots[1].owner, root);
    assert_eq!(snapshots[1].parent, None);
    assert_eq!(snapshots[1].opacity.to_bits(), 0.5_f32.to_bits());
    assert!(snapshots[1].generation > 0);
    assert_eq!(trees.effect_snapshot_for(None), Some(Vec::new()));

    trees.effects.remove(&EffectNodeId(root));
    assert!(
        trees
            .effect_snapshot_for(Some(EffectNodeId(child)))
            .is_none()
    );
}

#[test]
fn opacity_change_marks_effect_without_other_property_changes() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 1);
    set_opacity(&arena, root, 0.5);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let generation = trees.effects[&EffectNodeId(root)].generation;
    trees.sync(&arena, &[root]);

    set_opacity(&arena, root, 0.25);
    trees.sync(&arena, &[root]);

    let changes = trees.changes_for(root);
    assert!(changes.contains(PropertyChangeFlags::EFFECT));
    assert!(!changes.contains(PropertyChangeFlags::TRANSFORM));
    assert!(!changes.contains(PropertyChangeFlags::CLIP));
    assert!(!changes.contains(PropertyChangeFlags::SCROLL));
    assert_eq!(
        trees.effects[&EffectNodeId(root)].generation,
        generation + 1
    );
}

#[test]
fn effect_generation_stays_monotonic_across_inactive_and_readded_state() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 1);
    set_opacity(&arena, root, 0.5);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let first_generation = trees.effects[&EffectNodeId(root)].generation;

    set_opacity(&arena, root, 1.0);
    trees.sync(&arena, &[root]);
    assert!(!trees.effects.contains_key(&EffectNodeId(root)));

    set_opacity(&arena, root, 0.5);
    trees.sync(&arena, &[root]);
    assert!(trees.effects[&EffectNodeId(root)].generation > first_generation);
}

#[test]
fn effect_generation_survives_temporary_removal_from_active_roots() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 1);
    set_opacity(&arena, root, 0.5);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let first_generation = trees.effects[&EffectNodeId(root)].generation;

    trees.sync(&arena, &[]);
    assert!(!trees.effects.contains_key(&EffectNodeId(root)));
    assert!(!trees.states.contains_key(&root));

    trees.sync(&arena, &[root]);
    assert!(trees.effects[&EffectNodeId(root)].generation > first_generation);
}

#[test]
fn text_image_and_svg_effect_snapshots_use_the_trait_contract() {
    let mut arena = NodeArena::new();

    let mut text = Text::new_with_id(1, 0.0, 0.0, 80.0, 20.0, "text");
    text.set_opacity(0.25);
    let text_key = arena.insert(Node::new(Box::new(text)));

    let mut image = Image::new_with_id(
        2,
        ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: Arc::from([255, 255, 255, 255]),
        },
    );
    image.apply_style(opacity_style(0.5));
    let image_key = arena.insert(Node::new(Box::new(image)));

    let mut svg = Svg::new_with_id(
        3,
        SvgSource::Content("<svg xmlns=\"http://www.w3.org/2000/svg\"/>".into()),
    );
    svg.apply_style(opacity_style(0.75));
    let svg_key = arena.insert(Node::new(Box::new(svg)));

    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[text_key, image_key, svg_key]);

    assert_eq!(trees.effects[&EffectNodeId(text_key)].opacity, 0.25);
    assert_eq!(trees.effects[&EffectNodeId(image_key)].opacity, 0.5);
    assert_eq!(trees.effects[&EffectNodeId(svg_key)].opacity, 0.75);
}

#[test]
fn reparent_preserves_effect_id_and_updates_parent_topology() {
    let mut arena = NodeArena::new();
    let left = insert_element(&mut arena, 1);
    let right = insert_element(&mut arena, 2);
    let child = insert_element(&mut arena, 3);
    set_opacity(&arena, left, 0.8);
    set_opacity(&arena, right, 0.6);
    set_opacity(&arena, child, 0.4);
    append_child(&mut arena, left, child);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[left, right]);
    let generation = trees.effects[&EffectNodeId(child)].generation;

    arena.set_children(left, Vec::new());
    arena.set_parent(child, Some(right));
    arena.push_child(right, child);
    trees.sync(&arena, &[left, right]);

    let effect = trees.effects[&EffectNodeId(child)];
    assert_eq!(effect.parent, Some(EffectNodeId(right)));
    assert_eq!(effect.generation, generation + 1);
    assert!(
        trees
            .changes_for(child)
            .contains(PropertyChangeFlags::TOPOLOGY)
    );
}

#[test]
fn sync_prunes_removed_nodes_and_generational_keys_do_not_alias() {
    let mut arena = NodeArena::new();
    let old = insert_element(&mut arena, 1);
    set_opacity(&arena, old, 0.5);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[old]);
    assert!(trees.states.contains_key(&old));

    arena.remove(old);
    // A caller may retain a stale generational root key until its own
    // root list is compacted; it must not keep shadow entries alive.
    trees.sync(&arena, &[old]);
    assert!(!trees.states.contains_key(&old));
    assert!(!trees.effects.contains_key(&EffectNodeId(old)));

    let new = insert_element(&mut arena, 2);
    assert_ne!(old, new);
    trees.sync(&arena, &[new]);
    assert!(!trees.states.contains_key(&old));
    assert!(trees.states.contains_key(&new));
}
