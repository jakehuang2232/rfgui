//! One module per page; each demo in its own file.

pub mod animation;
pub mod appearance;
pub mod containers;
pub mod custom_gpu;
pub mod error_boundary;
pub mod feedback;
pub mod inputs;
pub mod layout;
pub mod licenses;
pub mod media;
pub mod state;
pub mod text;
pub mod transform;
pub mod welcome;

use crate::page::Page;
use crate::rfgui::style::{Layout, Length, Padding};
use crate::rfgui::ui::{Binding, RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::use_theme;
use animation::AnimationPage;
use appearance::AppearancePage;
use containers::ContainersPage;
use custom_gpu::CustomGpuPage;
use error_boundary::ErrorBoundaryPage;
use feedback::FeedbackPage;
use inputs::InputsPage;
use layout::LayoutPage;
use licenses::LicensesPage;
use media::MediaPage;
use state::StatePage;
use text::TextPage;
use transform::TransformPage;
use welcome::WelcomePage;

/// A page's title and summary above its demos.
#[component]
pub fn PageView(page: Page, navigate: Binding<Page>, animation_on: Binding<bool>) -> RsxNode {
    let (theme, _) = use_theme();
    let animations = animation_on.get();
    let content = match page {
        Page::Welcome => rsx! { <WelcomePage navigate={navigate} /> },
        Page::Layout => rsx! { <LayoutPage /> },
        Page::Appearance => rsx! { <AppearancePage /> },
        Page::Text => rsx! { <TextPage /> },
        Page::Media => rsx! { <MediaPage animation_on={animations} /> },
        Page::Inputs => rsx! { <InputsPage /> },
        Page::Feedback => rsx! { <FeedbackPage /> },
        Page::Containers => rsx! { <ContainersPage /> },
        Page::Transform => rsx! { <TransformPage /> },
        Page::Animation => rsx! { <AnimationPage animation_on={animations} /> },
        Page::State => rsx! { <StatePage /> },
        Page::CustomGpu => rsx! { <CustomGpuPage animation_on={animation_on} /> },
        Page::ErrorBoundary => rsx! { <ErrorBoundaryPage /> },
        Page::Licenses => rsx! { <LicensesPage /> },
    };
    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            layout: Layout::flow().column().no_wrap(),
            gap: theme.spacing.lg,
            padding: Padding::uniform(theme.spacing.xl),
        }}>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.xs,
            }}>
                <Text style={{
                    font_size: theme.typography.size.xl,
                    font_weight: theme.typography.weight.bold,
                }}>{page.title()}</Text>
                <Text style={{ color: theme.color.text.secondary.clone() }}>{page.summary()}</Text>
            </Element>
            {content}
        </Element>
    }
}
