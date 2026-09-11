//! Single-entry memos for immutable text inputs, never for live tree authority.
//! Holding strong Arcs prevents address reuse and forces copy-on-write mutations
//! to acquire a different identity. Dirty flags are not an invalidation key.
//! This keeps one CPU payload per Text, including while a whole frame falls
//! back to Legacy; it trades bounded per-owner storage for warm-frame work.
//! A successful payload miss replaces the paint entry; hidden/empty paint or
//! preparation failure leaves it intact. The install entry is replaced only
//! when a new input pair reaches structural comparison. Both release their
//! strong references when Text is dropped, not merely when it leaves the
//! viewport or is removed from an arena and retained by the caller. Other Arc
//! owners may keep the allocations alive. Entry counts are bounded per Text;
//! total bytes are not bounded globally or by the visible working set.
//! Staging uses logical scale 1.0 and no scissor/stencil; DPR and live property
//! scopes are resolved outside this payload and are not memoized here.

use super::{InlineFormattingContext, InlineIfcTextPassPaintInput, Text};
use std::{cell::RefCell, sync::Arc};

pub(super) struct TextPaintMemo {
    pub(super) key: TextPaintKey,
    pub(super) payload: Arc<super::render::PreparedShadowTextPayload>,
}

enum TextPaintSource {
    Standalone(Arc<InlineFormattingContext>),
    Owned(Arc<InlineIfcTextPassPaintInput>),
}

pub(super) struct TextPaintKey {
    source: TextPaintSource,
    bounds: [u32; 4],
    offset: [u32; 2],
    opacity: u32,
    color: Option<[u32; 4]>,
}

impl TextPaintKey {
    // Called only after the current source has passed visibility, preparation
    // and empty-content checks. Those checks must still run on every lookup.
    pub(super) fn new(
        text: &Text,
        source: &super::render::ShadowTextPaintSource<'_>,
        offset: [f32; 2],
        opacity: f32,
    ) -> Self {
        let bounds = source.bounds;
        Self {
            source: match &text.inline_ifc_owned {
                Some(owned) => TextPaintSource::Owned(owned.paint_input.clone()),
                None => TextPaintSource::Standalone(text.shaped_context.as_ref().unwrap().clone()),
            },
            bounds: [bounds.x, bounds.y, bounds.width, bounds.height].map(f32::to_bits),
            offset: offset.map(f32::to_bits),
            opacity: opacity.to_bits(),
            color: source.color_override.map(|color| color.map(f32::to_bits)),
        }
    }

    pub(super) fn matches(&self, other: &Self) -> bool {
        let same_source = match (&self.source, &other.source) {
            (TextPaintSource::Standalone(a), TextPaintSource::Standalone(b)) => Arc::ptr_eq(a, b),
            (TextPaintSource::Owned(a), TextPaintSource::Owned(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        same_source
            && self.bounds == other.bounds
            && self.offset == other.offset
            && self.opacity == other.opacity
            && self.color == other.color
    }
}

pub(super) struct TextInstallMemo {
    installed: Arc<InlineIfcTextPassPaintInput>,
    expected: Arc<InlineIfcTextPassPaintInput>,
    equal: bool,
}

impl TextInstallMemo {
    pub(super) fn matches(
        slot: &RefCell<Option<Self>>,
        installed: &Arc<InlineIfcTextPassPaintInput>,
        expected: &Arc<InlineIfcTextPassPaintInput>,
    ) -> bool {
        if let Some(memo) = slot.borrow().as_ref().filter(|memo| {
            Arc::ptr_eq(&memo.installed, installed) && Arc::ptr_eq(&memo.expected, expected)
        }) {
            return memo.equal;
        }
        // Pointer equality alone is insufficient: a NaN-bearing payload is
        // unequal even to itself. Memoize the actual structural comparison.
        let equal = installed.as_ref() == expected.as_ref();
        *slot.borrow_mut() = Some(Self {
            installed: installed.clone(),
            expected: expected.clone(),
            equal,
        });
        equal
    }
}
