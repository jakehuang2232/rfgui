use super::*;

#[test]
fn same_owner_transform_effect_scroll_role_stamp_is_canonical_and_tamper_evident() {
    let (transform, contract, inner, outer) = same_owner_transform_effect_scroll_role_fixture();
    let accepts = |stamp: &SameOwnerTransformEffectScrollRasterRoleStamp| {
        stamp.is_canonical_for_roles(transform, &contract, &inner)
    };

    assert!(accepts(&outer));

    let mut stable = outer.clone();
    stable.stable_id ^= 1;
    assert!(!accepts(&stable));

    let mut transform_role = outer.clone();
    transform_role.transform = TransformNodeId(transform_role.content_root);
    assert!(!accepts(&transform_role));

    let mut effect_role = outer.clone();
    effect_role.effect = EffectNodeId(effect_role.content_root);
    assert!(!accepts(&effect_role));

    let mut scroll_role = outer.clone();
    scroll_role.scroll = ScrollNodeId(scroll_role.content_root);
    assert!(!accepts(&scroll_role));

    let mut clip_role = outer.clone();
    clip_role.contents_clip.owner = clip_role.content_root;
    assert!(!accepts(&clip_role));

    let mut content = outer;
    content.content_stable_id ^= 1;
    assert!(!accepts(&content));
}
