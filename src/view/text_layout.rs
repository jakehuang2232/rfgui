use parley::{
    Affinity, Alignment as ParleyAlignment, AlignmentOptions, Cursor as ParleyCursor, FontData,
    FontFamily, FontFamilyName, FontWeight, Layout as ParleyLayout, LineHeight, OverflowWrap,
    StyleProperty, TextWrapMode, YieldData,
};
use std::borrow::Cow;
use std::sync::{Arc, OnceLock};

use crate::ui::Rect;
use crate::view::font_system::with_shared_parley_context;

const TEXT_LAYOUT_WRAP_EPSILON: f32 = 2.0;
const PARLEY_SAFE_CHUNK_CHARS: usize = 240;
const PARLEY_CLUSTER_BREAK: char = '\u{200B}';

#[derive(Clone)]
pub(crate) struct TextLayout {
    inner: ParleyLayout<[u8; 4]>,
    lines: Arc<OnceLock<Vec<TextLine>>>,
    byte_map: Option<Arc<ParleyByteMap>>,
}

#[derive(Clone)]
pub(crate) struct BuiltTextLayout {
    pub(crate) layout: TextLayout,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextLayoutStyle {
    pub(crate) font_size: f32,
    pub(crate) line_height: f32,
    pub(crate) font_weight: u16,
    pub(crate) align: TextLayoutAlignment,
    pub(crate) allow_wrap: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[allow(dead_code)]
pub(crate) enum TextLayoutAlignment {
    Left,
    Center,
    Right,
    Justified,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextLine {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) baseline: f32,
    pub(crate) text_start: usize,
    pub(crate) text_end: usize,
    pub(crate) glyphs: Vec<TextGlyph>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextGlyph {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) id: u32,
    pub(crate) font_size: f32,
    pub(crate) font_data: Option<FontData>,
    pub(crate) normalized_coords_hash: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TextCursorGeometry {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextVisualCaretStop {
    pub(crate) char_index: usize,
    pub(crate) byte_index: usize,
    pub(crate) x: f32,
    pub(crate) y_top: f32,
    pub(crate) height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextVisualCaretLine {
    pub(crate) y_top: f32,
    pub(crate) y_bottom: f32,
    pub(crate) stops: Vec<TextVisualCaretStop>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextLayoutLineFragment {
    pub(crate) content: String,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) baseline: f32,
}

impl TextLayout {
    fn new(inner: ParleyLayout<[u8; 4]>, byte_map: Option<ParleyByteMap>) -> Self {
        Self {
            inner,
            lines: Arc::new(OnceLock::new()),
            byte_map: byte_map.map(Arc::new),
        }
    }

    pub(crate) fn measure_size(&self) -> (f32, f32) {
        let mut max_width = 0.0_f32;
        let mut max_bottom = 0.0_f32;
        let mut line_count = 0_usize;
        for line in self.lines() {
            line_count += 1;
            let glyph_right = line
                .glyphs
                .iter()
                .map(|glyph| glyph.x + glyph.width.max(0.0))
                .fold(0.0_f32, f32::max);
            max_width = max_width.max(glyph_right);
            max_bottom = max_bottom.max(line.y + line.height);
        }
        if line_count == 0 {
            return (1.0, 1.0);
        }
        (max_width.max(1.0), max_bottom.max(1.0))
    }

    pub(crate) fn cursor_geometry(&self, byte_index: usize, upstream: bool) -> TextCursorGeometry {
        let affinity = if upstream {
            Affinity::Upstream
        } else {
            Affinity::Downstream
        };
        let layout_byte = self.original_to_layout_byte(byte_index);
        let cursor = ParleyCursor::from_byte_index(&self.inner, layout_byte, affinity);
        let rect = cursor.geometry(&self.inner, 0.0);
        TextCursorGeometry {
            x: rect.x0 as f32,
            y: rect.y0 as f32,
            height: (rect.y1 - rect.y0).max(1.0) as f32,
        }
    }

    pub(crate) fn hit_byte(&self, x: f32, y: f32) -> usize {
        self.layout_to_original_byte(ParleyCursor::from_point(&self.inner, x, y).index())
    }

    pub(crate) fn caret_geometry_for_char_with_affinity(
        &self,
        content: &str,
        char_index: usize,
        upstream: bool,
    ) -> TextCursorGeometry {
        let byte_index = byte_index_at_char(content, char_index);
        self.cursor_geometry(byte_index, upstream)
    }

    pub(crate) fn visual_caret_lines(&self, content: &str) -> Vec<TextVisualCaretLine> {
        let mut lines: Vec<TextVisualCaretLine> = self
            .inner
            .lines()
            .map(|line| {
                let metrics = line.metrics();
                TextVisualCaretLine {
                    y_top: metrics.block_min_coord,
                    y_bottom: metrics.block_min_coord + metrics.line_height.max(1.0),
                    stops: Vec::new(),
                }
            })
            .collect();

        if lines.is_empty() {
            let geom = self.cursor_geometry(0, false);
            lines.push(TextVisualCaretLine {
                y_top: geom.y,
                y_bottom: geom.y + geom.height.max(1.0),
                stops: Vec::new(),
            });
        }

        if content.is_empty() {
            let geom = self.cursor_geometry(0, false);
            push_visual_caret_stop(
                &mut lines,
                TextVisualCaretStop {
                    char_index: 0,
                    byte_index: 0,
                    x: 0.0,
                    y_top: geom.y,
                    height: geom.height.max(1.0),
                },
            );
            normalize_visual_caret_lines(&mut lines);
            return lines;
        }

        let char_count = content.chars().count();
        for char_index in 0..=char_count {
            let byte_index = byte_index_at_char(content, char_index);
            let downstream = self.cursor_geometry(byte_index, false);
            push_visual_caret_stop(
                &mut lines,
                TextVisualCaretStop {
                    char_index,
                    byte_index,
                    x: downstream.x,
                    y_top: downstream.y,
                    height: downstream.height.max(1.0),
                },
            );

            let upstream = self.cursor_geometry(byte_index, true);
            if (upstream.y - downstream.y).abs() > downstream.height.max(1.0) * 0.25
                || (upstream.x - downstream.x).abs() > 0.5
            {
                push_visual_caret_stop(
                    &mut lines,
                    TextVisualCaretStop {
                        char_index,
                        byte_index,
                        x: upstream.x,
                        y_top: upstream.y,
                        height: upstream.height.max(1.0),
                    },
                );
            }
        }

        normalize_visual_caret_lines(&mut lines);
        lines
    }

    pub(crate) fn visual_line_heads(&self) -> Vec<TextCursorGeometry> {
        self.inner
            .lines()
            .map(|line| {
                let metrics = line.metrics();
                TextCursorGeometry {
                    x: metrics.offset,
                    y: metrics.block_min_coord,
                    height: metrics.line_height,
                }
            })
            .collect()
    }

    pub(crate) fn lines(&self) -> Vec<TextLine> {
        self.lines.get_or_init(|| self.compute_lines()).clone()
    }

    fn compute_lines(&self) -> Vec<TextLine> {
        self.inner
            .lines()
            .map(|line| {
                let metrics = line.metrics();
                let text_range = line.text_range();
                let mut glyphs = Vec::new();
                for item in line.items() {
                    if let parley::PositionedLayoutItem::GlyphRun(run) = item {
                        let font = run.run().font().clone();
                        let font_size = run.run().font_size();
                        let normalized_coords = run.run().normalized_coords();
                        let normalized_coords_hash = if normalized_coords.is_empty() {
                            0
                        } else {
                            hash_text_layout_value(&normalized_coords)
                        };
                        for glyph in run.positioned_glyphs() {
                            glyphs.push(TextGlyph {
                                x: glyph.x,
                                y: glyph.y - metrics.baseline,
                                width: glyph.advance.max(0.0),
                                id: glyph.id,
                                font_size,
                                font_data: Some(font.clone()),
                                normalized_coords_hash,
                            });
                        }
                    }
                }
                TextLine {
                    x: metrics.offset + metrics.inline_min_coord,
                    y: metrics.block_min_coord,
                    width: (metrics.inline_max_coord - metrics.inline_min_coord).max(0.0),
                    height: metrics.line_height,
                    baseline: (metrics.baseline - metrics.block_min_coord).max(0.0),
                    text_start: self.layout_to_original_byte(text_range.start),
                    text_end: self.layout_to_original_byte(text_range.end),
                    glyphs,
                }
            })
            .collect()
    }

    fn original_to_layout_byte(&self, byte: usize) -> usize {
        self.byte_map
            .as_ref()
            .map(|map| map.original_to_layout_byte(byte))
            .unwrap_or(byte)
    }

    fn layout_to_original_byte(&self, byte: usize) -> usize {
        self.byte_map
            .as_ref()
            .map(|map| map.layout_to_original_byte(byte))
            .unwrap_or(byte)
    }

    pub(crate) fn inline_line_fragments(&self, content: &str) -> Vec<TextLayoutLineFragment> {
        self.lines()
            .into_iter()
            .map(|line| {
                let text_start = clamp_utf8_boundary(content, line.text_start);
                let text_end = clamp_utf8_boundary(content, line.text_end);
                let fragment_content = if text_start <= text_end {
                    content[text_start..text_end].to_string()
                } else {
                    String::new()
                };
                let glyph_right = line
                    .glyphs
                    .iter()
                    .map(|glyph| glyph.x + glyph.width.max(0.0))
                    .fold(0.0_f32, f32::max);
                TextLayoutLineFragment {
                    content: fragment_content,
                    width: glyph_right.max(1.0),
                    height: line.height.max(1.0),
                    baseline: line.baseline.max(0.0),
                }
            })
            .collect()
    }

    pub(crate) fn selection_rects(
        &self,
        content: &str,
        local_start: usize,
        local_end: usize,
    ) -> Vec<Rect> {
        let start_char = local_start.min(local_end);
        let end_char = local_start.max(local_end);
        if start_char == end_char {
            return Vec::new();
        }

        let start_byte = byte_index_at_char(content, start_char);
        let end_byte = byte_index_at_char(content, end_char);
        let mut out = Vec::new();
        for line in self.lines() {
            let line_start = line.text_start.max(start_byte);
            let line_end = line.text_end.min(end_byte);
            if line_end <= line_start {
                continue;
            }
            let start = self.cursor_geometry(line_start, false);
            let end = self.cursor_geometry(line_end, true);
            let left = start.x.min(end.x);
            let right = start.x.max(end.x);
            out.push(Rect {
                x: left,
                y: line.y,
                width: (right - left).max(1.0),
                height: line.height.max(start.height).max(end.height).max(1.0),
            });
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ParleyByteMap {
    insertions: Vec<ParleyInsertedText>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ParleyInsertedText {
    original_byte: usize,
    layout_start: usize,
    layout_end: usize,
}

impl ParleyByteMap {
    fn original_to_layout_byte(&self, byte: usize) -> usize {
        byte + self
            .insertions
            .iter()
            .filter(|insertion| insertion.original_byte <= byte)
            .map(|insertion| insertion.layout_end - insertion.layout_start)
            .sum::<usize>()
    }

    fn layout_to_original_byte(&self, byte: usize) -> usize {
        let mut inserted_before = 0;
        for insertion in &self.insertions {
            if byte < insertion.layout_start {
                break;
            }
            let inserted_len = insertion.layout_end - insertion.layout_start;
            if byte < insertion.layout_end {
                return insertion.original_byte;
            }
            inserted_before += inserted_len;
        }
        byte.saturating_sub(inserted_before)
    }
}

struct ParleySafeText<'a> {
    text: Cow<'a, str>,
    byte_map: Option<ParleyByteMap>,
}

fn parley_safe_text(content: &str) -> ParleySafeText<'_> {
    let content = if content.is_empty() { " " } else { content };
    let mut out: Option<String> = None;
    let mut insertions = Vec::new();
    let mut chunk_chars = 0_usize;

    for (byte, ch) in content.char_indices() {
        if chunk_chars >= PARLEY_SAFE_CHUNK_CHARS {
            let out = out.get_or_insert_with(|| content[..byte].to_string());
            let layout_start = out.len();
            out.push(PARLEY_CLUSTER_BREAK);
            insertions.push(ParleyInsertedText {
                original_byte: byte,
                layout_start,
                layout_end: out.len(),
            });
            chunk_chars = 0;
        }
        if let Some(out) = out.as_mut() {
            out.push(ch);
        }
        chunk_chars += 1;
    }

    match out {
        Some(text) => ParleySafeText {
            text: Cow::Owned(text),
            byte_map: Some(ParleyByteMap { insertions }),
        },
        None => ParleySafeText {
            text: Cow::Borrowed(content),
            byte_map: None,
        },
    }
}

fn hash_text_layout_value<T: std::hash::Hash + ?Sized>(value: &T) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(value, &mut hasher);
    hasher.finish()
}

fn push_visual_caret_stop(lines: &mut Vec<TextVisualCaretLine>, stop: TextVisualCaretStop) {
    let line_idx = lines
        .iter()
        .position(|line| {
            let height = (line.y_bottom - line.y_top).max(stop.height).max(1.0);
            (line.y_top - stop.y_top).abs() <= height * 0.5
        })
        .unwrap_or_else(|| {
            lines.push(TextVisualCaretLine {
                y_top: stop.y_top,
                y_bottom: stop.y_top + stop.height.max(1.0),
                stops: Vec::new(),
            });
            lines.len() - 1
        });
    let line = &mut lines[line_idx];
    line.y_top = line.y_top.min(stop.y_top);
    line.y_bottom = line.y_bottom.max(stop.y_top + stop.height.max(1.0));
    line.stops.push(stop);
}

fn normalize_visual_caret_lines(lines: &mut Vec<TextVisualCaretLine>) {
    lines.sort_by(|a, b| {
        a.y_top
            .partial_cmp(&b.y_top)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for line in lines.iter_mut() {
        line.stops
            .sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        let mut deduped: Vec<TextVisualCaretStop> = Vec::with_capacity(line.stops.len());
        for stop in line.stops.drain(..) {
            if let Some(last) = deduped.last_mut()
                && last.char_index == stop.char_index
            {
                if stop.x > last.x {
                    *last = stop;
                }
                continue;
            }
            deduped.push(stop);
        }
        line.stops = deduped;
    }
    lines.retain(|line| !line.stops.is_empty());
}

pub(crate) fn build_text_layout(
    content: &str,
    width: Option<f32>,
    allow_wrap: bool,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    align: TextLayoutAlignment,
    font_families: &[String],
) -> BuiltTextLayout {
    let style = TextLayoutStyle {
        font_size,
        line_height,
        font_weight,
        align,
        allow_wrap,
    };
    build_text_layout_with_style(content, width, style, font_families)
}

pub(crate) fn build_text_layout_with_line_widths(
    content: &str,
    first_width: f32,
    full_width: f32,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    align: TextLayoutAlignment,
    font_families: &[String],
) -> BuiltTextLayout {
    let style = TextLayoutStyle {
        font_size,
        line_height,
        font_weight,
        align,
        allow_wrap: true,
    };
    build_text_layout_with_style_and_line_widths(
        content,
        first_width.max(1.0),
        full_width.max(1.0),
        style,
        font_families,
    )
}

pub(crate) fn build_text_layout_with_style(
    content: &str,
    width: Option<f32>,
    style: TextLayoutStyle,
    font_families: &[String],
) -> BuiltTextLayout {
    let safe_text = parley_safe_text(content);
    let content = safe_text.text.as_ref();
    with_shared_parley_context(|ctx| {
        let mut builder = ctx.layout.ranged_builder(&mut ctx.font, content, 1.0, true);
        builder.push_default(StyleProperty::FontSize(style.font_size.max(1.0)));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            style.line_height.max(0.8),
        )));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(
            style.font_weight as f32,
        )));
        builder.push_default(StyleProperty::TextWrapMode(if style.allow_wrap {
            TextWrapMode::Wrap
        } else {
            TextWrapMode::NoWrap
        }));
        if style.allow_wrap {
            builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
        }
        builder.push_default(StyleProperty::FontFamily(parley_font_family(font_families)));

        let mut layout = builder.build(content);
        break_parley_lines(&mut layout, style.allow_wrap, width, width);
        layout.align(
            to_parley_alignment(style.align),
            AlignmentOptions::default(),
        );
        BuiltTextLayout {
            layout: TextLayout::new(layout, safe_text.byte_map),
        }
    })
}

fn build_text_layout_with_style_and_line_widths(
    content: &str,
    first_width: f32,
    full_width: f32,
    style: TextLayoutStyle,
    font_families: &[String],
) -> BuiltTextLayout {
    let safe_text = parley_safe_text(content);
    let content = safe_text.text.as_ref();
    with_shared_parley_context(|ctx| {
        let mut builder = ctx.layout.ranged_builder(&mut ctx.font, content, 1.0, true);
        builder.push_default(StyleProperty::FontSize(style.font_size.max(1.0)));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            style.line_height.max(0.8),
        )));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(
            style.font_weight as f32,
        )));
        builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::Wrap));
        builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
        builder.push_default(StyleProperty::FontFamily(parley_font_family(font_families)));

        let mut layout = builder.build(content);
        break_parley_lines(&mut layout, true, Some(first_width), Some(full_width));
        layout.align(
            to_parley_alignment(style.align),
            AlignmentOptions::default(),
        );
        BuiltTextLayout {
            layout: TextLayout::new(layout, safe_text.byte_map),
        }
    })
}

fn break_parley_lines(
    layout: &mut ParleyLayout<[u8; 4]>,
    allow_wrap: bool,
    first_width: Option<f32>,
    full_width: Option<f32>,
) {
    if !allow_wrap {
        layout.break_all_lines(None);
        return;
    }

    let first_width = first_width.map(|w| w.max(1.0));
    let full_width = full_width.map(|w| w.max(1.0));
    match (first_width, full_width) {
        (Some(first_width), Some(full_width)) if (first_width - full_width).abs() > 0.01 => {
            let first_width = first_width + TEXT_LAYOUT_WRAP_EPSILON;
            let full_width = full_width + TEXT_LAYOUT_WRAP_EPSILON;
            let layout_max = first_width.max(full_width);
            let mut breaker = layout.break_lines();
            breaker.state_mut().set_layout_max_advance(layout_max);
            breaker.state_mut().set_line_max_advance(first_width);
            while let Some(yield_data) = breaker.break_next() {
                if matches!(yield_data, YieldData::LineBreak(_)) {
                    breaker.state_mut().set_line_max_advance(full_width);
                }
            }
            breaker.finish();
        }
        (Some(width), _) | (_, Some(width)) => {
            layout.break_all_lines(Some(width + TEXT_LAYOUT_WRAP_EPSILON))
        }
        (None, None) => layout.break_all_lines(None),
    }
}

fn byte_index_at_char(content: &str, char_index: usize) -> usize {
    content
        .char_indices()
        .nth(char_index)
        .map(|(byte, _)| byte)
        .unwrap_or(content.len())
}

fn clamp_utf8_boundary(content: &str, byte: usize) -> usize {
    let mut byte = byte.min(content.len());
    while byte > 0 && !content.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

fn to_parley_alignment(align: TextLayoutAlignment) -> ParleyAlignment {
    match align {
        TextLayoutAlignment::Left => ParleyAlignment::Left,
        TextLayoutAlignment::Center => ParleyAlignment::Center,
        TextLayoutAlignment::Right => ParleyAlignment::Right,
        TextLayoutAlignment::Justified => ParleyAlignment::Justify,
        TextLayoutAlignment::End => ParleyAlignment::End,
    }
}

fn parley_font_family(font_families: &[String]) -> FontFamily<'_> {
    if font_families.is_empty() {
        return FontFamily::from("sans-serif");
    }

    let names = font_families
        .iter()
        .map(|family| {
            FontFamilyName::parse(family.as_str())
                .unwrap_or_else(|| FontFamilyName::named(family.as_str()))
        })
        .collect::<Vec<_>>();
    FontFamily::List(Cow::Owned(names))
}

#[cfg(test)]
mod tests {
    use super::*;
    use parley::GenericFamily;

    fn test_layout(content: &str, width: Option<f32>) -> TextLayout {
        build_text_layout(
            content,
            width,
            true,
            14.0,
            1.25,
            400,
            TextLayoutAlignment::Left,
            &[],
        )
        .layout
    }

    #[test]
    fn parley_font_family_preserves_ordered_family_list() {
        let families = vec![
            "Noto Sans CJK TC".to_string(),
            "PingFang TC".to_string(),
            "sans-serif".to_string(),
        ];

        let font_family = parley_font_family(&families);

        let FontFamily::List(list) = font_family else {
            panic!("expected ordered font family list");
        };
        assert_eq!(list.len(), 3);
        assert!(matches!(
            &list[0],
            FontFamilyName::Named(name) if name.as_ref() == "Noto Sans CJK TC"
        ));
        assert!(matches!(
            &list[1],
            FontFamilyName::Named(name) if name.as_ref() == "PingFang TC"
        ));
        assert!(matches!(
            list[2],
            FontFamilyName::Generic(GenericFamily::SansSerif)
        ));
    }

    #[test]
    fn visual_caret_lines_emit_head_and_tail_stops() {
        let content = "hello";
        let layout = test_layout(content, Some(300.0));
        let lines = layout.visual_caret_lines(content);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].stops.first().unwrap().char_index, 0);
        assert_eq!(lines[0].stops.last().unwrap().char_index, 5);
        assert!(
            lines[0]
                .stops
                .iter()
                .all(|stop| stop.byte_index <= content.len())
        );
    }

    #[test]
    fn soft_wrap_boundary_has_affinity_distinct_visual_slots() {
        let content = "the quick brown fox jumps over the lazy dog";
        let layout = test_layout(content, Some(80.0));
        let lines = layout.visual_caret_lines(content);
        assert!(
            lines.len() >= 2,
            "fixture should wrap, got {} lines",
            lines.len()
        );
        let shared = lines.windows(2).find_map(|pair| {
            pair[0].stops.iter().find_map(|upper| {
                pair[1]
                    .stops
                    .iter()
                    .any(|lower| lower.char_index == upper.char_index)
                    .then_some(upper.char_index)
            })
        });
        let char_index = shared.expect("expected a boundary char to appear on adjacent lines");
        let upstream = layout.caret_geometry_for_char_with_affinity(content, char_index, true);
        let downstream = layout.caret_geometry_for_char_with_affinity(content, char_index, false);
        assert!(
            upstream.y < downstream.y,
            "upstream should be on upper visual line; up={upstream:?}, down={downstream:?}",
        );
    }

    #[test]
    fn empty_content_does_not_expose_synthetic_space_as_real_char() {
        let layout = test_layout("", Some(300.0));
        let lines = layout.visual_caret_lines("");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].stops.len(), 1);
        assert_eq!(lines[0].stops[0].char_index, 0);
        assert_eq!(lines[0].stops[0].byte_index, 0);
    }

    #[test]
    fn trailing_newline_produces_blank_line_caret_stop() {
        let content = "a\n";
        let layout = test_layout(content, Some(300.0));
        let lines = layout.visual_caret_lines(content);
        assert!(
            lines.len() >= 2,
            "trailing newline should expose a blank visual line, got {lines:?}",
        );
        let last = lines.last().unwrap();
        assert!(
            last.stops.iter().any(|stop| stop.char_index == 2),
            "document end should be reachable on trailing blank line: {last:?}",
        );
    }

    #[test]
    fn glyph_y_is_relative_to_line_baseline() {
        let layout = test_layout("A", Some(300.0));
        let line = layout.lines().into_iter().next().expect("expected a line");
        let glyph = line.glyphs.first().expect("expected a glyph");

        assert!(
            line.baseline > 1.0,
            "fixture should have a meaningful baseline: {line:?}"
        );
        assert!(
            glyph.y.abs() < 0.01,
            "glyph y must be baseline-relative; absolute Parley glyph y would double-apply baseline: line={line:?}, glyph={glyph:?}"
        );
    }

    #[test]
    fn preedit_effective_text_cursor_geometry_can_land_inside_inserted_text() {
        let content = "abXYZcd";
        let layout = test_layout(content, Some(300.0));
        let inside_preedit = 4;
        let geom = layout.caret_geometry_for_char_with_affinity(content, inside_preedit, false);
        let lines = layout.visual_caret_lines(content);
        assert!(
            lines[0]
                .stops
                .iter()
                .any(|stop| stop.char_index == inside_preedit),
            "preedit effective-text char boundary should be represented",
        );
        assert!(geom.height >= 1.0);
    }

    #[test]
    fn overlong_combining_cluster_does_not_overflow_parley_map_len() {
        let mut content = String::from("a");
        for _ in 0..300 {
            content.push('\u{0301}');
        }
        content.push('b');

        let layout = test_layout(&content, Some(300.0));
        let fragments = layout.inline_line_fragments(&content);
        let combined = fragments
            .iter()
            .map(|fragment| fragment.content.as_str())
            .collect::<String>();

        assert_eq!(combined, content);
        assert_eq!(layout.hit_byte(0.0, 0.0).min(content.len()), 0);
        let tail =
            layout.caret_geometry_for_char_with_affinity(&content, content.chars().count(), false);
        assert!(tail.height >= 1.0);
    }

    #[test]
    fn long_plain_text_keeps_original_line_fragments_after_safe_chunking() {
        let content = "a".repeat(PARLEY_SAFE_CHUNK_CHARS * 2 + 3);

        let layout = test_layout(&content, Some(10_000.0));
        let fragments = layout.inline_line_fragments(&content);
        let combined = fragments
            .iter()
            .map(|fragment| fragment.content.as_str())
            .collect::<String>();

        assert_eq!(combined, content);
        assert_eq!(
            layout.hit_byte(10_000.0, 0.0).min(content.len()),
            content.len()
        );
    }
}
