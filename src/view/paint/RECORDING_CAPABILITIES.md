# Stage C-1 recording contract and evidence

Status (2026-09-11): C-1.1–C-1.4 recording implementation and specified
acceptance are complete, and Claude has approved this closure batch after
`300970d`. The later-stage failures listed below remain open.

The migration direction is recorded commands + complete state → generic plan
→ raster/composite → cache/reuse. Property families describe state, not
component-combination admission cases. Legacy rendering remains available
throughout migration.

C-1 closes recording capability and its evidence inventory. C-2/C-3 own
planning and production execution coverage; C-5 owns the cross-feature corpus;
C-6 owns selector convergence. A recording gate does not authorize selector
cutover, imply every combination is supported, or authorize Legacy deletion.

## Recording boundary

`record_surface_dag_frame_artifact` uses the SurfaceDag policy, supplies no
Legacy TextArea coverage authority, compares metadata with full recording,
then closes transitive property snapshots. The artifact owns ordered typed
commands, resource payloads/identity, owner topology, paint/contents endpoints,
and clip/effect/transform/layout-position/visual-offset/scroll snapshots.
The consumer needs no arena or component callback. Missing parents, wrong
owners, incompatible live packages, unfrozen resources, and metadata/full
drift reject; they are not silently replaced by neutral state.

`PaintLegacyTextAreaCoverageAuthority` remains only for the existing retained
bridge. Its exact boundary recognizers use `legacy_boundary_eq`, as already
specified by `PropertyTreeState`; recorded artifact equality remains six
dimensional. This is a semantic change in twelve preflights: whole-struct
equality becomes comparison of the four dimensions consumed by Legacy.
TextArea now supplies `layout_position` and `visual_offset` snapshots; these
were absent on the affected paths before this batch. Continuing to compare
the whole struct against the old four-dimensional expected state would reject
the newly supplied information. The change preserves the old checks on the
state Legacy consumes, while full spatial validation remains in the artifact
path. New recording does not project through the old authority or suppress
a caret into an old resident/overlay split.

## C-1 subitems

| Item | Delivery and acceptance |
| --- | --- |
| C-1.1 | Active nonempty Loading/Error children: owner order/count, inactive exclusion, independent pixels and resource lifecycle. `slot_content_tests.rs`; committed in `85586ea`. |
| C-1.2 | Image/SVG owner-scoped frozen state for Loading/Error/Ready, ancestor/self clip and effect, local placement, and Legacy owner paint scope. `resource_wrapper_state_tests.rs`, `subtree_self_clip_tests.rs`, `wrapper_effect_tests` including ancestor and Ready cases; commits `7b3702b`, `d3a9078`, `b7429b2`, `300970d`. Transparent wrappers are covered below. |
| C-1.3 | TextArea text, selection, caret and IME use generic frozen commands. TextArea and transparent projection segments capture spatial edges during placement. `generic_text_area_tests.rs` verifies phases, full contents state, internal-scroll roundtrip coordinates, and rejection of a missing frozen parent. Native `text_area_recording_tests.rs` compiles after arena drop and verifies cold/warm GPU execution alongside independent Legacy pixels. |
| C-1.4 | The executable matrix in `generic_recording_capability_tests.rs` and the inventory below distinguish recording/compiler evidence, hardware evidence and intentional fallback. This document is tracked with source, unlike the local ignored design notes. |

TextArea's internal viewport offset remains baked by its IFC layout; it is
explicitly frozen as the child coordinate-reference offset, separately from
ancestor compositor ScrollNodes. It can invalidate raster content. This does
not claim internal-scroll-only reuse or an independent caret surface.

## Native capability evidence

Paths below are relative to `tests/` unless stated otherwise. Hardware tests
must actually be run with `--ignored`; the deletion/name inventory is only a
guard against silently removing tests.

| Paint capability | Frozen recording / compiler evidence | Hardware evidence and limits |
| --- | --- | --- |
| Fill, gradient, border | `generic_recording_capability_matrix_closes_and_compiles_native_commands`, `whole_frame_tests`, rect payload validators | Existing native rect/border gates; materialization style gradient gates assert absolute red/blue coverage at DPR 1/2. |
| Outer shadow | Same generic matrix; `outer_shadow_tests` validates order, identity and malformed descriptors | `native_outer_shadow_artifact_matches_independent_anchor_oracle` is RED on both clean `300970d` and this batch: opacity 0.5 at (7,30), actual `[10,33,152,77]`, expected `[51,102,204,77]`. Recording/compiler acceptance passes; shadow RGB execution/oracle diagnosis remains C-5/C-7. Neither oracle nor renderer was changed to conceal it. |
| Inline decoration and text | Same generic matrix; `inline_span_tests`, `owning_inline_root_tests`, `owning_inline_root_atomic_tests` verify package ownership and phased order | Native nested IFC text gate exists. Exhaustive decorated-span pixel/transform/clip combinations remain C-5 coverage, not proven by name counts. |
| Image and SVG, including inline atomic payloads | Same generic matrix, prepared image/SVG and owning-inline tests; frozen uploads and generations | Ready owner-scope DPR 1/2 gates, native image pixel oracles, SVG expected-color/fit gates; resource completion/freeze/pressure gates. |
| Loading/Error nonempty and empty wrappers | `generic_recording_transparent_resource_slots_preserve_only_active_children`, wrapper state and slot tests | Nonempty slots and effects have independent Legacy/Artifact pixel gates. Empty owner chunks are canonical and contain no commands; they do not require an invented paint op. |
| Contents/self clip, descendant masks | `subtree_self_clip_tests`, `child_mask_and_self_decoration_tests`; complete snapshot closure | Ancestor/Ready scope gates include geometrically checked outside-clip probes. Arbitrary transformed clips are a C-2/C-5 coverage obligation. |
| Deferred viewport overlay | `generic_recording_deferred_phase_and_scrollbar_overlay_keep_canonical_order`: deferred child deliberately precedes normal child in arena order but paints last; frozen output compiles | Deferred cross-feature pixel combinations remain named C-5 coverage. Only validated exact deferred scopes are recordable. |
| Scrollbar overlay | Same generic phase test requires a real terminal `PreparedScrollbarOverlay`, outside content clip; wrong owner/stable ID/policy/snapshot rejects without manufacturing a Legacy witness | `native_generic_and_legacy_frozen_scrollbar_absolute_alpha` checks frozen generic and Legacy output independently at DPR 1/2, with track alpha derived from composition and content/clip probes. The older scroll-scene suites stop at their fractional Hidden case; their later visible cases did not execute in this run. |
| TextArea glyphs, selection, caret, IME and projection | `generic_text_area_tests`, existing plain/projection selection/preedit negative tests; frozen spatial-chain closure | `native_generic_text_area_frozen_ime_caret_and_reuse` and `native_legacy_text_area_frozen_ime_caret_pixels`: empty caret, selection over spaces, plain/projected IME, DPR 1/2, arena dropped before execution. Glyph font raster shapes are not an absolute bitmap oracle. |

## Intentional fail-closed boundaries

- Arbitrary custom GPU passes/backend handles/callbacks remain whole-frame
  Legacy. Typed custom leaf/wrapper adapters have their own command/ownership
  contracts; they cannot forge native layout or property authority.
- Missing or stale IFC/projection packages, wrong owner/source ranges,
  unfrozen image/SVG resources, malformed clip chains, and unsupported deferred
  scopes remain rejection cases. They must not be admitted by a family label.
- SVG's first post-layout raster acquisition can require the explicitly named
  `MissingPreparedSvg` fallback until the next resource freeze.
- Existing overflow-late ordering and IFC-owned paint boundaries must be
  proven before adding more scopes. Generic arena order cannot substitute for
  a different Legacy phase order.
- Legacy's nonempty transform + opacity group behavior changed in `b7429b2`.
  `ancestor_slot_tests` covers the actual combination: style-derived translation,
  opacity 0.5/0.25, and nonempty descendants, for both renderers at DPR 1/2.
  Ready leaf or transform-only gates are not its evidence. Arbitrary rotation/
  scale and inline/deferred transformed clips remain C-2/C-5/C-7 obligations.
- At the C-1 checkpoint, production selection still excluded outer-scroll
  TextArea. The C-1 hardware tests call the common generic executor and do not
  themselves prove selector takeover. The subsequent C-3 general-primary
  integration and single-Viewport TextArea evidence, including its remaining
  fallback limits, are tracked in `EXECUTION_INTEGRATION.md`.

Keep these limits visible when reporting C-1 completion. They remain work in
the named later stages; neither a green recording inventory nor a localized
hardware gate constitutes complete RetainMode acceptance.

## Baseline failures retained for C-5/C-7

The independent shadow gate above and the two gates below fail identically in
an isolated archive of `300970d` and this working tree. No test, expected pixel,
tolerance, or deletion-inventory entry was removed or relaxed:

- `native_scroll_scene_single_backing_pixels_match_and_reuse`: fractional
  Hidden frame, 48 pixels, maximum channel delta 47, bounds `[8,27,55,27]`.
- `native_scroll_scene_tiled_cross_tile_pixels_match_and_reuse`: fractional
  Hidden frame, 48 pixels, maximum channel delta 47, bounds `[8,31,55,31]`.

These compare an older retained bridge with Legacy and are not passing
full-corpus evidence. C-1.4 closes the honest capability/evidence inventory;
it does not close these rendering/oracle discrepancies. The new direct
scrollbar gate has its own absolute alpha/content/clip expectations.

## Validation of this batch

- Full CPU library suite: **2,110 passed, 0 failed, 90 ignored**;
  `/tmp/retain-c1-final-lib.log`. Nine CPU tests and three ignored native
  tests were added relative to `300970d`.
- Explicit Apple M5 Metal acceptance: **51 passed, 0 failed**;
  `/tmp/retain-c1-acceptance-metal.log`. This includes all 44 materialization
  gates, three image gates, two SVG gates, one nested IFC gate and the old
  focused projection scroll-forest gate. The last now uses the existing
  thread-cache cleanup guard so TLS destruction cannot abort the test runner.
- The three baseline failures above were also executed, separately from the
  passing acceptance set. Clean-baseline logs:
  `/tmp/retain-c1-baseline-shadow.log`, `/tmp/retain-c1-baseline-scroll.log`.
  Current logs: `/tmp/retain-c1-current-shadow.log`,
  `/tmp/retain-c1-final-metal.log` (51 passed, 2 scroll-scene failures).
- Production `cargo check --lib` passes; `/tmp/retain-c1-check.log`.
  Native tests use `--ignored --nocapture` and the isolated target directory
  `/tmp/rfgui-retain-c12-target`; the default CPU ignored count is not GPU
  evidence. Existing dead-code/future-compatibility warnings remain.
- The legacy deletion ledger's file denominator grows by 40 lines: 36 from
  formatting twelve existing comparisons and four explanatory comment lines.
  This explains the line-count delta only; the comparison's semantic change
  and coexistence rationale are described in "Recording boundary" above.
  Its exact counts were updated, with no new legacy bridge item/authority.
  The native deletion inventory includes all three new gates; it is only a
  deletion guard, not proof of execution.
