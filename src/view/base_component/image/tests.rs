use super::{ActiveSlot, Image};
use crate::style::{BorderRadius, BoxShadow, ClipMode, Color, Position};
use crate::style::{ComputedStyle, EdgeInsets, Length, ParsedValue, PropertyId, Style};
use crate::style::{Layout, ScrollDirection};
use crate::view::ImageSource;
use crate::view::base_component::{
    ComputedStyleConsumer, DirtyFlags, Element, ElementTrait, EventTarget, LayoutConstraints,
    LayoutPlacement, Layoutable, ShadowPaintBlocker, ShadowPaintRecordingCapability, Size,
    UiBuildContext,
};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::frame_graph::FrameGraph;
use crate::view::image_resource::{ImageSnapshot, ReadyImage};
use crate::view::node_arena::{Node, NodeArena, NodeKey};
use crate::view::sampled_texture::{ImageAssetId, SampledTextureId};
use crate::view::test_support::{
    commit_child, commit_element, measure_and_place, new_test_arena,
};
use glam::{Mat4, Vec3};

fn rgba_source(width: u32, height: u32) -> ImageSource {
    ImageSource::Rgba {
        width,
        height,
        pixels: std::sync::Arc::<[u8]>::from(vec![255; (width * height * 4) as usize]),
    }
}



fn insert_inactive_slot_subtree(
    arena: &mut NodeArena,
    owner: NodeKey,
    id: u64,
) -> (NodeKey, NodeKey) {
    let root = arena.insert(Node::with_parent(
        Box::new(Element::new_with_id(id, 0.0, 0.0, 1.0, 1.0)),
        Some(owner),
    ));
    let child = arena.insert(Node::with_parent(
        Box::new(Element::new_with_id(id + 1, 0.0, 0.0, 1.0, 1.0)),
        Some(root),
    ));
    arena.set_children(root, vec![child]);
    (root, child)
}


fn path_source(label: &str) -> ImageSource {
    ImageSource::Path(std::path::PathBuf::from(format!(
        "/rfgui-m9b1-no-io-{label}.png"
    )))
}

fn prepared_ready_image(
    id: u64,
    source: ImageSource,
    width: u32,
    height: u32,
    pixels: std::sync::Arc<[u8]>,
) -> (
    crate::view::node_arena::NodeArena,
    crate::view::node_arena::NodeKey,
    ImageAssetId,
    u64,
) {
    let mut image = Image::new_with_id(id, source);
    let asset_id = image.source_handle.asset_id();
    let generation = crate::view::image_resource::replace_ready_image_for_test(
        asset_id, width, height, pixels,
    );
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(8.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(8.0)));
    image.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(image));
    measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 100.0,
            max_height: 100.0,
            viewport_width: 100.0,
            viewport_height: 100.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(100.0),
        },
        LayoutPlacement {
            parent_x: 1.25,
            parent_y: 2.75,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 100.0,
            available_height: 100.0,
            viewport_width: 100.0,
            viewport_height: 100.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(100.0),
        },
    );
    arena
        .get_mut(root)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyFlags::ALL);
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    (arena, root, asset_id, generation)
}

fn image_recording_context(
    arena: &crate::view::node_arena::NodeArena,
    root: crate::view::node_arena::NodeKey,
) -> crate::view::paint::PaintRecordingContext {
    arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context(Default::default())
}

fn record_image_metadata_and_artifact(
    arena: &crate::view::node_arena::NodeArena,
    root: crate::view::node_arena::NodeKey,
) -> (
    crate::view::paint::PaintChunkMetadata,
    crate::view::paint::PaintArtifact,
) {
    let context = image_recording_context(arena, root);
    let revision = crate::view::paint::PaintContentRevision {
        self_paint_revision: 1,
        composite_revision: 1,
        topology_revision: 1,
    };
    let node = arena.get(root).unwrap();
    assert_eq!(
        node.element
            .shadow_paint_recording_capability(arena, false, context),
        ShadowPaintRecordingCapability::Recordable
    );
    let metadata = node
        .element
        .record_shadow_paint_metadata(root, Default::default(), revision, arena, context)
        .expect("ready Image metadata");
    let artifact = node
        .element
        .record_shadow_paint_artifact(root, Default::default(), revision, arena, context)
        .expect("ready Image artifact");
    (metadata, artifact)
}

fn assert_missing_prepared_image_fallback(arena: &NodeArena, root: NodeKey) {
    let context = image_recording_context(arena, root);
    let revision = crate::view::paint::PaintContentRevision {
        self_paint_revision: 1,
        composite_revision: 1,
        topology_revision: 1,
    };
    let node = arena.get(root).unwrap();
    assert_eq!(
        node.element
            .shadow_paint_recording_capability(arena, false, context),
        ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedImage)
    );
    assert!(
        node.element
            .record_shadow_paint_metadata(root, Default::default(), revision, arena, context)
            .is_none()
    );
    assert!(
        node.element
            .record_shadow_paint_artifact(root, Default::default(), revision, arena, context)
            .is_none()
    );
}

fn prepared_ready_image_with_inactive_slots(
    id: u64,
) -> (NodeArena, NodeKey, NodeKey, NodeKey, NodeKey, NodeKey) {
    let (mut arena, root, _, _) = prepared_ready_image(
        id,
        path_source(&format!("ready-inactive-{id}")),
        2,
        2,
        std::sync::Arc::from([0x5a_u8; 16]),
    );
    let (loading_root, loading_child) = insert_inactive_slot_subtree(&mut arena, root, id + 1);
    let (error_root, error_child) = insert_inactive_slot_subtree(&mut arena, root, id + 3);
    arena.with_element_taken(root, |element, _arena| {
        let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
        image.attach_loading_slot_cold(vec![loading_root]);
        image.attach_error_slot_cold(vec![error_root]);
    });
    (
        arena,
        root,
        loading_root,
        loading_child,
        error_root,
        error_child,
    )
}

fn rounded_active_slot_image_fixture(
    id: u64,
    state: ActiveSlot,
) -> (NodeArena, NodeKey, NodeKey) {
    let mut image = Image::new_with_id(id, path_source(&format!("rounded-slot-{id}")));
    let asset_id = image.source_handle.asset_id();
    match state {
        ActiveSlot::Loading => {
            crate::view::image_resource::set_image_loading_for_test(asset_id)
        }
        ActiveSlot::Error => crate::view::image_resource::set_image_error_for_test(
            asset_id,
            "synthetic rounded-slot error",
        ),
        ActiveSlot::None => unreachable!(),
    }
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.set_border_radius(BorderRadius::uniform(Length::px(12.0)));
    image.apply_style(style);

    let mut arena = new_test_arena();
    let owner = commit_element(&mut arena, Box::new(image));
    let (loading_root, _) = insert_inactive_slot_subtree(&mut arena, owner, id + 1);
    let (error_root, _) = insert_inactive_slot_subtree(&mut arena, owner, id + 0x10);
    arena.with_element_taken(owner, |element, _arena| {
        let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
        image.attach_loading_slot_cold(vec![loading_root]);
        image.attach_error_slot_cold(vec![error_root]);
    });
    measure_and_place(
        &mut arena,
        owner,
        LayoutConstraints {
            max_width: 100.0,
            max_height: 100.0,
            viewport_width: 100.0,
            viewport_height: 100.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 100.0,
            available_height: 100.0,
            viewport_width: 100.0,
            viewport_height: 100.0,
            percent_base_width: None,
            percent_base_height: None,
        },
    );
    let active_root = match state {
        ActiveSlot::Loading => loading_root,
        ActiveSlot::Error => error_root,
        ActiveSlot::None => unreachable!(),
    };
    (arena, owner, active_root)
}

mod retained_paint_tests;
mod slot_lifecycle_tests;
mod paint_recording_tests;
mod shadow_recording_tests;
mod resource_freeze_tests;
mod layout_tests;
