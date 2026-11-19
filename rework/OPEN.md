# 未解決の事項

## 差戻し（設計者への差戻し）— 解消済み

| # | 時期 | unit | 場所 | 解消 |
|---|---|---|---|---|
| 1〜5 | W1 | M1、M2 | `xtask-policy`、`xtask-workspace`、`xtask-source` | 設計者が d196eaa で付属書を改訂し、fix lane（f6685ca、286038e）で解消。W1 tier b 成功（d3641fc） |
| 6 | W2 | L2 | `chains.rs`：`backward_dummy_layer_removed` の期待値 | 設計者が付属書 L 4.5 を改訂（期待値は Step 6、向き `[true,false,false,false,false,true]`、`layer_count` 5）。fix lane `W2-1-fix`（3e8cfba）で試験の期待値を合わせた |
| 7 | W2 | M3a | `analysis.rs`：`disabled_root_keeps_root_module` | 設計者が契約を改めた（root が cfg で無効なら modules は空、診断も無い。既存の `cfg_excluded_modules_do_not_require_roles` を守る）。試験は削除（fix lane 3f3c058） |
| 8 | W2 | M3a | 手順からの逸脱（(a) cfg_attr の二度評価、(b) file:line の欠落、(c) roles の試験の書換え、(d) 手順 7 の残り） | 設計者が付属書を改訂し、fix lane 3f3c058 で (a)(b)(d) を直した。(c) は移行の表に載せた（既存の期待値は変えず、名前と削除した API の assert だけ） |

W2 の tier b（root の check、xtask-archive・xtask-policy・xtask-source・xtask-workspace・xtask-modules・xtask-layout の試験、clippy の三 crate、viewer の check と xtask-render の試験）は、`9e9ea2f` の時点で成功。ただし W2 のウェーブの中の M3b・M3c は未実施。

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
