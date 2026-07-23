use super::*;

#[test]
#[ignore = "manual place microbenchmark: cargo test --lib nested_inline_place_microbench -- --ignored --nocapture"]
fn nested_inline_place_microbench() {
    // Default layout is Inline post-S1: every container is an IFC root
    // whose element children are atomic inline boxes. This mirrors a real
    // app tree far better than a flat list of paragraphs.
    let tree = nested_default_layout_tree(4, 5, 0);

    let mut viewport = Viewport::new();
    viewport.set_size(1200, 3000);
    viewport.render_rsx(&tree).expect("render bench tree");
    run_layout_for_test(&mut viewport, 1200.0, 3000.0);

    let constraints = crate::view::base_component::LayoutConstraints {
        max_width: 1200.0,
        max_height: 3000.0,
        viewport_width: 1200.0,
        viewport_height: 3000.0,
        percent_base_width: Some(1200.0),
        percent_base_height: Some(3000.0),
    };
    for round in 0..6 {
        let placement = crate::view::base_component::LayoutPlacement {
            parent_x: (round + 1) as f32,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 1200.0,
            available_height: 3000.0,
            viewport_width: 1200.0,
            viewport_height: 3000.0,
            percent_base_width: Some(1200.0),
            percent_base_height: Some(3000.0),
        };
        crate::view::base_component::reset_layout_place_profile();
        crate::view::base_component::set_layout_place_profile_enabled(true);
        let mut arena = std::mem::take(&mut viewport.scene.node_arena);
        let root_keys = viewport.scene.ui_root_keys.clone();
        for &root in &root_keys {
            arena.refresh_subtree_dirty_cache(root);
        }
        for &root in &root_keys {
            arena.with_element_taken(root, |el, arena| {
                el.measure(constraints, arena);
            });
        }
        let place_started = crate::time::Instant::now();
        for &root in &root_keys {
            arena.refresh_subtree_dirty_cache(root);
        }
        for &root in &root_keys {
            arena.with_element_taken(root, |el, arena| {
                el.place(placement, arena);
            });
        }
        let place_ms = place_started.elapsed().as_secs_f64() * 1000.0;
        viewport.scene.node_arena = arena;
        crate::view::base_component::set_layout_place_profile_enabled(false);
        let profile = crate::view::base_component::take_layout_place_profile();
        println!(
            "round {round}: place={place_ms:.3}ms nodes={} child_place_calls={} skipped={}",
            profile.node_count, profile.child_place_calls, profile.skipped_child_place_calls
        );
        println!(
            "  place_self={:.3} place_children={:.3} child_place_excl={:.3} ifc_install={:.3} (calls={} reuse={}) update_content={:.3} clamp={:.3} hit_test={:.3} ifc_measure cheap/sc/full={}/{}/{}",
            profile.place_self_ms,
            profile.place_children_ms,
            profile.non_axis_child_place_ms,
            profile.inline_ifc_root_install_ms,
            profile.inline_ifc_root_install_calls,
            profile.inline_ifc_root_install_reuse_calls,
            profile.update_content_size_ms,
            profile.clamp_scroll_ms,
            profile.recompute_hit_test_ms,
            profile.ifc_measure_cheap,
            profile.ifc_measure_shortcircuit,
            profile.ifc_measure_full,
        );
    }
}

#[test]
#[ignore = "manual place microbenchmark: cargo test --lib inline_ifc_place_microbench -- --ignored --nocapture"]
fn inline_ifc_place_microbench() {
    use crate::view::Text as HostText;

    let paragraphs = (0..200)
        .map(|i| {
            rsx! {
                <HostElement style={{
                    layout: Layout::Inline,
                    width: Length::percent(100.0),
                }}>
                    <HostElement style={{
                        padding: Padding::uniform(Length::px(3.0)),
                    }}>
                        <HostText>{format!("badge {i}")}</HostText>
                    </HostElement>
                    <HostText>
                        {format!("paragraph body text number {i} with several words to shape and wrap")}
                    </HostText>
                </HostElement>
            }
        })
        .collect::<Vec<_>>();
    let tree = rsx! {
        <HostElement style={{
            layout: Layout::flow().column().no_wrap(),
            width: Length::px(800.0),
        }}>{paragraphs}</HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_size(800, 4000);
    viewport.render_rsx(&tree).expect("render bench tree");
    run_layout_for_test(&mut viewport, 800.0, 4000.0);

    let constraints = crate::view::base_component::LayoutConstraints {
        max_width: 800.0,
        max_height: 4000.0,
        viewport_width: 800.0,
        viewport_height: 4000.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(4000.0),
    };

    // Rounds 0-2 shift the origin (drag); rounds 3-5 repeat the same
    // placement (idle) — those should skip the whole tree.
    for round in 0..6 {
        let placement = crate::view::base_component::LayoutPlacement {
            parent_x: (round + 1).min(4) as f32,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 4000.0,
            viewport_width: 800.0,
            viewport_height: 4000.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(4000.0),
        };
        crate::view::base_component::reset_layout_place_profile();
        crate::view::base_component::set_layout_place_profile_enabled(true);
        let started = crate::time::Instant::now();
        let mut arena = std::mem::take(&mut viewport.scene.node_arena);
        let root_keys = viewport.scene.ui_root_keys.clone();
        for &root in &root_keys {
            arena.refresh_subtree_dirty_cache(root);
        }
        for &root in &root_keys {
            arena.with_element_taken(root, |el, arena| {
                el.measure(constraints, arena);
            });
        }
        let measure_ms = started.elapsed().as_secs_f64() * 1000.0;
        let place_started = crate::time::Instant::now();
        for &root in &root_keys {
            arena.refresh_subtree_dirty_cache(root);
        }
        for &root in &root_keys {
            arena.with_element_taken(root, |el, arena| {
                el.place(placement, arena);
            });
        }
        let place_ms = place_started.elapsed().as_secs_f64() * 1000.0;
        viewport.scene.node_arena = arena;
        crate::view::base_component::set_layout_place_profile_enabled(false);
        let profile = crate::view::base_component::take_layout_place_profile();
        println!(
            "round {round}: measure={measure_ms:.3}ms place={place_ms:.3}ms nodes={} child_place_calls={} skipped={}",
            profile.node_count, profile.child_place_calls, profile.skipped_child_place_calls
        );
        println!(
            "  place_self={:.3} place_children={:.3} child_place_excl={:.3} ifc_install={:.3} (calls={} reuse={}) update_content={:.3} clamp={:.3} hit_test={:.3} inline_axis={:.3}",
            profile.place_self_ms,
            profile.place_children_ms,
            profile.non_axis_child_place_ms,
            profile.inline_ifc_root_install_ms,
            profile.inline_ifc_root_install_calls,
            profile.inline_ifc_root_install_reuse_calls,
            profile.update_content_size_ms,
            profile.clamp_scroll_ms,
            profile.recompute_hit_test_ms,
            profile.place_layout_inline_ms,
        );
    }
}

#[test]
#[ignore = "manual drag microbenchmark: cargo test --lib rsx_window_drag_microbench -- --ignored --nocapture"]
fn rsx_window_drag_microbench() {
    fn bench_editor_lines() -> usize {
        std::env::var("BENCH_LINES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(200)
    }

    use crate::style::ClipMode::Parent;
    use crate::style::{Anchor, BorderRadius, BoxShadow};
    use crate::view::Text as HostText;
    use crate::view::TextArea as HostTextArea;

    // Mirrors rfgui-components Window drag: every frame updates the
    // absolute left/top style through a full rsx rebuild + reconcile.
    fn window_tree(x: f32, y: f32) -> RsxNode {
        let paragraphs = (0..40)
            .map(|i| {
                rsx! {
                    <HostElement style={{
                        layout: Layout::Inline,
                        width: Length::percent(100.0),
                    }}>
                        <HostElement style={{
                            padding: Padding::uniform(Length::px(3.0)),
                        }}>
                            <HostText>{format!("badge {i}")}</HostText>
                        </HostElement>
                        <HostText>
                            {format!("window body paragraph {i} with several words")}
                        </HostText>
                    </HostElement>
                }
            })
            .collect::<Vec<_>>();
        let windows = (0..7)
            .map(|w| {
                let body = (0..40)
                    .map(|i| {
                        rsx! {
                            <HostElement style={{
                                layout: Layout::Inline,
                                width: Length::percent(100.0),
                            }}>
                                <HostElement style={{
                                    padding: Padding::uniform(Length::px(3.0)),
                                }}>
                                    <HostText>{format!("badge {w}-{i}")}</HostText>
                                </HostElement>
                                <HostText>
                                    {format!("window {w} body paragraph {i} with several words")}
                                </HostText>
                            </HostElement>
                        }
                    })
                    .collect::<Vec<_>>();
                let (wx, wy) = if w == 0 {
                    (x, y)
                } else {
                    (30.0 + (w as f32) * 90.0, 120.0)
                };
                rsx! {
                    <HostElement
                        key={format!("window-{w}")}
                        style={{
                            position: Position::absolute()
                                .left(Length::px(wx))
                                .top(Length::px(wy))
                                .anchor(Anchor::Parent)
                                .clip(Parent),
                            layout: Layout::flow().column().no_wrap(),
                            width: Length::px(420.0),
                            height: Length::px(600.0),
                            border_radius: BorderRadius::uniform(Length::px(8.0)),
                            box_shadow: vec![BoxShadow {
                                offset_x: 0.0,
                                offset_y: 6.0,
                                blur: 24.0,
                                spread: 0.0,
                                ..BoxShadow::new()
                            }],
                        }}
                    >
                        <HostElement style={{
                            height: Length::px(32.0),
                        }}>
                            <HostText>{format!("Window {w}")}</HostText>
                        </HostElement>
                        <HostElement style={{
                            layout: Layout::flow().column().no_wrap(),
                            width: Length::percent(100.0),
                        }}>{body}</HostElement>
                        <HostTextArea content={
                            (0..(if w == 0 { bench_editor_lines() } else { 30 }))
                                .map(|line| format!("fn line_{line}() {{ let value = {line}; }}"))
                                .collect::<Vec<_>>()
                                .join("\n")
                        } />
                    </HostElement>
                }
            })
            .collect::<Vec<_>>();
        let _ = paragraphs;
        rsx! {
            <HostElement style={{
                width: Length::px(1200.0),
                height: Length::px(900.0),
            }}>{windows}</HostElement>
        }
    }

    let mut viewport = Viewport::new();
    viewport.set_size(1200, 900);
    viewport
        .render_rsx(&window_tree(50.0, 50.0))
        .expect("initial render");
    run_layout_for_test(&mut viewport, 1200.0, 900.0);

    for round in 0..6 {
        let x = 50.0 + ((round + 1) as f32) * 5.0;
        let rsx_started = crate::time::Instant::now();
        viewport
            .render_rsx(&window_tree(x, 50.0))
            .expect("drag render");
        let rsx_ms = rsx_started.elapsed().as_secs_f64() * 1000.0;
        let measure_started = crate::time::Instant::now();
        {
            let constraints = crate::view::base_component::LayoutConstraints {
                max_width: 1200.0,
                max_height: 900.0,
                viewport_width: 1200.0,
                viewport_height: 900.0,
                percent_base_width: Some(1200.0),
                percent_base_height: Some(900.0),
            };
            let mut arena = std::mem::take(&mut viewport.scene.node_arena);
            let root_keys = viewport.scene.ui_root_keys.clone();
            for &root in &root_keys {
                arena.refresh_subtree_dirty_cache(root);
            }
            for &root in &root_keys {
                arena.with_element_taken(root, |el, arena| {
                    el.measure(constraints, arena);
                });
            }
            viewport.scene.node_arena = arena;
        }
        let measure_ms = measure_started.elapsed().as_secs_f64() * 1000.0;
        let layout_started = crate::time::Instant::now();
        let (_gate, profile) = run_layout_for_test_with_gate_profile(&mut viewport, 1200.0, 900.0);
        let layout_ms = layout_started.elapsed().as_secs_f64() * 1000.0;
        println!("  pre-measure={measure_ms:.3}ms");
        println!(
            "round {round}: rsx={rsx_ms:.3}ms layout={layout_ms:.3}ms nodes={} child_place={} skipped={}",
            profile.node_count, profile.child_place_calls, profile.skipped_child_place_calls
        );
        println!(
            "  ifc_install={:.3}ms (calls={} reuse={}) place_self={:.3} place_children={:.3} child_place_excl={:.3} update_content={:.3} hit_test={:.3}",
            profile.inline_ifc_root_install_ms,
            profile.inline_ifc_root_install_calls,
            profile.inline_ifc_root_install_reuse_calls,
            profile.place_self_ms,
            profile.place_children_ms,
            profile.non_axis_child_place_ms,
            profile.update_content_size_ms,
            profile.recompute_hit_test_ms,
        );
        println!(
            "  ifc_measure cheap/sc/full={}/{}/{} measure_ran self/child/proposal={}/{}/{}",
            profile.ifc_measure_cheap,
            profile.ifc_measure_shortcircuit,
            profile.ifc_measure_full,
            profile.measure_ran_self_dirty,
            profile.measure_ran_child_dirty,
            profile.measure_ran_proposal_changed,
        );
    }
}
