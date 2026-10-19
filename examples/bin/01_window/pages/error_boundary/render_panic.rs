use crate::rfgui::style::{Border, Layout, Length, Padding};
use crate::rfgui::ui::{
    ClickEvent, ErrorBoundary, ErrorBoundaryHandle, RenderError, RsxNode, component, rsx,
    use_context, use_state,
};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::{Button, ButtonColor, ButtonSize, use_theme};
use crate::showcase::Demo;
use std::rc::Rc;

/// A widget that cannot render its data once it is broken.
#[component]
fn Forecast(broken: bool) -> RsxNode {
    let (theme, _) = use_theme();
    if broken {
        panic!("Forecast received a reading it cannot display");
    }
    rsx! {
        <Element style={{
            width: Length::px(240.0),
            layout: Layout::flow().column().no_wrap(),
            gap: theme.spacing.xs,
            padding: Padding::uniform(theme.spacing.md),
            background: theme.color.layer.raised.clone(),
            border_radius: theme.radius.md,
        }}>
            <Text style={{ font_size: theme.typography.size.lg }}>{"Taipei · 24°C"}</Text>
            <Text style={{ color: theme.color.text.secondary.clone() }}>Partly cloudy</Text>
        </Element>
    }
}

/// What the boundary shows instead; it reads the caught error from context.
#[component]
fn ForecastFallback(on_recover: &Rc<dyn Fn()>) -> RsxNode {
    let (theme, _) = use_theme();
    let handle = use_context::<ErrorBoundaryHandle>();
    let message = handle
        .as_ref()
        .map(|handle| handle.error().message().to_string())
        .unwrap_or_default();
    let on_recover = on_recover.clone();
    let retry = move |_: &mut ClickEvent| {
        on_recover();
        if let Some(handle) = &handle {
            handle.reset();
        }
    };
    rsx! {
        <Element style={{
            width: Length::px(240.0),
            layout: Layout::flow().column().no_wrap(),
            gap: theme.spacing.sm,
            padding: Padding::uniform(theme.spacing.md),
            border: Border::uniform(Length::px(1.0), theme.color.error.base.as_ref()),
            border_radius: theme.radius.md,
        }}>
            <Text style={{ color: theme.color.error.base.clone() }}>The forecast failed to render</Text>
            <Text style={{ color: theme.color.text.secondary.clone() }}>{message}</Text>
            <Button size={Some(ButtonSize::Small)} on_click={retry}>Try again</Button>
        </Element>
    }
}

#[component]
pub fn RenderPanic() -> RsxNode {
    let (theme, _) = use_theme();
    let broken = use_state(|| false);
    let caught = use_state(|| 0_u32);
    let break_it = {
        let broken = broken.clone();
        move |_: &mut ClickEvent| broken.set(true)
    };
    let recover = {
        let broken = broken.clone();
        Rc::new(move || broken.set(false)) as Rc<dyn Fn()>
    };
    let count_error = {
        let caught = caught.clone();
        Rc::new(move |_: &RenderError| caught.update(|count| *count += 1))
            as Rc<dyn Fn(&RenderError)>
    };
    rsx! {
        <Demo
            title="A panic while rendering"
            description="Break the forecast: its render panics, the ErrorBoundary around it unmounts it and shows its fallback, and everything outside the boundary keeps working. The fallback reads the error through context and resets the boundary to try again."
        >
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.md,
            }}>
                <Button color={Some(ButtonColor::Error)} on_click={break_it}>Break the forecast</Button>
                <Text style={{ color: theme.color.text.secondary.clone() }}>
                    {format!("Errors caught: {}", caught.get())}
                </Text>
            </Element>
            <ErrorBoundary
                fallback={rsx! { <ForecastFallback on_recover={recover} /> }}
                on_error={count_error}
            >
                <Forecast broken={broken.get()} />
            </ErrorBoundary>
        </Demo>
    }
}
