#[cfg(test)]
mod attempts;

use super::*;

#[cfg(test)]
mod single_viewport_frame_test_support;
#[cfg(test)]
pub(crate) use single_viewport_frame_test_support::SingleViewportFrameObservation;

fn build_root_legacy(
    graph: &mut FrameGraph,
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    ctx: crate::view::base_component::UiBuildContext,
) -> crate::view::base_component::BuildState {
    arena
        .with_element_taken(root_key, |root, arena| root.build(graph, arena, ctx))
        .expect("root should exist during the build walk")
}

enum ArtifactFrameCompileOutcome {
    Compiled {
        state: crate::view::base_component::BuildState,
        eligibility: crate::view::paint::FrameArtifactEligibility,
    },
    CompileRejected(crate::view::paint::ArtifactCompileErrorKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutoAuthorityKind {
    Artifact,
    Legacy,
}

#[derive(Clone, Debug)]
enum AutoAuthorityRejection {
    Artifact {
        eligibility: crate::view::paint::FrameArtifactEligibility,
    },
    ArtifactPrepare {
        error: RecordedArtifactSurfacePrepareError,
    },
}

/// Optional diagnostics plus the unconditional prepare-rejection stage marker.
/// Each selection attempt records at most one rejection. Capture never changes
/// selection or fallback stage.
#[derive(Clone, Debug, Default)]
struct AutoAuthorityTrace {
    capture_rejections: bool,
    // Frame lifecycle consumes this even when diagnostic capture is disabled.
    artifact_prepare_rejected: bool,
    rejections: Vec<AutoAuthorityRejection>,
}

impl AutoAuthorityTrace {
    fn new(capture_rejections: bool) -> Self {
        Self {
            capture_rejections,
            artifact_prepare_rejected: false,
            rejections: Vec::new(),
        }
    }

    fn reject_artifact_prepare(&mut self, error: RecordedArtifactSurfacePrepareError) {
        self.artifact_prepare_rejected = true;
        self.capture(|| AutoAuthorityRejection::ArtifactPrepare { error });
    }

    fn capture(&mut self, rejection: impl FnOnce() -> AutoAuthorityRejection) {
        if self.capture_rejections {
            self.rejections.push(rejection());
        }
    }
}

fn auto_artifact_legacy_fallback_stage(trace: &AutoAuthorityTrace) -> PaintAuthorityFallbackStage {
    if trace.artifact_prepare_rejected {
        PaintAuthorityFallbackStage::Prepare
    } else {
        PaintAuthorityFallbackStage::Selection
    }
}

impl AutoAuthorityRejection {
    fn debug_label(&self) -> String {
        match self {
            Self::Artifact { eligibility } => format!("artifact:{:?}", eligibility.reasons),
            Self::ArtifactPrepare { error } => format!("artifact-prepare:{error:?}"),
        }
    }
}

impl AutoAuthorityKind {
    fn label(self) -> &'static str {
        match self {
            Self::Artifact => "artifact",
            Self::Legacy => "legacy",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaintAuthorityKind {
    Legacy,
    Artifact,
}

impl PaintAuthorityKind {
    fn from_auto(authority: AutoAuthorityKind) -> Self {
        match authority {
            AutoAuthorityKind::Artifact => Self::Artifact,
            AutoAuthorityKind::Legacy => Self::Legacy,
        }
    }
    fn from_named_mode(mode: ViewportPaintRendererMode) -> Self {
        match mode {
            ViewportPaintRendererMode::Legacy => Self::Legacy,
            ViewportPaintRendererMode::RetainedAuto => {
                unreachable!("automatic mode supplies its selected authority")
            }
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Artifact => "artifact",
            Self::Legacy => "legacy",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaintAuthorityFallbackStage {
    Selection,
    Build,
    Prepare,
    Compile,
    Execute,
}

impl PaintAuthorityFallbackStage {
    fn label(self) -> &'static str {
        match self {
            Self::Selection => "selection",
            Self::Build => "build",
            Self::Prepare => "prepare",
            Self::Compile => "compile",
            Self::Execute => "execute",
        }
    }
}

#[derive(Clone, Debug)]
enum PaintAuthoritySelectionRejection {
    Auto(AutoAuthorityRejection),
    Artifact(crate::view::paint::FrameArtifactEligibility),
}

impl PaintAuthoritySelectionRejection {
    fn debug_label(&self) -> String {
        match self {
            Self::Auto(r) => r.debug_label(),
            Self::Artifact(e) => format!("artifact:{:?}", e.reasons),
        }
    }
}

#[derive(Clone, Debug)]
struct PaintAuthorityTelemetry {
    requested_mode: ViewportPaintRendererMode,
    selected: PaintAuthorityKind,
    selection_rejections: Vec<PaintAuthoritySelectionRejection>,
    legacy_fallback_stage: Option<PaintAuthorityFallbackStage>,
    terminal_failure_stage: Option<PaintAuthorityFallbackStage>,
    legacy_debug_boundaries: Vec<crate::view::paint::FrameArtifactDebugBoundary>,
    legacy_boundary_owners: Vec<crate::view::node_arena::NodeKey>,
    resident_release_count: Option<usize>,
    detail: String,
}

impl PaintAuthorityTelemetry {
    fn from_selection(
        requested_mode: ViewportPaintRendererMode,
        _selection: &FramePaintSelection,
        auto: Option<(AutoAuthorityKind, AutoAuthorityTrace)>,
    ) -> Self {
        let mut candidate_debug_boundaries = auto
            .as_ref()
            .into_iter()
            .flat_map(|(_, trace)| &trace.rejections)
            .filter_map(|rejection| match rejection {
                AutoAuthorityRejection::Artifact { eligibility } => Some(eligibility),
                _ => None,
            })
            .flat_map(|eligibility| eligibility.debug_boundaries.iter().copied())
            .collect::<Vec<_>>();
        candidate_debug_boundaries.sort_unstable_by_key(|boundary| boundary.owner);
        candidate_debug_boundaries.dedup();
        let mut candidate_boundary_owners = auto
            .as_ref()
            .into_iter()
            .flat_map(|(_, trace)| &trace.rejections)
            .filter_map(|rejection| match rejection {
                AutoAuthorityRejection::Artifact { eligibility } => Some(eligibility),
                _ => None,
            })
            .flat_map(|eligibility| {
                eligibility
                    .reasons
                    .iter()
                    .filter_map(artifact_fallback_reason_owner)
            })
            .chain(
                candidate_debug_boundaries
                    .iter()
                    .map(|boundary| boundary.owner),
            )
            .collect::<Vec<_>>();
        candidate_boundary_owners.sort_unstable();
        candidate_boundary_owners.dedup();
        let (selected, selection_rejections) = if let Some((authority, trace)) = auto {
            (
                PaintAuthorityKind::from_auto(authority),
                trace
                    .rejections
                    .into_iter()
                    .map(PaintAuthoritySelectionRejection::Auto)
                    .collect(),
            )
        } else {
            let selected = PaintAuthorityKind::from_named_mode(requested_mode);
            (selected, Vec::new())
        };
        let legacy_debug_boundaries = if requested_mode == ViewportPaintRendererMode::RetainedAuto
            && selected == PaintAuthorityKind::Legacy
        {
            candidate_debug_boundaries.clone()
        } else {
            Vec::new()
        };
        let legacy_boundary_owners = if requested_mode == ViewportPaintRendererMode::RetainedAuto
            && selected == PaintAuthorityKind::Legacy
        {
            candidate_boundary_owners.clone()
        } else {
            Vec::new()
        };
        Self {
            requested_mode,
            selected,
            selection_rejections,
            legacy_fallback_stage: None,
            terminal_failure_stage: None,
            legacy_debug_boundaries,
            legacy_boundary_owners,
            resident_release_count: None,
            detail: String::new(),
        }
    }

    fn note_artifact_rejection(
        &mut self,
        eligibility: crate::view::paint::FrameArtifactEligibility,
    ) {
        self.legacy_debug_boundaries
            .extend(eligibility.debug_boundaries.iter().copied());
        self.legacy_debug_boundaries
            .sort_unstable_by_key(|boundary| boundary.owner);
        self.legacy_debug_boundaries.dedup();
        self.legacy_boundary_owners.extend(
            eligibility
                .reasons
                .iter()
                .filter_map(artifact_fallback_reason_owner),
        );
        self.legacy_boundary_owners.extend(
            eligibility
                .debug_boundaries
                .iter()
                .map(|boundary| boundary.owner),
        );
        self.legacy_boundary_owners.sort_unstable();
        self.legacy_boundary_owners.dedup();
        self.selection_rejections
            .push(PaintAuthoritySelectionRejection::Artifact(eligibility));
    }

    fn note_legacy_fallback(&mut self, stage: PaintAuthorityFallbackStage) {
        self.legacy_fallback_stage = Some(stage);
    }

    fn note_terminal_failure(&mut self, stage: PaintAuthorityFallbackStage) {
        self.terminal_failure_stage = Some(stage);
    }

    fn final_authority_is_legacy(&self) -> bool {
        self.selected == PaintAuthorityKind::Legacy || self.legacy_fallback_stage.is_some()
    }

    fn final_authority(&self) -> PaintAuthorityKind {
        if self.final_authority_is_legacy() {
            PaintAuthorityKind::Legacy
        } else {
            self.selected
        }
    }

    fn set_detail(&mut self, detail: String) {
        let mut detail = detail;
        if self.requested_mode == ViewportPaintRendererMode::RetainedAuto
            && let Some(prefix_end) = detail.find(' ')
        {
            detail = detail.split_off(prefix_end + 1);
        }
        self.detail = detail;
    }

    fn fallback_boundary_nodes(&self) -> Vec<crate::view::node_arena::NodeKey> {
        if !self.final_authority_is_legacy() {
            return Vec::new();
        }
        let mut owners = self.legacy_boundary_owners.clone();
        owners.sort_unstable();
        owners.dedup();
        owners
    }

    fn authority_label(&self) -> String {
        match self.requested_mode {
            ViewportPaintRendererMode::RetainedAuto => {
                format!("retained-auto:{}", self.final_authority().label())
            }
            ViewportPaintRendererMode::Legacy => "legacy".to_owned(),
        }
    }

    fn format_debug(&self) -> String {
        let rejections = self
            .selection_rejections
            .iter()
            .map(PaintAuthoritySelectionRejection::debug_label)
            .collect::<Vec<_>>()
            .join(";");
        let legacy_fallback = self
            .legacy_fallback_stage
            .map_or("none", PaintAuthorityFallbackStage::label);
        let terminal_failure = self
            .terminal_failure_stage
            .map_or("none", PaintAuthorityFallbackStage::label);
        let releases = self
            .resident_release_count
            .map_or_else(|| "unavailable".to_owned(), |count| count.to_string());
        format!(
            "{} requested={:?} selected={} candidate-rejections=[{}] legacy-fallback-stage={} terminal-failure-stage={} resident-releases={} detail=[{}]",
            self.authority_label(),
            self.requested_mode,
            self.final_authority().label(),
            rejections,
            legacy_fallback,
            terminal_failure,
            releases,
            self.detail,
        )
    }

    #[cfg(any(test, feature = "renderer-test-support"))]
    fn snapshot(&self) -> PaintAuthorityTelemetrySnapshot {
        PaintAuthorityTelemetrySnapshot {
            authority_label: self.authority_label(),
            selected: self.final_authority(),
            rejection_labels: self
                .selection_rejections
                .iter()
                .map(PaintAuthoritySelectionRejection::debug_label)
                .collect(),
            legacy_fallback_stage: self.legacy_fallback_stage,
            terminal_failure_stage: self.terminal_failure_stage,
            resident_release_count: self.resident_release_count,
        }
    }

    #[cfg(test)]
    fn note_resident_release_delta(&mut self, before: usize, after: usize) {
        self.resident_release_count = Some(after.saturating_sub(before));
    }
}

fn artifact_fallback_reason_owner(
    reason: &crate::view::paint::FrameArtifactFallbackReason,
) -> Option<crate::view::node_arena::NodeKey> {
    use crate::view::paint::FrameArtifactFallbackReason;

    match reason {
        FrameArtifactFallbackReason::PropertyBoundary(owner)
        | FrameArtifactFallbackReason::MissingRootEffect(owner)
        | FrameArtifactFallbackReason::InvalidRootEffect(owner)
        | FrameArtifactFallbackReason::NestedEffect(owner)
        | FrameArtifactFallbackReason::NonEffectProperty(owner)
        | FrameArtifactFallbackReason::DeferredBoundary(owner) => Some(*owner),
        FrameArtifactFallbackReason::RendererLegacy
        | FrameArtifactFallbackReason::LegacyBoundary(_)
        | FrameArtifactFallbackReason::RootCount(_)
        | FrameArtifactFallbackReason::Validation(_) => None,
    }
}

fn debug_artifact_fallback(
    reason: &crate::view::paint::FrameArtifactFallbackReason,
) -> (
    crate::view::debug::DebugFallbackCategory,
    crate::view::debug::DebugFallbackDetail,
) {
    use crate::view::debug::{DebugFallbackCategory as Category, DebugFallbackDetail as Detail};
    use crate::view::paint::FrameArtifactFallbackReason as Reason;

    let code = |code: &'static str| Detail::Code { code };
    match reason {
        Reason::RendererLegacy => (Category::Coverage, code("renderer-legacy")),
        Reason::LegacyBoundary(reason) => debug_legacy_fallback(*reason),
        Reason::PropertyBoundary(_) => (Category::PropertyTopology, code("property-boundary")),
        Reason::RootCount(_) => (Category::Coverage, code("root-count")),
        Reason::MissingRootEffect(_) => (Category::PropertyTopology, code("missing-root-effect")),
        Reason::InvalidRootEffect(_) => (Category::PropertyTopology, code("invalid-root-effect")),
        Reason::NestedEffect(_) => (Category::PropertyTopology, code("nested-effect")),
        Reason::NonEffectProperty(_) => (Category::PropertyTopology, code("non-effect-property")),
        Reason::DeferredBoundary(_) => (Category::DeferredPaint, code("deferred-boundary")),
        Reason::Validation(_) => (Category::Validation, code("coverage-validation")),
    }
}

fn debug_requested_mode(
    mode: ViewportPaintRendererMode,
) -> crate::view::debug::DebugPaintRequestedMode {
    match mode {
        ViewportPaintRendererMode::Legacy => crate::view::debug::DebugPaintRequestedMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto => {
            crate::view::debug::DebugPaintRequestedMode::RetainedAuto
        }
    }
}

fn debug_paint_authority(
    authority: PaintAuthorityKind,
) -> crate::view::debug::DebugFramePaintAuthority {
    match authority {
        PaintAuthorityKind::Legacy => crate::view::debug::DebugFramePaintAuthority::Legacy,
        PaintAuthorityKind::Artifact => crate::view::debug::DebugFramePaintAuthority::Artifact,
    }
}

fn debug_fallback_stage(
    stage: PaintAuthorityFallbackStage,
) -> crate::view::debug::DebugFallbackStage {
    use crate::view::debug::DebugFallbackStage as DebugStage;
    match stage {
        PaintAuthorityFallbackStage::Selection => DebugStage::Selection,
        PaintAuthorityFallbackStage::Build => DebugStage::Recording,
        PaintAuthorityFallbackStage::Prepare => DebugStage::Preparation,
        PaintAuthorityFallbackStage::Compile => DebugStage::Compilation,
        PaintAuthorityFallbackStage::Execute => DebugStage::Execution,
    }
}

fn debug_legacy_fallback(
    reason: crate::view::paint::LegacyPaintReason,
) -> (
    crate::view::debug::DebugFallbackCategory,
    crate::view::debug::DebugFallbackDetail,
) {
    use crate::view::debug::{DebugFallbackCategory as Category, DebugFallbackDetail as Detail};
    use crate::view::paint::LegacyPaintReason;
    let category = match reason {
        LegacyPaintReason::UnknownHost | LegacyPaintReason::HasChildren => {
            Category::UnsupportedHost
        }
        LegacyPaintReason::Transform
        | LegacyPaintReason::BoxShadow
        | LegacyPaintReason::SelfClip
        | LegacyPaintReason::ChildClip
        | LegacyPaintReason::ScrollContainer => Category::PropertyTopology,
        LegacyPaintReason::InlineIfc => Category::Coverage,
        LegacyPaintReason::Deferred => Category::DeferredPaint,
        LegacyPaintReason::LayoutTransition => Category::LayoutTransition,
        LegacyPaintReason::StatefulPaint | LegacyPaintReason::TextAreaSelection => {
            Category::Coverage
        }
        LegacyPaintReason::MissingPaintIdentity => Category::Validation,
        LegacyPaintReason::MissingPreparedInlineDecoration
        | LegacyPaintReason::MissingPreparedInlineRoot
        | LegacyPaintReason::MissingPreparedText
        | LegacyPaintReason::MissingPreparedImage
        | LegacyPaintReason::MissingPreparedSvg => Category::Resource,
    };
    (
        category,
        Detail::Boundary {
            reason: legacy_fallback_reason_label(reason),
        },
    )
}

/// One un-staged fallback record derived from a rejection payload.
///
/// `owner` is `None` for whole-scene rejections that name no node.
type RejectionDebugRecord = (
    Option<crate::view::node_arena::NodeKey>,
    crate::view::debug::DebugFallbackCategory,
    crate::view::debug::DebugFallbackDetail,
);

/// One observational fallback record derived from a candidate rejection.
struct SelectionRejectionDebugRecord {
    stage: crate::view::debug::DebugFallbackStage,
    owner: Option<crate::view::node_arena::NodeKey>,
    category: crate::view::debug::DebugFallbackCategory,
    detail: crate::view::debug::DebugFallbackDetail,
}

/// Fallback records the census coverage pass adds on top of what the artifact
/// and planner paths already reported.
///
/// The artifact path reports its boundaries through `legacy_debug_boundaries`,
/// and the coverage walk produces at most one boundary per node, so a node
/// that already carries a record would otherwise be counted twice. Existing
/// records win: they come from the authority that actually ran.
fn census_coverage_fallback_additions(
    items: &[crate::view::paint::PaintCoverageItem],
    existing: &[crate::view::debug::DebugRetainedAutoFallbackCaptureInput],
    identity: impl Fn(
        crate::view::node_arena::NodeKey,
    ) -> Option<(u64, &'static str, crate::view::debug::DebugRect)>,
) -> Vec<crate::view::debug::DebugRetainedAutoFallbackCaptureInput> {
    let mut additions: Vec<crate::view::debug::DebugRetainedAutoFallbackCaptureInput> = Vec::new();
    for item in items {
        let crate::view::paint::PaintCoverageItem::LegacyBoundary { root, reason, .. } = item
        else {
            continue;
        };
        let already_reported = existing
            .iter()
            .chain(additions.iter())
            .any(|fallback| fallback.owner == Some(*root));
        if already_reported {
            continue;
        }
        let (category, detail) = debug_legacy_fallback(*reason);
        let identity = identity(*root);
        additions.push(crate::view::debug::DebugRetainedAutoFallbackCaptureInput {
            stage: crate::view::debug::DebugFallbackStage::Recording,
            category,
            detail,
            owner: Some(*root),
            stable_id: identity.map(|identity| identity.0),
            element_type: identity.map(|identity| identity.1),
            bounds: identity.map(|identity| identity.2),
        });
    }
    additions
}

/// Artifact eligibility reasons that are not already represented by exact
/// `legacy_debug_boundaries`.
fn artifact_rejection_debug_records(
    eligibility: &crate::view::paint::FrameArtifactEligibility,
) -> Vec<RejectionDebugRecord> {
    use crate::view::paint::FrameArtifactFallbackReason;

    eligibility
        .reasons
        .iter()
        .filter(|reason| !matches!(reason, FrameArtifactFallbackReason::LegacyBoundary(_)))
        .map(|reason| {
            let (category, detail) = debug_artifact_fallback(reason);
            (artifact_fallback_reason_owner(reason), category, detail)
        })
        .collect()
}

/// The bare code inside a debug detail, ignoring which candidate raised it.
///
/// `Code` and `CandidateCode` describe the same invariant; only the latter
/// also names the grammar. Deduplication has to see through that difference.
fn fallback_detail_code(detail: &crate::view::debug::DebugFallbackDetail) -> Option<&'static str> {
    match detail {
        crate::view::debug::DebugFallbackDetail::Code { code }
        | crate::view::debug::DebugFallbackDetail::CandidateCode { code, .. } => Some(code),
        crate::view::debug::DebugFallbackDetail::None
        | crate::view::debug::DebugFallbackDetail::Boundary { .. }
        | crate::view::debug::DebugFallbackDetail::Validation { .. }
        | crate::view::debug::DebugFallbackDetail::Resource { .. }
        | crate::view::debug::DebugFallbackDetail::Capacity { .. } => None,
    }
}

/// Live-snapshot drift records the planners could not report.
///
/// A planner that rejects on the live-snapshot precondition returns before its
/// per-node validation runs, so it reports one drifting node and nothing else
/// from the whole scene. A census wants every drifting node, and existing
/// records win so the one the planner already named is not repeated.
fn census_live_snapshot_fallback_additions(
    mismatches: &[crate::view::compositor::paint_generation::LiveSnapshotMismatch],
    existing: &[crate::view::debug::DebugRetainedAutoFallbackCaptureInput],
    identity: impl Fn(
        crate::view::node_arena::NodeKey,
    ) -> Option<(u64, &'static str, crate::view::debug::DebugRect)>,
) -> Vec<crate::view::debug::DebugRetainedAutoFallbackCaptureInput> {
    let mut additions: Vec<crate::view::debug::DebugRetainedAutoFallbackCaptureInput> = Vec::new();
    for mismatch in mismatches {
        let code = mismatch.field.code();
        let detail = crate::view::debug::DebugFallbackDetail::Code { code };
        // Compare the code, not the whole detail: a planner's record carries
        // the candidate that raised it, so the same drift arrives here as
        // `CandidateCode` while this pass produces a bare `Code`.
        let already_reported = existing.iter().chain(additions.iter()).any(|fallback| {
            fallback.owner == mismatch.owner && fallback_detail_code(&fallback.detail) == Some(code)
        });
        if already_reported {
            continue;
        }
        let identity = mismatch.owner.and_then(&identity);
        additions.push(crate::view::debug::DebugRetainedAutoFallbackCaptureInput {
            stage: crate::view::debug::DebugFallbackStage::Planning,
            category: crate::view::debug::DebugFallbackCategory::Validation,
            detail,
            owner: mismatch.owner,
            stable_id: identity.map(|identity| identity.0),
            element_type: identity.map(|identity| identity.1),
            bounds: identity.map(|identity| identity.2),
        });
    }
    additions
}

fn selection_rejection_debug_records(
    rejection: &PaintAuthoritySelectionRejection,
) -> Vec<SelectionRejectionDebugRecord> {
    use crate::view::debug::DebugFallbackStage;
    let (stage, records) = match rejection {
        PaintAuthoritySelectionRejection::Auto(AutoAuthorityRejection::Artifact {
            eligibility,
        })
        | PaintAuthoritySelectionRejection::Artifact(eligibility) => (
            DebugFallbackStage::Selection,
            artifact_rejection_debug_records(eligibility),
        ),
        PaintAuthoritySelectionRejection::Auto(AutoAuthorityRejection::ArtifactPrepare {
            ..
        }) => (DebugFallbackStage::Preparation, Vec::new()),
    };
    records
        .into_iter()
        .map(|(owner, category, detail)| SelectionRejectionDebugRecord {
            stage,
            owner,
            category,
            detail,
        })
        .collect()
}

fn legacy_fallback_reason_label(reason: crate::view::paint::LegacyPaintReason) -> &'static str {
    use crate::view::paint::LegacyPaintReason;
    match reason {
        LegacyPaintReason::UnknownHost => "unknown-host",
        LegacyPaintReason::HasChildren => "has-children",
        LegacyPaintReason::Transform => "transform",
        LegacyPaintReason::BoxShadow => "box-shadow",
        LegacyPaintReason::SelfClip => "self-clip",
        LegacyPaintReason::ChildClip => "child-clip",
        LegacyPaintReason::ScrollContainer => "scroll-container",
        LegacyPaintReason::InlineIfc => "inline-ifc",
        LegacyPaintReason::Deferred => "deferred-paint",
        LegacyPaintReason::LayoutTransition => "layout-transition",
        LegacyPaintReason::StatefulPaint => "stateful-paint",
        LegacyPaintReason::TextAreaSelection => "text-area-selection",
        LegacyPaintReason::MissingPaintIdentity => "missing-paint-identity",
        LegacyPaintReason::MissingPreparedInlineDecoration => "missing-inline-decoration",
        LegacyPaintReason::MissingPreparedInlineRoot => "missing-inline-root",
        LegacyPaintReason::MissingPreparedText => "missing-text",
        LegacyPaintReason::MissingPreparedImage => "missing-image",
        LegacyPaintReason::MissingPreparedSvg => "missing-svg",
    }
}

fn retained_auto_overlay_label(
    element_type: &'static str,
    stable_id: u64,
    fallback_reason: Option<crate::view::paint::LegacyPaintReason>,
) -> String {
    let element_type = element_type.rsplit("::").next().unwrap_or(element_type);
    match fallback_reason {
        Some(reason) => format!(
            "{element_type}#{stable_id} fallback={}",
            legacy_fallback_reason_label(reason)
        ),
        None => format!("{element_type}#{stable_id}"),
    }
}

fn retained_auto_fallback_overlay_records(
    telemetry: &PaintAuthorityTelemetry,
    roots: &[crate::view::node_arena::NodeKey],
) -> Vec<(
    crate::view::node_arena::NodeKey,
    Option<crate::view::paint::LegacyPaintReason>,
)> {
    if !telemetry.final_authority_is_legacy() {
        return Vec::new();
    }
    let mut fallback_nodes = telemetry.fallback_boundary_nodes();
    if fallback_nodes.is_empty() {
        fallback_nodes.extend_from_slice(roots);
    }
    fallback_nodes
        .into_iter()
        .map(|owner| {
            let reason = telemetry
                .legacy_debug_boundaries
                .iter()
                .find_map(|boundary| {
                    (boundary.owner == owner).then_some(match boundary.kind {
                        crate::view::paint::FrameArtifactDebugBoundaryKind::Legacy(reason) => {
                            reason
                        }
                    })
                });
            (owner, reason)
        })
        .collect()
}

#[cfg(any(test, feature = "renderer-test-support"))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct PaintAuthorityTelemetrySnapshot {
    authority_label: String,
    selected: PaintAuthorityKind,
    rejection_labels: Vec<String>,
    legacy_fallback_stage: Option<PaintAuthorityFallbackStage>,
    terminal_failure_stage: Option<PaintAuthorityFallbackStage>,
    resident_release_count: Option<usize>,
}

#[cfg(any(test, feature = "renderer-test-support"))]
std::thread_local! {
    static PAINT_AUTHORITY_TEST_CAPTURE_ENABLED: std::cell::Cell<bool> =
        std::cell::Cell::new(false);
    static LAST_PAINT_AUTHORITY_TELEMETRY: std::cell::RefCell<Option<PaintAuthorityTelemetrySnapshot>> =
        std::cell::RefCell::new(None);
}

#[cfg(any(test, feature = "renderer-test-support"))]
struct PaintAuthorityTestCaptureGuard {
    previous: bool,
}

#[cfg(any(test, feature = "renderer-test-support"))]
impl Drop for PaintAuthorityTestCaptureGuard {
    fn drop(&mut self) {
        clear_paint_authority_test_snapshot();
        PAINT_AUTHORITY_TEST_CAPTURE_ENABLED.with(|enabled| enabled.set(self.previous));
    }
}

#[cfg(any(test, feature = "renderer-test-support"))]
fn enable_paint_authority_test_capture() -> PaintAuthorityTestCaptureGuard {
    let previous = PAINT_AUTHORITY_TEST_CAPTURE_ENABLED.with(|enabled| {
        let previous = enabled.get();
        enabled.set(true);
        previous
    });
    clear_paint_authority_test_snapshot();
    PaintAuthorityTestCaptureGuard { previous }
}

#[cfg(any(test, feature = "renderer-test-support"))]
fn paint_authority_test_capture_enabled() -> bool {
    PAINT_AUTHORITY_TEST_CAPTURE_ENABLED.with(std::cell::Cell::get)
}

#[cfg(not(any(test, feature = "renderer-test-support")))]
fn paint_authority_test_capture_enabled() -> bool {
    false
}

#[cfg(any(test, feature = "renderer-test-support"))]
fn store_paint_authority_test_snapshot(telemetry: &PaintAuthorityTelemetry) {
    if paint_authority_test_capture_enabled() {
        LAST_PAINT_AUTHORITY_TELEMETRY
            .with(|snapshot| snapshot.replace(Some(telemetry.snapshot())));
    }
}

#[cfg(any(test, feature = "renderer-test-support"))]
fn clear_paint_authority_test_snapshot() {
    LAST_PAINT_AUTHORITY_TELEMETRY.with(|snapshot| snapshot.borrow_mut().take());
}

#[cfg(any(test, feature = "renderer-test-support"))]
fn begin_paint_authority_telemetry_attempt() {
    if paint_authority_test_capture_enabled() {
        clear_paint_authority_test_snapshot();
    }
}

#[cfg(not(any(test, feature = "renderer-test-support")))]
fn begin_paint_authority_telemetry_attempt() {}

#[cfg(any(test, feature = "renderer-test-support"))]
fn take_paint_authority_test_snapshot() -> Option<PaintAuthorityTelemetrySnapshot> {
    LAST_PAINT_AUTHORITY_TELEMETRY.with(|snapshot| snapshot.borrow_mut().take())
}

enum RecordedArtifactPayload {
    /// Generic current-target artifact authority, fully prepared and resident
    /// sealed before payload-dependent resident staging.
    ArtifactSurface(crate::view::paint::PreparedArtifactSurfaceFrame),
}

struct RecordedArtifactCandidate {
    payload: RecordedArtifactPayload,
    eligibility: crate::view::paint::FrameArtifactEligibility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecordedArtifactSurfacePrepareError {
    UnexpectedArtifactTarget,
    RasterPlan(crate::view::paint::ArtifactSurfaceRasterPlanError),
    ResidentSeal(crate::view::paint::ArtifactSurfaceResidentSealError),
    DetachedSurfacesUnsupported {
        candidates: usize,
    },
    MissingDetachedSurface,
    UnsupportedScrollContentSurfaceRole {
        surface: crate::view::paint::SurfaceDagNodeId,
        role: crate::view::paint::RetainedSurfaceRasterRole,
    },
}

enum RecordedArtifactCandidateRejection {
    Eligibility(crate::view::paint::FrameArtifactEligibility),
    Prepare(RecordedArtifactSurfacePrepareError),
}

/// One complete generic attempt selects an owned sealed artifact or whole-frame Legacy.
/// There is no property-specific retained payload that dispatch could select after rejection.
enum RetainedAutoDecision {
    Artifact {
        candidate: RecordedArtifactCandidate,
        trace: AutoAuthorityTrace,
    },
    Legacy {
        trace: AutoAuthorityTrace,
    },
}

enum FramePaintSelection {
    Inactive,
    Auto(RetainedAutoDecision),
    AutoArtifact(RecordedArtifactCandidate),
    AutoLegacy,
}

fn retained_auto_circuit_breaker_selection(
    terminal_failure: Option<RetainedAutoTerminalFailureStage>,
    capture_trace: bool,
) -> Option<FramePaintSelection> {
    terminal_failure.map(|_| {
        FramePaintSelection::Auto(RetainedAutoDecision::Legacy {
            trace: AutoAuthorityTrace::new(capture_trace),
        })
    })
}

fn retained_auto_terminal_fallback_stage(
    stage: RetainedAutoTerminalFailureStage,
) -> PaintAuthorityFallbackStage {
    match stage {
        RetainedAutoTerminalFailureStage::Compile => PaintAuthorityFallbackStage::Compile,
        RetainedAutoTerminalFailureStage::Execute => PaintAuthorityFallbackStage::Execute,
    }
}

fn terminal_failure_stage(
    compiled: bool,
    executed: bool,
) -> Option<RetainedAutoTerminalFailureStage> {
    if !compiled {
        Some(RetainedAutoTerminalFailureStage::Compile)
    } else if !executed {
        Some(RetainedAutoTerminalFailureStage::Execute)
    } else {
        None
    }
}

fn frame_disposition(compiled: bool, executed: bool) -> FrameDisposition {
    if compiled && executed {
        FrameDisposition::SubmitAndPresent
    } else {
        FrameDisposition::Abort
    }
}

fn should_store_compile_cache(compiled: bool, executed: bool) -> bool {
    compiled && executed
}

// Per-frame generic detached color + depth payload limit (128 MiB).
// Count every materialized target, including warm reused targets, using real
// physical descriptors after DPR and content-envelope resolution. This is
// not a cap on total GPU residency, sampled resources, or Legacy rendering.
// C-3 full-window gates cover 1280x720 at DPR 1/2 and aggregate rejection.
// A budget/dimension rejection selects whole-frame Legacy directly; it must
// not retry an older retained planner with a different accounting policy.
const ARTIFACT_SURFACE_AGGREGATE_BUDGET_BYTES: u64 = 128 * 1024 * 1024;

fn artifact_surface_raster_context(
    ctx: &crate::view::base_component::UiBuildContext,
    max_texture_dimension_2d: u32,
    max_texture_bytes: u64,
) -> crate::view::paint::ArtifactSurfaceRasterContext {
    let viewport = ctx.viewport();
    crate::view::paint::ArtifactSurfaceRasterContext::new(
        viewport.scale_factor(),
        viewport.target_format(),
        ctx.paint_offset(),
        ctx.graphics_pass_context().logical_scissor_rect(),
        max_texture_dimension_2d,
        max_texture_bytes,
    )
    .expect("production artifact surface raster context is canonical")
}

fn require_detached_artifact_surface_plan(
    plan: crate::view::paint::PreparedArtifactSurfaceRasterPlan,
) -> Result<
    crate::view::paint::PreparedArtifactSurfaceRasterPlan,
    RecordedArtifactSurfacePrepareError,
> {
    if plan.nodes().is_empty() {
        return Err(RecordedArtifactSurfacePrepareError::MissingDetachedSurface);
    }
    // Keep the role domain exhaustive at compile time. All variants are
    // produced by the generic sealer; there is no legacy role admission gate.
    for node in plan.nodes() {
        match node.identity().role {
            crate::view::paint::RetainedSurfaceRasterRole::Transform
            | crate::view::paint::RetainedSurfaceRasterRole::PropertyEffect
            | crate::view::paint::RetainedSurfaceRasterRole::ScrollContent => {}
        }
    }
    Ok(plan)
}

#[derive(Clone, Copy)]
enum RecordedArtifactSurfaceRequirement {
    General,
    ZeroResident,
    Detached,
    ScrollContentOnly,
}

fn prepare_recorded_artifact_candidate(
    outcome: crate::view::paint::FrameArtifactRecordOutcome,
    raster_context: crate::view::paint::ArtifactSurfaceRasterContext,
    requirement: RecordedArtifactSurfaceRequirement,
) -> Result<RecordedArtifactCandidate, RecordedArtifactCandidateRejection> {
    match outcome {
        crate::view::paint::FrameArtifactRecordOutcome::Artifact {
            artifact,
            eligibility,
        } => {
            let payload = match artifact.target {
                crate::view::paint::PaintArtifactTarget::CurrentTarget => {
                    #[cfg(test)]
                    attempts::record("raster-plan");
                    let plan = crate::view::paint::prepare_artifact_surface_raster_plan(
                        artifact,
                        raster_context,
                    )
                    .map_err(RecordedArtifactSurfacePrepareError::RasterPlan)
                    .map_err(RecordedArtifactCandidateRejection::Prepare)?;
                    let plan = match requirement {
                        RecordedArtifactSurfaceRequirement::General if plan.nodes().is_empty() => {
                            Ok(plan)
                        }
                        RecordedArtifactSurfaceRequirement::General => {
                            require_detached_artifact_surface_plan(plan)
                        }
                        RecordedArtifactSurfaceRequirement::ZeroResident => {
                            require_zero_resident_artifact_surface_plan(plan)
                        }
                        RecordedArtifactSurfaceRequirement::Detached => {
                            require_detached_artifact_surface_plan(plan)
                        }
                        RecordedArtifactSurfaceRequirement::ScrollContentOnly => {
                            require_scroll_content_artifact_surface_plan(plan)
                        }
                    }
                    .map_err(RecordedArtifactCandidateRejection::Prepare)?;
                    #[cfg(test)]
                    attempts::record("resident-seal");
                    RecordedArtifactPayload::ArtifactSurface(
                        crate::view::paint::seal_prepared_artifact_surface_frame(plan)
                            .map_err(RecordedArtifactSurfacePrepareError::ResidentSeal)
                            .map_err(RecordedArtifactCandidateRejection::Prepare)?,
                    )
                }
                crate::view::paint::PaintArtifactTarget::RootOpacityGroup { .. } => {
                    return Err(RecordedArtifactCandidateRejection::Prepare(
                        RecordedArtifactSurfacePrepareError::UnexpectedArtifactTarget,
                    ));
                }
            };
            Ok(RecordedArtifactCandidate {
                payload,
                eligibility,
            })
        }
        crate::view::paint::FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) => {
            Err(RecordedArtifactCandidateRejection::Eligibility(eligibility))
        }
    }
}

fn require_zero_resident_artifact_surface_plan(
    plan: crate::view::paint::PreparedArtifactSurfaceRasterPlan,
) -> Result<
    crate::view::paint::PreparedArtifactSurfaceRasterPlan,
    RecordedArtifactSurfacePrepareError,
> {
    if !plan.nodes().is_empty() {
        return Err(
            RecordedArtifactSurfacePrepareError::DetachedSurfacesUnsupported {
                candidates: plan.nodes().len(),
            },
        );
    }
    Ok(plan)
}

fn require_scroll_content_artifact_surface_plan(
    plan: crate::view::paint::PreparedArtifactSurfaceRasterPlan,
) -> Result<
    crate::view::paint::PreparedArtifactSurfaceRasterPlan,
    RecordedArtifactSurfacePrepareError,
> {
    // All three plan-side rejections are unreachable for current
    // ScrollContent-only production inputs. An authored scroll container with
    // no scroll snapshot is rejected earlier by the recorder as
    // LegacyBoundary(ScrollContainer), so no empty raster plan is created. A
    // valid recording derives at least one ScrollContent surface. The detached
    // role gate therefore protects future Surface DAG roles, while the role
    // check below protects agreement between source admission and derivation.
    let plan = require_detached_artifact_surface_plan(plan)?;
    if let Some(node) = plan.nodes().iter().find(|node| {
        node.identity().role != crate::view::paint::RetainedSurfaceRasterRole::ScrollContent
    }) {
        return Err(
            RecordedArtifactSurfacePrepareError::UnsupportedScrollContentSurfaceRole {
                surface: node.source(),
                role: node.identity().role,
            },
        );
    }
    Ok(plan)
}

fn record_auto_detached_surface_candidate(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    raster_context: crate::view::paint::ArtifactSurfaceRasterContext,
    requirement: RecordedArtifactSurfaceRequirement,
) -> Result<RecordedArtifactCandidate, RecordedArtifactCandidateRejection> {
    #[cfg(test)]
    attempts::record("generic-record");
    let outcome = crate::view::paint::record_surface_dag_frame_artifact(
        arena,
        roots,
        property_trees,
        paint_generations,
        crate::view::paint::RendererMode::Auto,
    )
    .expect("automatic production selection never forces artifact recording");
    prepare_recorded_artifact_candidate(outcome, raster_context, requirement)
}

fn select_retained_auto_frame(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    ctx: &crate::view::base_component::UiBuildContext,
    artifact_surface_max_texture_dimension_2d: u32,
    artifact_surface_max_texture_bytes: u64,
    capture_trace: bool,
) -> RetainedAutoDecision {
    let mut trace = AutoAuthorityTrace::new(capture_trace);
    // Complete recording and the common plan/seal decide whether this frame
    // can execute. Property counts, host families and topology do not gate
    // this attempt. Every rejection selects whole-frame Legacy before graph
    // mutation. Never retry with a planner using different coverage or accounting.
    match record_auto_detached_surface_candidate(
        arena,
        roots,
        property_trees,
        paint_generations,
        artifact_surface_raster_context(
            ctx,
            artifact_surface_max_texture_dimension_2d,
            artifact_surface_max_texture_bytes,
        ),
        RecordedArtifactSurfaceRequirement::General,
    ) {
        Ok(candidate) => return RetainedAutoDecision::Artifact { candidate, trace },
        Err(RecordedArtifactCandidateRejection::Eligibility(eligibility)) => {
            trace.capture(|| AutoAuthorityRejection::Artifact { eligibility });
        }
        Err(RecordedArtifactCandidateRejection::Prepare(error)) => {
            trace.reject_artifact_prepare(error);
        }
    }
    RetainedAutoDecision::Legacy { trace }
}

#[cfg(test)]
fn select_retained_auto_authority(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    ctx: &crate::view::base_component::UiBuildContext,
    capture_trace: bool,
) -> RetainedAutoDecision {
    select_retained_auto_authority_with_artifact_budget_for_test(
        arena,
        roots,
        property_trees,
        paint_generations,
        ctx,
        ARTIFACT_SURFACE_AGGREGATE_BUDGET_BYTES,
        capture_trace,
    )
}

#[cfg(test)]
fn select_retained_auto_authority_with_artifact_budget_for_test(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    ctx: &crate::view::base_component::UiBuildContext,
    artifact_surface_max_texture_bytes: u64,
    capture_trace: bool,
) -> RetainedAutoDecision {
    select_retained_auto_frame(
        arena,
        roots,
        property_trees,
        paint_generations,
        ctx,
        wgpu::Limits::default().max_texture_dimension_2d,
        artifact_surface_max_texture_bytes,
        capture_trace,
    )
}

fn try_compile_auto_artifact_frame(
    viewport: &mut Viewport,
    owner: crate::view::viewport::RetainedSurfaceFrameStageOwner,
    graph: &mut FrameGraph,
    candidate: RecordedArtifactCandidate,
    ctx: &crate::view::base_component::UiBuildContext,
) -> ArtifactFrameCompileOutcome {
    let stage_is_active = viewport.retained_surface_frame_stage_owner_is_active(owner);
    debug_assert!(
        stage_is_active,
        "artifact dispatch requires an active owner and an empty pending slot"
    );
    if !stage_is_active {
        return ArtifactFrameCompileOutcome::CompileRejected(
            crate::view::paint::ArtifactCompileErrorKind::SurfaceExecution(
                crate::view::paint::ArtifactSurfaceExecutionError::InactiveFrameStageOwner,
            ),
        );
    }
    let RecordedArtifactCandidate {
        payload,
        eligibility,
    } = candidate;
    match payload {
        RecordedArtifactPayload::ArtifactSurface(frame) => {
            let artifact_ctx = crate::view::base_component::UiBuildContext::from_parts(
                ctx.viewport(),
                ctx.state_clone(),
            );
            match crate::view::paint::emit_prepared_artifact_surface_frame_from_pool(
                viewport,
                owner,
                frame,
                graph,
                artifact_ctx,
            ) {
                Ok(state) => ArtifactFrameCompileOutcome::Compiled { state, eligibility },
                Err(error) => {
                    if error
                        != crate::view::paint::ArtifactSurfaceExecutionError::InactiveFrameStageOwner
                    {
                        assert!(
                            viewport.stage_retained_surface_clear(),
                            "active artifact surface rejection must stage exactly one clear transaction"
                        );
                    }
                    ArtifactFrameCompileOutcome::CompileRejected(
                        crate::view::paint::ArtifactCompileErrorKind::SurfaceExecution(error),
                    )
                }
            }
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ArtifactSurfaceIntermediateReadbackForTest {
    pub(crate) color_key: crate::view::frame_graph::PersistentTextureKey,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Destination-space physical coordinate occupied by intermediate texel
    /// `(0, 0)`. This is the same sealed sampling fact consumed by the
    /// composite pass, not a test-side reconstruction.
    pub(crate) source_physical_origin: [f32; 2],
}

#[cfg(test)]
pub(crate) struct AutoArtifactSurfaceEmissionForTest {
    pub(crate) frame_owner: crate::view::viewport::RetainedSurfaceFrameStageOwner,
    pub(crate) surface_count: usize,
    pub(crate) aggregate_texture_bytes: u64,
    pub(crate) actions: Vec<crate::view::paint::RetainedSurfaceCompileAction>,
    pub(crate) intermediate_surfaces: Vec<ArtifactSurfaceIntermediateReadbackForTest>,
}

/// Native Stage C gate seam for both zero-resident and detached artifact
/// surface frames. Selection and dispatch both use the production functions;
/// each named gate owns the surface-count contract for its fixture, while only
/// the compact observations are test-only.
#[cfg(test)]
pub(crate) fn emit_retained_auto_artifact_surface_for_test(
    viewport: &mut Viewport,
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    graph: &mut FrameGraph,
    ctx: &crate::view::base_component::UiBuildContext,
) -> Result<AutoArtifactSurfaceEmissionForTest, String> {
    let decision =
        select_retained_auto_authority(arena, roots, property_trees, paint_generations, ctx, true);
    let RetainedAutoDecision::Artifact { candidate, trace } = decision else {
        return Err("production selector did not choose Artifact".to_owned());
    };
    let RecordedArtifactCandidate {
        payload,
        eligibility,
    } = candidate;
    let RecordedArtifactPayload::ArtifactSurface(frame) = payload else {
        return Err("production selector returned the non-surface artifact payload".to_owned());
    };
    let surface_count = frame.raster_plan().nodes().len();
    let aggregate_texture_bytes = frame
        .raster_plan()
        .nodes()
        .iter()
        .try_fold(0_u64, |total, node| {
            let color = crate::view::raster_cost::texture_desc_payload_bytes(&node.target().color);
            let depth = crate::view::raster_cost::texture_desc_payload_bytes(&node.target().depth);
            total
                .checked_add(color.bytes)
                .and_then(|bytes| bytes.checked_add(depth.bytes))
        })
        .ok_or_else(|| "artifact surface descriptor byte total overflowed".to_owned())?;
    let intermediate_surfaces = frame
        .raster_plan()
        .nodes()
        .iter()
        .filter_map(|node| {
            let (color_key, width, height, source_physical_origin) =
                node.intermediate_readback_observation_for_test()?;
            Some(ArtifactSurfaceIntermediateReadbackForTest {
                color_key,
                width,
                height,
                source_physical_origin,
            })
        })
        .collect();
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .ok_or_else(|| "artifact surface frame stage is already active".to_owned())?;
    let _ = crate::view::paint::take_last_production_actions_for_test();
    let attempt = try_compile_auto_artifact_frame(
        viewport,
        owner,
        graph,
        RecordedArtifactCandidate {
            payload: RecordedArtifactPayload::ArtifactSurface(frame),
            eligibility,
        },
        ctx,
    );
    match attempt {
        ArtifactFrameCompileOutcome::Compiled { .. } => {}
        ArtifactFrameCompileOutcome::CompileRejected(error) => {
            return Err(format!(
                "selected artifact surface compile rejected: {error:?}"
            ));
        }
    }
    let actions = crate::view::paint::take_last_production_actions_for_test();
    if actions.len() != surface_count {
        return Err(format!(
            "artifact action count drifted: surfaces={surface_count}, actions={}",
            actions.len()
        ));
    }
    if trace
        .rejections
        .iter()
        .any(|rejection| matches!(rejection, AutoAuthorityRejection::ArtifactPrepare { .. }))
    {
        return Err(format!(
            "selected artifact retained a terminal prepare rejection: {:?}",
            trace
                .rejections
                .iter()
                .map(AutoAuthorityRejection::debug_label)
                .collect::<Vec<_>>()
        ));
    }
    Ok(AutoArtifactSurfaceEmissionForTest {
        frame_owner: owner,
        surface_count,
        aggregate_texture_bytes,
        actions,
        intermediate_surfaces,
    })
}

fn finish_frame_dirty_lifecycle(
    arena: &mut crate::view::node_arena::NodeArena,
    root_keys: &[crate::view::node_arena::NodeKey],
    compiled: bool,
    executed: bool,
) {
    if !compiled || !executed {
        return;
    }

    let consumed = crate::view::base_component::DirtyFlags::PAINT
        .union(crate::view::base_component::DirtyFlags::COMPOSITE);
    for &root_key in root_keys {
        crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
            arena, root_key, consumed,
        );
    }
}

fn build_layout_pass_trace_children(
    traversal_profile: &super::frame::LayoutTraversalProfile,
    measure_ms: f64,
    measure_children: Vec<TraceRenderNode>,
    place_ms: f64,
    place_profile: &crate::view::base_component::LayoutPlaceProfile,
    collect_box_models_ms: f64,
) -> Vec<TraceRenderNode> {
    vec![
        TraceRenderNode::new(
            "sync_registered_elements".to_string(),
            traversal_profile.sync_registered_elements_ms,
        ),
        TraceRenderNode::new(
            format!(
                "dirty_refresh_before_measure (roots={})",
                traversal_profile.root_count
            ),
            traversal_profile.dirty_refresh_before_measure_ms,
        ),
        TraceRenderNode::with_children("measure", measure_ms, measure_children),
        TraceRenderNode::new(
            format!(
                "measure_clean_child_candidates (clean={}, dirty={})",
                traversal_profile.measure_candidate_clean_children,
                traversal_profile.measure_dirty_children
            ),
            0.0,
        ),
        TraceRenderNode::new(
            format!(
                "dirty_refresh_before_place (roots={})",
                traversal_profile.root_count
            ),
            traversal_profile.dirty_refresh_before_place_ms,
        ),
        TraceRenderNode::with_children(
            "place",
            place_ms,
            build_layout_place_trace_nodes(place_profile),
        ),
        TraceRenderNode::new(
            format!(
                "placement_clean_child_candidates (clean={}, dirty={})",
                traversal_profile.placement_candidate_clean_children,
                traversal_profile.placement_dirty_children
            ),
            0.0,
        ),
        TraceRenderNode::new(
            format!(
                "skipped_child_place_calls (count={})",
                traversal_profile.skipped_child_place_calls
            ),
            0.0,
        ),
        TraceRenderNode::new(
            format!(
                "collect_box_models (roots={})",
                traversal_profile.root_count
            ),
            collect_box_models_ms,
        ),
    ]
}

impl Viewport {
    /// Run a single layout pass: measure → place → collect_box_models.
    /// Returns profiling data for the pass.
    pub(super) fn run_layout_pass(&mut self) -> LayoutPassResult {
        self.run_layout_pass_with_registered_sync(true)
    }

    /// A transition-triggered second layout belongs to the same rendered
    /// frame. Resource-backed hosts were already frozen by the first pass, so
    /// repeating the arena sync here could mix two async resource generations
    /// (and even two child-slot topologies) in one frame.
    fn run_relayout_pass(&mut self) -> LayoutPassResult {
        self.run_layout_pass_with_registered_sync(false)
    }

    fn run_layout_pass_with_registered_sync(
        &mut self,
        sync_registered_elements: bool,
    ) -> LayoutPassResult {
        self.compositor.frame_box_models.clear();
        crate::view::base_component::reset_text_measure_profile();
        crate::view::base_component::reset_layout_gate_candidate_profile();

        // Take the arena out of the scene so we can pass it by &mut into
        // layout without aliasing the viewport; restore at the end.
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut traversal_profile = super::frame::LayoutTraversalProfile {
            root_count: root_keys.len(),
            ..Default::default()
        };

        let measure_started_at = Instant::now();
        let constraints = crate::view::base_component::LayoutConstraints {
            max_width: self.logical_width,
            max_height: self.logical_height,
            viewport_width: self.logical_width,
            viewport_height: self.logical_height,
            percent_base_width: Some(self.logical_width),
            percent_base_height: Some(self.logical_height),
        };
        // Flush deferred arena mutations for explicitly registered hosts.
        // Must run before measure so layout sees their current arena state.
        if sync_registered_elements {
            let sync_registered_elements_started_at = Instant::now();
            arena.sync_registered_elements();
            traversal_profile.sync_registered_elements_ms =
                sync_registered_elements_started_at.elapsed().as_secs_f64() * 1000.0;
        }
        // Refresh the per-node subtree-dirty cache once at the top of the
        // measure pass so every Element::measure / place can read
        // subtree_dirty_flags via an O(1) cache lookup instead of walking
        // its entire subtree (an O(N²) trap pre-cache).
        let dirty_refresh_before_measure_started_at = Instant::now();
        let measure_dirty_roots = root_keys
            .iter()
            .map(|&root_key| {
                arena
                    .refresh_subtree_dirty_cache(root_key)
                    .intersects(crate::view::base_component::DirtyFlags::LAYOUT)
            })
            .collect::<Vec<_>>();
        traversal_profile.dirty_refresh_before_measure_ms = dirty_refresh_before_measure_started_at
            .elapsed()
            .as_secs_f64()
            * 1000.0;
        let measure_roots_started_at = Instant::now();
        for &root_key in &root_keys {
            arena.with_element_taken(root_key, |root, arena| {
                root.measure(constraints, arena);
            });
        }
        for (&root_key, was_layout_dirty) in root_keys.iter().zip(measure_dirty_roots) {
            if was_layout_dirty {
                arena.clear_cached_arena_dirty_subtree(
                    root_key,
                    crate::view::base_component::DirtyFlags::LAYOUT,
                );
            }
        }
        traversal_profile.measure_roots_ms =
            measure_roots_started_at.elapsed().as_secs_f64() * 1000.0;
        let measure_ms = measure_started_at.elapsed().as_secs_f64() * 1000.0;
        let text_measure_profile = crate::view::base_component::take_text_measure_profile();

        let place_started_at = Instant::now();
        crate::view::base_component::reset_layout_place_profile();
        let placement = crate::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: self.logical_width,
            available_height: self.logical_height,
            viewport_width: self.logical_width,
            viewport_height: self.logical_height,
            percent_base_width: Some(self.logical_width),
            percent_base_height: Some(self.logical_height),
        };
        // Measure mutated per-node dirty bits, so refresh the cache again
        // before place so `Element::place` can read it in O(1).
        let dirty_refresh_before_place_started_at = Instant::now();
        let place_dirty_roots = root_keys
            .iter()
            .map(|&root_key| {
                arena
                    .refresh_subtree_dirty_cache(root_key)
                    .intersects(crate::view::base_component::DirtyFlags::PLACE)
            })
            .collect::<Vec<_>>();
        traversal_profile.dirty_refresh_before_place_ms = dirty_refresh_before_place_started_at
            .elapsed()
            .as_secs_f64()
            * 1000.0;
        let place_roots_started_at = Instant::now();
        for &root_key in &root_keys {
            arena.with_element_taken(root_key, |root, arena| {
                root.place(placement, arena);
            });
        }
        for (&root_key, was_place_dirty) in root_keys.iter().zip(place_dirty_roots) {
            if was_place_dirty {
                arena.clear_cached_arena_dirty_subtree(
                    root_key,
                    crate::view::base_component::DirtyFlags::PLACE,
                );
            }
        }
        traversal_profile.place_roots_ms = place_roots_started_at.elapsed().as_secs_f64() * 1000.0;
        let place_ms = place_started_at.elapsed().as_secs_f64() * 1000.0;
        let place_profile = crate::view::base_component::take_layout_place_profile();
        let gate_profile = crate::view::base_component::take_layout_gate_candidate_profile();
        traversal_profile.measure_candidate_clean_children =
            gate_profile.measure_candidate_clean_children;
        traversal_profile.measure_dirty_children = gate_profile.measure_dirty_children;
        traversal_profile.placement_candidate_clean_children =
            gate_profile.placement_candidate_clean_children;
        traversal_profile.placement_dirty_children = gate_profile.placement_dirty_children;
        traversal_profile.skipped_child_place_calls = place_profile.skipped_child_place_calls;

        self.scene.node_arena = arena;
        let collect_started_at = Instant::now();
        self.refresh_frame_box_models();
        let collect_box_models_ms = collect_started_at.elapsed().as_secs_f64() * 1000.0;
        traversal_profile.collect_box_models_ms = collect_box_models_ms;

        LayoutPassResult {
            measure_ms,
            place_ms,
            collect_box_models_ms,
            traversal_profile,
            text_measure_profile,
            place_profile,
        }
    }

    fn push_retained_auto_debug_overlay(
        &mut self,
        telemetry: Option<&PaintAuthorityTelemetry>,
        roots: &[crate::view::node_arena::NodeKey],
    ) {
        if !self.debug_options.retained_auto_overlay
            || self.paint_renderer_mode != ViewportPaintRendererMode::RetainedAuto
        {
            return;
        }
        let Some(telemetry) = telemetry else {
            return;
        };
        let scale = self.scale_factor.max(0.0001);
        let screen_w = self.gpu.surface_config.width.max(1) as f32;
        let screen_h = self.gpu.surface_config.height.max(1) as f32;
        let mut records = Vec::<(
            crate::view::node_arena::NodeKey,
            [f32; 4],
            Option<crate::view::paint::LegacyPaintReason>,
        )>::new();

        if self.debug_options.retained_auto_authority {
            records.extend(roots.iter().copied().map(|root| {
                (
                    root,
                    [45.0 / 255.0, 140.0 / 255.0, 1.0, 242.0 / 255.0],
                    None,
                )
            }));
        }
        if self.debug_options.retained_auto_fallback_reasons
            && telemetry.final_authority_is_legacy()
        {
            records.extend(
                retained_auto_fallback_overlay_records(telemetry, roots)
                    .into_iter()
                    .map(|(owner, reason)| {
                        (
                            owner,
                            [1.0, 51.0 / 255.0, 51.0 / 255.0, 242.0 / 255.0],
                            reason,
                        )
                    }),
            );
        }

        for (owner, color, fallback_reason) in records {
            let Some((snapshot, label)) = (|| {
                let node = self.scene.node_arena.get(owner)?;
                Some((
                    node.element.box_model_snapshot(),
                    retained_auto_overlay_label(
                        node.element.element_type_name(),
                        node.element.stable_id(),
                        fallback_reason,
                    ),
                ))
            })() else {
                continue;
            };
            if !snapshot.should_render {
                continue;
            }
            let (vertices, indices) = build_debug_overlay_geometry(
                &snapshot,
                scale,
                screen_w,
                screen_h,
                color,
                Some(&label),
            );
            self.push_debug_overlay_geometry(&vertices, &indices);
        }
    }

    fn retained_auto_debug_identity(
        &self,
        owner: crate::view::node_arena::NodeKey,
    ) -> Option<(u64, &'static str, crate::view::debug::DebugRect)> {
        let node = self.scene.node_arena.get(owner)?;
        let bounds = node.element.box_model_snapshot();
        Some((
            node.element.stable_id(),
            node.element.element_type_name(),
            crate::view::debug::DebugRect {
                x: bounds.x,
                y: bounds.y,
                width: bounds.width,
                height: bounds.height,
            },
        ))
    }

    fn build_retained_auto_debug_capture(
        &self,
        telemetry: &PaintAuthorityTelemetry,
        roots: &[crate::view::node_arena::NodeKey],
        compiled: bool,
        executed: bool,
    ) -> crate::view::debug::DebugRetainedAutoCaptureInput {
        use crate::view::debug::{
            DebugCoverageKind as Coverage, DebugFallbackCategory as Category,
            DebugFallbackDetail as Detail, DebugFrameDisposition as Disposition,
        };

        let disposition = if !compiled {
            Disposition::Rejected
        } else if !executed {
            Disposition::Aborted
        } else if telemetry.final_authority_is_legacy() {
            Disposition::FellBackToLegacy
        } else {
            Disposition::Presented
        };
        let final_authority = telemetry.final_authority();
        let root_coverage = match final_authority {
            PaintAuthorityKind::Legacy => Coverage::LegacyBoundary,
            PaintAuthorityKind::Artifact => Coverage::ArtifactChunk,
        };
        let mut nodes = FxHashMap::<
            crate::view::node_arena::NodeKey,
            crate::view::debug::DebugRetainedAutoNodeCaptureInput,
        >::default();
        for &root in roots {
            if let Some((stable_id, element_type, bounds)) = self.retained_auto_debug_identity(root)
            {
                nodes.insert(
                    root,
                    crate::view::debug::DebugRetainedAutoNodeCaptureInput {
                        owner: Some(root),
                        stable_id: Some(stable_id),
                        element_type,
                        bounds: Some(bounds),
                        coverage: vec![root_coverage],
                        resident_action: None,
                        fallbacks: Vec::new(),
                    },
                );
            }
        }

        let surfaces: Vec<crate::view::debug::DebugRetainedAutoSurfaceCaptureInput> = Vec::new();

        let fallback_stage = telemetry
            .legacy_fallback_stage
            .map(debug_fallback_stage)
            .unwrap_or(crate::view::debug::DebugFallbackStage::Selection);
        let mut fallbacks = Vec::new();
        if telemetry.final_authority_is_legacy() {
            for boundary in &telemetry.legacy_debug_boundaries {
                let (category, detail, coverage) = match boundary.kind {
                    crate::view::paint::FrameArtifactDebugBoundaryKind::Legacy(reason) => {
                        let (category, detail) = debug_legacy_fallback(reason);
                        (category, detail, Coverage::LegacyBoundary)
                    }
                };
                let identity = self.retained_auto_debug_identity(boundary.owner);
                let fallback = crate::view::debug::DebugRetainedAutoFallbackCaptureInput {
                    stage: fallback_stage,
                    category,
                    detail,
                    owner: Some(boundary.owner),
                    stable_id: identity.map(|identity| identity.0),
                    element_type: identity.map(|identity| identity.1),
                    bounds: identity.map(|identity| identity.2),
                };
                if let Some(node) = nodes.get_mut(&boundary.owner) {
                    if !node.coverage.contains(&coverage) {
                        node.coverage.push(coverage);
                    }
                    node.fallbacks.push(fallback.clone());
                } else if let Some((stable_id, element_type, bounds)) = identity {
                    nodes.insert(
                        boundary.owner,
                        crate::view::debug::DebugRetainedAutoNodeCaptureInput {
                            owner: Some(boundary.owner),
                            stable_id: Some(stable_id),
                            element_type,
                            bounds: Some(bounds),
                            coverage: vec![coverage],
                            resident_action: None,
                            fallbacks: vec![fallback.clone()],
                        },
                    );
                }
                fallbacks.push(fallback);
            }
        }
        // Plan-level candidate rejections. Several ladder paths return Legacy
        // without ever reaching the artifact candidate, so without these the
        // whole frame collapses into one unattributed record even though the
        // planner named the rejected node. Exact artifact LegacyBoundary
        // reasons came through `legacy_debug_boundaries` above; the remaining
        // artifact eligibility reasons are emitted here.
        if telemetry.final_authority_is_legacy() {
            for rejection in &telemetry.selection_rejections {
                for record in selection_rejection_debug_records(rejection) {
                    let identity = record
                        .owner
                        .and_then(|owner| self.retained_auto_debug_identity(owner));
                    let fallback = crate::view::debug::DebugRetainedAutoFallbackCaptureInput {
                        stage: record.stage,
                        category: record.category,
                        detail: record.detail,
                        owner: record.owner,
                        stable_id: identity.map(|identity| identity.0),
                        element_type: identity.map(|identity| identity.1),
                        bounds: identity.map(|identity| identity.2),
                    };
                    if let Some(node) = record.owner.and_then(|owner| nodes.get_mut(&owner)) {
                        if !node.coverage.contains(&Coverage::LegacyBoundary) {
                            node.coverage.push(Coverage::LegacyBoundary);
                        }
                        node.fallbacks.push(fallback.clone());
                    }
                    fallbacks.push(fallback);
                }
            }
        }
        // Preserve a generic property-boundary record only when neither an
        // exact artifact boundary nor a candidate rejection named the owner.
        for owner in telemetry.fallback_boundary_nodes() {
            if fallbacks
                .iter()
                .any(|fallback| fallback.owner == Some(owner))
            {
                continue;
            }
            let identity = self.retained_auto_debug_identity(owner);
            let fallback = crate::view::debug::DebugRetainedAutoFallbackCaptureInput {
                stage: fallback_stage,
                category: Category::PropertyTopology,
                detail: Detail::Code {
                    code: "property-boundary",
                },
                owner: Some(owner),
                stable_id: identity.map(|identity| identity.0),
                element_type: identity.map(|identity| identity.1),
                bounds: identity.map(|identity| identity.2),
            };
            if let Some(node) = nodes.get_mut(&owner) {
                if !node.coverage.contains(&Coverage::LegacyBoundary) {
                    node.coverage.push(Coverage::LegacyBoundary);
                }
                node.fallbacks.push(fallback.clone());
            }
            fallbacks.push(fallback);
        }
        // Census-only coverage pass.
        //
        // Per-node blockers are produced by the coverage manifest walk, which
        // only runs inside a candidate that gets far enough to record. Several
        // ladder paths return Legacy before any candidate records, so a
        // scroll-heavy scene yields candidate rejections and no per-node data
        // at all. Re-running the walk here is the only way to attribute
        // blockers to components on those frames.
        //
        // This is the one debug path that costs an extra traversal, so it is
        // gated on the census flag alone and never on the overlay or trace
        // options. The walk reads `&` state and calls the recording hooks the
        // contract already requires to be pure and repeatable, so it cannot
        // change authority, resources, or pixels — see
        // `coverage_manifest::tests::recording_is_side_effect_free_and_deterministic`.
        if self.debug_options.retained_auto_census && telemetry.final_authority_is_legacy() {
            let manifest = crate::view::paint::record_coverage_manifest(
                &self.scene.node_arena,
                roots,
                false,
                true,
                crate::view::paint::CoverageRecordingMode::MetadataOnly,
                &self.compositor.property_trees,
                &self.compositor.paint_generations,
            );
            let live_snapshot = census_live_snapshot_fallback_additions(
                &self.compositor.paint_generations.live_snapshot_mismatches(
                    &self.scene.node_arena,
                    roots,
                    &self.compositor.property_trees,
                ),
                &fallbacks,
                |owner| self.retained_auto_debug_identity(owner),
            );
            let additions =
                census_coverage_fallback_additions(&manifest.items, &fallbacks, |owner| {
                    self.retained_auto_debug_identity(owner)
                });
            for fallback in live_snapshot.into_iter().chain(additions) {
                if let Some(owner) = fallback.owner
                    && let Some(node) = nodes.get_mut(&owner)
                {
                    if !node.coverage.contains(&Coverage::LegacyBoundary) {
                        node.coverage.push(Coverage::LegacyBoundary);
                    }
                    node.fallbacks.push(fallback.clone());
                }
                fallbacks.push(fallback);
            }
        }
        if telemetry.final_authority_is_legacy() && fallbacks.is_empty() {
            fallbacks.push(crate::view::debug::DebugRetainedAutoFallbackCaptureInput {
                stage: fallback_stage,
                category: Category::Unknown,
                detail: Detail::Code {
                    code: "whole-frame-legacy-fallback",
                },
                owner: None,
                stable_id: None,
                element_type: None,
                bounds: None,
            });
        }
        if let Some(stage) = telemetry.terminal_failure_stage {
            let category = match stage {
                PaintAuthorityFallbackStage::Compile => Category::Compiler,
                PaintAuthorityFallbackStage::Execute => Category::Runtime,
                _ => Category::ForcedFailure,
            };
            fallbacks.push(crate::view::debug::DebugRetainedAutoFallbackCaptureInput {
                stage: debug_fallback_stage(stage),
                category,
                detail: Detail::Code {
                    code: "terminal-frame-failure",
                },
                owner: None,
                stable_id: None,
                element_type: None,
                bounds: None,
            });
        }

        let resident_reuses = surfaces
            .iter()
            .filter(|surface| {
                surface.resident_action == crate::view::debug::DebugResidentAction::Reuse
            })
            .count() as u64;
        let resident_rerasterizations = surfaces
            .iter()
            .filter(|surface| {
                surface.resident_action == crate::view::debug::DebugResidentAction::Reraster
            })
            .count() as u64;
        let mut nodes = nodes.into_iter().collect::<Vec<_>>();
        nodes.sort_unstable_by_key(|(owner, _)| *owner);
        let nodes = nodes.into_iter().map(|(_, node)| node).collect::<Vec<_>>();
        let statistics = crate::view::debug::DebugRetainedAutoStatistics {
            reachable_nodes: self.scene.node_arena.len() as u64,
            covered_nodes: nodes.len() as u64,
            artifact_chunks: u64::from(final_authority == PaintAuthorityKind::Artifact),
            property_surfaces: 0,
            retained_surfaces: surfaces.len() as u64,
            legacy_nodes: if disposition == Disposition::FellBackToLegacy {
                self.scene.node_arena.len() as u64
            } else {
                0
            },
            culled_nodes: 0,
            fallback_count: fallbacks.len() as u64,
            resident_commits: 0,
            resident_reuses,
            resident_rerasterizations,
            transform_nodes: self.compositor.property_trees.transforms.len() as u64,
            effect_nodes: self.compositor.property_trees.effects.len() as u64,
            scroll_nodes: self.compositor.property_trees.scrolls.len() as u64,
        };
        crate::view::debug::DebugRetainedAutoCaptureInput {
            frame: crate::view::debug::DebugRetainedAutoFrameCaptureInput {
                attempt_id: self.frame.frame_number,
                requested_mode: debug_requested_mode(telemetry.requested_mode),
                selected_authority: debug_paint_authority(final_authority),
                disposition,
                fallback_stages: fallbacks,
                statistics,
            },
            nodes,
            surfaces,
        }
    }

    /// Build the hierarchical trace tree from collected frame timings.
    fn build_frame_trace_tree(t: &FrameTimings, opts: &ViewportDebugOptions) -> TraceRenderNode {
        let any_detail =
            opts.trace_layout_detail || opts.trace_compile_detail || opts.trace_execute_detail;
        let layout_with_transition_ms = t.layout_total_ms;

        // --- begin_frame (expand when any detail flag is on) ---
        let begin_frame = if any_detail {
            TraceRenderNode::with_children(
                "begin_frame",
                t.begin_frame_ms,
                vec![
                    TraceRenderNode::new("acquire_surface_texture", t.begin_frame_acquire_ms),
                    TraceRenderNode::new("create_surface_view", t.begin_frame_create_view_ms),
                    TraceRenderNode::new("create_command_encoder", t.begin_frame_create_encoder_ms),
                ],
            )
        } else {
            TraceRenderNode::new("begin_frame", t.begin_frame_ms)
        };

        // --- layout ---
        let layout = if opts.trace_layout_detail {
            let layout_measure_children =
                build_text_measure_trace_nodes(&t.layout_text_measure_profile);
            let layout_traversal_children = build_layout_pass_trace_children(
                &t.layout_traversal_profile,
                t.layout_measure_ms,
                layout_measure_children,
                t.layout_place_ms,
                &t.layout_place_profile,
                t.layout_collect_box_models_ms,
            );
            let relayout_traversal_children = build_layout_pass_trace_children(
                &t.relayout_traversal_profile,
                t.relayout_measure_ms,
                Vec::new(),
                t.relayout_place_ms,
                &t.relayout_place_profile,
                t.relayout_collect_box_models_ms,
            );
            TraceRenderNode::with_children(
                "layout",
                layout_with_transition_ms,
                vec![
                    TraceRenderNode::with_children(
                        "layout_traversal",
                        t.layout_ms,
                        layout_traversal_children,
                    ),
                    TraceRenderNode::new("post_layout_transition", t.post_layout_transition_ms),
                    TraceRenderNode::with_children(
                        "relayout_after_transition",
                        t.relayout_ms,
                        vec![TraceRenderNode::with_children(
                            "layout_traversal",
                            t.relayout_measure_ms
                                + t.relayout_place_ms
                                + t.relayout_collect_box_models_ms,
                            relayout_traversal_children,
                        )],
                    ),
                ],
            )
        } else {
            TraceRenderNode::new("layout", layout_with_transition_ms)
        };

        // --- compile ---
        let compile = if opts.trace_compile_detail {
            TraceRenderNode::with_children("compile", t.compile_ms, t.compile_children.clone())
        } else {
            TraceRenderNode::new("compile", t.compile_ms)
        };

        // --- execute ---
        let execute = if opts.trace_execute_detail {
            let mut execute_children = if t.execute_ordered_passes.is_empty() {
                vec![TraceRenderNode::new(
                    format!("passes ({})", t.execute_pass_count),
                    0.0,
                )]
            } else {
                build_execute_detail_trace_nodes(t.execute_ordered_passes.clone())
            };
            if !t.execute_detail_ordered_passes.is_empty() {
                let detail_total_ms: f64 = t
                    .execute_detail_ordered_passes
                    .iter()
                    .map(|(_, ms, _)| *ms)
                    .sum();
                let detail_children =
                    build_execute_detail_trace_nodes(t.execute_detail_ordered_passes.clone());
                execute_children.push(TraceRenderNode::with_children(
                    "execute_detail",
                    detail_total_ms,
                    detail_children,
                ));
            }
            TraceRenderNode::with_children(
                format!("execute (passes={})", t.execute_pass_count),
                t.execute_ms,
                t.execute_profile_ms
                    .map(|ms| {
                        vec![TraceRenderNode::with_children(
                            "execute_graph",
                            ms,
                            execute_children,
                        )]
                    })
                    .unwrap_or_default(),
            )
        } else {
            TraceRenderNode::new(
                format!("execute (passes={})", t.execute_pass_count),
                t.execute_ms,
            )
        };

        // --- end_frame (expand when any detail flag is on) ---
        let end_frame = if any_detail {
            TraceRenderNode::with_children(
                "end_frame",
                t.end_frame_ms,
                vec![
                    TraceRenderNode::new("queue_submit", t.end_frame_submit_ms),
                    TraceRenderNode::new("present", t.end_frame_present_ms),
                ],
            )
        } else {
            TraceRenderNode::new("end_frame", t.end_frame_ms)
        };

        // Each first-level phase spans consecutive wall-clock boundaries,
        // including caller bookkeeping and failed attempts. Nested profiles
        // retain their narrower diagnostic scopes. RSX is outside total_ms.
        TraceRenderNode::with_children(
            format!("render_frame #{}", t.frame_number),
            t.rsx_build_ms + t.total_ms,
            vec![
                TraceRenderNode::new("rsx_build", t.rsx_build_ms),
                begin_frame,
                layout,
                TraceRenderNode::new("prepare_paint", t.prepare_paint_ms),
                TraceRenderNode::new("sync_properties", t.sync_properties_ms),
                TraceRenderNode::new("build_graph", t.build_graph_ms),
                compile,
                execute,
                TraceRenderNode::new("finish_render", t.finish_render_ms),
                end_frame,
            ],
        )
    }

    fn render_render_tree(
        &mut self,
        dt: f32,
        now_seconds: f64,
        semantic_now: crate::time::Instant,
    ) -> bool {
        // Profiling is deliberately a separate clock read. It may only feed
        // elapsed-time diagnostics; retained frame semantics use the sample
        // captured once by `render_rsx`.
        let profile_start = Instant::now();
        let mut phase_clock = super::frame::FramePhaseClock::new(profile_start);
        self.frame.frame_number = self.frame.frame_number.saturating_add(1);
        let frame_number = self.frame.frame_number;
        // A failed surface acquisition still represents a render attempt.
        // Clear test-only capture before `begin_frame` so callers can never
        // observe telemetry retained from the preceding successful frame.
        begin_paint_authority_telemetry_attempt();
        let begin_frame_profile = match self.begin_frame() {
            Some(profile) => profile,
            None => {
                return false;
            }
        };

        let mut timings = FrameTimings {
            begin_frame_ms: phase_clock.checkpoint_ms(),
            begin_frame_acquire_ms: begin_frame_profile.acquire_ms,
            begin_frame_create_view_ms: begin_frame_profile.create_view_ms,
            begin_frame_create_encoder_ms: begin_frame_profile.create_encoder_ms,
            rsx_build_ms: self.frame.rsx_build_ms,
            frame_number,
            ..Default::default()
        };

        // --- Layout ---
        crate::view::base_component::set_text_measure_profile_enabled(
            self.debug_options.trace_render_time,
        );
        crate::view::base_component::set_layout_place_profile_enabled(
            self.debug_options.trace_render_time,
        );
        let layout_started_at = Instant::now();
        let layout_result = self.run_layout_pass();
        timings.layout_measure_ms = layout_result.measure_ms;
        timings.layout_place_ms = layout_result.place_ms;
        timings.layout_collect_box_models_ms = layout_result.collect_box_models_ms;
        timings.layout_traversal_profile = layout_result.traversal_profile;
        timings.layout_text_measure_profile = layout_result.text_measure_profile;
        timings.layout_place_profile = layout_result.place_profile;
        timings.layout_ms = layout_started_at.elapsed().as_secs_f64() * 1000.0;

        // After layout is resolved for this frame, immediately run visual/style/scroll transitions
        // so their updated endpoints are visible in the same frame.
        let post_layout_transition_started_at = Instant::now();
        let post_layout_transition = self.run_post_layout_transitions(dt, now_seconds);
        timings.post_layout_transition_ms =
            post_layout_transition_started_at.elapsed().as_secs_f64() * 1000.0;

        // --- Relayout after transition (if needed) ---
        let relayout_started_at = Instant::now();
        if post_layout_transition.relayout_required {
            let relayout_result = self.run_relayout_pass();
            timings.relayout_measure_ms = relayout_result.measure_ms;
            timings.relayout_place_ms = relayout_result.place_ms;
            timings.relayout_collect_box_models_ms = relayout_result.collect_box_models_ms;
            timings.relayout_traversal_profile = relayout_result.traversal_profile;
            timings.relayout_place_profile = relayout_result.place_profile;
        }
        timings.relayout_ms = relayout_started_at.elapsed().as_secs_f64() * 1000.0;

        timings.layout_total_ms = phase_clock.checkpoint_ms();
        // Layout-affecting transitions (scroll, layout) can move elements
        // under a stationary pointer — re-run hover hit-test so
        // PointerEnter/PointerLeave fire without requiring a real PointerMove.
        if post_layout_transition.relayout_required {
            self.resync_pointer_hover();
        }

        // Scrollbar visibility depends on final scroll geometry. Resolve it
        // only after layout/relayout, using the viewport entry's sole
        // semantic time sample, before any property or paint observation.
        let post_layout_animation_changed = {
            let mut arena = std::mem::take(&mut self.scene.node_arena);
            let root_keys = self.scene.ui_root_keys.clone();
            let changed = crate::view::base_component::tick_post_layout_animation_frames(
                &mut arena,
                &root_keys,
                semantic_now,
            );
            self.scene.node_arena = arena;
            changed
        };

        // Final layout is now stable. Freeze resource-backed paint payloads
        // exactly once for this frame before property-tree observation and
        // paint recording. This pass cannot mutate arena
        // topology; slot changes caused by async completion wait until the
        // next frame's pre-layout sync.
        self.scene.node_arena.prepare_registered_paint_resources(
            crate::view::base_component::PaintResourcePreparationContext {
                frame_number,
                device_scale: self.scale_factor,
                now: semantic_now,
            },
        );

        #[cfg(test)]
        single_viewport_frame_test_support::run_after_resource_freeze(self);
        self.prune_gpu_paint_sources();
        timings.prepare_paint_ms = phase_clock.checkpoint_ms();

        // Observe the final resolved frame state after transition sampling
        // and any required relayout.  These shadow trees do not yet drive
        // rendering or dirty classification.
        self.sync_compositor_property_trees();
        timings.sync_properties_ms = phase_clock.checkpoint_ms();

        // --- Build frame graph ---
        self.clear_debug_overlay_geometry();
        let mut graph = FrameGraph::new();
        let mut ctx = crate::view::base_component::UiBuildContext::new(
            self.gpu.surface_config.width,
            self.gpu.surface_config.height,
            self.offscreen_format(),
            self.scale_factor,
        );
        let retained_surface_frame_owner = self.begin_retained_surface_frame_stage();
        let root_keys_for_build = self.scene.ui_root_keys.clone();
        let capture_paint_authority_telemetry = self.debug_options.trace_render_time
            || self.debug_options.retained_auto_overlay
            || self.debug_options.retained_auto_census
            || paint_authority_test_capture_enabled();
        let artifact_surface_max_texture_dimension_2d = self
            .device()
            .map(|device| device.limits().max_texture_dimension_2d)
            .unwrap_or_else(|| wgpu::Limits::default().max_texture_dimension_2d);
        let retained_auto_terminal_failure = self.retained_auto_terminal_failure;
        let frame_paint_selection = if self.paint_renderer_mode == ViewportPaintRendererMode::Legacy
        {
            FramePaintSelection::Inactive
        } else {
            retained_auto_circuit_breaker_selection(
                retained_auto_terminal_failure,
                capture_paint_authority_telemetry,
            )
            .unwrap_or_else(|| {
                FramePaintSelection::Auto(select_retained_auto_frame(
                    &self.scene.node_arena,
                    &root_keys_for_build,
                    &self.compositor.property_trees,
                    &self.compositor.paint_generations,
                    &ctx,
                    artifact_surface_max_texture_dimension_2d,
                    ARTIFACT_SURFACE_AGGREGATE_BUDGET_BYTES,
                    capture_paint_authority_telemetry,
                ))
            })
        };
        let (frame_paint_selection, auto_authority_trace) = match frame_paint_selection {
            FramePaintSelection::Auto(decision) => match decision {
                RetainedAutoDecision::Artifact { candidate, trace } => (
                    FramePaintSelection::AutoArtifact(candidate),
                    Some((AutoAuthorityKind::Artifact, trace)),
                ),
                RetainedAutoDecision::Legacy { trace } => (
                    FramePaintSelection::AutoLegacy,
                    Some((AutoAuthorityKind::Legacy, trace)),
                ),
            },
            selection => (selection, None),
        };
        let clear_uses_premultiplied_alpha = matches!(
            self.gpu.surface_config.alpha_mode,
            wgpu::CompositeAlphaMode::PostMultiplied | wgpu::CompositeAlphaMode::PreMultiplied
        );
        let mut clear_rgba = self.clear_color.to_rgba_f32();
        if clear_uses_premultiplied_alpha {
            let a = clear_rgba[3].clamp(0.0, 1.0);
            clear_rgba[0] *= a;
            clear_rgba[1] *= a;
            clear_rgba[2] *= a;
            clear_rgba[3] = a;
        }
        let auto_legacy_fallback_stage = auto_authority_trace
            .as_ref()
            .map(|(_, trace)| auto_artifact_legacy_fallback_stage(trace));
        let mut paint_authority_telemetry = capture_paint_authority_telemetry.then(|| {
            PaintAuthorityTelemetry::from_selection(
                self.paint_renderer_mode,
                &frame_paint_selection,
                auto_authority_trace,
            )
        });
        let mut dispatch_legacy_fallback_stage = auto_legacy_fallback_stage;
        if paint_authority_telemetry.is_some()
            && let Some(stage) = retained_auto_terminal_failure
        {
            dispatch_legacy_fallback_stage = Some(retained_auto_terminal_fallback_stage(stage));
        }
        #[cfg(test)]
        let retained_release_count_before = paint_authority_telemetry
            .as_ref()
            .map(|_| self.retained_surface_release_log_for_test().len());
        {
            let output = ctx.allocate_target(&mut graph);
            let output_handle = output.handle();
            ctx.set_current_target(output.clone());
            let clear_pass = crate::view::frame_graph::ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new(clear_rgba),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: output.clone(),
                    ..Default::default()
                },
            );
            if let Some(handle) = output_handle {
                ctx.set_color_target(Some(handle));
            }
            graph.add_graphics_pass(clear_pass);
            ctx.set_current_target(output);
        }
        // Take the arena out of the scene for the duration of the build
        // walk so the build chain can thread `&mut NodeArena` through
        // without fighting the outer `&mut self` borrow. Put it back
        // before returning (any early-return below restores it first).
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        // Once per frame: compact the popup stack (drop unmounted ids),
        // auto-register newly-mounted viewport-clip nodes at the top,
        // then seed `ctx`'s deferred list bottom → top so the top of the
        // stack is painted last (on top visually).
        arena.seed_defer_render_with_stack(&mut self.scene.popup_stack, &mut ctx);
        let (build_whole_frame_legacy, mut paint_authority_trace) = match frame_paint_selection {
            FramePaintSelection::AutoArtifact(candidate) => {
                let artifact_attempt = retained_surface_frame_owner.map_or_else(
                        || {
                            // A missing owner means another transaction owns
                            // the slot. Preserve it rather than staging Clear.
                            ArtifactFrameCompileOutcome::CompileRejected(
                                crate::view::paint::ArtifactCompileErrorKind::SurfaceExecution(
                                    crate::view::paint::ArtifactSurfaceExecutionError::InactiveFrameStageOwner,
                                ),
                            )
                        },
                        |owner| {
                            try_compile_auto_artifact_frame(
                                self,
                                owner,
                                &mut graph,
                                candidate,
                                &ctx,
)
                        },
                    );
                match artifact_attempt {
                    ArtifactFrameCompileOutcome::Compiled { state, eligibility } => {
                        ctx.set_state(state);
                        (
                            false,
                            format!(
                                "retained-auto authority=artifact chunks={} ops={}",
                                eligibility.chunk_count, eligibility.op_count
                            ),
                        )
                    }
                    ArtifactFrameCompileOutcome::CompileRejected(kind) => {
                        if paint_authority_telemetry.is_some() {
                            dispatch_legacy_fallback_stage =
                                Some(PaintAuthorityFallbackStage::Compile);
                        }
                        (
                            true,
                            format!("retained-auto authority=legacy compile-rejected={kind:?}"),
                        )
                    }
                }
            }
            FramePaintSelection::AutoLegacy => {
                self.stage_retained_surface_clear();
                let reason = retained_auto_terminal_failure.map_or_else(
                    || "reason=selection-rejected".to_owned(),
                    |stage| format!("reason=terminal-circuit-breaker prior={stage:?}"),
                );
                (true, format!("retained-auto authority=legacy {reason}"))
            }
            FramePaintSelection::Auto(_) => {
                unreachable!("automatic decision is flattened before frame-graph mutation")
            }
            FramePaintSelection::Inactive => {
                self.stage_retained_surface_clear();
                (true, "legacy authority=legacy".to_owned())
            }
        };
        if build_whole_frame_legacy && self.paint_renderer_mode != ViewportPaintRendererMode::Legacy
        {
            if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                telemetry.note_legacy_fallback(
                    dispatch_legacy_fallback_stage.unwrap_or(PaintAuthorityFallbackStage::Build),
                );
            }
        }
        if build_whole_frame_legacy {
            for &root_key in &root_keys_for_build {
                let child_ctx = crate::view::base_component::UiBuildContext::from_parts(
                    ctx.viewport(),
                    ctx.state_clone(),
                );
                let next_state = build_root_legacy(&mut graph, &mut arena, root_key, child_ctx);
                ctx.set_state(next_state);
            }
            while let Some(node) = ctx.next_deferred() {
                crate::view::base_component::build_node_by_key(
                    node.key,
                    node.stable_id,
                    &mut graph,
                    &mut arena,
                    &mut ctx,
                );
            }
        }
        // Build walk is done — give the arena back to the scene.
        self.scene.node_arena = arena;
        self.push_retained_auto_debug_overlay(
            paint_authority_telemetry.as_ref(),
            &root_keys_for_build,
        );
        let dependency_handle = ctx.current_target().and_then(|target| target.handle());
        if let Some(dep_handle) = dependency_handle {
            let present_pass =
                crate::view::render_pass::present_surface_pass::PresentSurfacePass::new(
                    crate::view::render_pass::present_surface_pass::PresentSurfaceParams,
                    crate::view::render_pass::present_surface_pass::PresentSurfaceInput {
                        source:
                            crate::view::render_pass::draw_rect_pass::RenderTargetIn::with_handle(
                                dep_handle,
                            ),
                        ..Default::default()
                    },
                    crate::view::render_pass::present_surface_pass::PresentSurfaceOutput::default(),
                );
            let present_handle = graph.add_graphics_pass(present_pass);
            graph
                .add_pass_sink(
                    present_handle,
                    crate::view::frame_graph::ExternalSinkKind::SurfacePresent,
                )
                .expect("surface present sink should register");
        }
        timings.build_graph_ms = phase_clock.checkpoint_ms();

        // --- Compile ---
        // Take the cache out (moves ownership) so we can pass self mutably to compile.
        // On cache hit the graph is reused in-place; on miss it is dropped. Either way
        // the returned compiled_graph is stored back for the next frame.
        let prior_cache = self
            .frame
            .compile_cache
            .take()
            .map(|c| (c.topology_key, c.graph));
        let mut compiled_topology_key = None;
        let compiled = match graph.compile_with_upload_cached(self, prior_cache) {
            Ok((profile, topology_key)) => {
                timings.compile_children =
                    build_compile_trace_nodes(&profile, self.debug_options.trace_compile_detail);
                compiled_topology_key = Some(topology_key);
                true
            }
            Err(err) => {
                eprintln!("[warn] frame graph compile failed: {:?}", err);
                // compile_cache already cleared by take() above
                false
            }
        };

        // Include cache transfer, diagnostic construction and errors, even
        // when compilation fails without returning an internal profile.
        timings.compile_ms = phase_clock.checkpoint_ms();

        // --- Execute ---
        let mut executed = false;
        if compiled {
            match graph.execute_profiled(self, self.debug_options.trace_render_time) {
                Ok(profile) => {
                    timings.execute_profile_ms = Some(profile.total_ms);
                    timings.execute_pass_count = profile.pass_count;
                    timings.execute_ordered_passes = profile.ordered_passes;
                    timings.execute_detail_ordered_passes = profile.detail_ordered;
                    executed = true;
                }
                Err(error) => eprintln!("[warn] frame graph execution failed: {error:?}"),
            }
        }
        // Failed execution still consumes this phase; a missing profile
        // must not turn the time spent into a zero-duration attempt.
        timings.execute_ms = phase_clock.checkpoint_ms();
        let root_keys = self.scene.ui_root_keys.clone();
        finish_frame_dirty_lifecycle(&mut self.scene.node_arena, &root_keys, compiled, executed);
        self.finish_retained_surface_transaction_for_frame(
            retained_surface_frame_owner,
            compiled && executed,
        );
        let terminal_failure = terminal_failure_stage(compiled, executed);
        if let Some(stage) = terminal_failure {
            if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                telemetry.note_terminal_failure(retained_auto_terminal_fallback_stage(stage));
            }
            self.arm_retained_auto_terminal_failure(stage);
        }
        if !compiled || !executed {
            paint_authority_trace.push_str(&format!(
                " retained-state=invalidated compiled={compiled} executed={executed}"
            ));
        }
        #[cfg(test)]
        {
            if let (Some(telemetry), Some(before)) = (
                paint_authority_telemetry.as_mut(),
                retained_release_count_before,
            ) {
                telemetry.note_resident_release_delta(
                    before,
                    self.retained_surface_release_log_for_test().len(),
                );
            }
        }
        if let Some(telemetry) = paint_authority_telemetry.as_ref() {
            self.frame.last_retained_auto_debug = Some(self.build_retained_auto_debug_capture(
                telemetry,
                &root_keys_for_build,
                compiled,
                executed,
            ));
        }
        if let Some(telemetry) = paint_authority_telemetry.as_mut() {
            telemetry.set_detail(paint_authority_trace);
            #[cfg(any(test, feature = "renderer-test-support"))]
            store_paint_authority_test_snapshot(telemetry);
        }

        // Never retain topology from a terminal frame. In particular, an
        // execute failure has a compiled graph but it is not a successful
        // cross-frame cache seed; a manual circuit reset must retry cleanly.
        self.frame.compile_cache = None;
        if should_store_compile_cache(compiled, executed)
            && let Some(topology_key) = compiled_topology_key
        {
            if let Some(compiled_graph) = graph.take_compiled_graph() {
                self.frame.compile_cache = Some(CachedCompiledGraph {
                    topology_key,
                    graph: compiled_graph,
                });
            }
        }

        timings.finish_render_ms = phase_clock.checkpoint_ms();

        // --- Complete frame ---
        // Transaction rollback and the retained-auto circuit breaker above
        // must settle before the acquired frame is either submitted or
        // discarded. A terminal compile/execute failure never submits a
        // partially recorded encoder and never presents its surface image.
        let end_frame_profile = self.complete_frame(frame_disposition(compiled, executed));
        timings.end_frame_ms = phase_clock.checkpoint_ms();
        timings.end_frame_submit_ms = end_frame_profile.submit_ms;
        timings.end_frame_present_ms = end_frame_profile.present_ms;
        timings.total_ms = phase_clock.total_ms();

        #[cfg(test)]
        frame_timing_tests::assert_frame_accounting(&timings);

        // --- Trace output ---
        if self.debug_options.trace_render_time {
            if let Some(telemetry) = paint_authority_telemetry.as_ref() {
                println!("paint-authority {}", telemetry.format_debug());
            }
            if !self.frame.gpu_paint_sources.is_empty() {
                println!(
                    "gpu-paint frame={} sources={:?}",
                    frame_number,
                    self.gpu_paint_observations()
                );
                self.print_retained_raster_diagnostics(frame_number);
            }
            let trace_root = Self::build_frame_trace_tree(&timings, &self.debug_options);
            println!("{}", format_trace_render_tree(&trace_root));
        }
        crate::view::base_component::set_text_measure_profile_enabled(false);
        crate::view::base_component::set_layout_place_profile_enabled(false);
        self.frame.frame_stats.record_frame(profile_start.elapsed());
        // Only persist the graph when compile succeeded; a failed compile
        // leaves the graph in an inconsistent state.
        self.frame.last_frame_graph = if compiled { Some(graph) } else { None };
        post_layout_transition.redraw_changed || post_layout_animation_changed
    }

    pub fn render_rsx(&mut self, root: &RsxNode) -> Result<(), String> {
        // The sole semantic engine-time sample for this viewport frame. Every
        // retained animation tick and paint-resource freeze observes this
        // exact value; profiling clocks below remain observational only.
        self.render_rsx_at(root, crate::time::Instant::now())
    }

    fn render_rsx_at(
        &mut self,
        root: &RsxNode,
        semantic_now: crate::time::Instant,
    ) -> Result<(), String> {
        let state_dirty = take_state_dirty();
        // Apply any viewport mutations that component event handlers
        // enqueued via `use_viewport()` during the previous tick. Must
        // run before dirty evaluation so toggles like trace_render_time
        // take effect on the upcoming frame.
        self.apply_pending_viewport_actions();
        // Reset the animation flag — transition plugins below will set
        // it back to true if any of them still want more frames.
        self.is_animating = false;
        let resource_dirty = crate::view::image_resource::take_image_redraw_dirty()
            || crate::view::svg_resource::take_svg_redraw_dirty();
        let root_changed = self.scene.last_rsx_root.as_ref() != Some(root);
        let mut needs_rebuild = state_dirty.needs_rebuild() || root_changed;
        if root_changed && self.try_apply_placement_updates(root)? {
            needs_rebuild = false;
        }
        // Incremental Fiber-commit path.
        //
        // Only engaged when ALL of:
        //   - incremental commit is enabled (default true),
        //   - a previous `last_rsx_root` exists (not a cold start),
        //   - the full-rebuild path below would otherwise run,
        //   - we have at least one arena root,
        //   - every reconcile patch can be translated into FiberWork
        //     and applied safely.
        //
        // Any failure leaves `needs_rebuild` untouched and falls
        // through to the full-rebuild path.
        if needs_rebuild
            && self.scene.use_incremental_commit
            && self.scene.last_rsx_root.is_some()
            && !self.scene.ui_root_keys.is_empty()
        {
            let previous_root = self.scene.last_rsx_root.as_ref().unwrap();
            // 軌 1 #4 Fragment-at-root: unpack Fragment root into its
            // children so `reconcile_multi` sees the same arity that
            // the arena stores (Fragment root → N arena roots).
            let old_roots = unpack_root_set(previous_root);
            let new_roots = unpack_root_set(root);
            let rooted_patches = crate::ui::reconcile_multi(Some(&old_roots), &new_roots);
            let descriptor_ctx = crate::view::fiber_work::DescriptorContext {
                new_rsx_root: root,
                // 軌 1 #6: pass the previous tree so the translator
                // can identity-validate parent_path walks for
                // InsertChild patches.
                old_rsx_root: Some(previous_root),
                inherited_style: &self.style,
                viewport_width: self.logical_width,
                viewport_height: self.logical_height,
            };
            let translated = crate::view::fiber_work::translate_rooted_patches_all_or_nothing(
                rooted_patches,
                self.scene.node_arena.stable_id_index(),
                &self.scene.node_arena,
                &self.scene.ui_root_keys,
                &old_roots,
                &new_roots,
                Some(&descriptor_ctx),
            );
            if let Some(works) = translated {
                let all_committable = works
                    .iter()
                    .all(|w| w.is_committable(&self.scene.node_arena));
                if all_committable {
                    // Cross-parent keyed moves can translate as delete+create;
                    // preserve host scroll state by stable id across the batch.
                    let mut incremental_scroll_offsets = FxHashMap::default();
                    Self::save_scroll_states(
                        &self.scene.node_arena,
                        &self.scene.ui_root_keys,
                        &mut incremental_scroll_offsets,
                    );
                    let apply_ctx = crate::view::fiber_work::ApplyContext {
                        viewport_style: &self.style,
                        viewport_width: self.logical_width,
                        viewport_height: self.logical_height,
                    };
                    let incremental_result = crate::view::fiber_work::apply_fiber_works(
                        &mut self.scene.node_arena,
                        apply_ctx,
                        works,
                    );
                    match incremental_result {
                        Ok(()) => {
                            // Keep the arena roots view in lockstep: ReplaceRoot
                            // mints a new root NodeKey, so always refresh from
                            // the arena after a committed batch.
                            let refreshed_roots = self.scene.node_arena.roots().to_vec();
                            self.scene.ui_root_keys = refreshed_roots;
                            Self::restore_scroll_states(
                                &self.scene.node_arena,
                                &self.scene.ui_root_keys,
                                &incremental_scroll_offsets,
                            );
                            self.scene.last_rsx_root = Some(root.clone());
                            needs_rebuild = false;
                        }
                        Err(error) => {
                            // Earlier work in this non-transactional batch may
                            // already have rewritten the arena root set. The
                            // cold path below must remove the current roots as
                            // well as any still-live roots from the stale
                            // viewport mirror, or newly-created subtrees and
                            // their stable-id/sync registrations would leak.
                            self.scene
                                .refresh_roots_for_cold_rebuild_after_incremental_failure();
                            eprintln!(
                                "[render_rsx] incremental apply failed; cold rebuild: {error:?}"
                            );
                        }
                    }
                }
            }
        }
        if needs_rebuild {
            // Clear and save current scroll states
            self.scene.scroll_offsets.clear();
            Self::save_scroll_states(
                &self.scene.node_arena,
                &self.scene.ui_root_keys,
                &mut self.scene.scroll_offsets,
            );
            let layout_snapshots =
                crate::view::viewport::transitions_tick::collect_layout_transition_snapshots(
                    &self.scene.node_arena,
                    &self.scene.ui_root_keys,
                );
            let (converted_descriptors, conversion_errors) =
                crate::view::renderer_adapter::rsx_to_descriptors_with_context(
                    root,
                    &self.style,
                    self.logical_width,
                    self.logical_height,
                );
            if !conversion_errors.is_empty() {
                eprintln!(
                    "[render_rsx] skipped {} invalid node(s):\n{}",
                    conversion_errors.len(),
                    conversion_errors.join("\n")
                );
            }
            if converted_descriptors.is_empty() {
                eprintln!("[render_rsx] no valid root nodes converted; keep previous render tree");
                self.scene.last_rsx_root = Some(root.clone());
                return Ok(());
            }
            // Approach-C: drop the previous arena subtree and commit the
            // freshly-built descriptor trees as new arena roots. `ui_roots`
            // (the legacy boxed mirror) stays empty — arena is the source
            // of truth; the still-legacy render/layout boxed traversal below
            // ignores it and walks the arena via root keys instead.
            for old_key in std::mem::take(&mut self.scene.ui_root_keys) {
                self.scene.node_arena.remove_subtree(old_key);
            }
            let mut new_root_keys = Vec::with_capacity(converted_descriptors.len());
            for desc in converted_descriptors {
                let key = crate::view::renderer_adapter::commit_descriptor_tree(
                    &mut self.scene.node_arena,
                    None,
                    desc,
                );
                new_root_keys.push(key);
            }
            self.scene.ui_root_keys = new_root_keys.clone();
            self.scene.node_arena.set_roots(new_root_keys);
            self.scene.last_rsx_root = Some(root.clone());

            // Restore scroll states into new elements
            Self::restore_scroll_states(
                &self.scene.node_arena,
                &self.scene.ui_root_keys,
                &self.scene.scroll_offsets,
            );
            {
                let mut arena = std::mem::take(&mut self.scene.node_arena);
                let root_keys = self.scene.ui_root_keys.clone();
                crate::view::viewport::transitions_tick::seed_layout_transition_snapshots(
                    &mut arena,
                    &root_keys,
                    &layout_snapshots,
                );
                self.scene.node_arena = arena;
            }
            // Drop tracks for channels the rebuilt tree no longer declares
            // before applying in-flight samples — otherwise a removed
            // transition would re-stamp the stale interpolated value over
            // the freshly synced target.
            let _ = self.cancel_disallowed_transition_tracks();
            let has_inflight_transition = self.sync_inflight_transition_state();
            if has_inflight_transition {
                self.request_redraw();
            }
        }
        self.sync_focus_dispatch();
        let animation_changed = {
            let mut arena = std::mem::take(&mut self.scene.node_arena);
            let root_keys = self.scene.ui_root_keys.clone();
            let changed = crate::view::base_component::tick_animation_frames(
                &mut arena,
                &root_keys,
                semantic_now,
            );
            self.scene.node_arena = arena;
            changed
        };
        let canceled_tracks = self.cancel_disallowed_transition_tracks();
        // Reconciling runtime transition state is a whole-tree walk; when
        // no claims are active now AND none were active last frame there
        // is no per-node state left to clear, so the walk is a no-op.
        let claims_empty = self.transitions.transition_claims.is_empty();
        let reconcile_skippable = claims_empty && self.transitions.claims_were_empty;
        self.transitions.claims_were_empty = claims_empty;
        let reconciled_transition_state = if reconcile_skippable {
            false
        } else {
            let mut arena = std::mem::take(&mut self.scene.node_arena);
            let root_keys = self.scene.ui_root_keys.clone();
            let result =
                crate::view::viewport::transitions_tick::reconcile_transition_runtime_state(
                    &mut arena,
                    &root_keys,
                    &active_channels_by_node(&self.transitions.transition_claims),
                );
            self.scene.node_arena = arena;
            result
        };
        let (dt, now_seconds) = self.transition_timing(semantic_now);
        let transition_changed_before_render = canceled_tracks
            || reconciled_transition_state
            || self.run_pre_layout_transitions(dt, now_seconds);
        let mut transition_changed_after_layout = false;
        if !self.scene.ui_root_keys.is_empty() {
            transition_changed_after_layout =
                self.render_render_tree(dt, now_seconds, semantic_now);
        }
        let next_hover_target = self.pointer_position_viewport().and_then(|(x, y)| {
            Self::hit_test_pointer_target(
                &self.scene.node_arena,
                &self.scene.popup_stack,
                &self.scene.ui_root_keys,
                x,
                y,
            )
            .map(|(_, t)| t)
        });
        // Re-applying hover flags is a whole-tree walk; skip it when the
        // hover target is unchanged and the arena was not rebuilt this
        // frame (a rebuild drops the per-node hover flags).
        let hover_changed =
            if next_hover_target == self.input_state.hovered_node_id && !needs_rebuild {
                false
            } else {
                let mut arena = std::mem::take(&mut self.scene.node_arena);
                let root_keys = self.scene.ui_root_keys.clone();
                let result = Self::sync_hover_visual_only(
                    &mut arena,
                    &root_keys,
                    &mut self.input_state.hovered_node_id,
                    next_hover_target,
                );
                self.scene.node_arena = arena;
                result
            };
        if resource_dirty
            || hover_changed
            || animation_changed
            || transition_changed_before_render
            || transition_changed_after_layout
        {
            self.request_redraw();
        }
        if self.scene.ui_root_keys.iter().any(|&root_key| {
            crate::view::base_component::has_animation_frame_request(
                &self.scene.node_arena,
                root_key,
            )
        }) {
            self.request_redraw();
        }
        if std::mem::take(&mut self.frame.frame_presented) {
            self.notify_cursor_handler();
        }
        Ok(())
    }

    /// Build RSX (if dirty) and render a frame in one call.
    ///
    /// Requires a live `App` set via `set_app`. Checks global dirty
    /// state, calls `App::build` when a rebuild is needed, then
    /// delegates to `render_rsx` for the GPU work.
    pub fn render_frame(
        &mut self,
        services: crate::platform::PlatformServices<'_>,
    ) -> super::RenderFrameResult {
        if self.app.is_none() {
            return super::RenderFrameResult::Ok;
        }

        if peek_state_dirty().needs_rebuild() {
            self.needs_rebuild = true;
        }

        if self.needs_rebuild || self.cached_rsx.is_none() {
            let build_start = Instant::now();
            let rsx = self.with_app(services, |app, ctx| app.build(ctx));
            self.frame.rsx_build_ms = build_start.elapsed().as_secs_f64() * 1000.0;
            self.cached_rsx = Some(rsx);
            self.needs_rebuild = false;
        } else {
            self.frame.rsx_build_ms = 0.0;
        }

        if let Some(rsx) = self.cached_rsx.clone() {
            let _ = self.render_rsx(&rsx);
        }

        if self.cached_rsx.is_some() && self.frame_box_models().is_empty() {
            super::RenderFrameResult::NeedsRetry
        } else {
            super::RenderFrameResult::Ok
        }
    }

    /// Forward an `AppEvent` to the held `App::on_event`.
    pub fn dispatch_app_event(
        &mut self,
        event: &crate::app::AppEvent,
        services: crate::platform::PlatformServices<'_>,
    ) {
        self.with_app(services, |app, ctx| app.on_event(event, ctx));
    }

    /// Call `App::on_ready` exactly once (subsequent calls are no-ops).
    pub fn app_on_ready(&mut self, services: crate::platform::PlatformServices<'_>) {
        if self.ready_dispatched {
            return;
        }
        self.ready_dispatched = true;
        self.with_app(services, |app, ctx| app.on_ready(ctx));
    }

    /// Call `App::on_shutdown`.
    pub fn app_on_shutdown(&mut self, services: crate::platform::PlatformServices<'_>) {
        if self.app.is_none() {
            return;
        }
        self.with_app(services, |app, ctx| app.on_shutdown(ctx));
    }

    /// Temporarily extract the App, build an AppContext, call the
    /// closure, then put the App back. This sidesteps the borrow-checker
    /// conflict between `&mut self` (for `ViewportControl`) and
    /// `&mut self.app`.
    ///
    /// The reborrowing of `services` fields breaks the invariant lifetime
    /// binding that `&'a mut` references carry, allowing the compiler to
    /// pick a shorter, block-scoped lifetime for the `AppContext`.
    fn with_app<R>(
        &mut self,
        services: crate::platform::PlatformServices<'_>,
        f: impl FnOnce(&mut dyn crate::app::App, &mut crate::app::AppContext<'_>) -> R,
    ) -> R {
        let mut app = self.app.take().expect("no app set");
        let result = {
            let mut ctx = crate::app::AppContext {
                viewport: super::ViewportControl::new(self),
                services: crate::platform::PlatformServices {
                    clipboard: &mut *services.clipboard,
                    cursor: &mut *services.cursor,
                    redraw: services.redraw,
                },
            };
            f(&mut *app, &mut ctx)
        };
        self.app = Some(app);
        result
    }

    /// Drain the thread-local queue populated by `ui::use_viewport()` and
    /// apply each action to this viewport. Called at the top of
    /// `render_rsx` so event handlers from the prior frame land
    /// before dirty flags are read.
    fn apply_pending_viewport_actions(&mut self) {
        let actions = crate::ui::drain_viewport_actions();
        if actions.is_empty() {
            return;
        }
        for action in actions {
            match action {
                crate::ui::ViewportAction::SetDebugTraceFps(on) => {
                    self.debug_options.trace_fps = on;
                    self.frame.frame_stats.set_enabled(on);
                }
                crate::ui::ViewportAction::SetDebugTraceRenderTime(on) => {
                    self.debug_options.trace_render_time = on;
                }
                crate::ui::ViewportAction::SetDebugTraceLayoutDetail(on) => {
                    self.debug_options.trace_layout_detail = on;
                }
                crate::ui::ViewportAction::SetDebugTraceCompileDetail(on) => {
                    self.debug_options.trace_compile_detail = on;
                }
                crate::ui::ViewportAction::SetDebugTraceExecuteDetail(on) => {
                    self.debug_options.trace_execute_detail = on;
                }
                crate::ui::ViewportAction::SetDebugGeometryOverlay(on) => {
                    self.debug_options.geometry_overlay = on;
                }
                crate::ui::ViewportAction::SetDebugRetainedAutoOverlay(on) => {
                    self.debug_options.retained_auto_overlay = on;
                }
                crate::ui::ViewportAction::SetDebugRetainedAutoAuthority(on) => {
                    self.debug_options.retained_auto_authority = on;
                }
                crate::ui::ViewportAction::SetDebugRetainedAutoReuseActions(on) => {
                    self.debug_options.retained_auto_reuse_actions = on;
                }
                crate::ui::ViewportAction::SetDebugRetainedAutoFallbackReasons(on) => {
                    self.debug_options.retained_auto_fallback_reasons = on;
                }
                crate::ui::ViewportAction::SetClearColor(color) => {
                    self.set_clear_color(Box::new(color));
                }
                crate::ui::ViewportAction::SetCursor(cursor) => {
                    self.set_cursor(cursor);
                }
                crate::ui::ViewportAction::RequestRedraw => self.request_redraw(),
            }
        }
    }

    fn begin_frame(&mut self) -> Option<BeginFrameProfile> {
        // If a frame is already in progress (e.g. recursive render call),
        // return a zero-cost profile so the caller proceeds with the
        // existing encoder rather than skipping the frame entirely.
        if self.frame.frame_state.is_some() {
            return Some(BeginFrameProfile {
                acquire_ms: 0.0,
                create_view_ms: 0.0,
                create_encoder_ms: 0.0,
            });
        }
        if !self.apply_pending_reconfigure() {
            return None;
        }
        self.frame.offscreen_render_target_pool.begin_frame();
        self.reclaim_idle_frame_gpu_pools();
        self.frame.draw_rect_uniform_cursor = 0;
        self.frame.draw_rect_uniform_offset = 0;
        self.frame.gradient_stops_byte_cursor = 0;
        crate::view::render_pass::draw_rect_pass::begin_draw_rect_resources_frame();
        crate::view::render_pass::shadow_module::begin_shadow_resources_frame();
        crate::view::render_pass::text_pass::begin_text_resources_frame();

        let surface = match &self.gpu.surface {
            Some(s) => s,
            None => return None,
        };
        let device = match &self.gpu.device {
            Some(d) => d,
            None => return None,
        };

        let acquire_started_at = Instant::now();
        let render_texture = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                surface.configure(device, &self.gpu.surface_config);
                texture
            }
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                println!("[warn] surface lost, recreate render texture");
                surface.configure(device, &self.gpu.surface_config);
                match surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(texture)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
                    _ => return None,
                }
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return None,
        };
        let acquire_ms = acquire_started_at.elapsed().as_secs_f64() * 1000.0;

        let create_view_started_at = Instant::now();
        let surface_view = render_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor {
                format: Some(self.gpu.surface_target_format),
                ..Default::default()
            });
        let (view, resolve_view) = (surface_view, None);
        let create_view_ms = create_view_started_at.elapsed().as_secs_f64() * 1000.0;

        let create_encoder_started_at = Instant::now();
        let encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let create_encoder_ms = create_encoder_started_at.elapsed().as_secs_f64() * 1000.0;

        self.frame.frame_state = Some(FrameState {
            #[cfg(not(any(test, feature = "renderer-test-support")))]
            render_texture,
            #[cfg(any(test, feature = "renderer-test-support"))]
            render_texture: Some(render_texture),
            #[cfg(any(test, feature = "renderer-test-support"))]
            offscreen_texture: None,
            view,
            resolve_view,
            encoder,
            depth_view: self.gpu.depth_view.clone(),
        });
        Some(BeginFrameProfile {
            acquire_ms,
            create_view_ms,
            create_encoder_ms,
        })
    }

    #[cfg(any(test, feature = "renderer-test-support"))]
    pub(crate) fn begin_offscreen_test_frame(
        &mut self,
        device: wgpu::Device,
        queue: wgpu::Queue,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Result<(), String> {
        if self.frame.frame_state.is_some() {
            return Err("an offscreen test frame is already active".to_string());
        }
        let width = width.max(1);
        let height = height.max(1);
        self.gpu.device = Some(device.clone());
        self.gpu.queue = Some(queue);
        self.gpu.surface = None;
        self.gpu.surface_config.width = width;
        self.gpu.surface_config.height = height;
        self.gpu.surface_config.format = format;
        self.gpu.surface_config.usage =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC;
        self.gpu.surface_config.view_formats.clear();
        self.gpu.surface_target_format = format;
        self.gpu.msaa_sample_count = 1;
        self.gpu.depth_texture = None;
        self.gpu.depth_view = None;
        self.scale_factor = 1.0;
        self.logical_width = width as f32;
        self.logical_height = height as f32;

        self.frame.offscreen_render_target_pool.begin_frame();
        self.frame.draw_rect_uniform_cursor = 0;
        self.frame.draw_rect_uniform_offset = 0;
        self.frame.gradient_stops_byte_cursor = 0;
        crate::view::render_pass::draw_rect_pass::begin_draw_rect_resources_frame();
        crate::view::render_pass::shadow_module::begin_shadow_resources_frame();
        crate::view::render_pass::text_pass::begin_text_resources_frame();

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rfgui native pixel parity output"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.frame.frame_state = Some(FrameState {
            render_texture: None,
            offscreen_texture: Some(texture),
            view,
            resolve_view: None,
            encoder,
            depth_view: None,
        });
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn encode_offscreen_test_readback(
        &mut self,
        buffer: &wgpu::Buffer,
        padded_bytes_per_row: u32,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let frame = self
            .frame
            .frame_state
            .as_mut()
            .ok_or_else(|| "no active offscreen test frame".to_string())?;
        let texture = frame
            .offscreen_texture
            .as_ref()
            .ok_or_else(|| "active test frame has no offscreen texture".to_string())?;
        frame.encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn end_offscreen_test_frame(&mut self) -> Result<(), String> {
        if self.frame.frame_state.is_none() {
            return Err("no active offscreen test frame".to_string());
        }
        let _ = self.submit_and_present_frame();
        Ok(())
    }

    fn complete_frame(&mut self, disposition: FrameDisposition) -> EndFrameProfile {
        match disposition {
            FrameDisposition::SubmitAndPresent => self.submit_and_present_frame(),
            FrameDisposition::Abort => self.abort_frame(),
        }
    }

    fn abort_frame(&mut self) -> EndFrameProfile {
        self.frame.frame_presented = false;
        let Some(frame) = self.frame.frame_state.take() else {
            return EndFrameProfile::default();
        };

        frame.discard_unsubmitted();
        self.finish_gpu_paint_frame(false);
        self.frame.offscreen_render_target_pool.finish_frame();

        #[cfg(not(target_arch = "wasm32"))]
        {
            // StagingBelt has no abort/reset operation. `recall()` is only
            // valid after every encoder containing its copies was submitted,
            // so abandon the belt and lazily recreate it on the next upload.
            self.gpu.upload_staging_belt = None;
        }
        #[cfg(target_arch = "wasm32")]
        crate::view::render_pass::destroy_frame_transient_buffers();

        #[cfg(any(test, feature = "renderer-test-support"))]
        {
            self.frame.completion_counts.aborts =
                self.frame.completion_counts.aborts.saturating_add(1);
        }

        EndFrameProfile::default()
    }

    fn submit_and_present_frame(&mut self) -> EndFrameProfile {
        let frame = match self.frame.frame_state.take() {
            Some(frame) => frame,
            None => return EndFrameProfile::default(),
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(staging_belt) = self.gpu.upload_staging_belt.as_mut() {
            staging_belt.finish();
        }

        let submit_started_at = Instant::now();
        let queue = self.gpu.queue.as_ref().unwrap();
        let _submission_index = queue.submit(Some(frame.encoder.finish()));
        #[cfg(any(test, feature = "renderer-test-support"))]
        {
            self.frame.completion_counts.submits =
                self.frame.completion_counts.submits.saturating_add(1);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(staging_belt) = self.gpu.upload_staging_belt.as_mut() {
            staging_belt.recall();
        }
        #[cfg(target_arch = "wasm32")]
        crate::view::render_pass::destroy_frame_transient_buffers();
        self.frame.offscreen_render_target_pool.finish_frame();
        let submit_ms = submit_started_at.elapsed().as_secs_f64() * 1000.0;

        let present_started_at = Instant::now();
        #[cfg(not(any(test, feature = "renderer-test-support")))]
        queue.present(frame.render_texture);
        #[cfg(any(test, feature = "renderer-test-support"))]
        if let Some(render_texture) = frame.render_texture {
            queue.present(render_texture);
            self.frame.completion_counts.presents =
                self.frame.completion_counts.presents.saturating_add(1);
        }
        let present_ms = present_started_at.elapsed().as_secs_f64() * 1000.0;
        self.finish_gpu_paint_frame(true);
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Surface latency limits acquired swapchain images, but it does not
            // bound every resource referenced by submitted command buffers.
            // Keep a small native submission pipeline and wait only when the
            // oldest frame falls outside it, so per-frame buffers and bind
            // groups can be retired instead of accumulating indefinitely.
            const MAX_IN_FLIGHT_SUBMISSIONS: usize = 2;
            self.gpu.in_flight_submissions.push_back(_submission_index);
            if self.gpu.in_flight_submissions.len() > MAX_IN_FLIGHT_SUBMISSIONS {
                let oldest = self
                    .gpu
                    .in_flight_submissions
                    .pop_front()
                    .expect("submission queue exceeded its non-zero limit");
                if let Some(device) = self.gpu.device.as_ref() {
                    let _ = device.poll(wgpu::PollType::Wait {
                        submission_index: Some(oldest),
                        timeout: None,
                    });
                }
            }
        }
        self.frame.frame_presented = true;
        EndFrameProfile {
            submit_ms,
            present_ms,
        }
    }

    #[cfg(any(test, feature = "renderer-test-support"))]
    fn frame_completion_counts_for_test(&self) -> (u64, u64, u64) {
        let counts = self.frame.completion_counts;
        (counts.submits, counts.presents, counts.aborts)
    }
}

#[cfg(test)]
mod frame_timing_tests;
#[cfg(test)]
mod legacy_root_render_tests;
#[cfg(test)]
mod selection_rejection_debug_tests;

/// Flatten a Fragment-at-root into its children so multi-root reconcile
/// sees the same arity as the arena (Fragment root → N arena roots).
/// Non-Fragment roots pass through as a single-element slice.
fn unpack_root_set(root: &crate::ui::RsxNode) -> Vec<&crate::ui::RsxNode> {
    match root {
        crate::ui::RsxNode::Fragment(frag) => frag.children.iter().collect(),
        other => vec![other],
    }
}

#[cfg(feature = "renderer-test-support")]
pub(super) mod downstream_test_support;
