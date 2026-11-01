# xtask-core

[English](README.md)

Rust workspace向けのリポジトリ規則検査とアーキテクチャ図の生成ツール。
`cargo xtask` から実行する。

`cargo xtask check` の1コマンドで次を行う。

- ファイルごとのコード行数の上限を検査する。
- package間の依存を許可リスト（`visible_to`）で制限する。
- 全crate root・moduleに1行の `Role:` 宣言を要求する。
- module間の参照循環を禁止する。
- [Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html)のうち、機械的に検査できる項目を任意で適用する。
- crate・moduleの依存図をSVGと、単一ファイルの対話型HTMLとして出力する。

## 動作要件

- Rust 1.85以降。
- `Cargo.lock` と依存crateのキャッシュ。Cargoは `--locked --offline` で実行する。
- 図の書き出しにNode.js。`PATH` 上の `node` 以外を使う場合は `XTASK_NODE` で指定する。
  `elkjs` 0.12.0を同梱しているため、npmのインストールやネットワーク接続は不要。`--no-write` ではNode.jsを使わない。
- style検査を有効にする場合は `rustfmt` と `clippy` コンポーネント。

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
cargo xtask check              # 検査し、SVGとHTMLを書き出す
cargo xtask check --no-write   # 検査のみ。CI向け
cargo xtask check --root PATH  # 別のリポジトリを検査する
cargo xtask check --help
```

違反はすべて集約して報告する。検査に失敗しても、生成できる図は書き出す。
終了コードは、成功で0、違反または実行エラーで1、引数の誤りで2。

`--no-write` は全検査と図の構造検証を行い、SVG・HTMLは書き出さない。
style検査が有効な場合はrustfmtとClippyを実行するため、`target/` 以下へのビルド出力と、build script・procedural macroの実行が発生する。

各targetは通常構成と `cfg(test)` 構成の両方を解析する。

## 設定

policyは `dev/xtask/policy/workspace.toml` から読み込む。未知のキーはエラーになる。

```toml
output = "architecture"   # SVG・HTMLの出力先。ルートからの相対パス

[limits]
max_code_lines = 1000     # ファイルごと。1000以下
max_role_chars = 80       # Role説明の文字数（Unicode文字単位）

[visible_to]
internal = ["facade", "app-*"]
app = []

[diagrams]
targets = "all"           # "all"、"lib-bin"、"lib"

[analysis]
target = "x86_64-unknown-linux-gnu"
features = ["app/feature"]
all_features = false
default_features = true
cfg = ["custom_cfg"]
trusted_attributes = ["serde"]
trusted_derives = ["Deserialize", "Serialize"]
trusted_macros = ["proptest", "prop_assert"]

[style]
enabled = true
```

### 行数上限

空行・コメント・文字列リテラルの内容を除いた行数を数える。

### 依存の可視性

`visible_to` のキーは依存される側のpackage、値はそれに依存してよいpackageの一覧。
末尾の `*` は前方一致。`[]` はすべての依存を禁止し、キーのないpackageは制限しない。キーとパターンはworkspace memberに一致する必要がある。
normal・dev・build・optional・target条件付きの依存をすべて検査する。

### 役割宣言

全crate root（library・binary・test・bench・example・build script）と全moduleの先頭に、1行の内側ドキュメントを書く。

```rust
//! Role: アプリケーション全体を制御する。

mod config {
    //! Role: 実行設定を保持する。
}
```

別ファイルのmoduleでは、そのファイルの先頭に書く。`mod name;` 側の外側コメントでは代用できない。
`/*! Role: ... */` や `#![doc = "Role: ..."]` も使える。説明は空でなく、`max_role_chars` 以下とする。

### 図

`[diagrams] targets` で、module図を生成するtargetを選ぶ。

| 値 | 対象 |
| --- | --- |
| `"all"`（既定） | 全target |
| `"lib-bin"` | library系とbinary |
| `"lib"` | `lib`、`rlib`、`staticlib`、`cdylib`、`dylib`、`proc-macro` |

選択外のtargetも検査は行い、図だけを省く。

### 解析

既定では、host target・Cargoの既定feature・追加cfgなしで解析する。
`[analysis]` はソース解析とClippyの両方に適用される。

`trusted_attributes`、`trusted_derives`、`trusted_macros` には、module構造に影響しないとみなす項目を指定する。
trusted指定したfunction-like macroの呼出し（呼出し箇所に書かれたpathで完全一致）は展開しないため、その中の参照は解析されない。

## 出力

```text
architecture/
├── index.html
├── crates.svg
└── modules/<package>/
    ├── <target>-<kind>-normal.svg
    └── <target>-<kind>-test.svg
```

`index.html` はSVG・CSS・JavaScriptをすべて埋め込んだ単一ファイルで、crate依存図から開く。

- crate・moduleにホバーすると、依存先と依存元を強調する。クリックで強調を固定し、Escまたは空白部分のクリックで解除する。
- crateをダブルクリックするとmodule図を開く。他のtargetやテスト構成は選択欄で切り替える。
- 左ペインに全crate・target・moduleのツリーを表示する。
- ドラッグで移動、ホイールで拡大・縮小する。
- キーボードではTabで移動、Spaceで固定、Enterで開く。

書き出しのたびに、今回生成しなかった `<output>/modules` 以下のmodule SVG（削除・選択外になったtargetの図）を削除する。
解析に失敗した場合、実在するtargetの図は残す。`--no-write` は削除対象を報告するだけで、ファイルは変更しない。

## style検査

`[style]` は、Google Rust Style Guideのうち機械的に検査できる規則を適用する。
ガイド全体への準拠を保証するものではない。文書と安全性コメントは、存在だけを検査する。

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
| 14 | `glob-imports` | glob importの禁止。inline moduleの `super::*`、関数本体内の `use Enum::*`、非leaf moduleの `pub use child::*` は許可 | 構文解析 |
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

規則を免除する方法は例外の指定だけで、選択したlintはソース中の `#[allow(...)]` より優先する。
`path` はリポジトリルートからの相対パスで、末尾の `*` で前方一致にできる。
`item` を指定すると、module階層内の1つのitemに限定できる。`reason` は必須。
ツールの未導入やコンパイル失敗などの実行エラーは、例外では免除されない。

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

let mut config = Config::new("path/to/repository");
config.no_write = true;
let report = run(&config)?;
for diagnostic in &report.style_diagnostics {
    println!("{diagnostic}");
}
```

`Config` で出力先、Role文字数の上限、`[analysis]` の設定を上書きできる。
`analyze(&Config)` はstyle検査と書き出しを行わず、targetごとのmodule graphを返す。
style違反は構造化された `Report::style_diagnostics` で取得でき、`Report::errors` にも含まれる。

## 制限事項

- `syn` による静的解析のため、型推論とmacro展開は再現しない。未対応の構文は無視せずエラーとして報告する。
- Rust 2018・2021・2024 editionに対応する。

## 開発

```sh
cargo test --locked --offline --workspace
cargo fmt --all -- --check
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
node --test crates/xtask-core/src/arch_map/viewer.test.cjs
cargo xtask check
```

別のリポジトリでの実行時間は `benchmark` exampleで計測できる。OUTPUTに絶対パスを指定すると、対象リポジトリの図を変更しない。

```sh
cargo run --release -p xtask-core --example benchmark -- ROOT OUTPUT MODE
```

`MODE` は `analyze`、`no-write`、`write` のいずれか。

workspaceのcrate構成：

| Crate | 役割 |
| --- | --- |
| `xtask-core` | CLI、検査、style規則、出力 |
| `xtask-workspace` | Cargo metadataとpolicy |
| `xtask-source` | ソース探索と行数計測 |
| `xtask-modules` | moduleと参照の解析 |
| `xtask-graph` | グラフ構築とSVG書き出し |
| `xtask-layout` | Node.js経由のELKレイアウト |
| `xtask-svg` | SVG描画 |

## ライセンス

Copyright (c) 2026 Kashiwade.

次のいずれかのライセンスを選択して利用できる。

- Apache License, Version 2.0（[LICENSE-APACHE](LICENSE-APACHE) または <https://www.apache.org/licenses/LICENSE-2.0>）
- MIT license（[LICENSE-MIT](LICENSE-MIT) または <https://opensource.org/licenses/MIT>）

明示的に別段の意思表示をしない限り、Apache-2.0ライセンスの定義に従って本プロジェクトへの取り込みを意図して提出された貢献は、追加の条件なしに上記のデュアルライセンスで提供されるものとする。

`crates/xtask-layout/vendor/elkjs` は個別のライセンスで配布される。同ディレクトリ内のファイルを参照。
