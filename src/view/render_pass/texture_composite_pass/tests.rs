use super::*;
use crate::view::sampled_texture::{
    ImageAssetId, SampledTextureAlphaMode, SampledTextureId, SampledTextureUpload,
};
use std::sync::Arc;

fn sampled_pass(pixels: Arc<[u8]>) -> TextureCompositePass {
    TextureCompositePass::new(
        TextureCompositeParams {
            bounds: [1.25, 2.5, 3.75, 4.0],
            uv_bounds: Some([0.0, 0.0, 1.0, 1.0]),
            opacity: 0.5,
            scissor_rect: Some([1, 2, 3, 4]),
            ..Default::default()
        },
        TextureCompositeInput::from_sampled_texture(
            SampledTextureUpload {
                id: SampledTextureId::Image(ImageAssetId::for_test(81)),
                generation: 7,
                width: 1,
                height: 1,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                alpha_mode: SampledTextureAlphaMode::Straight,
                pixels,
                sampling: ImageSampling::Linear,
            },
            Default::default(),
            RenderPassContext::default(),
        ),
        TextureCompositeOutput::default(),
    )
}

#[test]
fn strict_snapshot_compares_actual_pixels_and_every_sampled_render_field() {
    let base = sampled_pass(Arc::from([1_u8, 2, 3, 4])).test_snapshot();
    let changed_pixels = sampled_pass(Arc::from([1_u8, 2, 3, 5])).test_snapshot();
    assert_ne!(base, changed_pixels);

    let mut changed = sampled_pass(Arc::from([1_u8, 2, 3, 4]));
    changed.params.bounds[0] = -0.0;
    assert_ne!(base, changed.test_snapshot());

    let mut changed = sampled_pass(Arc::from([1_u8, 2, 3, 4]));
    changed.params.opacity = 0.25;
    assert_ne!(base, changed.test_snapshot());

    let mut changed = sampled_pass(Arc::from([1_u8, 2, 3, 4]));
    changed.input.sampled_source.as_mut().unwrap().generation += 1;
    assert_ne!(base, changed.test_snapshot());

    let mut changed = sampled_pass(Arc::from([1_u8, 2, 3, 4]));
    changed.input.sampled_source.as_mut().unwrap().id =
        SampledTextureId::Image(ImageAssetId::for_test(82));
    assert_ne!(base, changed.test_snapshot());

    let mut changed = sampled_pass(Arc::from([1_u8, 2, 3, 4]));
    changed.input.sampled_source.as_mut().unwrap().width = 2;
    assert_ne!(base, changed.test_snapshot());

    let mut changed = sampled_pass(Arc::from([1_u8, 2, 3, 4]));
    changed.input.sampled_source.as_mut().unwrap().height = 2;
    assert_ne!(base, changed.test_snapshot());

    let mut changed = sampled_pass(Arc::from([1_u8, 2, 3, 4]));
    changed.input.sampled_source.as_mut().unwrap().format = wgpu::TextureFormat::Rgba8Unorm;
    assert_ne!(base, changed.test_snapshot());

    let mut changed = sampled_pass(Arc::from([1_u8, 2, 3, 4]));
    changed.input.sampled_source.as_mut().unwrap().sampling = ImageSampling::Nearest;
    assert_ne!(base, changed.test_snapshot());

    let mut changed = sampled_pass(Arc::from([1_u8, 2, 3, 4]));
    changed.params.uv_bounds = Some([0.25, 0.0, 0.75, 1.0]);
    assert_ne!(base, changed.test_snapshot());

    let mut changed = sampled_pass(Arc::from([1_u8, 2, 3, 4]));
    changed.params.quad_positions = Some([[0.0, 0.0]; 4]);
    assert_ne!(base, changed.test_snapshot());

    let mut changed = sampled_pass(Arc::from([1_u8, 2, 3, 4]));
    changed.params.mask_uv_bounds = Some([0.0, 0.0, 0.5, 0.5]);
    assert_ne!(base, changed.test_snapshot());

    let mut changed = sampled_pass(Arc::from([1_u8, 2, 3, 4]));
    changed.params.use_mask = true;
    assert_ne!(base, changed.test_snapshot());

    let mut changed = sampled_pass(Arc::from([1_u8, 2, 3, 4]));
    changed.params.source_is_premultiplied = true;
    assert_ne!(base, changed.test_snapshot());

    let mut changed = sampled_pass(Arc::from([1_u8, 2, 3, 4]));
    changed.params.scissor_rect = Some([9, 9, 9, 9]);
    assert_ne!(base, changed.test_snapshot());

    let mut changed = sampled_pass(Arc::from([1_u8, 2, 3, 4]));
    changed.input.pass_context.scissor_rect = Some(
        crate::view::render_pass::render_target::GraphicsPassScissor::Logical([9, 8, 7, 6]),
    );
    assert_ne!(base, changed.test_snapshot());
}

fn assert_rgba_close(actual: [f32; 4], expected: [f32; 4]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
    }
}

fn composite_sample(color: [f32; 4], factor: f32, source_is_premultiplied: bool) -> [f32; 4] {
    let alpha = color[3] * factor;
    if source_is_premultiplied {
        [
            color[0] * factor,
            color[1] * factor,
            color[2] * factor,
            alpha,
        ]
    } else {
        [color[0] * alpha, color[1] * alpha, color[2] * alpha, alpha]
    }
}

#[test]
fn premultiplied_sources_do_not_apply_alpha_twice() {
    let premultiplied_color = [0.30, 0.12, 0.06, 0.60];
    let out = composite_sample(premultiplied_color, 0.5, true);
    assert_rgba_close(out, [0.15, 0.06, 0.03, 0.30]);
}

#[test]
fn straight_alpha_sources_still_convert_to_premultiplied_output() {
    let straight_alpha_color = [1.0, 0.4, 0.2, 0.60];
    let out = composite_sample(straight_alpha_color, 0.5, false);
    assert_rgba_close(out, [0.30, 0.12, 0.06, 0.30]);
}
