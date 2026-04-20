---
name: m08-state-management
description: rfgui state management reference — use_state / Binding / GlobalState / provide_context / use_context / UiDirtyState / render_memoized_component / with_component_key / use_timeout / use_interval / use_mount. Use whenever the user asks where to put state, how to share state between parent and child components, how to build compound components (ToggleButtonGroup / Tabs / RadioGroup), why something re-renders, when to use REDRAW vs REBUILD, whether to use cloneElement or context, or how to keep state alive across reorders.
---

# State Management

## Decision Tree

1. 只一個 component 用 → `use_state`
2. Parent → direct child → `Binding<T>` as prop（`state.binding()`）
3. Parent → deep / compound child → `provide_context<T>(value, || children)` + `use_context<T>()`
4. App-wide singleton（theme / viewport / user）→ `global_state` + `use_global_state`
5. 跨 reorder / parent 變動要保留 state → `with_component_key(RsxKey::Global(k))`
6. 只影響 paint 不影響 tree（hover / animation）→ `use_state_with_dirty_state(REDRAW)`
7. Props 相等想跳 render → `render_memoized_component`（`#[component(memo)]`）

---

## Primitives

| API | 範圍 | 用途 |
|---|---|---|
| `use_state<T>() -> State<T>` | component-local | 常用 state slot |
| `State<T>` | — | `.get()` `.set()` `.update()` `.binding()` |
| `Binding<T>` | shared handle | prop 傳遞（`IntoPropValue`） |
| `Binding::new(init)` | 外部建立 | 跳過 hook slot |
| `global_state<T>(init)` | TypeId singleton | 初始化 app-wide state |
| `use_global_state<T>()` | — | 讀取已初始化 singleton |
| `GlobalState<T>` | — | `.get()` `.set()` `.update()` `.binding()` |
| `use_state_with_dirty_state(init, REDRAW)` | component-local | 只重繪，不 rebuild |
| `render_memoized_component::<T, P>(props, render)` | — | Props `PartialEq` 相等跳 render |
| `with_component_key(Some(RsxKey::Local/Global), f)` | — | state 綁 key 存活 reorder |
| `provide_context<T>(value, \|\| f)` | subtree | 對後裔暴露 `T`（TypeId stack，支援 nesting / shadowing） |
| `use_context<T>() -> Option<T>` | — | 讀最近祖先提供值 |
| `use_context_expect<T>() -> T` | — | 缺 provider 即 panic（根組件用） |
| `use_timeout(enabled, dur, cb)` | effect | 單次 |
| `use_interval(enabled, dur, cb)` | effect | 重複 |
| `use_mount(|| cleanup)` | effect | 首掛 / unmount cleanup |

---

## UiDirtyState

| Kind | 效果 |
|---|---|
| `NONE` | 無事 |
| `REDRAW` | 重繪 pass，不 rebuild tree |
| `REBUILD` | 重 render component |

- `use_state` 預設 REBUILD
- 動畫 / hover 視覺態用 REDRAW，省 rebuild
- `GlobalState` / free `Binding` 變動：保守清整 memo cache
- Component-owned state 變動：只 invalidate 該 component 的 memo 條目

---

## Keys / Identity

- `RsxKey::Local(u64)`：sibling 間穩定身份，reorder 不丟 state
- `RsxKey::Global(GlobalKey)`：跨 parent / subtree 移動不丟 state（memo）
- `GlobalKey` build 內唯一，重複 panic
- 用 `with_component_key(Some(...), || render)` 包裹

---

## Compound Component 模式

父層要控制 children props（ToggleButtonGroup / Tabs / RadioGroup）→ 用 **context**：

```rust
#[derive(Clone)]
struct ToggleGroupCtx {
    value: Binding<Option<String>>,
}

#[component]
fn ToggleButtonGroup(value: Binding<Option<String>>, children: Vec<RsxNode>) -> RsxNode {
    let ctx = ToggleGroupCtx { value };
    provide_context(ctx, || rsx! { <Fragment>{children}</Fragment> })
}

#[component]
fn ToggleButton(value: String, children: Vec<RsxNode>) -> RsxNode {
    let ctx = use_context_expect::<ToggleGroupCtx>();
    let selected = ctx.value.get().as_ref() == Some(&value);
    let on_click = move |_| ctx.value.set(Some(value.clone()));
    rsx! { <Element on_click={on_click}>{children}</Element> }
}
```

- Context 值為 `Clone + 'static`；包 `Binding<T>` 取得讀寫 + 變動通知（沿用 binding dirty pipeline）
- Nesting OK：內層 provider shadow 外層，離開 scope 自動還原
- Panic-safe：`provide_context` guard 負責 pop

**不要做**：
- `cloneElement` 掃 children 注 prop — 違背 arena 語意
- `GlobalState` 當 group state — TypeId 撞，多 group 共享同值（context 每個 provider push 獨立 stack 層，無此問題）

---

## Forbidden

- `GlobalState` 當 component-specific state（TypeId 撞）
- `Binding::new` 在 render body 內（每 render 新 handle，破 identity）
- 同 build 重複 `GlobalKey`（panic）
- cloneElement 式 children 注入
