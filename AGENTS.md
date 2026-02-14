# AGENT.md

本文件定義本專案（rust-gui）的 UI / Style / Layout 核心規範，作為後續實作與重構的單一依據。

## 1. Style 三層模型

1. Parsed Style（外部輸入層）
- 目的：承接 RSX / DSL / 類 CSS 表達。
- 型別策略：禁止字串 value；全部用明確型別（例如 `Length::px(10.0)`、`Length::percent(50.0)`）。
- 屬性 key 使用 enum（`PropertyId`），避免動態字串鍵。

2. ComputedStyle（引擎核心層）
- 必須是 struct（不可用 map 代表）。
- 儘量與 Parsed Style 共用型別（`Length`、`BorderRadius`、`Border` 等）。
- 不在此層做字串 parse。

3. LayoutState（solver 輸出層）
- 僅放排版結果（位置、尺寸、基線等）。
- 不屬於 style，不混入宣告屬性。

## 2. 型別化 Style 規範

- 禁止 style 值使用字串（顏色例外可透過 `Color::hex(...)` 建構，但不是裸字串解析流程）。
- `Length` 為基礎尺寸單位：
  - `Length::px(f32)`
  - `Length::percent(f32)`
  - `Length::Zero`
- `%` 規則（重要）：
  1. 只對「已知 content size 的 parent」生效。
  2. parent size 未定時，`%` 視為 `auto`（對 `width/height`）。
  3. 不允許 `%` 反向影響 parent measure。

## 3. Color 系統

- 顏色實作集中於 `style::color` 模組。
- 一律以 `ColorLike`/`Color` 系列型別流動，不走字串 parse 流程。
- `ElementStylePropSchema` 與 `parsed_style` 中所有 color 欄位都使用 ColorLike 導向設計。
- Style 中的 color value 一律採 `ColorLike`。

## 4. Element 與 Props 方針

- `Element` 的視覺樣式統一由 `style` 提供。
- 移除舊式視覺 props（例如 `background`、`border_color`、`border_width` 等直接 props）。
- 使用方式：
  - `style={{ background: Color::hex("#000") }}`
  - `style={{ border: Border::uniform(...) }}`
  - `style={{ border_radius: BorderRadius::uniform(...) }}`
- `border-radius` 與 `border` 分離，不耦合。

## 5. Box Model API

### Padding
- 支援 fluent API：`uniform/all/x/y/top/right/bottom/left/xy`
- 例：`Padding::all(Length::px(10.0)).xy(Length::percent(20.0), Length::px(8.0))`

### Border
- 採 CSS 風格：uniform + 各邊覆寫
- 支援 `top/right/bottom/left/x/y` 覆寫寬度與顏色
- 例：`Border::uniform(Length::px(2.0), &Color::hex("#000")).top(Some(Length::px(4.0)), None)`

### BorderRadius
- 四角獨立：`uniform/top/right/bottom/left/top_left/top_right/bottom_left/bottom_right`
- 內外圓角裁切需保持一致（包含 border + inner clip）。

## 6. Layout 方向（SwiftUI 心智 + CSS 表達）

- 取消依賴 `x/y` 做一般定位，改以容器排版。
- 支援 flow / inline + flex wrap 行為，依可用寬度換行。
- `width/height` 放入 `style`，型別使用 `Length`。
- `Length::percent` 的基準為 parent inner size（在可解析時）。

## 7. Scroll 模型

- 不使用 `overflow`；採 `ScrollDirection`（SwiftUI 風格）：
  - `None / Vertical / Horizontal / Both`
- 事件：wheel 走 hit-test + bubble，遇可捲容器處理。
- 可視狀態：`scroll_offset`、`content_size` 由 Element 維護。
- 捲軸 UI：
  - 自動顯示/淡出（hover/scroll/drag 觸發）
  - 支援 thumb 拖曳
  - 支援點擊 track 跳轉
  - track 與 thumb 都要 render

## 8. Hover / Re-render / 穩定識別

- hover 狀態變化必須觸發 redraw。
- 同一 node id 必須穩定，不可每幀重建導致 id 飄移。
- RSX render pipeline 需避免每次 redraw 重建整棵 UI tree（否則會重置互動狀態，如 scroll_offset）。

## 9. RSX 體驗

- `rsx!` 內 `style` props 需可被跳轉（IDE 導航友善）。
- `ElementStylePropSchema` 維持與 style 系統一致，不保留過時欄位（例如 `padding_x` 類舊欄位）。

## 10. 實作守則

- 先維持型別正確，再擴充語意。
- 新增樣式能力時，依序更新：
  1. parsed_style
  2. computed_style
  3. schema / macro
  4. element sync
  5. renderer
  6. tests + example
- 優先補回歸測試（特別是 `%`、border radius、scroll、text measurement）。

