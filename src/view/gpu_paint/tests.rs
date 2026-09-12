use super::*;
const SHADER: &str = r#"
struct U { color:vec4<f32> }
@group(0) @binding(0) var<uniform> u:U;
@vertex fn vs_main(@location(0) p:vec2<f32>) -> @builtin(position) vec4<f32> {return vec4(p,0.,1.);}
@fragment fn fs_main() -> @location(0) vec4<f32> {return u.color;}
"#;
fn program(shader: &str) -> Result<Arc<GpuPaintProgram>, String> {
    GpuPaintProgram::new(
        shader.into(),
        8,
        wgpu::VertexStepMode::Vertex,
        Arc::from([wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        }]),
    )
}
#[test]
fn rejects_undeclared_gpu_source_resources() {
    assert!(program(SHADER).is_ok());
    for shader in [
        "invalid wgsl".into(),
        SHADER.replace("@binding(0)", "@binding(1)"),
        format!("{SHADER}\n@group(1) @binding(0) var extra:texture_2d<f32>;"),
        SHADER
            .replace("@location(0) p:vec2<f32>", "@location(0) p:vec3<f32>")
            .replace("vec4(p,0.,1.)", "vec4(p,1.)"),
    ] {
        assert!(
            program(&shader).is_err(),
            "undeclared or mismatched interface accepted: {shader}"
        );
    }
}
#[test]
fn rejects_cross_stage_and_color_target_mismatches() {
    for shader in [
        SHADER.replace(
            "@fragment fn fs_main()",
            "@fragment fn fs_main(@location(1) missing:vec4<f32>)",
        ),
        SHADER.replace("-> @location(0)", "-> @location(1)"),
        SHADER.replace(
            "-> @location(0) vec4<f32> {return u.color;}",
            "-> @location(0) vec4<u32> {return vec4<u32>(u.color);}",
        ),
    ] {
        assert!(program(&shader).is_err(), "{shader}");
    }
}
#[test]
fn source_packets_validate_dimensions_and_preserve_exact_payload() {
    let id = GpuPaintSourceId::new();
    let p = program(SHADER).unwrap();
    let make = |revision, extent, scale, uniforms: Arc<[u8]>, vertices: Arc<[u8]>| {
        GpuPaintSource::new(
            id,
            revision,
            extent,
            scale,
            p.clone(),
            uniforms,
            vertices,
            6,
            1,
        )
    };
    let u: Arc<[u8]> = vec![0; 16].into();
    let v: Arc<[u8]> = vec![0; 48].into();
    let first = make(1, [20, 16], 1., u.clone(), v.clone()).unwrap();
    assert_eq!(first, make(1, [20, 16], 1., u.clone(), v.clone()).unwrap());
    let mut changed = u.to_vec();
    changed[0] = 1;
    assert_ne!(
        first,
        make(1, [20, 16], 1., changed.into(), v.clone()).unwrap(),
        "same revision cannot hide changed bytes"
    );
    for (revision, extent, scale) in [
        (0, [20, 16], 1.),
        (1, [0, 16], 1.),
        (1, [8193, 16], 1.),
        (1, [20, 16], f32::NAN),
        (1, [20, 16], 0.),
    ] {
        assert!(make(revision, extent, scale, u.clone(), v.clone()).is_err());
    }
    assert!(make(1, [20, 16], 1., Arc::from([0; 12]), v.clone()).is_err());
    assert!(make(1, [20, 16], 1., u, Arc::from([0; 44])).is_err());
    assert_ne!(id, GpuPaintSourceId::new());
}
