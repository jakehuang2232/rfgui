use super::*;
use rfgui::style::BoxShadow;
use rfgui::view::Text;

fn static_tree() -> RsxNode {
    rfgui::ui::rsx_scope(|| {
        rfgui::ui::rsx! {
            <Element style={{width:Length::px(160.),height:Length::px(96.),layout:Layout::Grid,background:Color::rgb(6,9,12)}}>
                <Text style={{color:Color::rgb(255,255,255)}}>GPU scope</Text>
                <Element style={{width:Length::px(20.),height:Length::px(16.),position:Position::absolute().left(Length::px(100.)).top(Length::px(8.)),background:Color::rgb(0,0,255),opacity:Opacity::new(0.5),box_shadow:vec![BoxShadow::new().color(Color::rgb(255,0,0)).blur(4.)]}} />
            </Element>
        }
    })
}

fn pixels(
    viewport: &mut Viewport,
    gpu: &gpu::Gpu,
    root: &RsxNode,
    dpr: u32,
) -> Result<Vec<u8>, String> {
    let frame = viewport.render_rsx_offscreen_for_test(
        root,
        gpu.device.clone(),
        gpu.queue.clone(),
        [160 * dpr, 96 * dpr],
        dpr as f32,
        Instant::now(),
    )?;
    gpu.read(&frame.texture, [160 * dpr, 96 * dpr])
}

#[test]
#[ignore = "requires two independent native GPU Instances"]
fn independent_devices_keep_pass_resources_and_survive_other_viewport_release() -> Result<(), String>
{
    // Gpu::new creates an independent Instance each time. Backend-local object
    // IDs can coincide, unlike two devices allocated from one shared Instance.
    let first_gpu = gpu::Gpu::new()?;
    let second_gpu = gpu::Gpu::new()?;
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for dpr in [1, 2] {
            let root = static_tree();
            let mut first = Viewport::new();
            let mut second = Viewport::new();
            first.set_paint_renderer_mode(mode);
            second.set_paint_renderer_mode(mode);
            let expected = pixels(&mut first, &first_gpu, &root, dpr)?;
            assert!(
                expected
                    .chunks_exact(4)
                    .any(|p| p[0] > 150 && p[1] > 150 && p[2] > 150),
                "text must actually draw"
            );
            assert_eq!(pixels(&mut second, &second_gpu, &root, dpr)?, expected);
            first.release_render_resource_caches();
            assert_eq!(
                pixels(&mut second, &second_gpu, &root, dpr)?,
                expected,
                "releasing A preserves B"
            );
            assert_eq!(
                pixels(&mut first, &first_gpu, &root, dpr)?,
                expected,
                "A rebuilds its own resources"
            );
            drop(first);
            assert_eq!(
                pixels(&mut second, &second_gpu, &root, dpr)?,
                expected,
                "dropping A preserves B"
            );
        }
    }
    Ok(())
}
