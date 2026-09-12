//! Helpers used only by unit tests.

pub(super) fn complete_persistent_pair_witness(
    color_compatible: bool,
    depth_compatible: bool,
) -> bool {
    color_compatible && depth_compatible
}
