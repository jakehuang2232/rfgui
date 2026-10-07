//! Layout-frame chunk geometry.
//!
//! A recorded chunk moves into the layout frame its property state names
//! (`layout_position`: the owner's own frame, or the nearest ancestor's) when
//! every coordinate of its bounds and payload identity survives the round
//! trip back to the frame origin bit for bit; otherwise it stays in viewport
//! space. The compiler places layout-frame chunks at the frame's derived
//! origin, which therefore reproduces the recorded viewport geometry exactly.
//!
//! Identity translation mirrors the compiler's op localization field for
//! field, so a translated identity equals the identity rebuilt from the
//! translated ops.
use super::*;

/// Live viewport origin of the layout frame a chunk's own property state
/// names. The chunk is recorded relative to it, and the compiler places it
/// back at the same frame's origin derived from the spatial graph. Without a
/// frame, the chunk stays in viewport space.
fn live_layout_frame_origin(
    arena: &crate::view::node_arena::NodeArena,
    properties: &PropertyTreeState,
) -> Option<[f32; 2]> {
    let bounds = arena
        .get(properties.layout_position?.0)?
        .element
        .box_model_snapshot();
    Some([bounds.x, bounds.y])
}

/// Translates one point. Returning `None` rejects the whole translation.
trait PointTranslation: Fn([u32; 2]) -> Option<[u32; 2]> {}
impl<F: Fn([u32; 2]) -> Option<[u32; 2]>> PointTranslation for F {}

fn translated_point_bits(bits: [u32; 2], delta: [f32; 2]) -> Option<[u32; 2]> {
    let point = [
        f32::from_bits(bits[0]) + delta[0],
        f32::from_bits(bits[1]) + delta[1],
    ];
    point
        .iter()
        .all(|value| value.is_finite())
        .then(|| point.map(f32::to_bits))
}

/// A translation that translating back by `-delta` undoes bit for bit.
fn exactly_translated_point_bits(bits: [u32; 2], delta: [f32; 2]) -> Option<[u32; 2]> {
    let translated = translated_point_bits(bits, delta)?;
    (translated_point_bits(translated, [-delta[0], -delta[1]])? == bits).then_some(translated)
}

/// A translated origin whose far corner, `origin + size`, must stay finite.
fn translated_extent_bits(
    point: &impl PointTranslation,
    origin_bits: [u32; 2],
    size_bits: [u32; 2],
) -> Option<[u32; 2]> {
    let origin = point(origin_bits)?;
    let [x, y] = origin.map(f32::from_bits);
    let [width, height] = size_bits.map(f32::from_bits);
    ((x + width).is_finite() && (y + height).is_finite()).then_some(origin)
}

fn translated_rect(point: &impl PointTranslation, rect: Rect) -> Option<Rect> {
    let [x, y] = point([rect.x, rect.y].map(f32::to_bits))?.map(f32::from_bits);
    Some(Rect { x, y, ..rect })
}

fn translated_all<T>(items: &[T], translate: impl Fn(&T) -> Option<T>) -> Option<Arc<[T]>> {
    items
        .iter()
        .map(translate)
        .collect::<Option<Vec<_>>>()
        .map(Arc::from)
}

impl PreparedDrawRectIdentity {
    fn translated(&self, point: &impl PointTranslation) -> Option<Self> {
        let mut translated = self.clone();
        translated.params.position_bits =
            translated_extent_bits(point, self.params.position_bits, self.params.size_bits)?;
        Some(translated)
    }
}

impl PreparedShadowIdentity {
    fn translated(&self, point: &impl PointTranslation) -> Option<Self> {
        let mut translated = self.clone();
        let [x, y] = point([self.shape_bits[0], self.shape_bits[1]])?;
        translated.shape_bits[0] = x;
        translated.shape_bits[1] = y;
        Some(translated)
    }
}

impl PreparedScrollbarAxisIdentity {
    fn translated(&self, point: &impl PointTranslation) -> Option<Self> {
        Some(Self {
            track_shadow: PreparedScrollbarShadowIdentity(self.track_shadow.0.translated(point)?),
            track: self.track.translated(point)?,
            thumb_shadow: PreparedScrollbarShadowIdentity(self.thumb_shadow.0.translated(point)?),
            thumb: self.thumb.translated(point)?,
        })
    }
}

impl PreparedScrollbarOverlayIdentity {
    fn translated(&self, point: &impl PointTranslation) -> Option<Self> {
        Some(Self {
            track_shadow: PreparedScrollbarShadowIdentity(self.track_shadow.0.translated(point)?),
            track: self.track.translated(point)?,
            thumb_shadow: PreparedScrollbarShadowIdentity(self.thumb_shadow.0.translated(point)?),
            thumb: self.thumb.translated(point)?,
            secondary: match &self.secondary {
                Some(axis) => Some(Box::new(axis.translated(point)?)),
                None => None,
            },
        })
    }
}

impl PreparedTextIdentity {
    /// Fragments move; glyph final positions are recomputed from the moved
    /// fragment origin exactly as op localization recomputes them, so they
    /// follow their fragment exactly whenever it moved exactly. Text that
    /// carries its own clip is not translatable.
    fn translated(&self, point: &impl PointTranslation) -> Option<Self> {
        if self.scissor_rect.is_some() || self.stencil_clip_id.is_some() {
            return None;
        }
        let fragments = translated_all(&self.fragments, |fragment| {
            Some(PreparedTextFragmentIdentity {
                origin_bits: point(fragment.origin_bits)?,
                ..*fragment
            })
        })?;
        let glyphs = translated_all(&self.glyphs, |glyph| {
            let origin = fragments
                .get(glyph.fragment_index as usize)?
                .origin_bits
                .map(f32::from_bits);
            let local = glyph.local_pos_bits.map(f32::from_bits);
            let position = [origin[0] + local[0], origin[1] + local[1]];
            position
                .iter()
                .all(|value| value.is_finite())
                .then(|| PreparedTextGlyphIdentity {
                    final_paint_pos_bits: position.map(f32::to_bits),
                    ..glyph.clone()
                })
        })?;
        Some(Self {
            glyphs,
            fragments,
            ..self.clone()
        })
    }
}

impl PreparedTextOp {
    /// This op translated by `delta`. Translation preserves every validated
    /// relation, so the frozen identity is translated alongside instead of
    /// re-derived from the glyph run.
    pub(crate) fn translated(&self, delta: [f32; 2]) -> Option<Self> {
        let identity = self
            .identity
            .translated(&|bits| translated_point_bits(bits, delta))?;
        let mut params = self.params.as_ref().clone();
        for (fragment, translated) in params.fragments.iter_mut().zip(identity.fragments.iter()) {
            fragment.origin = translated.origin_bits.map(f32::from_bits);
        }
        for (glyph, translated) in params
            .staging_input
            .glyphs
            .iter_mut()
            .zip(identity.glyphs.iter())
        {
            glyph.final_paint_pos = translated.final_paint_pos_bits.map(f32::from_bits);
        }
        let params = Arc::new(params);
        Some(Self {
            validated_params: params.clone(),
            params,
            uniform_opacity_bits: self.uniform_opacity_bits,
            identity,
        })
    }
}

impl PreparedImageIdentity {
    fn translated(&self, point: &impl PointTranslation) -> Option<Self> {
        let [x, y] = point([self.bounds_bits[0], self.bounds_bits[1]])?;
        Some(Self {
            bounds_bits: [x, y, self.bounds_bits[2], self.bounds_bits[3]],
            ..*self
        })
    }
}

impl PreparedSvgIdentity {
    fn translated(&self, point: &impl PointTranslation) -> Option<Self> {
        let [x, y] = point([self.bounds_bits[0], self.bounds_bits[1]])?;
        Some(Self {
            bounds_bits: [x, y, self.bounds_bits[2], self.bounds_bits[3]],
            ..*self
        })
    }
}

impl PreparedGpuIdentity {
    fn translated(&self, point: &impl PointTranslation) -> Option<Self> {
        let [x, y] = point([self.bounds[0], self.bounds[1]])?;
        Some(Self {
            bounds: [x, y, self.bounds[2], self.bounds[3]],
            ..self.clone()
        })
    }
}

impl PreparedInlineIfcRectIdentity {
    fn translated(&self, point: &impl PointTranslation) -> Option<Self> {
        Some(Self {
            position_bits: translated_extent_bits(point, self.position_bits, self.size_bits)?,
            ..self.clone()
        })
    }
}

impl PreparedInlineIfcDecorationIdentity {
    fn translated(&self, point: &impl PointTranslation) -> Option<Self> {
        Some(Self {
            fill: self.fill.translated(point)?,
            border: match &self.border {
                Some(border) => Some(border.translated(point)?),
                None => None,
            },
            ..self.clone()
        })
    }
}

impl PaintPayloadIdentity {
    /// The identity of this payload's commands translated by `delta`.
    pub(crate) fn translated(&self, delta: [f32; 2]) -> Option<Self> {
        self.translated_with(&|bits| translated_point_bits(bits, delta))
    }

    /// [`Self::translated`], or `None` unless translating back by `-delta`
    /// would restore every coordinate bit for bit.
    fn exactly_translated(&self, delta: [f32; 2]) -> Option<Self> {
        self.translated_with(&|bits| exactly_translated_point_bits(bits, delta))
    }

    fn translated_with(&self, point: &impl PointTranslation) -> Option<Self> {
        let rects = |rects: &Arc<[PreparedDrawRectIdentity]>| {
            translated_all(rects, |rect| rect.translated(point))
        };
        let shadows = |shadows: &Arc<[PreparedShadowIdentity]>| {
            translated_all(shadows, |shadow| shadow.translated(point))
        };
        Some(match self {
            Self::None => Self::None,
            Self::Image(image, decoration) => {
                Self::Image(image.translated(point)?, rects(decoration)?)
            }
            Self::ImageWithShadows(image, outer, decoration) => Self::ImageWithShadows(
                image.translated(point)?,
                shadows(outer)?,
                rects(decoration)?,
            ),
            Self::Gpu(gpu) => Self::Gpu(gpu.translated(point)?),
            Self::Svg(svg, decoration) => Self::Svg(svg.translated(point)?, rects(decoration)?),
            Self::SvgWithShadows(svg, outer, decoration) => {
                Self::SvgWithShadows(svg.translated(point)?, shadows(outer)?, rects(decoration)?)
            }
            Self::PreparedShadows(outer, decoration) => {
                Self::PreparedShadows(shadows(outer)?, rects(decoration)?)
            }
            Self::PreparedTexts(texts) => {
                Self::PreparedTexts(translated_all(texts, |text| text.translated(point))?)
            }
            Self::PreparedRects(decoration) => Self::PreparedRects(rects(decoration)?),
            Self::TextSelection(selection) => Self::TextSelection(TextSelectionPayloadIdentity {
                rects: rects(&selection.rects)?,
                ..selection.clone()
            }),
            Self::PreparedScrollbarOverlay(overlay) => {
                Self::PreparedScrollbarOverlay(Arc::new(overlay.translated(point)?))
            }
            Self::InlineIfcDecorations(outer, decorations) => Self::InlineIfcDecorations(
                shadows(outer)?,
                translated_all(decorations, |decoration| decoration.translated(point))?,
            ),
        })
    }
}

impl PaintChunkMetadata {
    /// This chunk in its owner's frame, or `None` when some coordinate does
    /// not return exactly to its recorded value at `origin`.
    fn in_layout_frame(&self, origin: [f32; 2]) -> Option<Self> {
        if self.frame != PaintChunkFrame::Viewport {
            return None;
        }
        let to_local = [-origin[0], -origin[1]];
        Some(Self {
            frame: PaintChunkFrame::Layout,
            bounds: translated_rect(
                &|bits| exactly_translated_point_bits(bits, to_local),
                self.bounds,
            )?,
            payload_identity: self.payload_identity.exactly_translated(to_local)?,
            ..self.clone()
        })
    }
}

/// Moves every chunk of one owner's metadata plan that converts exactly into
/// the layout frame its property state names.
pub(crate) fn metadata_plan_in_layout_frame(
    plan: PaintNodePlan<PaintChunkMetadata>,
    arena: &crate::view::node_arena::NodeArena,
) -> PaintNodePlan<PaintChunkMetadata> {
    let _profile = crate::view::paint::work_profile::scope("metadata_plan_in_layout_frame");
    let localize = |chunks: Vec<PaintChunkMetadata>| {
        chunks
            .into_iter()
            .map(|chunk| {
                live_layout_frame_origin(arena, &chunk.properties)
                    .and_then(|origin| chunk.in_layout_frame(origin))
                    .unwrap_or(chunk)
            })
            .collect()
    };
    PaintNodePlan {
        before_children: localize(plan.before_children),
        after_children: localize(plan.after_children),
    }
}

fn chunk_metadata(chunk: &PaintChunk) -> PaintChunkMetadata {
    PaintChunkMetadata {
        id: chunk.id,
        owner: chunk.owner,
        frame: chunk.frame,
        bounds: chunk.bounds,
        properties: chunk.properties,
        content_revision: chunk.content_revision,
        payload_identity: chunk.payload_identity.clone(),
    }
}

/// The command form of [`metadata_plan_in_layout_frame`]: each single-chunk
/// artifact takes exactly the frame its metadata takes, and its commands are
/// translated with it. `None` means a command disagrees with its own payload
/// identity after translation.
pub(crate) fn artifact_plan_in_layout_frame(
    plan: PaintNodePlan<PaintArtifact>,
    arena: &crate::view::node_arena::NodeArena,
) -> Option<PaintNodePlan<PaintArtifact>> {
    let _profile = crate::view::paint::work_profile::scope("artifact_plan_in_layout_frame");
    let localize = |artifacts: Vec<PaintArtifact>| -> Option<Vec<PaintArtifact>> {
        artifacts
            .into_iter()
            .map(|artifact| {
                let [chunk] = artifact.chunks.as_slice() else {
                    return Some(artifact);
                };
                let Some(origin) = live_layout_frame_origin(arena, &chunk.properties) else {
                    return Some(artifact);
                };
                let Some(local) = chunk_metadata(chunk).in_layout_frame(origin) else {
                    return Some(artifact);
                };
                let to_local = [-origin[0], -origin[1]];
                let ops = artifact
                    .ops
                    .iter()
                    .map(|op| super::super::compiler::localize_artifact_surface_op(op, to_local))
                    .collect::<Result<Vec<_>, _>>()
                    .ok()?;
                (local
                    .payload_identity
                    .rebuild_from_localized_ops(&ops)
                    .as_ref()
                    == Some(&local.payload_identity))
                .then_some(())?;
                let mut chunks = super::super::shared_sequence::SharedSequence::new();
                chunks.push(PaintChunk {
                    frame: PaintChunkFrame::Layout,
                    bounds: local.bounds,
                    payload_identity: local.payload_identity,
                    ..chunk.clone()
                });
                let mut local_ops = super::super::shared_sequence::SharedSequence::new();
                for op in ops {
                    local_ops.push(op);
                }
                Some(PaintArtifact {
                    chunks,
                    ops: local_ops,
                    ..artifact
                })
            })
            .collect()
    };
    Some(PaintNodePlan {
        before_children: localize(plan.before_children)?,
        after_children: localize(plan.after_children)?,
    })
}

impl PaintChunk {
    /// This chunk and its commands in viewport space: a layout-frame chunk
    /// first moves to its frame's `origin`, then every chunk takes its
    /// owner's pixel snap `snap`. The two translations stay separate so each
    /// coordinate is the recorded value plus the snap, exactly as the legacy
    /// renderer adds its paint offset.
    pub(crate) fn placed(
        &self,
        ops: impl IntoIterator<Item = PaintOp>,
        origin: Option<[f32; 2]>,
        snap: [f32; 2],
    ) -> Option<(Self, Vec<PaintOp>)> {
        if origin.is_some() != (self.frame == PaintChunkFrame::Layout) {
            return None;
        }
        let snap = (snap.map(f32::to_bits) != [0.0_f32.to_bits(); 2]).then_some(snap);
        let deltas = origin.into_iter().chain(snap);
        let ops = ops
            .into_iter()
            .map(|op| {
                deltas.clone().try_fold(op, |op, delta| {
                    super::super::compiler::localize_artifact_surface_op(&op, delta)
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        let mut bounds = self.bounds;
        let mut payload_identity = self.payload_identity.clone();
        for delta in deltas {
            bounds = translated_rect(&|bits| translated_point_bits(bits, delta), bounds)?;
            payload_identity = payload_identity.translated(delta)?;
        }
        Some((
            Self {
                frame: PaintChunkFrame::Viewport,
                bounds,
                payload_identity,
                ..self.clone()
            },
            ops,
        ))
    }
}
