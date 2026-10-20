use naga_oil::compose::{Composer, NagaModuleDescriptor, ShaderDefValue, ShaderType};
use rustc_hash::FxHashMap;

use super::draw_rect_pass::RectRenderMode;

const RECT_WGSL: &str = include_str!("../../shader/rect.wgsl");

thread_local! {
    /// Building a `Composer` costs about as much as composing a variant, so
    /// each thread that composes keeps one: the rect pipeline workers, or the
    /// render thread without them. Without imports or `@binding(auto)`,
    /// composing leaves no state behind to carry over.
    static COMPOSER: std::cell::RefCell<Composer> = std::cell::RefCell::new(Composer::default());
}

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

pub(crate) fn build_rect_shader(device: &wgpu::Device, key: RectShaderKey) -> wgpu::ShaderModule {
    // naga_oil and wgpu share one naga, so wgpu takes the composed IR and
    // validates it as is, instead of parsing WGSL written out from it.
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&format!("Rect Shader {:?}", key)),
        source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(rect_shader_module(key))),
    })
}

/// The composed module of one rect shader variant.
fn rect_shader_module(key: RectShaderKey) -> naga::Module {
    debug_assert!(!RECT_WGSL.contains("@binding(auto)"));
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

    let shader_defs: std::collections::HashMap<String, ShaderDefValue> = defs.into_iter().collect();

    COMPOSER
        .with(|composer| {
            composer
                .borrow_mut()
                .make_naga_module(NagaModuleDescriptor {
                    source: RECT_WGSL,
                    file_path: "rect.wgsl",
                    shader_type: ShaderType::Wgsl,
                    shader_defs,
                    additional_imports: &[],
                })
        })
        .unwrap_or_else(|e| panic!("compose rect shader (key={:?}): {}", key, e))
}

#[cfg(test)]
mod tests;
