use super::*;

#[test]
fn platform_requests_empty_by_default() {
    let r = PlatformRequests::default();
    assert!(r.is_empty());
}
