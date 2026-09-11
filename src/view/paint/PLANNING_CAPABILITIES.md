# Stage C-2: generic planning and materialization

This is the planning acceptance contract, following
[C-1 recording](RECORDING_CAPABILITIES.md). Recorded commands and complete
property/resource state feed one planner, raster/composite executor and reuse
pool. Property combinations describe fixtures, never admission routes or
completion milestones. Legacy stays usable throughout migration.

## C-2.1: complete-recording corpus

`tests/gpu_equivalence_tests/native_artifact_surface_materialization_tests/`
`planning_corpus_tests.rs` defines nine scenes using real style and layout:

- scale, 90-degree rotation, and 45-degree rotation;
- a negative raster origin;
- four nested scale boundaries, a co-located opacity group, interleaved
  painted siblings with an intermediate surface, and a second gradient root;
- named-anchor positioning;
- nested Intersect/Replace clips with overflow-late paint;
- the same escape with its own scale/opacity surface;
- that escape with a nonzero inner scroll offset.

The CPU contract records through the production recorder, then drops the
arena. It runs both DPRs and reverses descendant owner/property registries.
Parentless owner order remains fixed: it explicitly defines scene-root paint
order. Candidates follow recorded command cursors; equal cursors preserve
registry/local boundary order and are not topological ordinals.

Recursive plan flattening checks every chunk identity and source op interval,
the complete ordered op-index sequence, exactly one visit per surface, actual
receiver links, and the strict execution seal. This complements the typed-op
payload and native-host coverage in C-1 rather than replacing that corpus
with nine fill scenes. Reversal does not change the source commands.

## C-2.2: retained geometry and clip scopes

Three production defects found by this corpus are fixed in common code:

1. `owner_viewport_transform` contains only its owner's authored transform.
   Inverting a receiver's matrix cancelled an ancestor that had not yet been
   applied. Every retained boundary now applies its own matrix once; receiver
   closure validation remains in place independently of that calculation.
2. Reprojecting normalized raster coordinates could round a rotated quad's
   dimensions differently and panic after sealing. Preparation now freezes
   the original projected corners relative to their AABB. Placement translates
   its origin without recalculating extents. Sealing and execution validate
   the same frozen quad, with exact extent equality and no added epsilon.
3. Ancestry alone captured overflow-late commands inside an already closed
   scroll mask. Frozen mask begin/end scopes now constrain coverage,
   transition witnesses and receiver selection together. A closed scroll
   boundary can be skipped while its owner's other boundaries remain eligible.
   Missing, duplicated or crossed recorded scroll-mask scopes fail closed with
   `InvalidScrollMaskScope`; artifacts without such mask commands retain the
   existing property-state membership contract.

Hardware gates render nine scenes x two DPRs x two paint offsets x two frames
x two renderers: **144 frames**. Both renderers use the same absolute geometric
color/alpha expectations (1 LSB), independently of each other's readback.
The 45-degree case includes clear probes inside the AABB but outside the quad.
The scrolled escape moves eight logical pixels, multiplied by its parent scale
to ten output pixels; probes lie beyond the scale filters' edge footprint.

Artifact frames use the common real-pool emitter after dropping the arena.
Pixels are checked before actions: every cold target must Reraster and every
unchanged warm target must Reuse. Scale/rotation preserve the 20 x 16 local
raster at DPR 1/2; geometry changes occur at composition. Negative-origin and
paint-offset cases also pass the same strict seal. Unit tests reject nonfinite
or inconsistent frozen quads and preserve original-coordinate projection after
raster-origin normalization.

## C-2.3: materialization policy

The existing obligation-based policy stays conservative. Own raster content,
isolation, non-translation, multiple nested targets, unproven clip transfer and
uncomposed adjacent boundaries keep their specific retention reasons. This
batch does not add adjacent elimination or a property-family shortcut. No
resource objective requires those optional optimizations for C-2 acceptance.
Existing materialization reason/transfer/snapshot-order and absolute reuse/
resource gates remain part of the executed regression suite.

## Completion boundary

C-2 covers the common planner and executor contract for the recorded corpus.
The new harness uses a layout Viewport and a separate offscreen render/pool
Viewport, and explicitly selects the generic recorder/executor. It does not
prove production selector takeover or single-Viewport integration of this
whole corpus (C-3/C-6), aggregate full-window budgets, multi-target failure
recovery, perspective/3D raster quality, or arbitrary transformed IFC/deferred
scopes. Those are not closed by successful Reraster/Reuse accounting here.

No legacy path, native gate, pixel expectation or tolerance was removed.
The two new native gates are in the deletion inventory, which guards their
presence and is not execution evidence. The three known C-1 shadow/older
scroll-bridge failures stay named in `RECORDING_CAPABILITIES.md`; they are not
reported as fixed by C-2.

## Validation and status (2026-09-11)

C-2.1/C-2.2 implementation and specified acceptance are complete; C-2.3 keeps
the existing conservative policy. Claude has approved this batch,
based on `ca4e87e`; the negative-origin clarification below is part of its
review closeout.

- Full CPU library: **2,114 passed, 0 failed, 92 ignored**;
  `/tmp/retain-c2-final-lib.log`. Four CPU and two ignored native tests were
  added; ignored tests are explicitly executed below.
- Apple M5 Metal acceptance: **62 passed, 0 failed**;
  `/tmp/retain-c2-final-metal.log`. This includes all 46 materialization gates,
  nine existing transform gates and the seven additional C-1 acceptance gates
  for prepared Image/SVG, nested IFC and the projection scroll forest.
- The two new native gates execute all **144 frames** described above.
- All three known baseline gates were separately executed and still fail
  with the same signatures recorded in C-1: shadow `[10,33,152,77]` instead of
  `[51,102,204,77]`, and the two fractional-scroll cases each with 48 differing
  pixels / maximum delta 47. `/tmp/retain-c2-baseline-gates.log` records these
  failures. They are outside the 62 passing acceptance gates, not omitted
  from the overall state or described as passing.
- Production `cargo check --lib` passes; `/tmp/retain-c2-check.log`.
  The isolated target is `/tmp/rfgui-retain-c12-target`. Existing dead-code
  and dependency future-compatibility warnings remain.


## Review closeout: Legacy negative-origin content remains unresolved

The Legacy `NegativeOrigin` scene passed all eight frames (two DPRs x two
paint offsets x cold/warm), both in the final acceptance log and the focused
review rerun. This closes the named **visible uniform-fill/output-extent**
acceptance, not preservation of arbitrary negative-source content.

A temporary allocation trace in that same hardware gate confirmed that Legacy
still allocates **12 x 16** at DPR 1 and **24 x 32** at DPR 2, with color origins
`(0,20)` / `(0,40)`, at both paint offsets. The full source `[-8,20,20,16]`
requires widths 20 / 40 respectively. The trace and passing pixels are together
in `/tmp/retain-c2-negative-origin-review.log`; the trace instrumentation was
removed after the observation, with the test behavior unchanged.

The production chain remains `build_legacy_layer_subtree` -> raw
`legacy_transform_surface_bounds` -> `texture_desc_for_logical_bounds`, whose
origin is clamped to zero. Composite UVs retain the original bounds and use a
`ClampToEdge` sampler (`texture_composite_pass.rs`). Consequently, uniform red
can extend across missing negative-source texels while the destination quad
and right edge remain correct. The far/right probes reject a shortened output
extent, but do not reject this existing source crop. The `b7429b2` group-opacity
rewrite did not close that clamp path.

The interior probe is now named `visible uniform-fill interior`; neighboring
probes explicitly name the right extent. Their coordinates, colors and
1 LSB tolerance are unchanged. A nonuniform source with distinct content in
negative coordinates is needed to validate content preservation after a
Legacy descriptor/UV repair. That existing Legacy limitation remains named
for C-5/C-7; this is neither a C-2 regression nor a reason to delete Legacy
before full RetainMode acceptance. Generic normalized target sizes remain
20 x 16 / 40 x 32 in the C-2 CPU contract.
