//! Diagnostic baseline for native hosts, not a permanent rejection allowlist.
//! Use actual layout + property sync + the production Auto selector (record,
//! plan, seal). No GPU execution, pixel correctness or reuse is claimed here.
//! When a normal-state gap is fixed, retain its scene and update its expectation.
//! BeforePreparation rows deliberately call the low-level selector between a
//! source mutation and frame preparation. Their rejection guards stale data;
//! it is not an acceptable fallback outcome for a complete production frame.
use super::*;
use crate::view::paint::{FrameArtifactDebugBoundaryKind, LegacyPaintReason};
use crate::view::test_support::get_element_mut;

#[derive(Clone, Copy, Debug)]
enum Status {
    Supported,
    BeforePreparation,
    InvalidSnapshot,
}

struct Scene {
    arena: NodeArena,
    root: NodeKey,
    viewport: Viewport,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
    dpr: f32,
    frame_number: u64,
}

impl Scene {
    fn new(element: Box<dyn ElementTrait>, dpr: f32) -> Self {
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, element);
        if let Some(area) = arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
        {
            area.set_self_node_key(root);
        }
        Self {
            arena,
            root,
            viewport: Viewport::new(),
            properties: Default::default(),
            generations: Default::default(),
            dpr,
            frame_number: 0,
        }
    }

    fn layout(&mut self) {
        self.viewport.set_scale_factor(self.dpr);
        crate::view::viewport::layout_artifact_style_scene_for_test(
            &mut self.viewport,
            &mut self.arena,
            self.root,
            [320.0, 240.0],
        );
        self.frame_number += 1;
        // Same order as render_render_tree: final layout, registered resource
        // preparation, property/generation observation, then Auto selection.
        self.arena.prepare_registered_paint_resources(
            crate::view::base_component::PaintResourcePreparationContext {
                frame_number: self.frame_number,
                device_scale: self.dpr,
                now: crate::time::Instant::now(),
            },
        );
        self.sync();
    }

    fn sync(&mut self) {
        self.arena.refresh_subtree_dirty_cache(self.root);
        self.properties.sync(&self.arena, &[self.root]);
        self.generations
            .sync(&self.arena, &[self.root], &self.properties);
    }

    fn observe(
        &self,
        state: &str,
        status: Status,
        expected: Option<LegacyPaintReason>,
    ) -> [usize; 7] {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, self.dpr);
        let selected = select_retained_auto_authority(
            &self.arena,
            &[self.root],
            &self.properties,
            &self.generations,
            &ctx,
            true,
        );
        let node = self.arena.get(self.root).unwrap();
        let host = node.element.element_type_name();
        let selected_name = if matches!(selected, AutoAuthorityDecision::Artifact { .. }) {
            "Artifact"
        } else {
            "Legacy"
        };
        let mut commands = [0; 7];
        if let AutoAuthorityDecision::Artifact { candidate, .. } = &selected {
            let RecordedArtifactPayload::ArtifactSurface(frame) = &candidate.payload;
            assert!(frame.is_canonical());
            let plan = frame.raster_plan();
            for step in plan
                .roots()
                .iter()
                .flat_map(|root| root.steps())
                .chain(plan.nodes().iter().flat_map(|node| node.steps()))
            {
                if let crate::view::paint::PreparedArtifactSurfaceRasterStep::ArtifactSpan(span) =
                    step
                {
                    for op in span.chunks().iter().flat_map(|chunk| chunk.localized_ops()) {
                        use crate::view::paint::PaintOp;
                        commands[match op {
                            PaintOp::DrawRect(_) => 0,
                            PaintOp::PreparedInlineIfcDecoration(_) => 1,
                            PaintOp::PreparedShadow(_) => 2,
                            PaintOp::PreparedScrollbarOverlay(_) => 3,
                            PaintOp::PreparedText(_) => 4,
                            PaintOp::PreparedImage(_) => 5,
                            PaintOp::PreparedSvg(_) => 6,
                        }] += 1;
                    }
                }
            }
        }
        let trace = auto_authority_trace(&selected);
        println!(
            "NATIVE_INVENTORY\t{host}\t{state}\t{}\t{status:?}\t{selected_name}\towner={:?}\tstable_id={}\tcommands={commands:?}\t{:?}",
            self.dpr,
            self.root,
            node.element.stable_id(),
            trace.rejections
        );
        // Capture is optional telemetry; it must not affect the actual decision.
        let quiet = select_retained_auto_authority(
            &self.arena,
            &[self.root],
            &self.properties,
            &self.generations,
            &ctx,
            false,
        );
        assert_eq!(
            std::mem::discriminant(&selected),
            std::mem::discriminant(&quiet)
        );
        match expected {
            None => {
                assert!(
                    matches!(selected, AutoAuthorityDecision::Artifact { .. }),
                    "{host}/{state}: {:?}",
                    trace.rejections
                );
                assert!(trace.rejections.is_empty());
            }
            Some(reason) => {
                let [AutoAuthorityRejection::Artifact { eligibility }] =
                    trace.rejections.as_slice()
                else {
                    panic!(
                        "{host}/{state}: expected recording rejection {reason:?}, got {:?}",
                        trace.rejections
                    );
                };
                assert!(matches!(selected, AutoAuthorityDecision::Legacy { .. }));
                assert_eq!(
                    eligibility.reasons,
                    vec![crate::view::paint::FrameArtifactFallbackReason::LegacyBoundary(reason)],
                    "{host}/{state}"
                );
                assert_eq!(eligibility.debug_boundaries.len(), 1, "{host}/{state}");
                assert_eq!(eligibility.debug_boundaries[0].owner, self.root);
                assert_eq!(
                    eligibility.debug_boundaries[0].kind,
                    FrameArtifactDebugBoundaryKind::Legacy(reason)
                );
            }
        }
        commands
    }
}

fn box_style(width: f32, height: f32) -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(200, 40, 20)),
    );
    style
}

fn element(id: u64, width: f32, height: f32) -> Element {
    let mut element = Element::new_with_id(id, 0.0, 0.0, width, height);
    element.apply_style(box_style(width, height));
    element
}

#[test]
fn native_inventory_element_inline_clip_and_deferred_states() {
    for dpr in [1.0, 2.0] {
        for state in ["inline-text", "rounded-child-clip", "deferred-child"] {
            let mut scene = Scene::new(Box::new(element(0xa160, 48.0, 40.0)), dpr);
            let mut style = box_style(48.0, 40.0);
            if state == "inline-text" {
                style.insert(
                    PropertyId::Layout,
                    ParsedValue::Layout(Layout::flow().into()),
                );
                let mut text =
                    Text::new_with_id(0xa161, 0.0, 0.0, 120.0, 24.0, "inline native text");
                text.set_font("sans-serif");
                commit_child(&mut scene.arena, scene.root, Box::new(text));
            } else {
                let mut child = element(0xa161, 80.0, 60.0);
                if state == "deferred-child" {
                    let mut child_style = box_style(80.0, 60.0);
                    child_style.insert(
                        PropertyId::Position,
                        ParsedValue::Position(Position::absolute().clip(ClipMode::Viewport)),
                    );
                    child.apply_style(child_style);
                } else {
                    style.set_border_radius(crate::style::BorderRadius::uniform(Length::px(8.0)));
                }
                commit_child(&mut scene.arena, scene.root, Box::new(child));
            }
            get_element_mut::<Element>(&scene.arena, scene.root).apply_style(style);
            scene.layout();
            let commands = scene.observe(state, Status::Supported, None);
            if state == "inline-text" {
                assert!(commands[4] > 0, "inline scene must record glyphs");
            }
            if state == "deferred-child" {
                let child = scene.arena.children_of(scene.root)[0];
                assert!(
                    scene
                        .arena
                        .get(child)
                        .unwrap()
                        .element
                        .is_deferred_to_root_viewport_render()
                );
            }
            if state == "rounded-child-clip" {
                assert!(
                    scene
                        .arena
                        .get(scene.root)
                        .unwrap()
                        .element
                        .retained_child_mask_plan(&scene.arena, Default::default())
                        .is_some(),
                    "mask fixture must require a recorded child mask"
                );
            }
        }
    }
}

#[test]
fn native_inventory_element_scroll_states() {
    for dpr in [1.0, 2.0] {
        for (name, child_height, scroll, status, reason) in [
            ("plain", None, false, Status::Supported, None),
            (
                "scroll-empty",
                None,
                true,
                Status::Supported,
                None,
            ),
            (
                "scroll-fitting",
                Some(20.0),
                true,
                Status::Supported,
                None,
            ),
            (
                "scroll-overflowing",
                Some(120.0),
                true,
                Status::Supported,
                None,
            ),
        ] {
            let mut root = element(0xa110, 48.0, 40.0);
            let mut style = box_style(48.0, 40.0);
            if scroll {
                style.insert(
                    PropertyId::ScrollDirection,
                    ParsedValue::ScrollDirection(ScrollDirection::Vertical),
                );
            }
            root.apply_style(style);
            let mut scene = Scene::new(Box::new(root), dpr);
            if let Some(height) = child_height {
                commit_child(
                    &mut scene.arena,
                    scene.root,
                    Box::new(element(0xa111, 20.0, height)),
                );
            }
            scene.layout();
            assert_eq!(
                scene.properties.scrolls.len(),
                usize::from(child_height == Some(120.0))
            );
            scene.observe(name, status, reason);
        }
    }
}

#[test]
fn native_inventory_text_preparation_states() {
    for dpr in [1.0, 2.0] {
        for (name, text) in [("empty", ""), ("visible", "Native retained text")] {
            let mut text = Text::new_with_id(0xa120, 0.0, 0.0, 180.0, 32.0, text);
            text.set_font("sans-serif");
            text.set_font_size(16.0);
            let mut scene = Scene::new(Box::new(text), dpr);
            scene.layout();
            scene.observe(name, Status::Supported, None);
            if name == "visible" {
                // Deliberate missing payload, not a normal post-layout state.
                get_element_mut::<Text>(&scene.arena, scene.root)
                    .clear_prepared_standalone_text_for_test();
                scene.sync();
                scene.observe(
                    "missing-prepared-text-injected",
                    Status::InvalidSnapshot,
                    Some(LegacyPaintReason::MissingPreparedText),
                );
                get_element_mut::<Text>(&scene.arena, scene.root)
                    .set_text("Native text edited after injected loss");
                scene.layout();
                scene.observe("prepared-after-text-edit", Status::Supported, None);
            }
        }
    }
}

#[test]
fn native_inventory_text_area_interaction_states() {
    for dpr in [1.0, 2.0] {
        let mut area = TextArea::with_stable_id(0xa130);
        area.set_text("native text area".into());
        area.font_size = 16.0;
        let mut scene = Scene::new(Box::new(area), dpr);
        scene.layout();
        scene.observe("plain", Status::Supported, None);
        {
            let mut area = get_element_mut::<TextArea>(&scene.arena, scene.root);
            area.is_focused = true;
            area.select_range(1, 5);
        }
        scene.layout();
        scene.observe("selection", Status::Supported, None);
        {
            let mut area = get_element_mut::<TextArea>(&scene.arena, scene.root);
            area.set_text("updated text area".into());
            area.ime_preedit = "注音".into();
            area.ime_preedit_cursor = Some((0, 1));
            area.children_dirty = true;
            area.bump_unified_ifc_source_revision();
            area.dirty_flags = DirtyFlags::ALL;
        }
        scene.layout();
        assert_eq!(
            get_element_mut::<TextArea>(&scene.arena, scene.root).ime_preedit,
            "注音"
        );
        scene.observe("focused-ime", Status::Supported, None);
        // Invalid IME owner state: preserve this rejection rather than classify
        // it as unsupported selection or IME rendering.
        get_element_mut::<TextArea>(&scene.arena, scene.root).is_focused = false;
        scene.sync();
        scene.observe(
            "unfocused-ime-injected",
            Status::InvalidSnapshot,
            Some(LegacyPaintReason::StatefulPaint),
        );
    }
}

#[test]
fn native_inventory_image_resource_states() {
    for dpr in [1.0, 2.0] {
        for state in ["ready", "loading", "error"] {
            let mut image = Image::new_with_id(
                0xa140,
                ImageSource::Rgba {
                    width: 1,
                    height: 1,
                    pixels: Arc::from([31_u8, 41, dpr as u8, 255]),
                },
            );
            image.apply_style(box_style(48.0, 32.0));
            // Resource delivery is controlled, not a network/worker timing test.
            if state == "loading" {
                image.set_resource_loading_for_test();
            }
            if state == "error" {
                image.set_resource_error_for_test();
            }
            let mut scene = Scene::new(Box::new(image), dpr);
            scene.layout();
            let commands = scene.observe(state, Status::Supported, None);
            assert_eq!(
                commands[5],
                usize::from(state == "ready"),
                "Image payload must agree with named resource state"
            );
            if state == "ready" {
                get_element_mut::<Image>(&scene.arena, scene.root).set_source(ImageSource::Rgba {
                    width: 1,
                    height: 1,
                    pixels: Arc::from([91_u8, 81, dpr as u8, 255]),
                });
                scene.sync();
                scene.observe(
                    "source-changed-before-freeze",
                    Status::BeforePreparation,
                    Some(LegacyPaintReason::MissingPreparedImage),
                );
                scene.layout();
                scene.observe("replacement-frozen", Status::Supported, None);
            }
        }
    }
}

#[test]
fn native_inventory_svg_resource_states() {
    for dpr in [1.0, 2.0] {
        for state in ["ready", "loading", "error"] {
            let source = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="18"><!-- native inventory {dpr} {state} --><rect width="24" height="18" fill="#2266cc"/></svg>"##
            );
            let svg_source = SvgSource::Content(source.clone());
            let _document = crate::view::svg_resource::prime_svg_document_ready_for_test(
                &svg_source,
                24.0,
                18.0,
            );
            let mut svg = Svg::new_with_id(0xa150, svg_source);
            svg.apply_style(box_style(48.0, 36.0));
            if state == "loading" {
                svg.set_document_loading_for_transform_test();
            }
            if state == "error" {
                svg.set_document_error_for_transform_test();
            }
            let mut scene = Scene::new(Box::new(svg), dpr);
            scene.layout();
            if state == "ready" {
                scene.observe(
                    "raster-acquired-after-freeze",
                    Status::Supported,
                    None,
                );
                // Deterministic resource completion using the existing raster
                // fixture; subsequent recording still passes production layout.
                get_element_mut::<Svg>(&scene.arena, scene.root)
                    .prepare_content_paint_for_test(&source, (24.0, 18.0), dpr)
                    .unwrap();
                scene.layout();
            }
            let commands = scene.observe(state, Status::Supported, None);
            assert_eq!(
                commands[6],
                usize::from(state == "ready"),
                "SVG payload must agree with named resource state"
            );
            if state == "ready" {
                get_element_mut::<Svg>(&scene.arena, scene.root)
                    .set_source(SvgSource::Content(format!("{source}<!-- replacement -->")));
                scene.sync();
                scene.observe(
                    "source-changed-before-freeze",
                    Status::BeforePreparation,
                    Some(LegacyPaintReason::MissingPreparedSvg),
                );
            }
        }
    }
}
