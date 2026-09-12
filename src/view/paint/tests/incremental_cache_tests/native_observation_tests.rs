use super::*;
use std::{cell::Cell, rc::Rc};

#[derive(Clone)]
struct LiveColor(Rc<Cell<[f32; 4]>>);
impl crate::style::ColorLike for LiveColor {
    fn box_clone(&self) -> Box<dyn crate::style::ColorLike> {
        Box::new(self.clone())
    }
    fn to_rgba_f32(&self) -> [f32; 4] {
        self.0.get()
    }
}

#[test]
fn native_install_observation_polls_external_text_color_without_dirty_notification() {
    let (mut arena, roots, root, text) = prepared_fixed_owning_inline_text_root();
    let color = LiveColor(Rc::new(Cell::new([1., 0., 0., 1.])));
    arena
        .get_mut(text)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Text>()
        .unwrap()
        .set_color(color.clone());
    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .mark_layout_dirty();
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    for key in [root, text] {
        arena.clear_element_dirty_flags(key, DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    let (properties, generations) = sync_identity(&arena, &roots);
    let mut cache = RecordingCache::default();
    for _ in 0..2 {
        let _ = artifact(
            record_surface_dag_frame_artifact_cached(
                &arena,
                &roots,
                &properties,
                &generations,
                &mut cache,
            )
            .unwrap(),
        );
    }
    let revision = arena.mutation_revision(text);
    color.0.set([0., 0., 1., 1.]);
    assert_eq!(arena.mutation_revision(text), revision);
    assert!(
        arena
            .get(text)
            .unwrap()
            .element
            .local_dirty_flags()
            .is_empty()
    );
    let before = Element::inline_root_witness_checks_for_test();
    assert!(matches!(
        record_surface_dag_frame_artifact_cached(
            &arena,
            &roots,
            &properties,
            &generations,
            &mut cache
        )
        .unwrap(),
        FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_)
    ));
    assert!(
        Element::inline_root_witness_checks_for_test() > before,
        "external color drift must revalidate the installed IFC paint input"
    );
    assert!(matches!(
        record_surface_dag_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto
        )
        .unwrap(),
        FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_)
    ));
}

#[test]
fn native_noop_ticks_and_render_dirty_consumption_preserve_install_observation() {
    let (mut arena, roots, root, text) = prepared_fixed_owning_inline_text_root();
    let (properties, generations) = sync_identity(&arena, &roots);
    let mut cache = RecordingCache::default();
    let cold = artifact(
        record_surface_dag_frame_artifact_cached(
            &arena,
            &roots,
            &properties,
            &generations,
            &mut cache,
        )
        .unwrap(),
    );
    let revisions = [root, text].map(|key| arena.mutation_revision(key));
    let capture = arena.capture_render_changes();
    let now = crate::time::Instant::now();
    assert!(!crate::view::base_component::tick_animation_frames(
        &mut arena, &roots, now
    ));
    assert!(
        !crate::view::base_component::tick_post_layout_animation_frames(&mut arena, &roots, now)
    );
    for key in [root, text] {
        arena.clear_element_dirty_flags(key, NodeArena::render_consumption_mask());
    }
    arena.commit_render_changes(capture);
    assert_eq!(
        [root, text].map(|key| arena.mutation_revision(key)),
        revisions
    );
    let before = Element::inline_root_witness_checks_for_test();
    let warm = artifact(
        record_surface_dag_frame_artifact_cached(
            &arena,
            &roots,
            &properties,
            &generations,
            &mut cache,
        )
        .unwrap(),
    );
    assert_eq!(Element::inline_root_witness_checks_for_test(), before);
    assert_eq!(format!("{warm:?}"), format!("{cold:?}"));
}

#[test]
fn unrelated_native_mutation_does_not_revalidate_an_unchanged_install() {
    let (mut arena, roots, _, _) = prepared_fixed_owning_inline_text_root();
    let unrelated = arena.insert(crate::view::node_arena::Node::new(Box::new(
        Element::new_with_id(0xbeef_9001, 0., 0., 20., 16.),
    )));
    let (properties, generations) = sync_identity(&arena, &roots);
    let mut cache = RecordingCache::default();
    let _ = artifact(
        record_surface_dag_frame_artifact_cached(
            &arena,
            &roots,
            &properties,
            &generations,
            &mut cache,
        )
        .unwrap(),
    );
    arena
        .get_mut(unrelated)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .set_background_color(Color::rgb(0, 0, 255));
    let before = Element::inline_root_witness_checks_for_test();
    let _ = artifact(
        record_surface_dag_frame_artifact_cached(
            &arena,
            &roots,
            &properties,
            &generations,
            &mut cache,
        )
        .unwrap(),
    );
    assert_eq!(Element::inline_root_witness_checks_for_test(), before);
}
