//! Cold-path + incremental prop dispatch for Text.

use crate::ui::{PropValue, RsxElementNode};
use crate::view::fiber_work::{ApplyContext, PropApplyOutcome};
use crate::view::node_arena::{NodeArena, NodeKey};

use super::Text;

impl Text {
    pub(super) fn ingest_props_impl(&mut self, node: &RsxElementNode) -> Result<(), String> {
        use crate::view::renderer_adapter::{as_f32, as_string, as_text_align};
        for (key, value) in node.props.iter() {
            match *key {
                // Cold-path shell owns identity, layered style, and
                // cascade-resolved font_size.
                "key" | "style" | "font_size" => {}
                "line_height" => self.set_line_height(as_f32(value, key)?),
                "align" => self.set_text_align(as_text_align(value, key)?),
                "font" => self.set_font(as_string(value, key)?),
                "opacity" => self.set_opacity(as_f32(value, key)?),
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
        use crate::view::fiber_work::{PropApplyOutcome, resolve_font_size_px_with_inherited};
        use crate::view::renderer_adapter::{
            InheritedTextStyle, as_f32, as_string, as_text_align, as_text_style,
            inherited_text_style_at_parent,
        };

        let resolve_inherited = || -> InheritedTextStyle {
            match arena.parent_of(self_key) {
                Some(p) => inherited_text_style_at_parent(
                    arena,
                    p,
                    ctx.viewport_style,
                    ctx.viewport_width,
                    ctx.viewport_height,
                ),
                None => InheritedTextStyle::from_viewport_style(
                    ctx.viewport_style,
                    ctx.viewport_width,
                    ctx.viewport_height,
                ),
            }
        };

        match name {
            "style" => {
                // 軌 1 #8: replay cold-path style fan-out on the live
                // Text. Explicit flags are preserved (M3) so any
                // declaration dropped from the new style picks up the
                // ancestor cascade rather than re-resetting.
                let Ok(style) = as_text_style(&value, name) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                let inherited = resolve_inherited();
                self.apply_style_incremental(Some(&style), &inherited);
                PropApplyOutcome::Applied
            }
            "font_size" => {
                let inherited = resolve_inherited();
                let Some(px) = resolve_font_size_px_with_inherited(&value, &inherited) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.set_font_size(px);
                PropApplyOutcome::Applied
            }
            "line_height" => {
                let Ok(v) = as_f32(&value, name) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.set_line_height(v);
                PropApplyOutcome::Applied
            }
            "align" => {
                let Ok(align) = as_text_align(&value, name) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.set_text_align(align);
                PropApplyOutcome::Applied
            }
            "opacity" => {
                let Ok(v) = as_f32(&value, name) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.set_opacity(v);
                PropApplyOutcome::Applied
            }
            "font" => {
                let Ok(family) = as_string(&value, name) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.set_font(family);
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
        use crate::view::renderer_adapter::{InheritedTextStyle, inherited_text_style_at_parent};

        match name {
            "opacity" => {
                self.set_opacity(1.0);
                PropApplyOutcome::Applied
            }
            "style" => {
                // 軌 1 #8: `style` removed entirely. Reset every
                // explicit flag and replay ancestor cascade so all
                // formerly-authored props fall back to inherited
                // values (or Text defaults where inherited is None).
                let inherited = match arena.parent_of(self_key) {
                    Some(p) => inherited_text_style_at_parent(
                        arena,
                        p,
                        ctx.viewport_style,
                        ctx.viewport_width,
                        ctx.viewport_height,
                    ),
                    None => InheritedTextStyle::from_viewport_style(
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
