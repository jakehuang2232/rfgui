# State → build → render：A＋B

範圍：以前一版 `ddb7925` 為基準，補前段觀測、消除重複 reconciliation、縮小 incremental scroll 搬移。State 快照／batching、memo 失效策略、layout dirty 刷新和 renderer authority 維持原有語意。

## 觀測入口

開啟既有 `trace_render_time`，render trace 會列出：

- `state_frame`：進入當幀狀態邊界，包含尚未處理的 queue flush。
- `rsx_build`：App build 與 build bookkeeping；`unwrap` 是其內含細項，不額外加入總和。
- `scene_update`：reconciliation、placement 檢查／套用、patch translation、incremental／cold commit、focus／pre-layout animation 同步。
- 原有 layout、resource freeze、property sync、graph build、compile、execute、submit／present 階段。

`Viewport::frontend_profile()` 回傳最近一次嘗試的前段區間與工作量。`renderer_performance_sample()` 的既有十欄仍是 renderer-only，沒有改變欄位語意。Native window performance 輸出另外附上 `frontend_ms=[total,state,build,scene]` 和 `frontend_work`，可與既有 `wall_ms` 對照。直接呼叫 `render_rsx` 的 RSX 是呼叫端先建好的，因此這個入口的 `rsx_build_ms` 為零。

細項記錄在 tracing、unit test、`renderer-test-support` 或明確的 `ui::profile_ui_work(|| ...)` scope 中啟用。關閉時不讀取逐節點計時器；只有輕量啟用判斷。外部 scope 可以涵蓋事件 flush、build 與 render；巢狀 scope 回傳外層的累計觀察，Viewport 以進入時快照相減，避免上一幀數字滲入本幀。

`UiWorkProfile` 記錄實際處理的 state targets／actions／changed targets、component render、memo hit、unwrap visits、reconcile calls／visits／shared bailout、patches、FiberWork，以及 scroll 保存／恢復訪問數。它不代表 GPU 執行時間，也不代表 heap 配置量。

事件 batch 結束時已經完成的 flush 不屬於下一幀的 `state_frame`。要量事件到畫面的總成本，需把事件也放入明確觀測 scope。遞迴 unwrap 同類計時只計最外層，避免重複加總。

前段 trace 在進入 renderer 前凍結；renderer trace 在提交／呈現階段完成。呈現後的 hover/redraw 排程，以及 host event loop 本身，不包含在這份 trace 的總和內。

## 一次 reconciliation

上一棵樹與新樹共用同一 `Rc` 時直接跳過。其他情況只呼叫一次 `reconcile_multi`，不先做整樹 `PartialEq`，不因 placement 檢查失敗而再次 diff。

同一份 root-relative patches 先交給 placement 驗證，未接管時原封不動交給 incremental translator。Root reorder／結構修改走一般 translator；placement 只接受所有 patches 都是可安全套用的 style 更新，並保留 Fragment 路徑轉換。

空 patch 結果也會接受新樹作為上一幀快照，因此後續重畫可直接以該樹的指標跳過比對。

## Scroll 搬移邊界

- 純排序、插入、原生文字修改，以及保證不替換子節點的 props，不保存整棵 scene 的 scroll。
- Delete、ReplaceRootAt 只保存被移除的子樹。
- ReplaceNode 的索引可能被前面的工作改動，保守保存其 parent 子樹；重疊範圍去重。
- ReplaceAllRoots 保存整組 roots。
- `ElementTrait::prop_preserves_child_identity` 是 host 宣告的 apply/reset contract，預設保守。Image／SVG 的 loading/error 更新仍保存該 host 的子樹。未知 host 不會被誤認為無結構修改。
- 恢復時只查找已保存的非零 scroll owner，透過 arena 的 stable-id index 定位；一般更新不遍歷無關子樹。
- Incremental 部分失敗時，保存資料延續到 cold recovery，避免已刪除節點的 scroll 在第二次保存時消失。

這不變更原有 scroll 傳承的 stable-id 語意；完整 cold rebuild 仍使用既有整樹保存／恢復路徑。

## 重現 CPU 基準

```sh
cargo test -p rfgui --test frontend_pipeline_bench -- --ignored --nocapture --test-threads=1
```

不開 trace、不開 `renderer-test-support`，獨立執行。場景由 App 經實際 rsx/component 建立 128／512／2048 個 Row，每次更新 App 的 state，讓最後一個 Row 的值改變；30 次暖機後取 200 次。計時包含 setter、pending flush、App build、展開、reconcile、arena commit 及 headless frame 返回。

此 benchmark 沒有 surface，不包含 layout、paint、GPU 或 present。它用來檢查前段改善；不能以此宣稱完整幀或 GPU 加速。

## 本次驗證（2026-09-20）
基準為 `ddb7925`，比較對象為本次 A＋B 修改。測試在同一部 Apple M5 電腦，以相同 Cargo test profile 執行；未開 trace／renderer-test-support。修改前後各執行 5 輪，交錯次序，且不與編譯或 GPU 測試同時執行。每輪每種規模暖機 30 次、測量 200 次。下表是各輪 p50／p95 的中位數，不是將樣本混合後計算的 percentile。
| Row 數 | 修改前 p50 ms | 修改後 p50 ms | p50 降幅 | 修改前 p95 ms | 修改後 p95 ms |
|---:|---:|---:|---:|---:|---:|
| 128 | 0.117917 | 0.093875 | 20.39% | 0.135250 | 0.105292 |
| 512 | 0.455375 | 0.354958 | 22.05% | 0.509417 | 0.394000 |
| 2048 | 1.851666 | 1.440541 | 22.20% | 2.081583 | 1.599875 |

這是指定場景的前段 CPU 改善，不能外推為所有頁面或完整 GPU 幀的加速比例。

- `cargo test --workspace --lib --bins --all-features`：1,872 通過、102 ignored；新增 13 項測試。
- `cargo check --workspace --all-targets --all-features`：0 warnings、0 errors。
- `cargo check -p examples --target wasm32-unknown-unknown --all-features`：0 warnings、0 errors。
- `cargo test -p rfgui-components --test retained_controls -- --ignored --nocapture`：Apple M5／Metal 實際通過，15 控制項 × 2 renderer modes × 2 DPR × 9 幀 = 540 幀，包含像素、狀態更新及 backing reuse 檢查。沒有宣稱其他 ignored GPU 測試已全部執行。

新增回歸涵蓋 queue 實際工作數、巢狀計時不重複計算、panic 後 scope 回復、一般更新只 diff 一次且不搬 scroll、Fragment placement、root 排序、跨父節點移動、Image／SVG loading slot 替換、整組 roots 替換、incremental 部分失敗後 cold recovery、相同新樹的後續 pointer bailout，以及 App／直接 render 間觀測資料不殘留。

## 後續 C：狀態依賴索引

C 已將 memo 失效改為 target 層級的反向索引與每批去重標記；詳細生命週期、觀測與適用邊界見 [State dependencies](../../ui/state/DEPENDENCIES.md)。上面的 CPU 比較與測試數字是 A＋B 當時的紀錄，不是 C 的延遲量測。

## 後續 D：一般 component memo

D 已將正式 component 展開路徑接上 C 的 memo，並加入 children／context 檢查與 `#[component(no_memo)]`。語意、限制及 D 的獨立比較紀錄見 [Component memo](../../ui/component/MEMO.md)。

## E：dirty 觀察與量測結果重用

本批以前述 A～D 的未提交工作樹為基準，並保留 Legacy／RetainedAuto。沒有新增依賴，也沒有改 renderer selector 或 cache authority。

### 已落地的行為

- `NodeArena::refresh_subtree_dirty_cache` 以 subtree mutation revision 重用已觀察的 dirty flags 與 placement metadata。只有 host 宣告可追蹤、子孫也符合條件、父子連結一致時才建立證明；自訂 host 預設每次重新觀察。版本飽和時回到逐節點觀察。
- props／事件／動畫／layout／資源同步原有的可變借用及 topology API 會推進 mutation revision。arena dirty 清除與原生 Element／Text 的 bookkeeping 清除另使 dirty 證明失效，不把清除誤記為新的 paint 內容，也不提前消耗 render-change journal。
- Element、Text、TextArea、Image、Svg 採用上述契約。Text 尚待唯讀 preparation 填入共用 OnceCell 時保持即時觀察；Image／Svg 的外部資源仍在既有 frame 同步邊界進入 arena。
- Axis 與非 axis layout 的一般子節點 measure，先檢查既有原生量測結果是否仍有效。乾淨兄弟節點避免完整 measure 呼叫；axis layout 的 assigned dimension 清除仍經追蹤的可變借用執行。proposal、父層百分比尺寸、字型或子孫 layout dirty 改變時重新量測。Inline IFC 專用所有權流程保留。
- 原有 root box-model 快取改成直接附加到既有輸出 buffer，避免先複製出暫存 Vec。沒有建立新的跨幀幾何有效性來源；hit-test、clip、scroll、IME 仍沿用原本失效規則。

### 實際工作量與完整幀觀測

`ui::profile_ui_work` 新增 dirty observations／subtree reuses、native measure calls／measure reuses、place calls、box-model reads／reused snapshots、dirty-clear visits、render-change observations。這些是實際入口計數；measure call 包含進入後立刻返回的呼叫，不等於文字 shaping 次數。Layout trace 另外列出 layout work；前段 profile 的範圍仍止於 layout 之前。

重現入口：

```sh
cargo test -p rfgui --test frame_pipeline_bench --features renderer-test-support -- --ignored --nocapture --test-threads=1
```

基準包含 Binding 更新／flush、RSX 建立、reconcile、layout、paint、submit；`cpu_p50_ms` 是呼叫端 CPU wall time，含測試觀測與離屏 target 配置。`completed_p50_ms` 另外等候 GPU 完成，兩者都不是 GPU timestamp，也不含視窗 present。十欄 renderer CPU phases 保留既有定義。文字案例是 State 驅動的文字更新；原生輸入與 IME 另有下列 GPU 回歸。

2026-09-20，Apple M5／Metal、rustc 1.98.1、Cargo test profile（opt-level=1），同一份觀測程式編進修改前後兩版，各五輪交錯執行，量測期間沒有同時編譯或跑其他測試。每組 20 暖機＋60 測量幀，表中為五輪 p50 的中位數。所有數字只適用這個帶觀測的離屏列表場景。

| Renderer | 更新 | 列數 | CPU 前→後 ms | GPU 完成 wall 前→後 ms | layout 前→後 ms |
|---|---|---:|---:|---:|---:|
| Legacy | 顏色 | 128 | 0.4239 → 0.4163 | 0.7299 → 0.7095 | 0.0242 → 0.0229 |
| RetainedAuto | 顏色 | 128 | 0.5695 → 0.5771 | 0.8948 → 0.9178 | 0.0239 → 0.0229 |
| Legacy | 文字 | 128 | 0.7341 → 0.7166 | 1.0429 → 1.0135 | 0.2863 → 0.2825 |
| RetainedAuto | 文字 | 128 | 0.9098 → 0.9027 | 1.2625 → 1.2577 | 0.2815 → 0.2801 |
| Legacy | 捲動 | 128 | 0.6199 → 0.6232 | 0.9159 → 0.9183 | 0.0411 → 0.0401 |
| RetainedAuto | 捲動 | 128 | 1.0056 → 0.9964 | 1.4003 → 1.3862 | 0.0414 → 0.0403 |
| Legacy | 局部寬度 | 128 | 0.7225 → 0.7101 | 1.0219 → 1.0133 | 0.2808 → 0.2797 |
| RetainedAuto | 局部寬度 | 128 | 0.9219 → 0.9045 | 1.2744 → 1.2598 | 0.2831 → 0.2786 |
| Legacy | 顏色 | 512 | 0.8772 → 0.8622 | 1.1600 → 1.1570 | 0.0867 → 0.0816 |
| RetainedAuto | 顏色 | 512 | 1.3054 → 1.2869 | 1.6611 → 1.6324 | 0.0883 → 0.0808 |
| Legacy | 文字 | 512 | 2.1178 → 2.0869 | 2.4312 → 2.5590 | 1.1875 → 1.1722 |
| RetainedAuto | 文字 | 512 | 2.6210 → 2.6047 | 3.1505 → 3.1386 | 1.1790 → 1.1825 |
| Legacy | 捲動 | 512 | 1.2239 → 1.1925 | 1.5680 → 1.5365 | 0.1644 → 0.1593 |
| RetainedAuto | 捲動 | 512 | 1.9588 → 1.9258 | 2.4297 → 2.4019 | 0.1644 → 0.1605 |
| Legacy | 局部寬度 | 512 | 2.1010 → 2.0907 | 2.5321 → 2.4369 | 1.1728 → 1.1727 |
| RetainedAuto | 局部寬度 | 512 | 2.6330 → 2.5961 | 3.1613 → 3.1254 | 1.1815 → 1.1705 |

512 列場景的每幀工作量：

| 更新 | dirty 觀察前→後（Legacy／RetainedAuto） | measure 呼叫前→後 | 重用量測 |
|---|---:|---:|---:|
| 顏色 | 2050 → 530／514 | 0 → 0 | 0 |
| 文字 | 2050 → 1539／1539 | 513 → 2 | 511 |
| 捲動 | 2050 → 1025／1025 | 0 → 0 | 0 |
| 局部寬度 | 2050 → 1539／1539 | 513 → 2 | 511 |

工作量下降明確，但完整 CPU 幀的變化僅約 -1.3%～+2.6%（正數為改善），GPU 完成 wall time 也沒有一致加速；這些小幅延遲差異可能包含量測雜訊。本批不宣稱整幀大幅加速。這個列表每幀仍有 513 個 place 呼叫、1025 個 box-model 讀取、2050 個 dirty-clear visits、1025 個 render-change observations；parent solver、placement、dirty 清除、property sync／paint 仍有整樹工作，並未完成端到端 O(變動節點) 更新。

### 驗證與限制

- Workspace lib／bins all-features：1,911 通過、102 ignored；E 新增 10 項測試，覆蓋 unknown/shared host、dirty 消耗、journal 保留、搬移／移除、不一致連結、panic 還原、revision 飽和、局部文字、顏色、百分比尺寸與字型。
- Native workspace all-targets／all-features，以及 examples wasm32 all-features：均為 0 warnings、0 errors。
- 完整幀基準：16 組場景 × 80 幀 × 2 版本 × 5 輪 = 12,800 個實際 GPU 幀；四種更新均改變畫面。各版本內 Legacy／RetainedAuto 像素一致，另保存 32 份輸出，修改前後逐位元組相同。
- `retained_controls`：15 controls × 2 modes × 2 DPR × 9 幀 = 540 幀通過。
- 實際 GPU `native_single_viewport_text_area_caret_selection_ime_{artifact,legacy}`：2 tests／48 幀通過；`native_text_area_ime_event_lifecycle_stays_retained`：1 test／132 幀通過，兩條 renderer 與 DPR=1／2 均執行。
- 額外的密集 Flex 壓縮診斷仍失敗：128 列壓進 400px 高度時，Legacy／RetainedAuto 有 198,372 個 byte 不同。修改前與 E 版本都重現相同差異數；這不是 E 通過的 gate，也沒有修正或豁免。用 `RFGUI_BENCH_FLEX=1 RFGUI_BENCH_SAMPLES=2` 執行上述基準可重現，會回傳失敗。正式比較用可捲動的 Flow 列表。

本機證據：`/tmp/rfgui-e-comparison.json`、`/tmp/rfgui-e-{before,after}-round-{0..4}.log`、`/tmp/rfgui-e-{before,after}-pixels/`、`/tmp/rfgui-e-workspace-final.log`、`/tmp/rfgui-e-{native,wasm}-check.log`、`/tmp/rfgui-e-gpu-{controls,textarea,ime}.log`、`/tmp/rfgui-e-{before,after}-flex.log`。這些路徑是本次本機產物，不是版本庫中的固定 benchmark 結果。

## F0：失效原因與細項成本（2026-09-20）

F0 已完成觀測與定位；本批沒有改變 layout 重用條件、dirty 傳播、renderer selector 或 cache authority。此節為 F1 實作前的紀錄；F1 結果見下節。診斷資料由實際 measure／place／IFC／同步入口產生，候選數不當成成功重用數。

### 入口與數字語意

`renderer-test-support` 提供 `Viewport::set_renderer_diagnostics_for_test(true)`；成功離屏幀的 `RendererTestFrame::diagnostics` 包含：

- `phases_ms`：measure、place、box models 等包含子工作的區間。
- `exclusive_ms`：沿用 layout 計時堆疊，扣除已觀測子 scope，拆出 axis solver、IFC 收集／candidate／geometry／install、dirty clear、property sync、generation sync、change capture 等工作。
- `counts`：實際尺寸重新指派、placement 返回／執行、dirty 與輸入變更原因、IFC plan 重建原因，以及既有 property／generation／recording／planning 重用數。

預設關閉，逐節點 scope 不讀時鐘、不建立診斷輸出向量，仍有啟用判斷成本。啟用狀態由 scope 還原，包含 panic 路徑。原本 measure 完成後才重設 layout profile，會丟失 measure 原因；現改為 layout pass 開始時重設，box models 完成後取出，保留當 pass 的資料，避免下一 pass 沿用。

包含子工作的區間與 exclusive 明細不能相加；各欄 p50 也不能相加當成某幀的總時間。未包 scope 的程式與部分觀測成本仍未歸屬。`axis_solve` 是求解函式扣除已觀測子 scope 後的時間，不是純數學運算時間。placement／IFC 原因可能重疊；`assignment_dirty_previously_clean` 只看該節點自己的 placement／paint flags，不保證子孫乾淨。`assignment_dirty_same_placed_size` 是逐軸 bitwise 相等的觀察，不能直接當作 transition、百分比尺寸或 IFC 內容可重用的證明。

`ifc_measure_full` 表示重新建立幾何／plan，不能解讀成文字重新 shaping。`ifc_candidate_calls` 包含 cache lookup 與 package distribution；`ifc_candidate_rebuilt` 才是該 candidate 的底層 IFC context cache miss。此次 benchmark 在兩個值之間切換，暖機後兩者均已快取，因此不能推論首次輸入新字串的 shaping 成本。

重現：

```sh
RFGUI_BENCH_DIAGNOSTICS=1 RFGUI_BENCH_IDLE=1 RFGUI_BENCH_SAMPLES=60 \
  cargo test -p rfgui --test frame_pipeline_bench --features renderer-test-support \
  -- --ignored --nocapture --test-threads=1
```

關閉診斷時移除 `RFGUI_BENCH_DIAGNOSTICS`，不要設成 `0`（此開關看變數是否存在）。`RFGUI_BENCH_IDLE=1` 新增「state 仍切換、但輸出 RSX 不變」場景，不等於 host 完全休眠。既有 `RFGUI_PROFILE_PAINT=1` 可另外列出 retained build 細項，應獨立執行，不能把印出逐幀資料的時間用作正式效能基準。

### 已確認的失效傳播

512 列 Flow 列表、局部文字或局部寬度更新：

1. 父層 measure 執行一次 axis solver；E 讓 511 個乾淨兄弟節點重用 measure 結果，但仍依原有語意清除 `layout_assigned_width/height`（`layout/measure.rs::measure_child_if_needed`、`element/layout_trait.rs::clear_measure_assignment`）。
2. 父層 place 重新指派尺寸，512 次指派都與先前已放置尺寸相同；`set_layout_width/height` 比較的是已清空的 assignment Option，於是呼叫 `mark_place_dirty`。
3. `element/impl_core.rs::mark_place_dirty` 同時標記 placement、paint、composite。511 個兄弟節點因此無法通過 IFC 的 paint-clean 檢查。
4. `run_inline_ifc_root_after_place` 重建這 511 份原可沿用的 plan；加上有改動的列，每幀共有 512 次 candidate 更新與 IFC install。property sync 觀察整棵 1,025 節點，generation 觀察重用為零。

這條路徑在 Legacy／RetainedAuto 都成立。E 已避免大部分 measure 呼叫，卻沒有解除後續 assignment bookkeeping 造成的 paint 失效。僅以「尺寸相等」跳過所有 dirty 仍不安全；文字、字型、paint、clip、transition、資源世代等有效性必須分別保留。

512 列、RetainedAuto、每個測量幀的實際工作：

| 更新 | measure 呼叫 | 實際執行 place | clean 返回 | 相同尺寸重新指派 | IFC install／沿用 plan | paint dirty 引起 plan 重建 | property 觀察／重用 |
|---|---:|---:|---:|---:|---:|---:|---:|
| 顏色 | 0 | 2 | 511 | 0 | 1／0 | 1 | 514／511 |
| 文字 | 2 | 513 | 0 | 512 | 512／0 | 511 | 1025／0 |
| 捲動 | 0 | 513 | 0 | 0 | 512／512 | 0 | 1025／0 |
| 局部寬度 | 2 | 513 | 0 | 512 | 512／0 | 511 | 1025／0 |
| 輸出不變 | 0 | 0 | 0 | 0 | 0／0 | 0 | 1／1024 |

所有活動場景都呼叫 513 次 place，但顏色場景有 511 次立即返回；因此不能把入口次數全當作重新配置。Flow 在既有 replay 的 layout 類型檢查即被排除，每幀 512 次；這是尚不支援的條件，不代表移除檢查便能安全重用。捲動確實改變 512 列的 placement 輸入，且已沿用全部 IFC plans；不可把這些工作都歸成無效重排。輸出不變時已有 root box-model 與 generation 重用，但仍有 1,025 次 dirty-clear visits、1,025 次 render-change observations。

### 成本與量測負擔

Apple M5／Metal，同一 Cargo test profile（opt-level=1）。診斷 off／on 各三輪交錯執行；每組 20 暖機＋60 測量幀，128／512 列、五種更新、兩個 renderer，共 9,600 幀。量測期間沒有編譯或其他測試。以下數字是各輪 p50 的中位數。

512 列 RetainedAuto，關閉細項診斷的完整 CPU 幀／layout／property sync／render build：

| 更新 | 完整 CPU ms | layout ms | property sync ms | build ms | 開啟診斷的整幀增幅 |
|---|---:|---:|---:|---:|---:|
| 顏色 | 1.2381 | 0.0831 | 0.2312 | 0.4384 | 8.3% |
| 文字 | 2.5500 | 1.1364 | 0.3634 | 0.5326 | 11.9% |
| 捲動 | 1.8614 | 0.1564 | 0.3573 | 0.6741 | 12.9% |
| 局部寬度 | 2.5515 | 1.1386 | 0.3643 | 0.5445 | 12.3% |
| 輸出不變 | 0.4946 | 0.0047 | 0.0404 | 0.1503 | 6.2% |

診斷開啟時，文字場景的 axis solver exclusive 約 0.0049 ms，IFC candidate／geometry／install exclusive 分別約 0.3146／0.3756／0.2669 ms。主要改善機會是避免無效 IFC plan 工作；不能依入口名稱把 1.1 ms layout 全歸給 solver。計時介入會增加成本，這些細項只用於定位，F1 加速比須在診斷關閉時重新量測。

獨立的 paint-detail run（1,600 幀）顯示 512 列 RetainedAuto 的顏色／捲動場景仍花費時間於 `subtree_recording_key`（約 0.1000／0.1312 ms）、`walk_manifest`（約 0.0993／0.1081 ms）及 recording context／capability 觀察。輸出不變時 `walk_manifest` 約 0.0732 ms。這些是開啟 paint profiler 的 exclusive 數據，支持後續分析記錄與觀察遍歷，不證明整幀 raster 或 GPU 時間相同。

另外以 E binary／F0 診斷關閉的 binary，各三輪交錯比較原本四種更新（7,680 幀）。16 組場景的 CPU p50 差異為 −0.79%～+1.83%，全部工作量相同；目前沒有顯著加速或退化的證據，也不宣稱觀測成本為零。

### F1 範圍與驗收

F1 優先處理 assignment 的暫存清除／回填與實際 layout 輸出變更的區分，避免無變動兄弟節點被擴大標成 paint dirty；保留所有真正內容、資源與幾何變更的失效路徑。先不擴大為全面 Flow placement replay、renderer 重寫或新一層 cache。

- 在上述局部文字／尺寸場景，讓 511 個無變動兄弟的 IFC plan 確實重用，並量到 property／generation 觀察縮減；只減少 measure 呼叫不算完成。
- 同時驗證首次新字串、換行／內在高度改變、父尺寸／百分比／字型變更、Flex／Flow 軸向與 gap、transition、clip／scroll、插入刪除，以及資源更新。相同尺寸不能掩蓋真正的 paint dirty；未知 host 保留保守路徑。
- 比對 Legacy／RetainedAuto 畫面，重跑 TextArea／IME／caret 及既有 controls。效能使用診斷關閉、交錯多輪的完整幀；逐幀觀測與 GPU 完成 wall time 分別報告。
- F1 後才依剩餘成本安排 F2 的 box models、dirty clear、journal／property 局部化，以及 retained recording 遍歷。密集 Flex 壓縮的既有像素差異仍另案處理，不放寬或豁免。

### F0 驗證

- Workspace lib／bins all-features：1,914 通過、102 ignored，新增三項診斷測試（disabled path、巢狀 scope panic 還原、跨 pass 重設與 measure 原因保留）。新字串 cache miss 也有實際斷言，不以暖機後 miss=0 取代驗證。
- Native workspace all-targets／all-features、examples wasm32 all-features：0 warnings、0 errors。
- 三輪診斷 off／on：60 組成對場景的既有工作量計數全部相同，40 份保存畫面逐位元組一致；32 份既有 E 輸出與 F0 也逐位元組一致。各 run 內均檢查 Legacy／RetainedAuto 像素一致，活動場景確實改變畫面，輸出不變場景確實不變。
- 這次沒有重跑上節 E 的 540 controls／180 TextArea、IME 幀，也沒有宣稱全部 ignored GPU 測試已通過。

本機證據：`/tmp/rfgui-f0-comparison.json`、`/tmp/rfgui-f0-default-comparison.json`、`/tmp/rfgui-f0-{off,on}-round-{0..2}.log`、`/tmp/rfgui-f0-{off,on}-pixels/`、`/tmp/rfgui-f0-default-{e,f0}-{0..2}.log`、`/tmp/rfgui-f0-paint-detail.log`、`/tmp/rfgui-f0-workspace.log`、`/tmp/rfgui-f0-{native,wasm}-check.log`、`/tmp/rfgui-f0-profile-final.log`。本批保留 A～E 未提交更動，尚未 commit。

## F1：相同尺寸指派回填（2026-09-20）

F1 實作完成。基準為上述 F0 未提交工作樹；保留 A～F0 的修改，尚未 commit。本批沒有擴大 Flow placement replay，也沒有改 renderer authority、State 快照或加入依賴。

### 重用條件與失效邊界

`measure_child_if_needed` 通過原有 `can_reuse_measure_output`（proposal、自己的 layout dirty、子孫 layout dirty）後，原生 Element 清除 assignment 時暫存先前兩軸的 `Option<f32>`。只有已完成過 placement、沒有新的 layout／placement dirty、沒有 active layout transition runtime 的節點能建立這份紀錄。

父層一般 axis placement 算好 main size 與 stretch 條件後，先比較「接下來兩軸會持有的 assignment」與先前紀錄。兩軸包含 `None`／`Some` 都必須 bitwise 相同；成功才回填 assignment，接下來原有 setters 看到值未變，便不會額外呼叫 `mark_place_dirty`。原有 paint dirty 完整保留，offset、clip、IFC、子孫 dirty 與 placement 入口仍照常檢查。

紀錄只可消耗一次。實際 measure、一般 place 或既有 Flex replay 都會丟棄它；新的 layout／placement dirty 或 transition runtime 會拒絕回填。取消 cross stretch、換軸、不同尺寸都回到原本 setter 失效路徑。未知 host 不建立紀錄。這份紀錄只描述量測時暫時移除的指派來源，不是新的幾何或 paint cache。

新增 `assignment_restores` 計數；其意義是成功回填一個節點的完整兩軸指派，不是跳過整個子樹繪製。未變動兄弟會在原有 place early-return 保留 IFC plan，不會增加 `inline_ifc_root_install_reuse_calls`，因為根本不需再次進入 install。

512 列、局部文字／局部寬度更新，每個測量幀的工作量：

| 工作 | F0 | F1 |
|---|---:|---:|
| measure 呼叫／重用 | 2／511 | 2／511 |
| assignment 成功回填 | 0 | 511 |
| assignment 新增 dirty | 512 | 1 |
| place 入口呼叫 | 513 | 513 |
| 實際執行 place／clean 返回 | 513／0 | 2／511 |
| IFC candidate／install | 512／512 | 1／1 |
| 兄弟節點因 paint dirty 重建 plan | 511 | 0 |
| RetainedAuto property 觀察／重用 | 1025／0 | 514／511 |
| RetainedAuto generation 觀察重用 | 0 | 511 |
| Legacy property 觀察／重用 | 1025／0 | 530／495 |
| Legacy generation 觀察重用 | 0 | 495 |

仍有全樹工作：每幀 1,025 次 box-model reads、2,050 次 dirty-clear visits、1,025 次 render-change observations。F1 不宣稱更新已變成端到端 O(變動節點)。

### 完整幀比較

Apple M5／Metal、Cargo test profile（opt-level=1），F0／F1 各五輪交錯執行，細項診斷與 paint profiler 關閉，量測期間沒有編譯或其他測試。128／512 列 × 五種更新 × 兩種 renderer，每組 20 暖機＋60 測量幀，共 16,000 GPU 幀。以下為每輪 p50 的中位數；CPU 包含 State 更新、build、layout、render、submit 與離屏測試成本，completed 另等待 GPU 完成，皆不是 GPU timestamp 或視窗 present。

| Renderer | 更新 | 列數 | CPU F0→F1 ms | CPU 降幅 | GPU 完成 wall F0→F1 ms | layout F0→F1 ms |
|---|---|---:|---:|---:|---:|---:|
| Legacy | 文字 | 128 | 0.7405 → 0.4505 | 39.2% | 1.0226 → 0.7304 | 0.2935 → 0.0360 |
| RetainedAuto | 文字 | 128 | 0.9329 → 0.6080 | 34.8% | 1.2541 → 0.9157 | 0.2954 → 0.0359 |
| Legacy | 局部寬度 | 128 | 0.7394 → 0.4487 | 39.3% | 1.0197 → 0.7295 | 0.2920 → 0.0345 |
| RetainedAuto | 局部寬度 | 128 | 0.9400 → 0.6190 | 34.2% | 1.2576 → 0.9242 | 0.2951 → 0.0344 |
| Legacy | 文字 | 512 | 2.1209 → 0.9117 | 57.0% | 2.5926 → 1.1762 | 1.1935 → 0.1162 |
| RetainedAuto | 文字 | 512 | 2.6736 → 1.3520 | 49.4% | 3.2011 → 1.6867 | 1.2029 → 0.1179 |
| Legacy | 局部寬度 | 512 | 2.1206 → 0.9135 | 56.9% | 2.4359 → 1.1815 | 1.1894 → 0.1162 |
| RetainedAuto | 局部寬度 | 512 | 2.6914 → 1.3576 | 49.6% | 3.2237 → 1.6909 | 1.2059 → 0.1170 |

顏色、捲動、輸出不變場景的 CPU 變化約 −1.1%～+1.0%（耗時增減），未見明顯改變。512 列 RetainedAuto 文字場景的 property sync 約 0.3687 → 0.2483 ms、build 約 0.5759 → 0.4723 ms；兄弟 paint 失效減少，也縮小後續同步工作。效益限於這些列表場景，首次新文字 shaping 成本不由暖機後兩值切換的 benchmark 代表。

### 驗證與仍未通過的診斷

- Workspace lib／bins all-features：1,919 通過、102 ignored。新增五項單元回歸，包含完整指派比對、取消 stretch／換軸／尺寸改變、保留 paint dirty、transition／新 layout dirty、重複 measure 及一次性紀錄，以及真正新字串只重建變動列的 IFC。
- Native workspace all-targets／all-features、examples wasm32 all-features：0 warnings、0 errors。
- 正式 benchmark 每個 run 都檢查 Legacy／RetainedAuto 像素一致；保存的 40 份 F0／F1 輸出逐位元組相同。另有一輪 1,600 幀細項診斷確認上述工作量，這一輪不算入效能比較。
- 新增 `layout_assignment_regression::incremental_assignment_matches_cold_geometry_and_pixels` 實際通過 896 GPU 幀：Flow／Flex × row／column，DPR=1／2，七種更新（新文字、換行與內在高度、父尺寸與百分比、字型、paint、插入刪除、stretch 切換），每個步驟分別確認 Legacy／RetainedAuto 的增量畫面與該 renderer 全新建立的畫面逐位元組一致。這項驗證不代表兩個 renderer 在全部額外場景互相一致。
- `retained_controls`：540 幀通過；TextArea caret／selection／IME：48 幀通過；IME lifecycle：132 幀通過。既有資源 slot／unknown host／transition 回歸也包含在 workspace 單元測試中；本批未增加異步資源 GPU 測試。
- 新增獨立 `intrinsic_assignment_renderer_parity` 診斷仍失敗：Flow row、auto intrinsic width、stretch，第一個畫面在 DPR=1 有 352 bytes 不同。從 F0 原始檔建立的獨立命名測試 binary 也在相同初始場景得到 352 bytes；此時尚未發生 assignment 回填。測試保留實際失敗，沒有忽略差異、容忍門檻或 renderer fallback 豁免。
- 既有密集 Flex 診斷也重新執行：F0／F1 均為 198,372 bytes 不同。上述兩項是 renderer parity 的未解問題，不列入通過數；F1 並未完成所有 renderer parity 驗收。

重現新增驗證：

```sh
cargo test -p rfgui --test layout_assignment_regression --features renderer-test-support \
  incremental_assignment_matches_cold_geometry_and_pixels -- --ignored --nocapture --test-threads=1
# 目前會失敗的獨立 renderer parity 診斷：
cargo test -p rfgui --test layout_assignment_regression --features renderer-test-support \
  intrinsic_assignment_renderer_parity -- --ignored --nocapture --test-threads=1
```

後續（2026-10-11）：上述兩項 renderer parity 差異都不再重現。`intrinsic_assignment_renderer_parity` 896 幀逐位元組一致；密集 Flex 的 color、text、size 也逐位元組一致，剩下的失敗只是列被壓到捲動或改字都不改變像素。準備 Legacy 退場時刪除了這個診斷，上面第二個指令已無法執行。

下一步維持 F2 的 box models、dirty clear、journal／property 局部化，但應依 F1 後剩餘成本選第一個窄範圍；兩項 renderer parity 問題另案修正，不把它們當成已驗收。F1 不延伸為這些後續更動。

本機證據：`/tmp/rfgui-f1-comparison.json`、`/tmp/rfgui-f1-{before,after}-round-{0..4}.log`、`/tmp/rfgui-f1-{before,after}-pixels/`、`/tmp/rfgui-f1-diagnostics.log`、`/tmp/rfgui-f1-cold-regression.log`、`/tmp/rfgui-f1-gpu-{controls,textarea,ime}.log`、`/tmp/rfgui-f1-{reference,intrinsic}-parity.log`、`/tmp/rfgui-f1-reference-unique-build.{jsonl,log}`、`/tmp/rfgui-f1-{before,after}-flex.log`、`/tmp/rfgui-f1-workspace-final.log`、`/tmp/rfgui-f1-{native,wasm}-check-final.log`。
