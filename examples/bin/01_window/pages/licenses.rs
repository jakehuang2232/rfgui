//! `licenses.txt` comes from cargo-about with the repository's `about.hbs`:
//! `cargo about generate about.hbs -o examples/bin/01_window/pages/licenses.txt`

use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::Text;
use crate::rfgui_components::use_theme;
use crate::showcase::Demo;

const LICENSES: &str = include_str!("licenses.txt");

struct License<'a> {
    name: &'a str,
    crates: Vec<&'a str>,
    text: &'a str,
}

/// Sections start at `=== name`, list their crates as `- crate version`,
/// and end with the license text.
fn licenses(source: &str) -> Vec<License<'_>> {
    source
        .strip_prefix("=== ")
        .unwrap_or(source)
        .split("\n=== ")
        .map(|section| {
            let (name, rest) = section.split_once('\n').unwrap_or((section, ""));
            let mut crates = Vec::new();
            let mut text = rest;
            while let Some(line) = text.strip_prefix("- ") {
                let (entry, next) = line.split_once('\n').unwrap_or((line, ""));
                crates.push(entry);
                text = next;
            }
            License {
                name: name.trim(),
                crates,
                text: text.trim_matches('\n'),
            }
        })
        .collect()
}

#[component]
pub fn LicensesPage() -> RsxNode {
    let (theme, _) = use_theme();
    let cards = licenses(LICENSES)
        .into_iter()
        .enumerate()
        .map(|(index, license)| {
            rsx! {
                <Demo
                    key={index}
                    title={license.name.to_string()}
                    description={format!("Used by {}", license.crates.join(", "))}
                >
                    <Text style={{
                        color: theme.color.text.secondary.clone(),
                        font_size: theme.typography.size.xs,
                    }}>{license.text.to_string()}</Text>
                </Demo>
            }
        })
        .collect::<Vec<_>>();
    RsxNode::fragment(cards)
}
