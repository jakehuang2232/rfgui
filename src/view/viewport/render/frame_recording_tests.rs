use crate::style::{Border, Rotate, Scale, Transition, TransitionProperty, Transitions};
use crate::style::{
    ClipMode, Color, Layout, Length, ParsedValue, Position, PropertyId, ScrollDirection, Style,
    Transform, Translate,
};
use crate::view::base_component::{
    DirtyFlags, DirtyPassMask, Element, ElementTrait, EventTarget, Image, LayoutConstraints,
    LayoutPlacement, Size, Svg, Text, TextArea, UiBuildContext,
};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::frame_graph::FrameGraph;
use crate::view::node_arena::{Node, NodeArena, NodeKey};
use crate::view::test_support::{commit_child, commit_element, measure_and_place, new_test_arena};
use crate::view::{ImageSource, SvgSource, image_resource};
use std::sync::Arc;

use crate::view::paint::PreparedArtifactSurfaceFrame;

use super::{
    ARTIFACT_SURFACE_AGGREGATE_BUDGET_BYTES, FrameDisposition, RecordedFrame, Viewport,
    artifact_surface_raster_context, compile_recorded_frame, finish_frame_dirty_lifecycle,
    frame_disposition, record_frame_for_test, should_store_compile_cache,
};

fn constraints() -> (LayoutConstraints, LayoutPlacement) {
    (
        LayoutConstraints {
            max_width: 320.0,
            max_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 320.0,
            available_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        },
    )
}

fn colored_element(id: u64, x: f32, color: Color) -> Element {
    let mut element = Element::new_with_id(id, x, 20.0, 80.0, 40.0);
    let mut style = Style::new();
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn prepared_safe_leaf() -> (NodeArena, Vec<NodeKey>) {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(colored_element(1, 10.0, Color::rgb(230, 20, 30))),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

fn prepared_native_text() -> (NodeArena, Vec<NodeKey>) {
    prepared_native_text_with_opacity(1.0)
}

fn prepared_native_text_with_opacity(opacity: f32) -> (NodeArena, Vec<NodeKey>) {
    let mut arena = new_test_arena();
    let mut text = Text::new_with_id(
        0xd3_a010,
        3.25,
        5.5,
        180.0,
        48.0,
        "native Text retained closure",
    );
    text.set_font("sans-serif");
    text.set_font_size(18.0);
    text.set_color(Color::rgb(30, 80, 210));
    text.set_opacity(opacity);
    let root = commit_element(&mut arena, Box::new(text));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

fn prepared_transparent_native_text() -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let mut text = Text::new_with_id(
        0xd3_a011,
        3.25,
        5.5,
        180.0,
        48.0,
        "transparent native Text retained closure",
    );
    text.set_font("sans-serif");
    text.set_font_size(18.0);
    text.set_color(Color::rgb(30, 80, 210));
    text.set_opacity(0.0);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(text));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root], root)
}

fn prepared_empty_native_text() -> (NodeArena, Vec<NodeKey>) {
    let mut arena = new_test_arena();
    let mut text = Text::new_with_id(0xd3_a011, 3.25, 5.5, 180.0, 48.0, "");
    text.set_font("sans-serif");
    text.set_font_size(18.0);
    let root = commit_element(&mut arena, Box::new(text));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

fn prepared_native_image() -> (NodeArena, Vec<NodeKey>) {
    prepared_native_image_with_opacity(1.0)
}

fn prepared_native_image_with_opacity(opacity: f32) -> (NodeArena, Vec<NodeKey>) {
    let mut image = Image::new_with_id(
        0xd3_a020,
        ImageSource::Rgba {
            width: 2,
            height: 2,
            pixels: Arc::from([
                255_u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ]),
        },
    );
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(48.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(32.0)));
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(crate::style::Opacity::new(opacity)),
    );
    image.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(image));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

fn prepared_native_image_path_state(
    label: &str,
    opacity: f32,
    error: bool,
) -> (NodeArena, Vec<NodeKey>) {
    let source = ImageSource::Path(std::path::PathBuf::from(format!(
        "/rfgui-retained-root-opacity-{label}.png"
    )));
    let handle = image_resource::acquire_image_resource(&source);
    let mut image = Image::new_with_id(0xd3_a021, source);
    if error {
        image_resource::set_image_error_for_test(
            handle.asset_id(),
            "synthetic root-opacity image error",
        );
    } else {
        image_resource::set_image_loading_for_test(handle.asset_id());
    }
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(48.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(32.0)));
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(crate::style::Opacity::new(opacity)),
    );
    image.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(image));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

#[cfg(not(target_arch = "wasm32"))]
fn prepared_native_svg() -> (NodeArena, Vec<NodeKey>) {
    prepared_native_svg_with_opacity(1.0)
}

#[cfg(not(target_arch = "wasm32"))]
fn prepared_native_svg_with_opacity(opacity: f32) -> (NodeArena, Vec<NodeKey>) {
    const SVG: &str = r##"<svg width="24" height="18" viewBox="0 0 24 18" xmlns="http://www.w3.org/2000/svg"><rect width="24" height="18" fill="#22c55e"/></svg>"##;
    let mut svg = Svg::new_with_id(0xd3_a030, SvgSource::Content(SVG.to_string()));
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(48.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(36.0)));
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(crate::style::Opacity::new(opacity)),
    );
    svg.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(svg));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    arena.with_element_taken(root, |element, _arena| {
        element
            .as_any_mut()
            .downcast_mut::<Svg>()
            .expect("native Svg fixture")
            .prepare_content_paint_for_test(SVG, (24.0, 18.0), 1.0)
            .expect("prepare exact native Svg raster");
    });
    (arena, vec![root])
}

#[cfg(not(target_arch = "wasm32"))]
fn prepared_native_svg_path_state(
    label: &str,
    opacity: f32,
    error: bool,
) -> (NodeArena, Vec<NodeKey>) {
    let source = SvgSource::Path(std::path::PathBuf::from(format!(
        "/rfgui-retained-root-opacity-{label}.svg"
    )));
    let source_key = crate::view::svg_resource::acquire_svg_document(&source);
    let mut svg = Svg::new_with_id(0xd3_a031, source);
    if error {
        crate::view::svg_resource::set_svg_document_error_for_test(source_key);
    } else {
        crate::view::svg_resource::set_svg_document_loading_for_test(source_key);
    }
    crate::view::svg_resource::release_svg_document(source_key);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(48.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(36.0)));
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(crate::style::Opacity::new(opacity)),
    );
    svg.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(svg));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

#[cfg(not(target_arch = "wasm32"))]
fn prepared_native_media_transform(host: &str, state: &str) -> (NodeArena, Vec<NodeKey>) {
    const SVG: &str = r##"<svg width="24" height="18" viewBox="0 0 24 18" xmlns="http://www.w3.org/2000/svg"><rect width="24" height="18" fill="#0ea5e9"/></svg>"##;
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(48.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(36.0)));
    style.set_transform(Transform::new([Translate::x(Length::px(7.0))]));
    let stable_id = if host == "Image" {
        0xd3_a040
    } else {
        0xd3_a050
    } + match state {
        "ready" => 0,
        "loading" => 1,
        "error" => 2,
        _ => panic!("unknown native media state"),
    };
    let native: Box<dyn ElementTrait> = match host {
        "Image" => {
            let source = if state == "ready" {
                ImageSource::Rgba {
                    width: 1,
                    height: 1,
                    pixels: Arc::from([14_u8, 165, 233, 255]),
                }
            } else {
                ImageSource::Path(format!("/rfgui-retained-transform-image-{state}.png").into())
            };
            let mut image = Image::new_with_id(stable_id, source);
            image.apply_style(style);
            match state {
                "loading" => image.set_resource_loading_for_test(),
                "error" => image.set_resource_error_for_test(),
                _ => {}
            }
            Box::new(image)
        }
        "Svg" => {
            let source = if state == "ready" {
                SvgSource::Content(SVG.into())
            } else {
                SvgSource::Path(format!("/rfgui-retained-transform-svg-{state}.svg").into())
            };
            let mut svg = Svg::new_with_id(stable_id, source);
            svg.apply_style(style);
            match state {
                "loading" => svg.set_document_loading_for_transform_test(),
                "error" => svg.set_document_error_for_transform_test(),
                _ => {}
            }
            Box::new(svg)
        }
        _ => panic!("unknown native media host"),
    };
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, native);
    arena
        .with_element_taken(root, |element, arena| element.sync_arena(arena))
        .expect("freeze native media transform state");
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    if host == "Svg" && state == "ready" {
        arena.with_element_taken(root, |element, _arena| {
            element
                .as_any_mut()
                .downcast_mut::<Svg>()
                .expect("native transformed Svg")
                .prepare_content_paint_for_test(SVG, (24.0, 18.0), 1.0)
                .expect("prepare transformed Svg raster");
        });
    }
    (arena, vec![root])
}

#[cfg(not(target_arch = "wasm32"))]
fn prepared_nested_native_effect(host: &str, state: &str) -> (NodeArena, Vec<NodeKey>, NodeKey) {
    const SVG: &str = r##"<svg width="18" height="14" viewBox="0 0 18 14" xmlns="http://www.w3.org/2000/svg"><rect width="18" height="14" fill="#22c55e"/></svg>"##;
    let mut parent = Element::new_with_id(0xd3_a060, 0.0, 0.0, 64.0, 40.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    parent_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(15, 23, 42)),
    );
    parent.apply_style(parent_style);

    let mut child_style = Style::new();
    child_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(18.0)));
    child_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(14.0)));
    child_style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(crate::style::Opacity::new(0.5)),
    );
    let child_id = match host {
        "Text" => 0xd3_a061,
        "Image" => 0xd3_a062,
        "Svg" => 0xd3_a063,
        _ => panic!("unknown nested native host"),
    };
    let child: Box<dyn ElementTrait> = match host {
        "Text" => {
            let mut text = Text::new_with_id(child_id, 0.0, 0.0, 18.0, 14.0, "nested");
            text.set_opacity(0.5);
            Box::new(text)
        }
        "Image" => {
            let source = if state == "ready" {
                ImageSource::Rgba {
                    width: 1,
                    height: 1,
                    pixels: Arc::from([34_u8, 197, 94, 255]),
                }
            } else {
                ImageSource::Path(format!("/rfgui-retained-effect-image-{state}.png").into())
            };
            let mut image = Image::new_with_id(child_id, source);
            image.apply_style(child_style);
            match state {
                "loading" => image.set_resource_loading_for_test(),
                "error" => image.set_resource_error_for_test(),
                _ => {}
            }
            Box::new(image)
        }
        "Svg" => {
            let source = if state == "ready" {
                SvgSource::Content(SVG.into())
            } else {
                SvgSource::Path(format!("/rfgui-retained-effect-svg-{state}.svg").into())
            };
            let mut svg = Svg::new_with_id(child_id, source);
            svg.apply_style(child_style);
            match state {
                "loading" => svg.set_document_loading_for_transform_test(),
                "error" => svg.set_document_error_for_transform_test(),
                _ => {}
            }
            Box::new(svg)
        }
        _ => unreachable!(),
    };

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(parent));
    let child = commit_child(&mut arena, root, child);
    arena
        .with_element_taken(child, |element, arena| element.sync_arena(arena))
        .expect("freeze nested native effect state");
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    if host == "Svg" && state == "ready" {
        arena.with_element_taken(child, |element, _arena| {
            element
                .as_any_mut()
                .downcast_mut::<Svg>()
                .expect("nested effect Svg")
                .prepare_content_paint_for_test(SVG, (18.0, 14.0), 1.0)
                .expect("prepare nested effect Svg raster");
        });
    }
    (arena, vec![root], child)
}

fn prepared_auto_text_area(
    scroll_y: f32,
    pending_caret_scroll: bool,
) -> (NodeArena, Vec<NodeKey>, NodeKey) {
    prepared_auto_text_area_with_content(
        "RetainedAuto must admit bounded internal TextArea scrolling without a ScrollNode",
        scroll_y,
        pending_caret_scroll,
    )
}

fn prepared_auto_text_area_with_content(
    content: &str,
    scroll_y: f32,
    pending_caret_scroll: bool,
) -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let width = 108.0;
    let height = 28.0;
    let mut arena = new_test_arena();
    let mut text_area = TextArea::with_stable_id(0xd3_a100);
    text_area.set_text(content.to_string());
    text_area.font_size = 17.5;
    text_area.line_height = 1.3;
    let root = commit_element(&mut arena, Box::new(text_area));
    arena.with_element_taken(root, |element, _arena| {
        element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .set_self_node_key(root);
    });
    let measure = LayoutConstraints {
        max_width: width,
        max_height: height,
        viewport_width: 320.0,
        viewport_height: 240.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(240.0),
    };
    let place = LayoutPlacement {
        parent_x: 7.25,
        parent_y: 11.75,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: width,
        available_height: height,
        viewport_width: 320.0,
        viewport_height: 240.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(240.0),
    };
    measure_and_place(&mut arena, root, measure, place);
    arena.with_element_taken(root, |element, arena| {
        {
            let text_area = element.as_any_mut().downcast_mut::<TextArea>().unwrap();
            let max_y = (text_area.layout_state.content_size.height
                - text_area.viewport_size.height)
                .max(0.0);
            assert!(scroll_y.is_nan() || scroll_y <= max_y);
            text_area.scroll_y = scroll_y;
        }
        if scroll_y.is_finite() {
            element.place(place, arena);
        }
        element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .pending_caret_scroll = pending_caret_scroll;
    });
    let mut stack = vec![root];
    while let Some(key) = stack.pop() {
        stack.extend(arena.children_of(key));
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    (arena, vec![root], root)
}

fn prepared_transform_child_isolation_tree() -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey)
{
    let mixed_element = |id, x, y, width, height, color| {
        let mut element = Element::new_with_id(id, x, y, width, height);
        let mut style = Style::new();
        style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(crate::style::Layout::Grid),
        );
        style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
        element.apply_style(style);
        element
    };
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(mixed_element(
            0xd1_b100,
            0.25,
            0.25,
            40.0,
            24.0,
            Color::rgb(20, 40, 80),
        )),
    );
    let child = commit_child(
        &mut arena,
        root,
        Box::new(mixed_element(
            0xd1_b101,
            4.25,
            1.5,
            18.0,
            10.0,
            Color::rgb(180, 60, 20),
        )),
    );
    let descendant = commit_child(
        &mut arena,
        child,
        Box::new(mixed_element(
            0xd1_b102,
            5.0,
            1.75,
            1.0,
            1.0,
            Color::rgb(200, 160, 20),
        )),
    );
    let (measure, mut place) = constraints();
    place.parent_x = 0.25;
    place.parent_y = 0.25;
    measure_and_place(&mut arena, root, measure, place);
    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            100.0, 0.0, 0.0,
        ))));
    crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.5);
    (arena, vec![root], root, child, descendant)
}

fn prepared_transform_scroll_scene(
    matrix: glam::Mat4,
) -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xe2_c300, 0.0, 0.0, 120.0, 90.0,
    ))));
    let scroll = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xe2_c301, 0.0, 0.0, 120.0, 90.0,
    ))));
    let content = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xe2_c302, 0.0, -20.0, 120.0, 240.0,
    ))));
    arena.set_parent(scroll, Some(root));
    arena.push_child(root, scroll);
    arena.set_parent(content, Some(scroll));
    arena.push_child(scroll, content);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    crate::view::test_support::get_element_mut::<Element>(&arena, root).apply_style(root_style);
    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_resolved_transform_for_test(Some(matrix));
    let mut scroll_style = Style::new();
    scroll_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    scroll_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, scroll);
        element.apply_style(scroll_style);
        element.layout_state.content_size = Size {
            width: 120.0,
            height: 240.0,
        };
        element.set_scroll_offset((0.0, 20.0));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    crate::view::test_support::get_element_mut::<Element>(&arena, content)
        .set_background_color_value(Color::rgb(24, 48, 72));
    arena
        .get_mut(content)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    arena.refresh_subtree_dirty_cache(root);
    let roots = vec![root];
    let (properties, generations) = synced_paint_state(&arena, &roots);
    (arena, roots, properties, generations)
}

fn prepared_same_owner_transform_scroll_scene() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (mut arena, roots, _, _) = prepared_transform_scroll_scene(glam::Mat4::from_translation(
        glam::Vec3::new(7.0, 5.0, 0.0),
    ));
    let root = roots[0];
    let scroll = arena.children_of(root)[0];
    let content = arena.children_of(scroll)[0];
    arena.set_children(root, vec![content]);
    arena.set_parent(content, Some(root));
    arena.set_parent(scroll, None);
    let mut root_style = Style::new();
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        element.apply_style(root_style);
        element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
            glam::Vec3::new(7.0, 5.0, 0.0),
        )));
        element.layout_state.content_size = Size {
            width: 120.0,
            height: 240.0,
        };
        element.set_scroll_offset((0.0, 20.0));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(root);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    (arena, roots, properties, generations)
}

fn prepared_same_owner_effect_scroll_scene() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (arena, roots, _, _) = prepared_same_owner_transform_scroll_scene();
    let root = roots[0];
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        element.set_resolved_transform_for_test(None);
        element.set_opacity(0.625);
    }
    arena.refresh_subtree_dirty_cache(root);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    (arena, roots, properties, generations)
}

fn prepared_transform_effect_scroll_scene() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (mut arena, roots, _, _) = prepared_transform_scroll_scene(glam::Mat4::from_translation(
        glam::Vec3::new(3.0, 0.0, 0.0),
    ));
    let transform_root = roots[0];
    let scroll = arena.children_of(transform_root)[0];
    let mut effect = Element::new_with_id(0xe2_c3f0, 0.0, 0.0, 120.0, 90.0);
    let mut effect_style = Style::new();
    effect_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    effect.apply_style(effect_style);
    effect.set_opacity(0.5);
    let effect = arena.insert(Node::new(Box::new(effect)));
    arena.set_parent(effect, Some(transform_root));
    arena.set_children(transform_root, vec![effect]);
    arena.set_parent(scroll, Some(effect));
    arena.set_children(effect, vec![scroll]);
    arena.refresh_subtree_dirty_cache(transform_root);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    (arena, roots, properties, generations)
}

fn prepared_exact_scroll_scene() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xe2_a300, 0.0, 0.0, 100.0, 80.0,
    ))));
    let child = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xe2_a301, 0.0, -20.0, 100.0, 300.0,
    ))));
    arena.set_parent(child, Some(root));
    arena.push_child(root, child);
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut root_node = arena.get_mut(root).expect("scroll root");
        let root_element = root_node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("Element scroll root");
        root_element.apply_style(style);
        root_element.layout_state.content_size = Size {
            width: 100.0,
            height: 300.0,
        };
        root_element.set_scroll_offset((0.0, 20.0));
        root_element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena
        .get_mut(child)
        .expect("scroll content")
        .element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    arena.refresh_subtree_dirty_cache(root);
    let (properties, generations) = synced_paint_state(&arena, &[root]);
    (arena, vec![root], properties, generations)
}

fn prepared_exact_nested_scroll_scene() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (arena, outer, _inner, _leaf, properties, generations) =
        crate::view::paint::nested_scroll_plan_fixture();
    (arena, vec![outer], properties, generations)
}

fn synced_paint_state(
    arena: &NodeArena,
    roots: &[NodeKey],
) -> (PropertyTrees, PaintGenerationTracker) {
    let mut properties = PropertyTrees::default();
    properties.sync(arena, roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(arena, roots, &properties);
    (properties, generations)
}

fn assert_generic_primary(
    arena: &NodeArena,
    roots: &[NodeKey],
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
    ctx: &UiBuildContext,
) {
    let RecordedFrame { frame, .. } =
        record_frame_for_test(arena, roots, properties, generations, ctx);
    let count = frame.raster_plan().nodes().len();
    let mut viewport = Viewport::new();
    assert_eq!(
        emit_selected_artifact_surface(
            "generic primary",
            &mut viewport,
            frame,
            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone())
        ),
        count
    );
}

fn assert_native_host_retained_closure(host: &str, arena: &NodeArena, roots: &[NodeKey]) {
    let (properties, generations) = synced_paint_state(arena, roots);
    let record = |mode| {
        crate::view::paint::record_coverage_manifest(
            arena,
            roots,
            true,
            mode,
            &properties,
            &generations,
        )
    };
    let metadata = record(crate::view::paint::CoverageRecordingMode::MetadataOnly);
    let full = record(crate::view::paint::CoverageRecordingMode::FullArtifact);

    assert!(
        crate::view::paint::canonical_manifest_matches_for_test(&metadata, &full),
        "{host} metadata/full manifests must be canonical"
    );
}

fn assert_native_root_opacity_artifact(
    host: &str,
    arena: &NodeArena,
    roots: &[NodeKey],
    opacity: f32,
) {
    let root = roots[0];
    let (properties, generations) = synced_paint_state(arena, roots);
    let metadata = crate::view::paint::record_coverage_manifest(
        arena,
        roots,
        true,
        crate::view::paint::CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
    );
    let full = crate::view::paint::record_coverage_manifest(
        arena,
        roots,
        true,
        crate::view::paint::CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
    );
    assert!(
        crate::view::paint::canonical_manifest_matches_for_test(&metadata, &full),
        "{host}: metadata/full canonical identity"
    );

    if opacity != 1.0 {
        assert_eq!(
            properties.effects[&crate::view::compositor::property_tree::EffectNodeId(root)]
                .opacity
                .to_bits(),
            opacity.to_bits()
        );
    }
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let RecordedFrame { frame, .. } =
        record_frame_for_test(arena, roots, &properties, &generations, &ctx);
    let resident_count = frame.residents().len();
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let mut ctx = ctx;
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    compile_recorded_frame(&mut viewport, Some(owner), &mut graph, frame, &ctx);
    assert_eq!(
        viewport
            .pending_artifact_surface_resident_keys_for_test()
            .unwrap()
            .len(),
        resident_count
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
}

fn assert_native_artifact_surface_authority(
    host: &str,
    arena: &NodeArena,
    roots: &[NodeKey],
    emit_transaction: bool,
) {
    let recording_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (properties, generations) = synced_paint_state(arena, roots);
    let RecordedFrame { frame, .. } =
        record_frame_for_test(arena, roots, &properties, &generations, &recording_ctx);
    assert!(
        !frame.raster_plan().nodes().is_empty(),
        "{host}: detached surfaces"
    );
    assert!(frame.raster_plan().nodes().iter().all(|node| matches!(
        node.identity().role,
        crate::view::paint::RetainedSurfaceRasterRole::Transform
            | crate::view::paint::RetainedSurfaceRasterRole::PropertyEffect
    )));
    if !emit_transaction {
        return;
    }

    let mut viewport = Viewport::new();
    let frame_owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("native artifact surface frame stage");
    let mut graph = FrameGraph::new();
    let mut execution_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = execution_ctx.allocate_target(&mut graph);
    execution_ctx.set_current_target(target);
    compile_recorded_frame(
        &mut viewport,
        Some(frame_owner),
        &mut graph,
        frame,
        &execution_ctx,
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true));
}

fn emit_selected_artifact_surface(
    host: &str,
    viewport: &mut Viewport,
    frame: PreparedArtifactSurfaceFrame,
    ctx: UiBuildContext,
) -> usize {
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .unwrap_or_else(|| panic!("{host}: begin artifact resident transaction"));
    let mut graph = FrameGraph::new();
    let mut ctx = ctx;
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    compile_recorded_frame(viewport, Some(owner), &mut graph, frame, &ctx);
    let pending = viewport
        .pending_artifact_surface_resident_keys_for_test()
        .unwrap_or_else(|| panic!("{host}: artifact transaction must be staged"));
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    pending.len()
}

fn observe_compositor_state(
    arena: &NodeArena,
    roots: &[NodeKey],
    properties: &mut PropertyTrees,
    generations: &mut PaintGenerationTracker,
) {
    properties.sync(arena, roots);
    generations.sync(arena, roots, properties);
}

fn set_opacity_with_invalidation(arena: &mut NodeArena, key: NodeKey, opacity: f32) {
    arena
        .mutate_element_with_invalidation(key, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<Element>()
                .expect("test root should be Element")
                .set_opacity_with_invalidation(opacity, cx);
        })
        .expect("test root should exist");
}

fn assert_consumed_dirty_cleared(arena: &NodeArena, key: NodeKey) {
    let consumed = DirtyFlags::PAINT.union(DirtyFlags::COMPOSITE);
    assert!(
        !arena
            .get(key)
            .expect("test node should exist")
            .element
            .local_dirty_flags()
            .intersects(consumed)
    );
    assert!(!arena.arena_local_dirty(key).intersects(consumed));
    assert!(!arena.cached_subtree_dirty(key).intersects(consumed));
}

fn assert_composite_dirty_preserved(arena: &NodeArena, key: NodeKey) {
    assert!(
        arena
            .get(key)
            .expect("test node should exist")
            .element
            .local_dirty_flags()
            .contains(DirtyFlags::COMPOSITE)
    );
    assert!(arena.arena_local_dirty(key).contains(DirtyFlags::COMPOSITE));
    assert!(
        arena
            .cached_subtree_dirty(key)
            .contains(DirtyFlags::COMPOSITE)
    );
}

mod composite_dirty_tests;
mod frame_failure_tests;
mod generic_selection_tests;
mod native_authority_tests;
#[cfg(not(target_arch = "wasm32"))]
mod native_component_inventory_tests;
mod nested_scroll_tests;
mod property_boundary_forest_branching_tests;
mod property_boundary_forest_depth_three_tests;
mod property_boundary_forest_linear_tests;
mod property_boundary_forest_multi_root_tests;
mod property_boundary_forest_plain_root_tests;
mod property_boundary_forest_tests;
#[cfg(not(target_arch = "wasm32"))]
mod stage_c_surface_dag_producer_tests;
#[cfg(not(target_arch = "wasm32"))]
mod stage_c_surface_materialization_tests;
mod stage_c_zero_surface_v2_tests;
mod text_transform_tests;

fn prepared_sampled_inline_span_layout_transition() -> (NodeArena, Vec<NodeKey>) {
    let mut arena = new_test_arena();
    let mut parent = Element::new_with_id(0xe2_a180, 0.0, 0.0, 92.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(92.0)));
    parent.apply_style(parent_style);
    let parent_key = commit_element(&mut arena, Box::new(parent));

    let mut span = Element::new_with_id(0xe2_a181, 0.0, 0.0, 0.0, 0.0);
    let mut span_style = Style::new();
    span_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    span_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#bfdbfe")),
    );
    span_style.set_border(Border::uniform(Length::px(2.0), &Color::hex("#2563eb")));
    span.apply_style(span_style);
    let span_key = commit_child(&mut arena, parent_key, Box::new(span));
    commit_child(
        &mut arena,
        span_key,
        Box::new(Text::new_with_id(
            0xe2_a182,
            0.0,
            0.0,
            0.0,
            0.0,
            "alpha beta gamma delta epsilon zeta",
        )),
    );

    let (mut measure, mut place) = constraints();
    measure.max_width = 92.0;
    measure.max_height = 220.0;
    place.available_width = 92.0;
    place.available_height = 220.0;
    measure_and_place(&mut arena, parent_key, measure, place);
    {
        let mut node = arena.get_mut(span_key).unwrap();
        let span = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
        assert!(
            span.inline_fragment_rects().len() >= 2,
            "fixture must exercise a wrapping inline-owned Element"
        );
        span.set_layout_transition_width(71.0);
        span.set_layout_transition_height(39.0);
        span.clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(span_key, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(span_key);
    (arena, vec![span_key])
}

fn prepared_native_text_transform(
    transform: Transform,
    nested: bool,
    sampled_parent: bool,
) -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let mut text = Text::new_with_id(0xd3_a012, 3.0, 5.0, 96.0, 28.0, "native Text transform");
    text.set_font("sans-serif");
    text.set_font_size(18.0);
    text.set_color(Color::rgb(30, 80, 210));
    text.set_transform(transform);
    let mut arena = new_test_arena();
    if !nested {
        let root = commit_element(&mut arena, Box::new(text));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        return (arena, vec![root], root);
    }

    let mut parent = Element::new_with_id(0xd3_a013, 0.0, 0.0, 180.0, 72.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    if sampled_parent {
        parent_style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Width,
                200,
            ))),
        );
    }
    parent.apply_style(parent_style);
    let root = commit_element(&mut arena, Box::new(parent));
    let child = commit_child(&mut arena, root, Box::new(text));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    if sampled_parent {
        let mut node = arena.get_mut(root).expect("sampled Text transform parent");
        let parent = node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("Element parent");
        parent.set_layout_transition_width(164.0);
        parent.set_layout_transition_height(64.0);
        drop(node);
        measure_and_place(&mut arena, root, measure, place);
    }
    (arena, vec![root], child)
}

fn selected_artifact_surface(
    host: &str,
    arena: &NodeArena,
    roots: &[NodeKey],
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
    ctx: &UiBuildContext,
) -> (PreparedArtifactSurfaceFrame, usize) {
    let RecordedFrame { frame, .. } =
        record_frame_for_test(arena, roots, properties, generations, ctx);
    let surface_count = frame.raster_plan().nodes().len();
    assert_ne!(surface_count, 0, "{host}: detached surface count");
    assert!(frame.raster_plan().nodes().iter().all(|node| matches!(
        node.identity().role,
        crate::view::paint::RetainedSurfaceRasterRole::Transform
            | crate::view::paint::RetainedSurfaceRasterRole::PropertyEffect
    )));
    (frame, surface_count)
}
