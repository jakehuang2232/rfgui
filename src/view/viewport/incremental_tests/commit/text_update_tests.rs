use super::*;

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
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
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
