use rfgui::style::{FontFamily, TextWrap};
use rfgui::ui::{RsxNode, props, rsx};
use rfgui::view::register_font_bytes;
use rfgui::view::{Element, ElementStylePropSchema, Text};
use std::sync::Once;

#[derive(Clone)]
#[props]
pub struct MaterialSymbolIconProps {
    pub style: Option<ElementStylePropSchema>,
    pub line_height: Option<f64>,
}

/// One Material Symbol, embedded as its own single-glyph variable font.
///
/// `build.rs` cuts every symbol out of the bundled font, keeping all
/// variation axes, and emits one static per symbol. Each generated icon
/// component references only its own static, so the linker drops the font
/// data of every icon an application never renders.
pub(crate) struct MaterialSymbolGlyph {
    family: &'static str,
    text: &'static str,
    font: &'static [u8],
    registered: Once,
}

impl MaterialSymbolGlyph {
    pub(crate) const fn new(family: &'static str, text: &'static str, font: &'static [u8]) -> Self {
        Self {
            family,
            text,
            font,
            registered: Once::new(),
        }
    }

    fn ensure_registered(&self) {
        self.registered.call_once(|| {
            let _ = register_font_bytes(self.font);
        });
    }
}

pub(crate) fn render_material_symbol_icon(
    glyph: &'static MaterialSymbolGlyph,
    props: MaterialSymbolIconProps,
) -> RsxNode {
    glyph.ensure_registered();
    let style = material_symbol_icon_style(glyph.family, props.style);
    let line_height = props.line_height.unwrap_or(1.0);

    rsx! {
        <Element style={style}>
            <Text line_height={line_height}>{glyph.text}</Text>
        </Element>
    }
}

fn material_symbol_icon_style(
    family: &'static str,
    style: Option<ElementStylePropSchema>,
) -> ElementStylePropSchema {
    let mut style = style.unwrap_or_default();
    if style.font.is_none() {
        style.font = Some(FontFamily::new([family]));
    }
    if style.text_wrap.is_none() {
        style.text_wrap = Some(TextWrap::NoWrap);
    }
    style
}

include!(concat!(env!("OUT_DIR"), "/material_symbols_outlined.rs"));
