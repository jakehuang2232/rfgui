use super::*;

/// A `style` update that changes a text-cascading decl (here
/// `font_size`) on an Element with a Text child must fall back to
/// the full-rebuild path — the incremental setter doesn't recascade
/// descendants, so letting it commit would diverge from cold-path
/// behaviour. We assert the boundary by checking the Text child's
/// resolved font_size after the re-render: only the full rebuild
/// path walks the convert pipeline, which re-resolves Text fonts
/// against the new inherited cascade.
/// 軌 A #7: a text-cascading style change on an Element ancestor
/// now commits incrementally — the apply side calls
/// `recascade_text_subtree`, which walks Text/TextArea descendants
/// and re-applies ancestor-derived props via `apply_inherited`
/// (explicit-flag gated). Parent NodeKey survives and the Text
/// child's `font_size` matches the cold-path cascade.
#[test]
fn incremental_commit_applies_text_cascading_style_change_recascades_descendants() {
    use crate::view::Text as HostText;
    use crate::view::base_component::Text as TextHost;

    let parent_20 = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 20.0f32,
        }}>
            <HostText>"hi"</HostText>
        </HostElement>
    };
    let parent_30 = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 30.0f32,
        }}>
            <HostText>"hi"</HostText>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&parent_20).expect("cold render");
    let original_parent_key = viewport.scene.ui_root_keys[0];
    let original_text_key = viewport.scene.node_arena.children_of(original_parent_key)[0];

    viewport
        .render_rsx(&parent_30)
        .expect("cascading style change must commit incrementally");

    // NodeKeys stable — no fallback full rebuild fired.
    assert_eq!(viewport.scene.ui_root_keys, vec![original_parent_key]);
    let arena = &viewport.scene.node_arena;
    let children = arena.children_of(original_parent_key);
    assert_eq!(children, vec![original_text_key]);
    let text = arena
        .get(original_text_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<TextHost>()
        .unwrap()
        .font_size();
    assert!(
        (text - 30.0).abs() < 1e-4,
        "recascade_text_subtree must flow new parent font_size into Text; got {}",
        text
    );
}

/// 軌 A #7: explicit-prop tracking preserves author overrides.
/// A Text child whose style sets `font_size: 14` must NOT be clobbered
/// when the ancestor's cascading font_size changes.
#[test]
fn incremental_commit_recascade_preserves_explicit_text_font_size() {
    use crate::view::Text as HostText;
    use crate::view::base_component::Text as TextHost;

    let parent_20_explicit_14 = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 20.0f32,
        }}>
            <HostText style={{ font_size: 14.0f32 }}>"hi"</HostText>
        </HostElement>
    };
    let parent_30_explicit_14 = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 30.0f32,
        }}>
            <HostText style={{ font_size: 14.0f32 }}>"hi"</HostText>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&parent_20_explicit_14)
        .expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let text_key = viewport.scene.node_arena.children_of(parent_key)[0];

    viewport
        .render_rsx(&parent_30_explicit_14)
        .expect("cascade change must commit incrementally");

    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    let arena = &viewport.scene.node_arena;
    let text = arena
        .get(text_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<TextHost>()
        .unwrap()
        .font_size();
    assert!(
        (text - 14.0).abs() < 1e-4,
        "style font_size 14 must survive ancestor cascade change; got {}",
        text
    );
}

#[test]
fn incremental_commit_vertical_align_keeps_fragmentable_badge_text_aligned() {
    use crate::view::base_component::Text as TextHost;

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&inline_badge_vertical_align_tree(VerticalAlign::Baseline))
        .expect("cold render");
    run_layout_for_test(&mut viewport, 960.0, 240.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let children = viewport.scene.node_arena.children_of(root_key);
    let lead_key = children[0];
    let badge_key = children[1];
    let badge_text_key = viewport.scene.node_arena.children_of(badge_key)[0];

    viewport
        .render_rsx(&inline_badge_vertical_align_tree(VerticalAlign::Middle))
        .expect("vertical-align change should render");
    run_layout_for_test(&mut viewport, 960.0, 240.0);

    assert_eq!(viewport.scene.ui_root_keys, vec![root_key]);
    assert_eq!(
        viewport.scene.node_arena.children_of(root_key)[1],
        badge_key
    );

    let lead_y = viewport
        .scene
        .node_arena
        .get(lead_key)
        .expect("lead text")
        .element
        .as_any()
        .downcast_ref::<TextHost>()
        .expect("lead text")
        .inline_fragment_positions()[0]
        .1
        .y;
    let badge_y = viewport
        .scene
        .node_arena
        .get(badge_text_key)
        .expect("badge text")
        .element
        .as_any()
        .downcast_ref::<TextHost>()
        .expect("badge text")
        .inline_fragment_positions()[0]
        .1
        .y;
    assert!(
        (lead_y - badge_y).abs() < 0.5,
        "fragmentable badge text must track sibling inline text after incremental vertical-align update: lead_y={lead_y}, badge_y={badge_y}"
    );
}

#[test]
fn incremental_commit_recascade_updates_text_area_inherited_vertical_align() {
    use crate::view::TextArea as HostTextArea;
    use crate::view::base_component::TextArea as TextAreaHost;

    fn tree(vertical_align: VerticalAlign) -> RsxNode {
        rsx! {
            <HostElement style={{
                width: Length::px(240.0),
                height: Length::px(120.0),
                vertical_align: vertical_align,
            }}>
                <HostTextArea content={"abc".to_string()} />
            </HostElement>
        }
    }

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&tree(VerticalAlign::Middle))
        .expect("cold render");
    let root_key = viewport.scene.ui_root_keys[0];
    let text_area_key = viewport.scene.node_arena.children_of(root_key)[0];

    viewport
        .render_rsx(&tree(VerticalAlign::Bottom))
        .expect("parent vertical-align update should commit incrementally");

    assert_eq!(viewport.scene.ui_root_keys, vec![root_key]);
    let text_area_node = viewport
        .scene
        .node_arena
        .get(text_area_key)
        .expect("TextArea node");
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<TextAreaHost>()
        .expect("TextArea host");
    assert_eq!(
        text_area.vertical_align,
        VerticalAlign::Bottom,
        "TextArea without its own style must follow parent inherited vertical_align updates",
    );
}

#[test]
fn incremental_commit_recascade_updates_text_area_inherited_line_height() {
    use crate::view::TextArea as HostTextArea;
    use crate::view::base_component::TextArea as TextAreaHost;
    use crate::view::base_component::text_area::TextAreaTextRun;

    fn tree(line_height: f32) -> RsxNode {
        rsx! {
            <HostElement style={{
                width: Length::px(240.0),
                height: Length::px(120.0),
                line_height: line_height,
            }}>
                <HostTextArea content={"abc".to_string()} />
            </HostElement>
        }
    }

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&tree(1.1)).expect("cold render");
    run_layout_for_test(&mut viewport, 240.0, 120.0);
    let root_key = viewport.scene.ui_root_keys[0];
    let text_area_key = viewport.scene.node_arena.children_of(root_key)[0];

    viewport
        .render_rsx(&tree(1.8))
        .expect("parent line-height update should commit incrementally");
    run_layout_for_test(&mut viewport, 240.0, 120.0);

    assert_eq!(viewport.scene.ui_root_keys, vec![root_key]);
    let text_area_node = viewport
        .scene
        .node_arena
        .get(text_area_key)
        .expect("TextArea node");
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<TextAreaHost>()
        .expect("TextArea host");
    assert!(
        (text_area.line_height - 1.8).abs() < 1e-4,
        "TextArea without its own style must follow parent inherited line_height updates, got {}",
        text_area.line_height,
    );
    let run_key = text_area.children[0];
    let run_node = viewport
        .scene
        .node_arena
        .get(run_key)
        .expect("TextArea run");
    let run = run_node
        .element
        .as_any()
        .downcast_ref::<TextAreaTextRun>()
        .expect("TextAreaTextRun");
    assert!(
        (run.line_height - 1.8).abs() < 1e-4,
        "TextArea run children must be rebuilt with updated inherited line_height, got {}",
        run.line_height,
    );
}

/// Conversely, a cascading style change on a *leaf* Element (no
/// descendants) has no one to recascade into, so it may commit
/// incrementally — NodeKey survives.
#[test]
fn incremental_commit_applies_text_cascading_style_change_on_leaf() {
    let first = rsx! {
        <HostElement style={{
            width: Length::px(120.0),
            height: Length::px(40.0),
            font_size: 20.0f32,
        }} />
    };
    let second = rsx! {
        <HostElement style={{
            width: Length::px(120.0),
            height: Length::px(40.0),
            font_size: 30.0f32,
        }} />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&second)
        .expect("cascading style change on leaf must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "no descendants → no recascade risk → NodeKey preserved",
    );
}

// ---------------------------------------------------------------------------
// M6: cascade reconstruction for InsertChild
// ---------------------------------------------------------------------------

/// M6 cascade: an incremental InsertChild under a parent that
/// authored `style={{ font_size: 22 }}` must build the new Text child
/// with `font_size == 22`, matching what the cold-path converter
/// would do via `StyleCascadeContext::merge_style`. M5.0 previously
/// shipped with the viewport root style as the approximation, which
/// would have resolved to the default 16.0.
#[test]
fn incremental_commit_insert_child_inherits_parent_font_size_from_cascade() {
    use crate::view::Text as HostText;
    use crate::view::base_component::Text as TextHost;

    // Parent Element authors a font-cascading style. Children inherit
    // font_size 22 through the cascade.
    let parent_with_no_text = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 22.0f32,
        }} />
    };
    let parent_with_text = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 22.0f32,
        }}>
            <HostText>"hi"</HostText>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&parent_with_no_text)
        .expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let parent_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&parent_with_text)
        .expect("InsertChild with cascade must commit incrementally");

    // Parent identity survives and the Text child is parented to it.
    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    let arena = &viewport.scene.node_arena;
    let children = arena.children_of(parent_key);
    assert_eq!(children.len(), 1, "Text child should be inserted");
    let text_node = arena.get(children[0]).expect("text child node");
    let text = text_node
        .element
        .as_any()
        .downcast_ref::<TextHost>()
        .expect("Text host");
    assert!(
        (text.font_size() - 22.0).abs() < 1e-4,
        "incremental InsertChild must inherit parent font_size 22.0 via cascade, got {}",
        text.font_size(),
    );
}

// ---------------------------------------------------------------------------
// 軌 1 #2 / #3 / #4: context-free setter surface, slot hot-swap, source
// ---------------------------------------------------------------------------

/// An `em` font size in an updated Text style resolves against the
/// parent's font size on the arena, as the first build does.
#[test]
fn incremental_commit_resolves_em_font_size_via_inherited_cascade() {
    use crate::style::FontSize;
    use crate::view::Text as HostText;
    use crate::view::base_component::Text as TextHost;

    let tree = |font_size: FontSize| {
        rsx! {
            <HostElement style={{
                width: Length::px(200.0),
                height: Length::px(80.0),
                font_size: 20.0f32,
            }}>
                <HostText style={{ font_size: font_size }}>"hi"</HostText>
            </HostElement>
        }
    };
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport
        .render_rsx(&tree(FontSize::px(14.0)))
        .expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let text_key = viewport.scene.node_arena.children_of(parent_key)[0];

    viewport
        .render_rsx(&tree(FontSize::em(2.0)))
        .expect("font-size update must commit incrementally");

    assert_eq!(
        viewport.scene.node_arena.children_of(parent_key),
        vec![text_key]
    );
    let text =
        crate::view::test_support::get_element::<TextHost>(&viewport.scene.node_arena, text_key);
    assert!(
        (text.font_size() - 40.0).abs() < 1e-4,
        "Em(2.0) × parent 20px = 40px; got {}",
        text.font_size(),
    );
}

/// A re-render rebuilds every `style` object, but an Element child whose
/// style value is unchanged receives no patch. Its inherited base layers
/// (cursor, text wrap, line height) must still follow the ancestor through
/// the recascade walk rather than through a redundant style re-application.
#[test]
fn incremental_commit_recascades_element_child_with_unchanged_style() {
    use crate::style::TextWrap;
    use crate::view::base_component::Element as ElementHost;

    fn tree(cursor: Cursor, text_wrap: TextWrap, line_height: f32) -> RsxNode {
        rsx! {
            <HostElement style={{
                width: Length::px(240.0),
                height: Length::px(120.0),
                cursor: cursor,
                text_wrap: text_wrap,
                line_height: line_height,
            }}>
                <HostElement style={{ width: Length::px(100.0), height: Length::px(20.0) }} />
            </HostElement>
        }
    }

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport
        .render_rsx(&tree(Cursor::Default, TextWrap::Wrap, 1.1))
        .expect("cold render");
    let root_key = viewport.scene.ui_root_keys[0];
    let child_key = viewport.scene.node_arena.children_of(root_key)[0];

    viewport
        .render_rsx(&tree(Cursor::Pointer, TextWrap::NoWrap, 1.8))
        .expect("parent cascade update should commit incrementally");

    assert_eq!(viewport.scene.ui_root_keys, vec![root_key]);
    assert_eq!(
        viewport.scene.node_arena.children_of(root_key),
        vec![child_key]
    );
    let node = viewport.scene.node_arena.get(child_key).expect("child");
    let style = node
        .element
        .as_any()
        .downcast_ref::<ElementHost>()
        .expect("Element host")
        .parsed_style();
    assert_eq!(
        style.get(PropertyId::Cursor),
        Some(&ParsedValue::Cursor(Cursor::Pointer))
    );
    assert_eq!(
        style.get(PropertyId::TextWrap),
        Some(&ParsedValue::TextWrap(TextWrap::NoWrap))
    );
    assert_eq!(
        style.get(PropertyId::LineHeight),
        Some(&ParsedValue::LineHeight(crate::style::LineHeight::new(1.8)))
    );
}
