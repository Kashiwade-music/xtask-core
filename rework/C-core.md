# 付属書 C：`xtask-core`、`xtask-style`、文書

設計書 5.1 節の契約（CLI と library の API）を実装する手順。旧い図の経路（Node、SVG、HTML）を消す。

旧い実装は `git show e7eb600:<path>` で読む。

---

## C1：`xtask-style` の新設

### 所有

- `crates/xtask-style/**`（新設）
- `crates/xtask-core/src/style.rs`、`style_source.rs`、`style_source/**`、`style_tools.rs`、`style_tools/**`（ここから移して消す）
- `crates/xtask-core/tests/style.rs` は移さない。xtask-core の公開 API を通す端から端の試験なので、C2c が新しい API に直す。

### 手順

1. **crate の新設と移動。** `crates/xtask-style` を作り、上の xtask-core のファイルを移す。
   - 依存は `xtask-policy`、`xtask-workspace`、`xtask-source`、`syn`、`proc-macro2`、`toml`、`serde_json`、`tempfile`。
2. **公開 API。** 次の一つにする。

   ```rust
   pub struct StyleInput<'a> {
       pub root: &'a Path,                       // xtask_source::normalize 済み
       pub workspace: &'a xtask_workspace::Workspace,
       pub analysis: &'a xtask_policy::AnalysisSettings,
       /// 選んだ構成の feature（`Workspace::features` の結果）。Err なら compiler の検査はその文を errors に入れて行わない。
       pub features: &'a Result<BTreeMap<String, BTreeSet<String>>, String>,
       pub style: &'a xtask_policy::StyleSettings,
       pub files: &'a [PathBuf],                 // 検査する .rs（正規化・重複除去済み）
       /// file → その file の module の path（`crate::a::b`）。inline module の item の名前付けに使う。
       pub modules: &'a BTreeMap<PathBuf, String>,
   }
   pub fn check(input: &StyleInput<'_>) -> StyleReport;
   pub struct StyleReport { pub diagnostics: Vec<StyleDiagnostic>, pub errors: Vec<String>, pub notes: Vec<String> }
   ```

   - `StyleDiagnostic` の型は xtask-core の `model.rs` から移す。
   - `errors` は、道具の実行の誤り（rustfmt の未導入、compile の失敗など）とファイルの読込み・parse の誤り。
   - `notes` は、今の `style.rs` が出している通知をそのまま持つ：適用した例外とその理由（`style exception [..] ..: {reason}`）、使われなかった例外（`unused style exception [..] ..`）、検査した規則の数（`checked {n} style rules; ...`）。文面は変えない。
   - `ToolReport` を消し、`StyleReport` に一本化する（S4）。
3. **一回の parse と item の索引（B1、I2）。** `style_source::check(files, settings) -> SourceReport` にする。

   ```rust
   pub(crate) struct SourceReport { diagnostics: Vec<StyleDiagnostic>, errors: Vec<String>, items: BTreeMap<PathBuf, Vec<ItemSpan>> }
   /// file の中の item の行の範囲（1 から、両端を含む）と、module の中の名前。start_line の昇順。
   pub(crate) struct ItemSpan { start_line: usize, end_line: usize, item: String }
   ```

   - 各ファイルを一回だけ読み、一回だけ parse する。読込みと parse の誤りはそのファイルの `errors` に入れて、次のファイルに進む（一つの壊れたファイルで全体を止めない）。
   - parse した AST から、構文の診断と `ItemSpan` の列を同時に作り、AST はそのファイルを終えたら捨てる（全ファイルの AST を持ち続けない）。
   - 旧い `style.rs` の `source_items`（診断のあるファイルを読み直して parse する関数）を消し、`items` の索引で行から item の名前を引く。
   - `style_source::check` は、`check` の中の `std::thread::scope` の一つの thread で走らせる（手順 9）。その thread が終わると proc-macro2 の span の記録も解放される（付属書 M の M2 の手順 5）。
4. **visitor の状態（設計書 6.1 節の 4）。** `Checker` の `inline`、`in_block`、`module_items` の save/restore をやめる。
   - 範囲の情報を `#[derive(Clone, Copy)] struct Scope<'a> { module: ModuleScope, items: &'a [Item], in_body: bool }`（`enum ModuleScope { File, Inline }`）にまとめ、`Checker` は `scope: Scope<'a>` を持つ。`Checker` の `scope` は作った後に書き換えない。
   - inline module と block に入るときは、新しい `Scope` を持つ子の `Checker`（`diagnostics` は同じ `&mut Vec` を借りる）を作って visit する。親の `Checker` の `scope` は変わらないので、戻す処理は要らない。
5. **root の一本化（B4）。**
   - `style.rs` の `canonicalize(config.root)` と `without_verbatim_prefix` を消す。root は呼び出し側が `xtask_source::normalize` 済みのものを渡す。
   - `probe.rs`、`clippy.rs`、`style_tools.rs` の `root.join(...)` は、この root を使う。
   - clippy の `file_name` を join した後、その path を `xtask_source::normalize` で正規化する。
6. **並べ替えの順（B7）と file の一覧（C6、I3）。**
   - path の正規化を先に行い、その後で sort と dedup を行う。
   - `style.rs` の中のファイルの探索（`rust_files` の二度目の呼出し）を消し、`input.files` を使う。
   - `source_items` の中の、module ごとの canonicalize の loop を消し、`input.modules` で引く。
7. **metadata と probe（I1、DEC-20）。**
   - `probe.rs` の `cargo metadata` の実行と `resolved_features` の呼出しを消し、`input.workspace` と `input.features` を使う。`input.features` が `Err(e)` なら、probe は `style options-default: {e}` を `errors` に一つ入れて終わる（今の動作と同じ）。
   - clippy は今のとおり `Package.id` で検査の群を作り、Cargo の JSON の `package_id` で診断を選ぶ。
   - root の `Cargo.toml` の parse は、`check` の呼出しの中で一回だけ行い、各 probe に渡す。
   - probe は、selector ごとに一つの crate を作る今の方式を保つ（一つの crate にまとめない。DEC-20）。各 probe の一時 directory と `Cargo.lock` の写しは、`check` の中で一つ作った一時 directory の下に置いて共有する。
8. **rustfmt の width の問合せ（I4）。**
   - `width_limit` は、source の directory ごとではなく、最も近い `rustfmt.toml`（または `.rustfmt.toml`）の directory ごとに一回だけ `rustfmt --print-config` を実行する。
   - 設定ファイルが無い directory はすべて root の一回にまとめる。
9. **独立な処理の並列化（I5）。** source の検査（手順 3）、fmt の検査、clippy の群は、`std::thread::scope` で並列に走らせる。結果は「source、fmt、clippy の群の順」に連結する。
10. **重複の検証の削除（C5）。** `style.rs` の先頭の policy の再検証（`style.validate()` 相当）を消す。

### 既存の試験の移行

| 試験 | 移行 |
|---|---|
| `src/style_tools/tests.rs` の次の 8 件（外部の process を起動する）：`real_clippy_distinguishes_standard_aliases_and_same_named_methods`、`changed_clippy_configuration_rechecks_cached_sources`、`default_probe_distinguishes_missing_impl_and_invalid_selector_and_preserves_lock`、`cross_package_feature_requests_are_accepted_for_each_package`、`configured_cfg_activates_compiler_checked_code`、`format_uses_project_width_and_rejects_widths_over_the_limit`、`flexible_references_include_borrowed_options`、`unwrap_is_reported_in_production_code_but_not_in_test_code` | `crates/xtask-style/tests/tools.rs`（統合試験）に移す。公開の `check` を通す形に書き換え、期待値は変えない。内部の関数を使っていて書き換えられなければ STOP する。これらは unit では実行しない（CP で実行する） |
| `src/style_tools/tests.rs` の残り（`lint_selection_is_explicit_and_non_exhaustive_is_scoped`、`configuration_preserves_project_settings_and_extends_banned_methods`、`compiler_json_separates_policy_violations_and_compile_errors`、`unknown_lints_without_spans_are_not_silently_ignored`、`configured_rustflags_are_not_silently_replaced_by_analysis_cfg`、`empty_cfg_does_not_override_or_reject_existing_cargo_flags`） | `crates/xtask-style/src/` の単体試験のまま移す。期待値は変えない |
| `src/style_source/tests.rs` の全件 | `crates/xtask-style/src/` の単体試験のまま移し、`SourceReport` の API に合わせる。期待値は変えない |

上の分類は、`fixture()`（`cargo generate-lockfile` を実行する）・`Workspace::load`・`check` を呼ぶかどうかで決めた。表に無い試験を見つけたら、process を起動するなら `tests/tools.rs`、しないなら `src` に置く。

### 試験（足すもの、すべて `src` の単体試験）

- `unparsable_file_does_not_hide_others`：二つのファイルのうち一つが構文の誤りのとき、もう一つのファイルの `hex-literals` の違反が報告され、`errors` に壊れたファイルの path が入る。
- `nested_scopes_do_not_leak`：一つのファイルに `mod inner { fn f() { use std::cmp::Ordering::*; } }` と、その後の最上位の `use std::cmp::Ordering::*;` を書く。inner の関数の本体の中の enum の glob は報告されず、後の最上位のものは報告される（規則は今の enum の glob の規則）。
- `item_index_names_line_diagnostics`：`fn a() {}\nmod m {\n    fn b() {}\n}` の索引で、3 行目が `m::b` に、1 行目が `a` に当たる。

### 確認

```bash
cargo check -p xtask-style --all-targets
cargo test -p xtask-style --lib
cargo clippy -p xtask-style --all-targets -- -D warnings
```

---

## C2：`xtask-core` の組み直し（C2a → C2b → C2c、同じ lane で順に行う）

### 所有（三つの unit で共通）

- `crates/xtask-core/**`（C1 が移したファイルを除く。`tests/style.rs` を含む）
- `dev/layout-baseline/capture/`、`dev/layout-baseline/README.md`
- `crates/xtask-graph/**`、`crates/xtask-svg/**`（削除）
- root の `Cargo.toml`
- `dev/xtask/policy/workspace.toml`（M3c が所有した `trusted_macros` の行を除く）
- `architecture/`（削除）
- `.cargo/config.toml`

### 既存の試験の移行（C2 全体）

旧い試験は `git show e7eb600:crates/xtask-core/tests/<path>` で読む。「維持」は、同じ不変条件を図のファイル（`xtask_archive::decode` した `Archive`）への assert で確かめる新しい形に書き換えること。

| 試験（旧いファイル） | 扱い | unit |
|---|---|---|
| `successful_check_alone_generates_crate_and_nested_module_diagrams`（integration.rs） | 維持：`check --write` で、`diagrams[0]` が Crates、fixture の lib の Normal と Test の module 図があり、module 図が Drawn で region の親が 3 段に入れ子になり、item `crate` の node から `crate::middle::leaf` の node への edge があり、item の role が source の Role と等しく、`Cargo.lock` が変わらない。ELK の属性・`crates.dot`・ファイルの数の assert は消す | C2c |
| `failed_check_keeps_line_cycle_and_role_diagnostics_and_generates_diagrams`（integration.rs） | 維持：違反の assert は変えず、`--write` の図のファイルに Crates の Drawn と両方の構成の module 図がある。root の Role を壊した後も Crates が Drawn | C2c |
| `visibility_errors_do_not_prevent_the_dependency_diagram`（integration.rs） | 維持：`--write` の Crates の図に item `fixture` から `provider` への edge があり、module 図が Drawn | C2c |
| `early_analysis_and_file_errors_do_not_hide_other_checks_or_the_crate_diagram`（integration.rs） | 維持：stderr の assert は変えない。Node の有無の分岐を消し、`--write` の図のファイルの Crates の図が Drawn で item `fixture` と `provider` を持ち、module 図が Failed | C2c |
| `diagram_runtime_failure_does_not_discard_check_diagnostics`（integration.rs） | 置換：`archive_write_failure_keeps_check_diagnostics` にする。出力先の path に directory を作っておき `--write` すると、終了コード 1、stderr に `reference cycle`、`5 code lines (limit 3)`、`missing module role` と、出力先の path を含む書込みの誤りがある | C2c |
| `no_write_runs_every_rule_without_node_or_filesystem_mutation`（integration.rs） | 維持：`no_write` の代わりに既定（`Output::None`）で、同じ snapshot（`Cargo.lock` を含む）が変わらず、`target/xtask` が作られない | C2c |
| `help_succeeds_and_old_commands_and_flags_are_rejected`（integration.rs） | 維持：`--no-write` と `--prune` も終了コード 2 の誤りに加える | C2c |
| `role_limits_reach_analysis_and_all_check_diagrams_with_api_precedence`（integration.rs） | 契約の変更：`Config::max_role_chars` を消したので、API の上書きの部分を消す。policy の `max_role_chars` が解析に届く部分は維持 | C2c |
| `analysis_is_output_free_and_policy_optional_but_check_requires_policy`（integration.rs） | 契約の変更（X-C3）：policy のファイルが無いとき、`check` も既定の policy で動く。「check は policy を要る」の assert を、成功の assert に替える。他は維持 | C2c |
| `policy_output_paths_and_api_overrides_resolve_from_the_repository_root`（integration.rs） | 維持：最上位の `output` を `[diagrams] output` に、API の上書きを `Output::Path` に替えて、root からの相対で解決されることを確かめる | C2c |
| その他の integration.rs の試験 | API の追従だけ。期待値は変えない | C2c |
| `diagram_targets_select_generated_diagrams_but_not_checked_targets`（integration/diagrams.rs） | 維持：`[diagrams] targets`（既定、`all`、`lib-bin`、`lib`）で、図のファイルの Modules の図の `(target, mode)` の集合が選択と一致し、選択の外の target の Role の誤りは報告される | C2c |
| `stale_module_diagrams_are_reported_in_no_write_and_pruned_by_default`、`obsolete_diagrams_are_pruned_even_while_the_report_has_rule_errors`、`default_and_all_targets_never_report_their_own_diagrams_as_stale`（diagrams.rs） | 削除（prune を消した。図は一つのファイルを毎回書き直す） | C2a |
| `failed_target_or_workspace_analysis_keeps_current_diagrams_and_prunes_removed_ones`（diagrams.rs） | 置換：`failed_analysis_is_recorded_in_the_archive`。構文の誤りの target の module 図が `Content::Failed` で理由に file 名を含み、存在しない feature（`fixture/missing`）のときは全 module 図が Failed で Crates は Drawn | C2c |
| `targets_skipped_for_inactive_required_features_keep_their_diagrams`（diagrams.rs） | 置換：`skipped_targets_are_recorded_in_the_archive`。required-features が無効な選択内の target の図が `Content::Skipped("required features are inactive")`。`skipped fixture::gated` の通知の assert は維持 | C2c |
| `integration/arch_map.rs` の全 6 件 | 削除（HTML を消した）。図の中身・target の選択・解析の失敗・何も書かないことは、上の維持・置換の試験が確かめる | C2a |
| `tests/style.rs` の全件 | 維持：`Report.style`・`Output::None` の API に合わせる。`check_unchanged` の「`architecture` の directory が無い」の assert は「`target/xtask` が無い」に替える。通知（`checked 20 style rules`、例外の理由、`unused style exception` が無いこと）の assert は `Report.notes` で変えない | C2c |
| `wildcard_exceptions_cannot_suppress_invalid_rule_targets`（tests/style.rs） | 契約の変更（W-B10）：例外の `path='*'` を `path='src/*'` に替える（`*` だけの path は policy の読込みで誤りになるため）。無効な selector が抑制されないことの assert は変えない | C2c |
| `src/` の単体試験（cli、config、run など） | API の追従。消した機能（prune、no_write、arch_map、graphs）の試験は消す | C2a |

---

### C2a：削除、CLI、Config

#### 手順

1. **消すもの。**
   - `crates/xtask-graph`、`crates/xtask-svg` の directory 全体。
   - `crates/xtask-core/src/arch_map.rs` と `src/arch_map/`、`src/graphs.rs` と `src/graphs/`。
   - `tests/integration/arch_map.rs` と、`tests/integration/diagrams.rs` の削除の 3 件（移行の表）。
   - リポジトリの `architecture/` の directory 全体（生成物を commit しない）。
   - root の `Cargo.toml` の `xtask-graph`、`xtask-svg` の行。
   - dev-dependency の `roxmltree`。
2. **CLI（S7、C1、C7、B8）。** `cli.rs` を次に替える。

   ```rust
   pub enum Command { Help, Check(Config) }
   pub fn parse_args(root: &Path, arguments: impl IntoIterator<Item = String>) -> Result<Command, String>
   pub fn main(root: &Path) -> ExitCode
   ```

   - 受け付けるのは `check`、`--root PATH`、`--write`、`--help`、`-h` だけ。`--no-write` と `--prune` は「unknown option」の誤り（終了コード 2）にする。
   - `--root` の値は次の引数をそのまま使う（`-` で始まる値も受け付ける）。値が無ければ誤り。相対の path は、process の今の directory を基準にする（`std::env::current_dir()?.join(値)`）。
   - help の判定は `parse_args` の中だけで行い、`main` は `Command::Help` を見て USAGE を出す。
   - USAGE の文：

   ```text
   usage: xtask check [--root PATH] [--write]
          xtask --help|-h

   check   runs every repository rule. It writes nothing unless --write is given.
   --write also lays out the architecture diagrams and writes them to the file
           named by [diagrams] output (default target/xtask/architecture.xtaskmap).
   ```

3. **Config（S6、C3、C4）。**

   ```rust
   pub struct Config {
       pub root: PathBuf,
       pub policy: PathBuf,
       pub output: Output,
       /// None なら policy の [analysis] を使う。
       pub analysis: Option<xtask_policy::AnalysisSettings>,
   }
   pub enum Output {
       /// 図を書かない（既定）。
       None,
       /// policy の [diagrams] output に書く。
       Policy,
       /// 指定の path（root からの相対、または絶対）に書く。
       Path(PathBuf),
   }
   ```

   - `Config::new(root)` は `Output::None` にする。`--write` は `Output::Policy` にする。
   - このリポジトリの `src/main.rs` は、`current_dir()` ではなく `Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2)` を root にする（このリポジトリの自己検査の専用。subdirectory から実行しても正しい root になる）。
   - `max_role_chars` の上書きと `prune`、`no_write` を消す。
4. この時点では `run.rs` の中が compile できなくてよい。C2a の確認は、`cli` と `config` の単体試験だけ。

#### 試験

- `root_value_may_start_with_dash`：`["check", "--root", "-x"]` の結果の `Config.root` が `<今の directory>/-x`。
- `root_requires_a_value`：`["check", "--root"]` が誤り。
- `old_flags_are_rejected`（`src` の単体試験）：`--no-write` と `--prune` が `Err`。

#### 確認

この時点の xtask-core は、まだ compile できない（`run.rs` は C2b が組み直す。設計書 9 章）。C2a の確認は `cargo fmt -p xtask-core` だけにし、上の試験は書くだけで、C2b の確認（`cargo test -p xtask-core --lib`）で実行される。compile error は報告に書くが STOP にしない。

---

### C2b：段階の流れ、並列、書出し

#### 手順

1. **段階の分離（S1、S2、S3、C07）。** `run.rs` を次の流れに組み直す。共有の `Report` を段階ごとに書き換える方式をやめる。各段階は自分の結果を返し、最後に連結する。

   ```text
   let context = Context::load(config)?;            // policy と Workspace::load（構造の metadata）。失敗は致命的（run の Err）
   let environment = Environment::load(&context);   // Result<Environment, String>：feature（Workspace::features）、target と host の rustc cfg
   let analysis = analyze(&context, &environment);  // (Vec<TargetAnalysis>, Findings)。environment が Err なら全 target を Failed
   let lines = check_lines(&context, &analysis);    // Findings
   let visibility = visibility(&context);           // Findings
   let style = style(&context, &environment, &analysis);  // Option<StyleReport>（StyleSettings が Some のとき）
   let written = match config.output { Output::None => None, _ => Some(write_archive(&context, &analysis)) };  // Option<Result<PathBuf, String>>
   Report::merge(...)
   ```

   - `Environment::load` が `Err(e)` のとき：`analysis` の全 target の全構成を `TargetOutcome::Failed(format!("module analysis: {e}"))` にし、`module analysis: {e}` を errors に一度だけ入れる。行数・可視性・style（source の検査）・図の書出しは続ける。
   - `write_archive` の `Err(e)` は `Report.errors` に入れる（他の診断を消さない。C06）。`Ok(path)` は `Report.notes` に `wrote {path} ({n} diagrams)` を足し、`Report.archive` に path を入れる。

   ```rust
   pub struct TargetAnalysis { pub package: String, pub target: String, pub kinds: Vec<xtask_workspace::TargetKind>, pub outcomes: Vec<(Mode, TargetOutcome)> }
   // Mode は xtask_archive::Mode を使う（同じ事実の型を二つ持たない）。
   pub enum TargetOutcome { Analyzed(xtask_modules::ModuleGraph), FeatureGated, UnsupportedEdition(String), Failed(String) }
   pub struct Report { pub errors: Vec<String>, pub notes: Vec<String>, pub style: Vec<StyleDiagnostic>, pub archive: Option<PathBuf> }
   impl Report { pub fn is_success(&self) -> bool { self.errors.is_empty() && self.style.is_empty() } }
   ```

   - `Analysis`、`SkippedTarget`、`TargetGraph`、`Report.artifacts`、`Report.style_diagnostics` を消す。
   - style の診断は `Report.style` にだけ入れ、`errors` に写さない。style の `errors` は `Report.errors` に、`notes` は `Report.notes` に入れる。CLI は `errors` と `style` を両方表示する。
   - `analyze` の公開の型は `pub fn analyze(config: &Config) -> Result<(Vec<TargetAnalysis>, Report), String>` にする。
   - root の一本化（W の B6）：`Context::load` は、`xtask_source::normalize(config.root)` と `Workspace.root` が違えば、`--root must name the workspace root ({workspace root})` の誤りにする。以後の root は `Workspace.root` だけを使う。
   - policy のファイルが無いときは、`run` と `analyze` のどちらも `Policy::default()` を使う（X-C3）。
2. **解析の job（I1、S5、I10）。**
   - job は target ごとに一つにする。target の構成の列（Normal、Test、または両方）を `xtask_modules::analyze(root, &settings, &configurations)` に一度で渡す（付属書 M の M3a）。
   - `xtask_modules::Settings` は **package ごとに一回**作る（`features` は `environment.features[package]`）。同じ package の job は、その `Settings` を `&` で共有し、clone しない。
   - host の cfg は、`[analysis] target` があるときだけ `rustc --print cfg` を二回目として実行する。無いときは target の cfg を使う。どちらも `Environment::load` の中で、job の作成の前に決める。
3. **並列化（B2、DEC-16）。** `src/parallel.rs` を次のとおりに書く（Rust 1.85 で compile と動作を確かめたもの。このとおりに書く）。`std::panic::catch_unwind` は使わない。

   ```rust
   //! Role: 入力ごとに一つの scoped thread で処理し、panic を入力ごとの誤りにする。
   use std::collections::BTreeMap;
   use std::num::NonZeroUsize;
   use std::sync::mpsc;
   use std::thread;

   /// 入力ごとの結果（入力と同じ順）。f が panic した入力は Err（panic の文）。
   /// 同時に動く thread は CPU の数まで。
   pub(crate) fn map<T: Sync, R: Send>(inputs: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<Result<R, String>> {
       let limit = thread::available_parallelism().map_or(1, NonZeroUsize::get);
       map_with_limit(inputs, limit, f)
   }

   pub(crate) fn map_with_limit<T: Sync, R: Send>(
       inputs: &[T],
       limit: usize,
       f: impl Fn(&T) -> R + Sync,
   ) -> Vec<Result<R, String>> {
       let limit = limit.max(1);
       let mut finished = BTreeMap::new();
       thread::scope(|scope| {
           let (sender, receiver) = mpsc::channel::<usize>();
           let mut running = BTreeMap::new();
           let mut next = 0;
           loop {
               while running.len() < limit && next < inputs.len() {
                   let signal = Finished { index: next, sender: sender.clone() };
                   let (input, f) = (&inputs[next], &f);
                   running.insert(next, scope.spawn(move || {
                       let _signal = signal;
                       f(input)
                   }));
                   next += 1;
               }
               if running.is_empty() {
                   break;
               }
               let index = receiver.recv().expect("a running worker holds a sender");
               let handle = running.remove(&index).expect("the finished worker is running");
               finished.insert(index, handle.join().map_err(|payload| panic_message(&*payload)));
           }
       });
       finished.into_values().collect()
   }

   /// worker の終わり（正常の return と panic の巻戻しの両方）で index を送る。
   struct Finished {
       index: usize,
       sender: mpsc::Sender<usize>,
   }

   impl Drop for Finished {
       fn drop(&mut self) {
           let _ = self.sender.send(self.index);
       }
   }

   fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
       if let Some(text) = payload.downcast_ref::<&str>() {
           (*text).to_owned()
       } else if let Some(text) = payload.downcast_ref::<String>() {
           text.clone()
       } else {
           "worker panicked".to_owned()
       }
   }
   ```

   - 状態は関数の中の局所の値（`running`、`next`、`finished`）だけで、すべての入力にちょうど一つの結果が付く。
   - root の `Cargo.toml` の profile に `panic = "abort"` を足さない（巻戻しが要る）。

   呼出し側の扱い：
   - 解析の job の `Err` は、その target の構成すべてを `TargetOutcome::Failed(format!("analysis panicked: {文}"))` にする。
   - 配置（`draw`）の `Err` は、`xtask_diagram::failed(plan, format!("layout panicked: {文}"))` にする。
4. **行数の検査（C6、B8、M10）。**
   - 検査するファイルは、次の三つの和にする。どれも `xtask_source::normalize` した path で dedup する。
     1. package の directory ごとの `xtask_source::rust_files`。
     2. **すべての Cargo target の `src_path`**（feature で無効、未対応の edition、解析の失敗に関わらず、無条件に）。
     3. 解析した module のファイル。
   - この一覧を C1 の `StyleInput.files` にも渡す。
   - file → module の path の表（`StyleInput.modules`）は、解析結果から一回だけ作る。同じファイルが複数の target に現れたら、最初の target の module の path を使う。
5. **可視性の検査。** `src/visibility.rs` に `fn visibility(workspace: &Workspace, visible_to: &BTreeMap<String, Vec<String>>) -> Vec<String>` を置く。内容は旧 `Workspace::check_visibility`（`git show e7eb600:crates/xtask-workspace/src/model.rs`）から policy の再検証を除いたもの。試験は下の `visibility_checks_stale_providers_consumers_and_all_declared_edges`。
6. **図のファイルの書出し。** `fn write_archive(context, analysis) -> Result<PathBuf, String>`。
   - `xtask_diagram::Input` を組み立てる。
     - `targets` には、解析したすべての target の構成を入れる。`drawn` は policy の `[diagrams] targets` で決める。選択の関数 `fn selected(targets: DiagramTargets, kinds: &[TargetKind]) -> bool` を xtask-core に置く。`All` は常に true、`Lib` は `is_library` のものがあれば true、`LibBin` はそれに `Bin` を加える。
     - `TargetOutcome::FeatureGated` は `Outcome::Skipped("required features are inactive")` にする（mode は Normal の一件）。
     - `UnsupportedEdition(s)` と `Failed(s)` は `Outcome::Failed(s)` にする。
     - crate の Role は、`plan` が `primary_target` で求める（付属書 A の 2.3 節）。xtask-core は求めない。
   - `let plans = xtask_diagram::plan(&input);`、`parallel::map(&plans, xtask_diagram::draw)` で並列に配置し、`Err` は `xtask_diagram::failed` にする。
   - `Archive { root_name, diagrams, notices }` を組み立てる。root_name は正規化した root の最後の要素。notices は、解析の段階の errors と notes。
   - `xtask_archive::encode` で符号化する。`Err(e)` は `Err(format!("cannot encode {path}: {e}"))`。
   - 出力先の親 directory を作り、`tempfile::NamedTempFile::new_in(親)` に書いて `persist(出力先)` する。Windows の「先に消してから rename」はしない（B3）。各段階の I/O の誤りは `Err(format!("cannot write {path}: {e}"))`。
7. **library の公開の名前。** `lib.rs` の公開を次にする。

   ```rust
   pub use cli::{main, parse_args, Command};
   pub use config::{Config, Output};
   pub use model::{Report, TargetAnalysis, TargetOutcome};
   pub use xtask_archive::Mode;
   pub use run::{analyze, run};
   pub use xtask_policy::{AnalysisSettings, DiagramTargets, Features, Policy};
   pub use xtask_modules::ModuleGraph;
   pub use xtask_style::StyleDiagnostic;
   ```

   - doc の Node・SVG・HTML の記述を消す。
8. **依存。**
   - root の `Cargo.toml` の `[workspace.dependencies]` に、`xtask-style = { path = "crates/xtask-style" }` と `xtask-diagram = { path = "crates/xtask-diagram" }` を足す。
   - `xtask-core/Cargo.toml` の依存は、xtask-source、xtask-policy、xtask-workspace、xtask-modules、xtask-style、xtask-diagram、xtask-archive、tempfile。
   - `serde_json`、`syn`、`proc-macro2`、`toml` は、残ったコードが使わなければ消す。

#### 試験（`src` の単体試験）

- `parallel::tests::results_follow_input_order`：`map_with_limit(&[3, 1, 2], 2, |x| x * 10)` が `[Ok(30), Ok(10), Ok(20)]`。
- `parallel::tests::panic_isolated_with_one_worker`：`map_with_limit(&[0, 1], 1, |&x| if x == 0 { panic!("boom") } else { x })` が `[Err("boom"), Ok(1)]`（panic の文を見えなくするため、試験の中で `std::panic::set_hook` は使わない。出力に panic の文が出るのは正常）。
- `parallel::tests::every_input_may_panic`：3 個の入力がすべて panic しても、結果は 3 個の `Err`。
- `parallel::tests::send_only_results`：`R = std::cell::Cell<u8>`（`Send` だが `Sync` でない）で compile でき、結果が入力の順に並ぶ。
- `visibility_checks_stale_providers_consumers_and_all_declared_edges`：旧い xtask-workspace の同名の試験（`git show e7eb600:crates/xtask-workspace/src/tests.rs`）を、同じ fixture・同じ期待値（誤り 3 件と 3 つの文）で、`Workspace::load` と `visibility` を使って書く。
- `selection_by_diagram_targets`：`selected` が、`Lib` で `[Bin]` に false、`LibBin` で `[Bin]` に true、`Lib` で `[ProcMacro]` に true。

#### 確認

```bash
cargo check -p xtask-core --lib
cargo test -p xtask-core --lib
```

（`tests/` の統合試験は C2c が移すまで compile できないので、`--all-targets` は C2c で確かめる。）

---

### C2c：試験の移行、policy、profile、後始末

#### 手順

1. **試験の移行。** 上の「既存の試験の移行」の表で unit が C2c のものを、表のとおりに書き換える。
2. **試験を足す**（`tests/integration.rs`）。
   - `check_writes_nothing_by_default`：一時のリポジトリで `check` を実行した後、`target/xtask` が存在しない。
   - `write_creates_archive`：`--write` で `target/xtask/architecture.xtaskmap` ができる。`xtask_archive::decode` が成功し、`diagrams[0]` が Crates になる。
   - `old_flags_are_rejected`：`--no-write` の終了コードが 2 になる。
   - `root_must_be_the_workspace_root`：workspace の subdirectory を `--root` に渡すと誤りになり、文に workspace の root を含む。
   - `missing_policy_uses_default_in_run_and_analyze`：policy のファイルが無い一時のリポジトリで、`run` と `analyze` のどちらも成功する。
   - `custom_target_roots_are_counted`：`[[bin]] name = "tool" path = "tools/custom.rs" required-features = ["off"]`（feature は無効）の `tools/custom.rs` が 4 code 行、`max_code_lines = 3` のとき、`custom.rs` の行数の誤りが報告される。同じ file に構文の誤りを足しても、行数の誤りが報告される。
3. **policy のファイル。** `dev/xtask/policy/workspace.toml` を次の内容にする（M3c の行を除く）。
   - 最上位の `output` を消す。
   - `visible_to` を設計書 4.2 節の表に替える。
4. **基準の取得の script の削除（AC-1）。**
   - `dev/layout-baseline/capture/tee.cjs` と `node-tee.cmd` を消す。
   - `dev/layout-baseline/README.md` の「取得方法」の節を、次の 1 文に替える：「Node.js と elkjs を使って取得した。改修で Node.js を削除したため、再取得の手段は無い。取得に使った script は commit `e7eb600` にある」。
   - `split.py` は残す。
5. **`examples/benchmark.rs`。** 引数の `OUTPUT` と `no-write`・`write` の mode を消し、`check` と `check --write` の時間を出すようにする。
6. **最適化の profile。** root の `Cargo.toml` に次を足す。debug build でも図の配置を速くするため。README（C3）にも、利用側の root の `Cargo.toml` に同じ行を足すよう書く。

   ```toml
   [profile.dev.package.xtask-layout]
   opt-level = 3
   ```

   同じ `Cargo.toml` の `[workspace]` に `exclude = ["viewer"]` を足す。

#### 確認

```bash
cargo check --workspace --all-targets
cargo test -p xtask-core --lib
cargo test -p xtask-core --test integration -- check_writes_nothing_by_default write_creates_archive old_flags_are_rejected archive_write_failure_keeps_check_diagnostics custom_target_roots_are_counted
cargo clippy -p xtask-core --all-targets -- -D warnings
```

統合の試験の全件と `tests/style.rs` は W5 の tier b と CP で Orchestrator が実行する。

---

## C3：文書

### 所有

`README.md`、`README_JP.md`、`THIRD-PARTY-NOTICES.md`（あれば）。他の文書（設計書、付属書、`dev/layout-baseline/README.md`）は変えない。

### 手順

1. `README_JP.md` と `README.md` を、設計書 3 章（利用者の経路）と 5.1 節に合わせて書き直す。二つの README は同じ構成・同じ内容にする。

   | 対象 | 変更 |
   |---|---|
   | 動作要件 | Node.js と `XTASK_NODE` の項を消す |
   | 使い方 | `check` と `check --write` だけにする |
   | 設定 | 最上位の `output` を消し、`[diagrams] output` を書く。`[analysis]` の `features`・`all_features`・`default_features` の組合せの制約を書く。例外の `path` は `*` だけにできないこと |
   | 出力 | 節を「図のファイル」に書き替える：一つの圧縮したファイル、既定の場所、viewer で開くこと、`viewer/` の build の方法、GitHub Actions の成果物 |
   | 移行 | 節を足す：`--no-write` は既定になったこと、`--prune` の削除、旧い `architecture/` の SVG・HTML を手で消すこと、設計書 11 章の表の各項目 |
   | crate の表 | 設計書 4.1 節に合わせる |
   | 第三者の notice | elkjs の行を消す |

2. 解析の注意（README の「解析」節）に、次を足す。
   - `json!` と `Token!` の特別扱いを止めたので、使う repository は `trusted_macros` に書くこと。信頼した macro の中の参照は依存の辺にならないこと。
   - `cfg(true)` に対応したこと。
   - `trusted_globs` は「その glob を未知の提供元とみなさない」意味で、glob が与える attribute・macro を使うなら `trusted_attributes`・`trusted_macros` にも書くこと。`allowed_globs`（style）とは別に書く必要があること（設計書 DEC-9）。
   - `thread_local!` は `thread_local!`・`std::thread_local!` と、同じ module の `use std::thread_local as X;` の別名で呼ぶこと（DEC-19）。
   - Role は最初の inner doc の最初の行に置くこと。

### 確認

二つの README の見出しの列が一致することを、目で確かめる。
