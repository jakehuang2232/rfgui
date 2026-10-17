use crate::rfgui::style::{
    ClipMode, Color, Layout, Length, Padding, Position, Transition, TransitionProperty,
};
use crate::rfgui::ui::{Binding, RsxNode, component, rsx, use_state, use_viewport};
use crate::rfgui_components::{Checkbox, Switch, Window, on_move, use_theme};
use rfgui::view::Element;
use std::rc::Rc;

/// Where the Inspector first opens: the top right of the default window.
const INITIAL_POSITION: (f32, f32) = (896.0, 24.0);

/// A floating window of theme, animation and renderer debug switches.
///
/// It stays mounted while closed, so its switches keep showing the debug
/// options the viewport still has on.
#[component]
pub fn InspectorPanel(
    open: bool,
    dark_mode: Binding<bool>,
    on_dark_mode: Rc<dyn Fn(bool)>,
    animation_on: Binding<bool>,
) -> RsxNode {
    let position = use_state(|| INITIAL_POSITION);
    let debug_geometry_overlay = use_state(|| false);
    let debug_render_time = use_state(|| false);
    let detail_layout = use_state(|| false);
    let detail_compile = use_state(|| false);
    let detail_execute = use_state(|| false);
    let debug_retained_auto = use_state(|| false);
    let show_reuse = use_state(|| true);
    let show_reraster = use_state(|| true);
    let show_transient = use_state(|| true);

    let show_render_detail = debug_render_time.get();
    let show_retained_auto_detail = debug_retained_auto.get();

    // One viewport handle captured up front; cloned into every callback.
    let viewport = use_viewport();
    let (theme, _) = use_theme();

    let on_geometry_overlay = {
        let vp = viewport;
        Rc::new(move |on: bool| vp.set_debug_geometry_overlay(on)) as Rc<dyn Fn(bool)>
    };
    let on_render_time = {
        let vp = viewport;
        Rc::new(move |on: bool| vp.set_debug_trace_render_time(on)) as Rc<dyn Fn(bool)>
    };
    let on_detail_layout = {
        let vp = viewport;
        Rc::new(move |on: bool| vp.set_debug_trace_layout_detail(on)) as Rc<dyn Fn(bool)>
    };
    let on_detail_compile = {
        let vp = viewport;
        Rc::new(move |on: bool| vp.set_debug_trace_compile_detail(on)) as Rc<dyn Fn(bool)>
    };
    let on_detail_execute = {
        let vp = viewport;
        Rc::new(move |on: bool| vp.set_debug_trace_execute_detail(on)) as Rc<dyn Fn(bool)>
    };
    let on_retained_auto = {
        let vp = viewport;
        Rc::new(move |on: bool| vp.set_debug_retained_auto_overlay(on)) as Rc<dyn Fn(bool)>
    };
    let on_show_reuse = {
        let vp = viewport;
        Rc::new(move |on: bool| vp.set_debug_retained_auto_show_reuse(on)) as Rc<dyn Fn(bool)>
    };
    let on_show_reraster = {
        let vp = viewport;
        Rc::new(move |on: bool| vp.set_debug_retained_auto_show_reraster(on)) as Rc<dyn Fn(bool)>
    };
    let on_show_transient = {
        let vp = viewport;
        Rc::new(move |on: bool| vp.set_debug_retained_auto_show_transient(on)) as Rc<dyn Fn(bool)>
    };
    let swatch = |color: &str| {
        rsx! {
            <Element style={{
                width: Length::px(12.0),
                height: Length::px(12.0),
                background: Color::hex(color),
            }} />
        }
    };

    if !open {
        return RsxNode::fragment(Vec::new());
    }
    let on_window_move = {
        let position = position.clone();
        on_move(move |x, y| position.set((x, y)))
    };

    rsx! {
        <Window
            title="Inspector Panel"
            width={360.0}
            height={240.0}
            position={position.get()}
            on_move={on_window_move}
        >
            <Element style={{
                gap: theme.spacing.xs,
                layout: Layout::flow().column().no_wrap(),
                width: Length::percent(100.0),
            }}>
                <Switch
                    label="Dark mode"
                    binding={dark_mode}
                    on_change={on_dark_mode}
                />
                <Switch
                    label="Animation On"
                    binding={animation_on}
                />
                <Element style={{ layout: Layout::flow().column().no_wrap() }}>
                    <Switch
                        label="Debug Render Time"
                        binding={debug_render_time.binding()}
                        on_change={on_render_time}
                    />
                    <Element style={{
                        layout: Layout::flow().column().no_wrap(),
                        gap: theme.spacing.xs,
                        padding: Padding::new().left(theme.spacing.md).y(theme.spacing.xs),
                        position: Position::static_().clip(ClipMode::Parent),
                        height: if show_render_detail { None } else { Length::Zero },
                        transition: [
                            Transition::new(
                                TransitionProperty::Height,
                                theme.motion.duration.normal,
                            )
                            .ease_in_out(),
                        ],
                    }}>
                        <Checkbox
                            label="Layout Detail"
                            binding={detail_layout.binding()}
                            on_change={on_detail_layout}
                        />
                        <Checkbox
                            label="Compile Detail"
                            binding={detail_compile.binding()}
                            on_change={on_detail_compile}
                        />
                        <Checkbox
                            label="Execute Detail"
                            binding={detail_execute.binding()}
                            on_change={on_detail_execute}
                        />
                    </Element>
                </Element>
                <Switch
                    label="Debug Geometry Overlay"
                    binding={debug_geometry_overlay.binding()}
                    on_change={on_geometry_overlay}
                />
                <Element style={{ layout: Layout::flow().column().no_wrap() }}>
                    <Switch
                        label="Debug RetainedAuto"
                        binding={debug_retained_auto.binding()}
                        on_change={on_retained_auto}
                    />
                    <Element style={{
                        layout: Layout::flow().column().no_wrap(),
                        gap: theme.spacing.xs,
                        padding: Padding::new().left(theme.spacing.md).y(theme.spacing.xs),
                        position: Position::static_().clip(ClipMode::Parent),
                        height: if show_retained_auto_detail { None } else { Length::Zero },
                        transition: [
                            Transition::new(
                                TransitionProperty::Height,
                                theme.motion.duration.normal,
                            )
                            .ease_in_out(),
                        ],
                    }}>
                        <Element style={{ layout: Layout::flow().row().no_wrap(), gap: theme.spacing.xs }}>
                            {swatch("#26f25a")}
                            <Checkbox
                                label="Reuse"
                                binding={show_reuse.binding()}
                                on_change={on_show_reuse}
                            />
                        </Element>
                        <Element style={{ layout: Layout::flow().row().no_wrap(), gap: theme.spacing.xs }}>
                            {swatch("#ff731a")}
                            <Checkbox
                                label="Reraster"
                                binding={show_reraster.binding()}
                                on_change={on_show_reraster}
                            />
                        </Element>
                        <Element style={{ layout: Layout::flow().row().no_wrap(), gap: theme.spacing.xs }}>
                            {swatch("#ff3b30")}
                            <Checkbox
                                label="Transient"
                                binding={show_transient.binding()}
                                on_change={on_show_transient}
                            />
                        </Element>
                    </Element>
                </Element>
            </Element>
        </Window>
    }
}
