use rfgui::ui::{RsxChildrenPolicy, RsxComponent, RsxNode, build_typed_prop, props, rsx};
use rfgui::view::{Element, ElementStylePropSchema, Text};
use rfgui::{FontFamily, FontSize, TextWrap, register_font_bytes};
use std::sync::Once;

const MATERIAL_SYMBOLS_OUTLINED_FONT_BYTES: &[u8] =
    include_bytes!("../../assets/MaterialSymbolsOutlined.ttf");

pub const MATERIAL_SYMBOLS_OUTLINED_FONT_FAMILY: &str = "Material Symbols Outlined";

static MATERIAL_SYMBOLS_OUTLINED_INIT: Once = Once::new();

#[props]
pub struct MaterialSymbolIconProps {
    pub style: Option<ElementStylePropSchema>,
    pub line_height: Option<f64>,
}

pub struct MaterialSymbolIcon;

impl RsxComponent<MaterialSymbolIconProps> for MaterialSymbolIcon {
    fn render(props: MaterialSymbolIconProps, children: Vec<RsxNode>) -> RsxNode {
        let ligature = children
            .into_iter()
            .find_map(|child| match child {
                RsxNode::Text(text) => Some(text.content.clone()),
                _ => None,
            })
            .unwrap_or_default();
        render_material_symbol_icon(ligature.as_str(), props)
    }
}

impl RsxChildrenPolicy for MaterialSymbolIcon {
    const ACCEPTS_CHILDREN: bool = true;
}

pub fn ensure_material_symbols_outlined_registered() {
    MATERIAL_SYMBOLS_OUTLINED_INIT.call_once(|| {
        let _ = register_font_bytes(MATERIAL_SYMBOLS_OUTLINED_FONT_BYTES);
    });
}

pub(crate) fn render_material_symbol_icon(
    ligature: &str,
    props: MaterialSymbolIconProps,
) -> RsxNode {
    ensure_material_symbols_outlined_registered();
    let style = material_symbol_icon_style(props.style);
    let line_height = props.line_height.unwrap_or(1.0);

    rsx! {
        <Element style={style}>
            <Text line_height={line_height}>{ligature}</Text>
        </Element>
    }
}

fn material_symbol_icon_style(style: Option<ElementStylePropSchema>) -> ElementStylePropSchema {
    let mut style = style.unwrap_or_else(|| build_typed_prop::<ElementStylePropSchema, _>(|_| {}));
    if style.font.is_none() {
        style.font = Some(FontFamily::new([MATERIAL_SYMBOLS_OUTLINED_FONT_FAMILY]));
    }
    if style.font_size.is_none() {
        style.font_size = Some(FontSize::px(24.0));
    }
    if style.text_wrap.is_none() {
        style.text_wrap = Some(TextWrap::NoWrap);
    }
    style
}

include!(concat!(env!("OUT_DIR"), "/material_symbols_outlined.rs"));
