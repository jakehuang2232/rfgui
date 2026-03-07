# ROADMAP

## 目標
- 建立 typed-only 的樣式與佈局能力，避免字串解析路徑。
- 讓 RSX 撰寫體驗更一致，並維持互動狀態穩定（hover/scroll/node id）。
- 以可驗收的里程碑推進：每項都要有測試與範例。

## 里程碑 M1（近期）

### 1. 新增 `:focus` 狀態樣式
**範圍**
- parsed style 新增 focus 狀態欄位（typed）。
- computed style 合併 focus 狀態規則。
- element sync + renderer 支援 focus 狀態切換重繪。
- RSX/schema 可宣告 focus 相關 style。

**執行步驟**
1. 在 `parsed_style` 新增 focus 變體（不走字串鍵值）。
2. 在 `computed_style` 加入 focus 合成規則，維持 struct 化欄位。
3. 在事件流程接入 focus/blur，觸發節點重繪。
4. 在 renderer 套用 focus 視覺（例如 outline/border 變化）。
5. 補測試與範例（鍵盤導覽、點擊切換 focus）。

**驗收條件**
- focus/blur 後，目標節點可穩定更新視覺。
- 不會導致整棵樹重建或 node id 漂移。
- 測試涵蓋 focus 切換與重繪路徑。

### 2. `Display` 命名調整為 `Layout`
**範圍**
- 型別、欄位、RSX schema、文件與範例的命名統一。
- 僅做語意對齊，不引入行為變更。

**執行步驟**
1. 盤點 `Display` 的 enum/type/欄位與公開 API。
2. 以相容遷移方式改名為 `Layout`（必要時先保留 alias）。
3. 更新 rsx-macro/schema 錯誤訊息與文件用語。
4. 更新 examples 與 README 對應片段。

**驗收條件**
- 專案可編譯、既有測試通過。
- `Layout` 成為主命名；舊命名若保留需標記 deprecate。
- 無行為回歸（layout 結果與改名前一致）。

## 里程碑 M2（下一步）

### 3. `Position::Absolute` 重構為 `Placement`
**目標**
- 用單一 `Placement` 模型同時支援 `Edges`（CSS 風格）與 `Align`（UI 對齊點風格）。
- 保持 typed-only，遵守 `%` 在可解析容器下才生效的規則。

**精簡設計草案**
```rust
#[derive(Clone, Debug)]
pub struct AbsSpec {
    pub anchor: Option<AnchorName>,
    pub placement: AbsPlacement, // Edges 與 Align 二選一
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

**執行步驟（依專案規範順序）**
1. `parsed_style`：加入 `Placement` typed 欄位與解析入口。
2. `computed_style`：建立 `AbsSpec/AbsPlacement` 對應欄位。
3. `schema/macro`：RSX props 綁定到新型別，移除舊 absolute 寫法。
4. `element sync`：將 placement 同步到 element 佈局資料。
5. `renderer/layout solver`：實作 Edges 與 Align 的定位計算。
6. `tests + examples`：覆蓋 `%`、anchor、line wrap 交互情境。

**驗收條件**
- `Edges` 路徑可涵蓋 top/right/bottom/left 任意組合。
- `Align` 路徑可用 `Length::percent(0/50/100)` 與 `offset` 取得預期位置。
- 容器尺寸不定時，`%` 不反向影響父層量測。
- 舊 absolute API 有遷移說明與 deprecate 策略。

### 4. 整理 `base_element` trait 能力邊界
**目標**
- 盤點並收斂 `ElementTrait`、`Layoutable`、`EventTarget`、`Renderable` 的責任邊界。
- 降低 `as_any + downcast::<Element>` 依賴，改用能力導向介面。
- 此階段先完成設計與遷移計畫，不先做大規模行為變更。

**範圍**
- 輸出能力矩陣（容器/文字/可編輯元件各自需要的能力）。
- 定義最小核心節點介面與可選能力 trait（layout / event / render / transition / snapshot）。
- 盤點 `mod.rs` 與 `element.rs` 內的 downcast 熱點並提出替代 API。

**執行步驟**
1. 建立現況盤點文件：trait 方法使用率、呼叫點、元件覆寫差異。
2. 設計「核心介面 + 能力 trait」草案（先不改行為）。
3. 列出所有 downcast 呼叫點與對應替代方法（例如 transition/hit-test/defer render）。
4. 規劃分階段遷移順序與相容策略（避免一次性重寫）。
5. 補上回歸測試清單與風險控制項。

**驗收條件**
- 有可執行的遷移清單（含檔案、方法、順序）。
- 明確標出可刪除的 downcast 路徑與替代 API。
- 不引入字串樣式管線，維持 typed-only 原則。

### 5. 以 Layout 接管 Bounds，並移除獨立 Bounds 管線
**目標**
- 以 layout 結果作為唯一幾何來源，移除額外 `bounds` 概念與同步成本。
- hit-test、clip、可視判定、scroll 範圍統一依據 `layout_position/layout_size/layout_inner_*`。

**範圍**
- 事件命中與 bubbling 的 local 座標計算。
- 渲染裁切與可視判定流程。
- scroll 邊界與內容大小推導。

**執行步驟**
1. 盤點所有 `bounds` 讀寫點與等價 layout 欄位來源。
2. 建立 layout 幾何存取 API（outer/inner/content/clip）作為唯一入口。
3. 先切換 hit-test 與事件 local 座標到 layout 幾何。
4. 再切換 render clip/可視判定與 scroll 邊界計算。
5. 移除剩餘 `bounds` 欄位與同步邏輯，補上遷移註記。

**驗收條件**
- `bounds` 不再作為資料來源；幾何判定全由 layout 提供。
- hover/scroll/焦點互動不回歸，node id 與狀態維持穩定。
- `%` 尺寸、absolute clip、scroll bubble 路徑測試通過。

## 里程碑 M3（Viewport 整理）

### 6. Viewport 現況架構盤點（layout/renderer/style/rsx schema）
**目標**
- 明確化 viewport 在 `layout -> renderer -> style -> rsx schema` 的資料流與責任邊界。
- 作為後續效能與可維護性重構的基準文件。

**現況摘要**
1. `Viewport` 主導每幀流程：`measure -> place -> collect_box_models -> build graph -> compile -> execute`。
2. transition 在 layout 後會再觸發一次 `place + collect_box_models`（需要時）。
3. `Element::measure` 有 `layout_dirty + last_layout_proposal` 快取；`place` 仍完整執行。
4. `%/vw/vh` 透過 typed `Length::resolve_with_base` 解算，`%` 在 base 不可解時不回推父層。
5. absolute 的 `ClipMode::Viewport` / `CollisionBoundary::Viewport` 於 place 階段依 runtime viewport 尺寸解算。
6. RSX macro 以 `ElementStylePropSchema` 做 style key 編譯期檢查。

**輸出物**
- 一份維護中的架構圖（文字版即可）與模組責任對照。
- 一份 hot path 清單（layout、build graph、hit-test、transition）。

### 7. Viewport 問題清單收斂（效能 / 可維護性 / 可讀性）
**效能問題**
1. 每幀都重建並 compile frame graph，CPU 開銷高。
2. post-layout transition 觸發二次 `place + collect_box_models`，動畫密集時成本放大。
3. `overflow_child_indices.contains(&idx)` 在 loop 內重複線性查找，存在 O(n^2) 風險。

**可維護性問題**
1. `PLACEMENT_RUNTIME` 使用 thread-local 隱式狀態，資料流不透明。
2. `ElementPropSchema` / renderer 仍保留 legacy 視覺 props（如 `padding_x`），與 typed-style 單一路徑不一致。
3. `TextAreaPropSchema` 仍含 `String/f64` 幾何與顏色欄位，未與 typed-style 收斂。

**可讀性問題**
1. `viewport.rs` 與 `element.rs` 職責過重（layout/render/input/transition 混合），檔案過大。
2. `set_size` / `set_scale_factor` 不主動 request redraw，依賴呼叫端記憶，語意不夠直觀。

### 8. Viewport 改善方案（不含風險控管）
**短期（先拿效能）**
1. 將 `overflow_child_indices` 改為 `Vec<bool>` 或 `HashSet<usize>`，移除 O(n^2) 路徑。
2. transition 分流：僅幾何變更才觸發 relayout；純樣式/視覺變更避免二次 place。
3. 導入 frame graph 重用策略：優先快取靜態 pass 結構，僅更新動態參數。

**中期（提高可維護性）**
1. `viewport` 拆模組：`input_dispatch.rs`、`render_pipeline.rs`、`transition_runtime.rs`。
2. `element` 拆模組：`measure.rs`、`place.rs`、`clip_hit_test.rs`、`paint.rs`。
3. 將 `PLACEMENT_RUNTIME` 改為顯式 `PlacementContext` 傳遞，降低隱式共享狀態。

**中期（schema/API 收斂）**
1. 移除 `ElementPropSchema` legacy 視覺欄位，統一走 `style`。
2. `TextArea` 幾何與顏色改用 typed style（`Length` / `ColorLike`）。
3. RSX macro 繼續保留編譯期 schema 驗證，並與 AGENTS typed-only 規範同步。

## 跨里程碑品質門檻
- 每一項能力都需附最小可重現 example。
- 回歸測試優先順序：`%` 解析、scroll 狀態穩定、hover/focus 重繪、border radius 裁切一致性。
- 僅使用 typed API（`Length`/`Border`/`BorderRadius`/`ColorLike`），禁止字串 style 管線。
