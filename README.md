# xtask-core

Rust workspace向けの共通検査・アーキテクチャ可視化ツール。各リポジトリの `dev/xtask` から利用する。

## 機能

- **コード行数**：コメント・文字列内容・空行を除き、1ファイルn行以下
- **依存制限**：`visible_to` によるcrate間の依存許可リスト
- **役割宣言**：全crate root・moduleに `Role:` を必須化
- **循環検出**：module間の参照循環を禁止
- **Rust style検査**：Google Rust Style Guideから採用した20項目を、構文解析・rustfmt・Clippyで検査
- **SVG出力**：crate依存図、module階層・参照図を生成
- **対話型HTML**：無限キャンバスで依存関係を強調し、crateからmodule図へ移動

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
        .parent()
        .expect("xtask must be inside dev")
        .parent()
        .expect("dev must be inside the repository");
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

[style]
enabled = true

[visible_to]
internal = ["facade", "app-*"]
app = []
```

`visible_to` は依存される側をキーに、依存を許可するpackage名を指定する。未指定なら制限なし、`[]` は外部crateからの依存禁止、末尾の `*` は前方一致。normal・dev・build・optional・target条件付き依存も検査する。

行数上限は最大1000、役割文字数の既定値は80（変更可能）。

style検査にはrustfmtとClippyが必要。使用するツールチェーンへ `rustup component add rustfmt clippy` で導入する。
この設定で基本13項目が有効になり、親リポジトリの `cargo xtask check` から自動実行される。既存policyとの互換性のため、`[style]` の省略時は無効。
通常の `cargo build` やPRの作成では実行されない。CIでも `cargo xtask check --no-write` を呼ぶ。

## Rust style検査

[Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html)から、対象を機械的に定義できる検査を採用している。
ガイド全体への準拠を保証するものではない。文書・安全性コメントは存在を検査し、説明の正しさや設計意図はレビューで確認する。

| # | rule ID | 検査内容・範囲 | 実装 |
| --- | --- | --- | --- |
| 1 | `format` | 親リポジトリのrustfmt設定（`cargo fmt`）への一致と、有効な `max_width` が100以下であること。全物理行が100桁以下である保証ではない | rustfmt |
| 2 | `hex-literals` | 数値の16進リテラルを小文字で記述。macro呼出し内のリテラルも対象。文字列・コメントの内容は対象外 | 構文解析 |
| 3 | `import-granularity` | 1つの `use` の波括弧内で異なるmoduleからimportしない。複数の `use` の統合は要求しない | 構文解析 |
| 4 | `unstable-features` | 有効な `#![feature(...)]` を禁止 | rustc `unstable_features` |
| 5 | `unwrap` | `Option` / `Result` の `unwrap` / `unwrap_err` を検出。テストコード（`#[test]` 関数、`#[cfg(test)]` module、`tests/` の統合テスト）は対象外 | Clippy `unwrap_used` |
| 6 | `checked-unwrap` | 値の確認後のunwrapを、Clippyが認識するパターンで検出 | Clippy `unnecessary_unwrap` |
| 7 | `flexible-references` | 不要な `&Box<T>`、`&Vec<T>`、`&String`、`&Option<T>` を検出 | Clippy `borrowed_box` / `ptr_arg` / `ref_option` |
| 8 | `derivable-impls` | deriveで置換できる手書き `Default` など、Clippyの対応範囲を検出 | Clippy `derivable_impls` |
| 9 | `new-default` | 対象となる引数なし公開コンストラクタに `Default` を要求 | Clippy `new_without_default` |
| 10 | `len-is-empty` | 公開 `len()` に対応する `is_empty()` を要求 | Clippy `len_without_is_empty` |
| 11 | `missing-docs` | 公開APIの文書の欠落を検出。自明なAPIなどで省略する場合は例外を指定 | rustc `missing_docs` |
| 12 | `safety-docs` | unsafe APIのSafety節の欠落を検出 | Clippy `missing_safety_doc` |
| 13 | `safety-comments` | unsafe block / implの安全性コメントの欠落を検出 | Clippy `undocumented_unsafe_blocks` |
| 14 | `glob-imports` | glob importを制限。inline moduleの `super::*`、関数本体内の `use Enum::*`（最終segmentがUpperCamelCase）、非leaf moduleからの子moduleの公開再exportは許可 | 構文解析 |
| 15 | `non-leaf-implementation` | 非leaf moduleの実装を制限。module宣言・`use`・test module（`test` / `tests` / `#[cfg(test)]`）・`#[cxx::bridge]` は許可し、結合コードには例外を指定 | 構文解析 |
| 16 | `rustfmt-skip` | `#[rustfmt::skip]`（`rustfmt::skip::macros(...)` などを含む）を制限。生成コード・手動整列には例外を指定 | 構文解析 |
| 17 | `non-exhaustive` | 指定packageの公開enum・全fieldが公開されたstructに `#[non_exhaustive]` を要求 | Clippy `exhaustive_enums` / `exhaustive_structs` |
| 18 | `catch-unwind` | `std::panic::catch_unwind` を制限。防御目的で許可する箇所には例外を指定 | Clippy `disallowed_methods` |
| 19 | `poison-access` | `std::sync::PoisonError` の `into_inner` / `get_ref` / `get_mut` を制限 | Clippy `disallowed_methods` |
| 20 | `options-default` | 指定した公開options型が `Default` を実装することを確認 | Rustコンパイル検査 |

`enabled = true` では1〜13を検査する。14〜20は適用対象や裁量例外の設定が必要なため、`extra_rules` で追加する。
例えば、全20項目を使用するpolicyは次のように記述する。`my-library` と型のpathは実際のpackage・定義に置き換える。

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
api_packages = ["my-library"]
options_structs = ["my-library::options::Options"]
# 使用するツールチェーンを固定する場合:
# toolchain = "1.85.0"

[[style.exceptions]]
rule = "glob-imports"
path = "crates/my-library/src/parser.rs"
reason = "このmoduleは1つのenumのvariantを扱う"

[[style.exceptions]]
rule = "rustfmt-skip"
path = "crates/my-library/src/generated/*"
reason = "生成元の配置を維持する"
```

`api_packages` はpackage名で指定する。`options_structs` は `package::module::Item` の完全一致で指定し、存在しない対象はエラーになる。
`options-default` は別crateから参照できる公開library型が対象で、型引数が必要なgeneric型は指定できない。
外部の一時crateから `Default` 境界をコンパイル検査するため、指定pathは公開APIのpathとする。
検査用lockfileは親リポジトリのlockfileを一時領域へコピーし、その一時領域でのみoffline解決する。親のmanifest・lockfileは変更しない。

例外の `path` は親リポジトリルートからの相対pathで、末尾の `*` のみ前方一致に使用できる。`reason` は必須。
必要に応じて `item = "module::item"` で対象itemも限定できる。例外を指定しても、ツール未導入・設定不正・コンパイル失敗などの実行エラーは免除しない。
選択したlintはソース側の `#[allow(...)]` より優先して検査する。style規則を免除する場合は、このpolicyの理由付き例外を使う。
無効なcfg配下の外部moduleなど、解析結果からmodule pathを復元できないファイルでは、itemの限定を省略してpath単位の例外を指定する。
基本ルールをプロジェクト全体で無効にする場合は、例えば `disabled_rules = ["missing-docs"]` を `[style]` に指定する。

構文検査は探索したRustソースを対象とし、無効なcfg内のコードも検査する。macroのtokenはRust構文として解釈せず（16進リテラルのみ検査する）、生成コードは展開しない。
`format` はプロジェクトのrustfmt設定をそのまま使う。`max_width` を上書きしないため、`cargo fmt` の結果は常に検査を通過する。有効な `max_width` が100を超える場合は、その設定ファイルを違反として報告する。
`flexible-references` の `ref_option` はClippy 1.83以降で利用できる。
Clippyは `[analysis]` のtarget・feature・cfgに従って、workspaceの全targetを検査する。全feature・全targetの組合せを自動的に網羅するものではない。
`analysis.cfg` とCargo設定の `rustflags` を併用する場合は、既存のflagsを含む `RUSTFLAGS` または `CARGO_ENCODED_RUSTFLAGS` を明示する。後者は引数をU+001Fで区切る。既存のCargo flagsを暗黙に上書きする構成は実行エラーにする。
`poison-access` はMutexに限定せず、RwLockなどから得た同じ `PoisonError` APIも対象となる。
Clippyの既存設定は引き継ぎ、今回の禁止APIを加えた一時設定で実行する。ただし規則の対象範囲を固定するため、`allow-unwrap-in-tests = true`、`check-private-items = true`、`avoid-breaking-exported-api = false` は上書きする。診断はrule IDと位置を含めて既存の検査結果へ集約し、違反があれば非ゼロ終了する。
Rust APIでは `Report.style_diagnostics` からrule ID・ファイル・行・列・対象itemを取得できる。同じ違反はCLI互換のため `Report.errors` にも含まれる。

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

`check` 一つでコード行数・依存制限・役割宣言・module参照循環と、有効にしたRust styleルールを検査し、crate依存図と全module図、および対話型HTMLを生成する。
検査違反があっても生成可能な図は出力し、診断をまとめて非ゼロ終了する。図の生成に失敗した場合もエラーとする。
通常構成とCargoで有効なテスト構成は毎回解析する。

SVGとHTMLの出力先はpolicyのトップレベル `output` で指定する。省略時は `architecture/`。
相対パスはリポジトリルート基準で、絶対パスも使用できる（例：`output = "docs/architecture"`）。

```text
architecture/
├── index.html
├── crates.svg
└── modules/<package>/
    ├── <target>-<kind>-normal.svg
    └── <target>-<kind>-test.svg
```

生成された `architecture/index.html` をブラウザで開くと、最初にcrate依存図を表示する。
SVG・CSS・JavaScriptをすべて埋め込んだ単一ファイルなので、HTTPサーバーや追加インストール、ネットワーク接続は不要。

- crate・moduleをホバーすると、その項目に依存する側と、その項目が依存する側を矢印ごと色分けする。矢印は依存元から依存先へ向かう。シングルクリックでハイライトを固定し、マウスを外したりキャンバスをドラッグ・ズームしたりしても維持する。何もない場所をクリックするかEscで固定を解除する。
- crateをダブルクリックするとmodule依存図へ移動する。最初は通常構成のlibrary、なければbinaryを優先し、他のtargetやテスト構成は選択欄から切り替えられる。
- moduleの名前・領域・`(self)`をホバーしても、直接の依存元・依存先を強調する。戻るアイコンや `Crates` リンクで元の図に戻れる。ヘッダーのパスは各階層へのリンクで、package名を押すとそのcrateの既定targetへ移動する。
- 左ペインは全crate・target構成・moduleのツリーを常に保持する。ツリー上でもホバーで依存元・依存先を色分けし、クリックで該当図へ移動して固定できる。表示中の図にかかわらず他のcrate・moduleを選択でき、展開状態とスクロール位置も維持する。
- キャンバスのドラッグで自由に移動、ホイールでカーソル位置を中心に拡大・縮小できる。全体表示とズームボタンも使用できる。
- キーボードではTabで項目へ移動して依存関係を確認し、Spaceでハイライトを固定、Enterでcrateを開ける。

HTMLにも `[diagrams] targets` の選択を適用する。解析・描画に失敗したtargetや、無効なrequired-featuresで省略したtargetの古いSVGはHTMLへ取り込まない。選択対象に図がないcrateではその旨を表示する。検査エラーはHTML内でも確認できる。

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

CIでは `--no-write` でソースや図を変更せずに検査できる。
全ルールと図の構造を検査し、SVG・HTML生成と書き込みを省くためNode.jsは不要。既存SVG・HTMLとの差分検査は行わない。
style検査を有効にした場合もrustfmt / Clippyは実行する。Cargoによる `target/` 以下へのビルド出力や、一時検査ファイルの作成は行う。また、Clippyのコンパイル中にbuild scriptやprocedural macroが実行される。
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
`Config.output = Some(path)` はSVG・HTMLの出力先を上書きし、`Config.no_write = true` はCIと同じ検査を行う。書き込み時は古いmodule図を自動削除するため、`Config.prune` は互換性のためのflagで、明示的に有効にする必要はない。
出力不要の解析だけを行う場合はRust APIの `analyze(&Config)` を使用する。`analyze` はmoduleのsource-level解析APIで、style検査やrustfmt / Clippyの起動は行わない。

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

同日、`a64b666` に追加改善を適用した。属性の無い式・文ではcfgやderiveの診断が不要なので、その判定用の名前表の複製を省く。SVGの交差判定では線分・曲線の外接矩形を事前計算し、確実に離れた組の詳細計算を省く。矩形はパラメータ・距離の許容誤差と浮動小数の丸め誤差を含めて拡張し、従来の判定結果と順序を保つ。

同じ環境・対象・policyで前回版と追加版を交互に3回ずつ実行した結果。全実行で26 crate / 201 target・構成、エラー0。最初の改善前から追加版まで、53 SVGの相対パスとSHA-256はすべて一致した。

| mode | 前回版の中央値（再計測） | 追加版の中央値 | 追加の速度比 | 時間短縮 |
| --- | ---: | ---: | ---: | ---: |
| no-write | 8.906秒 | 1.638秒 | 5.44倍 | 81.6% |
| write | 18.291秒 | 8.200秒 | 2.23倍 | 55.2% |

測定値（秒）は、前回版 `no-write = [8.906362, 8.855301, 8.934785]`、`write = [18.290827, 18.773331, 17.790138]`、追加版 `no-write = [1.638381, 1.654961, 1.634049]`、`write = [8.116300, 8.200194, 8.240718]`。追加版の生ログ・集計は `target/benchmarks/second-*`。空属性の最小変更だけの予備計測では解析のみ1.393798秒だった。別途1回の工程計測では、空属性改善後・交差判定改善前のSVG配置が計6.27秒、描画が計2.34秒だった。

## 本リポジトリの検証

```sh
cargo test --locked --offline --workspace
cargo fmt --all -- --check
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
node --test crates/xtask-core/src/arch_map/viewer.test.cjs
cargo xtask check
```
