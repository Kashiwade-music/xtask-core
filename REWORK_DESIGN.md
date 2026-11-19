# xtask-core 改修設計書：Node.js の除去、図のファイル、viewer

版 1.1（2026-11-19）。1.0 に、批判的レビュー（`20261118_2331_CODEX_REVIEW.md`）への回答を入れた。各指摘の扱いは [rework/R-review-1.md](rework/R-review-1.md) に一行ずつ書く。

進め方は [REWORK_WORK_ORDER.md](REWORK_WORK_ORDER.md)（以下「作業指示書」）、unit ごとの手順は付属書 `rework/*.md`（以下「付属書」）に書く。

## 0. 文書の構成と優先順位

| 文書 | 持つもの |
|---|---|
| 本書 | 目的、受入れ条件、構成、公開の契約、内部の規則、監査の判定、設計者の決定 |
| 付属書 | unit ごとの手順と試験 |
| 作業指示書 | wave・lane・gate・STOP の規則と、unit ごとの追加の指示 |

**優先順位（全文書で同じ）：** 本書 5 章（契約）＞ 作業指示書 4 章の「追加の指示」（unit ごとの例外）＞ 付属書 ＞ 作業指示書の共通の規則。

- 上位と下位が食い違うときは上位に従う。
- ただし、付属書または追加の指示が本書 5 章と食い違うときは、従わずに STOP する（作業指示書 0.4 節）。

| 付属書 | 範囲 |
|---|---|
| [rework/L-layout.md](rework/L-layout.md) | 配置エンジン `xtask-layout` |
| [rework/A-archive.md](rework/A-archive.md) | 図のファイル `xtask-archive` と、図の組立て `xtask-diagram` |
| [rework/M-model.md](rework/M-model.md) | `xtask-policy`（新設）、`xtask-workspace`、`xtask-source`、`xtask-modules` |
| [rework/C-core.md](rework/C-core.md) | `xtask-style`（新設）、`xtask-core`、README |
| [rework/V-viewer.md](rework/V-viewer.md) | viewer（`viewer/`）と GitHub Actions |
| [rework/R-review-1.md](rework/R-review-1.md) | レビュー 1 の各指摘への回答（台帳） |

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
| AC-1 | リポジトリに `.js`・`.cjs`・`.html`・`.css` が無く、`node` を解決できない `PATH` で `cargo xtask check --write` が成功する | CP の手順（作業指示書 2.3 節） |
| AC-2 | `cargo xtask check` は何も書かない。`--write` は `[diagrams] output` に一つのファイルを書き、`xtask_archive::decode` で読める | `xtask-core` の統合試験 |
| AC-3 | `dev/layout-baseline` の全 146 件の入力で、配置が成功し、出力の不変条件を満たし、決定的である | `xtask-layout` の `tests/baseline.rs` |
| AC-4 | viewer が 5.5 節の機能を持ち、main への push で GitHub Actions が Windows 向けの実行ファイルを成果物として残す | V2 の単体試験、V3 の GUI 試験、workflow の定義 |
| AC-5 | 7 章の判定で「対応」とした所見と、R-review-1 で「採用」とした指摘が、すべて「確かめ方」の証拠で閉じている | CP の点検 |
| AC-6 | `cargo test --workspace`（root）と `cargo test`（viewer）が成功する。root の workspace が Rust 1.85 で compile できる（`cargo +1.85.0 check --workspace --all-targets --locked`）。xtask 自身の検査（`cargo xtask check`）を、このリポジトリと viewer の両方に対して実行し、違反が 0 件 | CP |
| AC-7 | `README.md` と `README_JP.md` が同じ構成で、新しい挙動を説明する | CP の点検 |

---

## 2. KPI

KPI は自動の gate ではないが、**完了の条件**である。目標に届かなければ設計者への差戻しにし、差戻しが残る間は完了を宣言しない（作業指示書 0.5 節）。実装者は KPI のために設計を変えない。

最初の測定は W3 の tier b（配置エンジンが揃った直後）で行い、未達なら残りの wave と並行して設計者が対処する。最終の測定は CP で行う。

| ID | 指標 | 目標 | 測り方 |
|---|---|---|---|
| K1 | 交差数の比 | 1.3 以下 | `baseline_report` の合計の行。全 146 件の、本実装の交差数の和 ÷ ELK の交差数の和。ELK の和が 0 なら本実装も 0 |
| K2 | 折れの数の比 | 1.3 以下 | K1 と同じく、折れの数の和の比 |
| K3 | 面積の比 | 1.5 以下 | K1 と同じく、`width × height` の和の比 |
| K4 | 配置の時間 | 3.0 秒以下 | `kivact` の 121 件について、release、1 thread で、一度ずつ配置した時間の和。測る前に全件を一度配置する（暖機） |
| K5 | 図のファイルの大きさ | 102 400 バイト以下 | このリポジトリで `check --write` が書いたファイルのバイト数 |
| K6 | 検査の時間 | 改修前の 1.05 倍以下 | 作業指示書 2.4 節の手順。build 済みの `cargo run -p xtask-core -- check` を、暖機の 1 回の後に 3 回測った中央値。改修前は W0 に `--no-write` で同じ手順で測る |

---

## 3. 利用者の経路

### 3.1 検査（変わらない部分）

`cargo xtask check` は、行数・可視性・Role・循環・style を検査し、違反を報告する。終了コードは、成功が 0、違反または実行の誤りが 1、引数の誤りが 2。**何も書かない。**

### 3.2 図を作る

`cargo xtask check --write` は、検査に加えて図を配置し、`[diagrams] output`（既定 `target/xtask/architecture.xtaskmap`）に一つのファイルを書く。

- 検査の違反があっても、作れる図は書く。
- 解析に失敗した target の図は、失敗の理由として記録する。
- ファイルを書けなかったときは、その誤りを他の違反と一緒に報告する（他の診断を消さない）。

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
| `xtask-workspace` | Cargo metadata（構造と feature） | 整理（M1） |
| `xtask-modules` | module の解析 | 整理（M3a〜M3c） |
| `xtask-style` | Google style の検査 | **新設**（C1、xtask-core から分離） |
| `xtask-layout` | 配置エンジン（Rust のみ） | **書き直し**（L1〜L8） |
| `xtask-archive` | 図のファイルの型・検証・符号化・圧縮・文字の規則 | **新設**（A1） |
| `xtask-diagram` | 解析結果から図を組み立てて配置する | **新設**（A2、xtask-core の graphs.rs と xtask-graph から） |
| `xtask-core` | CLI と全体の制御、行数・可視性の検査 | 整理（C2a〜C2c） |
| `xtask-graph` | — | **削除** |
| `xtask-svg` | — | **削除**（描画の規則は viewer の `xtask-render` へ） |

**viewer の workspace（`viewer/`、Rust 1.98.1、Kivact）：**

| crate | 役割 |
|---|---|
| `xtask-render` | `Scene` から描画の一覧（角の丸め、交差の跳び、矢印、文字の位置）を作る。Kivact に依存しない |
| `xtask-viewer` | Kivact の GUI。描画は `xtask-render` の結果を Kivact の `Draw` に写すだけ。状態の遷移は純粋な関数に置く |

分割の理由（境界）：

| crate | 境界 |
|---|---|
| `xtask-policy` | 純粋な TOML の検証。process を起動しない |
| `xtask-style` | 入力が file の一覧と policy で、出力が診断と通知。外部の道具（rustfmt、clippy）を起動する |
| `xtask-archive` | 二つの workspace と二つのツールチェーンから使う、形式と検証の唯一の定義 |
| `xtask-diagram` | 入力が解析の結果で、出力が `Diagram` の列 |
| `xtask-render` | GUI を起動せずに試験できる幾何の計算 |

これ以上の crate は作らない。共通の幾何の検証は、形式の正本である `xtask-archive` に置く（新しい crate を作らない）。

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

`dev/xtask/policy/workspace.toml` の `[visible_to]`（C2c が書く）：

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
  - `dev/layout-baseline/capture/tee.cjs`、`node-tee.cmd`
- **HTML**：`crates/xtask-core/src/arch_map.rs`、`src/arch_map/`（`viewer.html`、`viewer.css`、`viewer.js`、`viewer.test.cjs`）
- **SVG の出力**：`crates/xtask-svg`、`crates/xtask-graph`、`xtask-core/src/graphs.rs` の書出しと古い SVG の削除（prune）
- **CLI と設定**：`--no-write`、`--prune`、`Config::no_write`、`Config::prune`、`Config::max_role_chars`、最上位の `output`
- **生成物**：リポジトリの `architecture/`
- **使われていない API**：`Package::active_features`、`Package.features`、`crate_role`、`crate_role_with_limit`、`DEFAULT_MAX_ROLE_CHARS`、`Workspace::resolved_features`、`code_line_count` の寛容な fallback
  - `Package.id` は消さない（clippy の診断を package に対応付けるのに使う）。
- **移すもの**：`Workspace::check_visibility` は xtask-core の `visibility` に移す（C2b）。
- **自分自身の解析のための特別扱い**：`xtask-modules/src/json.rs`、`intrinsic.rs` の `serde_json::json` と `syn::Token`

旧い実装を参照する手順は、すべて `git show e7eb600:<path>` で読む（作業指示書 0.2 節の 14）。作業ツリーのファイルは、どの wave で消えるか分からないので頼らない。

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

- `run` が `Err` を返すのは、policy と Cargo metadata を得られないときだけ。それ以後の失敗（解析の環境、解析、配置、ファイルの書出し）は `Report.errors` に入れ、他の診断を消さない。
- **policy**：
  - 最上位の `output` を `[diagrams] output` に移す（既定 `target/xtask/architecture.xtaskmap`）。
  - すべての節は TOML の表でなければならない。
  - 値の検証は付属書 M の M1 の手順 4。

### 5.2 図のファイル（`xtask-archive`）

付属書 A の 1.3〜1.6 節の型・形式・検証・API が契約。

- 形式の版は 1。
- 圧縮は zlib（`miniz_oxide` 0.8、level 6）。本体（圧縮前）の上限は 64 MiB。
- `encode` と `decode` は同じ検証（付属書 A 1.5 節の `validate`）を行う。`encode` は検証に通らない値を `Err` にし、`decode` は検証に通った値だけを返す。描画側（`xtask-render`）は検証済みであることを前提にしてよい。
- 同じ入力からは同じバイト列になる。
- `decode` はどんな入力でも panic せず、確保する memory は本体の長さに比例する上限を超えない。

### 5.3 配置（`xtask-layout`）

付属書 L の 2.1 節の型と、次の三つの関数が契約。

```rust
pub fn layout(graph: &Graph) -> Result<Layout, LayoutError>;
pub fn verify(graph: &Graph, layout: &Layout) -> Result<(), String>;
pub fn metrics(width: f64, height: f64, routes: &[Vec<(f64, f64)>]) -> Metrics;
```

- 座標・大きさは `i64`（px）。図のファイル（`i32`）への変換は `xtask-diagram` が行い、範囲を超えれば、その図を `Content::Failed` にする。
- 要素の数と大きさの上限（付属書 L 10.1 節）を超える要求は `Err`。上限の中では、整数の演算は溢れない。
- node も region も無い要求は、余白だけの図（`2 * DIAGRAM_PAD` の正方形）になる。

出力の不変条件（付属書 L の 10.2 節）：
- 重なりが無い。
- 入れ子が保たれる。region の見出しの場所に、子が入らない。
- node と region とラベルは、要求した大きさ以上を持つ（ラベルはちょうど要求の大きさ）。
- 辺は直交の折れ線で、始点の右辺から出て終点の左辺に入る。
- 辺は端以外の node を通らない。
- 結果は決定的。

### 5.4 図の組立て（`xtask-diagram`）

付属書 A の 2.2 節の `Input`、`plan`、`draw`、`failed` が契約。

- `plan` は配置をしない。図ごとの `Plan` の列を返す。
- `draw` は一つの図を配置し、失敗を `Content::Failed` にする。
- 呼出し側（xtask-core）が `draw` を並列に呼び、`Archive` を組み立てる。

### 5.5 viewer の機能（v1）

| 区分 | 機能 |
|---|---|
| 開く | コマンドラインの引数、Open のダイアログ、drag & drop。開くと crate 図・空の履歴・Fit から始める。F5 で今のファイルを読み直す |
| 自動の読直し | 500 ms ごとにファイルの更新時刻と大きさを調べる。最後に読もうとした時の値と違えば読み直す。読み直しでは、同じ図（`DiagramKind` で探す）と視点を保ち、固定の強調は解く。読めなければ前の内容を保って通知し、同じ内容を繰り返し読まない。ファイルの情報を 2 回続けて取れなければ通知する |
| 図の移動 | ダブルクリックか Enter で module 図を開く。同じ package の target と構成を選ぶ欄、Back（Backspace）、Fit（0）。履歴は `DiagramKind` で持ち、読み直しで消えた図は履歴から除く |
| 強調 | hover で、その要素と依存先・依存元と辺を強調する。クリック（押して離す、動きが 3 px 未満）か Space で固定し、Esc か背景のクリックで解く。固定の間も hover の tooltip は出る |
| 視点 | ドラッグ（要素の上からでもよい。3 px 以上動いたらドラッグ）で移動し、wheel で pointer を中心に拡大・縮小する |
| 木と検索 | crate、target・構成、module の木。crate 図が失敗しても module 図の package から木を作る。検索欄で名前を絞り込む。項目を Enter かダブルクリックで開き、module の項目はその要素を中央に固定する。単クリックは選択だけ |
| 表示 | hover で、名前と Role の tooltip を出す。通知の欄に、生成時の通知、図の失敗・未解析の理由、読直しの失敗を出す |
| 書出し | 表示中の図を SVG に書き出す（Kivact の Offscreen を UI の外の thread で作り、画面と同じ描画の関数を使う） |

作らないもの（将来も）：ダークテーマ、エディタで開く。

### 5.6 CI

`.github/workflows/viewer-windows.yml`：
- `push` の `main` だけで起動する。
- `windows-latest` で `viewer/` を release で build し、`xtask-viewer.exe` を artifact として残す。
- 公開リポジトリなので無料の枠で動く。

---

## 6. 内部の規則

### 6.1 状態

1. 三つ以上の状態、または排他の事実は、enum で表す。bool や `Option` の組合せで表さない。例：`Output`、`Command`、`TargetOutcome`、`Content`、`Document`、`Request`、`Gesture`、`Camera`、`Features`、`DirOwnership`、`LocalBinding`。
   - 同時に成り立ちうる独立の事実は、無理に一つの enum にまとめない（例：固定の強調 `pinned` と hover は別の field）。
2. 構築の後に埋める field（「未完成の object」）を作らない。段階は、入力を受け取って新しい値を返す関数にする（配置の全段階、xtask-core の段階）。
3. **独立に変わりうる**同じ事実を二度保存しない。例：
   - `kind` の文字列と `Vec<String>` の二重
   - `Module.parent` の文字列と id の接頭辞の二重
   - `errors` と `style_diagnostics` の二重
   - 呼出し側が長さと順をそろえる並行の配列（`targets` と `selected`）

   許すもの：一度作ったら変わらない値（snapshot）の中の、派生の索引・cache。作る関数を一つにし、snapshot と一緒に作り直す（例：`Loaded` の `pictures` と `kinds`、`Chains.layer_count`）。
4. mode の field の save/restore と、番兵の値（`"$external_globs"`、`usize::MAX` で「無し」、空集合で「隠す」、使わない欄の 0 など）を使わない。
5. 検証は境界で一度だけ行う。検証済みの値は、private な field と読出しの method で守るか（`Policy`）、検証の関数を一つにして入口で必ず通す（`xtask_archive::validate`）。

### 6.2 性能

1. 一つの target のファイルは、構成（Normal と Test）をまたいで一回だけ parse する（M3a）。parse の失敗も記憶し、読み直さない。
2. `cargo metadata` は、構造のための `--all-features` の一回と、選んだ構成の feature のための一回（`Features::All` で target が無ければ省く）の、最大二回にする（M1）。
3. 解析と図の配置は並列に行う。並列化は「入力ごとに一つの thread、同時の数は CPU の数まで」の一つの関数で行う（C2b）。
4. 配置の計算量：
   - network simplex の繰返しには上限を設ける。上限で止まった解は実行可能だが最適とは限らない。
   - 交差の削減は最大 12 往復で、改善が 2 回止まったら終える。
   - 不変条件の検査は全組の比較でよい（基準の最大は node 199、辺 679。K4 で監視する）。
5. root の `Cargo.toml` で `xtask-layout` を `opt-level = 3` にし、debug の build でも配置を速くする（C2c）。
6. style の検査は、各ファイルを一回だけ parse し、診断と item の索引を同時に作る。AST を全部持ち続けない（C1）。
7. viewer では次を守る。
   - 静的な図と強調の重ね描きを、二つの canvas に分ける。hover で静的な図を描き直さない。
   - 描画の一覧（`Picture`）は、読込みの thread で一度だけ作る。交差の探索は、垂直の線分を x で並べた二分探索で絞る。
   - SVG の書出し（Offscreen の描画と書込み）は UI の thread の外で行う。

### 6.3 crate・module

1. 1 関数 80 行、1 ファイル 1000 code 行、引数 7 個を上限にする。
2. 全 crate root と module に `Role:` を書く（このリポジトリ自身の規則）。`Role:` は最初の inner doc の最初の行に置く。
3. `HashMap` と `HashSet` を、出力の順に影響する場所で使わない。
4. 新しい依存は `miniz_oxide`（xtask-archive）と `kivact`（viewer）だけ。
5. root の workspace は Rust 1.85 で compile できること（`rust-version`）。

### 6.4 試験と gate

1. 各 unit は、付属書の「試験」の欄の試験を足す。期待値を緩めない。
2. 既存の試験の期待値は変えない。**例外は、付属書の「既存の試験の移行」の表に挙げたものだけ**で、表のとおりに変える。表に無い試験が通らなければ STOP する。
3. 時間のかかる試験は、unit の中で実行しない。実行する者と回数は次のとおり。

   | 試験 | 実行する者と時点 |
   |---|---|
   | clippy を起動する style の試験（`xtask-style/tests/tools.rs`） | Orchestrator、CP で 1 回 |
   | 基準の全件の試験と比較表 | Orchestrator、W3 の tier b と CP で各 1 回 |
   | viewer の GUI の試験（`tests/viewer.rs`） | V3 の実装者が unit の中で 1 回、Orchestrator が CP で 1 回 |

4. viewer の振舞いは、できるだけ純粋な関数（遷移、座標、木、読込みの要求の制御）に置き、単体試験で確かめる。GUI の試験は、結線と、実際の大きさ・gesture が成り立つことだけを確かめる。
5. gate は 3 段（unit、wave、CP）。正確なコマンドは作業指示書 2 章。

---

## 7. 監査の判定

監査は 2026-11-18 に、`feat/kashiwade/layout-baseline` の `e7eb600` に対して行った。ID の頭文字は監査の範囲を表す。まとめて書いていた ID（`W-C*` など）は、本版で一つずつに分け、番号を振った。

| 頭文字 | 範囲 |
|---|---|
| X | xtask-core |
| W | xtask-workspace、xtask-source、README |
| D | xtask-modules |

「確かめ方」は CP で AC-5 を点検するときの証拠。試験名は付属書が定めるもの。`rg` は、そのパターンが root の `crates/` に 0 件であることを確かめる。

| ID | 所見 | 判定 | unit | 確かめ方 |
|---|---|---|---|---|
| X-B1 | 一つの壊れたファイルで、構文の style の診断がすべて失われる | 対応 | C1 | `unparsable_file_does_not_hide_others` |
| X-B2 | worker の panic で、全 target の解析と理由が失われる | 対応 | C2b | `parallel::tests` の 4 件 |
| X-B3 | Windows で、消してから rename するので原子的でない | 対応（NamedTempFile::persist） | C2b | `rg "remove_file" crates/xtask-core/src/run.rs` |
| X-B4 | verbatim の root に相対 path を join する | 対応 | M2、C1 | `normalize_strips_verbatim` |
| X-B5 | crate の列の閉路の報告に、閉路の外の crate が混ざる | 対応（SCC） | A2 | `cyclic_ranks_report_only_cycle_members` |
| X-B6 | required-features の `dep/feat` が常に無効と判定される | **後続**（依存の feature の情報が要る。12 章） | — | — |
| X-B7 | 正規化の前に sort・dedup している | 対応 | C1 | C1 の手順 5 のコードを目で確かめる |
| X-B8 | `--root` に `-` で始まる値を渡せない | 対応 | C2a | `root_value_may_start_with_dash` |
| X-C1 | `--prune` が何もしない | 対応（削除） | C2a | `old_flags_are_rejected` |
| X-C2 | README と、図を残す条件が違う | 消滅（prune の削除） | — | — |
| X-C3 | policy の扱いが `run` と `analyze` で違う | 対応 | C2b | `missing_policy_uses_default_in_run_and_analyze` |
| X-C4 | 出力先の既定が二か所にある | 対応 | M1 | `rg "architecture.xtaskmap"` が xtask-policy だけ |
| X-C5 | policy の検証が三回ある | 対応 | M1、C1 | `rg "fn validate" crates/xtask-style crates/xtask-core` が 0 件 |
| X-C6 | ファイルの一覧を二度、別の規則で作る | 対応 | C2b、C1 | `rg "rust_files" crates/xtask-style` が 0 件 |
| X-C7 | help の判定が二か所にある | 対応 | C2a | `rg "\"--help\"" crates/xtask-core/src` が `cli.rs` だけ |
| X-C8 | doc に Node・SVG・HTML の記述がある | 対応 | C2a、C3 | `rg -i "node\.js\|elk\|svg\|html" crates/xtask-core/src README*.md` を目で確かめる |
| X-S1 | 共有の `Report` を段階ごとに書き換える | 対応 | C2b | `rg "&mut Report" crates/xtask-core/src` が 0 件 |
| X-S2 | target の結果が三つの集まりに散る | 対応（`TargetOutcome`） | C2b | `rg "SkippedTarget\|TargetGraph"` が 0 件 |
| X-S3 | `kind` の文字列と `test: bool` | 対応 | M1、C2b | `TargetKind` と `Mode` の型 |
| X-S4 | style の診断を二重に持つ | 対応 | C1、C2b | `rg "style_diagnostics\|ToolReport"` が 0 件 |
| X-S5 | `host_cfg` を後から埋める | 対応 | C2b | `rg "host_cfg = "` が 0 件 |
| X-S6 | `no_write`・`prune` の flag | 対応（`Output`） | C2a | `rg "no_write\|prune"` が 0 件 |
| X-S7 | `parse_args` の bool の組 | 対応（`Command`） | C2a | `Command` の型 |
| X-I1 | `cargo metadata` を何度も実行する | 対応（最大 2 回。style の probe でも再実行しない） | M1、C1 | `selection_run_only_when_needed`、`rg "\"metadata\"" crates/xtask-style` が 0 件 |
| X-I2 | ファイルを何度も読み・parse する | 一部対応（解析は構成をまたいで 1 回、style は 1 回。解析と style の間は、proc-macro2 の span が thread ごとのため別に parse する） | M3a、C1 | `parse_cache_keeps_failures`、`item_index_names_line_diagnostics`、`rg "fn source_items" crates/xtask-style` が 0 件 |
| X-I3 | 診断のあるファイルごとに canonicalize の loop を回す | 対応 | C1 | `rg "canonicalize" crates/xtask-style` が 0 件 |
| X-I4 | directory ごとに rustfmt を起動する | 対応 | C1 | C1 の手順 8 のコードを目で確かめる |
| X-I5 | 外部の process を直列に起動する | 一部対応（fmt と clippy の群を並列にする。probe の一括化は後続） | C1 | C1 の手順 9 のコードを目で確かめる |
| X-I6 | 図を直列に配置する | 対応 | C2b | `parallel::map` を使う |
| X-I7 | 書いた SVG を読み戻す | 消滅 | — | — |
| X-I8 | Workspace を誤りの経路のためだけに clone する | 対応 | C2b | `rg "workspace.clone()"` が 0 件 |
| W-B1 | `[limits]` と `[diagrams]` が配列を受け付ける | 対応 | M1 | `limits_must_be_a_table` |
| W-B2 | `max_code_lines = 0` を受け付ける | 対応 | M1 | `zero_code_lines_rejected` |
| W-B3 | 空の文字列と重複を検証しない | 対応 | M1 | `empty_feature_rejected`、`duplicate_visible_to_rejected` |
| W-B4 | style の検証が `enabled` に依存する | 対応 | M1 | `style_checks_ignore_enabled` |
| W-B5 | path の形が混在する | 対応 | M1、M2 | `rg "canonicalize" crates/xtask-workspace` が 0 件 |
| W-B6 | root を二か所で決める | 対応（一致しなければ誤り） | C2b | `root_must_be_the_workspace_root` |
| W-B7 | `active_features` の誤り | 消滅（削除） | M1 | `rg "active_features"` が 0 件 |
| W-B8 | `rust_files` の取りこぼしと入れ子の package | 対応 | M2、C2b | `nested_package_skipped`、`custom_target_roots_are_counted` |
| W-B9 | 空白の判定と span の記録の保持 | 対応（span は job の thread の終わりで解放される） | M2、C2b | `nbsp_is_not_whitespace` |
| W-B10 | 例外の `path = "*"` を受け付ける | 対応 | M1 | `bare_wildcard_exception_rejected` |
| W-C1 | library の種類の一覧が四か所にある | 対応 | M1 | `rg "\"proc-macro\"" crates` が `TargetKind` の変換だけ |
| W-C2 | `ALL[..13]` | 対応 | M1 | `rg "\.\.13\]"` が 0 件 |
| W-C3 | 既定値の重複 | 対応 | M1 | `rg "80" crates/xtask-modules/src` に Role の既定値が無い |
| W-C4 | `trusted_globs` と `allowed_globs` の二重の宣言 | README に明記（意味が違うので二つ残す。DEC-9） | C3 | README の該当の節 |
| W-C5 | README と code のずれ | 対応 | C3 | AC-7 の点検 |
| W-S1 | 公開 field の Policy | 対応 | M1 | `Policy` の field が private |
| W-S2 | StylePolicy の再計算 | 対応 | M1 | `StyleSettings::rules` が読込み時に決まる |
| W-S3 | features の組合せ | 対応（`Features`） | M1 | `Features` の型 |
| W-S4 | 文字列の型 | 対応（`TargetKind`、`Edition`、`DependencyKind`） | M1 | 型 |
| W-S5 | `Option` の resolve | 対応 | M1 | `rg "resolve: Option"` が 0 件 |
| W-I1 | metadata・JSON の二重 | 対応 | M1 | `selection_run_only_when_needed` |
| W-I2 | 行の数え方の割当て | 対応 | M2 | `rg "source_text()" crates/xtask-source` が 0 件 |
| W-I3 | `visible_to` の照合が P×M | 対応しない（規模が小さい） | — | — |
| D-B1 | 局所の thread_local が glob を隠さない | 対応 | M3b | `local_thread_local_shadows_glob` |
| D-B2 | 外部 glob の判定が三種類ある | 対応 | M3b | `rg "fn is_trusted_glob"` が 1 件 |
| D-B3 | `"$external_globs"` の上書きと漏れ | 対応（専用の field。glob で名前と同じ規則で伝わる） | M3b | `rg "\\$external_globs"` が 0 件、`external_glob_reexport_keeps_uncertainty` |
| D-B4 | glob が最初の解決先しか使わない | 対応 | M3b | `glob_uses_every_resolved_module` |
| D-B5 | `cfg(true)` を偽と評価する | 対応 | M3a | `cfg_true_is_enabled` |
| D-B6 | verbatim の path が漏れる | 対応 | M3a、M2 | `normalize_strips_verbatim` |
| D-B7 | 誤りに位置が無い | 対応 | M3a | `errors_have_location` |
| D-B8 | inline module の attribute を二度診断する | 対応 | M3a | `invalid_cfg_reported_once` |
| D-B9 | 文法の取りこぼし | 一部対応（表への追加、暗黙の取込み）。pattern と guard は**後続** | M3c | `implicit_format_capture`、`macro_grammar_table` |
| D-B10 | 可視性を模型化していない | **後続** | — | — |
| D-B11 | `thread_local!` の中の参照が落ちる | 対応 | M3b | `thread_local_type_reference` |
| D-C1 | cfg の誤りの扱いが二通り | 対応 | M3a | `invalid_cfg_reported_once` |
| D-C2 | glob が与えうる attribute を検査しない | 対応 | M3b | `block_glob_does_not_hide_module_glob` |
| D-C3 | 第三者の crate を組込みとして扱う | 対応（削除し、policy で信頼する） | M3c | `json_requires_trust` |
| D-C4 | Role の既定値と検証の重複 | 対応 | M1、M3a | `rg "DEFAULT_MAX_ROLE_CHARS"` が 0 件 |
| D-C5 | Role の位置を強制していない | 対応 | M3c | `role_must_be_first` |
| D-C6 | Role の二重の検査 | 対応 | M3c | M3c の手順のコードを目で確かめる |
| D-S1 | bool の組の段階（`external_fallback`、`generated_values`） | 対応 | M3b | `rg "external_fallback\|generated_values"` が 0 件 |
| D-S2 | `strong` の表 | 対応（`Scope`） | M3b | `rg "strong" crates/xtask-modules/src` が 0 件 |
| D-S3 | namespace の mode の save/restore | 対応 | M3b | `rg "self.namespace ="` が 0 件 |
| D-S4 | 番兵の束縛（`LocalValue`） | 対応（`LocalBinding`） | M3b | `rg "LocalValue"` が 0 件 |
| D-S5 | loader の mode（`inline` と `explicit_path`） | 対応（`DirOwnership`） | M3a | `rg "explicit_path"` が 0 件 |
| D-S6 | Role の `Option` | 対応（`Result`） | M3a | `Module.role` の型 |
| D-S7 | parent の二重 | 対応（index） | M3a | `Module.parent` の型 |
| D-S8 | 無効の再導出 | 対応 | M3a | `rg "format!\(\"\{owner\}::"` が 0 件 |
| D-I1 | 多重の parse | 対応 | M3a | `one_parse_two_configurations`、`parse_cache_keeps_failures` |
| D-I2 | 表の clone | 対応 | M3b | `rg "tables.clone()"` が 0 件 |
| D-I3 | scope の materialize | 対応 | M3b | `rg "symbols.extend"` が 0 件 |
| D-I4 | 収束の clone | 対応（worklist） | M3b | `rg "previous == tables"` が 0 件 |
| D-I5 | 二重の処理 | 対応 | M3a | `block_use_is_processed_once` |
| D-I6 | 部分木の clone | 対応 | M3a | loader が `ItemMod` を clone しない（目で確かめる） |
| D-I7 | thread_local の再計算 | 対応（構文から一回） | M3b | `rg "fn uncertain_binding"` が 0 件 |
| D-I8 | cfg の再評価 | 対応（item の cfg は loader だけが評価する） | M3a | `invalid_cfg_reported_once` |
| D-I9 | 文字列の id | 対応 | M3b | resolve の module が `usize` |
| D-I10 | options の clone | 対応（package ごとに一回作り、共有） | C2b | `rg "settings.clone()"` が 0 件 |
| D-D1 | json・Token の特別扱い | 対応 | M3c | `src/json.rs` が無い |
| D-D2 | 未使用の API | 対応 | M3a | `rg "crate_role"` が 0 件 |
| D-D3 | dead code | 対応 | M3c | `cargo clippy -p xtask-modules -- -D warnings` |
| D-D4 | path を文字列にする helper の重複 | 対応 | M3c | `rg "fn path_text"` が 1 件 |
| D-D5 | derive の名前の重複 | 対応 | M3c | `intrinsic::DERIVES` だけ |
| D-D6 | 試験の一時 directory | 対応 | M3a | `tests/common/mod.rs` |

配置の経路（Node、ELK、SVG、HTML）の所見は、経路ごと消すので個別に挙げない。設計者が自分で読んで見つけた点と、その扱いは次のとおり。

| 所見 | 扱い |
|---|---|
| `Direction::TopToBottom` は使われていないうえ、矢印の描き方が上下向きに対応していない | 向きを左から右だけにする |
| `EdgeStyle::Dev` の doc が「正弦波の線」と書いているが、実際は点線 | doc を消す |
| 検証が `xtask-graph`、`xtask-layout`、`xtask-svg` の三か所に重複している | 配置は `check.rs`、図のファイルは `xtask_archive::validate` の二つにする（入力と責任が違う） |
| `xtask-svg` の交差の判定が、二次曲線の一般の交点を解いている | 直交の線分の上だけで判定する |
| `prepare.rs` が同じ列の node の幅を求めるとき、`rank.contains` で O(n²) になる | 列の表を使う |

---

## 8. 設計者の決定（作者が覆してよい）

| ID | 決定 | 理由 |
|---|---|---|
| DEC-1 | `check` は既定で書かない。`--write` で図のファイルを書く | 作者の指示。Kivact の `cargo xtask diagrams` と同じ考え方 |
| DEC-2 | 既定の出力先は `target/xtask/architecture.xtaskmap`。拡張子は `.xtaskmap` | 生成物なので `target/` に置けば、どのリポジトリでも git の対象外になる |
| DEC-3 | 圧縮は `miniz_oxide` 0.8 の zlib（level 6）。復号は低水準の `inflate::core::decompress` で、消費した入力の長さまで確かめる | 純 Rust、unsafe 無し、Kivact の lock と cargo の cache にすでにある。zlib の adler32 で破損を検出できる。高水準の関数は stream の後ろの余分なバイトを見逃す |
| DEC-4 | 座標は整数（px）にする。配置の中と公開の型は `i64`、図のファイルは `i32` | 決定性と network simplex の都合。`i64` なら 10.1 節の上限の中で溢れの検査が要らない。ファイルは小さく保つ |
| DEC-5 | 階層は下から region ごとに配置し、境界の port で辺を区切る | 平らな配置の関数を一つにでき、段ごとに試験できる。ELK の INCLUDE_CHILDREN との違い（port の順は内側で決まる）は、交差数の KPI で監視する |
| DEC-6 | 層の割当てと縦の座標に、同じ network simplex を使う | 最も丁寧に試験する部品を一つにする |
| DEC-7 | crate 図の列の中の順は、名前の順ではなく交差の削減で決める（初めの並びは名前の順） | ELK の時は列を一つの node にする工夫で名前の順に固定していた。自前の実装では交差を減らすほうが読みやすい |
| DEC-8 | 逆向きの辺と同じ列の辺は、U ターンのダミーで回り込ませる。辺は必ず右から出て左に入る | 内部の構造が「隣の層の間の線分の列」だけになり、通路の配線を一つの規則で書ける |
| DEC-9 | `trusted_globs`（解析）と `allowed_globs`（style）は二つ残す | 意味が違う。前者は「この glob を未知の提供元とみなさない」。glob が与える attribute・macro を使うときは、`trusted_attributes`・`trusted_macros` にも書く。後者は「glob の import を style で許す」 |
| DEC-10 | `json!` と `Token!` の特別扱いを止め、このリポジトリの policy の `trusted_macros` に書く。信頼した macro の中の token は解析しない | 自分自身の解析のための例外を、道具の中に持たない |
| DEC-11 | viewer は Kivact の `main`（`2b8df5ac`）に固定する | Kivact は改修中で、改修の branch は公開の API を変える |
| DEC-12 | viewer は font を同梱しない。`FONT_FAMILY` は総称の `monospace` にする | 固定の Kivact の SVG 出力は、`,` で並べた候補を一つの名前として扱うので、総称の一つだけにする。字幅の見積り（ASCII 0.62em、それ以外 1.05em）は一般の等幅 font より広い見積りで、はみ出しにくいが保証ではない。はみ出しても重なるのは文字だけで、配置の不変条件は壊れない |
| DEC-13 | SVG の書出しは Kivact の `Offscreen::render_svg` に、画面と同じ描画の関数を渡して作る。Offscreen は `blocking` の中で作る（`!Send` のため） | 画面と書出しの見た目を一つの実装で保つ |
| DEC-14 | 強調は、静的な図を暗くせず、強調する要素を色の付いた線で重ね描きする | hover で静的な図を描き直さないため（6.2 節） |
| DEC-15 | module 図の当たり判定は region（module の枠）に、crate 図は node に付ける | 一つの item に当たり判定を一つにする |
| DEC-16 | 並列化は「入力ごとに一つの scoped thread、同時の数は CPU の数まで」にする。終わった thread は drop の guard が channel で知らせる | 結果は `join` で受け取るので、共有の枠・処理中の印・番兵が要らない。panic した入力だけが `Err` になり、全入力に必ず結果が一つ付く。`R: Send` だけで済む。thread の終わりで proc-macro2 の span の記録も解放される |
| DEC-17 | 構造は `--all-features`・platform の絞込み無しの metadata 一回から作る。依存の辺は Cargo の宣言から作り、依存先の同一性は path の依存なら path、同名の registry の依存はその resolve で決める。選んだ構成の feature は別の一回で求め、その失敗は解析の環境の失敗として扱う | lock は全部の依存を持つので、構造の metadata は lock があれば必ず解決でき、無効な optional の依存も構造の検査から外れない。target や feature の設定の誤りで、行数・可視性などの独立な検査を止めない |
| DEC-18 | 名前付きの `use` と item の宣言の名前は、構文から先に決める。glob を隠すかどうかは名前の有無だけで決まるので、glob の伝播は単調に増え、worklist で収束する。名前付きの `use` は三つの名前空間すべてで glob を隠す（近似） | 後から隠すことによる取消しが無くなる。名前付きの use の名前空間は解決するまで分からないため、安全側の近似にする |
| DEC-19 | `thread_local!` と認めるのは、`thread_local`・`std::thread_local`・`::std::thread_local` の書き方と、同じ module の `use std::thread_local [as X];` の別名だけ | 名前の解決の前に宣言を決められる（DEC-18） |
| DEC-20 | style の options-default の probe は、selector ごとの crate のままにする（一括化しない） | 一つの crate にまとめると、名前の解決の誤りで他の selector の型検査の診断が出なくなる |
| DEC-21 | viewer の木は、単クリックで選び、Enter かダブルクリックで開く | 固定の Kivact の tree の振舞い（`on_activate`）に合わせる。選択を遷移につなぐ effect を作らない |

---

## 9. 作業の段階

| wave | 内容 | lane（→ は同じ lane で順に行う） |
|---|---|---|
| W1 | 土台：配置の型と最適化の部品、図のファイル、policy と workspace、source | L1、A1、M1、M2 |
| W2 | 配置の各段階と検証、module の解析、viewer の骨組みと描画の計算 | L2→L6、L3、L4、L5、M3a→M3b→M3c、V1 |
| W3 | 配置の階層、基準の比較、style、viewer の画面 | L7、L8、C1、V2a、V2b→V2d、V2c |
| W4 | 図の組立て、viewer の GUI 試験、README | A2、V3、C3 |
| W5 | xtask-core の組み直しと旧い経路の削除 | C2a→C2b→C2c |
| CP | 全体の試験、KPI、受入れ条件の点検 | Orchestrator |

- 同じ wave の中で、他の lane の未完成の関数（`todo!()`）を**実行する**試験は無い（compile だけは通る）。
- W1〜W4 の間は、`xtask-core`（と、消す予定の `xtask-graph`・`xtask-svg`）が compile できない。W5 で直す。各 wave の gate は、その wave までに完成した crate だけを対象にする（作業指示書 2 章）。

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
| `json!`・`Token!` を使うリポジトリ | `trusted_macros` に書く。その中の参照は依存の辺にならない |
| Role の位置 | 最初の inner doc の最初の行に置く |
| `thread_local!` を別の module の別名で呼ぶコード | `thread_local!` か `std::thread_local!` で呼ぶ |
| 例外の `path = "*"` | `src/*` など、directory の接頭辞にする |
| root の `Cargo.toml` | `[profile.dev.package.xtask-layout] opt-level = 3` を足す |

Kivact の `dev/xtask` の修正は、本改修の範囲外にする（Kivact 側の作業）。

## 12. 後続（本改修では行わない）

- X-B6：required-features の `dep/feat` の判定。
- D-B9 の残り：定数を名指す pattern、guard の `if let` の束縛。
- D-B10：可視性の模型化（名前付きの `use` の名前空間の精密化を含む）。
- style の probe の一括化（`cargo check --keep-going` で複数の probe の crate を一度に検査する）。
- viewer の macOS 対応（Kivact が未対応）。
