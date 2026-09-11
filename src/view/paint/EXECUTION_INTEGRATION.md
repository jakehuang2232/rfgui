# Stage C-3: production selection and Viewport execution

Checkpoint: 2026-09-11, based on `91c2a32`, approved by Claude review.
The direction remains recorded commands + complete state -> generic plan ->
raster/composite -> cache/reuse. Legacy remains independently usable throughout
migration and is not deleted before complete RetainMode acceptance.

## Progress and acceptance boundary

| Item | This checkpoint |
| --- | --- |
| C-3.1: general production selection | Valid complete recordings get the first generic plan/seal attempt, without property-family or topology admission. Final rejection/fallback convergence remains C-6. |
| C-3.2: single-Viewport corpus | Nine C-2 scenes plus four outer-scroll TextArea cases execute through production selection, layout, paint and submit; both renderer modes have independent pixel expectations. |
| C-3.3: full-window budget | Pending. The provisional 128 MiB aggregate budget is not a proven whole-mode memory ceiling. Full-window descriptors, rejection side effects and fallback policy still need acceptance. |
| C-3.4: multi-target atomic failure | Pending. Successful branching/multi-root frames do not prove preparation/execution failure rollback or complete recovery. C-4 single-target failures do not close this item. |

C-3 is **partially complete**. This checkpoint does not count selector cleanup
or deletion of older authorities as completed work.

## Production change

`select_retained_auto_authority_with_semantics` first records the full frame
with `record_surface_dag_frame_artifact` and applies the common raster plan and
seal. The `General` requirement permits both zero-resident and supported
detached plans. Successful selection carries `ArtifactSurface` into the
existing generic pool executor. It does not depend on counts of transforms,
effects or scrolls, TextArea membership, branching, or a named property
combination. The existing role, snapshot, geometry and resource validators
remain in force. No authority variant or executor was added.

After a rejected general attempt, the extracted
`select_retained_auto_compatibility_authority_with_semantics` still runs the
previous cascade with its accumulated rejection trace. Thus some rejected
inputs can still select older retained planners or `ExistingArtifact`.
**This is not yet an artifact-or-whole-frame-Legacy-only selector.** In
particular, a generic aggregate-budget rejection does not prove that every
compatibility path obeys that same aggregate budget. C-3.3 must define that
policy; C-6 owns final convergence. Unknown custom paint and malformed
snapshot/deferred witnesses retain explicit primary-selector rejection tests.

### C-6 acceptance: remove duplicate selection work

While the compatibility cascade remains, every invocation that reaches it
has already paid for a rejected generic attempt. Rejection can happen during
recording or planning, before sealing; this does not mean every rejected
frame completes all three stages. The extra work is real but unmeasured in
this batch. C-6 acceptance must compare selection attempt counts and CPU cost
before/after convergence for accepted frames, early recording rejections and
late planning/sealing rejections, including cold and unchanged warm frames.
Report the generic acceptance/fallback rate of the measured corpus alongside
timings, and verify that no second compatibility preparation remains after
convergence. Removing the cascade removes duplicate attempts, not the cost
of the initial generic attempt or necessary whole-frame Legacy rendering.

## Evidence through real frames

The planning corpus's `viewport_tests.rs` starts with unlaid-out styles and
installs the complete root forest into one Viewport. That Viewport performs
the first layout, recording, selection, execution, submission and subsequent
warm frames. Nine scenes include scale, quarter/oblique rotation, negative
origin, deep heterogeneous roots, named anchors and clip/scroll scope cases.
Two DPRs x three frames x nine scenes x two renderer modes = **108 frames**.
The older C-2 fixture retains its separate layout/common-executor contract.

The TextArea `viewport_tests.rs` adds an explicit outer scroll container to
empty caret, selection over spaces, plain IME and projected IME fixtures.
Two DPRs x three frames x four cases x two modes = **48 frames**. Setup uses
the existing navigation/layout and interaction fixture; each rendered frame
then performs production layout and paint in the installed Viewport. Because
outer scrolling can change projected line wrapping, caret probe coordinates
come from that frame's layout/navigation geometry. Colors and antialiasing
coverage use the existing mathematical expectation, not readback or recorded
paint ops. This is not an independent oracle for navigation correctness or
OS IME event delivery, and does not test RSX reconciliation.
If navigation and painting share the same coordinate error, the probe can
follow that error and still pass; independent colors do not make the probe
an independent position oracle.

Both modes compare submitted pixels against the same geometry-derived
expectations; Legacy is never the Artifact pixel oracle. Pixels are checked
before reuse/resource accounting. RetainedAuto must select Artifact and emit
nonempty generic actions: all cold targets Reraster, both unchanged warm
frames Reuse. Every target must have an actual compatible persistent GPU
pair and retain its logical identity/descriptor across frames. The planning
corpus additionally reconciles reported bytes with every descriptor's
`width * height * 12` (RGBA8 plus Depth32FloatStencil8). This reconciliation
is accounting evidence, not an independent expected envelope-size oracle.

The shared frame harness calls production `render_render_tree`; only window
surface acquisition is replaced by offscreen acquisition. It requires one
completed frame, no unexpected fallback or terminal failure, and returns the
submitted texture. Each frame sets DPR after offscreen initialization and
asserts logical dimensions, since initialization resets scale. Legacy mode
must explicitly report Legacy authority.

The new corpus uses viewport paint offset zero. C-2's common-executor corpus
separately covers nonzero host paint offsets; this batch does not prove
arbitrary host embedding through the production entry. Warm frames here are
unchanged; C-4 owns content, placement, opacity and resource-change sequences.

## Preserved compatibility protection

Older bridge-specific selection, payload tamper, preflight and transaction
tests now explicitly call the compatibility entry where their assertions
require those older payloads. Their assertions remain; they are not evidence
that valid frames still select a bridge through the new primary entry.
Generic primary selection/emission is separately checked for representative
scroll/forest scenes, heterogeneous roots, native root-effect hosts and
Window-like/deferred fixtures. Invalid snapshot generations and deferred
witnesses are tested against the primary entry too. The zero-surface test
that previously required root opacity to use `ExistingArtifact` now requires
the common generic executor, while the old root-effect compiler retains its
own compatibility assertions.

No native gate or deletion-inventory entry was removed. Four new native gates
are included in the inventory; their presence is not execution evidence.

## Verification and unresolved baseline

- Full CPU library: **2,116 passed, 0 failed, 96 ignored**;
  `/tmp/retain-c3-final-lib.log`. Two new CPU tests and four native gates.
- Apple M5 Metal acceptance: **66 passed, 0 failed**;
  `/tmp/retain-c3-final-metal.log`: 50 materialization, nine transform and
  seven additional C-1 prepared-resource/IFC/projection gates. The four new
  gates execute the **156 frames** above.
- Production library check passes; `/tmp/retain-c3-check.log`.
- Separately rerun baseline gates remain **three failures**:
  `/tmp/retain-c3-baseline-gates.log`. Outer shadow still produces
  `[10,33,152,77]` instead of `[51,102,204,77]` at `(7,30)`; the single/tiled
  fractional-scroll bridge gates still differ at 48 pixels, maximum delta 47,
  bounds `[8,27,55,27]` / `[8,31,55,31]`. None is reported fixed or passing.
- Legacy negative-source content cropping remains unresolved, as documented
  in `PLANNING_CAPABILITIES.md`. Passing uniform-fill probes here cannot close
  that limitation. Full cross-feature acceptance remains C-5/C-7.

All checks use `/tmp/rfgui-retain-c12-target`. Existing dead-code and dependency
future-compatibility warnings remain. Logs are local execution evidence; the
contracts and tests above are tracked with the source.
