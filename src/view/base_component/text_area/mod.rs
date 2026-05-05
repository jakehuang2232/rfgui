//! TextArea v2 — inline formatting context container.
//!
//! P1 skeleton. Real impls land in P2 (layout/render), P3 (edit), P4 (IME),
//! P5 (projection), P6 (reconcile diff), P7 (migration). See
//! `docs/design/textarea-v2.md` for the design and phase plan.
//!
//! v2 lives under tag `<TextArea>` while v1 (`<TextArea>`) is unchanged.
//! Rename + v1 removal happens in P7.

#![allow(dead_code)] // P1: stubs; fields wired in later phases.

mod caret_map;
mod edit;
mod events;
mod hit_test;
mod ime_context;
mod layout;
mod projection;
mod reconcile;
mod render;
mod render_string;
mod run;
mod segment;
mod state;

pub use ime_context::TextAreaImeContext;
pub use render_string::{TextAreaRenderProjection, TextAreaRenderString};
#[allow(unused_imports)] // re-exported for P2+; not yet referenced outside the module.
pub(crate) use run::TextAreaTextRun;
#[allow(unused_imports)] // P8 M1+: emitted by TextArea schema render.
pub(crate) use segment::TextAreaProjectionSegment;

use std::ops::Range;

use crate::style::Cursor;
use crate::time::Instant;
use crate::ui::{
    Binding, BlurHandlerProp, Rect, TextAreaFocusHandlerProp, TextAreaRenderHandlerProp,
    TextChangeHandlerProp,
};
use crate::view::base_component::{BoxModelSnapshot, DirtyFlags, ElementTrait};
use crate::view::layout::{FlexLayoutInfo, LayoutState};
use crate::view::node_arena::NodeKey;

use super::next_ui_node_id;

/// TextArea v2 — see `docs/design/textarea-v2.md`.
///
/// Decision A1: NOT-IS-A Element. Box model lives in a wrapping `<Element>`.
/// Decision A3: children are mixed `TextAreaTextRun` + projection RsxNodes,
/// all real arena children laid out via `view/layout/*` Inline pipeline.
/// Decision A9: char index is the single source of truth; children carry no
/// cursor/selection/IME state.
pub struct TextArea {
    // text
    pub(crate) content: String,
    pub(crate) placeholder: String,
    pub(crate) placeholder_color: crate::style::Color,
    pub(crate) read_only: bool,
    pub(crate) multiline: bool,
    pub(crate) auto_wrap: bool,
    pub(crate) max_length: Option<usize>,
    pub(crate) text_binding: Option<Binding<String>>,
    pub(crate) font_families: Vec<String>,
    pub(crate) font_size: f32,
    pub(crate) font_weight: u16,
    pub(crate) line_height: f32,
    pub(crate) color: crate::style::Color,
    pub(crate) cursor: Cursor,

    // cursor / selection / IME / focus
    pub(crate) cursor_char: usize,
    /// Soft-wrap caret affinity. char index alone is ambiguous at a wrap
    /// boundary (= "end of upper line" === "start of lower line"); this
    /// disambiguates. Default `Downstream` (start of lower line) matches
    /// the long-standing pre-affinity behaviour. Set to `Upstream` by
    /// Cmd+Right and other line-end navigations.
    pub(crate) cursor_affinity: caret_map::CaretAffinity,
    pub(crate) selection_anchor_char: Option<usize>,
    pub(crate) selection_focus_char: Option<usize>,
    pub(crate) selection_background_color: crate::style::Color,
    pub(crate) pointer_selecting: bool,
    pub(crate) is_focused: bool,
    pub(crate) scroll_x: f32,
    pub(crate) scroll_y: f32,
    pub(crate) viewport_size: crate::view::base_component::Size,
    pub(crate) pending_caret_scroll: bool,
    pub(crate) ime_preedit: String,
    pub(crate) ime_preedit_cursor: Option<(usize, usize)>,
    pub(crate) vertical_cursor_x: Option<f32>,
    pub(crate) caret_blink_started_at: Instant,

    // children (TextAreaTextRun + projection mixed)
    pub(crate) on_render_handler: Option<TextAreaRenderHandlerProp>,
    pub(crate) children: Vec<NodeKey>,
    pub(crate) child_char_ranges: Vec<Range<usize>>,
    /// P6 reconcile metadata, parallel to `children` / `child_char_ranges`.
    /// `Run` slots carry no user-state identity (Runs are owned by TextArea);
    /// `Projection` slots remember the projection root's `RsxNodeIdentity`
    /// (post-Provider-unwrap) plus the last `RsxNode` so the next rebuild
    /// can identity-match → `reconcile_existing_subtree` instead of full
    /// teardown.
    pub(crate) child_slots: Vec<crate::view::base_component::text_area::projection::ChildSlot>,
    pub(crate) self_node_key: Option<NodeKey>,
    pub(crate) children_dirty: bool,

    // layout output
    pub(crate) flow_offset: crate::view::base_component::Position,
    pub(crate) layout_state: LayoutState,
    pub(crate) inline_paint_fragments: Vec<Rect>,
    pub(crate) flex_info: Option<FlexLayoutInfo>,
    pub(crate) dirty_flags: DirtyFlags,

    // handlers
    pub(crate) on_change_handlers: Vec<TextChangeHandlerProp>,
    pub(crate) on_focus_handlers: Vec<TextAreaFocusHandlerProp>,
    pub(crate) on_blur_handlers: Vec<BlurHandlerProp>,

    // identity
    pub(crate) node_id: u64,
    pub(crate) parent_id: Option<u64>,
}

impl Default for TextArea {
    fn default() -> Self {
        Self {
            content: String::new(),
            placeholder: String::new(),
            placeholder_color: crate::style::Color::rgba(125, 133, 150, 255),
            read_only: false,
            multiline: true,
            auto_wrap: true,
            max_length: None,
            text_binding: None,
            font_families: Vec::new(),
            font_size: 14.0,
            font_weight: 400,
            line_height: 1.25,
            color: crate::style::Color::rgba(17, 17, 17, 255),
            cursor: Cursor::Text,

            cursor_char: 0,
            cursor_affinity: caret_map::CaretAffinity::Downstream,
            selection_anchor_char: None,
            selection_focus_char: None,
            selection_background_color: crate::style::Color::rgba(71, 133, 240, 89),
            pointer_selecting: false,
            is_focused: false,
            scroll_x: 0.0,
            scroll_y: 0.0,
            viewport_size: crate::view::base_component::Size {
                width: 0.0,
                height: 0.0,
            },
            pending_caret_scroll: false,
            ime_preedit: String::new(),
            ime_preedit_cursor: None,
            vertical_cursor_x: None,
            caret_blink_started_at: Instant::now(),

            on_render_handler: None,
            children: Vec::new(),
            child_char_ranges: Vec::new(),
            child_slots: Vec::new(),
            self_node_key: None,
            children_dirty: true,

            flow_offset: crate::view::base_component::Position { x: 0.0, y: 0.0 },
            layout_state: LayoutState::new(0.0, 0.0, 0.0, 0.0),
            inline_paint_fragments: Vec::new(),
            flex_info: None,
            dirty_flags: DirtyFlags::ALL,

            on_change_handlers: Vec::new(),
            on_focus_handlers: Vec::new(),
            on_blur_handlers: Vec::new(),

            node_id: next_ui_node_id(),
            parent_id: None,
        }
    }
}

impl TextArea {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct with an externally-supplied stable id (matches the
    /// `from_*_with_id` pattern used by v1 builtin host tags so the
    /// descriptor pipeline can keep node identity stable across renders).
    pub fn with_stable_id(node_id: u64) -> Self {
        Self {
            node_id,
            ..Self::default()
        }
    }

    pub(crate) fn set_self_node_key(&mut self, key: NodeKey) {
        self.self_node_key = Some(key);
    }

    /// Patch entrypoint for `Patch::SetText` from incremental commit.
    /// Mirrors v1's surface so `fiber_work::apply_set_text_to_host` can
    /// route SetText to either Text or TextArea uniformly.
    pub fn set_text(&mut self, value: String) {
        self.set_content_from_external(value);
    }

    /// Re-apply ancestor-derived inherited cascade to props the user
    /// didn't author explicitly. Called by
    /// [`crate::view::fiber_work::recascade_text_subtree`] after an
    /// ancestor style change.
    ///
    /// v2 doesn't yet track per-prop "explicit" flags (all TextArea
    /// authors set values through `apply_prop` which writes
    /// unconditionally). Conservative behaviour: overwrite
    /// font_families when empty (default), font_size only when it still
    /// matches the previously-cached inherited value (i.e. nothing else
    /// has touched it since), and likewise for color / font_weight.
    /// Marks content dirty so the next rebuild re-cascades the Run
    /// children.
    pub(crate) fn apply_inherited(
        &mut self,
        inherited: &crate::view::renderer_adapter::InheritedTextStyle,
    ) -> bool {
        let mut changed = false;
        if self.font_families.is_empty() && !inherited.font_families.is_empty() {
            self.font_families = inherited.font_families.clone();
            changed = true;
        }
        if let Some(fs) = inherited.font_size
            && (self.font_size - fs).abs() > f32::EPSILON
            && (self.font_size - 14.0).abs() < f32::EPSILON
        {
            self.font_size = fs;
            changed = true;
        }
        if let Some(fw) = inherited.font_weight
            && self.font_weight == 400
            && fw != 400
        {
            self.font_weight = fw;
            changed = true;
        }
        if let Some(color) = inherited.color
            && self.color == crate::style::Color::rgba(17, 17, 17, 255)
        {
            self.color = color;
            changed = true;
        }
        if changed {
            self.mark_content_dirty();
        }
        changed
    }
}

impl ElementTrait for TextArea {
    fn stable_id(&self) -> u64 {
        self.node_id
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.node_id,
            parent_id: self.parent_id,
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self.layout_state.layout_size.width,
            height: self.layout_state.layout_size.height,
            border_radius: 0.0,
            should_render: self.layout_state.should_render,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn children(&self) -> &[NodeKey] {
        &self.children
    }

    fn children_mut(&mut self) -> Option<&mut Vec<NodeKey>> {
        Some(&mut self.children)
    }

    fn parent_id(&self) -> Option<u64> {
        self.parent_id
    }

    fn set_parent_id(&mut self, parent_id: Option<u64>) {
        self.parent_id = parent_id;
    }

    fn local_dirty_flags(&self) -> DirtyFlags {
        self.dirty_flags
    }

    fn clear_local_dirty_flags(&mut self, flags: DirtyFlags) {
        self.dirty_flags = self.dirty_flags.without(flags);
    }

    fn promotion_node_info(&self) -> crate::view::promotion::PromotionNodeInfo {
        // Selection / glyphs / caret = three render passes on a fully-
        // populated frame. Opacity 1.0 since v2 has no opacity field
        // (decision A1: opacity lives on the wrapping `<Element>`).
        crate::view::promotion::PromotionNodeInfo {
            estimated_pass_count: 3,
            opacity: 1.0,
            ..Default::default()
        }
    }

    /// Phase 2: TextArea's children loop dispatches promoted children
    /// through `Element::build_promoted_child` (allocates a layer target,
    /// runs the build, and composites the layer back onto TextArea's
    /// current target). Projection `<Element>` children can therefore
    /// promote independently and benefit from layer reuse.
    fn supports_promoted_descendants(&self) -> bool {
        true
    }

    /// Hash every visible-state field so a promoted ancestor's
    /// `base_signature` dirties on edit / cursor / selection / IME / focus
    /// / blink. Without this the default `0` lets the ancestor reuse a
    /// stale layer texture (text edits invisible until the ancestor is
    /// independently dirtied).
    fn promotion_self_signature(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.layout_state.should_render.hash(&mut hasher);
        self.layout_state
            .layout_position
            .x
            .to_bits()
            .hash(&mut hasher);
        self.layout_state
            .layout_position
            .y
            .to_bits()
            .hash(&mut hasher);
        self.layout_state
            .layout_size
            .width
            .max(0.0)
            .to_bits()
            .hash(&mut hasher);
        self.layout_state
            .layout_size
            .height
            .max(0.0)
            .to_bits()
            .hash(&mut hasher);
        self.content.hash(&mut hasher);
        self.placeholder.hash(&mut hasher);
        self.color.to_rgba_u8().hash(&mut hasher);
        self.placeholder_color.to_rgba_u8().hash(&mut hasher);
        self.selection_background_color
            .to_rgba_u8()
            .hash(&mut hasher);
        self.font_families.hash(&mut hasher);
        self.font_size.to_bits().hash(&mut hasher);
        self.line_height.to_bits().hash(&mut hasher);
        self.font_weight.hash(&mut hasher);
        self.multiline.hash(&mut hasher);
        self.auto_wrap.hash(&mut hasher);
        self.read_only.hash(&mut hasher);
        self.cursor_char.hash(&mut hasher);
        self.cursor_affinity.hash(&mut hasher);
        self.selection_anchor_char.hash(&mut hasher);
        self.selection_focus_char.hash(&mut hasher);
        self.scroll_x.to_bits().hash(&mut hasher);
        self.scroll_y.to_bits().hash(&mut hasher);
        self.ime_preedit.hash(&mut hasher);
        self.ime_preedit_cursor.hash(&mut hasher);
        self.is_focused.hash(&mut hasher);
        self.should_draw_caret().hash(&mut hasher);
        self.children.len().hash(&mut hasher);
        hasher.finish()
    }

    fn apply_inherited(&mut self, inherited: &crate::view::renderer_adapter::InheritedTextStyle) {
        TextArea::apply_inherited(self, inherited);
    }

    fn after_commit(&mut self, _arena: &mut crate::view::node_arena::NodeArena, self_key: NodeKey) {
        self.set_self_node_key(self_key);
    }

    fn build_children(
        &self,
        _node: &crate::ui::RsxElementNode,
        _path: &[u64],
        _global_path: Option<&crate::view::renderer_adapter::GlobalNodePath>,
        _inherited: &crate::view::renderer_adapter::InheritedTextStyle,
    ) -> Result<Vec<crate::view::renderer_adapter::ElementDescriptor>, String> {
        // Spawn a single `TextAreaTextRun` from `self.content` (or
        // placeholder fallback). RSX `node.children` are not walked
        // here — projection segments rebuild lazily via
        // `rebuild_projection_tree_if_dirty` after the TextArea
        // exists in the arena.
        let mut child_descriptors: Vec<crate::view::renderer_adapter::ElementDescriptor> =
            Vec::new();
        let (display_text, is_placeholder) = if !self.content.is_empty() {
            (self.content.clone(), false)
        } else if !self.placeholder.is_empty() {
            (self.placeholder.clone(), true)
        } else {
            (String::new(), false)
        };
        if !display_text.is_empty() {
            let char_count = display_text.chars().count();
            let mut run = run::TextAreaTextRun::new(display_text, 0..char_count);
            run.is_placeholder = is_placeholder;
            run.cascade_style(
                self.font_families.clone(),
                self.font_size,
                self.line_height,
                self.font_weight,
                if is_placeholder {
                    self.placeholder_color
                } else {
                    self.color
                },
                self.cursor,
                self.auto_wrap,
            );
            child_descriptors.push(crate::view::renderer_adapter::ElementDescriptor::leaf(
                Box::new(run) as Box<dyn ElementTrait>,
            ));
        }
        Ok(child_descriptors)
    }

    fn ingest_props(&mut self, node: &crate::ui::RsxElementNode) -> Result<(), String> {
        use crate::ui::FromPropValue;
        use crate::view::base_component::as_blur_handler;
        use crate::view::renderer_adapter::{
            as_binding_string, as_bool, as_owned_string, as_usize,
        };
        for (key, value) in node.props.iter() {
            match *key {
                // Cold-path-owned: identity, layered style, explicit
                // font-priority block (cascade-resolved).
                "key" | "style" | "font" | "font_size" => {}
                "content" => self.content = as_owned_string(value, key)?,
                "placeholder" => self.placeholder = as_owned_string(value, key)?,
                "binding" => self.text_binding = Some(as_binding_string(value, key)?),
                "multiline" => self.multiline = as_bool(value, key)?,
                "auto_wrap" => self.auto_wrap = as_bool(value, key)?,
                "read_only" => self.read_only = as_bool(value, key)?,
                "max_length" => self.max_length = as_usize(value, key)?,
                "on_focus" => self.on_focus_handlers.push(
                    crate::ui::TextAreaFocusHandlerProp::from_prop_value(value.clone()).map_err(
                        |_| format!("prop `{key}` expects text area focus handler value"),
                    )?,
                ),
                "on_blur" => self.on_blur_handlers.push(as_blur_handler(value, key)?),
                "on_change" => self.on_change_handlers.push(
                    crate::ui::TextChangeHandlerProp::from_prop_value(value.clone())
                        .map_err(|_| format!("prop `{key}` expects text change handler value"))?,
                ),
                "on_render" => {
                    self.on_render_handler = Some(
                        crate::ui::TextAreaRenderHandlerProp::from_prop_value(value.clone())
                            .map_err(|_| {
                                format!("prop `{key}` expects text area render handler value")
                            })?,
                    );
                }
                _ => return Err(format!("unknown prop `{}` on <TextArea>", key)),
            }
        }
        Ok(())
    }

    /// Real incremental apply path. Mirrors v1's surface (decision: keep
    /// the apply matrix shape parity-with-v1 to ease P7 migration), minus
    /// the box-model props that v2 rejects per design A1.
    fn apply_prop(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
        ctx: &crate::view::fiber_work::ApplyContext<'_>,
        name: &'static str,
        value: crate::ui::PropValue,
    ) -> crate::view::fiber_work::PropApplyOutcome {
        use crate::style::{ParsedValue, PropertyId};
        use crate::ui::FromPropValue;
        use crate::view::fiber_work::{PropApplyOutcome, resolve_font_size_px_with_inherited};
        use crate::view::renderer_adapter::{InheritedTextStyle, inherited_text_style_at_parent};

        self.set_self_node_key(self_key);

        let resolve_inherited = |arena: &crate::view::node_arena::NodeArena| -> InheritedTextStyle {
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
            "key" => PropApplyOutcome::Applied,
            "binding" => {
                let Ok(bound) = crate::ui::Binding::<String>::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.set_content_from_external(bound.get());
                self.text_binding = Some(bound);
                PropApplyOutcome::Applied
            }
            "content" => {
                let Ok(s) = String::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.set_content_from_external(s);
                PropApplyOutcome::Applied
            }
            "placeholder" => {
                let Ok(s) = String::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                if self.placeholder != s {
                    self.placeholder = s;
                    self.mark_content_dirty();
                }
                PropApplyOutcome::Applied
            }
            "multiline" => {
                let Ok(v) = bool::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                if self.multiline != v {
                    self.multiline = v;
                    if !v && self.content.contains('\n') {
                        self.content = self.content.replace('\n', " ");
                        self.sync_bound_text();
                    }
                    self.mark_content_dirty();
                }
                PropApplyOutcome::Applied
            }
            "auto_wrap" => {
                let Ok(v) = bool::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                if self.auto_wrap != v {
                    self.auto_wrap = v;
                    // mark_content_dirty triggers rebuild_children_if_dirty
                    // which re-cascades the Run subtree. No standalone
                    // recascade needed.
                    self.mark_content_dirty();
                }
                PropApplyOutcome::Applied
            }
            "read_only" => {
                let Ok(v) = bool::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.read_only = v;
                PropApplyOutcome::Applied
            }
            "max_length" => {
                let v = match &value {
                    crate::ui::PropValue::I64(i) => Some((*i).max(0) as usize),
                    crate::ui::PropValue::F64(f) => Some((*f).max(0.0) as usize),
                    _ => return PropApplyOutcome::DecodeFailed(name),
                };
                self.max_length = v;
                PropApplyOutcome::Applied
            }
            "font" => {
                let Ok(s) = String::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.font_families = vec![s];
                self.mark_content_dirty();
                PropApplyOutcome::Applied
            }
            "font_size" => {
                let inherited = resolve_inherited(arena);
                let Some(px) = resolve_font_size_px_with_inherited(&value, &inherited) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                if (self.font_size - px).abs() > f32::EPSILON {
                    self.font_size = px;
                    self.mark_content_dirty();
                }
                PropApplyOutcome::Applied
            }
            "style" => {
                let Ok(style) = crate::view::renderer_adapter::as_element_style(&value, name)
                else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                let inherited = resolve_inherited(arena);
                if let Some(ParsedValue::Color(color)) = style.get(PropertyId::Color) {
                    self.color = color.to_color();
                }
                if let Some(ParsedValue::FontSize(size)) = style.get(PropertyId::FontSize) {
                    self.font_size = size.resolve_px(
                        inherited.font_size.unwrap_or(inherited.root_font_size),
                        inherited.root_font_size,
                        inherited.viewport_width,
                        inherited.viewport_height,
                    );
                }
                if let Some(ParsedValue::FontFamily(family)) = style.get(PropertyId::FontFamily) {
                    self.font_families = family.as_slice().to_vec();
                }
                if let Some(ParsedValue::FontWeight(fw)) = style.get(PropertyId::FontWeight) {
                    self.font_weight = fw.value();
                }
                self.mark_content_dirty();
                PropApplyOutcome::Applied
            }
            "on_change" => {
                let Ok(handler) = crate::ui::TextChangeHandlerProp::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.on_change_handlers.clear();
                self.on_change_handlers.push(handler);
                PropApplyOutcome::Applied
            }
            "on_focus" => {
                let Ok(handler) = crate::ui::TextAreaFocusHandlerProp::from_prop_value(value)
                else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.on_focus_handlers.clear();
                self.on_focus_handlers.push(handler);
                PropApplyOutcome::Applied
            }
            "on_blur" => {
                let Ok(handler) = crate::ui::BlurHandlerProp::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.on_blur_handlers.clear();
                self.on_blur_handlers.push(handler);
                PropApplyOutcome::Applied
            }
            "on_render" => {
                let Ok(handler) = crate::ui::TextAreaRenderHandlerProp::from_prop_value(value)
                else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.on_render_handler = Some(handler);
                self.mark_content_dirty();
                PropApplyOutcome::Applied
            }
            _ => PropApplyOutcome::UnknownProp,
        }
    }

    fn reset_prop(
        &mut self,
        _arena: &mut crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
        _ctx: &crate::view::fiber_work::ApplyContext<'_>,
        name: &'static str,
    ) -> crate::view::fiber_work::PropApplyOutcome {
        use crate::view::fiber_work::PropApplyOutcome;
        match name {
            "key" => PropApplyOutcome::Applied,
            "binding" => {
                self.text_binding = None;
                PropApplyOutcome::Applied
            }
            "content" => {
                self.set_content_from_external(String::new());
                PropApplyOutcome::Applied
            }
            "placeholder" => {
                if !self.placeholder.is_empty() {
                    self.placeholder.clear();
                    self.mark_content_dirty();
                }
                PropApplyOutcome::Applied
            }
            "multiline" => {
                self.multiline = true;
                self.mark_content_dirty();
                PropApplyOutcome::Applied
            }
            "auto_wrap" => {
                self.auto_wrap = true;
                self.mark_content_dirty();
                PropApplyOutcome::Applied
            }
            "read_only" => {
                self.read_only = false;
                PropApplyOutcome::Applied
            }
            "max_length" => {
                self.max_length = None;
                PropApplyOutcome::Applied
            }
            "on_change" => {
                self.on_change_handlers.clear();
                PropApplyOutcome::Applied
            }
            "on_focus" => {
                self.on_focus_handlers.clear();
                PropApplyOutcome::Applied
            }
            "on_blur" => {
                self.on_blur_handlers.clear();
                PropApplyOutcome::Applied
            }
            "on_render" => {
                self.on_render_handler = None;
                self.mark_content_dirty();
                PropApplyOutcome::Applied
            }
            // Style / font props can't be reverted to "inherited" without a
            // full descriptor rebuild — fall back to the cold path.
            "style" | "font" | "font_size" => PropApplyOutcome::CannotReset(name),
            _ => PropApplyOutcome::CannotReset(name),
        }
    }
}

fn known_prop(name: &str) -> bool {
    matches!(
        name,
        "content"
            | "binding"
            | "style"
            | "on_focus"
            | "on_blur"
            | "on_change"
            | "on_render"
            | "placeholder"
            | "font"
            | "font_size"
            | "multiline"
            | "auto_wrap"
            | "read_only"
            | "max_length"
    )
}
