//! Fallback census over one captured `RetainedAuto` attempt.
//!
//! The retained debug schema already records every fallback the authority
//! search produced, but only as a flat list plus overlay colors. Deciding
//! *which* native component to work on next needs the same data aggregated by
//! `(element type, stage, category, detail)`.
//!
//! This module is purely observational: it reads a finished
//! [`DebugRetainedAutoSnapshot`] and never touches authority selection,
//! artifacts, resources, or resident actions.
//!
//! # Reading the numbers
//!
//! A legacy boundary claims its whole subtree — the coverage walker does not
//! descend past a node that reported
//! `ShadowPaintRecordingCapability::Legacy`. A census is therefore a *lower
//! bound* on the blockers present in the tree: closing a boundary near the
//! root routinely reveals blockers underneath it that the previous census
//! could not observe. Re-run the census after every change rather than
//! treating one run as a total work estimate.
//!
//! [`DebugRetainedAutoStatistics`] is not a coverage measurement.
//! `legacy_nodes` is the whole arena whenever the frame fell back, and
//! `covered_nodes` counts the nodes that reached the debug capture — the
//! debug capture performs no RetainedAuto-only traversal, so
//! neither number is a per-node coverage census. Read
//! [`DebugFallbackCensus::entries`] for the actionable data.

use super::{
    DebugFallbackCategory, DebugFallbackDetail, DebugFallbackStage, DebugFrameDisposition,
    DebugFramePaintAuthority, DebugPaintRequestedMode, DebugRetainedAutoSnapshot,
    DebugRetainedAutoStatistics,
};

/// Element type reported for a fallback whose owner identity could not be
/// resolved, such as a whole-frame rejection with no owning node.
pub const UNATTRIBUTED_ELEMENT_TYPE: &str = "<unattributed>";

/// One aggregated fallback group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebugFallbackCensusEntry {
    /// Fully qualified type name of the owning host, or
    /// [`UNATTRIBUTED_ELEMENT_TYPE`].
    pub element_type: &'static str,
    pub stage: DebugFallbackStage,
    pub category: DebugFallbackCategory,
    pub detail: DebugFallbackDetail,
    /// Number of fallbacks in this attempt that share the whole group key.
    pub count: u64,
    /// Stable ids of the owners in this group, ascending and deduplicated.
    ///
    /// Empty when no owner identity resolved. Cross-referencing these tells a
    /// reader whether several rules describe one node failing repeatedly or
    /// separate nodes each failing once — a distinction the counts alone
    /// cannot make.
    pub owners: Vec<u64>,
}

impl DebugFallbackCensusEntry {
    /// Trailing path segment of [`Self::element_type`], for display.
    pub fn short_element_type(&self) -> &'static str {
        short_element_type(self.element_type)
    }
}

/// Aggregated view of one `RetainedAuto` attempt.
#[derive(Clone, Debug, PartialEq)]
pub struct DebugFallbackCensus {
    pub attempt_id: u64,
    pub requested_mode: DebugPaintRequestedMode,
    pub selected_authority: DebugFramePaintAuthority,
    pub disposition: DebugFrameDisposition,
    pub statistics: DebugRetainedAutoStatistics,
    /// Groups ordered by descending count, then by group key. The order is a
    /// pure function of the snapshot.
    pub entries: Vec<DebugFallbackCensusEntry>,
}

impl DebugFallbackCensus {
    /// Aggregate the fallbacks recorded for one attempt.
    ///
    /// Counting reads [`DebugRetainedAutoSnapshot::frame`]'s `fallback_stages`
    /// exclusively. The per-node `fallbacks` lists mirror the same records but
    /// only for owners whose identity resolved, so counting both would double
    /// count the resolved ones and still miss the unattributed ones.
    pub fn from_snapshot(snapshot: &DebugRetainedAutoSnapshot) -> Self {
        let mut entries: Vec<DebugFallbackCensusEntry> = Vec::new();
        for fallback in &snapshot.frame.fallback_stages {
            let element_type = fallback.element_type.unwrap_or(UNATTRIBUTED_ELEMENT_TYPE);
            match entries.iter_mut().find(|entry| {
                entry.element_type == element_type
                    && entry.stage == fallback.stage
                    && entry.category == fallback.category
                    && entry.detail == fallback.detail
            }) {
                Some(entry) => {
                    entry.count += 1;
                    if let Some(stable_id) = fallback.stable_id {
                        entry.owners.push(stable_id);
                    }
                }
                None => entries.push(DebugFallbackCensusEntry {
                    element_type,
                    stage: fallback.stage,
                    category: fallback.category,
                    detail: fallback.detail.clone(),
                    count: 1,
                    owners: fallback.stable_id.into_iter().collect(),
                }),
            }
        }
        for entry in &mut entries {
            entry.owners.sort_unstable();
            entry.owners.dedup();
        }
        entries.sort_by(|left, right| {
            right
                .count
                .cmp(&left.count)
                .then_with(|| left.element_type.cmp(right.element_type))
                .then_with(|| {
                    fallback_stage_label(left.stage).cmp(fallback_stage_label(right.stage))
                })
                .then_with(|| {
                    fallback_category_label(left.category)
                        .cmp(fallback_category_label(right.category))
                })
                .then_with(|| {
                    fallback_detail_label(&left.detail).cmp(&fallback_detail_label(&right.detail))
                })
        });
        Self {
            attempt_id: snapshot.frame.attempt_id,
            requested_mode: snapshot.frame.requested_mode,
            selected_authority: snapshot.frame.selected_authority,
            disposition: snapshot.frame.disposition,
            statistics: snapshot.frame.statistics.clone(),
            entries,
        }
    }

    /// Total fallbacks counted across every group.
    pub fn total_fallbacks(&self) -> u64 {
        self.entries.iter().map(|entry| entry.count).sum()
    }

    /// Fallbacks counted for one fully qualified element type.
    pub fn total_for_element_type(&self, element_type: &str) -> u64 {
        self.entries
            .iter()
            .filter(|entry| entry.element_type == element_type)
            .map(|entry| entry.count)
            .sum()
    }

    /// Whether this attempt reached the screen under a retained authority.
    ///
    /// Per the contract, retained success is a non-`Legacy` authority together
    /// with `Presented`; an earlier candidate rejection does not decide the
    /// frame.
    pub fn is_retained_success(&self) -> bool {
        matches!(self.disposition, DebugFrameDisposition::Presented)
            && !matches!(self.selected_authority, DebugFramePaintAuthority::Legacy)
    }

    /// Render a fixed-width census table.
    pub fn render_table(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "retained-auto census attempt={} requested={} authority={} disposition={}\n",
            self.attempt_id,
            requested_mode_label(self.requested_mode),
            frame_authority_label(self.selected_authority),
            frame_disposition_label(self.disposition),
        ));
        let statistics = &self.statistics;
        // `legacy_nodes` is the whole arena whenever the frame fell back, and
        // `covered_nodes` counts nodes present in the debug capture rather
        // than nodes a retained authority covered. Label both for what they
        // are so the header is not read as a coverage ratio.
        out.push_str(&format!(
            "  arena-nodes={} legacy-owned={} culled={} fallbacks={}\n",
            statistics.reachable_nodes,
            statistics.legacy_nodes,
            statistics.culled_nodes,
            statistics.fallback_count,
        ));
        out.push_str(&format!(
            "  debug-attributed-nodes={} surfaces property={} retained={}\n",
            statistics.covered_nodes, statistics.property_surfaces, statistics.retained_surfaces,
        ));
        // Property-node counts gate which candidates are attempted, so they
        // explain rejections that name no node.
        out.push_str(&format!(
            "  property-nodes transform={} effect={} scroll={}\n",
            statistics.transform_nodes, statistics.effect_nodes, statistics.scroll_nodes,
        ));
        out.push_str(&format!(
            "  chunks={} commit={} reuse={} reraster={}\n",
            statistics.artifact_chunks,
            statistics.resident_commits,
            statistics.resident_reuses,
            statistics.resident_rerasterizations,
        ));
        if self.entries.is_empty() {
            out.push_str("  (no fallbacks)\n");
            return out;
        }

        let rows = self
            .entries
            .iter()
            .map(|entry| {
                [
                    entry.count.to_string(),
                    entry.short_element_type().to_string(),
                    fallback_stage_label(entry.stage).to_string(),
                    fallback_category_label(entry.category).to_string(),
                    fallback_detail_label(&entry.detail),
                    render_owners(&entry.owners),
                ]
            })
            .collect::<Vec<_>>();
        let headers = ["count", "element", "stage", "category", "detail", "nodes"];
        let mut widths = headers.map(str::len);
        for row in &rows {
            for (width, cell) in widths.iter_mut().zip(row.iter()) {
                *width = (*width).max(cell.len());
            }
        }
        let mut push_row = |cells: [&str; 6]| {
            out.push_str("  ");
            for (index, (cell, width)) in cells.iter().zip(widths.iter()).enumerate() {
                if index + 1 == cells.len() {
                    out.push_str(cell);
                } else {
                    out.push_str(&format!("{cell:<width$}  ", width = *width));
                }
            }
            out.push('\n');
        };
        push_row(headers);
        for row in &rows {
            push_row([&row[0], &row[1], &row[2], &row[3], &row[4], &row[5]]);
        }
        out
    }
}

/// Owner stable ids for one group, capped so a wide group stays readable.
///
/// The point of showing them is cross-referencing groups, which only needs
/// enough ids to recognise a repeated set.
fn render_owners(owners: &[u64]) -> String {
    const SHOWN: usize = 6;
    if owners.is_empty() {
        return "-".to_string();
    }
    let head = owners
        .iter()
        .take(SHOWN)
        .map(|id| format!("#{id}"))
        .collect::<Vec<_>>()
        .join(" ");
    match owners.len().checked_sub(SHOWN) {
        Some(rest) if rest > 0 => format!("{head} +{rest}"),
        _ => head,
    }
}

/// Trailing path segment of a fully qualified element type name.
pub fn short_element_type(element_type: &str) -> &str {
    element_type.rsplit("::").next().unwrap_or(element_type)
}

pub fn requested_mode_label(mode: DebugPaintRequestedMode) -> &'static str {
    match mode {
        DebugPaintRequestedMode::Legacy => "legacy",
        DebugPaintRequestedMode::ArtifactCanary => "artifact-canary",
        DebugPaintRequestedMode::RetainedTransformCanary => "retained-transform-canary",
        DebugPaintRequestedMode::RetainedSurfaceTreeCanary => "retained-surface-tree-canary",
        DebugPaintRequestedMode::RetainedIsolationCanary => "retained-isolation-canary",
        DebugPaintRequestedMode::RetainedEffectTreeCanary => "retained-effect-tree-canary",
        DebugPaintRequestedMode::RetainedScrollHostCanary => "retained-scroll-host-canary",
        DebugPaintRequestedMode::RetainedScrollSceneCanary => "retained-scroll-scene-canary",
        DebugPaintRequestedMode::RetainedAuto => "retained-auto",
    }
}

pub fn frame_authority_label(authority: DebugFramePaintAuthority) -> &'static str {
    match authority {
        DebugFramePaintAuthority::Unselected => "unselected",
        DebugFramePaintAuthority::Legacy => "legacy",
        DebugFramePaintAuthority::Artifact => "artifact",
        DebugFramePaintAuthority::PropertyScene => "property-scene",
        DebugFramePaintAuthority::RetainedTransformSurface => "retained-transform-surface",
        DebugFramePaintAuthority::RetainedEffectSurface => "retained-effect-surface",
        DebugFramePaintAuthority::RetainedScrollHost => "retained-scroll-host",
        DebugFramePaintAuthority::RetainedScrollScene => "retained-scroll-scene",
        DebugFramePaintAuthority::NativeScrollForest => "native-scroll-forest",
    }
}

pub fn frame_disposition_label(disposition: DebugFrameDisposition) -> &'static str {
    match disposition {
        DebugFrameDisposition::Presented => "presented",
        DebugFrameDisposition::FellBackToLegacy => "fell-back-to-legacy",
        DebugFrameDisposition::Rejected => "rejected",
        DebugFrameDisposition::Aborted => "aborted",
    }
}

pub fn fallback_stage_label(stage: DebugFallbackStage) -> &'static str {
    match stage {
        DebugFallbackStage::Selection => "selection",
        DebugFallbackStage::Planning => "planning",
        DebugFallbackStage::Recording => "recording",
        DebugFallbackStage::Preparation => "preparation",
        DebugFallbackStage::Compilation => "compilation",
        DebugFallbackStage::Execution => "execution",
        DebugFallbackStage::Terminal => "terminal",
    }
}

pub fn fallback_category_label(category: DebugFallbackCategory) -> &'static str {
    match category {
        DebugFallbackCategory::UnsupportedHost => "unsupported-host",
        DebugFallbackCategory::PropertyTopology => "property-topology",
        DebugFallbackCategory::DeferredPaint => "deferred-paint",
        DebugFallbackCategory::LayoutTransition => "layout-transition",
        DebugFallbackCategory::Coverage => "coverage",
        DebugFallbackCategory::Validation => "validation",
        DebugFallbackCategory::Resource => "resource",
        DebugFallbackCategory::Capacity => "capacity",
        DebugFallbackCategory::Compiler => "compiler",
        DebugFallbackCategory::Runtime => "runtime",
        DebugFallbackCategory::ForcedFailure => "forced-failure",
        DebugFallbackCategory::Unknown => "unknown",
    }
}

pub fn fallback_detail_label(detail: &DebugFallbackDetail) -> String {
    match detail {
        DebugFallbackDetail::None => "-".to_string(),
        DebugFallbackDetail::Code { code } => (*code).to_string(),
        DebugFallbackDetail::CandidateCode { candidate, code } => format!("{candidate}:{code}"),
        DebugFallbackDetail::Boundary { reason } => (*reason).to_string(),
        DebugFallbackDetail::Validation { invariant } => (*invariant).to_string(),
        DebugFallbackDetail::Resource { resource } => (*resource).to_string(),
        DebugFallbackDetail::Capacity {
            resource,
            requested,
            limit,
        } => format!("{resource} {requested}/{limit}"),
    }
}

#[cfg(test)]
mod tests;
