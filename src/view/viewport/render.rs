use super::*;

fn build_non_promoted_root_legacy(
    graph: &mut FrameGraph,
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    ctx: crate::view::base_component::UiBuildContext,
) -> crate::view::base_component::BuildState {
    arena
        .with_element_taken(root_key, |root, arena| root.build(graph, arena, ctx))
        .expect("non-promoted root should exist during the build walk")
}

enum PropertyNeutralArtifactAttempt {
    Compiled {
        state: crate::view::base_component::BuildState,
        eligibility: crate::view::paint::FrameArtifactEligibility,
        root_effect_transaction: Option<PendingRootEffectTransaction>,
    },
    WholeFrameLegacy {
        eligibility: crate::view::paint::FrameArtifactEligibility,
    },
    CompileRejected(crate::view::paint::ArtifactCompileErrorKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutoAuthorityKind {
    PropertyScene,
    Artifact,
    Legacy,
}

#[derive(Clone, Debug)]
enum AutoAuthorityRejection {
    Plan {
        authority: AutoAuthorityKind,
        error: crate::view::paint::FramePaintPlanError,
    },
    PropertyScrollPlan {
        error: crate::view::paint::PropertyScrollScenePlanError,
    },
    NestedScrollPlan {
        error: crate::view::paint::PropertyScrollScenePlanError,
    },
    DirectScrollTransformPlan {
        error: crate::view::paint::PropertyScrollScenePlanError,
    },
    TransformScrollPlan {
        error: crate::view::paint::PropertyScrollScenePlanError,
    },
    EffectScrollPlan {
        error: crate::view::paint::PropertyScrollScenePlanError,
    },
    TransformEffectScrollPlan {
        error: crate::view::paint::PropertyScrollScenePlanError,
    },
    Artifact {
        eligibility: crate::view::paint::FrameArtifactEligibility,
    },
}

#[derive(Clone, Debug, Default)]
struct AutoAuthorityTrace {
    capture_rejections: bool,
    rejections: Vec<AutoAuthorityRejection>,
}

impl AutoAuthorityTrace {
    fn new(capture_rejections: bool) -> Self {
        Self {
            capture_rejections,
            rejections: Vec::new(),
        }
    }

    fn capture(&mut self, rejection: impl FnOnce() -> AutoAuthorityRejection) {
        if self.capture_rejections {
            self.rejections.push(rejection());
        }
    }
}

impl AutoAuthorityRejection {
    fn debug_label(&self) -> String {
        match self {
            Self::Plan { authority, error } => {
                format!("plan({}):{:?}", authority.label(), error.reasons)
            }
            Self::PropertyScrollPlan { error } => {
                format!("plan(property-scene-scroll):{error:?}")
            }
            Self::NestedScrollPlan { error } => {
                format!("plan(property-scene-nested-scroll):{error:?}")
            }
            Self::DirectScrollTransformPlan { error } => {
                format!("plan(property-scene-scroll-transform):{error:?}")
            }
            Self::TransformScrollPlan { error } => {
                format!("plan(property-scene-transform-scroll):{error:?}")
            }
            Self::EffectScrollPlan { error } => {
                format!("plan(property-scene-effect-scroll):{error:?}")
            }
            Self::TransformEffectScrollPlan { error } => {
                format!("plan(property-scene-transform-effect-scroll):{error:?}")
            }
            Self::Artifact { eligibility } => {
                format!("artifact:{:?}", eligibility.reasons)
            }
        }
    }
}

impl AutoAuthorityKind {
    fn label(self) -> &'static str {
        match self {
            Self::PropertyScene => "property-scene",
            Self::Artifact => "artifact",
            Self::Legacy => "legacy",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaintAuthorityKind {
    Legacy,
    Artifact,
    Transform,
    SurfaceTree,
    Isolation,
    EffectTree,
    PropertyScene,
    ScrollHost,
    ScrollScene,
}

impl PaintAuthorityKind {
    fn from_auto(authority: AutoAuthorityKind) -> Self {
        match authority {
            AutoAuthorityKind::PropertyScene => Self::PropertyScene,
            AutoAuthorityKind::Artifact => Self::Artifact,
            AutoAuthorityKind::Legacy => Self::Legacy,
        }
    }

    fn from_named_mode(mode: ViewportPaintRendererMode) -> Self {
        match mode {
            ViewportPaintRendererMode::Legacy => Self::Legacy,
            ViewportPaintRendererMode::ArtifactCanary => Self::Artifact,
            ViewportPaintRendererMode::RetainedTransformCanary => Self::Transform,
            ViewportPaintRendererMode::RetainedSurfaceTreeCanary => Self::SurfaceTree,
            ViewportPaintRendererMode::RetainedIsolationCanary => Self::Isolation,
            ViewportPaintRendererMode::RetainedEffectTreeCanary => Self::EffectTree,
            ViewportPaintRendererMode::RetainedScrollHostCanary => Self::ScrollHost,
            ViewportPaintRendererMode::RetainedScrollSceneCanary => Self::ScrollScene,
            ViewportPaintRendererMode::RetainedAuto => {
                unreachable!("automatic mode supplies its selected authority")
            }
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Artifact => "artifact",
            Self::Transform => "transform",
            Self::SurfaceTree => "surface-tree",
            Self::Isolation => "isolation",
            Self::EffectTree => "effect-tree",
            Self::PropertyScene => "property-scene",
            Self::ScrollHost => "scroll-host",
            Self::ScrollScene => "scroll-scene",
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
    NoTransform,
    Shape {
        authority: PaintAuthorityKind,
        transforms: usize,
        effects: usize,
        scrolls: usize,
    },
    Plan {
        authority: PaintAuthorityKind,
        error: crate::view::paint::FramePaintPlanError,
    },
    Artifact(crate::view::paint::FrameArtifactEligibility),
}

impl PaintAuthoritySelectionRejection {
    fn debug_label(&self) -> String {
        match self {
            Self::Auto(rejection) => rejection.debug_label(),
            Self::NoTransform => "no-transform".to_owned(),
            Self::Shape {
                authority,
                transforms,
                effects,
                scrolls,
            } => format!(
                "shape({}):transforms={transforms},effects={effects},scrolls={scrolls}",
                authority.label()
            ),
            Self::Plan { authority, error } => {
                format!("plan({}):{:?}", authority.label(), error.reasons)
            }
            Self::Artifact(eligibility) => format!("artifact:{:?}", eligibility.reasons),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScrollContentAuthorityTelemetry {
    backing: crate::view::paint::ScrollSceneBackingKind,
    tile_count: usize,
    reraster_count: usize,
    reuse_count: usize,
    pair_bytes: u64,
}

#[derive(Clone, Debug)]
struct PaintAuthorityTelemetry {
    requested_mode: ViewportPaintRendererMode,
    selected: PaintAuthorityKind,
    selection_rejections: Vec<PaintAuthoritySelectionRejection>,
    legacy_fallback_stage: Option<PaintAuthorityFallbackStage>,
    terminal_failure_stage: Option<PaintAuthorityFallbackStage>,
    scroll_content: Option<ScrollContentAuthorityTelemetry>,
    resident_release_count: Option<usize>,
    detail: String,
}

impl PaintAuthorityTelemetry {
    fn from_selection(
        requested_mode: ViewportPaintRendererMode,
        selection: &RetainedTransformCanarySelection,
        auto: Option<(AutoAuthorityKind, AutoAuthorityTrace)>,
    ) -> Self {
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
            let rejection = match selection {
                RetainedTransformCanarySelection::NoTransform => {
                    Some(PaintAuthoritySelectionRejection::NoTransform)
                }
                RetainedTransformCanarySelection::SingletonShapeRejected { transform_count }
                | RetainedTransformCanarySelection::TreeShapeRejected { transform_count } => {
                    Some(PaintAuthoritySelectionRejection::Shape {
                        authority: selected,
                        transforms: *transform_count,
                        effects: 0,
                        scrolls: 0,
                    })
                }
                RetainedTransformCanarySelection::EffectTreeShapeRejected {
                    transform_count,
                    effect_count,
                } => Some(PaintAuthoritySelectionRejection::Shape {
                    authority: selected,
                    transforms: *transform_count,
                    effects: *effect_count,
                    scrolls: 0,
                }),
                RetainedTransformCanarySelection::ScrollHostShapeRejected { scroll_count }
                | RetainedTransformCanarySelection::ScrollSceneShapeRejected { scroll_count } => {
                    Some(PaintAuthoritySelectionRejection::Shape {
                        authority: selected,
                        transforms: 0,
                        effects: 0,
                        scrolls: *scroll_count,
                    })
                }
                RetainedTransformCanarySelection::PlanRejected(error)
                | RetainedTransformCanarySelection::TreePlanRejected(error)
                | RetainedTransformCanarySelection::IsolationPlanRejected(error)
                | RetainedTransformCanarySelection::EffectTreePlanRejected(error)
                | RetainedTransformCanarySelection::ScrollHostPlanRejected(error) => {
                    Some(PaintAuthoritySelectionRejection::Plan {
                        authority: selected,
                        error: error.clone(),
                    })
                }
                _ => None,
            };
            (selected, rejection.into_iter().collect())
        };
        Self {
            requested_mode,
            selected,
            selection_rejections,
            legacy_fallback_stage: None,
            terminal_failure_stage: None,
            scroll_content: None,
            resident_release_count: None,
            detail: String::new(),
        }
    }

    fn note_artifact_rejection(
        &mut self,
        eligibility: crate::view::paint::FrameArtifactEligibility,
    ) {
        self.selection_rejections
            .push(PaintAuthoritySelectionRejection::Artifact(eligibility));
    }

    fn note_legacy_fallback(&mut self, stage: PaintAuthorityFallbackStage) {
        self.legacy_fallback_stage = Some(stage);
    }

    fn note_terminal_failure(&mut self, stage: PaintAuthorityFallbackStage) {
        self.terminal_failure_stage = Some(stage);
    }

    fn note_scroll_content(&mut self, trace: crate::view::paint::ScrollSceneBuildTrace) {
        self.scroll_content = Some(ScrollContentAuthorityTelemetry {
            backing: trace.backing,
            tile_count: trace.tile_count,
            reraster_count: trace.reraster_count,
            reuse_count: trace.reuse_count,
            pair_bytes: trace.content_pair_bytes,
        });
    }

    fn note_property_scroll_content(
        &mut self,
        trace: &crate::view::paint::RetainedPropertyScrollSceneBuildTrace,
    ) {
        self.scroll_content = Some(ScrollContentAuthorityTelemetry {
            backing: trace.backing,
            tile_count: trace.tile_count,
            reraster_count: trace.reraster_count,
            reuse_count: trace.reuse_count,
            pair_bytes: trace.content_pair_bytes,
        });
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

    fn authority_label(&self) -> String {
        if self.requested_mode == ViewportPaintRendererMode::RetainedAuto {
            if self.legacy_fallback_stage.is_some() {
                "retained-auto:legacy".to_owned()
            } else {
                format!("retained-auto:{}", self.selected.label())
            }
        } else {
            match self.requested_mode {
                ViewportPaintRendererMode::Legacy => "legacy".to_owned(),
                ViewportPaintRendererMode::ArtifactCanary => "artifact-canary".to_owned(),
                ViewportPaintRendererMode::RetainedTransformCanary => {
                    "retained-transform-canary".to_owned()
                }
                ViewportPaintRendererMode::RetainedSurfaceTreeCanary => {
                    "retained-surface-tree-canary".to_owned()
                }
                ViewportPaintRendererMode::RetainedIsolationCanary => {
                    "retained-isolation-canary".to_owned()
                }
                ViewportPaintRendererMode::RetainedEffectTreeCanary => {
                    "retained-effect-tree-canary".to_owned()
                }
                ViewportPaintRendererMode::RetainedScrollHostCanary => {
                    "retained-scroll-host-canary".to_owned()
                }
                ViewportPaintRendererMode::RetainedScrollSceneCanary => {
                    "retained-scroll-scene-canary".to_owned()
                }
                ViewportPaintRendererMode::RetainedAuto => unreachable!(),
            }
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
        let scroll = self.scroll_content.map_or_else(
            || "none".to_owned(),
            |scroll| {
                format!(
                    "backing={:?},tiles={},reraster={},reuse={},pair-bytes={}",
                    scroll.backing,
                    scroll.tile_count,
                    scroll.reraster_count,
                    scroll.reuse_count,
                    scroll.pair_bytes,
                )
            },
        );
        let releases = self
            .resident_release_count
            .map_or_else(|| "unavailable".to_owned(), |count| count.to_string());
        format!(
            "{} requested={:?} selected={} selection-rejections=[{}] legacy-fallback-stage={} terminal-failure-stage={} scroll-content=[{}] resident-releases={} detail=[{}]",
            self.authority_label(),
            self.requested_mode,
            self.selected.label(),
            rejections,
            legacy_fallback,
            terminal_failure,
            scroll,
            releases,
            self.detail,
        )
    }

    #[cfg(test)]
    fn snapshot(&self) -> PaintAuthorityTelemetrySnapshot {
        PaintAuthorityTelemetrySnapshot {
            authority_label: self.authority_label(),
            selected: self.selected,
            rejection_labels: self
                .selection_rejections
                .iter()
                .map(PaintAuthoritySelectionRejection::debug_label)
                .collect(),
            legacy_fallback_stage: self.legacy_fallback_stage,
            terminal_failure_stage: self.terminal_failure_stage,
            scroll_content: self.scroll_content,
            resident_release_count: self.resident_release_count,
        }
    }

    #[cfg(test)]
    fn note_resident_release_delta(&mut self, before: usize, after: usize) {
        self.resident_release_count = Some(after.saturating_sub(before));
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
struct PaintAuthorityTelemetrySnapshot {
    authority_label: String,
    selected: PaintAuthorityKind,
    rejection_labels: Vec<String>,
    legacy_fallback_stage: Option<PaintAuthorityFallbackStage>,
    terminal_failure_stage: Option<PaintAuthorityFallbackStage>,
    scroll_content: Option<ScrollContentAuthorityTelemetry>,
    resident_release_count: Option<usize>,
}

#[cfg(test)]
std::thread_local! {
    static PAINT_AUTHORITY_TEST_CAPTURE_ENABLED: std::cell::Cell<bool> =
        std::cell::Cell::new(false);
    static LAST_PAINT_AUTHORITY_TELEMETRY: std::cell::RefCell<Option<PaintAuthorityTelemetrySnapshot>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
struct PaintAuthorityTestCaptureGuard {
    previous: bool,
}

#[cfg(test)]
impl Drop for PaintAuthorityTestCaptureGuard {
    fn drop(&mut self) {
        clear_paint_authority_test_snapshot();
        PAINT_AUTHORITY_TEST_CAPTURE_ENABLED.with(|enabled| enabled.set(self.previous));
    }
}

#[cfg(test)]
fn enable_paint_authority_test_capture() -> PaintAuthorityTestCaptureGuard {
    let previous = PAINT_AUTHORITY_TEST_CAPTURE_ENABLED.with(|enabled| {
        let previous = enabled.get();
        enabled.set(true);
        previous
    });
    clear_paint_authority_test_snapshot();
    PaintAuthorityTestCaptureGuard { previous }
}

#[cfg(test)]
fn paint_authority_test_capture_enabled() -> bool {
    PAINT_AUTHORITY_TEST_CAPTURE_ENABLED.with(std::cell::Cell::get)
}

#[cfg(not(test))]
fn paint_authority_test_capture_enabled() -> bool {
    false
}

#[cfg(test)]
fn store_paint_authority_test_snapshot(telemetry: &PaintAuthorityTelemetry) {
    if paint_authority_test_capture_enabled() {
        LAST_PAINT_AUTHORITY_TELEMETRY
            .with(|snapshot| snapshot.replace(Some(telemetry.snapshot())));
    }
}

#[cfg(test)]
fn clear_paint_authority_test_snapshot() {
    LAST_PAINT_AUTHORITY_TELEMETRY.with(|snapshot| snapshot.borrow_mut().take());
}

#[cfg(test)]
fn begin_paint_authority_telemetry_attempt() {
    if paint_authority_test_capture_enabled() {
        clear_paint_authority_test_snapshot();
    }
}

#[cfg(not(test))]
fn begin_paint_authority_telemetry_attempt() {}

#[cfg(test)]
fn take_paint_authority_test_snapshot() -> Option<PaintAuthorityTelemetrySnapshot> {
    LAST_PAINT_AUTHORITY_TELEMETRY.with(|snapshot| snapshot.borrow_mut().take())
}

#[derive(Clone, Debug)]
struct RecordedArtifactCandidate {
    artifact: crate::view::paint::PaintArtifact,
    eligibility: crate::view::paint::FrameArtifactEligibility,
}

/// One owning M11A decision. Selection records or plans only; it has no
/// frame-graph handle and cannot mutate the viewport runtime. The decision is
/// consumed exactly once by the post-clear dispatch.
enum AutoAuthorityDecision {
    NestedScrollScene {
        prepared: crate::view::paint::PreparedNestedScrollReceiverGeometry,
        trace: AutoAuthorityTrace,
    },
    DirectScrollTransformScene {
        scene: crate::view::paint::ValidatedDirectScrollTransformTransaction,
        trace: AutoAuthorityTrace,
    },
    PropertyScrollScene {
        scene: crate::view::paint::ValidatedPropertyScrollScene,
        trace: AutoAuthorityTrace,
    },
    TransformScrollScene {
        scene: crate::view::paint::ValidatedTransformScrollScene,
        trace: AutoAuthorityTrace,
    },
    EffectScrollScene {
        scene: crate::view::paint::ValidatedEffectScrollSceneCheckpoint,
        trace: AutoAuthorityTrace,
    },
    TransformEffectScrollScene {
        scene: crate::view::paint::ValidatedTransformEffectScrollScene,
        trace: AutoAuthorityTrace,
    },
    PropertyScene {
        plan: crate::view::paint::FramePaintPlan,
        trace: AutoAuthorityTrace,
    },
    Artifact {
        candidate: RecordedArtifactCandidate,
        trace: AutoAuthorityTrace,
    },
    Legacy {
        trace: AutoAuthorityTrace,
    },
}

/// Selection is completed before the common clear or any renderer-specific
/// frame-graph mutation.  Keeping the owned plan here prevents a rejected
/// retained-transform frame from partially entering another paint authority.
enum RetainedTransformCanarySelection {
    Inactive,
    NoTransform,
    SingletonShapeRejected {
        transform_count: usize,
    },
    Planned(crate::view::paint::FramePaintPlan),
    PlanRejected(crate::view::paint::FramePaintPlanError),
    TreePlanned(crate::view::paint::FramePaintPlan),
    TreeShapeRejected {
        transform_count: usize,
    },
    TreePlanRejected(crate::view::paint::FramePaintPlanError),
    IsolationPlanned(crate::view::paint::FramePaintPlan),
    IsolationPlanRejected(crate::view::paint::FramePaintPlanError),
    EffectTreePlanned(crate::view::paint::FramePaintPlan),
    PropertyScenePlanned(crate::view::paint::FramePaintPlan),
    PropertyScenePrepared,
    PropertyScenePrepareRejected(crate::view::paint::RetainedSurfacePrepareError),
    PropertyScrollScenePlanned(crate::view::paint::ValidatedPropertyScrollScene),
    PropertyScrollScenePrepared,
    PropertyScrollScenePrepareRejected(crate::view::paint::RetainedPropertyScrollScenePrepareError),
    NestedScrollScenePlanned(crate::view::paint::PreparedNestedScrollReceiverGeometry),
    NestedScrollScenePrepared,
    NestedScrollScenePrepareRejected(crate::view::paint::RetainedPropertyScrollScenePrepareError),
    DirectScrollTransformScenePlanned(
        crate::view::paint::ValidatedDirectScrollTransformTransaction,
    ),
    DirectScrollTransformScenePrepared,
    DirectScrollTransformScenePrepareRejected(
        crate::view::paint::RetainedPropertyScrollScenePrepareError,
    ),
    TransformScrollScenePlanned(crate::view::paint::ValidatedTransformScrollScene),
    TransformScrollScenePrepared,
    TransformScrollScenePrepareRejected(
        crate::view::paint::RetainedPropertyScrollScenePrepareError,
    ),
    EffectScrollScenePlanned(crate::view::paint::ValidatedEffectScrollSceneCheckpoint),
    EffectScrollScenePrepared,
    EffectScrollScenePrepareRejected(crate::view::paint::RetainedPropertyScrollScenePrepareError),
    TransformEffectScrollScenePlanned(crate::view::paint::ValidatedTransformEffectScrollScene),
    TransformEffectScrollScenePrepared,
    TransformEffectScrollScenePrepareRejected(
        crate::view::paint::RetainedPropertyScrollScenePrepareError,
    ),
    EffectTreeShapeRejected {
        transform_count: usize,
        effect_count: usize,
    },
    EffectTreePlanRejected(crate::view::paint::FramePaintPlanError),
    ScrollHostPlanned(crate::view::paint::FramePaintPlan),
    ScrollHostShapeRejected {
        scroll_count: usize,
    },
    ScrollHostPlanRejected(crate::view::paint::FramePaintPlanError),
    ScrollSceneActive,
    ScrollSceneShapeRejected {
        scroll_count: usize,
    },
    Auto(AutoAuthorityDecision),
    AutoArtifact(RecordedArtifactCandidate),
    AutoLegacy,
}

fn retained_auto_circuit_breaker_selection(
    terminal_failure: Option<RetainedAutoTerminalFailureStage>,
    capture_trace: bool,
) -> Option<RetainedTransformCanarySelection> {
    terminal_failure.map(|_| {
        RetainedTransformCanarySelection::Auto(AutoAuthorityDecision::Legacy {
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

fn preflight_direct_scroll_transform_selection(
    viewport: &mut Viewport,
    graph: &mut FrameGraph,
    ctx: crate::view::base_component::UiBuildContext,
    clear_rgba: [f32; 4],
    frame_owner: Option<crate::view::viewport::RetainedSurfaceFrameStageOwner>,
    selection: RetainedTransformCanarySelection,
) -> (
    RetainedTransformCanarySelection,
    Option<crate::view::paint::RetainedPropertyScrollSceneBuildOutcome>,
) {
    let RetainedTransformCanarySelection::DirectScrollTransformScenePlanned(scene) = selection
    else {
        return (selection, None);
    };
    let Some(frame_owner) = frame_owner else {
        return (
            RetainedTransformCanarySelection::DirectScrollTransformScenePrepareRejected(
                crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable,
            ),
            None,
        );
    };
    match crate::view::paint::prepare_direct_scroll_transform_scene_from_pool(
        viewport,
        scene,
        graph,
        ctx,
        clear_rgba,
        frame_owner,
    ) {
        Ok(prepared) => (
            RetainedTransformCanarySelection::DirectScrollTransformScenePrepared,
            Some(crate::view::paint::emit_prepared_direct_scroll_transform_scene(prepared)),
        ),
        Err(error) => (
            RetainedTransformCanarySelection::DirectScrollTransformScenePrepareRejected(error),
            None,
        ),
    }
}

fn preflight_nested_scroll_selection(
    viewport: &mut Viewport,
    graph: &mut FrameGraph,
    ctx: crate::view::base_component::UiBuildContext,
    clear_rgba: [f32; 4],
    frame_owner: Option<crate::view::viewport::RetainedSurfaceFrameStageOwner>,
    selection: RetainedTransformCanarySelection,
) -> (
    RetainedTransformCanarySelection,
    Option<crate::view::paint::RetainedPropertyScrollSceneBuildOutcome>,
) {
    let RetainedTransformCanarySelection::NestedScrollScenePlanned(prepared_geometry) = selection
    else {
        return (selection, None);
    };
    let Some(frame_owner) = frame_owner else {
        return (
            RetainedTransformCanarySelection::NestedScrollScenePrepareRejected(
                crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable,
            ),
            None,
        );
    };
    match crate::view::paint::prepare_nested_scroll_scene_from_pool(
        viewport,
        prepared_geometry,
        graph,
        ctx,
        clear_rgba,
        frame_owner,
    ) {
        Ok(prepared) => (
            RetainedTransformCanarySelection::NestedScrollScenePrepared,
            Some(crate::view::paint::emit_prepared_nested_scroll_scene(
                prepared,
            )),
        ),
        Err(error) => (
            RetainedTransformCanarySelection::NestedScrollScenePrepareRejected(error),
            None,
        ),
    }
}

fn preflight_transform_effect_scroll_selection(
    viewport: &mut Viewport,
    graph: &mut FrameGraph,
    ctx: crate::view::base_component::UiBuildContext,
    clear_rgba: [f32; 4],
    frame_owner: Option<crate::view::viewport::RetainedSurfaceFrameStageOwner>,
    selection: RetainedTransformCanarySelection,
) -> (
    RetainedTransformCanarySelection,
    Option<crate::view::paint::RetainedPropertyScrollSceneBuildOutcome>,
) {
    let RetainedTransformCanarySelection::TransformEffectScrollScenePlanned(scene) = selection
    else {
        return (selection, None);
    };
    let Some(frame_owner) = frame_owner else {
        return (
            RetainedTransformCanarySelection::TransformEffectScrollScenePrepareRejected(
                crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable,
            ),
            None,
        );
    };
    match crate::view::paint::prepare_retained_transform_effect_scroll_scene_from_pool(
        viewport,
        scene,
        graph,
        ctx,
        clear_rgba,
        frame_owner,
    ) {
        Ok(prepared) => (
            RetainedTransformCanarySelection::TransformEffectScrollScenePrepared,
            Some(
                crate::view::paint::emit_prepared_retained_transform_effect_scroll_scene(prepared),
            ),
        ),
        Err(error) => (
            RetainedTransformCanarySelection::TransformEffectScrollScenePrepareRejected(error),
            None,
        ),
    }
}

fn direct_scroll_transform_prepare_rejection_dispatch(
    error: &crate::view::paint::RetainedPropertyScrollScenePrepareError,
) -> (bool, String) {
    (
        true,
        format!(
            "retained-auto authority=legacy direct-scroll-transform-prepare-rejected={error:?}"
        ),
    )
}

fn direct_scroll_transform_prepare_rejection_fallback_stage() -> PaintAuthorityFallbackStage {
    PaintAuthorityFallbackStage::Prepare
}

fn nested_scroll_prepare_rejection_dispatch(
    error: &crate::view::paint::RetainedPropertyScrollScenePrepareError,
) -> (bool, String) {
    (
        true,
        format!("retained-auto authority=legacy nested-scroll-prepare-rejected={error:?}"),
    )
}

fn nested_scroll_prepare_rejection_fallback_stage() -> PaintAuthorityFallbackStage {
    PaintAuthorityFallbackStage::Prepare
}

fn nested_scroll_success_trace(
    trace: &crate::view::paint::RetainedPropertyScrollSceneBuildTrace,
) -> String {
    format!(
        "retained-auto authority=property-scene phase=nested-scroll topology=S0->S1->leaf roots={} generic-surfaces={} scroll-groups={} backing={:?} tiles={} aggregate-pair-bytes={} reraster={} reuse={} a0=transient-keyless",
        trace.root_count,
        trace.generic_surface_count,
        trace.scroll_group_count,
        trace.backing,
        trace.tile_count,
        trace.content_pair_bytes,
        trace.reraster_count,
        trace.reuse_count,
    )
}

fn transform_effect_scroll_prepare_rejection_dispatch(
    error: &crate::view::paint::RetainedPropertyScrollScenePrepareError,
) -> (bool, String) {
    (
        true,
        format!(
            "retained-auto authority=legacy transform-effect-scroll-prepare-rejected={error:?}"
        ),
    )
}

fn transform_effect_scroll_prepare_rejection_fallback_stage() -> PaintAuthorityFallbackStage {
    PaintAuthorityFallbackStage::Prepare
}

fn record_auto_artifact_candidate(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    promoted_node_ids: &FxHashSet<u64>,
) -> Result<RecordedArtifactCandidate, crate::view::paint::FrameArtifactEligibility> {
    let has_single_root_effect = roots.first().is_some_and(|root| {
        roots.len() == 1
            && property_trees
                .paint_state_for(*root)
                .is_some_and(|properties| properties.effect.is_some())
    });
    let outcome = if has_single_root_effect {
        crate::view::paint::record_root_group_opacity_frame_artifact(
            arena,
            roots,
            promoted_node_ids,
            property_trees,
            paint_generations,
            crate::view::paint::RendererMode::Auto,
        )
    } else {
        crate::view::paint::record_clip_enabled_frame_artifact(
            arena,
            roots,
            promoted_node_ids,
            property_trees,
            paint_generations,
            crate::view::paint::RendererMode::Auto,
        )
    }
    .expect("automatic production selection never forces artifact recording");
    match outcome {
        crate::view::paint::FrameArtifactRecordOutcome::Artifact {
            artifact,
            eligibility,
        } => Ok(RecordedArtifactCandidate {
            artifact,
            eligibility,
        }),
        crate::view::paint::FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) => {
            Err(eligibility)
        }
    }
}

fn reachable_tree_has_scroll_container(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
) -> bool {
    let mut pending = roots.to_vec();
    let mut seen = FxHashSet::default();
    while let Some(key) = pending.pop() {
        if !seen.insert(key) {
            continue;
        }
        let Some(node) = arena.get(key) else {
            continue;
        };
        if node.element.promotion_node_info().is_scroll_container {
            return true;
        }
        pending.extend(node.children().iter().copied());
    }
    false
}

fn select_retained_auto_authority_with_semantics(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    promoted_node_ids: &FxHashSet<u64>,
    ctx: &crate::view::base_component::UiBuildContext,
    semantic_frame_time: crate::time::Instant,
    scroll_budget: crate::view::paint::ScrollSceneSingleTextureBudget,
    capture_trace: bool,
) -> AutoAuthorityDecision {
    let transforms = property_trees.transforms.len();
    let effects = property_trees.effects.len();
    let scrolls = property_trees.scrolls.len();
    let mut trace = AutoAuthorityTrace::new(capture_trace);

    if scrolls != 0 || reachable_tree_has_scroll_container(arena, roots) {
        let viewport = ctx.viewport();
        // Coarse shape only controls whether the dedicated candidate is
        // attempted. The graph-inert planner/compiler/geometry chain below
        // remains the sole authority for exact S0 -> S1 -> leaf admission.
        if roots.len() == 1 && scrolls == 2 {
            match crate::view::paint::plan_and_prepare_nested_scroll_scene(
                arena,
                roots,
                promoted_node_ids,
                property_trees,
                paint_generations,
                viewport.scale_factor(),
                ctx.paint_offset(),
                ctx.graphics_pass_context().scissor_rect,
                viewport.target_format(),
                scroll_budget,
            ) {
                Ok(prepared) => {
                    return AutoAuthorityDecision::NestedScrollScene { prepared, trace };
                }
                Err(error) => {
                    trace.capture(|| AutoAuthorityRejection::NestedScrollPlan { error });
                }
            }
        }
        match crate::view::paint::plan_and_validate_property_scroll_scene(
            arena,
            roots,
            promoted_node_ids,
            property_trees,
            paint_generations,
            viewport.scale_factor(),
            ctx.paint_offset(),
            ctx.graphics_pass_context().scissor_rect,
            semantic_frame_time,
            viewport.target_format(),
            scroll_budget,
        ) {
            Ok(scene) => return AutoAuthorityDecision::PropertyScrollScene { scene, trace },
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::PropertyScrollPlan { error });
            }
        }
        match crate::view::paint::plan_and_validate_transform_scroll_scene(
            arena,
            roots,
            promoted_node_ids,
            property_trees,
            paint_generations,
            viewport.scale_factor(),
            ctx.paint_offset(),
            ctx.graphics_pass_context().scissor_rect,
            semantic_frame_time,
            viewport.target_format(),
            scroll_budget,
        ) {
            Ok(scene) => return AutoAuthorityDecision::TransformScrollScene { scene, trace },
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::TransformScrollPlan { error });
            }
        }
        match crate::view::paint::plan_and_validate_effect_scroll_scene_checkpoint(
            arena,
            roots,
            promoted_node_ids,
            property_trees,
            paint_generations,
            viewport.scale_factor(),
            ctx.paint_offset(),
            ctx.graphics_pass_context().scissor_rect,
            semantic_frame_time,
            viewport.target_format(),
            scroll_budget,
        ) {
            Ok(scene) => return AutoAuthorityDecision::EffectScrollScene { scene, trace },
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::EffectScrollPlan { error });
            }
        }
        match crate::view::paint::plan_and_validate_transform_effect_scroll_scene(
            arena,
            roots,
            promoted_node_ids,
            property_trees,
            paint_generations,
            viewport.scale_factor(),
            ctx.paint_offset(),
            ctx.graphics_pass_context().scissor_rect,
            semantic_frame_time,
            viewport.target_format(),
            scroll_budget,
        ) {
            Ok(scene) => return AutoAuthorityDecision::TransformEffectScrollScene { scene, trace },
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::TransformEffectScrollPlan { error });
            }
        }
        return match crate::view::paint::plan_and_validate_direct_scroll_transform_scene(
            arena,
            roots,
            promoted_node_ids,
            property_trees,
            paint_generations,
            viewport.scale_factor(),
            ctx.paint_offset(),
            ctx.graphics_pass_context().scissor_rect,
            viewport.target_format(),
            scroll_budget,
        ) {
            Ok(scene) => AutoAuthorityDecision::DirectScrollTransformScene { scene, trace },
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::DirectScrollTransformPlan { error });
                AutoAuthorityDecision::Legacy { trace }
            }
        };
    }

    if effects != 0 {
        let plan_context = crate::view::paint::TransformSurfacePlanContext::new(
            ctx.paint_offset(),
            ctx.graphics_pass_context().scissor_rect,
        );
        return match crate::view::paint::plan_property_effect_scene_with_context(
            arena,
            roots,
            promoted_node_ids,
            property_trees,
            paint_generations,
            plan_context,
        ) {
            Ok(plan) => AutoAuthorityDecision::PropertyScene { plan, trace },
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::Plan {
                    authority: AutoAuthorityKind::PropertyScene,
                    error,
                });
                AutoAuthorityDecision::Legacy { trace }
            }
        };
    }

    if transforms != 0 && effects == 0 {
        let plan_context = crate::view::paint::TransformSurfacePlanContext::new(
            ctx.paint_offset(),
            ctx.graphics_pass_context().scissor_rect,
        );
        return match crate::view::paint::plan_transform_property_scene_with_context(
            arena,
            roots,
            promoted_node_ids,
            property_trees,
            paint_generations,
            plan_context,
        ) {
            Ok(plan) => AutoAuthorityDecision::PropertyScene { plan, trace },
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::Plan {
                    authority: AutoAuthorityKind::PropertyScene,
                    error,
                });
                AutoAuthorityDecision::Legacy { trace }
            }
        };
    }

    match record_auto_artifact_candidate(
        arena,
        roots,
        property_trees,
        paint_generations,
        promoted_node_ids,
    ) {
        Ok(candidate) => AutoAuthorityDecision::Artifact { candidate, trace },
        Err(eligibility) => {
            trace.capture(|| AutoAuthorityRejection::Artifact { eligibility });
            AutoAuthorityDecision::Legacy { trace }
        }
    }
}

#[cfg(test)]
fn select_retained_auto_authority(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    promoted_node_ids: &FxHashSet<u64>,
    ctx: &crate::view::base_component::UiBuildContext,
    capture_trace: bool,
) -> AutoAuthorityDecision {
    let scroll_budget = crate::view::paint::ScrollSceneSingleTextureBudget::new(
        wgpu::Limits::default().max_texture_dimension_2d,
        128 * 1024 * 1024,
    )
    .expect("test scroll budget is non-zero");
    select_retained_auto_authority_with_semantics(
        arena,
        roots,
        property_trees,
        paint_generations,
        promoted_node_ids,
        ctx,
        crate::time::Instant::now(),
        scroll_budget,
        capture_trace,
    )
}

#[cfg(test)]
fn select_retained_transform_canary(
    mode: ViewportPaintRendererMode,
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    promoted_node_ids: &FxHashSet<u64>,
    ctx: &crate::view::base_component::UiBuildContext,
) -> RetainedTransformCanarySelection {
    let scroll_budget = crate::view::paint::ScrollSceneSingleTextureBudget::new(
        wgpu::Limits::default().max_texture_dimension_2d,
        128 * 1024 * 1024,
    )
    .expect("test scroll budget is non-zero");
    select_retained_transform_canary_with_trace_capture(
        mode,
        arena,
        roots,
        property_trees,
        paint_generations,
        promoted_node_ids,
        ctx,
        crate::time::Instant::now(),
        scroll_budget,
        false,
    )
}

fn select_retained_transform_canary_with_trace_capture(
    mode: ViewportPaintRendererMode,
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    promoted_node_ids: &FxHashSet<u64>,
    ctx: &crate::view::base_component::UiBuildContext,
    semantic_frame_time: crate::time::Instant,
    scroll_budget: crate::view::paint::ScrollSceneSingleTextureBudget,
    capture_auto_trace: bool,
) -> RetainedTransformCanarySelection {
    match mode {
        ViewportPaintRendererMode::RetainedTransformCanary
            if property_trees.transforms.is_empty() =>
        {
            RetainedTransformCanarySelection::NoTransform
        }
        ViewportPaintRendererMode::RetainedTransformCanary
            if property_trees.transforms.len() != 1 =>
        {
            RetainedTransformCanarySelection::SingletonShapeRejected {
                transform_count: property_trees.transforms.len(),
            }
        }
        ViewportPaintRendererMode::RetainedTransformCanary => {
            let plan_context = crate::view::paint::TransformSurfacePlanContext::new(
                ctx.paint_offset(),
                ctx.graphics_pass_context().scissor_rect,
            );
            match crate::view::paint::plan_single_root_transform_surface_with_context(
                arena,
                roots,
                promoted_node_ids,
                property_trees,
                paint_generations,
                plan_context,
            ) {
                Ok(plan) => RetainedTransformCanarySelection::Planned(plan),
                Err(error) => RetainedTransformCanarySelection::PlanRejected(error),
            }
        }
        ViewportPaintRendererMode::RetainedSurfaceTreeCanary
            if property_trees.transforms.len() != 2 =>
        {
            RetainedTransformCanarySelection::TreeShapeRejected {
                transform_count: property_trees.transforms.len(),
            }
        }
        ViewportPaintRendererMode::RetainedSurfaceTreeCanary => {
            let plan_context = crate::view::paint::TransformSurfacePlanContext::new(
                ctx.paint_offset(),
                ctx.graphics_pass_context().scissor_rect,
            );
            match crate::view::paint::plan_single_root_transform_surface_with_context(
                arena,
                roots,
                promoted_node_ids,
                property_trees,
                paint_generations,
                plan_context,
            ) {
                Ok(plan) => RetainedTransformCanarySelection::TreePlanned(plan),
                Err(error) => RetainedTransformCanarySelection::TreePlanRejected(error),
            }
        }
        ViewportPaintRendererMode::RetainedIsolationCanary => {
            let viewport = ctx.viewport();
            match crate::view::paint::plan_single_root_isolation_surface(
                arena,
                roots,
                promoted_node_ids,
                property_trees,
                paint_generations,
                viewport.target_width(),
                viewport.target_height(),
                viewport.scale_factor(),
                ctx.graphics_pass_context().scissor_rect,
            ) {
                Ok(plan) => RetainedTransformCanarySelection::IsolationPlanned(plan),
                Err(error) => RetainedTransformCanarySelection::IsolationPlanRejected(error),
            }
        }
        ViewportPaintRendererMode::RetainedEffectTreeCanary
            if property_trees.transforms.len() != 1 || property_trees.effects.len() != 1 =>
        {
            RetainedTransformCanarySelection::EffectTreeShapeRejected {
                transform_count: property_trees.transforms.len(),
                effect_count: property_trees.effects.len(),
            }
        }
        ViewportPaintRendererMode::RetainedEffectTreeCanary => {
            let plan_context = crate::view::paint::TransformSurfacePlanContext::new(
                ctx.paint_offset(),
                ctx.graphics_pass_context().scissor_rect,
            );
            match crate::view::paint::plan_single_root_transform_child_isolation_surface_with_context(
                arena,
                roots,
                promoted_node_ids,
                property_trees,
                paint_generations,
                plan_context,
            ) {
                Ok(plan) => RetainedTransformCanarySelection::EffectTreePlanned(plan),
                Err(error) => RetainedTransformCanarySelection::EffectTreePlanRejected(error),
            }
        }
        ViewportPaintRendererMode::RetainedScrollHostCanary
            if property_trees.scrolls.len() != 1 =>
        {
            RetainedTransformCanarySelection::ScrollHostShapeRejected {
                scroll_count: property_trees.scrolls.len(),
            }
        }
        ViewportPaintRendererMode::RetainedScrollHostCanary => {
            let viewport = ctx.viewport();
            match crate::view::paint::plan_single_root_scroll_host_surface(
                arena,
                roots,
                promoted_node_ids,
                property_trees,
                paint_generations,
                viewport.scale_factor(),
                ctx.paint_offset(),
                ctx.graphics_pass_context().scissor_rect,
            ) {
                Ok(plan) => RetainedTransformCanarySelection::ScrollHostPlanned(plan),
                Err(error) => RetainedTransformCanarySelection::ScrollHostPlanRejected(error),
            }
        }
        ViewportPaintRendererMode::RetainedScrollSceneCanary
            if property_trees.scrolls.len() != 1 =>
        {
            RetainedTransformCanarySelection::ScrollSceneShapeRejected {
                scroll_count: property_trees.scrolls.len(),
            }
        }
        ViewportPaintRendererMode::RetainedScrollSceneCanary => {
            RetainedTransformCanarySelection::ScrollSceneActive
        }
        ViewportPaintRendererMode::RetainedAuto => {
            RetainedTransformCanarySelection::Auto(select_retained_auto_authority_with_semantics(
                arena,
                roots,
                property_trees,
                paint_generations,
                promoted_node_ids,
                ctx,
                semantic_frame_time,
                scroll_budget,
                capture_auto_trace,
            ))
        }
        ViewportPaintRendererMode::Legacy | ViewportPaintRendererMode::ArtifactCanary => {
            RetainedTransformCanarySelection::Inactive
        }
    }
}

#[derive(Clone)]
struct RootEffectBuildPlan {
    committed: RootEffectRetainedState,
    key: crate::view::frame_graph::PersistentTextureKey,
    target: crate::view::paint::RootEffectRasterInputs,
    pair_resident: bool,
}

/// Single production dispatch point for the artifact canary. A single root
/// effect selects M6C1 true group opacity; an effect-neutral frame selects
/// baked-opacity authority with validated property-tree clips. Either path
/// owns the complete frame or emits no artifact pass at all.
/// The function either
/// compiles one owning artifact for the complete frame or emits no artifact
/// pass at all, leaving the caller free to run the unchanged legacy walk.
fn try_build_property_neutral_artifact_frame(
    graph: &mut FrameGraph,
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    promoted_node_ids: &FxHashSet<u64>,
    mode: ViewportPaintRendererMode,
    ctx: &crate::view::base_component::UiBuildContext,
    root_effect_plan: Option<&RootEffectBuildPlan>,
) -> PropertyNeutralArtifactAttempt {
    let recorder_mode = match mode {
        ViewportPaintRendererMode::Legacy => crate::view::paint::RendererMode::Legacy,
        ViewportPaintRendererMode::ArtifactCanary => crate::view::paint::RendererMode::Auto,
        // The retained-transform canary has a separate whole-frame dispatch
        // point and must never enter the generic artifact compiler.
        ViewportPaintRendererMode::RetainedTransformCanary
        | ViewportPaintRendererMode::RetainedSurfaceTreeCanary
        | ViewportPaintRendererMode::RetainedIsolationCanary
        | ViewportPaintRendererMode::RetainedEffectTreeCanary
        | ViewportPaintRendererMode::RetainedScrollHostCanary
        | ViewportPaintRendererMode::RetainedScrollSceneCanary
        | ViewportPaintRendererMode::RetainedAuto => crate::view::paint::RendererMode::Legacy,
    };
    let has_single_root_effect = roots.first().is_some_and(|root| {
        roots.len() == 1
            && property_trees
                .paint_state_for(*root)
                .is_some_and(|properties| properties.effect.is_some())
    });
    let outcome = if has_single_root_effect {
        crate::view::paint::record_root_group_opacity_frame_artifact(
            arena,
            roots,
            promoted_node_ids,
            property_trees,
            paint_generations,
            recorder_mode,
        )
    } else {
        crate::view::paint::record_clip_enabled_frame_artifact(
            arena,
            roots,
            promoted_node_ids,
            property_trees,
            paint_generations,
            recorder_mode,
        )
    }
    .expect("production paint modes never request forced artifact recording");
    match outcome {
        crate::view::paint::FrameArtifactRecordOutcome::Artifact {
            artifact,
            eligibility,
        } => try_compile_recorded_artifact_frame(
            graph,
            RecordedArtifactCandidate {
                artifact,
                eligibility,
            },
            ctx,
            root_effect_plan,
        ),
        crate::view::paint::FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) => {
            PropertyNeutralArtifactAttempt::WholeFrameLegacy { eligibility }
        }
    }
}

fn try_compile_recorded_artifact_frame(
    graph: &mut FrameGraph,
    candidate: RecordedArtifactCandidate,
    ctx: &crate::view::base_component::UiBuildContext,
    root_effect_plan: Option<&RootEffectBuildPlan>,
) -> PropertyNeutralArtifactAttempt {
    let RecordedArtifactCandidate {
        artifact,
        eligibility,
    } = candidate;
    let artifact_ctx =
        crate::view::base_component::UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
    let root_effect = match artifact.target {
        crate::view::paint::PaintArtifactTarget::RootOpacityGroup { root, .. } => {
            let Some(plan) = root_effect_plan.filter(|plan| {
                plan.key == crate::view::base_component::root_effect_stable_key(root)
            }) else {
                return PropertyNeutralArtifactAttempt::CompileRejected(
                    crate::view::paint::ArtifactCompileErrorKind::InvalidStore,
                );
            };
            let Some(stamp) =
                crate::view::paint::validated_root_effect_raster_stamp(&artifact, plan.target)
            else {
                return PropertyNeutralArtifactAttempt::CompileRejected(
                    crate::view::paint::ArtifactCompileErrorKind::InvalidStore,
                );
            };
            let action = plan
                .committed
                .compile_action(&stamp, plan.key, plan.pair_resident);
            Some((
                action,
                PendingRootEffectTransaction::Commit {
                    stamp,
                    key: plan.key,
                    action,
                },
            ))
        }
        crate::view::paint::PaintArtifactTarget::CurrentTarget => None,
    };
    let compiled = match &root_effect {
        Some((action, _)) => crate::view::paint::try_compile_root_effect_artifact(
            &artifact,
            *action,
            graph,
            artifact_ctx,
        ),
        None => crate::view::paint::try_compile_artifact(&artifact, graph, artifact_ctx),
    };
    match compiled {
        Ok(state) => PropertyNeutralArtifactAttempt::Compiled {
            state,
            eligibility,
            root_effect_transaction: root_effect.map(|(_, transaction)| transaction),
        },
        Err(error) => PropertyNeutralArtifactAttempt::CompileRejected(error.kind()),
    }
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

    fn push_debug_reuse_overlay_geometry(&mut self, reuse_records: &[DebugReusePathRecord]) {
        if !self.debug_options.trace_reuse_path {
            return;
        }
        let scale = self.scale_factor.max(0.0001);
        let screen_w = self.gpu.surface_config.width.max(1) as f32;
        let screen_h = self.gpu.surface_config.height.max(1) as f32;
        let arena = &self.scene.node_arena;
        let mut snapshots_by_id: FxHashMap<u64, crate::view::base_component::BoxModelSnapshot> =
            FxHashMap::default();
        for &root_key in &self.scene.ui_root_keys {
            for snapshot in
                crate::view::viewport::scene_helpers::collect_box_models(root_key, arena)
            {
                snapshots_by_id.insert(snapshot.node_id, snapshot);
            }
        }
        let mut overlay_batches = Vec::new();
        let promoted_node_ids = self.compositor.promotion_state.promoted_node_ids.clone();
        for record in reuse_records {
            let Some(snapshot) = snapshots_by_id.get(&record.node_id).copied() else {
                continue;
            };
            if !snapshot.should_render {
                continue;
            }
            let color = reuse_overlay_color(record.actual, record.reason);
            let label = promoted_node_ids
                .contains(&record.node_id)
                .then(|| record.node_id.to_string());
            let (vertices, indices) = build_reuse_overlay_geometry(
                &snapshot,
                scale,
                screen_w,
                screen_h,
                color,
                label.as_deref(),
            );
            overlay_batches.push((vertices, indices));
        }
        for (vertices, indices) in overlay_batches {
            self.push_debug_overlay_geometry(&vertices, &indices);
        }
    }

    /// Build the hierarchical trace tree from collected frame timings.
    fn build_frame_trace_tree(&self, t: &FrameTimings) -> TraceRenderNode {
        let opts = &self.debug_options;
        let any_detail =
            opts.trace_layout_detail || opts.trace_compile_detail || opts.trace_execute_detail;
        let layout_with_transition_ms = t.layout_ms + t.post_layout_transition_ms + t.relayout_ms;

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
                        t.layout_measure_ms + t.layout_place_ms + t.layout_collect_box_models_ms,
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
                execute_children,
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

        TraceRenderNode::with_children(
            format!("render_frame #{}", t.frame_number),
            t.rsx_build_ms + t.total_ms,
            vec![
                TraceRenderNode::new("rsx_build", t.rsx_build_ms),
                begin_frame,
                layout,
                TraceRenderNode::new("update_promotion_state", t.update_promotion_ms),
                TraceRenderNode::new("build_graph", t.build_graph_ms),
                compile,
                execute,
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
        self.frame.frame_number = self.frame.frame_number.saturating_add(1);
        let frame_number = self.frame.frame_number;
        set_debug_trace_enabled(self.debug_options.trace_reuse_path);
        trace_promoted_build_frame_marker();
        begin_debug_reuse_path_frame();
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
            begin_frame_ms: begin_frame_profile.total_ms,
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
        // exactly once for this frame before property-tree observation,
        // promotion, and paint recording. This pass cannot mutate arena
        // topology; slot changes caused by async completion wait until the
        // next frame's pre-layout sync.
        self.scene.node_arena.prepare_registered_paint_resources(
            crate::view::base_component::PaintResourcePreparationContext {
                frame_number,
                device_scale: self.scale_factor,
                now: semantic_now,
            },
        );

        // Observe the final resolved frame state after transition sampling
        // and any required relayout.  These shadow trees do not yet drive
        // rendering, promotion, or dirty classification.
        self.sync_compositor_property_trees();

        // --- Promotion ---
        let update_promotion_started_at = Instant::now();
        self.maybe_evaluate_raster_budget_readiness();
        self.update_promotion_state();
        self.maybe_sync_shadow_layer_tree();
        self.maybe_plan_prospective_raster_resources();
        timings.update_promotion_ms = update_promotion_started_at.elapsed().as_secs_f64() * 1000.0;

        // --- Build frame graph ---
        let build_graph_started_at = Instant::now();
        self.clear_debug_overlay_geometry();
        let mut graph = FrameGraph::new();
        let mut ctx = crate::view::base_component::UiBuildContext::new(
            self.gpu.surface_config.width,
            self.gpu.surface_config.height,
            self.offscreen_format(),
            self.scale_factor,
        );
        self.apply_promotion_runtime(&mut ctx);
        let retained_surface_frame_owner = self.begin_retained_surface_frame_stage();
        let root_keys_for_build = self.scene.ui_root_keys.clone();
        let capture_paint_authority_telemetry =
            self.debug_options.trace_render_time || paint_authority_test_capture_enabled();
        let property_scroll_budget = crate::view::paint::production_single_texture_budget(self);
        let retained_auto_terminal_failure = self.retained_auto_terminal_failure;
        let retained_transform_selection = retained_auto_circuit_breaker_selection(
            retained_auto_terminal_failure,
            capture_paint_authority_telemetry,
        )
        .unwrap_or_else(|| {
            select_retained_transform_canary_with_trace_capture(
                self.paint_renderer_mode,
                &self.scene.node_arena,
                &root_keys_for_build,
                &self.compositor.property_trees,
                &self.compositor.paint_generations,
                &self.compositor.promotion_state.promoted_node_ids,
                &ctx,
                semantic_now,
                property_scroll_budget,
                capture_paint_authority_telemetry,
            )
        });
        let (mut retained_transform_selection, auto_authority_trace) =
            match retained_transform_selection {
                RetainedTransformCanarySelection::Auto(decision) => match decision {
                    AutoAuthorityDecision::NestedScrollScene { prepared, trace } => (
                        RetainedTransformCanarySelection::NestedScrollScenePlanned(prepared),
                        Some((AutoAuthorityKind::PropertyScene, trace)),
                    ),
                    AutoAuthorityDecision::DirectScrollTransformScene { scene, trace } => (
                        RetainedTransformCanarySelection::DirectScrollTransformScenePlanned(scene),
                        Some((AutoAuthorityKind::PropertyScene, trace)),
                    ),
                    AutoAuthorityDecision::PropertyScrollScene { scene, trace } => (
                        RetainedTransformCanarySelection::PropertyScrollScenePlanned(scene),
                        Some((AutoAuthorityKind::PropertyScene, trace)),
                    ),
                    AutoAuthorityDecision::TransformScrollScene { scene, trace } => (
                        RetainedTransformCanarySelection::TransformScrollScenePlanned(scene),
                        Some((AutoAuthorityKind::PropertyScene, trace)),
                    ),
                    AutoAuthorityDecision::EffectScrollScene { scene, trace } => (
                        RetainedTransformCanarySelection::EffectScrollScenePlanned(scene),
                        Some((AutoAuthorityKind::PropertyScene, trace)),
                    ),
                    AutoAuthorityDecision::TransformEffectScrollScene { scene, trace } => (
                        RetainedTransformCanarySelection::TransformEffectScrollScenePlanned(scene),
                        Some((AutoAuthorityKind::PropertyScene, trace)),
                    ),
                    AutoAuthorityDecision::PropertyScene { plan, trace } => (
                        RetainedTransformCanarySelection::PropertyScenePlanned(plan),
                        Some((AutoAuthorityKind::PropertyScene, trace)),
                    ),
                    AutoAuthorityDecision::Artifact { candidate, trace } => (
                        RetainedTransformCanarySelection::AutoArtifact(candidate),
                        Some((AutoAuthorityKind::Artifact, trace)),
                    ),
                    AutoAuthorityDecision::Legacy { trace } => (
                        RetainedTransformCanarySelection::AutoLegacy,
                        Some((AutoAuthorityKind::Legacy, trace)),
                    ),
                },
                selection => (selection, None),
            };
        let property_scene_plan_owner = if matches!(
            retained_transform_selection,
            RetainedTransformCanarySelection::PropertyScenePlanned(_)
        ) {
            let selection = std::mem::replace(
                &mut retained_transform_selection,
                RetainedTransformCanarySelection::PropertyScenePrepared,
            );
            let RetainedTransformCanarySelection::PropertyScenePlanned(plan) = selection else {
                unreachable!("property-scene preflight extracts only its owned plan")
            };
            Some(plan)
        } else {
            None
        };
        let mut prepared_property_scene = None;
        if let Some(property_scene_plan) = property_scene_plan_owner.as_ref() {
            match crate::view::paint::prepare_retained_property_scene_from_pool(
                self,
                property_scene_plan,
                &graph,
                &ctx,
            ) {
                Ok(prepared) => prepared_property_scene = Some(prepared),
                Err(error) => {
                    retained_transform_selection =
                        RetainedTransformCanarySelection::PropertyScenePrepareRejected(error);
                }
            }
        }
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
        let (selection, mut pre_emitted_nested_scroll) = if matches!(
            retained_transform_selection,
            RetainedTransformCanarySelection::NestedScrollScenePlanned(_)
        ) {
            let selection = std::mem::replace(
                &mut retained_transform_selection,
                RetainedTransformCanarySelection::NestedScrollScenePrepared,
            );
            let scroll_ctx = crate::view::base_component::UiBuildContext::from_parts(
                ctx.viewport(),
                ctx.state_clone(),
            );
            preflight_nested_scroll_selection(
                self,
                &mut graph,
                scroll_ctx,
                clear_rgba,
                retained_surface_frame_owner,
                selection,
            )
        } else {
            (retained_transform_selection, None)
        };
        retained_transform_selection = selection;
        let (selection, mut pre_emitted_direct_scroll_transform) = if matches!(
            retained_transform_selection,
            RetainedTransformCanarySelection::DirectScrollTransformScenePlanned(_)
        ) {
            let selection = std::mem::replace(
                &mut retained_transform_selection,
                RetainedTransformCanarySelection::DirectScrollTransformScenePrepared,
            );
            let scroll_ctx = crate::view::base_component::UiBuildContext::from_parts(
                ctx.viewport(),
                ctx.state_clone(),
            );
            preflight_direct_scroll_transform_selection(
                self,
                &mut graph,
                scroll_ctx,
                clear_rgba,
                retained_surface_frame_owner,
                selection,
            )
        } else {
            (retained_transform_selection, None)
        };
        retained_transform_selection = selection;
        let property_scroll_scene_owner = if matches!(
            retained_transform_selection,
            RetainedTransformCanarySelection::PropertyScrollScenePlanned(_)
        ) {
            let selection = std::mem::replace(
                &mut retained_transform_selection,
                RetainedTransformCanarySelection::PropertyScrollScenePrepared,
            );
            let RetainedTransformCanarySelection::PropertyScrollScenePlanned(scene) = selection
            else {
                unreachable!("property-scroll preflight extracts only its owned scene")
            };
            Some(scene)
        } else {
            None
        };
        let mut pre_emitted_property_scroll = None;
        if let Some(scene) = property_scroll_scene_owner {
            let scroll_ctx = crate::view::base_component::UiBuildContext::from_parts(
                ctx.viewport(),
                ctx.state_clone(),
            );
            match retained_surface_frame_owner {
                Some(frame_owner) => {
                    match crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
                        self,
                        scene,
                        &mut graph,
                        scroll_ctx,
                        clear_rgba,
                        frame_owner,
                    ) {
                        Ok(prepared) => {
                            pre_emitted_property_scroll = Some(
                                crate::view::paint::emit_prepared_retained_property_scroll_forest(
                                    prepared,
                                ),
                            );
                        }
                        Err(error) => {
                            retained_transform_selection = RetainedTransformCanarySelection::
                                PropertyScrollScenePrepareRejected(error);
                        }
                    }
                }
                None => {
                    retained_transform_selection =
                        RetainedTransformCanarySelection::PropertyScrollScenePrepareRejected(
                            crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable,
                        );
                }
            }
        }
        let transform_scroll_scene_owner = if matches!(
            retained_transform_selection,
            RetainedTransformCanarySelection::TransformScrollScenePlanned(_)
        ) {
            let selection = std::mem::replace(
                &mut retained_transform_selection,
                RetainedTransformCanarySelection::TransformScrollScenePrepared,
            );
            let RetainedTransformCanarySelection::TransformScrollScenePlanned(scene) = selection
            else {
                unreachable!("transform-scroll preflight extracts only its owned scene")
            };
            Some(scene)
        } else {
            None
        };
        let mut pre_emitted_transform_scroll = None;
        if let Some(scene) = transform_scroll_scene_owner {
            let scroll_ctx = crate::view::base_component::UiBuildContext::from_parts(
                ctx.viewport(),
                ctx.state_clone(),
            );
            match retained_surface_frame_owner {
                Some(frame_owner) => {
                    match crate::view::paint::prepare_retained_transform_scroll_scene_from_pool(
                        self,
                        scene,
                        &mut graph,
                        scroll_ctx,
                        clear_rgba,
                        frame_owner,
                    ) {
                        Ok(prepared) => {
                            pre_emitted_transform_scroll = Some(
                                crate::view::paint::emit_prepared_retained_transform_scroll_scene(
                                    prepared,
                                ),
                            );
                        }
                        Err(error) => {
                            retained_transform_selection = RetainedTransformCanarySelection::
                                TransformScrollScenePrepareRejected(error);
                        }
                    }
                }
                None => {
                    retained_transform_selection =
                        RetainedTransformCanarySelection::TransformScrollScenePrepareRejected(
                            crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable,
                        );
                }
            }
        }
        let effect_scroll_scene_owner = if matches!(
            retained_transform_selection,
            RetainedTransformCanarySelection::EffectScrollScenePlanned(_)
        ) {
            let selection = std::mem::replace(
                &mut retained_transform_selection,
                RetainedTransformCanarySelection::EffectScrollScenePrepared,
            );
            let RetainedTransformCanarySelection::EffectScrollScenePlanned(scene) = selection
            else {
                unreachable!("effect-scroll preflight extracts only its owned scene")
            };
            Some(scene)
        } else {
            None
        };
        let mut pre_emitted_effect_scroll = None;
        if let Some(scene) = effect_scroll_scene_owner {
            let scroll_ctx = crate::view::base_component::UiBuildContext::from_parts(
                ctx.viewport(),
                ctx.state_clone(),
            );
            match retained_surface_frame_owner {
                Some(frame_owner) => {
                    match crate::view::paint::prepare_retained_effect_scroll_scene_from_pool(
                        self,
                        scene,
                        &mut graph,
                        scroll_ctx,
                        clear_rgba,
                        frame_owner,
                    ) {
                        Ok(prepared) => {
                            pre_emitted_effect_scroll = Some(
                                crate::view::paint::emit_prepared_retained_effect_scroll_scene(
                                    prepared,
                                ),
                            );
                        }
                        Err(error) => {
                            retained_transform_selection =
                                RetainedTransformCanarySelection::EffectScrollScenePrepareRejected(
                                    error,
                                );
                        }
                    }
                }
                None => {
                    retained_transform_selection =
                        RetainedTransformCanarySelection::EffectScrollScenePrepareRejected(
                            crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable,
                        );
                }
            }
        }
        let (selection, mut pre_emitted_transform_effect_scroll) = if matches!(
            retained_transform_selection,
            RetainedTransformCanarySelection::TransformEffectScrollScenePlanned(_)
        ) {
            let selection = std::mem::replace(
                &mut retained_transform_selection,
                RetainedTransformCanarySelection::TransformEffectScrollScenePrepared,
            );
            let scroll_ctx = crate::view::base_component::UiBuildContext::from_parts(
                ctx.viewport(),
                ctx.state_clone(),
            );
            preflight_transform_effect_scroll_selection(
                self,
                &mut graph,
                scroll_ctx,
                clear_rgba,
                retained_surface_frame_owner,
                selection,
            )
        } else {
            (retained_transform_selection, None)
        };
        retained_transform_selection = selection;
        let mut paint_authority_telemetry = capture_paint_authority_telemetry.then(|| {
            PaintAuthorityTelemetry::from_selection(
                self.paint_renderer_mode,
                &retained_transform_selection,
                auto_authority_trace,
            )
        });
        let mut dispatch_legacy_fallback_stage =
            paint_authority_telemetry
                .as_ref()
                .and_then(|_| match &retained_transform_selection {
                    RetainedTransformCanarySelection::Planned(_)
                    | RetainedTransformCanarySelection::PropertyScenePlanned(_)
                    | RetainedTransformCanarySelection::PropertyScenePrepared
                    | RetainedTransformCanarySelection::PropertyScenePrepareRejected(_)
                    | RetainedTransformCanarySelection::PropertyScrollScenePlanned(_)
                    | RetainedTransformCanarySelection::PropertyScrollScenePrepared
                    | RetainedTransformCanarySelection::PropertyScrollScenePrepareRejected(_)
                    | RetainedTransformCanarySelection::NestedScrollScenePlanned(_)
                    | RetainedTransformCanarySelection::NestedScrollScenePrepared
                    | RetainedTransformCanarySelection::DirectScrollTransformScenePlanned(_)
                    | RetainedTransformCanarySelection::DirectScrollTransformScenePrepared
                    | RetainedTransformCanarySelection::TransformScrollScenePlanned(_)
                    | RetainedTransformCanarySelection::TransformScrollScenePrepared
                    | RetainedTransformCanarySelection::TransformScrollScenePrepareRejected(_)
                    | RetainedTransformCanarySelection::EffectScrollScenePlanned(_)
                    | RetainedTransformCanarySelection::EffectScrollScenePrepared
                    | RetainedTransformCanarySelection::EffectScrollScenePrepareRejected(_)
                    | RetainedTransformCanarySelection::TransformEffectScrollScenePlanned(_)
                    | RetainedTransformCanarySelection::TransformEffectScrollScenePrepared
                    | RetainedTransformCanarySelection::TreePlanned(_)
                    | RetainedTransformCanarySelection::IsolationPlanned(_)
                    | RetainedTransformCanarySelection::EffectTreePlanned(_)
                    | RetainedTransformCanarySelection::ScrollHostPlanned(_)
                    | RetainedTransformCanarySelection::ScrollSceneActive => {
                        Some(PaintAuthorityFallbackStage::Prepare)
                    }
                    RetainedTransformCanarySelection::TransformEffectScrollScenePrepareRejected(
                        _,
                    ) => Some(transform_effect_scroll_prepare_rejection_fallback_stage()),
                    RetainedTransformCanarySelection::DirectScrollTransformScenePrepareRejected(
                        _,
                    ) => Some(direct_scroll_transform_prepare_rejection_fallback_stage()),
                    RetainedTransformCanarySelection::NestedScrollScenePrepareRejected(_) => {
                        Some(nested_scroll_prepare_rejection_fallback_stage())
                    }
                    RetainedTransformCanarySelection::NoTransform
                    | RetainedTransformCanarySelection::SingletonShapeRejected { .. }
                    | RetainedTransformCanarySelection::PlanRejected(_)
                    | RetainedTransformCanarySelection::TreeShapeRejected { .. }
                    | RetainedTransformCanarySelection::TreePlanRejected(_)
                    | RetainedTransformCanarySelection::IsolationPlanRejected(_)
                    | RetainedTransformCanarySelection::EffectTreeShapeRejected { .. }
                    | RetainedTransformCanarySelection::EffectTreePlanRejected(_)
                    | RetainedTransformCanarySelection::ScrollHostShapeRejected { .. }
                    | RetainedTransformCanarySelection::ScrollHostPlanRejected(_)
                    | RetainedTransformCanarySelection::ScrollSceneShapeRejected { .. }
                    | RetainedTransformCanarySelection::AutoLegacy => {
                        Some(PaintAuthorityFallbackStage::Selection)
                    }
                    RetainedTransformCanarySelection::AutoArtifact(_) => {
                        Some(PaintAuthorityFallbackStage::Compile)
                    }
                    RetainedTransformCanarySelection::Inactive => (self.paint_renderer_mode
                        == ViewportPaintRendererMode::ArtifactCanary)
                        .then_some(PaintAuthorityFallbackStage::Selection),
                    RetainedTransformCanarySelection::Auto(_) => None,
                });
        if paint_authority_telemetry.is_some()
            && let Some(stage) = retained_auto_terminal_failure
        {
            dispatch_legacy_fallback_stage = Some(retained_auto_terminal_fallback_stage(stage));
        }
        #[cfg(test)]
        let retained_release_count_before = paint_authority_telemetry
            .as_ref()
            .map(|_| self.retained_surface_release_log_for_test().len());
        if pre_emitted_nested_scroll.is_none()
            && pre_emitted_direct_scroll_transform.is_none()
            && pre_emitted_property_scroll.is_none()
            && pre_emitted_transform_scroll.is_none()
            && pre_emitted_effect_scroll.is_none()
            && pre_emitted_transform_effect_scroll.is_none()
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
        let root_effect_plan = (!matches!(
            self.paint_renderer_mode,
            ViewportPaintRendererMode::RetainedTransformCanary
                | ViewportPaintRendererMode::RetainedSurfaceTreeCanary
                | ViewportPaintRendererMode::RetainedIsolationCanary
                | ViewportPaintRendererMode::RetainedEffectTreeCanary
                | ViewportPaintRendererMode::RetainedScrollHostCanary
                | ViewportPaintRendererMode::RetainedScrollSceneCanary
        ))
        .then(|| {
            root_keys_for_build.first().copied().and_then(|root| {
                (root_keys_for_build.len() == 1).then(|| {
                    let key = crate::view::base_component::root_effect_stable_key(root);
                    let desc = ctx.persistent_full_viewport_target_desc(key);
                    RootEffectBuildPlan {
                        committed: self.compositor.root_effect_retained.clone(),
                        key,
                        target: crate::view::paint::RootEffectRasterInputs {
                            width: desc.width(),
                            height: desc.height(),
                            format: desc.format(),
                            sample_count: desc.sample_count(),
                            scale_factor_bits: ctx.viewport().scale_factor().to_bits(),
                        },
                        pair_resident: self
                            .has_compatible_persistent_render_target_pair(key, &desc),
                    }
                })
            })
        })
        .flatten();
        let (build_whole_frame_legacy, mut paint_authority_trace) =
            match retained_transform_selection {
                RetainedTransformCanarySelection::Planned(plan) => {
                    let surface_ctx = crate::view::base_component::UiBuildContext::from_parts(
                        ctx.viewport(),
                        ctx.state_clone(),
                    );
                    match crate::view::paint::build_retained_surface_from_pool(
                        self,
                        &plan,
                        &mut graph,
                        surface_ctx,
                    ) {
                        Ok(outcome) => {
                            let (state, trace) = outcome.into_parts();
                            ctx.set_state(state);
                            self.stage_root_effect_clear();
                            (
                                false,
                                format!(
                                    "retained-transform-canary authority=retained-transform action={:?} boundary={:?} desc={}x{} chunks={} ops={}",
                                    trace.action,
                                    trace.boundary_root,
                                    trace.descriptor_size[0],
                                    trace.descriptor_size[1],
                                    trace.chunk_count,
                                    trace.op_count,
                                ),
                            )
                        }
                        Err(error) => {
                            self.stage_retained_surface_clear();
                            self.stage_root_effect_clear();
                            (
                                true,
                                format!(
                                    "retained-transform-canary authority=legacy prepare-rejected={error:?}"
                                ),
                            )
                        }
                    }
                }
                RetainedTransformCanarySelection::PropertyScenePrepared => {
                    let surface_ctx = crate::view::base_component::UiBuildContext::from_parts(
                        ctx.viewport(),
                        ctx.state_clone(),
                    );
                    let outcome = crate::view::paint::emit_prepared_retained_property_scene(
                        self,
                        prepared_property_scene
                            .take()
                            .expect("prepared selection owns one pre-clear property-scene token"),
                        &mut graph,
                        surface_ctx,
                    );
                    let (state, trace) = outcome.into_parts();
                    ctx.set_state(state);
                    self.stage_root_effect_clear();
                    let surfaces =
                        paint_authority_telemetry
                            .as_ref()
                            .map_or_else(String::new, |_| {
                                trace
                                    .surfaces
                                    .iter()
                                    .map(|surface| {
                                        format!(
                                            "boundary={:?},action={:?},desc={}x{},chunks={},ops={}",
                                            surface.boundary_root,
                                            surface.action,
                                            surface.descriptor_size[0],
                                            surface.descriptor_size[1],
                                            surface.chunk_count,
                                            surface.op_count,
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join("; ")
                            });
                    (
                        false,
                        format!(
                            "retained-auto authority=property-scene roots={} surfaces={} reraster={} reuse={} surface-details=[{}]",
                            trace.root_count,
                            trace.surface_count,
                            trace.reraster_count,
                            trace.reuse_count,
                            surfaces,
                        ),
                    )
                }
                RetainedTransformCanarySelection::NestedScrollScenePrepared => {
                    let outcome = pre_emitted_nested_scroll
                        .take()
                        .expect("prepared nested-scroll selection emitted under its lease");
                    let (state, trace) = outcome.into_parts();
                    ctx.set_state(state);
                    if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                        telemetry.note_property_scroll_content(&trace);
                    }
                    self.stage_root_effect_clear();
                    (false, nested_scroll_success_trace(&trace))
                }
                RetainedTransformCanarySelection::DirectScrollTransformScenePrepared => {
                    let outcome = pre_emitted_direct_scroll_transform.take().expect(
                        "prepared direct scroll-transform selection emitted under its lease",
                    );
                    let (state, trace) = outcome.into_parts();
                    ctx.set_state(state);
                    if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                        telemetry.note_property_scroll_content(&trace);
                    }
                    self.stage_root_effect_clear();
                    (
                        false,
                        format!(
                            "retained-auto authority=property-scene phase=scroll-transform topology=S->T roots={} generic-surfaces={} scroll-groups={} backing={:?} tiles={} pair-bytes={} reraster={} reuse={}",
                            trace.root_count,
                            trace.generic_surface_count,
                            trace.scroll_group_count,
                            trace.backing,
                            trace.tile_count,
                            trace.content_pair_bytes,
                            trace.reraster_count,
                            trace.reuse_count,
                        ),
                    )
                }
                RetainedTransformCanarySelection::PropertyScrollScenePrepared => {
                    let outcome = pre_emitted_property_scroll
                        .take()
                        .expect("prepared property-scroll selection emitted under its lease");
                    let (state, trace) = outcome.into_parts();
                    ctx.set_state(state);
                    if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                        telemetry.note_property_scroll_content(&trace);
                    }
                    self.stage_root_effect_clear();
                    (
                        false,
                        format!(
                            "retained-auto authority=property-scene phase=scroll roots={} scroll-groups={} backing={:?} tiles={} pair-bytes={} reraster={} reuse={}",
                            trace.root_count,
                            trace.scroll_group_count,
                            trace.backing,
                            trace.tile_count,
                            trace.content_pair_bytes,
                            trace.reraster_count,
                            trace.reuse_count,
                        ),
                    )
                }
                RetainedTransformCanarySelection::TransformScrollScenePrepared => {
                    let outcome = pre_emitted_transform_scroll
                        .take()
                        .expect("prepared transform-scroll selection emitted under its lease");
                    let (state, trace) = outcome.into_parts();
                    ctx.set_state(state);
                    if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                        telemetry.note_property_scroll_content(&trace);
                    }
                    self.stage_root_effect_clear();
                    (
                        false,
                        format!(
                            "retained-auto authority=property-scene phase=transform-scroll topology=T->S roots={} generic-surfaces={} scroll-groups={} backing={:?} tiles={} pair-bytes={} reraster={} reuse={}",
                            trace.root_count,
                            trace.generic_surface_count,
                            trace.scroll_group_count,
                            trace.backing,
                            trace.tile_count,
                            trace.content_pair_bytes,
                            trace.reraster_count,
                            trace.reuse_count,
                        ),
                    )
                }
                RetainedTransformCanarySelection::EffectScrollScenePrepared => {
                    let outcome = pre_emitted_effect_scroll
                        .take()
                        .expect("prepared effect-scroll selection emitted under its lease");
                    let (state, trace) = outcome.into_parts();
                    ctx.set_state(state);
                    if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                        telemetry.note_property_scroll_content(&trace);
                    }
                    self.stage_root_effect_clear();
                    (
                        false,
                        format!(
                            "retained-auto authority=property-scene phase=effect-scroll topology=E->S roots={} generic-surfaces={} effect-surfaces={} scroll-groups={} backing={:?} tiles={} pair-bytes={} reraster={} reuse={}",
                            trace.root_count,
                            trace.generic_surface_count,
                            trace.effect_surface_count,
                            trace.scroll_group_count,
                            trace.backing,
                            trace.tile_count,
                            trace.content_pair_bytes,
                            trace.reraster_count,
                            trace.reuse_count,
                        ),
                    )
                }
                RetainedTransformCanarySelection::TransformEffectScrollScenePrepared => {
                    let outcome = pre_emitted_transform_effect_scroll.take().expect(
                        "prepared transform-effect-scroll selection emitted under its lease",
                    );
                    let (state, trace) = outcome.into_parts();
                    ctx.set_state(state);
                    if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                        telemetry.note_property_scroll_content(&trace);
                    }
                    self.stage_root_effect_clear();
                    (
                        false,
                        format!(
                            "retained-auto authority=property-scene phase=transform-effect-scroll topology=T->E->S roots={} generic-surfaces={} effect-surfaces={} scroll-groups={} backing={:?} tiles={} pair-bytes={} reraster={} reuse={}",
                            trace.root_count,
                            trace.generic_surface_count,
                            trace.effect_surface_count,
                            trace.scroll_group_count,
                            trace.backing,
                            trace.tile_count,
                            trace.content_pair_bytes,
                            trace.reraster_count,
                            trace.reuse_count,
                        ),
                    )
                }
                RetainedTransformCanarySelection::PropertyScrollScenePrepareRejected(error) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-auto authority=legacy property-scroll-prepare-rejected={error:?}"
                        ),
                    )
                }
                RetainedTransformCanarySelection::NestedScrollScenePrepareRejected(error) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    nested_scroll_prepare_rejection_dispatch(&error)
                }
                RetainedTransformCanarySelection::DirectScrollTransformScenePrepareRejected(
                    error,
                ) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    direct_scroll_transform_prepare_rejection_dispatch(&error)
                }
                RetainedTransformCanarySelection::TransformScrollScenePrepareRejected(error) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-auto authority=legacy transform-scroll-prepare-rejected={error:?}"
                        ),
                    )
                }
                RetainedTransformCanarySelection::EffectScrollScenePrepareRejected(error) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-auto authority=legacy effect-scroll-prepare-rejected={error:?}"
                        ),
                    )
                }
                RetainedTransformCanarySelection::TransformEffectScrollScenePrepareRejected(
                    error,
                ) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    transform_effect_scroll_prepare_rejection_dispatch(&error)
                }
                RetainedTransformCanarySelection::PropertyScrollScenePlanned(_) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        "retained-auto authority=legacy property-scroll-preflight-missing"
                            .to_owned(),
                    )
                }
                RetainedTransformCanarySelection::NestedScrollScenePlanned(_) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        "retained-auto authority=legacy nested-scroll-preflight-missing".to_owned(),
                    )
                }
                RetainedTransformCanarySelection::DirectScrollTransformScenePlanned(_) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        "retained-auto authority=legacy direct-scroll-transform-preflight-missing"
                            .to_owned(),
                    )
                }
                RetainedTransformCanarySelection::TransformScrollScenePlanned(_) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        "retained-auto authority=legacy transform-scroll-preflight-missing"
                            .to_owned(),
                    )
                }
                RetainedTransformCanarySelection::EffectScrollScenePlanned(_) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        "retained-auto authority=legacy effect-scroll-preflight-missing".to_owned(),
                    )
                }
                RetainedTransformCanarySelection::TransformEffectScrollScenePlanned(_) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        "retained-auto authority=legacy transform-effect-scroll-preflight-missing"
                            .to_owned(),
                    )
                }
                RetainedTransformCanarySelection::PropertyScenePrepareRejected(error) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-auto authority=legacy property-scene-prepare-rejected={error:?}"
                        ),
                    )
                }
                RetainedTransformCanarySelection::PropertyScenePlanned(_) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        "retained-auto authority=legacy property-scene-preflight-missing"
                            .to_owned(),
                    )
                }
                RetainedTransformCanarySelection::TreePlanned(plan) => {
                    let surface_ctx = crate::view::base_component::UiBuildContext::from_parts(
                        ctx.viewport(),
                        ctx.state_clone(),
                    );
                    match crate::view::paint::build_retained_surface_tree_from_pool(
                        self,
                        &plan,
                        &mut graph,
                        surface_ctx,
                    ) {
                        Ok(outcome) => {
                            let (state, traces) = outcome.into_parts();
                            assert_eq!(
                                traces.len(),
                                2,
                                "retained effect-tree canary owns exactly two surfaces"
                            );
                            ctx.set_state(state);
                            self.stage_root_effect_clear();
                            let surfaces = traces
                                .iter()
                                .map(|trace| {
                                    format!(
                                        "boundary={:?} action={:?} desc={}x{} chunks={} ops={}",
                                        trace.boundary_root,
                                        trace.action,
                                        trace.descriptor_size[0],
                                        trace.descriptor_size[1],
                                        trace.chunk_count,
                                        trace.op_count,
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("; ");
                            (
                                false,
                                format!(
                                    "retained-surface-tree-canary authority=retained-tree surfaces=[{surfaces}]"
                                ),
                            )
                        }
                        Err(error) => {
                            self.stage_retained_surface_clear();
                            self.stage_root_effect_clear();
                            (
                                true,
                                format!(
                                    "retained-surface-tree-canary authority=legacy prepare-rejected={error:?}"
                                ),
                            )
                        }
                    }
                }
                RetainedTransformCanarySelection::IsolationPlanned(plan) => {
                    let surface_ctx = crate::view::base_component::UiBuildContext::from_parts(
                        ctx.viewport(),
                        ctx.state_clone(),
                    );
                    match crate::view::paint::build_retained_isolation_surface_from_pool(
                        self,
                        &plan,
                        &mut graph,
                        surface_ctx,
                    ) {
                        Ok(outcome) => {
                            let (state, trace) = outcome.into_parts();
                            ctx.set_state(state);
                            self.stage_root_effect_clear();
                            (
                                false,
                                format!(
                                    "retained-isolation-canary authority=retained-isolation action={:?} boundary={:?} desc={}x{} chunks={} ops={}",
                                    trace.action,
                                    trace.boundary_root,
                                    trace.descriptor_size[0],
                                    trace.descriptor_size[1],
                                    trace.chunk_count,
                                    trace.op_count,
                                ),
                            )
                        }
                        Err(error) => {
                            self.stage_retained_surface_clear();
                            self.stage_root_effect_clear();
                            (
                                true,
                                format!(
                                    "retained-isolation-canary authority=legacy prepare-rejected={error:?}"
                                ),
                            )
                        }
                    }
                }
                RetainedTransformCanarySelection::EffectTreePlanned(plan) => {
                    let surface_ctx = crate::view::base_component::UiBuildContext::from_parts(
                        ctx.viewport(),
                        ctx.state_clone(),
                    );
                    match crate::view::paint::build_retained_effect_tree_from_pool(
                        self,
                        &plan,
                        &mut graph,
                        surface_ctx,
                    ) {
                        Ok(outcome) => {
                            let (state, traces) = outcome.into_parts();
                            ctx.set_state(state);
                            self.stage_root_effect_clear();
                            let surfaces = traces
                                .iter()
                                .map(|trace| {
                                    format!(
                                        "boundary={:?} action={:?} desc={}x{} chunks={} ops={}",
                                        trace.boundary_root,
                                        trace.action,
                                        trace.descriptor_size[0],
                                        trace.descriptor_size[1],
                                        trace.chunk_count,
                                        trace.op_count,
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("; ");
                            (
                                false,
                                format!(
                                    "retained-effect-tree-canary authority=retained-effect-tree surface-count={} surfaces=[{surfaces}]",
                                    traces.len(),
                                ),
                            )
                        }
                        Err(error) => {
                            self.stage_retained_surface_clear();
                            self.stage_root_effect_clear();
                            (
                                true,
                                format!(
                                    "retained-effect-tree-canary authority=legacy prepare-rejected={error:?}"
                                ),
                            )
                        }
                    }
                }
                RetainedTransformCanarySelection::ScrollHostPlanned(plan) => {
                    let surface_ctx = crate::view::base_component::UiBuildContext::from_parts(
                        ctx.viewport(),
                        ctx.state_clone(),
                    );
                    match crate::view::paint::build_retained_scroll_host_surface_from_pool(
                        self,
                        &plan,
                        &mut graph,
                        surface_ctx,
                    ) {
                        Ok(outcome) => {
                            let (state, trace) = outcome.into_parts();
                            ctx.set_state(state);
                            self.stage_root_effect_clear();
                            (
                                false,
                                format!(
                                    "retained-scroll-host-canary authority=retained-scroll-host action={:?} boundary={:?} desc={}x{} chunks={} ops={}",
                                    trace.action,
                                    trace.boundary_root,
                                    trace.descriptor_size[0],
                                    trace.descriptor_size[1],
                                    trace.chunk_count,
                                    trace.op_count,
                                ),
                            )
                        }
                        Err(error) => {
                            self.stage_retained_surface_clear();
                            self.stage_root_effect_clear();
                            (
                                true,
                                format!(
                                    "retained-scroll-host-canary authority=legacy prepare-rejected={error:?}"
                                ),
                            )
                        }
                    }
                }
                RetainedTransformCanarySelection::ScrollSceneActive => {
                    let scene_ctx = crate::view::base_component::UiBuildContext::from_parts(
                        ctx.viewport(),
                        ctx.state_clone(),
                    );
                    match crate::view::paint::build_scroll_scene_from_pool(
                        self,
                        &arena,
                        &root_keys_for_build,
                        &mut graph,
                        scene_ctx,
                    ) {
                        Ok(outcome) => {
                            let (state, trace) = outcome.into_parts();
                            if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                                telemetry.note_scroll_content(trace);
                            }
                            ctx.set_state(state);
                            self.stage_root_effect_clear();
                            (
                                false,
                                format!(
                                    "retained-scroll-scene-canary authority=retained-scroll-scene action={:?} content={:?} desc={}x{} chunks={} ops={} pair-bytes={} tiles={} reraster={} reuse={}",
                                    trace.action,
                                    trace.content_root,
                                    trace.descriptor_size[0],
                                    trace.descriptor_size[1],
                                    trace.content_chunk_count,
                                    trace.content_op_count,
                                    trace.content_pair_bytes,
                                    trace.tile_count,
                                    trace.reraster_count,
                                    trace.reuse_count,
                                ),
                            )
                        }
                        Err(error) => {
                            if paint_authority_telemetry.is_some() {
                                dispatch_legacy_fallback_stage = Some(match &error {
                                    crate::view::paint::ScrollSceneFromLiveError::LiveSnapshotDrift
                                    | crate::view::paint::ScrollSceneFromLiveError::Plan(_) => {
                                        PaintAuthorityFallbackStage::Build
                                    }
                                    crate::view::paint::ScrollSceneFromLiveError::Prepare(_) => {
                                        PaintAuthorityFallbackStage::Prepare
                                    }
                                });
                            }
                            self.stage_retained_surface_clear();
                            self.stage_root_effect_clear();
                            (
                                true,
                                format!(
                                    "retained-scroll-scene-canary authority=legacy prepare-rejected={error:?}"
                                ),
                            )
                        }
                    }
                }
                RetainedTransformCanarySelection::NoTransform => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        "retained-transform-canary authority=legacy reason=no-transform".to_owned(),
                    )
                }
                RetainedTransformCanarySelection::SingletonShapeRejected { transform_count } => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-transform-canary authority=legacy reason=exact-singleton-required transforms={transform_count}"
                        ),
                    )
                }
                RetainedTransformCanarySelection::PlanRejected(error) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-transform-canary authority=legacy plan-rejected={:?}",
                            error.reasons
                        ),
                    )
                }
                RetainedTransformCanarySelection::TreeShapeRejected { transform_count } => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-surface-tree-canary authority=legacy reason=exact-depth-two-required transforms={transform_count}"
                        ),
                    )
                }
                RetainedTransformCanarySelection::TreePlanRejected(error) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-surface-tree-canary authority=legacy plan-rejected={:?}",
                            error.reasons
                        ),
                    )
                }
                RetainedTransformCanarySelection::IsolationPlanRejected(error) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-isolation-canary authority=legacy plan-rejected={:?}",
                            error.reasons
                        ),
                    )
                }
                RetainedTransformCanarySelection::EffectTreeShapeRejected {
                    transform_count,
                    effect_count,
                } => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-effect-tree-canary authority=legacy reason=exact-one-transform-one-effect-required transforms={transform_count} effects={effect_count}"
                        ),
                    )
                }
                RetainedTransformCanarySelection::EffectTreePlanRejected(error) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-effect-tree-canary authority=legacy plan-rejected={:?}",
                            error.reasons
                        ),
                    )
                }
                RetainedTransformCanarySelection::ScrollHostShapeRejected { scroll_count } => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-scroll-host-canary authority=legacy reason=exact-single-scroll-required scrolls={scroll_count}"
                        ),
                    )
                }
                RetainedTransformCanarySelection::ScrollHostPlanRejected(error) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-scroll-host-canary authority=legacy plan-rejected={:?}",
                            error.reasons
                        ),
                    )
                }
                RetainedTransformCanarySelection::ScrollSceneShapeRejected { scroll_count } => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-scroll-scene-canary authority=legacy reason=exact-single-scroll-required scrolls={scroll_count}"
                        ),
                    )
                }
                RetainedTransformCanarySelection::AutoArtifact(candidate) => {
                    self.stage_retained_surface_clear();
                    match try_compile_recorded_artifact_frame(
                        &mut graph,
                        candidate,
                        &ctx,
                        root_effect_plan.as_ref(),
                    ) {
                        PropertyNeutralArtifactAttempt::Compiled {
                            state,
                            eligibility,
                            root_effect_transaction,
                        } => {
                            ctx.set_state(state);
                            if let Some(transaction) = root_effect_transaction {
                                self.stage_root_effect_transaction(transaction);
                            } else {
                                self.stage_root_effect_clear();
                            }
                            (
                                false,
                                format!(
                                    "retained-auto authority=artifact chunks={} ops={}",
                                    eligibility.chunk_count, eligibility.op_count
                                ),
                            )
                        }
                        PropertyNeutralArtifactAttempt::WholeFrameLegacy { eligibility } => {
                            if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                                telemetry.note_artifact_rejection(eligibility.clone());
                                dispatch_legacy_fallback_stage =
                                    Some(PaintAuthorityFallbackStage::Selection);
                            }
                            self.stage_root_effect_clear();
                            (
                                true,
                                format!(
                                    "retained-auto authority=legacy fallback={:?}",
                                    eligibility.reasons
                                ),
                            )
                        }
                        PropertyNeutralArtifactAttempt::CompileRejected(kind) => {
                            if paint_authority_telemetry.is_some() {
                                dispatch_legacy_fallback_stage =
                                    Some(PaintAuthorityFallbackStage::Compile);
                            }
                            self.stage_root_effect_clear();
                            (
                                true,
                                format!("retained-auto authority=legacy compile-rejected={kind:?}"),
                            )
                        }
                    }
                }
                RetainedTransformCanarySelection::AutoLegacy => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    let reason = retained_auto_terminal_failure.map_or_else(
                        || "reason=selection-rejected".to_owned(),
                        |stage| format!("reason=terminal-circuit-breaker prior={stage:?}"),
                    );
                    (true, format!("retained-auto authority=legacy {reason}"))
                }
                RetainedTransformCanarySelection::Auto(_) => {
                    unreachable!("automatic decision is flattened before frame-graph mutation")
                }
                RetainedTransformCanarySelection::Inactive => {
                    // Legacy and the existing artifact canary do not own the
                    // retained-transform set for this frame.
                    self.stage_retained_surface_clear();
                    let artifact_attempt = try_build_property_neutral_artifact_frame(
                        &mut graph,
                        &arena,
                        &root_keys_for_build,
                        &self.compositor.property_trees,
                        &self.compositor.paint_generations,
                        &self.compositor.promotion_state.promoted_node_ids,
                        self.paint_renderer_mode,
                        &ctx,
                        root_effect_plan.as_ref(),
                    );
                    match artifact_attempt {
                        PropertyNeutralArtifactAttempt::Compiled {
                            state,
                            eligibility,
                            root_effect_transaction,
                        } => {
                            ctx.set_state(state);
                            if let Some(transaction) = root_effect_transaction {
                                self.stage_root_effect_transaction(transaction);
                            } else {
                                self.stage_root_effect_clear();
                            }
                            (
                                false,
                                format!(
                                    "artifact-canary chunks={} ops={}",
                                    eligibility.chunk_count, eligibility.op_count
                                ),
                            )
                        }
                        PropertyNeutralArtifactAttempt::WholeFrameLegacy { eligibility } => {
                            if self.paint_renderer_mode == ViewportPaintRendererMode::ArtifactCanary
                            {
                                if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                                    telemetry.note_artifact_rejection(eligibility.clone());
                                    dispatch_legacy_fallback_stage =
                                        Some(PaintAuthorityFallbackStage::Selection);
                                }
                            }
                            self.stage_root_effect_clear();
                            if self.paint_renderer_mode == ViewportPaintRendererMode::Legacy {
                                (true, "legacy authority=legacy".to_owned())
                            } else {
                                (true, format!("legacy fallback={:?}", eligibility.reasons))
                            }
                        }
                        PropertyNeutralArtifactAttempt::CompileRejected(kind) => {
                            if self.paint_renderer_mode == ViewportPaintRendererMode::ArtifactCanary
                            {
                                if paint_authority_telemetry.is_some() {
                                    dispatch_legacy_fallback_stage =
                                        Some(PaintAuthorityFallbackStage::Compile);
                                }
                            }
                            self.stage_root_effect_clear();
                            (true, format!("legacy compile-rejected={kind:?}"))
                        }
                    }
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
                // Peek at root id and promotion status without holding the
                // element out (avoid aliasing arena).
                let Some(root_id) = arena.get(root_key).map(|n| n.element.stable_id()) else {
                    continue;
                };
                if ctx.is_node_promoted(root_id) {
                    let requested_update = ctx
                        .promoted_update_kind(root_id)
                        .unwrap_or(PromotedLayerUpdateKind::Reraster);
                    // Try inline promotion rendering reason on Element first.
                    let (inline_reason, inline_clip_rect): (Option<&'static str>, _) = {
                        let node = arena.get(root_key).unwrap();
                        if let Some(el) = node
                            .element
                            .as_any()
                            .downcast_ref::<crate::view::base_component::Element>()
                        {
                            (
                                el.inline_promotion_rendering_reason(&arena),
                                el.absolute_clip_scissor_rect(),
                            )
                        } else {
                            (None, None)
                        }
                    };
                    if let Some(reason) = inline_reason {
                        if reason != "child-scissor-clip-inline"
                            && reason != "child-stencil-clip-inline"
                        {
                            record_debug_reuse_path(DebugReusePathRecord {
                                node_id: root_id,
                                context: DebugReusePathContext::Root,
                                requested: requested_update,
                                can_reuse: false,
                                actual: PromotedLayerUpdateKind::Reraster,
                                reason: Some(reason),
                                clip_rect: inline_clip_rect,
                            });
                            let child_ctx = crate::view::base_component::UiBuildContext::from_parts(
                                ctx.viewport(),
                                ctx.state_clone(),
                            );
                            let next_state = arena
                                .with_element_taken(root_key, |root, arena| {
                                    if let Some(element) =
                                        root.as_any_mut()
                                            .downcast_mut::<crate::view::base_component::Element>()
                                    {
                                        element.build(&mut graph, arena, child_ctx)
                                    } else {
                                        root.build(&mut graph, arena, child_ctx)
                                    }
                                })
                                .unwrap();
                            ctx.set_state(next_state);
                            continue;
                        }
                    }
                    let update_kind = requested_update;
                    let (can_reuse_subtree, composite_bounds) = {
                        let node = arena.get(root_key).unwrap();
                        let element = node.element.as_ref();
                        (
                            crate::view::viewport::scene_helpers::can_reuse_promoted_subtree(
                                element, &ctx, &arena,
                            ),
                            element.promotion_composite_bounds(),
                        )
                    };
                    let can_reuse = matches!(
                        update_kind,
                        crate::view::promotion::PromotedLayerUpdateKind::Reuse
                    ) && can_reuse_subtree;
                    let mut root_ctx = crate::view::base_component::UiBuildContext::from_parts(
                        ctx.viewport(),
                        ctx.layer_subtree_state_with_ancestor_clip(ctx.ancestor_clip_context()),
                    );
                    let layer_target = root_ctx.allocate_promoted_layer_target(
                        &mut graph,
                        root_id,
                        composite_bounds,
                    );
                    root_ctx.set_current_target(layer_target);
                    let next_state = arena
                        .with_element_taken(root_key, |root, arena| {
                            if let Some(element) =
                                root.as_any_mut()
                                    .downcast_mut::<crate::view::base_component::Element>()
                            {
                                element.build_promoted_layer(
                                    &mut graph,
                                    arena,
                                    root_ctx,
                                    update_kind,
                                    can_reuse,
                                    DebugReusePathContext::Root,
                                )
                            } else if can_reuse {
                                record_debug_reuse_path(DebugReusePathRecord {
                                    node_id: root_id,
                                    context: DebugReusePathContext::Root,
                                    requested: update_kind,
                                    can_reuse,
                                    actual: PromotedLayerUpdateKind::Reuse,
                                    reason: None,
                                    clip_rect: None,
                                });
                                root_ctx.into_state()
                            } else {
                                record_debug_reuse_path(DebugReusePathRecord {
                                    node_id: root_id,
                                    context: DebugReusePathContext::Root,
                                    requested: update_kind,
                                    can_reuse,
                                    actual: PromotedLayerUpdateKind::Reraster,
                                    reason: if matches!(update_kind, PromotedLayerUpdateKind::Reuse)
                                    {
                                        Some("reuse-blocked")
                                    } else {
                                        None
                                    },
                                    clip_rect: None,
                                });
                                graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
                                    crate::view::render_pass::clear_pass::ClearParams::new([
                                        0.0, 0.0, 0.0, 0.0,
                                    ]),
                                    crate::view::render_pass::clear_pass::ClearInput {
                                        pass_context: root_ctx.graphics_pass_context(),
                                        clear_depth_stencil: true,
                                    },
                                    crate::view::render_pass::clear_pass::ClearOutput {
                                        render_target: layer_target,
                                    },
                                ));
                                root.build(&mut graph, arena, root_ctx)
                            }
                        })
                        .unwrap();
                    ctx.merge_child_render_state(&next_state);
                    let layer_target = next_state.current_target().unwrap_or(layer_target);
                    // Composite the promoted root back into the parent target.
                    {
                        let node = arena.get(root_key).unwrap();
                        self.composite_promoted_root(
                            &mut graph,
                            &mut ctx,
                            node.element.as_ref(),
                            layer_target,
                        );
                    }
                } else {
                    let child_ctx = crate::view::base_component::UiBuildContext::from_parts(
                        ctx.viewport(),
                        ctx.state_clone(),
                    );
                    let next_state =
                        build_non_promoted_root_legacy(&mut graph, &mut arena, root_key, child_ctx);
                    ctx.set_state(next_state);
                }
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
        let reuse_records = take_debug_reuse_path();
        self.push_debug_reuse_overlay_geometry(&reuse_records);
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
        timings.build_graph_ms = build_graph_started_at.elapsed().as_secs_f64() * 1000.0;

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
                timings.compile_ms = profile.total_ms;
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

        // --- Execute ---
        let mut executed = false;
        if compiled {
            match graph.execute_profiled(self, self.debug_options.trace_render_time) {
                Ok(profile) => {
                    timings.execute_ms = profile.total_ms;
                    timings.execute_pass_count = profile.pass_count;
                    timings.execute_ordered_passes = profile.ordered_passes;
                    timings.execute_detail_ordered_passes = profile.detail_ordered;
                    executed = true;
                }
                Err(error) => eprintln!("[warn] frame graph execution failed: {error:?}"),
            }
        }
        let root_keys = self.scene.ui_root_keys.clone();
        finish_frame_dirty_lifecycle(&mut self.scene.node_arena, &root_keys, compiled, executed);
        if executed {
            self.maybe_sync_raster_cache_observation(&graph);
        } else {
            self.maybe_mark_raster_cache_observation_failed(compiled);
        }
        self.finish_root_effect_transaction(compiled && executed);
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
        if let Some(telemetry) = paint_authority_telemetry.as_mut() {
            telemetry.set_detail(paint_authority_trace);
            #[cfg(test)]
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

        // --- Complete frame ---
        // Transaction rollback and the retained-auto circuit breaker above
        // must settle before the acquired frame is either submitted or
        // discarded. A terminal compile/execute failure never submits a
        // partially recorded encoder and never presents its surface image.
        let end_frame_profile = self.complete_frame(frame_disposition(compiled, executed));
        timings.end_frame_ms = end_frame_profile.total_ms;
        timings.end_frame_submit_ms = end_frame_profile.submit_ms;
        timings.end_frame_present_ms = end_frame_profile.present_ms;
        timings.total_ms = profile_start.elapsed().as_secs_f64() * 1000.0;

        // --- Trace output ---
        if self.debug_options.trace_render_time {
            if let Some(telemetry) = paint_authority_telemetry.as_ref() {
                println!("paint-authority {}", telemetry.format_debug());
            }
            let trace_root = self.build_frame_trace_tree(&timings);
            println!("{}", format_trace_render_tree(&trace_root));
            println!(
                "{}",
                format_promotion_trace(
                    &self.compositor.promotion_state.decisions,
                    &self.compositor.promoted_layer_updates,
                    self.compositor.promotion_config.base_threshold,
                )
            );
        }
        crate::view::base_component::set_text_measure_profile_enabled(false);
        crate::view::base_component::set_layout_place_profile_enabled(false);
        if self.debug_options.trace_reuse_path {
            let mut reuse_records = reuse_records;
            println!("{}", format_reuse_path_trace(&mut reuse_records));
            println!("{}", format_style_request_trace());
            println!("{}", format_style_sample_trace());
            println!("{}", format_style_promotion_trace());
        }
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
        let semantic_now = crate::time::Instant::now();
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
        let (dt, now_seconds) = self.transition_timing();
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
        let mut promotion_dirty = false;
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
                crate::ui::ViewportAction::SetDebugTraceReusePath(on) => {
                    self.debug_options.trace_reuse_path = on;
                }
                crate::ui::ViewportAction::SetDebugGeometryOverlay(on) => {
                    self.debug_options.geometry_overlay = on;
                }
                crate::ui::ViewportAction::SetPromotionEnabled(on) => {
                    let mut cfg = self.compositor.promotion_config.clone();
                    cfg.enabled = on;
                    // Scene that previously relied on the atomic threshold
                    // swap in 01_window gets the same behavior here: a
                    // large threshold effectively disables layer promotion
                    // even though the `enabled` flag remains true in
                    // other call paths.
                    cfg.base_threshold = if on {
                        ViewportPromotionConfig::default().base_threshold
                    } else {
                        1000
                    };
                    self.set_promotion_config(cfg);
                    promotion_dirty = true;
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
        if promotion_dirty {
            self.invalidate_promoted_layer_reuse();
        }
    }

    fn begin_frame(&mut self) -> Option<BeginFrameProfile> {
        let total_started_at = Instant::now();
        // If a frame is already in progress (e.g. recursive render call),
        // return a zero-cost profile so the caller proceeds with the
        // existing encoder rather than skipping the frame entirely.
        if self.frame.frame_state.is_some() {
            return Some(BeginFrameProfile {
                total_ms: 0.0,
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
            #[cfg(not(test))]
            render_texture,
            #[cfg(test)]
            render_texture: Some(render_texture),
            #[cfg(test)]
            offscreen_texture: None,
            view,
            resolve_view,
            encoder,
            depth_view: self.gpu.depth_view.clone(),
        });
        Some(BeginFrameProfile {
            total_ms: total_started_at.elapsed().as_secs_f64() * 1000.0,
            acquire_ms,
            create_view_ms,
            create_encoder_ms,
        })
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
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

    #[cfg(all(test, not(target_arch = "wasm32")))]
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

    #[cfg(all(test, not(target_arch = "wasm32")))]
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
        let total_started_at = Instant::now();
        self.frame.frame_presented = false;
        let Some(frame) = self.frame.frame_state.take() else {
            return EndFrameProfile::default();
        };

        frame.discard_unsubmitted();

        #[cfg(not(target_arch = "wasm32"))]
        {
            // StagingBelt has no abort/reset operation. `recall()` is only
            // valid after every encoder containing its copies was submitted,
            // so abandon the belt and lazily recreate it on the next upload.
            self.gpu.upload_staging_belt = None;
        }
        #[cfg(target_arch = "wasm32")]
        crate::view::render_pass::destroy_frame_transient_buffers();

        #[cfg(test)]
        {
            self.frame.completion_counts.aborts =
                self.frame.completion_counts.aborts.saturating_add(1);
        }

        EndFrameProfile {
            total_ms: total_started_at.elapsed().as_secs_f64() * 1000.0,
            ..EndFrameProfile::default()
        }
    }

    fn submit_and_present_frame(&mut self) -> EndFrameProfile {
        let total_started_at = Instant::now();
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
        #[cfg(test)]
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
        let submit_ms = submit_started_at.elapsed().as_secs_f64() * 1000.0;

        let present_started_at = Instant::now();
        #[cfg(not(test))]
        queue.present(frame.render_texture);
        #[cfg(test)]
        if let Some(render_texture) = frame.render_texture {
            queue.present(render_texture);
            self.frame.completion_counts.presents =
                self.frame.completion_counts.presents.saturating_add(1);
        }
        let present_ms = present_started_at.elapsed().as_secs_f64() * 1000.0;
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
            total_ms: total_started_at.elapsed().as_secs_f64() * 1000.0,
            submit_ms,
            present_ms,
        }
    }

    #[cfg(test)]
    fn frame_completion_counts_for_test(&self) -> (u64, u64, u64) {
        let counts = self.frame.completion_counts;
        (counts.submits, counts.presents, counts.aborts)
    }
}

#[cfg(test)]
mod legacy_root_render_tests {
    use crate::style::{
        BoxShadow, ClipMode, Color, Layout, Length, ParsedValue, Position, PropertyId,
        ScrollDirection, Style, Transform, Translate,
    };
    use crate::view::base_component::{
        BoxModelSnapshot, BuildState, DirtyFlags, DirtyPassMask, Element, ElementTrait,
        EventTarget, Image, LayoutConstraints, LayoutPlacement, Layoutable, Renderable,
        ShadowPaintRecordingCapability, Size, TextArea, UiBuildContext,
    };
    use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
    use crate::view::frame_graph::FrameGraph;
    use crate::view::node_arena::{Node, NodeArena, NodeKey};
    use crate::view::test_support::{
        commit_child, commit_element, measure_and_place, new_test_arena,
    };
    use crate::view::viewport::ViewportPaintRendererMode;
    use crate::view::{ImageSource, image_resource};
    use rustc_hash::FxHashSet;
    use std::any::Any;
    use std::sync::Arc;

    use super::{
        AutoAuthorityDecision, AutoAuthorityKind, AutoAuthorityRejection, CachedCompiledGraph,
        FrameDisposition, PaintAuthorityFallbackStage, PaintAuthorityKind, PaintAuthorityTelemetry,
        PendingRootEffectTransaction, PropertyNeutralArtifactAttempt,
        RetainedAutoTerminalFailureStage, RetainedTransformCanarySelection, RootEffectBuildPlan,
        RootEffectRetainedState, Viewport, begin_paint_authority_telemetry_attempt,
        build_non_promoted_root_legacy, direct_scroll_transform_prepare_rejection_dispatch,
        direct_scroll_transform_prepare_rejection_fallback_stage,
        enable_paint_authority_test_capture, finish_frame_dirty_lifecycle, frame_disposition,
        nested_scroll_prepare_rejection_dispatch, nested_scroll_prepare_rejection_fallback_stage,
        nested_scroll_success_trace, paint_authority_test_capture_enabled,
        preflight_direct_scroll_transform_selection, preflight_nested_scroll_selection,
        preflight_transform_effect_scroll_selection, retained_auto_circuit_breaker_selection,
        retained_auto_terminal_fallback_stage, select_retained_auto_authority,
        select_retained_transform_canary, should_store_compile_cache,
        store_paint_authority_test_snapshot, take_paint_authority_test_snapshot,
        terminal_failure_stage, transform_effect_scroll_prepare_rejection_dispatch,
        transform_effect_scroll_prepare_rejection_fallback_stage,
        try_build_property_neutral_artifact_frame,
    };

    fn constraints() -> (LayoutConstraints, LayoutPlacement) {
        (
            LayoutConstraints {
                max_width: 320.0,
                max_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 320.0,
                available_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
        )
    }

    fn seed_empty_compile_cache(viewport: &mut Viewport) {
        let mut graph = FrameGraph::new();
        graph
            .compile()
            .expect("empty graph compiles for cache fixture");
        let topology_key = graph.topology_cache_key_for_test();
        let compiled_graph = graph
            .take_compiled_graph()
            .expect("compiled empty graph owns a topology cache payload");
        viewport.frame.compile_cache = Some(CachedCompiledGraph {
            topology_key,
            graph: compiled_graph,
        });
    }

    fn colored_element(id: u64, x: f32, color: Color) -> Element {
        let mut element = Element::new_with_id(id, x, 20.0, 80.0, 40.0);
        let mut style = Style::new();
        style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
        element.apply_style(style);
        element
    }

    fn prepared_safe_leaf() -> (NodeArena, Vec<NodeKey>) {
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(colored_element(1, 10.0, Color::rgb(230, 20, 30))),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        (arena, vec![root])
    }

    fn prepared_auto_text_area(
        scroll_y: f32,
        pending_caret_scroll: bool,
    ) -> (NodeArena, Vec<NodeKey>, NodeKey) {
        prepared_auto_text_area_with_content(
            "RetainedAuto must admit bounded internal TextArea scrolling without a ScrollNode",
            scroll_y,
            pending_caret_scroll,
        )
    }

    fn prepared_auto_text_area_with_content(
        content: &str,
        scroll_y: f32,
        pending_caret_scroll: bool,
    ) -> (NodeArena, Vec<NodeKey>, NodeKey) {
        let width = 108.0;
        let height = 28.0;
        let mut arena = new_test_arena();
        let mut text_area = TextArea::with_stable_id(0xd3_a100);
        text_area.set_text(content.to_string());
        text_area.font_size = 17.5;
        text_area.line_height = 1.3;
        let root = commit_element(&mut arena, Box::new(text_area));
        arena.with_element_taken(root, |element, _arena| {
            element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap()
                .set_self_node_key(root);
        });
        let measure = LayoutConstraints {
            max_width: width,
            max_height: height,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        };
        let place = LayoutPlacement {
            parent_x: 7.25,
            parent_y: 11.75,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: height,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        };
        measure_and_place(&mut arena, root, measure, place);
        arena.with_element_taken(root, |element, arena| {
            {
                let text_area = element.as_any_mut().downcast_mut::<TextArea>().unwrap();
                let max_y = (text_area.layout_state.content_size.height
                    - text_area.viewport_size.height)
                    .max(0.0);
                assert!(scroll_y.is_nan() || scroll_y <= max_y);
                text_area.scroll_y = scroll_y;
            }
            if scroll_y.is_finite() {
                element.place(place, arena);
            }
            element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap()
                .pending_caret_scroll = pending_caret_scroll;
        });
        let mut stack = vec![root];
        while let Some(key) = stack.pop() {
            stack.extend(arena.children_of(key));
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(root);
        (arena, vec![root], root)
    }

    fn prepared_transform_leaf() -> (NodeArena, Vec<NodeKey>) {
        let mut element = colored_element(0xc4_b001, 10.0, Color::rgb(230, 20, 30));
        let mut style = Style::new();
        style.set_transform(Transform::new([Translate::x(Length::px(6.0))]));
        element.apply_style(style);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(element));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        (arena, vec![root])
    }

    fn prepared_nested_transform_tree() -> (NodeArena, Vec<NodeKey>, NodeKey) {
        let nested_colored = |id, x, width, height, color| {
            let mut element = Element::new_with_id(id, x, 1.0, width, height);
            let mut style = Style::new();
            style.insert(
                PropertyId::Layout,
                ParsedValue::Layout(crate::style::Layout::Grid),
            );
            style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
            element.apply_style(style);
            element
        };
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(nested_colored(
                0xc5_c001,
                4.0,
                40.0,
                24.0,
                Color::rgb(20, 40, 80),
            )),
        );
        let child = commit_child(
            &mut arena,
            root,
            Box::new(nested_colored(
                0xc5_c002,
                8.0,
                18.0,
                10.0,
                Color::rgb(180, 60, 20),
            )),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                10.0, 0.0, 0.0,
            ))));
        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                20.0, 0.0, 0.0,
            ))));
        (arena, vec![root], child)
    }

    fn prepared_general_transform_scene() -> (NodeArena, Vec<NodeKey>) {
        let general_colored = |id, x, width, height, color| {
            let mut element = Element::new_with_id(id, x, 4.0, width, height);
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
            element.apply_style(style);
            element
        };
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(general_colored(
                0xc5_d001,
                4.0,
                120.0,
                120.0,
                Color::rgb(20, 40, 80),
            )),
        );
        let child = commit_child(
            &mut arena,
            root,
            Box::new(general_colored(
                0xc5_d002,
                8.0,
                70.0,
                70.0,
                Color::rgb(180, 60, 20),
            )),
        );
        let deep = commit_child(
            &mut arena,
            child,
            Box::new(general_colored(
                0xc5_d003,
                12.0,
                20.0,
                20.0,
                Color::rgb(40, 180, 20),
            )),
        );
        let sibling = commit_child(
            &mut arena,
            root,
            Box::new(general_colored(
                0xc5_d004,
                44.0,
                20.0,
                20.0,
                Color::rgb(80, 20, 180),
            )),
        );
        let second_root = commit_element(
            &mut arena,
            Box::new(general_colored(
                0xc5_d005,
                140.0,
                40.0,
                40.0,
                Color::rgb(200, 120, 20),
            )),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        measure_and_place(&mut arena, second_root, measure, place);
        for (node, x) in [
            (root, 5.0),
            (child, 7.0),
            (deep, 9.0),
            (sibling, 11.0),
            (second_root, 13.0),
        ] {
            crate::view::test_support::get_element_mut::<Element>(&arena, node)
                .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                    glam::Vec3::new(x, 0.0, 0.0),
                )));
        }
        (arena, vec![root, second_root])
    }

    fn prepared_transform_child_isolation_tree()
    -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey) {
        let mixed_element = |id, x, y, width, height, color| {
            let mut element = Element::new_with_id(id, x, y, width, height);
            let mut style = Style::new();
            style.insert(
                PropertyId::Layout,
                ParsedValue::Layout(crate::style::Layout::Grid),
            );
            style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
            element.apply_style(style);
            element
        };
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(mixed_element(
                0xd1_b100,
                0.25,
                0.25,
                40.0,
                24.0,
                Color::rgb(20, 40, 80),
            )),
        );
        let child = commit_child(
            &mut arena,
            root,
            Box::new(mixed_element(
                0xd1_b101,
                4.25,
                1.5,
                18.0,
                10.0,
                Color::rgb(180, 60, 20),
            )),
        );
        let descendant = commit_child(
            &mut arena,
            child,
            Box::new(mixed_element(
                0xd1_b102,
                5.0,
                1.75,
                1.0,
                1.0,
                Color::rgb(200, 160, 20),
            )),
        );
        let (measure, mut place) = constraints();
        place.parent_x = 0.25;
        place.parent_y = 0.25;
        measure_and_place(&mut arena, root, measure, place);
        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                100.0, 0.0, 0.0,
            ))));
        crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.5);
        (arena, vec![root], root, child, descendant)
    }

    fn prepared_nested_opacity_tree() -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey) {
        let (arena, roots, root, child, descendant) = prepared_transform_child_isolation_tree();
        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_resolved_transform_for_test(None);
        crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.5);
        crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.25);
        crate::view::test_support::get_element_mut::<Element>(&arena, descendant).set_opacity(0.75);
        (arena, roots, root, child, descendant)
    }

    fn prepared_transform_scroll_scene(
        matrix: glam::Mat4,
    ) -> (
        NodeArena,
        Vec<NodeKey>,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xe2_c300, 0.0, 0.0, 120.0, 90.0,
        ))));
        let scroll = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xe2_c301, 0.0, 0.0, 120.0, 90.0,
        ))));
        let content = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xe2_c302, 0.0, -20.0, 120.0, 240.0,
        ))));
        arena.set_parent(scroll, Some(root));
        arena.push_child(root, scroll);
        arena.set_parent(content, Some(scroll));
        arena.push_child(scroll, content);
        let mut root_style = Style::new();
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        crate::view::test_support::get_element_mut::<Element>(&arena, root).apply_style(root_style);
        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_resolved_transform_for_test(Some(matrix));
        let mut scroll_style = Style::new();
        scroll_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        scroll_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        {
            let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, scroll);
            element.apply_style(scroll_style);
            element.layout_state.content_size = Size {
                width: 120.0,
                height: 240.0,
            };
            element.set_scroll_offset((0.0, 20.0));
            element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        crate::view::test_support::get_element_mut::<Element>(&arena, content)
            .set_background_color_value(Color::rgb(24, 48, 72));
        arena
            .get_mut(content)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        arena.refresh_subtree_dirty_cache(root);
        let roots = vec![root];
        let (properties, generations) = synced_paint_state(&arena, &roots);
        (arena, roots, properties, generations)
    }

    fn prepared_transform_effect_scroll_scene() -> (
        NodeArena,
        Vec<NodeKey>,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let (mut arena, roots, _, _) = prepared_transform_scroll_scene(
            glam::Mat4::from_translation(glam::Vec3::new(3.0, 0.0, 0.0)),
        );
        let transform_root = roots[0];
        let scroll = arena.children_of(transform_root)[0];
        let mut effect = Element::new_with_id(0xe2_c3f0, 0.0, 0.0, 120.0, 90.0);
        let mut effect_style = Style::new();
        effect_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        effect.apply_style(effect_style);
        effect.set_opacity(0.5);
        let effect = arena.insert(Node::new(Box::new(effect)));
        arena.set_parent(effect, Some(transform_root));
        arena.set_children(transform_root, vec![effect]);
        arena.set_parent(scroll, Some(effect));
        arena.set_children(effect, vec![scroll]);
        arena.refresh_subtree_dirty_cache(transform_root);
        let (properties, generations) = synced_paint_state(&arena, &roots);
        (arena, roots, properties, generations)
    }

    fn prepared_exact_scroll_scene() -> (
        NodeArena,
        Vec<NodeKey>,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xe2_a300, 0.0, 0.0, 100.0, 80.0,
        ))));
        let child = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xe2_a301, 0.0, -20.0, 100.0, 300.0,
        ))));
        arena.set_parent(child, Some(root));
        arena.push_child(root, child);
        let mut style = Style::new();
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        {
            let mut root_node = arena.get_mut(root).expect("scroll root");
            let root_element = root_node
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .expect("Element scroll root");
            root_element.apply_style(style);
            root_element.layout_state.content_size = Size {
                width: 100.0,
                height: 300.0,
            };
            root_element.set_scroll_offset((0.0, 20.0));
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena
            .get_mut(child)
            .expect("scroll content")
            .element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        arena.refresh_subtree_dirty_cache(root);
        let (properties, generations) = synced_paint_state(&arena, &[root]);
        (arena, vec![root], properties, generations)
    }

    fn prepared_scroll_text_area_scene() -> (
        NodeArena,
        Vec<NodeKey>,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        prepared_scroll_text_area_scene_with(
            20.0,
            9.0,
            "RetainedAuto must admit bounded internal TextArea scrolling without a ScrollNode",
        )
    }

    fn prepared_focused_atomic_projection_scroll_text_area_scene() -> (
        NodeArena,
        Vec<NodeKey>,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        prepared_focused_atomic_projection_scroll_text_area_scene_with_preedit(None)
    }

    fn prepared_focused_atomic_projection_scroll_text_area_scene_with_preedit(
        preedit: Option<(&str, Option<(usize, usize)>)>,
    ) -> (
        NodeArena,
        Vec<NodeKey>,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let width = 108.0;
        let outer_scroll_y = 20.0;
        let content_height = 300.0;
        let content = "before projected after";
        let mut arena = new_test_arena();
        let mut text_area = TextArea::with_stable_id(0xd3_a1c3);
        text_area.set_text(content.to_string());
        text_area.font_size = 17.5;
        text_area.line_height = 1.3;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(|render| {
            render.range(7..16, |_text_area| crate::ui::RsxNode::text("projected"));
        }));
        text_area.is_focused = true;
        text_area.caret_visible = true;
        text_area.cursor_char = if preedit.is_some() { 8 } else { 7 };
        if let Some((preedit, cursor)) = preedit {
            text_area.ime_preedit = preedit.to_string();
            text_area.ime_preedit_cursor = cursor;
            text_area.children_dirty = true;
            text_area.bump_unified_ifc_source_revision();
            text_area.dirty_flags = DirtyFlags::ALL;
        }

        let text_area = commit_element(&mut arena, Box::new(text_area));
        arena.with_element_taken(text_area, |element, _arena| {
            element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap()
                .set_self_node_key(text_area);
        });
        measure_and_place(
            &mut arena,
            text_area,
            LayoutConstraints {
                max_width: width,
                max_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: width,
                available_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
        );

        let wrapper = commit_element(
            &mut arena,
            Box::new(Element::new_with_id(
                0xe2_a3c1,
                0.0,
                -outer_scroll_y,
                width,
                content_height,
            )),
        );
        let root = commit_element(
            &mut arena,
            Box::new(Element::new_with_id(0xe2_a3c0, 0.0, 0.0, width, 80.0)),
        );
        arena.set_parent(text_area, Some(wrapper));
        arena.set_children(wrapper, vec![text_area]);
        arena.set_parent(wrapper, Some(root));
        arena.set_children(root, vec![wrapper]);
        arena.with_element_taken(text_area, |element, arena| {
            element.place(
                LayoutPlacement {
                    parent_x: 0.0,
                    parent_y: -outer_scroll_y,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: width,
                    available_height: 240.0,
                    viewport_width: 320.0,
                    viewport_height: 240.0,
                    percent_base_width: Some(320.0),
                    percent_base_height: Some(240.0),
                },
                arena,
            );
        });

        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        crate::view::test_support::get_element_mut::<Element>(&arena, wrapper)
            .apply_style(wrapper_style);

        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        {
            let mut root_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, root);
            root_element.apply_style(root_style);
            root_element.layout_state.content_size = Size {
                width,
                height: content_height,
            };
            root_element.set_scroll_offset((0.0, outer_scroll_y));
            root_element.clear_local_dirty_flags(DirtyFlags::ALL);
        }

        let mut stack = vec![wrapper];
        while let Some(key) = stack.pop() {
            stack.extend(arena.children_of(key));
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(root);
        let roots = vec![root];
        let (properties, generations) = synced_paint_state(&arena, &roots);
        (arena, roots, properties, generations)
    }

    fn prepared_scroll_text_area_scene_with(
        outer_scroll_y: f32,
        local_scroll_y: f32,
        content: &str,
    ) -> (
        NodeArena,
        Vec<NodeKey>,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let (mut arena, _, text_area) =
            prepared_auto_text_area_with_content(content, local_scroll_y, false);
        let wrapper = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xe2_a311,
            0.0,
            -outer_scroll_y,
            100.0,
            300.0,
        ))));
        let root = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xe2_a310, 0.0, 0.0, 100.0, 80.0,
        ))));
        arena.set_parent(text_area, Some(wrapper));
        arena.set_children(wrapper, vec![text_area]);
        arena.set_parent(wrapper, Some(root));
        arena.set_children(root, vec![wrapper]);

        let text_area_place = LayoutPlacement {
            parent_x: 0.0,
            parent_y: -outer_scroll_y,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 108.0,
            available_height: 28.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        };
        arena.with_element_taken(text_area, |element, arena| {
            element.place(text_area_place, arena);
        });
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        crate::view::test_support::get_element_mut::<Element>(&arena, wrapper)
            .apply_style(wrapper_style);

        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        {
            let mut root_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, root);
            root_element.apply_style(root_style);
            root_element.layout_state.content_size = Size {
                width: 100.0,
                height: 300.0,
            };
            root_element.set_scroll_offset((0.0, outer_scroll_y));
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena
            .get_mut(wrapper)
            .expect("scroll content wrapper")
            .element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        let mut stack = vec![text_area];
        while let Some(key) = stack.pop() {
            stack.extend(arena.children_of(key));
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(root);
        let roots = vec![root];
        let (properties, generations) = synced_paint_state(&arena, &roots);
        (arena, roots, properties, generations)
    }

    fn update_prepared_scroll_text_area_scene(
        arena: &mut NodeArena,
        roots: &[NodeKey],
        properties: &mut PropertyTrees,
        generations: &mut PaintGenerationTracker,
        outer_scroll_y: f32,
        local_scroll_y: f32,
    ) {
        let [root] = roots else {
            panic!("C1 fixture must have one root")
        };
        let root_children = arena.children_of(*root);
        let [wrapper] = root_children.as_slice() else {
            panic!("C1 root must have one content wrapper")
        };
        let wrapper = *wrapper;
        let wrapper_children = arena.children_of(wrapper);
        let [text_area] = wrapper_children.as_slice() else {
            panic!("C1 wrapper must have one TextArea")
        };
        let text_area = *text_area;
        crate::view::test_support::get_element_mut::<Element>(arena, wrapper)
            .layout_state
            .layout_position
            .y = -outer_scroll_y;
        {
            let mut root_element =
                crate::view::test_support::get_element_mut::<Element>(arena, *root);
            root_element.set_scroll_offset((0.0, outer_scroll_y));
        }
        let text_area_place = LayoutPlacement {
            parent_x: 0.0,
            parent_y: -outer_scroll_y,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 108.0,
            available_height: 28.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        };
        arena.refresh_subtree_dirty_cache(text_area);
        arena.with_element_taken(text_area, |element, arena| {
            let text_area = element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("C1 child remains TextArea");
            text_area.scroll_y = local_scroll_y;
            element.place(text_area_place, arena);
            let text_area = element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("C1 child remains TextArea");
            text_area.pending_caret_scroll = false;
            text_area.caret_visible = false;
        });
        let mut stack = vec![*root];
        while let Some(key) = stack.pop() {
            stack.extend(arena.children_of(key));
            arena
                .get_mut(key)
                .expect("C1 fixture owner")
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(*root, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(*root);
        properties.sync(arena, roots);
        generations.sync(arena, roots, properties);
    }

    fn update_prepared_scroll_text_area_selection(
        arena: &NodeArena,
        roots: &[NodeKey],
        properties: &mut PropertyTrees,
        generations: &mut PaintGenerationTracker,
        selection: (Option<usize>, Option<usize>),
        color: Option<Color>,
    ) {
        let [root] = roots else {
            panic!("C2a fixture must have one root")
        };
        let wrapper = arena.children_of(*root)[0];
        let text_area = arena.children_of(wrapper)[0];
        let mut node = arena.get_mut(text_area).expect("C2a TextArea");
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("C2a TextArea type");
        text_area.selection_anchor_char = selection.0;
        text_area.selection_focus_char = selection.1;
        if let Some(color) = color {
            text_area.selection_background_color = color;
        }
        drop(node);
        properties.sync(arena, roots);
        generations.sync(arena, roots, properties);
    }

    fn prepared_exact_nested_scroll_scene() -> (
        NodeArena,
        Vec<NodeKey>,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let (arena, outer, _inner, _leaf, properties, generations) =
            crate::view::paint::nested_scroll_plan_fixture();
        (arena, vec![outer], properties, generations)
    }

    fn prepared_exact_multi_scroll_scene() -> (
        NodeArena,
        Vec<NodeKey>,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let mut arena = NodeArena::new();
        let mut roots = Vec::new();
        for (ordinal, offset_y) in [20.0_f32, 36.0].into_iter().enumerate() {
            let stable_base = 0xe2_b300 + u64::try_from(ordinal).unwrap() * 10;
            let root = arena.insert(Node::new(Box::new(Element::new_with_id(
                stable_base,
                0.0,
                0.0,
                100.0,
                80.0,
            ))));
            let child = arena.insert(Node::new(Box::new(Element::new_with_id(
                stable_base + 1,
                0.0,
                -offset_y,
                100.0,
                300.0,
            ))));
            arena.set_parent(child, Some(root));
            arena.push_child(root, child);
            let mut style = Style::new();
            style.insert(
                PropertyId::ScrollDirection,
                ParsedValue::ScrollDirection(ScrollDirection::Vertical),
            );
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            {
                let mut root_node = arena.get_mut(root).expect("scroll root");
                let root_element = root_node
                    .element
                    .as_any_mut()
                    .downcast_mut::<Element>()
                    .expect("Element scroll root");
                root_element.apply_style(style);
                root_element.layout_state.content_size = Size {
                    width: 100.0,
                    height: 300.0,
                };
                root_element.set_scroll_offset((0.0, offset_y));
                root_element
                    .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
            }
            arena
                .get_mut(child)
                .expect("scroll content")
                .element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
            arena.refresh_subtree_dirty_cache(root);
            roots.push(root);
        }
        let (properties, generations) = synced_paint_state(&arena, &roots);
        (arena, roots, properties, generations)
    }

    fn synced_paint_state(
        arena: &NodeArena,
        roots: &[NodeKey],
    ) -> (PropertyTrees, PaintGenerationTracker) {
        let mut properties = PropertyTrees::default();
        properties.sync(arena, roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(arena, roots, &properties);
        (properties, generations)
    }

    fn auto_decision(
        arena: &NodeArena,
        roots: &[NodeKey],
        promoted: &FxHashSet<u64>,
        ctx: &UiBuildContext,
    ) -> AutoAuthorityDecision {
        let (properties, generations) = synced_paint_state(arena, roots);
        select_retained_auto_authority(arena, roots, &properties, &generations, promoted, ctx, true)
    }

    fn telemetry_for_auto_decision(decision: AutoAuthorityDecision) -> PaintAuthorityTelemetry {
        let (selection, authority, trace) = match decision {
            AutoAuthorityDecision::NestedScrollScene { prepared, trace } => (
                RetainedTransformCanarySelection::NestedScrollScenePlanned(prepared),
                AutoAuthorityKind::PropertyScene,
                trace,
            ),
            AutoAuthorityDecision::DirectScrollTransformScene { scene, trace } => (
                RetainedTransformCanarySelection::DirectScrollTransformScenePlanned(scene),
                AutoAuthorityKind::PropertyScene,
                trace,
            ),
            AutoAuthorityDecision::PropertyScrollScene { scene, trace } => (
                RetainedTransformCanarySelection::PropertyScrollScenePlanned(scene),
                AutoAuthorityKind::PropertyScene,
                trace,
            ),
            AutoAuthorityDecision::TransformScrollScene { scene, trace } => (
                RetainedTransformCanarySelection::TransformScrollScenePlanned(scene),
                AutoAuthorityKind::PropertyScene,
                trace,
            ),
            AutoAuthorityDecision::EffectScrollScene { scene, trace } => (
                RetainedTransformCanarySelection::EffectScrollScenePlanned(scene),
                AutoAuthorityKind::PropertyScene,
                trace,
            ),
            AutoAuthorityDecision::TransformEffectScrollScene { scene, trace } => (
                RetainedTransformCanarySelection::TransformEffectScrollScenePlanned(scene),
                AutoAuthorityKind::PropertyScene,
                trace,
            ),
            AutoAuthorityDecision::PropertyScene { plan, trace } => (
                RetainedTransformCanarySelection::PropertyScenePlanned(plan),
                AutoAuthorityKind::PropertyScene,
                trace,
            ),
            AutoAuthorityDecision::Artifact { candidate, trace } => (
                RetainedTransformCanarySelection::AutoArtifact(candidate),
                AutoAuthorityKind::Artifact,
                trace,
            ),
            AutoAuthorityDecision::Legacy { trace } => (
                RetainedTransformCanarySelection::AutoLegacy,
                AutoAuthorityKind::Legacy,
                trace,
            ),
        };
        PaintAuthorityTelemetry::from_selection(
            ViewportPaintRendererMode::RetainedAuto,
            &selection,
            Some((authority, trace)),
        )
    }

    fn auto_authority_kind(decision: &AutoAuthorityDecision) -> AutoAuthorityKind {
        match decision {
            AutoAuthorityDecision::NestedScrollScene { .. } => AutoAuthorityKind::PropertyScene,
            AutoAuthorityDecision::DirectScrollTransformScene { .. } => {
                AutoAuthorityKind::PropertyScene
            }
            AutoAuthorityDecision::PropertyScrollScene { .. } => AutoAuthorityKind::PropertyScene,
            AutoAuthorityDecision::TransformScrollScene { .. } => AutoAuthorityKind::PropertyScene,
            AutoAuthorityDecision::EffectScrollScene { .. } => AutoAuthorityKind::PropertyScene,
            AutoAuthorityDecision::TransformEffectScrollScene { .. } => {
                AutoAuthorityKind::PropertyScene
            }
            AutoAuthorityDecision::PropertyScene { .. } => AutoAuthorityKind::PropertyScene,
            AutoAuthorityDecision::Artifact { .. } => AutoAuthorityKind::Artifact,
            AutoAuthorityDecision::Legacy { .. } => AutoAuthorityKind::Legacy,
        }
    }

    fn auto_authority_trace(decision: &AutoAuthorityDecision) -> &super::AutoAuthorityTrace {
        match decision {
            AutoAuthorityDecision::NestedScrollScene { trace, .. }
            | AutoAuthorityDecision::DirectScrollTransformScene { trace, .. }
            | AutoAuthorityDecision::PropertyScrollScene { trace, .. }
            | AutoAuthorityDecision::TransformScrollScene { trace, .. }
            | AutoAuthorityDecision::EffectScrollScene { trace, .. }
            | AutoAuthorityDecision::TransformEffectScrollScene { trace, .. }
            | AutoAuthorityDecision::PropertyScene { trace, .. }
            | AutoAuthorityDecision::Artifact { trace, .. }
            | AutoAuthorityDecision::Legacy { trace } => trace,
        }
    }

    #[test]
    fn retained_auto_selects_one_exact_authority_by_property_topology() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let promoted = FxHashSet::default();
        let graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();

        let (neutral_arena, neutral_roots) = prepared_safe_leaf();
        assert!(matches!(
            auto_decision(&neutral_arena, &neutral_roots, &promoted, &ctx),
            AutoAuthorityDecision::Artifact { .. }
        ));

        let (clip_arena, clip_roots) = prepared_contents_clipped_leaf();
        assert!(matches!(
            auto_decision(&clip_arena, &clip_roots, &promoted, &ctx),
            AutoAuthorityDecision::Artifact { .. }
        ));

        let (transform_arena, transform_roots) = prepared_transform_leaf();
        assert!(matches!(
            auto_decision(&transform_arena, &transform_roots, &promoted, &ctx),
            AutoAuthorityDecision::PropertyScene { .. }
        ));

        let (tree_arena, tree_roots, _) = prepared_nested_transform_tree();
        assert!(matches!(
            auto_decision(&tree_arena, &tree_roots, &promoted, &ctx),
            AutoAuthorityDecision::PropertyScene { .. }
        ));

        let (general_arena, general_roots) = prepared_general_transform_scene();
        match auto_decision(&general_arena, &general_roots, &promoted, &ctx) {
            AutoAuthorityDecision::PropertyScene { .. } => {}
            AutoAuthorityDecision::Legacy { trace } => panic!(
                "general transform scene rejected: {:?}",
                trace
                    .rejections
                    .iter()
                    .map(AutoAuthorityRejection::debug_label)
                    .collect::<Vec<_>>()
            ),
            _ => panic!("general transform scene selected the wrong authority"),
        }

        let (effect_tree_arena, effect_tree_roots, _, _, _) =
            prepared_transform_child_isolation_tree();
        assert!(matches!(
            auto_decision(&effect_tree_arena, &effect_tree_roots, &promoted, &ctx),
            AutoAuthorityDecision::PropertyScene { .. }
        ));

        let (isolation_arena, isolation_roots) = prepared_safe_leaf();
        crate::view::test_support::get_element_mut::<Element>(&isolation_arena, isolation_roots[0])
            .set_opacity(0.5);
        assert!(matches!(
            auto_decision(&isolation_arena, &isolation_roots, &promoted, &ctx),
            AutoAuthorityDecision::PropertyScene { .. }
        ));

        assert_eq!(
            graph.build_state_snapshot_for_test(),
            graph_before,
            "automatic selection cannot mutate the frame graph"
        );
    }

    #[test]
    fn retained_auto_text_area_zero_and_bounded_scroll_select_artifact_and_invalid_states_legacy() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let promoted = FxHashSet::default();

        for scroll_y in [0.0, 9.0] {
            let (arena, roots, root) = prepared_auto_text_area(scroll_y, false);
            let (properties, _) = synced_paint_state(&arena, &roots);
            assert!(properties.scrolls.is_empty());
            let state = properties.node_state_for(root).unwrap();
            assert_ne!(state.paint.clip, state.descendants.clip);
            let AutoAuthorityDecision::Artifact { candidate, trace } =
                auto_decision(&arena, &roots, &promoted, &ctx)
            else {
                panic!("bounded TextArea scroll {scroll_y} must select Artifact")
            };
            assert!(candidate.eligibility.eligible);
            assert!(trace.rejections.is_empty());
        }

        for (scroll_y, pending) in [(f32::NAN, false), (0.0, true)] {
            let (arena, roots, _) = prepared_auto_text_area(scroll_y, pending);
            let AutoAuthorityDecision::Legacy { trace } =
                auto_decision(&arena, &roots, &promoted, &ctx)
            else {
                panic!("invalid or pending TextArea scroll state must select Legacy")
            };
            assert!(matches!(
                trace.rejections.first(),
                Some(AutoAuthorityRejection::Artifact { eligibility })
                    if eligibility.reasons.contains(
                        &crate::view::paint::FrameArtifactFallbackReason::LegacyBoundary(
                            crate::view::paint::LegacyPaintReason::StatefulPaint,
                        )
                    )
            ));
        }
    }

    #[test]
    fn retained_auto_routes_nested_effects_and_reports_typed_plan_rejection_for_interleave() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let promoted = FxHashSet::default();
        let (arena, roots, _, child, _) = prepared_nested_opacity_tree();
        let decision = auto_decision(&arena, &roots, &promoted, &ctx);
        assert_eq!(
            auto_authority_kind(&decision),
            AutoAuthorityKind::PropertyScene
        );
        assert!(auto_authority_trace(&decision).rejections.is_empty());
        assert_eq!(
            telemetry_for_auto_decision(decision)
                .snapshot()
                .authority_label,
            "retained-auto:property-scene"
        );

        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                3.0, 0.0, 0.0,
            ))));
        let rejected = auto_decision(&arena, &roots, &promoted, &ctx);
        assert_eq!(auto_authority_kind(&rejected), AutoAuthorityKind::Legacy);
        let [AutoAuthorityRejection::Plan { authority, error }] =
            auto_authority_trace(&rejected).rejections.as_slice()
        else {
            panic!("rejected effect/transform interleave has one typed plan rejection")
        };
        assert_eq!(*authority, AutoAuthorityKind::PropertyScene);
        assert!(error.reasons.iter().any(|reason| matches!(
            reason,
            crate::view::paint::FramePaintPlanRejection::CoLocatedTransformEffect(_)
                | crate::view::paint::FramePaintPlanRejection::UnsupportedPropertyInterleave(_)
        )));
    }

    #[test]
    fn retained_auto_trace_capture_does_not_change_authority_decision() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);

        let (arena, roots) = prepared_transform_leaf();
        let (properties, generations) = synced_paint_state(&arena, &roots);
        let promoted = FxHashSet::default();
        let captured = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &promoted,
            &ctx,
            true,
        );
        let uncaptured = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &promoted,
            &ctx,
            false,
        );
        assert_eq!(
            auto_authority_kind(&captured),
            auto_authority_kind(&uncaptured)
        );
        assert_eq!(
            auto_authority_kind(&captured),
            AutoAuthorityKind::PropertyScene
        );
        assert!(auto_authority_trace(&uncaptured).rejections.is_empty());

        let (effect_scroll_arena, effect_scroll_roots, _, _) =
            prepared_transform_scroll_scene(glam::Mat4::IDENTITY);
        crate::view::test_support::get_element_mut::<Element>(
            &effect_scroll_arena,
            effect_scroll_roots[0],
        )
        .set_resolved_transform_for_test(None);
        crate::view::test_support::get_element_mut::<Element>(
            &effect_scroll_arena,
            effect_scroll_roots[0],
        )
        .set_opacity(0.5);
        effect_scroll_arena.refresh_subtree_dirty_cache(effect_scroll_roots[0]);
        let (effect_scroll_properties, effect_scroll_generations) =
            synced_paint_state(&effect_scroll_arena, &effect_scroll_roots);
        let captured = select_retained_auto_authority(
            &effect_scroll_arena,
            &effect_scroll_roots,
            &effect_scroll_properties,
            &effect_scroll_generations,
            &promoted,
            &ctx,
            true,
        );
        let uncaptured = select_retained_auto_authority(
            &effect_scroll_arena,
            &effect_scroll_roots,
            &effect_scroll_properties,
            &effect_scroll_generations,
            &promoted,
            &ctx,
            false,
        );
        assert!(matches!(
            &captured,
            AutoAuthorityDecision::EffectScrollScene { .. }
        ));
        assert!(matches!(
            &uncaptured,
            AutoAuthorityDecision::EffectScrollScene { .. }
        ));
        assert!(!auto_authority_trace(&captured).rejections.is_empty());
        assert!(auto_authority_trace(&uncaptured).rejections.is_empty());

        let (arena, roots) = prepared_safe_leaf();
        let (properties, generations) = synced_paint_state(&arena, &roots);
        let promoted = FxHashSet::from_iter([1]);
        let captured = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &promoted,
            &ctx,
            true,
        );
        let uncaptured = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &promoted,
            &ctx,
            false,
        );
        assert_eq!(
            auto_authority_kind(&captured),
            auto_authority_kind(&uncaptured)
        );
        assert_eq!(auto_authority_kind(&captured), AutoAuthorityKind::Legacy);
        assert!(!auto_authority_trace(&captured).rejections.is_empty());
        assert!(auto_authority_trace(&uncaptured).rejections.is_empty());
    }

    #[test]
    fn retained_auto_telemetry_labels_every_selected_authority_without_named_aliases() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let promoted = FxHashSet::default();

        let (arena, roots) = prepared_safe_leaf();
        assert_eq!(
            telemetry_for_auto_decision(auto_decision(&arena, &roots, &promoted, &ctx))
                .snapshot()
                .authority_label,
            "retained-auto:artifact"
        );

        let (arena, roots) = prepared_transform_leaf();
        assert_eq!(
            telemetry_for_auto_decision(auto_decision(&arena, &roots, &promoted, &ctx))
                .snapshot()
                .authority_label,
            "retained-auto:property-scene"
        );

        let (arena, roots, _) = prepared_nested_transform_tree();
        assert_eq!(
            telemetry_for_auto_decision(auto_decision(&arena, &roots, &promoted, &ctx))
                .snapshot()
                .authority_label,
            "retained-auto:property-scene"
        );

        let (arena, roots, _, _, _) = prepared_transform_child_isolation_tree();
        assert_eq!(
            telemetry_for_auto_decision(auto_decision(&arena, &roots, &promoted, &ctx))
                .snapshot()
                .authority_label,
            "retained-auto:property-scene"
        );

        let (arena, roots) = prepared_safe_leaf();
        crate::view::test_support::get_element_mut::<Element>(&arena, roots[0]).set_opacity(0.5);
        let isolation_telemetry =
            telemetry_for_auto_decision(auto_decision(&arena, &roots, &promoted, &ctx));
        assert_eq!(
            isolation_telemetry.snapshot().authority_label,
            "retained-auto:property-scene"
        );
        let formatted = isolation_telemetry.format_debug();
        assert!(formatted.contains("retained-auto:property-scene"));
        assert!(!formatted.contains("retained-isolation-canary"));

        let (arena, roots, properties, generations) = prepared_exact_scroll_scene();
        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &promoted,
            &ctx,
            true,
        );
        assert_eq!(
            telemetry_for_auto_decision(decision)
                .snapshot()
                .authority_label,
            "retained-auto:property-scene"
        );
    }

    #[test]
    fn paint_authority_telemetry_keeps_rejections_stages_and_scroll_costs_structured() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let (arena, roots) = prepared_safe_leaf();
        let promoted = FxHashSet::from_iter([1]);
        let mut telemetry =
            telemetry_for_auto_decision(auto_decision(&arena, &roots, &promoted, &ctx));
        telemetry.note_legacy_fallback(PaintAuthorityFallbackStage::Selection);
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.authority_label, "retained-auto:legacy");
        assert_eq!(snapshot.selected, PaintAuthorityKind::Legacy);
        assert_eq!(
            snapshot.legacy_fallback_stage,
            Some(PaintAuthorityFallbackStage::Selection)
        );
        assert!(
            snapshot
                .rejection_labels
                .iter()
                .any(|label| label.contains("PromotedBoundary"))
        );
        assert!(telemetry.format_debug().contains("PromotedBoundary"));

        let (arena, roots) = prepared_transform_leaf();
        let mut prepare_failure =
            telemetry_for_auto_decision(auto_decision(&arena, &roots, &FxHashSet::default(), &ctx));
        prepare_failure.note_legacy_fallback(PaintAuthorityFallbackStage::Prepare);
        assert_eq!(
            prepare_failure.snapshot().legacy_fallback_stage,
            Some(PaintAuthorityFallbackStage::Prepare)
        );
        let mut build_failure = prepare_failure.clone();
        build_failure.note_legacy_fallback(PaintAuthorityFallbackStage::Build);
        assert_eq!(
            build_failure.snapshot().legacy_fallback_stage,
            Some(PaintAuthorityFallbackStage::Build)
        );
        let mut compile_failure = prepare_failure.clone();
        compile_failure.note_legacy_fallback(PaintAuthorityFallbackStage::Compile);
        assert_eq!(
            compile_failure.snapshot().legacy_fallback_stage,
            Some(PaintAuthorityFallbackStage::Compile)
        );
        let mut terminal_failure = prepare_failure;
        terminal_failure.note_terminal_failure(PaintAuthorityFallbackStage::Execute);
        assert_eq!(
            terminal_failure.snapshot().terminal_failure_stage,
            Some(PaintAuthorityFallbackStage::Execute)
        );

        let mut scroll =
            telemetry_for_auto_decision(auto_decision(&arena, &roots, &FxHashSet::default(), &ctx));
        scroll.note_scroll_content(crate::view::paint::ScrollSceneBuildTrace {
            backing: crate::view::paint::ScrollSceneBackingKind::Single,
            action: crate::view::paint::RetainedSurfaceCompileAction::Reuse,
            content_root: roots[0],
            descriptor_size: [64, 128],
            content_chunk_count: 2,
            content_op_count: 3,
            content_pair_bytes: 65_536,
            tile_count: 1,
            reraster_count: 0,
            reuse_count: 1,
        });
        let single = scroll.snapshot().scroll_content.expect("single telemetry");
        assert_eq!(
            single.backing,
            crate::view::paint::ScrollSceneBackingKind::Single
        );
        assert_eq!(single.tile_count, 1);
        assert_eq!(single.pair_bytes, 65_536);
        assert_eq!(scroll.snapshot().resident_release_count, None);

        scroll.note_scroll_content(crate::view::paint::ScrollSceneBuildTrace {
            backing: crate::view::paint::ScrollSceneBackingKind::Tiled,
            action: crate::view::paint::RetainedSurfaceCompileAction::Reraster,
            content_root: roots[0],
            descriptor_size: [64, 64],
            content_chunk_count: 2,
            content_op_count: 3,
            content_pair_bytes: 131_072,
            tile_count: 3,
            reraster_count: 2,
            reuse_count: 1,
        });
        let tiled = scroll.snapshot().scroll_content.expect("tiled telemetry");
        assert_eq!(
            tiled.backing,
            crate::view::paint::ScrollSceneBackingKind::Tiled
        );
        assert_eq!(
            (tiled.tile_count, tiled.reraster_count, tiled.reuse_count),
            (3, 2, 1)
        );
        assert_eq!(tiled.pair_bytes, 131_072);
        let tiled_debug = scroll.format_debug();
        assert!(tiled_debug.contains("pair-bytes=131072"));
        assert!(tiled_debug.contains("resident-releases=unavailable"));
        assert!(!tiled_debug.contains("resident-bytes"));
    }

    #[test]
    fn paint_authority_test_capture_is_explicit_and_thread_local() {
        assert!(!paint_authority_test_capture_enabled());
        assert!(take_paint_authority_test_snapshot().is_none());

        {
            let _guard = enable_paint_authority_test_capture();
            assert!(paint_authority_test_capture_enabled());

            let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
            let (arena, roots) = prepared_transform_leaf();
            let telemetry = telemetry_for_auto_decision(auto_decision(
                &arena,
                &roots,
                &FxHashSet::default(),
                &ctx,
            ));
            store_paint_authority_test_snapshot(&telemetry);

            let snapshot = take_paint_authority_test_snapshot().expect("captured snapshot");
            assert_eq!(snapshot.selected, PaintAuthorityKind::PropertyScene);

            store_paint_authority_test_snapshot(&telemetry);
        }

        assert!(!paint_authority_test_capture_enabled());
        assert!(
            take_paint_authority_test_snapshot().is_none(),
            "dropping the capture guard must discard its last snapshot"
        );
    }

    #[test]
    fn failed_begin_frame_attempt_cannot_reuse_previous_authority_snapshot() {
        let _guard = enable_paint_authority_test_capture();
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let (arena, roots) = prepared_transform_leaf();
        let telemetry =
            telemetry_for_auto_decision(auto_decision(&arena, &roots, &FxHashSet::default(), &ctx));

        // Model a completed frame whose snapshot has not yet been consumed,
        // followed by a render attempt that returns from `begin_frame`.
        store_paint_authority_test_snapshot(&telemetry);
        begin_paint_authority_telemetry_attempt();
        assert!(take_paint_authority_test_snapshot().is_none());

        // The same failed-attempt boundary remains empty when the successful
        // frame snapshot was already consumed by the test.
        store_paint_authority_test_snapshot(&telemetry);
        assert!(take_paint_authority_test_snapshot().is_some());
        begin_paint_authority_telemetry_attempt();
        assert!(take_paint_authority_test_snapshot().is_none());
    }

    #[test]
    fn resident_release_telemetry_reports_each_frame_delta() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let (arena, roots) = prepared_transform_leaf();

        let mut first_frame =
            telemetry_for_auto_decision(auto_decision(&arena, &roots, &FxHashSet::default(), &ctx));
        first_frame.note_resident_release_delta(4, 7);

        let mut second_frame =
            telemetry_for_auto_decision(auto_decision(&arena, &roots, &FxHashSet::default(), &ctx));
        second_frame.note_resident_release_delta(7, 7);

        assert_eq!(first_frame.snapshot().resident_release_count, Some(3));
        assert_eq!(second_frame.snapshot().resident_release_count, Some(0));
    }

    #[test]
    fn retained_auto_nested_scroll_selects_and_emits_one_atomic_cold_scene() {
        let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let (arena, roots, properties, generations) = prepared_exact_nested_scroll_scene();
        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &FxHashSet::default(),
            &ctx,
            true,
        );
        let AutoAuthorityDecision::NestedScrollScene { prepared, trace } = decision else {
            panic!("exact S0->S1->leaf must select the dedicated nested authority")
        };
        assert!(prepared.is_canonical());
        assert!(trace.rejections.is_empty());

        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        let pool_before = viewport.retained_surface_transaction_shape_for_test();
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            pool_before
        );
        let (selection, outcome) = preflight_nested_scroll_selection(
            &mut viewport,
            &mut graph,
            ctx,
            [0.0, 0.0, 0.0, 1.0],
            Some(owner),
            RetainedTransformCanarySelection::NestedScrollScenePlanned(prepared),
        );
        assert!(matches!(
            selection,
            RetainedTransformCanarySelection::NestedScrollScenePrepared
        ));
        let (state, build_trace) = outcome.unwrap().into_parts();
        assert_eq!(state.opaque_rect_order(), 1);
        assert_eq!(build_trace.root_count, 1);
        assert_eq!(build_trace.generic_surface_count, 0);
        assert_eq!(build_trace.scroll_group_count, 1);
        assert_eq!(build_trace.reraster_count, 1);
        assert_eq!(build_trace.reuse_count, 0);
        assert!(nested_scroll_success_trace(&build_trace).contains("topology=S0->S1->leaf"));
        assert!(nested_scroll_success_trace(&build_trace).contains("a0=transient-keyless"));
        assert_eq!(graph.declared_persistent_texture_keys().count(), 2);
        let clears = graph.test_graphics_passes::<crate::view::frame_graph::ClearPass>();
        assert_eq!(clears.len(), 3, "root + A0 + cold R1 clears");
        let root_target = clears[0].test_snapshot().output_target;
        assert_eq!(
            clears
                .iter()
                .filter(|clear| clear.test_snapshot().output_target == root_target)
                .count(),
            1,
            "nested emit owns the root clear exactly once"
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));

        let (arena, roots, properties, generations) = prepared_exact_nested_scroll_scene();
        let telemetry = telemetry_for_auto_decision(select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &FxHashSet::default(),
            &UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            true,
        ));
        assert_eq!(
            telemetry.snapshot().authority_label,
            "retained-auto:property-scene"
        );
    }

    #[test]
    fn retained_auto_unready_image_and_nonexact_svg_nested_leafs_stay_whole_frame_legacy() {
        for kind in [
            crate::view::paint::NestedMediaLeafKind::Image,
            crate::view::paint::NestedMediaLeafKind::Svg,
        ] {
            let (arena, outer, properties, generations) =
                crate::view::paint::nested_scroll_unready_media_fixture_for_test(kind);
            let roots = [outer];
            assert_eq!(properties.scrolls.len(), 2, "{kind:?} keeps exact topology");

            let viewport = Viewport::new();
            let graph = FrameGraph::new();
            let graph_before = graph.build_state_snapshot_for_test();
            let pool_before = viewport.retained_surface_transaction_shape_for_test();
            let AutoAuthorityDecision::Legacy { trace } = select_retained_auto_authority(
                &arena,
                &roots,
                &properties,
                &generations,
                &FxHashSet::default(),
                &UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
                true,
            ) else {
                panic!("{kind:?} missing/nonexact prepared resource must remain whole-frame legacy")
            };
            assert!(matches!(
                trace.rejections.first(),
                Some(AutoAuthorityRejection::NestedScrollPlan { .. })
            ));
            assert_eq!(
                graph.build_state_snapshot_for_test(),
                graph_before,
                "{kind:?}"
            );
            assert_eq!(
                viewport.retained_surface_transaction_shape_for_test(),
                pool_before,
                "{kind:?}"
            );
            assert!(viewport.retained_property_scroll_scene_stage_is_available());
        }
    }

    #[test]
    fn retained_auto_missing_and_inline_owned_text_nested_leafs_stay_whole_frame_legacy() {
        for kind in [
            crate::view::paint::NestedTextFallbackKind::MissingPrepared,
            crate::view::paint::NestedTextFallbackKind::InlineIfcOwned,
        ] {
            let (arena, outer, properties, generations) =
                crate::view::paint::nested_scroll_unready_text_fixture_for_test(kind);
            let roots = [outer];
            assert_eq!(properties.scrolls.len(), 2, "{kind:?} keeps exact topology");

            let viewport = Viewport::new();
            let graph = FrameGraph::new();
            let graph_before = graph.build_state_snapshot_for_test();
            let pool_before = viewport.retained_surface_transaction_shape_for_test();
            let AutoAuthorityDecision::Legacy { trace } = select_retained_auto_authority(
                &arena,
                &roots,
                &properties,
                &generations,
                &FxHashSet::default(),
                &UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
                true,
            ) else {
                panic!("{kind:?} Text must remain whole-frame legacy")
            };
            assert!(matches!(
                trace.rejections.first(),
                Some(AutoAuthorityRejection::NestedScrollPlan { .. })
            ));
            assert_eq!(
                graph.build_state_snapshot_for_test(),
                graph_before,
                "{kind:?}"
            );
            assert_eq!(
                viewport.retained_surface_transaction_shape_for_test(),
                pool_before,
                "{kind:?}"
            );
            assert!(viewport.retained_property_scroll_scene_stage_is_available());
        }
    }

    #[test]
    fn retained_auto_nested_scroll_preflight_failures_are_atomic() {
        let select = || {
            let (arena, roots, properties, generations) = prepared_exact_nested_scroll_scene();
            let decision = select_retained_auto_authority(
                &arena,
                &roots,
                &properties,
                &generations,
                &FxHashSet::default(),
                &UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
                true,
            );
            let AutoAuthorityDecision::NestedScrollScene { prepared, .. } = decision else {
                panic!("exact nested fixture selects dedicated authority")
            };
            prepared
        };

        let mut stage_viewport = Viewport::new();
        let mut stage_graph = FrameGraph::new();
        let stage_graph_before = stage_graph.build_state_snapshot_for_test();
        let stage_pool_before = stage_viewport.retained_surface_transaction_shape_for_test();
        let (selection, outcome) = preflight_nested_scroll_selection(
            &mut stage_viewport,
            &mut stage_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            None,
            RetainedTransformCanarySelection::NestedScrollScenePlanned(select()),
        );
        assert!(outcome.is_none());
        assert!(matches!(
            selection,
            RetainedTransformCanarySelection::NestedScrollScenePrepareRejected(
                crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable
            )
        ));
        assert_eq!(
            stage_graph.build_state_snapshot_for_test(),
            stage_graph_before
        );
        assert_eq!(
            stage_viewport.retained_surface_transaction_shape_for_test(),
            stage_pool_before
        );

        let mut context_viewport = Viewport::new();
        let context_owner = context_viewport
            .begin_retained_surface_frame_stage()
            .unwrap();
        let mut context_graph = FrameGraph::new();
        let context_graph_before = context_graph.build_state_snapshot_for_test();
        let context_pool_before = context_viewport.retained_surface_transaction_shape_for_test();
        let mut bad_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        bad_ctx.push_scissor_rect(Some([1, 2, 3, 4]));
        let (selection, outcome) = preflight_nested_scroll_selection(
            &mut context_viewport,
            &mut context_graph,
            bad_ctx,
            [0.0; 4],
            Some(context_owner),
            RetainedTransformCanarySelection::NestedScrollScenePlanned(select()),
        );
        assert!(outcome.is_none());
        assert!(matches!(
            selection,
            RetainedTransformCanarySelection::NestedScrollScenePrepareRejected(
                crate::view::paint::RetainedPropertyScrollScenePrepareError::ContextMismatch
            )
        ));
        assert_eq!(
            context_graph.build_state_snapshot_for_test(),
            context_graph_before
        );
        assert_eq!(
            context_viewport.retained_surface_transaction_shape_for_test(),
            context_pool_before
        );
        assert!(context_viewport.retained_surface_frame_stage_owner_is_active(context_owner));
        assert!(
            context_viewport
                .finish_retained_surface_transaction_for_frame(Some(context_owner), false,)
        );

        let mut collision_viewport = Viewport::new();
        let collision_owner = collision_viewport
            .begin_retained_surface_frame_stage()
            .unwrap();
        let collision_prepared = select();
        let (collision_key, collision_desc) = collision_prepared.leaf_target_for_test();
        let mut collision_graph = FrameGraph::new();
        let mut declaring_ctx =
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let _ = declaring_ctx.allocate_persistent_target_with_desc(
            &mut collision_graph,
            collision_desc,
            collision_key,
        );
        let collision_graph_before = collision_graph.build_state_snapshot_for_test();
        let collision_pool_before =
            collision_viewport.retained_surface_transaction_shape_for_test();
        let (selection, outcome) = preflight_nested_scroll_selection(
            &mut collision_viewport,
            &mut collision_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            Some(collision_owner),
            RetainedTransformCanarySelection::NestedScrollScenePlanned(collision_prepared),
        );
        assert!(outcome.is_none());
        assert!(matches!(
            selection,
            RetainedTransformCanarySelection::NestedScrollScenePrepareRejected(
                crate::view::paint::RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(key)
            ) if key == collision_key
        ));
        assert_eq!(
            collision_graph.build_state_snapshot_for_test(),
            collision_graph_before
        );
        assert_eq!(
            collision_viewport.retained_surface_transaction_shape_for_test(),
            collision_pool_before
        );
        assert!(
            collision_viewport
                .finish_retained_surface_transaction_for_frame(Some(collision_owner), false,)
        );

        let (fallback, trace) = nested_scroll_prepare_rejection_dispatch(
            &crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable,
        );
        assert!(fallback);
        assert!(trace.contains("nested-scroll-prepare-rejected=StageUnavailable"));
        assert_eq!(
            nested_scroll_prepare_rejection_fallback_stage(),
            PaintAuthorityFallbackStage::Prepare
        );
    }

    #[test]
    fn retained_auto_exact_scroll_selects_scene_and_never_baked_host() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let promoted = FxHashSet::default();
        let (arena, roots, properties, generations) = prepared_exact_scroll_scene();
        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &promoted,
            &ctx,
            true,
        );
        let trace = match decision {
            AutoAuthorityDecision::PropertyScrollScene { trace, .. } => trace,
            AutoAuthorityDecision::Legacy { trace } => panic!(
                "exact scroll topology rejected PropertyScene: {:?}",
                trace.rejections
            ),
            _ => panic!("exact scroll topology selected a non-scroll authority"),
        };
        assert!(trace.rejections.is_empty());
        assert_eq!(properties.scrolls.len(), 1);

        let promoted = FxHashSet::from_iter([0xe2_a300]);
        let AutoAuthorityDecision::Legacy { trace } = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &promoted,
            &ctx,
            true,
        ) else {
            panic!("rejected PropertyScene scroll must fall directly to whole-frame legacy")
        };
        assert!(matches!(
            trace.rejections.as_slice(),
            [
                AutoAuthorityRejection::PropertyScrollPlan {
                    error: crate::view::paint::PropertyScrollScenePlanError::InvalidContract,
                },
                AutoAuthorityRejection::TransformScrollPlan { .. },
                AutoAuthorityRejection::EffectScrollPlan { .. },
                AutoAuthorityRejection::TransformEffectScrollPlan { .. },
                AutoAuthorityRejection::DirectScrollTransformPlan { .. }
            ]
        ));
    }

    #[test]
    fn retained_auto_scroll_text_area_subtree_selects_typed_property_scene() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let (arena, roots, properties, generations) = prepared_scroll_text_area_scene();
        assert_eq!(properties.scrolls.len(), 1);
        assert_eq!(properties.clips.len(), 2);
        let wrapper = arena.children_of(roots[0])[0];
        let text_area = arena.children_of(wrapper)[0];
        let wrapper_node = arena.get(wrapper).unwrap();
        let wrapper_element = wrapper_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap();
        assert_eq!(
            wrapper_element.children(),
            &[text_area],
            "fixture wrapper component children must mirror the arena"
        );
        let wrapper_offset = wrapper_element
            .exact_retained_scroll_content_wrapper_recording_offset([0.0, 20.0])
            .expect("fixture wrapper must satisfy the sibling oracle");
        let text_area_node = arena.get(text_area).unwrap();
        let text_area_element = text_area_node
            .element
            .as_any()
            .downcast_ref::<TextArea>()
            .unwrap();
        assert!(
            text_area_element.exact_retained_property_scroll_glyph_subtree(
                text_area,
                &arena,
                wrapper_offset,
            ),
            "fixture TextArea must satisfy the glyph-only oracle at {wrapper_offset:?}"
        );
        let root_node = arena.get(roots[0]).unwrap();
        let root_element = root_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap();
        assert!(
            root_element
                .exact_retained_scroll_text_area_subtree_admission(roots[0], &arena, 1.0)
                .is_some(),
            "fixture must satisfy the typed component admission"
        );
        let outer_clip = crate::view::compositor::property_tree::ClipNodeId {
            owner: roots[0],
            role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
        };
        let outer_scroll = crate::view::compositor::property_tree::ScrollNodeId(roots[0]);
        let text_clip = crate::view::compositor::property_tree::ClipNodeId {
            owner: text_area,
            role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
        };
        let outer_state = crate::view::compositor::property_tree::PropertyTreeState {
            clip: Some(outer_clip),
            scroll: Some(outer_scroll),
            ..Default::default()
        };
        let text_state = crate::view::compositor::property_tree::PropertyTreeState {
            clip: Some(text_clip),
            scroll: Some(outer_scroll),
            ..Default::default()
        };
        let root_state = properties.node_state_for(roots[0]).unwrap();
        assert_eq!(root_state.paint, Default::default());
        assert_eq!(root_state.descendants, outer_state);
        let wrapper_state = properties.node_state_for(wrapper).unwrap();
        assert_eq!(wrapper_state.paint, outer_state);
        assert_eq!(wrapper_state.descendants, outer_state);
        let text_area_state = properties.node_state_for(text_area).unwrap();
        assert_eq!(text_area_state.paint, outer_state);
        assert_eq!(text_area_state.descendants, text_state);
        for child in arena.children_of(text_area) {
            let child_state = properties.node_state_for(child).unwrap();
            assert_eq!(child_state.paint, text_state);
            assert_eq!(child_state.descendants, text_state);
        }

        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &FxHashSet::default(),
            &ctx,
            true,
        );
        let (scene, trace) = match decision {
            AutoAuthorityDecision::PropertyScrollScene { scene, trace } => (scene, trace),
            AutoAuthorityDecision::Legacy { trace } => panic!(
                "exact S->C->TextArea glyph subtree rejected: {:?}; root={:?} wrapper={:?} text={:?} children={:?}",
                trace.rejections,
                roots[0],
                wrapper,
                text_area,
                arena.children_of(text_area),
            ),
            _ => panic!("exact S->C->TextArea glyph subtree selected wrong authority"),
        };
        assert!(trace.rejections.is_empty());
        assert_eq!(scene.boundary_count(), 1);
        assert!(scene.is_canonical());

        let mut viewport = Viewport::new();
        let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let prepared = crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            [0.0, 0.0, 0.0, 1.0],
            frame_owner,
        )
        .expect("typed TextArea scene must prepare as one atomic forest");
        let stamps = prepared.scroll_content_stamps_for_test();
        let [stamp] = stamps.as_slice() else {
            panic!("C1 prepare must seal exactly one resident stamp")
        };
        let [local_clip] = stamp.clip_nodes.as_slice() else {
            panic!("C1 resident must retain exactly one local TextArea clip")
        };
        assert_eq!(local_clip.owner, text_area);
        assert!(local_clip.parent.is_none());
        assert!(crate::view::paint::retained_surface_raster_stamp_is_canonical(stamp));

        let mut parent_tamper = stamp.clone();
        parent_tamper.clip_nodes[0].parent = Some(outer_clip);
        assert!(!crate::view::paint::retained_surface_raster_stamp_is_canonical(&parent_tamper));
        let mut owner_tamper = stamp.clone();
        owner_tamper.clip_nodes[0].owner = roots[0];
        assert!(!crate::view::paint::retained_surface_raster_stamp_is_canonical(&owner_tamper));
        let outcome = crate::view::paint::emit_prepared_retained_property_scroll_forest(prepared);
        let (_state, trace) = outcome.into_parts();
        assert_eq!(trace.root_count, 1);
        assert_eq!(trace.scroll_group_count, 1);
        assert_eq!(
            trace.backing,
            crate::view::paint::ScrollSceneBackingKind::Single
        );
        assert_eq!(trace.tile_count, 1);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true));
    }

    #[test]
    fn retained_auto_focused_atomic_projection_text_area_selects_property_scene() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let (arena, roots, properties, generations) =
            prepared_focused_atomic_projection_scroll_text_area_scene();
        let wrapper = arena.children_of(roots[0])[0];
        let text_area = arena.children_of(wrapper)[0];
        let root_node = arena.get(roots[0]).unwrap();
        let root_element = root_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap();
        assert!(
            root_element
                .exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission(
                    roots[0], &arena, 1.0,
                )
                .is_some(),
            "focused projection fixture must satisfy C3b admission",
        );

        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &FxHashSet::default(),
            &ctx,
            true,
        );
        let (scene, trace) = match decision {
            AutoAuthorityDecision::PropertyScrollScene { scene, trace } => (scene, trace),
            AutoAuthorityDecision::Legacy { trace } => panic!(
                "focused atomic projection TextArea rejected PropertyScrollScene: {:?}",
                trace.rejections
            ),
            _ => panic!("focused atomic projection TextArea selected wrong authority"),
        };
        assert!(trace.rejections.is_empty());
        assert_eq!(scene.boundary_count(), 1);
        assert!(scene.is_canonical());

        let mut viewport = Viewport::new();
        let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        let prepared = crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            ctx,
            [0.0, 0.0, 0.0, 1.0],
            frame_owner,
        )
        .expect("focused atomic projection scene must prepare through native RetainedAuto path");
        assert_eq!(
            prepared.graph_build_state_snapshot_for_test(),
            graph_before,
            "prepare must remain graph-inert until emit",
        );
        let stamps = prepared.scroll_content_stamps_for_test();
        let [stamp] = stamps.as_slice() else {
            panic!("focused C3b prepare must seal exactly one resident stamp")
        };
        let [local_clip] = stamp.clip_nodes.as_slice() else {
            panic!("focused C3b resident must retain exactly one local TextArea clip")
        };
        assert_eq!(local_clip.owner, text_area);
        assert!(local_clip.parent.is_none());
        assert!(crate::view::paint::retained_surface_raster_stamp_is_canonical(stamp));

        let outcome = crate::view::paint::emit_prepared_retained_property_scroll_forest(prepared);
        let (state, trace) = outcome.into_parts();
        assert_eq!(state.opaque_rect_order(), 1);
        assert_eq!(
            trace.backing,
            crate::view::paint::ScrollSceneBackingKind::Single
        );
        assert_eq!(trace.tile_count, 1);
        assert_eq!(trace.reraster_count, 1);
        let pass_names = graph
            .pass_descriptors()
            .iter()
            .map(|pass| pass.name)
            .collect::<Vec<_>>();
        let composite = pass_names
            .iter()
            .position(|name| name.ends_with("TextureCompositePass"))
            .expect("resident atomic projection content must composite");
        let caret = pass_names
            .iter()
            .position(|name| name.ends_with("OpaqueRectPass"))
            .expect("visible focused atomic caret must emit dynamically");
        assert!(composite < caret, "caret must follow resident composite");
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true));
    }

    #[test]
    fn retained_auto_focused_atomic_projection_preedit_selects_property_scene() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let (arena, roots, properties, generations) =
            prepared_focused_atomic_projection_scroll_text_area_scene_with_preedit(Some((
                "中",
                Some((0, "中".len())),
            )));
        let wrapper = arena.children_of(roots[0])[0];
        let text_area = arena.children_of(wrapper)[0];
        let root_node = arena.get(roots[0]).unwrap();
        let root_element = root_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap();
        assert!(
            root_element
                .exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission(
                    roots[0], &arena, 1.0,
                )
                .is_some(),
            "focused projection preedit fixture must satisfy C3b admission",
        );

        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &FxHashSet::default(),
            &ctx,
            true,
        );
        let (scene, trace) = match decision {
            AutoAuthorityDecision::PropertyScrollScene { scene, trace } => (scene, trace),
            AutoAuthorityDecision::Legacy { trace } => panic!(
                "focused atomic projection preedit rejected PropertyScrollScene: {:?}",
                trace.rejections
            ),
            _ => panic!("focused atomic projection preedit selected wrong authority"),
        };
        assert!(trace.rejections.is_empty());
        assert_eq!(scene.boundary_count(), 1);
        assert!(scene.is_canonical());

        let mut viewport = Viewport::new();
        let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let prepared = crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            ctx,
            [0.0, 0.0, 0.0, 1.0],
            frame_owner,
        )
        .expect(
            "focused atomic projection preedit scene must prepare through native RetainedAuto path",
        );
        let stamps = prepared.scroll_content_stamps_for_test();
        let [stamp] = stamps.as_slice() else {
            panic!("focused projection preedit prepare must seal exactly one resident stamp")
        };
        let [local_clip] = stamp.clip_nodes.as_slice() else {
            panic!(
                "focused projection preedit resident must retain exactly one local TextArea clip"
            )
        };
        assert_eq!(local_clip.owner, text_area);
        assert!(local_clip.parent.is_none());
        assert!(crate::view::paint::retained_surface_raster_stamp_is_canonical(stamp));

        let outcome = crate::view::paint::emit_prepared_retained_property_scroll_forest(prepared);
        let (state, trace) = outcome.into_parts();
        assert_eq!(state.opaque_rect_order(), 2);
        assert_eq!(
            trace.backing,
            crate::view::paint::ScrollSceneBackingKind::Single
        );
        assert_eq!(trace.tile_count, 1);
        assert_eq!(trace.reraster_count, 1);
        let pass_names = graph
            .pass_descriptors()
            .iter()
            .map(|pass| pass.name)
            .collect::<Vec<_>>();
        let composite = pass_names
            .iter()
            .position(|name| name.ends_with("TextureCompositePass"))
            .expect("resident atomic projection content must composite");
        let post_rects = pass_names
            .iter()
            .enumerate()
            .filter_map(|(index, name)| name.ends_with("OpaqueRectPass").then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(
            post_rects.len(),
            2,
            "preedit underline and caret must be post-composite sidecars"
        );
        assert!(
            post_rects.into_iter().all(|index| composite < index),
            "preedit sidecars must follow resident composite",
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true));
    }

    #[test]
    fn retained_auto_scroll_text_area_subtree_interaction_and_budget_fail_closed() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let (arena, roots, _, _) = prepared_scroll_text_area_scene_with(
            0.0,
            0.0,
            "RetainedAuto must admit bounded internal TextArea scrolling without a ScrollNode",
        );
        let wrapper = arena.children_of(roots[0])[0];
        let text_area = arena.children_of(wrapper)[0];
        {
            let mut node = arena.get_mut(text_area).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.is_focused = true;
            text_area.caret_visible = true;
        }
        let (properties, generations) = synced_paint_state(&arena, &roots);
        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &FxHashSet::default(),
            &ctx,
            true,
        );
        let scene = match decision {
            AutoAuthorityDecision::PropertyScrollScene { scene, trace } => {
                assert!(trace.rejections.is_empty());
                scene
            }
            AutoAuthorityDecision::Legacy { trace } => panic!(
                "focused TextArea must reach C2b/C2c retained authority: {:?}",
                trace.rejections
            ),
            _ => panic!("focused TextArea selected the wrong retained authority"),
        };
        assert!(scene.is_canonical());
        assert!(scene.rejects_synchronized_interactive_caret_width_tamper_for_test());
        assert!(scene.rejects_synchronized_interactive_caret_position_tamper_for_test());
        assert!(scene.rejects_synchronized_interactive_caret_height_tamper_for_test());
        let mut viewport = Viewport::new();
        let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        let prepared = crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            [0.0, 0.0, 0.0, 1.0],
            frame_owner,
        )
        .expect("interactive Single backing must prepare without graph mutation");
        assert_eq!(
            prepared.graph_build_state_snapshot_for_test(),
            graph_before,
            "successful interactive prepare must remain graph-inert"
        );
        let outcome = crate::view::paint::emit_prepared_retained_property_scroll_forest(prepared);
        let (state, trace) = outcome.into_parts();
        assert_eq!(state.opaque_rect_order(), 1);
        assert_eq!(
            trace.backing,
            crate::view::paint::ScrollSceneBackingKind::Single
        );
        assert_eq!(trace.tile_count, 1);
        assert_eq!(trace.reraster_count, 1);
        assert_ne!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::render_pass::texture_composite_pass::TextureCompositePass>()
                .len(),
            1
        );
        let pass_names = graph
            .pass_descriptors()
            .iter()
            .map(|pass| pass.name)
            .collect::<Vec<_>>();
        let composite = pass_names
            .iter()
            .position(|name| name.ends_with("TextureCompositePass"))
            .expect("resident base must composite");
        let caret = pass_names
            .iter()
            .position(|name| name.ends_with("OpaqueRectPass"))
            .expect("visible opaque caret must emit dynamically");
        assert!(composite < caret, "caret must follow resident composite");
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true));

        let (arena, roots, _, _) = prepared_scroll_text_area_scene();
        let wrapper = arena.children_of(roots[0])[0];
        let text_area = arena.children_of(wrapper)[0];
        {
            let mut node = arena.get_mut(text_area).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.on_render_handler = Some(crate::ui::on_text_area_render(|render| {
                render.range(0..1, |_text_area| crate::ui::RsxNode::text("projection"))
            }));
        }
        let (properties, generations) = synced_paint_state(&arena, &roots);
        assert!(matches!(
            select_retained_auto_authority(
                &arena,
                &roots,
                &properties,
                &generations,
                &FxHashSet::default(),
                &ctx,
                true,
            ),
            AutoAuthorityDecision::Legacy { .. }
        ));

        for interactive in ["focused-selection", "focused-preedit"] {
            let (mut arena, roots, _, _) = prepared_scroll_text_area_scene();
            let wrapper = arena.children_of(roots[0])[0];
            let text_area = arena.children_of(wrapper)[0];
            {
                let mut node = arena.get_mut(text_area).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.is_focused = true;
                match interactive {
                    "focused-selection" => {
                        text_area.selection_anchor_char = Some(2);
                        text_area.selection_focus_char = Some(8);
                    }
                    "focused-preedit" => {
                        text_area.cursor_char = 2;
                        text_area.ime_preedit = "中".to_string();
                        text_area.ime_preedit_cursor = Some((0, "中".len()));
                        text_area.children_dirty = true;
                        text_area.bump_unified_ifc_source_revision();
                    }
                    _ => unreachable!(),
                }
            }
            if interactive == "focused-preedit" {
                arena.with_element_taken(text_area, |element, arena| {
                    element.measure(
                        LayoutConstraints {
                            max_width: 108.0,
                            max_height: 28.0,
                            viewport_width: 320.0,
                            viewport_height: 240.0,
                            percent_base_width: Some(320.0),
                            percent_base_height: Some(240.0),
                        },
                        arena,
                    );
                    element.place(
                        LayoutPlacement {
                            parent_x: 0.0,
                            parent_y: -20.0,
                            visual_offset_x: 0.0,
                            visual_offset_y: 0.0,
                            available_width: 108.0,
                            available_height: 28.0,
                            viewport_width: 320.0,
                            viewport_height: 240.0,
                            percent_base_width: Some(320.0),
                            percent_base_height: Some(240.0),
                        },
                        arena,
                    );
                });
                let mut stack = vec![roots[0]];
                while let Some(owner) = stack.pop() {
                    stack.extend(arena.children_of(owner));
                    arena
                        .get_mut(owner)
                        .unwrap()
                        .element
                        .clear_local_dirty_flags(DirtyFlags::ALL);
                }
                arena.clear_arena_dirty_subtree(roots[0], DirtyFlags::ALL);
                arena.refresh_subtree_dirty_cache(roots[0]);
            }
            let (properties, generations) = synced_paint_state(&arena, &roots);
            let root_node = arena.get(roots[0]).unwrap();
            let root_element = root_node
                .element
                .as_any()
                .downcast_ref::<Element>()
                .unwrap();
            assert!(
                root_element
                    .exact_retained_scroll_interactive_text_area_subtree_admission(
                        roots[0], &arena, 1.0,
                    )
                    .is_some(),
                "{interactive} fixture must satisfy component admission"
            );
            let decision = select_retained_auto_authority(
                &arena,
                &roots,
                &properties,
                &generations,
                &FxHashSet::default(),
                &ctx,
                true,
            );
            let (scene, trace) = match decision {
                AutoAuthorityDecision::PropertyScrollScene { scene, trace } => (scene, trace),
                AutoAuthorityDecision::Legacy { trace } => panic!(
                    "{interactive} must reach validated graph-inert authority: {:?}",
                    trace.rejections
                ),
                _ => panic!("{interactive} selected wrong retained authority"),
            };
            assert!(trace.rejections.is_empty());
            assert!(scene.is_canonical());
        }

        for interaction in [
            "caret",
            "pointer",
            "pending-scroll",
            "preedit",
            "preedit-selection",
        ] {
            let (arena, roots, _, _) = prepared_scroll_text_area_scene();
            let wrapper = arena.children_of(roots[0])[0];
            let text_area = arena.children_of(wrapper)[0];
            {
                let mut node = arena.get_mut(text_area).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                match interaction {
                    "caret" => text_area.caret_visible = true,
                    "pointer" => text_area.pointer_selecting = true,
                    "pending-scroll" => text_area.pending_caret_scroll = true,
                    "preedit" => text_area.ime_preedit = "中".to_string(),
                    "preedit-selection" => {
                        text_area.ime_preedit = "中".to_string();
                        text_area.selection_anchor_char = Some(2);
                        text_area.selection_focus_char = Some(8);
                    }
                    _ => unreachable!(),
                }
            }
            let (properties, generations) = synced_paint_state(&arena, &roots);
            let AutoAuthorityDecision::Legacy { trace } = select_retained_auto_authority(
                &arena,
                &roots,
                &properties,
                &generations,
                &FxHashSet::default(),
                &ctx,
                true,
            ) else {
                panic!("{interaction} TextArea must remain whole-frame Legacy")
            };
            assert!(matches!(
                trace.rejections.first(),
                Some(AutoAuthorityRejection::PropertyScrollPlan { .. })
            ));
        }

        let (arena, roots, properties, generations) = prepared_scroll_text_area_scene();
        let graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        let viewport = Viewport::new();
        let residents_before = viewport.compositor.retained_surfaces.clone();
        let budget = crate::view::paint::ScrollSceneSingleTextureBudget::new(4096, 1).unwrap();
        assert_eq!(
            crate::view::paint::plan_and_validate_property_scroll_scene(
                &arena,
                &roots,
                &FxHashSet::default(),
                &properties,
                &generations,
                1.0,
                [0.0; 2],
                None,
                crate::time::Instant::now(),
                wgpu::TextureFormat::Bgra8Unorm,
                budget,
            )
            .err(),
            Some(crate::view::paint::PropertyScrollScenePlanError::BackingBudget)
        );
        assert_eq!(
            graph.build_state_snapshot_for_test(),
            graph_before,
            "C1 budget rejection must remain graph-inert"
        );
        assert_eq!(
            viewport.compositor.retained_surfaces, residents_before,
            "C1 budget rejection must remain pool-inert"
        );
    }

    #[test]
    fn retained_auto_interactive_text_area_reuses_dynamic_caret_and_invalidates_resident_base() {
        let make_scene = |kind: &str| {
            let (outer_scroll, local_scroll) = if kind == "culled" {
                (20.0, 9.0)
            } else {
                (0.0, 0.0)
            };
            let (mut arena, roots, _, _) = prepared_scroll_text_area_scene_with(
                outer_scroll,
                local_scroll,
                "Interactive TextArea resident identity separates caret from base raster",
            );
            let wrapper = arena.children_of(roots[0])[0];
            let text_area = arena.children_of(wrapper)[0];
            {
                let mut node = arena.get_mut(text_area).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.is_focused = true;
                match kind {
                    "visible" => text_area.caret_visible = true,
                    "culled" | "outer-scrollbar" => text_area.caret_visible = true,
                    "transparent" => {
                        text_area.caret_visible = true;
                        text_area.color = Color::rgba(0, 0, 0, 0);
                        text_area.children_dirty = true;
                        text_area.bump_unified_ifc_source_revision();
                    }
                    "hidden" => text_area.caret_visible = false,
                    "cursor" => {
                        text_area.caret_visible = true;
                        text_area.cursor_char = 1;
                    }
                    "selection" => {
                        text_area.caret_visible = false;
                        text_area.selection_anchor_char = Some(0);
                        text_area.selection_focus_char = Some(2);
                    }
                    "preedit" => {
                        text_area.caret_visible = false;
                        text_area.cursor_char = 1;
                        text_area.ime_preedit = "中".to_string();
                        text_area.ime_preedit_cursor = Some((0, "中".len()));
                        text_area.children_dirty = true;
                        text_area.bump_unified_ifc_source_revision();
                    }
                    _ => unreachable!(),
                }
            }
            if kind == "outer-scrollbar" {
                crate::view::test_support::get_element_mut::<Element>(&arena, roots[0])
                    .set_sampled_scrollbar_alpha_for_test(1.0);
            }
            if matches!(kind, "preedit" | "transparent") {
                arena.with_element_taken(text_area, |element, arena| {
                    element.measure(
                        LayoutConstraints {
                            max_width: 108.0,
                            max_height: 28.0,
                            viewport_width: 320.0,
                            viewport_height: 240.0,
                            percent_base_width: Some(320.0),
                            percent_base_height: Some(240.0),
                        },
                        arena,
                    );
                    element.place(
                        LayoutPlacement {
                            parent_x: 0.0,
                            parent_y: 0.0,
                            visual_offset_x: 0.0,
                            visual_offset_y: 0.0,
                            available_width: 108.0,
                            available_height: 28.0,
                            viewport_width: 320.0,
                            viewport_height: 240.0,
                            percent_base_width: Some(320.0),
                            percent_base_height: Some(240.0),
                        },
                        arena,
                    );
                });
                let mut stack = vec![roots[0]];
                while let Some(owner) = stack.pop() {
                    stack.extend(arena.children_of(owner));
                    arena
                        .get_mut(owner)
                        .unwrap()
                        .element
                        .clear_local_dirty_flags(DirtyFlags::ALL);
                }
                arena.clear_arena_dirty_subtree(roots[0], DirtyFlags::ALL);
                arena.refresh_subtree_dirty_cache(roots[0]);
            }
            let (properties, generations) = synced_paint_state(&arena, &roots);
            match select_retained_auto_authority(
                &arena,
                &roots,
                &properties,
                &generations,
                &FxHashSet::default(),
                &UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
                true,
            ) {
                AutoAuthorityDecision::PropertyScrollScene { scene, .. } => scene,
                AutoAuthorityDecision::Legacy { trace } => {
                    panic!(
                        "interactive {kind} fixture rejected: {:?}",
                        trace.rejections
                    )
                }
                _ => panic!("interactive {kind} selected wrong authority"),
            }
        };
        let prepare_emit =
            |viewport: &mut Viewport, scene: crate::view::paint::ValidatedPropertyScrollScene| {
                let owner = viewport.begin_retained_surface_frame_stage().unwrap();
                let mut graph = FrameGraph::new();
                let mut prepared =
                    crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
                        viewport,
                        scene,
                        &mut graph,
                        UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
                        [0.0, 0.0, 0.0, 1.0],
                        owner,
                    )
                    .unwrap();
                prepared.refresh_actions_from_committed_test_pool();
                let stamps = prepared.scroll_content_stamps_for_test();
                let [stamp] = stamps.as_slice() else {
                    panic!("interactive Single backing must have one resident stamp")
                };
                let stamp = stamp.clone();
                let outcome =
                    crate::view::paint::emit_prepared_retained_property_scroll_forest(prepared);
                let (state, trace) = outcome.into_parts();
                let pass_names = graph
                    .pass_descriptors()
                    .iter()
                    .map(|pass| pass.name)
                    .collect::<Vec<_>>();
                assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
                (stamp, state.opaque_rect_order(), trace, pass_names)
            };

        let (dynamic_arena, dynamic_roots, _, _) = prepared_scroll_text_area_scene_with(
            0.0,
            0.0,
            "Interactive TextArea resident identity separates caret from base raster",
        );
        let dynamic_wrapper = dynamic_arena.children_of(dynamic_roots[0])[0];
        let dynamic_text_area = dynamic_arena.children_of(dynamic_wrapper)[0];
        let select_dynamic = |arena: &NodeArena, roots: &[NodeKey]| {
            let (properties, generations) = synced_paint_state(arena, roots);
            match select_retained_auto_authority(
                arena,
                roots,
                &properties,
                &generations,
                &FxHashSet::default(),
                &UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
                true,
            ) {
                AutoAuthorityDecision::PropertyScrollScene { scene, .. } => scene,
                AutoAuthorityDecision::Legacy { trace } => {
                    panic!(
                        "dynamic interactive fixture rejected: {:?}",
                        trace.rejections
                    )
                }
                _ => panic!("dynamic interactive fixture selected wrong authority"),
            }
        };
        {
            let mut node = dynamic_arena.get_mut(dynamic_text_area).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.is_focused = true;
            text_area.caret_visible = true;
        }
        let mut viewport = Viewport::new();
        let visible_scene = select_dynamic(&dynamic_arena, &dynamic_roots);
        assert_eq!(
            visible_scene.interactive_post_composite_opaque_delta_for_test(),
            Some(1)
        );
        let (base_stamp, visible_order, visible, visible_passes) =
            prepare_emit(&mut viewport, visible_scene);
        assert_eq!((visible.reraster_count, visible.reuse_count), (1, 0));
        assert_eq!(visible_order, 1);
        assert!(
            visible_passes
                .iter()
                .any(|name| name.ends_with("OpaqueRectPass"))
        );

        {
            let mut node = dynamic_arena.get_mut(dynamic_text_area).unwrap();
            node.element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap()
                .caret_visible = false;
        }
        let (hidden_stamp, hidden_order, hidden, hidden_passes) = prepare_emit(
            &mut viewport,
            select_dynamic(&dynamic_arena, &dynamic_roots),
        );
        assert_eq!(hidden_stamp, base_stamp);
        assert_eq!((hidden.reraster_count, hidden.reuse_count), (0, 1));
        assert_eq!(hidden_order, 0);
        assert!(
            !hidden_passes
                .iter()
                .any(|name| name.ends_with("OpaqueRectPass"))
        );

        {
            let mut node = dynamic_arena.get_mut(dynamic_text_area).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.caret_visible = true;
            text_area.cursor_char = 1;
        }
        let (cursor_stamp, cursor_order, cursor, _) = prepare_emit(
            &mut viewport,
            select_dynamic(&dynamic_arena, &dynamic_roots),
        );
        assert_eq!(cursor_stamp, base_stamp);
        assert_eq!((cursor.reraster_count, cursor.reuse_count), (0, 1));
        assert_eq!(cursor_order, 1);

        let (selection_stamp, _, selection, _) =
            prepare_emit(&mut viewport, make_scene("selection"));
        assert_ne!(
            selection_stamp.interactive_text_area_resident,
            base_stamp.interactive_text_area_resident
        );
        assert_eq!((selection.reraster_count, selection.reuse_count), (1, 0));

        let (preedit_stamp, _, preedit, _) = prepare_emit(&mut viewport, make_scene("preedit"));
        assert_ne!(
            preedit_stamp.interactive_text_area_resident,
            selection_stamp.interactive_text_area_resident
        );
        assert_eq!((preedit.reraster_count, preedit.reuse_count), (1, 0));

        let culled_scene = make_scene("culled");
        assert!(culled_scene.interactive_caret_is_culled_for_test());
        assert_eq!(
            culled_scene.interactive_post_composite_opaque_delta_for_test(),
            Some(0)
        );
        let (_, culled_order, _, culled_passes) = prepare_emit(&mut viewport, culled_scene);
        assert_eq!(culled_order, 0);
        assert!(
            !culled_passes
                .iter()
                .any(|name| name.ends_with("OpaqueRectPass"))
        );

        let transparent_scene = make_scene("transparent");
        assert_eq!(
            transparent_scene.interactive_post_composite_opaque_delta_for_test(),
            Some(0)
        );
        let (_, transparent_order, _, transparent_passes) =
            prepare_emit(&mut viewport, transparent_scene);
        assert_eq!(transparent_order, 0);
        let transparent_composite = transparent_passes
            .iter()
            .position(|name| name.ends_with("TextureCompositePass"))
            .unwrap();
        let transparent_caret = transparent_passes
            .iter()
            .rposition(|name| name.ends_with("DrawRectPass"))
            .unwrap();
        assert!(transparent_composite < transparent_caret);

        let (_, _, _, scrollbar_passes) =
            prepare_emit(&mut viewport, make_scene("outer-scrollbar"));
        let composite = scrollbar_passes
            .iter()
            .position(|name| name.ends_with("TextureCompositePass"))
            .unwrap();
        let caret = scrollbar_passes
            .iter()
            .position(|name| name.ends_with("OpaqueRectPass"))
            .unwrap();
        let overlay = scrollbar_passes
            .iter()
            .rposition(|name| name.ends_with("DrawRectPass"))
            .unwrap();
        assert!(composite < caret && caret < overlay);

        let collision_scene = make_scene("visible");
        let (collision_key, collision_desc) = collision_scene
            .first_single_backing_declaration_for_test()
            .expect("interactive content must be Single-backed");
        let mut collision_viewport = Viewport::new();
        let collision_owner = collision_viewport
            .begin_retained_surface_frame_stage()
            .unwrap();
        let mut collision_graph = FrameGraph::new();
        let mut declaring_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let _ = declaring_ctx.allocate_persistent_target_with_desc(
            &mut collision_graph,
            collision_desc,
            collision_key,
        );
        let graph_before = collision_graph.build_state_snapshot_for_test();
        let pool_before = collision_viewport.retained_surface_transaction_shape_for_test();
        let result = crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
            &mut collision_viewport,
            collision_scene,
            &mut collision_graph,
            UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            [0.0, 0.0, 0.0, 1.0],
            collision_owner,
        );
        assert_eq!(
            result.err(),
            Some(
                crate::view::paint::RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                    collision_key,
                ),
            )
        );
        assert_eq!(
            collision_graph.build_state_snapshot_for_test(),
            graph_before
        );
        assert_eq!(
            collision_viewport.retained_surface_transaction_shape_for_test(),
            pool_before
        );
        assert!(
            collision_viewport
                .finish_retained_surface_transaction_for_frame(Some(collision_owner), false,)
        );
    }

    #[test]
    fn retained_auto_scroll_text_area_normalized_identity_reuses_outer_scroll_only() {
        let select = |stage: &str,
                      arena: &NodeArena,
                      roots: &[NodeKey],
                      properties: &PropertyTrees,
                      generations: &PaintGenerationTracker| {
            let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
            match select_retained_auto_authority(
                arena,
                roots,
                properties,
                generations,
                &FxHashSet::default(),
                &ctx,
                true,
            ) {
                AutoAuthorityDecision::PropertyScrollScene { scene, .. } => scene,
                AutoAuthorityDecision::Legacy { trace } => panic!(
                    "normalized C1 {stage} fixture must select PropertyScene: {:?}",
                    trace.rejections,
                ),
                _ => panic!("normalized C1 fixture selected the wrong retained authority"),
            }
        };
        let prepare_emit =
            |viewport: &mut Viewport, scene: crate::view::paint::ValidatedPropertyScrollScene| {
                let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
                let mut graph = FrameGraph::new();
                let mut prepared =
                    crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
                        viewport,
                        scene,
                        &mut graph,
                        UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
                        [0.0, 0.0, 0.0, 1.0],
                        frame_owner,
                    )
                    .unwrap();
                prepared.refresh_actions_from_committed_test_pool();
                let stamps = prepared.scroll_content_stamps_for_test();
                let [stamp] = stamps.as_slice() else {
                    panic!("single C1 boundary must prepare one stamp")
                };
                let stamp = stamp.clone();
                let outcome =
                    crate::view::paint::emit_prepared_retained_property_scroll_forest(prepared);
                let (_state, trace) = outcome.into_parts();
                assert!(
                    viewport
                        .finish_retained_surface_transaction_for_frame(Some(frame_owner), true,)
                );
                (stamp, trace)
            };

        let content =
            "RetainedAuto must admit bounded internal TextArea scrolling without a ScrollNode";
        let (mut arena, roots, mut properties, mut generations) =
            prepared_scroll_text_area_scene_with(20.0, 9.0, content);
        let wrapper = arena.children_of(roots[0])[0];
        let text_area = arena.children_of(wrapper)[0];
        let text_area_clip = crate::view::compositor::property_tree::ClipNodeId {
            owner: text_area,
            role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
        };
        let baseline_live_clip_generation = properties
            .clip_snapshot_for(Some(text_area_clip))
            .expect("baseline TextArea live clip chain")[0]
            .generation;
        let baseline_raw_self_paint_revision = generations
            .snapshot(text_area)
            .expect("baseline TextArea paint generation")
            .self_paint_revision;
        let mut viewport = Viewport::new();
        let (baseline_stamp, baseline) = prepare_emit(
            &mut viewport,
            select("baseline", &arena, &roots, &properties, &generations),
        );
        assert_eq!((baseline.reraster_count, baseline.reuse_count), (1, 0));

        update_prepared_scroll_text_area_scene(
            &mut arena,
            &roots,
            &mut properties,
            &mut generations,
            30.0,
            9.0,
        );
        let outer_live_clip_generation = properties
            .clip_snapshot_for(Some(text_area_clip))
            .expect("outer-scroll TextArea live clip chain")[0]
            .generation;
        let outer_raw_self_paint_revision = generations
            .snapshot(text_area)
            .expect("outer-scroll TextArea paint generation")
            .self_paint_revision;
        assert_ne!(
            outer_live_clip_generation, baseline_live_clip_generation,
            "same-arena outer scroll must advance the raw live TextArea clip generation"
        );
        assert_ne!(
            outer_raw_self_paint_revision, baseline_raw_self_paint_revision,
            "same-arena outer scroll must advance the raw TextArea self-paint revision"
        );
        let (outer_scroll_stamp, outer_scroll) = prepare_emit(
            &mut viewport,
            select("outer-scroll", &arena, &roots, &properties, &generations),
        );
        assert!(
            outer_scroll_stamp == baseline_stamp,
            "outer-scroll-only motion must preserve the normalized detached-content stamp"
        );
        assert_eq!(
            (outer_scroll.reraster_count, outer_scroll.reuse_count),
            (0, 1)
        );

        update_prepared_scroll_text_area_scene(
            &mut arena,
            &roots,
            &mut properties,
            &mut generations,
            30.0,
            10.0,
        );
        let (local_scroll_stamp, local_scroll) = prepare_emit(
            &mut viewport,
            select("local-scroll", &arena, &roots, &properties, &generations),
        );
        assert!(
            local_scroll_stamp != outer_scroll_stamp,
            "local TextArea scroll must change the detached-content stamp"
        );
        assert_eq!(
            (local_scroll.reraster_count, local_scroll.reuse_count),
            (1, 0)
        );

        let (content_arena, content_roots, content_properties, content_generations) =
            prepared_scroll_text_area_scene_with(
                30.0,
                10.0,
                "RetainedAuto must admit bounded internal TextArea scrolling without a ScrollNode; changed payload must reraster while preserving the same generated owner",
            );
        let (content_stamp, changed_content) = prepare_emit(
            &mut viewport,
            select(
                "content",
                &content_arena,
                &content_roots,
                &content_properties,
                &content_generations,
            ),
        );
        assert!(
            content_stamp != local_scroll_stamp,
            "TextArea payload changes must change the detached-content stamp"
        );
        assert_eq!(
            (changed_content.reraster_count, changed_content.reuse_count),
            (1, 0)
        );
    }

    #[test]
    fn retained_auto_scroll_text_area_selection_is_exact_reusable_and_invalidating() {
        let select = |arena: &NodeArena,
                      roots: &[NodeKey],
                      properties: &PropertyTrees,
                      generations: &PaintGenerationTracker| {
            match select_retained_auto_authority(
                arena,
                roots,
                properties,
                generations,
                &FxHashSet::default(),
                &UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
                true,
            ) {
                AutoAuthorityDecision::PropertyScrollScene { scene, trace } => {
                    assert!(trace.rejections.is_empty());
                    scene
                }
                AutoAuthorityDecision::Legacy { trace } => {
                    panic!("exact C2a selection rejected: {:?}", trace.rejections)
                }
                _ => panic!("exact C2a selection chose the wrong authority"),
            }
        };
        let prepare_emit =
            |viewport: &mut Viewport, scene: crate::view::paint::ValidatedPropertyScrollScene| {
                let owner = viewport.begin_retained_surface_frame_stage().unwrap();
                let mut graph = FrameGraph::new();
                let mut prepared =
                    crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
                        viewport,
                        scene,
                        &mut graph,
                        UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
                        [0.0, 0.0, 0.0, 1.0],
                        owner,
                    )
                    .expect("C2a scene prepares atomically");
                prepared.refresh_actions_from_committed_test_pool();
                let stamps = prepared.scroll_content_stamps_for_test();
                let [stamp] = stamps.as_slice() else {
                    panic!("C2a single backing seals one stamp")
                };
                let stamp = stamp.clone();
                let outcome =
                    crate::view::paint::emit_prepared_retained_property_scroll_forest(prepared);
                let (_state, trace) = outcome.into_parts();
                assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
                (stamp, trace)
            };

        let (mut arena, roots, mut properties, mut generations) = prepared_scroll_text_area_scene();
        update_prepared_scroll_text_area_selection(
            &arena,
            &roots,
            &mut properties,
            &mut generations,
            (Some(2), Some(18)),
            None,
        );
        let wrapper = arena.children_of(roots[0])[0];
        let text_area = arena.children_of(wrapper)[0];
        let clip_id = crate::view::compositor::property_tree::ClipNodeId {
            owner: text_area,
            role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
        };
        let baseline_clip_generation = properties
            .clip_snapshot_for(Some(clip_id))
            .expect("C2a clip chain")[0]
            .generation;
        let baseline_self_revision = generations.snapshot(text_area).unwrap().self_paint_revision;
        let mut viewport = Viewport::new();
        let (baseline_stamp, baseline) = prepare_emit(
            &mut viewport,
            select(&arena, &roots, &properties, &generations),
        );
        assert_eq!((baseline.reraster_count, baseline.reuse_count), (1, 0));
        assert!(matches!(
            baseline_stamp.text_area_paint_grammar,
            Some(crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                start_char: 2,
                end_char: 18,
                ..
            })
        ));
        assert_eq!(
            baseline_stamp
                .chunks
                .iter()
                .map(|chunk| chunk.id.role)
                .collect::<Vec<_>>(),
            vec![
                crate::view::paint::PaintChunkRole::SelfDecoration,
                crate::view::paint::PaintChunkRole::SelectionUnderlay,
                crate::view::paint::PaintChunkRole::TextGlyphs,
            ]
        );
        assert!(crate::view::paint::retained_surface_raster_stamp_is_canonical(&baseline_stamp));

        let pool_before_tamper = viewport.compositor.retained_surfaces.clone();
        let mut role = baseline_stamp.clone();
        role.chunks[1].id.role = crate::view::paint::PaintChunkRole::TextDecoration;
        let mut slot = baseline_stamp.clone();
        slot.chunks[1].id.slot = 1;
        let mut order = baseline_stamp.clone();
        order.chunks.swap(1, 2);
        let mut op_count = baseline_stamp.clone();
        op_count.chunks[1].op_count = 0;
        let mut payload = baseline_stamp.clone();
        payload.chunks[1].payload_identity = Default::default();
        let mut grammar_legal_range = baseline_stamp.clone();
        let Some(
            crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                start_char,
                end_char,
                ..
            },
        ) = grammar_legal_range.text_area_paint_grammar.as_mut()
        else {
            unreachable!()
        };
        *start_char = 3;
        *end_char = 17;
        let legal_range_grammar = grammar_legal_range.text_area_paint_grammar.unwrap();
        let [crate::view::paint::RetainedSurfaceRasterStepStamp::ArtifactSpan(artifact_span)] =
            baseline_stamp.ordered_steps.as_slice()
        else {
            unreachable!()
        };
        assert!(
            crate::view::paint::validated_scroll_text_area_content_raster_stamp(
                baseline_stamp.identity.boundary_root,
                baseline_stamp.identity.stable_id,
                baseline_stamp.target.clone(),
                artifact_span.clone(),
                baseline_stamp.opaque_order_span.clone(),
                legal_range_grammar,
            )
            .is_none(),
            "the constructor seam must reject a legal range that does not match the sealed payload"
        );
        let mut grammar_range = baseline_stamp.clone();
        let Some(
            crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                start_char,
                end_char,
                ..
            },
        ) = grammar_range.text_area_paint_grammar.as_mut()
        else {
            unreachable!()
        };
        *start_char = *end_char;
        let mut grammar_kind = baseline_stamp.clone();
        grammar_kind.text_area_paint_grammar =
            Some(crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly);
        let mut grammar_nan = baseline_stamp.clone();
        let Some(
            crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                color_rgba_bits,
                ..
            },
        ) = grammar_nan.text_area_paint_grammar.as_mut()
        else {
            unreachable!()
        };
        color_rgba_bits[0] = f32::NAN.to_bits();
        let mut grammar_out_of_range = baseline_stamp.clone();
        let Some(
            crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                color_rgba_bits,
                ..
            },
        ) = grammar_out_of_range.text_area_paint_grammar.as_mut()
        else {
            unreachable!()
        };
        color_rgba_bits[3] = 1.5_f32.to_bits();
        for tampered in [
            role,
            slot,
            order,
            op_count,
            payload,
            grammar_legal_range,
            grammar_range,
            grammar_kind,
            grammar_nan,
            grammar_out_of_range,
        ] {
            assert!(!crate::view::paint::retained_surface_raster_stamp_is_canonical(&tampered));
        }
        assert_eq!(viewport.compositor.retained_surfaces, pool_before_tamper);

        update_prepared_scroll_text_area_scene(
            &mut arena,
            &roots,
            &mut properties,
            &mut generations,
            30.0,
            9.0,
        );
        assert_ne!(
            properties
                .clip_snapshot_for(Some(clip_id))
                .expect("moved C2a clip chain")[0]
                .generation,
            baseline_clip_generation
        );
        assert_ne!(
            generations.snapshot(text_area).unwrap().self_paint_revision,
            baseline_self_revision
        );
        let (outer_stamp, outer) = prepare_emit(
            &mut viewport,
            select(&arena, &roots, &properties, &generations),
        );
        assert!(
            outer_stamp == baseline_stamp,
            "outer-only raw generation drift must preserve the retained raster stamp"
        );
        assert_eq!((outer.reraster_count, outer.reuse_count), (0, 1));

        update_prepared_scroll_text_area_selection(
            &arena,
            &roots,
            &mut properties,
            &mut generations,
            (Some(5), Some(24)),
            None,
        );
        let (range_stamp, range) = prepare_emit(
            &mut viewport,
            select(&arena, &roots, &properties, &generations),
        );
        assert!(
            range_stamp != outer_stamp,
            "selection-range drift must invalidate the retained raster stamp"
        );
        assert_eq!((range.reraster_count, range.reuse_count), (1, 0));

        update_prepared_scroll_text_area_selection(
            &arena,
            &roots,
            &mut properties,
            &mut generations,
            (Some(5), Some(24)),
            Some(Color::rgba(12, 34, 56, 128)),
        );
        let (color_stamp, color) = prepare_emit(
            &mut viewport,
            select(&arena, &roots, &properties, &generations),
        );
        assert!(
            color_stamp != range_stamp,
            "selection-color drift must invalidate the retained raster stamp"
        );
        assert_eq!((color.reraster_count, color.reuse_count), (1, 0));

        update_prepared_scroll_text_area_scene(
            &mut arena,
            &roots,
            &mut properties,
            &mut generations,
            30.0,
            12.0,
        );
        let (local_scroll_stamp, local_scroll) = prepare_emit(
            &mut viewport,
            select(&arena, &roots, &properties, &generations),
        );
        assert!(matches!(
            local_scroll_stamp.text_area_paint_grammar,
            Some(
                crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                    start_char: 5,
                    end_char: 24,
                    ..
                }
            )
        ));
        assert!(
            local_scroll_stamp != color_stamp,
            "C2a local TextArea scroll must invalidate the selection resident stamp"
        );
        assert_eq!(
            (local_scroll.reraster_count, local_scroll.reuse_count),
            (1, 0)
        );
    }

    #[test]
    fn retained_auto_scroll_text_area_selection_noncanonical_states_fail_closed() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        let viewport = Viewport::new();
        let pool_before = viewport.compositor.retained_surfaces.clone();
        for (selection, label) in [
            ((Some(3), Some(3)), "collapsed"),
            ((Some(3), None), "missing focus"),
            ((None, Some(3)), "missing anchor"),
            ((Some(0), Some(usize::MAX)), "out-of-range endpoint"),
        ] {
            let (arena, roots, mut properties, mut generations) = prepared_scroll_text_area_scene();
            update_prepared_scroll_text_area_selection(
                &arena,
                &roots,
                &mut properties,
                &mut generations,
                selection,
                None,
            );
            let AutoAuthorityDecision::Legacy { trace } = select_retained_auto_authority(
                &arena,
                &roots,
                &properties,
                &generations,
                &FxHashSet::default(),
                &ctx,
                true,
            ) else {
                panic!("noncanonical C2a {label} selection must stay Legacy")
            };
            assert!(matches!(
                trace.rejections.first(),
                Some(AutoAuthorityRejection::PropertyScrollPlan { .. })
            ));
        }
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(viewport.compositor.retained_surfaces, pool_before);
    }

    #[test]
    fn retained_auto_scroll_text_area_forest_rejects_nonexact_owner_sets_and_stable_ids() {
        let plan = |arena: &NodeArena,
                    roots: &[NodeKey],
                    properties: &PropertyTrees,
                    generations: &PaintGenerationTracker| {
            crate::view::paint::plan_and_validate_property_scroll_scene(
                arena,
                roots,
                &FxHashSet::default(),
                properties,
                generations,
                1.0,
                [0.0; 2],
                None,
                crate::time::Instant::now(),
                wgpu::TextureFormat::Bgra8Unorm,
                crate::view::paint::ScrollSceneSingleTextureBudget::new(4096, 128 * 1024 * 1024)
                    .unwrap(),
            )
        };

        let (mut arena, roots, mut properties, generations) = prepared_scroll_text_area_scene();
        let extra = arena.insert(Node::new(Box::new(Element::new_with_id(
            0xe2_aff0, 0.0, 0.0, 1.0, 1.0,
        ))));
        let extra_state = properties
            .states
            .get(&roots[0])
            .expect("root property state")
            .clone();
        properties.states.insert(extra, extra_state);
        assert!(
            plan(&arena, &roots, &properties, &generations).is_err(),
            "an extra unreachable property-state key must fail closed"
        );

        let (arena, roots, mut properties, generations) = prepared_scroll_text_area_scene();
        let wrapper = arena.children_of(roots[0])[0];
        let text_area = arena.children_of(wrapper)[0];
        let generated = arena.children_of(text_area)[0];
        properties.states.remove(&generated);
        assert!(
            plan(&arena, &roots, &properties, &generations).is_err(),
            "a missing generated-child property-state key must fail closed"
        );

        let (arena, roots, _, _) = prepared_scroll_text_area_scene();
        let wrapper = arena.children_of(roots[0])[0];
        let text_area = arena.children_of(wrapper)[0];
        let wrapper_stable_id = arena.get(wrapper).expect("wrapper").element.stable_id();
        arena
            .get_mut(text_area)
            .expect("TextArea")
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea type")
            .node_id = wrapper_stable_id;
        let (properties, generations) = synced_paint_state(&arena, &roots);
        assert!(
            plan(&arena, &roots, &properties, &generations).is_err(),
            "TextArea/wrapper stable-id collision must fail closed"
        );

        let (arena, roots, _, _) = prepared_scroll_text_area_scene();
        let wrapper = arena.children_of(roots[0])[0];
        let text_area = arena.children_of(wrapper)[0];
        let generated = arena.children_of(text_area)[0];
        let text_area_stable_id = arena.get(text_area).expect("TextArea").element.stable_id();
        let mut generated_node = arena.get_mut(generated).expect("generated TextArea child");
        if let Some(run) = generated_node
            .element
            .as_any_mut()
            .downcast_mut::<crate::view::base_component::text_area::TextAreaTextRun>(
        ) {
            run.node_id = text_area_stable_id;
        } else if let Some(line_break) = generated_node
            .element
            .as_any_mut()
            .downcast_mut::<crate::view::base_component::text_area::TextAreaLineBreak>(
        ) {
            line_break.node_id = text_area_stable_id;
        } else {
            panic!("C1 fixture generated child must be run/line-break")
        }
        drop(generated_node);
        let (properties, generations) = synced_paint_state(&arena, &roots);
        assert!(
            plan(&arena, &roots, &properties, &generations).is_err(),
            "TextArea/generated-child stable-id collision must fail closed"
        );
    }

    #[test]
    fn retained_auto_transform_scroll_selects_and_emits_one_atomic_scene() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let (arena, roots, properties, generations) = prepared_transform_scroll_scene(
            glam::Mat4::from_translation(glam::Vec3::new(7.0, 5.0, 0.0)),
        );
        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &FxHashSet::default(),
            &ctx,
            true,
        );
        let AutoAuthorityDecision::TransformScrollScene { scene, trace } = decision else {
            panic!("exact T->S must select the transform-scroll property scene")
        };
        assert!(scene.is_canonical());
        assert!(matches!(
            trace.rejections.as_slice(),
            [AutoAuthorityRejection::PropertyScrollPlan { .. }]
        ));

        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let prepared = crate::view::paint::prepare_retained_transform_scroll_scene_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            ctx,
            [0.0, 0.0, 0.0, 1.0],
            owner,
        )
        .unwrap();
        let outcome = crate::view::paint::emit_prepared_retained_transform_scroll_scene(prepared);
        let (_, trace) = outcome.into_parts();
        assert_eq!(trace.root_count, 1);
        assert_eq!(trace.generic_surface_count, 1);
        assert_eq!(trace.scroll_group_count, 1);
        assert_eq!(trace.reraster_count, 2);
        assert_eq!(trace.reuse_count, 0);
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .len(),
            3,
            "one root clear plus receiver/content reraster clears"
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    }

    #[test]
    fn retained_auto_effect_scroll_selects_and_emits_one_atomic_scene() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let (arena, roots, _, _) = prepared_transform_scroll_scene(glam::Mat4::IDENTITY);
        crate::view::test_support::get_element_mut::<Element>(&arena, roots[0])
            .set_resolved_transform_for_test(None);
        crate::view::test_support::get_element_mut::<Element>(&arena, roots[0]).set_opacity(0.5);
        arena.refresh_subtree_dirty_cache(roots[0]);
        let (properties, generations) = synced_paint_state(&arena, &roots);
        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &FxHashSet::default(),
            &ctx,
            true,
        );
        let AutoAuthorityDecision::EffectScrollScene { scene, trace } = decision else {
            panic!("exact E->S must select the effect-scroll property scene")
        };
        assert!(scene.is_canonical());
        assert!(matches!(
            trace.rejections.as_slice(),
            [
                AutoAuthorityRejection::PropertyScrollPlan { .. },
                AutoAuthorityRejection::TransformScrollPlan { .. }
            ]
        ));

        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let prepared = crate::view::paint::prepare_retained_effect_scroll_scene_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            ctx,
            [0.0, 0.0, 0.0, 1.0],
            owner,
        )
        .unwrap();
        let outcome = crate::view::paint::emit_prepared_retained_effect_scroll_scene(prepared);
        let (_, trace) = outcome.into_parts();
        assert_eq!(trace.root_count, 1);
        assert_eq!(trace.generic_surface_count, 1);
        assert_eq!(trace.effect_surface_count, 1);
        assert_eq!(trace.scroll_group_count, 1);
        assert_eq!(trace.reraster_count, 2);
        assert_eq!(trace.reuse_count, 0);
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .len(),
            3,
            "one root clear plus effect/content reraster clears"
        );
        let composites = graph.test_graphics_passes::<
            crate::view::render_pass::composite_layer_pass::CompositeLayerPass,
        >();
        assert_eq!(composites.len(), 1);
        assert_eq!(
            composites[0].test_snapshot().opacity_bits,
            0.5_f32.to_bits()
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    }

    #[test]
    fn retained_auto_exact_multi_scroll_selects_one_atomic_scene() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let (arena, roots, properties, generations) = prepared_exact_multi_scroll_scene();
        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &FxHashSet::default(),
            &ctx,
            true,
        );
        let AutoAuthorityDecision::PropertyScrollScene { scene, trace } = decision else {
            panic!("two exact top-level scroll roots must select one property scene")
        };
        assert_eq!(scene.boundary_count(), 2);
        assert!(trace.rejections.is_empty());
        assert_eq!(properties.scrolls.len(), 2);

        let promoted = FxHashSet::from_iter([0xe2_b310]);
        let AutoAuthorityDecision::Legacy { trace } = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &promoted,
            &ctx,
            true,
        ) else {
            panic!("one invalid root must reject the entire multi-root scene")
        };
        assert!(matches!(
            trace.rejections.as_slice(),
            [
                AutoAuthorityRejection::PropertyScrollPlan {
                    error: crate::view::paint::PropertyScrollScenePlanError::InvalidContract,
                },
                AutoAuthorityRejection::TransformScrollPlan { .. },
                AutoAuthorityRejection::EffectScrollPlan { .. },
                AutoAuthorityRejection::TransformEffectScrollPlan { .. },
                AutoAuthorityRejection::DirectScrollTransformPlan { .. }
            ]
        ));
    }

    #[test]
    fn retained_auto_occupied_pending_falls_back_without_finishing_foreign_owner() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let (arena, roots, properties, generations) = prepared_exact_scroll_scene();
        let AutoAuthorityDecision::PropertyScrollScene { scene, .. } =
            select_retained_auto_authority(
                &arena,
                &roots,
                &properties,
                &generations,
                &FxHashSet::default(),
                &ctx,
                true,
            )
        else {
            panic!("exact B0 scroll must select PropertyScene before prepare")
        };

        let mut viewport = Viewport::new();
        assert!(viewport.stage_retained_surface_clear());
        let foreign_pending = viewport.compositor.pending_retained_surfaces.clone();
        let foreign_owner = viewport.compositor.pending_retained_surface_owner;
        let resident_before = viewport.compositor.retained_surfaces.clone();
        let frame_owner = viewport.begin_retained_surface_frame_stage();
        assert!(frame_owner.is_none());

        let mut graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        assert!(scene.is_canonical());
        assert!(!viewport.retained_property_scroll_scene_stage_is_available());
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);

        let mut legacy_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let output = legacy_ctx.allocate_target(&mut graph);
        graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: legacy_ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: output,
            },
        ));
        assert!(!viewport.stage_retained_surface_clear());
        assert!(!viewport.finish_retained_surface_transaction_for_frame(frame_owner, true));
        assert_eq!(
            viewport.compositor.pending_retained_surfaces,
            foreign_pending
        );
        assert_eq!(
            viewport.compositor.pending_retained_surface_owner,
            foreign_owner
        );
        assert_eq!(viewport.compositor.retained_surfaces, resident_before);

        viewport.finish_retained_surface_transaction(true);
        assert!(viewport.compositor.pending_retained_surfaces.is_none());
    }

    #[test]
    fn retained_auto_selects_supported_scroll_topologies_and_rejects_the_rest() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let assert_typed_rejection = |arena: &NodeArena, roots: &[NodeKey]| {
            let (properties, generations) = synced_paint_state(arena, roots);
            let AutoAuthorityDecision::Legacy { trace } = select_retained_auto_authority(
                arena,
                roots,
                &properties,
                &generations,
                &FxHashSet::default(),
                &ctx,
                true,
            ) else {
                panic!("unsupported scroll topology must remain whole-frame legacy")
            };
            let expected = matches!(
                trace.rejections.as_slice(),
                [
                    AutoAuthorityRejection::PropertyScrollPlan { .. },
                    AutoAuthorityRejection::TransformScrollPlan { .. },
                    AutoAuthorityRejection::EffectScrollPlan { .. },
                    AutoAuthorityRejection::TransformEffectScrollPlan { .. },
                    AutoAuthorityRejection::DirectScrollTransformPlan { .. }
                ]
            );
            assert!(expected, "typed scroll rejection: {:?}", trace.rejections);
        };

        let (transform_arena, transform_roots, _, _) = prepared_exact_scroll_scene();
        let mut transform_style = Style::new();
        transform_style.set_transform(Transform::new([Translate::x(Length::px(3.0))]));
        crate::view::test_support::get_element_mut::<Element>(&transform_arena, transform_roots[0])
            .apply_style(transform_style);
        assert_typed_rejection(&transform_arena, &transform_roots);

        let (effect_arena, effect_roots, _, _) = prepared_exact_scroll_scene();
        crate::view::test_support::get_element_mut::<Element>(&effect_arena, effect_roots[0])
            .set_opacity(0.5);
        assert_typed_rejection(&effect_arena, &effect_roots);

        let (mut nested_arena, nested_roots, mut nested_properties, mut nested_generations) =
            prepared_exact_nested_scroll_scene();
        let outer = nested_roots[0];
        let inner = nested_arena.children_of(outer)[0];
        let extra_leaf = nested_arena.insert(Node::new(Box::new(Element::new_with_id(
            0xe2_b320, 10.0, 20.0, 100.0, 60.0,
        ))));
        nested_arena.set_parent(extra_leaf, Some(inner));
        nested_arena.push_child(inner, extra_leaf);
        nested_arena
            .get_mut(extra_leaf)
            .expect("extra nested leaf")
            .element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        nested_arena.refresh_subtree_dirty_cache(outer);
        nested_properties.sync(&nested_arena, &nested_roots);
        nested_generations.sync(&nested_arena, &nested_roots, &nested_properties);
        assert_eq!(nested_roots.len(), 1);
        assert_eq!(nested_properties.scrolls.len(), 2);
        let captured = select_retained_auto_authority(
            &nested_arena,
            &nested_roots,
            &nested_properties,
            &nested_generations,
            &FxHashSet::default(),
            &ctx,
            true,
        );
        let uncaptured = select_retained_auto_authority(
            &nested_arena,
            &nested_roots,
            &nested_properties,
            &nested_generations,
            &FxHashSet::default(),
            &ctx,
            false,
        );
        assert!(matches!(&captured, AutoAuthorityDecision::Legacy { .. }));
        assert!(matches!(&uncaptured, AutoAuthorityDecision::Legacy { .. }));
        assert_eq!(
            auto_authority_kind(&captured),
            auto_authority_kind(&uncaptured),
            "trace capture must not change malformed nested-scroll authority"
        );
        assert!(matches!(
            auto_authority_trace(&captured).rejections.as_slice(),
            [
                AutoAuthorityRejection::NestedScrollPlan { .. },
                AutoAuthorityRejection::PropertyScrollPlan { .. },
                AutoAuthorityRejection::TransformScrollPlan { .. },
                AutoAuthorityRejection::EffectScrollPlan { .. },
                AutoAuthorityRejection::TransformEffectScrollPlan { .. },
                AutoAuthorityRejection::DirectScrollTransformPlan { .. }
            ]
        ));
        assert!(auto_authority_trace(&uncaptured).rejections.is_empty());

        for matrix in [
            glam::Mat4::from_scale(glam::Vec3::new(1.1, 1.0, 1.0)),
            glam::Mat4::from_rotation_z(0.2),
        ] {
            let (arena, roots, _, _) = prepared_transform_scroll_scene(matrix);
            assert_typed_rejection(&arena, &roots);
        }

        let (mut clipped_arena, clipped_roots, _, _) =
            prepared_transform_scroll_scene(glam::Mat4::IDENTITY);
        let clipped_effect = clipped_roots[0];
        crate::view::test_support::get_element_mut::<Element>(&clipped_arena, clipped_effect)
            .set_resolved_transform_for_test(None);
        crate::view::test_support::get_element_mut::<Element>(&clipped_arena, clipped_effect)
            .set_opacity(0.5);
        let clip_root = clipped_arena.insert(Node::new(Box::new(TransparentContentsClipParent {
            id: 0xe2_c3e0,
            scissor: [4, 6, 100, 70],
            children: Vec::new(),
        })));
        clipped_arena.set_parent(clipped_effect, Some(clip_root));
        clipped_arena.push_child(clip_root, clipped_effect);
        clipped_arena.refresh_subtree_dirty_cache(clip_root);
        assert_typed_rejection(&clipped_arena, &[clip_root]);

        let (mut nested_effect_arena, nested_effect_roots, _, _) =
            prepared_transform_scroll_scene(glam::Mat4::IDENTITY);
        let outer_effect = nested_effect_roots[0];
        crate::view::test_support::get_element_mut::<Element>(&nested_effect_arena, outer_effect)
            .set_resolved_transform_for_test(None);
        crate::view::test_support::get_element_mut::<Element>(&nested_effect_arena, outer_effect)
            .set_opacity(0.5);
        let scroll = nested_effect_arena.children_of(outer_effect)[0];
        let mut inner_effect = Element::new_with_id(0xe2_c3e1, 0.0, 0.0, 120.0, 90.0);
        let mut inner_style = Style::new();
        inner_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        inner_effect.apply_style(inner_style);
        inner_effect.set_opacity(0.25);
        let inner_effect = nested_effect_arena.insert(Node::new(Box::new(inner_effect)));
        nested_effect_arena.set_parent(inner_effect, Some(outer_effect));
        nested_effect_arena.set_children(outer_effect, vec![inner_effect]);
        nested_effect_arena.set_parent(scroll, Some(inner_effect));
        nested_effect_arena.set_children(inner_effect, vec![scroll]);
        nested_effect_arena.refresh_subtree_dirty_cache(outer_effect);
        assert_typed_rejection(&nested_effect_arena, &nested_effect_roots);

        let (scroll_effect_arena, scroll_effect_roots, _, _) = prepared_exact_scroll_scene();
        let scroll_effect_child = scroll_effect_arena.children_of(scroll_effect_roots[0])[0];
        crate::view::test_support::get_element_mut::<Element>(
            &scroll_effect_arena,
            scroll_effect_child,
        )
        .set_opacity(0.5);
        scroll_effect_arena.refresh_subtree_dirty_cache(scroll_effect_roots[0]);
        assert_typed_rejection(&scroll_effect_arena, &scroll_effect_roots);

        let (scroll_transform_arena, scroll_transform_roots, _, _) = prepared_exact_scroll_scene();
        let scroll_transform_child =
            scroll_transform_arena.children_of(scroll_transform_roots[0])[0];
        crate::view::test_support::get_element_mut::<Element>(
            &scroll_transform_arena,
            scroll_transform_child,
        )
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
            glam::Vec3::new(3.0, 0.0, 0.0),
        )));
        scroll_transform_arena.refresh_subtree_dirty_cache(scroll_transform_roots[0]);
        let (scroll_transform_properties, scroll_transform_generations) =
            synced_paint_state(&scroll_transform_arena, &scroll_transform_roots);
        let AutoAuthorityDecision::DirectScrollTransformScene { scene, trace } =
            select_retained_auto_authority(
                &scroll_transform_arena,
                &scroll_transform_roots,
                &scroll_transform_properties,
                &scroll_transform_generations,
                &FxHashSet::default(),
                &ctx,
                true,
            )
        else {
            panic!("exact S->T must select only after all older scroll authorities reject")
        };
        assert!(scene.is_canonical());
        assert!(matches!(
            trace.rejections.as_slice(),
            [
                AutoAuthorityRejection::PropertyScrollPlan { .. },
                AutoAuthorityRejection::TransformScrollPlan { .. },
                AutoAuthorityRejection::EffectScrollPlan { .. },
                AutoAuthorityRejection::TransformEffectScrollPlan { .. }
            ]
        ));
        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let prepared = crate::view::paint::prepare_direct_scroll_transform_scene_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            [0.0, 0.0, 0.0, 1.0],
            owner,
        )
        .unwrap();
        let outcome = crate::view::paint::emit_prepared_direct_scroll_transform_scene(prepared);
        let (_, build_trace) = outcome.into_parts();
        assert_eq!(build_trace.root_count, 1);
        assert_eq!(build_trace.generic_surface_count, 1);
        assert_eq!(build_trace.scroll_group_count, 0);
        assert_eq!(build_trace.reraster_count, 1);
        assert_eq!(build_trace.reuse_count, 0);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));

        let (effect_scroll_arena, effect_scroll_roots, _, _) = prepared_transform_scroll_scene(
            glam::Mat4::from_translation(glam::Vec3::new(3.0, 0.0, 0.0)),
        );
        crate::view::test_support::get_element_mut::<Element>(
            &effect_scroll_arena,
            effect_scroll_roots[0],
        )
        .set_resolved_transform_for_test(None);
        crate::view::test_support::get_element_mut::<Element>(
            &effect_scroll_arena,
            effect_scroll_roots[0],
        )
        .set_opacity(0.5);
        effect_scroll_arena.refresh_subtree_dirty_cache(effect_scroll_roots[0]);
        let (effect_scroll_properties, effect_scroll_generations) =
            synced_paint_state(&effect_scroll_arena, &effect_scroll_roots);
        let AutoAuthorityDecision::EffectScrollScene { scene, trace } =
            select_retained_auto_authority(
                &effect_scroll_arena,
                &effect_scroll_roots,
                &effect_scroll_properties,
                &effect_scroll_generations,
                &FxHashSet::default(),
                &ctx,
                true,
            )
        else {
            panic!("exact direct E->S must select the effect-scroll property scene")
        };
        assert!(scene.is_canonical());
        assert!(matches!(
            trace.rejections.as_slice(),
            [
                AutoAuthorityRejection::PropertyScrollPlan { .. },
                AutoAuthorityRejection::TransformScrollPlan { .. }
            ]
        ));

        let (mut transform_effect_scroll_arena, transform_effect_scroll_roots, _, _) =
            prepared_transform_scroll_scene(glam::Mat4::from_translation(glam::Vec3::new(
                3.0, 0.0, 0.0,
            )));
        let transform_root = transform_effect_scroll_roots[0];
        let scroll = transform_effect_scroll_arena.children_of(transform_root)[0];
        let mut effect = Element::new_with_id(0xe2_c3f0, 0.0, 0.0, 120.0, 90.0);
        let mut effect_style = Style::new();
        effect_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        effect.apply_style(effect_style);
        effect.set_opacity(0.5);
        let effect = transform_effect_scroll_arena.insert(Node::new(Box::new(effect)));
        transform_effect_scroll_arena.set_parent(effect, Some(transform_root));
        transform_effect_scroll_arena.set_children(transform_root, vec![effect]);
        transform_effect_scroll_arena.set_parent(scroll, Some(effect));
        transform_effect_scroll_arena.set_children(effect, vec![scroll]);
        transform_effect_scroll_arena.refresh_subtree_dirty_cache(transform_root);
        let (properties, generations) = synced_paint_state(
            &transform_effect_scroll_arena,
            &transform_effect_scroll_roots,
        );
        let AutoAuthorityDecision::TransformEffectScrollScene { scene, trace } =
            select_retained_auto_authority(
                &transform_effect_scroll_arena,
                &transform_effect_scroll_roots,
                &properties,
                &generations,
                &FxHashSet::default(),
                &ctx,
                true,
            )
        else {
            panic!("exact T->E->S must select the transform-effect-scroll property scene")
        };
        assert!(scene.is_canonical());
        assert!(matches!(
            trace.rejections.as_slice(),
            [
                AutoAuthorityRejection::PropertyScrollPlan { .. },
                AutoAuthorityRejection::TransformScrollPlan { .. },
                AutoAuthorityRejection::EffectScrollPlan { .. }
            ]
        ));

        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let prepared =
            crate::view::paint::prepare_retained_transform_effect_scroll_scene_from_pool(
                &mut viewport,
                scene,
                &mut graph,
                UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
                [0.0, 0.0, 0.0, 1.0],
                owner,
            )
            .unwrap();
        let outcome =
            crate::view::paint::emit_prepared_retained_transform_effect_scroll_scene(prepared);
        let (_, trace) = outcome.into_parts();
        assert_eq!(trace.root_count, 1);
        assert_eq!(trace.generic_surface_count, 2);
        assert_eq!(trace.effect_surface_count, 1);
        assert_eq!(trace.scroll_group_count, 1);
        assert_eq!(trace.reraster_count, 3);
        assert_eq!(trace.reuse_count, 0);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));

        let promoted = FxHashSet::from_iter([transform_effect_scroll_arena
            .get(transform_root)
            .unwrap()
            .element
            .stable_id()]);
        let AutoAuthorityDecision::Legacy { trace } = select_retained_auto_authority(
            &transform_effect_scroll_arena,
            &transform_effect_scroll_roots,
            &properties,
            &generations,
            &promoted,
            &ctx,
            true,
        ) else {
            panic!("tampered exact T->E->S authority must fall directly to whole-frame legacy")
        };
        assert!(matches!(
            trace.rejections.as_slice(),
            [
                AutoAuthorityRejection::PropertyScrollPlan { .. },
                AutoAuthorityRejection::TransformScrollPlan { .. },
                AutoAuthorityRejection::EffectScrollPlan { .. },
                AutoAuthorityRejection::TransformEffectScrollPlan { .. },
                AutoAuthorityRejection::DirectScrollTransformPlan { .. }
            ]
        ));
    }

    #[test]
    fn retained_auto_direct_scroll_transform_production_preflight_and_rejection_dispatch() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let (arena, roots, _, _) = prepared_exact_scroll_scene();
        let child = arena.children_of(roots[0])[0];
        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                3.0, 0.0, 0.0,
            ))));
        arena.refresh_subtree_dirty_cache(roots[0]);
        let (properties, generations) = synced_paint_state(&arena, &roots);
        let select = || {
            select_retained_auto_authority(
                &arena,
                &roots,
                &properties,
                &generations,
                &FxHashSet::default(),
                &ctx,
                true,
            )
        };

        let AutoAuthorityDecision::DirectScrollTransformScene { scene, trace } = select() else {
            panic!("exact S->T must reach the production direct preflight")
        };
        assert_eq!(trace.rejections.len(), 4);
        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let (selection, outcome) = preflight_direct_scroll_transform_selection(
            &mut viewport,
            &mut graph,
            UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            [0.125, 0.25, 0.5, 1.0],
            Some(owner),
            RetainedTransformCanarySelection::DirectScrollTransformScenePlanned(scene),
        );
        assert!(matches!(
            selection,
            RetainedTransformCanarySelection::DirectScrollTransformScenePrepared
        ));
        let outcome = outcome.expect("production preflight pre-emits one sealed S->T outcome");
        let (_, build_trace) = outcome.into_parts();
        assert_eq!(
            (
                build_trace.generic_surface_count,
                build_trace.scroll_group_count,
                build_trace.reraster_count,
                build_trace.reuse_count,
            ),
            (1, 0, 1, 0)
        );
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .len(),
            2,
            "pre-emitted S->T owns the root and cold T clears; common clear must be skipped"
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));

        let AutoAuthorityDecision::DirectScrollTransformScene { scene, .. } = select() else {
            unreachable!()
        };
        assert!(viewport.stage_retained_surface_clear());
        let missing_owner = viewport.begin_retained_surface_frame_stage();
        assert!(missing_owner.is_none());
        let mut rejected_graph = FrameGraph::new();
        let graph_before = rejected_graph.build_state_snapshot_for_test();
        let (selection, outcome) = preflight_direct_scroll_transform_selection(
            &mut viewport,
            &mut rejected_graph,
            UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            [0.0; 4],
            missing_owner,
            RetainedTransformCanarySelection::DirectScrollTransformScenePlanned(scene),
        );
        assert!(outcome.is_none());
        let RetainedTransformCanarySelection::DirectScrollTransformScenePrepareRejected(error) =
            &selection
        else {
            panic!("occupied pending slot must become a prepare-stage fallback")
        };
        assert_eq!(
            *error,
            crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable
        );
        assert_eq!(rejected_graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            direct_scroll_transform_prepare_rejection_fallback_stage(),
            PaintAuthorityFallbackStage::Prepare
        );
        let (legacy, label) = direct_scroll_transform_prepare_rejection_dispatch(error);
        assert!(legacy);
        assert!(label.contains("direct-scroll-transform-prepare-rejected=StageUnavailable"));
        assert!(viewport.compositor.pending_retained_surfaces.is_some());
        viewport.finish_retained_surface_transaction(true);
    }

    #[test]
    fn retained_auto_transform_effect_scroll_production_preflight_and_rejection_dispatch() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let (arena, roots, properties, generations) = prepared_transform_effect_scroll_scene();
        let select = || {
            select_retained_auto_authority(
                &arena,
                &roots,
                &properties,
                &generations,
                &FxHashSet::default(),
                &ctx,
                true,
            )
        };

        let AutoAuthorityDecision::TransformEffectScrollScene { scene, trace } = select() else {
            panic!("exact T->E->S must reach the production joint preflight")
        };
        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let (selection, outcome) = preflight_transform_effect_scroll_selection(
            &mut viewport,
            &mut graph,
            UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            [0.0; 4],
            Some(owner),
            RetainedTransformCanarySelection::TransformEffectScrollScenePlanned(scene),
        );
        assert!(matches!(
            selection,
            RetainedTransformCanarySelection::TransformEffectScrollScenePrepared
        ));
        let outcome = outcome.expect("production preflight emits one sealed joint outcome");
        let (_, build_trace) = outcome.into_parts();
        assert_eq!(
            (
                build_trace.generic_surface_count,
                build_trace.effect_surface_count,
                build_trace.scroll_group_count,
            ),
            (2, 1, 1)
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));

        let AutoAuthorityDecision::TransformEffectScrollScene { scene, .. } = select() else {
            unreachable!()
        };
        assert!(viewport.stage_retained_surface_clear());
        let missing_owner = viewport.begin_retained_surface_frame_stage();
        assert!(missing_owner.is_none());
        let mut rejected_graph = FrameGraph::new();
        let graph_before = rejected_graph.build_state_snapshot_for_test();
        let (selection, outcome) = preflight_transform_effect_scroll_selection(
            &mut viewport,
            &mut rejected_graph,
            UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            [0.0; 4],
            missing_owner,
            RetainedTransformCanarySelection::TransformEffectScrollScenePlanned(scene),
        );
        assert!(outcome.is_none());
        let RetainedTransformCanarySelection::TransformEffectScrollScenePrepareRejected(error) =
            &selection
        else {
            panic!("occupied production stage must become the typed prepare rejection")
        };
        assert_eq!(
            error,
            &crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable
        );
        assert_eq!(rejected_graph.build_state_snapshot_for_test(), graph_before);
        let (whole_frame_legacy, detail) =
            transform_effect_scroll_prepare_rejection_dispatch(error);
        assert!(whole_frame_legacy);
        assert!(detail.contains("authority=legacy"));
        let fallback_stage = transform_effect_scroll_prepare_rejection_fallback_stage();
        assert_eq!(fallback_stage, PaintAuthorityFallbackStage::Prepare);
        let mut telemetry = PaintAuthorityTelemetry::from_selection(
            ViewportPaintRendererMode::RetainedAuto,
            &selection,
            Some((AutoAuthorityKind::PropertyScene, trace)),
        );
        telemetry.note_legacy_fallback(fallback_stage);
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.authority_label, "retained-auto:legacy");
        assert_eq!(
            snapshot.legacy_fallback_stage,
            Some(PaintAuthorityFallbackStage::Prepare)
        );
        viewport.finish_retained_surface_transaction(true);
    }

    #[test]
    fn retained_auto_does_not_treat_plain_overflow_as_an_authored_scroll_boundary() {
        let mut arena = new_test_arena();
        let mut root_element = colored_element(0xe2_a320, 0.0, Color::rgb(20, 40, 80));
        let mut layout_style = Style::new();
        layout_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        root_element.apply_style(layout_style);
        let root = commit_element(&mut arena, Box::new(root_element));
        let child = commit_child(
            &mut arena,
            root,
            Box::new(Element::new_with_id(0xe2_a321, 0.0, 0.0, 120.0, 120.0)),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        assert!(arena.get(child).is_some());
        assert!(!super::reachable_tree_has_scroll_container(&arena, &[root]));
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let decision = auto_decision(&arena, &[root], &FxHashSet::default(), &ctx);
        assert!(!matches!(
            decision,
            AutoAuthorityDecision::PropertyScrollScene { .. }
        ));
        if let AutoAuthorityDecision::Legacy { trace } = decision {
            assert!(!trace.rejections.iter().any(|rejection| matches!(
                rejection,
                AutoAuthorityRejection::PropertyScrollPlan { .. }
            )));
        }
    }

    #[test]
    fn retained_auto_promotion_and_deferred_frames_select_structured_legacy() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let (arena, roots) = prepared_safe_leaf();
        let promoted = FxHashSet::from_iter([1]);
        let AutoAuthorityDecision::Legacy { trace } =
            auto_decision(&arena, &roots, &promoted, &ctx)
        else {
            panic!("promoted frame must remain whole-frame legacy")
        };
        assert!(trace.rejections.iter().any(|rejection| matches!(
            rejection,
            AutoAuthorityRejection::Artifact { eligibility }
                if eligibility.reasons.contains(
                    &crate::view::paint::FrameArtifactFallbackReason::PromotedBoundary
                )
        )));

        let mut deferred = colored_element(0xe2_a310, 10.0, Color::rgb(230, 20, 30));
        let mut style = Style::new();
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(4.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        deferred.apply_style(style);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(deferred));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let AutoAuthorityDecision::Legacy { trace } =
            auto_decision(&arena, &[root], &FxHashSet::default(), &ctx)
        else {
            panic!("deferred frame must remain whole-frame legacy")
        };
        assert!(trace.rejections.iter().any(|rejection| matches!(
            rejection,
            AutoAuthorityRejection::Artifact { eligibility }
                if eligibility.reasons.contains(
                    &crate::view::paint::FrameArtifactFallbackReason::LegacyBoundary(
                        crate::view::paint::LegacyPaintReason::Deferred,
                    )
                )
        )));
    }

    #[test]
    fn retained_auto_is_opt_in_and_named_modes_remain_isolated() {
        let viewport = Viewport::new();
        assert_eq!(
            viewport.paint_renderer_mode(),
            ViewportPaintRendererMode::Legacy
        );

        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let promoted = FxHashSet::default();
        let (arena, roots) = prepared_transform_leaf();
        let (properties, generations) = synced_paint_state(&arena, &roots);
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedAuto,
                &arena,
                &roots,
                &properties,
                &generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::Auto(AutoAuthorityDecision::PropertyScene { .. })
        ));
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedTransformCanary,
                &arena,
                &roots,
                &properties,
                &generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::Planned(_)
        ));
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedScrollSceneCanary,
                &arena,
                &roots,
                &properties,
                &generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::ScrollSceneShapeRejected { scroll_count: 0 }
        ));

        let (scroll_arena, scroll_roots, scroll_properties, scroll_generations) =
            prepared_exact_scroll_scene();
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedScrollSceneCanary,
                &scroll_arena,
                &scroll_roots,
                &scroll_properties,
                &scroll_generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::ScrollSceneActive
        ));

        let (isolation_arena, isolation_roots) = prepared_safe_leaf();
        crate::view::test_support::get_element_mut::<Element>(&isolation_arena, isolation_roots[0])
            .set_opacity(0.5);
        let (isolation_properties, isolation_generations) =
            synced_paint_state(&isolation_arena, &isolation_roots);
        let isolation_selection = select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedIsolationCanary,
            &isolation_arena,
            &isolation_roots,
            &isolation_properties,
            &isolation_generations,
            &promoted,
            &ctx,
        );
        let isolation_telemetry = PaintAuthorityTelemetry::from_selection(
            ViewportPaintRendererMode::RetainedIsolationCanary,
            &isolation_selection,
            None,
        );
        assert_eq!(
            isolation_telemetry.snapshot().authority_label,
            "retained-isolation-canary"
        );
        assert_eq!(
            isolation_telemetry.snapshot().selected,
            PaintAuthorityKind::Isolation
        );

        let (neutral_arena, neutral_roots) = prepared_safe_leaf();
        let (neutral_properties, neutral_generations) =
            synced_paint_state(&neutral_arena, &neutral_roots);
        let rejected_isolation = select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedIsolationCanary,
            &neutral_arena,
            &neutral_roots,
            &neutral_properties,
            &neutral_generations,
            &promoted,
            &ctx,
        );
        let mut rejected_telemetry = PaintAuthorityTelemetry::from_selection(
            ViewportPaintRendererMode::RetainedIsolationCanary,
            &rejected_isolation,
            None,
        );
        rejected_telemetry.note_legacy_fallback(PaintAuthorityFallbackStage::Selection);
        let rejected_snapshot = rejected_telemetry.snapshot();
        assert_eq!(
            rejected_snapshot.authority_label,
            "retained-isolation-canary"
        );
        assert_eq!(
            rejected_snapshot.legacy_fallback_stage,
            Some(PaintAuthorityFallbackStage::Selection)
        );
        assert!(
            rejected_snapshot
                .rejection_labels
                .iter()
                .any(|label| label.contains("InvalidIsolationEffect"))
        );
    }

    #[test]
    fn retained_auto_terminal_failure_outcome_is_typed_and_named_modes_do_not_arm() {
        assert_eq!(
            terminal_failure_stage(false, false),
            Some(RetainedAutoTerminalFailureStage::Compile)
        );
        assert_eq!(
            terminal_failure_stage(true, false),
            Some(RetainedAutoTerminalFailureStage::Execute)
        );
        assert_eq!(terminal_failure_stage(true, true), None);
        assert_eq!(frame_disposition(false, false), FrameDisposition::Abort);
        assert_eq!(frame_disposition(true, false), FrameDisposition::Abort);
        assert_eq!(
            frame_disposition(true, true),
            FrameDisposition::SubmitAndPresent
        );
        assert!(!should_store_compile_cache(false, false));
        assert!(!should_store_compile_cache(true, false));
        assert!(should_store_compile_cache(true, true));

        for mode in [
            ViewportPaintRendererMode::Legacy,
            ViewportPaintRendererMode::ArtifactCanary,
            ViewportPaintRendererMode::RetainedTransformCanary,
        ] {
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            viewport.take_redraw_request();
            assert!(
                !viewport
                    .arm_retained_auto_terminal_failure(RetainedAutoTerminalFailureStage::Compile)
            );
            assert_eq!(viewport.retained_auto_terminal_failure, None);
            assert!(!viewport.take_redraw_request());
        }
    }

    #[test]
    fn compile_terminal_failure_chooses_abort_completion() {
        let mut graph = FrameGraph::new();
        let desc = crate::view::frame_graph::TextureDesc::new(
            1,
            1,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureDimension::D2,
        );
        let duplicate_key = crate::view::frame_graph::PersistentTextureKey::Generic(0xab07);
        graph.declare_persistent_texture_internal::<()>(desc.clone(), duplicate_key);
        graph.declare_persistent_texture_internal::<()>(desc, duplicate_key);
        assert!(
            graph.compile().is_err(),
            "duplicate persistent keys are a compile-terminal fixture"
        );
        assert_eq!(
            terminal_failure_stage(false, false),
            Some(RetainedAutoTerminalFailureStage::Compile)
        );
        assert_eq!(frame_disposition(false, false), FrameDisposition::Abort);
    }

    #[test]
    fn partial_execute_terminal_failure_chooses_abort_completion() {
        // `execute_profiled` reports this state after stopping at a failed
        // execute step; preceding steps may already have recorded commands.
        assert_eq!(
            terminal_failure_stage(true, false),
            Some(RetainedAutoTerminalFailureStage::Execute)
        );
        assert_eq!(frame_disposition(true, false), FrameDisposition::Abort);
    }

    #[test]
    #[ignore = "requires a native GPU adapter"]
    fn abort_frame_discards_encoder_resets_staging_and_next_frame_submits() -> Result<(), String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            flags: wgpu::InstanceFlags::empty(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|error| format!("abort-frame test requires a GPU adapter: {error:?}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rfgui abort-frame test device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        }))
        .map_err(|error| format!("failed to create abort-frame test device: {error:?}"))?;

        let mut viewport = Viewport::new();
        viewport.begin_offscreen_test_frame(
            device.clone(),
            queue.clone(),
            4,
            4,
            wgpu::TextureFormat::Rgba8Unorm,
        )?;
        assert!(viewport.frame.frame_state.is_some());
        assert!(
            viewport
                .upload_draw_rect_uniform(&[1, 2, 3, 4], 256, 256)
                .is_some(),
            "fixture must record a native staging-belt copy"
        );
        assert!(viewport.gpu.upload_staging_belt.is_some());

        let profile = viewport.complete_frame(FrameDisposition::Abort);
        assert!(viewport.frame.frame_state.is_none());
        assert_eq!(viewport.frame_completion_counts_for_test(), (0, 0, 1));
        assert!(!viewport.frame.frame_presented);
        assert!(viewport.gpu.upload_staging_belt.is_none());
        assert_eq!(profile.submit_ms, 0.0);
        assert_eq!(profile.present_ms, 0.0);

        viewport.begin_offscreen_test_frame(
            device,
            queue,
            4,
            4,
            wgpu::TextureFormat::Rgba8Unorm,
        )?;
        assert!(
            viewport
                .upload_draw_rect_uniform(&[5, 6, 7, 8], 256, 256)
                .is_some(),
            "the frame after abort must lazily recreate the staging belt"
        );
        assert!(viewport.gpu.upload_staging_belt.is_some());
        viewport.end_offscreen_test_frame()?;
        assert!(viewport.frame.frame_state.is_none());
        assert_eq!(viewport.frame_completion_counts_for_test(), (1, 0, 1));
        Ok(())
    }

    #[test]
    fn retained_auto_terminal_failure_latches_once_and_same_mode_setter_resets_it() {
        let mut viewport = Viewport::new();
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedAuto);
        assert!(viewport.take_redraw_request());

        let owner = viewport
            .begin_retained_surface_frame_stage()
            .expect("fresh viewport owns the retained transaction stage");
        assert!(viewport.stage_retained_surface_clear());
        viewport.stage_root_effect_clear();
        viewport.finish_root_effect_transaction(false);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, None)
        );
        assert!(viewport.retained_property_scroll_scene_stage_is_available());

        assert!(
            viewport.arm_retained_auto_terminal_failure(RetainedAutoTerminalFailureStage::Compile)
        );
        assert_eq!(
            viewport.paint_renderer_mode(),
            ViewportPaintRendererMode::RetainedAuto,
            "the public getter keeps the requested mode"
        );
        assert_eq!(
            viewport.retained_auto_terminal_failure,
            Some(RetainedAutoTerminalFailureStage::Compile)
        );
        assert!(viewport.take_redraw_request());

        assert!(
            !viewport.arm_retained_auto_terminal_failure(RetainedAutoTerminalFailureStage::Execute)
        );
        assert_eq!(
            viewport.retained_auto_terminal_failure,
            Some(RetainedAutoTerminalFailureStage::Compile),
            "the first terminal stage remains authoritative"
        );
        assert!(
            !viewport.take_redraw_request(),
            "an open breaker cannot spin redraws"
        );

        assert_eq!(terminal_failure_stage(true, true), None);
        assert_eq!(
            viewport.retained_auto_terminal_failure,
            Some(RetainedAutoTerminalFailureStage::Compile),
            "a successful Legacy recovery does not half-open automatically"
        );

        seed_empty_compile_cache(&mut viewport);
        assert!(viewport.frame.compile_cache.is_some());
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedAuto);
        assert_eq!(viewport.retained_auto_terminal_failure, None);
        assert!(
            viewport.frame.compile_cache.is_none(),
            "manual circuit reset must discard the failed-frame topology cache"
        );
        assert!(viewport.take_redraw_request());
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedAuto);
        assert!(
            !viewport.take_redraw_request(),
            "ordinary same-mode set stays idempotent"
        );

        assert!(
            viewport.arm_retained_auto_terminal_failure(RetainedAutoTerminalFailureStage::Execute)
        );
        viewport.take_redraw_request();
        seed_empty_compile_cache(&mut viewport);
        assert!(viewport.frame.compile_cache.is_some());
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::Legacy);
        assert_eq!(viewport.retained_auto_terminal_failure, None);
        assert!(
            viewport.frame.compile_cache.is_none(),
            "paint mode switches must discard the prior mode's topology cache"
        );
        assert!(viewport.take_redraw_request());
    }

    #[test]
    fn retained_auto_open_breaker_forces_auto_legacy_with_capture_invariant_telemetry() {
        let (arena, roots) = prepared_transform_leaf();
        let (properties, generations) = synced_paint_state(&arena, &roots);
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedAuto,
                &arena,
                &roots,
                &properties,
                &generations,
                &FxHashSet::default(),
                &ctx,
            ),
            RetainedTransformCanarySelection::Auto(AutoAuthorityDecision::PropertyScene { .. })
        ));

        for capture_trace in [false, true] {
            let Some(RetainedTransformCanarySelection::Auto(AutoAuthorityDecision::Legacy {
                trace,
            })) = retained_auto_circuit_breaker_selection(
                Some(RetainedAutoTerminalFailureStage::Execute),
                capture_trace,
            )
            else {
                panic!("an open breaker must bypass retained planning as AutoLegacy")
            };
            assert_eq!(trace.capture_rejections, capture_trace);
            assert!(trace.rejections.is_empty());

            let selection = RetainedTransformCanarySelection::AutoLegacy;
            let mut telemetry = PaintAuthorityTelemetry::from_selection(
                ViewportPaintRendererMode::RetainedAuto,
                &selection,
                Some((AutoAuthorityKind::Legacy, trace)),
            );
            telemetry.note_legacy_fallback(retained_auto_terminal_fallback_stage(
                RetainedAutoTerminalFailureStage::Execute,
            ));
            let snapshot = telemetry.snapshot();
            assert_eq!(snapshot.authority_label, "retained-auto:legacy");
            assert_eq!(snapshot.selected, PaintAuthorityKind::Legacy);
            assert_eq!(
                snapshot.legacy_fallback_stage,
                Some(PaintAuthorityFallbackStage::Execute)
            );
        }
    }

    #[test]
    fn retained_transform_canary_selection_is_independent_and_fail_closed() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let promoted = FxHashSet::default();

        let (neutral_arena, neutral_roots) = prepared_safe_leaf();
        let (neutral_properties, neutral_generations) =
            synced_paint_state(&neutral_arena, &neutral_roots);
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedTransformCanary,
                &neutral_arena,
                &neutral_roots,
                &neutral_properties,
                &neutral_generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::NoTransform
        ));

        let (mut transform_arena, transform_roots) = prepared_transform_leaf();
        let (transform_properties, transform_generations) =
            synced_paint_state(&transform_arena, &transform_roots);
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::ArtifactCanary,
                &transform_arena,
                &transform_roots,
                &transform_properties,
                &transform_generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::Inactive
        ));
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedTransformCanary,
                &transform_arena,
                &transform_roots,
                &transform_properties,
                &transform_generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::Planned(_)
        ));

        let second_root = commit_element(
            &mut transform_arena,
            Box::new(colored_element(0xc4_b002, 120.0, Color::rgb(20, 210, 40))),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut transform_arena, second_root, measure, place);
        let mut invalid_roots = transform_roots.clone();
        invalid_roots.push(second_root);
        let (invalid_properties, invalid_generations) =
            synced_paint_state(&transform_arena, &invalid_roots);
        let RetainedTransformCanarySelection::PlanRejected(error) =
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedTransformCanary,
                &transform_arena,
                &invalid_roots,
                &invalid_properties,
                &invalid_generations,
                &promoted,
                &ctx,
            )
        else {
            panic!("multi-root transform frame must reject as a whole");
        };
        assert!(
            error
                .reasons
                .contains(&crate::view::paint::FramePaintPlanRejection::RootCount(2))
        );
    }

    #[test]
    fn retained_surface_tree_canary_is_independent_and_exact_depth_two_only() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let promoted = FxHashSet::default();
        let (arena, roots, _child) = prepared_nested_transform_tree();
        let (properties, generations) = synced_paint_state(&arena, &roots);
        assert_eq!(properties.transforms.len(), 2);

        let tree_selection = select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedSurfaceTreeCanary,
            &arena,
            &roots,
            &properties,
            &generations,
            &promoted,
            &ctx,
        );
        if let RetainedTransformCanarySelection::TreePlanRejected(error) = &tree_selection {
            panic!("exact depth-two fixture rejected: {:?}", error.reasons);
        }
        assert!(matches!(
            tree_selection,
            RetainedTransformCanarySelection::TreePlanned(_)
        ));
        let selection_graph = FrameGraph::new();
        let graph_before = selection_graph.build_state_snapshot_for_test();
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedTransformCanary,
                &arena,
                &roots,
                &properties,
                &generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::SingletonShapeRejected { transform_count: 2 }
        ));
        assert_eq!(
            selection_graph.build_state_snapshot_for_test(),
            graph_before,
            "singleton nested-shape rejection is resolved before common graph mutation"
        );
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::ArtifactCanary,
                &arena,
                &roots,
                &properties,
                &generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::Inactive
        ));

        let (singleton_arena, singleton_roots) = prepared_transform_leaf();
        let (singleton_properties, singleton_generations) =
            synced_paint_state(&singleton_arena, &singleton_roots);
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedSurfaceTreeCanary,
                &singleton_arena,
                &singleton_roots,
                &singleton_properties,
                &singleton_generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::TreeShapeRejected { transform_count: 1 }
        ));
    }

    #[test]
    fn retained_isolation_canary_is_independent_and_fail_closed_before_graph_mutation() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let promoted = FxHashSet::default();
        let (arena, roots) = prepared_safe_leaf();
        crate::view::test_support::get_element_mut::<Element>(&arena, roots[0]).set_opacity(0.5);
        let (properties, generations) = synced_paint_state(&arena, &roots);
        let graph = FrameGraph::new();
        let before = graph.build_state_snapshot_for_test();
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedIsolationCanary,
                &arena,
                &roots,
                &properties,
                &generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::IsolationPlanned(_)
        ));
        assert_eq!(graph.build_state_snapshot_for_test(), before);
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::ArtifactCanary,
                &arena,
                &roots,
                &properties,
                &generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::Inactive
        ));

        let (neutral_arena, neutral_roots) = prepared_safe_leaf();
        let (neutral_properties, neutral_generations) =
            synced_paint_state(&neutral_arena, &neutral_roots);
        let RetainedTransformCanarySelection::IsolationPlanRejected(error) =
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedIsolationCanary,
                &neutral_arena,
                &neutral_roots,
                &neutral_properties,
                &neutral_generations,
                &promoted,
                &ctx,
            )
        else {
            panic!("effect-neutral frame cannot enter retained isolation");
        };
        assert!(error.reasons.contains(
            &crate::view::paint::FramePaintPlanRejection::InvalidIsolationEffect(neutral_roots[0],)
        ));
        assert_eq!(graph.build_state_snapshot_for_test(), before);
    }

    #[test]
    fn retained_effect_tree_canary_selection_requires_exact_one_transform_and_one_effect() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let promoted = FxHashSet::default();

        let (arena, roots, _root, _child, _descendant) = prepared_transform_child_isolation_tree();
        let (properties, generations) = synced_paint_state(&arena, &roots);
        assert_eq!(properties.transforms.len(), 1);
        assert_eq!(properties.effects.len(), 1);
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedEffectTreeCanary,
                &arena,
                &roots,
                &properties,
                &generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::EffectTreePlanned(_)
        ));

        let (neutral_arena, neutral_roots) = prepared_safe_leaf();
        let (neutral_properties, neutral_generations) =
            synced_paint_state(&neutral_arena, &neutral_roots);
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedEffectTreeCanary,
                &neutral_arena,
                &neutral_roots,
                &neutral_properties,
                &neutral_generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::EffectTreeShapeRejected {
                transform_count: 0,
                effect_count: 0,
            }
        ));

        let (two_transform_arena, two_transform_roots, _) = prepared_nested_transform_tree();
        let (two_transform_properties, two_transform_generations) =
            synced_paint_state(&two_transform_arena, &two_transform_roots);
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedEffectTreeCanary,
                &two_transform_arena,
                &two_transform_roots,
                &two_transform_properties,
                &two_transform_generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::EffectTreeShapeRejected {
                transform_count: 2,
                effect_count: 0,
            }
        ));

        let (two_effect_arena, two_effect_roots, _, _, descendant) =
            prepared_transform_child_isolation_tree();
        crate::view::test_support::get_element_mut::<Element>(&two_effect_arena, descendant)
            .set_opacity(0.75);
        let (two_effect_properties, two_effect_generations) =
            synced_paint_state(&two_effect_arena, &two_effect_roots);
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedEffectTreeCanary,
                &two_effect_arena,
                &two_effect_roots,
                &two_effect_properties,
                &two_effect_generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::EffectTreeShapeRejected {
                transform_count: 1,
                effect_count: 2,
            }
        ));
    }

    #[test]
    fn retained_effect_tree_canary_is_not_selected_by_old_tree_or_isolation_modes() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let promoted = FxHashSet::default();
        let (arena, roots, _, _, _) = prepared_transform_child_isolation_tree();
        let (properties, generations) = synced_paint_state(&arena, &roots);

        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedSurfaceTreeCanary,
                &arena,
                &roots,
                &properties,
                &generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::TreeShapeRejected { transform_count: 1 }
        ));
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedIsolationCanary,
                &arena,
                &roots,
                &properties,
                &generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::IsolationPlanRejected(_)
        ));
    }

    #[test]
    fn retained_scroll_scene_canary_is_independent_and_rejects_before_baked_fallback() {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let promoted = FxHashSet::default();
        let (arena, roots) = prepared_safe_leaf();
        let (properties, generations) = synced_paint_state(&arena, &roots);

        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedScrollSceneCanary,
                &arena,
                &roots,
                &properties,
                &generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::ScrollSceneShapeRejected { scroll_count: 0 }
        ));
        assert!(matches!(
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedScrollHostCanary,
                &arena,
                &roots,
                &properties,
                &generations,
                &promoted,
                &ctx,
            ),
            RetainedTransformCanarySelection::ScrollHostShapeRejected { scroll_count: 0 }
        ));
    }

    #[test]
    fn retained_effect_tree_canary_plan_and_prepare_reject_without_graph_mutation() {
        let promoted = FxHashSet::default();
        let (arena, roots, _, _, _) = prepared_transform_child_isolation_tree();
        let (properties, generations) = synced_paint_state(&arena, &roots);

        let mut rejected_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        rejected_ctx.push_scissor_rect(Some([1, 2, 30, 40]));
        let selection_graph = FrameGraph::new();
        let selection_before = selection_graph.build_state_snapshot_for_test();
        let RetainedTransformCanarySelection::EffectTreePlanRejected(error) =
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedEffectTreeCanary,
                &arena,
                &roots,
                &properties,
                &generations,
                &promoted,
                &rejected_ctx,
            )
        else {
            panic!("outer scissor must reject the mixed plan")
        };
        assert!(
            error
                .reasons
                .contains(&crate::view::paint::FramePaintPlanRejection::IsolationOuterScissor)
        );
        assert_eq!(
            selection_graph.build_state_snapshot_for_test(),
            selection_before
        );

        let clean_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let RetainedTransformCanarySelection::EffectTreePlanned(plan) =
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedEffectTreeCanary,
                &arena,
                &roots,
                &properties,
                &generations,
                &promoted,
                &clean_ctx,
            )
        else {
            panic!("clean exact mixed fixture")
        };
        let mut graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        let mut execution_ctx = clean_ctx;
        execution_ctx.push_scissor_rect(Some([1, 2, 30, 40]));
        let mut viewport = Viewport::new();
        assert!(
            crate::view::paint::build_retained_effect_tree_from_pool(
                &mut viewport,
                &plan,
                &mut graph,
                execution_ctx,
            )
            .is_err()
        );
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, None)
        );
    }

    #[test]
    fn production_retained_effect_tree_canary_uses_pool_only_two_surface_authority() {
        let (arena, roots, root, child, _) = prepared_transform_child_isolation_tree();
        let (properties, generations) = synced_paint_state(&arena, &roots);
        let promoted = FxHashSet::default();
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        ctx.set_paint_offset([3.5, 2.25]);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: target,
            },
        ));
        ctx.set_current_target(target);
        let RetainedTransformCanarySelection::EffectTreePlanned(plan) =
            select_retained_transform_canary(
                ViewportPaintRendererMode::RetainedEffectTreeCanary,
                &arena,
                &roots,
                &properties,
                &generations,
                &promoted,
                &ctx,
            )
        else {
            panic!("eligible mixed frame must produce its owned production plan")
        };

        let mut viewport = Viewport::new();
        let outcome = crate::view::paint::build_retained_effect_tree_from_pool(
            &mut viewport,
            &plan,
            &mut graph,
            ctx,
        )
        .expect("production mixed canary dispatch");
        let (_, traces) = outcome.into_parts();
        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].boundary_root, root);
        assert_eq!(traces[1].boundary_root, child);
        assert!(traces.iter().all(|trace| {
            trace.action == crate::view::paint::RetainedSurfaceCompileAction::Reraster
                && trace.descriptor_size[0] > 0
                && trace.descriptor_size[1] > 0
                && trace.chunk_count > 0
                && trace.op_count > 0
        }));
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, Some(2)),
            "the canary stages the exact parent/child full set atomically"
        );
        viewport.finish_retained_surface_transaction(false);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, None),
            "failed compile/execute invalidates the complete staged set"
        );
    }

    #[test]
    fn production_retained_transform_orchestrator_uses_real_pool_authority() {
        let (arena, roots) = prepared_transform_leaf();
        let (properties, generations) = synced_paint_state(&arena, &roots);
        let promoted = FxHashSet::default();
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: target,
            },
        ));
        ctx.set_current_target(target);
        let RetainedTransformCanarySelection::Planned(plan) = select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedTransformCanary,
            &arena,
            &roots,
            &properties,
            &generations,
            &promoted,
            &ctx,
        ) else {
            panic!("eligible transform frame must produce an owned production plan");
        };

        let mut viewport = Viewport::new();
        let outcome = crate::view::paint::build_retained_surface_from_pool(
            &mut viewport,
            &plan,
            &mut graph,
            ctx,
        )
        .expect("production orchestrator must accept its exact plan");
        let (_, trace) = outcome.into_parts();
        assert_eq!(
            trace.action,
            crate::view::paint::RetainedSurfaceCompileAction::Reraster,
            "without a real resident GPU pair, pool-only authority must reraster"
        );
        assert_eq!(trace.boundary_root, roots[0]);
        assert_eq!(trace.descriptor_size, [80, 40]);
        assert!(trace.chunk_count > 0);
        assert!(trace.op_count > 0);
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .len(),
            2,
            "common clear plus retained-surface raster clear"
        );
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::render_pass::texture_composite_pass::TextureCompositePass>()
                .len(),
            1
        );
        viewport.finish_retained_surface_transaction(false);
    }

    struct TransparentContentsClipParent {
        id: u64,
        scissor: [u32; 4],
        children: Vec<NodeKey>,
    }

    impl Layoutable for TransparentContentsClipParent {
        fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
        fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
        fn measured_size(&self) -> (f32, f32) {
            (1.0, 1.0)
        }
        fn set_layout_width(&mut self, _width: f32) {}
        fn set_layout_height(&mut self, _height: f32) {}
    }

    impl EventTarget for TransparentContentsClipParent {}

    impl Renderable for TransparentContentsClipParent {
        fn build(
            &mut self,
            _graph: &mut FrameGraph,
            _arena: &mut NodeArena,
            ctx: UiBuildContext,
        ) -> BuildState {
            ctx.into_state()
        }
    }

    impl ElementTrait for TransparentContentsClipParent {
        fn stable_id(&self) -> u64 {
            self.id
        }

        fn box_model_snapshot(&self) -> BoxModelSnapshot {
            BoxModelSnapshot {
                node_id: self.id,
                parent_id: None,
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                border_radius: 0.0,
                should_render: true,
            }
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn shadow_paint_recording_capability(
            &self,
            _arena: &NodeArena,
            _deferred_phase_root: bool,
            _recording_context: crate::view::paint::PaintRecordingContext,
        ) -> ShadowPaintRecordingCapability {
            ShadowPaintRecordingCapability::Transparent
        }

        fn contents_logical_scissor(&self) -> Option<[u32; 4]> {
            Some(self.scissor)
        }

        fn children(&self) -> &[NodeKey] {
            &self.children
        }

        fn sync_children_mirror(&mut self, children: &[NodeKey]) {
            self.children.clear();
            self.children.extend_from_slice(children);
        }
    }

    fn prepared_contents_clipped_leaf() -> (NodeArena, Vec<NodeKey>) {
        let mut arena = new_test_arena();
        let parent = commit_element(
            &mut arena,
            Box::new(TransparentContentsClipParent {
                id: 0x8c20,
                scissor: [4, 6, 24, 18],
                children: Vec::new(),
            }),
        );
        let child = commit_child(
            &mut arena,
            parent,
            Box::new(colored_element(0x8c21, 10.0, Color::rgb(230, 20, 30))),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, child, measure, place);
        (arena, vec![parent])
    }

    fn prepared_outer_shadow_leaf(opacity: f32, blur: f32) -> (NodeArena, Vec<NodeKey>) {
        let mut element = colored_element(0x6d50, 10.25, Color::rgb(230, 20, 30));
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(230, 20, 30)),
        );
        style.set_box_shadow(vec![
            BoxShadow::new()
                .color(Color::rgb(20, 40, 220))
                .offset_x(2.0)
                .offset_y(3.0)
                .blur(blur),
        ]);
        element.apply_style(style);
        element.set_opacity(opacity);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(element));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        (arena, vec![root])
    }

    #[test]
    fn transition_relayout_keeps_resource_generation_frozen_until_next_frame_layout() {
        let pixels: Arc<[u8]> = Arc::from([10_u8, 20, 30, 255]);
        let source = ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: pixels.clone(),
        };
        let handle = image_resource::acquire_image_resource(&source);
        let asset_id = handle.asset_id();
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(Image::new_with_id(0x4d34, source)));
        let mut viewport = Viewport::new();
        viewport.logical_width = 100.0;
        viewport.logical_height = 100.0;
        viewport.scene.node_arena = arena;
        viewport.scene.ui_root_keys = vec![root];

        viewport.run_layout_pass();
        assert_eq!(
            viewport
                .scene
                .node_arena
                .get(root)
                .unwrap()
                .element
                .measured_size(),
            (1.0, 1.0)
        );

        image_resource::replace_ready_image_for_test(
            asset_id,
            2,
            1,
            Arc::from([40_u8, 50, 60, 255, 70, 80, 90, 255]),
        );
        viewport.run_relayout_pass();
        assert_eq!(
            viewport
                .scene
                .node_arena
                .get(root)
                .unwrap()
                .element
                .measured_size(),
            (1.0, 1.0),
            "the second layout pass in one frame must keep the first frozen snapshot"
        );

        viewport.run_layout_pass();
        assert_eq!(
            viewport
                .scene
                .node_arena
                .get(root)
                .unwrap()
                .element
                .measured_size(),
            (2.0, 1.0),
            "the next frame's first layout pass must refresh the resource snapshot"
        );
    }

    fn prepared_mixed_eligibility_roots() -> (NodeArena, Vec<NodeKey>) {
        let mut arena = new_test_arena();
        let safe_leaf = commit_element(
            &mut arena,
            Box::new(colored_element(10, 10.0, Color::rgb(230, 20, 30))),
        );
        let legacy_subtree = commit_element(
            &mut arena,
            Box::new(colored_element(20, 110.0, Color::rgb(20, 210, 40))),
        );
        commit_child(
            &mut arena,
            legacy_subtree,
            Box::new(colored_element(21, 10.0, Color::rgb(30, 40, 220))),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, safe_leaf, measure, place);
        measure_and_place(&mut arena, legacy_subtree, measure, place);
        (arena, vec![safe_leaf, legacy_subtree])
    }

    fn build_roots_graph(
        mut arena: NodeArena,
        roots: &[NodeKey],
        through_production_dispatch: bool,
    ) -> FrameGraph {
        if through_production_dispatch {
            return build_roots_graph_with_renderer_mode(
                arena,
                roots,
                ViewportPaintRendererMode::Legacy,
            );
        }
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        for &root_key in roots {
            let child_ctx = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
            let next_state = arena
                .with_element_taken(root_key, |root, arena| {
                    root.build(&mut graph, arena, child_ctx)
                })
                .expect("legacy root should exist");
            ctx.set_state(next_state);
        }
        graph
    }

    fn build_roots_graph_with_renderer_mode(
        mut arena: NodeArena,
        roots: &[NodeKey],
        mode: ViewportPaintRendererMode,
    ) -> FrameGraph {
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, roots, &properties);

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: target,
            },
        ));
        ctx.set_current_target(target);
        let root_effect_plan = roots.first().copied().and_then(|root| {
            (roots.len() == 1).then(|| {
                let key = crate::view::base_component::root_effect_stable_key(root);
                let desc = ctx.persistent_full_viewport_target_desc(key);
                RootEffectBuildPlan {
                    committed: RootEffectRetainedState::Invalid,
                    key,
                    target: crate::view::paint::RootEffectRasterInputs {
                        width: desc.width(),
                        height: desc.height(),
                        format: desc.format(),
                        sample_count: desc.sample_count(),
                        scale_factor_bits: ctx.viewport().scale_factor().to_bits(),
                    },
                    pair_resident: false,
                }
            })
        });
        let attempt = try_build_property_neutral_artifact_frame(
            &mut graph,
            &arena,
            roots,
            &properties,
            &generations,
            &FxHashSet::default(),
            mode,
            &ctx,
            root_effect_plan.as_ref(),
        );
        match attempt {
            PropertyNeutralArtifactAttempt::Compiled { state, .. } => ctx.set_state(state),
            PropertyNeutralArtifactAttempt::WholeFrameLegacy { .. }
            | PropertyNeutralArtifactAttempt::CompileRejected(_) => {
                for &root_key in roots {
                    let child_ctx = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
                    let next_state =
                        build_non_promoted_root_legacy(&mut graph, &mut arena, root_key, child_ctx);
                    ctx.set_state(next_state);
                }
            }
        }
        graph
    }

    fn artifact_canary_attempt(
        arena: &NodeArena,
        roots: &[NodeKey],
        promoted_node_ids: &FxHashSet<u64>,
    ) -> PropertyNeutralArtifactAttempt {
        let mut properties = PropertyTrees::default();
        properties.sync(arena, roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(arena, roots, &properties);
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        try_build_property_neutral_artifact_frame(
            &mut graph,
            arena,
            roots,
            &properties,
            &generations,
            promoted_node_ids,
            ViewportPaintRendererMode::ArtifactCanary,
            &ctx,
            None,
        )
    }

    fn preflight_fallback_reasons(
        arena: &NodeArena,
        roots: &[NodeKey],
        promoted_node_ids: &FxHashSet<u64>,
    ) -> Vec<crate::view::paint::FrameArtifactFallbackReason> {
        crate::view::paint::take_full_artifact_record_count();
        let attempt = artifact_canary_attempt(arena, roots, promoted_node_ids);
        let PropertyNeutralArtifactAttempt::WholeFrameLegacy { eligibility } = attempt else {
            panic!("unsupported production property must fall back during metadata preflight")
        };
        assert_eq!(
            crate::view::paint::take_full_artifact_record_count(),
            0,
            "metadata rejection must happen before every full hook",
        );
        eligibility.reasons
    }

    fn observe_compositor_state(
        arena: &NodeArena,
        roots: &[NodeKey],
        properties: &mut PropertyTrees,
        generations: &mut PaintGenerationTracker,
    ) {
        properties.sync(arena, roots);
        generations.sync(arena, roots, properties);
    }

    fn set_opacity_with_invalidation(arena: &mut NodeArena, key: NodeKey, opacity: f32) {
        arena
            .mutate_element_with_invalidation(key, |element, cx| {
                element
                    .as_any_mut()
                    .downcast_mut::<Element>()
                    .expect("test root should be Element")
                    .set_opacity_with_invalidation(opacity, cx);
            })
            .expect("test root should exist");
    }

    fn assert_consumed_dirty_cleared(arena: &NodeArena, key: NodeKey) {
        let consumed = DirtyFlags::PAINT.union(DirtyFlags::COMPOSITE);
        assert!(
            !arena
                .get(key)
                .expect("test node should exist")
                .element
                .local_dirty_flags()
                .intersects(consumed)
        );
        assert!(!arena.arena_local_dirty(key).intersects(consumed));
        assert!(!arena.cached_subtree_dirty(key).intersects(consumed));
    }

    fn assert_composite_dirty_preserved(arena: &NodeArena, key: NodeKey) {
        assert!(
            arena
                .get(key)
                .expect("test node should exist")
                .element
                .local_dirty_flags()
                .contains(DirtyFlags::COMPOSITE)
        );
        assert!(arena.arena_local_dirty(key).contains(DirtyFlags::COMPOSITE));
        assert!(
            arena
                .cached_subtree_dirty(key)
                .contains(DirtyFlags::COMPOSITE)
        );
    }

    #[test]
    fn production_safe_leaf_uses_direct_legacy_build_without_artifact_recording() {
        let (arena, roots) = prepared_safe_leaf();
        crate::view::paint::take_full_artifact_record_count();
        crate::view::paint::take_artifact_compile_count();
        let production_graph = build_roots_graph(arena, &roots, true);
        assert_eq!(
            crate::view::paint::take_full_artifact_record_count(),
            0,
            "production legacy authority must not invoke the full artifact recorder"
        );
        assert_eq!(crate::view::paint::take_artifact_compile_count(), 0);

        let (legacy_arena, legacy_roots) = prepared_safe_leaf();
        let direct_legacy_graph = build_roots_graph(legacy_arena, &legacy_roots, false);
        assert!(!production_graph.test_rect_pass_snapshots().is_empty());
        assert_eq!(
            production_graph.test_rect_pass_snapshots(),
            direct_legacy_graph.test_rect_pass_snapshots(),
            "production dispatch must preserve the direct legacy pass snapshot"
        );
    }

    #[test]
    fn production_artifact_canary_compiles_an_eligible_whole_frame() {
        let (arena, roots) = prepared_safe_leaf();
        crate::view::paint::take_full_artifact_record_count();
        crate::view::paint::take_artifact_compile_count();
        let artifact_graph = build_roots_graph_with_renderer_mode(
            arena,
            &roots,
            ViewportPaintRendererMode::ArtifactCanary,
        );
        assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
        assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);
        assert!(artifact_graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
            .is_empty(), "opacity=1 must stay on the direct M6A target");

        let (legacy_arena, legacy_roots) = prepared_safe_leaf();
        let legacy_graph = build_roots_graph(legacy_arena, &legacy_roots, false);
        assert_eq!(
            artifact_graph.test_rect_pass_snapshots(),
            legacy_graph.test_rect_pass_snapshots(),
            "the canary must preserve the eligible frame's pass semantics"
        );
    }

    #[test]
    fn production_artifact_canary_compiles_a_real_contents_clipped_frame() {
        let (arena, roots) = prepared_contents_clipped_leaf();
        crate::view::paint::take_full_artifact_record_count();
        crate::view::paint::take_artifact_compile_count();
        let mut graph = build_roots_graph_with_renderer_mode(
            arena,
            &roots,
            ViewportPaintRendererMode::ArtifactCanary,
        );
        assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
        assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);
        let snapshots = graph.test_rect_pass_snapshots();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].effective_scissor_rect, Some([4, 6, 24, 18]));
        assert!(
            graph.test_compile_snapshot().is_ok(),
            "clip-enabled artifact graph must compile strictly",
        );
    }

    #[test]
    fn production_clip_policy_rejects_every_non_clip_boundary_before_full_hooks() {
        let (arena, roots) = prepared_safe_leaf();
        let promoted = FxHashSet::from_iter([1]);
        let reasons = preflight_fallback_reasons(&arena, &roots, &promoted);
        assert!(
            reasons.contains(&crate::view::paint::FrameArtifactFallbackReason::PromotedBoundary)
        );

        let mut deferred = colored_element(0x8c30, 10.0, Color::rgb(230, 20, 30));
        let mut style = Style::new();
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(4.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        deferred.apply_style(style);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(deferred));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let reasons = preflight_fallback_reasons(&arena, &[root], &FxHashSet::default());
        assert!(reasons.contains(
            &crate::view::paint::FrameArtifactFallbackReason::LegacyBoundary(
                crate::view::paint::LegacyPaintReason::Deferred,
            ),
        ));

        let mut arena = new_test_arena();
        let effect = commit_element(
            &mut arena,
            Box::new(colored_element(0x8c31, 10.0, Color::rgb(230, 20, 30))),
        );
        arena
            .get_mut(effect)
            .expect("effect root")
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("Element")
            .set_opacity(0.5);
        let neutral = commit_element(
            &mut arena,
            Box::new(colored_element(0x8c32, 110.0, Color::rgb(20, 210, 40))),
        );
        measure_and_place(&mut arena, effect, measure, place);
        measure_and_place(&mut arena, neutral, measure, place);
        let reasons = preflight_fallback_reasons(&arena, &[effect, neutral], &FxHashSet::default());
        assert!(
            reasons.contains(
                &crate::view::paint::FrameArtifactFallbackReason::PropertyBoundary(effect),
            )
        );

        let mut transformed = colored_element(0x8c33, 10.0, Color::rgb(230, 20, 30));
        let mut style = Style::new();
        style.set_transform(Transform::new([Translate::x(Length::px(3.0))]));
        transformed.apply_style(style);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(transformed));
        measure_and_place(&mut arena, root, measure, place);
        let reasons = preflight_fallback_reasons(&arena, &[root], &FxHashSet::default());
        assert!(reasons.contains(
            &crate::view::paint::FrameArtifactFallbackReason::LegacyBoundary(
                crate::view::paint::LegacyPaintReason::Transform,
            ),
        ));

        let mut scroller = colored_element(0x8c34, 10.0, Color::rgb(230, 20, 30));
        let mut style = Style::new();
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        scroller.apply_style(style);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(scroller));
        measure_and_place(&mut arena, root, measure, place);
        let reasons = preflight_fallback_reasons(&arena, &[root], &FxHashSet::default());
        assert!(reasons.contains(
            &crate::view::paint::FrameArtifactFallbackReason::LegacyBoundary(
                crate::view::paint::LegacyPaintReason::ScrollContainer,
            ),
        ));
    }

    #[test]
    fn production_root_opacity_with_clip_records_and_compiles_once() {
        let mut clipped = colored_element(0x8c40, 10.0, Color::rgb(230, 20, 30));
        let mut style = Style::new();
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(4.0))
                    .top(Length::px(5.0))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        clipped.apply_style(style);
        clipped.set_opacity(0.5);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(clipped));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let roots = vec![root];

        crate::view::paint::take_full_artifact_record_count();
        crate::view::paint::take_artifact_compile_count();
        let graph = build_roots_graph_with_renderer_mode(
            arena,
            &roots,
            ViewportPaintRendererMode::ArtifactCanary,
        );
        assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
        assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);
        let rects = graph.test_rect_pass_snapshots();
        assert!(!rects.is_empty());
        assert!(rects.iter().all(|rect| {
            rect.opacity_bits == 1.0_f32.to_bits() && rect.effective_scissor_rect.is_some()
        }));
        let composites = graph.test_graphics_passes::<
            crate::view::render_pass::composite_layer_pass::CompositeLayerPass,
        >();
        assert_eq!(composites.len(), 1);
        assert_eq!(
            composites[0].test_params().opacity.to_bits(),
            0.5_f32.to_bits()
        );
    }

    #[test]
    fn production_artifact_canary_culls_hidden_parent_and_paintable_child() {
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(Element::new_with_id(0x8c41, 0.0, 0.0, 0.0, 10.0)),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        assert!(
            !arena
                .get(root)
                .unwrap()
                .element
                .box_model_snapshot()
                .should_render
        );
        let child = commit_child(
            &mut arena,
            root,
            Box::new(colored_element(0x8c42, 10.0, Color::rgb(20, 210, 40))),
        );
        measure_and_place(&mut arena, child, measure, place);
        assert!(
            arena
                .get(child)
                .unwrap()
                .element
                .box_model_snapshot()
                .should_render
        );
        let visible = commit_element(
            &mut arena,
            Box::new(colored_element(0x8c43, 110.0, Color::rgb(30, 60, 220))),
        );
        measure_and_place(&mut arena, visible, measure, place);
        let roots = vec![root, visible];

        crate::view::paint::take_full_artifact_record_count();
        crate::view::paint::take_artifact_compile_count();
        let graph = build_roots_graph_with_renderer_mode(
            arena,
            &roots,
            ViewportPaintRendererMode::ArtifactCanary,
        );
        assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
        assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);
        let rects = graph.test_rect_pass_snapshots();
        assert_eq!(rects.len(), 1, "only the visible sibling root may paint");
        assert_eq!(rects[0].position_bits[0], 110.0_f32.to_bits());
        assert!(
            graph
                .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
                .is_empty()
        );
    }

    #[test]
    fn production_artifact_canary_uses_one_root_group_composite_for_root_effect() {
        let (arena, roots) = prepared_safe_leaf();
        arena
            .get_mut(roots[0])
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .set_opacity(0.5);
        crate::view::paint::take_full_artifact_record_count();
        crate::view::paint::take_artifact_compile_count();
        let graph = build_roots_graph_with_renderer_mode(
            arena,
            &roots,
            ViewportPaintRendererMode::ArtifactCanary,
        );
        assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
        assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);
        let composites = graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>();
        assert_eq!(composites.len(), 1);
        assert_eq!(
            composites[0].test_params().opacity.to_bits(),
            0.5_f32.to_bits()
        );
        assert!(
            graph
                .test_rect_pass_snapshots()
                .iter()
                .all(|rect| rect.opacity_bits == 1.0_f32.to_bits())
        );
    }

    #[test]
    fn production_root_effect_second_opacity_only_frame_has_zero_raster_passes() {
        fn build(
            arena: &NodeArena,
            roots: &[NodeKey],
            committed: RootEffectRetainedState,
            pair_resident: bool,
        ) -> (FrameGraph, PendingRootEffectTransaction) {
            let mut properties = PropertyTrees::default();
            properties.sync(arena, roots);
            let mut generations = PaintGenerationTracker::default();
            generations.sync(arena, roots, &properties);
            let mut graph = FrameGraph::new();
            let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
            let target = ctx.allocate_target(&mut graph);
            ctx.set_current_target(target);
            graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: target,
                },
            ));
            let root = roots[0];
            let key = crate::view::base_component::root_effect_stable_key(root);
            let desc = ctx.persistent_full_viewport_target_desc(key);
            let plan = RootEffectBuildPlan {
                committed,
                key,
                target: crate::view::paint::RootEffectRasterInputs {
                    width: desc.width(),
                    height: desc.height(),
                    format: desc.format(),
                    sample_count: desc.sample_count(),
                    scale_factor_bits: ctx.viewport().scale_factor().to_bits(),
                },
                pair_resident,
            };
            let attempt = try_build_property_neutral_artifact_frame(
                &mut graph,
                arena,
                roots,
                &properties,
                &generations,
                &FxHashSet::default(),
                ViewportPaintRendererMode::ArtifactCanary,
                &ctx,
                Some(&plan),
            );
            let PropertyNeutralArtifactAttempt::Compiled {
                root_effect_transaction: Some(transaction),
                ..
            } = attempt
            else {
                panic!("root effect artifact should compile");
            };
            (graph, transaction)
        }

        let (mut arena, roots) = prepared_safe_leaf();
        set_opacity_with_invalidation(&mut arena, roots[0], 0.5);
        let (_first_graph, first_transaction) =
            build(&arena, &roots, RootEffectRetainedState::Invalid, false);
        let PendingRootEffectTransaction::Commit { stamp, key, .. } = first_transaction else {
            panic!("first frame must stage a retained commit");
        };

        set_opacity_with_invalidation(&mut arena, roots[0], 0.25);
        let (mut second_graph, second_transaction) = build(
            &arena,
            &roots,
            RootEffectRetainedState::Resident { stamp, key },
            true,
        );
        assert!(matches!(
            second_transaction,
            PendingRootEffectTransaction::Commit {
                action: crate::view::paint::RootEffectCompileAction::Reuse,
                ..
            }
        ));
        let snapshot = second_graph.test_compile_snapshot().unwrap();
        assert!(matches!(
            snapshot.pass_payloads(),
            [
                crate::view::frame_graph::FramePassTestPayload::Clear(_),
                crate::view::frame_graph::FramePassTestPayload::CompositeLayer(_)
            ]
        ));
    }

    #[test]
    fn production_artifact_canary_dispatches_outer_shadow_atomically_for_m6a_and_c1() {
        let (arena, roots) = prepared_outer_shadow_leaf(1.0, 0.0);
        crate::view::paint::take_full_artifact_record_count();
        crate::view::paint::take_artifact_compile_count();
        let m6a = build_roots_graph_with_renderer_mode(
            arena,
            &roots,
            ViewportPaintRendererMode::ArtifactCanary,
        );
        assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
        assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);
        assert!(m6a
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
            .is_empty());
        assert_eq!(
            m6a.test_graphics_passes::<crate::view::render_pass::shadow_module::ShadowFillPass>()
                .len(),
            1
        );

        let (arena, roots) = prepared_outer_shadow_leaf(0.4, 0.0);
        crate::view::paint::take_full_artifact_record_count();
        crate::view::paint::take_artifact_compile_count();
        let c1 = build_roots_graph_with_renderer_mode(
            arena,
            &roots,
            ViewportPaintRendererMode::ArtifactCanary,
        );
        assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
        assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);
        let composites = c1.test_graphics_passes::<
            crate::view::render_pass::composite_layer_pass::CompositeLayerPass,
        >();
        assert_eq!(composites.len(), 1);
        assert_eq!(
            composites[0].test_params().opacity.to_bits(),
            0.4_f32.to_bits()
        );
        let fills =
            c1.test_graphics_passes::<crate::view::render_pass::shadow_module::ShadowFillPass>();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].test_snapshot().color_bits[3], 1.0_f32.to_bits());

        let (arena, roots) = prepared_outer_shadow_leaf(1.0, 0.000_5);
        crate::view::paint::take_full_artifact_record_count();
        crate::view::paint::take_artifact_compile_count();
        let rejected = build_roots_graph_with_renderer_mode(
            arena,
            &roots,
            ViewportPaintRendererMode::ArtifactCanary,
        );
        assert_eq!(crate::view::paint::take_full_artifact_record_count(), 0);
        assert_eq!(crate::view::paint::take_artifact_compile_count(), 0);
        assert_eq!(
            rejected
                .test_graphics_passes::<crate::view::render_pass::shadow_module::ShadowFillPass>()
                .len(),
            1,
            "unsupported tiny blur must route the whole frame through legacy"
        );
    }

    #[test]
    fn production_artifact_canary_falls_back_the_entire_non_neutral_frame() {
        let (arena, roots) = prepared_mixed_eligibility_roots();
        arena
            .get_mut(roots[0])
            .expect("safe root exists")
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("safe root is Element")
            .set_opacity(0.5);
        crate::view::paint::take_full_artifact_record_count();
        crate::view::paint::take_artifact_compile_count();
        let canary_graph = build_roots_graph_with_renderer_mode(
            arena,
            &roots,
            ViewportPaintRendererMode::ArtifactCanary,
        );
        assert_eq!(
            crate::view::paint::take_full_artifact_record_count(),
            0,
            "a non-neutral reachable node must reject before every full hook"
        );
        assert_eq!(crate::view::paint::take_artifact_compile_count(), 0);

        let (legacy_arena, legacy_roots) = prepared_mixed_eligibility_roots();
        legacy_arena
            .get_mut(legacy_roots[0])
            .expect("safe root exists")
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("safe root is Element")
            .set_opacity(0.5);
        let legacy_graph = build_roots_graph(legacy_arena, &legacy_roots, false);
        assert_eq!(
            canary_graph.test_rect_pass_snapshots(),
            legacy_graph.test_rect_pass_snapshots(),
            "one property boundary must keep every root on legacy"
        );
    }

    #[test]
    fn viewport_paint_renderer_rollout_defaults_legacy_and_is_runtime_configurable() {
        let mut viewport = Viewport::new();
        assert_eq!(
            viewport.paint_renderer_mode(),
            ViewportPaintRendererMode::Legacy
        );
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::ArtifactCanary);
        assert_eq!(
            viewport.paint_renderer_mode(),
            ViewportPaintRendererMode::ArtifactCanary
        );
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedTransformCanary);
        assert_eq!(
            viewport.paint_renderer_mode(),
            ViewportPaintRendererMode::RetainedTransformCanary
        );
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedSurfaceTreeCanary);
        assert_eq!(
            viewport.paint_renderer_mode(),
            ViewportPaintRendererMode::RetainedSurfaceTreeCanary
        );
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedIsolationCanary);
        assert_eq!(
            viewport.paint_renderer_mode(),
            ViewportPaintRendererMode::RetainedIsolationCanary
        );
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedEffectTreeCanary);
        assert_eq!(
            viewport.paint_renderer_mode(),
            ViewportPaintRendererMode::RetainedEffectTreeCanary
        );
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedScrollHostCanary);
        assert_eq!(
            viewport.paint_renderer_mode(),
            ViewportPaintRendererMode::RetainedScrollHostCanary
        );
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedScrollSceneCanary);
        assert_eq!(
            viewport.paint_renderer_mode(),
            ViewportPaintRendererMode::RetainedScrollSceneCanary
        );
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedAuto);
        assert_eq!(
            viewport.paint_renderer_mode(),
            ViewportPaintRendererMode::RetainedAuto
        );
    }

    #[test]
    fn production_multi_root_frame_never_mixes_artifact_and_legacy_authority() {
        let (arena, roots) = prepared_mixed_eligibility_roots();
        crate::view::paint::take_full_artifact_record_count();
        let production_graph = build_roots_graph(arena, &roots, true);
        assert_eq!(
            crate::view::paint::take_full_artifact_record_count(),
            0,
            "safe roots must not record artifacts beside legacy-only roots"
        );

        let (legacy_arena, legacy_roots) = prepared_mixed_eligibility_roots();
        let direct_legacy_graph = build_roots_graph(legacy_arena, &legacy_roots, false);
        assert_eq!(
            production_graph.test_rect_pass_snapshots(),
            direct_legacy_graph.test_rect_pass_snapshots(),
            "every root in the frame must use the same direct legacy authority"
        );
    }

    #[test]
    fn successful_frame_clears_new_node_initial_composite_dirty() {
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(colored_element(30, 10.0, Color::rgb(230, 20, 30))),
        );
        let roots = vec![root];
        assert!(
            arena
                .get(root)
                .expect("root should exist")
                .element
                .local_dirty_flags()
                .contains(DirtyFlags::ALL),
            "a newly committed Element begins with all local work dirty"
        );
        arena.refresh_subtree_dirty_cache(root);
        assert!(
            arena
                .cached_subtree_dirty(root)
                .contains(DirtyFlags::COMPOSITE),
            "arena subtree aggregate must include the new Element's local composite bit"
        );

        let mut properties = PropertyTrees::default();
        let mut generations = PaintGenerationTracker::default();
        observe_compositor_state(&arena, &roots, &mut properties, &mut generations);
        assert!(properties.paint_state_for(root).is_some());
        assert!(generations.snapshot(root).is_some());

        finish_frame_dirty_lifecycle(&mut arena, &roots, true, true);
        assert_consumed_dirty_cleared(&arena, root);
    }

    #[test]
    fn opacity_composite_dirty_is_observed_in_frame_then_cleared_after_execute() {
        let (mut arena, roots) = prepared_safe_leaf();
        let root = roots[0];
        let mut properties = PropertyTrees::default();
        let mut generations = PaintGenerationTracker::default();
        observe_compositor_state(&arena, &roots, &mut properties, &mut generations);
        let initial_composite_revision = generations
            .snapshot(root)
            .expect("initial generation should exist")
            .composite_revision;
        finish_frame_dirty_lifecycle(&mut arena, &roots, true, true);

        set_opacity_with_invalidation(&mut arena, root, 0.5);
        assert_composite_dirty_preserved(&arena, root);
        observe_compositor_state(&arena, &roots, &mut properties, &mut generations);

        let paint_state = properties
            .paint_state_for(root)
            .expect("property state should be observed before build");
        let effect = paint_state
            .effect
            .expect("non-unit opacity should create an effect node");
        assert_eq!(
            properties.effects[&effect].opacity.to_bits(),
            0.5_f32.to_bits()
        );
        assert_ne!(
            generations
                .snapshot(root)
                .expect("updated generation should exist")
                .composite_revision,
            initial_composite_revision,
            "paint generation must consume this frame's effect change before dirty clear"
        );

        finish_frame_dirty_lifecycle(&mut arena, &roots, true, true);
        assert_consumed_dirty_cleared(&arena, root);
    }

    #[test]
    fn compile_or_execute_failure_preserves_composite_dirty() {
        for (compiled, executed) in [(false, false), (true, false)] {
            let (mut arena, roots) = prepared_safe_leaf();
            let root = roots[0];
            let mut properties = PropertyTrees::default();
            let mut generations = PaintGenerationTracker::default();
            observe_compositor_state(&arena, &roots, &mut properties, &mut generations);
            finish_frame_dirty_lifecycle(&mut arena, &roots, true, true);

            set_opacity_with_invalidation(&mut arena, root, 0.5);
            observe_compositor_state(&arena, &roots, &mut properties, &mut generations);
            finish_frame_dirty_lifecycle(&mut arena, &roots, compiled, executed);

            assert_composite_dirty_preserved(&arena, root);
            assert!(
                arena
                    .get(root)
                    .expect("root should exist")
                    .element
                    .local_dirty_flags()
                    .contains(DirtyFlags::PAINT),
                "failed frame must preserve the coupled paint work"
            );
        }
    }
}

/// Flatten a Fragment-at-root into its children so multi-root reconcile
/// sees the same arity as the arena (Fragment root → N arena roots).
/// Non-Fragment roots pass through as a single-element slice.
fn unpack_root_set(root: &crate::ui::RsxNode) -> Vec<&crate::ui::RsxNode> {
    match root {
        crate::ui::RsxNode::Fragment(frag) => frag.children.iter().collect(),
        other => vec![other],
    }
}
