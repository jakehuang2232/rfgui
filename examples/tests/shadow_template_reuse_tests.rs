use rfgui::style::{BorderRadius, BoxShadow, Color, Length, Position};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, profile_ui_work, rsx};
use rfgui::view::viewport::ViewportPaintRendererMode;
use rfgui::view::{Element, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const WIDTH: u32 = 240;
const HEIGHT: u32 = 180;

fn scene(left: f32, card_width: f32) -> RsxNode {
    rsx! {
        <Element style={{
            width: Length::px(WIDTH as f32),
            height: Length::px(HEIGHT as f32),
            background_color: Color::hex("#f4f4f4"),
        }}>
            <Element style={{
                position: Position::absolute().left(Length::px(left)).top(Length::px(30.0)),
                width: Length::px(card_width),
                height: Length::px(90.0),
                border_radius: BorderRadius::uniform(Length::px(12.0)),
                background_color: Color::hex("#ffffff"),
                box_shadow: vec![
                    BoxShadow::new().color(Color::rgba(0, 0, 0, 120)).offset_y(6.0).blur(14.0),
                ],
            }} />
            <Element style={{
                position: Position::absolute().left(Length::px(left + 10.0)).top(Length::px(132.0)),
                width: Length::px(card_width - 20.0),
                height: Length::px(36.0),
                border_radius: BorderRadius::uniform(Length::px(8.0)),
                background_color: Color::hex("#dde6ff"),
                box_shadow: vec![
                    BoxShadow::new().color(Color::rgba(0, 0, 80, 160)).blur(6.0).inset(true),
                ],
            }} />
        </Element>
    }
}

fn render(
    viewport: &mut Viewport,
    gpu: &gpu::Gpu,
    root: &RsxNode,
    dpr: u32,
) -> Result<(Vec<u8>, usize), String> {
    let size = [WIDTH * dpr, HEIGHT * dpr];
    let (rendered, work) = profile_ui_work(|| {
        viewport.render_rsx_offscreen_for_test(
            root,
            gpu.device.clone(),
            gpu.queue.clone(),
            size,
            dpr as f32,
            Instant::now(),
        )
    });
    Ok((
        gpu.read(&rendered?.texture, size)?,
        work.shadow_template_builds,
    ))
}

/// Moving or widening a shadowed card redraws its cached blur templates;
/// only the first frame blurs. Every warm frame matches a cold render.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn moved_and_widened_shadows_reuse_their_templates() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        for dpr in [1_u32, 2] {
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let (_, builds) = render(&mut viewport, &gpu, &scene(30.0, 150.0), dpr)?;
            assert!(
                builds >= 2,
                "{mode:?} DPR {dpr}: cold frame builds templates"
            );
            for (frame, (left, card_width)) in [(31.0, 150.0), (45.0, 150.0), (45.0, 170.0)]
                .into_iter()
                .enumerate()
            {
                let root = scene(left, card_width);
                let (pixels, builds) = render(&mut viewport, &gpu, &root, dpr)?;
                assert_eq!(builds, 0, "{mode:?} DPR {dpr} frame {frame}");
                let mut cold = Viewport::new();
                cold.set_paint_renderer_mode(mode);
                let (expected, _) = render(&mut cold, &gpu, &root, dpr)?;
                assert!(
                    pixels == expected,
                    "{mode:?} DPR {dpr} frame {frame}: warm template pixels differ from a cold render"
                );
            }
        }
    }
    Ok(())
}

/// Template production that never reaches a submitted encoder must not be
/// trusted: the retried frame produces the templates again.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn aborted_frame_does_not_validate_its_templates() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        let dpr = 2;
        let size = [WIDTH * dpr, HEIGHT * dpr];
        let root = scene(30.0, 150.0);
        let mut viewport = Viewport::new();
        viewport.set_paint_renderer_mode(mode);
        viewport.fail_next_surface_acquisition_for_test();
        let (aborted, work) = profile_ui_work(|| {
            viewport.render_rsx_redraw_offscreen_for_test(
                &root,
                gpu.device.clone(),
                gpu.queue.clone(),
                size,
                dpr as f32,
                Instant::now(),
            )
        });
        assert!(aborted?.is_none(), "{mode:?}: the frame aborts");
        assert!(
            work.shadow_template_builds >= 2,
            "{mode:?}: templates were produced before acquisition"
        );
        let (pixels, builds) = render(&mut viewport, &gpu, &root, dpr)?;
        assert!(builds >= 2, "{mode:?}: the retry produces them again");
        let mut cold = Viewport::new();
        cold.set_paint_renderer_mode(mode);
        assert!(pixels == render(&mut cold, &gpu, &root, dpr)?.0, "{mode:?}");
    }
    Ok(())
}
