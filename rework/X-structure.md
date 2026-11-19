# 付属書 X：module の参照の閉路の解消（CP の差戻し 19・20）

CP で、xtask 自身の検査（`cargo run -p xtask-core -- check`）が module の参照の閉路を 12 件報告した（差戻し 20）。原因は設計にある：付属書が、型・定数・共有の関数を crate root（`lib.rs`）や、他の module を呼ぶ module に置き、そこを下位の module から参照させていた。本付属書は、閉路を作らない規則（設計書 6.3 節の 6、DEC-24）と、crate ごとの移し方を定める。あわせて差戻し 19（clippy の `large_enum_variant`）を直す。

## 1. 規則（設計書 6.3 節の 6）

1. crate root（`lib.rs`）は、`mod`・`pub use` と、他の module の関数を呼ぶだけの公開の関数だけを持つ。型・定数・共有の関数を定義しない。
2. crate root が参照する module（`pub use` する module、関数を呼ぶ module）は、crate root の項目を参照しない。`use crate::X`（X が `lib.rs` で定義または再 export されたもの）を書かず、定義した module の path（`use crate::model::X`）で参照する。
3. 二つの module が互いの項目を要るときは、共有する項目を、どちらも参照しない葉の module（多くは `model.rs`）に移す。
4. 試験の module（`#[cfg(test)] mod tests`）は、crate root を参照してよい（crate root は試験の module を参照しないので閉路にならない）。

移すのは定義の場所だけで、型・関数の中身と公開の名前（`pub use` による外からの path）は変えない。

## 2. crate ごとの移し方

各 unit の「確認」は、その crate の `cargo check --all-targets`、`cargo test`、`cargo clippy --all-targets -- -D warnings` と、リポジトリの検査の該当の行が消えること（`cargo run -p xtask-core -- check 2>&1 | grep "<crate 名>"` に `module reference cycle` が無い）。

### X1：`xtask-archive`（閉路：crate ↔ codec）

- `lib.rs` の `DecodeError`（`Display` と `Error` の impl を含む）、`MAX_BODY_LEN`、`MAX_FILE_LEN` を `model.rs` に移す。
- `codec.rs`、`validate.rs`、`style.rs` の `use crate::{..}` のうち crate root の項目は、`crate::model::..` に替える。
- `lib.rs` は `pub use model::*;` で公開の path を保つ。`encode`・`decode` は `lib.rs` に残してよい（`codec` と `validate` を呼ぶだけにする）。

### X2：`xtask-diagram`（閉路：crate ↔ crate_graph、module_graph、scene）

- `crate_graph.rs`、`module_graph.rs`、`scene.rs`、`text.rs` が `use crate::{..}` で使っている `lib.rs` の項目（`Outcome`、`TargetInput`、`Input`、`Plan`、`Body`、`Drawing`、`OpenIndices`、`analyzed_root_role`、`key` など）を、新しい `model.rs`（`//! Role: 図の組立ての入力・計画・下書きの型と、それらの小さな helper を定義する。`）に移す。移す関数が `lib.rs` の他の関数を呼ぶなら、その関数も移す。
- `lib.rs` は `pub use model::{Input, Outcome, Plan, TargetInput};` で公開の path を保ち、`primary_target`・`plan`・`draw`・`failed` は他の module を呼ぶだけにする（`crate_graph` などが `primary_target` を使うなら、`primary_target` も `model.rs` に移して `pub use` する）。
- `model.rs` は他の module（`crate_graph`、`module_graph`、`scene`）を参照しない。`text.rs` は参照してよい（`text.rs` が `model.rs` を参照しないこと）。

### X3：`xtask-layout`（閉路：level ↔ chains、layering、order、position、route）

- `level.rs` の `run_level` を、唯一の呼出し元の `hierarchy.rs` に移し、private な関数にする（本体は付属書 L の 9.1 節のまま）。
- `level.rs` は型だけを持ち、他の段の module を参照しない。

### X4：`xtask-modules`（閉路：thread_local_items ↔ visitor）

- `visitor.rs` の `local_macro_targets` と `is_local_macro` を `symbols.rs` に移す（名前の解決の helper。visitor の状態を使わない）。
- `thread_local_items.rs` と `visitor.rs` は `crate::symbols::{is_local_macro, local_macro_targets}` を使う。`thread_local_items.rs` は `visitor` を参照しない。
- 移した関数が `visitor.rs` の他の項目を使っていれば、それも `symbols.rs` に移す。`symbols.rs` が `visitor` か `thread_local_items` を参照することになるなら STOP する。

### X5：`xtask-style`（閉路：style ↔ style_source、style_tools、clippy、probe）

- `style.rs` の `StyleInput` と `enabled` を `model.rs` に移す。
- `style_source.rs`、`style_tools.rs`、`style_tools/clippy.rs`、`style_tools/probe.rs` は `crate::model::{StyleInput, enabled}` を使い、`crate::style` を参照しない。
- `lib.rs` の公開の path（`StyleInput` など）は `pub use model::..` で保つ。

### X6：`xtask-render`（viewer。閉路：crate ↔ bridge、build、highlight、palette、route）

- `lib.rs` の型（`P`、`R`、`Rgb`、`Cmd`、`Text`、`NodeShape`、`RegionShape`、`EdgeShape`、`Bridge`、`Picture`、`Highlight`）を、新しい `model.rs`（`//! Role: 描画の一覧の点・矩形・色・形の型を定義する。`）に移す。
- `bridge.rs`、`build.rs`、`highlight.rs`、`palette.rs`、`route.rs` は `crate::model::..` を使う。
- `lib.rs` は `pub use model::*;` と、`picture`・`neighbors`・`item_rect` を、それぞれの module の関数を呼ぶだけの関数として持つ（`palette` は `pub mod` のまま）。
- 確認は `(cd viewer && cargo check -p xtask-render --all-targets && cargo test -p xtask-render && cargo clippy -p xtask-render --all-targets -- -D warnings)` と、`cargo run -p xtask-core -- check --root viewer` に閉路が無いこと。

### X7：`xtask-core` の `Command`（差戻し 19）

- 設計書 5.1 節の `Command` を `pub enum Command { Help, Check(Box<Config>) }` に改めた。`cli.rs` の `parse_args` は `Command::Check(Box::new(config))` を返し、`main` は `Command::Check(config)` の `config` を `&config`（`&Config` に自動で deref される形）で `run` に渡す。
- 試験の `Command::Check(config)` の pattern は、`*config` や `config.root` で中を見る形に直す（期待値は変えない。API の追従）。
- `#[allow]` は足さない。

## 3. 進め方（CP の fix）

X1〜X7 は互いに別の crate のファイルだけを変えるので、並行の fix lane にできる。最大 6 lane：X1 と X7 を同じ lane（順に）、X2、X3、X4、X5、X6 をそれぞれの lane。全部を merge した後、CP（作業指示書 2.3 節）を最初から実行する。

AC-1 の `check --write` が終了コード 1 だったのは、この閉路の違反のため。閉路が消えれば、同じ手順で終了コード 0 になることを CP で確かめる。
