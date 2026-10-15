//! Cold-path + incremental prop dispatch for Text.

use crate::ui::{PropValue, RsxElementNode};
use crate::view::fiber_work::{ApplyContext, PropApplyOutcome};
use crate::view::node_arena::{NodeArena, NodeKey};

use super::Text;

impl Text {
    pub(super) fn ingest_props_impl(&mut self, node: &RsxElementNode) -> Result<(), String> {
        for (key, _) in node.props.iter() {
            match *key {
                // The cold-path shell owns identity and the style.
                "key" | "style" => {}
                _ => return Err(format!("unknown prop `{}` on <Text>", key)),
            }
        }
        Ok(())
    }

    pub(super) fn apply_prop_impl(
        &mut self,
        arena: &mut NodeArena,
        self_key: NodeKey,
        ctx: &ApplyContext<'_>,
        name: &'static str,
        value: PropValue,
    ) -> PropApplyOutcome {
        use crate::view::fiber_work::PropApplyOutcome;
        use crate::view::renderer_adapter::{
            StyleCascadeContext, as_text_style, style_cascade_at_parent,
        };

        let resolve_inherited = || -> StyleCascadeContext {
            match arena.parent_of(self_key) {
                Some(p) => style_cascade_at_parent(
                    arena,
                    p,
                    ctx.viewport_style,
                    ctx.viewport_width,
                    ctx.viewport_height,
                ),
                None => StyleCascadeContext::from_viewport_style(
                    ctx.viewport_style,
                    ctx.viewport_width,
                    ctx.viewport_height,
                ),
            }
        };

        match name {
            "style" => {
                // Replays the cold-path style on the live Text. A field
                // dropped from the new style falls back to the inherited
                // value, or else the default.
                let Ok(style) = as_text_style(&value, name) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                let inherited = resolve_inherited();
                self.apply_style_incremental(Some(&style), &inherited);
                PropApplyOutcome::Applied
            }
            _ => PropApplyOutcome::UnknownProp,
        }
    }

    pub(super) fn reset_prop_impl(
        &mut self,
        arena: &mut NodeArena,
        self_key: NodeKey,
        ctx: &ApplyContext<'_>,
        name: &'static str,
    ) -> PropApplyOutcome {
        use crate::view::fiber_work::PropApplyOutcome;
        use crate::view::renderer_adapter::{StyleCascadeContext, style_cascade_at_parent};

        match name {
            "style" => {
                // 軌 1 #8: `style` removed entirely. Reset every
                // explicit flag and replay ancestor cascade so all
                // formerly-authored props fall back to inherited
                // values (or Text defaults where inherited is None).
                let inherited = match arena.parent_of(self_key) {
                    Some(p) => style_cascade_at_parent(
                        arena,
                        p,
                        ctx.viewport_style,
                        ctx.viewport_width,
                        ctx.viewport_height,
                    ),
                    None => StyleCascadeContext::from_viewport_style(
                        ctx.viewport_style,
                        ctx.viewport_width,
                        ctx.viewport_height,
                    ),
                };
                self.apply_style_incremental(None, &inherited);
                PropApplyOutcome::Applied
            }
            _ => PropApplyOutcome::CannotReset(name),
        }
    }
}
