use naga::{Binding, BuiltIn, Module, Type, TypeInner};
use std::collections::BTreeMap;

fn leaves(
    module: &Module,
    ty: naga::Handle<Type>,
    binding: &Option<Binding>,
    out: &mut Vec<(Binding, naga::Handle<Type>)>,
) -> Result<(), String> {
    if let Some(binding) = binding {
        out.push((binding.clone(), ty));
    } else if let TypeInner::Struct { members, .. } = &module.types[ty].inner {
        for member in members {
            leaves(module, member.ty, &member.binding, out)?;
        }
    } else {
        return Err("unbound GPU paint stage value".into());
    }
    Ok(())
}
fn float4(module: &Module, ty: naga::Handle<Type>) -> bool {
    matches!(
        module.types[ty].inner,
        TypeInner::Vector {
            size: naga::VectorSize::Quad,
            scalar: naga::Scalar {
                kind: naga::ScalarKind::Float,
                width: 4
            }
        }
    )
}
/// Naga validates entry points separately. Freeze the cross-stage contract too,
/// before a bad varying or color target can reach GPU pipeline creation.
pub(super) fn validate(module: &Module) -> Result<(), String> {
    let vertex = &module
        .entry_points
        .iter()
        .find(|e| e.stage == naga::ShaderStage::Vertex)
        .ok_or("missing vertex")?
        .function;
    let fragment = &module
        .entry_points
        .iter()
        .find(|e| e.stage == naga::ShaderStage::Fragment)
        .ok_or("missing fragment")?
        .function;
    let mut outputs = Vec::new();
    let result = vertex.result.as_ref().ok_or("vertex has no output")?;
    leaves(module, result.ty, &result.binding, &mut outputs)?;
    let mut varyings = BTreeMap::new();
    let mut position = false;
    let mut components = 0_u32;
    for (binding, ty) in outputs {
        match binding {
            Binding::BuiltIn(BuiltIn::Position { .. }) if float4(module, ty) => position = true,
            Binding::Location { location, .. } if location < 16 => {
                components += match module.types[ty].inner {
                    TypeInner::Scalar(_) => 1,
                    TypeInner::Vector { size, .. } => size as u32,
                    _ => return Err("unsupported GPU paint varying type".into()),
                };
                if components > 60 {
                    return Err("GPU paint exceeds portable varying limits".into());
                }
                varyings.insert(location, (binding, ty));
            }
            _ => return Err("unsupported GPU paint vertex output".into()),
        }
    }
    if !position {
        return Err("missing vertex position".into());
    }
    let mut inputs = Vec::new();
    for arg in &fragment.arguments {
        leaves(module, arg.ty, &arg.binding, &mut inputs)?;
    }
    for (binding, ty) in inputs {
        match &binding {
            Binding::Location { location, .. } => {
                let Some((output_binding, output_ty)) = varyings.get(location) else {
                    return Err("missing GPU paint varying".into());
                };
                if output_binding != &binding
                    || module.types[*output_ty].inner != module.types[ty].inner
                {
                    return Err("GPU paint varying mismatch".into());
                }
            }
            Binding::BuiltIn(BuiltIn::Position { .. } | BuiltIn::FrontFacing) => {}
            _ => return Err("unsupported GPU paint fragment input".into()),
        }
    }
    let mut colors = Vec::new();
    let result = fragment.result.as_ref().ok_or("fragment has no color")?;
    leaves(module, result.ty, &result.binding, &mut colors)?;
    if colors.len() != 1
        || !matches!(
            colors[0].0,
            Binding::Location {
                location: 0,
                blend_src: None,
                ..
            }
        )
        || !float4(module, colors[0].1)
    {
        return Err("GPU paint requires one float4 color output at location 0".into());
    }
    Ok(())
}
