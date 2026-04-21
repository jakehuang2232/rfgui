---
name: m11-rfgui-components-lib
description: Authoring rules for `lib/rfgui-components` (Button/IconButton/ToggleButton/…). MUI-aligned API, theme integration, compound components via context, enum-as-string props, color/size/variant patterns, border-merge for grouped buttons. Use whenever creating or extending anything under `lib/rfgui-components/src/`.
---

# rfgui-components lib authoring

## Layout

- `inputs/` — form controls (Button, Checkbox, Select, Slider, Switch, NumberField, IconButton, ToggleButton, ToggleButtonGroup, …)
- `layout/` — containers (Window, Accordion, TreeView, …)
- `icons/` — Material Symbols glyphs via `MaterialSymbolIcon`
- `theme.rs` — Theme struct, `use_theme()` hook
- `lib.rs` — `pub use inputs::*; pub use layout::*; pub use icons::*; pub use theme::*;`

One component per file. mod.rs just wires `mod X; pub use X::*;`.

## Component boilerplate

```rust
pub struct Foo;

#[derive(Clone)]
#[props]
pub struct FooProps {
    pub required_thing: String,        // required
    pub optional_thing: Option<Bar>,   // optional
}

impl RsxComponent<FooProps> for Foo {
    fn render(props: FooProps, children: Vec<RsxNode>) -> RsxNode {
        rsx! {
            <FooView ...>{children}</FooView>
        }
    }
}

#[rfgui::ui::component]          // emits ComponentVTable for lazy pipeline
impl rfgui::ui::RsxTag for Foo {
    type Props = __FooPropsInit;
    type StrictProps = FooProps;
    const ACCEPTS_CHILDREN: bool = true;

    fn into_strict(p: Self::Props) -> Self::StrictProps { p.into() }
    fn create_node(props, children, _key) -> RsxNode {
        <Self as RsxComponent<FooProps>>::render(props, children)
    }
}

#[component]
fn FooView(required_thing: String, optional_thing: Option<Bar>, children: Vec<RsxNode>) -> RsxNode {
    let theme = use_theme().0;
    // resolve Option defaults here
    rsx! { <Element ...>{children}</Element> }
}
```

Rules:
- `#[derive(Clone)]` on Props — required by vtable `clone_props` shim
- `#[rfgui::ui::component]` on `impl RsxTag` block (not `#[component]`) — emits vtable
- `ACCEPTS_CHILDREN: true` if children used; macro checks at call site
- Render body = thin; real work in inner `#[component] fn FooView`
- Option resolution lives in View, not in props struct

## Build with base components only

- Use `Element`, `Text`, `Image`, `Svg` from `rfgui::view`
- Do NOT add new low-level primitives
- Compose styles via `style={{ ... }}`

## Enum-like string props

For `variant="contained"` / `size="small"` / `color="primary"` / `orientation="horizontal"`:

```rust
impl From<&str> for FooVariant { ... match strings panic unknown ... }
impl From<String> for FooVariant { from &str }

impl rfgui::ui::IntoOptionalProp<FooVariant> for &str {
    fn into_optional_prop(self) -> Option<FooVariant> { Some(self.into()) }
}
impl rfgui::ui::IntoOptionalProp<FooVariant> for String { ... }
```

Without both impls, string literal prop = compile error.

## MUI alignment

Follow MUI API when possible. Diverge only when Rust types force it.

### Patent / license hygiene

- **MUI Core + MUI X community** (Button, Accordion, `SimpleTreeView`, `TreeItem`, etc.) are MIT. Mirroring API shape + semantic names is fine. Don't paste source verbatim — re-implement from the documented API surface.
- **MUI X Pro / Premium** (virtualization, drag-reorder, tri-state tree checkboxes, rich tree headless API, data-grid server-side features, pickers Pro) are commercial. Do **not** mirror Pro-tier API surface or feature set. If a similar capability is needed, design the API independently so it doesn't track the Pro signature.
- **Naming** — use generic / rfgui-native names, not MUI trademarks. `TreeView` (not `SimpleTreeView` / `RichTreeView`), `TreeNode.value` (not `itemId`), `Select` / `Slider` / `Accordion` (fine — generic terms). When in doubt prefer the HTML/ARIA role name.
- **Icon assets** — Material Symbols is Apache 2.0, already bundled via `lib/rfgui-components/assets/`. Reuse that path. Don't add third-party icon packs without checking the license.
- **Doc header** — when a component is directly inspired by a specific upstream component, say so in the module-level `//!` doc (e.g. `TreeView` says "Inspired by MUI X SimpleTreeView / TreeItem (MIT-licensed source)") so the provenance is visible.
- **Don't mirror** — copy-paste upstream source, reuse proprietary SVG / font assets, or clone paid-tier feature names.

- Button: variant (contained/outlined/text), size (small/medium/large), color (primary/secondary/error/warning/info/success/inherit), start_icon/end_icon, full_width, disabled — `ACCEPTS_CHILDREN: true`, children = label
- IconButton: separate component, circular (icon_button_radius)
- ToggleButton: selected prop, square
- ToggleButtonGroup: compound — see §Compound components
- Don't merge components to save lines. One component per role.

Rust-specific splits:
- exclusive vs multi variants of group → separate components, not `exclusive: bool`
- generic value type → `pub struct Foo<V = String>(PhantomData<V>) where V: 'static;` with `V: Clone + PartialEq + 'static`. Default to `String` so the common call site stays `<Foo .../>` without turbofish; type-erased users opt in with `<Foo::<MyEnum> .../>`.
- generic value used as list key → add `Hash` bound (`V: Clone + PartialEq + Hash + 'static`) so rows can set `key={value.clone()}`. See §Keyed lists.

## Theme

- All visual constants live in `theme.rs`, not hardcoded in component
- Component reads via `let theme = use_theme().0;`
- Component-specific sub-theme under `theme.component.<name>` (e.g. `theme.component.button.size.medium`)
- Atom One palette exposed as `theme.color.atom.{red,blue,...}`, semantic sets mapped on top (`primary` = blue, `error` = red, etc.)
- Don't introduce new sizing / spacing values inline — add to theme first

## Color inheritance

Element's `color` prop cascades to `Text` and icon glyphs (they render as Text).
Pattern: set root Element `color: resolved_text_color`, omit `color` on inner Text, icons inherit automatically.

## Compound components via context

For parents that inject state into children (ToggleButtonGroup, RadioGroup, TabList…):

```rust
use rfgui::ui::Provider;

#[derive(Clone)]
pub struct GroupContext { pub value: Binding<...>, pub on_change: ..., ... }

fn render(props, children) -> RsxNode {
    rsx! {
        <Provider::<GroupContext> value={ctx}>
            <Element ...>{children}</Element>
        </Provider::<GroupContext>>
    }
}
```

Child reads via `use_context::<GroupContext>()` — `Some(_)` = in-group, override own selected / on_click / size / color.

Close tag must repeat the full generic path (rsx parser matches token streams): `</Provider::<GroupContext>>`.

### Walker-ancestry + context-snapshot wipe gotcha

`<Provider::<T> value={v}>{child}</Provider>` emits an `RsxNode::Provider`. When `unwrap_components` descends into it, it pushes `(TypeId::of::<T>(), v)` onto `CONTEXT_STACK` for the entire child walk, then pops.

**Gotcha — pre-built Component children do NOT receive Provider values.** When the walker enters a `RsxNode::Component`, it calls `with_installed_context_snapshot(&snapshot, …)` which **replaces** the live stack with the snapshot captured at that Component's construction time (in `create_element`). Any value an ancestor `<Provider>` pushed during walk is wiped for the duration of that Component's render.

- Inline construction (`provide_context(v, || rsx!{<Child/>})` or a child literally typed inside `<Provider>{<Child/>}</Provider>` at the same rsx invocation): `create_element` captures the snapshot while `v` is live, so Child's render sees it.
- Pre-built construction (caller passes `children: Vec<RsxNode>` from outside into the group component): those Components were built in an outer rsx where `v` wasn't on the stack yet. Their snapshots are stale; Provider's walker-push can't patch them.

Implication: **compound components that consume `children: Vec<RsxNode>` from the outside cannot reliably inject context into those children.** `<ToggleButtonGroup>{<ToggleButton/>...}</ToggleButtonGroup>` only works because both live in the same caller rsx — the group's Provider wraps ToggleButton descriptors that were built in the same outer scope, so the children's snapshots are _older than_ the Provider push and the Provider's walker-push _does_ reach them via the live stack… until the walker enters a deeper user Component, at which point the snapshot installs and wipes. For flat children like ToggleButton (no further component nesting inside), this happens to work. For anything that nests more components (e.g. `<TreeView><TreeItem><TreeItem/></TreeItem></TreeView>`), the inner Component's snapshot wipes the group's context. Don't rely on it.

Lower-level API `rfgui::ui::provide_context_node(value, child) -> RsxNode` is what `<Provider>` expands to.

### Recipes

- **Same rsx invocation, flat children (e.g. `<Group><Item/><Item/></Group>`)** — use `<Provider>`. Works today for the ToggleButtonGroup case.
- **Nested component children (TreeView / Form / Table with compound rows)** — switch to a **data-driven API** (`nodes: Vec<TreeNode>`, `rows: Vec<Row>`, …). The parent renders every row itself from one render scope, wires bindings + click handlers inline, no context needed. This is what `TreeView` does; see `layout/tree_view.rs`.
- **Inline-built descendants** — `provide_context(v, || rsx!{<Child/>})` inside your render body. The closure form builds the child's `context_snapshot` with `v` included, so it works for descendants literally constructed inside the closure.

### Border merge (group outline)

When grouping visually connected buttons:
- Group wrapper Element: `border` + `border_radius` + `position: Position::static_().clip(ClipMode::Parent)`
- Child reads `ctx.in_group` → set own `border: None`, `border_radius: None`
- Flatten `RsxNode::Fragment` children first (user `.map().collect()` produces fragments)
- Inject 1px divider Element between adjacent child Component matches (detect via `type_id == TypeId::of::<ChildTag>()`)
- `CrossSize::Stretch` on layout so children + dividers match cross-axis size

### Event forwarding

Group's overridden on_click wraps user-provided on_click:
```rust
move |event| {
    if let Some(user) = user_on_click.as_ref() { user.call(event); }
    binding.set(next);
    if let Some(cb) = on_change.as_ref() { cb(event, next); }
}
```
on_change signature: `Fn(&mut ClickEvent, NewValue)` — matches MUI `(event, value) => void`.

## Data-driven vs composition

Decision tree:

- List / tree / table where every row is the same component and the parent owns all state → **data-driven**. Prop is `Vec<RowData>` (or `Vec<TreeNode<V>>`). Parent walks data in its render body and emits rows directly. All bindings + click handlers wired from one scope — no cross-component context plumbing needed.
- Heterogeneous layout where children are different component types (Accordion content, Window body, ToggleButtonGroup of flat ToggleButton siblings) → **composition**. Take `children: Vec<RsxNode>` and render them as-is.

Reach for data-driven whenever children would need to read parent context AND nest further components inside themselves. The context-wipe gotcha (§Walker-ancestry) makes composition unreliable in that shape.

Example: `TreeView` takes `nodes: Vec<TreeNode<V>>` instead of `<TreeView><TreeItem/>…</TreeView>` because each item re-nests child items — Provider-based context would wipe at the nested boundary.

## Keyed lists

When a data-driven component emits a `Vec<RsxNode>` of row Elements from user data:

```rust
rsx! {
    <Element
        key={value.clone()}
        …
    >
        …
    </Element>
}
```

- `key=` accepts any `T: Hash + Any` (rsx-macro calls `classify_component_key`, hashes to `RsxKey::Local(u64)`).
- Value must impl `Hash` — add the bound when component is generic over `V`.
- Reorder / insert / delete in the source `Vec<data>` → row state (hooks, animation mid-state) tracks the value, not the positional index. Same semantics as React `key`.
- Without `key`, siblings are identified positionally; dropping an early row shifts all subsequent rows' state one up.

## Icon slots (data-driven)

For components whose data rows carry an icon:

- Store `icon: Option<String>` — a Material Symbols ligature (`"folder"`, `"code"`, `"description"`). Storing component types (`FolderIcon`) in data is awkward and locks the caller into a specific icon set.
- Optional `expanded_icon: Option<String>` for stateful pairs (folder / folder_open). Fall back to `icon` when the state-specific one is unset.
- Render with `<MaterialSymbolIcon>{ligature}</MaterialSymbolIcon>` in a fixed-size slot Element; `None` → `RsxNode::fragment(vec![])` so the row collapses instead of reserving blank space.

Hard-coded icons inside a component (e.g. `<ChevronRightIcon/>` for the expand affordance) are fine — those are part of the visual contract, not caller data.

## Keep internals private

- Helper fns (`resolve_color`, `size_spec`, `rewire_context_snapshots`) → `pub(crate)` or private
- Only the component struct, Props, public enums cross the module boundary
- Shared color/size lookup lives in `inputs/button.rs` (pub(crate)) so IconButton / ToggleButton reuse

## Don't

- Hardcode colors / sizes — always go through theme
- Write a new native view component to avoid composing (`Element` + `Text` covers everything)
- Use `#[component]` on `impl RsxTag` — use `#[rfgui::ui::component]` (emits vtable)
- Skip `#[derive(Clone)]` on Props — vtable needs it
- Mutate `RsxNode::Component` props directly — use `into_render_parts` / reconstruct
- Use `options: Vec<...>` prop to avoid context when compound API makes sense — prefer children + context (but see §Data-driven vs composition first)
- Use `children: Vec<RsxNode>` if component has no children — set `ACCEPTS_CHILDREN: false`
- Rely on `<Provider>` to reach nested Component children of caller-supplied `children` — snapshot wipe breaks it (see §Walker-ancestry)
- Store component types (`FolderIcon`, `CodeIcon`) in data-shape structs — use ligature strings + `MaterialSymbolIcon`
- Emit a `Vec<RsxNode>` of rows without `key=` when rows are tied to mutable source data — reorder loses hook state
