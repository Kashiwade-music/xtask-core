# 付属書 F：完成後のレビューによる修正（CP の後）

CP（`27f0849`）の後に、設計者が 5 つの領域を独立にレビューさせ、再現で確かめた所見を設計者が判定した。本付属書は、採った所見の直し方を定める。採らなかった所見は 7 節。

共通の規則：
- 作業指示書 0.2 の禁止事項と、設計書 6 章の規則に従う。特に、型・共有の関数は葉の module に置き、下位の module は crate root を参照しない（設計書 6.3 節の 6）。
- 既存の試験の期待値は変えない。各節の「試験」を足す。
- 各 lane の確認は、その crate の `cargo check --all-targets`、`cargo test`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt -p <crate>`。

---

## F1：`xtask-layout`

1. **均衡の計算量（`layering.rs` の `balance`）。** 層ごとの数の表 `counts: Vec<usize>`（長さは最大の rank + 1）を最初に一度作る。各 node の移し先は `counts[low..=high]` を走査して決め（数が最も少ない層、同数なら小さい層）、`counts[target] < counts[rank[node]]` のときだけ移し、移したら二つの数を直す。`crowd` 関数は消す。結果は今と同じ（3 章の規則は変えない）。
2. **閉路の除去の計算量（`acyclic.rs` の `Remaining`）。** `Remaining::new` で node ごとに接する辺の index の列 `touching: Vec<Vec<usize>>` を作り、`remove` は `self.arcs` 全体ではなく `self.touching[node]` だけをたどって in・out を直す。結果は今と同じ。
3. **`order.rs` の `path_order` を消す。** link は path の順に作られ、各 link は一つの path にだけ属し、path の中で index が増える（`chains.rs` の `finish`）。したがって path の順の比較は link の index の比較と同じ。`path_order` とその引数を消し、TurnEast・TurnWest の tie-break と port の並べ替えは `(相手の pos, link の index)` で比べる。`Option` の欄が無くなる。
4. **参照の検査を一つにする。** `level.rs` に、`pub(crate) fn check_chains(chains: &Chains) -> Result<(), LayoutError>`（link の両端と path の link の index が範囲内）と `pub(crate) fn check_order(chains: &Chains, order: &Order) -> Result<(), LayoutError>`（`east`・`west` の長さが link の数、`layers` が全 item を一度ずつ含む）を置く。`hierarchy.rs` の `run_level` で、`chains::build` の後に `check_chains`、`order::arrange` の後に `check_order` を一度ずつ呼ぶ。`position.rs` の `check_references` と `route.rs` の同じ検査を消す。`order.rs` の `get`・`get_mut` で範囲外を黙って飛ばす所は、直接の index にする（検査済みのため）。
5. **`chains.rs` の `Span.region: bool` を消す。** Root か Region かは `input.frame` が持つ。`kept_layers` に `&input.frame` を渡し、`Frame::Region { .. }` で分ける。
6. `run_level` は `hierarchy.rs` のままにする（付属書 X の X3。`level.rs` に戻すと module の閉路が戻る）。

試験：既存の試験と基準の試験がそのまま通ること。足す試験は無い。

---

## F2：`xtask-archive` と `xtask-diagram`

1. **自分への依存の辺を図に入れない（`xtask-diagram/src/crate_graph.rs` の `edges`）。** `dev-dependencies` で自分の path を指す（feature を有効にする定番の書き方）と、`from == to` の辺ができ、配置が `SelfLoop` で失敗して crate 図全体が Failed になる。`edges()` で `from == to` の依存を飛ばす（module 図の 2.5 節と同じ）。列の計算（`normal_ranks` など）には影響しない。
   - 試験 `self_dev_dependency_is_skipped`：package a、b（a が b に Normal）と、a の a への Dev の依存で、crate 図が `Content::Drawn` で edge が 1 本。
2. **確保の予算を encode と decode で同じにする（`xtask-archive/src/codec.rs`）。** 予算の計算を一つにする：`Writer` も `Reader` と同じ費用（`vec<T>` の要素数 n に `n * size_of::<T>()`、`str` の n バイトに n、items の列は下の 3 の費用）を数え、`write_body` の最後に、費用が `8 * 本体の長さ + 4096` を超えれば `Err("archive is too large")` を返す。費用の関数（要素の型ごとの費用）は一つだけ置き、Writer と Reader の両方が使う。
   - 試験 `encode_rejects_what_decode_would_reject`：Crates の図と、最小の Modules の図（package・target・kind が 1 文字、題が空、`Skipped("")`）700 枚を、kind の昇順で並べた Archive で、`encode` が `Err("archive is too large")`。
3. **`validate` の一時の確保を予算に入れる（`validate.rs` の `check_coverage`）。** `referenced` の列を集めず、`scene.regions`（無ければ `scene.nodes`）を直接たどって、item ごとの参照の数を `Vec<u8>`（飽和の加算）で数える。この 1 バイトの分を、Reader が items の列を読むときの要素の費用に含める（`size_of::<Item>() + 1`）。2 の費用の関数で同じ値を使う。

---

## F3a：`xtask-modules` の正しさ（F3b の前）

1. **block の中の attribute を、block の `use` の後に調べる（`visitor.rs`）。** 今は block の item ごとに `enabled()` が cfg と同時に `expansions()`・`intrinsic_derives()` を評価し、その後で `block_uses()` が block の glob を足す。そのため `fn f() { use ext::*; #[derive(Clone)] struct S; }` で、module の直下なら出る `derive macro expansion is unsupported: Clone` が出ない（旧い実装からの後退）。
   - `enabled` を二つに分ける：cfg を評価して生きているかと展開済みの attribute を返す部分と、展開の診断（`expansions`）の部分。block では、(1) 全 item の cfg を評価し、(2) 宣言を足し、`block_uses` を行い、(3) その後で生きている item ごとに展開の診断を行う。module の直下も同じ順にする。
   - 試験 `block_glob_reaches_block_derive`：上の source で、`derive macro expansion is unsupported: Clone` の診断がある。block の中の `use other::Clone;` も `intrinsic_derives` で考える。
2. **`extern crate` の名前を glob より上に置かない（`resolve.rs` の `add_externs`）。** 今は root の `extern crate x;` の名前を、root 以外の全 module の `declared` に三つの名前空間で足すので、glob で入った同名の関数を隠す。Rust では extern prelude の名前は Type の名前空間だけで、glob に負ける。
   - root の `extern crate`（`extern crate self as foo;` を含む）の名前は、別の「extern prelude」の表に置く。引きでは `declared`・`explicit`・`globbed` のどれにも無いときだけ、Type の名前空間でだけ引く。glob では伝えない。
   - 試験 `extern_crate_does_not_hide_glob_value`：root に `extern crate alloc;`、`mod a { pub fn alloc() {} } mod g { pub use crate::a::*; } mod m { use crate::g::*; fn h() { alloc(); } }` で、`crate::m` から `crate::a` への辺がある。
3. **std の `thread_local!` は、信頼に無くても診断せず、中の型と初期化式を visit する（付属書 M3b の手順 6 のとおり）。** 今は item の位置で `trusted_macros` に無いと `item macro expansion is unsupported: thread_local` を出し、型を visit しない。block の中（`Stmt::Macro` として `visit_macro` に来る）では、信頼されていると早く return して中を visit しない。
   - std と判定した呼出し（付属書 M3b の手順 6 の 1〜3）は、信頼の有無にかかわらず `Standard.calls` に記録し、診断を出さず、static の型と初期化式を visit する。
   - `visit_macro` では、信頼による早い return の前に、std か信頼された `thread_local` の呼出しを `enabled_statics` で扱う。
   - std でない呼出しは今のまま（信頼に無ければ `item macro expansion is unsupported`）。
   - 試験 `untrusted_standard_thread_local_is_visited`：既定の Settings で `thread_local!{ static CACHE: RefCell<crate::cache::C> = RefCell::new(crate::cache::C); }` が、診断無しで `crate::cache` への辺を作る。試験 `block_thread_local_is_visited`：`fn f() { thread_local!{ static X: RefCell<crate::cache::C> = RefCell::new(crate::cache::C); } }` で、`crate::cache` への辺がある。
   - 既存の試験が、std の `thread_local!` に `item macro expansion is unsupported` を期待して落ちるなら、その試験を変えずに STOP する。
4. **未知の外部 glob の下の prelude の attribute macro（付属書 M3b の手順 9、D-C2）。** std の prelude の attribute macro は glob で隠されうる。glob で隠されない組込みの inert な attribute（`inline`、`cfg`、`doc`、`allow` など）は対象にしない。
   - 引きの道筋に未知の外部 glob があるとき、attribute の名前が `test`、`bench`、`global_allocator`、`test_case` のどれかなら、`{module}: attribute provider cannot be established: {name}` の診断を一つ出す。`derive` は対象にしない（中の derive macro の診断が既にある）。
   - 試験 `prelude_attribute_under_unknown_glob`：`mod c { use ext::*; #[test] fn f() {} #[inline] fn g() {} }`（cfg に `test`）で、診断がちょうど 1 件で `test` を含む。既存の `unknown_external_globs_require_known_macro_providers`（2 件）は変わらない。
5. **raw identifier の module の file 名（`loader.rs`）。** `mod r#type;` の file 名は `type.rs`（`type/mod.rs`）。file 名を作るときは `syn::ext::IdentExt::unraw` した名前を使う。module の id も unraw した名前にする。
   - 試験 `raw_identifier_module_file`：`mod r#type;` と `type.rs` で解析が成功し、module `crate::type` がある。

## F3b：`xtask-modules` の整理（F3a の後）と `xtask-policy`

6. **worklist の戻し先（`resolve.rs` の `settle`）。** `layout.uses` から「module → その module を use で参照する module の列」の逆引きの表を一度作り、値が増えた module を参照する module だけを worklist に戻す（付属書 M3b の手順 3）。`settle` と `import_targets` の `external: bool` は `enum Fallback { LocalOnly, External }` にする（設計書 6.1 節の 1）。
7. **glob の信頼の判定を一つにする（`thread_local_items.rs`）。** 信頼の判定と `is_standard_glob` を消し、`symbols::is_trusted_glob` を `pub(crate)` にして使う（先頭の segment が局所の Type に解決されないことの確かめは残す）。使われていない `Provider { Qualified, Alias }` を消し、`provider` を `fn names_thread_local(..) -> bool` にする。
8. **一つの表と一つの関数にする。**
   - `visitor.rs` の、模型化済みとして別に持っている macro の列（`stringify`、`concat`、`env`、`file`、`include_str` など）を、`macros::grammar` の表に `Opaque` として移し、別の列を消す。
   - path を手で文字列にしている所（`visitor.rs`、`cfg.rs`、`thread_local_items.rs`）を `ast::path_text` にする。
   - `resolve.rs` の `declare` が `use` ごとに `use_imports` を呼び直す所を、`layout.uses` の結果を使う形にする。
   - `xtask-policy` の例外の path が `*` だけのときの誤りの文を、付属書 M1 の手順 4 の `exception path must name a file or a directory prefix` にする（既存の試験の期待値が文の一部だけを見ていれば、そのまま通る）。

---

## F4a：`xtask-style`

1. **rustfmt の width の問合せの群（`style_tools.rs` の `config_groups`）。** root の外の file（`[[bin]] path = "../ext/tool.rs"` など）が先に並ぶと、root の群の問合せに root の外の file が使われ、root の `rustfmt.toml` が読まれない。
   - root の下の file は、最も近い設定の directory（無ければ root）で群にする。root の外の file は、その file から上へ最も近い設定の directory（無ければ file の親の directory）で群にする。
   - 群ごとの問合せには、その群の directory の下の file を使う。root の群は必ず root の下の file で問い合わせる。
   - 試験（`tests/tools.rs`。process を起動する）`root_rustfmt_config_wins_over_outside_files`：root に `rustfmt.toml`（`max_width = 120`）、`[[bin]] path = "../ext/tool.rs"` の fixture で、width の診断が出る。

## F4b：`xtask-core` と README

2. **`examples/benchmark.rs`。** 引数は ROOT だけにし、`Output::None` と `Output::Policy` の二つを順に実行して、それぞれの時間を出す（README の Development の節と付属書 C の C2c の手順 5 のとおり）。
3. **解析の段階の未完成の object と共有の `Report`（`analysis.rs`、`run.rs`）。** 設計書 6.1 節の 2 と付属書 C の C2b の手順 1 に合わせる。
   - `plan` は `Vec<Planned>` を返す。`enum Planned { Done(TargetAnalysis), Job(Job) }`。`Done` は解析しない target（feature で無効、未対応の edition）で、outcomes まで完成した値。`Job` は解析する target。
   - `parallel::map` の後に、`Job` の結果から `TargetAnalysis` を一度で作る。空の outcomes を作って後で埋めない。
   - 解析の段階は `(Vec<TargetAnalysis>, Findings)` を返す。`Findings { errors: Vec<String>, notes: Vec<String> }`。`plan`・`skipped`・`record`・`trusted_note` は `&mut Report` を受け取らず、自分の Findings を返す。
   - `Report` は `run` の最後に、各段階の結果から一度で作る。図のファイルの notices は、解析の段階の Findings をそのまま使う（`run.rs` で「後の検査が足す前に写す」順序の依存を消す）。
4. **file → module の表が無い file の item の名前（`sources.rs`）。** 表は解析に成功した module からだけ作るので、解析の環境の失敗や、feature で無効・未対応の edition の target の file では、item の例外が当たらなくなる（独立な検査が、解析の環境の失敗に影響される）。
   - 表に無い file は、その file を含む target（target の順で最初のもの）の root の directory からの相対 path で module の path を求める。root の file は `crate`。`a.rs` は `crate::a`、`a/mod.rs` は `crate::a`、`a/b.rs` は `crate::a::b`。表の他の値と同じ書き方にする。
   - 試験（`tests/style.rs`）`item_exceptions_survive_analysis_environment_failure`：`src/a.rs` に hex-literal の違反と、その item の例外を持つ fixture に `[analysis] target = "bogus-target-triple"` を足しても、hex-literal の違反と `unused style exception` が報告されない。
5. **可視性の規則を一つにする（`visibility.rs`）。** `invisible_edge` と `matches` を消し、`xtask_policy::Policy::visible` を使う（規則は xtask-policy だけが持つ）。`visibility` の引数は `&Policy` にしてよい。
6. **crate root の参照（設計書 6.3 節の 6）。** `cli.rs`、`context.rs`、`archive.rs` の `use crate::{Config, Output, run}` などを、定義した module の path（`crate::config::{Config, Output}`、`crate::run::run`）にする。
7. **README の解析の説明。** `README.md` と `README_JP.md` の「各 target の normal と cfg(test) の両方を解析する」を、実際の規則に直す：test・bench の target は cfg(test) だけ、`test = false` の target（既定の example を含む）は normal だけ、feature で無効な target は normal の一件（FeatureGated）。二つの README を同じ内容にする。
8. **feature の表の clone（`run.rs`）。** feature の結果は `run` の中で `Result<BTreeMap<..>, String>` として一度だけ持ち、解析と `StyleInput.features` の両方が借りる。`Environment` に入れて clone しない。

---

## F5：viewer（`viewer/crates/xtask-viewer`）

1. **tooltip が pointer を奪う（`canvas.rs`）。** `overlay_at(..)` の結果に `.hit_testable(false)` を付ける（内側の text だけでは足りない）。
2. **空の検索欄の Backspace が Back になる（`sidebar.rs`）。** 検索の `text_input` に `.on_key(|e| if e.key() == Key::Backspace { Handled::Yes } else { Handled::No })` を付ける。
3. **文書や図が替わっても押した item が残る（`canvas.rs`、`document.rs`、`navigate.rs`）。** `hover` を None にしている所（`document::open` の適用の後、`navigate::go`・`go_back`・`reveal`）で、`state.gesture.set(Gesture::Idle)` も行う。`Up(p)` は `p.button == MouseButton::Primary` のときだけ扱う。
4. **見張りの古い読直し（`document.rs` の `watch`）。** Stamp を `blocking` で取った後、`request` が `Idle` で、`document` の path と `attempted` が取る前と同じときだけ、読直しを始める。違えば、その回は何もしない。
5. **canvas に入った最初の pointer で hover が付かない（`canvas.rs`）。** `PointerPhase::Enter` を、gesture が Idle のときの `Move` と同じに扱う。
6. **強調した edge の跳び（`paint.rs` の `highlight`）。** 強調の path を描いた後、その edge の bridge ごとに、白い halo（`EDGE_WIDTH + 3`、丸い端）と ACCENT の半円を、base の `bridges()` と同じ形で描く。
7. **大きさの確かめと読込みを一つにする（`document.rs` の `load`）。** `File::open(path)?.take(MAX_FILE_LEN + 1).read_to_end(&mut bytes)` で読み、`MAX_FILE_LEN` より多く読めたら `file is too large` の誤りにする。`metadata` の長さだけで判定しない（Stamp には今のとおり `metadata` を使う）。

試験（`document.rs` の単体試験）：`load_rejects_large_files` がそのまま通ること。1〜6 は GUI の結線で、`tests/viewer.rs` の既存の試験がそのまま通ること。

---

## 6. 進め方

lane ごとに一つの worktree で行う：F1、F2、F3a → F3b、F4a、F4b、F5（最大 6）。lane は別の crate のファイルだけを変える（F3b の `xtask-policy` の文は F3 の lane が持つ）。設計者がすべてを merge し、作業指示書 2.3 節の CP を最初から実行する。

## 7. 採らなかった所見

- `run_level` を `level.rs` に戻す（layout）：付属書 X の X3 で閉路を解くために移した。戻すと閉路が戻る。
- `decompose` の O(深さ²)（layout）：深さ 400 で 86 ms。付属書が求める計算量の中で、実際の図の深さは小さい。
- `verify` に `i64::MAX` 近くの手作りの `Rect` を渡すと溢れる（layout）：公開の `verify` に渡るのは配置の結果か、10.1 節の上限の中の値だけ。
