# 未解決の事項

## 差戻し（設計者への差戻し）— 解消済み

| # | 時期 | unit | 場所 | 解消 |
|---|---|---|---|---|
| 1〜5 | W1 | M1、M2 | `xtask-policy`、`xtask-workspace`、`xtask-source` | 設計者が d196eaa で付属書を改訂し、fix lane（f6685ca、286038e）で解消。W1 tier b 成功（d3641fc） |
| 6 | W2 | L2 | `chains.rs`：`backward_dummy_layer_removed` の期待値 | 設計者が付属書 L 4.5 を改訂（期待値は Step 6、向き `[true,false,false,false,false,true]`、`layer_count` 5）。fix lane `W2-1-fix`（3e8cfba）で試験の期待値を合わせた |
| 7 | W2 | M3a | `analysis.rs`：`disabled_root_keeps_root_module` | 設計者が契約を改めた（root が cfg で無効なら modules は空、診断も無い。既存の `cfg_excluded_modules_do_not_require_roles` を守る）。試験は削除（fix lane 3f3c058） |
| 8 | W2 | M3a | 手順からの逸脱（(a) cfg_attr の二度評価、(b) file:line の欠落、(c) roles の試験の書換え、(d) 手順 7 の残り） | 設計者が付属書を改訂し、fix lane 3f3c058 で (a)(b)(d) を直した。(c) は移行の表に載せた（既存の期待値は変えず、名前と削除した API の assert だけ） |

W2 の tier b（root の check、xtask-archive・xtask-policy・xtask-source・xtask-workspace・xtask-modules・xtask-layout の試験、clippy の三 crate、viewer の check と xtask-render の試験）は、`9e9ea2f` の時点で成功。ただし W2 のウェーブの中の M3b・M3c は未実施。

## 差戻し（W2 の M3b で新規、未解決）

M3b（commit 695c72a、merge 済み 27a56d3）の報告による。STOP の項目は次の二件。`thread_local` の試験 11 件が失敗しており、W2 のウェーブは完了できない。M3c は M3b の差戻しが解消するまで起動しない。

| # | unit | 場所 | STOP の文（要旨） | 付属書の節 |
|---|---|---|---|---|
| 9 | M3b | `crates/xtask-modules/src/thread_local_items.rs`：`register`・`confirm` | 手順 6 の「登録は取り消さない（診断で不確かさを報告する）」と「`std::thread_local` の qualified は確か」は、既存の試験 10 件の期待値と食い違う。その期待値は「提供元が不確かなとき static を作らず診断を増やさない」「local の std module や local の `thread_local!` は提供元にならない」というもの。失敗した試験：`other_macro_providers_do_not_gain_static_declarations`、`local_std_module_is_not_the_standard_macro_provider`、`delayed_external_glob_provider_does_not_leave_a_generated_static`、`external_crate_globs_do_not_create_bare_statics`、`unknown_external_macro_use_does_not_infer_bare_standard_provider`、`unresolved_imports_behind_local_globs_remain_uncertain`、`unresolved_explicit_providers_and_globs_do_not_create_statics`、`effective_macro_use_attributes_only_block_selected_macro_names`、`resolved_value_imports_and_inactive_imports_preserve_standard_providers`、`user_defined_thread_local_macros_are_not_standard` | M-model.md の M3b の手順 6、DEC-19、既存の試験の移行（M3 全体） |
| 10 | M3b | `thread_local_items.rs`：`provider` | 手順 6 と DEC-19 は、alias の登録を同じ module の `use std::thread_local as X` に限る。既存の試験 `standard_macro_alias_resolves_after_reexport` は、別 module の再公開（`crate::macros::tls`）を要求する。失敗中 | M-model.md の M3b の手順 6、DEC-19 |

注（STOP ではない）：`crates/xtask-modules/src/visitor.rs` の `visit_macro` が 207 行で、作業指示書 0.2 の 6（1 関数 80 code 行）を超えたまま。意味のある helper に分ける必要がある。設計者の判断の後の fix で直す。

## 差戻し（W2 の M3b の fix で新規、未解決）

M3b の fix lane（0b36b98、merge 済み 17db880）の報告による。9・10 は解消。残るのは次の一件。

| # | unit | 場所 | STOP の文（要旨） | 付属書の節 |
|---|---|---|---|---|
| 11 | M3b | `crates/xtask-modules/tests/thread_local.rs`：`untrusted_glob_makes_thread_local_uncertain`（698 行目） | この試験は、provider が不確かなときの診断 `crate::b: macro provider cannot be established: thread_local` を要求する。同じ規則の `external_crate_globs_do_not_create_bare_statics`（462 行目）は、同じ状況で診断が 1 件（`::FAKE`）だけであることを要求する。付属書の「診断を足さない」に従うと後者は通るが前者が落ち、旧 `confirm()` の診断を残すと後者が落ちる。両方の試験の違いは、後者の consumer が `FAKE` を import している点だけ（報告による）。**W2 の tier b（`cargo test -p xtask-modules`）の失敗の原因（残り 1 件）** | M-model.md の M3b の手順 6（`uncertain` と診断の規則）、既存の試験の移行（M3 全体） |

注（STOP ではない）：`crates/xtask-modules/src/cfg.rs` の `expansion_diagnostics` が 94 code 行で、作業指示書 0.2 の 6（1 関数 80 code 行）を超えたまま。fix の lane で分ける。

## 差戻し（W2 の M3b で新規、解消済み）

12 と 13 は設計者が付属書 M3b を改訂（commit a535fd5）。fix lane（ab5d1e5）を merge して解消（6e3adb4）。xtask-modules の全件、clippy、viewer の tier b が成功。以下は解消の記録。

| # | unit | 場所 | STOP の文（要旨） | 付属書の節 |
|---|---|---|---|---|
| 12 | M3b | `crates/xtask-modules/tests/thread_local.rs`：`untrusted_glob_makes_thread_local_uncertain`（706 行目） | 既定の Settings（`trusted_macros` が空）で、診断が 2 件出る：`crate::b: item macro expansion is unsupported: thread_local`（item 位置の `thread_local!` が、信頼されていない macro として未対応の診断を出す）と `crate::c: unresolved explicit local import: crate::b::C`。付属書の試験の欄は「診断がちょうど 1 件で `::C` で終わる」を求める。`trusted_macros` に `thread_local` を入れて試すと 1 件（`::C`）で通ることを確かめたが、試験の設定は付属書どおりに戻した。**W2 の tier b（`cargo test -p xtask-modules`）の失敗の原因（残り 1 件）** | M-model.md の M3b の「試験」の欄（`untrusted_glob_makes_thread_local_uncertain`） |
| 13 | M3b | `crates/xtask-modules/src/thread_local_items.rs`：`uncertain` | 付属書 M3b の手順 6 の `uncertain` の文どおりに実装すると、trusted_globs に入れた glob（解決できないもの）を不確かとみなし、`trusted_glob_allows_thread_local`（診断が空を求める）が通らない。実装は、trusted_globs に入っている glob を飛ばすようにした（`Settings` の doc「known to provide no macros」に合わせた）。付属書の文と実装が違う逸脱のため、設計者が規則を確かめる | M-model.md の M3b の手順 6（`uncertain`） |

## 差戻し（W2 の M3c で新規、解消済み）

M3c（2e50e6b、merge 済み 9bf361f）の報告による。14 は設計者が付属書 M3c の移行表を改訂（b2a98c2）し、fix lane（8c186b3）を merge して解消（11ad3d8）。以下は解消の記録。

| # | unit | 場所 | STOP の文（要旨） | 付属書の節 |
|---|---|---|---|---|
| 14 | M3c | `crates/xtask-modules/tests/analysis.rs`：`known_macro_and_derive_provider_aliases_remain_supported`（1003 行目） | 既存の試験が移行の表に無いのに失敗する。`use serde_json::json as data;` の `data!` が `unsupported macro provider: data` の診断になる（json の特別扱いの削除（D1、DEC-10）の結果）。手順 1 の削除は行い、試験の期待値は変えていない。設計者の判断が要る：(a) 移行の表にこの試験の期待値の変更を足す、(b) json の分岐を戻す（`src/json.rs` と visitor の分岐） | M-model.md の M3c の手順 1（json・Token の特別扱いの削除）、既存の試験の移行（M3 全体） |

注（STOP ではない）：`implicit_format_capture` は、実装の前から成功していた（`use crate::a::N;` の import の辺で、期待する辺が既に出るため）。付属書の期待値は変えていない。取込みの振舞いは `macros.rs` の `format_capture_rules` が確かめている。

## 事前承認による移行

作業指示書 0.5 の事前承認の行に当たり、差戻しにせず、ここに一行ずつ書く（試験、除いたもの、対象の機能）。

- `known_macro_and_derive_provider_aliases_remain_supported`（`crates/xtask-modules/tests/analysis.rs`、M3c）：`use serde_json::json as data;` の行と、`let _ = data!({"x": crate::a::f()});` の文を除いた（対象の機能：`json!` の特別扱い、DEC-10）。他の入力と assert は変えていない。この行は、設計者の移行表の改訂（b2a98c2）の後に fix lane（8c186b3）で入った。

## 未解決の注意（設計者の判断が要る）

M3a の fix lane（3f3c058）の報告による。STOP ではないが、付属書の手順と食い違う可能性がある。

1. `crates/xtask-modules/src/thread_local_items.rs` の `thread_local!` の中の `static` の item の cfg を、loader の外（visitor 側）で評価している。付属書 M の M3a 手順 4 は「item の属性の評価は loader だけ」。loader は macro の本体を解析しない。`?` で一つの predicate の誤りが設定全体を止める（付属書 B5 と C1 の「診断を一つ出して、その item を無効とみなす」と食い違う）。
2. 読込みの io の誤りと、root の normalize の誤りには行番号が無く、`{file}: ` だけが付く。
3. impl と trait の fn の属性は、二度評価されたまま（M3b の手順 7 の範囲）。
4. 振る舞いの変更：不正な `cfg_attr` を持つ `extern crate` の item は、以前は設定全体を止めていたが、今は loader の診断で item を無効にする。試験はすべて通る。
5. visitor の cfg の誤りの文が `{module}: {error}` から `{file}:{line}: {error}` に変わった（試験は依存していない）。

## 未解決の注意（差戻しではない）

- `dev/xtask/policy/workspace.toml` の最上位 `output = "architecture"`：付属書 C の C2c（W5）で消す。
- `.gitattributes` の elkjs の行：C2c（W5）で消す。
- M1 は試験を先に書いたが、実装前に赤になることを確かめていない（crate の新設のため）。作業指示書 0.3 の手順からの逸脱として記録する。
- L3 の読み方の記録（報告のみ）：5.2 節の Box の frac は、その link の辺の port の y で決めた。5.7 節の「辺ごと」は item の左辺・右辺ごと（側ごと）と読んだ。TurnEast・TurnWest の tie-break は path の（path 番号、step 番号）で決めた。
- L6 の付記：`check::output` は最初に `check::input` を呼ぶ（付属書に無い。手で作った不正な Graph を `verify` に渡しても panic せず `Err` を返すため）。
- V1 の付記：`impl IntoView` を返す `root`・`view`・`diagram_view`・`sidebar` は、`todo!()` の本体だと compile が通らないため、内側の fn `unbuilt()` に置いた。V2 の lane が本体を書くときに消す。
- `loader.rs` の clippy 指摘の直し方は、clippy の提案（let chain）ではなく同じ振舞いの `match` にした。let chain は Rust 1.88 以降が要り、契約の Rust 1.85（AC-6）と衝突するため。
