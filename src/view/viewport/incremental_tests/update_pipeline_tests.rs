use super::super::{Viewport, ViewportPaintRendererMode};
use super::common::*;
use crate::style::{Length, Transform, Translate};
use crate::ui::{Binding, GlobalKey, RsxKey, RsxNode, component, profile_ui_work, rsx};
use crate::view::Element;
use crate::view::node_arena::NodeKey;

fn modes() -> [ViewportPaintRendererMode; 2] {
    [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ]
}
fn set_scroll(v: &Viewport, key: NodeKey) {
    v.scene
        .node_arena
        .mutate_element_ref_with_invalidation(key, |e, _| e.set_scroll_offset((0.0, 37.0)));
}
fn scroll(v: &Viewport, key: NodeKey) -> (f32, f32) {
    v.scene
        .node_arena
        .get(key)
        .unwrap()
        .element
        .get_scroll_offset()
}

#[test]
fn one_text_update_diffs_once_and_does_not_transfer_scroll() {
    for mode in modes() {
        let mut v = Viewport::new();
        v.set_paint_renderer_mode(mode);
        let siblings = (0..64).map(|_| host_el()).collect::<Vec<_>>();
        let tree = |text: &str| {
            let mut root = host_el().with_child(text_leaf(text));
            for child in &siblings {
                root = root.with_child(child.clone());
            }
            root
        };
        v.render_rsx(&tree("before")).unwrap();
        let key = v.scene.ui_root_keys[0];
        set_scroll(&v, key);
        let (_, p) = profile_ui_work(|| v.render_rsx(&tree("after")).unwrap());
        assert_eq!(p.reconcile_calls, 1);
        assert_eq!(p.patches, 1);
        assert_eq!(p.fiber_works, 1);
        assert_eq!((p.scroll_save_nodes, p.scroll_restore_nodes), (0, 0));
        assert_eq!(v.scene.ui_root_keys[0], key);
        assert_eq!(scroll(&v, key), (0.0, 37.0));
        assert_eq!(p.shared_subtree_hits, 64);
    }
}

#[test]
fn fragment_placement_reuses_the_same_root_relative_patches() {
    for mode in modes() {
        let mut v = Viewport::new();
        v.set_paint_renderer_mode(mode);
        let leaf = |x| rsx! { <Element style={{ transform: Transform::new([Translate::x(Length::px(x))]) }} /> };
        let first = RsxNode::fragment(vec![
            host_el().with_key(RsxKey::Local(1)),
            host_el()
                .with_key(RsxKey::Local(2))
                .with_child(RsxNode::fragment(vec![leaf(0.0)])),
        ]);
        let next = RsxNode::fragment(vec![
            host_el().with_key(RsxKey::Local(1)),
            host_el()
                .with_key(RsxKey::Local(2))
                .with_child(RsxNode::fragment(vec![leaf(24.0)])),
        ]);
        v.render_rsx(&first).unwrap();
        let roots = v.scene.ui_root_keys.clone();
        let (_, p) = profile_ui_work(|| v.render_rsx(&next).unwrap());
        assert_eq!(p.reconcile_calls, 1);
        assert_eq!(p.fiber_works, 0, "placement consumed the shared patches");
        assert_eq!(v.scene.ui_root_keys, roots);
        let child = v.scene.node_arena.children_of(roots[1])[0];
        let n = v.scene.node_arena.get(child).unwrap();
        assert_eq!(
            n.element
                .as_any()
                .downcast_ref::<crate::view::base_component::Element>()
                .unwrap()
                .debug_transform(),
            &Transform::new([Translate::x(Length::px(24.0))])
        );
    }
}

#[test]
fn root_reorder_with_update_keeps_scroll_on_the_correct_owner() {
    for mode in modes() {
        let mut v = Viewport::new();
        v.set_paint_renderer_mode(mode);
        let a = host_el()
            .with_key(RsxKey::Local(1))
            .with_child(text_leaf("a"));
        let b = host_el()
            .with_key(RsxKey::Local(2))
            .with_child(text_leaf("b"));
        v.render_rsx(&RsxNode::fragment(vec![a.clone(), b]))
            .unwrap();
        let roots = v.scene.ui_root_keys.clone();
        set_scroll(&v, roots[1]);
        let next = RsxNode::fragment(vec![
            host_el()
                .with_key(RsxKey::Local(2))
                .with_child(text_leaf("new b")),
            a,
        ]);
        let (_, p) = profile_ui_work(|| v.render_rsx(&next).unwrap());
        assert_eq!(p.reconcile_calls, 1);
        assert_eq!((p.scroll_save_nodes, p.scroll_restore_nodes), (0, 0));
        assert_eq!(v.scene.ui_root_keys, vec![roots[1], roots[0]]);
        assert_eq!(scroll(&v, roots[1]), (0.0, 37.0));
    }
}

#[test]
fn cross_parent_move_transfers_only_the_removed_subtree_scroll() {
    for mode in modes() {
        let mut v = Viewport::new();
        v.set_paint_renderer_mode(mode);
        let moving = host_el().with_key(GlobalKey::new());
        let first = host_el()
            .with_child(
                host_el()
                    .with_key(RsxKey::Local(1))
                    .with_child(moving.clone()),
            )
            .with_child(host_el().with_key(RsxKey::Local(2)));
        let next = host_el()
            .with_child(host_el().with_key(RsxKey::Local(1)))
            .with_child(host_el().with_key(RsxKey::Local(2)).with_child(moving));
        v.render_rsx(&first).unwrap();
        let parents = v
            .scene
            .node_arena
            .children_of(v.scene.ui_root_keys[0])
            .to_vec();
        let old = v.scene.node_arena.children_of(parents[0])[0];
        set_scroll(&v, old);
        let (_, p) = profile_ui_work(|| v.render_rsx(&next).unwrap());
        let moved = v.scene.node_arena.children_of(parents[1])[0];
        assert_eq!(scroll(&v, moved), (0.0, 37.0));
        assert_eq!(p.reconcile_calls, 1);
        assert_eq!((p.scroll_save_nodes, p.scroll_restore_nodes), (1, 1));
    }
}

#[component]
fn ProfileLeaf(value: i32) -> RsxNode {
    host_el().with_child(text_leaf(&value.to_string()))
}
struct ProfileApp(Binding<i32>);
impl crate::app::App for ProfileApp {
    fn build(&mut self, _: &mut crate::app::AppContext<'_>) -> RsxNode {
        rsx! { <ProfileLeaf value={self.0.snapshot().get()} /> }
    }
}
#[test]
fn app_frame_captures_flush_build_and_commit_without_stale_direct_samples() {
    let mut v = Viewport::new();
    let binding = Binding::new(0);
    v.set_app(Box::new(ProfileApp(binding.clone())));
    let mut host = crate::platform::HeadlessBackend::default();
    let mut render = |v: &mut Viewport| {
        v.render_frame(crate::platform::PlatformServices {
            clipboard: &mut host.clipboard,
            cursor: &mut host.cursor,
            redraw: &host.redraw,
        });
    };
    render(&mut v);
    binding.set(1);
    binding.update(|n| *n += 1);
    render(&mut v);
    let p = v.frontend_profile();
    assert_eq!(
        (
            p.work.state_targets,
            p.work.state_actions,
            p.work.changed_targets
        ),
        (1, 2, 1)
    );
    assert_eq!(p.work.component_renders, 1);
    assert_eq!(p.work.reconcile_calls, 1);
    assert!(p.rsx_build_ms >= p.work.unwrap_ms);
    assert!(
        p.scene_update_ms
            >= p.work.reconcile_ms + p.work.translate_ms + p.work.incremental_commit_ms
    );
    let root = v.scene.last_rsx_root.clone().unwrap();
    profile_ui_work(|| {
        v.render_rsx(&root).unwrap();
        let direct = v.frontend_profile();
        assert_eq!(direct.rsx_build_ms, 0.0);
        assert_eq!(direct.work.component_renders, 0);
        assert_eq!(direct.work.reconcile_calls, 0);
        v.render_rsx(&root).unwrap();
        assert_eq!(v.frontend_profile().work.reconcile_calls, 0);
    });
}

#[test]
fn resource_slot_prop_replacement_preserves_scroll_for_both_hosts() {
    use crate::ui::{IntoPropValue, RsxTagDescriptor};
    use crate::view::{ImageSource, SvgSource};
    for mode in modes() {
        for svg in [false, true] {
            let mut v = Viewport::new();
            v.set_paint_renderer_mode(mode);
            let slot_key = GlobalKey::new();
            let tree = |text: &str| {
                let slot = host_el().with_key(slot_key).with_child(text_leaf(text));
                let (tag, descriptor, source) = if svg {
                    (
                        "Svg",
                        RsxTagDescriptor::for_tag::<crate::view::Svg>(),
                        SvgSource::Content("<svg xmlns='http://www.w3.org/2000/svg'/>".into())
                            .into_prop_value(),
                    )
                } else {
                    (
                        "Image",
                        RsxTagDescriptor::for_tag::<crate::view::Image>(),
                        ImageSource::Rgba {
                            width: 1,
                            height: 1,
                            pixels: std::sync::Arc::from([0, 0, 0, 255]),
                        }
                        .into_prop_value(),
                    )
                };
                RsxNode::tagged(tag, descriptor)
                    .with_prop("source", source)
                    .with_prop("loading", slot.into_prop_value())
            };
            v.render_rsx(&tree("before")).unwrap();
            let owner = v.scene.ui_root_keys[0];
            v.scene.node_arena.with_element_taken(owner, |host, arena| {
                if svg {
                    host.as_any()
                        .downcast_ref::<crate::view::base_component::Svg>()
                        .unwrap()
                        .set_resource_loading_for_test();
                } else {
                    host.as_any()
                        .downcast_ref::<crate::view::base_component::Image>()
                        .unwrap()
                        .set_resource_loading_for_test();
                }
                host.sync_arena(arena);
            });
            let slot = v.scene.node_arena.children_of(owner)[0];
            set_scroll(&v, slot);
            let stable_id = v.scene.node_arena.get(slot).unwrap().element.stable_id();
            let (_, p) = profile_ui_work(|| v.render_rsx(&tree("after")).unwrap());
            let replacement = v.scene.node_arena.find_by_stable_id(stable_id).unwrap();
            assert_ne!(slot, replacement, "slot host was actually replaced");
            assert_eq!(v.scene.ui_root_keys[0], owner);
            assert_eq!(scroll(&v, replacement), (0.0, 37.0));
            assert!(p.scroll_save_nodes > 0);
            assert_eq!(p.scroll_restore_nodes, 1);
        }
    }
}

#[test]
fn root_set_replacement_restores_scroll_after_new_keys_are_committed() {
    for mode in modes() {
        let mut v = Viewport::new();
        v.set_paint_renderer_mode(mode);
        let a = host_el().with_key(GlobalKey::new());
        let b = host_el().with_key(GlobalKey::new());
        v.render_rsx(&RsxNode::fragment(vec![a.clone(), b.clone()]))
            .unwrap();
        let old = v.scene.ui_root_keys[0];
        set_scroll(&v, old);
        let (_, p) = profile_ui_work(|| {
            v.render_rsx(&RsxNode::fragment(vec![a, b, host_el()]))
                .unwrap()
        });
        assert_ne!(v.scene.ui_root_keys[0], old);
        assert_eq!(scroll(&v, v.scene.ui_root_keys[0]), (0.0, 37.0));
        assert_eq!((p.scroll_save_nodes, p.scroll_restore_nodes), (2, 1));
    }
}

#[test]
fn equal_new_tree_becomes_the_snapshot_for_following_redraws() {
    let mut v = Viewport::new();
    let old = host_el().with_child(text_leaf("same"));
    let new = host_el().with_child(text_leaf("same"));
    assert!(!RsxNode::ptr_eq(&old, &new));
    v.render_rsx(&old).unwrap();
    let key = v.scene.ui_root_keys[0];
    let (_, first) = profile_ui_work(|| v.render_rsx(&new).unwrap());
    assert_eq!(first.reconcile_calls, 1);
    assert_eq!(first.patches, 0);
    assert_eq!(v.scene.ui_root_keys[0], key);
    let (_, warm) = profile_ui_work(|| v.render_rsx(&new).unwrap());
    assert_eq!(warm.reconcile_calls, 0);
    assert!(RsxNode::ptr_eq(
        v.scene.last_rsx_root.as_ref().unwrap(),
        &new
    ));
}

#[test]
fn partial_incremental_failure_keeps_saved_scroll_through_cold_recovery() {
    use crate::ui::{IntoPropValue, RsxTagDescriptor};
    use crate::view::node_arena::Node;
    use crate::view::{Image, ImageSource};
    for mode in modes() {
        let mut v = Viewport::new();
        v.set_paint_renderer_mode(mode);
        let moving = host_el().with_key(GlobalKey::new());
        let image = |text: &str| {
            RsxNode::tagged("Image", RsxTagDescriptor::for_tag::<Image>())
                .with_prop(
                    "source",
                    ImageSource::Rgba {
                        width: 1,
                        height: 1,
                        pixels: std::sync::Arc::from([0, 0, 0, 255]),
                    }
                    .into_prop_value(),
                )
                .with_prop(
                    "loading",
                    host_el().with_child(text_leaf(text)).into_prop_value(),
                )
        };
        let first = host_el()
            .with_child(
                host_el()
                    .with_key(RsxKey::Local(1))
                    .with_child(moving.clone()),
            )
            .with_child(host_el().with_key(RsxKey::Local(2)))
            .with_child(image("old"));
        let next = host_el()
            .with_child(host_el().with_key(RsxKey::Local(1)))
            .with_child(host_el().with_key(RsxKey::Local(2)).with_child(moving))
            .with_child(image("new"));
        v.render_rsx(&first).unwrap();
        let root = v.scene.ui_root_keys[0];
        let children = v.scene.node_arena.children_of(root);
        let old = v.scene.node_arena.children_of(children[0])[0];
        set_scroll(&v, old);
        let id = v.scene.node_arena.get(old).unwrap().element.stable_id();
        // Corrupt the Image's active edge list, matching the established
        // structural-slot failure fixture. Earlier delete/create work remains
        // applied when loading replacement rejects the inconsistent mirror.
        let image_key = children[2];
        let rogue = v.scene.node_arena.insert(Node::with_parent(
            Box::new(crate::view::base_component::Element::new_with_id(
                0xAB_AB_01, 0.0, 0.0, 1.0, 1.0,
            )),
            Some(image_key),
        ));
        v.scene.node_arena.set_children(image_key, vec![rogue]);
        let (_, p) = profile_ui_work(|| v.render_rsx(&next).unwrap());
        assert_ne!(
            v.scene.ui_root_keys[0], root,
            "failed incremental batch took cold recovery"
        );
        let new_key = v.scene.node_arena.find_by_stable_id(id).unwrap();
        assert_eq!(scroll(&v, new_key), (0.0, 37.0));
        assert!(p.scroll_save_nodes > 0);
        assert_eq!(p.reconcile_calls, 1);
        assert_eq!(v.scene.node_arena.roots(), &v.scene.ui_root_keys);
    }
}
