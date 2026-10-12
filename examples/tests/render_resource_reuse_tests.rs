use rfgui::style::{Color, Length};
use rfgui::time::Instant;
use rfgui::ui::{profile_ui_work, rsx};
use rfgui::view::{Element, Text, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn warm_render_reuses_text_inputs_and_composite_bindings() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for dpr in [1_u32, 2] {
        let root = rsx! {
            <Element style={{ width: Length::px(120.), height: Length::px(64.) }}>
                <Text style={{ color: Color::hex("#ff0000") }}>{"Warm text lookup"}</Text>
                <Element style={{ width: Length::px(20.), height: Length::px(20.), opacity: 0.5, background_color: Color::hex("#0000ff") }}>
                    <Element style={{ width: Length::px(10.), height: Length::px(10.), background_color: Color::hex("#00ff00") }} />
                </Element>
            </Element>
        };
        let mut viewport = Viewport::new();
        let size = [120 * dpr, 64 * dpr];
        let mut baseline = None;
        for frame in 0..3 {
            let (rendered, work) = profile_ui_work(|| {
                viewport.render_rsx_offscreen_for_test(
                    &root,
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    size,
                    dpr as f32,
                    Instant::now(),
                )
            });
            let pixels = gpu.read(&rendered?.texture, size)?;
            if let Some(expected) = &baseline {
                assert_eq!(&pixels, expected);
            } else {
                assert!(pixels.chunks_exact(4).any(|p| p[0] > 0 && p[3] > 0));
                baseline = Some(pixels);
            }
            if frame == 2 {
                assert_eq!(work.text_input_glyph_observations, 0, "DPR {dpr}: {work:?}");
                assert_eq!(
                    work.composite_bind_group_creations, 0,
                    "DPR {dpr}: {work:?}"
                );
            }
            if frame == 0 {
                assert!(work.text_input_glyph_observations > 0);
                assert!(
                    work.composite_bind_group_creations > 0,
                    "fixture must execute composite"
                );
            }
        }
    }
    Ok(())
}
