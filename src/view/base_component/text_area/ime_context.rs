//! `TextAreaImeContext` — pushed to projection child subtree via
//! `provide_context_node` when the TextArea cursor falls inside a projection.
//! Projection-internal widgets read it via `use_context::<TextAreaImeContext>()`.
//!
//! See decision A7 in `docs/design/textarea-v2.md`.

#[derive(Clone, Debug)]
pub struct TextAreaImeContext {
    pub preedit: String,
    pub preedit_cursor: Option<(usize, usize)>,
    pub local_cursor_in_projection: usize,
}
