# 付属書 M：policy・workspace・source・modules

設計書 7 章の所見のうち、`xtask-policy`（新設）、`xtask-workspace`、`xtask-source`、`xtask-modules` に関わるものを直す手順。

各 unit は、先に「試験」の欄の試験を書く。その試験が失敗することを確かめ（試験が無いことによる compile error も失敗とみなす）、それから手順を実装する。

---

## M1：`xtask-policy` の新設と `xtask-workspace` の整理

### 所有

- `crates/xtask-policy/**`（新設）
- `crates/xtask-workspace/**`
- root の `Cargo.toml` の `[workspace.dependencies]` の `xtask-policy` の 1 行（A1 も同じ節に別の行を足す。作業指示書 1.3 節の規則で merge する）

### 目的

- policy の読込みと検証を、Cargo metadata から切り離す。
- 検証を一度だけ行い、検証済みの型だけが出回るようにする。
- `cargo metadata` の実行を一回にする。

### 手順

1. **crate の新設。** `crates/xtask-policy` を作る。
   - 依存は `serde`、`toml`（workspace）だけ。
   - `xtask-workspace/src/policy.rs` と `src/style.rs`（`StyleRule`、`StylePolicy`、`StyleException`）をここへ移す。
   - `xtask-workspace` から policy の型を消し、`pub use` も消す。
2. **型。** 公開の型は次のとおりにする。field は private にし、読出しの method を付ける。構築は `Policy::load(path)` と `Policy::from_toml(text)` だけにし、どちらも検証済みの値を返す。`Policy::validate` は公開しない。

   ```rust
   pub struct Policy { limits: Limits, diagrams: Diagrams, analysis: AnalysisSettings, style: Option<StyleSettings>, visible_to: BTreeMap<String, Vec<String>> }
   pub struct Limits { max_code_lines: u16 /* 1..=1000 */, max_role_chars: u16 /* 1 以上 */ }
   pub struct Diagrams { targets: DiagramTargets, output: PathBuf }
   pub enum DiagramTargets { All, Lib, LibBin }   // includes() は持たない（選択は xtask-core が TargetKind で行う）
   pub struct AnalysisSettings { target: Option<String>, features: Features, cfg: BTreeSet<String>, trusted_attributes: BTreeSet<String>, trusted_derives: BTreeSet<String>, trusted_macros: BTreeSet<String>, trusted_globs: BTreeSet<String> }
   pub enum Features { All, Selected { features: BTreeSet<String>, default_features: bool } }
   /// [style] enabled = true のときだけ Some。規則の組合せは読込み時に一度だけ決める。
   pub struct StyleSettings { rules: BTreeSet<StyleRule>, api_packages: BTreeSet<String>, options_structs: BTreeSet<String>, allowed_globs: BTreeSet<String>, toolchain: Option<String>, exceptions: Vec<StyleException> }
   ```

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
   - `max_code_lines` は 1 以上 1000 以下。`max_role_chars` は 1 以上。
   - 文字列の一覧（features、cfg、trusted_*、api_packages、options_structs、allowed_globs、visible_to の値）に、空の文字列、前後の空白を含む文字列、重複が無い。重複は誤りにする（黙って捨てない）。
   - `all_features = true` のとき、`features` が空で、`default_features` を書いていない（`true` の既定のまま）。違えば `"all_features cannot be combined with features or default_features"`。
   - style の規則の組合せの検査（non-exhaustive の `api_packages`、options-default の `options_structs` など）は、`enabled` にかかわらず、`extra_rules` に挙がっていれば行う。
   - 例外の `path` が `*` だけのものは誤りにする（`"exception path must name a file or a directory prefix"`）。
5. **出力先の移動。** 最上位の `output` を消し、`[diagrams] output` にする。既定値は `target/xtask/architecture.xtaskmap`。
   - 最上位に `output` が書かれた policy は、未知のキーとして誤りになる。誤りの文に `"output moved to [diagrams] output"` を足す。
6. **Workspace の型。** `xtask-workspace` の公開の型を次のとおりにする。

   ```rust
   pub struct Workspace { pub root: PathBuf, pub packages: Vec<Package>, pub edges: Vec<Dependency>, pub features: BTreeMap<String, BTreeSet<String>> }
   pub struct Package { pub name: String, pub manifest_path: PathBuf, pub targets: Vec<Target> }
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

   - `Package.id`、`Package.features`、`Package::active_features` は使われていないので消す（B7、O8）。
   - `features` は、package ごとの有効な feature（Cargo の resolve の結果）。
   - 例の struct には、既存の derive（`Clone, Debug` など）をそのまま付ける。
   - `is_library` の対象は Lib、Rlib、Staticlib、Cdylib、Dylib、ProcMacro。library の種類の一覧は、この関数だけに置く。
7. **metadata の一回化。** `Workspace::load(root: &Path, analysis: &xtask_policy::AnalysisSettings) -> Result<Workspace, String>` にする。`xtask-workspace` の依存に `xtask-policy` と `xtask-source` を足す。
   - command は一つの関数 `metadata_command(root, analysis)` で作る。内容は `cargo metadata --format-version 1 --locked --offline` に、features の指定と、`target` があれば `--filter-platform` を付けたもの。
   - 一回だけ実行し、一回だけ parse する。
   - package、target、依存の宣言は `packages` から、依存先の同一性と有効な feature は `resolve` から取る。
   - 同じ名前の registry の依存（patch）の判定も `resolve` を使う。`resolve` に無い依存（無効な optional の依存）は、workspace の member ではないとみなす。
   - `resolved_features`、`resolved_features_from_json`、`--no-deps` の実行、`--all-features` での再実行を消す。
   - 誤りの文 `provide full unfiltered Cargo metadata` は `cargo metadata did not return a resolve graph` に替える。
8. **path の正規化（B5）。** metadata から得る path（root、manifest、src_path）は、すべて `xtask_source::normalize(path)`（M2）で一度だけ正規化する。
   - `canonical()` の直接の呼出しをこの関数に替える。
   - M2 と同じ wave で並行して作るので、M1 では `xtask_source::normalize` の存在を前提に書いてよい。compile が通らなければ M2 の merge を待つ。STOP にはしない。
9. **可視性の検査。** `Workspace::check_visibility(&self, policy)` を `xtask_workspace` から消す。xtask-core の C1 が `fn visibility(workspace: &Workspace, visible_to: &BTreeMap<..>) -> Vec<String>` として持つ。policy の再検証もしない。
10. **試験の分割。** `src/tests.rs` を `src/tests/metadata.rs`（xtask-workspace）と、`crates/xtask-policy/src/tests.rs` に分ける。

### 試験

- 既存の試験のうち、消した API を使うものは新しい API に書き換える。
- 次を足す。
  - `limits_must_be_a_table`：`limits = [500, 40]` が誤りになる。
  - `zero_code_lines_rejected`
  - `empty_feature_rejected`
  - `duplicate_visible_to_rejected`
  - `style_checks_ignore_enabled`：`enabled = false` かつ `extra_rules = ["non-exhaustive"]` で、`api_packages` が無ければ誤りになる。
  - `output_moved`：最上位の `output` が誤りになり、その文に `[diagrams] output` を含む。
  - `target_kind_library`：`TargetKind::ProcMacro.is_library()` が true になる。

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
   - xtask-core の `style.rs` の `without_verbatim_prefix` 相当の処理は、この関数に一本化する（C2 で消す）。
2. **行の数え方の割当ての削減。** `lines.rs` の `span.source_text()`（token ごとに String を作り、group では全体を写す）を、`span.byte_range()` で元の文字列の先頭の 2 バイトを見る方式に替える。
3. **前置きの写しの削減。** `without_file_preamble` は、前置き（shebang など）が無いときに写しを作らず、`Cow::Borrowed` を返す。
4. **shebang の判定（B9）。** `starts_inner_attribute` の空白の判定を、Rust の空白（`' '`、`'\t'`、`'\n'`、`'\r'`、U+000B、U+000C、U+0085、U+200E、U+200F、U+2028、U+2029）に限る。
5. **span の記録の解放。** 1 ファイルを数え終えるたびに `proc_macro2::extra::invalidate_current_thread_spans()` を呼ぶ。
   - この関数が使える proc-macro2 の版か、`Cargo.lock` で確かめる。無ければこの手順を飛ばし、報告に書く。
6. **探索の範囲（B8）。** `rust_files` は、`Cargo.toml` を含む directory（入れ子の別の package）に下りない。
7. **未使用の API の削除。** `code_line_count` の寛容な fallback（使っているのが試験だけのもの）を消す。

### 試験

- `normalize_strips_verbatim`：`#[cfg(windows)]` で、`normalize` の結果が `\\?\` で始まらない。
- `nested_package_skipped`：`tests/fixture/Cargo.toml` を持つ一時 directory で、その下の `.rs` が `rust_files` に含まれない。
- `nbsp_is_not_whitespace`：`"#!\u{a0}[allow(x)]"` が shebang として扱われる。

### 確認

```bash
cargo test -p xtask-source
```

---

## M3：`xtask-modules`

### 所有

`crates/xtask-modules/**`。ただし `dev/xtask/policy/workspace.toml` の `[analysis] trusted_macros` の行も、この unit が所有する。

### 目的

- 名前解決の優先順（局所の宣言、明示の import、glob の import）を、データの構造で表す。
- 一つの target の解析で、ファイルの parse を一回にする。
- 暗黙の状態（bool の組、番兵の値、save/restore）を無くす。

### 手順

1. **API。** `analyze` を次に替える。

   ```rust
   /// 一つの crate root を、与えた構成ごとに解析する。各ファイルの parse は一回。
   pub fn analyze(root_file: &Path, settings: &Settings, configurations: &[Configuration]) -> Vec<Result<ModuleGraph, String>>
   pub struct Settings { pub features: BTreeSet<String>, pub trusted_attributes: BTreeSet<String>, pub trusted_derives: BTreeSet<String>, pub trusted_macros: BTreeSet<String>, pub trusted_globs: BTreeSet<String>, pub max_role_chars: usize }
   pub struct Configuration { pub cfg: BTreeSet<String> }
   ```

   - 結果は `configurations` と同じ順に返す。
   - `AnalysisOptions` を消す。
   - `crate_role` と `crate_role_with_limit` を消す（D2）。crate の Role は `ModuleGraph` の root module の role を使う。
   - `DEFAULT_MAX_ROLE_CHARS` を消す。既定値は `xtask-policy` だけが持つ（C4）。
2. **parse の共有（I1）。**
   - loader の中に、path から `Rc<syn::File>` への表（`BTreeMap<PathBuf, Rc<syn::File>>`）を置く。表は `analyze` の呼出しの中だけで生き、構成ごとの load で共有する。
   - inline の module の部分木は、`syn::ItemMod` の参照（index の path）で持ち、`clone` しない（I6）。
   - loader の `Source` は、その module の item を `Vec<Item>` で写さない。`Rc<syn::File>` と、その中の item の列への道筋（ファイルの root、または inline module の index の列）で持つ。
3. **module の型（S6、S7、S8）。**

   ```rust
   pub struct Module { pub id: String, pub file: PathBuf, pub parent: Option<usize>, pub kind: ModuleKind, pub role: Result<String, String> }
   pub enum ModuleKind { File, Inline }
   ```

   - `parent` は `modules` の index。
   - `role` の `Err` は Role の診断の文（diagnostics にも入る）。
   - loader は cfg で無効にした `mod` を記録する。resolve の `format!("{owner}::{}")` による無効の再判定（`resolve.rs` の 2 か所）を消す。
4. **名前の表（S2、B1、B3、B4）。** `Tables` と `strong` を、次の構造に替える。

   ```rust
   pub(crate) struct Scope {
       /// この module の item の宣言（thread_local の static と #[macro_export] を含む）。
       declared: BTreeMap<String, BTreeSet<Target>>,
       /// 名前付きの use（別名を含む）。
       explicit: BTreeMap<String, BTreeSet<Target>>,
       /// glob の use で入った名前。
       globbed: BTreeMap<String, BTreeSet<Target>>,
       /// 外部の glob（`use foo::*;`、std・core・alloc と trusted_globs を除く）の path。
       external_globs: BTreeSet<Vec<String>>,
   }
   ```

   - **引き**：`declared` と `explicit` に名前があれば、その和だけを返す。無ければ `globbed` を返す。これで局所の宣言が glob を隠す（B1）。後から `declared` に足した thread_local の名前も、自動で glob を隠す。
   - **glob の import**：相手の module の `declared ∪ explicit ∪ globbed` を、自分の `globbed` に写す。相手の `external_globs` は写さない（B3）。glob の path が複数の module に解決されたら、すべての module から写す（B4）。
   - **`"$external_globs"` の番兵の key** を消し、`external_globs` の field にする。
   - **block の中の scope**：`symbols.extend(self.symbols())` のような上書きを止める。引きは「block の scope を内から外へ、最後に module の scope」の順にたどる関数にする（I3）。block ごとの `Scope` は、block の `use` と item だけを持つ。
5. **外部 glob の判定の一本化（B2）。** `fn is_trusted_glob(path, settings) -> bool` を一つだけ作り、`visitor.rs` の 2 か所と `thread_local_items.rs` で使う。
   - std・core・alloc の path と `trusted_globs` の path を信頼する。
   - `has_unknown_external_glob` は、引きの道筋の scope の `external_globs` が空でないことと同じにする。
6. **収束の手順（S1、I4）。** `(external_fallback, generated_values)` の bool の組の loop を、次の明示の手順に替える。
   1. `resolve(Fallback::LocalOnly)`
   2. `resolve(Fallback::External)`
   3. `register_thread_locals()`
   4. `resolve(Fallback::External)`

   `resolve` は worklist で行う。`apply_import` が「表が増えたか」を返す。増えた module の import を持つ module を、worklist に戻す。表全体の clone と比較（`previous == tables`）は消す。
7. **名前空間（S3）。** `References.namespace` の field と、save/restore の 6 か所を消す。`fn reference(&mut self, path: &[String], namespace: Namespace)` に名前空間を引数で渡す。
8. **局所の束縛（S4）。** 局所の scope の値を `enum LocalBinding { Generic, Value, Item(Target) }` にする。空集合と `{LocalValue}` の番兵をやめる。`Target::LocalValue` を消す。module の直下と block の中は、visitor の引数の `Context::Module` と `Context::Block` で区別する。`scopes.is_empty()` による判定をやめる。
9. **二重の処理の削除（I2、I5、B8）。**
   - `intrinsic_derives()` は、attribute に `derive` があるときだけ求める。
   - block の中の `use` は一回だけ処理する（`visit_block` と `visit_item_use` の二重を止める）。
   - impl・trait の fn の `enabled` を一回だけ評価する。
   - inline module の attribute は、親だけが診断する（子の `source.attrs` では診断しない）。
10. **cfg（B5、C1）。**
    - `cfg(true)` は真、`cfg(false)` は偽にする。
    - 二つ以上の segment の `Meta::Path` は誤りにする。
    - item の cfg の誤りと式の cfg の誤りを、どちらも「診断を一つ出して、その item・式を無効とみなす」に統一する。解析は止めない。
11. **誤りの位置（B7）。** loader と cfg の誤りの文の先頭に、`{file}:{line}: ` を付ける。line は `span.start().line`（`span-locations` が有効）。
12. **thread_local!（B11）。** std の `thread_local!`（`thread_local`、`std::thread_local`、`::std::thread_local` と、それらへの別名）を、組込みの文法として扱う。
    - `trusted_macros` に無くても診断しない。
    - 宣言した名前を `declared` に足す。型と初期化式の参照を visit する。
    - parse は `thread_local_items.rs` の `Statics` を一回だけ使う。
13. **json! と Token! の特別扱いの削除（D1、C3）。**
    - `src/json.rs`、`intrinsic.rs` の `serde_json::json` と `syn::Token` の項、`visitor.rs` の json の分岐を消す。
    - このリポジトリの `dev/xtask/policy/workspace.toml` の `[analysis] trusted_macros` に、`"json"`、`"serde_json::json"`、`"Token"`、`"syn::Token"` を足す（`Token![...]` は呼出しの path の書き方で一致させる）。
14. **文字列の id の削減（I9）。**
    - resolve の中で module を `usize` の index で持つ。`String` の id は、`ModuleGraph` を作るときだけ使う。
    - `cfg.rs` の path を join してから split する処理を、segment の列のまま比べるようにする。
15. **試験の helper。** 3 つの試験ファイルの一時 directory の helper を、`tests/common/mod.rs` の一つにする。一時 directory は `tempfile::tempdir()`（OS の一時領域）に作る（D6）。
16. **loader の path と directory の所有（B6、S5）。**
    - loader が扱うファイルの path は、すべて `xtask_source::normalize` で正規化する。`role::root` の非正規の path も同じにする。
    - `Context.inline: bool` と `explicit_path` の組を、`enum DirOwnership { ModRs(PathBuf), NonModRs(PathBuf), Inline(PathBuf) }` に替える。
    - 読込み中の file の集合（`stack`）は、関数の引数で渡す `&mut Vec<PathBuf>` にする。`?` の誤りの経路でも外れるよう、push と pop を一つの関数の中で対にする。
17. **文法の表（I7、D4、D5）。**
    - `visitor.rs` の macro の文法（`format!` の類、`assert!` の類など）と `intrinsic.rs` を、新しい `src/macros.rs` の一つの表 `MacroGrammar` に移す。
    - 表に `unreachable`、`assert_matches`、`debug_assert_matches`、`compile_error` を足す（B9 の一部）。
    - `format!` の類では、書式の文字列の中の暗黙の取込み（`{NAME}`、`{NAME:?}`）の名前を、値の名前空間の参照として visit する。
    - `loader::attrs` を `ast.rs` に移す。`intrinsic::DERIVES` と `derive()` の中の名前の重複を一つにする。
    - path を文字列にする関数を `ast::path_text` の一つにする。
    - `thread_local_items.rs` の `uncertain_binding` は、`imports()` を呼び直さず、resolve が作った use の表を引数で受け取る。
18. **glob が与えうる attribute（C2）。** module の引きの道筋に未知の外部 glob があるとき、組込みの attribute 名（`test`、`derive` など）も「提供元が不明」の診断にする。
    - 判定は手順 5 の `is_trusted_glob` と `external_globs` を使う。
19. **Role の位置（C5、C6、D3）。**
    - `Role:` は、module の最初の inner doc の最初の行だけを有効にする。それ以外の行の `Role:` は `Role must be the first line of the first inner doc` の診断にする。
    - `role.rs` の、`invalid_line` と重なる二度目の検査を消す。
20. **後続に回すもの。** 次は本改修では行わない（設計書 12 章）。
    - 定数を名指す識別子の pattern の扱いと、guard の `if let` の束縛（B9 の残り）。
    - 可視性（`pub`）の模型化（B10）。

### 試験

- **`local_thread_local_shadows_glob`**：`mod a { pub static FOO: u8 = 0; } mod b { use crate::a::*; thread_local!{ static FOO: u8 = 1; } fn f(){ FOO.with(|_| ()); } }` で、診断が空になり、`crate::b` から `crate::a` への辺が無い。
- **`block_glob_does_not_hide_module_glob`**：module で `use foo::*;`（外部）、block で `use std::fmt::*;` のとき、`#[derive(Clone)]` が「提供元が不明」の診断になる。
- **`trusted_glob_allows_thread_local`**：trusted_globs に `framework::prelude` を入れ、`use framework::prelude::*; thread_local!{...}` で、診断が空になる。
- **`cfg_true_is_enabled`**：`#[cfg(true)] mod x;` の module が解析に含まれる。
- **`thread_local_type_reference`**：`thread_local!{ static C: RefCell<crate::cache::C> = ... }` で、`crate::cache` への辺がある。
- **`one_parse_two_configurations`**：`analyze(root, settings, &[normal, test])` が 2 件の結果を返し、`#[cfg(test)] mod tests;` が test の構成だけに現れる。
- **`errors_have_location`**：不正な `#[path]` の誤りの文が、`lib.rs:` を含む。
- **`role_must_be_first`**：`//! Doc.\n//! Role: x` が診断になる。
- **`implicit_format_capture`**：`mod a { pub const N: u8 = 1; } mod b { use crate::a::N; fn f() -> String { format!("{N}") } }` で、`crate::b` から `crate::a` への辺がある。

既存の試験は、新しい API に書き換えて残す。期待値は変えない。変えないと通らないときは STOP にする。

### 確認

```bash
cargo check -p xtask-modules --all-targets
cargo test -p xtask-modules
```
