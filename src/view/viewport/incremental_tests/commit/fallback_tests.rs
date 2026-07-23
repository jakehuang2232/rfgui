use super::*;

#[test]
fn structural_slot_failure_after_root_create_cold_rebuilds_without_orphans() {
    use crate::ui::{IntoPropValue, RsxComponent};
    use crate::view::ImageSource;
    use crate::view::base_component::{Element as ElementHost, Image};
    use crate::view::fiber_work::{FiberWork, UpdateFailure, apply_fiber_works};
    use crate::view::node_arena::Node;
    use crate::view::renderer_adapter::ElementDescriptor;
    use crate::view::tags::{Image as ImageTag, ImagePropSchema};

    fn image_tree(source: ImageSource, loading: RsxNode) -> RsxNode {
        <ImageTag as RsxComponent<ImagePropSchema>>::render(
            ImagePropSchema {
                source,
                style: None,
                fit: None,
                sampling: None,
                loading: Some(loading),
                error: None,
            },
            Vec::new(),
        )
    }

    let source = ImageSource::Rgba {
        width: 1,
        height: 1,
        pixels: std::sync::Arc::<[u8]>::from(vec![0, 0, 0, 255]),
    };
    let old_loading = host_el();
    let new_loading = host_el().with_child(host_el());
    let first = image_tree(source.clone(), old_loading);
    let second = image_tree(source, new_loading.clone());
    let mut viewport = Viewport::new();
    viewport.render_rsx(&first).expect("cold image render");
    let old_root = viewport.scene.ui_root_keys[0];
    let retained_len_before_corruption = viewport.scene.node_arena.len();

    // Make the Image's arena children disagree with its latent-slot mirror so
    // the later loading replacement returns a structural failure.
    let rogue_sid = 0x51_07_u64;
    let rogue = viewport.scene.node_arena.insert(Node::with_parent(
        Box::new(ElementHost::new_with_id(rogue_sid, 0.0, 0.0, 1.0, 1.0)),
        Some(old_root),
    ));
    viewport
        .scene
        .node_arena
        .set_children(old_root, vec![rogue]);

    // The root Create succeeds before the slot Update fails. This models a
    // partially-applied, non-transactional Fiber batch.
    let extra_sid = 0x51_08_u64;
    let extra_descriptor = ElementDescriptor::leaf(Box::new(ElementHost::new_with_id(
        extra_sid, 0.0, 0.0, 1.0, 1.0,
    )));
    let result = apply_fiber_works(
        &mut viewport.scene.node_arena,
        test_apply_ctx(),
        vec![
            FiberWork::Create {
                parent: None,
                index: 0,
                descriptor: extra_descriptor,
                stable_id: extra_sid,
            },
            FiberWork::Update {
                key: old_root,
                changed: vec![("loading", new_loading.into_prop_value())],
                removed: Vec::new(),
            },
        ],
    );
    assert_eq!(
        result,
        Err(UpdateFailure::StructuralPropApplyFailed("loading"))
    );
    assert_eq!(
        viewport.scene.node_arena.len(),
        retained_len_before_corruption + 2,
        "only the deliberately-created rogue and extra root may remain"
    );
    assert_eq!(viewport.scene.ui_root_keys, vec![old_root]);
    assert_eq!(viewport.scene.node_arena.roots().len(), 2);
    assert!(
        viewport
            .scene
            .node_arena
            .find_by_stable_id(extra_sid)
            .is_some()
    );
    {
        let old_image_node = viewport.scene.node_arena.get(old_root).unwrap();
        let old_image = old_image_node
            .element
            .as_any()
            .downcast_ref::<Image>()
            .unwrap();
        assert_eq!(
            old_image.loading_slot_len(),
            1,
            "old slot remains authoritative"
        );
    }

    // This is the exact recovery hook used by render_rsx's Err branch. The
    // subsequent forced cold render must remove both current arena roots and
    // the stale viewport mirror before committing the new RSX tree.
    viewport
        .scene
        .refresh_roots_for_cold_rebuild_after_incremental_failure();
    assert_eq!(viewport.scene.ui_root_keys.len(), 2);
    viewport.set_use_incremental_commit(false);
    viewport.render_rsx(&second).expect("cold fallback render");

    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    assert_eq!(
        viewport.scene.node_arena.roots(),
        viewport.scene.ui_root_keys.as_slice()
    );
    assert_ne!(viewport.scene.ui_root_keys[0], old_root);
    assert!(!viewport.scene.node_arena.contains_key(old_root));
    assert_eq!(viewport.scene.node_arena.find_by_stable_id(extra_sid), None);
    assert_eq!(viewport.scene.node_arena.find_by_stable_id(rogue_sid), None);
    let mut cold_oracle = Viewport::new();
    cold_oracle.set_use_incremental_commit(false);
    cold_oracle
        .render_rsx(&second)
        .expect("fresh cold oracle render");
    assert_eq!(
        viewport.scene.node_arena.len(),
        cold_oracle.scene.node_arena.len(),
        "fallback arena must contain exactly the fresh cold tree"
    );
    assert_eq!(
        viewport.scene.node_arena.arena_sync_node_count_for_test(),
        1,
        "only the rebuilt Image host may remain registered for arena sync"
    );
    let rebuilt_root = viewport.scene.ui_root_keys[0];
    let rebuilt_image_node = viewport.scene.node_arena.get(rebuilt_root).unwrap();
    let rebuilt_image = rebuilt_image_node
        .element
        .as_any()
        .downcast_ref::<Image>()
        .unwrap();
    assert_eq!(rebuilt_image.loading_slot_len(), 1);
}

#[test]
fn viewport_structural_slot_failure_automatically_falls_back_to_cold_rebuild() {
    use crate::ui::RsxComponent;
    use crate::view::ImageSource;
    use crate::view::base_component::Element as ElementHost;
    use crate::view::node_arena::Node;
    use crate::view::tags::{Image as ImageTag, ImagePropSchema};

    fn image_tree(source: ImageSource, loading: RsxNode) -> RsxNode {
        <ImageTag as RsxComponent<ImagePropSchema>>::render(
            ImagePropSchema {
                source,
                style: None,
                fit: None,
                sampling: None,
                loading: Some(loading),
                error: None,
            },
            Vec::new(),
        )
    }

    let source = ImageSource::Rgba {
        width: 1,
        height: 1,
        pixels: std::sync::Arc::<[u8]>::from(vec![0, 0, 0, 255]),
    };
    let first = image_tree(source.clone(), host_el());
    let second = image_tree(source, host_el().with_child(host_el()));
    let mut viewport = Viewport::new();
    viewport.render_rsx(&first).expect("cold image render");
    let old_root = viewport.scene.ui_root_keys[0];
    let rogue_sid = 0x51_09_u64;
    let rogue = viewport.scene.node_arena.insert(Node::with_parent(
        Box::new(ElementHost::new_with_id(rogue_sid, 0.0, 0.0, 1.0, 1.0)),
        Some(old_root),
    ));
    viewport
        .scene
        .node_arena
        .set_children(old_root, vec![rogue]);

    // render_rsx must observe StructuralPropApplyFailed, keep needs_rebuild
    // set, clean the failed retained tree, and commit the authoritative RSX.
    viewport
        .render_rsx(&second)
        .expect("structural failure cold fallback");

    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    assert_ne!(viewport.scene.ui_root_keys[0], old_root);
    assert_eq!(
        viewport.scene.node_arena.roots(),
        viewport.scene.ui_root_keys.as_slice()
    );
    assert!(!viewport.scene.node_arena.contains_key(old_root));
    assert_eq!(viewport.scene.node_arena.find_by_stable_id(rogue_sid), None);
    assert_eq!(viewport.scene.last_rsx_root.as_ref(), Some(&second));

    let mut cold_oracle = Viewport::new();
    cold_oracle.set_use_incremental_commit(false);
    cold_oracle.render_rsx(&second).expect("cold oracle render");
    assert_eq!(
        viewport.scene.node_arena.len(),
        cold_oracle.scene.node_arena.len()
    );
    assert_eq!(
        viewport.scene.node_arena.arena_sync_node_count_for_test(),
        1
    );
}

/// M5: the flag is on by default. Flipping it off must still work
/// (call sites can A/B test or bisect regressions), and a render
/// round-trip in off-mode should succeed via the legacy full-rebuild
/// path.
#[test]
fn flag_default_on_and_off_switch_survives_round_trip() {
    let first = single_element(120.0);
    let second = single_element(120.0);

    let mut viewport = Viewport::new();
    assert!(
        viewport.use_incremental_commit(),
        "M5 default: flag starts on",
    );

    viewport.set_use_incremental_commit(false);
    viewport.render_rsx(&first).expect("cold render (flag off)");
    viewport
        .render_rsx(&second)
        .expect("identical re-render with flag off must still succeed");
    assert!(!viewport.use_incremental_commit());
}

// ---------------------------------------------------------------------------
// 軌 1 #4 Fragment-at-root: multi-root incremental path
// ---------------------------------------------------------------------------

#[test]
fn transition_sample_resolves_global_target_after_reparent_parent_chain_drift() {
    let target = rsx! {
        <HostElement style={{
            width: Length::px(88.0),
            height: Length::px(40.0),
            background: Color::hex("#61afef"),
            transition: [Transition::new(TransitionProperty::All, 10_000)],
        }} />
    };
    let tree = RsxNode::fragment(vec![
        rsx! { <HostElement>{target}</HostElement> },
        host_el(),
    ]);
    let mut viewport = Viewport::new();
    viewport.render_rsx(&tree).expect("cold render");

    let actual_root = viewport.scene.ui_root_keys[0];
    let stale_parent_root = viewport.scene.ui_root_keys[1];
    let target_key = viewport.scene.node_arena.children_of(actual_root)[0];
    let target_id = viewport
        .scene
        .node_arena
        .get(target_key)
        .expect("target node")
        .element
        .stable_id();

    // Cross-parent GlobalKey moves update the active child walk before every
    // retained parent link has necessarily converged. Stable ids are global,
    // so transition dispatch must not reject the resolved target based on the
    // stale parent chain.
    viewport
        .scene
        .node_arena
        .set_parent(target_key, Some(stale_parent_root));
    let stale_duplicate = viewport
        .scene
        .node_arena
        .insert(crate::view::node_arena::Node::new(Box::new(
            crate::view::base_component::Element::new_with_id(target_id, 0.0, 0.0, 1.0, 1.0),
        )));
    assert_eq!(
        viewport.scene.node_arena.find_by_stable_id(target_id),
        Some(stale_duplicate),
        "fixture must make the secondary index point at the retiring duplicate"
    );

    assert!(
        crate::view::viewport::transitions_tick::set_style_field_by_id(
            &mut viewport.scene.node_arena,
            actual_root,
            target_id,
            StyleField::BackgroundColor,
            StyleValue::Color(Color::rgb(198, 120, 221)),
        ),
        "globally unique transition target must remain writable after reparent"
    );
}
