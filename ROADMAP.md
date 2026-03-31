# ROADMAP

## Goals
- Establish typed-only style and layout capabilities and avoid string-based parsing paths.
- Keep the RSX authoring experience consistent while preserving stable interactive state such as hover, scroll, and node identity.
- Move forward through verifiable milestones, with tests and examples for every item.

## Milestone M1 (Near Term)

### 1. Add `:focus` State Styling
**Scope**
- Add typed focus-state fields to parsed style.
- Merge focus-state rules into computed style.
- Support redraw on focus-state transitions in element sync and renderer.
- Allow focus-related style declarations through RSX/schema.

**Execution Steps**
1. Add a focus variant to `parsed_style` without introducing string-key lookups.
2. Add focus composition rules to `computed_style` while keeping struct-based fields.
3. Hook focus/blur into the event flow and trigger node redraws.
4. Apply focus visuals in the renderer, such as outline or border changes.
5. Add tests and examples for keyboard navigation and click-driven focus changes.

**Acceptance Criteria**
- The target node updates its visuals consistently after focus/blur.
- Focus changes do not rebuild the whole tree or cause node id drift.
- Tests cover focus switching and redraw paths.

### 2. Rename `Display` to `Layout`
**Scope**
- Unify naming across types, fields, RSX schema, docs, and examples.
- Keep this as a semantic rename only, with no behavior changes.

**Execution Steps**
1. Inventory all public APIs, enums, types, and fields that currently use `Display`.
2. Rename them to `Layout` with a compatibility migration path, keeping aliases temporarily if needed.
3. Update rsx-macro/schema diagnostics and documentation wording.
4. Update matching examples and README snippets.

**Acceptance Criteria**
- The project builds and existing tests pass.
- `Layout` becomes the primary name; any retained old names are marked deprecated.
- No layout behavior regressions are introduced.

## Milestone M2 (Next)

### 3. Refactor `Position::Absolute` into `Placement`
**Goal**
- Use a single `Placement` model to support both `Edges` (CSS-style) and `Align` (UI anchor-style) positioning.
- Keep the system typed-only and preserve the rule that `%` only resolves against definite container sizes.

**Condensed Design Draft**
```rust
#[derive(Clone, Debug)]
pub struct AbsSpec {
    pub anchor: Option<AnchorName>,
    pub placement: AbsPlacement,
}

#[derive(Clone, Debug)]
pub enum AbsPlacement {
    Edges(AbsEdges),
    Align(AbsAlign),
}

#[derive(Clone, Debug, Default)]
pub struct AbsEdges {
    pub top: Option<Length>,
    pub right: Option<Length>,
    pub bottom: Option<Length>,
    pub left: Option<Length>,
}

#[derive(Clone, Debug, Default)]
pub struct AbsAlign {
    pub origin: Axis2<Length>,
    pub offset: Axis2<Length>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Axis2<T> {
    pub x: Option<T>,
    pub y: Option<T>,
}
```

**Execution Steps**
1. Add typed `Placement` fields and parsing entry points in `parsed_style`.
2. Add matching `AbsSpec/AbsPlacement` fields in `computed_style`.
3. Bind RSX props to the new types through schema/macro and remove the old absolute API path.
4. Sync placement state into element layout data.
5. Implement `Edges` and `Align` positioning in the renderer/layout solver.
6. Add tests and examples covering `%`, anchors, and wrapping interactions.

**Acceptance Criteria**
- `Edges` supports arbitrary combinations of top/right/bottom/left.
- `Align` supports expected positions using `Length::percent(0/50/100)` and `offset`.
- `%` does not back-propagate into parent measurement when the container size is indefinite.
- The old absolute API has a migration path and deprecation strategy.

### 4. Reorganize `base_element` Trait Boundaries
**Goal**
- Audit and reduce responsibility overlap between `ElementTrait`, `Layoutable`, `EventTarget`, and `Renderable`.
- Reduce reliance on `as_any + downcast::<Element>` and move toward capability-oriented interfaces.
- This stage focuses on design and migration planning, not a large behavior rewrite.

**Scope**
- Produce a capability matrix for containers, text nodes, and editable components.
- Define a minimal core node interface plus optional capability traits for layout, events, rendering, transitions, and snapshots.
- Audit downcast-heavy paths in `mod.rs` and `element.rs` and propose replacement APIs.

**Execution Steps**
1. Produce an audit document of trait method usage, call sites, and per-component overrides.
2. Draft a "core interface + capability traits" design without changing behavior yet.
3. List downcast call sites and proposed replacements, such as transition, hit-test, and deferred render APIs.
4. Plan a staged migration order and compatibility strategy to avoid a big-bang rewrite.
5. Add a regression test list and risk controls.

**Acceptance Criteria**
- A concrete migration checklist exists with files, methods, and order.
- Downcast-removal candidates and replacement APIs are clearly identified.
- The design keeps the typed-only rule and does not reintroduce string style paths.

### 5. Let Layout Fully Replace Bounds
**Goal**
- Make layout results the single source of truth for geometry and remove the separate `bounds` concept and sync cost.
- Use `layout_position/layout_size/layout_inner_*` consistently for hit-testing, clipping, visibility, and scroll ranges.

**Scope**
- Event hit-testing and bubbling local coordinate calculation.
- Render clipping and visibility evaluation.
- Scroll bounds and content size derivation.

**Execution Steps**
1. Audit all `bounds` reads/writes and map them to equivalent layout fields.
2. Introduce a layout geometry access API for outer/inner/content/clip geometry.
3. Switch hit-testing and local event coordinates to layout geometry first.
4. Then switch render clip/visibility and scroll bounds to layout geometry.
5. Remove remaining `bounds` fields and sync logic and document the migration.

**Acceptance Criteria**
- `bounds` is no longer a geometry source.
- Hover, scroll, and focus behavior remain stable without node id drift.
- Tests pass for `%`, absolute clip, and scroll bubbling paths.

## Milestone M3 (Viewport Cleanup)

### 6. Audit Current Viewport Architecture (layout / renderer / style / rsx schema)
**Goal**
- Clarify viewport responsibilities and data flow across `layout -> renderer -> style -> rsx schema`.
- Establish a baseline document for later performance and maintainability work.

**Current Summary**
1. `Viewport` drives the frame lifecycle: `measure -> place -> collect_box_models -> build graph -> compile -> execute`.
2. Transitions can trigger an extra `place + collect_box_models` after layout when needed.
3. `Element::measure` already has `layout_dirty + last_layout_proposal` caching, while `place` still runs fully.
4. `%/vw/vh` are resolved via typed `Length::resolve_with_base`, and `%` does not back-propagate when the base is indefinite.
5. Absolute `ClipMode::Viewport` and `CollisionBoundary::Viewport` are resolved during place using the runtime viewport size.
6. The RSX macro uses `ElementStylePropSchema` for compile-time style key validation.

**Outputs**
- A maintained architecture map, even if text-only.
- A hot-path inventory covering layout, build graph, hit-testing, and transitions.

### 7. Consolidate Viewport Problem List (Performance / Maintainability / Readability)
**Performance Issues**
1. The frame graph is rebuilt and recompiled every frame, which is CPU-expensive.
2. Post-layout transitions can trigger a second `place + collect_box_models`, amplifying animation cost.
3. `overflow_child_indices.contains(&idx)` is used in a loop and can cause O(n^2) behavior.

**Maintainability Issues**
1. `PLACEMENT_RUNTIME` uses thread-local implicit state, making data flow harder to reason about.
2. `ElementPropSchema` and renderer paths still carry legacy visual props such as `padding_x`, which conflict with the typed-style single path.
3. `TextAreaPropSchema` still contains `String/f64` geometry and color fields instead of typed style fields.

**Readability Issues**
1. `viewport.rs` and `element.rs` still mix layout, render, input, and transition responsibilities and remain too large.
2. `set_size` and `set_scale_factor` do not explicitly request redraw, relying on callers to remember to do so.

### 8. Viewport Improvement Plan (Without Risk Controls)
**Short Term**
1. Replace `overflow_child_indices` with `Vec<bool>` or `HashSet<usize>` to remove the O(n^2) path.
2. Split transitions by effect type so only geometry changes trigger relayout, while pure visual changes avoid a second place pass.
3. Introduce frame graph reuse so static pass structure can be cached and only dynamic parameters need updating.

**Mid Term**
1. Split `viewport` into `input_dispatch.rs`, `render_pipeline.rs`, and `transition_runtime.rs`.
2. Split `element` into `measure.rs`, `place.rs`, `clip_hit_test.rs`, and `paint.rs`.
3. Replace `PLACEMENT_RUNTIME` with an explicit `PlacementContext`.

**Mid Term (Schema / API Convergence)**
1. Remove legacy visual fields from `ElementPropSchema` and route everything through `style`.
2. Move `TextArea` geometry and color to typed style (`Length` / `ColorLike`).
3. Keep RSX macro compile-time schema validation aligned with the typed-only rules in AGENTS.

## Milestone M4 (Promoted Layer Performance Convergence)

### 9. Reduce Fixed Stencil Cost in Promoted Composition
**Goal**
- Now that promoted layer reuse is correct, reduce the fixed per-frame `StencilIncrement/StencilDecrement` and clip-scope pass cost.

**Execution Steps**
1. Make composition-stage clip decisions depend only on descendants that are actually composited, not all children.
2. Evaluate whether child clip scopes can be moved further into the promoted final layer cache instead of being rebuilt on the parent target every frame.
3. Add pass / draw-call tracing that separates clip stencil, text, and shadow/blur fixed costs.

**Acceptance Criteria**
- `StencilIncrement/StencilDecrement` counts drop further while reuse remains enabled.
- `actual_reuse` does not regress and rendering correctness stays intact.

### 10. Reduce Intermediate Targets and Pass Fragmentation in Shadow / Blur
**Goal**
- Reduce the current `different_target` split count by simplifying intermediate target switching and ping-pong behavior in shadow / blur modules.

**Execution Steps**
1. Audit target flow in `ShadowFillPass` and `BlurStagePass`.
2. Merge reusable intermediate targets where possible and reduce unnecessary switching between `Fixed(1)` and `SurfaceDefault`.
3. Evaluate whether blur passes can avoid extra clear operations and target bouncing.

**Acceptance Criteria**
- `execute(passes=...)` drops further.
- Metal captures show materially fewer `different_target` split cases.

### 11. Clean Up Promotion / Reuse Debugging and Warnings
**Goal**
- Keep the necessary debugging tools while reducing long-term noise from temporary trace code and warnings.

**Execution Steps**
1. Keep `Debug Reuse Path` and the overlay, but narrow deep style/promotion traces so they are only enabled when explicitly needed.
2. Remove or refactor currently unused debug fields in `promotion_builder.rs`.
3. Document the purpose and limitations of the current debug overlays and traces.

**Acceptance Criteria**
- `cargo check` no longer reports the known promotion debug warnings.
- Normal runs do not emit unnecessary debug logs.

## Cross-Milestone Quality Bar
- Every capability must include a minimal reproducible example.
- Regression test priority remains: `%` resolution, stable scroll state, hover/focus redraw, and consistent border-radius clipping.
- Only typed APIs are allowed (`Length`, `Border`, `BorderRadius`, `ColorLike`); string-based style pipelines remain forbidden.
