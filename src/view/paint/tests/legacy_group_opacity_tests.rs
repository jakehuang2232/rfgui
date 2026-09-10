use super::*;

#[test]
fn legacy_nonempty_group_neutralizes_only_owner_and_restores_receiver_scope() {
    for (opacity, transformed, expected_layers) in [
        (0.5, false, 1),
        (0.5, true, 1),
        (1.0, false, 0),
        (1.0, true, 1),
    ] {
        let mut arena = new_test_arena();
        let mut root = Element::new_with_id(0xc1_3300, 0.0, 0.0, 20.0, 16.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(16.0)));
        style.insert(
            PropertyId::Opacity,
            ParsedValue::Opacity(Opacity::new(opacity)),
        );
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(255, 0, 0)),
        );
        if transformed {
            style.set_transform(Transform::new([Translate::xy(
                Length::px(3.0),
                Length::px(4.0),
            )]));
        }
        root.apply_style(style);
        let root = commit_element(&mut arena, Box::new(root));
        let mut child = Element::new_with_id(0xc1_3301, 0.0, 0.0, 20.0, 32.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(32.0)));
        style.insert(
            PropertyId::Opacity,
            ParsedValue::Opacity(Opacity::new(0.25)),
        );
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(0, 0, 255)),
        );
        child.apply_style(style);
        commit_child(&mut arena, root, Box::new(child));
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
        // Blended composites read the receiver: establish a real producer,
        // matching the clear performed by a production frame.
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
        ctx.push_scissor_rect(Some([0, 0, 48, 48]));
        let incoming = ctx.viewport();
        let state = arena
            .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
            .unwrap();
        let restored = UiBuildContext::from_parts(incoming, state);
        assert_eq!(
            restored.current_target().and_then(|target| target.handle()),
            target.handle()
        );
        assert_eq!(
            restored.graphics_pass_context().logical_scissor_rect(),
            Some([0, 0, 48, 48])
        );
        let snapshot = graph.test_compile_snapshot().unwrap();
        let layers: Vec<_> = snapshot
            .pass_payloads()
            .iter()
            .filter_map(|pass| match pass {
                FramePassTestPayload::TextureComposite(composite) => Some(composite),
                _ => None,
            })
            .collect();
        assert_eq!(
            layers.len(),
            expected_layers,
            "opacity shares the existing transform layer"
        );
        if let Some(layer) = layers.first() {
            assert_eq!(f32::from_bits(layer.opacity_bits), opacity);
            assert_eq!(
                layer.bounds_bits.map(f32::from_bits),
                [0.0, 0.0, 20.0, 32.0],
                "the group envelope must include content beyond the owner's 16px height"
            );
            assert!(layer.source_is_premultiplied);
        }
        let mut own = 0;
        let mut descendants = 0;
        for pass in snapshot.pass_payloads() {
            if let FramePassTestPayload::DrawRect(rect) = pass {
                let color = rect.fill_color_bits.map(f32::from_bits);
                if color[0] == 1.0 && color[2] == 0.0 {
                    assert_eq!(f32::from_bits(rect.opacity_bits), 1.0);
                    own += 1;
                } else if color[2] == 1.0 && color[0] == 0.0 {
                    assert_eq!(
                        f32::from_bits(rect.opacity_bits),
                        0.25,
                        "only owner opacity is neutralized; descendants keep their local alpha"
                    );
                    descendants += 1;
                }
            }
        }
        assert_eq!((own, descendants), (1, 1));
    }
}
