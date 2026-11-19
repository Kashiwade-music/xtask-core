# 未解決の事項

## 差戻し（設計者への差戻し）— W1 で解消

作業指示書 0.4・0.5 節の差戻し。設計者が付属書 M（M1、M2）と付属書 C（C2）を d196eaa で改訂し、統合の branch の HEAD から fix の lane で STOP の項目だけを直した。

| # | unit | 場所 | 解消 |
|---|---|---|---|
| 1 | M1 | `xtask-policy`：`analysis_settings_parse_cargo_cfg_and_trusted_expansions_with_other_rules` | 設計者が移行表を改訂（入力を直す、`duplicate_trusted_names_rejected` を足す）。fix lane `W1-3-fix`（f6685ca）で解消 |
| 2 | M1 | `xtask-workspace`：`feature_closure_handles_cycles_dependency_entries_and_unknown_requests` | 設計者が削除を承認（B7 で API を消したため）。M1 で削除済み |
| 3 | M1 | `xtask-policy`：`diagram_targets_default_to_all_and_parse_by_name` | 設計者が分割を承認。読込みの部分は policy に残し、`includes` の部分は C2b の `selection_by_diagram_targets`（W5）に移す。fix lane（f6685ca）で確認 |
| 4 | M1 | `xtask-policy`：svg 出力の既定値の試験 3 件 | 設計者が名前・既定値（`target/xtask/architecture.xtaskmap`）・入力の移行を承認。fix lane（f6685ca）で解消 |
| 5 | M2 | `xtask-source`：`code_line_count` の寛容な fallback | 設計者が手順 7 を改訂（`code_line_count` を `Result` にし、`malformed_or_unreadable_files_are_errors` を移行）。fix lane `W1-4-fix`（286038e）で解消 |

## 未解決の注意（差戻しではない）

- `dev/xtask/policy/workspace.toml` の最上位 `output = "architecture"`：設計者が付属書 C の C2c（W5）に割り当てた。W5 まで残る。
- `.gitattributes` の elkjs の行：C2c（W5）で消す。
- M1 は試験を先に書いたが、実装前に赤になることを確かめていない（crate の新設のため）。作業指示書 0.3 の手順からの逸脱として記録する。
