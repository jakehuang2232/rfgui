//! Immutable, engine-scheduled GPU paint. A source owns only its declared
//! offscreen output. Scene clips, transforms and effects belong to the compiler.
//! The initial protocol supports one triangle-list draw, one vertex stream and
//! one uniform block. External textures, queues, callbacks and storage writes
//! are deliberately outside this protocol: they need explicit dependencies.
use std::sync::Arc;

/// Process-unique source namespace, independent of any consumer's node ID.
/// Copy this identity when multiple paint owners sample the same producer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GpuPaintSourceId(u64);
impl Default for GpuPaintSourceId {
    fn default() -> Self {
        Self::new()
    }
}
impl GpuPaintSourceId {
    pub fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |n| n.checked_add(1),
            )
            .expect("GPU paint source identity exhausted");
        Self(id)
    }
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuPaintProgram {
    pub(crate) shader: Arc<str>,
    pub(crate) stride: u64,
    pub(crate) step: wgpu::VertexStepMode,
    pub(crate) attributes: Arc<[wgpu::VertexAttribute]>,
    pub(crate) uniform_size: u64,
}

impl GpuPaintProgram {
    /// Validate the complete shader and its resource interface without a GPU.
    /// The fragment output is premultiplied linear RGBA; the engine supplies
    /// the sRGB target conversion and premultiplied blending.
    pub fn new(
        shader: Arc<str>,
        stride: u64,
        step: wgpu::VertexStepMode,
        attributes: Arc<[wgpu::VertexAttribute]>,
    ) -> Result<Arc<Self>, String> {
        let module =
            naga::front::wgsl::parse_str(&shader).map_err(|e| e.emit_to_string(&shader))?;
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .map_err(|e| e.to_string())?;
        if stride == 0 || stride > 2048 || stride % 4 != 0 || attributes.len() > 16 {
            return Err("unsupported GPU paint vertex layout".into());
        }
        let mut locations = std::collections::BTreeSet::new();
        for a in attributes.iter() {
            if a.shader_location >= 16
                || !locations.insert(a.shader_location)
                || a.offset % 4 != 0
                || a.offset
                    .checked_add(a.format.size())
                    .is_none_or(|end| end > stride)
                || !matches!(
                    a.format,
                    wgpu::VertexFormat::Float32
                        | wgpu::VertexFormat::Float32x2
                        | wgpu::VertexFormat::Float32x3
                        | wgpu::VertexFormat::Float32x4
                )
            {
                return Err("unsupported GPU paint attribute".into());
            }
        }
        let mut uniform_size = None;
        for (_, global) in module.global_variables.iter() {
            if let Some(binding) = &global.binding {
                if binding.group != 0
                    || binding.binding != 0
                    || global.space != naga::AddressSpace::Uniform
                    || uniform_size.is_some()
                {
                    return Err("GPU paint only permits uniform group 0 binding 0".into());
                }
                let naga::TypeInner::Struct { span, .. } = module.types[global.ty].inner else {
                    return Err("GPU paint uniform must be a struct".into());
                };
                uniform_size = Some(u64::from(span));
            } else if !matches!(global.space, naga::AddressSpace::Private) {
                return Err("undeclared GPU paint resource".into());
            }
        }
        let uniform_size = uniform_size.ok_or("GPU paint requires one uniform block")?;
        if uniform_size == 0 || uniform_size > 65536 {
            return Err("GPU paint uniform exceeds portable limits".into());
        }
        let vertex = module
            .entry_points
            .iter()
            .find(|e| e.name == "vs_main" && e.stage == naga::ShaderStage::Vertex)
            .ok_or("missing vs_main")?;
        if module.entry_points.len() != 2
            || !module
                .entry_points
                .iter()
                .any(|e| e.name == "fs_main" && e.stage == naga::ShaderStage::Fragment)
        {
            return Err("GPU paint requires exactly vs_main and fs_main".into());
        }
        let mut input_locations = std::collections::BTreeSet::new();
        fn check_input(
            module: &naga::Module,
            ty: naga::Handle<naga::Type>,
            binding: &Option<naga::Binding>,
            attrs: &[wgpu::VertexAttribute],
            seen: &mut std::collections::BTreeSet<u32>,
        ) -> Result<(), String> {
            if let Some(naga::Binding::Location { location, .. }) = binding {
                let size = match module.types[ty].inner {
                    naga::TypeInner::Scalar(naga::Scalar {
                        kind: naga::ScalarKind::Float,
                        width: 4,
                    }) => 4,
                    naga::TypeInner::Vector {
                        size,
                        scalar:
                            naga::Scalar {
                                kind: naga::ScalarKind::Float,
                                width: 4,
                            },
                    } => size as u64 * 4,
                    _ => return Err("GPU paint vertex input must be float32".into()),
                };
                if !seen.insert(*location)
                    || !attrs
                        .iter()
                        .any(|a| a.shader_location == *location && a.format.size() == size)
                {
                    return Err("GPU paint shader/layout mismatch".into());
                }
            } else if binding.is_none() {
                let naga::TypeInner::Struct { ref members, .. } = module.types[ty].inner else {
                    return Err("unbound vertex input".into());
                };
                for m in members {
                    check_input(module, m.ty, &m.binding, attrs, seen)?;
                }
            }
            Ok(())
        }
        for arg in &vertex.function.arguments {
            check_input(
                &module,
                arg.ty,
                &arg.binding,
                &attributes,
                &mut input_locations,
            )?;
        }
        if input_locations != locations {
            return Err("unused GPU paint vertex attributes".into());
        }
        interface::validate(&module)?;
        Ok(Arc::new(Self {
            shader,
            stride,
            step,
            attributes,
            uniform_size,
        }))
    }
}

/// Work actually encoded for a source in the most recently completed frame.
/// None in an observation means a valid enclosing raster avoided reading it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuPaintWork {
    Rendered,
    Reused,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuPaintObservation {
    pub id: GpuPaintSourceId,
    pub revision: u64,
    pub extent: [u32; 2],
    pub work: Option<GpuPaintWork>,
    pub valid_resident: bool,
}

/// Frozen input, including exact bytes. Revision is not used as a substitute
/// for payload equality. Cloning keeps immutable input alive until graph work
/// finishes; no user callback can mutate it during recording or execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuPaintSource {
    pub(crate) id: GpuPaintSourceId,
    pub(crate) revision: u64,
    pub(crate) extent: [u32; 2],
    pub(crate) scale_bits: u32,
    pub(crate) program: Arc<GpuPaintProgram>,
    pub(crate) uniforms: Arc<[u8]>,
    pub(crate) vertices: Arc<[u8]>,
    pub(crate) vertex_count: u32,
    pub(crate) instance_count: u32,
}
impl GpuPaintSource {
    pub fn new(
        id: GpuPaintSourceId,
        revision: u64,
        extent: [u32; 2],
        scale: f32,
        program: Arc<GpuPaintProgram>,
        uniforms: Arc<[u8]>,
        vertices: Arc<[u8]>,
        vertex_count: u32,
        instance_count: u32,
    ) -> Result<Self, &'static str> {
        let count = match program.step {
            wgpu::VertexStepMode::Vertex => vertex_count,
            wgpu::VertexStepMode::Instance => instance_count,
        };
        if revision == 0
            || extent.iter().any(|n| *n == 0 || *n > 8192)
            || !scale.is_finite()
            || scale <= 0.0
            || uniforms.len() as u64 != program.uniform_size
            || vertex_count % 3 != 0
            || vertex_count > 1_000_000
            || instance_count > 1_000_000
            || program.stride.checked_mul(u64::from(count)) != Some(vertices.len() as u64)
            || vertices.len() > 64 * 1024 * 1024
        {
            return Err("invalid frozen GPU paint source");
        }
        Ok(Self {
            id,
            revision,
            extent,
            scale_bits: scale.to_bits(),
            program,
            uniforms,
            vertices,
            vertex_count,
            instance_count,
        })
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn extent(&self) -> [u32; 2] {
        self.extent
    }
    pub(crate) fn allocated_bytes(&self) -> u64 {
        u64::from(self.extent[0]) * u64::from(self.extent[1]) * 4
            + self.uniforms.len() as u64
            + self.vertices.len() as u64
    }
    pub(crate) fn matches_size(&self, size: [f32; 2]) -> bool {
        size.into_iter()
            .zip(self.extent)
            .all(|(logical, physical)| {
                logical.is_finite()
                    && logical > 0.0
                    && (logical * f32::from_bits(self.scale_bits)).ceil() == physical as f32
            })
    }
    pub(crate) fn key(&self) -> crate::view::frame_graph::PersistentTextureKey {
        crate::view::frame_graph::PersistentTextureKey::retained(
            crate::view::frame_graph::RetainedTextureRole::GpuSourceColor,
            self.id.get(),
        )
    }
    pub(crate) fn descriptor(&self) -> crate::view::frame_graph::texture_resource::TextureDesc {
        crate::view::frame_graph::texture_resource::TextureDesc::new(
            self.extent[0],
            self.extent[1],
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureDimension::D2,
        )
        .with_label("GPU Paint Source")
    }
    /// Legacy and Artifact consume the same preparation. This does not advance
    /// simulation or expose the parent target to producer code.
    pub fn paint(
        &self,
        graph: &mut crate::view::frame_graph::FrameGraph,
        ctx: &mut crate::view::base_component::UiBuildContext,
        mut bounds: [f32; 4],
    ) {
        let offset = ctx.paint_offset();
        bounds[0] += offset[0];
        bounds[1] += offset[1];
        self.emit(
            graph,
            ctx,
            crate::view::render_pass::texture_composite_pass::TextureCompositeParams {
                bounds,
                source_is_premultiplied: true,
                ..Default::default()
            },
        );
    }
}

mod interface;
pub(crate) mod render;
#[cfg(test)]
mod tests;
