# C：依狀態槽追蹤 memo 依賴

這一批接續 A＋B 的前段觀測與 reconciliation 改善，修改 State、Binding、GlobalState 的 memo 失效策略。更新仍先排隊；事件 batch 結束或下一個最外層 frame 開始時提交，當次 render 與舊 callback 各自保留既有快照。

## 身分與依賴

每個 `BindingPropPayload` 配一個單調遞增的 `StateTargetId`。State 衍生的 Binding、GlobalState 衍生的 Binding、clone、snapshot 與 Binding prop 轉換都保留同一個 target；同一元件的不同 hook slots 有不同 target。卸載後重新建立的 slot 配新 target，舊 setter 仍由原有 lifetime gate 拒絕。

讀取值、取得新 snapshot、取得 GlobalState、產生 Binding 或轉換 Binding props 時，若正在 memo render，就登記 target。單純 callback 在 render 外讀取不會建立依賴。`get_committed` 在 render 中被呼叫時也會登記；它仍是原生輸入 bridge 的已提交值讀取入口，不會讀 pending actions。

`MemoEntry` 保留正向依賴，StateStore 維護兩組反向索引：

- `target_consumers`：target → 讀取它的 memo，以及包含其已解析輸出的 memo 祖先。
- `component_memos`：狀態 owner → 自身 memo，以及包含該元件輸出的 memo 祖先。自己的 state 即使未讀值，仍需要讓 owner 重新 render。

cache hit 重播既有依賴到外層 frame；成功 render 時移除舊邊並安裝新邊。因此條件式讀取從 A 切到 B 後，更新 A 不再失效該 memo。卸載移除所有相關邊與空索引項。每次 build 的既有生命週期清理仍會掃描 cache；state flush 不再掃描整份 cache。

## 批次提交

先拆出所有 target 的 queue，再依序執行 updater。updater 中新增的更新繼續留到下一批。所有 target 值提交後，合併 changed targets 的讀取者與去重後的 owners／祖先，對每個受影響的 memo 標記一次 dirty。

GlobalState 與獨立 Binding 不再清空 cache。最終值相同的 target 不加入失效；`REDRAW` 不升級成 `REBUILD`。Viewport pointer state 使用原有 subscriber hooks 經相同 owner 索引失效，保留 hook 外無關 memo。

失效時暫留舊 entry 與依賴，只禁止 cache hit。成功 render 後替換，或卸載時移除。這讓 updater／render panic 不會讓已失效的畫面重新命中；已提交的前半批更新，即使後續 updater panic，也會在 unwind 時完成失效標記。這不是整批 rollback。

退休的 memo props／node 在 StateStore borrow 結束後才釋放，避免使用者 destructor 重入時發生 borrow panic。

## 觀測與成本

`UiWorkProfile` 新增：

- `memo_invalidation_visits`：每次失效合併後，實際訪問的不同 memo 數。
- `memo_invalidations`：由可重用轉成 dirty 的 memo 數。跨批仍 dirty 的 memo 可再次被訪問，但不重複計入轉換。

這兩個數字也出現在 `state_frame` trace 與 window performance profile。事件結束時已完成的失效，需用外層 `profile_ui_work` 才能連同事件一起量到。

flush 成本取決於變動 target 的索引 fan-out 與受影響 memo；索引邊仍可能指向相同祖先，先合併才標記。代價是額外的反向索引儲存空間，以及 memo miss／卸載時維護索引的成本。這批沒有宣稱完整幀或 GPU 時間的改善比例。

## 驗收案例

`dependencies/tests.rs` 直接經 build、state queue 與 memo render 驗證：

- 同一 owner 的不同 slots、GlobalState、獨立 Binding 不牽連無關讀取者。
- 同批兩個 targets 共用一個 consumer，提交後只訪問／失效一次。
- 256 個 memo 中只更新一個 target：1 次失效訪問，下一次 build 255 次 memo hit。
- 條件依賴與 props 切換、cached child 依賴向祖先傳播、owner 未讀 state。
- 卸載清除索引、remount target 不重用、舊 setter 不影響新 slot。
- 無有效變化、REDRAW-only、updater／render panic、Binding prop identity。
- Pointer state 訂閱者失效、退休 props destructor 可讀 StateStore。

這些是工作量與行為驗證，不是延遲 benchmark。原有 queue tests 繼續驗證 FIFO、固定快照、舊 callback、提交期間新增更新與事件通知。

## 邊界

這批只改善現有 `render_memoized_component` 的失效機制。C 完成時一般 `#[component]` 尚未自動套用 memo；後續 D 已補上正式元件路徑、props 比較與 context 依賴，見 [Component memo](../component/MEMO.md)。C 的測試與下列歷史驗證數字本身，不代表 D 的驗收。

## 本次驗證（2026-09-20）

以下結果包含工作樹中既有的 A＋B，以及本次 C 修改：

- `cargo test --workspace --lib --bins --all-features`：1,887 通過、0 失敗、102 ignored；C 新增 15 項回歸。
- `cargo check --workspace --all-targets --all-features`：0 warnings、0 errors。
- `cargo check -p examples --target wasm32-unknown-unknown --all-features`：0 warnings、0 errors。
- `cargo test -p rfgui-components --test retained_controls -- --ignored --nocapture`：Apple M5／Metal 實際通過。15 控制項 × Legacy／RetainedAuto × 2 DPR × 9 幀 = 540 幀，驗證狀態切換、像素與 backing reuse。其他 ignored GPU 測試不在本次已執行範圍。

GPU 控制項回歸驗證共用 state 路徑；精準 memo 失效與減少 render 的直接證據來自上述 dependency 測試，兩者不可互相代替。
