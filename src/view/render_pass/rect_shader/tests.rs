use super::*;

fn compose(key: RectShaderKey) -> Result<naga29::Module, String> {
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
    let shader_defs: std::collections::HashMap<String, ShaderDefValue> = defs.into_iter().collect();

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
                                if matches!(pass, Combined) && !has_fill && matches!(border, None) {
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
