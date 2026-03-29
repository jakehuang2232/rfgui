use crate::ui::RsxNode;
use crate::ui::{
    BlurHandlerProp, ClickHandlerProp, FocusHandlerProp, FromPropValue, IntoPropValue,
    KeyDownHandlerProp, KeyUpHandlerProp, MouseDownHandlerProp, MouseEnterHandlerProp,
    MouseLeaveHandlerProp, MouseMoveHandlerProp, MouseUpHandlerProp, RsxChildrenPolicy,
    RsxComponent, RsxPropsStyleSchema, RsxStyleSchema, SharedPropValue, TextAreaFocusHandlerProp,
    TextChangeHandlerProp, props,
};
use crate::{
    Align, BorderRadius, BoxShadow, ColorLike, CrossSize, Cursor, Flex, FontFamily, FontSize,
    FontWeight, Layout, Length, Opacity, Padding, Position, ScrollDirection, SelectionStyle,
    Style, TextAlign, TextWrap, Transitions,
};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

pub struct Element;
pub struct Text;
pub struct TextArea;
pub struct Image;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFit {
    Contain,
    Cover,
    Fill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageSampling {
    Linear,
    Nearest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageSource {
    Path(PathBuf),
    Rgba {
        width: u32,
        height: u32,
        pixels: Arc<[u8]>,
    },
}

#[props]
pub struct ElementPropSchema {
    pub anchor: Option<String>,
    pub style: Option<ElementStylePropSchema>,
    pub on_mouse_down: Option<MouseDownHandlerProp>,
    pub on_mouse_up: Option<MouseUpHandlerProp>,
    pub on_mouse_move: Option<MouseMoveHandlerProp>,
    pub on_mouse_enter: Option<MouseEnterHandlerProp>,
    pub on_mouse_leave: Option<MouseLeaveHandlerProp>,
    pub on_click: Option<ClickHandlerProp>,
    pub on_key_down: Option<KeyDownHandlerProp>,
    pub on_key_up: Option<KeyUpHandlerProp>,
    pub on_focus: Option<FocusHandlerProp>,
    pub on_blur: Option<BlurHandlerProp>,
}

#[derive(Clone)]
#[props]
pub struct ElementStylePropSchema {
    pub position: Option<Position>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub min_width: Option<Length>,
    pub max_width: Option<Length>,
    pub min_height: Option<Length>,
    pub max_height: Option<Length>,
    pub layout: Option<Layout>,
    pub cross_size: Option<CrossSize>,
    pub align: Option<Align>,
    pub flex: Option<Flex>,
    pub gap: Option<Length>,
    pub scroll_direction: Option<ScrollDirection>,
    pub cursor: Option<Cursor>,
    pub color: Option<Box<dyn ColorLike>>,
    pub border: Option<crate::Border>,
    pub background: Option<Box<dyn ColorLike>>,
    pub background_color: Option<Box<dyn ColorLike>>,
    pub font: Option<FontFamily>,
    pub font_size: Option<FontSize>,
    pub font_weight: Option<FontWeight>,
    pub text_wrap: Option<TextWrap>,
    pub border_radius: Option<BorderRadius>,
    pub hover: Option<HoverElementStylePropSchema>,
    pub selection: Option<SelectionStylePropSchema>,
    pub opacity: Option<Opacity>,
    pub box_shadow: Option<Vec<BoxShadow>>,
    pub padding: Option<Padding>,
    pub transition: Option<Transitions>,
}

#[derive(Clone)]
#[props]
pub struct HoverElementStylePropSchema {
    pub position: Option<Position>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub min_width: Option<Length>,
    pub max_width: Option<Length>,
    pub min_height: Option<Length>,
    pub max_height: Option<Length>,
    pub layout: Option<Layout>,
    pub cross_size: Option<CrossSize>,
    pub align: Option<Align>,
    pub flex: Option<Flex>,
    pub gap: Option<Length>,
    pub scroll_direction: Option<ScrollDirection>,
    pub cursor: Option<Cursor>,
    pub color: Option<Box<dyn ColorLike>>,
    pub border: Option<crate::Border>,
    pub background: Option<Box<dyn ColorLike>>,
    pub background_color: Option<Box<dyn ColorLike>>,
    pub font: Option<FontFamily>,
    pub font_size: Option<FontSize>,
    pub font_weight: Option<FontWeight>,
    pub text_wrap: Option<TextWrap>,
    pub border_radius: Option<BorderRadius>,
    pub selection: Option<SelectionStylePropSchema>,
    pub opacity: Option<Opacity>,
    pub box_shadow: Option<Vec<BoxShadow>>,
    pub padding: Option<Padding>,
    pub transition: Option<Transitions>,
}

#[derive(Clone)]
#[props]
pub struct TextStylePropSchema {
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub color: Option<Box<dyn ColorLike>>,
    pub font: Option<FontFamily>,
    pub font_size: Option<FontSize>,
    pub font_weight: Option<FontWeight>,
    pub text_wrap: Option<TextWrap>,
    pub cursor: Option<Cursor>,
    pub hover: Option<HoverTextStylePropSchema>,
    pub opacity: Option<Opacity>,
    pub transition: Option<Transitions>,
}

#[derive(Clone)]
#[props]
pub struct HoverTextStylePropSchema {
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub color: Option<Box<dyn ColorLike>>,
    pub font: Option<FontFamily>,
    pub font_size: Option<FontSize>,
    pub font_weight: Option<FontWeight>,
    pub text_wrap: Option<TextWrap>,
    pub cursor: Option<Cursor>,
    pub opacity: Option<Opacity>,
    pub transition: Option<Transitions>,
}

#[derive(Clone)]
#[props]
pub struct SelectionStylePropSchema {
    pub background: Option<Box<dyn ColorLike>>,
}

#[derive(Clone)]
#[props]
pub struct BorderStylePropSchema {
    pub width: Option<Length>,
    pub color: Option<Box<dyn ColorLike>>,
}

impl RsxStyleSchema for ElementStylePropSchema {
    type SelectionSchema = SelectionStylePropSchema;
}

impl RsxStyleSchema for TextStylePropSchema {
    type SelectionSchema = SelectionStylePropSchema;
}

impl RsxPropsStyleSchema for ElementPropSchema {
    type StyleSchema = ElementStylePropSchema;
}

impl RsxPropsStyleSchema for TextPropSchema {
    type StyleSchema = TextStylePropSchema;
}

impl RsxPropsStyleSchema for TextAreaPropSchema {
    type StyleSchema = ElementStylePropSchema;
}

impl RsxPropsStyleSchema for ImagePropSchema {
    type StyleSchema = ElementStylePropSchema;
}

#[props]
pub struct TextPropSchema {
    pub style: Option<TextStylePropSchema>,
    pub align: Option<TextAlign>,
    pub font_size: Option<FontSize>,
    pub line_height: Option<f64>,
    pub font: Option<String>,
    pub opacity: Option<f64>,
}

#[props]
pub struct TextAreaPropSchema {
    pub content: Option<String>,
    pub binding: Option<crate::ui::Binding<String>>,
    pub style: Option<ElementStylePropSchema>,
    pub on_focus: Option<TextAreaFocusHandlerProp>,
    pub on_blur: Option<BlurHandlerProp>,
    pub on_change: Option<TextChangeHandlerProp>,
    pub placeholder: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub font_size: Option<FontSize>,
    pub font: Option<String>,
    pub opacity: Option<f64>,
    pub multiline: Option<bool>,
    pub read_only: Option<bool>,
    pub max_length: Option<i64>,
}

#[props]
pub struct ImagePropSchema {
    pub source: ImageSource,
    pub style: Option<ElementStylePropSchema>,
    pub fit: Option<ImageFit>,
    pub sampling: Option<ImageSampling>,
    pub loading: Option<RsxNode>,
    pub error: Option<RsxNode>,
}

impl RsxComponent<ElementPropSchema> for Element {
    fn render(props: ElementPropSchema, children: Vec<RsxNode>) -> RsxNode {
        let mut node = RsxNode::tagged("Element", crate::ui::RsxTagDescriptor::of::<Element>());
        if let Some(anchor) = props.anchor {
            node = node.with_prop("anchor", anchor);
        }
        if let Some(style) = props.style {
            node = node.with_prop("style", style);
        }
        if let Some(handler) = props.on_mouse_down {
            node = node.with_prop("on_mouse_down", handler);
        }
        if let Some(handler) = props.on_mouse_up {
            node = node.with_prop("on_mouse_up", handler);
        }
        if let Some(handler) = props.on_mouse_move {
            node = node.with_prop("on_mouse_move", handler);
        }
        if let Some(handler) = props.on_mouse_enter {
            node = node.with_prop("on_mouse_enter", handler);
        }
        if let Some(handler) = props.on_mouse_leave {
            node = node.with_prop("on_mouse_leave", handler);
        }
        if let Some(handler) = props.on_click {
            node = node.with_prop("on_click", handler);
        }
        if let Some(handler) = props.on_key_down {
            node = node.with_prop("on_key_down", handler);
        }
        if let Some(handler) = props.on_key_up {
            node = node.with_prop("on_key_up", handler);
        }
        if let Some(handler) = props.on_focus {
            node = node.with_prop("on_focus", handler);
        }
        if let Some(handler) = props.on_blur {
            node = node.with_prop("on_blur", handler);
        }
        for child in children {
            node = node.with_child(child);
        }
        node
    }
}

impl RsxComponent<TextPropSchema> for Text {
    fn render(props: TextPropSchema, children: Vec<RsxNode>) -> RsxNode {
        let mut node = RsxNode::tagged("Text", crate::ui::RsxTagDescriptor::of::<Text>());
        if let Some(style) = props.style {
            node = node.with_prop("style", style);
        }
        if let Some(align) = props.align {
            node = node.with_prop("align", align);
        }
        if let Some(font_size) = props.font_size
            && !is_unset_font_size(font_size)
        {
            node = node.with_prop("font_size", font_size);
        }
        if let Some(line_height) = props.line_height
            && line_height != 0.0
        {
            node = node.with_prop("line_height", line_height);
        }
        if let Some(font) = props.font
            && !font.is_empty()
        {
            node = node.with_prop("font", font);
        }
        if let Some(opacity) = props.opacity
            && opacity != 0.0
        {
            node = node.with_prop("opacity", opacity);
        }
        for child in children {
            node = node.with_child(child);
        }
        node
    }
}

impl RsxComponent<TextAreaPropSchema> for TextArea {
    fn render(props: TextAreaPropSchema, children: Vec<RsxNode>) -> RsxNode {
        let mut node = RsxNode::tagged("TextArea", crate::ui::RsxTagDescriptor::of::<TextArea>());
        if let Some(content) = props.content
            && !content.is_empty()
        {
            node = node.with_prop("content", content);
        }
        if let Some(binding) = props.binding {
            node = node.with_prop(
                "binding",
                crate::ui::IntoPropValue::into_prop_value(binding),
            );
        }
        if let Some(style) = props.style {
            node = node.with_prop("style", style);
        }
        if let Some(handler) = props.on_focus {
            node = node.with_prop("on_focus", handler);
        }
        if let Some(handler) = props.on_blur {
            node = node.with_prop("on_blur", handler);
        }
        if let Some(handler) = props.on_change {
            node = node.with_prop("on_change", handler);
        }
        if let Some(placeholder) = props.placeholder
            && !placeholder.is_empty()
        {
            node = node.with_prop("placeholder", placeholder);
        }
        if let Some(x) = props.x
            && x != 0.0
        {
            node = node.with_prop("x", x);
        }
        if let Some(y) = props.y
            && y != 0.0
        {
            node = node.with_prop("y", y);
        }
        if let Some(font_size) = props.font_size
            && !is_unset_font_size(font_size)
        {
            node = node.with_prop("font_size", font_size);
        }
        if let Some(font) = props.font
            && !font.is_empty()
        {
            node = node.with_prop("font", font);
        }
        if let Some(opacity) = props.opacity
            && opacity != 0.0
        {
            node = node.with_prop("opacity", opacity);
        }
        if let Some(multiline) = props.multiline {
            node = node.with_prop("multiline", multiline);
        }
        if let Some(read_only) = props.read_only {
            node = node.with_prop("read_only", read_only);
        }
        if let Some(max_length) = props.max_length
            && max_length != 0
        {
            node = node.with_prop("max_length", max_length);
        }
        for child in children {
            node = node.with_child(child);
        }
        node
    }
}

impl RsxComponent<ImagePropSchema> for Image {
    fn render(props: ImagePropSchema, _children: Vec<RsxNode>) -> RsxNode {
        let mut node = RsxNode::tagged("Image", crate::ui::RsxTagDescriptor::of::<Image>())
            .with_prop(
                "source",
                crate::ui::IntoPropValue::into_prop_value(props.source),
            );
        if let Some(style) = props.style {
            node = node.with_prop("style", style);
        }
        if let Some(fit) = props.fit {
            node = node.with_prop("fit", crate::ui::IntoPropValue::into_prop_value(fit));
        }
        if let Some(sampling) = props.sampling {
            node = node.with_prop(
                "sampling",
                crate::ui::IntoPropValue::into_prop_value(sampling),
            );
        }
        if let Some(loading) = props.loading {
            node = node.with_prop(
                "loading",
                crate::ui::IntoPropValue::into_prop_value(loading),
            );
        }
        if let Some(error) = props.error {
            node = node.with_prop("error", crate::ui::IntoPropValue::into_prop_value(error));
        }
        node
    }
}

impl RsxChildrenPolicy for Element {
    const ACCEPTS_CHILDREN: bool = true;
}

impl RsxChildrenPolicy for Text {
    const ACCEPTS_CHILDREN: bool = true;
}

impl RsxChildrenPolicy for TextArea {
    const ACCEPTS_CHILDREN: bool = true;
}

impl RsxChildrenPolicy for Image {
    const ACCEPTS_CHILDREN: bool = false;
}

fn into_shared_prop_value<T: 'static>(value: T) -> crate::ui::PropValue {
    crate::ui::PropValue::Shared(SharedPropValue::new(Rc::new(value)))
}

fn from_shared_prop_value<T: Clone + 'static>(
    value: crate::ui::PropValue,
    expected: &str,
) -> Result<T, String> {
    match value {
        crate::ui::PropValue::Shared(shared) => shared
            .value()
            .downcast::<T>()
            .map(|value| (*value).clone())
            .map_err(|_| format!("expected {expected} value")),
        _ => Err(format!("expected {expected} value")),
    }
}

macro_rules! impl_shared_style_prop_value {
    ($ty:ty, $label:literal) => {
        impl IntoPropValue for $ty {
            fn into_prop_value(self) -> crate::ui::PropValue {
                into_shared_prop_value(self)
            }
        }

        impl From<$ty> for crate::ui::PropValue {
            fn from(value: $ty) -> Self {
                into_shared_prop_value(value)
            }
        }

        impl FromPropValue for $ty {
            fn from_prop_value(value: crate::ui::PropValue) -> Result<Self, String> {
                from_shared_prop_value(value, $label)
            }
        }
    };
}

impl_shared_style_prop_value!(ElementStylePropSchema, "ElementStylePropSchema");
impl_shared_style_prop_value!(HoverElementStylePropSchema, "HoverElementStylePropSchema");
impl_shared_style_prop_value!(TextStylePropSchema, "TextStylePropSchema");
impl_shared_style_prop_value!(HoverTextStylePropSchema, "HoverTextStylePropSchema");
impl_shared_style_prop_value!(SelectionStylePropSchema, "SelectionStylePropSchema");
impl_shared_style_prop_value!(BorderStylePropSchema, "BorderStylePropSchema");

fn apply_box_color(
    style: &mut Style,
    property: crate::PropertyId,
    color: &Option<Box<dyn ColorLike>>,
) {
    if let Some(color) = color {
        style.insert(property, crate::style_color_value(color.clone()));
    }
}

fn apply_selection(selection: &Option<SelectionStylePropSchema>) -> Option<SelectionStyle> {
    let selection = selection.as_ref()?;
    let mut output = SelectionStyle::new();
    if let Some(background) = &selection.background {
        output.set_background(background.clone());
    }
    Some(output)
}

fn apply_element_style_fields(style: &mut Style, schema: &HoverElementStylePropSchema) {
    if let Some(position) = schema.position.clone() {
        style.insert(crate::PropertyId::Position, crate::ParsedValue::Position(position));
    }
    if let Some(width) = schema.width {
        crate::insert_style_length(style, crate::PropertyId::Width, width);
    }
    if let Some(height) = schema.height {
        crate::insert_style_length(style, crate::PropertyId::Height, height);
    }
    if let Some(min_width) = schema.min_width {
        crate::insert_style_length(style, crate::PropertyId::MinWidth, min_width);
    }
    if let Some(max_width) = schema.max_width {
        crate::insert_style_length(style, crate::PropertyId::MaxWidth, max_width);
    }
    if let Some(min_height) = schema.min_height {
        crate::insert_style_length(style, crate::PropertyId::MinHeight, min_height);
    }
    if let Some(max_height) = schema.max_height {
        crate::insert_style_length(style, crate::PropertyId::MaxHeight, max_height);
    }
    if let Some(layout) = schema.layout {
        style.insert(crate::PropertyId::Layout, crate::ParsedValue::Layout(layout));
    }
    if let Some(cross_size) = schema.cross_size {
        style.insert(
            crate::PropertyId::CrossSize,
            crate::ParsedValue::CrossSize(cross_size),
        );
    }
    if let Some(align) = schema.align {
        style.insert(crate::PropertyId::Align, crate::ParsedValue::Align(align));
    }
    if let Some(flex) = schema.flex {
        crate::insert_style_flex(style, crate::PropertyId::Flex, flex);
    }
    if let Some(gap) = schema.gap {
        crate::insert_style_length(style, crate::PropertyId::Gap, gap);
    }
    if let Some(scroll_direction) = schema.scroll_direction {
        style.insert(
            crate::PropertyId::ScrollDirection,
            crate::ParsedValue::ScrollDirection(scroll_direction),
        );
    }
    if let Some(cursor) = schema.cursor {
        style.insert(crate::PropertyId::Cursor, crate::ParsedValue::Cursor(cursor));
    }
    apply_box_color(style, crate::PropertyId::Color, &schema.color);
    apply_box_color(style, crate::PropertyId::BackgroundColor, &schema.background);
    apply_box_color(style, crate::PropertyId::BackgroundColor, &schema.background_color);
    if let Some(border) = &schema.border {
        style.set_border(border.clone());
    }
    if let Some(font) = &schema.font {
        style.insert(
            crate::PropertyId::FontFamily,
            crate::ParsedValue::FontFamily(font.clone()),
        );
    }
    if let Some(font_size) = schema.font_size {
        crate::insert_style_font_size(style, crate::PropertyId::FontSize, font_size);
    }
    if let Some(font_weight) = schema.font_weight {
        crate::insert_style_font_weight(style, crate::PropertyId::FontWeight, font_weight);
    }
    if let Some(text_wrap) = schema.text_wrap {
        crate::insert_style_text_wrap(style, crate::PropertyId::TextWrap, text_wrap);
    }
    if let Some(border_radius) = schema.border_radius {
        style.set_border_radius(border_radius);
    }
    if let Some(opacity) = schema.opacity {
        style.insert(
            crate::PropertyId::Opacity,
            crate::ParsedValue::Opacity(opacity),
        );
    }
    if let Some(box_shadow) = &schema.box_shadow {
        style.insert(
            crate::PropertyId::BoxShadow,
            crate::ParsedValue::BoxShadow(box_shadow.clone()),
        );
    }
    if let Some(padding) = schema.padding {
        style.set_padding(padding);
    }
    if let Some(transition) = &schema.transition {
        style.insert(
            crate::PropertyId::Transition,
            crate::ParsedValue::Transition(transition.clone()),
        );
    }
    if let Some(selection) = apply_selection(&schema.selection) {
        style.set_selection(selection);
    }
}

impl HoverElementStylePropSchema {
    pub fn to_style(&self) -> Style {
        let mut style = Style::new();
        apply_element_style_fields(&mut style, self);
        style
    }
}

impl ElementStylePropSchema {
    pub fn to_style(&self) -> Style {
        let mut style = Style::new();
        let hover_view = HoverElementStylePropSchema {
            position: self.position.clone(),
            width: self.width,
            height: self.height,
            min_width: self.min_width,
            max_width: self.max_width,
            min_height: self.min_height,
            max_height: self.max_height,
            layout: self.layout,
            cross_size: self.cross_size,
            align: self.align,
            flex: self.flex,
            gap: self.gap,
            scroll_direction: self.scroll_direction,
            cursor: self.cursor,
            color: self.color.clone(),
            border: self.border.clone(),
            background: self.background.clone(),
            background_color: self.background_color.clone(),
            font: self.font.clone(),
            font_size: self.font_size,
            font_weight: self.font_weight,
            text_wrap: self.text_wrap,
            selection: self.selection.clone(),
            border_radius: self.border_radius,
            opacity: self.opacity,
            box_shadow: self.box_shadow.clone(),
            padding: self.padding,
            transition: self.transition.clone(),
        };
        apply_element_style_fields(&mut style, &hover_view);
        if let Some(hover) = &self.hover {
            style.set_hover(hover.to_style());
        }
        style
    }
}

impl HoverTextStylePropSchema {
    pub fn to_style(&self) -> Style {
        let mut style = Style::new();
        if let Some(width) = self.width {
            crate::insert_style_length(&mut style, crate::PropertyId::Width, width);
        }
        if let Some(height) = self.height {
            crate::insert_style_length(&mut style, crate::PropertyId::Height, height);
        }
        apply_box_color(&mut style, crate::PropertyId::Color, &self.color);
        if let Some(font) = &self.font {
            style.insert(
                crate::PropertyId::FontFamily,
                crate::ParsedValue::FontFamily(font.clone()),
            );
        }
        if let Some(font_size) = self.font_size {
            crate::insert_style_font_size(&mut style, crate::PropertyId::FontSize, font_size);
        }
        if let Some(font_weight) = self.font_weight {
            crate::insert_style_font_weight(
                &mut style,
                crate::PropertyId::FontWeight,
                font_weight,
            );
        }
        if let Some(text_wrap) = self.text_wrap {
            crate::insert_style_text_wrap(&mut style, crate::PropertyId::TextWrap, text_wrap);
        }
        if let Some(cursor) = self.cursor {
            style.insert(crate::PropertyId::Cursor, crate::ParsedValue::Cursor(cursor));
        }
        if let Some(opacity) = self.opacity {
            style.insert(
                crate::PropertyId::Opacity,
                crate::ParsedValue::Opacity(opacity),
            );
        }
        if let Some(transition) = &self.transition {
            style.insert(
                crate::PropertyId::Transition,
                crate::ParsedValue::Transition(transition.clone()),
            );
        }
        style
    }
}

impl TextStylePropSchema {
    pub fn to_style(&self) -> Style {
        let mut style = HoverTextStylePropSchema {
            width: self.width,
            height: self.height,
            color: self.color.clone(),
            font: self.font.clone(),
            font_size: self.font_size,
            font_weight: self.font_weight,
            text_wrap: self.text_wrap,
            cursor: self.cursor,
            opacity: self.opacity,
            transition: self.transition.clone(),
        }
        .to_style();
        if let Some(hover) = &self.hover {
            style.set_hover(hover.to_style());
        }
        style
    }
}

impl crate::ui::IntoPropValue for ImageFit {
    fn into_prop_value(self) -> crate::ui::PropValue {
        crate::ui::PropValue::Shared(crate::ui::SharedPropValue::new(Rc::new(self)))
    }
}

impl crate::ui::IntoPropValue for ImageSampling {
    fn into_prop_value(self) -> crate::ui::PropValue {
        crate::ui::PropValue::Shared(crate::ui::SharedPropValue::new(Rc::new(self)))
    }
}

impl crate::ui::FromPropValue for ImageSampling {
    fn from_prop_value(value: crate::ui::PropValue) -> Result<Self, String> {
        match value {
            crate::ui::PropValue::Shared(shared) => shared
                .value()
                .downcast::<ImageSampling>()
                .map(|value| *value)
                .map_err(|_| "expected ImageSampling value".to_string()),
            _ => Err("expected ImageSampling value".to_string()),
        }
    }
}

impl crate::ui::FromPropValue for ImageFit {
    fn from_prop_value(value: crate::ui::PropValue) -> Result<Self, String> {
        match value {
            crate::ui::PropValue::Shared(shared) => shared
                .value()
                .downcast::<ImageFit>()
                .map(|value| *value)
                .map_err(|_| "expected ImageFit value".to_string()),
            _ => Err("expected ImageFit value".to_string()),
        }
    }
}

impl crate::ui::IntoPropValue for ImageSource {
    fn into_prop_value(self) -> crate::ui::PropValue {
        crate::ui::PropValue::Shared(crate::ui::SharedPropValue::new(Rc::new(self)))
    }
}

impl crate::ui::FromPropValue for ImageSource {
    fn from_prop_value(value: crate::ui::PropValue) -> Result<Self, String> {
        match value {
            crate::ui::PropValue::Shared(shared) => shared
                .value()
                .downcast::<ImageSource>()
                .map(|value| (*value).clone())
                .map_err(|_| "expected ImageSource value".to_string()),
            _ => Err("expected ImageSource value".to_string()),
        }
    }
}

impl crate::ui::IntoPropValue for RsxNode {
    fn into_prop_value(self) -> crate::ui::PropValue {
        crate::ui::PropValue::Shared(crate::ui::SharedPropValue::new(Rc::new(self)))
    }
}

impl crate::ui::FromPropValue for RsxNode {
    fn from_prop_value(value: crate::ui::PropValue) -> Result<Self, String> {
        match value {
            crate::ui::PropValue::Shared(shared) => shared
                .value()
                .downcast::<RsxNode>()
                .map(|value| (*value).clone())
                .map_err(|_| "expected RsxNode value".to_string()),
            _ => Err("expected RsxNode value".to_string()),
        }
    }
}

fn is_unset_font_size(font_size: FontSize) -> bool {
    match font_size {
        FontSize::Px(v)
        | FontSize::Percent(v)
        | FontSize::Em(v)
        | FontSize::Rem(v)
        | FontSize::Vw(v)
        | FontSize::Vh(v) => v == 0.0,
    }
}
