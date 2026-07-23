use super::*;

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
    NativeScrollForest,
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
    NativeScrollForestPlan {
        error: crate::view::paint::FramePaintPlanError,
    },
    PropertyBoundaryDagPlan {
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
            Self::NativeScrollForestPlan { error } => {
                format!("plan(native-scroll-forest):{:?}", error.reasons)
            }
            Self::PropertyBoundaryDagPlan { error } => {
                format!("plan(property-boundary-dag):{error:?}")
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
            Self::NativeScrollForest => "native-scroll-forest",
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
    NativeScrollForest,
    ScrollHost,
    ScrollScene,
}

impl PaintAuthorityKind {
    fn from_auto(authority: AutoAuthorityKind) -> Self {
        match authority {
            AutoAuthorityKind::PropertyScene => Self::PropertyScene,
            AutoAuthorityKind::NativeScrollForest => Self::NativeScrollForest,
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
            Self::NativeScrollForest => "native-scroll-forest",
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
    retained_surfaces: Vec<crate::view::paint::RetainedSurfaceBuildTrace>,
    legacy_debug_boundaries: Vec<crate::view::paint::FrameArtifactDebugBoundary>,
    legacy_boundary_owners: Vec<crate::view::node_arena::NodeKey>,
    resident_release_count: Option<usize>,
    detail: String,
}

impl PaintAuthorityTelemetry {
    fn from_selection(
        requested_mode: ViewportPaintRendererMode,
        selection: &RetainedTransformCanarySelection,
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
            scroll_content: None,
            retained_surfaces: Vec::new(),
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

    fn note_retained_surface(&mut self, trace: crate::view::paint::RetainedSurfaceBuildTrace) {
        self.retained_surfaces.push(trace);
    }

    fn note_retained_surfaces(&mut self, traces: &[crate::view::paint::RetainedSurfaceBuildTrace]) {
        self.retained_surfaces.extend_from_slice(traces);
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
        if self.requested_mode == ViewportPaintRendererMode::RetainedAuto {
            format!("retained-auto:{}", self.final_authority().label())
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
            "{} requested={:?} selected={} candidate-rejections=[{}] legacy-fallback-stage={} terminal-failure-stage={} scroll-content=[{}] resident-releases={} detail=[{}]",
            self.authority_label(),
            self.requested_mode,
            self.final_authority().label(),
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
            selected: self.final_authority(),
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

fn debug_requested_mode(
    mode: ViewportPaintRendererMode,
) -> crate::view::debug::DebugPaintRequestedMode {
    use crate::view::debug::DebugPaintRequestedMode as DebugMode;
    match mode {
        ViewportPaintRendererMode::Legacy => DebugMode::Legacy,
        ViewportPaintRendererMode::ArtifactCanary => DebugMode::ArtifactCanary,
        ViewportPaintRendererMode::RetainedTransformCanary => DebugMode::RetainedTransformCanary,
        ViewportPaintRendererMode::RetainedSurfaceTreeCanary => {
            DebugMode::RetainedSurfaceTreeCanary
        }
        ViewportPaintRendererMode::RetainedIsolationCanary => DebugMode::RetainedIsolationCanary,
        ViewportPaintRendererMode::RetainedEffectTreeCanary => DebugMode::RetainedEffectTreeCanary,
        ViewportPaintRendererMode::RetainedScrollHostCanary => DebugMode::RetainedScrollHostCanary,
        ViewportPaintRendererMode::RetainedScrollSceneCanary => {
            DebugMode::RetainedScrollSceneCanary
        }
        ViewportPaintRendererMode::RetainedAuto => DebugMode::RetainedAuto,
    }
}

fn debug_paint_authority(
    authority: PaintAuthorityKind,
) -> crate::view::debug::DebugFramePaintAuthority {
    use crate::view::debug::DebugFramePaintAuthority as DebugAuthority;
    match authority {
        PaintAuthorityKind::Legacy => DebugAuthority::Legacy,
        PaintAuthorityKind::Artifact => DebugAuthority::Artifact,
        PaintAuthorityKind::Transform => DebugAuthority::RetainedTransformSurface,
        PaintAuthorityKind::SurfaceTree
        | PaintAuthorityKind::Isolation
        | PaintAuthorityKind::EffectTree => DebugAuthority::RetainedEffectSurface,
        PaintAuthorityKind::PropertyScene => DebugAuthority::PropertyScene,
        PaintAuthorityKind::NativeScrollForest => DebugAuthority::NativeScrollForest,
        PaintAuthorityKind::ScrollHost => DebugAuthority::RetainedScrollHost,
        PaintAuthorityKind::ScrollScene => DebugAuthority::RetainedScrollScene,
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
    NativeScrollForest {
        plan: crate::view::paint::FramePaintPlan,
        trace: AutoAuthorityTrace,
    },
    PropertyBoundaryDagScene {
        scene: crate::view::paint::ValidatedPropertyBoundaryDagScene,
        trace: AutoAuthorityTrace,
    },
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
    FrameRootScrollScene {
        scene: crate::view::paint::ValidatedFrameRootScrollScene,
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
    NativeScrollForestPlanned(crate::view::paint::FramePaintPlan),
    NativeScrollForestPrepared,
    NativeScrollForestPrepareRejected(crate::view::paint::RetainedPropertyScrollScenePrepareError),
    PropertyScrollScenePlanned(crate::view::paint::ValidatedPropertyScrollScene),
    PropertyScrollScenePrepared,
    PropertyScrollScenePrepareRejected(crate::view::paint::RetainedPropertyScrollScenePrepareError),
    PropertyBoundaryDagScenePlanned(crate::view::paint::ValidatedPropertyBoundaryDagScene),
    PropertyBoundaryDagScenePrepared,
    PropertyBoundaryDagScenePrepareRejected(
        crate::view::paint::RetainedPropertyScrollScenePrepareError,
    ),
    FrameRootScrollScenePlanned(crate::view::paint::ValidatedFrameRootScrollScene),
    FrameRootScrollScenePrepared,
    FrameRootScrollScenePrepareRejected(
        crate::view::paint::RetainedPropertyScrollScenePrepareError,
    ),
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

fn preflight_frame_root_scroll_selection(
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
    let RetainedTransformCanarySelection::FrameRootScrollScenePlanned(scene) = selection else {
        return (selection, None);
    };
    let Some(frame_owner) = frame_owner else {
        return (
            RetainedTransformCanarySelection::FrameRootScrollScenePrepareRejected(
                crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable,
            ),
            None,
        );
    };
    match crate::view::paint::prepare_frame_root_scroll_scene(
        viewport,
        scene,
        graph,
        ctx,
        clear_rgba,
        frame_owner,
    ) {
        Ok(prepared) => (
            RetainedTransformCanarySelection::FrameRootScrollScenePrepared,
            Some(crate::view::paint::emit_prepared_frame_root_scroll_scene(
                prepared,
            )),
        ),
        Err(error) => (
            RetainedTransformCanarySelection::FrameRootScrollScenePrepareRejected(error),
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
) -> Result<RecordedArtifactCandidate, crate::view::paint::FrameArtifactEligibility> {
    // RetainedAuto owns and records the complete frame.
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
            property_trees,
            paint_generations,
            crate::view::paint::RendererMode::Auto,
        )
    } else {
        crate::view::paint::record_clip_enabled_frame_artifact(
            arena,
            roots,
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

fn is_exact_native_root_opacity_artifact(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
) -> bool {
    let [root] = roots else {
        return false;
    };
    if !property_trees.transforms.is_empty()
        || !property_trees.clips.is_empty()
        || !property_trees.scrolls.is_empty()
        || property_trees.effects.len() != 1
    {
        return false;
    }
    let Some(node) = arena.get(*root) else {
        return false;
    };
    let effect = crate::view::compositor::property_tree::EffectNodeId(*root);
    let exact_state = crate::view::compositor::property_tree::PropertyTreeState {
        effect: Some(effect),
        ..Default::default()
    };
    node.element.admits_exact_retained_root_opacity_artifact()
        && !node.element.is_deferred_to_root_viewport_render()
        && !node
            .element
            .placement_eligibility_metadata()
            .contains_runtime_layout_state
        && property_trees.effects.get(&effect).is_some_and(|snapshot| {
            snapshot.owner == *root
                && snapshot.parent.is_none()
                && snapshot.generation != 0
                && snapshot.opacity.is_finite()
                && (0.0..=1.0).contains(&snapshot.opacity)
        })
        && property_trees
            .node_state_for(*root)
            .is_some_and(|state| state.paint == exact_state && state.descendants == exact_state)
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
        if node.element.retained_paint_properties().is_scroll_container {
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
        match crate::view::paint::plan_and_validate_frame_root_scroll_scene(
            arena,
            roots,
            property_trees,
            paint_generations,
            viewport.scale_factor(),
            ctx.paint_offset(),
            ctx.graphics_pass_context().scissor_rect,
            viewport.target_format(),
        ) {
            Ok(scene) => return AutoAuthorityDecision::FrameRootScrollScene { scene, trace },
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::PropertyScrollPlan { error });
            }
        }
        match crate::view::paint::plan_and_validate_property_scroll_scene(
            arena,
            roots,
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
        if scrolls >= 2 {
            let plan_context = crate::view::paint::TransformSurfacePlanContext::new(
                ctx.paint_offset(),
                ctx.graphics_pass_context().scissor_rect,
            );
            match crate::view::paint::plan_native_scroll_forest_scaffold_with_context(
                arena,
                roots,
                property_trees,
                paint_generations,
                viewport.scale_factor(),
                plan_context,
            ) {
                Ok(plan) => {
                    return AutoAuthorityDecision::NativeScrollForest { plan, trace };
                }
                Err(error) => {
                    trace.capture(|| AutoAuthorityRejection::NativeScrollForestPlan { error });
                }
            }
        }
        match crate::view::paint::PropertyBoundaryDagCompiler::plan_and_validate(
            arena,
            roots,
            property_trees,
            paint_generations,
            viewport.scale_factor(),
            ctx.paint_offset(),
            ctx.graphics_pass_context().scissor_rect,
            semantic_frame_time,
            viewport.target_format(),
            scroll_budget,
        ) {
            Ok(scene) => return AutoAuthorityDecision::PropertyBoundaryDagScene { scene, trace },
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::PropertyBoundaryDagPlan { error });
            }
        }
        return match crate::view::paint::plan_and_validate_direct_scroll_transform_scene(
            arena,
            roots,
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
        // Native hosts explicitly admitted by ElementTrait use the existing
        // host-generic root-opacity artifact grammar. The full tree/property
        // witness and metadata/full-artifact pair remain authoritative, so a
        // resource, topology, property, or generation drift still fails
        // closed before emission.
        if is_exact_native_root_opacity_artifact(arena, roots, property_trees) {
            match record_auto_artifact_candidate(arena, roots, property_trees, paint_generations) {
                Ok(candidate) => return AutoAuthorityDecision::Artifact { candidate, trace },
                Err(eligibility) => {
                    trace.capture(|| AutoAuthorityRejection::Artifact { eligibility });
                }
            }
        }
        let plan_context = crate::view::paint::TransformSurfacePlanContext::new(
            ctx.paint_offset(),
            ctx.graphics_pass_context().scissor_rect,
        );
        return match crate::view::paint::plan_property_effect_scene_with_context(
            arena,
            roots,
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

    match record_auto_artifact_candidate(arena, roots, property_trees, paint_generations) {
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
    ctx: &crate::view::base_component::UiBuildContext,
    semantic_frame_time: crate::time::Instant,
    scroll_budget: crate::view::paint::ScrollSceneSingleTextureBudget,
    capture_auto_trace: bool,
) -> RetainedTransformCanarySelection {
    // Named retained canaries, like RetainedAuto, own their whole frame.
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
            property_trees,
            paint_generations,
            recorder_mode,
        )
    } else {
        crate::view::paint::record_clip_enabled_frame_artifact(
            arena,
            roots,
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
        if self.debug_options.retained_auto_reuse_actions {
            records.extend(telemetry.retained_surfaces.iter().map(|surface| {
                let color = match surface.action {
                    crate::view::paint::RetainedSurfaceCompileAction::Reuse => {
                        [38.0 / 255.0, 242.0 / 255.0, 90.0 / 255.0, 242.0 / 255.0]
                    }
                    crate::view::paint::RetainedSurfaceCompileAction::Reraster => {
                        [1.0, 115.0 / 255.0, 26.0 / 255.0, 242.0 / 255.0]
                    }
                };
                (surface.boundary_root, color, None)
            }));
            if telemetry.retained_surfaces.is_empty()
                && let (Some(scroll), Some(&root)) = (telemetry.scroll_content, roots.first())
            {
                let color = if scroll.reraster_count > 0 {
                    [1.0, 115.0 / 255.0, 26.0 / 255.0, 242.0 / 255.0]
                } else {
                    [38.0 / 255.0, 242.0 / 255.0, 90.0 / 255.0, 242.0 / 255.0]
                };
                records.push((root, color, None));
            }
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
            DebugResidentAction as Action, DebugSurfaceKind as SurfaceKind,
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
            PaintAuthorityKind::PropertyScene => Coverage::PropertySurface,
            PaintAuthorityKind::NativeScrollForest => Coverage::RetainedSurface,
            PaintAuthorityKind::Transform
            | PaintAuthorityKind::SurfaceTree
            | PaintAuthorityKind::Isolation
            | PaintAuthorityKind::EffectTree
            | PaintAuthorityKind::ScrollHost
            | PaintAuthorityKind::ScrollScene => Coverage::RetainedSurface,
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

        let mut surfaces = Vec::new();
        for trace in &telemetry.retained_surfaces {
            let Some((stable_id, element_type, bounds)) =
                self.retained_auto_debug_identity(trace.boundary_root)
            else {
                continue;
            };
            let action = match trace.action {
                crate::view::paint::RetainedSurfaceCompileAction::Reuse => Action::Reuse,
                crate::view::paint::RetainedSurfaceCompileAction::Reraster => Action::Reraster,
            };
            let properties = self
                .compositor
                .property_trees
                .paint_state_for(trace.boundary_root)
                .unwrap_or_default();
            let kind = if properties.scroll.is_some() {
                SurfaceKind::ScrollHost
            } else if properties.effect.is_some() {
                SurfaceKind::Effect
            } else {
                SurfaceKind::Transform
            };
            surfaces.push(crate::view::debug::DebugRetainedAutoSurfaceCaptureInput {
                owner: Some(trace.boundary_root),
                stable_id: Some(stable_id),
                element_type,
                bounds: Some(bounds),
                kind,
                coverage: Coverage::RetainedSurface,
                resident_action: action,
            });
            let node = nodes.entry(trace.boundary_root).or_insert_with(|| {
                crate::view::debug::DebugRetainedAutoNodeCaptureInput {
                    owner: Some(trace.boundary_root),
                    stable_id: Some(stable_id),
                    element_type,
                    bounds: Some(bounds),
                    coverage: Vec::new(),
                    resident_action: None,
                    fallbacks: Vec::new(),
                }
            });
            if !node.coverage.contains(&Coverage::RetainedSurface) {
                node.coverage.push(Coverage::RetainedSurface);
            }
            node.resident_action = Some(action);
        }

        if telemetry.retained_surfaces.is_empty()
            && let (Some(scroll), Some(&owner)) = (telemetry.scroll_content, roots.first())
            && let Some((stable_id, element_type, bounds)) =
                self.retained_auto_debug_identity(owner)
        {
            let action = if scroll.reraster_count > 0 {
                Action::Reraster
            } else if scroll.reuse_count > 0 {
                Action::Reuse
            } else {
                Action::None
            };
            surfaces.push(crate::view::debug::DebugRetainedAutoSurfaceCaptureInput {
                owner: Some(owner),
                stable_id: Some(stable_id),
                element_type,
                bounds: Some(bounds),
                kind: SurfaceKind::ScrollContent,
                coverage: Coverage::RetainedSurface,
                resident_action: action,
            });
            if let Some(node) = nodes.get_mut(&owner) {
                if !node.coverage.contains(&Coverage::RetainedSurface) {
                    node.coverage.push(Coverage::RetainedSurface);
                }
                node.resident_action = Some(action);
            }
        }

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
            .filter(|surface| surface.resident_action == Action::Reuse)
            .count() as u64;
        let resident_rerasterizations = surfaces
            .iter()
            .filter(|surface| surface.resident_action == Action::Reraster)
            .count() as u64;
        let mut nodes = nodes.into_iter().collect::<Vec<_>>();
        nodes.sort_unstable_by_key(|(owner, _)| *owner);
        let nodes = nodes.into_iter().map(|(_, node)| node).collect::<Vec<_>>();
        let statistics = crate::view::debug::DebugRetainedAutoStatistics {
            reachable_nodes: self.scene.node_arena.len() as u64,
            covered_nodes: nodes.len() as u64,
            artifact_chunks: u64::from(final_authority == PaintAuthorityKind::Artifact),
            property_surfaces: if final_authority == PaintAuthorityKind::PropertyScene {
                surfaces.len() as u64
            } else {
                0
            },
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

        // Observe the final resolved frame state after transition sampling
        // and any required relayout.  These shadow trees do not yet drive
        // rendering or dirty classification.
        self.sync_compositor_property_trees();

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
        let retained_surface_frame_owner = self.begin_retained_surface_frame_stage();
        let root_keys_for_build = self.scene.ui_root_keys.clone();
        let capture_paint_authority_telemetry = self.debug_options.trace_render_time
            || self.debug_options.retained_auto_overlay
            || paint_authority_test_capture_enabled();
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
                &ctx,
                semantic_now,
                property_scroll_budget,
                capture_paint_authority_telemetry,
            )
        });
        let (mut retained_transform_selection, auto_authority_trace) =
            match retained_transform_selection {
                RetainedTransformCanarySelection::Auto(decision) => match decision {
                    AutoAuthorityDecision::NativeScrollForest { plan, trace } => (
                        RetainedTransformCanarySelection::NativeScrollForestPlanned(plan),
                        Some((AutoAuthorityKind::NativeScrollForest, trace)),
                    ),
                    AutoAuthorityDecision::PropertyBoundaryDagScene { scene, trace } => (
                        RetainedTransformCanarySelection::PropertyBoundaryDagScenePlanned(scene),
                        Some((AutoAuthorityKind::PropertyScene, trace)),
                    ),
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
                    AutoAuthorityDecision::FrameRootScrollScene { scene, trace } => (
                        RetainedTransformCanarySelection::FrameRootScrollScenePlanned(scene),
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
        let native_scroll_forest_owner = if matches!(
            retained_transform_selection,
            RetainedTransformCanarySelection::NativeScrollForestPlanned(_)
        ) {
            let selection = std::mem::replace(
                &mut retained_transform_selection,
                RetainedTransformCanarySelection::NativeScrollForestPrepared,
            );
            let RetainedTransformCanarySelection::NativeScrollForestPlanned(plan) = selection
            else {
                unreachable!("native forest preflight extracts only its owned plan")
            };
            Some(plan)
        } else {
            None
        };
        let mut pre_emitted_native_scroll_forest = None;
        if let Some(plan) = native_scroll_forest_owner {
            if retained_surface_frame_owner.is_none() {
                retained_transform_selection =
                    RetainedTransformCanarySelection::NativeScrollForestPrepareRejected(
                        crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable,
                    );
            } else {
                match crate::view::paint::prepare_native_scroll_forest_transaction_from_pool(
                    self,
                    &plan,
                    self.offscreen_format(),
                ) {
                    Ok(prepared) => {
                        let mut forest_ctx =
                            crate::view::base_component::UiBuildContext::from_parts(
                                ctx.viewport(),
                                ctx.state_clone(),
                            );
                        let output = forest_ctx.allocate_target(&mut graph);
                        forest_ctx.set_current_target(output);
                        graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
                            crate::view::render_pass::clear_pass::ClearParams::new(clear_rgba),
                            crate::view::render_pass::clear_pass::ClearInput {
                                pass_context: forest_ctx.graphics_pass_context(),
                                clear_depth_stencil: true,
                            },
                            crate::view::render_pass::clear_pass::ClearOutput {
                                render_target: output,
                            },
                        ));
                        if let Some(handle) = output.handle() {
                            forest_ctx.set_color_target(Some(handle));
                        }
                        pre_emitted_native_scroll_forest = Some(
                            crate::view::paint::emit_prepared_native_scroll_forest_transaction(
                                self, &mut graph, forest_ctx, prepared,
                            ),
                        );
                    }
                    Err(error) => {
                        retained_transform_selection =
                            RetainedTransformCanarySelection::NativeScrollForestPrepareRejected(
                                error,
                            );
                    }
                }
            }
        }
        let property_boundary_dag_owner = if matches!(
            retained_transform_selection,
            RetainedTransformCanarySelection::PropertyBoundaryDagScenePlanned(_)
        ) {
            let selection = std::mem::replace(
                &mut retained_transform_selection,
                RetainedTransformCanarySelection::PropertyBoundaryDagScenePrepared,
            );
            let RetainedTransformCanarySelection::PropertyBoundaryDagScenePlanned(scene) =
                selection
            else {
                unreachable!("boundary-DAG preflight extracts only its owned scene")
            };
            Some(scene)
        } else {
            None
        };
        let mut pre_emitted_property_boundary_dag = None;
        if let Some(scene) = property_boundary_dag_owner {
            let scroll_ctx = crate::view::base_component::UiBuildContext::from_parts(
                ctx.viewport(),
                ctx.state_clone(),
            );
            match retained_surface_frame_owner {
                Some(frame_owner) => {
                    match crate::view::paint::prepare_property_boundary_dag_scene_from_pool(
                        self,
                        scene,
                        &mut graph,
                        scroll_ctx,
                        clear_rgba,
                        frame_owner,
                    ) {
                        Ok(prepared) => {
                            pre_emitted_property_boundary_dag = Some(
                                crate::view::paint::emit_prepared_property_boundary_dag_scene(
                                    prepared,
                                ),
                            );
                        }
                        Err(error) => {
                            retained_transform_selection = RetainedTransformCanarySelection::
                                PropertyBoundaryDagScenePrepareRejected(error);
                        }
                    }
                }
                None => {
                    retained_transform_selection = RetainedTransformCanarySelection::
                        PropertyBoundaryDagScenePrepareRejected(
                            crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable,
                        );
                }
            }
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
        let (selection, mut pre_emitted_frame_root_scroll) = if matches!(
            retained_transform_selection,
            RetainedTransformCanarySelection::FrameRootScrollScenePlanned(_)
        ) {
            let selection = std::mem::replace(
                &mut retained_transform_selection,
                RetainedTransformCanarySelection::FrameRootScrollScenePrepared,
            );
            let scroll_ctx = crate::view::base_component::UiBuildContext::from_parts(
                ctx.viewport(),
                ctx.state_clone(),
            );
            preflight_frame_root_scroll_selection(
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
                    | RetainedTransformCanarySelection::NativeScrollForestPlanned(_)
                    | RetainedTransformCanarySelection::NativeScrollForestPrepared
                    | RetainedTransformCanarySelection::PropertyScrollScenePlanned(_)
                    | RetainedTransformCanarySelection::PropertyScrollScenePrepared
                    | RetainedTransformCanarySelection::PropertyScrollScenePrepareRejected(_)
                    | RetainedTransformCanarySelection::PropertyBoundaryDagScenePlanned(_)
                    | RetainedTransformCanarySelection::PropertyBoundaryDagScenePrepared
                    | RetainedTransformCanarySelection::FrameRootScrollScenePlanned(_)
                    | RetainedTransformCanarySelection::FrameRootScrollScenePrepared
                    | RetainedTransformCanarySelection::FrameRootScrollScenePrepareRejected(_)
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
                    RetainedTransformCanarySelection::NativeScrollForestPrepareRejected(_) => {
                        Some(PaintAuthorityFallbackStage::Prepare)
                    }
                    RetainedTransformCanarySelection::TransformEffectScrollScenePrepareRejected(
                        _,
                    ) => Some(transform_effect_scroll_prepare_rejection_fallback_stage()),
                    RetainedTransformCanarySelection::PropertyBoundaryDagScenePrepareRejected(
                        _,
                    ) => Some(PaintAuthorityFallbackStage::Prepare),
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
        if pre_emitted_property_boundary_dag.is_none()
            && pre_emitted_native_scroll_forest.is_none()
            && pre_emitted_nested_scroll.is_none()
            && pre_emitted_direct_scroll_transform.is_none()
            && pre_emitted_frame_root_scroll.is_none()
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
                            if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                                telemetry.note_retained_surface(trace);
                            }
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
                    if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                        telemetry.note_retained_surfaces(&trace.surfaces);
                    }
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
                RetainedTransformCanarySelection::NativeScrollForestPrepared => {
                    let state = pre_emitted_native_scroll_forest
                        .take()
                        .expect("prepared native forest emitted under its joint lease");
                    ctx.set_state(state);
                    self.stage_root_effect_clear();
                    (
                        false,
                        "retained-auto authority=native-scroll-forest phase=arbitrary-native-scroll-forest"
                            .to_owned(),
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
                RetainedTransformCanarySelection::PropertyBoundaryDagScenePrepared => {
                    let outcome = pre_emitted_property_boundary_dag
                        .take()
                        .expect("prepared boundary-DAG selection emitted under its lease");
                    let (state, trace) = outcome.into_parts();
                    ctx.set_state(state);
                    if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                        telemetry.note_property_scroll_content(&trace);
                    }
                    self.stage_root_effect_clear();
                    (
                        false,
                        format!(
                            "retained-auto authority=property-scene phase=property-boundary-dag roots={} generic-surfaces={} effect-surfaces={} scroll-groups={} backing={:?} tiles={} pair-bytes={} reraster={} reuse={}",
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
                RetainedTransformCanarySelection::FrameRootScrollScenePrepared => {
                    let outcome = pre_emitted_frame_root_scroll
                        .take()
                        .expect("prepared frame-root scroll selection emitted under its lease");
                    let (state, trace) = outcome.into_parts();
                    ctx.set_state(state);
                    if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                        telemetry.note_property_scroll_content(&trace);
                    }
                    self.stage_root_effect_clear();
                    (
                        false,
                        format!(
                            "retained-auto authority=property-scene phase=frame-root-scroll roots={} scroll-groups={} backing={:?} tiles={} pair-bytes={} reraster={} reuse={}",
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
                RetainedTransformCanarySelection::NativeScrollForestPrepareRejected(error) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-auto authority=legacy native-scroll-forest-prepare-rejected={error:?}"
                        ),
                    )
                }
                RetainedTransformCanarySelection::PropertyBoundaryDagScenePrepareRejected(
                    error,
                ) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-auto authority=legacy property-boundary-dag-prepare-rejected={error:?}"
                        ),
                    )
                }
                RetainedTransformCanarySelection::FrameRootScrollScenePrepareRejected(error) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        format!(
                            "retained-auto authority=legacy frame-root-scroll-prepare-rejected={error:?}"
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
                RetainedTransformCanarySelection::NativeScrollForestPlanned(_) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        "retained-auto authority=legacy native-scroll-forest-preflight-missing"
                            .to_owned(),
                    )
                }
                RetainedTransformCanarySelection::PropertyBoundaryDagScenePlanned(_) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        "retained-auto authority=legacy property-boundary-dag-preflight-missing"
                            .to_owned(),
                    )
                }
                RetainedTransformCanarySelection::FrameRootScrollScenePlanned(_) => {
                    self.stage_retained_surface_clear();
                    self.stage_root_effect_clear();
                    (
                        true,
                        "retained-auto authority=legacy frame-root-scroll-preflight-missing"
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
                            if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                                telemetry.note_retained_surfaces(&traces);
                            }
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
                            if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                                telemetry.note_retained_surface(trace);
                            }
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
                            if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                                telemetry.note_retained_surfaces(&traces);
                            }
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
                            if let Some(telemetry) = paint_authority_telemetry.as_mut() {
                                telemetry.note_retained_surface(trace);
                            }
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
mod legacy_root_render_tests;

/// Flatten a Fragment-at-root into its children so multi-root reconcile
/// sees the same arity as the arena (Fragment root → N arena roots).
/// Non-Fragment roots pass through as a single-element slice.
fn unpack_root_set(root: &crate::ui::RsxNode) -> Vec<&crate::ui::RsxNode> {
    match root {
        crate::ui::RsxNode::Fragment(frag) => frag.children.iter().collect(),
        other => vec![other],
    }
}
