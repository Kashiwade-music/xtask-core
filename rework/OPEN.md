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
- `policy_analysis_is_used_and_api_analysis_replaces_all_of_it`（`crates/xtask-core/tests/integration.rs`、C2c）：旧い `run` の枝（`available_node()` の下で、`Report.artifacts` の 4 件＝HTML 1・SVG 3 を数えていた）を、図のファイル 1 件の中の図の数 3 件（crate 依存図と module 図 2 件）の assert に替えた。対象の機能：HTML・SVG の出力と `Report.artifacts`（設計書 4.3 節）。他の入力と assert は変えていない。（C2c の実装者が、試験の実行の前に、移行の表の範囲の変更として書いた。）

## 差戻し（W3 の C1 で新規、未解決）

C1（c3bf6a1、merge 済み 656b016）の報告による。STOP は次の二件。W4 の C3・A2・V3 と、W3 のほかの unit は、この差戻しに依存しない。W5 の C2 は C1 に依存するので、この差戻しが解消するまで起動しない。

| # | unit | 場所 | STOP の文（要旨） | 付属書の節 |
|---|---|---|---|---|
| 15 | C1 | `crates/xtask-style/src/style_source.rs`：`ItemSpan` | 付属書の `ItemSpan` は `start_line`・`end_line`・`item` だけだが、旧 `style.rs` の試験 `item_exception_does_not_expand_to_a_sibling_on_the_same_line`（移行の表に無い）は、同じ行の列の一致を要する。`start_column`・`end_column` を足して実装し、試験の期待値は変えずに通している。設計者の判断が要る | C-core.md の C1 |
| 16 | C1 | `crates/xtask-style/src/style.rs`：`check` | features が `Err` のときの扱い。`StyleInput` の doc（手順 2）は「Err なら compiler の検査は行わない」、手順 7 は「probe だけが errors に一つ入れて終わる（今の動作と同じ）」と書いており、二通りに読める。実装は手順 7 に従い、clippy は走らせている。設計者の判断が要る | C-core.md の C1 の手順 2・7 |

C2 への注意（報告による）：検査で外れた診断は `StyleReport.diagnostics` に入り、`errors` には入らない（旧は `errors` にも文字列で入れていた）。C2 で `Report.style` へ渡すときの扱いを決める必要がある。`xtask-core/src/model.rs` に旧い `StyleDiagnostic` が残っている（C2 で `xtask_style` の型に置き換える）。

## 差戻し（W3 の KPI で新規、未解決）

W3 の tier b で測った KPI（作業指示書 2.2 節の「未達なら差戻し」）。基準の試験（`every_baseline_input_lays_out`、`layout_is_deterministic`）は成功。

| # | 種別 | 指標 | 目標 | 実測 | 判定 |
|---|---|---|---|---|---|
| 17a | KPI | K1（交差数の比） | 1.3 以下 | 1.50 | 未達 |
| 17b | KPI | K4（配置の時間の和、kivact 121 件、release、1 thread） | 3.0 秒以下 | 19.643 秒 | 未達 |

K2（折れの数の比）は 1.13、K3（面積の比）は 0.57 で、目標に届いている。K5・K6 は CP で測る。W4 の A2・V3・C3 は、この KPI に依存しないので進める。設計者が、設計を直すか、目標を改めるかを決める。

## 差戻し（W5 の C2a で新規、未解決）

C2a（c5cd12c、merge 済み 34a4460）の報告による。STOP は無いが、既存の試験の入力を、付属書の移行表に無いまま変えているので、作業指示書 0.2 の 4 と 0.4 により差戻しにする。C2b は C2a に依存するため、この差戻しが解消するまで起動しない。

| # | unit | 場所 | 内容 | 付属書の節 |
|---|---|---|---|---|
| 18 | C2a | `crates/xtask-core/src/cli.rs`：`missing_and_surplus_arguments_return_usage_errors` | 既存の試験の入力 `["check", "--root", "--help"]` を、一覧から外した。設計書 5.1 の「`-` で始まる値も受け付ける」に従うと、この入力は誤りではなくなるため。付属書の移行表には、この変更の行が無い。`root_value_may_start_with_dash` の新しい試験は、`-x` を値として受け付けることを確かめる。設計者が決める：(a) 移行の表に「`--help` を値として受け付ける」の変更を足す（推奨。5.1 節と一致する）、(b) 既存の試験を元の入力に戻し、`--root` の値の規則を付属書で改める | C-core.md の C2a、既存の試験の移行（C2 全体） |

注：C2a の報告では、`xtask-core` の lib は C2b まで compile できない（残りの error 21 件は `src/run.rs`）。想定どおり。

## 差戻し（W5 の C2c で新規、未解決）

C2c（bb2425b、merge 済み c8679c0）の報告による。STOP は次の一件。CP の tier c（`cargo clippy --workspace --all-targets -- -D warnings`）を止めるので、CP の前に解消が要る。

| # | unit | 場所 | STOP の文（要旨） | 付属書の節 |
|---|---|---|---|---|
| 19 | C2c | `crates/xtask-core/src/cli.rs`：`Command::Check(Config)` | clippy の `large_enum_variant`（272 バイト、閾値 200）。clippy の提案どおり `Box` にすると、設計書 5.1 の `pub enum Command { Help, Check(Config) }` が変わる。`Config` を小さくするには、`Config.analysis`（`Option<AnalysisSettings>`）の型を変える必要があり、5.1 の `Config` の定義と食い違う。0.2 の 5 により `#[allow]` は使えない。この lint は C2c で入ったものではなく、C2a 以降の `cli.rs` と `Config` の形から出ている。設計者が決める | C-core.md の C2a（`Command`、`Config`）、設計書 5.1 |

注（報告による。STOP ではない）：`failed_analysis_is_recorded_in_the_archive` は、構文の誤りの reason に `lib.rs` が含まれることを仮定している。図の名前は `target-kind-mode`（例 `fixture-lib-normal`）。`role_limits_reach_analysis_and_all_check_diagrams_with_api_precedence` は、表の名前のまま残した（名前の `api_precedence` は実態と合わない）。

## 差戻し（CP の検査で新規、未解決）

CP の tier c（作業指示書 2.3 節）の途中で見つかった。CP は次の二件が解消するまで完了を宣言できない。

| # | 種別 | 場所 | 内容 | 必要な判断 |
|---|---|---|---|---|
| 19 | clippy | `crates/xtask-core/src/cli.rs`：`Command::Check(Config)` | `large_enum_variant`（272 バイト、閾値 200）。Box にすると設計書 5.1 の `Command` の形が変わり、`Config` を小さくすると 5.1 の `Config` の定義と食い違う。CP の `cargo clippy --workspace --all-targets -- -D warnings` を止める | 設計者が決める（前回の報告を参照） |
| 20 | xtask の検査 | `cargo run -p xtask-core -- check`：`module reference cycle among: ...` | 循環参照の違反。xtask-archive（crate、codec）、xtask-diagram（crate、crate_graph、module_graph、scene）、xtask-layout（chains、layering、level、order、position、route）、xtask-modules（thread_local_items、visitor）、xtask-style（style、style_source、style_tools、style_tools::clippy、style_tools::probe）、viewer の xtask-render（crate、bridge、build、highlight、palette、route）。CP の検査（違反 0 件）を満たすには、モジュール間の参照を循環しない形に直す必要がある。直し方（どの型・関数をどの module に移すか）は付属書に無く、設計の判断が要る | 設計者が、循環の解消の方針を付属書に書く（例：共有の型を親の module へ移す、関数を呼ぶ側に寄せる、など） |

Role の欠落（テストモジュール）と `analysis.rs` の行数超過は、機械的な違反なので fix lane（CP-fix2）で直す。

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

## W3・W4 の報告による注意（差戻しではない）

- L7（`hierarchy.rs`）：`push_simplified` と `collinear` が `route.rs` と同じ処理の複製になっている（`route.rs` の関数は private のため、L7 の lane は触らなかった）。後で一つにまとめる。
- V2a：`watch` の「cannot read」の通知は、成功した apply で消える（付属書どおり）。印が元に戻って読み直しが起きない場合は、通知が残る。Open… のダイアログに file の filter は付けていない（付属書に指定が無い）。
- V2b：highlight は、跳びのある edge でも path を直線で描き直すため、交差の位置に強調の直線が出る（付属書どおり）。overlay は透明なので白い halo で隠せない。見た目の扱いは設計者が決める。reveal の拡大率は、今の図の `camera::view` の値を使う（今の図が無いときは開く図の全体に合わせる）。付属書に書かれていない扱いなので、確認が要る。
- V3：clippy の指摘を二件、V2 の file（`app.rs` の `collapsible_if` を let chain に、`state.rs` の `new_without_default` に `Default` を足す）で直した。機械的な指摘で、振る舞いは変えていない。
