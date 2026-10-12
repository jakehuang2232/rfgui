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
        let _ = record_surface_dag_frame_artifact_cached(
            &arena,
            &roots,
            &properties,
            &generations,
            &mut cache,
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
    let cached = record_surface_dag_frame_artifact_cached(
        &arena,
        &roots,
        &properties,
        &generations,
        &mut cache,
    );
    let fresh = record_surface_dag_frame_artifact(&arena, &roots, &properties, &generations);
    assert_eq!(
        format!("{cached:?}"),
        format!("{fresh:?}"),
        "external color drift must reach the cached recording"
    );
}

#[test]
fn native_noop_ticks_and_render_dirty_consumption_preserve_install_observation() {
    let (mut arena, roots, root, text) = prepared_fixed_owning_inline_text_root();
    let (properties, generations) = sync_identity(&arena, &roots);
    let mut cache = RecordingCache::default();
    let cold = record_surface_dag_frame_artifact_cached(
        &arena,
        &roots,
        &properties,
        &generations,
        &mut cache,
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
    let warm = record_surface_dag_frame_artifact_cached(
        &arena,
        &roots,
        &properties,
        &generations,
        &mut cache,
    );
    assert_eq!(format!("{warm:?}"), format!("{cold:?}"));
}
