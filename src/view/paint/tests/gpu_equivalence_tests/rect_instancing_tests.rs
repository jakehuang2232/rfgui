use super::*;
use crate::view::render_pass::draw_rect_pass::{GradientPaint, GradientStopGpu};

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

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

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
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
        let (mut graph, mut ctx, target) = transformed_graph_prelude_with_size(1.0, None, size);
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
