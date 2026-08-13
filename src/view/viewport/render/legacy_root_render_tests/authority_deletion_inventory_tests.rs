use super::*;

fn register_stage_c_authority_payload<T>() -> &'static str {
    std::any::type_name::<T>()
        .rsplit("::")
        .next()
        .expect("a Rust type has a terminal name")
}

fn auto_authority_variant_payloads(
    source: &str,
) -> std::collections::BTreeMap<String, Option<String>> {
    // This declaration reader intentionally accepts the current braced,
    // single-line payload shape. A multi-line payload type is read as `None`
    // and fails the exact mapping below. A unit variant is skipped here, so
    // the exhaustive matcher remains the compiler-enforced protection for
    // that shape and must not be treated as redundant with this parser.
    let body = source
        .split_once("enum AutoAuthorityDecision {")
        .expect("render.rs declares AutoAuthorityDecision")
        .1
        .split_once("\n}")
        .expect("the authority declaration is brace-terminated")
        .0;
    let mut variants = std::collections::BTreeMap::new();
    let mut current = None;
    let mut payload = None;
    for line in body.lines().map(str::trim) {
        if let Some(name) = line.strip_suffix(" {") {
            current = Some(name.to_string());
            payload = None;
        } else if line == "}," {
            let name = current.take().expect("a variant block is open");
            assert!(variants.insert(name, payload.take()).is_none());
        } else if let Some((field, ty)) =
            line.strip_suffix(',').and_then(|line| line.split_once(':'))
            && field != "trace"
        {
            assert!(payload.is_none(), "one authority variant owns one payload");
            payload = Some(
                ty.trim()
                    .rsplit("::")
                    .next()
                    .expect("a payload type has a terminal name")
                    .to_string(),
            );
        }
    }
    assert!(current.is_none(), "every authority variant block is closed");
    variants
}

fn retained_authority_label(decision: AutoAuthorityDecision) -> Option<&'static str> {
    // The strings are diagnostic names, not a behavioral contract. The
    // contract is this match's exhaustiveness over all nine retained and two
    // non-retained variants, including unit variants the parser cannot see.
    match decision {
        AutoAuthorityDecision::NativeScrollForest { .. } => Some("native-scroll-forest"),
        AutoAuthorityDecision::PropertyBoundaryDagScene { .. } => {
            Some("property-boundary-dag-scene")
        }
        AutoAuthorityDecision::DirectScrollTransformScene { .. } => {
            Some("direct-scroll-transform-scene")
        }
        AutoAuthorityDecision::PropertyScrollScene { .. } => Some("property-scroll-scene"),
        AutoAuthorityDecision::FrameRootScrollScene { .. } => Some("frame-root-scroll-scene"),
        AutoAuthorityDecision::TransformScrollScene { .. } => Some("transform-scroll-scene"),
        AutoAuthorityDecision::EffectScrollScene { .. } => Some("effect-scroll-scene"),
        AutoAuthorityDecision::TransformEffectScrollScene { .. } => {
            Some("transform-effect-scroll-scene")
        }
        AutoAuthorityDecision::PropertyScene { .. } => Some("property-scene"),
        AutoAuthorityDecision::Artifact { .. } | AutoAuthorityDecision::Legacy { .. } => None,
    }
}

#[test]
fn stage_c_deletion_inventory_closes_auto_authority_variants_and_payloads() {
    let payloads: std::collections::BTreeSet<&str> =
        [
            register_stage_c_authority_payload::<crate::view::paint::FramePaintPlan>(),
            register_stage_c_authority_payload::<
                crate::view::paint::ValidatedPropertyBoundaryDagScene,
            >(),
            register_stage_c_authority_payload::<
                crate::view::paint::ValidatedDirectScrollTransformTransaction,
            >(),
            register_stage_c_authority_payload::<crate::view::paint::ValidatedPropertyScrollScene>(
            ),
            register_stage_c_authority_payload::<crate::view::paint::ValidatedFrameRootScrollScene>(
            ),
            register_stage_c_authority_payload::<crate::view::paint::ValidatedTransformScrollScene>(
            ),
            register_stage_c_authority_payload::<
                crate::view::paint::ValidatedEffectScrollSceneCheckpoint,
            >(),
            register_stage_c_authority_payload::<
                crate::view::paint::ValidatedTransformEffectScrollScene,
            >(),
        ]
        .into_iter()
        .collect();
    assert_eq!(
        payloads,
        [
            "FramePaintPlan",
            "ValidatedDirectScrollTransformTransaction",
            "ValidatedEffectScrollSceneCheckpoint",
            "ValidatedFrameRootScrollScene",
            "ValidatedPropertyBoundaryDagScene",
            "ValidatedPropertyScrollScene",
            "ValidatedTransformEffectScrollScene",
            "ValidatedTransformScrollScene",
        ]
        .into_iter()
        .collect(),
        "all eight unique retained authority payload types must be deleted together",
    );

    let actual = auto_authority_variant_payloads(include_str!("../../render.rs"));
    let expected = [
        ("Artifact", Some("RecordedArtifactCandidate")),
        (
            "DirectScrollTransformScene",
            Some("ValidatedDirectScrollTransformTransaction"),
        ),
        (
            "EffectScrollScene",
            Some("ValidatedEffectScrollSceneCheckpoint"),
        ),
        (
            "FrameRootScrollScene",
            Some("ValidatedFrameRootScrollScene"),
        ),
        ("Legacy", None),
        ("NativeScrollForest", Some("FramePaintPlan")),
        (
            "PropertyBoundaryDagScene",
            Some("ValidatedPropertyBoundaryDagScene"),
        ),
        ("PropertyScene", Some("FramePaintPlan")),
        ("PropertyScrollScene", Some("ValidatedPropertyScrollScene")),
        (
            "TransformEffectScrollScene",
            Some("ValidatedTransformEffectScrollScene"),
        ),
        (
            "TransformScrollScene",
            Some("ValidatedTransformScrollScene"),
        ),
    ]
    .into_iter()
    .map(|(variant, payload)| (variant.to_string(), payload.map(str::to_string)))
    .collect();
    assert_eq!(
        actual, expected,
        "the nine retained authority variants, their payload mapping, and the two non-retained variants form one closed declaration",
    );

    let exhaustive_matcher: fn(AutoAuthorityDecision) -> Option<&'static str> =
        retained_authority_label;
    let _ = exhaustive_matcher;
}
