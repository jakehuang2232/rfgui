#[test]
fn text_wgsl_validates() {
    let source = include_str!("../../../shader/text.wgsl");
    let module = naga29::front::wgsl::parse_str(source).expect("parse text.wgsl");
    naga29::valid::Validator::new(
        naga29::valid::ValidationFlags::default(),
        naga29::valid::Capabilities::default(),
    )
    .validate(&module)
    .expect("validate text.wgsl");
}

#[test]
fn text_wgsl_snap_tolerance_matches_the_cpu_mirror() {
    let source = include_str!("../../../shader/text.wgsl");
    let module = naga29::front::wgsl::parse_str(source).expect("parse text.wgsl");
    let (_, constant) = module
        .constants
        .iter()
        .find(|(_, constant)| constant.name.as_deref() == Some("TEXT_SNAP_TOLERANCE"))
        .expect("text.wgsl declares TEXT_SNAP_TOLERANCE");
    match module.global_expressions[constant.init] {
        naga29::Expression::Literal(naga29::Literal::F32(value)) => {
            assert_eq!(value, super::TEXT_SNAP_TOLERANCE)
        }
        ref other => panic!("TEXT_SNAP_TOLERANCE is not an f32 literal: {other:?}"),
    }
}
