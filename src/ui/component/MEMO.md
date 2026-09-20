# D：正式 component 路徑的 memo 與 context 依賴

一般 `#[component]` 現在會經由 `unwrap_components` 使用 C 的 memo cache。當 props、children、已讀取的外部 context 都不變，且沒有 state 失效時，直接回傳上一次完成 identity／host descriptor 處理的共享輸出，跳過 component render 與子樹展開。

## Props 與 children

- 函式形式：macro 在呼叫端產生逐欄比較。型別有 `PartialEq` 時使用它；不支援比較時回傳不相等。既有 props 不必新增 trait bound。
- 泛型欄位依函式宣告的 bounds 決定；`T: PartialEq` 可以比較，只有 `T: Clone` 時保守重新 render，即使某次具體型別有 `PartialEq` 也不做額外 specialization。
- `impl RsxTag` 形式：比較整個 `StrictProps`；有 `PartialEq` 才能命中，否則保守重新 render。
- `Handler` 既有的 callback identity 比較會偵測 handler 更換；未實作 equality 的 `Rc<dyn Fn...>` 不會誤判為相同 callback。
- children 是獨立輸入：數量相同，且每個 child 共用原本節點，或是相等的文字節點，才允許重用。不為了 memo 遞迴比較新建的 children 樹。
- Binding／State props 的 equality 繼續比較 target 與 snapshot 身分；即使傳入舊 handle，render 中讀到的 target 仍由 C 追蹤。

比較函式只借用具有相同 TypeId／vtable 的 props。cache 以 `Rc<ComponentNodeInner>` 保有 boxed props；miss 時透過既有 clone shim 產生 render 使用的所有權，hit、替換、卸載或 panic 都由 RAII 釋放。runtime、trait 與 macro 皆未引入具體 component type 分支。

## Context

`use_context<T>` 會記錄讀取的 publication，包含「沒有 Provider」的情況。快取命中前，先與當前最近一層同型別 Provider 比對：加入、移除或替換 publication 都會阻止重用。

context value 仍只要求 `Clone + 'static`，因此以 publication allocation 身分保守比較。重新呼叫 `provide_context_node` 即為新的 publication；即使值看起來相同，也允許 consumer 重新 render。重用同一個 Provider node／raw publication 則可保持身分。這批沒有要求所有 context value 新增 `PartialEq`，也沒有宣稱相同值的全新 publication 能自動命中。

每次 push 配一個 epoch，memo frame 保存進入時的 boundary。讀取者只把 frame 外已存在的 Provider 記為該 memo 的輸入；frame 內建立的 Provider 屬於其輸出。因此內層遮蔽不會錯誤依賴外層同型別值。cached child 命中時會把外部 context 讀取重播給當前 ancestors；條件分支不再讀取的依賴會隨下一次成功 render 移除。

context 本身沒有 setter 排程；host 仍需安排 build。context 裡的 Binding 使用 C 的 target 通知。publication 應視為不可變快照；原地修改未追蹤的 interior data 不會產生通知。

## Render contract 與停用方式

可重用元件的輸出應由 props、children、state、context 決定。render 外部副作用應放在事件或 mount／cleanup hook。對於必須直接讀取外部可變資料的元件，使用：

```rust
#[component(no_memo)]
fn ExternalView() -> RsxNode {
    // Read external data for every requested build.
    // ...
}
```

這個標記也支援 `impl RsxTag` 形式，並阻止包含它的已解析 ancestor cache 跳過該 render。沒有自動偵測任意外部可變資料的機制。

命中時仍重播 state slots、GlobalKey、timer、mount 與 viewport pointer hook 的存活資訊；state 更新仍穿過已快取的 ancestors。panic 後 dirty entry 不會恢復成有效快取。host descriptor 在存入 cache 前完成，避免命中後再以 `Rc::make_mut` 破壞共享身分。

## 驗證範圍

`memo/tests.rs` 直接透過真正的 macro、lazy component walker 與 C 的 scheduler，驗證 256 個 Rows 只改一個時的 render／hit 數、共享輸出、children、callback／不可比較 props、泛型與手寫 impl、state／mount／cleanup、舊 setter、外部資料 opt-out、context 新增／替換／移除／遮蔽／依賴切換、cached child 重播，以及 erased props 的 panic／卸載釋放。

效能基準沿用 `tests/frontend_pipeline_bench.rs`：它包含 setter、flush、App build、component expansion、reconcile、arena commit；不包含 layout、raster、GPU 或 present。GPU 回歸另行驗證 Legacy 與 RetainedAuto，不能用前段 CPU 時間代替像素或 backing reuse 證據。

## 本次驗證（2026-09-20）

工作樹包含原有 A＋B＋C 與本次 D；尚未 commit。

- `cargo test --workspace --lib --bins --all-features`：1,901 通過、0 失敗、102 ignored；D 新增 14 項回歸。
- `cargo check --workspace --all-targets --all-features`：0 warnings、0 errors。
- `cargo check -p examples --target wasm32-unknown-unknown --all-features`：0 warnings、0 errors。
- `cargo test -p rfgui-components --test retained_controls -- --ignored --nocapture`：Apple M5／Metal 通過。15 控制項 × 2 renderer modes × 2 DPR × 9 幀 = 540 幀，Legacy／RetainedAuto 各 270 幀。其他 ignored GPU tests 不在本次已執行範圍。

本機 Xcode linker 在驗證中途回報尚未接受 license，後續指令使用既有 `DEVELOPER_DIR=/Library/Developer/CommandLineTools`。未接受任何授權或變更系統設定。以下比較的兩個版本都以此工具鏈編譯。

## 相對 A＋B＋C 的 CPU 比較

Before 是開始 D 前保存的完整 source snapshot（含尚未提交的 A＋B＋C）；After 是本次 D。兩者使用相同 `frontend_pipeline_bench.rs`、Cargo test profile、相同 machine；均未開 trace／renderer-test-support。分別建立執行檔後才進行測量，不與編譯或 GPU 測試同時執行。

共 5 輪，交替 Before→After／After→Before。每輪每種規模暖機 30 次、量測 200 次。表格是各輪 p50／p95 的中位數，非混合所有樣本後的 percentile。

| Rows | Before p50 ms | D p50 ms | p50 降幅 | Before p95 ms | D p95 ms |
|---:|---:|---:|---:|---:|---:|
| 128 | 0.093708 | 0.040459 | 56.82% | 0.107209 | 0.046375 |
| 512 | 0.358354 | 0.147792 | 58.76% | 0.414750 | 0.167917 |
| 2048 | 1.474979 | 0.622146 | 57.82% | 1.648417 | 0.757750 |

這個場景的 Row 只有可比較的數值 props，能穩定重用未改變的 Row。不能外推為 callback 每幀重新建立、Provider publication 每次替換或大型 children 更新時的改善比例。memo 也增加 cached inputs／outputs、依賴索引的儲存與維護成本；這批沒有量 heap 配置量或 GPU 加速比例。
