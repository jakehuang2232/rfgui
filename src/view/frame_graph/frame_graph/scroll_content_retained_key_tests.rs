use super::*;

#[test]
fn scroll_content_pair_and_failed_execution_cleanup_roles_are_exact() {
    let color = PersistentTextureKey::retained(RetainedTextureRole::ScrollContentColor, 41);
    let depth = PersistentTextureKey::retained(RetainedTextureRole::ScrollContentDepthStencil, 41);
    assert_eq!(color.depth_stencil(), Some(depth));
    assert!(is_failed_execution_retained_color_key(color));
    assert!(!is_failed_execution_retained_color_key(depth));
    assert!(!is_failed_execution_retained_color_key(
        PersistentTextureKey::Generic(41)
    ));
    assert_ne!(
        color,
        PersistentTextureKey::retained(RetainedTextureRole::ScrollHostColor, 41)
    );
    assert_ne!(
        color,
        PersistentTextureKey::retained(RetainedTextureRole::TransformedColor, 41)
    );

    let tile = PersistentTextureKey::retained_scroll_content_tile(
        RetainedTextureRole::ScrollContentColor,
        41,
        3,
        7,
    )
    .unwrap();
    let tile_depth = PersistentTextureKey::retained_scroll_content_tile(
        RetainedTextureRole::ScrollContentDepthStencil,
        41,
        3,
        7,
    )
    .unwrap();
    assert_eq!(tile.depth_stencil(), Some(tile_depth));
    assert!(is_failed_execution_retained_color_key(tile));
    assert!(!is_failed_execution_retained_color_key(tile_depth));
    assert_ne!(
        tile,
        PersistentTextureKey::retained_scroll_content_tile(
            RetainedTextureRole::ScrollContentColor,
            41,
            4,
            7,
        )
        .unwrap()
    );
    assert_ne!(tile, color);
    assert!(
        PersistentTextureKey::retained_scroll_content_tile(
            RetainedTextureRole::ScrollHostColor,
            41,
            3,
            7,
        )
        .is_none()
    );
}
