use super::super::run::InlinePreedit;
use super::*;
use crate::view::base_component::{ElementTrait, Size};
use crate::view::renderer_adapter::ElementDescriptor;
use crate::view::test_support::{commit_descriptor, new_test_arena};

#[test]
fn unified_root_cache_keeps_the_package_out_of_line() {
    assert!(
        std::mem::size_of::<TextAreaUnifiedIfcRootCache>()
            < std::mem::size_of::<TextAreaUnifiedIfcRootPackage>() / 4,
        "an empty TextArea cache must not reserve inline space for a full package"
    );
}

fn text_area_with_run(
    text: &str,
    width: f32,
) -> (
    NodeArena,
    crate::view::node_arena::NodeKey,
    crate::view::node_arena::NodeKey,
) {
    let mut arena = new_test_arena();
    let mut text_area = TextArea::new();
    text_area.content = text.to_string();
    text_area.auto_wrap = true;
    text_area.viewport_size = Size {
        width,
        height: 120.0,
    };
    text_area.layout_state.layout_size = Size {
        width,
        height: 24.0,
    };
    let root = commit_descriptor(
        &mut arena,
        None,
        ElementDescriptor {
            element: Box::new(text_area) as Box<dyn ElementTrait>,
            children: vec![ElementDescriptor::leaf(Box::new(TextAreaTextRun::new(
                text.to_string(),
                0..text.chars().count(),
            ))
                as Box<dyn ElementTrait>)],
            side_slots: Vec::new(),
        },
    );
    arena.push_root(root);
    let run = arena.children_of(root)[0];
    (arena, root, run)
}

fn touch_unified_package(arena: &NodeArena, root: crate::view::node_arena::NodeKey) -> usize {
    arena
        .with_element_taken_ref(root, |el, arena| {
            let text_area = el
                .as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root");
            let package = text_area
                .cached_unified_inline_ifc_root_package(arena)
                .expect("unified package");
            assert_eq!(package.text_run_count(), 1);
            drop(package);
            text_area.unified_inline_ifc_root_cache_build_count()
        })
        .expect("TextArea root")
}

#[test]
fn text_area_unified_ifc_root_cache_reuses_package_for_repeated_queries() {
    let (arena, root, _) = text_area_with_run("hello", 120.0);

    let build_count = arena
        .with_element_taken_ref(root, |el, arena| {
            let text_area = el
                .as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root");
            let first = text_area
                .cached_unified_inline_ifc_root_package(arena)
                .expect("first package");
            assert_eq!(first.text_run_count(), 1);
            drop(first);

            let second = text_area
                .cached_unified_inline_ifc_root_package(arena)
                .expect("second package");
            assert_eq!(second.text_run_count(), 1);
            drop(second);

            text_area.unified_inline_ifc_root_cache_build_count()
        })
        .expect("TextArea root");

    assert_eq!(build_count, 1, "same key should reuse the cached package");
}

#[test]
fn text_area_unified_ifc_root_cache_invalidates_on_content_style_preedit_and_width() {
    let (arena, root, run) = text_area_with_run("hello", 120.0);

    assert_eq!(touch_unified_package(&arena, root), 1);
    assert_eq!(touch_unified_package(&arena, root), 1);

    arena
        .with_element_taken_ref(run, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextAreaTextRun>()
                .expect("TextAreaTextRun")
                .set_text("hello!".to_string(), 0..6);
        })
        .expect("TextAreaTextRun");
    // Every production run-text mutation flows through a TextArea
    // choke point (edits via mark_content_dirty, projection in-place
    // updates) that bumps the source revision; mirror that here.
    arena
        .with_element_taken_ref(root, |el, _| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root")
                .bump_unified_ifc_source_revision();
        })
        .expect("TextArea root");
    assert_eq!(touch_unified_package(&arena, root), 2);

    arena
        .with_element_taken_ref(root, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root")
                .font_size = 18.0;
        })
        .expect("TextArea root");
    assert_eq!(touch_unified_package(&arena, root), 3);

    arena
        .with_element_taken_ref(root, |el, _| {
            let text_area = el
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root");
            text_area.ime_preedit = "中".to_string();
            text_area.ime_preedit_cursor = Some((0, 1));
        })
        .expect("TextArea root");
    assert_eq!(touch_unified_package(&arena, root), 4);

    arena
        .with_element_taken_ref(root, |el, _| {
            let text_area = el
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root");
            text_area.viewport_size.width = 160.0;
            text_area.layout_state.layout_size.width = 160.0;
        })
        .expect("TextArea root");
    assert_eq!(touch_unified_package(&arena, root), 5);
}

#[test]
fn text_area_unified_ifc_root_package_uses_effective_text_and_preedit_range() {
    let (arena, root, run) = text_area_with_run("hello", 120.0);
    arena
        .with_element_taken_ref(run, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextAreaTextRun>()
                .expect("TextAreaTextRun")
                .set_inline_preedit(Some(InlinePreedit {
                    insert_at_local: 2,
                    preedit_text: "中".to_string(),
                    preedit_cursor: Some((0, "中".len())),
                }));
        })
        .expect("TextAreaTextRun");

    arena
        .with_element_taken_ref(root, |el, arena| {
            let text_area = el
                .as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root");
            let package = text_area
                .unified_inline_ifc_root_package(arena)
                .expect("unified package");
            assert_eq!(package.ifc.backing_text(), "he中llo");
            let segment = package
                .source_segments
                .iter()
                .find(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::TextRun)
                .expect("text run segment");
            assert_eq!(segment.preedit_backing_byte_range, Some(2.."he中".len()));
            assert!(
                !package.preedit_underline_rects().is_empty(),
                "root package should expose underline rects for the spliced preedit"
            );
        })
        .expect("TextArea root");
}
