# 未解決の事項

## 差戻し（設計者への差戻し）— W1 で解消

作業指示書 0.4・0.5 節の差戻し。設計者が付属書 M（M1、M2）と付属書 C（C2）を d196eaa で改訂し、統合の branch の HEAD から fix の lane で STOP の項目だけを直した。

| # | unit | 場所 | 解消 |
|---|---|---|---|
| 1 | M1 | `xtask-policy`：`analysis_settings_parse_cargo_cfg_and_trusted_expansions_with_other_rules` | 設計者が移行表を改訂。fix lane `W1-3-fix`（f6685ca）で解消 |
| 2 | M1 | `xtask-workspace`：`feature_closure_handles_cycles_dependency_entries_and_unknown_requests` | 設計者が削除を承認。M1 で削除済み |
| 3 | M1 | `xtask-policy`：`diagram_targets_default_to_all_and_parse_by_name` | 設計者が分割を承認（`includes` の部分は C2b の `selection_by_diagram_targets`、W5）。fix lane（f6685ca）で確認 |
| 4 | M1 | `xtask-policy`：svg 出力の既定値の試験 3 件 | 設計者が名前・既定値・入力の移行を承認。fix lane（f6685ca）で解消 |
| 5 | M2 | `xtask-source`：`code_line_count` の寛容な fallback | 設計者が手順 7 と移行表を改訂。fix lane `W1-4-fix`（286038e）で解消 |

## 差戻し（W2 で新規、未解決）

| # | unit | 場所 | STOP の文（要旨） | 付属書の節 |
|---|---|---|---|---|
| 6 | L2 | `crates/xtask-layout/src/chains.rs`：`backward_dummy_layer_removed` | 期待値の 5 step が、手順（4.2・4.3 節）からは出ない。段 b→a が層 0,2,4 にわたると 8 step、dummy だけの二つの層を消すと 6 step になる。手順どおりに実装し、期待値は変えていない（作業指示書 0.5 の「期待値を緩めない」）。**W2 の tier b（`cargo test -p xtask-layout --lib`）の失敗の原因** | L-layout.md の L2 の試験、4.2・4.3 節 |
| 7 | M3a | `crates/xtask-modules/tests/analysis.rs`：`disabled_root_keeps_root_module` | 手順 3「`#![cfg]` が偽の root も modules に残す」が、既存の `tests/roles.rs`：`cfg_excluded_modules_do_not_require_roles`（無効な root は modules が空）と食い違い、移行の表に無い。現行どおり「無効な root は module を作らない」を維持し、この新しい試験は失敗のまま。**W2 の tier b（`cargo test -p xtask-modules`）の失敗の原因** | M-model.md の M3a の手順 3、既存の試験の移行（M3 全体） |
| 8 | M3a | 手順からの逸脱（STOP なしで実施） | (a) 手順 4：item の属性の展開の診断で、visitor が `cfg::effective` を再び呼ぶ（`cfg_attr` の述語を二度評価）。手順 4 の「一つの担当」から外れる。(b) 手順 6：式・文の cfg の誤りに file:line を付けていない（module id のみ）。(c) 既存の試験の書換え（表に無い）：`roles.rs` の `root_only_api_matches_analysis_limits_and_preserves_default_api` は入力から `mod missing_file; include!(..)` を外した。`zero_limit_is_a_configuration_error_in_all_configurable_apis` は `crate_role_with_limit` の部分を外した（assert の削除に当たる可能性）。(d) 手順 7 の一部と、手順 3 の id の `usize` 化（M3b の範囲）は未実施 | M-model.md の M3a の手順 3・4・6・7、既存の試験の移行 |

差戻し 6 と 7 は、W2 の tier b の失敗の原因。差戻し 8 は STOP ではなく実装者の報告による逸脱。設計者が「許容する」か「直す」かを決める必要がある。M3b と M3c は、M3a の差戻しが解消するまで起動しない（作業指示書 1.4 節）。

## 未解決の注意（差戻しではない）

- `dev/xtask/policy/workspace.toml` の最上位 `output = "architecture"`：設計者が付属書 C の C2c（W5）に割り当てた。W5 まで残る。
- `.gitattributes` の elkjs の行：C2c（W5）で消す。
- M1 は試験を先に書いたが、実装前に赤になることを確かめていない（crate の新設のため）。作業指示書 0.3 の手順からの逸脱として記録する。
- L3 の読み方の記録（報告のみ、設計の判断ではない）：5.2 節の Box の frac は、その link の辺の port の y で決めた。5.7 節の「辺ごと」は item の左辺・右辺ごと（側ごと）と読んだ。TurnEast・TurnWest の tie-break は path の（path 番号、step 番号）で決めた。
- L6 の付記：`check::output` は最初に `check::input` を呼ぶ（付属書に無い。手で作った不正な Graph を `verify` に渡しても panic せず `Err` を返すため）。
- V1 の付記：`impl IntoView` を返す `root`・`view`・`diagram_view`・`sidebar` は、`todo!()` の本体だと `!` が `IntoView` にならず compile が通らないため、`todo!("Vxx")` を `View` を返す内側の fn `unbuilt()` に置いた。V2 の lane が本体を書くときに、この内側の fn は消す。
- W2 の clippy：`xtask-modules` の機械的な指摘 2 件（`cfg.rs:7`、`loader.rs:248`）は fix lane `W2-5-fix`（8103f2f）で直し、clippy の `-D warnings` は `xtask-archive`・`xtask-policy`・`xtask-modules` で成功。
- `loader.rs:248` の直し方は、clippy の提案（let chain）ではなく同じ振舞いの `match` にした。let chain は Rust 1.88 以降が要り、契約の Rust 1.85（AC-6）と衝突するため。設計者が確認すること。
