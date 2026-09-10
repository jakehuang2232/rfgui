use super::*;
use crate::view::base_component::Image;
use crate::view::image_resource::{acquire_image_resource, replace_ready_image_for_test};
use crate::view::sampled_texture::SampledTextureId;
use crate::view::{ImageFit, ImageSource};
use std::sync::Arc;

#[test]
fn legacy_ready_texture_preserves_source_and_shares_owner_snap_and_layer() {
    for (opacity, transformed) in [(1.0, false), (0.5, false), (1.0, true), (0.5, true)] {
        let source = ImageSource::Rgba {
            width: 3,
            height: 7,
            pixels: Arc::from([93, 177, 219, 255].repeat(21)),
        };
        let handle = acquire_image_resource(&source);
        let pixels: Arc<[u8]> = Arc::from([93, 177, 219, 255].repeat(21));
        let generation = replace_ready_image_for_test(handle.asset_id(), 3, 7, pixels.clone());
        let mut host = Image::new_with_id(0xc1_4200, source);
        host.set_fit(ImageFit::Fill);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(32.0)));
        style.insert(
            PropertyId::Opacity,
            ParsedValue::Opacity(Opacity::new(opacity)),
        );
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(0, 255, 0)),
        );
        if transformed {
            style.set_transform(Transform::new([Translate::xy(
                Length::px(6.0),
                Length::px(8.0),
            )]));
        }
        host.apply_style(style);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(host));
        let mut viewport = crate::view::viewport::Viewport::new();
        crate::view::viewport::layout_artifact_style_scene_for_test(
            &mut viewport,
            &mut arena,
            root,
            [64.0, 64.0],
        );
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(64, 64, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: target,
            },
        ));
        let outer_clip = Some([1, 2, 45, 46]);
        ctx.push_scissor_rect(outer_clip);
        ctx.set_paint_offset([3.25, 4.25]);
        let incoming = ctx.viewport();
        let state = arena
            .with_element_taken(root, |host, arena| host.build(&mut graph, arena, ctx))
            .unwrap();
        let restored = UiBuildContext::from_parts(incoming, state);
        assert_eq!(
            restored.current_target().and_then(|target| target.handle()),
            target.handle()
        );
        assert_eq!(
            restored.graphics_pass_context().logical_scissor_rect(),
            outer_clip
        );
        let snapshot = graph.test_compile_snapshot().unwrap();
        let payloads = snapshot.pass_payloads();
        let rects: Vec<_> = payloads
            .iter()
            .enumerate()
            .filter_map(|(i, pass)| match pass {
                FramePassTestPayload::DrawRect(rect) => Some((i, rect)),
                _ => None,
            })
            .collect();
        let textures: Vec<_> = payloads
            .iter()
            .enumerate()
            .filter_map(|(i, pass)| match pass {
                FramePassTestPayload::TextureComposite(texture)
                    if texture.sampled_source.is_some() =>
                {
                    Some((i, texture))
                }
                _ => None,
            })
            .collect();
        let layers: Vec<_> = payloads
            .iter()
            .enumerate()
            .filter_map(|(i, pass)| match pass {
                FramePassTestPayload::TextureComposite(texture)
                    if texture.sampled_source.is_none() =>
                {
                    Some((i, texture))
                }
                _ => None,
            })
            .collect();
        assert_eq!(rects.len(), 1, "owner background must be emitted once");
        assert_eq!(textures.len(), 1, "native payload must be emitted once");
        let (texture_index, texture) = textures[0];
        let upload = texture.sampled_source.as_ref().unwrap();
        assert_eq!(upload.id, SampledTextureId::Image(handle.asset_id()));
        assert_eq!(upload.generation, generation);
        assert!(
            Arc::ptr_eq(&upload.pixels, &pixels),
            "forward the frozen upload, without republishing or copying pixels"
        );
        let expected_bounds = if transformed {
            [0.0, 0.0, 20.0, 32.0]
        } else {
            [3.0, 4.0, 20.0, 32.0]
        };
        assert_eq!(
            texture.bounds_bits.map(f32::from_bits),
            expected_bounds,
            "snap once in the owner's raster coordinate space"
        );
        assert_eq!(f32::from_bits(texture.opacity_bits), 1.0);
        assert_eq!(f32::from_bits(rects[0].1.opacity_bits), 1.0);
        assert!(
            rects[0].0 < texture_index,
            "owner decoration precedes native content"
        );
        assert_eq!(layers.len(), usize::from(transformed || opacity < 1.0));
        if let Some((layer_index, layer)) = layers.first() {
            assert!(
                texture_index < *layer_index,
                "payload must be painted before the group composite"
            );
            assert_eq!(texture.output_target, layer.source_handle);
            assert_eq!(f32::from_bits(layer.opacity_bits), opacity);
            assert_eq!(layer.output_target, target.handle());
            assert_eq!(layer.effective_scissor_rect, outer_clip);
        } else {
            assert_eq!(texture.output_target, target.handle());
            assert_eq!(texture.effective_scissor_rect, outer_clip);
        }
    }
}
