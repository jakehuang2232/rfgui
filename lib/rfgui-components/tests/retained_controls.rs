//! Real component crate -> RSX reconciliation -> layout -> production renderer.
//! No recorded-artifact injection, legacy pixel oracle, or fallback exemptions.
use rfgui::style::{Color, Layout, Length, Opacity};
use rfgui::ui::{Binding, ClickHandlerProp, RsxNode, rsx};
use rfgui::view::viewport::{PointerButton, ViewportPaintRendererMode};
use rfgui::view::{Element, Text, Viewport};
use rfgui_components::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[path = "retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [640, 480];
const CASES: [&str; 15] = [
    "Button",
    "IconButton",
    "Checkbox",
    "Switch",
    "ToggleButton",
    "ToggleButtonGroup",
    "NumberField",
    "Slider",
    "Select",
    "Accordion",
    "TreeView",
    "Window",
    "Alert",
    "Snackbar",
    "Tooltip",
];
struct Model {
    flag: Binding<bool>,
    number: Binding<f64>,
    selected: Binding<String>,
    group: Binding<Option<String>>,
    expanded: Binding<Vec<String>>,
    selection: Binding<Option<String>>,
    position: Binding<(f32, f32)>,
    clicks: Rc<Cell<u32>>,
    tooltip: RefCell<Option<TooltipRef>>,
}
impl Model {
    fn new() -> Self {
        Self {
            position: Binding::new((20.0, 20.0)),
            flag: Binding::new(false),
            number: Binding::new(25.0),
            selected: Binding::new("A".into()),
            group: Binding::new(None),
            expanded: Binding::new(vec![]),
            selection: Binding::new(None),
            clicks: Rc::new(Cell::new(0)),
            tooltip: RefCell::new(None),
        }
    }
    fn click(&self) -> ClickHandlerProp {
        let n = self.clicks.clone();
        ClickHandlerProp::new(move |_| n.set(n.get() + 1))
    }
}
fn label(s: &String, _: usize) -> String {
    s.clone()
}
fn scene(case: &str, m: &Model) -> RsxNode {
    rfgui::ui::rsx_scope(|| {
        rfgui::ui::render_component::<Model, _>(|| {
            let tooltip = use_tooltip_ref();
            m.tooltip.replace(Some(tooltip.clone()));
            let content = match case {
                "Button" => rsx! {<Button on_click={m.click()}>"Target"</Button>},
                "IconButton" => {
                    rsx! {<IconButton on_click={m.click()}><material_symbol::CloseIcon /></IconButton>}
                }
                "Checkbox" => rsx! {<Checkbox label="Target" binding={m.flag.clone()}/>},
                "Switch" => rsx! {<Switch label="Target" binding={m.flag.clone()}/>},
                "ToggleButton" => {
                    rsx! {<ToggleButton selected={m.flag.get()} on_click={m.click()}>"Target"</ToggleButton>}
                }
                "ToggleButtonGroup" => {
                    rsx! {<ToggleButtonGroup value={m.group.clone()}><ToggleButton value="A">"Choice A"</ToggleButton><ToggleButton value="B">"Choice B"</ToggleButton></ToggleButtonGroup>}
                }
                "NumberField" => {
                    rsx! {<NumberField::<f64> binding={m.number.clone()} label="Value" min=0.0 max=100.0 step=1.0/>}
                }
                "Slider" => {
                    rsx! {<Slider binding={m.number.clone()} label="Value" min=0.0 max=100.0/>}
                }
                "Select" => {
                    rsx! {<Select::<String,String> value={m.selected.clone()} data={vec!["A".into(),"B".into()]} to_label={label as fn(&String,usize)->String}/>}
                }
                "Accordion" => {
                    rsx! {<Accordion title="Target" expanded_binding={m.flag.clone()}><Text>"Expanded content"</Text></Accordion>}
                }
                "TreeView" => {
                    rsx! {<TreeView::<String> nodes={vec![BranchNode::new("branch","Branch").with_children(vec![TreeNode::leaf("leaf","Leaf")]).into()]} expanded_binding={m.expanded.clone()} selected_binding={m.selection.clone()}/>}
                }
                "Window" => {
                    let pos = m.position.clone();
                    rsx! {<Window title="Target" width=240.0 height=160.0 position={m.position.get()} on_move={on_move(move |x,y|pos.set((x,y)))}><Text>"Window content"</Text></Window>}
                }
                "Alert" => {
                    let n = m.clicks.clone();
                    rsx! {<Alert on_close={Rc::new(move ||n.set(n.get()+1)) as Rc<dyn Fn()>}>"Target"</Alert>}
                }
                "Snackbar" => {
                    let n = m.clicks.clone();
                    rsx! {<Snackbar open={m.flag.get()} message={rsx!{<Text>"Message"</Text>}} action={rsx!{<Button on_click={m.click()}>"Action"</Button>}} on_close={Rc::new(move |_|n.set(n.get()+1)) as Rc<dyn Fn(SnackbarCloseReason)>}/>}
                }
                "Tooltip" => {
                    rsx! {<Element><Text>"Anchor"</Text><Tooltip handle={tooltip}><Text>"Tip"</Text></Tooltip></Element>}
                }
                _ => panic!("unknown control"),
            };
            rsx! {<Element style={{width:Length::px(320.0),height:Length::px(240.0),layout:Layout::Grid,background:Color::rgb(40,60,80),opacity:Opacity::new(0.75)}}>{content}</Element>}
        })
    })
}
fn text_point(v: &Viewport, needle: &str) -> [f32; 2] {
    let a = v.node_arena();
    fn find(a: &rfgui::view::NodeArena, k: rfgui::view::NodeKey, n: &str) -> Option<[f32; 2]> {
        let node = a.get(k)?;
        if let Some(t) = node
            .element
            .as_any()
            .downcast_ref::<rfgui::view::base_component::Text>()
        {
            if t.content() == n {
                let b = node.element.box_model_snapshot();
                return Some([b.x + b.width * 0.5, b.y + b.height * 0.5]);
            }
        }
        drop(node);
        a.children_of(k).iter().find_map(|k| find(a, *k, n))
    }
    a.roots()
        .iter()
        .find_map(|k| find(a, *k, needle))
        .unwrap_or_else(|| panic!("missing label {needle}"))
}
fn click(v: &mut Viewport, p: [f32; 2]) {
    v.set_pointer_position_viewport(p[0], p[1]);
    v.dispatch_pointer_move_event();
    v.set_pointer_button_pressed(PointerButton::Left, true);
    v.dispatch_pointer_down_event(PointerButton::Left);
    v.set_pointer_button_pressed(PointerButton::Left, false);
    v.dispatch_pointer_up_event(PointerButton::Left);
    v.dispatch_click_event(PointerButton::Left);
}

fn interact(case: &str, m: &Model, v: &mut Viewport, reverse: bool) {
    let target = match case {
        "IconButton" | "Alert" => "close",
        "ToggleButtonGroup" => "Choice A",
        "NumberField" => {
            if reverse {
                "remove"
            } else {
                "add"
            }
        }
        "Select" => {
            if reverse {
                "B"
            } else {
                "A"
            }
        }
        "TreeView" => "Branch",
        _ => "Target",
    };
    match case {
        "Slider" => {
            let a = v.node_arena();
            let root = a.roots()[0];
            let control = a.children_of(root)[0];
            let track = a.children_of(control)[0];
            let b = a.get(track).unwrap().element.box_model_snapshot();
            let ratio = if reverse { 0.25 } else { 0.75 };
            click(
                v,
                [b.x + 8.0 + (b.width - 16.0) * ratio, b.y + b.height * 0.5],
            );
            assert!((m.number.get() - if reverse { 25.0 } else { 75.0 }).abs() < 1.0);
        }
        "Window" => {
            let p = text_point(v, "Target").map(f32::round);
            v.set_pointer_position_viewport(p[0], p[1]);
            v.set_pointer_button_pressed(PointerButton::Left, true);
            v.dispatch_pointer_down_event(PointerButton::Left);
            let delta = if reverse { -20.0 } else { 20.0 };
            v.set_pointer_position_viewport(p[0] + delta, p[1] + delta);
            v.dispatch_pointer_move_event();
            v.set_pointer_button_pressed(PointerButton::Left, false);
            v.dispatch_pointer_up_event(PointerButton::Left);
        }
        "Snackbar" => {
            if reverse {
                click(v, text_point(v, "Action"));
                assert_eq!(m.clicks.get(), 1);
            }
            m.flag.set(!reverse);
        }
        "Tooltip" => {
            let t = m.tooltip.borrow();
            let t = t.as_ref().unwrap();
            if reverse { t.hide() } else { t.show() }
        }
        _ => {
            click(v, text_point(v, target));
        }
    }
    if case == "ToggleButton" {
        m.flag.set(!reverse);
    }
    match case {
        "Button" | "IconButton" | "ToggleButton" | "Alert" => assert_eq!(
            m.clicks.get(),
            if reverse { 2 } else { 1 },
            "{case} callback"
        ),
        "Checkbox" | "Switch" | "Accordion" => assert_eq!(m.flag.get(), !reverse, "{case} binding"),
        "ToggleButtonGroup" => {
            assert_eq!(m.group.get(), if reverse { None } else { Some("A".into()) })
        }
        "NumberField" => assert_eq!(m.number.get(), if reverse { 25.0 } else { 26.0 }),
        "Select" if reverse => assert_eq!(m.selected.get(), "B"),
        "Window" => assert_eq!(
            m.position.get(),
            if reverse { (20.0, 20.0) } else { (40.0, 40.0) }
        ),
        "TreeView" => {
            assert_eq!(m.selection.get(), Some("branch".into()));
            assert_eq!(
                m.expanded.get(),
                if reverse {
                    vec![]
                } else {
                    vec!["branch".to_owned()]
                }
            );
        }
        _ => {}
    }
    // Neutral pointer position makes warm/round-trip comparisons independent
    // of a lingering hover on the control that was just clicked.
    v.set_pointer_position_viewport(600.0, 400.0);
    v.dispatch_pointer_move_event();
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_controls_record_and_reuse_real_component_trees() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for case in CASES {
        for mode in [
            ViewportPaintRendererMode::RetainedAuto,
            ViewportPaintRendererMode::Legacy,
        ] {
            for dpr in [1_u32, 2] {
                let m = Model::new();
                let mut v = Viewport::new();
                v.set_paint_renderer_mode(mode);
                let now = rfgui::time::Instant::now();
                let mut previous = None;
                let mut baseline: Option<Vec<u8>> = None;
                let mut changed: Option<Vec<u8>> = None;
                let mut previous_resources = None;
                let mut state_rasters = 0;
                for frame in 0..9 {
                    if frame == 3 || frame == 6 {
                        state_rasters = 0;
                        interact(case, &m, &mut v, frame == 6);
                    }
                    let tree = scene(case, &m);
                    let o = v
                        .render_rsx_offscreen_for_test(
                            &tree,
                            gpu.device.clone(),
                            gpu.queue.clone(),
                            SIZE.map(|s| s * dpr),
                            dpr as f32,
                            now + std::time::Duration::from_secs(frame),
                        )
                        .map_err(|e| format!("{case} {mode:?} dpr={dpr} frame={frame}: {e}"))?;
                    let pixels = gpu.read(&o.texture, SIZE.map(|s| s * dpr))?;
                    assert_eq!(v.scale_factor(), dpr as f32);
                    assert_eq!(v.logical_size(), (SIZE[0] as f32, SIZE[1] as f32));
                    // Absolute geometry/color probes: the 320x240 group paints a known
                    // opaque backdrop at 0.75 opacity, and cannot fill the viewport outside.
                    // Color::rgb is sRGB: its linear RGBA8 readback is [5,12,20];
                    // premultiplication/8-bit storage/unpremultiplication preserves these bytes.
                    for (name, [x, y], expected) in [
                        ("group backdrop", [300, 220], [5, 12, 20, 191]),
                        ("outside right", [630, 220], [0, 0, 0, 0]),
                        ("outside bottom", [300, 470], [0, 0, 0, 0]),
                    ] {
                        let index = ((y * dpr * SIZE[0] * dpr + x * dpr) * 4) as usize;
                        assert!(
                            pixels[index..index + 4]
                                .iter()
                                .zip(expected)
                                .all(|(a, b)| a.abs_diff(b) <= 1),
                            "{case} {mode:?} DPR={dpr} frame={frame} {name}: {:?}",
                            &pixels[index..index + 4]
                        );
                    }
                    state_rasters += o.rerasterizations;
                    if frame % 3 == 2 {
                        let old: &Vec<u8> = previous.as_ref().unwrap();
                        let diffs: Vec<_> = pixels
                            .chunks_exact(4)
                            .zip(old.chunks_exact(4))
                            .enumerate()
                            .filter(|(_, (a, b))| a != b)
                            .collect();
                        assert!(
                            diffs.is_empty(),
                            "{case} {mode:?} DPR={dpr} frame={frame} warm pixels: {} differences; first={:?}",
                            diffs.len(),
                            &diffs[..diffs.len().min(8)]
                        );
                        if o.artifact_selected {
                            assert_eq!(o.rerasterizations, 0, "{case} warm must reuse");
                            assert!(
                                o.reuses > 0 && o.resident_rasters > 0,
                                "{case} must reuse real backing"
                            );
                            assert_eq!(
                                previous_resources.as_ref(),
                                Some(&(o.texture_bytes, o.persistent_targets.clone())),
                                "{case} warm backing descriptors and identity"
                            );
                        }
                        if frame == 2 {
                            if case != "Snackbar" {
                                assert!(
                                    pixels
                                        .chunks_exact(4)
                                        .filter(|p| p[3] > 0 && *p != [5, 12, 20, 191])
                                        .count()
                                        > 20,
                                    "{case} must paint real control content"
                                );
                            }
                            baseline = Some(pixels.clone());
                        }
                        if frame == 5 {
                            if !matches!(case, "Button" | "IconButton" | "Alert") {
                                assert!(
                                    baseline.as_ref() != Some(&pixels),
                                    "{case} state change must have visible output"
                                );
                                if o.artifact_selected && case != "Window" {
                                    assert!(
                                        state_rasters > 0,
                                        "{case} changed content must invalidate"
                                    );
                                }
                            }
                            changed = Some(pixels.clone());
                        }
                        if frame == 8 && !matches!(case, "Button" | "IconButton" | "Alert") {
                            assert!(
                                changed.as_ref() != Some(&pixels),
                                "{case} reverse/close must have visible output"
                            );
                            if !matches!(case, "Select" | "TreeView") {
                                assert!(
                                    baseline.as_ref() == Some(&pixels),
                                    "{case} round-trip restores same-path pixels"
                                );
                            }
                        }
                    }
                    previous = Some(pixels);
                    previous_resources = Some((o.texture_bytes, o.persistent_targets));
                    eprintln!(
                        "control {case} {mode:?} DPR={dpr} frame={frame} pairs={} reuse={} raster={}",
                        o.resident_rasters, o.reuses, o.rerasterizations
                    );
                }
            }
        }
    }
    Ok(())
}
