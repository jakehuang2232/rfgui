use super::*;

#[derive(Clone, Copy, Debug)]
enum StageAReuseCase {
    Plain,
    Interactive,
    AtomicProjection,
    FocusedAtomicProjection,
    AtomicProjectionSelection,
}

struct StageAReuseFixture {
    arena: NodeArena,
    roots: Vec<NodeKey>,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

#[derive(Debug, PartialEq, Eq)]
struct ScrollbarOverlayFillObservation {
    position_bits: [u32; 2],
    size_bits: [u32; 2],
    fill_color_bits: [u32; 4],
    opacity_bits: u32,
    mode: crate::view::render_pass::RectRenderMode,
}

impl StageAReuseCase {
    fn label(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Interactive => "interactive",
            Self::AtomicProjection => "atomic-projection",
            Self::FocusedAtomicProjection => "focused-atomic-projection",
            Self::AtomicProjectionSelection => "atomic-projection-selection",
        }
    }
}

fn reuse_fixture(case: StageAReuseCase, outer_scroll_y: f32) -> StageAReuseFixture {
    let (arena, roots, properties, generations) = match case {
        StageAReuseCase::Plain | StageAReuseCase::Interactive => {
            let (arena, roots, _, _) = prepared_scroll_text_area_scene_with(
                outer_scroll_y,
                9.0,
                "RetainedAuto Stage A reuse contract",
            );
            if matches!(case, StageAReuseCase::Interactive) {
                let wrapper = arena.children_of(roots[0])[0];
                let text_area = arena.children_of(wrapper)[0];
                let mut node = arena.get_mut(text_area).expect("interactive TextArea");
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .expect("interactive TextArea type");
                text_area.is_focused = true;
                text_area.caret_visible = true;
                text_area.caret_blink_epoch = None;
                text_area.cursor_char = 3;
            }
            let (properties, generations) = synced_paint_state(&arena, &roots);
            (arena, roots, properties, generations)
        }
        StageAReuseCase::AtomicProjection
        | StageAReuseCase::FocusedAtomicProjection
        | StageAReuseCase::AtomicProjectionSelection => {
            prepared_atomic_projection_scroll_text_area_scene_with(
                outer_scroll_y,
                "before projected after",
                matches!(case, StageAReuseCase::FocusedAtomicProjection),
                matches!(case, StageAReuseCase::AtomicProjectionSelection).then_some((0, 6)),
                None,
            )
        }
    };
    StageAReuseFixture {
        arena,
        roots,
        properties,
        generations,
    }
}

fn text_area_owner(fixture: &StageAReuseFixture) -> NodeKey {
    let wrapper = fixture.arena.children_of(fixture.roots[0])[0];
    fixture.arena.children_of(wrapper)[0]
}

fn restore_case_interaction(fixture: &StageAReuseFixture, case: StageAReuseCase) {
    let mut node = fixture
        .arena
        .get_mut(text_area_owner(fixture))
        .expect("reuse TextArea owner");
    let text_area = node
        .element
        .as_any_mut()
        .downcast_mut::<TextArea>()
        .expect("reuse TextArea type");
    text_area.is_focused = matches!(
        case,
        StageAReuseCase::Interactive | StageAReuseCase::FocusedAtomicProjection
    );
    text_area.caret_visible = text_area.is_focused;
    text_area.caret_blink_epoch = None;
    text_area.cursor_char = match case {
        StageAReuseCase::Interactive => 3,
        StageAReuseCase::FocusedAtomicProjection => 7,
        _ => text_area.cursor_char,
    };
    if matches!(case, StageAReuseCase::AtomicProjectionSelection) {
        text_area.selection_anchor_char = Some(0);
        text_area.selection_focus_char = Some(6);
    }
}

fn apply_content_change_in_place(fixture: &mut StageAReuseFixture, case: StageAReuseCase) {
    let root = fixture.roots[0];
    let text_area = text_area_owner(fixture);
    let projection = matches!(
        case,
        StageAReuseCase::AtomicProjection
            | StageAReuseCase::FocusedAtomicProjection
            | StageAReuseCase::AtomicProjectionSelection
    );
    let max_height = if projection { 240.0 } else { 28.0 };
    fixture
        .arena
        .with_element_taken(text_area, |element, arena| {
            element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("content-change TextArea type")
                .set_text(if projection {
                    "before projected aftex".to_string()
                } else {
                    "RetainedAuto Stage A reuse contracu".to_string()
                });
            element.measure(
                LayoutConstraints {
                    max_width: 108.0,
                    max_height,
                    viewport_width: 320.0,
                    viewport_height: 240.0,
                    percent_base_width: Some(320.0),
                    percent_base_height: Some(240.0),
                },
                arena,
            );
            if projection {
                element.place(
                    LayoutPlacement {
                        parent_x: 0.0,
                        parent_y: -20.0,
                        visual_offset_x: 0.0,
                        visual_offset_y: 0.0,
                        available_width: 108.0,
                        available_height: 240.0,
                        viewport_width: 320.0,
                        viewport_height: 240.0,
                        percent_base_width: Some(320.0),
                        percent_base_height: Some(240.0),
                    },
                    arena,
                );
            }
        });
    if !projection {
        update_prepared_scroll_text_area_scene(
            &mut fixture.arena,
            &fixture.roots,
            &mut fixture.properties,
            &mut fixture.generations,
            20.0,
            9.0,
        );
        let mut root_element =
            crate::view::test_support::get_element_mut::<Element>(&fixture.arena, root);
        root_element.set_scroll_offset((0.0, 20.0));
        root_element.settle_scrollbar_hidden_for_test();
    }
    restore_case_interaction(fixture, case);
    // Load-bearing: settle all residual layout, placement, caret-follow, and
    // paint dirt before observation. The expected Reraster must come from the
    // changed raster stamp, never from a dirty-bit shortcut.
    let mut stack = vec![root];
    while let Some(owner) = stack.pop() {
        stack.extend(fixture.arena.children_of(owner));
        fixture
            .arena
            .get_mut(owner)
            .expect("content-change fixture owner")
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    fixture
        .arena
        .clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    fixture.arena.refresh_subtree_dirty_cache(root);
    fixture.properties.sync(&fixture.arena, &fixture.roots);
    fixture
        .generations
        .sync(&fixture.arena, &fixture.roots, &fixture.properties);
}

fn apply_composition_only_scrollbar_overlay(fixture: &mut StageAReuseFixture) {
    let root = fixture.roots[0];
    crate::view::test_support::get_element_mut::<Element>(&fixture.arena, root)
        .set_sampled_scrollbar_alpha_for_test(1.0);
    fixture.arena.refresh_subtree_dirty_cache(root);
    fixture.properties.sync(&fixture.arena, &fixture.roots);
    fixture
        .generations
        .sync(&fixture.arena, &fixture.roots, &fixture.properties);
}

fn observe_reuse_action(
    viewport: &mut Viewport,
    case: StageAReuseCase,
    stage: &str,
    fixture: &StageAReuseFixture,
) -> (
    crate::view::paint::RetainedSurfaceCompileAction,
    crate::view::paint::RetainedSurfaceRasterStamp,
    Vec<ScrollbarOverlayFillObservation>,
) {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let decision = select_retained_auto_authority(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        &ctx,
        true,
    );
    let scene = match decision {
        AutoAuthorityDecision::PropertyScrollScene { scene, .. } => scene,
        AutoAuthorityDecision::Legacy { trace } => panic!(
            "{} {stage} reuse fixture rejected: {:?}",
            case.label(),
            trace.rejections
        ),
        _ => panic!(
            "{} {stage} selected the wrong reuse authority",
            case.label()
        ),
    };

    let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let mut prepared = crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
        viewport,
        scene,
        &mut graph,
        UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        frame_owner,
    )
    .unwrap_or_else(|error| panic!("{} {stage} reuse prepare failed: {error:?}", case.label()));
    prepared.refresh_actions_from_committed_test_pool();
    let actions = prepared.scroll_content_actions_for_test();
    let stamps = prepared.scroll_content_stamps_for_test();
    let ([action], [stamp]) = (actions.as_slice(), stamps.as_slice()) else {
        panic!(
            "{} must prepare one surface action and identity",
            case.label()
        )
    };
    let observation = (*action, stamp.clone());
    let outcome = crate::view::paint::emit_prepared_retained_property_scroll_forest(prepared);
    let (_state, _trace) = outcome.into_parts();
    let track_color = [0.95, 0.95, 0.95, 0.35].map(f32::to_bits);
    let thumb_color = [0.95, 0.95, 0.95, 0.58].map(f32::to_bits);
    let overlay_fills = graph
        .test_rect_pass_snapshots()
        .into_iter()
        .filter(|rect| rect.fill_color_bits == track_color || rect.fill_color_bits == thumb_color)
        .map(|rect| ScrollbarOverlayFillObservation {
            position_bits: rect.position_bits,
            size_bits: rect.size_bits,
            fill_color_bits: rect.fill_color_bits,
            opacity_bits: rect.opacity_bits,
            mode: rect.mode,
        })
        .collect();
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true));
    (observation.0, observation.1, overlay_fills)
}

fn expected_scrollbar_overlay_fills(case: StageAReuseCase) -> Vec<ScrollbarOverlayFillObservation> {
    let track_x = match case {
        StageAReuseCase::Plain | StageAReuseCase::Interactive => 91.0,
        StageAReuseCase::AtomicProjection
        | StageAReuseCase::FocusedAtomicProjection
        | StageAReuseCase::AtomicProjectionSelection => 99.0,
    };
    [
        ([track_x, 3.0], [6.0, 74.0], [0.95, 0.95, 0.95, 0.35]),
        ([track_x, 7.5454545], [6.0, 24.0], [0.95, 0.95, 0.95, 0.58]),
    ]
    .into_iter()
    .map(
        |(position, size, fill_color)| ScrollbarOverlayFillObservation {
            position_bits: position.map(f32::to_bits),
            size_bits: size.map(f32::to_bits),
            fill_color_bits: fill_color.map(f32::to_bits),
            opacity_bits: 1.0_f32.to_bits(),
            mode: crate::view::render_pass::RectRenderMode::FillOnly,
        },
    )
    .collect()
}

/// Every row owns exactly one retained scroll-content surface. The five cases
/// vary the content recorded into that surface; they are not five reuse-layer
/// surface shapes or five distinct reuse policies.
#[test]
fn stage_a_one_surface_reuse_contract_covers_five_content_shapes() {
    use crate::view::paint::RetainedSurfaceCompileAction::{Reraster, Reuse};

    for case in [
        StageAReuseCase::Plain,
        StageAReuseCase::Interactive,
        StageAReuseCase::AtomicProjection,
        StageAReuseCase::FocusedAtomicProjection,
        StageAReuseCase::AtomicProjectionSelection,
    ] {
        let mut viewport = Viewport::new();
        let mut fixture = reuse_fixture(case, 20.0);
        let (cold_action, cold_stamp, _) =
            observe_reuse_action(&mut viewport, case, "cold", &fixture);
        assert_eq!(cold_action, Reraster, "{} cold action", case.label());

        let (warm_action, warm_stamp, warm_overlay_fills) =
            observe_reuse_action(&mut viewport, case, "warm", &fixture);
        assert_eq!(warm_action, Reuse, "{} warm action", case.label());
        assert_eq!(warm_stamp, cold_stamp, "{} warm raster stamp", case.label());

        apply_composition_only_scrollbar_overlay(&mut fixture);
        let (composition_action, composition_stamp, composition_overlay_fills) =
            observe_reuse_action(&mut viewport, case, "composition-only", &fixture);
        assert_eq!(
            composition_action,
            Reuse,
            "{} composition-only action",
            case.label()
        );
        assert_eq!(
            composition_stamp.identity,
            cold_stamp.identity,
            "{} composition-only RetainedSurfaceRasterIdentity",
            case.label()
        );
        assert_eq!(
            composition_stamp,
            cold_stamp,
            "{} composition-only raster stamp",
            case.label()
        );
        assert_eq!(
            warm_overlay_fills,
            Vec::<ScrollbarOverlayFillObservation>::new(),
            "{} warm row keeps the scrollbar overlay hidden",
            case.label(),
        );
        assert_eq!(
            composition_overlay_fills,
            expected_scrollbar_overlay_fills(case),
            "{} composition-only row must emit the exact track and thumb fill ops",
            case.label(),
        );

        let mut content_viewport = Viewport::new();
        let mut content_fixture = reuse_fixture(case, 20.0);
        let (content_cold_action, content_baseline_stamp, _) = observe_reuse_action(
            &mut content_viewport,
            case,
            "content-baseline",
            &content_fixture,
        );
        assert_eq!(
            content_cold_action,
            Reraster,
            "{} independent content baseline action",
            case.label(),
        );
        apply_content_change_in_place(&mut content_fixture, case);
        let (content_action, content_stamp, _) = observe_reuse_action(
            &mut content_viewport,
            case,
            "content-change",
            &content_fixture,
        );
        assert_eq!(
            content_action,
            Reraster,
            "{} content-change action",
            case.label()
        );
        assert_eq!(
            content_stamp.identity,
            content_baseline_stamp.identity,
            "{} content change preserves resident identity",
            case.label()
        );
        assert_ne!(
            content_stamp,
            content_baseline_stamp,
            "{} content change must alter the raster stamp",
            case.label()
        );
    }
}
