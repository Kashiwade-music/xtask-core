# xtask-core

Rust workspace向けの共通検査・アーキテクチャ可視化ツール。各リポジトリの `dev/xtask` から利用する。

## 機能

- **コード行数**：コメント・文字列内容・空行を除き、1ファイルn行以下
- **依存制限**：`visible_to` によるcrate間の依存許可リスト
- **役割宣言**：全crate root・moduleに `Role:` を必須化
- **循環検出**：module間の参照循環を禁止
- **SVG出力**：crate依存図、module階層・参照図を生成

## 導入

共通リポジトリを `vendor/xtask-core` にsubmoduleとして配置し、以下を設定する。

**`dev/xtask/Cargo.toml`**

```toml
[package]
name = "xtask"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
xtask-core = { path = "../../vendor/xtask-core/crates/xtask-core" }
```

workspaceのmembersに `dev/xtask` を追加する。

**`dev/xtask/src/main.rs`**

```rust
//! Role: 共通xtaskを起動する。

fn main() -> std::process::ExitCode {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap();
    xtask_core::main(root)
}
```

**`.cargo/config.toml`**

```toml
[alias]
xtask = "run --package xtask --"
```

**`dev/xtask/policy/workspace.toml`**

```toml
output = "architecture"

[limits]
max_code_lines = 1000
max_role_chars = 80

[visible_to]
internal = ["facade", "app-*"]
app = []
```

`visible_to` は依存される側をキーに、依存を許可するpackage名を指定する。未指定なら制限なし、`[]` は外部crateからの依存禁止、末尾の `*` は前方一致。normal・dev・build・optional・target条件付き依存も検査する。

行数上限は最大1000、役割文字数の既定値は80（変更可能）。

## 役割宣言

全crate root（lib・bin・test・bench・example・build script）と各moduleに、内側ドキュメントで1行の役割宣言を書く。

```rust
//! Role: アプリケーション全体を制御する。

mod config {
    //! Role: 実行設定を管理する。
}
```

外部ファイルのmoduleはファイル先頭に記述する。`mod name;` 側の外側ドキュメントでは代用できない。

説明は1文字以上、設定上限以下。同等の `/*! Role: ... */` や `#![doc = "Role: ..."]` も使用できる。
未記載・不正な役割は図で `[役割未記載]` と表示し、検査エラーとして報告する。

## コマンド

```sh
cargo xtask check
```

`check` 一つでコード行数・依存制限・役割宣言・module参照循環を検査し、crate依存図と全module図を生成する。
検査違反があっても生成可能な図は出力し、診断をまとめて非ゼロ終了する。図の生成に失敗した場合もエラーとする。
通常構成とCargoで有効なテスト構成は毎回解析する。

SVGの出力先はpolicyのトップレベル `output` で指定する。省略時は `architecture/`。
相対パスはリポジトリルート基準で、絶対パスも使用できる（例：`output = "docs/architecture"`）。

```text
architecture/
├── crates.svg
└── modules/<package>/
    ├── <target>-<kind>-normal.svg
    └── <target>-<kind>-test.svg
```

図を生成するtargetはpolicyの `[diagrams] targets` で選ぶ。省略時は `"all"`（全target）で、既存の挙動と同じ。
`"lib"` は library系（`lib`・`rlib`・`staticlib`・`cdylib`・`dylib`・`proc-macro`）、`"lib-bin"` はそれに `bin` を加える。
選んだtargetの `cfg(test)` 図は含まれる。選外のtarget（test・bench・example・build scriptなど）も行数・役割・依存・循環の検査は行い、図だけを生成しない。

```toml
[diagrams]
targets = "lib-bin"
```

通常の `check` は、今回の実行で生成しなかった既存のmodule図（選外になったtarget、またはrename・削除されたpackage・targetの図）を `<output>/modules` 以下の `*.svg` から探して自動削除する。削除は `pruned stale module diagram` として報告し、空になったdirectoryも消す。`--no-write` は削除予定の図を報告するだけで、ファイルを変更しない。`--prune` は互換性のため引き続き受理するが、通常の `check` と同じ動作で、`--no-write` とは同時に使えない。

解析失敗や検査エラーがある場合はCargo metadataに存在する選択targetの図を保護し、確実に削除・renameされたpackage・targetや選外targetの図だけを削除する。解析全体が失敗しても、実在targetの図は保持する。`required-features` が有効でなく解析をskipしたtargetも、選択に含まれる限りその図は報告も削除もしない。Windowsではpathの大文字小文字を区別せずに比較する。`crates.svg`、`*.svg` 以外のファイル、`<output>/modules` 外のファイル、symlinkは削除対象外。

```sh
cargo xtask check
```

CIでは `--no-write` でファイルを変更せずに検査できる。
全ルールと図の構造を検査し、SVG生成・書き込みを省くためNode.jsは不要。既存SVGとの差分検査は行わない。
別リポジトリを指定する場合は `--root` を使用する。

```sh
cargo xtask check --root ../other-repo
cargo xtask check --no-write
cargo xtask check --help
```

target・feature・cfgや既知macroの解析設定は、policyファイルの `[analysis]` に記述する。

```toml
[analysis]
features = ["app/feature"]
cfg = ["custom_cfg"]
trusted_attributes = ["serde"]
trusted_derives = ["Deserialize", "Serialize"]
trusted_macros = ["proptest", "prop_assert", "prop_assert_eq"]
```

`trusted_macros` は呼び出しの書かれたパス（`::` 連結、先頭 `::` なし）に完全一致する function-like macro の展開・tokenを解析しない仮定で、生成・内包される参照はsource-level解析の外になる。macroパス自体の参照は従来どおり解決する。crate内の `macro_rules` の呼出しにも適用され、定義moduleへの参照を記録する。item位置の場合、呼出しを書いたmoduleがopaqueになり、そのmodule内の未解決名は呼出しmoduleへの参照として扱う（crate rootで呼べばcrate全体の未解決 `crate::` pathが対象）。

crate内の `macro_rules` の名前解決は、bare名の呼出しでは呼出しmodule・祖先moduleの定義（`#[macro_use] mod` 経由を含む）を定義と呼出しの前後順を見ない近似で探し、`#[macro_export]` は定義moduleによらずcrate rootの名前として扱い、`crate::name!` の呼出しと `use crate::name` で参照できる。

省略時はhost target・Cargo既定feature・追加cfgなしで解析する。
`target`、`all_features`、`default_features` も指定できる。役割文字数は `[limits] max_role_chars` で変更する。
Rust APIは `run(&Config)` で同じ処理を実行する。`Config.analysis = Some(AnalysisSettings { ... })` は
policyの解析設定全体を、`Config.max_role_chars = Some(n)` は文字数上限を上書きする。
`Config.output = Some(path)` はpolicyの出力先を上書きし、`Config.no_write = true` はCIと同じ検査を行う。書き込み時は古いmodule図を自動削除するため、`Config.prune` は互換性のためのflagで、明示的に有効にする必要はない。
出力不要の解析だけを行う場合はRust APIの `analyze(&Config)` を使用する。

## 解析・実行上の注意

- Rust 2018 / 2021 / 2024に対応。通常構成と `cfg(test)` を別々に解析する。
- `syn` による静的解析のため、型推論やmacro展開を完全には再現しない。未対応の構文・展開は原則エラーとする。
- 組込みのinert属性・derive helper（`#[default]`）とtool属性（`rustfmt`/`clippy`/`diagnostic`/`rust_analyzer`）は展開なしで扱う。
- Cargo依存解決は `--locked --offline`。事前にlockfileと依存キャッシュを準備する。
- SVG配置にはNode.jsが必要。`PATH` または `XTASK_NODE` で指定する。
- `elkjs 0.12.0` を同梱し、実行時のnpm・ネットワークアクセスは不要。

## 実行時間の計測

targetごとの解析とファイルごとの行数検査は、利用可能なCPU数に応じて最大8workerで並列実行し、結果を入力順に集約する。hostのrustc cfgは実行内で再利用する。SVG生成では一回の `run` 内でNodeプロセスと同梱ELKの読み込みを共有し、各図の配置と検証は独立して行う。連続したSVG出力を行うRust APIの利用者は `xtask_graph::SvgWriter`、配置だけを行う利用者は `xtask_layout::LayoutSession` を使用できる。

`benchmark` exampleで、ビルド時間を除く `analyze`、検査のみの `no-write`、SVG生成を含む `write` を計測できる。OUTPUTは絶対パスを指定すると、対象リポジトリの既存図を変更せずに測定できる。検査エラーがあれば非ゼロ終了する。

```powershell
cargo build --locked --offline --release -p xtask-core --example benchmark
target/release/examples/benchmark.exe W:/w/akeboshi-xtask-test W:/w/xtask-core/target/benchmark-output no-write
target/release/examples/benchmark.exe W:/w/akeboshi-xtask-test W:/w/xtask-core/target/benchmark-output write
```

2026-10-14に `W:/w/akeboshi-xtask-test` で測定した結果。改善前はmainの `ba6ac15`、改善後は解析・行数検査の並列化、host cfg再利用、Nodeランタイム再利用を適用した版。Windows、Ryzen 9 3900X（12 core / 24 thread）、Rust 1.98.1、Node 20.18.0、releaseビルドを使用し、各modeを別プロセスで3回実行した。コンパイルは計測外、ファイルキャッシュはクリアしていない。対象のpolicy（図は `lib-bin`）は変更していない。26 crate / 201 target・構成を検査し、全実行でエラー0、`write` は53 SVGを出力した。

| mode | 改善前の中央値 | 改善後の中央値 | 速度比 | 時間短縮 |
| --- | ---: | ---: | ---: | ---: |
| no-write | 35.268秒 | 9.360秒 | 3.77倍 | 73.5% |
| write | 61.660秒 | 17.779秒 | 3.47倍 | 71.2% |

測定値（秒）は、改善前 `no-write = [39.880264, 35.267826, 33.368537]`、`write = [61.660461, 61.386531, 62.229156]`、改善後 `no-write = [9.438426, 9.359520, 8.952320]`、`write = [17.778657, 17.990855, 17.667342]`。別途、解析のみは32.635416秒から9.421555秒（各1回）。改善前後の53 SVGは相対パスとSHA-256がすべて一致した。計測バイナリ・生ログ・JSON集計は `target/benchmarks/` に保存している。

## 本リポジトリの検証

```sh
cargo test --locked --offline --workspace
cargo fmt --all -- --check
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
cargo xtask check
```
