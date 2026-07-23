use super::{ActiveSlot, FrozenSvgPaint, Svg};
use crate::style::{
    BorderRadius, BoxShadow, ClipMode, Color, ComputedStyle, EdgeInsets, Layout, ParsedValue,
    Position, PropertyId, ScrollDirection, Style,
};
use crate::time::{Duration, Instant};
use crate::view::SvgSource;
use crate::view::base_component::{
    ComputedStyleConsumer, DirtyFlags, Element, ElementTrait, EventTarget, LayoutConstraints,
    LayoutPlacement, Layoutable, PaintResourcePreparationContext, ShadowPaintBlocker,
    ShadowPaintRecordingCapability, Size,
};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::image_resource::{ImageSnapshot, ReadyImage};
use crate::view::node_arena::{Node, NodeArena, NodeKey};
use crate::view::sampled_texture::{SampledTextureId, SvgRasterAssetId};
use crate::view::svg_resource::{
    SvgDocumentSnapshot, SvgRasterMode, SvgRasterRequest, acquire_svg_raster,
    prime_svg_document_ready_for_test, prime_svg_raster_ready_for_test,
    remove_svg_document_entry_for_test, remove_svg_raster_entry_for_test,
    replace_svg_raster_ready_for_test, set_svg_document_error_for_test,
    set_svg_document_loading_for_test, set_svg_raster_error_for_test,
    set_svg_raster_loading_for_test, set_svg_raster_ready_for_test, snapshot_svg_document,
    snapshot_svg_raster, svg_raster_ref_count_for_test,
};
use crate::view::test_support::{
    commit_child, commit_element, measure_and_place, new_test_arena,
};
use glam::{Mat4, Vec3};

fn simple_svg() -> SvgSource {
    SvgSource::Content(
        r##"<svg width="80" height="40" viewBox="0 0 80 40" xmlns="http://www.w3.org/2000/svg"><rect width="80" height="40" fill="#ff0000"/></svg>"##.to_string(),
    )
}




fn insert_inactive_slot_subtree(
    arena: &mut NodeArena,
    owner: NodeKey,
    id: u64,
) -> (NodeKey, NodeKey) {
    let root = arena.insert(Node::with_parent(
        Box::new(crate::view::base_component::Element::new_with_id(
            id, 0.0, 0.0, 1.0, 1.0,
        )),
        Some(owner),
    ));
    let child = arena.insert(Node::with_parent(
        Box::new(crate::view::base_component::Element::new_with_id(
            id + 1,
            0.0,
            0.0,
            1.0,
            1.0,
        )),
        Some(root),
    ));
    arena.set_children(root, vec![child]);
    (root, child)
}

fn active_slot_svg_fixture(
    id: u64,
    state: ActiveSlot,
) -> (NodeArena, NodeKey, NodeKey, NodeKey, NodeKey, NodeKey) {
    let mut svg = Svg::new_with_id(id, unique_svg(&format!("active-slot-{id}")));
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.set_box_shadow(vec![
        BoxShadow::new()
            .color(Color::rgb(220, 30, 20))
            .offset_x(1.5)
            .offset_y(-2.25),
    ]);
    svg.apply_style(style);
    match state {
        ActiveSlot::Loading => set_svg_document_loading_for_test(svg.source_key),
        ActiveSlot::Error => set_svg_document_error_for_test(svg.source_key),
        ActiveSlot::None => unreachable!(),
    }

    let mut arena = new_test_arena();
    let owner = commit_element(&mut arena, Box::new(svg));
    let (loading_root, loading_child) = insert_inactive_slot_subtree(&mut arena, owner, id + 1);
    let (error_root, error_child) = insert_inactive_slot_subtree(&mut arena, owner, id + 0x10);
    arena.with_element_taken(owner, |element, _arena| {
        let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
        svg.attach_loading_slot_cold(vec![loading_root]);
        svg.attach_error_slot_cold(vec![error_root]);
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
    let (active_root, active_child, inactive_root, inactive_child) = match state {
        ActiveSlot::Loading => (loading_root, loading_child, error_root, error_child),
        ActiveSlot::Error => (error_root, error_child, loading_root, loading_child),
        ActiveSlot::None => unreachable!(),
    };
    (
        arena,
        owner,
        active_root,
        active_child,
        inactive_root,
        inactive_child,
    )
}

fn make_svg_wrapper_rounded(arena: &mut NodeArena, owner: NodeKey) {
    arena.with_element_taken(owner, |element, _arena| {
        let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.set_border_radius(BorderRadius::uniform(crate::style::Length::px(12.0)));
        svg.apply_style(style);
    });
    for child in arena.children_of(owner) {
        arena.with_element_taken(child, |element, _arena| {
            if let Some(element) = element.as_any_mut().downcast_mut::<Element>() {
                let mut style = Style::new();
                style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
                element.apply_style(style);
            }
        });
    }
    measure_and_place(
        arena,
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
}













fn unique_svg(marker: &str) -> SvgSource {
    SvgSource::Content(format!(
        r##"<svg width="80" height="40" viewBox="0 0 80 40" xmlns="http://www.w3.org/2000/svg"><rect width="80" height="40" fill="#ff0000"/><desc>{marker}</desc></svg>"##
    ))
}

fn wait_until_document_ready(key: u64) {
    for _ in 0..500 {
        if matches!(
            snapshot_svg_document(key),
            Some(SvgDocumentSnapshot::Ready { .. })
        ) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("SVG document did not become ready");
}

fn wait_until_raster_ready(key: u64) {
    for _ in 0..500 {
        if matches!(snapshot_svg_raster(key), Some(ImageSnapshot::Ready(_))) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("SVG raster did not become ready");
}

fn layout_svg_element(svg: &mut Svg, width: f32, height: f32) {
    let mut style = Style::new();
    style.insert(
        crate::style::PropertyId::Width,
        crate::style::ParsedValue::Length(crate::style::Length::px(width)),
    );
    style.insert(
        crate::style::PropertyId::Height,
        crate::style::ParsedValue::Length(crate::style::Length::px(height)),
    );
    svg.apply_style(style);
    let mut arena = new_test_arena();
    svg.element.measure(
        LayoutConstraints {
            max_width: width,
            max_height: height,
            viewport_width: width,
            viewport_height: height,
            percent_base_width: Some(width),
            percent_base_height: Some(height),
        },
        &mut arena,
    );
    svg.element.place(
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: height,
            viewport_width: width,
            viewport_height: height,
            percent_base_width: Some(width),
            percent_base_height: Some(height),
        },
        &mut arena,
    );
}

fn freeze_ready_svg(id: u64, source: SvgSource, scale: f32) -> Svg {
    let source = match source {
        SvgSource::Content(content) => {
            SvgSource::Content(format!("{content}<!-- m9b2-fixture-{id} -->"))
        }
        SvgSource::Path(path) => SvgSource::Path(std::path::PathBuf::from(format!(
            "{}-m9b2-fixture-{id}",
            path.display()
        ))),
    };
    let primed_document = prime_svg_document_ready_for_test(&source, 80.0, 40.0);
    let mut svg = Svg::new_with_id(id, source);
    assert_eq!(svg.source_key, primed_document);
    layout_svg_element(&mut svg, 80.0, 40.0);
    let mut arena = new_test_arena();
    svg.sync_arena(&mut arena);
    let request = svg
        .resolve_raster_plan(80.0, 40.0, 80.0, 40.0, scale)
        .unwrap()
        .request;
    let pixels: std::sync::Arc<[u8]> = std::sync::Arc::from(vec![
        id as u8;
        (request.physical_width * request.physical_height * 4)
            as usize
    ]);
    let (primed_raster, _) = prime_svg_raster_ready_for_test(svg.source_key, request, pixels);
    svg.prepare_frozen_paint(PaintResourcePreparationContext {
        frame_number: 1,
        device_scale: scale,
        now: Instant::now(),
    });
    assert_eq!(svg.active_raster_key, Some(primed_raster));
    svg.sync_arena(&mut arena);
    svg.prepare_frozen_paint(PaintResourcePreparationContext {
        frame_number: 2,
        device_scale: scale,
        now: Instant::now(),
    });
    assert!(svg.frozen_request_is_exact);
    svg
}

fn prepared_ready_svg_with_inactive_slots(
    id: u64,
) -> (NodeArena, NodeKey, NodeKey, NodeKey, NodeKey, NodeKey) {
    let svg = freeze_ready_svg(id, unique_svg(&format!("ready-inactive-{id}")), 1.0);
    let mut arena = new_test_arena();
    let owner = commit_element(&mut arena, Box::new(svg));
    let (loading_root, loading_child) = insert_inactive_slot_subtree(&mut arena, owner, id + 1);
    let (error_root, error_child) = insert_inactive_slot_subtree(&mut arena, owner, id + 3);
    arena.with_element_taken(owner, |element, _arena| {
        let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
        svg.attach_loading_slot_cold(vec![loading_root]);
        svg.attach_error_slot_cold(vec![error_root]);
    });
    (
        arena,
        owner,
        loading_root,
        loading_child,
        error_root,
        error_child,
    )
}

fn assert_missing_prepared_svg_hooks(arena: &NodeArena, owner: NodeKey) {
    let node = arena.get(owner).unwrap();
    let context = node
        .element
        .shadow_paint_recording_context(Default::default());
    assert!(matches!(
        node.element
            .shadow_paint_recording_capability(arena, false, context),
        ShadowPaintRecordingCapability::Legacy(
            ShadowPaintBlocker::MissingPreparedSvg
                | ShadowPaintBlocker::MissingPreparedInlineRoot
        )
    ));
    let revision = crate::view::paint::PaintContentRevision {
        self_paint_revision: 1,
        composite_revision: 1,
        topology_revision: 1,
    };
    assert!(
        node.element
            .record_shadow_paint_metadata(owner, Default::default(), revision, arena, context,)
            .is_none()
    );
    assert!(
        node.element
            .record_shadow_paint_artifact(owner, Default::default(), revision, arena, context,)
            .is_none()
    );
}

mod retained_paint_tests;
mod layout_tests;
mod paint_recording_tests;
mod slot_lifecycle_tests;
mod shadow_recording_tests;
mod raster_pipeline_tests;
mod source_request_tests;
mod artifact_preflight_tests;
