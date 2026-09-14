# Inspector build_graph 修正結果 — 2026-09-15

三批修正已完成並獨立提交。在本輪固定小幅拖曳重播中，artifact 的 build_graph 中位數由 7.673 ms 降至 2.962 ms（約 61%）；完整 offscreen frame CPU 中位數由 13.006 ms 降至 7.395 ms（約 43%）。這不是對實際視窗 present 延遲的加速承諾。

**main 與提交邊界**

原本 main（b967a24，領先 origin/main 一個 commit）有 79 個已追蹤檔案修改、39 個新增檔案，共 118 個變更檔案。目前隔離 worktree 原先乾淨。所有非忽略的既有來源已完整複製並建立基線，原本 `/Users/jakehuang/Work/rfgui` 沒有被提交、覆寫或 reset；757 個來源檔案逐一 SHA-256 核對，內容與檔案集合均未變。基線 checkpoint 不表示這 118 個檔案已經逐一 review。

| commit | 內容 |
|---|---|
| 0c422b7 | 既有 WIP checkpoint（118 個檔案） |
| 329f332 | Image／Svg 靜態 loading、error slot 重用與 host scope |
| accb8e7 | 相同 native hover 狀態不記錄 mutation |
| b138789 | TextArea 重複 projection 依出現順序配對 |

**實作與保護條件**

- Slot 只對可完整比較的原生 Element／Text／Fragment 靜態輸入重用。Style 以值複製；callback、未知 host、其他 shared 輸入維持替換。重用也比較繼承樣式與 viewport 條件，並通過原有 root 存活、歸屬、alias 與 active children mirror 驗證。真正變更仍替換，沿用 cold path 的 host scope。全域 SharedPropValue equality 沒有改動。
- EventTarget 新增保守的 `hover_update_needed` 查詢。原生 host 確認 no-op 才略過 mutable arena access；未知 host 預設仍呼叫 setter 並記錄 mutation。每次仍遍歷目前的樹，處理新節點、祖先 hover 與狀態重置。
- TextArea 同 identity 的候選從 stack 改為 FIFO queue。未設 key 的重複節點依 occurrence 配對；明確 key 仍依自己的 bucket 移動。沒有排序 render chunks，也沒有放寬 graph-order validation。
- renderer witness、fail-closed fallback、cache validation、residency checks 均保留；沒有新增 dependency、平台耦合、macro component 特例或 S/T/E production 分支。

**量測方式與限制**

Apple M5／Metal；dev/test optimized + debuginfo profile；logical 1280×800、DPR 2、Rgba8Unorm。使用完整素材、1,900 個 live nodes，動畫關閉、Inspector debug panel 展開、計時 UI 的數值更新關閉。每個 process 150 幀：40 warmup、20 idle、20 空白 PointerMove、10 press、60 drag。state dirty 時重新建立 MainScene，並斷言 Inspector 的實際位置。

四個 checkpoint 各有 artifact／直接 Legacy 三輪，交錯順序 artifact、legacy、legacy、artifact、artifact、legacy；共 3,600 個正式重播幀。每個 checkpoint artifact 450／450 幀確實選 artifact，Legacy 450／450 幀確實選 Legacy，均無 fallback。沒有同時執行 Cargo 或另一個 GPU benchmark。額外 queue wait 在計時之外。像素讀回與 profile 分解另外執行，不混入效能統計。

以下採用本輪重新量測的 baseline；先前診斷的 8.76 ms 與本輪 7.67 ms 不混算。中間批次仍受系統／時段變異影響，不能把每批差值相加當成獨立因果成本。

| checkpoint | artifact idle | artifact 空白 move | artifact drag | Legacy drag |
|---|---:|---:|---:|---:|
| WIP 基線 | 0.879 | 4.453 | 7.673 | 0.354 |
| 第一批 | 0.872 | 4.400 | 7.294 | 0.356 |
| 第一＋二批 | 1.090 | 1.092 | 9.331 | 0.399 |
| 三批合併 | 0.852 | 0.850 | 2.962 | 0.346 |

單位 ms，中位數。第一＋二批的拖曳尚未改善，因此未把它當作通過拖曳效能的證據。

| artifact 項目 | 基線中位數 / P95 | 合併後中位數 / P95 |
|---|---:|---:|
| 空白 move build_graph | 4.453 / 7.613 | 0.850 / 0.867 |
| 拖曳 build_graph | 7.673 / 7.904 | 2.962 / 4.909 |
| 拖曳 frame CPU | 13.006 / 13.426 | 7.395 / 9.440 |

合併後三輪拖曳 build_graph 中位數分別為 2.936、2.961、2.986 ms。Legacy 拖曳 frame CPU 中位數 7.952 → 7.483 ms；其 build_graph 約維持 0.35 ms。

拖曳每幀新 NodeKey 12 → 0；空白 PointerMove mutation 1,887 → 1；拖曳事件 mutation 1,890 → 4。剩下的保守更新沒有被強制歸零。

額外 profile 的末幀（frame 149）顯示：stable-id index bind 沒有改變；metadata hook 444 次；subtree recording replay 616 次；command block 2,037 chunks 重用、73 chunks 驗證（總數 2,110）；graph／coverage／surface-structure replay 各 1 次。這是具體單幀觀察，不是所有幀的平均值。

**回歸與獨立像素驗證**

| 檢查 | 結果 |
|---|---|
| WIP baseline library tests | 1,757 passed、0 failed、96 ignored |
| 修正後完整 library tests | 1,769 passed、0 failed、96 ignored（新增 12） |
| Metal resource_lifecycle_tests | 29 passed、0 failed，實際執行原本 ignored tests |
| Metal text_area_recording_tests | 6 passed、0 failed，包含 artifact 與 Legacy 的 caret／selection／IME |
| 固定重播 authority | 四個 checkpoint 各 900 幀，預期 authority 全數一致、無 fallback |
| 同模式修正前後 GPU 像素 | artifact 6 組、Legacy 6 組，12／12 完全相同 |
| 實際 Legacy 視窗往返拖曳 | 通過，呈現計數持續增加，無 fallback assertion |
| 實際 artifact 視窗大幅往返拖曳 | 未通過嚴格無回退檢查；基線與修正後均同樣失敗 |

像素取樣 frame 59、79、89、109、129、149；每張 2,560×1,600 RGBA（4,096,000 pixels）。12 組同模式前後比對的 changed_pixels、max_channel_delta 均為 0，沒有使用寬容閾值或裁切排除區域。完整 raw pixels 以 `.rgba.gz` 無損保存，另有 SHA-256 manifest。

artifact 與 Legacy 的全 demo 原本已有差異：依畫面位置有 22,790–32,244 個不同 pixels，max channel delta 177。修正前後這些差異完全一致。這不是完整 RetainMode／Legacy pixel parity 已通過的證據。

**原生視窗新補到的既有失敗**

使用相同最終來源與 renderer-perf 嚴格入口，透過 UI automation 實際拖曳 Inspector。邏輯座標 `(184.17, 61.17) → (650, 70.92)`，再 `(650, 70.92) → (260, 81.75)`。修正後第一次移動可見且已 present；第二次大幅返回時出現 `LegacyBoundary(MissingPreparedInlineRoot)`，在 Selection 階段回退 Legacy。`render_frame_for_performance` 因禁止 fallback 而 panic；不能將此解讀成一般 production fallback 會直接 crash。

重新編譯 0c422b7 的原有 WIP，在相同 UI 操作座標重現相同 rejection／fallback／assertion。最終 Legacy 模式完成兩次拖曳。此原有 renderer preparation／admission 問題沒有併入三批局部修正；因此嚴格的原生視窗無回退 gate 仍未通過，不能宣告所有 native interaction 已驗收。兩份失敗 log 保留完整實際 pointer events 與 authority telemetry。

**可檢視的證據**

- `statistics.json`、`per-run-statistics.json`：各批、各階段中位數與 P95。
- `baseline-[0-9]-*.log`、`slots-[0-9]-*.log`、`hover-[0-9]-*.log`、`projection-[0-9]-*.log`：正式量測原始資料。
- `final-full-tests.log`、`native-resource_lifecycle_tests.log`、`native-text_area_recording_tests.log`：實際測試輸出。
- `pixel-comparison.json`、`pixel-sha256.json`、`baseline-pixels/`、`projection-pixels/`：像素證據；`fixed-artifact-109.png`、`fixed-legacy-109.png` 可預覽。
- `native-window-fixed-large-drag.log`、`native-window-baseline-large-drag.log`、`native-window-legacy.log`：原生視窗結果。
- `main-verification.json`、`source-manifest.json`、`native-binary-sha256.json`：來源與 binary 追溯。
- `replay.md`、`replay-overlay.patch`：診斷 harness 與重播方法。診斷 overlay 僅存在隔離目錄，沒有併入 production 修正。

供 reviewer 檢視的程式差異是 `0c422b7..b138789`，不是 main 的 118 個既有 WIP 檔案。commit 代表實作 checkpoint，沒有代替獨立 code review 或完整 RetainMode acceptance。
