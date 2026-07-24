use super::*;

#[test]
fn baked_scroll_compiler_rejects_malicious_geometry_clip_and_scrollbar_tokens() {
    let (arena, root, child, properties, generations) = fixture();
    let scroll_id = ScrollNodeId(root);
    let clip_id = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let scroll = properties.scroll_snapshot_for(scroll_id).unwrap();
    let contents_clip = properties
        .clip_snapshot_for(Some(clip_id))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let witness = PaintBakedScrollHostWitness::new(root, child, scroll, clip_id).unwrap();
    let artifact = record_baked_scroll_host_artifact_for_plan(
        &arena,
        &[root],
        &properties,
        &generations,
        witness,
    )
    .unwrap();
    let validates = |scroll, clip| {
        super::super::super::compiler::validate_baked_scroll_host_artifact(
            &artifact, root, child, scroll, clip,
        )
        .is_some()
    };
    assert!(validates(scroll, contents_clip));

    let mut malicious = scroll;
    malicious.scrollbar_overlay.paint_state =
        crate::view::base_component::ScrollbarPaintStateWitness::OpaqueNow;
    assert!(!validates(malicious, contents_clip));
    malicious = scroll;
    malicious.scrollbar_overlay.paint_state =
        crate::view::base_component::ScrollbarPaintStateWitness::TranslucentNow;
    assert!(!validates(malicious, contents_clip));
    malicious = scroll;
    malicious.offset.y = f32::NAN;
    assert!(!validates(malicious, contents_clip));
    malicious = scroll;
    malicious.offset.y = -1.0;
    assert!(!validates(malicious, contents_clip));
    malicious = scroll;
    malicious.offset.y = malicious.content_size.height;
    assert!(!validates(malicious, contents_clip));
    malicious = scroll;
    malicious.viewport.width = malicious.content_size.width + 1.0;
    assert!(!validates(malicious, contents_clip));
    malicious = scroll;
    malicious.viewport.x = -1.0;
    assert!(!validates(malicious, contents_clip));
    malicious = scroll;
    malicious.content_size.width = f32::NAN;
    assert!(!validates(malicious, contents_clip));
    malicious = scroll;
    malicious.layout_content_bounds_at_zero.x += 1.0;
    assert!(!validates(malicious, contents_clip));
    malicious = scroll;
    malicious.configured_axis = crate::view::base_component::ScrollAxisSnapshot::Horizontal;
    assert!(!validates(malicious, contents_clip));
    malicious = scroll;
    malicious.id = ScrollNodeId(child);
    assert!(!validates(malicious, contents_clip));
    malicious = scroll;
    malicious.owner = child;
    assert!(!validates(malicious, contents_clip));
    malicious = scroll;
    malicious.parent = Some(ScrollNodeId(child));
    assert!(!validates(malicious, contents_clip));
    malicious = scroll;
    malicious.generation = 0;
    assert!(!validates(malicious, contents_clip));
    malicious = scroll;
    let crate::view::base_component::ScrollContentsClipWitness::ExactRect(mut wrong_scissor) =
        malicious.contents_clip;
    wrong_scissor[0] += 1;
    malicious.contents_clip =
        crate::view::base_component::ScrollContentsClipWitness::ExactRect(wrong_scissor);
    assert!(!validates(malicious, contents_clip));

    let mut malicious_clip = contents_clip;
    malicious_clip.parent = Some(crate::view::compositor::property_tree::ClipNodeId {
        owner: root,
        role: crate::view::compositor::property_tree::ClipNodeRole::SelfClip,
    });
    assert!(!validates(scroll, malicious_clip));
    malicious_clip = contents_clip;
    malicious_clip.id.role = crate::view::compositor::property_tree::ClipNodeRole::SelfClip;
    assert!(!validates(scroll, malicious_clip));
    malicious_clip = contents_clip;
    malicious_clip.owner = child;
    assert!(!validates(scroll, malicious_clip));
    malicious_clip = contents_clip;
    malicious_clip.behavior = crate::view::compositor::property_tree::ClipBehavior::Replace;
    assert!(!validates(scroll, malicious_clip));
    malicious_clip = contents_clip;
    malicious_clip.generation = 0;
    assert!(!validates(scroll, malicious_clip));
    malicious_clip = contents_clip;
    malicious_clip.logical_scissor[0] += 1;
    assert!(!validates(scroll, malicious_clip));
}
