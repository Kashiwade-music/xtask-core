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
| L1 | W1 | 1 | 統合済み（a3b6dd8） |
| A1 | W1 | 2 | 統合済み（5cadc2b） |
| M1 | W1 | 3 | 統合済み（658a9f1、差戻し 1〜4 は fix lane f6685ca で解消） |
| M2 | W1 | 4 | 統合済み（f53b057、差戻し 5 は fix lane 286038e で解消） |
| L2 | W2 | 1 | 統合済み（8fe2ad8、差戻し 6 は fix 3e8cfba で解消。W2 の途中の tier b 成功） |
| L6 | W2 | 1（L2 の後） | 統合済み（e196bdc。W2 の途中の tier b 成功） |
| L3 | W2 | 2 | 統合済み（845010a。W2 の途中の tier b 成功） |
| L4 | W2 | 3 | 統合済み（757fa60。W2 の途中の tier b 成功） |
| L5 | W2 | 4 | 統合済み（eb5182d。W2 の途中の tier b 成功） |
| M3a | W2 | 5 | 統合済み（20353c5、差戻し 7・8 は fix 3f3c058 で解消。ただし OPEN.md の「未解決の注意」の 1〜5 は設計者の判断待ち） |
| M3b | W2 | 5（M3a の後） | 統合済み（695c72a、差戻し 9〜13 は fix 0b36b98・c2cf509・ab5d1e5 で解消。最終の merge は 6e3adb4） |
| M3c | W2 | 5（M3b の後） | 統合済み（2e50e6b、差戻し 14 は b2a98c2 の改訂と fix 8c186b3 で解消。最終の merge は 11ad3d8） |
| V1 | W2 | 6 | 統合済み（1adf297。viewer の tier b 成功） |
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

- `W1 merged e13ecfe: tier b 失敗`（xtask-policy の試験 1 件が失敗。設計者の付属書の改訂の後に解消）
- `W2 fix merged 93f1618: clippy 成功（xtask-archive・xtask-policy・xtask-modules）。tier b は未成功：xtask-layout の試験 1 件（OPEN.md の 6）と xtask-modules の試験 1 件（OPEN.md の 7）が設計者の判断待ち`
- `W2 partial fix merged 9e9ea2f: 途中の tier b 成功`（差戻し 6・7・8 の fix を merge。check、xtask-archive 12、xtask-policy 24、xtask-source 14、xtask-workspace 12、xtask-modules 89（unit 1、analysis 53、roles 16、thread_local 19）、xtask-layout 42、clippy の三 crate、viewer の check と xtask-render 8 が成功。M3b・M3c は未実施のため、W2 のウェーブは未完了）
- `W2 merged 571b7b5: tier b 失敗`（root check は成功。失敗：xtask-layout の試験 1 件（OPEN.md の 6）、xtask-modules の試験 1 件（OPEN.md の 7）、xtask-modules の clippy 指摘 2 件（fix lane W2-5-fix で直す）。viewer の check と xtask-render の試験 8 件は成功）
- `W1 fix merged d3641fc: tier b 成功`（fix lane f6685ca（M1）と 286038e（M2）を merge。check、xtask-archive 12、xtask-policy 24、xtask-source 14、xtask-workspace 12、xtask-layout --lib 10 が成功）

- `W2 M3b merged 27a56d3: tier b 失敗`（xtask-modules の試験：unit 1、analysis 60、roles 16 が成功、thread_local の試験 11 件が失敗。差戻し 9・10（OPEN.md）。visit_macro が 207 行で 0.2 の 6 を超える。M3c は M3b の差戻しのため未起動。W2 のウェーブは未完了）

- `W2 M3b fix merged 17db880: tier b 失敗`（9・10 は解消（二回の解決、visit_macro の分割、thread_local の試験 22 件が成功、analysis 60、roles 16、unit 1）。残り：thread_local の試験 1 件（OPEN.md の 11）。M3c は未起動。W2 のウェーブは未完了）

- `W2 M3b fix merged 1963ea2: tier b 失敗`（thread_local の試験 22 件が成功、1 件が失敗（OPEN.md の 12）。expansion_diagnostics を分割済み、clippy 成功。uncertain の文と実装の違い（OPEN.md の 13）。M3c は未起動。W2 のウェーブは未完了）

- `W2 M3b final merged 6e3adb4: tier b 成功（W2 の途中）`（xtask-modules：unit 1、analysis 60、roles 16、thread_local 23 が成功。xtask-layout 42、xtask-archive 12、xtask-policy 24、xtask-source 14、xtask-workspace 12、clippy の三 crate、viewer の check と xtask-render 8 が成功。M3c は未実施のため、W2 のウェーブは未完了）

- `W2 M3c merged 9bf361f: tier b 失敗`（xtask-modules の試験：lib 3、analysis 61 成功・1 失敗（OPEN.md の 14）、roles 17、thread_local 23 が成功。clippy と cargo check は成功。W2 のウェーブは未完了）

- `W2 merged 11ad3d8: tier b 成功（W2 のウェーブ完了）`（root：check、xtask-archive 12、xtask-policy 24、xtask-source 14、xtask-workspace 12、xtask-modules（lib 3、analysis 62、roles 17、thread_local 23）、xtask-layout 42、clippy の三 crate が成功。viewer：check と xtask-render 8 が成功）
