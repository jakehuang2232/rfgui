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

/// A Text style update that drops declarations: each dropped field takes
/// the ancestor's value again (font size), or else the default (line height
/// and opacity).
#[test]
fn incremental_commit_text_style_drops_fields_back_to_inherited_or_default() {
    use crate::view::Text as HostText;
    use crate::view::base_component::Text as TextHost;

    let tree = |own: bool| {
        rsx! {
            <HostElement style={{
                width: Length::px(200.0),
                height: Length::px(80.0),
                font_size: 20.0f32,
            }}>
                <HostText style={{
                    font_size: own.then_some(crate::style::FontSize::px(40.0)),
                    line_height: own.then_some(2.0),
                    opacity: own.then_some(crate::style::Opacity::new(0.5)),
                }}>"hi"</HostText>
            </HostElement>
        }
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&tree(true)).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let text_key = viewport.scene.node_arena.children_of(parent_key)[0];
    let fields = |viewport: &Viewport| {
        let text = crate::view::test_support::get_element::<TextHost>(
            &viewport.scene.node_arena,
            text_key,
        );
        (text.font_size(), text.line_height_value(), text.opacity())
    };
    assert_eq!(fields(&viewport), (40.0, 2.0, 0.5));

    viewport
        .render_rsx(&tree(false))
        .expect("a Text style losing fields must commit incrementally");
    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    assert_eq!(fields(&viewport), (20.0, 1.25, 1.0));
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
