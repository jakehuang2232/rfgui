use super::*;
use crate::view::inline_formatting_context::{
    InlineFormattingContext, InlineIfcInput, InlineIfcItem, InlineIfcSourceId,
};
use crate::view::inline_text_pass_adapter::inline_ifc_paint_input_to_text_pass_staging_input;
use std::cell::Cell;

thread_local! { static CONSTRUCTIONS: Cell<usize> = const { Cell::new(0) }; }
pub(super) fn note_construction() {
    CONSTRUCTIONS.with(|n| n.set(n.get() + 1));
}
pub(super) fn construction_count() -> usize {
    CONSTRUCTIONS.with(Cell::get)
}

fn params() -> TextPassPreparedParams {
    let context =
        InlineFormattingContext::build(InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
            source: InlineIfcSourceId(7),
            text: "Text 預檢".into(),
            style: None,
        }]));
    let input = context.text_pass_paint_input();
    assert!(!input.glyphs.is_empty());
    TextPassPreparedParams {
        staging_input: inline_ifc_paint_input_to_text_pass_staging_input(
            &input,
            [3.25, 5.5],
            0.65,
            0,
            2.0,
        ),
        fragments: vec![TextPassPreparedFragment {
            origin: [3.25, 5.5],
            size: [120.0, 40.0],
        }],
        scissor_rect: None,
        stencil_clip_id: None,
    }
}

#[test]
fn text_stream_validation_and_identity_recheck_preserve_field_rejections() {
    type Mutation = (&'static str, fn(&mut TextPassPreparedParams));
    let mutations: &[Mutation] = &[
        ("scale NaN", |p| p.staging_input.scale_factor = f32::NAN),
        ("scale zero", |p| p.staging_input.scale_factor = 0.0),
        ("no glyphs", |p| p.staging_input.glyphs.clear()),
        ("no fragments", |p| p.fragments.clear()),
        ("origin infinite", |p| {
            p.fragments[0].origin[0] = f32::INFINITY
        }),
        ("size zero", |p| p.fragments[0].size[1] = 0.0),
        ("size NaN", |p| p.fragments[0].size[0] = f32::NAN),
        ("missing font", |p| {
            p.staging_input.glyphs[0].raster.font_data = None
        }),
        ("font id", |p| {
            p.staging_input.glyphs[0].raster.font_data_id ^= 1
        }),
        ("font index", |p| {
            p.staging_input.glyphs[0].raster.font_index ^= 1
        }),
        ("glyph range", |p| {
            p.staging_input.glyphs[0].raster.glyph_id = 65536
        }),
        ("font size", |p| {
            p.staging_input.glyphs[0].raster.font_size = -1.0
        }),
        ("fragment index", |p| {
            p.staging_input.glyphs[0].paint.fragment_index = 1
        }),
        ("local NaN", |p| {
            p.staging_input.glyphs[0].paint.local_pos[0] = f32::NAN
        }),
        ("color range", |p| {
            p.staging_input.glyphs[0].paint.color[0] = 1.1
        }),
        ("color NaN", |p| {
            p.staging_input.glyphs[0].paint.color[2] = f32::NAN
        }),
        ("opacity range", |p| {
            p.staging_input.glyphs[0].paint.opacity = -0.1
        }),
        ("opacity NaN", |p| {
            p.staging_input.glyphs[0].paint.opacity = f32::NAN
        }),
        ("final NaN", |p| {
            p.staging_input.glyphs[0].final_paint_pos[1] = f32::NAN
        }),
        ("final drift", |p| {
            p.staging_input.glyphs[0].final_paint_pos[0] += 1.0
        }),
    ];
    let baseline = params();
    let op = PreparedTextOp::new(baseline.clone()).unwrap();
    assert!(op.has_canonical_identity());
    for &(label, mutate) in mutations {
        let mut broken = op.clone();
        mutate(Arc::make_mut(&mut broken.params));
        assert!(!broken.has_canonical_identity(), "{label}");
        assert!(
            !PreparedTextOp::validate_unclipped_glyph_stream(
                broken.params.staging_input.scale_factor,
                &broken.params.fragments,
                broken.params.staging_input.glyphs.clone().into_iter()
            ),
            "{label}"
        );
        assert!(PreparedTextOp::new(broken.params).is_none(), "{label}");
    }
}

#[test]
fn text_identity_recheck_detects_valid_payload_changes_and_lengths() {
    type Mutation = (&'static str, fn(&mut TextPassPreparedParams));
    let mutations: &[Mutation] = &[
        ("scale", |p| p.staging_input.scale_factor = 1.0),
        ("glyph id", |p| {
            p.staging_input.glyphs[0].raster.glyph_id ^= 1
        }),
        ("font size", |p| {
            p.staging_input.glyphs[0].raster.font_size += 1.0
        }),
        ("variation", |p| {
            p.staging_input.glyphs[0].raster.normalized_coords_hash ^= 1
        }),
        ("color", |p| p.staging_input.glyphs[0].paint.color[0] = 0.25),
        ("opacity", |p| {
            p.staging_input.glyphs[0].paint.opacity = 0.25
        }),
        ("position", |p| {
            p.staging_input.glyphs[0].paint.local_pos[0] += 1.0;
            p.staging_input.glyphs[0].final_paint_pos[0] += 1.0;
        }),
        ("fragment size", |p| p.fragments[0].size[0] += 1.0),
        ("extra fragment", |p| p.fragments.push(p.fragments[0])),
        ("extra glyph", |p| {
            p.staging_input
                .glyphs
                .push(p.staging_input.glyphs[0].clone())
        }),
        ("scissor", |p| p.scissor_rect = Some([0, 0, 20, 20])),
        ("stencil", |p| p.stencil_clip_id = Some(1)),
    ];
    let op = PreparedTextOp::new(params()).unwrap();
    for &(label, mutate) in mutations {
        let mut changed = op.clone();
        mutate(Arc::make_mut(&mut changed.params));
        assert!(!changed.has_canonical_identity(), "{label}");
        let rebuilt = PreparedTextOp::new(changed.params).expect(label);
        assert!(rebuilt.has_canonical_identity(), "{label}");
    }
    let mut zero_scissor = op.clone();
    Arc::make_mut(&mut zero_scissor.params).scissor_rect = Some([0, 0, 0, 20]);
    assert!(!zero_scissor.has_canonical_identity());
    assert!(PreparedTextOp::new(zero_scissor.params).is_none());
}

#[test]
fn immutable_text_replay_shares_input_but_changed_input_must_revalidate() {
    let source = PreparedTextOp::new(params()).unwrap();
    let mut replay = source.clone();
    assert!(Arc::ptr_eq(&source.params, &replay.params));
    assert!(replay.has_canonical_identity());
    Arc::make_mut(&mut replay.params).staging_input.glyphs[0]
        .paint
        .opacity = 0.125;
    assert!(!Arc::ptr_eq(&source.params, &replay.params));
    assert!(!replay.has_canonical_identity());
    assert!(source.has_canonical_identity());
}

#[test]
fn baked_opacity_summary_is_bound_to_the_validated_glyph_allocation() {
    let original = PreparedTextOp::new(params()).unwrap();
    assert!(original.has_baked_opacity(0.65_f32.to_bits()));
    assert!(!original.has_baked_opacity(0.5_f32.to_bits()));
    let mut changed = original.clone();
    let replacement = Arc::make_mut(&mut changed.params);
    for glyph in &mut replacement.staging_input.glyphs {
        glyph.paint.opacity = 0.5;
    }
    assert!(changed.has_baked_opacity(0.5_f32.to_bits()));
    assert!(!changed.has_baked_opacity(0.65_f32.to_bits()));
    assert!(!changed.has_canonical_identity());
    let rebuilt = PreparedTextOp::new(changed.params.clone()).unwrap();
    assert!(rebuilt.has_baked_opacity(0.5_f32.to_bits()));
    Arc::make_mut(&mut changed.params).staging_input.glyphs[0]
        .paint
        .opacity = 0.25;
    let mixed = PreparedTextOp::new(changed.params.clone()).unwrap();
    assert!(!mixed.has_baked_opacity(0.5_f32.to_bits()));
    assert!(!mixed.has_baked_opacity(0.25_f32.to_bits()));
    assert!(original.has_baked_opacity(0.65_f32.to_bits()));
}
