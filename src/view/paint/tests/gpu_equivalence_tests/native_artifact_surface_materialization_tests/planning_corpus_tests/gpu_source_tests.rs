use super::super::style_pipeline_tests::read_submitted_texture;
use super::*;
use crate::view::base_component::{
    BoxModelSnapshot, BuildState, DirtyFlags, ElementTrait, EventTarget, LayoutConstraints,
    LayoutPlacement, Layoutable, PaintResourcePreparationContext, Renderable,
};
use crate::view::gpu_paint::{GpuPaintProgram, GpuPaintSource, GpuPaintSourceId};
use crate::view::viewport::ViewportPaintRendererMode;
use std::sync::Arc;
mod lifecycle_tests;
mod mixed_changes_tests;

const SHADER: &str = r#"
struct Uniform { color: vec4<f32> }
@group(0) @binding(0) var<uniform> u: Uniform;
@vertex fn vs_main(@location(0) p: vec2<f32>) -> @builtin(position) vec4<f32> { return vec4(p,0.,1.); }
@fragment fn fs_main() -> @location(0) vec4<f32> { return u.color; }
"#;
fn program() -> Arc<GpuPaintProgram> {
    static P: std::sync::OnceLock<Arc<GpuPaintProgram>> = std::sync::OnceLock::new();
    P.get_or_init(|| {
        GpuPaintProgram::new(
            SHADER.into(),
            8,
            wgpu::VertexStepMode::Vertex,
            Arc::from([wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            }]),
        )
        .unwrap()
    })
    .clone()
}
fn source(
    id: GpuPaintSourceId,
    revision: u64,
    size: [f32; 2],
    dpr: f32,
    color: [f32; 4],
) -> GpuPaintSource {
    let vertices: [f32; 12] = [-1., -1., 1., -1., -1., 1., -1., 1., 1., -1., 1., 1.];
    GpuPaintSource::new(
        id,
        revision,
        size.map(|n| (n * dpr).ceil() as u32),
        dpr,
        program(),
        Arc::from(bytemuck::cast_slice(&color)),
        Arc::from(bytemuck::cast_slice(&vertices)),
        6,
        1,
    )
    .unwrap()
}
struct GpuHost {
    id: u64,
    source_id: GpuPaintSourceId,
    position: [f32; 2],
    size: [f32; 2],
    revision: u64,
    color: [f32; 4],
    visible: bool,
    source: Option<GpuPaintSource>,
    dirty: DirtyFlags,
}
impl GpuHost {
    fn new(id: u64) -> Self {
        Self {
            id,
            source_id: GpuPaintSourceId::new(),
            position: [0.; 2],
            size: [20., 16.],
            revision: 1,
            color: [1., 0., 0., 1.],
            visible: true,
            source: None,
            dirty: DirtyFlags::ALL,
        }
    }
}
impl Layoutable for GpuHost {
    fn requires_paint_resource_preparation(&self) -> bool {
        true
    }
    fn prepare_paint_resources(&mut self, c: PaintResourcePreparationContext) {
        self.source = self.visible.then(|| {
            source(
                self.source_id,
                self.revision,
                self.size,
                c.device_scale,
                self.color,
            )
        });
    }
    fn measure(&mut self, _: LayoutConstraints, _: &mut NodeArena) {}
    fn place(&mut self, p: LayoutPlacement, _: &mut NodeArena) {
        self.position = [p.parent_x, p.parent_y];
    }
    fn measured_size(&self) -> (f32, f32) {
        (self.size[0], self.size[1])
    }
    fn set_layout_width(&mut self, n: f32) {
        self.size[0] = n;
    }
    fn set_layout_height(&mut self, n: f32) {
        self.size[1] = n;
    }
}
impl EventTarget for GpuHost {}
impl Renderable for GpuHost {
    fn build(
        &mut self,
        g: &mut FrameGraph,
        _: &mut NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        if let Some(s) = &self.source {
            s.paint(
                g,
                &mut ctx,
                [
                    self.position[0],
                    self.position[1],
                    self.size[0],
                    self.size[1],
                ],
            );
        }
        ctx.into_state()
    }
}
impl ElementTrait for GpuHost {
    fn stable_id(&self) -> u64 {
        self.id
    }
    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.id,
            parent_id: None,
            x: self.position[0],
            y: self.position[1],
            width: self.size[0],
            height: self.size[1],
            border_radius: 0.,
            should_render: self.visible,
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn prepared_gpu_paint_source(&self) -> Option<&GpuPaintSource> {
        self.source.as_ref()
    }
    fn retained_paint_signature(&self) -> u64 {
        self.revision
    }
    fn retained_paint_signature_is_complete(&self) -> bool {
        true
    }
    fn local_dirty_flags(&self) -> DirtyFlags {
        self.dirty
    }
    fn clear_local_dirty_flags(&mut self, f: DirtyFlags) {
        self.dirty = self.dirty.without(f);
    }
}
fn scene() -> (NodeArena, NodeKey, NodeKey, NodeKey) {
    let mut a = NodeArena::new();
    let root = commit_element(
        &mut a,
        Box::new(element(0x6e00, [80., 64.], style([80., 64.], None, None))),
    );
    let mut gpu_style = style([20., 16.], Some([4., 8.]), None);
    gpu_style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
    let group = commit_child(
        &mut a,
        root,
        Box::new(element(0x6e01, [20., 16.], gpu_style)),
    );
    let host = commit_child(&mut a, group, Box::new(GpuHost::new(0x6e02)));
    let mut native_style = style([16., 16.], Some([40., 8.]), Some(BLUE));
    native_style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
    let native = commit_child(
        &mut a,
        root,
        Box::new(element(0x6e03, [16., 16.], native_style)),
    );
    (a, root, host, native)
}
#[test]
fn gpu_source_recording_freezes_owner_payload_and_budget() {
    let (mut arena, root, host, _) = scene();
    let mut viewport = Viewport::new();
    crate::view::viewport::layout_artifact_style_scene_for_test(
        &mut viewport,
        &mut arena,
        root,
        [80., 64.],
    );
    arena.prepare_registered_paint_resources(PaintResourcePreparationContext {
        frame_number: 1,
        device_scale: 1.,
        now: crate::time::Instant::now(),
    });
    let (properties, generations) = sync_identity(&arena, &[root]);
    let FrameArtifactRecordOutcome::Artifact { artifact, .. } = record_surface_dag_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .unwrap() else {
        panic!("GPU source rejected")
    };
    assert_eq!(
        artifact.chunks.iter().filter(|c| c.owner == host).count(),
        1
    );
    assert!(matches!(
        &artifact
            .ops
            .iter()
            .find(|op| matches!(op, PaintOp::PreparedGpu(_))),
        Some(PaintOp::PreparedGpu(_))
    ));
    let chunk = artifact.chunks.iter().find(|c| c.owner == host).unwrap();
    let ctx = crate::view::paint::PaintRecordingContext {
        surface_dag: true,
        ..Default::default()
    };
    let owner = arena.get(host).unwrap();
    assert!(
        owner
            .element
            .record_shadow_paint_metadata(
                host,
                chunk.properties,
                chunk.content_revision,
                &arena,
                ctx
            )
            .is_some()
    );
    assert!(
        owner
            .element
            .record_shadow_paint_metadata(
                root,
                chunk.properties,
                chunk.content_revision,
                &arena,
                ctx
            )
            .is_none(),
        "GPU source cannot authorize a different live owner"
    );
    drop(owner);
    // The complete recorder froze metadata and payload independently. Mutating
    // only the op must be rejected even if the source ID remains unchanged.
    for drift in 0..3 {
        let mut changed = artifact.clone();
        let PaintOp::PreparedGpu(op) = changed
            .ops
            .iter_mut()
            .find(|op| matches!(op, PaintOp::PreparedGpu(_)))
            .unwrap()
        else {
            unreachable!()
        };
        match drift {
            0 => op.source.revision += 1,
            1 => op.source.extent[0] += 1,
            _ => op.source.uniforms = Arc::from([0; 16]),
        }
        assert!(matches!(prepare_artifact_surface_raster_plan(changed,ArtifactSurfaceRasterContext::new(1.,FORMAT,[0.;2],None,8192,128*1024*1024).unwrap()),Err(crate::view::paint::ArtifactSurfaceRasterPlanError::ArtifactProgram(crate::view::paint::SingleTargetSurfaceDagPrepareError::InvalidArtifactStore))),"unsealed payload drift {drift}");
    }
    prepare(artifact.clone(), 1., [0.; 2]);
    let budget = ArtifactSurfaceRasterContext::new(1., FORMAT, [0.; 2], None, 8192, 1280).unwrap();
    assert!(matches!(
        prepare_artifact_surface_raster_plan(artifact.clone(), budget),
        Err(crate::view::paint::ArtifactSurfaceRasterPlanError::GpuSourceBudgetExceeded)
    ));
    assert!(matches!(
        prepare_artifact_surface_raster_plan(
            artifact,
            ArtifactSurfaceRasterContext::new(2., FORMAT, [0.; 2], None, 8192, 128 * 1024 * 1024)
                .unwrap()
        ),
        Err(crate::view::paint::ArtifactSurfaceRasterPlanError::InvalidGpuSource)
    ));
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_gpu_source_updates_preserve_independent_native_raster() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for dpr in [1_u32, 2] {
            let (arena, root, host, _) = scene();
            let mut v = Viewport::new();
            v.set_paint_renderer_mode(mode);
            v.install_single_viewport_scene_for_test(arena, root);
            for frame in 0..4 {
                if frame == 2 {
                    let mut h = v.node_arena().get_mut(host).unwrap();
                    let h = h.element.as_any_mut().downcast_mut::<GpuHost>().unwrap();
                    h.color = [0., 1., 0., 1.];
                    h.revision += 1;
                    h.dirty = DirtyFlags::PAINT;
                }
                v.begin_offscreen_test_frame(
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    80 * dpr,
                    64 * dpr,
                    FORMAT,
                )?;
                v.set_scale_factor(dpr as f32);
                let observed = v.render_single_viewport_scene_for_test()?;
                let pixels = read_submitted_texture(&observed.texture, gpu, [80 * dpr, 64 * dpr])?;
                for ([x, y], expected) in [
                    (
                        [8, 12],
                        if frame < 2 {
                            [255, 0, 0, 128]
                        } else {
                            [0, 255, 0, 128]
                        },
                    ),
                    ([44, 12], [0, 0, 255, 128]),
                    ([28, 12], [0; 4]),
                ] {
                    let at = ((y * dpr * 80 * dpr + x * dpr) * 4) as usize;
                    assert!(
                        pixels[at..at + 4]
                            .iter()
                            .zip(expected)
                            .all(|(a, b)| a.abs_diff(b) <= 1),
                        "{mode:?} DPR={dpr} frame={frame} ({x},{y}) {:?} expected {expected:?}",
                        &pixels[at..at + 4]
                    );
                }
                if mode == ViewportPaintRendererMode::RetainedAuto {
                    assert!(observed.artifact_selected);
                    assert_eq!(observed.actions.len(), 2);
                    if frame % 2 == 1 {
                        assert!(
                            observed
                                .actions
                                .iter()
                                .all(|a| *a == RetainedSurfaceCompileAction::Reuse)
                        );
                    }
                    if frame == 2 {
                        assert_eq!(
                            observed
                                .actions
                                .iter()
                                .filter(|a| **a == RetainedSurfaceCompileAction::Reuse)
                                .count(),
                            1
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn gpu_source_conflicting_shared_producers_are_rejected_before_scheduling() {
    let (mut arena, root, host, _) = scene();
    let id = arena
        .get(host)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<GpuHost>()
        .unwrap()
        .source_id;
    let mut other = GpuHost::new(0x6e20);
    other.source_id = id;
    other.color = [0., 1., 0., 1.];
    commit_child(&mut arena, root, Box::new(other));
    let mut viewport = Viewport::new();
    crate::view::viewport::layout_artifact_style_scene_for_test(
        &mut viewport,
        &mut arena,
        root,
        [80., 64.],
    );
    arena.prepare_registered_paint_resources(PaintResourcePreparationContext {
        frame_number: 1,
        device_scale: 1.,
        now: crate::time::Instant::now(),
    });
    let (properties, generations) = sync_identity(&arena, &[root]);
    let FrameArtifactRecordOutcome::Artifact { artifact, .. } = record_surface_dag_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .unwrap() else {
        panic!("each source is individually well formed")
    };
    assert!(
        matches!(
            prepare_artifact_surface_raster_plan(
                artifact,
                ArtifactSurfaceRasterContext::new(
                    1.,
                    FORMAT,
                    [0.; 2],
                    None,
                    8192,
                    128 * 1024 * 1024
                )
                .unwrap()
            ),
            Err(crate::view::paint::ArtifactSurfaceRasterPlanError::InvalidGpuSource)
        ),
        "one ID cannot name conflicting writes, even if both consumer records are individually valid"
    );
}
