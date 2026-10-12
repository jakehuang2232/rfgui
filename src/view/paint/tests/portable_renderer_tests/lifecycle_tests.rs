use super::*;
use crate::view::test_support::get_element_mut;

pub(super) async fn run(gpu: &Gpu) -> Result<usize, String> {
    const SIZE: [u32; 2] = [48, 40];
    let mut frames = 0;
    for initial_dpr in [1, 2] {
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(Element::new_with_id(0xc7_100, 0.0, 0.0, 20.0, 16.0)),
        );
        let mut viewport = Viewport::new();
        viewport.install_single_viewport_scene_for_test(arena, root);
        // Ordered deltas: cold, placement, opacity, content, no-op, DPR,
        // DPR back, content back, warm.
        for frame in 0..9 {
            let dpr = if frame == 5 {
                3 - initial_dpr
            } else {
                initial_dpr
            };
            let x = if frame == 0 { 4 } else { 8 };
            let alpha = if frame < 2 { 128 } else { 64 };
            let rgb = if frame < 3 || frame >= 7 { RED } else { BLUE };
            let mut style = style([20.0, 16.0], None, Some(rgb));
            style.insert(
                PropertyId::Opacity,
                ParsedValue::Opacity(Opacity::new(if alpha == 128 { 0.5 } else { 0.25 })),
            );
            style.set_transform(Transform::new([Translate::xy(
                Length::px(x as f32),
                Length::px(4.0),
            )]));
            get_element_mut::<Element>(viewport.node_arena(), root).apply_style(style);
            let label = format!("lifecycle initial DPR={initial_dpr} frame={frame}");
            progress(&label);
            begin(&mut viewport, gpu, SIZE, dpr)?;
            let observed = viewport.render_single_viewport_scene_for_test()?;
            let pixels = read_pixels(gpu, &observed.texture, SIZE.map(|v| v * dpr)).await?;
            let mut color = rgb;
            color[3] = alpha;
            for (name, at, expected) in [
                ("content", [x + 4, 8], color),
                ("left exterior", [x - 2, 8], CLEAR),
                ("right exterior", [x + 22, 8], CLEAR),
                ("bottom exterior", [x + 4, 22], CLEAR),
            ] {
                probe(&pixels, SIZE, dpr, name, at, expected)
                    .map_err(|e| format!("{label}: {e}"))?;
            }
            assert_eq!(
                observed.actions,
                [if [0, 3, 5, 6, 7].contains(&frame) {
                    RetainedSurfaceCompileAction::Reraster
                } else {
                    RetainedSurfaceCompileAction::Reuse
                }]
            );
            assert_eq!(observed.texture_bytes, 20 * 16 * 4 * u64::from(dpr * dpr));
            assert_eq!(observed.color_targets.len(), 1);
            let (key, desc) = &observed.color_targets[0];
            assert!(viewport.has_compatible_persistent_render_target(*key, desc));
            frames += 1;
        }
    }
    assert_eq!(frames, 18);
    Ok(frames)
}
