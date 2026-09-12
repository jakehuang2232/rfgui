use super::complete_persistent_pair_witness;

#[test]
fn root_effect_pair_witness_rejects_either_missing_or_incompatible_half() {
    assert!(complete_persistent_pair_witness(true, true));
    assert!(!complete_persistent_pair_witness(true, false));
    assert!(!complete_persistent_pair_witness(false, true));
    assert!(!complete_persistent_pair_witness(false, false));
}
