# 未解決の事項

## 差戻し（設計者への差戻し）

作業指示書 0.4・0.5 節のとおり、STOP を設計者への差戻しとして一行ずつ書く。解消は、設計者が付属書を直した後に、その unit の STOP の項目だけを fix の lane で行う（作業指示書 1.4 節）。

| # | unit | 場所 | STOP の文（要旨） | 付属書の節 |
|---|---|---|---|---|
| 1 | M1 | `crates/xtask-policy/src/tests.rs`：`analysis_settings_parse_cargo_cfg_and_trusted_expansions_with_other_rules` | 入力の `all_features = true` と `features` の併用、`trusted_*` の重複が手順 4（B3、B4）で拒否され、期待値が満たせない。**tier b（W1）の失敗の原因** | M-model.md の M1 の手順 4、試験 |
| 2 | M1 | `crates/xtask-workspace/tests/metadata.rs`：`feature_closure_handles_cycles_dependency_entries_and_unknown_requests` | 旧 `Package::active_features`（B7 で削除）を使う旧試験を削除した。移行の表に書換え先が無い | M-model.md の M1「既存の試験の移行」 |
| 3 | M1 | `crates/xtask-policy/src/tests.rs`：`diagram_targets_default_to_all_and_parse_by_name` | `DiagramTargets::includes` が無く、`includes` の assertion を移せない。表にも無い。試験の名前と assertion を変えた | M-model.md の M1 の手順 6、「既存の試験の移行」 |
| 4 | M1 | `crates/xtask-policy/src/tests.rs`：svg 出力先の既定値の試験 | 既定値の期待値 `"architecture"` を `"target/xtask/architecture.xtaskmap"` に変えた（設計書 5.1 に合わせた）。表に無く、0.2 の 4 に当たる疑い | M-model.md の M1「既存の試験の移行」 |
| 5 | M2 | `crates/xtask-source/src/lines.rs`・`src/lib.rs`：`code_line_count` の寛容な fallback 削除（手順 7） | `crates/xtask-source/tests/source.rs`：`malformed_or_unreadable_files_are_errors` の `code_line_count(...) == 2`（fallback の値）が通らなくなる。M2 に「既存の試験の移行」の表が無い。手順 7 は未実施 | M-model.md の M2 の手順 7 |

## 実装者の報告による未解決の注意（差戻しではない）

- `dev/xtask/policy/workspace.toml` の最上位の `output = "architecture"` は、新しい parser（xtask-policy）で拒否される。このファイルは M1 の所有外で、どの unit が直すかは付属書に無い。xtask-core の `check` を動かす前に直す必要がある。
- `.gitattributes` に `crates/xtask-layout/vendor/elkjs/elk.bundled.js` の行が残っている（L1 の所有外）。害は無いが、後の unit か CP で消す。
- M1 は試験を先に書いたが、実装前に赤になることを確かめていない（crate の新設のため）。作業指示書 0.3 の手順からの逸脱として記録する。
