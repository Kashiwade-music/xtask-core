# 付属書 M：policy・workspace・source・modules

設計書 7 章の所見のうち、`xtask-policy`（新設）、`xtask-workspace`、`xtask-source`、`xtask-modules` に関わるものを直す手順。

各 unit は、先に「試験」の欄の試験を書く。その試験が失敗することを確かめ（試験が無いことによる compile error も失敗とみなす）、それから手順を実装する。

旧い実装は `git show e7eb600:<path>` で読む。

---

## M1：`xtask-policy` の新設と `xtask-workspace` の整理

### 所有

- `crates/xtask-policy/**`（新設）
- `crates/xtask-workspace/**`
- root の `Cargo.toml` の `[workspace.dependencies]` の `xtask-policy` の 1 行（A1 も同じ節に別の行を足す。作業指示書 1.3 節の規則で merge する）

### 目的

- policy の読込みと検証を、Cargo metadata から切り離す。
- 検証を一度だけ行い、検証済みの型だけが出回るようにする。
- `cargo metadata` の実行を、構造の一回と、必要なときだけの feature の一回にする（設計書 DEC-17）。
- 依存の辺を Cargo の宣言から作り、無効な optional の依存も構造の検査から外さない。

### 手順

1. **crate の新設。** `crates/xtask-policy` を作る。
   - 依存は `serde`、`toml`（workspace）だけ。
   - `xtask-workspace/src/policy.rs` と `src/style.rs`（`StyleRule`、`StylePolicy`、`StyleException`）をここへ移す。
   - `xtask-workspace` から policy の型を消し、`pub use` も消す。
2. **型。** 公開の型は次のとおりにする。field は private にし、読出しの method を付ける。構築は `Policy::load(path)` と `Policy::from_toml(text)` だけにし、どちらも検証済みの値を返す。`Policy::validate` は公開しない。

   ```rust
   pub struct Policy { limits: Limits, diagrams: Diagrams, analysis: AnalysisSettings, style: Option<StyleSettings>, visible_to: BTreeMap<String, Vec<String>> }
   pub struct Limits { max_code_lines: usize /* 1 以上 */, max_role_chars: usize /* 1 以上 */ }
   pub struct Diagrams { targets: DiagramTargets, output: PathBuf }
   pub enum DiagramTargets { All, Lib, LibBin }   // includes() は持たない（選択は xtask-core が TargetKind で行う）
   pub struct AnalysisSettings { target: Option<String>, features: Features, cfg: BTreeSet<String>, trusted_attributes: BTreeSet<String>, trusted_derives: BTreeSet<String>, trusted_macros: BTreeSet<String>, trusted_globs: BTreeSet<String> }
   pub enum Features { All, Selected { features: BTreeSet<String>, default_features: bool } }
   /// [style] enabled = true のときだけ Some。規則の組合せは読込み時に一度だけ決める。
   pub struct StyleSettings { rules: BTreeSet<StyleRule>, api_packages: BTreeSet<String>, options_structs: BTreeSet<String>, allowed_globs: BTreeSet<String>, toolchain: Option<String>, exceptions: Vec<StyleException> }
   ```

   - 数の上限の型は、現在と同じ `usize` にする（契約を狭めない）。
   - `StyleRule::BASIC: [StyleRule; 13]` を明示の定数として書く。`ALL[..13]` を使う箇所をすべてこれに替える。
   - `Policy` は `Default` を実装する（policy のファイルが無いときに使う）。既定値は、各節の既定値の組。
   - `AnalysisSettings` は `Default` を実装する（`Features::Selected { features: 空, default_features: true }`）。
   - xtask-core の `Config` から上書き用に作れるよう、次の関数も用意する。手順 4 と同じ検証を行う。

     ```rust
     impl AnalysisSettings {
         pub fn new(target: Option<String>, features: Features, cfg: Vec<String>, trusted_attributes: Vec<String>, trusted_derives: Vec<String>, trusted_macros: Vec<String>, trusted_globs: Vec<String>) -> Result<Self, String>;
     }
     ```

   - `Limits` の既定値は `max_code_lines = 1000`、`max_role_chars = 80`。80 という既定値は、ここだけに置く。
3. **TOML の表の検査（B1）。** 節（`limits`、`diagrams`、`analysis`、`style`、`visible_to`）はすべて TOML の表でなければならない。
   - 文字列を `toml::Table` として読み、各節が `toml::Value::Table` であることを先に確かめる。違えば `"[limits] must be a table"` の形の誤りにする。
   - その後に serde の private な `Raw*` の struct へ変換し、`deny_unknown_fields` を付ける。
   - 現在の `analysis_settings` と `style_policy` の特別な deserializer は消す。
4. **値の検証（B2、B3、B4、B10）。** `Raw*` から公開の型を作るときに、次をすべて調べる。誤りは集めず、最初の一つを返す。
   - `max_code_lines` と `max_role_chars` は 1 以上。
   - 文字列の一覧（features、cfg、trusted_*、api_packages、options_structs、allowed_globs、visible_to の値）に、空の文字列、前後の空白を含む文字列、重複が無い。重複は誤りにする（黙って捨てない）。
   - `all_features = true` のとき、`features` が空で、`default_features` を書いていない（`true` の既定のまま）。違えば `"all_features cannot be combined with features or default_features"`。
   - style の規則の組合せの検査（non-exhaustive の `api_packages`、options-default の `options_structs` など）は、`enabled` にかかわらず、`extra_rules` に挙がっていれば行う。
   - 例外の `path` が `*` だけのものは誤りにする（`"exception path must name a file or a directory prefix"`）。`src/*` のような接頭辞は今までどおり受け付ける。
5. **出力先の移動。** 最上位の `output` を消し、`[diagrams] output` にする。既定値は `target/xtask/architecture.xtaskmap`。
   - 最上位に `output` が書かれた policy は、未知のキーとして誤りになる。誤りの文に `"output moved to [diagrams] output"` を足す。
6. **Workspace の型。** `xtask-workspace` の公開の型を次のとおりにする。

   ```rust
   pub struct Workspace { pub root: PathBuf, pub packages: Vec<Package>, pub edges: Vec<Dependency>, pub all_features: BTreeMap<String, BTreeSet<String>> }
   pub struct Package { pub id: String, pub name: String, pub manifest_path: PathBuf, pub targets: Vec<Target> }
   pub struct Target { pub name: String, pub kinds: Vec<TargetKind>, pub src_path: PathBuf, pub required_features: Vec<String>, pub test: bool, pub edition: Edition }
   #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
   pub enum TargetKind { Lib, Rlib, Staticlib, Cdylib, Dylib, ProcMacro, Bin, Example, Test, Bench, CustomBuild, Other(String) }
   impl TargetKind { pub fn is_library(&self) -> bool; pub fn as_str(&self) -> &str; }
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub enum Edition { E2015, E2018, E2021, E2024, Other }
   pub struct Dependency { pub from: String, pub to: String, pub kind: DependencyKind, pub target: Option<String> }
   #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
   pub enum DependencyKind { Normal, Dev, Build }
   ```

   - `Package.id` は Cargo の package id。xtask-style の clippy が、Cargo の JSON の `package_id` と照合するのに使う（消さない）。
   - `Package.features`、`Package::active_features` は使われていないので消す（B7）。
   - `all_features` は、構造の metadata（`--all-features`）の resolve の、package の名前ごとの feature。`Features::All` で `target` が無いときの feature にそのまま使う。
   - 例の struct には、既存の derive（`Clone, Debug` など）をそのまま付ける。
   - `is_library` の対象は Lib、Rlib、Staticlib、Cdylib、Dylib、ProcMacro。library の種類の一覧は、この関数だけに置く。
7. **metadata（DEC-17）。** 二つの関数にする。`xtask-workspace` の依存に `xtask-policy` と `xtask-source` を足す。

   ```rust
   impl Workspace {
       /// 構造の metadata。失敗は致命的（xtask-core の run が Err を返す）。
       pub fn load(root: &Path) -> Result<Workspace, String>;
       /// 選んだ構成の feature。失敗は解析の環境の失敗（xtask-core は他の検査を続ける）。
       pub fn features(&self, analysis: &xtask_policy::AnalysisSettings) -> Result<BTreeMap<String, BTreeSet<String>>, String>;
   }
   /// `features` が二回目の metadata を実行するか。
   pub fn needs_selection_run(analysis: &xtask_policy::AnalysisSettings) -> bool;
   ```

   - **`load`**：`cargo metadata --format-version 1 --locked --offline --all-features` を一回実行し、一回だけ parse する（`--filter-platform` は付けない）。lock は feature と platform によらず全部の依存を持つので、この形は lock があれば必ず解決できる。失敗の文は `cargo metadata failed: {stderr の最後の行}`。
     - package と target は `packages` から取る。
     - `all_features` は `resolve.nodes[].features` から取る。
     - **依存の辺**：workspace の member の `packages[].dependencies` の**宣言**から作る（有効かどうか、platform に合うかどうかにかかわらず、すべての宣言）。依存先が member かどうかは次で決める。
       1. 宣言に `path` があれば、その path を `xtask_source::normalize` した directory が、ある member の `manifest_path` の親と等しいとき、その member。
       2. 宣言に `path` が無く、実の名前（`package` の別名を含む）が member の名前と等しいとき（同名の registry の依存。`[patch]` で member に置き換わることがある）は、この metadata の `resolve` の、宣言した package の node の `deps` で決める。その名前の依存が member の id なら member、別の id なら外部。`--all-features` で platform の絞込みも無いので、すべての宣言の依存が `resolve` に現れる。現れなければ `cannot resolve registry dependency identity for {name} of {package}` の誤りにする。
       3. それ以外は外部（辺を作らない）。
   - **`features`**：`needs_selection_run(analysis)` が false（`Features::All` で `target` が無い）なら `self.all_features` を返す。true なら `cargo metadata --format-version 1 --locked --offline` に、features の指定（`--features`、`--no-default-features`、`--all-features`）と、`target` があれば `--filter-platform` を付けて一回実行し、`resolve.nodes[].features` を返す。
   - command は一つの関数 `metadata_command(root, selection: Option<&AnalysisSettings>)` で作る（`None` は `load` の形）。
   - `resolved_features`、`resolved_features_from_json`、`--no-deps` の実行、同名の依存のための再実行を消す。
   - 誤りの文 `provide full unfiltered Cargo metadata` は `cargo metadata did not return a resolve graph` に替える。
8. **path の正規化（B5）。** metadata から得る path（root、manifest、src_path、依存の path）は、すべて `xtask_source::normalize(path)`（M2）で一度だけ正規化する。
   - `canonical()` の直接の呼出しをこの関数に替える。
   - M2 と同じ wave で並行して作るので、M1 では `xtask_source::normalize` の存在を前提に書いてよい。compile が通らなければ M2 の merge を待つ。STOP にはしない。
9. **可視性の検査を外す。** `Workspace::check_visibility(&self, policy)` を `xtask_workspace` から消す。xtask-core の C2b が `fn visibility(workspace: &Workspace, visible_to: &BTreeMap<String, Vec<String>>) -> Vec<String>` として持つ。可視性の試験は、下の「既存の試験の移行」のとおり。
10. **試験の分割。** `src/tests.rs` を `src/tests/metadata.rs`（xtask-workspace）と、`crates/xtask-policy/src/tests.rs` に分ける。

### 既存の試験の移行

| 試験 | 移行 |
|---|---|
| `src/tests.rs` の `fixture()` helper | 書いた後に `cargo generate-lockfile --offline` を実行して `Cargo.lock` を作る（`load` の metadata が `--locked` で resolve を作るため）。fixture は外部の依存を持たないので offline で作れる。期待値は変えない |
| `visibility_checks_stale_providers_consumers_and_all_declared_edges` | xtask-workspace から消す。C2b が xtask-core に、同じ fixture・同じ期待値（誤り 3 件と、その 3 つの文）で作り直す（付属書 C の C2b） |
| `registry_patch_workspace_edges_are_resolved_by_package_id` | 辺の期待値（2 本、どちらも `to == "core"`）と、`Cargo.lock` が変わらないことの期待値は変えない。`Cargo.lock` を消した後の誤りの文の期待値だけ、`cannot resolve registry dependency identity` から `cargo metadata failed` に変える（lock が無いと `load` の metadata が失敗するため）。`Cargo.lock` が作られないことの期待値は変えない |
| `resolved_features` を呼ぶ試験 | 同じ fixture・同じ期待値で、`Workspace::load(..)?.features(&analysis)` を見る形に書き換える |
| `analysis_settings_parse_cargo_cfg_and_trusted_expansions_with_other_rules`（policy） | 契約の変更（W-B3、W-S3：`all_features` と `features` の併用と、名前の重複を拒む）。入力から `all_features=true` の行を消し、`trusted_attributes` を `['macro_lib::second','macro_lib::first']`、`trusted_macros` を `['proptest','prop_assert']` にする（重複を除き、並べ替えの確認は残す）。期待値は変えず、`features` が `Features::Selected { features: {"app/left", "right"}, default_features: false }` であることを足す。拒む側は `all_features_cannot_combine_with_explicit_features` と、下の `duplicate_trusted_names_rejected` が確かめる |
| `feature_closure_handles_cycles_dependency_entries_and_unknown_requests`（workspace） | 削除。確かめていた `Package::active_features` と `Package.features` を消した（B7）。feature の閉包は Cargo の resolve が求める（`features` の試験が確かめる） |
| `diagram_targets_default_to_all_and_select_library_like_kinds`（workspace） | 分割。読込みの部分（既定が All、`all`・`lib`・`lib-bin` の名前、誤りの 3 つの入力）は policy の `diagram_targets_default_to_all_and_parse_by_name` に、期待値を変えずに移す。`includes` の部分（kind の組ごとの選択）は `DiagramTargets::includes` を消したので、xtask-core の `selected` の試験 `selection_by_diagram_targets`（付属書 C の C2b）に、同じ組と同じ期待値で移す |
| `svg_output_defaults_with_and_without_other_policy_tables`（workspace） | 契約の変更（設計書 5.1 節、DEC-2）。policy の `diagram_output_defaults_with_and_without_other_policy_tables` にし、既定値の期待値を `architecture` から `target/xtask/architecture.xtaskmap` に、読む field を `diagrams().output()` に変える。入力の 5 つは変えない |
| `svg_output_accepts_relative_absolute_and_space_containing_paths`（workspace） | 契約の変更（最上位の `output` を `[diagrams] output` に移した）。policy の `diagram_output_accepts_relative_absolute_and_space_containing_paths` にし、`output='..'` を `[diagrams]\noutput='..'` に替える。path の期待値は変えない |
| `svg_output_rejects_blank_paths_wrong_types_and_unknown_fields`（workspace） | 契約の変更（同上）。policy の `diagram_output_rejects_blank_paths_wrong_types_and_unknown_fields` にし、拒む入力を `[diagrams]` の下の `output` に替える（`[output]
path='architecture'` は `[diagrams.output]
path='architecture'` に、`output='architecture'
unknown=true` は `[diagrams]
output='architecture'
unknown=true` に替える）。最後の「`Policy` を直接作って `validate` を呼ぶ」部分は、field と `validate` を private にしたので消す（空白だけの path は TOML の入力の側で確かめている）。最上位の `output` が拒まれることは `output_moved` が確かめる |
| 消した型・関数（policy の型、`check_visibility` 以外）を使う試験 | 新しい API に書き換える。期待値は変えない |

### 試験（足すもの）

- `limits_must_be_a_table`：`limits = [500, 40]` が誤りになる。
- `zero_code_lines_rejected`
- `empty_feature_rejected`
- `duplicate_visible_to_rejected`
- `style_checks_ignore_enabled`：`enabled = false` かつ `extra_rules = ["non-exhaustive"]` で、`api_packages` が無ければ誤りになる。
- `output_moved`：最上位の `output` が誤りになり、その文に `[diagrams] output` を含む。
- `bare_wildcard_exception_rejected`：例外の `path = "*"` が誤りになり、`path = "src/*"` は受け付ける。
- `target_kind_library`：`TargetKind::ProcMacro.is_library()` が true になる。
- `duplicate_trusted_names_rejected`：`[analysis]\ntrusted_macros=['proptest','proptest']` が誤りになる。
- `inactive_optional_path_dependency_is_an_edge`：member `app` が member `core` に `optional = true` の path の依存を持ち、feature で有効にしていないとき、`edges` に `app → core` がある。
- `selection_run_only_when_needed`：`needs_selection_run` が、`Features::All` で target 無しなら false、`AnalysisSettings::default()`（既定の feature）なら true、`Features::All` で target 有りなら true。
- `bad_feature_fails_only_features`：存在しない feature を選んだとき、`load` は成功し、`features` が `Err`。

### 確認

```bash
cargo check -p xtask-policy -p xtask-workspace --all-targets
cargo test -p xtask-policy
cargo test -p xtask-workspace
cargo clippy -p xtask-policy --all-targets -- -D warnings
```

---

## M2：`xtask-source`

### 所有

`crates/xtask-source/**`

### 手順

1. **`pub fn normalize(path: &Path) -> Result<PathBuf, String>`** を作る。
   - `fs::canonicalize` した後、Windows の verbatim の接頭辞を外す。`\\?\C:\` は `C:\` に、`\\?\UNC\server\share` は `\\server\share` にする。
   - それ以外の OS では canonicalize の結果をそのまま返す。
   - xtask-core の `style.rs` の `without_verbatim_prefix` 相当の処理は、この関数に一本化する（C1 で消す）。
2. **行の数え方の割当ての削減。** `lines.rs` の `span.source_text()`（token ごとに String を作り、group では全体を写す）を、`span.byte_range()` で元の文字列の先頭の 2 バイトを見る方式に替える。
3. **前置きの写しの削減。** `without_file_preamble` は、前置き（shebang など）が無いときに写しを作らず、`Cow::Borrowed` を返す。
4. **shebang の判定（B9）。** `starts_inner_attribute` の空白の判定を、Rust の空白（`' '`、`'\t'`、`'\n'`、`'\r'`、U+000B、U+000C、U+0085、U+200E、U+200F、U+2028、U+2029）に限る。
5. **span の記録は、この crate では解放しない。** `proc_macro2::extra::invalidate_current_thread_spans()` は同じ thread の全 span を無効にし、呼出し側の AST を壊すので、library の中で呼ばない。span の記録は thread ごとにあり、解析の job の thread（付属書 C の C2b の `parallel::map`）と style の thread（C1）が終わるときに解放される。
6. **探索の範囲（B8）。** `rust_files` は、`Cargo.toml` を含む directory（入れ子の別の package）に下りない。
7. **寛容な fallback の削除。** `code_line_count` の、tokenize できないときに空でない行を数える fallback を消す。`pub fn code_line_count(source: &str) -> Result<usize, String>` を、今の `try_code_line_count` の本体にする（`try_code_line_count` という名前は消す。呼出し元は `check_files` と試験だけ）。

### 既存の試験の移行

| 試験 | 移行 |
|---|---|
| `tests/source.rs` の `assert_eq!(code_line_count(source), n)` の形の assert（全件） | API の追従：`assert_eq!(code_line_count(source), Ok(n))` にする。数は変えない |
| `malformed_or_unreadable_files_are_errors`（`tests/source.rs`） | 契約の変更（手順 7）。`assert_eq!(code_line_count("fn main() {\nlet s = \"unterminated\n"), 2)` を、`code_line_count(..)` が `Err` で、文に `cannot tokenize` を含む、に替える。他の assert は変えない |

### 試験

- `normalize_strips_verbatim`：`#[cfg(windows)]` で、`normalize` の結果が `\\?\` で始まらない。
- `nested_package_skipped`：`tests/fixture/Cargo.toml` を持つ一時 directory で、その下の `.rs` が `rust_files` に含まれない。
- `nbsp_is_not_whitespace`：`"#!\u{a0}[allow(x)]"` が shebang として扱われる。

### 確認

```bash
cargo test -p xtask-source
```

---

## M3：`xtask-modules`（M3a → M3b → M3c、同じ lane で順に行う）

### 所有（三つの unit で共通）

`crates/xtask-modules/**`。M3c は `dev/xtask/policy/workspace.toml` の `[analysis] trusted_macros` の行も所有する。

### 目的

- 名前解決の優先順（局所の宣言、明示の import、glob の import）を、データの構造で表す。
- 一つの target の解析で、ファイルの parse を一回にする。
- 暗黙の状態（bool の組、番兵の値、save/restore）を無くす。
- 属性（cfg を含む）の評価と診断の担当を一つにする。

### 既存の試験の移行（M3 全体）

| 試験 | 移行 | unit |
|---|---|---|
| すべての既存の試験 | `analyze` と `AnalysisOptions` を新しい API（`analyze(root, &settings, &[configuration])` の 1 件目）に書き換える。期待値は変えない | M3a |
| `static_inner_doc_attributes_and_block_docs_have_the_same_semantics`（`tests/roles.rs`） | 4 つ目の source（`//! General module documentation.\n//! Role: ...`）だけ、期待値を「role が `Err`、診断に `Role must be the first line of the first inner doc` を含む」に変える。他の 3 つは変えない | M3c |
| `formatted_macro_patterns_guards_and_json_values_are_references`（`tests/analysis.rs`） | source から `serde_json::json!(..)` の行を除き、期待値の辺から、その行だけが作っていた辺を除く。他（書式・pattern・guard）は変えない。json の振舞いは新しい試験 `json_requires_trust` が確かめる | M3c |

表に無い既存の試験が通らなくなったら STOP する。

---

### M3a：API、parse の共有、module の型、loader、cfg

#### 手順

1. **API。** `analyze` を次に替える。

   ```rust
   /// 一つの crate root を、与えた構成ごとに解析する。各ファイルの parse は一回。
   pub fn analyze(root_file: &Path, settings: &Settings, configurations: &[Configuration]) -> Vec<Result<ModuleGraph, String>>
   pub struct Settings { pub features: BTreeSet<String>, pub trusted_attributes: BTreeSet<String>, pub trusted_derives: BTreeSet<String>, pub trusted_macros: BTreeSet<String>, pub trusted_globs: BTreeSet<String>, pub max_role_chars: usize }
   pub struct Configuration { pub cfg: BTreeSet<String> }
   ```

   - 結果は `configurations` と同じ順に返す。
   - `Settings` は package ごとに一つ作る（`features` が package ごとに違うため。付属書 C の C2b）。
   - `AnalysisOptions` を消す。
   - `crate_role` と `crate_role_with_limit` を消す（D2）。crate の Role は `ModuleGraph` の root module の role を使う。
   - `DEFAULT_MAX_ROLE_CHARS` を消す。既定値は `xtask-policy` だけが持つ（C4）。
2. **parse の共有（I1）。**
   - loader の中に、正規化した path から parse の結果への表 `BTreeMap<PathBuf, Result<Rc<syn::File>, String>>` を置く。**失敗も記憶する**（同じ壊れたファイルを構成ごとに読み直さない）。表は `analyze` の呼出しの中だけで生き、構成ごとの load で共有する。
   - ファイルを読むのは、表に path が無いときだけ（`entry(path).or_insert_with(..)`）。path は手順 7 の `normalize` を通した後のもので引くので、同じファイルの別の書き方も一つになる。
   - inline の module の部分木は、`syn::ItemMod` の参照（index の path）で持ち、`clone` しない（I6）。
   - loader の `Source` は、その module の item を `Vec<Item>` で写さない。`Rc<syn::File>` と、その中の item の列への道筋（ファイルの root、または inline module の index の列）で持つ。
3. **module の型と ModuleGraph の契約（S6、S7、S8）。**

   ```rust
   pub struct Module { pub id: String, pub file: PathBuf, pub parent: Option<usize>, pub kind: ModuleKind, pub role: Result<String, String> }
   pub enum ModuleKind { File, Inline }
   ```

   - **契約**：`modules` は id の昇順。`modules[0]` は root（id `crate`、`parent == None`）。他の module は `parent == Some(i)` で `i` は自分の index より小さい（親の id は子の id の真の接頭辞なので、id の昇順で必ず先に来る）。root の `#![cfg(..)]` が偽でも root は残る（item と子を持たない）。
   - **index の付け替えは一か所で行う。** loader は見つけた順（子が先）に module を並べ、親は loader の中の番号で持つ。最後の関数 `fn finish(sources) -> Vec<Module>` が、id で並べ替え、旧 → 新の表を作り、`parent` と辺の端を一度に付け替える。他の場所で並べ替えない。
   - `role` の `Err` は Role の診断の文（diagnostics にも入る）。
   - loader は cfg で無効にした `mod` を記録する。resolve の `format!("{owner}::{}")` による無効の再判定（`resolve.rs` の 2 か所）を消す。
4. **属性の評価の担当（I8、B8、C1）。**
   - item に付いた属性（`cfg`、`cfg_attr`、`path`、`macro_use` など）の評価と診断は **loader だけ**が行う。loader は構成ごとに、module ごとの「有効な item の index の列」を作る。resolve と visitor は、この列だけをたどり、item の cfg を評価しない。
   - 関数の本体の中の文・式の `cfg` は、visitor が評価する（loader は本体に下りない）。一つの属性を評価する担当は、どちらか一つだけ。
   - inline module の属性は、親の module の item として loader が一度だけ評価・診断する（子の `source.attrs` では診断しない）。
5. **cfg の値（B5、C1）。**
   - `cfg(true)` は真、`cfg(false)` は偽にする。
   - 二つ以上の segment の `Meta::Path` は誤りにする。
   - item の cfg の誤りと式の cfg の誤りを、どちらも「診断を一つ出して、その item・式を無効とみなす」に統一する。解析は止めない。
6. **誤りの位置（B7）。** loader と cfg の誤りの文の先頭に、`{file}:{line}: ` を付ける。line は `span.start().line`（`span-locations` が有効）。
7. **loader の path と directory の所有（B6、S5）。**
   - loader が扱うファイルの path は、すべて `xtask_source::normalize` で正規化する。`role::root` の非正規の path も同じにする。
   - `Context.inline: bool` と `explicit_path` の組を、`enum DirOwnership { ModRs(PathBuf), NonModRs(PathBuf), Inline(PathBuf) }` に替える。
   - 読込み中の file の集合（`stack`）は、関数の引数で渡す `&mut Vec<PathBuf>` にする。`?` の誤りの経路でも外れるよう、push と pop を一つの関数の中で対にする。
8. **試験の helper（D6）。** 3 つの試験ファイルの一時 directory の helper を、`tests/common/mod.rs` の一つにする。一時 directory は `tempfile::tempdir()`（OS の一時領域）に作る。

#### 試験

- **`cfg_true_is_enabled`**：`#[cfg(true)] mod x;` の module が解析に含まれる。
- **`one_parse_two_configurations`**：`analyze(root, settings, &[normal, test])` が 2 件の結果を返し、`#[cfg(test)] mod tests;` が test の構成だけに現れる。
- **`parse_cache_keeps_failures`**（`src` の単体試験）：構文の誤りのある `b.rs` を `mod b;` で読む root を、二つの構成で load する。両方の結果が同じ誤りの文を持ち、parse の表の `b.rs` の項目が 1 個で `Err` である。
- **`errors_have_location`**：不正な `#[path]` の誤りの文が、`lib.rs:` を含む。
- **`invalid_cfg_reported_once`**：`#[cfg(a::b)] mod x {}` の inline module で、一つの構成の結果の診断に、その cfg の誤りがちょうど 1 件ある。
- **`modules_are_sorted_with_parents_first`**：`crate`、`crate::a`、`crate::a::x`、`crate::a_b` の module を持つ crate で、`modules[0].id == "crate"`、すべての `parent` が自分の index より小さく、`crate::a::x` の parent が `crate::a` の index。
- **`disabled_root_keeps_root_module`**：root が `#![cfg(any())]` の crate で、`modules.len() == 1`、`modules[0].id == "crate"`。

#### 確認

```bash
cargo check -p xtask-modules --all-targets
cargo test -p xtask-modules
```

---

### M3b：名前の表と名前解決

#### 手順

1. **名前の表（S2、B1、B3、B4、DEC-18）。** `Tables` と `strong` を、次の構造に替える。`Namespace` は今の `symbols::Namespace`（Type、Value、Macro）。

   ```rust
   pub(crate) struct Scope {
       /// この module の item の宣言（thread_local の static と #[macro_export] を含む）。名前空間ごと。
       declared: BTreeMap<(Namespace, String), BTreeSet<Target>>,
       /// 名前付きの use（別名を含む）。名前空間を問わず glob を隠す（DEC-18 の近似）。
       explicit: BTreeMap<String, BTreeSet<Target>>,
       /// glob の use で入った名前。名前空間ごと。
       globbed: BTreeMap<(Namespace, String), BTreeSet<Target>>,
       /// 未知の外部の glob（`use foo::*;` で、std・core・alloc と trusted_globs を除く）の path。
       /// 自分の glob と、glob で取り込んだ module のものの和。
       external_globs: BTreeSet<Vec<String>>,
   }
   ```

   - **宣言の名前空間**：

     | item | 名前空間 |
     |---|---|
     | `fn`、`const`、`static`（thread_local を含む） | Value |
     | tuple・unit の `struct` | Type と Value |
     | 名前付き field の `struct`、`enum`、`union`、`trait`、`type`、`mod`、`extern crate` | Type |
     | `macro_rules!`（`#[macro_export]`）、`macro` | Macro |

   - **キーは解決の前に決める。** `declared` のキーと、`explicit` のキー（名前付きの use の最後の segment か別名。値は空の集合から始める）は、構文から先にすべて作る（手順 3 の 1）。解決は値の集合を増やすだけで、キーを足さない。
   - **引き `lookup(scope, ns, name)`**：`declared[(ns, name)]` か `explicit[name]` のどちらかのキーがあれば、その二つの和を返す（値が空でもキーがあれば glob を隠す）。どちらも無ければ `globbed[(ns, name)]` を返す。これで局所の宣言が同じ名前空間の glob を隠し（B1）、別の名前空間の glob は隠さない。
   - **glob の import**（`use p::*`）：p の見える名前（`declared` のキー、`explicit` のキー、`globbed` のキー）について、名前空間ごとに `lookup(p, ns, name)` を自分の `globbed[(ns, name)]` に足す。`explicit` のキーは三つの名前空間すべてに足す。p の `external_globs` も自分の `external_globs` に足す（名前と同じ規則で伝わる。M04）。glob の path が複数の module に解決されたら、すべての module から足す（B4）。
   - **検索と供給は同じ `lookup` を使う。** 隠された名前は、glob で次の module に伝わらない。
   - **`"$external_globs"` の番兵の key** を消し、`external_globs` の field にする（B3）。
   - **block の中の scope**：`symbols.extend(self.symbols())` のような上書きを止める。引きは「block の scope を内から外へ、最後に module の scope」の順に `lookup` をたどる関数にする（I3）。block ごとの `Scope` は、block の `use` と item だけを持つ。
2. **外部 glob の判定の一本化（B2）。** `fn is_trusted_glob(path, settings) -> bool` を一つだけ作り、glob を `external_globs` に入れるかどうかの判定に使う。
   - std・core・alloc の path と `trusted_globs` の path を信頼する。
   - 「未知の外部 glob がある」は、引きの道筋の scope の `external_globs` が空でないことと同じにする。
3. **解決の手順（S1、I4、DEC-18、DEC-19）。** `(external_fallback, generated_values)` の bool の組の loop を、次の明示の手順に替える。
   1. **集める**（構文だけ）：module ごとに `declared` と `explicit` のキー、glob の import の列、自分の `external_globs` を作る。`thread_local!` の static もここで `declared` に足す（手順 6）。
   2. `resolve(Fallback::LocalOnly)`：worklist で、局所の path だけを解決する。
   3. `resolve(Fallback::External)`：worklist で、解決できない名前付きの import を外部の path として足す（今の `apply_import` の `external_fallback` の規則）。
   4. **thread_local の提供元の確認**（手順 6）。

   `resolve` の worklist：最初はすべての module を入れる。module を取り出して、その import をすべて適用する。`apply_import` は「自分の表の値が増えたか」を返す。増えたら、その module を `use` で参照する module（名前付きでも glob でも）を worklist に戻す。キーは増えないので、値の集合が単調に増えるだけで、有限の回数で止まる。表全体の clone と比較（`previous == tables`）は消す。
4. **名前空間（S3）。** `References.namespace` の field と、save/restore の 6 か所を消す。`fn reference(&mut self, path: &[String], namespace: Namespace)` に名前空間を引数で渡す。
5. **局所の束縛（S4）。** 局所の scope の値を `enum LocalBinding { Generic, Value, Item(Target) }` にする。空集合と `{LocalValue}` の番兵をやめる。`Target::LocalValue` を消す。module の直下と block の中は、visitor の引数の `Context::Module` と `Context::Block` で区別する。`scopes.is_empty()` による判定をやめる。
6. **thread_local!（B11、DEC-19）。** std の `thread_local!` を、組込みの文法として扱う。
   - **登録（手順 3 の 1、構文だけ）**：macro の呼出しの path が `thread_local`、`std::thread_local`、`::std::thread_local` のどれか、または同じ module の `use std::thread_local as X;`（`::std` も同じ）で入れた別名 `X` のとき、`thread_local_items.rs` の `Statics` で一回だけ parse し、宣言した名前を `declared` の Value に足す。型と初期化式の参照は visitor が visit する。
   - **提供元の確認（手順 3 の 4、解決の後）**：path が `std::thread_local` か `::std::thread_local` なら確か。そうでなければ、次のどれかに当たるとき不確かとし、`{module}: macro provider cannot be established: thread_local` の診断を一つ出す。登録は取り消さない（診断で不確かさを報告する）。
     - その module か外側に、`thread_local` という名前の局所の macro（`macro_rules!`、Macro の名前空間の宣言）がある。
     - `explicit` の `thread_local` が std 以外の path に解決された。
     - 引きの道筋の `external_globs` が空でない（glob で伝わったものを含む）。
     - `#[macro_use] extern crate` が `thread_local` を与えうる（今の `imports_unknown_thread_local` の規則）。
   - `trusted_macros` に無くても、確かな thread_local には診断を出さない。
   - `uncertain_binding`（`imports()` を呼び直す関数）は消す。確認は解決済みの表を使う（I7）。
7. **二重の処理の削除（I2、I5）。**
   - `intrinsic_derives()` は、attribute に `derive` があるときだけ求める。
   - block の中の `use` は一回だけ処理する（`visit_block` と `visit_item_use` の二重を止める）。
   - impl・trait の fn の `enabled` を一回だけ評価する（M3a の手順 4 の有効な item の列を使う）。
8. **文字列の id の削減（I9）。**
   - resolve の中で module を `usize` の index で持つ。`String` の id は、`ModuleGraph` を作るとき（M3a の `finish`）だけ使う。
   - `cfg.rs` の path を join してから split する処理を、segment の列のまま比べるようにする。
9. **glob が与えうる attribute（C2）。** module の引きの道筋に未知の外部 glob があるとき、組込みの attribute 名（`test`、`derive` など）も「提供元が不明」の診断にする。判定は手順 2 の `is_trusted_glob` と `external_globs` を使う。

#### 試験

- **`local_thread_local_shadows_glob`**：

  ```rust
  mod a { pub static FOO: u8 = 0; }
  mod g { pub use crate::a::*; }
  mod b { use crate::g::*; thread_local!{ static FOO: u8 = 1; } fn f() { FOO.with(|_| ()); } }
  ```

  診断が空で、`crate::b` から `crate::g` への辺がある（import の辺）。`crate::b` から `crate::a` への辺が無い（FOO は局所の thread_local に解決される）。
- **`declared_type_does_not_hide_glob_value`**：`mod a { pub fn X() {} } mod b { use crate::a::*; type X = (); fn f() { X(); } }` で、`crate::b` から `crate::a` への辺がある（値の X は glob から来る）。
- **`shadowed_name_is_not_forwarded`**：`mod a { pub fn f() {} } mod b { pub use crate::a::*; pub fn f() {} } mod c { use crate::b::*; fn g() { f(); } }` で、`crate::c` から `crate::a` への辺が無い（b の f が a の f を隠し、c には b の f だけが伝わる）。module の並びを逆にした source（c、b、a の順に書く）でも同じ。
- **`three_level_reexport_chain`**：a の item を b が `pub use crate::a::*`、c が `pub use crate::b::*`、d が `use crate::c::*` で使うとき、`crate::d` から `crate::a` への辺がある。
- **`external_glob_reexport_keeps_uncertainty`**：`mod p { pub use ext::*; } mod c { use crate::p::*; #[derive(Clone)] struct S; }` で、`crate::c` に「提供元が不明」の診断がある。
- **`block_glob_does_not_hide_module_glob`**：module で `use foo::*;`（外部）、block で `use std::fmt::*;` のとき、`#[derive(Clone)]` が「提供元が不明」の診断になる。
- **`glob_uses_every_resolved_module`**：glob の path が二つの module に解決される場合に、両方の名前が使える。
- **`trusted_glob_allows_thread_local`**：trusted_globs に `framework::prelude` を入れ、`use framework::prelude::*; thread_local!{...}` で、診断が空になる。
- **`untrusted_glob_makes_thread_local_uncertain`**：`use framework::prelude::*; thread_local!{ static C: u8 = 0; }`（trusted_globs が空）で、`macro provider cannot be established: thread_local` の診断がある。
- **`thread_local_type_reference`**：`thread_local!{ static C: RefCell<crate::cache::C> = ... }` で、`crate::cache` への辺がある。
- **`block_use_is_processed_once`**：block の中の `use crate::a::f;` の誤り（解決できない局所の import）の診断が 1 件だけ出る。

#### 確認

```bash
cargo check -p xtask-modules --all-targets
cargo test -p xtask-modules
```

---

### M3c：macro の文法の表、json・Token、Role

#### 手順

1. **json! と Token! の特別扱いの削除（D1、C3、DEC-10）。**
   - `src/json.rs`、`intrinsic.rs` の `serde_json::json` と `syn::Token` の項、`visitor.rs` の json と Token の分岐を消す。
   - 信頼した macro（`trusted_macros`）の引数の token は、今までどおり解析しない（中の参照は辺にならない）。
   - このリポジトリの `dev/xtask/policy/workspace.toml` の `[analysis] trusted_macros` に、`"json"`、`"serde_json::json"`、`"Token"`、`"syn::Token"` を足す（`Token![...]` は呼出しの path の書き方で一致させる）。
2. **文法の表（I7、D4、D5）。** `visitor.rs` の macro の文法と `intrinsic.rs` を、新しい `src/macros.rs` の一つの表にまとめる。

   ```rust
   /// 組込みの macro の引数の文法。
   pub(crate) enum MacroGrammar {
       /// `,` で区切った式の列。例：vec!（`[x; n]` の形も式として読む）、dbg!。
       Exprs,
       /// 先頭に `leading` 個の式、その後に任意で書式の文字列と、その引数（式、または `name = 式`）。
       Format { leading: usize },
       /// 式、`,`、pattern、任意で `if` 式。例：matches!、assert_matches!、debug_assert_matches!（その後に任意で書式）。
       Matches { trailing_format: bool },
       /// 引数を解析しない。例：compile_error!（文字列の literal だけ）。
       Opaque,
   }
   pub(crate) fn grammar(name: &str) -> Option<MacroGrammar>;
   ```

   | 名前 | 文法 |
   |---|---|
   | `format`、`format_args`、`print`、`println`、`eprint`、`eprintln`、`panic`、`unreachable`、`todo`、`unimplemented` | `Format { leading: 0 }` |
   | `write`、`writeln`、`assert`、`debug_assert` | `Format { leading: 1 }` |
   | `assert_eq`、`assert_ne`、`debug_assert_eq`、`debug_assert_ne` | `Format { leading: 2 }` |
   | `vec`、`dbg` | `Exprs` |
   | `matches` | `Matches { trailing_format: false }` |
   | `assert_matches`、`debug_assert_matches` | `Matches { trailing_format: true }` |
   | `compile_error` | `Opaque` |

   - 表に無い名前は、今の扱い（`trusted_macros` か、「提供元が不明」・未対応の診断）のまま。
   - token を表の文法どおりに parse できなければ、今の visitor と同じく、その macro の引数を visit しない（今と同じ診断を出す。新しい診断は足さない）。
   - `loader::attrs` を `ast.rs` に移す。`intrinsic::DERIVES` と `derive()` の中の名前の重複を一つにする（`intrinsic::DERIVES` だけにする）。
   - path を文字列にする関数を `ast::path_text` の一つにする。
3. **書式の暗黙の取込み（B9 の一部）。** `Format` の書式の文字列が文字列の literal（`syn::LitStr`）のとき、次の名前を値の名前空間の一 segment の参照として visit する。literal でなければ（`concat!` など）何もしない。
   - 文字列を左から読む。`{{` と `}}` は文字なので飛ばす。`{` から次の `}` までを一つの置換として読む。
   - 置換の中を、最初の `:` で「引数」と「書式」に分ける。
   - 引数が識別子（`[A-Za-z_][A-Za-z0-9_]*`、`r#` で始まるものを含む）なら、その名前。数字や空なら無し。
   - 書式の中の、幅・精度の `name$`（`{:name$}`、`{:.name$}`、`{:>width$.prec$}` など）の `name` も名前として取る。`.*` は名前ではない。
   - 取った名前のうち、明示の名前付きの引数（`name = 式`）にある名前は除く（明示が優先する）。
4. **Role の位置（C5、C6、D3）。**
   - `Role:` は、module の最初の inner doc の最初の行だけを有効にする。それ以外の行の `Role:` は `Role must be the first line of the first inner doc` の診断にし、role は `Err(その文)` にする。
   - `role.rs` の、`invalid_line` と重なる二度目の検査を消す。
5. **後続に回すもの。** 次は本改修では行わない（設計書 12 章）。
   - 定数を名指す識別子の pattern の扱いと、guard の `if let` の束縛（B9 の残り）。
   - 可視性（`pub`）の模型化（B10）。

#### 試験

- **`json_requires_trust`**：`fn f() { serde_json::json!({"a": crate::a::f()}) }` を、既定の Settings では診断が 1 件以上ある。`trusted_macros = {"serde_json::json"}` では診断が空で、`crate::a` への辺が無い（信頼した macro の中は解析しない）。
- **`role_must_be_first`**：`//! Doc.\n//! Role: x` が診断になり、role が `Err`。
- **`implicit_format_capture`**：`mod a { pub const N: u8 = 1; } mod b { use crate::a::N; fn f() -> String { format!("{N}") } }` で、`crate::b` から `crate::a` への辺がある。
- **`format_capture_rules`**：`format!("{{N}} {x:>w$.p$} {0}", 1, x = 2)` の取込みの名前が `{w, p}` だけ（`N` は飛ばす、`x` は明示の引数、`0` は数字）。`src` の単体試験で、取込みの関数を直接呼ぶ。
- **`macro_grammar_table`**：`grammar("assert_eq")` が `Some(Format { leading: 2 })`、`grammar("assert_matches")` が `Some(Matches { trailing_format: true })`、`grammar("compile_error")` が `Some(Opaque)`、`grammar("json")` が `None`。

#### 確認

```bash
cargo check -p xtask-modules --all-targets
cargo test -p xtask-modules
cargo clippy -p xtask-modules --all-targets -- -D warnings
```
