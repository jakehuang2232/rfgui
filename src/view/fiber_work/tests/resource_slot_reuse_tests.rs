use super::*;
use crate::ui::{IntoPropValue, RsxTagDescriptor};
use crate::view::renderer_adapter::{
    StyleCascadeContext, convert_image_element_desc, convert_svg_element_desc,
};
use crate::view::{ImageSource, SvgSource};

fn static_slot(height: f32) -> RsxNode {
    host_element_node()
        .with_prop(
            "style",
            crate::view::tags::ElementStylePropSchema {
                height: Some(crate::style::Length::px(height)),
                ..Default::default()
            }
            .into_prop_value(),
        )
        .with_child(RsxNode::text("loading"))
}

fn cold_host(arena: &mut NodeArena, svg: bool, path: &[u64]) -> NodeKey {
    let source = if svg {
        SvgSource::Content("<svg xmlns='http://www.w3.org/2000/svg'/>".into()).into_prop_value()
    } else {
        ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: std::sync::Arc::from([0, 0, 0, 255]),
        }
        .into_prop_value()
    };
    let node = RsxNode::tagged(
        if svg { "Svg" } else { "Image" },
        if svg {
            RsxTagDescriptor::for_tag::<crate::view::tags::Svg>()
        } else {
            RsxTagDescriptor::for_tag::<crate::view::tags::Image>()
        },
    )
    .with_prop("source", source)
    .with_prop("loading", static_slot(10.0).into_prop_value())
    .with_prop("error", static_slot(20.0).into_prop_value());
    let RsxNode::Element(node) = node else {
        unreachable!()
    };
    let desc = if svg {
        convert_svg_element_desc(
            &node,
            path,
            None,
            &StyleCascadeContext::from_viewport_style(
                test_apply_ctx().viewport_style,
                800.0,
                600.0,
            ),
        )
    } else {
        convert_image_element_desc(
            &node,
            path,
            None,
            &StyleCascadeContext::from_viewport_style(
                test_apply_ctx().viewport_style,
                800.0,
                600.0,
            ),
        )
    }
    .unwrap();
    commit_descriptor_tree(arena, None, desc)
}

fn update(
    arena: &mut NodeArena,
    key: NodeKey,
    slot: &'static str,
    height: f32,
) -> Result<(), UpdateFailure> {
    apply_fiber_works(
        arena,
        test_apply_ctx(),
        vec![FiberWork::Update {
            key,
            changed: vec![(slot, static_slot(height).into_prop_value())],
            removed: vec![],
        }],
    )
}

#[test]
fn identical_static_slots_preserve_cold_keys_and_index_revision_for_both_hosts() {
    for svg in [false, true] {
        let mut arena = NodeArena::new();
        let key = cold_host(&mut arena, svg, &[17]);
        let index = arena.stable_id_index().clone();
        let revision = arena.stable_id_index_revision();
        for _ in 0..5 {
            update(&mut arena, key, "loading", 10.0).unwrap();
            update(&mut arena, key, "error", 20.0).unwrap();
        }
        assert_eq!(arena.stable_id_index(), &index);
        assert_eq!(arena.stable_id_index_revision(), revision);
        update(&mut arena, key, "loading", 30.0).unwrap();
        assert_ne!(arena.stable_id_index_revision(), revision);
        let changed = arena.stable_id_index().clone();
        update(&mut arena, key, "loading", 30.0).unwrap();
        assert_eq!(arena.stable_id_index(), &changed);
    }
}

#[test]
fn changed_slots_keep_host_scopes_disjoint() {
    for svg in [false, true] {
        let mut arena = NodeArena::new();
        let a = cold_host(&mut arena, svg, &[1]);
        let b = cold_host(&mut arena, svg, &[2]);
        update(&mut arena, a, "loading", 30.0).unwrap();
        update(&mut arena, b, "loading", 30.0).unwrap();
        assert_eq!(arena.stable_id_index().len(), arena.len());
        for (&id, &key) in arena.stable_id_index() {
            assert_eq!(arena.find_by_stable_id(id), Some(key));
        }
    }
}

#[test]
fn identical_slot_still_rejects_corrupt_owner_mirror() {
    for svg in [false, true] {
        let mut arena = NodeArena::new();
        let key = cold_host(&mut arena, svg, &[1]);
        let fake = arena.insert(Node::new(Box::new(
            crate::view::base_component::Element::new_with_id(99, 0.0, 0.0, 1.0, 1.0),
        )));
        arena.set_children(key, vec![fake]);
        assert!(update(&mut arena, key, "loading", 10.0).is_err());
    }
}

#[test]
fn identical_active_slots_survive_resource_state_switches() {
    for svg in [false, true] {
        let mut arena = NodeArena::new();
        let key = cold_host(&mut arena, svg, &[4]);
        for error in [false, true, false] {
            arena.with_element_taken(key, |el, arena| {
                if svg {
                    let image = el
                        .as_any_mut()
                        .downcast_mut::<crate::view::base_component::Svg>()
                        .unwrap();
                    if error {
                        image.set_resource_error_for_test();
                    } else {
                        image.set_resource_loading_for_test();
                    }
                } else {
                    let image = el
                        .as_any_mut()
                        .downcast_mut::<crate::view::base_component::Image>()
                        .unwrap();
                    if error {
                        image.set_resource_error_for_test();
                    } else {
                        image.set_resource_loading_for_test();
                    }
                }
                el.sync_arena(arena);
            });
            let children = arena.children_of(key);
            assert_eq!(children.len(), 1);
            let revision = arena.stable_id_index_revision();
            update(&mut arena, key, "loading", 10.0).unwrap();
            update(&mut arena, key, "error", 20.0).unwrap();
            assert_eq!(arena.children_of(key), children);
            assert_eq!(arena.stable_id_index_revision(), revision);
        }
    }
}

#[test]
fn same_slot_input_with_changed_inherited_context_is_rebuilt() {
    for svg in [false, true] {
        let mut arena = NodeArena::new();
        let key = cold_host(&mut arena, svg, &[5]);
        let before = arena.stable_id_index_revision();
        let mut ctx = test_apply_ctx();
        ctx.viewport_width = 1000.0;
        apply_fiber_works(
            &mut arena,
            ctx,
            vec![FiberWork::Update {
                key,
                changed: vec![("loading", static_slot(10.0).into_prop_value())],
                removed: vec![],
            }],
        )
        .unwrap();
        assert_ne!(arena.stable_id_index_revision(), before);
    }
}
