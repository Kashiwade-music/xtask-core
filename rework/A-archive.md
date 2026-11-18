# 付属書 A：図のファイル（`xtask-archive`）と図の組立て（`xtask-diagram`）

設計書 5.2 節と 5.4 節の契約を実装する手順。

---

## 1. `xtask-archive`（unit A1）

新しい crate `crates/xtask-archive`。図のファイルの型と、符号化・復号・圧縮・文字の寸法の規則だけを持つ。I/O（ファイルの読み書き）はしない。xtask 側（Rust 1.85）と viewer 側（Rust 1.98.1）の両方から使う。

- **所有**：
  - `crates/xtask-archive/**`
  - root の `Cargo.toml` の `[workspace.dependencies]` の `miniz_oxide` と `xtask-archive` の 2 行。M1 も同じ節に別の行を足す。作業指示書 1.3 節の規則で merge する。
- **確認**：
  ```bash
  cargo check -p xtask-archive --all-targets
  cargo test -p xtask-archive
  cargo clippy -p xtask-archive --all-targets -- -D warnings
  ```

### 1.1 `Cargo.toml`

```toml
[package]
name = "xtask-archive"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
miniz_oxide.workspace = true

[lints]
workspace = true
```

root の `Cargo.toml` の `[workspace.dependencies]` に、次の 2 行を足す。

```toml
miniz_oxide = "0.8"
xtask-archive = { path = "crates/xtask-archive" }
```

### 1.2 ファイル

| ファイル | 内容 |
|---|---|
| `src/lib.rs` | Role、`mod model; mod codec; pub mod style;`、`pub use model::*`、`encode`、`decode`、`DecodeError`。`style` は必ず `pub mod` にする（利用側は `xtask_archive::style::measure` の path で使う） |
| `src/model.rs` | 1.3 節の型 |
| `src/codec.rs` | 1.5 節の書き手（`Writer`）と読み手（`Reader`） |
| `src/style.rs` | 1.6 節の文字の規則と描画の定数 |
| `src/tests.rs` | 1.7 節の試験（`#[cfg(test)] mod tests;`） |

### 1.3 型（`src/model.rs`、このとおりに書く）

```rust
//! Role: 図のファイルが持つ図・要素・経路の型を定義する。

/// 図のファイルの全体。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Archive {
    /// リポジトリの directory の名前（window の題に使う）。
    pub root_name: String,
    /// 先頭は必ず crate 依存図。続けて module 図を (package, target, kind, mode) の昇順に並べる。
    pub diagrams: Vec<Diagram>,
    /// 生成時の通知（解析の誤りなど）。
    pub notices: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagram {
    pub kind: DiagramKind,
    pub title: String,
    pub content: Content,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagramKind {
    Crates,
    Modules {
        package: String,
        target: String,
        /// 表示用の target の種類（例 "lib"、"bin"、"lib+rlib"）。
        kind: String,
        mode: Mode,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    Normal,
    Test,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    Drawn(Scene),
    /// 解析または配置に失敗した理由。
    Failed(String),
    /// 解析しなかった理由（required-features が無効など）。
    Skipped(String),
}

/// 配置済みの図。座標は px、左上が (0, 0)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scene {
    pub width: i32,
    pub height: i32,
    /// 操作の単位（crate または module）。
    pub items: Vec<Item>,
    pub nodes: Vec<Node>,
    pub regions: Vec<Region>,
    pub edges: Vec<Edge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// crate 名、または module の path（`crate::a::b`）。
    pub name: String,
    pub role: Option<String>,
    /// ダブルクリックで開く図の index。
    pub open: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub item: u32,
    pub rect: Rect,
    /// 折り返し済みの文字の行。
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    pub item: u32,
    pub parent: Option<u32>,
    pub rect: Rect,
    /// 見出しの行（折り返し済み）。
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    /// node の index。
    pub from: u32,
    pub to: u32,
    pub style: EdgeStyle,
    /// 直交の折れ線。始点の右辺から出て終点の左辺に入る。
    pub points: Vec<Point>,
    pub label: Option<Label>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeStyle {
    Normal,
    Dev,
    Build,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub rect: Rect,
    pub lines: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}
```

### 1.4 ファイルの形式

**ファイル全体**（整数はすべて little-endian）：

| offset | 大きさ | 内容 |
|---|---|---|
| 0 | 8 | magic `b"XTASKMAP"` |
| 8 | 4 | 形式の版 `u32`。現在は `1` |
| 12 | 8 | 本体の長さ（圧縮前）`u64` |
| 20 | 残り | 本体を `miniz_oxide::deflate::compress_to_vec_zlib(&body, 6)` で圧縮したもの |

**本体の文法：**

```
Archive  := str(root_name) vec<Diagram> vec<str>(notices)
Diagram  := Kind str(title) Content
Kind     := u8(0)                                         -- Crates
          | u8(1) str(package) str(target) str(kind) Mode -- Modules
Mode     := u8(0) | u8(1)                                 -- Normal | Test
Content  := u8(0) Scene | u8(1) str | u8(2) str           -- Drawn | Failed | Skipped
Scene    := i32(width) i32(height) vec<Item> vec<Node> vec<Region> vec<Edge>
Item     := str(name) opt<str>(role) opt<u32>(open)
Node     := u32(item) Rect vec<str>(lines)
Region   := u32(item) opt<u32>(parent) Rect vec<str>(lines)
Edge     := u32(from) u32(to) Style vec<Point> opt<Label>
Style    := u8(0) | u8(1) | u8(2)                         -- Normal | Dev | Build
Label    := Rect vec<str>
Rect     := i32 i32 i32 i32                               -- x y width height
Point    := i32 i32
str      := u32(バイト数) UTF-8 のバイト列
vec<T>   := u32(要素数) T の並び
opt<T>   := u8(0) | u8(1) T
```

### 1.5 符号化と復号（`src/codec.rs`、`src/lib.rs`）

```rust
pub fn encode(archive: &Archive) -> Vec<u8>
pub fn decode(bytes: &[u8]) -> Result<Archive, DecodeError>

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// magic が違う、またはファイルが 20 バイトより短い。
    NotArchive,
    /// 読めない版。
    Version { found: u32 },
    /// 本体の長さが上限（256 MiB）を超える。
    TooLarge,
    /// 圧縮・文法・参照の誤り。文は最初の誤り。
    Corrupt(String),
}
```

`DecodeError` には `Display` と `std::error::Error` を実装する。文面は次のとおり。

| variant | 文面 |
|---|---|
| `NotArchive` | `not an xtask architecture file` |
| `Version` | `unsupported architecture file version {found}; this program reads version 1` |
| `TooLarge` | `architecture file is too large` |
| `Corrupt` | `corrupt architecture file: {文}` |

**`Writer`：**
- `Vec<u8>` に書く。method は `u8`、`u32`、`i32`、`str`、`vec(len)`、`opt(present: bool)` の 6 つ。
- 型ごとの書き出しは、`fn write_scene(writer: &mut Writer, scene: &Scene)` のような自由関数にする。
- `usize` から `u32` への変換は `u32::try_from(...)` で行う。超えたら panic でよい。xtask が作る値は小さい。

**`Reader`：**
- `&[u8]` と位置を持つ。各 method は `Result<_, DecodeError>` を返す。
- 足りないバイトは `Corrupt("unexpected end")` にする。
- `vec` の要素数は、残りのバイト数を超えてはならない。超えたら `Corrupt("length out of range")` を返す。巨大な確保を防ぐため、要素は 1 バイト以上と数える。
- 文字列は `String::from_utf8` で検査する。
- 未知の tag は `Corrupt("unknown tag {n}")` にする。

**`decode` の手順：**
1. 長さが 20 未満か magic が違えば `NotArchive`。
2. 版が 1 でなければ `Version`。
3. 本体の長さが `256 * 1024 * 1024` を超えれば `TooLarge`。
4. `miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&bytes[20..], 長さ)` で戻す。誤りは `Corrupt("bad compression")`。戻した長さが宣言と違っても `Corrupt("bad compression")`。
5. 本体を読み、最後まで消費したことを確かめる。余りがあれば `Corrupt("trailing bytes")`。
6. 参照を検査する。誤りは `Corrupt("bad reference in diagram {i}")` にする。
   - `Item.open < diagrams.len()`
   - `Node.item < items.len()`、`Region.item < items.len()`
   - `Region.parent` は自分の index より小さい（親が子より前に並ぶ。閉路を防ぐ）
   - `Edge.from < nodes.len()`、`Edge.to < nodes.len()`
   - `Edge.points.len() >= 2`

`decode` は、どんな入力でも panic しない。

### 1.6 文字と描画の規則（`src/style.rs`）

xtask 側の寸法の計算と、viewer 側の描画で同じ値を使う。現在の `xtask-layout/src/model.rs` と `prepare.rs` から移す。

```rust
//! Role: 図の文字の大きさ・行の高さ・字幅の規則と、折り返しを定義する。

pub const NODE_FONT_SIZE: f64 = 14.0;
pub const EDGE_FONT_SIZE: f64 = 14.0;
pub const REGION_FONT_SIZE: f64 = 12.0;
pub const TITLE_FONT_SIZE: f64 = 18.0;
pub const LINE_HEIGHT: f64 = 18.0;
pub const REGION_LINE_HEIGHT: f64 = 16.0;
/// 描画の font の候補。SVG の font-family と viewer の TextFormat.family に使う。
pub const FONT_FAMILY: &str = "Consolas, Noto Sans Mono, monospace";

/// 字幅の見積り。実際の等幅 font より広めにして、はみ出しを防ぐ。
pub fn advance(character: char, size: f64) -> f64 {
    if character.is_ascii() { size * 0.62 } else { size * 1.05 }
}

/// 折り返した文字の塊。
#[derive(Clone, Debug, PartialEq)]
pub struct TextBlock {
    pub lines: Vec<String>,
    pub width: f64,
    pub height: f64,
}

/// `\n` で行を分け、各行を `max_width` で折り返す。
pub fn measure(text: &str, size: f64, line_height: f64, max_width: f64) -> TextBlock
```

`measure` の本体は、`xtask-layout/src/prepare.rs` の `measure` と同じ手順にする。ただし `width` は折り返しの途中で行ごとに数えた幅の最大を使い、二度数えない。

### 1.7 試験（`src/tests.rs`）

- **`round_trip`**：crate 図（item 2 個、node 2 個、edge 1 本、ラベル付き）と、Failed、Skipped の図を持つ Archive が、`decode(encode(a)) == a` を満たす。
- **`encode_is_deterministic`**：同じ Archive を 2 回 encode すると同じバイト列になる。
- **`rejects_other_files`**：`b"hello"` は `NotArchive` になる。版を 2 に書き換えたものは `Version { found: 2 }` になる。
- **`rejects_bad_reference`**：`Edge.to` を node の数にしたものは `Corrupt` になる。
- **`truncation_never_panics`**：正しいファイルの長さ 0 から n − 1 までのすべての切り詰めで、`decode` が `Err` を返す。
- **`measure_wraps`**：`measure("abcdef", 10.0, 18.0, 20.0)` の行が `["abc", "def"]`、高さが 36.0 になる。

---

## 2. `xtask-diagram`（unit A2）

新しい crate `crates/xtask-diagram`。解析の結果から `Archive` を組み立てる。現在の `xtask-core/src/graphs.rs`、`xtask-graph/src/adapter.rs`、`xtask-layout/src/prepare.rs` の役割を引き継ぐ。

- **所有**：`crates/xtask-diagram/**`
- **確認**：
  ```bash
  cargo check -p xtask-diagram --all-targets
  cargo test -p xtask-diagram
  cargo clippy -p xtask-diagram --all-targets -- -D warnings
  ```
- 古い実装（`graphs.rs`、`prepare.rs`）は、W5 まで消されないので読める。写すときは関数名で探す。

### 2.1 `Cargo.toml` の依存

`xtask-archive`、`xtask-layout`、`xtask-modules`、`xtask-workspace`（すべて `.workspace = true`）。

### 2.2 公開 API（`src/lib.rs`）

```rust
//! Role: 解析結果から図の配置要求を作り、配置して図のファイルの型へ組み立てる。

/// 一つの target・構成の解析の結果。
pub enum Outcome<'a> {
    Analyzed(&'a xtask_modules::ModuleGraph),
    Failed(String),
    Skipped(String),
}

pub struct TargetInput<'a> {
    pub package: &'a str,
    pub target: &'a str,
    pub kinds: &'a [xtask_workspace::TargetKind],
    pub mode: xtask_archive::Mode,
    pub outcome: Outcome<'a>,
}

pub struct Input<'a> {
    pub root_name: &'a str,
    pub workspace: &'a xtask_workspace::Workspace,
    /// 解析したすべての target・構成（図を作らない target も含む。crate の Role を取るため）。
    pub targets: Vec<TargetInput<'a>>,
    /// 図を作るかどうか。`targets` と同じ順・同じ長さ。
    pub selected: Vec<bool>,
    pub notices: Vec<String>,
}

/// package の主な target の名前（2.4 節の開き先の選び方）。xtask-core もこれを使う。
pub fn primary_target(package: &xtask_workspace::Package) -> Option<&xtask_workspace::Target>;

/// 配置の前の図。並列に `draw` できる。
pub struct Plan { /* private */ }

/// 図を並べ、配置の要求を作る。結果の順は Archive.diagrams の順。
pub fn plan(input: &Input<'_>) -> Vec<Plan>;

/// 一つの図を配置する。配置に失敗したら Content::Failed にする。
pub fn draw(plan: &Plan) -> xtask_archive::Diagram;

/// 配置の worker が panic したときの図（kind と title は plan のもの）。
pub fn failed(plan: &Plan, message: String) -> xtask_archive::Diagram;
```

`Plan` の中身（private）：

```rust
pub struct Plan {
    kind: xtask_archive::DiagramKind,
    title: String,
    body: Body,
}

enum Body {
    Graph(Drawing),
    Failed(String),
    Skipped(String),
}

struct Drawing {
    graph: xtask_layout::Graph,
    items: Vec<xtask_archive::Item>,
    /// node ごとの (item, lines)。
    nodes: Vec<(u32, Vec<String>)>,
    /// region ごとの (item, lines)。parent は graph から取る。
    regions: Vec<(u32, Vec<String>)>,
    /// edge ごとの (style, ラベルの lines)。
    edges: Vec<(xtask_archive::EdgeStyle, Option<Vec<String>>)>,
}
```

### 2.3 図の順と題

1. `diagrams[0]` は crate 依存図にする。題は `Workspace crate dependencies (consumer -> provider)`。
2. 続けて、`selected` が true の `targets` を `(package, target, kind, mode)` の昇順に並べる（Normal が Test より先）。
   - `kind` は `kinds` の `as_str()` を `+` でつないだ文字列で、`DiagramKind::Modules.kind` にも使う。
   - 題は `{package}::{target} ({kind}, {normal|test}): module dependencies`。
3. **crate の Role**：package ごとに `primary_target` の target の、Normal の `Outcome::Analyzed(graph)` の root module（`parent` が None の module）の role（`ok()`）を使う。無ければ None。図を作らない target でも使う。
4. 並べ終えてから、crate の item の `open` を決める（2.4 節）。

### 2.4 crate 依存図

- **node**：package を名前の昇順に並べ、一つずつ作る。
  - 文字は `"{name}\n{role}"`。role が無ければ `[役割未記載]` を入れる。
  - `style::measure(文字, NODE_FONT_SIZE, LINE_HEIGHT, 320.0)` で測る。
  - 大きさは `width = max(ceil(w) + 28, 96)`、`height = max(ceil(h) + 24, 42)`。
- **列**：normal の依存だけで、消費側から最長の距離を求める。現在の `graphs.rs` の `normal_ranks` を移す。
  - 同じ列の node の幅を、その列の最大にそろえる。
  - `Layering::Fixed(列)` にする。
- **列の閉路**：Kahn 法で残った node から、残りの中で出次数 0 の node を繰り返し除く。残った node だけを、名前の昇順に `cannot rank cyclic normal crate dependencies: a, b` として `Body::Failed` にする。
- **edge**：`workspace.edges` の順に作る。style とラベルは次のとおり。

  | kind | style | ラベル |
  |---|---|---|
  | "normal" | Normal | target 条件があればその文 |
  | "dev" | Dev | `"dev"`、target 条件があれば `"dev: {target}"` |
  | それ以外 | Build | kind、target 条件があれば `"{kind}: {target}"` |

  ラベルは node と同じ規則で測る（最大幅 320）。大きさは `ceil` する。
- **item**：package ごとに一つ（node と同じ順）。
  - name は package 名、role は 2.3 節の crate の Role。
  - open は、`primary_target(package)` の target の Normal の module 図の index。その図が無ければ None。
- **`primary_target`**：現在の `graphs.rs` の `primary_target` を移す。
  - library の target（`TargetKind::is_library`）を `(name, src_path)` の昇順で最初に取る。
  - 無ければ bin の target を、package 名と同じ名前を先に、残りは `(name, src_path)` の昇順で取る。
  - どちらも無ければ None。

### 2.5 module 図

`Outcome::Failed(s)` は `Body::Failed(s)` に、`Outcome::Skipped(s)` は `Body::Skipped(s)` にする。`Outcome::Analyzed(graph)` は次のとおり。

- **item**：`graph.modules` の順に一つずつ作る。name は module の id、role は `module.role.clone().ok()`（付属書 M の M3 で `Result<String, String>`）、open は None。
- **region**：module ごとに一つ（同じ順）。
  - parent は親の module の index。
  - 見出しの文字は `"{id}\n{role}"`。role が無ければ `[役割未記載]` を入れる。
  - `measure(文字, REGION_FONT_SIZE, REGION_LINE_HEIGHT, 200.0)` で測り、header は `(ceil(w), ceil(h))`。
- **node**：module ごとに `(self)` の node を一つ作る。region はその module。大きさは crate 図と同じ規則で、文字 `"(self)"` から求める。
- **edge**：`graph.edges` の順に作る。from と to は module の index の node、style は Normal、ラベルは無し。from == to のものは除く。
- **層**：`Layering::Optimize`。

`ModuleGraph` の module の親の検査（現在の `module_clusters` の重複・欠落・閉路の検査）は書かない。解析が保証する（付属書 M の M3）。`graph.modules` は id の昇順なので、親は必ず子より前に並ぶ（1.5 節の `decode` の条件を満たす）。

### 2.6 `draw`

1. `xtask_layout::layout(&graph)` を呼ぶ。`Err(e)` なら `Content::Failed(e.to_string())` にする。
2. 成功したら `Scene` を作る。
   - width と height は `layout.size`。
   - node は `(item, layout.nodes[i] を Rect に, lines)`。
   - region は `(item, parent, rect, lines)`。
   - edge は `(from, to, style, points, label)`。label の rect は `layout.edges[i].label`、lines は Drawing のもの。
3. `Body::Failed` と `Body::Skipped` は、そのまま Content にする。

### 2.7 試験

- **`crate_plan_orders_and_links`**：package 2 個（a が b に normal で依存）と a の lib の module 図 1 枚から、次が成り立つ。
  - diagrams の 0 番が Crates、1 番が a の Normal。
  - item a の open が `Some(1)`。
  - 列が a=0、b=1。
- **`module_plan_has_region_per_module`**：module 3 個（crate、crate::x、crate::y）の graph から、region が 3 個、node が 3 個、crate::x の region の parent が crate の index になる。
- **`cyclic_ranks_fail_with_members`**：a→b、b→a、c→a（すべて normal）で、Failed の文が `a, b` だけを挙げる（c を含まない）。
- **`draw_produces_scene`**：上の module 図の plan を draw すると `Content::Drawn` になり、node の数が 3 になる。

この試験は `xtask_workspace::Workspace` を手で作る必要がある。`Workspace` の field が公開されていればそれを使う。公開されていなければ STOP する。付属書 M の M1 で `Workspace` の構成を変えるため、A2 は M1 の後の wave に置いている。
