use super::{DirtyFlags, DirtyPassMask};

#[test]
fn dirty_pass_masks_encode_phase_4a_dependencies() {
    assert_eq!(DirtyPassMask::LAYOUT, DirtyFlags::LAYOUT);

    let placement = DirtyFlags::PLACE
        .union(DirtyFlags::BOX_MODEL)
        .union(DirtyFlags::HIT_TEST);
    assert_eq!(DirtyPassMask::PLACEMENT, placement);
    assert!(!DirtyPassMask::PLACEMENT.intersects(DirtyFlags::LAYOUT));
    assert!(!DirtyPassMask::PLACEMENT.intersects(DirtyFlags::PAINT));
    assert!(!DirtyPassMask::PLACEMENT.intersects(DirtyFlags::COMPOSITE));

    assert_eq!(DirtyPassMask::BOX_MODEL, DirtyFlags::BOX_MODEL);
    assert_eq!(DirtyPassMask::HIT_TEST, DirtyFlags::HIT_TEST);
    assert_eq!(DirtyPassMask::PAINT, DirtyFlags::PAINT);
    assert_eq!(DirtyPassMask::COMPOSITE, DirtyFlags::COMPOSITE);
    assert!(!DirtyPassMask::PAINT.intersects(DirtyPassMask::PLACEMENT));
    assert!(!DirtyPassMask::PAINT.intersects(DirtyFlags::COMPOSITE));

    assert_eq!(
        DirtyPassMask::RUNTIME,
        DirtyPassMask::PLACEMENT
            .union(DirtyPassMask::PAINT)
            .union(DirtyPassMask::COMPOSITE)
    );
    assert_eq!(
        DirtyPassMask::RUNTIME,
        DirtyFlags::PLACE
            .union(DirtyFlags::BOX_MODEL)
            .union(DirtyFlags::HIT_TEST)
            .union(DirtyFlags::PAINT)
            .union(DirtyFlags::COMPOSITE)
    );
    assert!(!DirtyPassMask::RUNTIME.intersects(DirtyFlags::LAYOUT));
    assert!(DirtyPassMask::RUNTIME.contains(DirtyFlags::COMPOSITE));
    assert!(DirtyFlags::ALL.contains(DirtyFlags::COMPOSITE));
}
