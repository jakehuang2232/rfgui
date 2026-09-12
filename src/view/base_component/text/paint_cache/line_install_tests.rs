use super::super::super::TextIfcOwnedLine;
use super::*;

#[test]
fn line_install_memo_tracks_both_allocations_and_translation_without_dirty_flags() {
    let local: Arc<[TextIfcOwnedLine]> = vec![TextIfcOwnedLine {
        rect: crate::ui::Rect {
            x: 1.0,
            y: 2.0,
            width: 8.0,
            height: 12.0,
        },
        text_rect: crate::ui::Rect {
            x: 1.0,
            y: 3.0,
            width: 7.0,
            height: 10.0,
        },
        char_range: 0..2,
        caret_xs: vec![1.0, 4.0, 8.0],
    }]
    .into();
    let mut installed: Arc<[TextIfcOwnedLine]> = local
        .iter()
        .cloned()
        .map(|line| line.shifted(3.0, -2.0))
        .collect::<Vec<_>>()
        .into();
    let slot = RefCell::new(None);
    for _ in 0..2 {
        assert!(TextLineInstallMemo::matches(
            &slot,
            &installed,
            &local,
            [3.0, -2.0]
        ));
    }
    assert!(!TextLineInstallMemo::matches(
        &slot,
        &installed,
        &local,
        [4.0, -2.0]
    ));
    assert!(TextLineInstallMemo::matches(
        &slot,
        &installed,
        &local,
        [3.0, -2.0]
    ));
    Arc::make_mut(&mut installed)[0].caret_xs[1] += 1.0;
    assert!(!TextLineInstallMemo::matches(
        &slot,
        &installed,
        &local,
        [3.0, -2.0]
    ));
    let mut replacement = local.clone();
    Arc::make_mut(&mut replacement)[0].caret_xs[1] += 1.0;
    assert!(TextLineInstallMemo::matches(
        &slot,
        &installed,
        &replacement,
        [3.0, -2.0]
    ));
    Arc::make_mut(&mut replacement)[0].char_range = 1..3;
    assert!(!TextLineInstallMemo::matches(
        &slot,
        &installed,
        &replacement,
        [3.0, -2.0]
    ));
}
