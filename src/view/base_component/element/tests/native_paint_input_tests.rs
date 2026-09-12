use super::*;
thread_local! { static REPLAYS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
pub(crate) fn note_replay() {
    REPLAYS.with(|v| v.set(v.get() + 1));
}

#[derive(Clone)]
struct LiveColor(std::rc::Rc<std::cell::Cell<[f32; 4]>>);
impl crate::style::ColorLike for LiveColor {
    fn box_clone(&self) -> Box<dyn crate::style::ColorLike> {
        Box::new(self.clone())
    }
    fn to_rgba_f32(&self) -> [f32; 4] {
        self.0.get()
    }
}

#[test]
fn native_paint_capsule_reads_live_colors_and_rejects_invalid_offsets() {
    let mut arena = new_test_arena();
    let owner = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xbead1, 0., 0., 20., 16.)),
    );
    let node = arena.get(owner).unwrap();
    let element = node.element.as_any().downcast_ref::<Element>().unwrap();
    let context = crate::view::paint::PaintRecordingContext::default();
    let first = element.prepared_self_paint_record(owner, &context).unwrap();
    let before = REPLAYS.with(std::cell::Cell::get);
    let warm = element.prepared_self_paint_record(owner, &context).unwrap();
    assert_eq!(warm.payload_identity, first.payload_identity);
    assert_eq!(REPLAYS.with(std::cell::Cell::get), before + 1);
    drop(node);

    let color = LiveColor(std::rc::Rc::new(std::cell::Cell::new([1., 0., 0., 1.])));
    {
        let mut node = arena.get_mut(owner).unwrap();
        node.element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .set_background_color(color.clone());
    }
    let node = arena.get(owner).unwrap();
    let element = node.element.as_any().downcast_ref::<Element>().unwrap();
    let red = element.prepared_self_paint_record(owner, &context).unwrap();
    color.0.set([0., 0., 1., 1.]); // No style setter, generation or dirty notification.
    let before = REPLAYS.with(std::cell::Cell::get);
    let blue = element.prepared_self_paint_record(owner, &context).unwrap();
    assert_ne!(red.payload_identity, blue.payload_identity);
    assert_eq!(
        REPLAYS.with(std::cell::Cell::get),
        before,
        "changed resolved color must rebuild"
    );
    let invalid = crate::view::paint::PaintRecordingContext {
        paint_offset: [f32::NAN, 0.],
        ..context
    };
    assert!(element.prepared_self_paint_record(owner, &invalid).is_err());
    assert_eq!(
        element
            .prepared_self_paint_record(owner, &context)
            .unwrap()
            .payload_identity,
        blue.payload_identity
    );
}

#[test]
fn native_shadow_capsule_tracks_raw_geometry_and_every_shadow_parameter() {
    let mut arena = new_test_arena();
    let owner = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xbead2, 0., 0., 20., 16.)),
    );
    let mut node = arena.get_mut(owner).unwrap();
    let element = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
    element.box_shadows = vec![BoxShadow::default()];
    let context = crate::view::paint::PaintRecordingContext::default();
    let _ = element.prepared_self_paint_record(owner, &context).unwrap();
    let edits: &[fn(&mut Element)] = &[
        |e| e.box_shadows[0].offset_x = 2.,
        |e| e.box_shadows[0].offset_y = 3.,
        |e| e.box_shadows[0].blur = 4.,
        |e| e.box_shadows[0].spread = 1.,
        |e| e.box_shadows[0].inset = true,
        |e| e.box_shadows[0].color = crate::style::StyleColor::Srgb(Color::rgb(0, 0, 255)),
        |e| e.layout_state.layout_size.width = 24.,
        |e| e.layout_state.layout_position.x = -2.,
        |e| e.opacity = 0.5,
    ];
    for (index, edit) in edits.iter().enumerate() {
        edit(element); // Direct mutation deliberately bypasses dirty reporting.
        let before = REPLAYS.with(std::cell::Cell::get);
        let actual = element.prepared_self_paint_record(owner, &context).unwrap();
        assert_eq!(REPLAYS.with(std::cell::Cell::get), before, "edit {index}");
        element.paint_recording_inputs.borrow_mut().take();
        let fresh = element.prepared_self_paint_record(owner, &context).unwrap();
        assert_eq!(
            actual.payload_identity, fresh.payload_identity,
            "edit {index}"
        );
        let warm = element.prepared_self_paint_record(owner, &context).unwrap();
        assert!(
            std::sync::Arc::ptr_eq(&warm.shadows, &fresh.shadows),
            "edit {index}"
        );
    }
}

#[test]
fn child_mask_capsule_rechecks_geometry_order_partition_and_capability() {
    use crate::view::compositor::property_tree::PropertyTreeState;
    use crate::view::paint::{PaintContentRevision, PaintNodePhase, PaintRecordingContext};
    let mut arena = new_test_arena();
    let mut element = Element::new_with_id(0xbead3, 0., 0., 100., 80.);
    element.border_radii = CornerRadii::uniform(8.);
    let owner = commit_element(&mut arena, Box::new(element));
    let first = commit_child(&mut arena, owner, Box::new(Element::new(0., 0., 10., 10.)));
    let second = commit_child(&mut arena, owner, Box::new(Element::new(0., 0., 10., 10.)));
    let context = PaintRecordingContext::default();
    let plan = |arena: &crate::view::node_arena::NodeArena, context: &PaintRecordingContext| {
        let node = arena.get(owner).unwrap();
        node.element
            .retained_child_mask_plan(arena, context)
            .unwrap()
    };
    let cold = plan(&arena, &context);
    let warm = plan(&arena, &context);
    assert_eq!(cold.in_scope_children(), &[first, second]);
    assert_eq!(
        cold.in_scope_children().as_ptr(),
        warm.in_scope_children().as_ptr()
    );

    // Direct mutations intentionally bypass setters and dirty notifications.
    {
        let mut node = arena.get_mut(owner).unwrap();
        let element = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
        element.children.swap(0, 1);
    }
    let reordered = plan(&arena, &context);
    assert_eq!(reordered.in_scope_children(), &[second, first]);
    assert_ne!(
        reordered.in_scope_children().as_ptr(),
        warm.in_scope_children().as_ptr()
    );
    {
        let mut node = arena.get_mut(first).unwrap();
        let element = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
        element.computed_style.position = Position::absolute().clip(ClipMode::AnchorParent);
    }
    let partitioned = plan(&arena, &context);
    assert_eq!(partitioned.in_scope_children(), &[second]);
    assert_eq!(partitioned.overflow_children(), &[first]);
    let shifted = PaintRecordingContext {
        paint_offset: [4., 2.],
        ..context
    };
    let translated = plan(&arena, &shifted);
    let metadata = |mask: &crate::view::paint::RetainedChildMaskPlan| {
        mask.metadata(
            owner,
            PaintNodePhase::BeforeChildren,
            PropertyTreeState::default(),
            PaintContentRevision {
                self_paint_revision: 1,
                composite_revision: 1,
                topology_revision: 1,
            },
        )
    };
    assert_ne!(
        metadata(&translated).payload_identity,
        metadata(&partitioned).payload_identity
    );
    {
        let node = arena.get(owner).unwrap();
        let element = node.element.as_any().downcast_ref::<Element>().unwrap();
        element.child_mask_recording_inputs.borrow_mut().take();
    }
    let fresh = plan(&arena, &shifted);
    assert_eq!(
        metadata(&translated).payload_identity,
        metadata(&fresh).payload_identity
    );
    let a = metadata(&translated).bounds;
    let b = metadata(&fresh).bounds;
    assert_eq!(
        [a.x, a.y, a.width, a.height].map(f32::to_bits),
        [b.x, b.y, b.width, b.height].map(f32::to_bits)
    );
    let invalid = PaintRecordingContext {
        paint_offset: [f32::NAN, 0.],
        ..context
    };
    let node = arena.get(owner).unwrap();
    assert!(
        node.element
            .retained_child_mask_plan(&arena, &invalid)
            .is_none()
    );
    drop(node);
    {
        let mut node = arena.get_mut(owner).unwrap();
        let element = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
        element.scroll_direction = ScrollDirection::Vertical;
    }
    let node = arena.get(owner).unwrap();
    assert!(
        node.element
            .retained_child_mask_plan(&arena, &context)
            .is_none(),
        "a warm plan cannot grant scroll authority"
    );
}
