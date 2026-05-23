use crate::style::ComputedStyle;

/// Internal contract for retained host nodes that consume normalized computed
/// style. Each consumer owns how the computed fields affect its layout,
/// placement, paint, and transition state.
///
/// This trait is intentionally narrow: it is for hosts where a complete
/// `ComputedStyle` can be applied directly to the retained node. It does not
/// model authored-field masks, inherited cascade context, or explicit prop
/// priority. Components that need those inputs should normalize through a
/// local style bridge first and only apply the fields that were actually
/// authored for that component.
pub(crate) trait ComputedStyleConsumer {
    type Snapshot;

    fn apply_computed_style(
        &mut self,
        computed: ComputedStyle,
        previous_snapshot: Option<&Self::Snapshot>,
    );
}
