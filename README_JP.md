# xtask-core

[English](README.md)

Rust workspace向けのリポジトリ規則検査とアーキテクチャ図の生成ツール。
`cargo xtask` から実行する。

`cargo xtask check` は、リポジトリの規則をすべて検査し、何も書き出さない。

- ファイルごとのコード行数の上限を検査する。
- package間の依存を許可リスト（`visible_to`）で制限する。
- 全crate root・moduleに1行の `Role:` 宣言を要求する。
- module間の参照循環を禁止する。
- [Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html)のうち、機械的に検査できる項目を任意で適用する。

`cargo xtask check --write` は、さらに crate・module の依存図を配置して、圧縮した一つのファイルに書き出す。`xtask-viewer` の GUI がそのファイルを開く。詳しくは[出力](#出力)を参照。

## 動作要件

- Rust 1.85以降。Node.js は不要。
- `Cargo.lock` と依存crateのキャッシュ。Cargoは `--locked --offline` で実行する。
- style検査を有効にする場合は `rustfmt` と `clippy` コンポーネント。
- viewer を build する場合：Rust 1.98.1（`viewer/rust-toolchain.toml` で選ばれる）と、Kivact を取得するためのネットワーク接続。

## 導入

対象リポジトリに本リポジトリを配置し（例：`vendor/xtask-core` にsubmoduleとして追加）、xtask packageを作成する。

`dev/xtask/Cargo.toml`：

```toml
[package]
name = "xtask"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
xtask-core = { path = "../../vendor/xtask-core/crates/xtask-core" }
```

`dev/xtask/src/main.rs`：

```rust
//! Role: 共通のリポジトリ検査を実行する。

fn main() -> std::process::ExitCode {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be inside dev")
        .parent()
        .expect("dev must be inside the repository");
    xtask_core::main(root)
}
```

`.cargo/config.toml`：

```toml
[alias]
xtask = "run --package xtask --"
```

workspaceの `members` に `dev/xtask` を追加し、後述のpolicyファイルを作成する。

## 使い方

```sh
cargo xtask check              # 全検査を行う。何も書かない
cargo xtask check --write      # 全検査を行ったあと、図のファイルを書く
cargo xtask check --root PATH  # 別のリポジトリを検査する
cargo xtask check --help
```

違反はすべて集約して報告する。終了コードは、成功で0、違反または実行エラーで1、引数の誤りで2。削除された `--no-write` と `--prune` を含め、ほかのoptionは引数の誤りになる。

`--write` は、検査に失敗しても、作れる図はすべて書く。解析に失敗したtargetの図は、図の代わりに失敗の理由をファイルに記録する。ファイルを書けなかったときは、その誤りを他の違反と一緒に報告する。

style検査が有効な場合は、どのモードでもrustfmtとClippyを実行する。そのため `target/` 以下へのビルド出力が生じ、build scriptとprocedural macroが実行される。

test・benchのtargetは `cfg(test)` 構成だけを解析する。`test` が有効な library・binary・example・procedural macro のtargetは、通常構成と `cfg(test)` 構成の両方を解析する。それ以外のtarget（`test = false` のtargetを含む。exampleの既定値がこれにあたる）は通常構成だけを解析する。`required-features` が無効なtargetは解析せず、図にその旨を記録する。

## 設定

policyは `dev/xtask/policy/workspace.toml` から読み込む。ファイルが無いときは、すべての節を既定値で扱う。未知のキーはエラーになり、各節は `[limits]` のようなTOMLの表でなければならない。

```toml
[limits]
max_code_lines = 1000     # ファイルごと。1000以下
max_role_chars = 80       # Role説明の文字数（Unicode文字単位）

[visible_to]
internal = ["facade", "app-*"]
app = []

[diagrams]
targets = "all"          # "all"、"lib-bin"、"lib"
output = "target/xtask/architecture.xtaskmap"   # 既定値。ルートからの相対パス、または絶対パス

[analysis]
target = "x86_64-unknown-linux-gnu"
features = ["app/feature"]
all_features = false
default_features = true
cfg = ["custom_cfg"]
trusted_attributes = ["serde"]
trusted_derives = ["Deserialize", "Serialize"]
trusted_macros = ["proptest", "prop_assert"]
trusted_globs = ["framework::prelude"]
generated_type_attributes = ["component"]

[style]
enabled = true
```

### 行数上限

空行・コメント・文字列リテラルの内容を除いた行数を数える。

### 依存の可視性

`visible_to` のキーは依存される側のpackage、値はそれに依存してよいpackageの一覧。末尾の `*` は前方一致。`[]` はすべての依存を禁止し、キーのないpackageは制限しない。キーとパターンはworkspace memberに一致する必要がある。normal・dev・build・optional・target条件付きの依存をすべて検査する。

### 役割宣言

全crate root（library・binary・test・bench・example・build script）と全moduleは、最初の行が `Role:` で始まる1行の内側ドキュメントを持つ。

```rust
//! Role: アプリケーション全体を制御する。

mod config {
    //! Role: 実行設定を保持する。
}
```

`Role:` の行は、そのmoduleの最初の内側ドキュメントコメントの最初の行でなければならない。それ以外の場所の `Role:` 行（たとえば最初の内側ドキュメントコメントの2行目）は誤りになる。別ファイルのmoduleでは、そのファイルの先頭に書く。`mod name;` 側の外側コメントでは代用できない。`/*! Role: ... */` と `#![doc = "Role: ..."]` も同じ意味で使える。説明は空でなく、`max_role_chars` 以下とする。

### 図

`[diagrams] targets` で、module図を書くtargetを選ぶ。

| 値 | 対象 |
| --- | --- |
| `"all"`（既定） | 全target |
| `"lib-bin"` | library系とbinary |
| `"lib"` | `lib`、`rlib`、`staticlib`、`cdylib`、`dylib`、`proc-macro` |

選択外のtargetも検査は行い、図だけを省く。

`[diagrams] output` は図のファイルのpathで、リポジトリルートからの相対パスまたは絶対パス。既定値は `target/xtask/architecture.xtaskmap`。

### 解析

既定では、host target・Cargoの既定feature・追加cfgなしで解析する。`[analysis]` はソース解析とClippyの両方の設定を変える。

- `features` は有効にするfeatureの一覧。各項目は空でなく、重複が無く、前後に空白を含まない。`default_features`（既定 `true`）はCargoの既定featureを使うかを決める。
- `all_features = true` は、すべてのfeatureを有効にして解析する。`features` とは一緒に使えず、`default_features` を書いてはならない。違えば policy は `all_features cannot be combined with features or default_features` の誤りで拒まれる。
- `cfg(true)` は有効なものとして評価する。

`trusted_attributes`、`trusted_derives`、`trusted_macros` には、module構造に影響しないとみなす項目を指定する。trusted指定したfunction-like macroの呼出しは、呼出し箇所に書かれたpath（`serde_json::json` など）で完全一致させる。そのような呼出しは展開しないため、中の参照は解析されず、依存の辺にもならない。

`json!` と `Token!` は特別扱いしない。これらを使うリポジトリは、呼び出すときに書くpathを `trusted_macros` に書く（例：`serde_json::json`、`Token![...]` なら `syn::Token`）。

他crateからのglob importはどのmacro・derive・attributeでも提供しうるため、それを含むmoduleでは `vec!` や `#[derive(Clone)]` の提供元を特定できない。`trusted_globs` には、そのようなimportをpath（`framework::prelude` など）で指定し、未知の提供元として扱わないようにする。glob が提供するattributeやmacroをコードが使うなら、その名前を `trusted_attributes` または `trusted_macros` にも書く。

`trusted_globs` と `[style]` の `allowed_globs` は別の設定。`allowed_globs` は `glob-imports` 規則でそのglobを許すだけで、解析の結果は変えない。両方が必要なglobは、両方に書く。

`generated_type_attributes` には、印を付けたfnの隣に型を生成するattribute macro（`component` など）を指定する。これを持つfnは、PascalCaseにした名前の型（`#[component] fn user_row` なら `UserRow`）も型と値の名前空間に宣言するものとして扱い、生成された型へのpathが解決される。そのattributeは `trusted_attributes` にも書く必要がある。

## 出力

### 図のファイル

`--write` は、`[diagrams] output`（既定 `target/xtask/architecture.xtaskmap`）に一つのファイルを書く。`--write` のたびに、そのファイルを丸ごと書き直す。ほかのファイルは書かず、削除もしない。ファイルは zlib で圧縮したバイナリ（形式の版 1）で、その回に作ったすべての図と通知を含む。`target/` の下にあり、`target/` はビルド出力の directory なので、通常はバージョン管理の対象外（`.gitignore`）にする。

`--write` を付けない `check` は、このファイルに触れない。

### viewer

`xtask-viewer` は、図のファイルを開くGUIである。[`viewer/`](viewer) のworkspaceで、Kivactを使って作る。

- ファイルは、引数にpathを渡すか、Open のダイアログか、ウィンドウへのdrag & dropで開く。開くと crate 図から始まる。
- ファイルは500 msごとに確かめ、変わっていれば読み直す。F5で今すぐ読み直す。
- 要素にホバーすると、その依存先と依存元を強調する。クリックかSpaceで強調を固定し、Escか背景のクリックで解く。
- crateやmoduleをダブルクリックするか、Enterを押すと、module図を開く。Back（Backspace）で前の図に戻り、Fit（0）で全体を表示する。
- ドラッグで移動し、ホイールでポインタの位置を中心に拡大・縮小する。
- 左の木は crate・target・module を表示し、検索欄で絞り込める。Enter かダブルクリックで開く。単クリックは選択だけ。
- 表示中の図をSVGに書き出せる。

build（Windows）：

```sh
cd viewer
cargo build --release
```

実行ファイルは `viewer/target/release/xtask-viewer.exe`。buildには Rust 1.98.1（`viewer/rust-toolchain.toml` で選ばれる）と、固定の revision の Kivact を取得するためのネットワーク接続が要る。macOSは、Kivactが未対応のため、まだ対象外。

`main` への push で `.github/workflows/viewer-windows.yml` が動く。これは Windows で viewer を release build し、`xtask-viewer.exe` を `xtask-viewer-windows` という名前の成果物（artifact）として残す。

## 移行

以前の版を使っているリポジトリを直すには：

- Node.js と `XTASK_NODE` は使わなくなった。CI から Node.js と elkjs の設定を消す。
- `--no-write` は既定の動作になり、そのflagは削除された。scriptから消す。図のファイルを書くには `--write` を使う。
- `--prune` は削除された。`--write` のたびに図のファイルを丸ごと書き直すので、古い図は残らない。古い `architecture/` の directory は、中のSVGとHTMLを含めて手で消す。ツールはそれを消さない。
- policy の最上位の `output` は `[diagrams] output` に移った。`output` が残っている policy は拒まれ、その誤りは新しいキーを示す。
- Role 宣言は、そのmoduleの最初の内側ドキュメントコメントの最初の行に置く。
- 例外の `path` が `*` だけのものは誤りになる。`src/*` のような directory の接頭辞にする。
- `json!` や `Token!` を使うリポジトリは、それらを `trusted_macros` に書く。
- library を使う側：
  - `Config::no_write` は削除された。図のファイルを書くには `config.output = Output::Policy` とする。
  - `Config::max_role_chars` は削除された。Role の上限は `[limits] max_role_chars` だけで決まる。
  - `TargetGraph` は `TargetAnalysis` に替わり、その graph は `TargetOutcome::Analyzed(graph)` になる。
  - `Report::artifacts` は `Report::archive` に、`Report::style_diagnostics` は `Report::style` に替わった。
  - `Report::errors` は style の違反を含まなくなった。`Report::style` も見る。
- 利用側のリポジトリの root の `Cargo.toml` に、debug build でも配置を速くするため次を足す：

  ```toml
  [profile.dev.package.xtask-layout]
  opt-level = 3
  ```

## style検査

`[style]` は、Google Rust Style Guideのうち機械的に検査できる規則を適用する。ガイド全体への準拠を保証するものではない。文書と安全性コメントは、存在だけを検査する。

`enabled = true` で規則1〜13を選択する。規則14〜20は対象の指定やレビュー済みの例外が必要なため、`extra_rules` で追加する。

| # | 規則 | 検査内容 | 実装 |
| --- | --- | --- | --- |
| 1 | `format` | プロジェクトのrustfmt設定に一致し、有効な `max_width` が100以下であること | rustfmt |
| 2 | `hex-literals` | 16進整数リテラルは小文字。macro呼出し内も対象 | 構文解析 |
| 3 | `import-granularity` | 1つの `use` は1つのmoduleからimportする。波括弧内のネストしたpathは不可 | 構文解析 |
| 4 | `unstable-features` | `#![feature(...)]` の禁止 | rustc `unstable_features` |
| 5 | `unwrap` | テストコード以外での `Option` / `Result` の `unwrap` / `unwrap_err` の禁止 | Clippy `unwrap_used` |
| 6 | `checked-unwrap` | 値の確認後のunwrapの禁止 | Clippy `unnecessary_unwrap` |
| 7 | `flexible-references` | `&Box<T>`、`&Vec<T>`、`&String`、`&Option<T>` 引数の禁止 | Clippy `borrowed_box`、`ptr_arg`、`ref_option` |
| 8 | `derivable-impls` | `derive` で置換できる手書きimplの禁止 | Clippy `derivable_impls` |
| 9 | `new-default` | 引数なしの公開 `new` には `Default` を実装する | Clippy `new_without_default` |
| 10 | `len-is-empty` | 公開 `len` には `is_empty` を対応させる | Clippy `len_without_is_empty` |
| 11 | `missing-docs` | 公開itemの文書化 | rustc `missing_docs` |
| 12 | `safety-docs` | unsafe関数の `# Safety` 節 | Clippy `missing_safety_doc` |
| 13 | `safety-comments` | unsafe block・implの `SAFETY:` コメント | Clippy `undocumented_unsafe_blocks` |
| 14 | `glob-imports` | glob importの禁止。inline moduleの `super::*`、関数本体内の `use Enum::*`、非leaf moduleの `pub use child::*`、`allowed_globs` のpathは許可 | 構文解析 |
| 15 | `non-leaf-implementation` | 子moduleを持つmoduleは宣言・import・再exportのみ。test moduleと `#[cxx::bridge]` は許可 | 構文解析 |
| 16 | `rustfmt-skip` | あらゆる形の `#[rustfmt::skip]` の禁止 | 構文解析 |
| 17 | `non-exhaustive` | `api_packages` の公開enumと全field公開structに `#[non_exhaustive]` | Clippy `exhaustive_enums`、`exhaustive_structs` |
| 18 | `catch-unwind` | `std::panic::catch_unwind` の禁止 | Clippy `disallowed_methods` |
| 19 | `poison-access` | `PoisonError::into_inner`、`get_ref`、`get_mut` の禁止 | Clippy `disallowed_methods` |
| 20 | `options-default` | `options_structs` の型が `Default` を実装すること | コンパイル検査 |

```toml
[style]
enabled = true
extra_rules = [
    "glob-imports",
    "non-leaf-implementation",
    "rustfmt-skip",
    "non-exhaustive",
    "catch-unwind",
    "poison-access",
    "options-default",
]
disabled_rules = ["missing-docs"]
api_packages = ["my-library"]
options_structs = ["my-library::options::Options"]
allowed_globs = ["framework::prelude"]
toolchain = "1.85.0"      # 任意。rustupのツールチェーン名

[[style.exceptions]]
rule = "rustfmt-skip"
path = "crates/my-library/src/generated/*"
reason = "生成元の配置を維持する。"

[[style.exceptions]]
rule = "unwrap"
path = "crates/app/src/main.rs"
item = "startup"
reason = "設定はこの呼出しの前に検証済み。"
```

規則を免除する方法は例外の指定だけで、選択したlintはソース中の `#[allow(...)]` より優先する。`path` はリポジトリルートからの相対パスで、末尾の `*` で前方一致にできる（`src/*` など）。`*` だけの `path` は誤り。`item` を指定すると、module階層内の1つのitemに限定できる。`reason` は必須。ツールの未導入やコンパイル失敗などの実行エラーは、例外では免除されない。

補足：

- 構文検査は探索した全ソースを対象とし、無効なcfg配下のコードも含む。macroの入力はRust構文として解釈せず、生成コードは展開しない。
- Clippyは `[analysis]` の設定で全workspace targetを検査する。featureの全組合せは網羅しない。
- プロジェクトのClippy設定は引き継ぐ。ただし `allow-unwrap-in-tests = true`、`check-private-items = true`、`avoid-breaking-exported-api = false` を設定する。
- `ref_option` にはClippy 1.83以降が必要。
- `analysis.cfg` を使う場合、Cargo設定ファイルの `rustflags` は暗黙には統合しない。`RUSTFLAGS` または `CARGO_ENCODED_RUSTFLAGS` で明示する。
- `options-default` はlockfileの複製に対して一時crateをコンパイルする。リポジトリのmanifestとlockfileは変更しない。指定する型は公開されたgenericでない型とする。

## ライブラリAPI

検査はライブラリとしても利用できる。

```rust
use xtask_core::{Config, run};

let config = Config::new("path/to/repository"); // 既定では何も書かない
let report = run(&config)?;
for diagnostic in &report.style {
    println!("{diagnostic}");
}
```

- `Config` は `root`、`policy`、`output`、`analysis` を持つ。`output` は `Output::None`（既定。何も書かない）、`Output::Policy`（`[diagrams] output` の path）、`Output::Path(PathBuf)`（指定の path。ルートからの相対または絶対）のいずれか。`analysis` が `Some` のときは、policy の `[analysis]` の設定に代わる。
- `run` が `Err` を返すのは、policy と Cargo metadata を読めないときだけ。ほかの失敗は `Report::errors` に入る。
- `Report` は `errors`、`notes`、`style`、`archive` を持つ。`style` は構造化された style の違反で、`errors` には繰り返さない。
- `analyze(&config)` は target ごとの `TargetAnalysis` を返す。構成（`Mode::Normal` と `Mode::Test`）ごとに、`Analyzed(ModuleGraph)`、`FeatureGated`、`UnsupportedEdition`、`Failed` のいずれかの結果を持つ。何も書かず、style検査も行わない。

## 制限事項

- `syn` による静的解析のため、型推論とmacro展開は再現しない。未対応の構文は無視せずエラーとして報告する。
- Rust 2018・2021・2024 editionに対応する。

## 開発

```sh
cargo test --locked --offline --workspace
cargo fmt --all -- --check
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
cargo xtask check
cargo xtask check --write
```

viewer は別のworkspace（root の `Cargo.toml` で `exclude = ["viewer"]`）なので、検査は `viewer/` で行う。

```sh
cd viewer
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

別のリポジトリでの実行時間は `benchmark` exampleで計測できる。ROOT に対して `check` と `check --write` の時間を出す。`--write` の出力は ROOT の `target/` に書かれる。

```sh
cargo run --release -p xtask-core --example benchmark -- ROOT
```

workspaceのcrate構成：

| Crate | 役割 |
| --- | --- |
| `xtask-core` | CLI、行数と可視性の検査、図のファイル |
| `xtask-source` | ソース探索、path の正規化、行数計測 |
| `xtask-policy` | policyの読込みと検証 |
| `xtask-workspace` | Cargo metadata（構造とfeature） |
| `xtask-modules` | moduleと参照の解析 |
| `xtask-style` | Google Rust Style Guide の検査 |
| `xtask-layout` | 配置エンジン（Rustのみ） |
| `xtask-archive` | 図のファイル：型・検証・符号化・圧縮 |
| `xtask-diagram` | 解析結果から図を組み立てて配置する |

viewer の crate（`viewer/`）：

| Crate | 役割 |
| --- | --- |
| `xtask-render` | 描画の幾何（角の丸め、交差の跳び、矢印、文字の位置）。Kivactに依存しない |
| `xtask-viewer` | Kivact のGUI。`xtask-render` が計算した結果を描く |

## ライセンス

Copyright (c) 2026 Kashiwade.

次のいずれかのライセンスを選択して利用できる。

- Apache License, Version 2.0（[LICENSE-APACHE](LICENSE-APACHE) または <https://www.apache.org/licenses/LICENSE-2.0>）
- MIT license（[LICENSE-MIT](LICENSE-MIT) または <https://opensource.org/licenses/MIT>）

明示的に別段の意思表示をしない限り、Apache-2.0ライセンスの定義に従って本プロジェクトへの取り込みを意図して提出された貢献は、追加の条件なしに上記のデュアルライセンスで提供されるものとする。
