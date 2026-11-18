# xtask-core 改修設計書：Node.js の除去、図のファイル、viewer

版 1.0（2026-11-18）。進め方は [REWORK_WORK_ORDER.md](REWORK_WORK_ORDER.md)（以下「作業指示書」）、unit ごとの手順は付属書 `rework/*.md`（以下「付属書」）に書く。

## 0. 文書の構成と優先順位

| 文書 | 持つもの |
|---|---|
| 本書 | 目的、受入れ条件、構成、公開の契約、内部の規則、監査の判定、設計者の決定 |
| 付属書 | unit ごとの手順と試験 |
| 作業指示書 | wave・lane・gate・STOP の規則 |

食い違いがあれば、本書 5 章（契約）、付属書、作業指示書の順に優先する。付属書が 5 章と食い違うときは STOP する（作業指示書 0.4 節）。

| 付属書 | 範囲 |
|---|---|
| [rework/L-layout.md](rework/L-layout.md) | 配置エンジン `xtask-layout` |
| [rework/A-archive.md](rework/A-archive.md) | 図のファイル `xtask-archive` と、図の組立て `xtask-diagram` |
| [rework/M-model.md](rework/M-model.md) | `xtask-policy`（新設）、`xtask-workspace`、`xtask-source`、`xtask-modules` |
| [rework/C-core.md](rework/C-core.md) | `xtask-style`（新設）、`xtask-core`、README |
| [rework/V-viewer.md](rework/V-viewer.md) | viewer（`viewer/`）と GitHub Actions |

---

## 1. 目的と受入れ条件

### 1.1 目的

1. 図の生成に Node.js を要らなくする。elkjs の代わりに Rust の配置エンジンを持つ。
2. 図の出力を、圧縮した一つのファイルにする。`check` は既定で何も書かない。SVG と HTML は出さない。
3. そのファイルを読む GUI の viewer を、Kivact で作る。SVG への書出しは viewer が行う。
4. 監査で見つけた不具合・矛盾・非効率・不要な要素・暗黙の状態機械を、根本から整理する（7 章）。

### 1.2 受入れ条件

| ID | 条件 | 確かめ方 |
|---|---|---|
| AC-1 | リポジトリに `.js`・`.cjs`・`.html`・`.css` が無く、`node` が `PATH` に無い環境で `cargo xtask check --write` が成功する | CP の手順 |
| AC-2 | `cargo xtask check` は何も書かない。`--write` は `[diagrams] output` に一つのファイルを書き、`xtask_archive::decode` で読める | `xtask-core` の統合試験 |
| AC-3 | `dev/layout-baseline` の全 146 件の入力で、配置が成功し、出力の不変条件を満たし、決定的である | `xtask-layout` の `tests/baseline.rs` |
| AC-4 | viewer が 5.5 節の機能を持ち、main への push で GitHub Actions が Windows 向けの実行ファイルを成果物として残す | V3 の試験、workflow の定義 |
| AC-5 | 7 章の判定で「対応」とした所見がすべて閉じている | CP の点検 |
| AC-6 | `cargo test --workspace`（root）と `cargo test`（viewer）が成功する。xtask 自身の検査（`cargo xtask check`）を、このリポジトリと viewer の両方に対して実行し、違反が 0 件 | CP |
| AC-7 | `README.md` と `README_JP.md` が同じ構成で、新しい挙動を説明する | CP の点検 |

---

## 2. KPI

KPI は CP で測って記録する。gate ではないが、目標に届かなければ設計者への差戻しにする（作業指示書 0.5 節）。

| ID | 指標 | 目標 |
|---|---|---|
| K1 | ELK の基準と比べた交差数の合計の比（`baseline_report` の合計の行） | 1.3 以下 |
| K2 | 折れの数の合計の比 | 1.3 以下 |
| K3 | 面積の合計の比 | 1.5 以下 |
| K4 | Kivact の 121 件の配置の合計時間（release、1 thread） | 3 秒以下 |
| K5 | このリポジトリの図のファイル（`check --write`）の大きさ | 100 KB 以下 |
| K6 | このリポジトリでの `cargo xtask check`（2 回目、build 済み）の時間 | 改修前の `--no-write` 以下 |

---

## 3. 利用者の経路

### 3.1 検査（変わらない部分）

`cargo xtask check` は、行数・可視性・Role・循環・style を検査し、違反を報告する。終了コードは、成功が 0、違反または実行の誤りが 1、引数の誤りが 2。**何も書かない。**

### 3.2 図を作る

`cargo xtask check --write` は、検査に加えて図を配置し、`[diagrams] output`（既定 `target/xtask/architecture.xtaskmap`）に一つのファイルを書く。

- 検査の違反があっても、作れる図は書く。
- 解析に失敗した target の図は、失敗の理由として記録する。

### 3.3 図を見る

1. viewer（`xtask-viewer.exe`）を、引数にファイルを渡すか、Open のダイアログか、drag & drop で開く。
2. ファイルが書き換わると、自動で読み直す。
3. 左の木と検索で crate・target・module を探し、右の図で依存をたどる。
4. 表示中の図を SVG に書き出せる。

### 3.4 viewer を得る

- main に push すると、GitHub Actions が Windows 向けの `xtask-viewer.exe` を build し、成果物（artifact）として残す。
- 手元では `cd viewer && cargo build --release` で作れる。

---

## 4. 目標の構成

### 4.1 crate

**xtask の workspace（`crates/`、Rust 1.85）：**

| crate | 役割 | 変更 |
|---|---|---|
| `xtask-source` | ファイルの探索、path の正規化、行数 | 整理（M2） |
| `xtask-policy` | policy の読込みと検証（検証済みの型） | **新設**（M1、xtask-workspace から分離） |
| `xtask-workspace` | Cargo metadata（一回の実行） | 整理（M1） |
| `xtask-modules` | module の解析 | 整理（M3） |
| `xtask-style` | Google style の検査 | **新設**（C1、xtask-core から分離） |
| `xtask-layout` | 配置エンジン（Rust のみ） | **書き直し**（L1〜L8） |
| `xtask-archive` | 図のファイルの型・符号化・圧縮・文字の規則 | **新設**（A1） |
| `xtask-diagram` | 解析結果から図を組み立てて配置する | **新設**（A2、xtask-core の graphs.rs と xtask-graph から） |
| `xtask-core` | CLI と全体の制御、行数・可視性の検査 | 整理（C2） |
| `xtask-graph` | — | **削除** |
| `xtask-svg` | — | **削除**（描画の規則は viewer の `xtask-render` へ） |

**viewer の workspace（`viewer/`、Rust 1.98.1、Kivact）：**

| crate | 役割 |
|---|---|
| `xtask-render` | `Scene` から描画の一覧（角の丸め、交差の跳び、矢印、文字の位置）を作る。Kivact に依存しない |
| `xtask-viewer` | Kivact の GUI。描画は `xtask-render` の結果を Kivact の `Draw` に写すだけ |

分割の理由（境界）：

| crate | 境界 |
|---|---|
| `xtask-policy` | 純粋な TOML の検証。process を起動しない |
| `xtask-style` | 入力が file の一覧と policy で、出力が診断。外部の道具（rustfmt、clippy）を起動する |
| `xtask-archive` | 二つの workspace と二つのツールチェーンから使う、形式の唯一の定義 |
| `xtask-diagram` | 入力が解析の結果で、出力が `Archive` |
| `xtask-render` | GUI を起動せずに試験できる幾何の計算 |

### 4.2 依存の向きと `visible_to`

```text
xtask-core ─┬─> xtask-style ─┬─> xtask-policy
            │                ├─> xtask-workspace
            │                └─> xtask-source
            ├─> xtask-diagram ─┬─> xtask-layout
            │                  ├─> xtask-archive
            │                  ├─> xtask-modules ──> xtask-source
            │                  └─> xtask-workspace ─┬─> xtask-policy
            │                                       └─> xtask-source
            ├─> xtask-archive
            ├─> xtask-modules
            ├─> xtask-policy
            ├─> xtask-workspace
            └─> xtask-source
viewer: xtask-viewer ──> xtask-render ──> xtask-archive（path 依存）
        xtask-viewer ──> xtask-archive
```

`dev/xtask/policy/workspace.toml` の `[visible_to]`（C2 が書く）：

```toml
[visible_to]
xtask-source = ["xtask-core", "xtask-modules", "xtask-workspace", "xtask-style"]
xtask-policy = ["xtask-core", "xtask-style", "xtask-workspace"]
xtask-workspace = ["xtask-core", "xtask-style", "xtask-diagram"]
xtask-modules = ["xtask-core", "xtask-diagram"]
xtask-style = ["xtask-core"]
xtask-layout = ["xtask-diagram"]
xtask-archive = ["xtask-core", "xtask-diagram"]
xtask-diagram = ["xtask-core"]
```

### 4.3 消すもの（領域をまたぐ一覧）

- **Node と elkjs**：
  - `crates/xtask-layout/runtime/`、`vendor/`、`src/prepare.rs`、`src/runtime.rs`、`src/validate.rs`、`tests/elk.rs`
  - 環境変数 `XTASK_NODE`
  - README の Node と elkjs の記述と、第三者の notice
- **HTML**：`crates/xtask-core/src/arch_map.rs`、`src/arch_map/`（`viewer.html`、`viewer.css`、`viewer.js`、`viewer.test.cjs`）
- **SVG の出力**：`crates/xtask-svg`、`crates/xtask-graph`、`xtask-core/src/graphs.rs` の書出しと古い SVG の削除（prune）
- **CLI と設定**：`--no-write`、`--prune`、`Config::no_write`、`Config::prune`、`Config::max_role_chars`、最上位の `output`
- **生成物**：リポジトリの `architecture/`
- **使われていない API**：`Package::active_features`、`Package.features`、`Package.id`、`crate_role`、`crate_role_with_limit`、`DEFAULT_MAX_ROLE_CHARS`、`Workspace::resolved_features`、`Workspace::check_visibility`、`code_line_count` の寛容な fallback
- **自分自身の解析のための特別扱い**：`xtask-modules/src/json.rs`、`intrinsic.rs` の `serde_json::json` と `syn::Token`

---

## 5. 公開の契約

### 5.1 CLI と library（`xtask-core`）

- **CLI**：`xtask check [--root PATH] [--write]`、`xtask --help|-h`。それ以外の option は終了コード 2。
- **library**：

  ```rust
  pub enum Command { Help, Check(Config) }
  pub fn parse_args(root: &Path, arguments: impl IntoIterator<Item = String>) -> Result<Command, String>;
  pub fn main(root: &Path) -> ExitCode;
  pub struct Config { pub root: PathBuf, pub policy: PathBuf, pub output: Output, pub analysis: Option<AnalysisSettings> }
  pub enum Output { None, Policy, Path(PathBuf) }
  pub fn run(config: &Config) -> Result<Report, String>;
  pub fn analyze(config: &Config) -> Result<(Vec<TargetAnalysis>, Report), String>;
  pub struct Report { pub errors: Vec<String>, pub notes: Vec<String>, pub style: Vec<StyleDiagnostic>, pub archive: Option<PathBuf> }
  pub struct TargetAnalysis { pub package: String, pub target: String, pub kinds: Vec<TargetKind>, pub outcomes: Vec<(Mode, TargetOutcome)> }
  pub use xtask_archive::Mode; // Normal | Test。型を二つ持たない
  pub enum TargetOutcome { Analyzed(ModuleGraph), FeatureGated, UnsupportedEdition(String), Failed(String) }
  ```

- **policy**：
  - 最上位の `output` を `[diagrams] output` に移す（既定 `target/xtask/architecture.xtaskmap`）。
  - すべての節は TOML の表でなければならない。
  - 値の検証は付属書 M の M1 の手順 4。

### 5.2 図のファイル（`xtask-archive`）

付属書 A の 1.3〜1.5 節の型・形式・API が契約。

- 形式の版は 1。
- 圧縮は zlib（`miniz_oxide` 0.8、level 6）。
- 同じ入力からは同じバイト列になる。
- `decode` はどんな入力でも panic しない。

### 5.3 配置（`xtask-layout`）

付属書 L の 2.1 節の型と、次の三つの関数が契約。

```rust
pub fn layout(graph: &Graph) -> Result<Layout, LayoutError>;
pub fn verify(graph: &Graph, layout: &Layout) -> Result<(), String>;
pub fn metrics(width: f64, height: f64, routes: &[Vec<(f64, f64)>]) -> Metrics;
```

出力の不変条件（付属書 L の 10.2 節）：
- 重なりが無い。
- 入れ子が保たれる。
- 辺は直交の折れ線で、始点の右辺から出て終点の左辺に入る。
- 辺は端以外の node を通らない。
- 結果は決定的。

### 5.4 図の組立て（`xtask-diagram`）

付属書 A の 2.2 節の `Input`、`plan`、`draw` が契約。

- `plan` は配置をしない。
- `draw` は一つの図を配置し、失敗を `Content::Failed` にする。
- 呼出し側（xtask-core）が `draw` を並列に呼ぶ。

### 5.5 viewer の機能（v1）

| 区分 | 機能 |
|---|---|
| 開く | コマンドラインの引数、Open のダイアログ、drag & drop。F5 で読み直す |
| 自動の読直し | 500 ms ごとにファイルの更新時刻と大きさを調べる。変わったら読み直し、同じ図と視点を保つ。読めなければ前の内容を保って通知する |
| 図の移動 | crate 図から始める。ダブルクリックか Enter で module 図を開く。同じ package の target と構成を選ぶ欄、Back（Backspace）、Fit（0） |
| 強調 | hover で、その要素と依存先・依存元と辺を強調する。クリックか Space で固定し、Esc か背景のクリックで解く |
| 視点 | 背景のドラッグで移動し、wheel で pointer を中心に拡大・縮小する |
| 木と検索 | crate、target・構成、module の木。検索欄で名前を絞り込み、選ぶとその図を開いて要素を中央に固定する |
| 表示 | hover で、名前と Role の tooltip を出す。通知の欄に、生成時の通知、図の失敗・未解析の理由、読直しの失敗を出す |
| 書出し | 表示中の図を SVG に書き出す（Kivact の Offscreen を使い、画面と同じ描画の関数を使う） |

作らないもの（将来も）：ダークテーマ、エディタで開く。

### 5.6 CI

`.github/workflows/viewer-windows.yml`：
- `push` の `main` だけで起動する。
- `windows-latest` で `viewer/` を release で build し、`xtask-viewer.exe` を artifact として残す。
- 公開リポジトリなので無料の枠で動く。

---

## 6. 内部の規則

### 6.1 状態

1. 三つ以上の状態、または排他の事実は、enum で表す。bool や `Option` の組合せで表さない。例：`Output`、`Command`、`TargetOutcome`、`Content`、`Document`、`Focus`、`Camera`、`Features`、`DirOwnership`、`LocalBinding`。
2. 構築の後に埋める field（「未完成の object」）を作らない。段階は、入力を受け取って新しい値を返す関数にする（配置の全段階、xtask-core の段階）。
3. 同じ事実を二度保存しない。例：
   - `kind` の文字列と `Vec<String>` の二重
   - `Module.parent` の文字列と id の接頭辞の二重
   - `errors` と `style_diagnostics` の二重
4. mode の field の save/restore と、番兵の値（`"$external_globs"`、空集合で「隠す」を表すなど）を使わない。
5. 検証は境界で一度だけ行う。検証済みの値は、private な field と読出しの method で守る（`Policy`）。

### 6.2 性能

1. 一つの target のファイルは、構成（Normal と Test）をまたいで一回だけ parse する（M3）。
2. `cargo metadata` は一回だけ実行する（M1）。
3. 図は並列に配置する（C2）。
4. 配置の計算量：
   - network simplex の繰返しには上限を設ける。
   - 交差の削減は最大 12 往復で、改善が 2 回止まったら終える。
   - 不変条件の検査は二分探索で絞る。
5. root の `Cargo.toml` で `xtask-layout` を `opt-level = 3` にし、debug の build でも配置を速くする（C2）。
6. viewer では次を守る。
   - 静的な図と強調の重ね描きを、二つの canvas に分ける。hover で静的な図を描き直さない。
   - 描画の一覧（`Picture`）は、読込みのときに一度だけ作る。

### 6.3 crate・module

1. 1 関数 80 行、1 ファイル 1000 code 行、引数 7 個を上限にする。
2. 全 crate root と module に `Role:` を書く（このリポジトリ自身の規則）。
3. `HashMap` と `HashSet` を、出力の順に影響する場所で使わない。
4. 新しい依存は `miniz_oxide`（xtask-archive）と `kivact`（viewer）だけ。

### 6.4 試験と gate

1. 各 unit は、付属書の「試験」の欄の試験を足す。期待値を緩めない。
2. 時間のかかる試験は、unit の中で実行しない。CP で Orchestrator が実行する。対象は次のとおり。
   - clippy を起動する style の試験
   - 基準の全件の試験
   - viewer の GUI の試験
3. gate は 3 段（unit、wave、CP）。正確なコマンドは作業指示書 2 章。

---

## 7. 監査の判定

監査は 2026-11-18 に、`feat/kashiwade/layout-baseline` の `e7eb600` に対して行った。ID の頭文字は監査の範囲を表す。

| 頭文字 | 範囲 |
|---|---|
| X | xtask-core |
| W | xtask-workspace、xtask-source、README |
| D | xtask-modules |

| ID | 所見 | 判定 | unit |
|---|---|---|---|
| X-B1 | 一つの壊れたファイルで、構文の style の診断がすべて失われる | 対応 | C1 |
| X-B2 | worker の panic で、全 target の解析と理由が失われる | 対応 | C2 |
| X-B3 | Windows で、消してから rename するので原子的でない | 対応（NamedTempFile::persist） | C2 |
| X-B4 | verbatim の root に相対 path を join する | 対応 | M2、C1 |
| X-B5 | crate の列の閉路の報告に、閉路の外の crate が混ざる | 対応 | A2 |
| X-B6 | required-features の `dep/feat` が常に無効と判定される | **後続**（依存の feature の情報が要る。12 章） | — |
| X-B7 | 正規化の前に sort・dedup している | 対応 | C1 |
| X-B8 | `--root` に `-` で始まる値を渡せない | 対応 | C2 |
| X-C1 | `--prune` が何もしない | 対応（削除） | C2 |
| X-C2 | README と、図を残す条件が違う | 消滅（prune の削除） | — |
| X-C3 | policy の扱いが `run` と `analyze` で違う | 対応 | C2 |
| X-C4 | 出力先の既定が二か所にある | 対応 | M1 |
| X-C5 | policy の検証が三回ある | 対応 | M1、C1 |
| X-C6 | ファイルの一覧を二度、別の規則で作る | 対応 | C2、C1 |
| X-C7 | help の判定が二か所にある | 対応 | C2 |
| X-C8 | doc に Node・SVG・HTML の記述がある | 対応 | C2、C3 |
| X-S1 | 共有の `Report` を段階ごとに書き換える | 対応 | C2 |
| X-S2 | target の結果が三つの集まりに散る | 対応（`TargetOutcome`） | C2 |
| X-S3 | `kind` の文字列と `test: bool` | 対応 | M1、C2 |
| X-S4 | style の診断を二重に持つ | 対応 | C1、C2 |
| X-S5 | `host_cfg` を後から埋める | 対応 | C2 |
| X-S6 | `no_write`・`prune` の flag | 対応（`Output`） | C2 |
| X-S7 | `parse_args` の bool の組 | 対応（`Command`） | C2 |
| X-I1 | `cargo metadata` を何度も実行する | 対応 | M1、C1 |
| X-I2 | ファイルを何度も読み・parse する | 一部対応（解析は一回。style は proc-macro2 の型が `!Send` のため別に parse する） | M3、C2 |
| X-I3 | 診断のあるファイルごとに canonicalize の loop を回す | 対応 | C1 |
| X-I4 | directory ごとに rustfmt を起動する | 対応 | C1 |
| X-I5 | 外部の process を直列に起動する | 対応 | C1、C2 |
| X-I6 | 図を直列に配置する | 対応 | C2 |
| X-I7 | 書いた SVG を読み戻す | 消滅 | — |
| X-I8 | Workspace を誤りの経路のためだけに clone する | 対応 | C2 |
| W-B1 | `[limits]` と `[diagrams]` が配列を受け付ける | 対応 | M1 |
| W-B2 | `max_code_lines = 0` を受け付ける | 対応 | M1 |
| W-B3 | 空の文字列と重複を検証しない | 対応 | M1 |
| W-B4 | style の検証が `enabled` に依存する | 対応 | M1 |
| W-B5 | path の形が混在する | 対応 | M1、M2 |
| W-B6 | root を二か所で決める | 対応（一致しなければ誤り） | C2 |
| W-B7 | `active_features` の誤り | 消滅（削除） | M1 |
| W-B8 | `rust_files` の取りこぼしと入れ子の package | 対応 | M2、C2 |
| W-B9 | 空白の判定と span の記録の保持 | 対応 | M2 |
| W-B10 | 例外の `path = "*"` を受け付ける | 対応 | M1 |
| W-C* | library の種類の一覧が四か所、`ALL[..13]`、既定値の重複 | 対応 | M1、M3、C2 |
| W-C* | `trusted_globs` と `allowed_globs` の二重の宣言 | README に明記（意味が違うので二つ残す。DEC-9） | C3 |
| W-C* | README と code のずれ | 対応 | C3 |
| W-S* | 公開 field の Policy、StylePolicy の再計算、features の組合せ、文字列の型、`Option` の resolve | 対応 | M1 |
| W-I* | metadata・JSON の二重、行の数え方の割当て | 対応 | M1、M2 |
| W-I* | `visible_to` の照合が P×M | 対応しない（規模が小さい） | — |
| D-B1 | 局所の thread_local が glob を隠さない | 対応 | M3 |
| D-B2 | 外部 glob の判定が三種類ある | 対応 | M3 |
| D-B3 | `"$external_globs"` の上書きと漏れ | 対応 | M3 |
| D-B4 | glob が最初の解決先しか使わない | 対応 | M3 |
| D-B5 | `cfg(true)` を偽と評価する | 対応 | M3 |
| D-B6 | verbatim の path が漏れる | 対応 | M3、M2 |
| D-B7 | 誤りに位置が無い | 対応 | M3 |
| D-B8 | inline module の attribute を二度診断する | 対応 | M3 |
| D-B9 | 文法の取りこぼし | 一部対応（表への追加、暗黙の取込み）。pattern と guard は**後続** | M3 |
| D-B10 | 可視性を模型化していない | **後続** | — |
| D-B11 | `thread_local!` の中の参照が落ちる | 対応 | M3 |
| D-C1 | cfg の誤りの扱いが二通り | 対応 | M3 |
| D-C2 | glob が与えうる attribute を検査しない | 対応 | M3 |
| D-C3 | 第三者の crate を組込みとして扱う | 対応（削除し、policy で信頼する） | M3 |
| D-C4 | Role の既定値と検証の重複 | 対応 | M1、M3 |
| D-C5 | Role の位置を強制していない | 対応 | M3 |
| D-C6 | Role の二重の検査 | 対応 | M3 |
| D-S1〜S8 | bool の組の段階、`strong`、namespace の mode、番兵の束縛、loader の mode、Role の Option、parent の二重、無効の再導出 | 対応 | M3 |
| D-I1〜I10 | 多重の parse、表の clone、scope の materialize、収束の clone、二重の処理、部分木の clone、thread_local の再計算、cfg の再評価、文字列の id、options の clone | 対応 | M3、C2 |
| D-D1〜D6 | json・Token の特別扱い、未使用の API、dead code、helper の重複、試験の一時 directory | 対応 | M3 |

配置の経路（Node、ELK、SVG、HTML）の所見は、経路ごと消すので個別に挙げない。設計者が自分で読んで見つけた点と、その扱いは次のとおり。

| 所見 | 扱い |
|---|---|
| `Direction::TopToBottom` は使われていないうえ、矢印の描き方が上下向きに対応していない | 向きを左から右だけにする |
| `EdgeStyle::Dev` の doc が「正弦波の線」と書いているが、実際は点線 | doc を消す |
| 検証が `xtask-graph`、`xtask-layout`、`xtask-svg` の三か所に重複している | `check.rs` に一本化する |
| `xtask-svg` の交差の判定が、二次曲線の一般の交点を解いている | 直交の線分の上だけで判定する |
| `prepare.rs` が同じ列の node の幅を求めるとき、`rank.contains` で O(n²) になる | 列の表を使う |

---

## 8. 設計者の決定（作者が覆してよい）

| ID | 決定 | 理由 |
|---|---|---|
| DEC-1 | `check` は既定で書かない。`--write` で図のファイルを書く | 作者の指示。Kivact の `cargo xtask diagrams` と同じ考え方 |
| DEC-2 | 既定の出力先は `target/xtask/architecture.xtaskmap`。拡張子は `.xtaskmap` | 生成物なので `target/` に置けば、どのリポジトリでも git の対象外になる |
| DEC-3 | 圧縮は `miniz_oxide` 0.8 の zlib（level 6） | 純 Rust、unsafe 無し、Kivact の lock と cargo の cache にすでにある。zlib の adler32 で破損を検出できる |
| DEC-4 | 座標は整数（px）にする | 決定性と network simplex の都合 |
| DEC-5 | 階層は下から region ごとに配置し、境界の port で辺を区切る | 平らな配置の関数を一つにでき、段ごとに試験できる。ELK の INCLUDE_CHILDREN との違い（port の順は内側で決まる）は、交差数の KPI で監視する |
| DEC-6 | 層の割当てと縦の座標に、同じ network simplex を使う | 最も丁寧に試験する部品を一つにする |
| DEC-7 | crate 図の列の中の順は、名前の順ではなく交差の削減で決める（初めの並びは名前の順） | ELK の時は列を一つの node にする工夫で名前の順に固定していた。自前の実装では交差を減らすほうが読みやすい |
| DEC-8 | 逆向きの辺と同じ列の辺は、U ターンのダミーで回り込ませる。辺は必ず右から出て左に入る | 内部の構造が「隣の層の間の線分の列」だけになり、通路の配線を一つの規則で書ける |
| DEC-9 | `trusted_globs`（解析）と `allowed_globs`（style）は二つ残す | 意味が違う（前者は「macro を与えない」、後者は「glob の import を許す」） |
| DEC-10 | `json!` と `Token!` の特別扱いを止め、このリポジトリの policy の `trusted_macros` に書く | 自分自身の解析のための例外を、道具の中に持たない |
| DEC-11 | viewer は Kivact の `main`（`2b8df5ac`）に固定する | Kivact は改修中で、改修の branch は公開の API を変える |
| DEC-12 | viewer は font を同梱しない。`FONT_FAMILY` の system font を使う | 字幅の見積り（ASCII 0.62em、それ以外 1.05em）が実際の等幅 font より広いので、はみ出さない |
| DEC-13 | SVG の書出しは Kivact の `Offscreen::render_svg` に、画面と同じ描画の関数を渡して作る | 画面と書出しの見た目を一つの実装で保つ |
| DEC-14 | 強調は、静的な図を暗くせず、強調する要素を色の付いた線で重ね描きする | hover で静的な図を描き直さないため（6.2 節） |
| DEC-15 | module 図の当たり判定は region（module の枠）に、crate 図は node に付ける | 一つの item に当たり判定を一つにする |

---

## 9. 作業の段階

| wave | 内容 | lane |
|---|---|---|
| W1 | 土台：配置の型と最適化の部品、図のファイル、policy と workspace、source | L1、A1、M1、M2 |
| W2 | 配置の各段階、module の解析、viewer の骨組みと描画の計算 | L2、L3、L4、L5、M3、V1 |
| W3 | 配置の検証と階層、style、viewer の画面 | L6、L7、C1、V2a、V2b、V2c |
| W4 | 図の組立て、基準の比較、viewer の試験、README | A2、L8、V3、C3 |
| W5 | xtask-core の組み直しと旧い経路の削除 | C2 |
| CP | 全体の試験、KPI、受入れ条件の点検 | Orchestrator |

W1〜W4 の間は、`xtask-core`（と、消す予定の `xtask-graph`・`xtask-svg`）が compile できない。W5 で直す。各 wave の gate は、その wave までに完成した crate だけを対象にする（作業指示書 2 章）。

---

## 10. 付属書の一覧

0 章の表のとおり。

## 11. 利用側への影響（Kivact など）

xtask-core を取り込むリポジトリは、submodule を更新するときに次を直す。

| 変わるもの | 直し方 |
|---|---|
| `config.no_write = true/false` | 削除。書くときは `config.output = Output::Policy` |
| `TargetGraph` | `TargetAnalysis` と `TargetOutcome::Analyzed(graph)` |
| `report.errors` | style の違反を含まなくなった。`report.style` も見る |
| policy の最上位の `output` | `[diagrams] output` |
| `json!`・`Token!` を使うリポジトリ | `trusted_macros` に書く |
| Role の位置 | 最初の inner doc の最初の行に置く |
| root の `Cargo.toml` | `[profile.dev.package.xtask-layout] opt-level = 3` を足す |

Kivact の `dev/xtask` の修正は、本改修の範囲外にする（Kivact 側の作業）。

## 12. 後続（本改修では行わない）

- X-B6：required-features の `dep/feat` の判定。
- D-B9 の残り：定数を名指す pattern、guard の `if let` の束縛。
- D-B10：可視性の模型化。
- viewer の macOS 対応（Kivact が未対応）。
