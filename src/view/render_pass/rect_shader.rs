use std::borrow::Cow;

use naga_oil::compose::{Composer, NagaModuleDescriptor, ShaderDefValue, ShaderType};
use rustc_hash::FxHashMap;

use super::draw_rect_pass::RectRenderMode;

const RECT_WGSL: &str = include_str!("../../shader/rect.wgsl");

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum RectBorderKind {
    None,
    Uniform,
    PerSide,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct RectShaderKey {
    pub has_fill: bool,
    pub border: RectBorderKind,
    pub rounded: bool,
    pub opaque: bool,
    pub pass: RectRenderMode,
    pub has_gradient: bool,
    pub has_border_gradient: bool,
}

pub(crate) fn build_rect_shader(
    device: &wgpu::Device,
    key: RectShaderKey,
) -> wgpu::ShaderModule {
    let mut composer = Composer::default();
    let mut defs: FxHashMap<String, ShaderDefValue> = FxHashMap::default();
    let set = |defs: &mut FxHashMap<String, ShaderDefValue>, name: &str| {
        defs.insert(name.to_string(), ShaderDefValue::Bool(true));
    };
    if key.has_fill {
        set(&mut defs, "HAS_FILL");
    }
    match key.border {
        RectBorderKind::None => set(&mut defs, "BORDER_NONE"),
        RectBorderKind::Uniform => set(&mut defs, "BORDER_UNIFORM"),
        RectBorderKind::PerSide => set(&mut defs, "BORDER_PERSIDE"),
    }
    if key.rounded {
        set(&mut defs, "ROUNDED");
    }
    if key.opaque {
        set(&mut defs, "OPAQUE");
    }
    match key.pass {
        RectRenderMode::Combined => {}
        RectRenderMode::FillOnly => set(&mut defs, "PASS_FILL_ONLY"),
        RectRenderMode::BorderOnly => set(&mut defs, "PASS_BORDER_ONLY"),
    }
    if key.has_gradient {
        set(&mut defs, "HAS_GRADIENT");
    }
    if key.has_border_gradient {
        set(&mut defs, "HAS_BORDER_GRADIENT");
    }

    let shader_defs: std::collections::HashMap<String, ShaderDefValue> =
        defs.into_iter().collect();

    let module = composer
        .make_naga_module(NagaModuleDescriptor {
            source: RECT_WGSL,
            file_path: "rect.wgsl",
            shader_type: ShaderType::Wgsl,
            shader_defs,
            additional_imports: &[],
        })
        .unwrap_or_else(|e| panic!("compose rect shader (key={:?}): {}", key, e));

    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&format!("Rect Shader {:?}", key)),
        source: wgpu::ShaderSource::Naga(Cow::Owned(module)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compose(key: RectShaderKey) -> Result<naga::Module, String> {
        let mut composer = Composer::default();
        let mut defs: FxHashMap<String, ShaderDefValue> = FxHashMap::default();
        let mut set = |n: &str| {
            defs.insert(n.to_string(), ShaderDefValue::Bool(true));
        };
        if key.has_fill {
            set("HAS_FILL");
        }
        match key.border {
            RectBorderKind::None => set("BORDER_NONE"),
            RectBorderKind::Uniform => set("BORDER_UNIFORM"),
            RectBorderKind::PerSide => set("BORDER_PERSIDE"),
        }
        if key.rounded {
            set("ROUNDED");
        }
        if key.opaque {
            set("OPAQUE");
        }
        match key.pass {
            RectRenderMode::Combined => {}
            RectRenderMode::FillOnly => set("PASS_FILL_ONLY"),
            RectRenderMode::BorderOnly => set("PASS_BORDER_ONLY"),
        }
        if key.has_gradient {
            set("HAS_GRADIENT");
        }
        if key.has_border_gradient {
            set("HAS_BORDER_GRADIENT");
        }
        let shader_defs: std::collections::HashMap<String, ShaderDefValue> =
            defs.into_iter().collect();

        composer
            .make_naga_module(NagaModuleDescriptor {
                source: RECT_WGSL,
                file_path: "rect.wgsl",
                shader_type: ShaderType::Wgsl,
                shader_defs,
                additional_imports: &[],
            })
            .map_err(|e| format!("main ({:?}): {}", key, e))
    }

    #[test]
    fn compose_all_variants() {
        use RectBorderKind::*;
        use RectRenderMode::*;
        let borders = [None, Uniform, PerSide];
        let passes = [Combined, FillOnly, BorderOnly];
        for &has_fill in &[true, false] {
            for &border in &borders {
                for &rounded in &[true, false] {
                    for &opaque in &[true, false] {
                        for &pass in &passes {
                            for &has_gradient in &[false, true] {
                                for &has_border_gradient in &[false, true] {
                                    if matches!(pass, FillOnly) && !has_fill {
                                        continue;
                                    }
                                    if matches!(pass, BorderOnly) && matches!(border, None) {
                                        continue;
                                    }
                                    if matches!(pass, Combined)
                                        && !has_fill
                                        && matches!(border, None)
                                    {
                                        continue;
                                    }
                                    if matches!(pass, FillOnly) && has_border_gradient {
                                        continue;
                                    }
                                    if matches!(border, None) && has_border_gradient {
                                        continue;
                                    }
                                    if !has_fill && has_gradient {
                                        continue;
                                    }
                                    let key = RectShaderKey {
                                        has_fill,
                                        border,
                                        rounded,
                                        opaque,
                                        pass,
                                        has_gradient,
                                        has_border_gradient,
                                    };
                                    if let Err(e) = compose(key) {
                                        panic!("{}", e);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
