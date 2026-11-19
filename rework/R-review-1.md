# レビュー 1 への回答（台帳）

対象：`20261118_2331_CODEX_REVIEW.md`（2026-11-18、対象 HEAD `2788cc8`）の 85 件。回答は設計書・作業指示書・付属書の版 1.1（2026-11-19）に入れた。

## 回答の方針

- 指摘は、固定の commit のコードと Kivact のソースで一件ずつ照合してから判定した。照合で事実と確かめたものは「採用」、方式を変えて採ったものは「採用（方式を変更）」、一部だけ採ったものは「一部採用」と書く。
- 方式を選ぶときは、作者の実装方針（統一的で単純な構造、非効率の無い軽い処理、境界がはっきりしたときだけの crate 分割、暗黙の状態機械の解消、不具合・矛盾・非効率・不要な要素の根本的な整理）を優先した。レビューの提案の方が複雑になる場合は、より単純な方式で同じ欠陥を閉じた。
- 設計の成立を、実装の前に次の小さな見本で確かめた（設計書の scratchpad で実行。リポジトリには入れていない）。
  - `parallel::map`（C01〜C03）：Rust 1.85 で compile し、panic の隔離、入力の順、`R: Send` だけで動くことを実行で確かめた。
  - zlib の復号（A04）：miniz_oxide 0.8.9 の低水準の `decompress` で、後ろの余分なバイト・長さの不一致・adler32 の破損・全切詰めを拒むことを確かめた。
  - Kivact の canvas の大きさ（V03）：固定の commit で、`stack` に二枚の canvas を入れると高さが 0 になり、手で重ねる形（付属書 V の V2d の 1）では両方が残りの大きさを得ることを、`kivact::test` で確かめた。

「確かめ方」は CP で AC-5 を点検するときの証拠。試験名は付属書が定めるもの。

## 1. 実施順序・所有・gate

| ID | 判定 | 回答 | 変更先 | 確かめ方 |
|---|---|---|---|---|
| W01 | 採用 | 優先順位を一本にした：設計書 5 章 ＞ 作業指示書 4 章の追加の指示 ＞ 付属書 ＞ 共通の規則。全文書で同じ文 | 設計書 0 章、作業指示書 0.3 節・1.2 節 | 文書の点検 |
| W02 | 採用 | L6 を W2（L2 の後、同じ lane）に移した。`layout` を通す試験はすべて L7 に置き、L6 の試験は手で作った Layout だけにした。同じ wave で `todo!()` を実行する試験は無い | 作業指示書 4 章、付属書 L 1.3 節・10.4 節 | 作業指示書 4 章の表 |
| W03 | 採用 | GUI の試験は、V3 が 1 回（失敗時にもう 1 回）、W4 の tier b は再実行せず結果を記録、CP で 1 回 | 設計書 6.4 節、作業指示書 0.2 節の 7・2.2 節・4 章 | 文書の点検 |
| W04 | 採用 | 旧い実装は、すべて `git show e7eb600:<path>` で読む | 作業指示書 0.2 節の 14、付属書 A・C・M・V | 文書の点検 |
| W05 | 採用 | lock は lane が commit してよいが、merge のたびに Orchestrator が作り直す。C3 の所有は README と notice の個別の列挙にした。機械的な追従より所有の排他を優先する | 作業指示書 0.2 節の 12・13、1.3 節、付属書 C の C3 | 文書の点検 |
| W06 | 採用 | 雛形で、ソースの編集の範囲と build の出力先を分けた。`<WT>` は W0 で一度だけ絶対 path にする | 作業指示書 1.1 節・1.2 節 | 文書の点検 |
| W07 | 採用 | unit の状態（未着手・実装中・統合済み・差戻し）と、次の unit を起動できる条件、差戻しの解消の手順を定めた | 作業指示書 1.4 節 | 文書の点検 |
| W08 | 採用 | 記録は commit してから push する。CP の記録は検証した commit とともに別の commit にする | 作業指示書 1.3 節・2.3 節・3 章 | 文書の点検 |
| W09 | 採用 | viewer のコマンドは subshell に入れ、残存の検査は `git -C <MAIN> ls-files` にした | 作業指示書 0.6 節・2.2 節・2.3 節 | 文書の点検 |
| W10 | 採用 | node を含まない PATH を作り、`command -v node` が失敗することを確かめてから `--write` を実行する | 作業指示書 2.3 節 | CP の記録 |
| W11 | 採用 | CP に `cargo +1.85.0 check --workspace --all-targets --locked` を足し、AC-6 に入れた。W0 で 1.85 を入れる | 設計書 1.2 節・6.3 節、作業指示書 2.3 節・3 章 | CP の記録 |
| W12 | 採用 | W0 で Kivact を固定の commit で `<WT>/kivact` に clone し、付属書 V はそこを読む | 作業指示書 1.1 節・3 章、付属書 V 0 章 | 文書の点検 |

## 2. 配置エンジン

| ID | 判定 | 回答 | 変更先 | 確かめ方 |
|---|---|---|---|---|
| L01 | 採用 | 試験を、途中の層に他の node があり Dummy が残る形に替えた。Dummy だけの層が消える逆向きの試験を足した | 付属書 L 4.5 節 | `forward_span_three`、`backward_dummy_layer_removed` |
| L02 | 採用 | Fixed の列番号を、異なる値の順位に詰めてから倍にする | 付属書 L 3.2 節 | `fixed_columns_are_compressed` |
| L03 | 採用（方式を変更） | checked の演算を全体に入れる代わりに、座標・rank・重みを `i64` にし、数の上限（node・region 10 万、edge 100 万）を入力の検証に足した。上限の中で和が `10^18` 未満になる根拠を書き、溢れの検査そのものを不要にした。図のファイル（`i32`）への変換は xtask-diagram が行い、範囲外は Failed | 設計書 5.3 節・DEC-4、付属書 L 1.1 節・1.5 節・2.1 節・10.1 節、付属書 A 2.6 節 | `input_rejects_too_many` |
| L04 | 採用 | 空の要求は余白だけの図、空の region は見出しと余白だけの箱、空集合の外接は 0 と定めた | 設計書 5.3 節、付属書 L 8.4 節・9.6 節 | `empty_graph_is_padding_only`、`empty_region_has_header_space` |
| L05 | 採用 | `simplex::solve` と `position::vertical` を `Result` にし、閉路・範囲外・負の重み・入る辺なしを `Internal` にした | 付属書 L 1.4 節・1.5 節・6.1 節・7 章・9.1 節 | `cycle_is_internal_error` |
| L06 | 一部採用 | 契約を「上限の中で最適に届けば最適、届かなければ実行可能解」に直した。打切りの回数の記録は採らない（品質は K1〜K3 で測る） | 付属書 L 6.1 節、設計書 6.2 節 | 文書の点検 |
| L07 | 採用 | 鍵を `(pos, port の順)` の組の辞書順にした | 付属書 L 5.6 節 | `crossings_counts_inversions` |
| L08 | 採用（方式を変更） | 誤った二分探索の記述を消し、全組の比較にした（基準の最大で約百万回の比較。K4 で監視） | 付属書 L 10.2 節、設計書 6.2 節 | 文書の点検 |
| L09 | 採用 | node は要求の大きさ以上、ラベルはちょうど要求の大きさ、region は見出しと余白の大きさ以上で、子は見出しの帯にかからないことを不変条件に足した | 付属書 L 10.2 節、設計書 5.3 節 | `verify_rejects_shrunken_node`、`verify_rejects_header_overlap`、`verify_rejects_label_size` |
| L10 | 採用 | 二本の link の左右を幾何で決め、path の中で先に来る Step を置き換える規則と、前向き・逆向きの例を書いた | 付属書 L 4.3 節 | `backward_dummy_layer_removed` |
| L11 | 一部採用 | L8 を W3 に移し、配置エンジンが揃った W3 の tier b で基準の試験と K1〜K4 を測る（残りの wave と並行して設計者が対処できる）。実装の前の試作は行わない（試作の費用は実装とほぼ同じで、W3 の測定で同じ情報が早く得られる） | 設計書 2 章・9 章、作業指示書 2.2 節、付属書 L 11 章 | W3 の tier b の記録 |

## 3. policy・workspace・source・module 解析

| ID | 判定 | 回答 | 変更先 | 確かめ方 |
|---|---|---|---|---|
| M01 | 採用 | 照合で、import 自体が辺になることを確かめた。glob の供給元（a）と直接の import 先（g）を別の module にした fixture に替えた | 付属書 M の M3b | `local_thread_local_shadows_glob` |
| M02 | 採用 | 宣言と名前付きの use のキーを解決の前に構文から決め、検索と供給を同じ `lookup` にした。隠すかどうかはキーの有無だけで決まるので、伝播は単調で worklist が正しく止まる。thread_local の static も解決の前に登録する | 設計書 DEC-18・DEC-19、付属書 M の M3b | `shadowed_name_is_not_forwarded`（並びを逆にした形を含む）、`three_level_reexport_chain` |
| M03 | 採用 | 宣言と glob の表を `(名前空間, 名前)` のキーにした。名前付きの use は解決まで名前空間が分からないので、三つの名前空間すべてで隠す近似にした（DEC-18） | 付属書 M の M3b | `declared_type_does_not_hide_glob_value` |
| M04 | 採用 | `external_globs` を、名前と同じ規則で glob を通して伝える | 付属書 M の M3b | `external_glob_reexport_keeps_uncertainty` |
| M05 | 採用 | 照合で衝突を確かめた。移行の表で、該当の試験の 4 つ目の source の期待値を変えることを明示した | 付属書 M の M3「既存の試験の移行」 | `role_must_be_first` |
| M06 | 採用 | 照合で確かめた。移行の表で json の行を除き、新しい試験で「信頼が要る」「信頼した macro の中は解析しない」を確かめる | 付属書 M の M3「既存の試験の移行」・M3c | `json_requires_trust` |
| M07 | 採用 | 可視性は C2b が xtask-core の `visibility` として持ち、旧い試験を同じ期待値で作り直す。M1 は workspace から消すだけ | 付属書 M の M1、付属書 C の C2b | `visibility_checks_stale_providers_consumers_and_all_declared_edges` |
| M08 | 採用（方式を変更） | 構造の metadata を `--all-features`・platform の絞込み無しの一回にし、依存の辺を宣言から作る（無効な optional の依存も辺になる）。同名の registry の依存の同一性はその resolve で決める。選んだ構成の feature は別の一回にした（C07 と両立） | 設計書 DEC-17、付属書 M の M1 | `inactive_optional_path_dependency_is_an_edge`、`registry_patch_workspace_edges_are_resolved_by_package_id` |
| M09 | 採用 | `invalidate_current_thread_spans` を library から消した。span の記録は、job の thread と style の thread の終わりで解放される | 付属書 M の M2、付属書 C の C1・C2b | `rg "invalidate_current_thread_spans"` が 0 件 |
| M10 | 採用 | 照合で確かめた。行数の対象に、すべての Cargo target の `src_path` を無条件に入れる | 付属書 C の C2b | `custom_target_roots_are_counted` |
| M11 | 採用 | parse の表に失敗も記憶する。正規化した path で引く | 付属書 M の M3a | `parse_cache_keeps_failures` |
| M12 | 採用（範囲を限定） | item の属性の評価と診断は loader だけが行い、構成ごとの有効な item の列を作る。resolve と visitor は item の cfg を評価しない。本体の中の cfg は visitor だけが評価する。別の「構成別の view」の型は作らない（有効な item の列で足りる） | 付属書 M の M3a | `invalid_cfg_reported_once` |
| M13 | 採用 | `ModuleGraph` の契約（id の昇順、空でなければ `modules[0]` が root、parent は自分より小さい index、root が cfg で無効なら空。W2 の差戻し 7 で、既存の試験に合わせて「無効でも残る」を改めた）と、並べ替えと付け替えを一か所（`finish`）で行うことを定めた。手で作った値の違反は encode の検証が報告する | 付属書 M の M3a、付属書 A 2.5 節 | `modules_are_sorted_with_parents_first`、`cfg_excluded_modules_do_not_require_roles` |
| M14 | 採用 | `MacroGrammar` の variant、名前の表、書式の暗黙の取込みの規則（escape、幅・精度の `name$`、明示の引数の優先、literal でない場合）を定めた | 付属書 M の M3c | `format_capture_rules`、`macro_grammar_table` |
| M15 | 採用 | 照合で確かめた。fixture の helper が `generate-lockfile --offline` で lock を作る | 付属書 M の M1「既存の試験の移行」 | 既存の試験が通る |
| M16 | 採用 | 数の上限の型を今と同じ `usize` にし、`max_code_lines` の上限 1000 も付けない（契約を狭めない） | 付属書 M の M1 | 文書の点検 |
| M17 | 採用 | `Settings` を package ごとに一回作り、同じ package の job で共有する | 付属書 M の M3a、付属書 C の C2b | `rg "settings.clone()"` が 0 件 |

## 4. archive・diagram

| ID | 判定 | 回答 | 変更先 | 確かめ方 |
|---|---|---|---|---|
| A01 | 採用（方式を変更） | 互いに届くかどうか（package の数が小さいので幅優先の全探索）で閉路の構成員を決める。Tarjan より単純で同じ結果 | 付属書 A 2.4 節 | `cyclic_ranks_report_only_cycle_members`、`cycle_bridge_is_not_a_member` |
| A02 | 採用 | 描画が前提にする意味の規則（先頭が Crates、kind の重複なし、当たり判定の一意、大きさ、直交・右向きの経路など）を `validate` にまとめ、decode と encode の両方で通す | 設計書 5.2 節、付属書 A 1.5 節 | `validate_rules` |
| A03 | 採用 | 本体の上限を 64 MiB に下げ、要素数と文字列に確保の予算（本体の 8 倍 + 4096）をかけた。viewer は読む前にファイルの長さを確かめる | 付属書 A 1.5 節、付属書 V の V2a | `huge_count_rejected`、`allocation_budget_limits_decode`、`load_rejects_large_files` |
| A04 | 採用 | 低水準の `decompress` で消費した入力の長さまで確かめる（見本で確かめた） | 付属書 A 1.5 節、設計書 DEC-3 | `trailing_garbage_rejected` |
| A05 | 採用 | `encode` を `Result` にし、検証と大きさの上限を decode と共有する | 付属書 A 1.5 節 | `rejects_bad_reference` |
| A06 | 採用 | 図を作るかを `TargetInput.drawn` に持たせ、並行の配列を消した。`root_name` と `notices` は受け取らない（xtask-core が Archive を組み立てる）。`Drawing` は一つの関数だけが作る | 付属書 A 2.2 節、付属書 C の C2b | `undrawn_targets_still_give_roles` |
| A07 | 採用 | `DependencyKind` を `match` で全 variant 書く手順にした | 付属書 A 2.4 節 | `dependency_kinds_set_style_and_label` |

## 5. core・style・並列処理

| ID | 判定 | 回答 | 変更先 | 確かめ方 |
|---|---|---|---|---|
| C01 | 採用（方式を変更） | 共有の結果の枠をやめ、入力ごとに一つの scoped thread を作り、`join` で結果を受け取る方式にした。`R: Send` だけで済む（見本で確かめた）。コードはそのまま書く形で付属書に置いた | 設計書 DEC-16、付属書 C の C2b | `send_only_results` |
| C02 | 採用 | 各入力が自分の thread を持つので、panic しても他の入力は必ず処理される | 付属書 C の C2b | `panic_isolated_with_one_worker`、`every_input_may_panic` |
| C03 | 採用 | 状態は関数の中の局所の値（走っている handle の表、次の index、終わった結果の表）だけで、番兵も処理中の印も無い | 付属書 C の C2b | `results_follow_input_order` |
| C04 | 採用 | 照合で確かめた。`StyleReport.notes` を足し、今の通知の文面をそのまま運ぶ | 付属書 C の C1 | `tests/style.rs` の通知の assert |
| C05 | 採用 | 照合で確かめた。`Package.id` を残す | 付属書 M の M1、設計書 4.3 節 | clippy の試験（`tests/tools.rs`） |
| C06 | 採用 | 書出しの結果を `Result` にして `Report.errors` に入れ、他の診断を消さない | 設計書 3.2 節・5.1 節、付属書 C の C2b | `archive_write_failure_keeps_check_diagnostics` |
| C07 | 採用 | 致命的なのは policy と構造の metadata だけにし、feature と rustc の cfg（解析の環境）の失敗は全 target の Failed と一件の誤りにして、他の検査を続ける | 設計書 5.1 節、付属書 C の C2b | `early_analysis_and_file_errors_do_not_hide_other_checks_or_the_crate_diagram` |
| C08 | 採用 | 照合で確かめた。範囲を不変の `Scope` にまとめ、入れ子では子の `Checker` を作る。save/restore を無くした | 付属書 C の C1 | `nested_scopes_do_not_leak` |
| C09 | 採用 | 各ファイルを一回だけ parse し、診断と item の行の索引を同時に作って AST を捨てる。読み直しの関数を消した | 付属書 C の C1 | `item_index_names_line_diagnostics`、`rg "fn source_items"` が 0 件 |
| C10 | 採用（方式を変更） | probe の一括化そのものを取りやめた（名前の解決の誤りで他の selector の型検査が止まる）。selector ごとの crate を保ち、一時 directory と lock の写しだけを共有する。一括化は後続 | 設計書 DEC-20・12 章、付属書 C の C1 | 文書の点検 |
| C11 | 採用 | 照合で、process を起動する試験を 8 件と確定し、名前で列挙した | 付属書 C の C1「既存の試験の移行」 | 文書の点検 |
| C12 | 採用 | 照合で確かめた。fixture を `path='src/*'` に替え、元の不変条件を保つ。`*` だけの path を拒む試験は別に置く | 付属書 C の C2「既存の試験の移行」、付属書 M の M1 | `wildcard_exceptions_cannot_suppress_invalid_rule_targets`、`bare_wildcard_exception_rejected` |

## 6. viewer・描画・操作

| ID | 判定 | 回答 | 変更先 | 確かめ方 |
|---|---|---|---|---|
| V01 | 採用 | 読込みの要求を `Request`（id 付き）で持ち、古い完了は `accepts` で捨てる。走っている間は見張りが読込みを重ねない。最後に読もうとした印（`attempted`）を文書に持ち、同じ壊れたファイルを読み直さない | 付属書 V の V1 の state.rs、V2a | `stale_completion_is_rejected`、`failed_reload_keeps_document_and_records_stamp` |
| V02 | 採用 | 表示中の図と履歴を `DiagramKind`（安定したキー）で持ち、読直しで消えた図を除く。図の移動と読直しで固定を解く。移動は `navigate` と `document::apply` の純粋な関数だけで行う | 付属書 V の V1 の state.rs、V2a、V2b | `reload_keeps_diagram_and_filters_history`、`back_pops_and_clears_pin` |
| V03 | 採用 | 見本で、`stack` に二枚の canvas を入れると高さが 0 になることを確かめた。手で重ねる形に替え、GUI 試験で実際の大きさを確かめる | 付属書 V 0 章、V2d | `canvas_has_positive_size` |
| V04 | 採用 | 押した時は `Pressed`、3 px 動いたら `Dragging` にし、固定は離した時に決める。region の上からもドラッグできる | 付属書 V の V2d | `drag_on_region_pans` |
| V05 | 採用 | 照合で、同じ key の Part が保たれることを確かめた。Part の名前と handler は item の `Value` を読む closure にする | 付属書 V 0 章、V2d | `double_click_opens_module_diagram_and_renames_regions` |
| V06 | 採用 | 照合で、SVG の出力が候補の列を一つの名前として quote することを確かめた。`FONT_FAMILY` を総称の `monospace` にした（画面の側も総称に対応していることを確かめた） | 設計書 DEC-12、付属書 A 1.6 節 | `export_svg_matches_screen_paint` |
| V07 | 採用 | 新しく開く（crate 図・空の履歴・Fit）と同じファイルの読直し（図と視点を保つ）を、`apply` の表の別の行にした | 設計書 5.5 節、付属書 V の V2a | `open_success_starts_at_crates` |
| V08 | 採用 | 見張りの loop の局所の回数で、2 回続けて情報を取れないときだけ一度通知する。戻れば印が変わり、読直しの成功で通知が消える | 設計書 5.5 節、付属書 V の V2a | 文書の点検 |
| V09 | 採用 | `Gesture` が押した時の視点（`View`）全体を持つ。gesture の間の wheel は無視し、Leave・Cancel・Up の扱いを表にした | 付属書 V の V1 の state.rs、V2d | `drag_on_region_pans` |
| V10 | 採用 | 跳びを `Picture.bridges` に分け、全 edge の後に halo と一緒に描く | 付属書 V の V1 の手順 3、V2b | `crossing_adds_bridge`、`bridge_order_does_not_depend_on_edge_order` |
| V11 | 採用 | 同じ x の交点を一つにし、直前に残した交点から 8 未満のものを除く | 付属書 V の V1 の手順 3 | `close_crossings_are_merged` |
| V12 | 採用 | package の一覧を、crate 図の item と Modules の図の package の和から作る | 付属書 V の V2c | `failed_crate_diagram_still_lists_packages` |
| V13 | 採用 | 固定（`Nav.pinned`）と hover を独立の field にし、固定の間も tooltip を出す。role が無い item も名前を出す | 付属書 V の V1 の state.rs、V2d | 文書の点検 |
| V14 | 採用（契約を変更） | 照合で、`on_activate` が Enter かダブルクリックだけで呼ばれることを確かめた。契約を「単クリックは選択、Enter かダブルクリックで開く」に変えた | 設計書 5.5 節・DEC-21、付属書 V の V2c | 文書の点検 |
| V15 | 採用 | 照合で、prelude が attribute の `component` を再 export することを確かめた。`trusted_globs` の意味を「未知の提供元とみなさない」に直し、glob の attribute を使うなら `trusted_attributes` にも書くとした | 設計書 DEC-9、付属書 V の V1 の手順 8、付属書 C の C3 | 文書の点検 |
| V16 | 採用 | 画面の計算は Picture の座標だけを使う。`item_rect` を足した | 付属書 V の V1 の手順 3、V2b | `item_rect_uses_picture_coordinates`、`center_puts_rect_in_middle` |
| V17 | 採用 | 照合で escape を確かめた。期待値を escape した題にした | 付属書 V の V3 | `export_svg_matches_screen_paint` |
| V18 | 一部採用 | 交差の探索を、垂直の線分を x で並べた二分探索にした。`neighbors` は hover の変化ごとに edge を一度たどる形を保つ（edge は数百本）。Picture は読込みの thread で作るので UI を止めない。応答時間の受入れの項目は足さない（Kivact の大きさの図のファイルは、利用側の更新まで作れない） | 付属書 V の V1 の手順 3、設計書 6.2 節 | 文書の点検 |
| V19 | 採用 | 照合で、Offscreen が `!Send` であることを確かめた。SVG の生成と書込みを `blocking` の中で行い、Offscreen はその中で作る。書出しの対象はクリックした時の Picture の写し | 設計書 DEC-13、付属書 V の V2a | 文書の点検 |
| V20 | 採用 | 字幅の見積りは保証ではないと書き直し、はみ出しても配置の不変条件は壊れないことを書いた | 設計書 DEC-12、付属書 A 1.6 節 | 文書の点検 |

## 7. 受入れ判定・試験・文書

| ID | 判定 | 回答 | 変更先 | 確かめ方 |
|---|---|---|---|---|
| Q01 | 採用 | K6 を、build 済み・暖機 1 回・3 回の中央値・1.05 倍の許容で測る手順にした | 設計書 2 章、作業指示書 2.4 節・3 章 | CP の記録 |
| Q02 | 採用 | KPI は自動の gate ではないが完了の条件と明記し、K1〜K5 の測り方（対象、集計、単位）を固定した | 設計書 2 章、付属書 L 11.3 節 | 文書の点検 |
| Q03 | 採用 | 設計書 7 章の監査の ID を一つずつに分け、「確かめ方」の欄を足した。本書もその台帳になる | 設計書 7 章、本書 | CP の点検 |
| Q04 | 採用 | 旧い図の試験を「維持・置換・削除・契約の変更」に分ける移行の表を作り、維持するものは図のファイルへの assert にした | 付属書 C の C2「既存の試験の移行」 | 表の試験が通る |
| Q05 | 採用 | 遷移・座標・木・読込みの規則を純粋な関数にし、V2 の各 lane が単体試験で確かめる。GUI 試験は結線と大きさ・gesture に絞った | 設計書 6.4 節、付属書 V の V2・V3 | V2 の単体試験 |
| Q06 | 採用 | 「独立に変わりうる同じ事実を二度持たない」と書き直し、不変の snapshot の中の派生の索引は許すとした。`Layers` の使わない欄の 0 は消した | 設計書 6.1 節、付属書 L 2.2 節 | 文書の点検 |

## 8. レビューの 9 章（実装 AI へ渡す前の条件）への対応

| 条件 | 対応 |
|---|---|
| 1. P1 を解消した契約を先に凍結する | 上の表のとおり。並列、glob の解決、宣言の依存、ModuleId、archive の検証、viewer の読込みと遷移、canvas と gesture を版 1.1 で定めた |
| 2. compile 可能な見本で成立を確かめる | 並列の map、zlib の復号、Kivact の canvas の重ね方を見本で確かめた（「回答の方針」）。glob の連鎖は試験で、配置の KPI は W3 の測定で確かめる |
| 3. 既存の試験の移行表を作る | 付属書 M の M1・M3、付属書 C の C1・C2 に表を置き、表に無い期待値の変更を禁じた |
| 4. wave と gate を組み直す | 作業指示書 1.4 節・2 章・4 章 |
| 5. unit の粒度 | M3 を M3a〜M3c、C2 を C2a〜C2c、V2b を V2b と V2d に分けた（同じ lane で順に行う） |
| 6. 受入れ台帳 | 設計書 7 章の「確かめ方」と本書 |
