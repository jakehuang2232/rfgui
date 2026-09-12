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
