---
name: m12-element-debug
description: "Diagnose rfgui Element layout, placement, paint, hit-test, dirty-state, promotion, and reuse problems through targeted debug flags and Element debug_info output. Use whenever an Element behaves incorrectly or is difficult to inspect: first inventory and reuse the current DebugType and viewport/env debug capabilities; when none can expose the needed state, add the smallest phase-specific debug solution, verify it, and record the new flag and usage in this skill."
---

# Element Debugging

## Contract

- Treat repository code as the source of truth; confirm the current checkout before relying on this table.
- Reproduce and locate the failing phase before changing behavior.
- Reuse a current debug flag or capture API when it can expose the required state.
- Add a new debug solution only when current capabilities cannot answer the question.
- Add diagnostics at the phase authority that owns the state; avoid generic per-frame dumps.
- Keep `DebugType::empty()` free of output and extra work.
- Update this skill in the same change whenever adding, renaming, or removing an Element debug flag or reusable diagnostic.

## Source map

- `src/view/debug.rs`: public `DebugType` bitflags and debug capture/query types.
- `src/view/tags.rs`: optional RSX `debug_type` authoring prop.
- `src/view/base_component/element/mod.rs`: runtime field and cold/apply/reset prop paths.
- `src/view/base_component/element/impl_core.rs`: empty default and getter/setter.
- `src/view/viewport/input.rs`: `ViewportDebugOptions` and environment activation.
- `src/ui/use_viewport.rs`: runtime debug toggle actions.

## Current Element flags

No concrete `DebugType` flags are defined yet. The prop transport and empty runtime default exist as scaffolding.

When adding the first flag, replace this paragraph with a table containing:

| Flag | Global activation | Output site | Captured fields | Verification |
|---|---|---|---|---|
| `DebugType::...` | option/env/action | owning phase and file | minimal relevant state | focused command |

## Existing capabilities to inspect first

Search the checkout instead of assuming this list is complete:

```bash
rg -n "DebugType|debug_type|ViewportDebugOptions|RFGUI_(TRACE|DEBUG)_|trace_.*enabled|capture_debug|debug_info" src examples
```

Check these current capability groups:

- `DebugCapture` / `DebugQuery`: tree identity, layout, interaction, dirty, render, and arena snapshots.
- `trace_layout_detail`: frame-level layout timing detail.
- `trace_compile_detail` / `trace_execute_detail`: frame-graph compile and execution detail.
- `trace_reuse_path`: retained rendering and reuse decisions.
- `geometry_overlay`: geometry visualization.
- `RFGUI_TRACE_LAYOUT`: broad Element build geometry output.
- `RFGUI_TRACE_PROMOTED_BUILD`: promoted-build phase output.

Do not force an unrelated flag to carry different semantics merely to avoid adding a new one.

## Diagnosis workflow

1. Read the target Element call path and identify the owning phase: measure, place, paint/build, hit-test, dirty propagation, promotion, or reuse.
2. Inspect the current `DebugType` flags, viewport options, environment toggles, capture API, and phase-local trace helpers.
3. Select the narrowest existing capability that exposes the required state.
4. Mark only the target Element when an Element flag exists; enable its matching global trigger and collect focused output.
5. Diagnose from the output before implementing a behavior fix.
6. If current capabilities are insufficient, design and implement the smallest reusable diagnostic described below.
7. Verify both the diagnostic gating and the original Element problem.
8. Update the Current Element flags table and any affected capability notes in this skill.

## Adding a debug solution

When a new Element-scoped flag is required:

1. Add a named bit to `DebugType` in `src/view/debug.rs`.
2. Define one matching global activation rule. Reuse an existing trigger only when its semantics match; otherwise add a dedicated `ViewportDebugOptions` field, environment variable, setter/action, and UI hook as needed.
3. Gate output on both the target Element flag and the global activation rule.
4. Emit at the owning phase with a stable prefix, `stable_id`, phase name, and only state needed for that diagnosis.
5. Keep diagnostic metadata changes from marking layout, placement, paint, or composite dirty.
6. Factor predicates or formatting into testable helpers when direct stderr assertions would be brittle.
7. Test empty, matching, non-matching, and combined-bit behavior as applicable.
8. Add the flag, trigger, output site, captured fields, and focused verification command to this skill.

Prefer extending an existing phase-specific record when it already owns the needed facts. Do not build a full-tree `DebugCapture` inside a hot Element phase just to print one node.

## Architectural invariants

- Keep `rfgui` engine core independent of platform backends.
- Keep `rsx-macro` generic; never reference `Element`, `DebugType`, or another concrete component there.
- Use `crate::time::Instant::now()` for timing diagnostics; never call `std::time::Instant::now()` or `SystemTime`.

## Minimum verification

- Run the focused regression for the affected phase.
- Run tests for the debug predicate/formatter and flag combinations.
- Run `cargo check -q --lib` when public debug or Element prop APIs change.
- Run `cargo fmt --all -- --check` and `git diff --check`.
