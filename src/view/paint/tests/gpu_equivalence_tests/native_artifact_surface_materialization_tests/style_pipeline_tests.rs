use super::*;
use crate::style::{Opacity, Transform, Translate};
use crate::view::test_support::{commit_child, commit_element, get_element_mut};

mod single_viewport_tests;

#[derive(Clone, Copy, Debug)]
enum StyleScene {
    ScrollingGradient,
    TranslucentFill,
}

impl StyleScene {
    fn states(self) -> [([f32; 2], f32); 3] {
        match self {
            Self::ScrollingGradient => [([3.0, 0.0], 8.0), ([3.0, 0.0], 16.0), ([9.0, 4.0], 16.0)],
            Self::TranslucentFill => [([9.0, 4.0], 0.0), ([17.0, 4.0], 0.0), ([9.0, 4.0], 0.0)],
        }
    }
}

// Coverage boundary: this fixture owns a layout Viewport, while the GPU tests
// use a separate Viewport for rendering and the retained pool. They prove that
// production layout output feeds recording/painting correctly, including the
// explicitly tested selector path. They do not prove the production coupling
// of layout, paint, and resident state within a single Viewport. The separate
// single_viewport_tests gates exercise that lifecycle for TranslucentFill;
// ScrollingGradient still has no equivalent artifact-selector integration gate.
struct StyleFixture {
    scene: StyleScene,
    layout_viewport: Viewport,
    arena: NodeArena,
    root: crate::view::node_arena::NodeKey,
    transformed: crate::view::node_arena::NodeKey,
    content_style: Style,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn sized_grid(width: f32, height: f32) -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
    style
}

impl StyleFixture {
    fn new(scene: StyleScene) -> Self {
        let mut arena = NodeArena::new();
        let (root, transformed, content_style) = match scene {
            StyleScene::ScrollingGradient => {
                let mut root = Element::new_with_id(0xb4_8101, 0.0, 0.0, 48.0, 40.0);
                let mut style = sized_grid(48.0, 40.0);
                style.insert(
                    PropertyId::ScrollDirection,
                    ParsedValue::ScrollDirection(ScrollDirection::Vertical),
                );
                root.apply_style(style);
                let root = commit_element(&mut arena, Box::new(root));
                let mut content = Element::new_with_id(0xb4_8102, 0.0, 0.0, 48.0, 120.0);
                let mut style = sized_grid(48.0, 120.0);
                // Keep this a gradient-only background, even if style defaults
                // change: both renderers must paint it without a solid fill.
                style.insert(
                    PropertyId::BackgroundColor,
                    ParsedValue::color_like(Color::rgba(0, 0, 0, 0)),
                );
                let gradient = Gradient::linear(SideOrCorner::Bottom)
                    .stop(Color::rgb(224, 36, 28), Some(Length::percent(0.0)))
                    .stop(Color::rgb(224, 36, 28), Some(Length::percent(20.0)))
                    .stop(Color::rgb(24, 72, 224), Some(Length::percent(20.0)))
                    .stop(Color::rgb(24, 72, 224), Some(Length::percent(100.0)))
                    .build();
                style.set_background_image(gradient);
                content.apply_style(style.clone());
                let transformed = commit_child(&mut arena, root, Box::new(content));
                (root, transformed, style)
            }
            StyleScene::TranslucentFill => {
                let mut root = Element::new_with_id(0xb4_8201, 0.0, 0.0, 20.0, 16.0);
                let mut style = sized_grid(20.0, 16.0);
                style.insert(
                    PropertyId::BackgroundColor,
                    ParsedValue::color_like(Color::rgb(224, 36, 28)),
                );
                style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
                root.apply_style(style.clone());
                let root = commit_element(&mut arena, Box::new(root));
                (root, root, style)
            }
        };
        Self {
            scene,
            layout_viewport: Viewport::new(),
            arena,
            root,
            transformed,
            content_style,
            properties: PropertyTrees::default(),
            generations: PaintGenerationTracker::default(),
        }
    }

    fn update(&mut self, translation: [f32; 2], scroll_y: f32) {
        self.content_style
            .set_transform(Transform::new([Translate::xy(
                Length::px(translation[0]),
                Length::px(translation[1]),
            )]));
        get_element_mut::<Element>(&self.arena, self.transformed)
            .apply_style(self.content_style.clone());
        // set_scroll_offset only installs the offset and marks placement dirty;
        // it does not need a preceding layout. Apply all inputs, then perform
        // exactly one production layout pass per state for both scenes.
        if matches!(self.scene, StyleScene::ScrollingGradient) {
            get_element_mut::<Element>(&self.arena, self.root).set_scroll_offset((0.0, scroll_y));
        }
        // Establish actual spatial snapshots without a resolved-transform hook.
        self.layout();
        self.observe();
        let node = self.arena.get(self.transformed).unwrap();
        assert!(
            node.element
                .compositor_spatial_placement_snapshot()
                .is_some()
        );
        let snapshot = self
            .properties
            .transforms
            .values()
            .find(|node| node.owner == self.transformed)
            .unwrap();
        // Match surface_dag::finite_translation's exact numeric comparison.
        // Changes to its precision policy require reviewing this production-
        // matrix witness too; an epsilon needs separate raster-identity proof.
        assert_eq!(
            snapshot.local_matrix.to_cols_array(),
            glam::Mat4::from_translation(glam::Vec3::new(translation[0], translation[1], 0.0))
                .to_cols_array()
        );
    }

    fn observe(&mut self) {
        // Observe only after layout. Refresh caches from local dirty truth;
        // neither generations nor dirty flags are forced clean by this fixture.
        self.arena.refresh_subtree_dirty_cache(self.root);
        self.properties.sync(&self.arena, &[self.root]);
        assert!(
            self.properties.validation_errors.is_empty(),
            "{:?}",
            self.properties.validation_errors
        );
        self.generations
            .sync(&self.arena, &[self.root], &self.properties);
    }

    fn layout(&mut self) {
        crate::view::viewport::layout_artifact_style_scene_for_test(
            &mut self.layout_viewport,
            &mut self.arena,
            self.root,
            [WIDTH as f32, HEIGHT as f32],
        );
    }

    fn artifact(&self) -> crate::view::paint::PaintArtifact {
        let outcome = record_surface_dag_frame_artifact(
            &self.arena,
            &[self.root],
            &self.properties,
            &self.generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::Artifact { artifact, .. } = outcome else {
            panic!("style pipeline artifact rejected: {outcome:?}");
        };
        artifact
    }
}

#[test]
fn materialization_style_pipeline_resolves_and_eliminates_translation_boundaries() {
    for scene in [StyleScene::ScrollingGradient, StyleScene::TranslucentFill] {
        let mut fixture = StyleFixture::new(scene);
        for (translation, scroll_y) in scene.states() {
            fixture.update(translation, scroll_y);
            for dpr in [1.0, 2.0] {
                let plan = prepare_artifact_surface_raster_plan(
                    fixture.artifact(),
                    ArtifactSurfaceRasterContext::new(
                        dpr,
                        FORMAT,
                        [0.0, 0.0],
                        None,
                        4096,
                        128 * 1024 * 1024,
                    )
                    .unwrap(),
                )
                .unwrap();

                assert_eq!(
                    plan.nodes().len(),
                    1,
                    "{scene:?} {:?}",
                    plan.materialization_decisions()
                );
                assert_eq!(plan.materialization_decisions().len(), 2);
                assert_eq!(
                    plan.materialization_decisions()
                        .iter()
                        .filter(|decision| decision.outcome()
                            == SurfaceMaterializationOutcome::EliminatedPassThrough)
                        .count(),
                    1
                );
            }
        }
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_materialization_style_pipeline_scroll_executor_pixels_and_reuse() -> Result<(), String> {
    run_style_pipeline_pixels_and_reuse(StyleScene::ScrollingGradient)
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_materialization_style_pipeline_transform_effect_production_pixels_and_reuse()
-> Result<(), String> {
    run_style_pipeline_pixels_and_reuse(StyleScene::TranslucentFill)
}

fn run_style_pipeline_pixels_and_reuse(scene: StyleScene) -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    for dpr in [1_u32, 2] {
        let mut fixture = StyleFixture::new(scene);
        let mut viewport = Viewport::new();
        for (frame, (translation, scroll_y)) in scene.states().into_iter().enumerate() {
            fixture.update(translation, scroll_y);
            let (graph, owner, actions, bytes) = match scene {
                // Production selection still excludes this scene. Exercise
                // the common executor without claiming selector coverage.
                StyleScene::ScrollingGradient => {
                    materialized_two_boundary_graph(&mut viewport, fixture.artifact(), dpr as f32)?
                }
                StyleScene::TranslucentFill => {
                    // This scene already has production Artifact admission.
                    // Exercise the real selector/dispatch in addition to style
                    // resolution and persistent property/generation snapshots.
                    let (mut graph, ctx, target) = transformed_graph_prelude_with_size(
                        dpr as f32,
                        None,
                        [WIDTH * dpr, HEIGHT * dpr],
                    );
                    let emitted = emit_retained_auto_artifact_surface_for_test(
                        &mut viewport,
                        &fixture.arena,
                        &[fixture.root],
                        &fixture.properties,
                        &fixture.generations,
                        &mut graph,
                        &ctx,
                    )?;
                    assert_eq!(emitted.surface_count, 1);
                    add_present(&mut graph, &target)?;
                    (
                        graph,
                        emitted.frame_owner,
                        emitted.actions,
                        emitted.aggregate_texture_bytes,
                    )
                }
            };
            let expected_action = if frame == 0 {
                RetainedSurfaceCompileAction::Reraster
            } else {
                RetainedSurfaceCompileAction::Reuse
            };
            let expected_bytes = match scene {
                StyleScene::ScrollingGradient => 48 * 120 * 12,
                StyleScene::TranslucentFill => 20 * 16 * 12,
            } * u64::from(dpr * dpr);
            if actions != [expected_action] || bytes != expected_bytes {
                return Err(format!(
                    "{scene:?} DPR {dpr} frame {frame}: actions={actions:?}, bytes={bytes}, expected={expected_bytes}"
                ));
            }
            let pixels = render_on_viewport_with_size(
                graph,
                gpu,
                &mut viewport,
                dpr as f32,
                FORMAT,
                [WIDTH * dpr, HEIGHT * dpr],
            )?;
            if !viewport.finish_retained_surface_transaction_for_frame(Some(owner), true) {
                return Err("style pipeline commit failed".into());
            }
            validate_style_pixels(&pixels, scene, translation, scroll_y, dpr, frame)?;
        }
    }
    eprintln!(
        "style pipeline materialization pixels/live-generation reuse passed on {}",
        gpu.label()
    );
    Ok(())
}

// Absolute expectations are derived from fixture geometry, not a legacy render.
fn style_pixel_probes(
    scene: StyleScene,
    translation: [f32; 2],
    scroll_y: f32,
) -> Result<Vec<(u32, u32, [u8; 4])>, String> {
    let probes = match scene {
        StyleScene::ScrollingGradient => {
            let boundary_y = 24.0 + translation[1] - scroll_y;
            vec![
                (translation[0] + 4.0, boundary_y - 4.0, [190_u8, 4, 3, 255]),
                (translation[0] + 4.0, boundary_y + 4.0, [2, 17, 190, 255]),
                (1.0, 12.0, [0, 0, 0, 0]),
                (49.0, 12.0, [0, 0, 0, 0]),
                // Stay inside the translated content horizontally so that
                // transparency here can only be established by bottom clipping.
                (translation[0] + 4.0, 41.0, [0, 0, 0, 0]),
            ]
        }
        StyleScene::TranslucentFill => vec![
            (translation[0] + 3.0, 7.0, [190, 4, 3, 128]),
            (translation[0] - 2.0, 7.0, [0, 0, 0, 0]),
            (translation[0] + 22.0, 7.0, [0, 0, 0, 0]),
        ],
    };
    // Validate before converting: float-to-u32 saturates negative coordinates,
    // and subsequent unsigned subtraction would hide the invalid fixture state.
    probes.into_iter().map(|(x, y, expected)| {
        if !x.is_finite() || !y.is_finite()
            || x < 0.0 || x >= WIDTH as f32 || y < 0.0 || y >= HEIGHT as f32
            || x.fract() != 0.0 || y.fract() != 0.0 {
            return Err(format!(
                "{scene:?} invalid pixel probe ({x}, {y}) for translation={translation:?}, scroll_y={scroll_y}; expected integer coordinates inside {WIDTH}x{HEIGHT}"
            ));
        }
        Ok((x as u32, y as u32, expected))
    }).collect()
}

fn validate_style_pixels(
    pixels: &[u8],
    scene: StyleScene,
    translation: [f32; 2],
    scroll_y: f32,
    dpr: u32,
    frame: usize,
) -> Result<(), String> {
    let probes = style_pixel_probes(scene, translation, scroll_y)?;
    for (x, y, expected) in probes {
        let i = ((y * dpr * WIDTH * dpr + x * dpr) * 4) as usize;
        let actual: [u8; 4] = pixels[i..i + 4].try_into().unwrap();
        if actual
            .into_iter()
            .zip(expected)
            .any(|(a, e)| a.abs_diff(e) > 1)
        {
            let mut extent = [u32::MAX, u32::MAX, 0, 0];
            for py in 0..HEIGHT * dpr {
                for px in 0..WIDTH * dpr {
                    if pixels[((py * WIDTH * dpr + px) * 4 + 3) as usize] != 0 {
                        extent[0] = extent[0].min(px);
                        extent[1] = extent[1].min(py);
                        extent[2] = extent[2].max(px);
                        extent[3] = extent[3].max(py);
                    }
                }
            }
            return Err(format!(
                "{scene:?} DPR {dpr} frame {frame} @({x},{y}): {actual:?}, expected {expected:?}, visible extent={extent:?}"
            ));
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_materialization_style_pipeline_legacy_pixels() -> Result<(), String> {
    // ScrollingGradient explicitly has transparent background-color and a
    // visible gradient. This tests the population affected by shared culling
    // via Element::build, independently of artifact recording/execution.
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    for scene in [StyleScene::ScrollingGradient, StyleScene::TranslucentFill] {
        for dpr in [1_u32, 2] {
            let mut fixture = StyleFixture::new(scene);
            let mut viewport = Viewport::new();
            for (frame, (translation, scroll_y)) in scene.states().into_iter().enumerate() {
                fixture.update(translation, scroll_y);
                let (mut graph, ctx, target) = transformed_graph_prelude_with_size(
                    dpr as f32,
                    None,
                    [WIDTH * dpr, HEIGHT * dpr],
                );
                fixture
                    .arena
                    .with_element_taken(fixture.root, |element, arena| {
                        element.build(&mut graph, arena, ctx)
                    })
                    .ok_or("legacy style root disappeared")?;
                add_present(&mut graph, &target)?;
                let pixels = render_on_viewport_with_size(
                    graph,
                    gpu,
                    &mut viewport,
                    dpr as f32,
                    FORMAT,
                    [WIDTH * dpr, HEIGHT * dpr],
                )?;
                validate_style_pixels(&pixels, scene, translation, scroll_y, dpr, frame)?;
            }
        }
    }
    Ok(())
}

#[test]
fn materialization_style_pipeline_records_gradient_after_layout() {
    let mut fixture = StyleFixture::new(StyleScene::ScrollingGradient);
    fixture.update([3.0, 0.0], 8.0);
    let artifact = fixture.artifact();
    let gradients: Vec<_> = artifact
        .ops
        .iter()
        .filter_map(|op| match op {
            crate::view::paint::PaintOp::DrawRect(op) if op.params.gradient.is_some() => Some(op),
            _ => None,
        })
        .collect();
    assert_eq!(
        gradients.len(),
        1,
        "layout must not cull a gradient-only background"
    );
    assert_eq!(gradients[0].params.size, [48.0, 120.0]);
    assert_eq!(
        gradients[0].params.fill_color[3], 0.0,
        "a solid background must not mask the gradient-only culling regression"
    );
}

#[test]
fn materialization_style_pipeline_invalid_probe_coordinates_return_errors() {
    for (scene, translation, scroll_y) in [
        (StyleScene::ScrollingGradient, [3.0, 0.0], 40.0),
        (StyleScene::TranslucentFill, [1.0, 4.0], 0.0),
        (StyleScene::TranslucentFill, [f32::NAN, 4.0], 0.0),
        (StyleScene::TranslucentFill, [WIDTH as f32, 4.0], 0.0),
    ] {
        let error = style_pixel_probes(scene, translation, scroll_y).unwrap_err();
        assert!(error.contains("invalid pixel probe"), "{error}");
    }
}

#[test]
fn materialization_style_pipeline_bottom_probe_is_inside_content_outside_scrollport() {
    for (translation, scroll_y) in StyleScene::ScrollingGradient.states() {
        let probes =
            style_pixel_probes(StyleScene::ScrollingGradient, translation, scroll_y).unwrap();
        let &(x, y, expected) = probes.last().unwrap();
        let (x, y) = (x as f32, y as f32);
        assert_eq!(expected, [0; 4]);
        assert!(
            x >= translation[0] && x < translation[0] + 48.0,
            "bottom probe must be inside translated content: {translation:?}, x={x}"
        );
        assert!((0.0..48.0).contains(&x), "must not test a side clip");
        let content_top = translation[1] - scroll_y;
        assert!(y >= content_top && y < content_top + 120.0);
        assert!(y >= 40.0, "must test below the scrollport");
    }
}

#[test]
fn materialization_style_pipeline_unchanged_layout_preserves_dirty_and_generations() {
    // These fixtures have no asynchronous resource hosts. This verifies their
    // repeated-layout behavior, not universal run_layout_pass idempotence.
    for scene in [StyleScene::ScrollingGradient, StyleScene::TranslucentFill] {
        let mut fixture = StyleFixture::new(scene);
        for (translation, scroll_y) in scene.states() {
            fixture.update(translation, scroll_y);
            let owners = [fixture.root, fixture.transformed];
            let snapshot = |fixture: &StyleFixture| {
                owners.map(|owner| {
                    (
                        fixture.generations.snapshot(owner).unwrap(),
                        fixture
                            .arena
                            .get(owner)
                            .unwrap()
                            .element
                            .local_dirty_flags(),
                        fixture.arena.arena_local_dirty(owner),
                        fixture.arena.cached_subtree_dirty(owner),
                    )
                })
            };
            let before = snapshot(&fixture);
            fixture.layout();
            fixture.observe();
            assert_eq!(
                snapshot(&fixture),
                before,
                "unchanged layout drifted for {scene:?} at {translation:?}/{scroll_y}"
            );
        }
    }
}
