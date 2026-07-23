use super::*;

/// Style update on an Element host commits through the incremental
/// path and keeps the root NodeKey stable — no full rebuild.
#[test]
fn incremental_commit_applies_style_update_preserves_node_key() {
    let first = rsx! {
        <HostElement style={{ width: Length::px(120.0), height: Length::px(40.0), opacity: 0.9 }} />
    };
    let second = rsx! {
        <HostElement style={{ width: Length::px(120.0), height: Length::px(40.0), opacity: 0.5 }} />
    };

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
        .expect("style-change render must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "style update must preserve NodeKey via the M3 setter path",
    );
    assert!(
        viewport
            .scene
            .node_arena
            .arena_local_dirty(original_key)
            .contains(crate::view::base_component::DirtyPassMask::PAINT),
        "FiberWork::Update must propagate element-owned dirty flags into arena dirty",
    );
}

/// font_size update on a Text leaf commits as `FiberWork::Update`.
/// NodeKey must survive.
///
/// Uses numeric f64 directly so the prop lands as `PropValue::F64`
/// (the M3 Text font_size branch only handles numeric; `FontSize`-
/// typed values that need inherited-context resolution fall back).
#[test]
fn incremental_commit_applies_font_size_update_preserves_node_key() {
    use crate::view::Text as HostText;

    fn tree(size: f64) -> RsxNode {
        rsx! { <HostText font_size={size}>"hi"</HostText> }
    }

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&tree(14.0)).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&tree(20.0))
        .expect("font_size update must commit incrementally");
    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "font_size update must preserve NodeKey via the M3 setter path",
    );
}

/// A `style` prop update whose new `Style` lacks a declaration the old
/// one had must clear that declaration — proving the M4 #1
/// `replace_style` wiring is not using the additive `apply_style`
/// merge. Asserts directly against `Element::parsed_style()`.
#[test]
fn incremental_commit_replace_style_drops_absent_declaration() {
    use crate::style::Color;
    use crate::style::PropertyId;
    use crate::view::base_component::Element as ElementHost;

    let with_bg = rsx! {
        <HostElement style={{
            width: Length::px(120.0),
            height: Length::px(40.0),
            background_color: Color::hex("#FF0000"),
        }} />
    };
    let without_bg = rsx! {
        <HostElement style={{
            width: Length::px(120.0),
            height: Length::px(40.0),
        }} />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&with_bg).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];
    {
        let arena = &viewport.scene.node_arena;
        let node = arena.get(original_key).expect("root node");
        let el = node
            .element
            .as_any()
            .downcast_ref::<ElementHost>()
            .expect("Element host");
        assert!(
            el.parsed_style().get(PropertyId::BackgroundColor).is_some(),
            "background_color declaration must be present after cold render",
        );
    }

    viewport
        .render_rsx(&without_bg)
        .expect("style-drop render must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "replace_style path must preserve NodeKey",
    );
    let arena = &viewport.scene.node_arena;
    let node = arena.get(original_key).expect("root node after re-render");
    let el = node
        .element
        .as_any()
        .downcast_ref::<ElementHost>()
        .expect("Element host");
    assert!(
        el.parsed_style().get(PropertyId::BackgroundColor).is_none(),
        "replace_style must drop the declaration absent from the new Style",
    );
}

/// When the `style` prop itself is removed between renders, reconcile
/// emits a `removed: [\"style\"]` entry. M4 #1 routes that through
/// `Element::replace_style(...)`, clearing all authored declarations
/// while preserving the inherited base needed by the Element's own
/// computed style.
#[test]
fn incremental_commit_removes_style_prop_resets_parsed_style() {
    use crate::view::base_component::Element as ElementHost;

    let with_style = rsx! {
        <HostElement style={{ width: Length::px(120.0), height: Length::px(40.0) }} />
    };
    let without_style = host_el();

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&with_style).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];
    {
        let arena = &viewport.scene.node_arena;
        let node = arena.get(original_key).expect("root node");
        let el = node
            .element
            .as_any()
            .downcast_ref::<ElementHost>()
            .expect("Element host");
        assert!(
            !el.parsed_style().declarations().is_empty(),
            "initial style prop should author at least one declaration",
        );
    }

    viewport
        .render_rsx(&without_style)
        .expect("style-removal render must commit incrementally");

    // NodeKey equality is the identity-preservation contract. If the
    // removed-style patch had been rejected by `is_committable`, the
    // legacy full-rebuild path would have minted a fresh NodeKey.
    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "removed-style prop path must commit through replace_style and keep NodeKey",
    );
    let arena = &viewport.scene.node_arena;
    let node = arena.get(original_key).expect("root node after re-render");
    let el = node
        .element
        .as_any()
        .downcast_ref::<ElementHost>()
        .expect("Element host");
    assert!(
        el.text_cascade_style().declarations().is_empty(),
        "removed-style prop must clear the authored text cascade style",
    );
    assert!(
        !matches!(
            el.parsed_style().get(crate::style::PropertyId::Width),
            Some(crate::style::ParsedValue::Length(length))
                if *length == Length::px(120.0)
        ),
        "removed-style prop must drop the authored width declaration",
    );
}

// ---------------------------------------------------------------------------
// M5 #5/#6: Create via InsertChild translation
// ---------------------------------------------------------------------------

#[test]
fn incremental_commit_reset_element_style_restores_inherited_text_base() {
    use crate::view::Text as HostText;

    let first = rsx! {
        <HostElement style={{
            layout: Layout::Inline,
            width: Length::px(320.0),
            vertical_align: VerticalAlign::Bottom,
        }}>
            <HostElement style={{
                vertical_align: VerticalAlign::Middle,
                padding: Padding::uniform(Length::px(4.0)),
            }}>
                <HostText>"badge"</HostText>
            </HostElement>
        </HostElement>
    };
    let second = rsx! {
        <HostElement style={{
            layout: Layout::Inline,
            width: Length::px(320.0),
            vertical_align: VerticalAlign::Bottom,
        }}>
            <HostElement>
                <HostText>"badge"</HostText>
            </HostElement>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    run_layout_for_test(&mut viewport, 320.0, 120.0);
    let root_key = viewport.scene.ui_root_keys[0];
    let wrapper_key = viewport.scene.node_arena.children_of(root_key)[0];

    viewport
        .render_rsx(&second)
        .expect("style removal should commit incrementally");
    run_layout_for_test(&mut viewport, 320.0, 120.0);

    assert_eq!(viewport.scene.ui_root_keys, vec![root_key]);
    assert_eq!(
        viewport.scene.node_arena.children_of(root_key)[0],
        wrapper_key
    );
}
