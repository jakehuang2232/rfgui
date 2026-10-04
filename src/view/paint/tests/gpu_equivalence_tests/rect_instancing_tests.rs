use super::*;
use crate::view::inline_formatting_context::{
    InlineFormattingContext, InlineIfcInput, InlineIfcItem, InlineIfcLayoutOptions,
    InlineIfcSourceId, InlineIfcStyle,
};
use crate::view::render_pass::draw_rect_pass::{GradientPaint, GradientStopGpu};
use crate::view::render_pass::text_pass::{
    TextInput, TextOutput, TextPassPreparedFragment, TextPassPreparedParams, TextPreparedInputPass,
};

const SIZE: [u32; 2] = [64, 32];
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

fn rect(position: [f32; 2], size: [f32; 2], fill_color: [f32; 4]) -> DrawRectPass {
    DrawRectPass::new(
        RectPassParams {
            position,
            size,
            fill_color,
            opacity: 1.0,
            ..Default::default()
        },
        DrawRectInput::default(),
        DrawRectOutput::default(),
    )
}

fn gradient_rect(position: [f32; 2], size: [f32; 2], gradient: &GradientPaint) -> DrawRectPass {
    DrawRectPass::new(
        RectPassParams {
            position,
            size,
            fill_color: [0.0, 1.0, 0.0, 1.0],
            opacity: 1.0,
            gradient: Some(gradient.clone()),
            ..Default::default()
        },
        DrawRectInput::default(),
        DrawRectOutput::default(),
    )
}

/// The prelude's first draw on its transient target records in a render pass
/// of its own: the compiler promotes that first `Load` to `Clear`, which the
/// following loads cannot share. Spend it on a rounded primer whose pipeline
/// differs from every counted rect, so each scene's draw count is exactly one
/// more than the runs the instancing rules produce.
pub(super) fn emit_rect_run_primer(
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
    position: [f32; 2],
) {
    let mut primer = rect(position, [1.0, 1.0], [0.0, 1.0, 0.0, 1.0]);
    primer.set_border_radius(0.5);
    ctx.emit_draw_rect_pass(graph, primer);
}

fn primed_prelude(size: [u32; 2]) -> (FrameGraph, UiBuildContext, RenderTargetOut) {
    let (mut graph, mut ctx, target) = transformed_graph_prelude_with_size(1.0, None, size);
    emit_rect_run_primer(
        &mut graph,
        &mut ctx,
        [(size[0] - 1) as f32, (size[1] - 1) as f32],
    );
    (graph, ctx, target)
}

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
}

fn assert_near(actual: [u8; 4], expected: [u8; 4], case: &str) {
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(actual, expected)| actual.abs_diff(expected) <= 3),
        "{case}: actual={actual:?}, expected={expected:?}"
    );
}

/// Renders one frame on a fresh viewport and returns `(pixels, draw calls,
/// instances)` for its rect draws, including the primer.
fn render_rect_frame(
    mut graph: FrameGraph,
    target: RenderTargetOut,
    gpu: &NativeGpu,
) -> Result<(Vec<u8>, usize, usize), String> {
    add_present(&mut graph, &target)?;
    let mut viewport = Viewport::new();
    let (pixels, work) = crate::ui::profile_ui_work(|| {
        render_on_viewport_with_size(graph, gpu, &mut viewport, 1.0, FORMAT, SIZE)
    });
    Ok((pixels?, work.rect_draw_calls, work.rect_instances))
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_rect_runs_break_at_scissor_changes() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native graphics context");
    let (mut graph, mut ctx, target) = primed_prelude(SIZE);
    ctx.emit_draw_rect_pass(&mut graph, rect([0.0, 0.0], [8.0, 8.0], RED));
    ctx.emit_draw_rect_pass(&mut graph, rect([8.0, 0.0], [8.0, 8.0], RED));
    let previous = ctx.push_scissor_rect(Some([16, 0, 4, SIZE[1]]));
    ctx.emit_draw_rect_pass(&mut graph, rect([16.0, 0.0], [8.0, 8.0], BLUE));
    ctx.restore_scissor_rect(previous);
    ctx.emit_draw_rect_pass(&mut graph, rect([24.0, 0.0], [8.0, 8.0], RED));
    let (pixels, draws, instances) = render_rect_frame(graph, target, gpu)?;
    assert_eq!(
        (draws, instances),
        (1 + 3, 1 + 4),
        "a scissor change splits the run"
    );
    for (x, expected) in [
        (4, [255, 0, 0, 255]),
        (12, [255, 0, 0, 255]),
        (18, [0, 0, 255, 255]),
        (22, [0; 4]),
        (28, [255, 0, 0, 255]),
    ] {
        assert_eq!(pixel(&pixels, SIZE[0], x, 4), expected, "x={x}");
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_rect_runs_break_at_stencil_reference_changes() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native graphics context");
    let (mut graph, mut ctx, target) = primed_prelude(SIZE);
    // The clear leaves stencil 0 everywhere: reference 1 must reject its rect,
    // which a run merged with its reference-0 neighbours would draw.
    for (x, clip_id, color) in [
        (0.0, 0, RED),
        (8.0, 0, RED),
        (16.0, 1, BLUE),
        (24.0, 0, RED),
    ] {
        let mut pass = rect([x, 0.0], [8.0, 8.0], color);
        pass.set_stencil_test(clip_id);
        ctx.emit_draw_rect_pass(&mut graph, pass);
    }
    let (pixels, draws, instances) = render_rect_frame(graph, target, gpu)?;
    assert_eq!(
        (draws, instances),
        (1 + 3, 1 + 4),
        "a stencil reference change splits the run"
    );
    for (x, expected) in [
        (4, [255, 0, 0, 255]),
        (12, [255, 0, 0, 255]),
        (20, [0; 4]),
        (28, [255, 0, 0, 255]),
    ] {
        assert_eq!(pixel(&pixels, SIZE[0], x, 4), expected, "x={x}");
    }
    Ok(())
}

// Release thread-local text GPU objects before wgpu's own thread-local
// teardown, including when a pixel assertion panics.
struct TextGpuCleanup;
impl Drop for TextGpuCleanup {
    fn drop(&mut self) {
        crate::view::render_pass::text_pass::clear_text_resources_cache();
    }
}

fn blue_text_params() -> TextPassPreparedParams {
    let ifc = InlineFormattingContext::build_with_options(
        InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
            source: InlineIfcSourceId(1),
            text: "MMM".into(),
            style: Some(InlineIfcStyle {
                font_size: 12.0,
                line_height: 1.2,
                font_weight: 400,
                brush: [0, 0, 255, 255],
                font_families: vec!["sans-serif".into()].into(),
                vertical_align: crate::style::VerticalAlign::Baseline,
            }),
        }]),
        InlineIfcLayoutOptions::new(Some(40.0), true),
    );
    let staging_input =
        crate::view::inline_text_pass_adapter::inline_ifc_paint_input_to_text_pass_staging_input(
            &ifc.text_pass_paint_input(),
            [0.0, 0.0],
            1.0,
            0,
            1.0,
        );
    assert!(!staging_input.glyphs.is_empty());
    TextPassPreparedParams {
        staging_input,
        fragments: vec![TextPassPreparedFragment {
            origin: [4.0, 4.0],
            size: [36.0, 24.0],
        }],
        scissor_rect: None,
        stencil_clip_id: None,
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_rect_runs_flush_before_foreign_commands() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native graphics context");
    let _cleanup = TextGpuCleanup;
    let (mut graph, mut ctx, target) = primed_prelude(SIZE);
    ctx.emit_draw_rect_pass(&mut graph, rect([0.0, 0.0], [44.0, 32.0], RED));
    graph.add_graphics_pass(TextPreparedInputPass::new(
        blue_text_params(),
        TextInput {
            pass_context: ctx.graphics_pass_context(),
        },
        TextOutput {
            render_target: target,
        },
    ));
    ctx.emit_draw_rect_pass(&mut graph, rect([48.0, 0.0], [8.0, 8.0], RED));
    let (pixels, draws, instances) = render_rect_frame(graph, target, gpu)?;
    assert_eq!(
        (draws, instances),
        (1 + 2, 1 + 2),
        "the text pass splits the run"
    );
    // The pending rect must reach the render pass before the text commands:
    // drawn afterwards it would cover every glyph. Skip the anti-aliased
    // outermost pixel ring.
    let mut glyph_pixels = 0;
    for y in 1..SIZE[1] - 1 {
        for x in 1..43 {
            let [r, g, b, a] = pixel(&pixels, SIZE[0], x, y);
            assert_eq!(a, 255, "the rect stays opaque under the text at ({x},{y})");
            assert_eq!(g, 0);
            if b > r {
                glyph_pixels += 1;
            }
        }
    }
    assert!(glyph_pixels > 10, "text must draw over the earlier rect");
    assert_eq!(pixel(&pixels, SIZE[0], 52, 4), [255, 0, 0, 255]);
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_rect_run_blends_instances_in_order() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native graphics context");
    let (mut graph, mut ctx, target) = primed_prelude(SIZE);
    ctx.emit_draw_rect_pass(
        &mut graph,
        rect([0.0, 0.0], [16.0, 8.0], [1.0, 0.0, 0.0, 0.5]),
    );
    ctx.emit_draw_rect_pass(
        &mut graph,
        rect([8.0, 0.0], [16.0, 8.0], [0.0, 0.0, 1.0, 0.5]),
    );
    let (pixels, draws, instances) = render_rect_frame(graph, target, gpu)?;
    assert_eq!(
        (draws, instances),
        (1 + 1, 1 + 2),
        "translucent rects share one draw"
    );
    let red = premultiply([1.0, 0.0, 0.0, 0.5]);
    let blue = premultiply([0.0, 0.0, 1.0, 0.5]);
    for (x, expected) in [(4, red), (12, source_over(blue, red)), (20, blue)] {
        assert_near(
            pixel(&pixels, SIZE[0], x, 4),
            premultiplied_to_readback_rgba8(expected),
            &format!("x={x}"),
        );
    }
    Ok(())
}

fn follower_origin(index: u32) -> [f32; 2] {
    [(index % 20 * 4) as f32, (32 + index / 20 * 8) as f32]
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_gradient_stops_survive_growth_within_a_frame() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native graphics context");
    let size = [96, 48];
    let split = GradientPaint {
        axis: [0.0, 0.0, 24.0, 0.0],
        stops: [(0.0, RED), (0.5, RED), (0.5, BLUE), (1.0, BLUE)]
            .map(|(position, color)| GradientStopGpu {
                color,
                pos: [position, 0.0, 0.0, 0.0],
            })
            .into(),
        ..Default::default()
    };
    let green = GradientPaint {
        axis: [0.0, 0.0, 2.0, 0.0],
        stops: (0..8)
            .map(|index| GradientStopGpu {
                color: [0.0, 1.0, 0.0, 1.0],
                pos: [index as f32 / 7.0, 0.0, 0.0, 0.0],
            })
            .collect::<Vec<_>>()
            .into(),
        ..Default::default()
    };
    let mut viewport = Viewport::new();
    let mut sizes = Vec::new();
    for followers in [0, 40] {
        let (mut graph, mut ctx, target) = primed_prelude(size);
        ctx.emit_draw_rect_pass(&mut graph, gradient_rect([8.0, 8.0], [24.0, 16.0], &split));
        // 4 + 40 * 8 stops outgrow the 256-stop buffer the first frame left.
        for index in 0..followers {
            let follower = gradient_rect(follower_origin(index), [4.0, 4.0], &green);
            ctx.emit_draw_rect_pass(&mut graph, follower);
        }
        add_present(&mut graph, &target)?;
        let pixels = render_on_viewport_with_size(graph, gpu, &mut viewport, 1.0, FORMAT, size)?;
        sizes.push(viewport.gradient_stops_buffer_size_for_test());
        for (x, y, expected) in [(14, 16, [255, 0, 0, 255]), (26, 16, [0, 0, 255, 255])] {
            assert_eq!(
                pixel(&pixels, size[0], x, y),
                expected,
                "first gradient ({x},{y}) with {followers} followers"
            );
        }
        for index in 0..followers {
            let [x, y] = follower_origin(index).map(|v| v as u32 + 2);
            assert_eq!(
                pixel(&pixels, size[0], x, y),
                [0, 255, 0, 255],
                "follower {index}"
            );
        }
    }
    assert_eq!(
        sizes,
        [
            Some(crate::view::render_pass::draw_rect_pass::GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY),
            Some(
                crate::view::render_pass::draw_rect_pass::GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY
                    * 2
            ),
        ],
        "the second frame must grow the gradient buffer"
    );
    Ok(())
}
