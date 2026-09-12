use super::*;
use crate::style::Opacity;

mod viewport_tests;
mod ime_lifecycle_tests;
mod alignment_tests;

const EXTENT: [u32; 2] = [320, 240];

struct TextGpuCleanup;
impl Drop for TextGpuCleanup {
    fn drop(&mut self) {
        crate::view::render_pass::text_pass::clear_text_resources_cache();
    }
}

// These gates exercise generic recording/planning/execution, independently of
// the production selector. viewport_tests separately exercises C-3 takeover.
// Expected caret coordinates come from layout's navigation geometry, not a
// recorded op or a Legacy readback. Glyph shapes remain font-dependent.
fn fixture(case: u8) -> (NodeArena, Vec<NodeKey>, [f32; 2]) {
    let (mut arena, _, text) = if case == 3 {
        let (arena, roots, root, _, _) =
            prepared_projection_text_area_preedit_tree(8, "中🙂", Some((0, 7)));
        (arena, roots, root)
    } else if case == 2 {
        prepared_plain_text_area_preedit_tree("complete commands", 108.0, 3, "中🙂", Some((0, 7)))
    } else if case == 1 {
        prepared_plain_text_area_selection_tree("      ", 108.0, 0, 3)
    } else {
        prepared_plain_text_area_tree("")
    };
    let mut wrapper = Element::new_with_id(0xc1_3000, 0.0, 0.0, 320.0, 240.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
    wrapper.apply_style(style);
    let root = crate::view::test_support::commit_element(&mut arena, Box::new(wrapper));
    let constraints = LayoutConstraints {
        max_width: 320.0,
        max_height: 240.0,
        viewport_width: 320.0,
        viewport_height: 240.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(240.0),
    };
    let placement = LayoutPlacement {
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
    };
    crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);
    arena.set_parent(text, Some(root));
    arena.set_children(root, vec![text]);
    arena.with_element_taken(text, |element, arena| {
        let text = element.as_any_mut().downcast_mut::<TextArea>().unwrap();
        text.color = Color::rgb(255, 0, 0);
        text.selection_background_color = Color::rgb(255, 0, 0);
        text.is_focused = true;
        text.caret_visible = case != 1;
        text.children_dirty = true;
        text.bump_unified_ifc_source_revision();
        text.measure(
            LayoutConstraints {
                max_width: 132.0,
                ..constraints
            },
            arena,
        );
        text.set_layout_offset(8.0, 12.0);
        text.place(placement, arena);
        text.caret_blink_epoch = None;
    });
    let node = arena.get(text).unwrap();
    let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
    let (x, y, height) = text_area.caret_screen_position(&arena).unwrap();
    assert!(height > 4.0);
    let caret = if case == 1 { [9.0, 14.0] } else { [x, y + 2.0] };
    drop(node);
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
    (arena, vec![root], caret)
}

fn check_pixels(pixels: &[u8], caret: [f32; 2], dpr: u32, case: u8) {
    let width = EXTENT[0] * dpr;
    if case == 1 {
        let at = (((caret[1] as u32 * dpr) * width + caret[0] as u32 * dpr) * 4) as usize;
        for (actual, expected) in pixels[at..at + 4].iter().zip([255_u8, 0, 0, 128]) {
            assert!(
                actual.abs_diff(expected) <= 1,
                "case {case}, DPR {dpr}: {:?}",
                &pixels[at..at + 4]
            );
        }
        assert_eq!(&pixels[..4], &[0, 0, 0, 0]);
        return;
    }
    // Pick a physical pixel center inside the 1 logical-pixel-wide caret.
    let x = (caret[0] * dpr as f32 + 0.5).floor() as u32;
    let y = (caret[1] * dpr as f32).floor() as u32;
    assert!((x as f32 + 0.5) / dpr as f32 >= caret[0]);
    assert!((x as f32 + 0.5) / (dpr as f32) < caret[0] + 1.0);
    // The rectangle raster contract uses a one-physical-pixel smoothstep
    // around its signed edge distance. A 1px-wide caret has no fully covered
    // DPR1 sample: at its center coverage is 0.84375, not 1.0. Derive the
    // expected alpha from layout geometry and RGBA8/group quantization; do
    // not use another renderer's pixels or assume area-conserving box AA.
    let center = (x as f32 + 0.5) / dpr as f32;
    let distance = (center - (caret[0] + 0.5)).abs() - 0.5;
    let aa = 1.0 / dpr as f32;
    let t = ((aa - distance) / (2.0 * aa)).clamp(0.0, 1.0);
    let coverage = t * t * (3.0 - 2.0 * t);
    let expected_alpha = ((coverage * 255.0).round() * 0.5).round() as u8;
    let at = ((y * width + x) * 4) as usize;
    for (actual, expected) in pixels[at..at + 4]
        .iter()
        .zip([255_u8, 0, 0, expected_alpha])
    {
        assert!(
            actual.abs_diff(expected) <= 1,
            "caret case {case}, DPR {dpr}, ({x},{y}), expected alpha {expected_alpha}: {:?}",
            &pixels[at..at + 4]
        );
    }
    assert_eq!(
        &pixels[..4],
        &[0, 0, 0, 0],
        "outside content must be transparent"
    );
}

fn run(legacy: bool) -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    let _cleanup = TextGpuCleanup;
    // Empty caret, selection over spaces, plain IME, projected IME.
    for case in 0..4 {
        for dpr in [1_u32, 2] {
            let (mut arena, roots, caret) = fixture(case);
            let artifact = if legacy {
                None
            } else {
                let (properties, generations) = sync_identity(&arena, &roots);
                let FrameArtifactRecordOutcome::Artifact { artifact, .. } =
                    record_surface_dag_frame_artifact(
                        &arena,
                        &roots,
                        &properties,
                        &generations,
                        RendererMode::ForcedForTests,
                    )
                    .map_err(|error| format!("record case {case}: {error:?}"))?
                else {
                    panic!("fallback")
                };
                Some(artifact)
            };
            let mut viewport = Viewport::new();
            let mut graphs = Vec::new();
            if legacy {
                for _ in 0..2 {
                    let (mut graph, ctx, target) = transformed_graph_prelude_with_size(
                        dpr as f32,
                        None,
                        EXTENT.map(|v| v * dpr),
                    );
                    arena
                        .with_element_taken(roots[0], |el, arena| el.build(&mut graph, arena, ctx))
                        .unwrap();
                    add_present(&mut graph, &target)?;
                    graphs.push(graph);
                }
            }
            // Both recorders have finished. No live layout, IME host, or
            // projection authority is available during compilation/execution.
            drop(arena);
            for frame in 0..2 {
                let (graph, owner, actions) = if legacy {
                    (graphs.remove(0), None, Vec::new())
                } else {
                    let (mut graph, ctx, target) = transformed_graph_prelude_with_size(
                        dpr as f32,
                        None,
                        EXTENT.map(|v| v * dpr),
                    );
                    let context = ArtifactSurfaceRasterContext::new(
                        dpr as f32,
                        FORMAT,
                        ctx.paint_offset(),
                        None,
                        8192,
                        128 * 1024 * 1024,
                    )
                    .unwrap();
                    let plan = prepare_artifact_surface_raster_plan(
                        artifact.as_ref().unwrap().clone(),
                        context,
                    )
                    .map_err(|error| format!("plan case {case}: {error:?}"))?;
                    assert_eq!(plan.nodes().len(), 1);
                    let frame_plan =
                        seal_prepared_artifact_surface_frame(plan).map_err(|error| {
                            format!("seal case {case}, DPR {dpr}, frame {frame}: {error:?}")
                        })?;
                    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
                    let _ = take_last_production_actions_for_test();
                    emit_prepared_artifact_surface_frame_from_pool(
                        &mut viewport,
                        owner,
                        frame_plan,
                        &mut graph,
                        ctx,
                    )
                    .map_err(|error| format!("emit: {error:?}"))?;
                    let actions = take_last_production_actions_for_test();
                    add_present(&mut graph, &target)?;
                    (graph, Some(owner), actions)
                };
                let pixels = render_on_viewport_with_size(
                    graph,
                    gpu,
                    &mut viewport,
                    dpr as f32,
                    FORMAT,
                    EXTENT.map(|v| v * dpr),
                )?;
                check_pixels(&pixels, caret, dpr, case);
                if let Some(owner) = owner {
                    assert_eq!(
                        actions,
                        [if frame == 0 {
                            RetainedSurfaceCompileAction::Reraster
                        } else {
                            RetainedSurfaceCompileAction::Reuse
                        }]
                    );
                    assert!(
                        viewport.finish_retained_surface_transaction_for_frame(Some(owner), true)
                    );
                }
            }
        }
    }
    eprintln!(
        "generic TextArea frozen IME/caret, legacy={legacy}, passed on {}",
        gpu.label()
    );
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_generic_text_area_frozen_ime_caret_and_reuse() -> Result<(), String> {
    run(false)
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_legacy_text_area_frozen_ime_caret_pixels() -> Result<(), String> {
    run(true)
}

#[test]
fn generic_text_area_resident_span_uses_parent_edges_not_store_order() {
    let (arena, roots, _) = fixture(3);
    let (properties, generations) = sync_identity(&arena, &roots);
    let FrameArtifactRecordOutcome::Artifact { artifact, .. } = record_surface_dag_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .unwrap() else {
        panic!("fallback")
    };
    assert!(
        artifact
            .owner_nodes
            .iter()
            .enumerate()
            .any(|(i, snapshot)| snapshot.parent.is_some_and(|parent| {
                artifact
                    .owner_nodes
                    .iter()
                    .position(|candidate| candidate.owner == parent)
                    .unwrap()
                    > i
            })),
        "fixture must expose a transparent parent discovered after its paint child"
    );
    drop(arena);
    let context =
        ArtifactSurfaceRasterContext::new(1.0, FORMAT, [0.0, 0.0], None, 8192, 128 * 1024 * 1024)
            .unwrap();
    for reverse in [false, true] {
        let mut reordered = artifact.clone();
        if reverse {
            reordered.owner_nodes.reverse();
        }
        let plan = prepare_artifact_surface_raster_plan(reordered, context).unwrap();
        seal_prepared_artifact_surface_frame(plan)
            .expect("parent edges define canonical resident topology");
    }
    let mut duplicate = artifact;
    duplicate.owner_nodes.push(duplicate.owner_nodes[0]);
    assert!(
        prepare_artifact_surface_raster_plan(duplicate, context).is_err(),
        "storage order tolerance must not admit duplicate owners"
    );
}
