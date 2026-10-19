//! A user component whose render panics: inside an `ErrorBoundary` its
//! fallback draws in its place and the rest of the tree is unaffected;
//! outside any boundary the whole tree unmounts instead of crashing.
use rfgui::style::{Align, BorderRadius, Color, FontSize, Layout, Length, Padding};
use rfgui::time::Instant;
use rfgui::ui::{ErrorBoundary, RsxNode, component, render_root, rsx};
use rfgui::view::{Element, Text, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [480, 260];
const GREEN: [u8; 3] = [46, 125, 50];
const ORANGE: [u8; 3] = [239, 108, 0];
const PURPLE: [u8; 3] = [106, 27, 154];
const PANEL: [u8; 3] = [225, 235, 250];
const BAR: [u8; 3] = [229, 57, 53];
const FALLBACK: [u8; 3] = [120, 52, 0];

#[component]
fn Stat(label: &str, value: &str) -> RsxNode {
    rsx! {
        <Element style={{ layout: Layout::flow().column().no_wrap() }}>
            <Text style={{ color: Color::rgb(255, 255, 255), font_size: FontSize::px(22.0) }}>
                {value}
            </Text>
            <Text style={{ color: Color::rgb(230, 230, 230) }}>{label}</Text>
        </Element>
    }
}

/// Panics on an empty series, the way a render body that trusts its data does.
#[component]
fn RevenueChart(points: &[f32]) -> RsxNode {
    let peak = points
        .iter()
        .copied()
        .reduce(f32::max)
        .expect("RevenueChart: no data points");
    let [r, g, b] = BAR;
    rsx! {
        <Element style={{
            height: Length::px(50.0),
            layout: Layout::flex().row().align(Align::End),
            gap: Length::px(4.0),
        }}>
            {points.iter().map(|point| rsx! {
                <Element style={{
                    width: Length::px(14.0),
                    height: Length::px(50.0 * point / peak),
                    background: Color::rgb(r, g, b),
                }} />
            }).collect::<Vec<_>>()}
        </Element>
    }
}

#[component]
fn ChartPanel(points: &[f32]) -> RsxNode {
    let [r, g, b] = PANEL;
    rsx! {
        <Element style={{
            width: Length::px(120.0),
            height: Length::px(70.0),
            background: Color::rgb(r, g, b),
            border_radius: BorderRadius::uniform(Length::px(4.0)),
            padding: Padding::new().top(Length::px(10.0)).left(Length::px(10.0)),
        }}>
            <RevenueChart points={points.to_vec()} />
        </Element>
    }
}

#[component]
fn Card(title: &str, accent: [u8; 3], children: Vec<RsxNode>) -> RsxNode {
    let [r, g, b] = accent;
    rsx! {
        <Element style={{
            width: Length::px(140.0),
            height: Length::px(200.0),
            background: Color::rgb(r, g, b),
            border_radius: BorderRadius::uniform(Length::px(8.0)),
            padding: Padding::new().top(Length::px(10.0)).left(Length::px(10.0)),
            layout: Layout::flow().column().no_wrap(),
            gap: Length::px(8.0),
        }}>
            <Text style={{ color: Color::rgb(255, 255, 255) }}>{title}</Text>
            {children}
        </Element>
    }
}

#[component]
fn ChartUnavailable() -> RsxNode {
    let [r, g, b] = FALLBACK;
    rsx! {
        <Element style={{
            width: Length::px(120.0),
            height: Length::px(70.0),
            background: Color::rgb(r, g, b),
            border_radius: BorderRadius::uniform(Length::px(4.0)),
            padding: Padding::new().top(Length::px(26.0)).left(Length::px(10.0)),
        }}>
            <Text style={{ color: Color::rgb(255, 255, 255) }}>Chart unavailable</Text>
        </Element>
    }
}

/// The revenue chart sits inside an `ErrorBoundary` when `boundary` is set.
#[component]
fn Dashboard(revenue: &[f32], boundary: bool) -> RsxNode {
    let chart = rsx! { <ChartPanel points={revenue.to_vec()} /> };
    let chart = if boundary {
        rsx! { <ErrorBoundary fallback={rsx! { <ChartUnavailable /> }}>{chart}</ErrorBoundary> }
    } else {
        chart
    };
    rsx! {
        <Element style={{
            width: Length::px(480.0),
            height: Length::px(260.0),
            background: Color::rgb(30, 30, 36),
            padding: Padding::new().top(Length::px(12.0)).left(Length::px(16.0)),
            layout: Layout::flow().column().no_wrap(),
            gap: Length::px(12.0),
        }}>
            <Text style={{ color: Color::rgb(255, 255, 255), font_size: FontSize::px(18.0) }}>
                Dashboard
            </Text>
            <Element style={{ layout: Layout::flex().row(), gap: Length::px(12.0) }}>
                <Card title="Users" accent={GREEN}>
                    <Stat label="active" value="1,204" />
                </Card>
                <Card title="Revenue" accent={ORANGE}>
                    <Stat label="this week" value="$8.3k" />
                    {chart}
                </Card>
                <Card title="Errors" accent={PURPLE}>
                    <Stat label="today" value="3" />
                </Card>
            </Element>
        </Element>
    }
}

/// The offscreen target is not sRGB, so it holds linear values; a window's
/// sRGB surface would encode them for display.
fn to_srgb(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        for c in &mut pixel[..3] {
            let linear = f32::from(*c) / 255.0;
            let encoded = if linear <= 0.003_130_8 {
                12.92 * linear
            } else {
                1.055 * linear.powf(1.0 / 2.4) - 0.055
            };
            *c = (encoded * 255.0).round() as u8;
        }
    }
}

fn count(pixels: &[u8], [r, g, b]: [u8; 3]) -> usize {
    pixels
        .chunks_exact(4)
        .filter(|p| p[0].abs_diff(r) < 8 && p[1].abs_diff(g) < 8 && p[2].abs_diff(b) < 8)
        .count()
}

/// Renders the dashboard with `revenue` and saves the frame as `name`.
fn render(
    gpu: &gpu::Gpu,
    viewport: &mut Viewport,
    revenue: Vec<f32>,
    boundary: bool,
    name: &str,
) -> Result<Vec<u8>, String> {
    let root = render_root(|| rsx! { <Dashboard revenue={revenue} boundary={boundary} /> });
    let frame = viewport.render_rsx_offscreen_for_test(
        &root,
        gpu.device.clone(),
        gpu.queue.clone(),
        SIZE,
        1.0,
        Instant::now(),
    )?;
    let mut pixels = gpu.read(&frame.texture, SIZE)?;
    to_srgb(&mut pixels);
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    image::save_buffer(&path, &pixels, SIZE[0], SIZE[1], image::ColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    eprintln!("frame saved to {}", path.display());
    Ok(pixels)
}

fn cards(pixels: &[u8]) -> [usize; 3] {
    [GREEN, ORANGE, PURPLE].map(|color| count(pixels, color))
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn error_boundary_draws_its_fallback_in_place_of_a_panicking_component() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let mut viewport = Viewport::new();
    let pixels = render(
        &gpu,
        &mut viewport,
        Vec::new(),
        true,
        "component_render_panic_boundary.png",
    )?;
    assert!(
        cards(&pixels).iter().all(|&n| n > 10_000),
        "a card is missing"
    );
    assert!(count(&pixels, FALLBACK) > 5_000, "fallback missing");
    assert_eq!(
        count(&pixels, PANEL) + count(&pixels, BAR),
        0,
        "the failed chart drew"
    );
    Ok(())
}

/// The frame drawn before the panic does not stay on screen.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn panic_outside_any_boundary_unmounts_the_tree() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let mut viewport = Viewport::new();
    let drawn = render(
        &gpu,
        &mut viewport,
        vec![3.0, 5.0, 8.0],
        false,
        "component_render_panic_before.png",
    )?;
    assert!(
        cards(&drawn).iter().all(|&n| n > 10_000),
        "a card is missing"
    );
    let pixels = render(
        &gpu,
        &mut viewport,
        Vec::new(),
        false,
        "component_render_panic_unmounted.png",
    )?;
    assert_eq!(cards(&pixels), [0; 3], "the unmounted tree drew");
    Ok(())
}
