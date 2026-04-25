# Element Layout Functional Refactor

## 背景

`Element` 目前擔當：
- 元件本身（持有 style、children、layout state、event handlers 等）
- 排版演算法宿主（`measure` / `place` / `measure_inline` / `place_inline` / `compute_flex_info` / `place_flex_children` 等都直接 impl 在 Element 上）

排版演算法跟元件實體耦合，造成：

1. **重複實作的壓力**：TextArea v2 想用 inline formatting 必須繼承 Element 或重抄演算法。
2. **單檔過大**：`element/impl_layout.rs` 1547 行、`element/impl_render.rs` 2117 行（裡面藏 `measure_flex_children` / `compute_flex_info`，命名也錯置）、`element/layout_trait.rs` 726 行。
3. **三種 axis layout 黏在一起**：`Layout::Inline` / `Layout::Flex` / `Layout::Flow` 共用 `compute_flex_info` + `place_flex_children`，分支散布在多檔。
4. **inline fragment 邏輯散三處**：`layout_trait.rs:495`、`impl_layout.rs:1413`、`impl_render.rs:710`。
5. **演算法難以單獨測試**：要 setup arena、Element 實例才能跑單元測試。

## 設計原則

採 **Functional Core / Imperative Shell** 架構：

```
                  ┌─────────────────────────────────────┐
                  │  imperative shell                   │
                  │  Element::measure(&mut self, ...)   │
                  │   ↓ I (read state → inputs)         │
                  │  let inputs = gather(self);         │
                  │   ↓ FC (pure algorithm)             │
                  │  let outputs = layout::axis::       │
                  │      measure_axis(inputs, arena);   │
                  │   ↓ O (write outputs → state)       │
                  │  apply(self, outputs);              │
                  └─────────────────────────────────────┘
```

- **Functional Core**：`src/view/layout/` 模組，輸入資料 + 輸出資料 + arena（effect channel）。無隱藏 mutation、無 trait 多態。
- **Imperative Shell**：Element / TextArea 各自 impl `Layoutable`，內部 read self → 組 inputs → call core → write outputs。

### 為何不用 trait

考慮過 `LayoutHost: ElementTrait` extension trait（13 個 method），結論是**過度抽象**：

| 維度 | trait 版 | functional 版 |
|---|---|---|
| 抽象成本 | 13 method、impl 者全填 | 0 trait、資料結構即契約 |
| 測試 | 須 mock host | 直接 fixture in/out |
| Coupling | shell 跟 trait 雙向綁 | shell 單向呼 fn |
| 加新 host | 重 impl 13 method | 寫 input gather + output apply 即可 |
| 改演算法 | 動 trait 介面 → 動所有 impl | 動 fn 內部 + input struct，shell 微調 |

trait 包裝的是 mutation。functional 版 mutation 顯式（caller 自己寫）、algorithm 純粹。

## 用量盤點（refactor 範圍）

對 `element/{layout_trait,impl_layout,impl_render,mod}.rs` 統計 layout-related field 命中：

| Group | 欄位 | 命中 | 角色 |
|---|---|---:|---|
| **G1 input** | `computed_style.layout` | 9 | r |
| | `computed_style.gap` | 4 | r |
| | `computed_style.padding` | 20 | r |
| | `computed_style.border_widths` | 20 | r |
| **G2 output** | `layout_position` | 35 | r/w |
| | `layout_size` | 54 | r/w |
| | `layout_inner_position` | 5 | r/w |
| | `layout_inner_size` | 12 | r/w |
| | `layout_flow_position` | 20 | r/w |
| | `layout_flow_inner_position` | 9 | r/w |
| | `content_size` | 8 | r/w |
| | `should_render` | 8 | r/w |
| **G3 cache** | `flex_info` | 4 | r/w (take/set) |
| | `inline_paint_fragments` | 10 | r/w |
| **G4 scroll** | `scroll_offset` | 10 | r |
| **G5 dirty** | `dirty_flags` | 6 | r/w |
| | `mark_place_dirty` | 3 | call |

合計約 230+ 個 callsite 需要在 refactor 中對齊。

## 資料結構

### Inputs

```rust
pub struct AxisInputs {
    pub kind: Layout,
    pub style: AxisStyle,             // gap / direction / justify / align / wrap
    pub box_model: BoxModel,          // padding / border_widths
    pub children: Vec<NodeKey>,
    pub absolute_mask: Vec<bool>,     // child_is_absolute 預先算好
    pub proposal: LayoutProposal,
    pub viewport: Viewport,
}

pub struct PlaceInputs {
    pub axis: AxisInputs,
    pub placement: LayoutPlacement,
    pub scroll_offset: Vec2,
    pub flex_info_cache: Option<FlexLayoutInfo>,
}

pub struct AxisStyle {
    pub layout: Layout,
    pub gap: SizeValue,
    pub direction: FlowDirection,
    pub wrap: FlowWrap,
    pub justify_content: JustifyContent,
    pub cross_axis: CrossAxis,
}

pub struct BoxModel {
    pub padding: EdgeLengths,
    pub border_widths: EdgeLengths,
}
```

### Outputs

```rust
pub struct LayoutState {
    pub layout_position: Position,
    pub layout_size: Size,
    pub layout_inner_position: Position,
    pub layout_inner_size: Size,
    pub layout_flow_position: Position,
    pub layout_flow_inner_position: Position,
    pub content_size: Size,
    pub should_render: bool,
}

pub struct MeasureOutputs {
    pub measured_size: Size,
    pub flex_info: FlexLayoutInfo,
}

pub struct PlaceOutputs {
    pub state: LayoutState,
    pub flex_info: FlexLayoutInfo,
    pub inline_paint_fragments: Vec<Rect>,
    pub dirty_clear: DirtyFlags,
}

pub struct InlineMeasureOutputs {
    pub nodes: Vec<InlineNodeSize>,
    pub measured_size: Size,
}

pub struct InlinePlaceOutputs {
    pub state: LayoutState,
    pub paint_fragments: Vec<Rect>,
    pub dirty_clear: DirtyFlags,
}
```

### 演算法簽名（free fn）

```rust
pub fn measure_axis(
    inputs: AxisInputs,
    arena: &mut NodeArena,
) -> MeasureOutputs;

pub fn place_axis(
    inputs: PlaceInputs,
    arena: &mut NodeArena,
) -> PlaceOutputs;

pub fn measure_inline(
    inputs: AxisInputs,
    context: InlineMeasureContext,
    arena: &mut NodeArena,
) -> InlineMeasureOutputs;

pub fn place_inline(
    inputs: PlaceInputs,
    placement: InlinePlacement,
    arena: &mut NodeArena,
) -> InlinePlaceOutputs;

// 子層共用
pub fn compute_flex_info(
    inputs: &AxisInputs,
    arena: &mut NodeArena,
) -> FlexLayoutInfo;

pub fn measure_children(
    inputs: &AxisInputs,
    arena: &mut NodeArena,
);
```

## 邊界與不純成分

- **Effect channel**：`arena: &mut NodeArena` 用於 child 遞迴 measure / place，不可消除。
- **profile timing**：`LAYOUT_PLACE_PROFILE` thread-local 計時 hook 移到 shell。
- **dirty short-circuit**：在 shell 端做（讀 `dirty_flags` / `last_layout_proposal` 等），不進 core。

純的部分（可獨立單元測試）：
- flex line-break 演算法
- main / cross axis 推進
- justify-content / align-items 計算
- inline fragment 累積（給定 line widths → paint rects）

## 模組佈局

```
src/view/layout/
├── mod.rs                # pub use; 公共 types
├── types.rs              # AxisInputs / PlaceInputs / *Outputs / LayoutState /
│                           AxisStyle / BoxModel / FlexLayoutInfo / FlexLineItem
├── shared/
│   ├── mod.rs
│   ├── flex_solver.rs    # compute_flex_info（Inline/Flex/Flow 共用）
│   ├── measure.rs        # measure_children
│   └── helpers.rs        # cross_item_offset / main_axis_start_and_gap / cross_start_offset
├── inline.rs             # measure_inline / place_inline + fragmentation
├── flex.rs               # place_axis (Flex 變種)
├── flow.rs               # place_axis (Flow 變種)
├── grid.rs               # 占位 (TODO)
├── block.rs              # 非 axis 路徑（resolve lengths + per-child place）
└── tests/
```

每檔目標 < 500 行。

## Phase 規劃

| Phase | 內容 | LOC | 風險 |
|---|---|---:|---|
| **F0** | 建 `src/view/layout/` + 定義 `types.rs`（無實作） | +250 | 0 |
| **F1** | 抽 `LayoutState` sub-struct，Element 內部聚 8 個散 fields，所有 callsite 改 `self.layout_state.X` | +80 / -50 callsite | 中 |
| **F2** | 抽 `compute_flex_info` 為 free fn `(AxisInputs, &mut Arena) -> FlexLayoutInfo`，Element 改呼 | +200 / -200 | 中 |
| **F3** | 抽 `measure_children` / `measure_axis` | +250 / -250 | 中 |
| **F4** | 抽 `place_axis` 三條（Inline / Flex / Flow 各一 free fn） | +400 / -400 | 中高 |
| **F5** | 抽 `place_inline` fragmentable 邏輯（layout_trait.rs:495 那段） | +230 / -230 | 高 |
| **F6** | Element `layout_trait.rs` 變 thin shell（read state → call core → write state） | +150 / -500 | 中 |
| **F7** | 純單元測試補齊（fixture-based，不需 arena） | +400 | 低 |

**總工時**：5-6 工作天。

### 依賴

```
F0 (types)
  └─→ F1 (LayoutState)
        └─→ F2 (flex_solver)
              └─→ F3 (measure)
                    └─→ [F4 (place_axis) │ F5 (place_inline)]
                                 └─────────┴─→ F6 (Element shell)
                                                      └─→ F7 (tests)
```

F4 / F5 可並行（不同 layout mode / 不同 fn）。F6 是收斂步。

### 驗收條件每 Phase

1. `cargo test -p rfgui` 全綠（lib 391+）
2. `cargo test -p rfgui-components` 全綠（30+）
3. Element layout perf benchmark 退步 ≤ 5%
4. 每 phase 一個 commit

## 後續：TextArea v2 接面

F0–F7 完成後：

```rust
impl Layoutable for TextArea {
    fn measure(&mut self, constraints: LayoutConstraints, arena: &mut NodeArena) {
        let inputs = AxisInputs {
            kind: Layout::Inline,
            style: AxisStyle::inline_default(),
            box_model: BoxModel::ZERO,           // 純文字單元無 padding/border
            children: self.children.clone(),
            absolute_mask: vec![false; self.children.len()],
            proposal: constraints.into(),
            viewport: ...,
        };
        let outputs = layout::inline::measure_inline(inputs, ..., arena);
        self.layout_state = outputs.state_partial();
        self.flex_info = Some(outputs.flex_info);
    }
}
```

Element 跟 TextArea 共用 `layout::*` free fn，無 trait、無重複實作。

## 風險

| 風險 | 對策 |
|---|---|
| Silent regression（行為等價 refactor 最大風險） | 開工前對 Element flex/inline 跑現存全測 + 加 visual snapshot test，每 phase 全程比對 |
| Perf 退步（演算法搬遷後 cache miss / 不必要 alloc） | F0 先加 `LAYOUT_PLACE_PROFILE` criterion bench，每 phase 比對；output struct 大時走 pre-allocated buffer |
| Shell 端 read/write fields 邏輯漏帶（漏寫某 field） | F1 LayoutState 聚合先做，把零散 fields 變單點，shell 寫回時不易遺漏；每 phase 加 round-trip property test |
| F5 inline fragment 高風險 | 抽出來前先補 inline-heavy demo 截圖 baseline；F5 commit 跑同套 demo 視覺比對 |
| F4 / F5 並行衝突 | 不同 fn 不同檔，理論獨立；若同 phase 同 commit 則序列做 |

## Out of Scope

- Element 本身瘦身（移除 padding/border/background 等非 TextArea 用得到的 style）
- Grid 真實作（保留 `Grid => {}` 占位）
- TextArea v2 實作（在另一份 plan）
- 引 ropey / undo-redo 等大改

## 立即可做

F0 + F1 不互相阻擋、zero behavior change，可同時進行：

- F0：純加 types，不動現有檔案
- F1：純 Element 內部 fields 聚合，不動演算法

兩者可同 commit 或分兩 commit。完成後 F2–F7 序列推進。
