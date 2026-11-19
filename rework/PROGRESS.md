# 改修の進捗

- 統合の branch：`refactor/kashiwade/native-diagrams`（`<MAIN>` = `W:/w/xtask-core`）
- `<WT>` = `W:/w/xtask-core-wt`（絶対 path）
- `<KIVACT>` = `W:/w/xtask-core-wt/kivact`（commit `2b8df5aceea40c9ae23950f9d6c942559c89e840`）
- Rust toolchain：1.85.0（`rustup toolchain` に入っていることを確認済み）

## K6 の基準（W0、改修前）

測り方：作業指示書 2.4 節。`check --no-write` を 3 回、`CARGO_TARGET_DIR=<WT>/target-main`、同じ機械。build の時間は含めない。暖機 1 回。

| 回 | 秒 |
|---|---|
| 1 | 1.665 |
| 2 | 1.203 |
| 3 | 0.796 |

- 中央値：**1.203 秒**
- 判定の上限（改修後の中央値）：1.203 × 1.05 = **1.263 秒**

## unit の状態

状態は 1.4 節の四つ（未着手、実装中、統合済み、差戻し）。

| unit | wave | lane | 状態 |
|---|---|---|---|
| L1 | W1 | 1 | 実装済み・W1 の tier b 待ち（a3b6dd8 → merge 済み） |
| A1 | W1 | 2 | 実装済み・W1 の tier b 待ち（5cadc2b → merge 済み） |
| M1 | W1 | 3 | 差戻し（OPEN.md の 1〜4） |
| M2 | W1 | 4 | 差戻し（OPEN.md の 5） |
| L2 | W2 | 1 | 未着手 |
| L6 | W2 | 1（L2 の後） | 未着手 |
| L3 | W2 | 2 | 未着手 |
| L4 | W2 | 3 | 未着手 |
| L5 | W2 | 4 | 未着手 |
| M3a | W2 | 5 | 未着手 |
| M3b | W2 | 5（M3a の後） | 未着手 |
| M3c | W2 | 5（M3b の後） | 未着手 |
| V1 | W2 | 6 | 未着手 |
| L7 | W3 | 1 | 未着手 |
| L8 | W3 | 2 | 未着手 |
| C1 | W3 | 3 | 未着手 |
| V2a | W3 | 4 | 未着手 |
| V2b | W3 | 5 | 未着手 |
| V2d | W3 | 5（V2b の後） | 未着手 |
| V2c | W3 | 6 | 未着手 |
| A2 | W4 | 1 | 未着手 |
| V3 | W4 | 2 | 未着手 |
| C3 | W4 | 3 | 未着手 |
| C2a | W5 | 1 | 未着手 |
| C2b | W5 | 1（C2a の後） | 未着手 |
| C2c | W5 | 1（C2b の後） | 未着手 |

## wave の記録

- `W1 merged e13ecfe: tier b 失敗`（check は成功、xtask-archive・xtask-source・xtask-workspace・xtask-layout --lib の試験は成功。xtask-policy の試験 1 件が失敗：OPEN.md の 1。差戻しのため W2 は起動しない。作業指示書 1.4 節）
