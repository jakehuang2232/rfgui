use super::super::{Viewport, ViewportPaintRendererMode};
use crate::style::{Color, Layout, Length};
use crate::ui::{RsxNode, profile_ui_work, rsx};
use crate::view::{Element, Text};

fn geometry(
    b: &crate::view::base_component::BoxModelSnapshot,
) -> (u64, Option<u64>, [u32; 5], bool) {
    (
        b.node_id,
        b.parent_id,
        [b.x, b.y, b.width, b.height, b.border_radius].map(f32::to_bits),
        b.should_render,
    )
}

fn scene(text: &str, red: bool, width: f32, font: f32, axis: bool) -> RsxNode {
    let layout = if axis {
        Layout::flow().column().into()
    } else {
        Layout::Grid
    };
    rsx! {
        <Element style={{layout:layout,width:Length::px(width),height:Length::px(400.0)}}>
            {(0..32).map(|i| rsx! {
                <Element style={{width:Length::percent(50.0),height:Length::px(24.0),background:if i==0 && red {Color::rgb(180,40,40)} else {Color::rgb(40,60,80)}}}>
                    <Text style={{font_size:font}}>{if i==0 {text.to_owned()} else {format!("Row {i}")}}</Text>
                </Element>
            }).collect::<Vec<_>>()}
        </Element>
    }
}
fn warm(v: &mut Viewport, tree: &RsxNode) {
    v.logical_width = 640.;
    v.logical_height = 480.;
    v.render_rsx(tree).unwrap();
    v.run_layout_pass();
    v.run_layout_pass();
}

#[test]
fn color_update_does_not_measure_and_unchanged_geometry_reuses_box_models() {
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        let mut v = Viewport::new();
        v.set_paint_renderer_mode(mode);
        warm(&mut v, &scene("before", false, 400., 16., true));
        v.render_rsx(&scene("before", true, 400., 16., true))
            .unwrap();
        let (_, p) = profile_ui_work(|| v.run_layout_pass());
        assert_eq!(p.measure_calls, 0);
        // Once placement has consumed any runtime geometry flags, the next
        // unchanged pass must copy snapshots without rereading every host.
        let before = v.frame_box_models().to_vec();
        let (_, clean) = profile_ui_work(|| v.run_layout_pass());
        assert_eq!(clean.measure_calls, 0);
        assert_eq!(clean.place_calls, 0);
        assert_eq!(clean.box_model_reads, 0);
        assert_eq!(clean.box_model_reused_snapshots, before.len());
        assert_eq!(
            v.frame_box_models()
                .iter()
                .map(geometry)
                .collect::<Vec<_>>(),
            before.iter().map(geometry).collect::<Vec<_>>()
        );
    }
}

#[test]
fn local_text_only_measures_affected_branch_for_axis_and_non_axis_layout() {
    for axis in [false, true] {
        let mut v = Viewport::new();
        warm(&mut v, &scene("before", false, 400., 16., axis));
        let root = v.scene.ui_root_keys[0];
        let unchanged = v.scene.node_arena.children_of(root)[31];
        let before = v
            .scene
            .node_arena
            .get(unchanged)
            .unwrap()
            .element
            .box_model_snapshot();
        v.render_rsx(&scene("after editing", false, 400., 16., axis))
            .unwrap();
        let (_, p) = profile_ui_work(|| v.run_layout_pass());
        assert_eq!(p.measure_reuses, 31, "axis={axis}: {p:?}");
        assert!(
            p.measure_calls <= 3,
            "only root, changed row, optional text: {p:?}"
        );
        assert_eq!(
            geometry(
                &v.scene
                    .node_arena
                    .get(unchanged)
                    .unwrap()
                    .element
                    .box_model_snapshot()
            ),
            geometry(&before)
        );
    }
}

#[test]
fn parent_constraints_and_font_changes_invalidate_measured_geometry() {
    let mut v = Viewport::new();
    warm(&mut v, &scene("text", false, 400., 16., true));
    let root = v.scene.ui_root_keys[0];
    let row = v.scene.node_arena.children_of(root)[0];
    let text = v.scene.node_arena.children_of(row)[0];
    let before = v
        .scene
        .node_arena
        .get(row)
        .unwrap()
        .element
        .box_model_snapshot();
    v.render_rsx(&scene("text", false, 300., 16., true))
        .unwrap();
    let (_, p) = profile_ui_work(|| v.run_layout_pass());
    assert!(p.measure_calls >= 33);
    assert_eq!(p.measure_reuses, 0);
    let after = v
        .scene
        .node_arena
        .get(row)
        .unwrap()
        .element
        .box_model_snapshot();
    assert_eq!((before.width, after.width), (200., 150.));
    let text_before = v
        .scene
        .node_arena
        .get(text)
        .unwrap()
        .element
        .box_model_snapshot();
    v.render_rsx(&scene("text", false, 300., 24., true))
        .unwrap();
    let (_, p) = profile_ui_work(|| v.run_layout_pass());
    assert!(p.measure_calls >= 33);
    let text_after = v
        .scene
        .node_arena
        .get(text)
        .unwrap()
        .element
        .box_model_snapshot();
    assert!(text_after.height > text_before.height);
}

#[test]
fn detailed_layout_profile_preserves_measure_reasons_and_resets_next_pass() {
    use crate::view::base_component::{enable_layout_profile_scoped, layout_place_profile_enabled};
    let mut v = Viewport::new();
    warm(&mut v, &scene("before", false, 400., 16., true));
    v.render_rsx(&scene("edited", false, 400., 16., true))
        .unwrap();
    assert!(!layout_place_profile_enabled());
    {
        let _enabled = enable_layout_profile_scoped(true);
        let pass = v.run_layout_pass();
        let p = pass.place_profile;
        assert!(p.measure_ran_self_dirty + p.measure_ran_child_dirty > 0);
        assert_eq!(p.axis_solve_calls, 1);
        assert!(p.axis_replay_reject_layout > 0);
        assert!(p.ifc_candidate_calls > 0);
        // A new string exercises an actual cache miss, unlike the warmed
        // benchmark's alternating pair of already cached strings.
        assert!(p.ifc_candidate_rebuilt > 0);
        let clean = v.run_layout_pass().place_profile;
        assert_eq!(clean.axis_solve_calls, 0);
        assert_eq!(clean.ifc_candidate_calls, 0);
        assert_eq!(clean.ifc_candidate_rebuilt, 0);
        assert_eq!(
            clean.measure_ran_self_dirty + clean.measure_ran_child_dirty,
            0
        );
        assert_eq!(clean.axis_replay_reject_layout, 0);
    }
    assert!(!layout_place_profile_enabled());
}

#[test]
fn fresh_text_update_preserves_unchanged_sibling_ifc_plans() {
    use crate::view::base_component::{DirtyFlags, enable_layout_profile_scoped};
    let mut v = Viewport::new();
    warm(&mut v, &scene("before", false, 400., 16., true));
    let root = v.scene.ui_root_keys[0];
    // Model the completed frame's dirty consumption before the next update.
    super::super::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut v.scene.node_arena,
        root,
        DirtyFlags::ALL,
    );
    v.render_rsx(&scene("a fresh uncached string", false, 400., 16., true))
        .unwrap();
    let _enabled = enable_layout_profile_scoped(true);
    let p = v.run_layout_pass().place_profile;
    assert_eq!(p.assignment_restores, 31);
    assert_eq!(p.place_returned_clean, 31);
    assert_eq!(p.inline_ifc_root_install_calls, 1);
    assert_eq!(p.ifc_candidate_calls, 1);
    assert_eq!(p.ifc_candidate_rebuilt, 1);
    assert_eq!(p.ifc_rebuild_paint_dirty, 0);
}
