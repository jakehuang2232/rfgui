use super::*;
use crate::view::debug::{
    DebugRetainedAutoFallbackSnapshot, DebugRetainedAutoFrameSnapshot,
    DebugRetainedAutoNodeSnapshot,
};

const ELEMENT: &str = "rfgui::view::base_component::element::Element";
const IMAGE: &str = "rfgui::view::base_component::image::Image";

fn boundary(
    element_type: Option<&'static str>,
    reason: &'static str,
) -> DebugRetainedAutoFallbackSnapshot {
    DebugRetainedAutoFallbackSnapshot {
        stage: DebugFallbackStage::Selection,
        category: DebugFallbackCategory::UnsupportedHost,
        detail: DebugFallbackDetail::Boundary { reason },
        node: None,
        stable_id: None,
        element_type,
        bounds: None,
    }
}

fn snapshot(fallbacks: Vec<DebugRetainedAutoFallbackSnapshot>) -> DebugRetainedAutoSnapshot {
    snapshot_with_nodes(fallbacks, Vec::new())
}

fn snapshot_with_nodes(
    fallbacks: Vec<DebugRetainedAutoFallbackSnapshot>,
    nodes: Vec<DebugRetainedAutoNodeSnapshot>,
) -> DebugRetainedAutoSnapshot {
    let fallback_count = fallbacks.len() as u64;
    DebugRetainedAutoSnapshot {
        frame: DebugRetainedAutoFrameSnapshot {
            attempt_id: 7,
            requested_mode: DebugPaintRequestedMode::RetainedAuto,
            selected_authority: DebugFramePaintAuthority::Legacy,
            disposition: DebugFrameDisposition::FellBackToLegacy,
            fallback_stages: fallbacks,
            statistics: DebugRetainedAutoStatistics {
                fallback_count,
                ..DebugRetainedAutoStatistics::default()
            },
        },
        nodes,
        surfaces: Vec::new(),
    }
}

#[test]
fn identical_group_keys_collapse_into_one_counted_entry() {
    let census = DebugFallbackCensus::from_snapshot(&snapshot(vec![
        boundary(Some(IMAGE), "missing-image"),
        boundary(Some(IMAGE), "missing-image"),
        boundary(Some(IMAGE), "missing-image"),
    ]));

    assert_eq!(census.entries.len(), 1);
    assert_eq!(census.entries[0].count, 3);
    assert_eq!(census.entries[0].element_type, IMAGE);
    assert_eq!(census.total_fallbacks(), 3);
}

#[test]
fn distinct_reasons_on_one_element_type_stay_separate() {
    let census = DebugFallbackCensus::from_snapshot(&snapshot(vec![
        boundary(Some(ELEMENT), "transform"),
        boundary(Some(ELEMENT), "child-clip"),
        boundary(Some(ELEMENT), "child-clip"),
    ]));

    assert_eq!(census.entries.len(), 2);
    assert_eq!(
        census
            .entries
            .iter()
            .map(|entry| (fallback_detail_label(&entry.detail), entry.count))
            .collect::<Vec<_>>(),
        vec![("child-clip".to_string(), 2), ("transform".to_string(), 1)]
    );
    assert_eq!(census.total_for_element_type(ELEMENT), 3);
}

#[test]
fn entries_are_ordered_by_descending_count_then_group_key() {
    let census = DebugFallbackCensus::from_snapshot(&snapshot(vec![
        boundary(Some(IMAGE), "missing-image"),
        boundary(Some(ELEMENT), "transform"),
        boundary(Some(ELEMENT), "transform"),
        boundary(Some(ELEMENT), "self-clip"),
    ]));

    assert_eq!(
        census
            .entries
            .iter()
            .map(|entry| (entry.short_element_type(), entry.count))
            .collect::<Vec<_>>(),
        vec![("Element", 2), ("Element", 1), ("Image", 1)]
    );
}

#[test]
fn ordering_is_independent_of_snapshot_fallback_order() {
    let forward = DebugFallbackCensus::from_snapshot(&snapshot(vec![
        boundary(Some(ELEMENT), "transform"),
        boundary(Some(IMAGE), "missing-image"),
        boundary(Some(ELEMENT), "transform"),
    ]));
    let reversed = DebugFallbackCensus::from_snapshot(&snapshot(vec![
        boundary(Some(IMAGE), "missing-image"),
        boundary(Some(ELEMENT), "transform"),
        boundary(Some(ELEMENT), "transform"),
    ]));

    assert_eq!(forward.entries, reversed.entries);
}

#[test]
fn fallbacks_without_owner_identity_are_counted_as_unattributed() {
    let census =
        DebugFallbackCensus::from_snapshot(&snapshot(vec![boundary(None, "unknown-host")]));

    assert_eq!(census.entries.len(), 1);
    assert_eq!(census.entries[0].element_type, UNATTRIBUTED_ELEMENT_TYPE);
    assert_eq!(census.total_fallbacks(), 1);
}

#[test]
fn node_mirrored_fallbacks_are_not_counted_twice() {
    // The capture builder pushes each boundary into `frame.fallback_stages`
    // and additionally mirrors it onto the owning node when identity
    // resolved. Counting both would double count every resolved boundary.
    let fallback = boundary(Some(IMAGE), "missing-image");
    let node = DebugRetainedAutoNodeSnapshot {
        node: None,
        stable_id: None,
        element_type: IMAGE,
        bounds: None,
        coverage: Vec::new(),
        resident_action: None,
        fallbacks: vec![fallback.clone()],
    };
    let census =
        DebugFallbackCensus::from_snapshot(&snapshot_with_nodes(vec![fallback], vec![node]));

    assert_eq!(census.total_fallbacks(), 1);
}

#[test]
fn retained_success_requires_presented_and_non_legacy_authority() {
    let mut presented_retained = snapshot(Vec::new());
    presented_retained.frame.selected_authority = DebugFramePaintAuthority::Artifact;
    presented_retained.frame.disposition = DebugFrameDisposition::Presented;
    assert!(DebugFallbackCensus::from_snapshot(&presented_retained).is_retained_success());

    // A rejected frame is not a successful Legacy fallback either.
    let mut rejected = presented_retained.clone();
    rejected.frame.disposition = DebugFrameDisposition::Rejected;
    assert!(!DebugFallbackCensus::from_snapshot(&rejected).is_retained_success());

    let mut presented_legacy = presented_retained;
    presented_legacy.frame.selected_authority = DebugFramePaintAuthority::Legacy;
    assert!(!DebugFallbackCensus::from_snapshot(&presented_legacy).is_retained_success());
}

#[test]
fn an_earlier_candidate_rejection_does_not_decide_the_frame() {
    // Contract 1.2: a rejection entry may coexist with a retained authority.
    let mut retained_with_rejection = snapshot(vec![boundary(Some(ELEMENT), "transform")]);
    retained_with_rejection.frame.selected_authority = DebugFramePaintAuthority::Artifact;
    retained_with_rejection.frame.disposition = DebugFrameDisposition::Presented;

    let census = DebugFallbackCensus::from_snapshot(&retained_with_rejection);
    assert!(census.is_retained_success());
    assert_eq!(census.total_fallbacks(), 1);
}

#[test]
fn empty_attempt_reports_no_fallbacks() {
    let census = DebugFallbackCensus::from_snapshot(&snapshot(Vec::new()));

    assert!(census.entries.is_empty());
    assert_eq!(census.total_fallbacks(), 0);
    assert!(census.render_table().contains("(no fallbacks)"));
}

#[test]
fn rendered_table_lists_every_group_with_its_count() {
    let census = DebugFallbackCensus::from_snapshot(&snapshot(vec![
        boundary(Some(ELEMENT), "child-clip"),
        boundary(Some(ELEMENT), "child-clip"),
        boundary(Some(IMAGE), "missing-image"),
    ]));

    let table = census.render_table();
    assert!(table.contains("attempt=7"));
    assert!(table.contains("authority=legacy"));
    assert!(table.contains("disposition=fell-back-to-legacy"));
    assert!(table.contains("count"));
    assert!(table.contains("Element"));
    assert!(table.contains("child-clip"));
    assert!(table.contains("Image"));
    assert!(table.contains("missing-image"));
    // Attempt line, four statistic lines, the column header, two group rows.
    assert_eq!(table.lines().count(), 8);
}

#[test]
fn capacity_detail_keeps_its_numbers_in_the_group_key() {
    let capacity = |requested: u64| DebugRetainedAutoFallbackSnapshot {
        stage: DebugFallbackStage::Preparation,
        category: DebugFallbackCategory::Capacity,
        detail: DebugFallbackDetail::Capacity {
            resource: "surface-bytes",
            requested,
            limit: 4096,
        },
        node: None,
        stable_id: None,
        element_type: Some(ELEMENT),
        bounds: None,
    };
    let census = DebugFallbackCensus::from_snapshot(&snapshot(vec![
        capacity(8192),
        capacity(8192),
        capacity(9000),
    ]));

    assert_eq!(census.entries.len(), 2);
    assert_eq!(census.entries[0].count, 2);
    assert_eq!(
        fallback_detail_label(&census.entries[0].detail),
        "surface-bytes 8192/4096"
    );
}

#[test]
fn short_element_type_strips_the_module_path() {
    assert_eq!(short_element_type(ELEMENT), "Element");
    assert_eq!(short_element_type("Element"), "Element");
    assert_eq!(
        short_element_type(UNATTRIBUTED_ELEMENT_TYPE),
        UNATTRIBUTED_ELEMENT_TYPE
    );
}

#[test]
fn rendered_table_reports_property_node_counts() {
    // Candidate selection is gated on these counts, so a rejection that names
    // no node is only explainable with them in view.
    let mut with_shape = snapshot(Vec::new());
    with_shape.frame.statistics.transform_nodes = 3;
    with_shape.frame.statistics.effect_nodes = 0;
    with_shape.frame.statistics.scroll_nodes = 1;

    let table = DebugFallbackCensus::from_snapshot(&with_shape).render_table();

    assert!(table.contains("property-nodes transform=3 effect=0 scroll=1"));
}

fn owned_boundary(
    element_type: Option<&'static str>,
    reason: &'static str,
    stable_id: u64,
) -> DebugRetainedAutoFallbackSnapshot {
    DebugRetainedAutoFallbackSnapshot {
        stable_id: Some(stable_id),
        ..boundary(element_type, reason)
    }
}

#[test]
fn owners_are_collected_sorted_and_deduplicated() {
    let census = DebugFallbackCensus::from_snapshot(&snapshot(vec![
        owned_boundary(Some(ELEMENT), "scroll-boundary", 30),
        owned_boundary(Some(ELEMENT), "scroll-boundary", 10),
        owned_boundary(Some(ELEMENT), "scroll-boundary", 30),
        owned_boundary(Some(ELEMENT), "scroll-boundary", 20),
    ]));

    assert_eq!(census.entries.len(), 1);
    assert_eq!(census.entries[0].owners, vec![10, 20, 30]);
}

#[test]
fn owners_let_one_node_be_traced_across_several_rules() {
    // The question this answers: are four rules four separate nodes, or one
    // node tripping four consecutive checks?
    let census = DebugFallbackCensus::from_snapshot(&snapshot(vec![
        owned_boundary(Some(ELEMENT), "ancestor-boundary-not-consumed", 7),
        owned_boundary(Some(ELEMENT), "receiver-state-cursor-mismatch", 7),
        owned_boundary(Some(ELEMENT), "root-boundary-schedule-unsupported", 9),
    ]));

    let owners_for = |code: &str| {
        census
            .entries
            .iter()
            .find(|entry| fallback_detail_label(&entry.detail) == code)
            .map(|entry| entry.owners.clone())
            .expect("group is present")
    };

    assert_eq!(owners_for("ancestor-boundary-not-consumed"), vec![7]);
    assert_eq!(owners_for("receiver-state-cursor-mismatch"), vec![7]);
    assert_eq!(owners_for("root-boundary-schedule-unsupported"), vec![9]);
}

#[test]
fn a_group_with_no_resolved_owner_reports_none() {
    let census =
        DebugFallbackCensus::from_snapshot(&snapshot(vec![boundary(None, "unknown-host")]));

    assert!(census.entries[0].owners.is_empty());
    assert!(census.render_table().contains(" -"));
}

#[test]
fn rendered_owners_are_capped_with_a_remainder() {
    let census = DebugFallbackCensus::from_snapshot(&snapshot(
        (1..=9)
            .map(|id| owned_boundary(Some(ELEMENT), "scroll-boundary", id))
            .collect(),
    ));

    let table = census.render_table();
    assert!(table.contains("#1 #2 #3 #4 #5 #6 +3"), "{table}");
}

#[test]
fn owner_order_does_not_depend_on_snapshot_order() {
    let ids = [5_u64, 2, 9];
    let forward = DebugFallbackCensus::from_snapshot(&snapshot(
        ids.iter()
            .map(|id| owned_boundary(Some(ELEMENT), "scroll-boundary", *id))
            .collect(),
    ));
    let reversed = DebugFallbackCensus::from_snapshot(&snapshot(
        ids.iter()
            .rev()
            .map(|id| owned_boundary(Some(ELEMENT), "scroll-boundary", *id))
            .collect(),
    ));

    assert_eq!(forward.entries, reversed.entries);
    assert_eq!(forward.entries[0].owners, vec![2, 5, 9]);
}
