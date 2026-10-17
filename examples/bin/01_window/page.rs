//! The pages of the showcase and how the sidebar groups them.

use crate::rfgui::ui::{RsxNode, rsx};
use crate::rfgui_components::material_symbol::{
    AccountTreeIcon, AnimationIcon, BugReportIcon, DashboardIcon, GavelIcon, HomeIcon, ImageIcon,
    MemoryIcon, NotificationsIcon, PaletteIcon, TextFieldsIcon, ToggleOnIcon, ViewAgendaIcon,
    ViewInArIcon,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Welcome,
    Layout,
    Appearance,
    Text,
    Media,
    Inputs,
    Feedback,
    Containers,
    Transform,
    Animation,
    State,
    CustomGpu,
    ErrorBoundary,
    Licenses,
}

pub struct PageGroup {
    pub title: &'static str,
    pub pages: &'static [Page],
}

/// The sidebar's sections, in order. Licenses sits apart, in its footer.
pub const PAGE_GROUPS: [PageGroup; 4] = [
    PageGroup {
        title: "Getting Started",
        pages: &[Page::Welcome],
    },
    PageGroup {
        title: "Basics",
        pages: &[Page::Layout, Page::Appearance, Page::Text, Page::Media],
    },
    PageGroup {
        title: "Components",
        pages: &[Page::Inputs, Page::Feedback, Page::Containers],
    },
    PageGroup {
        title: "Advanced",
        pages: &[
            Page::Transform,
            Page::Animation,
            Page::State,
            Page::CustomGpu,
            Page::ErrorBoundary,
        ],
    },
];

impl Page {
    pub fn key(self) -> usize {
        self as usize
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Welcome => "Welcome",
            Self::Layout => "Layout",
            Self::Appearance => "Appearance",
            Self::Text => "Text",
            Self::Media => "Images & SVG",
            Self::Inputs => "Inputs",
            Self::Feedback => "Feedback",
            Self::Containers => "Containers",
            Self::Transform => "Transform & 3D",
            Self::Animation => "Animation",
            Self::State => "Components & State",
            Self::CustomGpu => "Custom GPU Paint",
            Self::ErrorBoundary => "Error Boundary",
            Self::Licenses => "Licenses",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            Self::Welcome => "A retained-mode GUI framework for Rust, drawn on the GPU.",
            Self::Layout => {
                "Inline, flow and flex layout, positioning against anchors, and scrolling."
            }
            Self::Appearance => "Backgrounds, borders, corner radii, shadows and opacity.",
            Self::Text => "Text that wraps across scripts, and editable text areas.",
            Self::Media => "Raster images, SVG documents and Material Symbols icons.",
            Self::Inputs => {
                "Buttons, toggles, checkboxes, switches, selects, sliders and number fields."
            }
            Self::Feedback => "Tooltips, alerts and snackbars.",
            Self::Containers => "Accordions, tree views and windows.",
            Self::Transform => {
                "2D and 3D transforms that hit-test, animate and can hide their back faces."
            }
            Self::Animation => {
                "Keyframe animations, and transitions on style, transform, layout and position."
            }
            Self::State => "State that stays with a component, even when it moves.",
            Self::CustomGpu => "A host element that draws itself with its own WGSL shader.",
            Self::ErrorBoundary => {
                "A component that panics while rendering gives way to a fallback; the rest of \
                 the app carries on."
            }
            Self::Licenses => "Licenses of the third-party crates this showcase is built with.",
        }
    }

    pub fn icon(self) -> RsxNode {
        match self {
            Self::Welcome => rsx! { <HomeIcon /> },
            Self::Layout => rsx! { <DashboardIcon /> },
            Self::Appearance => rsx! { <PaletteIcon /> },
            Self::Text => rsx! { <TextFieldsIcon /> },
            Self::Media => rsx! { <ImageIcon /> },
            Self::Inputs => rsx! { <ToggleOnIcon /> },
            Self::Feedback => rsx! { <NotificationsIcon /> },
            Self::Containers => rsx! { <ViewAgendaIcon /> },
            Self::Transform => rsx! { <ViewInArIcon /> },
            Self::Animation => rsx! { <AnimationIcon /> },
            Self::State => rsx! { <AccountTreeIcon /> },
            Self::CustomGpu => rsx! { <MemoryIcon /> },
            Self::ErrorBoundary => rsx! { <BugReportIcon /> },
            Self::Licenses => rsx! { <GavelIcon /> },
        }
    }
}
