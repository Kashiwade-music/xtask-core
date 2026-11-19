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
| L7 | W3 | 1 | 統合済み（67c1b89） |
| L8 | W3 | 2 | 統合済み（ed83685） |
| C1 | W3 | 3 | 差戻し（OPEN.md の 15・16。c3bf6a1 は merge 済み。W3 の tier b は通過） |
| V2a | W3 | 4 | 統合済み（079ffcf） |
| V2b | W3 | 5 | 統合済み（2d55e75） |
| V2d | W3 | 5（V2b の後） | 統合済み（de751f2） |
| V2c | W3 | 6 | 統合済み（cd721ff） |
| A2 | W4 | 1 | 統合済み（b9abc85） |
| V3 | W4 | 2 | 統合済み（1bb506f。GUI 試験 8 件は成功） |
| C3 | W4 | 3 | 統合済み（0867ef9） |
| C2a | W5 | 1 | 差戻し（OPEN.md の 18。c5cd12c は 34a4460 で merge 済み） |
| C2b | W5 | 1（C2a の後） | 統合済み（26826a8） |
| C2c | W5 | 1（C2b の後） | 差戻し（OPEN.md の 19。bb2425b は c8679c0 で merge 済み。integration の試験 1 件は fix lane W5-1-fix で直す） |

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

- `W3 merged 656b016、fix merged c4edc91: tier b 成功（KPI の未達は差戻し 17）`（root：check、xtask-archive 12、xtask-policy 24、xtask-source 14、xtask-workspace 12、xtask-modules（lib 3、analysis 62、roles 17、thread_local 23）、xtask-layout lib 48、xtask-style lib 20、clippy の四 crate、基準の試験 2 件（release）が成功。viewer：check、xtask-render 8、xtask-viewer lib 20 が成功。C1 の差戻し 15・16 は W3 の tier b を止めない（W5 の C2 だけに依存する））

### K1〜K4（W3、commit c4edc91、`baseline_report`）

| 指標 | 目標 | 実測 | 判定 |
|---|---|---|---|
| K1 交差数の比 | 1.3 以下 | 1.50 | 未達（差戻し 17a） |
| K2 折れの数の比 | 1.3 以下 | 1.13 | 達成 |
| K3 面積の比 | 1.5 以下 | 0.57 | 達成 |
| K4 配置の時間の和（121 件） | 3.0 秒以下 | 19.643 秒 | 未達（差戻し 17b） |

- `W4 merged 72a78ec: tier b 成功（viewer の GUI 試験は V3 の報告を記録。再実行しない）`（root：check（xtask-diagram を含む）、xtask-archive 12、xtask-policy 24、xtask-source 14、xtask-workspace 12、xtask-modules（lib 3、analysis 62、roles 17、thread_local 23）、xtask-layout lib 48、xtask-style lib 20、xtask-diagram 7、clippy の四 crate が成功。viewer：check、xtask-render 8、xtask-viewer lib 20 が成功）
- W5 は C2 が C1 に依存するため、差戻し 15・16 の解消を待つ。CP は差戻し 15〜17 の解消を待つ。

- `W5 partial merged 34a4460: L9 と C2a を merge（C2a は差戻し 18）`（非 core の check、xtask-layout lib 48 が成功。xtask-core は C2b まで compile できない。M3d は実行中）

- `W5 partial merged 6505d34: L9（5355dd3）と M3d（9f2f8ea）を merge。C2a は差戻し 18（34a4460）`（xtask-modules：lib 3、analysis 63、roles 17、thread_local 24、clippy が成功。xtask-layout：lib 48、基準の試験 2 件（release）が成功。xtask-core は C2b まで compile できない。C2b と C2c は C2a の差戻しのため未起動）

### K1〜K4（W5、commit 6505d34、`baseline_report`、L9 の後）

| 指標 | 目標 | 実測 | 判定 |
|---|---|---|---|
| K1 交差数の比 | 1.55 以下（DEC-22 で改定） | 1.50 | 達成 |
| K2 折れの数の比 | 1.3 以下 | 1.13 | 達成 |
| K3 面積の比 | 1.5 以下 | 0.54 | 達成 |
| K4 配置の時間の和（121 件） | 3.0 秒以下 | 1.816 秒 | 達成 |

K5（図のファイルの大きさ）と K6（検査の時間）は CP で測る。

- `W5 C2b and C2c merged c8679c0: tier b の途中`（xtask-core：check（workspace 全体）、lib 15、style 7 が成功。integration は 22 件成功・1 件失敗（stderr の target 名の欠落。fix lane W5-1-fix）。xtask-layout：clippy、lib 48、基準の試験 2 件が成功。xtask-modules：clippy、試験が成功。viewer：check、xtask-render 8、xtask-viewer lib 20 が成功。clippy の xtask-core は差戻し 19）

### K1〜K4（W5、commit c8679c0、`baseline_report`）

| 指標 | 目標 | 実測 | 判定 |
|---|---|---|---|
| K1 交差数の比 | 1.55 以下（DEC-22） | 1.50 | 達成 |
| K2 折れの数の比 | 1.3 以下 | 1.13 | 達成 |
| K3 面積の比 | 1.5 以下 | 0.54 | 達成 |
| K4 配置の時間の和（121 件） | 3.0 秒以下 | 1.783 秒 | 達成 |

### CP（途中。commit 9996441 の時点）

- K5 図のファイルの大きさ：17927 バイト（目標 102400 以下）。達成。`check --write` を node の無い PATH（`NODE_FREE_PATH`）で実行して測った。
- K6 検査の時間：`check` を暖機の後に 3 回測った。0.772、0.758、0.752 秒。中央値 0.758 秒（基準 1.203 × 1.05 = 1.263 秒以下）。達成。
- AC-1：`git ls-files` に `.js`・`.cjs`・`.html`・`.css` は無い。node の無い PATH で `check --write` は図のファイルを書いたが、循環参照の違反（差戻し 20）で終了コードが 1。未達。
- tier c の途中：`cargo fmt --all --check` 成功。`cargo +1.85.0 check --workspace --all-targets --locked` 成功（let チェーンを 1.85 で通る形に直した後、`c45ab73` を merge）。`cargo test --workspace` 成功（失敗 0）。viewer の fmt・clippy・`KIVACT_TEST_MIRROR=off cargo test --workspace` 成功。`cargo clippy --workspace --all-targets -- -D warnings` は差戻し 19 で未達。`cargo run -p xtask-core -- check` は差戻し 20 で未達（違反 23 件のうち、Role と行数は解消済み、循環 10 件が残る）。viewer の `check --root` も循環 2 件（xtask-render）が残る。

- `CP fixes merged c4ad8bd`：let チェーンを Rust 1.85 で通る形に（`c45ab73`）、テストモジュールの Role と analysis のテストの分割（`8c8d83d`）、テストの共有 helper を analysis 専用に移動（`e41f0b0`）。`cargo clippy --workspace --all-targets -- -D warnings` の残りは差戻し 19（cli.rs）だけ。`cargo run -p xtask-core -- check` の残りは差戻し 20（循環参照、xtask-core で 10 件、viewer で 2 件）だけ。CP は差戻し 19 と 20 の解消を待つ。

### CP passed 3837c84

作業指示書 2.3 節の tier c を、X1〜X7 の merge の後に最初から実行した。すべて成功。

| 項目 | 結果 |
|---|---|
| `cargo fmt --all --check` | 成功 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 成功（差戻し 19 は解消） |
| `cargo +1.85.0 check --workspace --all-targets --locked` | 成功（AC-6） |
| `cargo test --workspace` | 成功（失敗 0） |
| `cargo test -p xtask-layout --release --test baseline` | 成功（2 件。AC-3） |
| `cargo run -p xtask-layout --release --example baseline_report` | K1〜K4 達成（下表） |
| `cargo run -p xtask-core -- check` | 違反 0 件（ok、9 crate、31 graph）。差戻し 20 は解消 |
| `cargo run -p xtask-core -- check --root viewer` | ok（2 crate、7 graph） |
| viewer の `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`KIVACT_TEST_MIRROR=off cargo test --workspace` | 成功（失敗 0） |
| `git ls-files` の `.js`・`.cjs`・`.html`・`.css` | 0 件（AC-1） |
| node の無い PATH での `cargo run -p xtask-core -- check --write` | 終了コード 0（AC-1、AC-2 の書出し） |

| 指標 | 目標 | 実測 | 判定 |
|---|---|---|---|
| K1 交差数の比 | 1.55 以下（DEC-22） | 1.50 | 達成 |
| K2 折れの数の比 | 1.3 以下 | 1.13 | 達成 |
| K3 面積の比 | 1.5 以下 | 0.54 | 達成 |
| K4 配置の時間の和（121 件） | 3.0 秒以下 | 1.871 秒 | 達成 |
| K5 図のファイルの大きさ | 102400 バイト以下 | 17428 バイト | 達成 |
| K6 検査の時間 | 1.263 秒以下（基準 1.203 × 1.05） | 中央値 0.827 秒（1.204、0.827、0.807） | 達成 |

差戻し（OPEN.md）：すべて解消済み。
