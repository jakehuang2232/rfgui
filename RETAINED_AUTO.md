# RetainedAuto Mode Contract v1

> Status: normative contract for the current `ViewportPaintRendererMode::RetainedAuto` boundary. It defines correctness and extension requirements; it does not promise that every property combination already has a retained implementation.

`RetainedAuto` chooses the strongest authority that can prove a complete semantic frame. If no retained candidate proves the frame, Legacy owns the whole frame. Its goal is deterministic correctness with incremental reuse, not best-effort partial acceleration.

The words **MUST**, **MUST NOT**, **SHOULD**, and **MAY** are normative.

## 1. Frame authority

### 1.1 Whole-frame exclusivity

One semantic frame has exactly one paint authority:

- one retained/property/artifact plan owns the complete frame; or
- Legacy owns the complete frame.

Retained and Legacy output MUST NOT be mixed to fill gaps. Authority selection and retained preparation occur before the common frame clear. A rejected candidate MUST NOT emit passes, consume opaque-order state, install resident state, or mutate component state.

A compile or execute failure after retained ownership begins is terminal for that frame. The frame is rejected or aborted; it MUST NOT replay Legacy over partially emitted retained output. The viewport circuit breaker routes later `RetainedAuto` frames through whole-frame Legacy until an explicit renderer-mode setter resets it.

### 1.2 Candidate rejection is not final authority

`RetainedAuto` tries several candidates. `AutoAuthorityTrace` may therefore contain `AutoAuthorityRejection` entries even when a later candidate becomes the final retained authority. A candidate rejection says only that the named grammar did not prove the frame.

Read the final result from `DebugRetainedAutoSnapshot.frame`:

- non-`Legacy` `selected_authority` plus `Presented` means retained success;
- `Legacy` plus `FellBackToLegacy` means whole-frame Legacy;
- `Rejected` or `Aborted` means the frame was not successfully presented and is not a successful Legacy fallback.

A fallback outline or earlier rejection label alone MUST NOT be interpreted as final Legacy authority. Tests and diagnostics assert final authority and disposition first, then inspect candidate reasons.

### 1.3 Layer promotion is not authority

Layer promotion is not a `RetainedAuto` extension point. `RetainedAuto`, named retained canaries, and the whole-frame Legacy fallback selected by `RetainedAuto` MUST NOT use promotion candidates, promoted paint boundaries, promoted textures, or promotion metadata to decide authority or reuse.

Third-party components MUST NOT use promotion hints, scoring, or promoted-descendant hooks to claim retained support. They participate through the typed contracts in this document or remain whole-frame Legacy.

## 2. One semantic frame

The viewport supplies one engine-time sample. Animation ticks, post-layout ticks, resource preparation, property synchronization, metadata preflight, full recording, planning, and debug observation MUST describe state derived from that same sample.

Components MUST NOT read wall-clock time from paint, planning, or debug hooks. They use the supplied `crate::time::Instant`. This preserves the crate's wasm-compatible time abstraction and prevents metadata/full drift.

`ElementTrait::stable_id()` is immutable cross-frame identity for one live component. It is not current-frame topology authority. `NodeKey` and `NodeArena` own current parent/child relationships. `children()` and `sync_children_mirror()` MUST preserve exact arena order without filtering, sorting, duplication, or hidden traversal.

Every mutation MUST invalidate all dependent state. Geometry/style affects layout, placement, box model, hit test, and paint as applicable; payload/resource generation affects paint; child changes affect topology and paint; transform/clip/effect/scroll/opacity affect property and composite dependencies. A false-clean host is non-conforming.

Resource-backed components use two phases:

1. `sync_arena()` may commit deferred topology before layout;
2. `prepare_paint_resources()` freezes the post-layout request/upload snapshot.

`prepare_paint_resources()` has no arena access by design. It MUST NOT change children or invalidate completed layout. A completion that changes topology waits for the next frame's `sync_arena()`. Frozen identity includes every raster-affecting asset key, request geometry, sampling mode, device scale, format/alpha mode, generation, pixels, and retained handle identity.

## 3. Typed proof, metadata, identity, and store

Retained support is a closed proof chain, not a boolean capability flag:

1. capability probing establishes that a typed grammar may apply;
2. metadata preflight produces canonical `PaintChunkMetadata`;
3. full recording produces the complete `PaintArtifact`;
4. `PaintPayloadIdentity`, `PaintContentRevision`, owner/property snapshots, and operation order freeze raster inputs;
5. the artifact store validates the entire artifact under an engine-owned `ArtifactStoreValidationPolicy`;
6. the compiler consumes that store and creates a private typed validated token;
7. reusable surfaces carry a canonical `RetainedSurfaceRasterStamp`; multi-surface scroll scenes additionally carry one `RetainedPropertyScrollSceneTransaction` that owns the complete resident set.

Metadata/full hooks may be called independently and more than once. Given the same semantic frame they MUST be pure and bitwise deterministic. Metadata and full recording MUST agree on owner, phase, slot, role, bounds, property state, content revision, payload identity, and operation count/order.

Hooks MUST NOT mutate state, consume queues, poll a newer resource state, advance time, generate per-call identity, depend on call count/debug state/hash iteration/backend state, or retain recorder/context references.

Every proof stage validates its immediate input and relevant live snapshots. A later stage MUST NOT reconstruct missing proof from stable IDs, debug metadata, enum tags, or backend handles. Third-party code cannot mint compiler tokens, stores, stamps, resident keys, or transactions.

## 4. Property ownership and fixed composition

The engine owns transform, clip, effect, scroll, opacity, property-tree snapshots, chunk scope/phase/slot/role, owner topology, opaque order, and child traversal.

Normal order is fixed by `PaintNodePhase`:

1. self/wrapper `BeforeChildren`;
2. canonical arena children in order;
3. self/wrapper `AfterChildren`.

Chunk slots and opaque-order spans MUST be contiguous. A grammar that detaches a surface or scroll resident MUST seal the insertion marker, parent cursor before/after, property projection, clip chain, and final composite order. Reuse may skip only resident raster work; parent composites, masks, overlays, and after-children order still execute.

Custom recorders MUST NOT walk/build children, apply or bypass property clips, invent property IDs, issue backend passes, reorder children, or hide passes between children.

### Deferred viewport phase

Deferred viewport roots are a separate late phase after ordinary root paint. `is_deferred_to_root_viewport_render()` is not a general retained opt-in. Only an engine-owned grammar with typed late-phase witnesses such as `PaintDeferredViewportSelfClipWitness`, complete coverage, and fixed ordering may retain that phase.

The public custom leaf/wrapper grammar v1 rejects deferred hosts. An extension MUST NOT record a deferred subtree in the normal phase, visit it twice, or move it ahead of ordinary roots.

## 5. Preparation, emission, transaction, and reuse

Planning and compilation are graph-inert. Preparation validates the current context, device scale, target format, descriptors, geometry, resource generations, budgets, resident-key collisions, pool actions, and exact full set before frame-graph mutation.

Emission consumes only a prepared typed token. It MUST:

- emit passes in sealed order and preserve opaque-order cursors;
- consume every frozen reuse/reraster action exactly once;
- leave no undeclared resident key or unconsumed action;
- stage a multi-surface `RetainedPropertyScrollSceneTransaction` exactly once after successful emission;
- abort instead of replaying Legacy after terminal retained compile/execute failure.

Pool/stage atomicity is scene-wide. Preparation failure MUST leave the graph, pool, pending transaction, and component state unchanged. A successful transaction commits or replaces its exact active resident set together; it is never assembled from partial per-surface commits.

`Reuse` requires the complete raster stamp and descriptor pair to match. It may skip clear and raster compilation for that resident, but final composition still occurs. `Reraster` means a raster dependency changed while stable resident identity remains valid. `Commit` installs a resident set; `Clear` removes one. Dirty flags, stable IDs, texture dimensions, or a live texture alone MUST NOT authorize reuse.

## 6. Logical coordinates, device pixels, and DPR

Layout, artifact bounds, property snapshots, source bounds, and composite destinations are logical-space authority. Device scale is an explicit frozen frame input, never an ambient component value.

The engine derives physical texture dimensions, descriptor origins, and device scissors using canonical alignment/rounding. Extensions MUST NOT pre-scale selected fields or mix logical origins with physical extents. Descriptor size, origin, format, scale bits, source bounds, and pair-byte budget participate in validation and reuse identity where applicable.

DPR1 success does not prove DPR support. A grammar MUST test at least DPR1 and DPR2, including non-zero origins, clipping, descriptor drift, and unchanged-scale reuse. A scale change invalidates all device-dependent descriptors and stamps.

## 7. Fail closed

Absence of proof means Legacy, never inferred support. Selection, recording, store validation, compilation, preparation, and execution reject:

- unsupported property topology or paint grammar;
- missing, duplicate, stale, fragmented, or contradictory identity;
- non-finite geometry/color/opacity or invalid device scale;
- metadata/full drift;
- unknown child ownership or order;
- stale/unfrozen resources;
- descriptor, origin, format, clip, budget, pool-action, or transaction drift.

Rejection occurs before the rejected artifact emits. New support requires a complete typed contract and tests; validators MUST NOT be weakened to approximate support.

## 8. Debug overlay and reason codes

Debugging is observational. Enabling labels, traces, inspectors, counters, or overlay MUST NOT change authority, dirty flags, generations, resources, metadata/full payloads, topology, property snapshots, reuse/reraster decisions, application pass order, or application pixels before debug composition.

The public retained debug schema is structured:

- authority: `DebugFramePaintAuthority` and `DebugFrameDisposition`;
- location: `DebugFallbackStage`;
- category: `DebugFallbackCategory`;
- machine-readable detail: `DebugFallbackDetail`;
- reuse state: `DebugResidentAction`.

New rejection paths MUST use the narrowest stable category and deterministic lowercase code or boundary label. Codes describe invariants, not transient addresses or Rust `Debug` formatting. The unsupported custom-host path is internally `LegacyPaintReason::UnknownHost`, exposed as `DebugFallbackCategory::UnsupportedHost` with reason `unknown-host`.

Overlay labels/colors derive from the captured snapshot after selection. Consumers MUST NOT parse free-form telemetry or color to decide correctness. Earlier candidate reasons may remain visible even when the final authority is retained.

## 9. Third-party extension classes

### Native composition

Preferred. Compose built-in `Element`, `Text`, `TextArea`, `Image`, and `Svg` nodes with engine-owned styles/slots. Each native node uses its own typed contract.

### Native-like typed artifact hook

Use `ElementTrait::record_custom_leaf_paint` for a property-neutral leaf or `record_custom_wrapper_paint` for property-neutral self-paint around canonical children. Contract v1 accepts only its documented finite full-bounds fill grammar. The engine owns identities, properties, phases, traversal, store validation, and compilation.

A retained hook does not replace the complete `Renderable::build` Legacy implementation.

### Custom GPU surface

There is no public custom retained GPU-surface API in v1. Arbitrary shaders, textures, backend passes, or callbacks remain in the complete Legacy `Renderable::build` path.

A future retained GPU integration requires an engine-owned typed operation, witness, identity, artifact/store policy, compiler token, preparation/transaction contract, debug mapping, and the complete test matrix below. Until all exist, the final diagnostic category MUST be `UnsupportedHost`; a texture cache, stable ID, promotion hint, or successful Legacy build MUST NOT be presented as retained support.

Engine core remains backend-independent. `rfgui/Cargo.toml` MUST NOT add `winit`, `arboard`, `web-sys`, or another platform-facing crate. Backend code belongs in `examples/` or a downstream consumer.

## 10. Implementation checklist

- Define the exact accepted grammar and rejected topology.
- Define metadata/full parity, owner topology, property projection, phase/slot/role, and operation ordering.
- Freeze every raster/resource/device input in typed identity.
- Add store validation and a private compiler-sealed token.
- Define graph-inert planning/preparation and exact descriptor/budget/key checks.
- Define resident stamps, reuse/reraster dependencies, full-set transaction, and one-stage lifecycle.
- Add stable debug stage/category/detail mappings, including `UnsupportedHost` before support exists.
- Preserve a complete Legacy implementation and pixel/order parity fixture.
- Keep core backend-neutral and use `crate::time::Instant`.

## 11. Required test matrix

| Area | Required cases |
| --- | --- |
| Authority | admitted final non-Legacy authority; earlier candidate rejection followed by retained success; unsupported host final Legacy/`UnsupportedHost` |
| Metadata/store | repeated metadata/full equality; payload/order/topology/resource drift; malformed store rejected before emit |
| Properties/order | neutral baseline; each supported transform/clip/effect/scroll combination; unsupported interleave; before/child/after and deferred ordering |
| Atomicity | context, descriptor, budget, collision, and action-set failures leave graph/pool/stage unchanged; transaction stages once |
| Reuse | cold commit/reraster; unchanged warm reuse; every raster input rerasterizes; declared composition-only change still composites |
| DPR | DPR1/DPR2; non-zero origin; logical clip/device scissor; scale/descriptor/origin tampering |
| Debug | overlay off/on preserves authority, artifacts, actions, and application pixels; stable stage/category/detail mapping |
| Legacy parity | supported fixture matches Legacy application pixels/order; GPU/malformed fixtures retain complete Legacy output |

Minimum documentation/API verification:

```sh
cargo check -q -p rfgui
cargo doc -q -p rfgui --no-deps
git diff --check
```

Behavior changes additionally run focused tests and the library suite.

## 12. Prohibited shortcuts

- Treating a candidate rejection, debug color, dirty flag, stable ID, texture key, or promotion hint as final retained authority.
- Minting/deserializing internal witnesses, stores, stamps, resident keys, or transactions in third-party code.
- Mutating state, polling resources, advancing time, or allocating logical identity from retained recorders.
- Traversing children, applying properties, changing opaque order, or emitting backend passes from retained recorders.
- Mixing logical/device coordinates or reusing across scale/format/descriptor drift.
- Staging residents individually or falling back to Legacy after partial retained emission.
- Weakening validation for an operation without a typed compiler and failure-atomic executor.

## 13. Source and regression map

- Renderer mode and terminal breaker: [`ViewportPaintRendererMode`](src/view/viewport/mod.rs) and `RetainedAutoTerminalFailureStage`.
- Authority search/telemetry/debug mapping: [`AutoAuthorityDecision`, `AutoAuthorityTrace`, and `AutoAuthorityRejection`](src/view/viewport/render.rs).
- Public hooks, identity, dirty/resource/deferred contracts: [`ElementTrait`](src/view/base_component/element/mod.rs).
- Artifact identity and phase order: [`PaintArtifact`, `PaintChunkMetadata`, `PaintPayloadIdentity`, and `PaintNodePhase`](src/view/paint/artifact.rs).
- Recording and fallback: [`FrameArtifactFallbackReason`](src/view/paint/frame_recorder.rs) and [`LegacyPaintReason`](src/view/paint/recorder.rs).
- Store validation/compiler tokens: [`ArtifactStoreValidationPolicy`](src/view/paint/compiler.rs).
- Stamps, preparation, transactions, reuse, and DPR: [`scroll_scene.rs`](src/view/paint/scroll_scene.rs).
- Stable debug schema: [`DebugRetainedAutoSnapshot`](src/view/debug.rs).

Existing focused regression names include:

- `custom_leaf_typed_adapter_records_canonical_fill_and_compiles`
- `custom_leaf_metadata_full_drift_forces_whole_frame_fallback`
- `custom_wrapper_public_typed_phases_preserve_order_slots_and_compile`
- `custom_wrapper_topology_properties_and_unknown_child_fail_closed`
- `nested_and_multiple_deferred_viewport_roots_record_once_in_late_dfs_order`
- `nested_scroll_executor_preflight_failures_are_graph_pool_and_stage_atomic`
- `property_scene_transaction_commits_exact_multi_root_deep_forest_atomically`
- `frame_root_scroll_scene_dpr2_freezes_device_descriptors_and_emits`

See [Custom retained components](CUSTOM_RETAINED_COMPONENTS.md) for the third-party implementation guide.
