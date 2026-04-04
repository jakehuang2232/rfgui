use glyphon::cosmic_text::{Align, Weight};
use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Wrap};

pub(crate) fn build_text_buffer(
    font_system: &mut FontSystem,
    content: &str,
    width: Option<f32>,
    height: Option<f32>,
    allow_wrap: bool,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    align: Align,
    font_families: &[String],
) -> Buffer {
    let mut buffer = Buffer::new(
        font_system,
        Metrics::new(
            font_size.max(1.0),
            (font_size * line_height.max(0.8)).max(1.0),
        ),
    );
    buffer.set_wrap(
        font_system,
        if allow_wrap {
            Wrap::WordOrGlyph
        } else {
            Wrap::None
        },
    );
    buffer.set_size(font_system, width.map(|w| w.max(1.0)), height.map(|h| h.max(1.0)));

    let attrs = if let Some(first) = font_families.first() {
        Attrs::new()
            .family(Family::Name(first.as_str()))
            .weight(Weight(font_weight))
    } else {
        Attrs::new().weight(Weight(font_weight))
    };

    let content = if content.is_empty() { " " } else { content };
    buffer.set_text(font_system, content, &attrs, Shaping::Advanced, Some(align));
    buffer.shape_until_scroll(font_system, false);
    buffer
}

pub(crate) fn measure_buffer_size(buffer: &Buffer) -> (f32, f32) {
    let mut max_line_width = 0.0_f32;
    let mut line_count = 0_usize;
    for run in buffer.layout_runs() {
        line_count += 1;
        let mut max_glyph_right = 0.0_f32;
        for glyph in run.glyphs.iter() {
            max_glyph_right = max_glyph_right.max(glyph.x + glyph.w.max(0.0));
        }
        max_line_width = max_line_width.max(run.line_w.max(max_glyph_right));
    }
    let resolved_lines = line_count.max(1);
    let resolved_height = resolved_lines as f32 * buffer.metrics().line_height;
    (max_line_width.max(1.0), resolved_height.max(1.0))
}
