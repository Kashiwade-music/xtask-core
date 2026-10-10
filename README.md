# xtask-core

## これはなに

Rust プロジェクトの xtask として使える共通ルール。

下記のようにして利用する。

```plaintext
各リポジトリ/
├─ vendor/xtask-core/             ← 共通リポジトリの submodule
└─ dev/xtask/
   ├─ Cargo.toml                 ← xtask-core への path 依存
   ├─ src/main.rs                ← リポジトリ固有の設定を渡すだけ
   └─ policy/workspace.toml      ← そのリポジトリの規則
```

## ルール一覧

- コメント・文字列を除外する1ファイルあたりのコード行数が1000行以下
- crate単位の依存解析とSVG書き出し
- `visible_to`: 当該crateに依存できるcrateの許可リスト検査
- 各crate内モジュールの階層構造と参照構造のSVG書き出し
- 各crate内モジュールの参照構造の循環禁止

## SVG書き出しについて

ELKで配置と配線を計算し、その座標から独自のSVG描画実装で出力します。
出力ファイルはSVGのみです。
crate依存図とmodule依存図は、どちらも左から右へ配置します。
矢印の経路は水平・垂直の線で配線し、SVGでは曲がり角を小さく丸めます。
矢印は矩形の左右辺から出入りし、上辺・下辺には接続しません。
線どうしの交差は片方に半円の跨ぎを付け、接続点と区別します。
devの波線も跨ぎの区間を保ち、共有端点や平行な線の重なりには跨ぎを付けません。

crate依存図はconsumerからproviderへの矢印を描きます。通常依存は実線で、種類のラベルを付けません。
dev依存は `dev` ラベル付きの波線です。通常依存だけを使い、依存元からの最長距離で階層を決めます。
複数のcrateが同じproviderを利用する場合は木ではなくDAGになります。
dev・build依存は階層に影響させません。build依存は `build` ラベル付きの実線です。
target条件付きの辺には条件も表示します。
crate図では各階層内の縦配置と、階層間の配置・配線をELKで順に計算します。
階層を不可視の列として固定するため、dev・build依存を追加してもcrateの所属列は変わりません。
接続数に応じて矩形の高さを広げ、左右辺の接続点が密集しないようにします。

module依存図は各moduleの領域を親moduleの領域内に入れ子で配置します。
各領域の `(self)` ノードはそのmodule自身を表し、参照を実線矢印で結びます。
階層の親子関係は領域の包含で表し、参照と一枚の図にまとめます。
角の丸め、devの波線、交差点の跨ぎはELKが返す経路に描画処理を加えます。
SVGを読み直して経路や接続位置を補正する処理は行いません。

## 構成

| crate | 責務 |
| --- | --- |
| `xtask-core` | 利用者向けAPI、CLI、検査と出力の実行 |
| `xtask-workspace` | Cargo metadata、crate依存、policy、feature解決 |
| `xtask-source` | Rustの字句解析による物理コード行数とファイル探索 |
| `xtask-modules` | Rust構文木、モジュール探索、名前解決、循環検出 |
| `xtask-graph` | グラフモデル、配置要求の検証、配置と描画の実行 |
| `xtask-layout` | ELKによる配置・配線、ラベル寸法、座標モデル |
| `xtask-svg` | 座標からSVGを描画、角の丸め、波線、交差点の跨ぎ |

Kivact / Vexoraft の共通の行数・`visible_to` の意味を引き継ぎ、モジュール解析を追加しています。
各プロジェクト固有のprelude上限、`dependency_allowlist`、`check-all`、Mermaidの更新検査は含めません。

## 利用方法

`dev/xtask/Cargo.toml` の依存を次のように設定し、利用側workspaceのmemberに `dev/xtask` を追加します。

```toml
[package]
name = "xtask"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
xtask-core = { path = "../../vendor/xtask-core/crates/xtask-core" }
```

`dev/xtask/src/main.rs` はリポジトリの場所を明示的に渡します。
共通crate自身の `CARGO_MANIFEST_DIR` を利用側のルートとして扱いません。

```rust
fn main() -> std::process::ExitCode {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap();
    xtask_core::main(root)
}
```

利用側の `.cargo/config.toml`:

```toml
[alias]
xtask = "run --package xtask --"
```

`dev/xtask/policy/workspace.toml`:

```toml
[limits]
max_code_lines = 1000

[visible_to]
internal = ["facade", "app-*"]
app = []
```

上限は省略すると1000です。1000を超える設定は拒否し、より厳しい設定は許可します。
`visible_to` はproviderをキーに、依存してよいconsumerのpackage名を指定します。
未記載のproviderは公開、`[]` は自己依存以外禁止、末尾の `*` は前方一致です。
存在しないproviderや、workspaceに一致するconsumerがないパターンもエラーです。
normal・dev・build・optional・target条件付きの宣言依存をすべて検査します。
依存名の別名ではなく、package IDとmanifestの実体で内部依存を判定します。

```shell
cargo xtask check
cargo xtask deps-graph
cargo xtask modules-graph
cargo xtask graphs
```

`check` はpolicy、コード行数、モジュール参照循環を検査し、グラフを書き換えません。
グラフコマンドはSVGを明示的に生成します。
`deps-graph` はモジュール解析やpolicyを必要とせず、workspace内の直接依存を出力します。
module系コマンドは解析不能や循環があれば非ゼロ終了します。取得できたグラフは診断用に出力します。

出力先の既定値は `architecture/` です。

```plaintext
architecture/
├─ crates.svg
└─ modules/<package>/
   ├─ <target>-<kind>-normal.svg
   └─ <target>-<kind>-test.svg          # cfg(test) 構成
```

各crateのlib・bin・test・bench・example・build scriptを別のcrate rootとして解析します。
Cargoの `required-features` と `test = false` を反映します。
ファイル行数は全memberの `src/`・`tests/`・`benches/`・`examples/`・`build.rs`、
独自のCargo targetパス、および解析で到達したmoduleファイルを対象にします。
空行、コメントだけの行、文字列内容だけの行は数えません。
文字列開始・終了の区切りや、その行にある他のRustコードは数えます。
読み取り・字句解析・モジュール探索の失敗を黙って除外しません。

Node.jsをPATHに置くか、`XTASK_NODE` にNode.js実行ファイルのパスを設定します。
ELKは固定バージョンの `elkjs 0.12.0` を同梱し、実行時にnpmやネットワークを使用しません。
同梱ライブラリのライセンスと出典は
[NOTICE.md](crates/xtask-layout/vendor/elkjs/NOTICE.md) に記載しています。
Nodeの起動、ELKの配置、SVG描画が失敗した場合、既存のSVGを保持してエラーを返します。

library APIで実行ファイルを明示する場合は `xtask_graph::write_with_node` または
`write_with_node_and_layout` を利用します。出力パスにはSVGのパスだけを渡します。
配置要求を取得する場合は `Graph::layout_request` を利用します。
既定の配線は左右辺に接続する直交経路です。
`Layout.ranks` は領域を持たない左から右へのcrate図用です。
module図は `Layout.clusters` に領域の親子関係を渡します。

## 解析の範囲と設定

モジュール解析は `syn` の構文木に基づく、ソース上の参照解析です。
Rust 2018 / 2021 / 2024のpath規則を対象とし、その他のeditionは解析エラーにします。
inline / 外部ファイル / `#[path]` の階層を読み、`use` の別名・group・glob・再公開、
`crate`・`self`・`super`、型・式のpathから参照先の所属moduleを求めます。
階層の親子関係と参照の辺は別扱いです。同一module内の参照は循環として数えません。
循環は強連結成分を検出し、相互に到達できるmoduleの集合として報告します。

条件付き構文はCargoで解決したworkspaceのfeature集合と `rustc --print cfg` に基づいて選択します。
通常構成と `cfg(test)` 構成を別々に解析し、互いに排他的な辺を合成しません。
未選択featureや他targetまで検査したい場合は、それぞれの設定で実行してください。

```shell
cargo xtask check --all-features
cargo xtask check --no-default-features
cargo xtask check --features my-crate/extra
cargo xtask check --target x86_64-unknown-linux-gnu
cargo xtask graphs --output docs/graphs
```

`--root`・`--policy`・`--output` でパスを変更できます。
policy/outputの相対パスはリポジトリルート基準です。
`--no-tests` はテスト構成を省略します。
build scriptとproc-macroはhostのcfgを用い、proc-macroの組み込みcfgも反映します。
build scriptによる独自cfgや、RUSTFLAGSによるcfgは
自動収集しないため、`--cfg 'custom_flag'` / `--cfg 'key="value"'` を明示してください。
Cargo metadataが提供するpackage単位のfeature集合を使用し、rustcの全コンパイルunitの
host/target別feature集合や型推論・method dispatchを再現するものではありません。

Cargo feature解決は `--locked --offline` で実行します。
利用側の `Cargo.lock` と依存キャッシュを、事前に `cargo generate-lockfile` / `cargo fetch` で準備してください。
検査中は依存をダウンロードせず、lockfileを更新しません。

`include!`、moduleを生成しうるitem macro、未対応macroのtoken、block内module、
未知のprocedural attribute / deriveなどは解析不完全としてエラーにします。
標準の式macroは対応する引数を構文解析します。任意のtokenをRustのpathとみなす推測はしません。
外部の名前や型推論による参照はRustコンパイラによる検証も必要です。
アクセス可否のprivacy検査や、Rustの名前解決全体の妥当性はコンパイラに委ねます。

展開がmodule構造を変えないと把握しているattribute / deriveについてのみ、
`--trust-attribute path` / `--trust-derive path` を明示できます。
この仮定は実行結果に表示され、生成コード自体の参照は検査対象に入りません。
何も指定しない場合、未知の展開を検査成功として扱いません。
library APIでは `Config` と `run` で同じ設定・構造化された `Report` を利用できます。

module探索のファイル規則は [Rust Reference](https://doc.rust-lang.org/reference/items/modules.html) に従います。

## 本リポジトリの検証

```shell
cargo test --locked --offline --workspace
cargo fmt --all -- --check
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
cargo xtask check --trust-derive Deserialize --trust-derive Serialize --trust-attribute serde
cargo xtask graphs --trust-derive Deserialize --trust-derive Serialize --trust-attribute serde
```

本workspaceのmetadata・座標構造体はSerdeの `Deserialize` / `Serialize` を使うため、自己検査では上記の明示的な仮定を渡します。
元のKivact / Vexoraft側のxtaskやpolicyは変更しません。利用側で採用したルールだけを移行してください。
