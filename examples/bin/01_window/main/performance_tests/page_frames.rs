use super::window_drag::text_center;
use super::*;
use crate::page::Page;
use rfgui::view::viewport::PointerButton;

const CPU_PHASES: [&str; 6] = ["total", "layout", "prepare", "sync", "build", "compile"];
const CPU_PHASE_INDEX: [usize; 6] = [0, 2, 3, 4, 5, 6];

#[derive(Clone, Copy)]
enum Interaction {
    /// No input: the steady frame of an unchanged page.
    Idle,
    /// Wheel over the page, down then back up.
    Scroll,
    /// Pointer sweep across the page content.
    Hover,
    /// Click the label every frame, toggling the state it owns.
    Toggle(&'static str),
    /// Open the select showing `Option 1`, then wheel through its options.
    SelectScroll,
}

fn click(viewport: &mut Viewport, (x, y): (f32, f32)) {
    viewport.set_pointer_position_viewport(x, y);
    viewport.dispatch_pointer_move_event();
    viewport.set_pointer_button_pressed(PointerButton::Left, true);
    viewport.dispatch_pointer_down_event(PointerButton::Left);
    viewport.set_pointer_button_pressed(PointerButton::Left, false);
    viewport.dispatch_pointer_up_event(PointerButton::Left);
    viewport.dispatch_click_event(PointerButton::Left);
}

fn percentile(sorted: &[f64], p: usize) -> f64 {
    sorted[(sorted.len() * p).div_ceil(100) - 1]
}

/// Steady frames of heavy showcase pages through the production input,
/// App rebuild and offscreen render path (no window). The CPU phases split
/// layout, paint preparation, property sync and paint recording/planning
/// (`build`) from the GPU side. `RFGUI_PAGE_UI_PROFILE` also breaks down the
/// RSX work (component expansion, reconcile, commit); its counters add their
/// own cost, so compare frame times only between runs without it.
#[test]
#[ignore = "native hardware benchmark; run alone in release mode"]
fn native_showcase_page_frames() -> Result<(), String> {
    let _text_cache_cleanup = TextCacheCleanup;
    let (device, queue) = pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            flags: wgpu::InstanceFlags::empty(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|e| e.to_string())?;
        adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| e.to_string())
    })?;
    let frames = std::env::var("RFGUI_PAGE_FRAMES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(120);
    let only = std::env::var("RFGUI_PAGE_SCENE").ok();
    let ui_profile = std::env::var_os("RFGUI_PAGE_UI_PROFILE").is_some();
    const WARM: u64 = 40;
    const DPR: u32 = 2;
    let scenes = [
        ("licenses-idle", Page::Licenses, Interaction::Idle),
        ("licenses-scroll", Page::Licenses, Interaction::Scroll),
        ("text-hover", Page::Text, Interaction::Hover),
        (
            "inputs-toggle",
            Page::Inputs,
            Interaction::Toggle("Enable flag"),
        ),
        (
            "inputs-select-scroll",
            Page::Inputs,
            Interaction::SelectScroll,
        ),
        ("containers-hover", Page::Containers, Interaction::Hover),
    ];
    // Component state is thread-local: key each scene's scene root apart.
    for (case, (name, page, interaction)) in scenes.into_iter().enumerate() {
        if only.as_deref().is_some_and(|only| only != name) {
            continue;
        }
        let mut viewport = Viewport::new();
        let now = Instant::now();
        let mut root = None;
        let mut target = (0.0, 0.0);
        let mut frame_ms = Vec::new();
        let mut app_build_ms = Vec::new();
        let mut phases: [Vec<f64>; 6] = Default::default();
        let mut ui_work: Vec<rfgui::ui::UiWorkProfile> = Vec::new();
        for frame in 0..WARM + frames {
            let started = Instant::now();
            // Wheel the page until the control this scene needs is on screen.
            let label = match interaction {
                Interaction::Toggle(label) => Some(label),
                Interaction::SelectScroll => Some("Option 1"),
                _ => None,
            };
            if frame > 1
                && frame < WARM - 10
                && let Some(label) = label
                && text_center(&viewport, label).is_some_and(|(_, y)| y > 500.0)
            {
                viewport.set_pointer_position_viewport(760.0, 400.0);
                viewport.dispatch_pointer_move_event();
                viewport.dispatch_pointer_wheel_event(0.0, 80.0);
            }
            // Open the select early enough for its menu to settle.
            if frame == WARM - 10
                && let Interaction::SelectScroll = interaction
            {
                let select =
                    text_center(&viewport, "Option 1").ok_or("the select is not on screen")?;
                click(&mut viewport, select);
            }
            if frame == WARM {
                target = match interaction {
                    Interaction::Toggle(label) => {
                        text_center(&viewport, label).ok_or("toggle label is not on screen")?
                    }
                    Interaction::SelectScroll => {
                        text_center(&viewport, "Option 5").ok_or("the select menu is not open")?
                    }
                    _ => (760.0, 400.0),
                };
            }
            if frame >= WARM {
                let step = ((frame - WARM) % 80) as f32;
                let sweep = if step < 40.0 { step } else { 80.0 - step };
                match interaction {
                    Interaction::Idle => {}
                    Interaction::Scroll | Interaction::SelectScroll => {
                        viewport.set_pointer_position_viewport(target.0, target.1);
                        viewport.dispatch_pointer_move_event();
                        viewport.dispatch_pointer_wheel_event(
                            0.0,
                            if step < 40.0 { 60.0 } else { -60.0 },
                        );
                    }
                    Interaction::Hover => {
                        viewport.set_pointer_position_viewport(
                            target.0 - 300.0 + sweep * 15.0,
                            target.1 - 200.0 + sweep * 10.0,
                        );
                        viewport.dispatch_pointer_move_event();
                    }
                    Interaction::Toggle(_) => click(&mut viewport, target),
                }
            }
            let mut build_ms = 0.0;
            let mut run = || {
                rfgui::ui::batch_state_updates(|| {
                    if root.is_none() || rfgui::ui::peek_state_dirty().needs_rebuild() {
                        let build_started = Instant::now();
                        root = Some(rfgui::ui::render_root(
                            || rsx! { <MainScene key={case} initial_page={Some(page)} /> },
                        ));
                        build_ms = build_started.elapsed().as_secs_f64() * 1000.0;
                    }
                    viewport.render_rsx_offscreen_for_test(
                        root.as_ref().unwrap(),
                        device.clone(),
                        queue.clone(),
                        [1280 * DPR, 800 * DPR],
                        DPR as f32,
                        now + std::time::Duration::from_millis(frame * 16),
                    )
                })
            };
            let observation = if ui_profile {
                let (observation, work) = rfgui::ui::profile_ui_work(run);
                if frame > WARM {
                    ui_work.push(work);
                }
                observation
            } else {
                run()
            }?;
            let finished = Instant::now();
            device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(|e| e.to_string())?;
            if frame > WARM {
                frame_ms.push((finished - started).as_secs_f64() * 1000.0);
                app_build_ms.push(build_ms);
                for (values, index) in phases.iter_mut().zip(CPU_PHASE_INDEX) {
                    values.push(observation.cpu_ms[index]);
                }
            }
        }
        frame_ms.sort_by(f64::total_cmp);
        app_build_ms.sort_by(f64::total_cmp);
        let phase_summary = phases
            .iter_mut()
            .zip(CPU_PHASES)
            .map(|(values, name)| {
                values.sort_by(f64::total_cmp);
                format!(
                    "{name}={:.3}/{:.3}",
                    percentile(values, 50),
                    percentile(values, 95)
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "page-frames scene={name} dpr={DPR} nodes={} n={frames} frame_p50/p95_ms={:.3}/{:.3} app_build={:.3}/{:.3} render[{phase_summary}]",
            viewport.node_arena().len(),
            percentile(&frame_ms, 50),
            percentile(&frame_ms, 95),
            percentile(&app_build_ms, 50),
            percentile(&app_build_ms, 95),
        );
        if !ui_work.is_empty() {
            let p50 = |f: &dyn Fn(&rfgui::ui::UiWorkProfile) -> f64| {
                let mut values = ui_work.iter().map(f).collect::<Vec<_>>();
                values.sort_by(f64::total_cmp);
                percentile(&values, 50)
            };
            println!(
                "page-frames-ui scene={name} p50 state_flush={:.3} unwrap={:.3} reconcile={:.3} placement={:.3} translate={:.3} incr_commit={:.3} cold_commit={:.3} | renders={} memo_hits={} unwrap_nodes={} reconciled={} shared_hits={} patches={} fiber_works={}",
                p50(&|w| w.state_flush_ms),
                p50(&|w| w.unwrap_ms),
                p50(&|w| w.reconcile_ms),
                p50(&|w| w.placement_ms),
                p50(&|w| w.translate_ms),
                p50(&|w| w.incremental_commit_ms),
                p50(&|w| w.cold_commit_ms),
                p50(&|w| w.component_renders as f64),
                p50(&|w| w.memo_hits as f64),
                p50(&|w| w.unwrap_nodes as f64),
                p50(&|w| w.reconciled_nodes as f64),
                p50(&|w| w.shared_subtree_hits as f64),
                p50(&|w| w.patches as f64),
                p50(&|w| w.fiber_works as f64),
            );
        }
    }
    Ok(())
}
