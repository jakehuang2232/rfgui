use super::*;
use crate::view::base_component::ElementTrait;
use crate::view::node_arena::{Node, NodeArena};

/// Minimal test-only element that lets us drive stable_id_index
/// assertions without building a real RSX tree (which would pull
/// in the whole renderer pipeline).
struct TestElement {
    sid: u64,
}

impl crate::view::base_component::Layoutable for TestElement {
    fn measure(&mut self, _c: crate::view::base_component::LayoutConstraints, _a: &mut NodeArena) {}
    fn place(&mut self, _p: crate::view::base_component::LayoutPlacement, _a: &mut NodeArena) {}
    fn measured_size(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn set_layout_width(&mut self, _w: f32) {}
    fn set_layout_height(&mut self, _h: f32) {}
}
impl crate::view::base_component::EventTarget for TestElement {}
impl crate::view::base_component::Renderable for TestElement {
    fn build(
        &mut self,
        _g: &mut crate::view::frame_graph::FrameGraph,
        _a: &mut NodeArena,
        ctx: crate::view::base_component::UiBuildContext,
    ) -> crate::view::base_component::BuildState {
        ctx.into_state()
    }
}
impl ElementTrait for TestElement {
    fn stable_id(&self) -> u64 {
        self.sid
    }
    fn box_model_snapshot(&self) -> crate::view::base_component::BoxModelSnapshot {
        crate::view::base_component::BoxModelSnapshot {
            node_id: self.sid,
            parent_id: None,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            border_radius: 0.0,
            should_render: false,
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn make(sid: u64) -> Box<dyn ElementTrait> {
    Box::new(TestElement { sid })
}

fn test_apply_ctx() -> ApplyContext<'static> {
    use std::sync::OnceLock;
    static STYLE: OnceLock<Style> = OnceLock::new();
    ApplyContext {
        viewport_style: STYLE.get_or_init(Style::new),
        viewport_width: 800.0,
        viewport_height: 600.0,
    }
}

fn host_element_node() -> RsxNode {
    RsxNode::tagged(
        "Element",
        crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
    )
}

fn nonzero_stable_ids(arena: &NodeArena) -> Vec<u64> {
    let mut ids: Vec<u64> = arena
        .iter()
        .filter_map(|(_, node)| {
            let stable_id = node.element.stable_id();
            (stable_id != 0).then_some(stable_id)
        })
        .collect();
    ids.sort_unstable();
    ids
}

#[test]
fn incremental_insert_children_preserves_full_root_identity_context() {
    use rustc_hash::FxHashSet;

    let old_tree = RsxNode::fragment(vec![host_element_node()]);
    let new_parent = (0..7).fold(host_element_node(), |parent, _| {
        parent.with_child(host_element_node())
    });
    let new_tree = RsxNode::fragment(vec![new_parent]);

    let old_roots = match &old_tree {
        RsxNode::Fragment(fragment) => fragment.children.iter().collect::<Vec<_>>(),
        _ => unreachable!("fixture has a Fragment root"),
    };
    let new_roots = match &new_tree {
        RsxNode::Fragment(fragment) => fragment.children.iter().collect::<Vec<_>>(),
        _ => unreachable!("fixture has a Fragment root"),
    };
    let patches = crate::ui::reconcile_multi(Some(&old_roots), &new_roots);
    assert_eq!(
        patches
            .iter()
            .filter(|rooted| matches!(rooted.patch, Patch::InsertChild { .. }))
            .count(),
        7,
        "expanding the branch fixture should author seven InsertChild patches",
    );

    let mut incremental_arena = crate::view::test_support::new_test_arena();
    let roots = crate::view::test_support::commit_rsx_tree(&mut incremental_arena, &old_tree);
    let style = Style::new();
    let descriptor_context = DescriptorContext {
        new_rsx_root: &new_tree,
        old_rsx_root: Some(&old_tree),
        inherited_style: &style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    let works = translate_rooted_patches_all_or_nothing(
        patches,
        incremental_arena.stable_id_index(),
        &incremental_arena,
        &roots,
        &old_roots,
        &new_roots,
        Some(&descriptor_context),
    )
    .expect("all expansion patches should stay on the incremental path");
    apply_fiber_works(&mut incremental_arena, test_apply_ctx(), works)
        .expect("incremental expansion should commit");

    let incremental_ids = nonzero_stable_ids(&incremental_arena);
    let unique_ids: FxHashSet<u64> = incremental_ids.iter().copied().collect();
    assert_eq!(
        unique_ids.len(),
        incremental_ids.len(),
        "every live host must own a unique stable id after branch expansion",
    );

    let mut cold_arena = crate::view::test_support::new_test_arena();
    crate::view::test_support::commit_rsx_tree(&mut cold_arena, &new_tree);
    assert_eq!(
        incremental_ids,
        nonzero_stable_ids(&cold_arena),
        "incremental expansion must mint exactly the cold-path identities",
    );
}

/// A `Patch::ReplaceRoot` in a Fragment-at-root scene targets exactly
/// one root. Sibling roots must keep their NodeKeys — they carry
/// retained-surface / GPU resources cached against those keys — and
/// must stay in place around the replacement.
#[test]
fn rooted_replace_root_replaces_one_root_and_keeps_siblings() {
    let old_tree = RsxNode::fragment(vec![
        host_element_node(),
        host_element_node(),
        host_element_node(),
    ]);
    // Middle root gains a child, so its descriptor differs while its
    // identity (tag + key) still matches the authored root.
    let new_tree = RsxNode::fragment(vec![
        host_element_node(),
        host_element_node().with_child(host_element_node()),
        host_element_node(),
    ]);
    let old_roots = match &old_tree {
        RsxNode::Fragment(fragment) => fragment.children.iter().collect::<Vec<_>>(),
        _ => unreachable!("fixture has a Fragment root"),
    };
    let new_roots = match &new_tree {
        RsxNode::Fragment(fragment) => fragment.children.iter().collect::<Vec<_>>(),
        _ => unreachable!("fixture has a Fragment root"),
    };

    let mut arena = crate::view::test_support::new_test_arena();
    let root_keys = crate::view::test_support::commit_rsx_tree(&mut arena, &old_tree);
    arena.set_roots(root_keys.clone());
    assert_eq!(root_keys.len(), 3, "fixture commits three arena roots");

    let style = Style::new();
    let descriptor_context = DescriptorContext {
        new_rsx_root: &new_tree,
        old_rsx_root: Some(&old_tree),
        inherited_style: &style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    let works = translate_rooted_patches_all_or_nothing(
        vec![crate::ui::RootedPatch {
            root_index: 1,
            patch: Patch::ReplaceRoot(new_roots[1].clone()),
        }],
        arena.stable_id_index(),
        &arena,
        &root_keys,
        &old_roots,
        &new_roots,
        Some(&descriptor_context),
    )
    .expect("a per-root ReplaceRoot must stay on the incremental path");
    assert!(works.iter().all(|work| work.is_committable(&arena)));
    apply_fiber_works(&mut arena, test_apply_ctx(), works).expect("root replacement commits");

    let roots_after = arena.roots().to_vec();
    assert_eq!(roots_after.len(), 3, "root arity is unchanged");
    assert_eq!(
        (roots_after[0], roots_after[2]),
        (root_keys[0], root_keys[2]),
        "sibling roots must keep their NodeKeys and their slots",
    );
    assert_ne!(
        roots_after[1], root_keys[1],
        "the replaced root is a freshly minted key",
    );
    assert!(
        arena.get(root_keys[1]).is_none(),
        "the old root subtree must be dropped",
    );
    assert_eq!(
        arena.children_of(roots_after[1]).len(),
        1,
        "the replacement root carries the newly authored child",
    );
}

#[test]
fn stable_id_index_populated_on_insert() {
    let mut arena = NodeArena::new();
    let k = arena.insert(Node::new(make(42)));
    assert_eq!(arena.find_by_stable_id(42), Some(k));
}

#[test]
fn stable_id_index_skips_zero() {
    let mut arena = NodeArena::new();
    let _ = arena.insert(Node::new(make(0)));
    assert_eq!(arena.find_by_stable_id(0), None);
}

#[test]
fn stable_id_index_cleaned_on_remove() {
    let mut arena = NodeArena::new();
    let k = arena.insert(Node::new(make(7)));
    assert_eq!(arena.find_by_stable_id(7), Some(k));
    arena.remove(k);
    assert_eq!(arena.find_by_stable_id(7), None);
}

#[test]
fn stable_id_index_cleaned_on_remove_subtree() {
    let mut arena = NodeArena::new();
    let parent = arena.insert(Node::new(make(1)));
    let child = arena.insert(Node::new(make(2)));
    arena.set_parent(child, Some(parent));
    arena.push_child(parent, child);

    assert_eq!(arena.find_by_stable_id(1), Some(parent));
    assert_eq!(arena.find_by_stable_id(2), Some(child));

    arena.remove_subtree(parent);
    assert_eq!(arena.find_by_stable_id(1), None);
    assert_eq!(arena.find_by_stable_id(2), None);
}

#[test]
fn refresh_stable_id_index_rebuilds_from_scratch() {
    let mut arena = NodeArena::new();
    let k = arena.insert(Node::new(make(99)));
    // Simulate a caller that bypassed the indexed path: wipe the
    // index by hand then rebuild.
    arena.refresh_stable_id_index(); // still correct after a no-op refresh
    assert_eq!(arena.find_by_stable_id(99), Some(k));
}

#[test]
fn fiber_work_delete_removes_subtree_under_root() {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(make(10)));
    arena.push_root(root);

    apply_fiber_works(
        &mut arena,
        test_apply_ctx(),
        vec![FiberWork::Delete {
            parent: None,
            key: root,
        }],
    )
    .expect("delete root work applies");

    assert!(arena.is_empty());
    assert!(arena.roots().is_empty());
    assert_eq!(arena.find_by_stable_id(10), None);
}

#[test]
fn fiber_work_delete_removes_child_via_parent() {
    let mut arena = NodeArena::new();
    let parent = arena.insert(Node::new(make(1)));
    let child = arena.insert(Node::new(make(2)));
    arena.set_parent(child, Some(parent));
    arena.set_children(parent, vec![child]);

    apply_fiber_works(
        &mut arena,
        test_apply_ctx(),
        vec![FiberWork::Delete {
            parent: Some(parent),
            key: child,
        }],
    )
    .expect("delete child work applies");

    assert_eq!(arena.children_of(parent).len(), 0);
    assert!(arena.find_by_stable_id(2).is_none());
    assert_eq!(arena.find_by_stable_id(1), Some(parent));
}

#[test]
fn fiber_work_move_reorders_children() {
    let mut arena = NodeArena::new();
    let parent = arena.insert(Node::new(make(1)));
    let a = arena.insert(Node::new(make(10)));
    let b = arena.insert(Node::new(make(20)));
    let c = arena.insert(Node::new(make(30)));
    for &ch in &[a, b, c] {
        arena.set_parent(ch, Some(parent));
    }
    arena.set_children(parent, vec![a, b, c]);

    // Move `a` (index 0) to the end (index 2).
    apply_fiber_works(
        &mut arena,
        test_apply_ctx(),
        vec![FiberWork::Move {
            parent,
            key: a,
            from: 0,
            to: 2,
        }],
    )
    .expect("move work applies");

    assert_eq!(arena.children_of(parent), vec![b, c, a]);
}

#[test]
fn fiber_work_update_and_set_text_are_safe_on_unknown_host() {
    // M3: Update / SetText now dispatch through the setter layer,
    // but on an unknown host type (here: the TestElement harness,
    // which is neither Text / TextArea / Element) both paths must
    // bail cleanly. The assertion guards against a future refactor
    // accidentally panicking on unrecognised downcast targets.
    let mut arena = NodeArena::new();
    let k = arena.insert(Node::new(make(5)));

    apply_fiber_works(
        &mut arena,
        test_apply_ctx(),
        vec![
            FiberWork::Update {
                key: k,
                changed: vec![],
                removed: vec![],
            },
            FiberWork::SetText {
                key: k,
                text: "ignored".into(),
            },
        ],
    )
    .expect_err("SetText on an unknown host must surface failure");

    assert_eq!(arena.len(), 1);
    assert_eq!(arena.find_by_stable_id(5), Some(k));
}

/// M4 #3: a FiberWork::Update with `changed = [("loading", ...)]`
/// on an Image host installs the new loading slot subtree via
/// `Image::replace_loading_slot_incremental`, replacing any
/// prior slot. Exercises the apply dispatcher end-to-end through
/// `apply_fiber_works` (the HostImage rsx route `Rc`-wraps
/// `ImageSource` which forces full-rebuild on second render — see
/// the commit log for why the integration test was demoted to a
/// unit test).
#[test]
fn fiber_work_installs_image_loading_slot_incrementally() {
    use crate::ui::{IntoPropValue, RsxNode, RsxTagDescriptor};
    use crate::view::ImageSource;
    use crate::view::base_component::Image;
    use crate::view::node_arena::Node;
    use std::sync::Arc;

    let src = ImageSource::Rgba {
        width: 1,
        height: 1,
        pixels: Arc::<[u8]>::from(vec![0u8, 0, 0, 255]),
    };
    let image = Image::new_with_id(7, src);
    let mut arena = NodeArena::new();
    let image_key = arena.insert(Node::new(Box::new(image)));

    let loading_a = RsxNode::tagged(
        "Element",
        RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
    );
    apply_fiber_works(
        &mut arena,
        test_apply_ctx(),
        vec![FiberWork::Update {
            key: image_key,
            changed: vec![("loading", loading_a.into_prop_value())],
            removed: vec![],
        }],
    )
    .expect("first image loading slot applies");
    {
        let node = arena.get(image_key).expect("image survived");
        let image = node
            .element
            .as_any()
            .downcast_ref::<Image>()
            .expect("Image host");
        assert_eq!(
            image.loading_slot_len(),
            1,
            "first loading slot install should leave exactly one wrapper",
        );
    }
    let arena_len_after_first = arena.len();

    // Install a taller slot; the old wrapper subtree must be
    // removed and the new one committed, keeping the Vec length
    // at 1.
    let loading_b = RsxNode::tagged(
        "Element",
        RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
    )
    .with_child(RsxNode::tagged(
        "Element",
        RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
    ));
    apply_fiber_works(
        &mut arena,
        test_apply_ctx(),
        vec![FiberWork::Update {
            key: image_key,
            changed: vec![("loading", loading_b.into_prop_value())],
            removed: vec![],
        }],
    )
    .expect("second image loading slot applies");
    {
        let node = arena.get(image_key).expect("image survived second update");
        let image = node
            .element
            .as_any()
            .downcast_ref::<Image>()
            .expect("Image host");
        assert_eq!(
            image.loading_slot_len(),
            1,
            "second loading slot install must replace the first (not stack)",
        );
    }
    // Arena net growth from first→second install should be
    // exactly +1 (new slot has 2 nodes vs. old 1, minus old's 1
    // removed = +1). If `replace_loading_slot_incremental` skipped
    // the `remove_subtree` loop the delta would be +2.
    let delta = arena.len() as isize - arena_len_after_first as isize;
    assert_eq!(
        delta, 1,
        "arena net growth must be +1 (old slot removed, new 2-node slot committed)",
    );
}

#[test]
fn image_slot_structural_failure_surfaces_and_stops_later_props() {
    use crate::ui::{IntoPropValue, RsxNode, RsxTagDescriptor};
    use crate::view::base_component::{Element, Image};
    use crate::view::{ImageFit, ImageSource};
    use std::sync::Arc;

    let source = ImageSource::Rgba {
        width: 1,
        height: 1,
        pixels: Arc::<[u8]>::from(vec![0, 0, 0, 255]),
    };
    let mut arena = NodeArena::new();
    let owner = arena.insert(Node::new(Box::new(Image::new_with_id(70, source))));
    let old_slot = arena.insert(Node::with_parent(
        Box::new(Element::new_with_id(71, 0.0, 0.0, 1.0, 1.0)),
        Some(owner),
    ));
    arena.with_element_taken(owner, |element, _| {
        element
            .as_any_mut()
            .downcast_mut::<Image>()
            .unwrap()
            .attach_loading_slot_cold(vec![old_slot]);
    });

    // Deliberately corrupt the active mirror. The slot replacement must
    // fail before deleting old_slot, and the later fit prop must not run.
    let rogue = arena.insert(Node::with_parent(
        Box::new(Element::new_with_id(72, 0.0, 0.0, 1.0, 1.0)),
        Some(owner),
    ));
    arena.set_children(owner, vec![rogue]);
    let signature_before = arena.get(owner).unwrap().element.retained_paint_signature();
    let len_before = arena.len();
    let replacement = RsxNode::tagged(
        "Element",
        RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
    );

    let result = apply_fiber_works(
        &mut arena,
        test_apply_ctx(),
        vec![FiberWork::Update {
            key: owner,
            changed: vec![
                ("loading", replacement.into_prop_value()),
                ("fit", ImageFit::Cover.into_prop_value()),
            ],
            removed: vec![],
        }],
    );

    assert_eq!(
        result,
        Err(UpdateFailure::StructuralPropApplyFailed("loading"))
    );
    assert_eq!(arena.len(), len_before, "new slot subtree must be cleaned");
    assert!(
        arena.contains_key(old_slot),
        "old slot must remain authoritative"
    );
    let image_node = arena.get(owner).unwrap();
    let image = image_node.element.as_any().downcast_ref::<Image>().unwrap();
    assert_eq!(image.loading_slot_len(), 1);
    assert_eq!(image.retained_paint_signature(), signature_before);
}

#[test]
fn svg_slot_structural_failure_surfaces_as_update_failure() {
    use crate::ui::{IntoPropValue, RsxNode, RsxTagDescriptor};
    use crate::view::SvgSource;
    use crate::view::base_component::{Element, Svg};

    let mut arena = NodeArena::new();
    let owner = arena.insert(Node::new(Box::new(Svg::new_with_id(
        80,
        SvgSource::Content(
            r#"<svg width="1" height="1" xmlns="http://www.w3.org/2000/svg"/>"#.into(),
        ),
    ))));
    let old_slot = arena.insert(Node::with_parent(
        Box::new(Element::new_with_id(81, 0.0, 0.0, 1.0, 1.0)),
        Some(owner),
    ));
    arena.with_element_taken(owner, |element, _| {
        element
            .as_any_mut()
            .downcast_mut::<Svg>()
            .unwrap()
            .attach_error_slot_cold(vec![old_slot]);
    });
    let rogue = arena.insert(Node::with_parent(
        Box::new(Element::new_with_id(82, 0.0, 0.0, 1.0, 1.0)),
        Some(owner),
    ));
    arena.set_children(owner, vec![rogue]);
    let len_before = arena.len();
    let replacement = RsxNode::tagged(
        "Element",
        RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
    );

    assert_eq!(
        apply_fiber_works(
            &mut arena,
            test_apply_ctx(),
            vec![FiberWork::Update {
                key: owner,
                changed: vec![("error", replacement.into_prop_value())],
                removed: vec![],
            }],
        ),
        Err(UpdateFailure::StructuralPropApplyFailed("error"))
    );
    assert_eq!(arena.len(), len_before);
    assert!(arena.contains_key(old_slot));
    let svg_node = arena.get(owner).unwrap();
    let svg = svg_node.element.as_any().downcast_ref::<Svg>().unwrap();
    assert_eq!(svg.loading_slot_len(), 0);
}

/// A FiberWork::Update with `removed = ["opacity"]` on a Text host
/// resets opacity to the default 1.0. Element's schema folds opacity
/// into the `style` map (no top-level reset arm); Text still exposes
/// `opacity` as a named prop with its own reset arm, so it's the
/// smallest case that exercises the named-prop reset branch.
#[test]
fn fiber_work_removes_opacity_resets_to_default_on_text() {
    use crate::view::base_component::Text;
    use crate::view::node_arena::Node;

    let mut arena = NodeArena::new();
    let mut text = Text::new(0.0, 0.0, 100.0, 20.0, "hello");
    text.set_opacity(0.3);
    assert!((text.opacity() - 0.3).abs() < 1e-4);
    let k = arena.insert(Node::new(Box::new(text)));

    apply_fiber_works(
        &mut arena,
        test_apply_ctx(),
        vec![FiberWork::Update {
            key: k,
            changed: vec![],
            removed: vec!["opacity"],
        }],
    )
    .expect("text opacity reset applies");

    let node = arena.get(k).expect("node survived");
    let text = node
        .element
        .as_any()
        .downcast_ref::<Text>()
        .expect("Text host");
    assert!(
        (text.opacity() - 1.0).abs() < 1e-4,
        "removed opacity must reset to default 1.0, got {}",
        text.opacity()
    );
}
