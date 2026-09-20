# State 更新佇列與 render 快照

`State<T>`、獨立或衍生的 `Binding<T>`、`GlobalState<T>` 共用同一套更新機制。

- `get()` 讀取建立該 handle 時的快照。`clone()`、舊 callback 與 `State::binding()` 保留同一份快照。
- `set(value)` 排入替換；`update(move |value| ...)` 排入更新函式。更新函式依序接收前一筆結果，必須保持純粹，不在其中發通知、呼叫其他 setter 或讀寫外部資源。
- 每個 target 只加入排程一次；一批更新完成後比較最終值。沒有變化便保留原本的 `Rc<T>`，不產生新的 dirty 訊號。
- 快照透過 `Rc<T>` 共用。`T: Clone` 的深淺由型別決定：`Rc<RefCell<_>>` 仍是可變參照，不會自動變成不可變資料。
- Rust 的相等判斷採 `PartialEq`，不是 JavaScript `Object.is`。

## 事件與畫面邊界

Viewport 的事件分派涵蓋原生輸入變更、事件冒泡與所有 handler。巢狀 handler 共用外層 batch；最外層事件結束時才提交並通知。timer callback 各自是一個 batch。自訂 host 可以用 `batch_state_updates(|| ...)` 包住一個邏輯事件。

未被事件包住的 setter 只排隊並要求排程。`flush_state_updates()`、最外層 `build_scope` 或 `Viewport::render_frame` 會處理佇列。先提交狀態，再 build；從 build 到該幀 render 結束不會再次提交。期間新增的更新保留到下一次處理，不會被 `take_state_dirty()` 清掉。`peek_state_dirty()` 同時反映尚未處理的更新。

React 的 state 快照、更新順序與事件 batching 是本次對齊的範圍；這不是 React Fiber 的完整排程器。此引擎仍由 host 排定下一幀，因此不保證每個離散事件之前都已重新 build callback。render 中 setter 會延到下一幀，不支援 React 的同一 render-phase 重新執行，也不加入 concurrent rendering、transition lane 或 StrictMode 雙重呼叫。元件不應在每次 render 無條件排入更新。

## 遷移

```rust,ignore
// 同一 render 的三次替換都使用同一個快照，結果是 +1。
state.set(state.get() + 1);
state.set(state.get() + 1);
state.set(state.get() + 1);

// 三次更新依序累積，結果是 +3。
state.update(|n| *n += 1);
state.update(|n| *n += 1);
state.update(|n| *n += 1);
```

`use_state`、`use_global_state` 每次 render 取得新快照。長期持有的 `Binding` 或 `GlobalState` 在新 render 使用 `.snapshot()` 取得已提交值；在 callback 內隱含刷新會破壞快照語意，所以 `clone()` 不會刷新。傳入現有受控元件的 Binding 由元件在 render 入口刷新。

TextArea、拖曳與其他原生控制項在不同事件間需要持續操作同一輸入 session，使用明確的 `Binding::get_committed()` 橋接。它只讀已提交值，不會提早執行佇列。一般畫面與應用 callback 應使用快照；要依前值計算時使用純 updater。宿主必須在下一次離散操作前更新 UI，才能取得與 React 離散事件完全相同的 callback 更新時機。

`update` 現在要求 `'static`；借用事件資料的程式先取出所需資料，再使用 `move`。卸載後舊 State／衍生 Binding 的更新會被忽略；同 key 再掛載會取得新的存活 token。memo 更新會使 owner、包含其輸出的 ancestor、讀取該 state 的 memo 失效，其他 sibling 可繼續共用既有結構。

參考：[React useState](https://react.dev/reference/react/useState)、[State as a Snapshot](https://react.dev/learn/state-as-a-snapshot)、[Queueing a Series of State Updates](https://react.dev/learn/queueing-a-series-of-state-updates)。

## 本次驗證（2026-09-20）

- `cargo test --workspace --lib --bins`：核心 1,787、元件 34、macro 21、segmenter 12、範例 4 項通過；原有 102 項 ignored 未在此命令執行。新增 TooltipRef 測試後另跑 `cargo test -p rfgui-components --lib`：35 項通過。
- `cargo check --workspace --all-targets`：通過。
- `cargo check -p examples --target wasm32-unknown-unknown`：通過。
- `cargo test -p rfgui-components --test retained_controls -- --ignored --nocapture`：沙盒找不到 GPU，改用原生權限後在 Apple M5／Metal 通過。實際執行 15 種控制項 × 2 renderer modes × 2 DPR × 9 幀，共 540 幀，包含輸出像素探針、狀態改變、暖幀像素穩定、RetainedAuto 的 backing identity 與重用檢查。這是控制項回歸證據，不代表所有原有 ignored GPU 測試均已執行。

核心新增測試涵蓋 FIFO、舊 callback 快照、同事件跨 slot 一次通知、無效更新、REDRAW-only、卸載與重新掛載、巢狀 render 凍結、flush 期間排入的新更新、重入、mount cleanup、memo ancestor／共享 Binding consumer 失效、prop 快照 identity，以及 panic 後的下一次 build。TextArea 連續輸入／IME／外部受控更新另在 Legacy 與 RetainedAuto 各驗證一次。
