# 付属書 C：`xtask-core`、`xtask-style`、文書

設計書 5.1 節の契約（CLI と library の API）を実装する手順。旧い図の経路（Node、SVG、HTML）を消す。

---

## C1：`xtask-style` の新設

### 所有

- `crates/xtask-style/**`（新設）
- `crates/xtask-core/src/style.rs`、`style_source.rs`、`style_source/**`、`style_tools.rs`、`style_tools/**`（ここから移して消す）
- `crates/xtask-core/tests/style.rs` は移さない。xtask-core の公開 API を通す端から端の試験なので、C2 が新しい API に直す。

### 手順

1. **crate の新設と移動。** `crates/xtask-style` を作り、上の xtask-core のファイルを移す。
   - 依存は `xtask-policy`、`xtask-workspace`、`xtask-source`、`syn`、`proc-macro2`、`toml`、`serde_json`、`tempfile`。
2. **公開 API。** 次の一つにする。

   ```rust
   pub struct StyleInput<'a> {
       pub root: &'a Path,                       // xtask_source::normalize 済み
       pub workspace: &'a xtask_workspace::Workspace,
       pub analysis: &'a xtask_policy::AnalysisSettings,
       pub style: &'a xtask_policy::StyleSettings,
       pub files: &'a [PathBuf],                 // 検査する .rs（正規化・重複除去済み）
       /// file → その file の module の path（`crate::a::b`）。inline module の item の名前付けに使う。
       pub modules: &'a BTreeMap<PathBuf, String>,
   }
   pub fn check(input: &StyleInput<'_>) -> StyleReport;
   pub struct StyleReport { pub diagnostics: Vec<StyleDiagnostic>, pub errors: Vec<String> }
   ```

   - `StyleDiagnostic` の型は xtask-core の `model.rs` から移す。
   - `errors` は、道具の実行の誤り（rustfmt の未導入、compile の失敗など）。
   - `ToolReport` を消し、`StyleReport` に一本化する（S4）。
3. **一つの壊れたファイルで全体を止めない（B1）。** `style_source::check` は、ファイルの読込みと parse の誤りをそのファイルの `errors` に入れて、次のファイルに進む。
4. **root の一本化（B4）。**
   - `style.rs` の `canonicalize(config.root)` と `without_verbatim_prefix` を消す。root は呼び出し側が `xtask_source::normalize` 済みのものを渡す。
   - `probe.rs`、`clippy.rs`、`style_tools.rs` の `root.join(...)` は、この root を使う。
   - clippy の `file_name` を join した後、その path を `xtask_source::normalize` で正規化する。
5. **並べ替えの順（B7）。** path の正規化を先に行い、その後で sort と dedup を行う。
6. **file の一覧と module の表（C6、I3）。**
   - `style.rs` の中のファイルの探索（`rust_files` の二度目の呼出し）を消し、`input.files` を使う。
   - `source_items` の中の、module ごとの canonicalize の loop を消し、`input.modules` で引く。
7. **metadata と probe（I1、I5）。** `probe.rs` の `cargo metadata` の実行を消し、`input.workspace` を使う。
   - root の `Cargo.toml` の parse は、`check` の呼出しの中で一回だけ行い、各 probe に渡す。
   - 各 probe の一時 directory と `Cargo.lock` の写しは、一つの一時 directory を共有する。selector ごとに一つの probe の crate を作る現在の方式を、一つの crate に selector ごとの関数を並べる方式に替える。
8. **rustfmt の width の問合せ（I4）。**
   - `width_limit` は、source の directory ごとではなく、最も近い `rustfmt.toml`（または `.rustfmt.toml`）の directory ごとに一回だけ `rustfmt --print-config` を実行する。
   - 設定ファイルが無い directory はすべて root の一回にまとめる。
9. **独立な process の並列化（I5）。** fmt の検査と clippy の群は、`std::thread::scope` で並列に走らせる。結果は群の順に並べる。
10. **重複の検証の削除（C5）。** `style.rs` の先頭の policy の再検証（`style.validate()` 相当）を消す。

### 試験

- 既存の `src/style_tools/tests.rs` と `src/style_source/tests.rs` を移し、新しい API に合わせる。
- clippy・rustfmt・cargo を実際に起動する試験は、`crates/xtask-style/tests/tools.rs`（統合試験）に移す。対象は `real_clippy_*`、`changed_clippy_*`、`default_probe_*`、`cross_package_*`、`configured_*`、`empty_cfg_*`。
  - これらは時間がかかるので、unit では実行しない。checkpoint で実行する。
  - 内部の関数を使う試験は、その関数を `#[doc(hidden)] pub` にせず、公開の `check` を通す形に書き換える。書き換えられなければ STOP する。
- `src` に残す単体試験は、外部の process を起動しないものだけにする。
- `unparsable_file_does_not_hide_others`：二つのファイルのうち一つが構文の誤りのとき、もう一つのファイルの `hex-literals` の違反が報告され、`errors` に壊れたファイルの path が入る（単体試験）。

### 確認

```bash
cargo check -p xtask-style --all-targets
cargo test -p xtask-style --lib
cargo clippy -p xtask-style --all-targets -- -D warnings
```

---

## C2：`xtask-core` の組み直し

### 所有

- `crates/xtask-core/**`（C1 が移したファイルを除く。`tests/style.rs` を含む）
- `dev/layout-baseline/capture/`、`dev/layout-baseline/README.md`
- `crates/xtask-graph/**`、`crates/xtask-svg/**`（削除）
- root の `Cargo.toml`
- `dev/xtask/policy/workspace.toml`（M3 が所有する `trusted_macros` の行を除く）
- `architecture/`（削除）
- `.cargo/config.toml`

### 手順

1. **消すもの。**
   - `crates/xtask-graph`、`crates/xtask-svg` の directory 全体。
   - `crates/xtask-core/src/arch_map.rs` と `src/arch_map/`、`src/graphs.rs` と `src/graphs/`。
   - `tests/integration/arch_map.rs`、`tests/integration/diagrams.rs`。
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
   - root の一本化（W の B6）：`Context::load` は、`xtask_source::normalize(config.root)` と `Workspace.root` が違えば、`--root must name the workspace root ({workspace root})` の誤りにする。以後の root は `Workspace.root` だけを使う。
   - このリポジトリの `src/main.rs` は、`current_dir()` ではなく `Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2)` を root にする（このリポジトリの自己検査の専用。subdirectory から実行しても正しい root になる）。
   - `max_role_chars` の上書きと `prune`、`no_write` を消す。
   - policy のファイルが無いときは、`run` と `analyze` のどちらも `Policy::default()` を使う。挙動を一つにし、README にも書く。
4. **段階の分離（S1、S2、S3）。** `run.rs` を次の流れに組み直す。共有の `Report` を段階ごとに書き換える方式をやめる。各段階は自分の結果を返し、最後に連結する。

   ```text
   let context = Context::load(config)?;       // policy、Workspace（metadata 一回）、rustc cfg（target と host を最初に一度ずつ）
   let analysis = analyze(&context)?;            // Vec<TargetAnalysis>、Findings
   let lines = check_lines(&context, &analysis); // Findings
   let visibility = visibility(&context);        // Findings
   let style = style(&context, &analysis);       // Option<StyleReport>（StyleSettings が Some のとき）
   let written = match config.output { Output::None => None, _ => Some(write_archive(&context, &analysis)?) };
   Report::merge(...)
   ```

   ```rust
   pub struct TargetAnalysis { pub package: String, pub target: String, pub kinds: Vec<xtask_workspace::TargetKind>, pub outcomes: Vec<(Mode, TargetOutcome)> }
   // Mode は xtask_archive::Mode を使う（同じ事実の型を二つ持たない）。
   pub enum TargetOutcome { Analyzed(xtask_modules::ModuleGraph), FeatureGated, UnsupportedEdition(String), Failed(String) }
   pub struct Report { pub errors: Vec<String>, pub notes: Vec<String>, pub style: Vec<StyleDiagnostic>, pub archive: Option<PathBuf> }
   impl Report { pub fn is_success(&self) -> bool { self.errors.is_empty() && self.style.is_empty() } }
   ```

   - `Analysis`、`SkippedTarget`、`TargetGraph`、`Report.artifacts`、`Report.style_diagnostics` を消す。
   - style の診断は `Report.style` にだけ入れ、`errors` に写さない。CLI は `errors` と `style` を両方表示する。
   - `analyze` の公開の型は `pub fn analyze(config: &Config) -> Result<(Vec<TargetAnalysis>, Report), String>` にする。
5. **解析の job（I1、S5、I10）。**
   - job は target ごとに一つにする。target の構成の列（Normal、Test、または両方）を `xtask_modules::analyze(root, &settings, &configurations)` に一度で渡す（付属書 M の M3）。
   - `Settings` は job の間で `&` で共有し、clone しない。
   - host の cfg は、`[analysis] target` があるときだけ `rustc --print cfg` を二回目として実行する。無いときは target の cfg を使う。どちらも job の作成の前に決める。
6. **並列化の失敗の情報（B2）。** `parallel::map` を次の設計に替える。`std::panic::catch_unwind` は使わない（style の規則 18）。

   ```rust
   /// 入力ごとの結果。worker が panic した入力は Err（panic の文）。
   pub(crate) fn map<T: Sync, R: Send>(inputs: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<Result<R, String>>
   ```

   1. 入力ごとに結果の枠 `Vec<std::sync::OnceLock<R>>` を作る。次に取る入力の index を `AtomicUsize` で持つ。
   2. worker ごとに「処理中の index」の `AtomicUsize` を持つ（`usize::MAX` は「無し」）。
   3. worker は次を繰り返す。index を一つ取り、処理中の index に書き、`f` を呼び、結果を枠に入れ、処理中の index を `usize::MAX` に戻す。
   4. `thread::scope` の中で全 worker を join する。join が `Err(payload)` の worker については、その処理中の index を失敗とする。文は payload を `&str` か `String` に downcast したもの（できなければ `"worker panicked"`）。
   5. 他の worker は index を取り続けるので、残りの入力は処理される。
   6. 結果は、枠に値があれば `Ok`、失敗の印があれば `Err(文)` にする。両方とも無い枠は無い（全 index が取られている）。

   呼出し側の扱い：
   - 解析の job の `Err` は、その target の構成すべてを `TargetOutcome::Failed(format!("analysis panicked: {文}"))` にする。
   - 配置（`draw`）の `Err` は、`xtask_diagram::failed(plan, format!("layout panicked: {文}"))` にする。
7. **行数の検査（C6、B8）。**
   - 検査するファイルは、`xtask_source::rust_files`（package ごと）と、解析した module のファイルの和にする。どちらも正規化した path で dedup する。
   - この一覧を C1 の `StyleInput.files` にも渡す。
   - file → module の path の表（`StyleInput.modules`）は、解析結果から一回だけ作る。同じファイルが複数の target に現れたら、最初の target の module の path を使う。
8. **可視性の検査。** 付属書 M の M1 の手順 9 のとおり、`fn visibility(workspace, visible_to) -> Vec<String>` をここに置く。
   - 内容は旧 `Workspace::check_visibility` から policy の再検証を除いたもの。
9. **図のファイルの書出し。**
   - `xtask_diagram::plan` の入力を組み立てる。
     - `targets` には、解析したすべての target の構成を入れる。`selected` は、policy の `[diagrams] targets` で選ぶ。選択の関数 `fn selected(targets: DiagramTargets, kinds: &[TargetKind]) -> bool` を xtask-core に置く。`All` は常に true、`Lib` は `is_library` のものがあれば true、`LibBin` はそれに `Bin` を加える。
     - `TargetOutcome::FeatureGated` は `Outcome::Skipped("required features are inactive")` にする（mode は Normal の一件）。
     - `UnsupportedEdition(s)` と `Failed(s)` は `Outcome::Failed(s)` にする。
     - crate の Role は、`plan` が `primary_target` で求める（付属書 A の 2.3 節）。xtask-core は求めない。
   - `parallel::map(&plans, xtask_diagram::draw)` で並列に配置する。
   - `xtask_archive::encode` で符号化する。
   - 出力先の親 directory を作り、`tempfile::NamedTempFile::new_in(親)` に書いて `persist(出力先)` する。Windows の「先に消してから rename」はしない（B3）。
   - `Report.notes` に `wrote {path} ({n} diagrams)` を足し、`Report.archive` に path を入れる。
   - root_name は、正規化した root の最後の要素。
10. **library の公開の名前。** `lib.rs` の公開を次にする。

    ```rust
    pub use cli::{main, parse_args, Command};
    pub use config::{Config, Output};
    pub use model::{Report, TargetAnalysis, TargetOutcome};
    pub use xtask_archive::Mode;
    pub use run::{analyze, run};
    pub use xtask_policy::{AnalysisSettings, DiagramTargets, Features, Policy};
    pub use xtask_modules::ModuleGraph;
    ```

    - `StyleDiagnostic` は `xtask_style` から re-export する。
    - doc の Node・SVG・HTML の記述を消す。
11. **依存。**
    - root の `Cargo.toml` の `[workspace.dependencies]` に、`xtask-style = { path = "crates/xtask-style" }` と `xtask-diagram = { path = "crates/xtask-diagram" }` を足す。
    - `xtask-core/Cargo.toml` の依存は、xtask-source、xtask-policy、xtask-workspace、xtask-modules、xtask-style、xtask-diagram、xtask-archive、tempfile。
    - `serde_json`、`syn`、`proc-macro2`、`toml` は、残ったコードが使わなければ消す。
12. **policy のファイル。** `dev/xtask/policy/workspace.toml` を次の内容にする（M3 の行を除く）。
    - 最上位の `output` を消す。
    - `visible_to` を設計書 4.2 節の表に替える。
13. **基準の取得の script の削除（AC-1）。**
    - `dev/layout-baseline/capture/tee.cjs` と `node-tee.cmd` を消す。
    - `dev/layout-baseline/README.md` の「取得方法」の節を、次の 1 文に替える：「Node.js と elkjs を使って取得した。改修で Node.js を削除したため、再取得の手段は無い。取得に使った script は commit `e7eb600` にある」。
    - `split.py` は残す。
14. **`tests/style.rs`。** 端から端の style の試験を、新しい API（`Report.style`、`Output::None`）に直す。`check_unchanged` の「`architecture` の directory が無い」の assert は、「`target/xtask` が無い」に替える。
15. **試験の書換え。**
    - `tests/integration.rs` から次を消す：`available_node`、`XTASK_NODE`、`module_svg`、SVG と HTML を確かめる試験、`artifacts` の件数の assert。
    - 次を足す。
      - `check_writes_nothing_by_default`：一時のリポジトリで `check` を実行した後、`target/xtask` が存在しない。
      - `write_creates_archive`：`--write` で `target/xtask/architecture.xtaskmap` ができる。`xtask_archive::decode` が成功し、`diagrams[0]` が Crates になる。
      - `old_flags_are_rejected`：`--no-write` の終了コードが 2 になる。
    - `examples/benchmark.rs` は、引数の `OUTPUT` と `no-write`・`write` の mode を消し、`check` と `check --write` の時間を出すようにする。
16. **最適化の profile。** root の `Cargo.toml` に次を足す。debug build でも図の配置を速くするため。README（C3）にも、利用側の root の `Cargo.toml` に同じ行を足すよう書く。

    ```toml
    [profile.dev.package.xtask-layout]
    opt-level = 3
    ```

    同じ `Cargo.toml` の `[workspace]` に `exclude = ["viewer"]` を足す。

### 確認

```bash
cargo check --workspace --all-targets
cargo test -p xtask-core --lib
cargo test -p xtask-core --test integration -- check_writes_nothing_by_default write_creates_archive old_flags_are_rejected
```

---

## C3：文書

### 所有

`README.md`、`README_JP.md`、`LICENSE-*` を除く文書、`THIRD-PARTY-NOTICES.md`（あれば）

### 手順

1. `README_JP.md` と `README.md` を、設計書 3 章（利用者の経路）と 5.1 節に合わせて書き直す。二つの README は同じ構成・同じ内容にする。

   | 対象 | 変更 |
   |---|---|
   | 動作要件 | Node.js と `XTASK_NODE` の項を消す |
   | 使い方 | `check` と `check --write` だけにする |
   | 設定 | 最上位の `output` を消し、`[diagrams] output` を書く。`[analysis]` の `features`・`all_features`・`default_features` の組合せの制約を書く |
   | 出力 | 節を「図のファイル」に書き替える：一つの圧縮したファイル、既定の場所、viewer で開くこと、`viewer/` の build の方法、GitHub Actions の成果物 |
   | 移行 | 節を足す：`--no-write` は既定になったこと、`--prune` の削除、旧い `architecture/` の SVG・HTML を手で消すこと、`trusted_globs` と `allowed_globs` の両方に書く必要があること |
   | crate の表 | 設計書 4.1 節に合わせる |
   | 第三者の notice | elkjs の行を消す |

2. 解析の注意（README の「解析」節）に、次の 2 点を足す。
   - `json!` と `Token!` の特別扱いを止めたので、使う repository は `trusted_macros` に書くこと。
   - `cfg(true)` に対応したこと。

### 確認

二つの README の見出しの列が一致することを、目で確かめる。
