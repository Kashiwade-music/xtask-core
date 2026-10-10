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

## コマンド

```sh
cargo xtask check           # ルール検査
cargo xtask deps-graph      # crate依存図
cargo xtask modules-graph   # module階層・参照図
cargo xtask graphs          # 全SVG生成
```

`check` はSVGを変更しない。module解析に失敗・循環があれば非ゼロ終了する。

SVGの出力先は既定で `architecture/`。

```text
architecture/
├── crates.svg
└── modules/<package>/
    ├── <target>-<kind>-normal.svg
    └── <target>-<kind>-test.svg
```

主なオプション：

- `--all-features` / `--no-default-features` / `--features`：feature指定
- `--target` / `--cfg`：target・cfg指定
- `--no-tests`：テスト構成を省略
- `--root` / `--policy` / `--output`：パス変更
- `--max-role-chars`：役割文字数の上限を上書き
- `--trust-attribute` / `--trust-derive`：既知のmacro展開を信頼

## 解析・実行上の注意

- Rust 2018 / 2021 / 2024に対応。通常構成と `cfg(test)` を別々に解析する。
- `syn` による静的解析のため、型推論やmacro展開を完全には再現しない。未対応の構文・展開は原則エラーとする。
- Cargo依存解決は `--locked --offline`。事前にlockfileと依存キャッシュを準備する。
- SVG配置にはNode.jsが必要。`PATH` または `XTASK_NODE` で指定する。
- `elkjs 0.12.0` を同梱し、実行時のnpm・ネットワークアクセスは不要。

## 本リポジトリの検証

```sh
cargo test --locked --offline --workspace
cargo fmt --all -- --check
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
cargo xtask check --trust-derive Deserialize --trust-derive Serialize --trust-attribute serde
cargo xtask graphs --trust-derive Deserialize --trust-derive Serialize --trust-attribute serde
```
