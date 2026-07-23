# Custom components under RetainedAuto

This guide helps third-party authors choose an integration path without weakening correctness. Read the normative [RetainedAuto Mode Contract](RETAINED_AUTO.md) first.

## Choose the integration class

Use the first class that fits:

1. **Native composition** — compose built-in nodes/styles. This has the broadest retained coverage and least custom paint code.
2. **Native-like typed artifact hook** — use the narrow public rectangle recorder for property-neutral self-paint.
3. **Custom GPU surface, Legacy-only** — keep arbitrary GPU/backend work in `Renderable::build` until RFGUI has a complete engine-owned typed contract.

Do not approximate a shader, texture, clip, or multi-pass operation with an unrelated retained command merely to avoid fallback. Promotion is not a retained opt-in path.

An earlier candidate may reject while a later one wins. Determine success from `DebugRetainedAutoSnapshot.frame.selected_authority` together with `frame.disposition`, not from a fallback outline or rejection label. Presented non-Legacy authority is retained; `Legacy` plus `FellBackToLegacy` is whole-frame fallback.

## Common host obligations

Every `ElementTrait` host keeps these values coherent:

- `stable_id()` is unique and immutable while the host is alive;
- `box_model_snapshot().node_id` equals `stable_id()` and describes final placed geometry;
- `children()` mirrors canonical arena order and `sync_children_mirror()` copies it exactly;
- `Renderable::build` paints the complete correct Legacy fallback;
- setters invalidate the correct local and arena-owned layout/paint/topology state;
- animation uses the supplied `crate::time::Instant`;
- `prepare_paint_resources()` freezes post-layout resource state, while topology changes wait for the next `sync_arena()`.

Retained hooks are pure reads. Capability, metadata, and full recording may invoke them independently and repeatedly.

The hook is only the first proof step. The engine derives `PaintChunkMetadata`, records `PaintArtifact`, validates `PaintPayloadIdentity`, ownership/properties/store, and compiles a private typed token. The hook never owns chunk IDs, `ArtifactStoreValidationPolicy`, `RetainedSurfaceRasterStamp`, resident keys, or `RetainedPropertyScrollSceneTransaction`.

## Native composition

Prefer `#[component]`, RSX, and built-in `Element`, `Text`, `TextArea`, `Image`, and `Svg`. The engine then owns each native node's typed paint contract, properties, traversal, and resource lifecycle.

Native composition does not imply that every property combination is already retained. Unsupported combinations still fail closed as a complete Legacy frame.

## Native-like custom retained leaf

A v1 custom leaf has no children and records exactly one finite fill covering the engine-provided logical bounds.

```rust
use rfgui::view::base_component::{
    CustomLeafPaintContext, CustomLeafPaintRecorder, ElementTrait,
};

impl ElementTrait for StatusSwatch {
    // Other ElementTrait/supertrait methods are implemented elsewhere.
    fn record_custom_leaf_paint(
        &self,
        context: CustomLeafPaintContext,
        recorder: &mut CustomLeafPaintRecorder,
    ) {
        recorder.fill_rect(context.bounds(), self.linear_rgba, self.opacity);
    }
}
```

The current leaf grammar rejects:

- zero or multiple commands;
- a rectangle that differs bitwise from `context.bounds()`;
- zero, NaN, or infinite bounds;
- non-finite color/opacity or channels outside `0..=1`;
- children, deferred paint, active animation/runtime layout state, border, radius, shadow, scroll, or non-neutral property state.

Do not cache the context/recorder or create your own chunk identity.

## Native-like custom retained wrapper

A wrapper may record property-neutral self-paint before and/or after canonical arena children. The engine traverses children exactly once.

```rust
use rfgui::view::base_component::{
    CustomWrapperPaintContext, CustomWrapperPaintRecorder, ElementTrait,
};
use rfgui::view::NodeKey;

impl ElementTrait for PanelHost {
    fn record_custom_wrapper_paint(
        &self,
        context: CustomWrapperPaintContext,
        recorder: &mut CustomWrapperPaintRecorder,
    ) {
        let bounds = context.bounds();
        recorder.fill_rect_before_children(bounds, self.background_rgba, 1.0);
        recorder.fill_rect_after_children(bounds, self.overlay_rgba, self.overlay_opacity);
    }

    fn children(&self) -> &[NodeKey] {
        &self.children
    }

    fn sync_children_mirror(&mut self, children: &[NodeKey]) {
        self.children.clear();
        self.children.extend_from_slice(children);
    }
}
```

Each command covers the exact supplied bounds and uses finite linear-RGBA/opacity in `0..=1`. Append order becomes canonical slot order, and a phase cannot exceed `u16` slot capacity.

The fixed sequence is before-children commands, canonical children, then after-children commands. Never call `child.build(...)`, hide passes between children, reorder children for z-order, or use a stale child mirror.

## Resources and deferred paint

Keep asynchronous transitions out of recording:

```text
resource completion
    -> queue state/topology change
next frame sync_arena()
    -> commit loading/ready/error topology before layout
final layout
    -> prepare_paint_resources(frame_number, device_scale, now)
    -> freeze request/upload snapshot
metadata + full recording
    -> read exactly that frozen snapshot
```

If readiness or generation changes between metadata and full recording, reject/fallback rather than combining observations.

Deferred viewport paint is a distinct late phase. V1 custom leaf/wrapper recording rejects `is_deferred_to_root_viewport_render() == true`. Do not paint that subtree in the normal retained hook or repeat it after children. Without an engine-owned deferred witness/compiler contract, keep the full behavior in Legacy.

## DPR and coordinate spaces

Recorder contexts expose logical bounds. Pass the exact bounds back; do not pre-scale them. The engine freezes device scale and derives physical texture descriptors, descriptor origins, and scissors.

A future retained custom-surface identity must freeze logical source/composite geometry plus every device-dependent descriptor field. Test DPR1 and DPR2, non-zero origins, clips, unchanged-scale reuse, scale changes, and descriptor tampering. DPR1-only allocation is not retained DPR support.

## Custom GPU surface: Legacy until sealed

Arbitrary GPU work stays in complete Legacy `Renderable::build`. Leave `record_custom_leaf_paint` and `record_custom_wrapper_paint` at their defaults.

The internal fallback is `LegacyPaintReason::UnknownHost`; public debug snapshots expose `DebugFallbackCategory::UnsupportedHost` with reason `unknown-host`. This is the required final state when no compiler-sealed typed contract exists.

Do not put `wgpu` handles, callbacks, or backend resources into the custom retained recorder. A future retained surface needs all of:

- an engine-owned backend-neutral typed operation and witness;
- frozen identity and metadata/full parity;
- artifact/store validation and a private compiler token;
- graph-inert preparation with geometry/descriptor/budget/key checks;
- failure-atomic pool actions and one full-set transaction;
- explicit reuse/reraster dependencies;
- stable debug reason mapping and the required test matrix.

A stable texture, cached Legacy output, stable ID, or promotion hint does not satisfy this list and MUST NOT be described as retained.

RFGUI core remains platform-independent. Do not add `winit`, `arboard`, `web-sys`, or another host/backend dependency to `rfgui/Cargo.toml`; backend code belongs in `examples/` or a downstream consumer.

## Implementation checklist

- Choose native composition, typed leaf/wrapper, or explicit Legacy-only GPU behavior.
- Keep `stable_id`, box model, child mirror, dirty flags, and Legacy build coherent.
- Make retained hooks repeatable pure reads.
- Include every visible/resource/device input in deterministic identity.
- Preserve engine-owned properties, traversal, phase/slot order, and deferred phase.
- Verify unsupported/malformed states fail before emission.
- For engine changes, seal compiler/preparation/transaction authority before claiming support.
- Add stable `DebugFallbackStage`, `DebugFallbackCategory`, and `DebugFallbackDetail` mapping.
- Verify final authority/disposition rather than candidate reasons or colors.

## Required verification matrix

| Area | Required cases |
| --- | --- |
| Authority | canonical retained selection; earlier rejection then retained success; unsealed GPU host final Legacy/`UnsupportedHost` |
| Metadata/full | repeated equality; payload/resource/order/topology drift; malformed bounds/color/opacity/cardinality |
| Properties/order | neutral fixture; each supported property; unsupported interleave; before/children/after; deferred ordering |
| Atomicity | context/descriptor/budget/key/action failure leaves graph/pool/stage unchanged; one transaction stage |
| Reuse | cold commit/reraster; unchanged reuse; every raster mutation; declared composition-only change |
| DPR | DPR1/DPR2; non-zero origin; logical/device clip; scale/descriptor/origin tamper |
| Debug | overlay off/on preserves authority, artifacts, actions, and application pixels; stable reason codes |
| Legacy | complete fallback and application-pixel/order parity for admitted fixtures |

Focused tests to copy or extend:

- `custom_leaf_typed_adapter_records_canonical_fill_and_compiles`
- `custom_leaf_metadata_full_drift_forces_whole_frame_fallback`
- `custom_wrapper_public_typed_phases_preserve_order_slots_and_compile`
- `custom_wrapper_topology_properties_and_unknown_child_fail_closed`
- `nested_and_multiple_deferred_viewport_roots_record_once_in_late_dfs_order`
- `nested_scroll_executor_preflight_failures_are_graph_pool_and_stage_atomic`
- `property_scene_transaction_commits_exact_multi_root_deep_forest_atomically`
- `frame_root_scroll_scene_dpr2_freezes_device_descriptors_and_emits`

Run at minimum:

```sh
cargo check -q -p rfgui
cargo doc -q -p rfgui --no-deps
git diff --check
```

## Prohibited shortcuts

- Treating candidate rejection, debug color, dirty state, stable ID, texture key, or promotion as authority.
- Minting or deserializing internal witnesses, stores, stamps, keys, or transactions in third-party code.
- Mutating state, polling resources, advancing time, or allocating identity in retained hooks.
- Traversing children, applying properties, altering opaque order, or issuing backend passes in retained hooks.
- Mixing logical/device coordinates or reusing across scale/format/descriptor drift.
- Staging residents independently or invoking Legacy after partial retained emission.
- Weakening a validator when no typed compiler and failure-atomic executor exist.

## Source anchors

- [`ElementTrait` and custom recorders](src/view/base_component/element/mod.rs)
- [`PaintArtifact`, identities, and `PaintNodePhase`](src/view/paint/artifact.rs)
- [`LegacyPaintReason`](src/view/paint/recorder.rs) and [`FrameArtifactFallbackReason`](src/view/paint/frame_recorder.rs)
- [`ArtifactStoreValidationPolicy`](src/view/paint/compiler.rs)
- [transactions, preparation, reuse, and DPR tests](src/view/paint/scroll_scene.rs)
- [`DebugRetainedAutoSnapshot` and reason/action enums](src/view/debug.rs)
- [authority selection and debug mappings](src/view/viewport/render.rs)
