# 付属書 L：配置エンジン（`xtask-layout`）

設計書（`REWORK_DESIGN.md`）5.3 節の契約を実装する手順。Node.js と elkjs を使う現在の実装を、Rust だけの配置に置き換える。

## 1. 全体像

### 1.1 方式

- **階層は下から順に配置する。** region（入れ子の枠）ごとに、その中身を一つの平らなグラフとして配置する。配置の終わった region は、親の段では大きさの決まった箱（Box）として扱う。
- **region をまたぐ辺は境界の port で区切る。** 辺 u→v は、u と v の最も近い共通の祖先（LCA）の段で一本の区間になる。u 側の各 region の中では「u から右辺の出口 port（Exit）まで」の区間、v 側の各 region の中では「左辺の入口 port（Entry）から v まで」の区間になる。最後に区間をつないで一本の折れ線にする。
- **平らな段の配置は Sugiyama 法で行う。** 層の割当て、ダミーの挿入、交差の削減、縦の座標、通路への配線の順に進む。
- **最適化は一つの network simplex で行う。** 層の割当てと縦の座標の両方で、同じ実装（`simplex.rs`）を使う。
- **座標・rank・長さ・重みはすべて整数（`i64`）で扱う。** 浮動小数の誤差による非決定性を持ち込まない。公開の型も `i64`。10.1 節の上限の中では、どの和も 2^62 を超えないので、溢れの検査（`checked_*`）は書かない（1.5 節）。
- **向きは左から右だけにする。** 辺は必ず始点の右辺から出て、終点の左辺に左から入る。逆向きの辺と同じ層の辺は、U ターン用のダミー（TurnEast・TurnWest）を通して回り込ませる。

### 1.2 ファイルと担当 unit

| ファイル | 内容 | unit |
|---|---|---|
| `src/lib.rs` | 公開 API の re-export と `layout`・`verify`・`metrics`（1.4 節のとおり） | L1 |
| `src/model.rs` | 公開の型、`LayoutError`、間隔の定数 | L1 |
| `src/level.rs` | 段の内部の型（2.2 節）と、段の配置の順序の制御（`run_level`） | L1（型）、L7（`run_level`） |
| `src/simplex.rs` | network simplex | L1 |
| `src/acyclic.rs` | 閉路除去（Eades–Lin–Smyth） | L1 |
| `src/layering.rs` | 層の割当て | L2 |
| `src/chains.rs` | ダミーの挿入、U ターン、空の層の削除 | L2 |
| `src/order.rs` | 交差の削減と port の並び | L3 |
| `src/position.rs` | 縦の座標 | L4 |
| `src/route.rs` | 通路の track、横の座標、折れ線の組立て | L5 |
| `src/check.rs` | 入力の検証と、出力の不変条件（`verify`） | L6 |
| `src/metrics.rs` | 交差数・折れ数・面積・総延長 | L6 |
| `src/hierarchy.rs` | region の木、辺の分解、下からの配置、区間の接続 | L7 |
| `tests/baseline.rs` | 基準（`dev/layout-baseline`）の全入力で配置と不変条件を確かめる | L8 |
| `examples/baseline_report.rs` | ELK との指標の比較表 | L8 |

消すもの（L1 で消す）：`src/prepare.rs`、`src/runtime.rs`、`src/validate.rs`、`runtime/`、`vendor/`、`tests/elk.rs`。`Cargo.toml` の依存のうち `serde`、`serde_json`、`tempfile` を消す（`serde_json` は `[dev-dependencies]` に移す）。

### 1.3 unit の所有と確認

| unit | 所有 | 確認（tier a） |
|---|---|---|
| L1 | `crates/xtask-layout/Cargo.toml`、`src/lib.rs`、`src/model.rs`、`src/level.rs`（`run_level` の本体を除く）、`src/simplex.rs`、`src/acyclic.rs`、他の `src/*.rs` の骨組み（1.4 節）、消すもの | `cargo check -p xtask-layout --all-targets`、`cargo test -p xtask-layout --lib simplex::`、`cargo test -p xtask-layout --lib acyclic::` |
| L2 | `src/layering.rs`、`src/chains.rs` | `cargo check -p xtask-layout --all-targets`、`cargo test -p xtask-layout --lib layering::`、`cargo test -p xtask-layout --lib chains::` |
| L3 | `src/order.rs` | 同じ check と `cargo test -p xtask-layout --lib order::` |
| L4 | `src/position.rs` | 同じ check と `cargo test -p xtask-layout --lib position::` |
| L5 | `src/route.rs` | 同じ check と `cargo test -p xtask-layout --lib route::` |
| L6 | `src/check.rs`、`src/metrics.rs` | 同じ check と `cargo test -p xtask-layout --lib check::`、`cargo test -p xtask-layout --lib metrics::` |
| L7 | `src/hierarchy.rs`、`src/level.rs` の `run_level` | 同じ check と `cargo test -p xtask-layout --lib hierarchy::` |
| L8 | `tests/baseline.rs`、`tests/common/**`、`examples/baseline_report.rs` | `cargo check -p xtask-layout --all-targets`、`cargo test -p xtask-layout --release --test baseline --no-run` |

- 試験は各ファイルの `#[cfg(test)] mod tests` に書く。
- **実行の順。** L6 は W2（L2 の後、同じ lane）、L7 と L8 は W3。L6 の試験は手で作った `Layout` だけを使い、`layout` を呼ばない（W2 では他の段が `todo!()`）。`layout` を通す試験はすべて L7 が持つ。L7 の時点で L2〜L6 は統合済み。
- **L7 の試験**（`hierarchy::tests`）：
  - `nested_regions`：region r0 の中に r1、r1 の中に node a、r0 の中に node b、辺 a→b。`crate::layout` が Ok（`check::output` も通っている）。
  - `cross_region_edge`：r0 の中の a から、r1 の中の b への辺。上と同じく Ok で、経路の始点が a の右辺、終点が b の左辺にある。
  - `region_without_boundary_edges`：辺の無い region が一つ。Ok で、region の矩形が node を含む。
  - `layout_two_nodes_verifies`：node 2 個（大きさ 80×40）、辺 1 本を `crate::layout` した結果を、`crate::verify` が受け入れる。
  - `empty_graph_is_padding_only`：node も region も無い Graph の結果が `size == Size { width: 48, height: 48 }` で、配列がすべて空。
  - `empty_region_has_header_space`：node の無い region が一つ（header 100×16）。Ok で、region の矩形の幅が 140 以上、高さが 56 以上。
- `clippy -D warnings` は、crate が完成した後の W3 の tier b で Orchestrator が初めて実行する。それより前の unit は、骨組みの `todo!()` に由来する `dead_code`・`unused_variables` の警告を残してよい（作業指示書 0.6 節）。

### 1.4 L1 が作る骨組み（同じ wave の後の lane が `lib.rs` を触らないため）

L1 は、上の表のすべての `src/*.rs` を作る。後の unit が所有するファイルには、各章が定める関数の署名を書き、本体を `todo!("<unit>")` にする。署名は次のとおり。

| ファイル | 置く署名 |
|---|---|
| `layering.rs` | `pub(crate) fn assign(input: &LevelInput) -> Result<Layers, LayoutError>` |
| `chains.rs` | `pub(crate) fn build(input: &LevelInput, layers: &Layers) -> Result<Chains, LayoutError>` |
| `order.rs` | `pub(crate) fn arrange(chains: &Chains) -> Result<Order, LayoutError>`、`pub(crate) fn crossings(chains: &Chains, layers: &[Vec<ItemId>]) -> usize` |
| `position.rs` | `pub(crate) fn vertical(chains: &Chains, order: &Order) -> Result<Vec<i64>, LayoutError>` |
| `route.rs` | `pub(crate) fn finish(input: &LevelInput, chains: &Chains, order: &Order, y: &[i64]) -> Result<LevelOutput, LayoutError>` |
| `check.rs` | `pub(crate) fn input(graph: &Graph) -> Result<(), LayoutError>`、`pub(crate) fn output(graph: &Graph, layout: &Layout) -> Result<(), String>` |
| `metrics.rs` | 10.3 節の `Metrics` の型（このとおり）と `pub(crate) fn compute(width: f64, height: f64, routes: &[Vec<(f64, f64)>]) -> Metrics` |
| `hierarchy.rs` | `pub(crate) fn layout(graph: &Graph) -> Result<Layout, LayoutError>` |

`lib.rs` は L1 が完成させ、後の unit は変えない。

```rust
//! Role: 直交の階層配置の公開 API を提供する。
//! Native layered layout: regions bottom-up, network simplex, orthogonal channels.
mod acyclic;
mod chains;
mod check;
mod hierarchy;
mod layering;
mod level;
mod metrics;
mod model;
mod order;
mod position;
mod route;
mod simplex;

pub use metrics::Metrics;
pub use model::{Edge, Graph, Layering, Layout, LayoutError, Node, Point, Rect, Region, Route, Size};

/// 要求を検証して配置し、出力の不変条件を確かめて返す。
pub fn layout(graph: &Graph) -> Result<Layout, LayoutError> {
    check::input(graph)?;
    let layout = hierarchy::layout(graph)?;
    check::output(graph, &layout).map_err(LayoutError::Internal)?;
    Ok(layout)
}

/// 出力の不変条件（付属書 L 10.2 節）を確かめる。
pub fn verify(graph: &Graph, layout: &Layout) -> Result<(), String> {
    check::output(graph, layout)
}

/// 折れ線の指標。ELK の結果にも使えるよう f64 で受け取る。
pub fn metrics(width: f64, height: f64, routes: &[Vec<(f64, f64)>]) -> Metrics {
    metrics::compute(width, height, routes)
}
```

### 1.5 共通の規則

- 関数はすべて**純粋**にする。入力を参照で受け取り、新しい値を返す。段階をまたいで書き換える構造体（後から埋める field、`Option` で「未計算」を表す field）を作らない。
- 繰り返しの順序はすべて index の昇順にする。`HashMap` と `HashSet` を使わない（`BTreeMap`、`Vec` を使う）。並べ替えはすべて `sort_by` に明示の tie-break を付け、安定な比較にする。
- 乱数を使わない。
- `unwrap` と `expect` は、直前の行で成り立つことが明らかな場合だけ使う。内部の矛盾は `LayoutError::Internal(String)` で返す。内部の矛盾が起こりうる関数（`simplex::solve`、`position::vertical`、`order::arrange`、`route::finish` など）は、すべて `Result<_, LayoutError>` を返す。
- **溢れが起きない理由**（検査を書かない根拠）。10.1 節の上限で、要素（入力の node・region と、ダミー・ラベル・U ターン）の数は `nodes + regions + edges × (2 × (nodes + regions) + 5) < 2 × 10^11`。一つの要素が座標に足す量は `MAX_SIZE + 2 × NODE_GAP + PORT_PITCH × (link の数 + 1) + PORT_MARGIN` 以下。したがって座標・rank・重みの和は `10^18` 未満で、`i64::MAX`（約 9.2 × 10^18）に届かない。
- 1 関数 80 行、1 ファイル 1000 code 行を超えない。

---

## 2. 型

### 2.1 公開の型（`src/model.rs`、L1 がこのとおりに書く）

```rust
//! Role: 配置の要求・結果・誤りの公開型と、間隔の定数を定義する。
use std::fmt;

/// 幅と高さ（px）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Size {
    pub width: i64,
    pub height: i64,
}

/// 点（px）。y は下向きに増える。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

/// 左上の点と大きさ（px）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

/// 配置の要求。node・region・edge は配列の index で参照する。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub regions: Vec<Region>,
    pub edges: Vec<Edge>,
    pub layering: Layering,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    /// 文字に必要な大きさ。port の数に応じて高さは増えうる（3.3 節）。
    pub size: Size,
    /// 直接含む region。`None` は最上段。
    pub region: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    /// 親の region。`None` は最上段。
    pub parent: Option<usize>,
    /// 上辺に置く見出しの文字の大きさ。
    pub header: Size,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    /// 辺の中央に置くラベルの大きさ。
    pub label: Option<Size>,
}

/// 層の決め方。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Layering {
    /// 辺の総延長が最小になるよう決める。
    Optimize,
    /// node ごとの列番号（左から 0）。region が無いグラフだけで使える。
    Fixed(Vec<u32>),
}

/// 配置の結果。`nodes`・`regions`・`edges` は要求と同じ順。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub size: Size,
    pub nodes: Vec<Rect>,
    pub regions: Vec<Rect>,
    pub edges: Vec<Route>,
}

/// 辺の経路。始点の右辺から出て、終点の左辺に入る直交の折れ線。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub points: Vec<Point>,
    pub label: Option<Rect>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// node の region が範囲外。
    NodeRegion { node: usize },
    /// region の親が範囲外、または自分自身。
    RegionParent { region: usize },
    /// region の親をたどると閉路になる。
    RegionCycle { region: usize },
    /// edge の端が範囲外。
    EdgeEndpoint { edge: usize },
    /// 始点と終点が同じ edge。
    SelfLoop { edge: usize },
    /// 大きさが負、または 100000 を超える。
    InvalidSize,
    /// `Layering::Fixed` の長さが node の数と違う。
    FixedLength,
    /// `Layering::Fixed` と region は同時に使えない。
    FixedWithRegions,
    /// node・region・edge の数が上限（`MAX_NODES`、`MAX_REGIONS`、`MAX_EDGES`）を超える。
    TooMany,
    /// 配置の途中の矛盾（実装の誤り）。
    Internal(String),
}

impl fmt::Display for LayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NodeRegion { node } => write!(formatter, "node {node} refers to a missing region"),
            Self::RegionParent { region } => write!(formatter, "region {region} has an invalid parent"),
            Self::RegionCycle { region } => write!(formatter, "region {region} is in a parent cycle"),
            Self::EdgeEndpoint { edge } => write!(formatter, "edge {edge} refers to a missing node"),
            Self::SelfLoop { edge } => write!(formatter, "edge {edge} starts and ends at the same node"),
            Self::InvalidSize => write!(formatter, "a size is negative or larger than 100000"),
            Self::FixedLength => write!(formatter, "fixed layering must give one column per node"),
            Self::FixedWithRegions => write!(formatter, "fixed layering cannot be used with regions"),
            Self::TooMany => write!(formatter, "the graph has more than 100000 nodes or regions, or more than 1000000 edges"),
            Self::Internal(message) => write!(formatter, "internal layout error: {message}"),
        }
    }
}

impl std::error::Error for LayoutError {}

/// 同じ層で上下に並ぶ大きな要素（node、box、label）どうしの間隔。
pub(crate) const NODE_GAP: i64 = 40;
/// 大きな要素と小さな要素（dummy、turn、port）の間隔。
pub(crate) const MIXED_GAP: i64 = 16;
/// 小さな要素どうしの間隔。
pub(crate) const SMALL_GAP: i64 = 12;
/// 層と層の間の通路の最小の幅。
pub(crate) const LAYER_GAP: i64 = 48;
/// 通路の中の縦線（track）どうしの間隔。
pub(crate) const TRACK_GAP: i64 = 12;
/// 通路の端から最初の track までの最小の距離。
pub(crate) const TRACK_MARGIN: i64 = 16;
/// region の左・右・下の余白。上の余白は見出しの高さ + この値。
pub(crate) const REGION_PAD: i64 = 20;
/// 図全体の余白。
pub(crate) const DIAGRAM_PAD: i64 = 24;
/// U ターンの縦の長さ。
pub(crate) const TURN_HEIGHT: i64 = 12;
/// node の一辺に並ぶ port の最小の間隔と、上下の余り。
pub(crate) const PORT_PITCH: i64 = 16;
pub(crate) const PORT_MARGIN: i64 = 24;
/// 交差の削減の最大の往復回数。
pub(crate) const MAX_SWEEPS: usize = 12;
/// 大きさの上限。
pub(crate) const MAX_SIZE: i64 = 100_000;
/// 数の上限（溢れが起きない根拠。1.5 節）。
pub(crate) const MAX_NODES: usize = 100_000;
pub(crate) const MAX_REGIONS: usize = 100_000;
pub(crate) const MAX_EDGES: usize = 1_000_000;
```

### 2.2 段の内部の型（`src/level.rs` の前半、L1 がこのとおりに書く）

```rust
//! Role: 平らな段の要素・区間・中間結果の型と、段の配置の順序を定義する。
use crate::model::{Point, Rect, Size};

/// 段の中の要素の index。
pub(crate) type ItemId = usize;

/// 要素の種類。payload は Graph の node・region・edge の index。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ItemKind {
    /// 原子の node。
    Node(usize),
    /// 配置済みの子 region。
    Box(BoxItem),
    /// この段の region の左辺の入口 port（edge ごと）。
    Entry(usize),
    /// この段の region の右辺の出口 port（edge ごと）。
    Exit(usize),
    /// 長い辺が層を通り抜ける点（edge ごと）。
    Dummy(usize),
    /// 辺のラベル（edge ごと）。
    Label(usize),
    /// 始点の右で折り返す U ターン（edge ごと）。port は二つとも左辺。
    TurnEast(usize),
    /// 終点の左で折り返す U ターン（edge ごと）。port は二つとも右辺。
    TurnWest(usize),
}

/// 配置済みの子 region。port の y は box の上辺からの距離。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BoxItem {
    pub region: usize,
    /// (edge, y)。左辺の入口。
    pub entries: Vec<(usize, i64)>,
    /// (edge, y)。右辺の出口。
    pub exits: Vec<(usize, i64)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Item {
    pub kind: ItemKind,
    pub size: Size,
}

/// 段の中の区間。`from` の右辺から `to` の左辺へ。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Segment {
    pub edge: usize,
    pub from: ItemId,
    pub to: ItemId,
    pub label: Option<Size>,
}

/// 段の種類。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Frame {
    /// 図の最上段。`Fixed` は item ごとの列番号。
    Root(RootLayering),
    /// region の中。見出しの大きさを持つ。
    Region { header: Size },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RootLayering {
    Optimize,
    Fixed(Vec<u32>),
}

/// 段の入力。`items` は Node・Box・Entry・Exit だけを、この順に持つ
/// （内容の item（Node・Box）が先頭に続き、その後に Entry、Exit が続く。9.4 節）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LevelInput {
    pub items: Vec<Item>,
    pub segments: Vec<Segment>,
    pub frame: Frame,
}

impl LevelInput {
    /// 内容の item（Node・Box）の数。`items[..content_count()]` が内容の item。
    pub(crate) fn content_count(&self) -> usize {
        self.items
            .iter()
            .take_while(|item| matches!(item.kind, ItemKind::Node(_) | ItemKind::Box(_)))
            .count()
    }
}

/// 層の割当ての結果（3 章）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Layers {
    /// 内容の item（`items[..content_count()]`）ごとの層。Entry・Exit は持たない。
    pub layer: Vec<i64>,
}

/// 隣の層の二つの要素を結ぶ線。`left` は層 L、`right` は層 L + 1。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Link {
    pub edge: usize,
    pub left: ItemId,
    pub right: ItemId,
}

/// 区間がたどる link と向き。`forward` は left から right へ進むこと。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Step {
    pub link: usize,
    pub forward: bool,
}

/// ダミーを入れた後の段（4 章）。`build` だけが作り、その後は変えない。
/// `layer_count` は `layer` の最大 + 1 の派生の値（設計書 6.1 節の 3 の「許すもの」）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Chains {
    /// 入力の item（同じ index）の後にダミーが続く。大きさは port の数を反映済み。
    pub items: Vec<Item>,
    /// item ごとの層（0 から `layer_count - 1`）。
    pub layer: Vec<usize>,
    pub layer_count: usize,
    pub links: Vec<Link>,
    /// 入力の segment ごとの道のり。
    pub paths: Vec<Vec<Step>>,
}

/// 並びの結果（5 章）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Order {
    /// 層ごとの item の上から下への並び。
    pub layers: Vec<Vec<ItemId>>,
    /// link ごとの、left の item の上辺から port までの距離。
    pub east: Vec<i64>,
    /// link ごとの、right の item の上辺から port までの距離。
    pub west: Vec<i64>,
}

/// 段の配置の結果。座標は段の左上を (0, 0) とする。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LevelOutput {
    pub size: Size,
    /// 入力の item ごとの矩形。Entry・Exit は幅 0・高さ 0 の port の点。
    pub rects: Vec<Rect>,
    /// 入力の segment ごとの折れ線。
    pub pieces: Vec<Piece>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Piece {
    pub points: Vec<Point>,
    pub label: Option<Rect>,
}
```

L1 は同じファイルの後半に、次の関数を `todo!()` の本体で置く（L7 が本体を書く）。

```rust
/// 段を配置する。layering → chains → order → position → route の順。
pub(crate) fn run_level(input: &LevelInput) -> Result<LevelOutput, crate::LayoutError> {
    todo!("L7")
}
```

---

## 3. 層の割当て（`src/layering.rs`、L2）

```rust
pub(crate) fn assign(input: &LevelInput) -> Result<Layers, LayoutError>
```

### 3.1 対象

- 内容の item（`items[..content_count()]`、Node と Box）だけに層を付ける。Entry と Exit は層を持たない（境界の層は 4 章で付ける）。`Layers.layer` の長さは `content_count()`。
- 端が Entry・Exit の segment は、層の割当てに使わない。

### 3.2 `Frame::Root(RootLayering::Fixed(columns))`

1. 列番号を詰める。`columns` の異なる値を昇順に並べ、値から順位（0 から）への表を作る（`BTreeMap<u32, i64>`）。
2. `layer[i] = 2 * 順位(columns[i])`。奇数の層は、ラベルと U ターンのために空けておく。これで終わり。

列番号の値の大きさは層の数に影響しない（`[0, 1_000_000_000]` も `[0, 2]` になる）。

### 3.3 `Optimize`（Root の Optimize と、すべての Region）

1. **閉路の除去。** 3.1 節の対象の segment を、重み 1 の辺の多重集合として `acyclic::order` に渡す（6.2 節）。返る順位で `from` が `to` より後になる segment は、向きを逆にして扱う。
2. **問題の作成。** `simplex::Problem` の node は内容の item（`node_count = content_count()`）。3.1 節の対象の segment ごとに辺を一本足す。
   - 向きは、手順 1 で逆にしたものは `to → from`、それ以外は `from → to`。
   - `minlen` はラベル付きなら 2、それ以外は 1。
   - `weight` は 1。
3. **解く。** `simplex::solve(&problem)?` で、node ごとの rank を得る。最小の rank は 0 になる。
4. **均衡。** item の index の昇順に、次を行う。
   - 対象は、Node と Box の item のうち、入ってくる辺の重みの和と出ていく辺の重みの和が等しいもの（辺の無い item を含む）。Entry と Exit は対象にしない。
   - 置ける範囲 `[lo, hi]` を求める。`lo` は、入ってくる辺の `rank(from) + minlen` の最大（入る辺が無ければ 0）。`hi` は、出ていく辺の `rank(to) - minlen` の最小（無ければ、その時点の最大の rank）。
   - 範囲の中で、Node と Box の数が最も少ない層を選ぶ。同数なら小さい層を選ぶ。
   - 選んだ層の数が今の層の数より**少ないときだけ**移す。
5. `layer[i] = rank[i]`（内容の item だけ）。

### 3.4 試験（`#[cfg(test)] mod tests`）

- **`fixed_doubles_columns`**：Root Fixed `[0, 1, 1]` の結果が `[0, 2, 2]` になる。
- **`fixed_columns_are_compressed`**：Root Fixed `[0, 1_000_000_000, 7]` の結果が `[0, 4, 2]` になる。
- **`boundary_items_have_no_layer`**：Region の段で、node 1 個と Entry 1 個、segment が Entry→node のとき、`layer` の長さが 1。
- **`chain_layers`**：a→b、b→c（Region、header 0）の結果が `[0, 1, 2]` になる。
- **`label_needs_two_layers`**：a→b でラベルが `Size{width: 10, height: 10}` のとき、結果が `[0, 2]` になる。
- **`cycle_is_broken`**：a→b、b→a の結果で、二つの層が異なる。
- **`balance_moves_isolated`**：a→b と、辺の無い c, d の場合。
  - 均衡の前は c と d が層 0 にあり、層 0 は {a, c, d} の 3 個、層 1 は {b} の 1 個になる。
  - c を処理すると、層 1 の数（1）が今の層の数（3）より少ないので、c を層 1 に移す。
  - 続けて d を処理する。層 0 は {a, d}、層 1 は {b, c} でどちらも 2 個になり、「少ないときだけ移す」ので d は層 0 に残る。
  - 期待値は `[0, 1, 1, 0]`。

---

## 4. ダミーの挿入（`src/chains.rs`、L2）

```rust
pub(crate) fn build(input: &LevelInput, layers: &Layers) -> Result<Chains, LayoutError>
```

### 4.1 層の範囲を決める

1. 内容の item（Node・Box）の層を `layer` から取る。これを `l(i)` と書く。内容の item が一つも無いときは、内容の最小・最大の層を 0 とみなす。
2. segment を三つに分ける。
   - **境界の segment**：`from` が Entry、または `to` が Exit。
   - **前向き**：`l(to) > l(from)`。
   - **逆向き**：それ以外（同じ層を含む）。
3. 逆向きの segment が要る範囲を求める。U ターンは `l(from) + 1` と `l(to) - 1` に置く。
   - `need_min` は、`l(to) - 1` の最小と、内容の最小の層のうち小さいほう。
   - `need_max` は、`l(from) + 1` の最大と、内容の最大の層のうち大きいほう。
4. `shift` を決める。
   - Region なら、Entry の層を 0 にするため `shift = 1 - need_min` とする。内容は `l + shift` になり、層 0 は Entry だけになる。Exit の層は `need_max + shift + 1` にする。
   - Root なら `shift = -need_min` とする。Entry と Exit は無い。
5. `layer_count` を求める。Region では `Exit の層 + 1`、Root では `need_max + shift + 1`。

### 4.2 item と link を作る

結果の `items` は、入力の item を同じ index で写した後にダミーを足していく。ダミーの大きさは次のとおり。

| 種類 | 幅 | 高さ |
|---|---|---|
| Dummy | 0 | 0 |
| Label | ラベルの幅 | ラベルの高さ |
| TurnEast・TurnWest | 0 | `TURN_HEIGHT` |

segment の index の昇順に、次のとおり作る。`a = l(from)`、`b = l(to)` は shift した後の層。Entry の層は 0、Exit の層は `layer_count - 1` とする。

**前向き（境界の segment を含む。`b > a`）：**
1. 層 `a+1 .. b-1` に Dummy を一つずつ作る。
2. ラベル付きなら、層 `a + (b - a) / 2` の Dummy の代わりに Label を作る。`b - a < 2` のときは `Internal("label needs a free layer")` を返す。
3. link を `from → d(a+1) → … → d(b-1) → to` の順に作る。各 link の `left` は層の小さいほう。
4. path は、すべて `forward: true` の Step。

**逆向き（`b <= a`）：**
1. 作る item は次のとおり。
   - `t1 = TurnEast(edge)`：層 `a + 1`。
   - `d(k)`：k を `a, a-1, …, b` の順に一つずつ。Dummy で、ラベル付きなら `k = a - (a - b) / 2` だけ Label。
   - `t2 = TurnWest(edge)`：層 `b - 1`。
2. link と Step を道のりの順に作る。

   | link | Step |
   |---|---|
   | `Link{left: from, right: t1}` | forward |
   | `Link{left: d(a), right: t1}` | backward |
   | k = a から b+1 まで：`Link{left: d(k-1), right: d(k)}` | backward |
   | `Link{left: t2, right: d(b)}` | backward |
   | `Link{left: t2, right: to}` | forward |

### 4.3 空の層の削除

1. 削除の候補は、**内側の層**だけにする。
   - Region の段では、層 0（Entry の層）と層 `layer_count - 1`（Exit の層）を、空でも決して消さない。
   - Root の段では、すべての層が候補になる。
2. 候補の層のうち、そこにある item が一つ以上あって、すべて `Dummy` のものを消す。Label、Turn、Node、Box が一つでもあれば残す。
   - item が一つも無い内側の層は、ダミーが無いので消しても link は変わらない。これも消す。
3. 該当する層を、右（大きい層）から順に消す。消す層の Dummy `d` については次を行う。
   - `d` を right に持つ link `p`（`p.left` は層 L−1）と、`d` を left に持つ link `q`（`q.right` は層 L+1）は、ちょうど一本ずつある。
   - 二本を一本の link `n = Link { edge, left: p.left, right: q.right }` にまとめる（幾何の左右で決める。道のりの順ではない）。
   - `d` を通る path では、`p` と `q` の Step が隣り合い、向きは同じである。前向き（`forward: true`）の path では `p, q` の順、逆向きの path では `q, p` の順に並ぶ。
   - 隣り合う二つの Step のうち、**path の中で先に来る Step** を `Step { link: n, forward: 元の向き }` に替え、後に来る Step を消す。
   - 例：前向き `(a, d), (d, b)` → `(a, b)`。逆向き `Step(q = (d, b), backward), Step(p = (a, d), backward)` → `Step((a, b), backward)`。
   - 消した層より大きい層の番号を、一つずつ詰める。
4. 消した item は `items` から取り除き、index を詰め直す。link と path の index もそれに合わせて書き直す。
   - 入力の item は先頭にあり消えないので、入力の index は変わらない。
5. 実装は、削除の印を付けてから一度に新しい Vec を作る方式にする。index の書き換え表（旧 → 新）を使う。
6. 結果として、Region の段は必ず `layer_count >= 2` になる。

### 4.4 node の高さ

Node の item の高さを `max(size.height, (max(左辺の link の数, 右辺の link の数) + 1) * PORT_PITCH + PORT_MARGIN)` にする。左辺の link はこの item を `right` に持つもの、右辺の link は `left` に持つもの。

### 4.5 試験

- **`forward_span_three`**：Root Fixed、node a, c, e, b の列が `[0, 1, 2, 3]`（層 0, 2, 4, 6）で、segment は a→b だけ。c と e が層 2 と層 4 を残すので、結果は層 0〜6 の 7 層。a→b の Dummy は層 1, 2, 3, 4, 5 に作られ、Dummy だけの層 1, 3, 5 が消えて、Dummy が 2 個（層 2 と 4 の、新しい層 1 と 2）、`layer_count` が 4、a→b の link が 3 本、path が 3 Step（すべて forward）になる。
- **`backward_dummy_layer_removed`**：Root Fixed、node a, c, b の列が `[0, 1, 2]`（層 0, 2, 4）で、segment は b→a だけ（逆向き）。4.1 節の shift（`need_min = l(a) - 1 = -1` なので `shift = 1`）で、a, c, b は層 1, 3, 5、TurnEast は層 6、TurnWest は層 0、Dummy は層 5, 4, 3, 2, 1 に作られ、`layer_count` は 7、path は 8 Step になる。Dummy だけの層は 2 と 4 で、これを消すと `layer_count` が 5、path の Step が 6 個で、向きは `[true, false, false, false, false, true]` になる。各 link の `left` の層 + 1 が `right` の層に等しい。
- **`label_in_middle`**：層 0 → 2 のラベル付きで、層 1 が Label になる。
- **`backward_makes_turns`**：Root Optimize、層 a=1 → b=0 の segment で、TurnEast が層 3、TurnWest が層 0、`layer_count` が 4 になる。
  - shift した後の層は from が 2、to が 1。
  - path の Step は 5 個、向きは `[true, false, false, false, true]`。
- **`dummy_only_layer_removed`**：Root Fixed `[0, 1]`（層 0 と 2）の a→b で、層 1 の Dummy が消え、`layer_count` が 2、link が 1 本になる。
- **`node_height_grows`**：高さ 10 の node から右へ 3 本出るとき、高さが `(3 + 1) * 16 + 24 = 88` になる。
- **`region_keeps_boundary_layers`**：Region の段で、node 一つ、segment 無しのとき、`layer_count == 3`（Entry の空の層、node の層、Exit の空の層）になる。

---

## 5. 交差の削減と port の並び（`src/order.rs`、L3）

```rust
pub(crate) fn arrange(chains: &Chains) -> Result<Order, LayoutError>
pub(crate) fn crossings(chains: &Chains, layers: &[Vec<ItemId>]) -> usize
```

### 5.1 初めの並び

各層の item を index の昇順に並べる。入力の item は、hierarchy（9 章）が名前の順に作るので、名前の順になる。

### 5.2 位置の値

並びを固定した層 F にある item `j` の port の位置の値は `pos(j) + frac` とする。`pos` は F の中での順位（0 から）。`frac` は次のとおり。

- `j` が Box なら、port の y を box の高さ + 1 で割った値（`y as f64 / (height + 1) as f64`）。
- `j` が TurnEast・TurnWest なら、その link が二つの port のうちどちらに付くかまだ決まらないので 0.5。
- それ以外は 0.5。

### 5.3 掃引

```
best = 初めの並び, best_count = crossings(best)
stall = 0
for sweep in 0..MAX_SWEEPS:
    down: for L in 1..layer_count:  層 L を、層 L-1 を固定して並べ直す（5.4）
    up:   for L in (0..layer_count-1).rev(): 層 L を、層 L+1 を固定して並べ直す
    transpose（5.5）
    count = crossings(今の並び)
    if count < best_count: best = 今の並び, best_count = count, stall = 0
    else: stall += 1; if stall == 2: break
    if best_count == 0: break
return best
```

改善しなかった往復の後も、次の往復は `best` ではなく**今の並び**から続ける。

### 5.4 一つの層の並べ直し（重心法）

1. 層 L の各 item `v` について、固定した層にある隣の port の位置の値の平均を求める。
   - down では、`right == v` の link の left 側を使う。
   - up では、`left == v` の link の right 側を使う。
2. 隣が無い item は、今の順位（`pos(v) as f64`）をその値にする。
3. `(値, 今の順位)` の昇順で安定に並べる。値の比較は `f64::total_cmp` で行う。

### 5.5 transpose

1. 各層で、隣り合う二つの item u（上）、v（下）について `c(u, v)` と `c(v, u)` を比べる。
   - `c(u, v)` は、u を v の上に置いたときの、u・v と両隣の層との交差の数。
   - u の link e と v の link f の組（同じ隣の層に向かうもの）が交差するのは、e の相手の位置の値が f の相手の位置の値より**大きい**とき。
2. `c(v, u) < c(u, v)` なら入れ替える。
3. 層の上から下へ一回ずつ通すことを、入れ替えが起きなくなるまで、最大 4 回繰り返す。

### 5.6 交差の数（`crossings`）

1. 層の組 (L, L+1) ごとに、その間の link を集める。各 link の鍵を二つの組 `left_key = (pos(left), 左の port の順)`、`right_key = (pos(right), 右の port の順)` とする（`(usize, usize)` の辞書順で比べる。数を掛けて一つの整数にしない）。port の順は次のとおり。
   - Box は、その item の同じ辺の port を y の昇順に並べたときの順位。
   - それ以外は 0。
2. `(left_key, right_key)` の昇順に並べる。`right_key` の列の転倒の数（`i < j` かつ `right_key_i > right_key_j` の組の数）を、併合整列で数える。
3. すべての組の和を返す。

### 5.7 port の並び（最後に一回だけ）

最良の並びで、link ごとに `east` と `west` を決める。

- **Box**：`BoxItem` の entries・exits の y をそのまま使う。edge で探し、見つからなければ `Err(LayoutError::Internal(format!("box port for edge {edge} is missing")))` を返す。
- **Node・Label・Dummy・Entry・Exit**：辺ごとに、その辺の link を相手の位置の値の昇順に並べる。同じ値なら edge の index の昇順。k 番目（0 から）、全部で n 本のとき、port は `((k + 1) * height) / (n + 1)` にする（整数の除算）。高さ 0 の item は 0 になる。
- **TurnEast**（左辺に二つ）：二本の link の相手（left）の `pos` が小さいほうを 0、大きいほうを `TURN_HEIGHT` にする。同じなら、path で先に来る link を 0 にする。
- **TurnWest**（右辺に二つ）：相手（right）の `pos` で同じように決める。

### 5.8 試験

- **`two_crossing_links_uncross`**：層 0 に a, b、層 1 に c, d、link a→d、b→c の場合。初めの並びでは交差 1、arrange の後は交差 0 になる。
- **`crossings_counts_inversions`**：上の初めの並びで `crossings` が 1 を返す。
- **`ports_follow_targets`**：層 0 の node a から層 1 の c, d（c が上）へ二本出る場合。a の右の port が、c 側 < d 側の順になる。
- **`turn_ports_are_ordered`**：4.5 節の `backward_makes_turns` の Chains に arrange をかけた結果で、TurnEast の二つの port が 0 と 12 に分かれる。

---

## 6. 共通の部品（L1）

### 6.1 network simplex（`src/simplex.rs`）

```rust
pub(crate) struct Problem {
    pub node_count: usize,
    pub edges: Vec<Constraint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Constraint {
    pub from: usize,
    pub to: usize,
    pub minlen: i64,
    pub weight: i64,
}

/// rank(to) - rank(from) >= minlen を満たす rank を返す。目的は Σ weight * (rank(to) - rank(from)) を
/// 小さくすること。繰返しの上限（手順 7）の中で最適に届けば最適解、届かなければその時点の実行可能解。
/// 辺は閉路を作らないこと。閉路・範囲外の端・負の weight は `Err(LayoutError::Internal(..))`。
/// 最小の rank は 0。
pub(crate) fn solve(problem: &Problem) -> Result<Vec<i64>, LayoutError>
```

最初に入力を調べる：端が `node_count` 以上、`weight < 0`、Kahn 法（手順 3）で全 node を取り出せない（閉路）のどれかなら `Err(LayoutError::Internal(..))`。手順 7 で入る辺の候補が一つも無いときも `Err(LayoutError::Internal("no entering edge"))`（正しい実装では起きない）。

> **W3 の測定の後の改訂（L9 が置き換える）。** 下の手順 2（全 node への根の辺）と手順 7 の「交換のたびに全部を作り直す」は、K4 の測定で配置の時間の 99% を占めた（約 20 秒のうち 19.8 秒）。L9 は、`simplex.rs` を `rework/reference/simplex.rs.txt`（設計者が書き、Rust 1.85 で試験と基準の全件を通したもの）で置き換える。違いは次の 3 点で、他の手順（単純化、Kahn の初めの rank、締まった木、cut value の式、tie-break）は同じ：
> - 根の辺を足さず、連結成分ごとに解く（成分は最小の node の昇順、成分の中の木の根は最小の node）。各成分の最小の rank を 0 にする。
> - 交換では、変わる部分だけを直す（graphviz の `ns.c` と同じ方式）：入る辺の slack の分だけ下側の部分木の rank をずらし、入る辺の両端から共通の祖先までの木の辺の cut value を直し、その祖先の部分木だけの parent・low・lim を振り直す。
> - 出る辺は、木の辺の並びを前回の位置から巡り、見つけた負の cut value の最初の 30 個のうち最も負のもの。入る辺は、出る辺の下側の部分木だけをたどって探す。
> 測定（Kivact の 121 件、release）：配置の時間の和 19.6 秒 → 2.5 秒。基準の全件で不変条件と決定性を満たす。

手順（Gansner ほか 1993 の方法。dagre の `network-simplex.js` と同じ構造。コードは写さず、この手順から書く）：

1. **単純化。** 同じ `(from, to)` の辺を一本にまとめ、`weight` は和、`minlen` は最大を取る。まとめた辺は `(from, to)` の昇順に並べる。
2. **根の追加。** node `r = node_count` を足す。すべての node `v` に辺 `r → v`（minlen 0、weight 0）を足す。これでグラフが連結になる。
3. **初めの rank。** Kahn 法で求める。入次数 0 の node のうち index の最小のものから取り出す（`BTreeSet` を使う）。`rank(v) = max(rank(u) + minlen)`、源は 0。
4. **実行可能な木。**
   - 木 T を {r} から始める。
   - T の node から、締まった辺（slack = rank(to) - rank(from) - minlen が 0）でつながる T の外の node を加える。node は加えた順に見て、辺は index の順に見る。
   - T が全 node を含めば終わる。
   - そうでなければ、端の一方だけが T にある辺のうち slack が最小のもの（同じなら index の最小）を選ぶ。`from` が T にあれば T の全 node の rank に slack を足し、`to` が T にあれば slack を引く。その後、加える手順に戻る。
5. **木の表現。**
   - node ごとに、親の node、親との木の辺の index、木の子の一覧を持つ。根は r。
   - 後順で `lim(v)` を振る（1 から）。`low(v)` は部分木の `lim` の最小。子は index の昇順にたどる。
6. **cut value。** 根以外の node `c`（親 `p`、木の辺 `e`）について、後順で求める。
   ```
   child_is_tail = (e.from == c)
   cut = e.weight
   for f in c に接する辺（e 以外。index の順）:
       is_out = (f.from == c); other = if is_out { f.to } else { f.from }
       if other == p: continue
       points_to_head = (is_out == child_is_tail)
       cut += if points_to_head { f.weight } else { -f.weight }
       if parent(other) == Some(c):          // other は c の木の子
           cut += if points_to_head { -cut_value(other) } else { cut_value(other) }
   cut_value(c) = cut
   ```
7. **改善の繰返し。**
   - **出る辺**：cut value が負の木の辺のうち、子の node の index が最小のもの。無ければ最適なので終わる。
   - **入る辺**：出る辺 `e = (v → w)` に対して、`v` と `w` のうち `lim` が小さいほうを `tail` とする。`tail` が `w` のときは `flip = true` にする。
     - 次を満たす木の外の辺 `f = (x → y)` を候補にする：`flip == is_descendant(x, tail)` かつ `flip != is_descendant(y, tail)`。
     - `is_descendant(n, t)` は `low(t) <= lim(n) <= lim(t)`。
     - 候補のうち slack が最小のもの（同じなら index の最小）を選ぶ。
   - **交換**：e を木から外して f を入れる。
     - 根 r から木をたどり直して、親・子・`low`・`lim`・cut value を作り直す。
     - rank は根から木の辺に沿って決め直す。木の辺 g で親 p から子 c へ進むとき、`g.from == p` なら `rank(c) = rank(p) + g.minlen`、そうでなければ `rank(c) = rank(p) - g.minlen`。
   - 繰返しは最大 `20 * 辺の数 + 100` 回にする。超えたら、その時点の rank（実行可能だが最適とは限らない）で終わる。これは誤りではない。
   - 交換のたびに全部を作り直す（dagre と同じ）ので、計算量は O(辺の数²) になりうる。W3 の tier b で K4 を測り、届かなければ設計者が差分の更新に替える（差戻し）。実装者は最適化しない。
8. **後処理。** r を外し、最小の rank が 0 になるよう全体をずらして返す。

試験：
- **`chain`**：0→1→2 の結果が `[0, 1, 2]` になる。
- **`moves_source_down`**：0→1、1→2、3→2 の結果が `[0, 1, 2, 1]` になる。初めの rank では 3 が 0 に置かれ、費用は 4。最適の費用は 3。
- **`respects_minlen`**：0→1（minlen 3）の結果が `[0, 3]` になる。
- **`zero_weight_free`**：0→1 の weight が 0 のとき、`rank(1) - rank(0) >= 1` を満たす。
- **`disconnected`**：node 3 個、辺 0 本の結果が `[0, 0, 0]` になる。
- **`offsets`**：node 0, 1, 2 と、辺 2→0（minlen 10、weight 1）、2→1（minlen 0、weight 1）の場合。結果では `rank(0) - 10 == rank(1)`、つまり `[10, 0, 0]` になる。
- **`cycle_is_internal_error`**：0→1、1→0 で `Err(LayoutError::Internal(_))` になる。

### 6.2 閉路の除去（`src/acyclic.rs`）

```rust
/// 重み付きの辺 (from, to, weight) の多重集合から、逆向きの辺の重みの和が小さくなる順位を返す。
/// 結果は node ごとの順位（0 から）。自己ループは無視する。
pub(crate) fn order(node_count: usize, edges: &[(usize, usize, i64)]) -> Vec<usize>
```

Eades–Lin–Smyth の貪欲法：

```
残りの node 集合 R = 全部。in(v), out(v) = R の中の辺の重みの和
s1 = [], s2 = []（s2 は後で逆順に連結する）
while R が空でない:
    while R に out(v) == 0 の v がある: index が最小の v を s2 に push、R から除く（隣の in/out を更新）
    while R に in(v) == 0 の v がある:  index が最小の v を s1 に push、R から除く
    if R が空でない: out(v) - in(v) が最大の v（同じなら index の最小）を s1 に push、R から除く
順序 = s1 ++ reverse(s2)
```

各手順の後に、R に out 0 の node が残っていないか確かめ直す（while の入れ子のとおり）。計算量は O(V²) でよい。

試験：
- **`triangle`**：0→1→2→0 の結果が `[0, 1, 2]` になる（逆向きは 2→0）。
- **`dag_keeps_order`**：0→2、1→2 で、`order[0] < order[2]` かつ `order[1] < order[2]` になる。
- **`weights_matter`**：0→1（重み 5）、1→0（重み 1）で、`order[0] < order[1]` になる。

---

## 7. 縦の座標（`src/position.rs`、L4）

```rust
/// item ごとの上辺の y。最小は 0。simplex の誤りはそのまま返す。
pub(crate) fn vertical(chains: &Chains, order: &Order) -> Result<Vec<i64>, LayoutError>
```

`simplex::Problem` を次のとおり作って解く。

1. **node。** item ごとに一つ（index は item と同じ）。続けて link ごとに一つ（index は `items.len() + link`）。
2. **間隔の辺。** 各層で上から順に、隣り合う a（上）、b（下）について、辺 `a → b` を足す。`minlen = height(a) + gap(a, b)`、`weight = 0`。`gap` は次の分類で決める。

   | a | b | gap |
   |---|---|---|
   | 大（Node・Box・Label） | 大 | `NODE_GAP` |
   | 大 | 小（Dummy・TurnEast・TurnWest・Entry・Exit） | `MIXED_GAP` |
   | 小 | 大 | `MIXED_GAP` |
   | 小 | 小 | `SMALL_GAP` |

3. **link の辺。** link e（left a の port `oa = order.east[e]`、right b の port `ob = order.west[e]`）ごとに、`c = max(oa, ob)` として次の二本を足す。重さは ω(e)。
   - `n_e → a`：`minlen = c - oa`
   - `n_e → b`：`minlen = c - ob`

   ω(e) は a、b がそれぞれ実体（Node・Box・Entry・Exit）か仮（Dummy・Label・TurnEast・TurnWest）かで決める。

   | a と b | ω(e) |
   |---|---|
   | 両方とも実体 | 1 |
   | 片方が仮 | 2 |
   | 両方とも仮 | 8 |

4. 解いて、item の rank をそのまま y とする。item の中で最小の y が 0 になるようずらす。

この作り方で、`|(y_a + oa) - (y_b + ob)|` の重み付きの和が最小になる。証明は不要。

試験：
- **`aligns_ports`**：層 0 の a（高さ 20）、層 1 の b（高さ 40）で、`east = 10`、`west = 20` の場合。結果は `y_a + 10 == y_b + 20` で、`[10, 0]` になる。
- **`keeps_gap`**：同じ層の a（高さ 30）、b（高さ 30）が両方 Node のとき、`y_b - y_a >= 70` になる。
- **`straight_dummy_chain`**：a → d → b（d は Dummy、a・b は高さ 40、port は 20）で、`y_a + 20 == y_d == y_b + 20` になる。

---

## 8. 通路と経路（`src/route.rs`、L5）

```rust
pub(crate) fn finish(input: &LevelInput, chains: &Chains, order: &Order, y: &[i64]) -> Result<LevelOutput, LayoutError>
```

### 8.1 通路の track

層 L と L+1 の間の通路ごとに、次を行う。

1. その間の link のうち、`ya = y[left] + east[e]` と `yb = y[right] + west[e]` が異なるものを集める（区画 `s`）。`lo = min(ya, yb)`、`hi = max(ya, yb)` とする。
2. 区画 s, t は、`s.lo < t.hi + TRACK_GAP` かつ `t.lo < s.hi + TRACK_GAP` のとき**衝突する**。
3. 衝突する組ごとに、どちらを左にするかの費用を求める。`[c]` は条件が真なら 1、偽なら 0。開区間の比較で、端の一致は数えない。
   - `cost(s 左, t 右) = [s.lo < t.ya < s.hi] + [t.lo < s.yb < t.hi]`
   - `cost(t 左, s 右) = [t.lo < s.ya < t.hi] + [s.lo < t.yb < s.hi]`
4. 依存のグラフを作る。index s < t の組で、次のとおり辺を足す。
   - `cost(s 左) < cost(t 左)` なら、辺 s→t（重み = 差）。
   - `cost(s 左) > cost(t 左)` なら、辺 t→s（重み = 差）。
   - 等しければ、辺を足さない。この組の左右は、手順 5 の順位で決まる。
5. `acyclic::order(区画の数, 依存の辺)` で順位を得る。
6. track を求める。衝突する組をすべて「順位の小さいほう → 大きいほう」に向ける。`track(t) = max(track(s) + 1)`、無ければ 0。これは順位の昇順に一回走査して求める。
7. 区画の数 0 のときの track の数 `T` は 0。それ以外は `T = max(track) + 1`。

### 8.2 横の座標

1. 層の幅 `W(L)` は、その層の item の幅の最大（無ければ 0）。
2. 通路の幅 `C(L)`（L と L+1 の間）を決める。
   - 基準 `base` は、Region の段で `L == 0` または `L + 1 == layer_count - 1` なら `REGION_PAD`、それ以外は `LAYER_GAP`。
   - `T == 0` なら `C = base`。それ以外は `C = max(base, 2 * TRACK_MARGIN + (T - 1) * TRACK_GAP)`。
3. 層の左端 `X(0) = 0`、`X(L+1) = X(L) + W(L) + C(L)`。
4. item の x は `X(L) + (W(L) - width) / 2`。
5. track k の x は `X(L) + W(L) + (C(L) - (T - 1) * TRACK_GAP) / 2 + k * TRACK_GAP`。
6. **Region の最小の幅。** `width_needed = header.width + 2 * REGION_PAD` とする。`X(layer_count - 1)`（Exit の層の x）がこれより小さければ、差を最後の通路 `C(layer_count - 2)` に足し、Exit の層の x を求め直す。
   - Region の段は Entry の層と Exit の層を必ず持つので、`layer_count >= 2`。

### 8.3 折れ線

segment ごとに、path の Step を順にたどる。

1. link e の点列を作る。順方向の点列は次のとおり。
   - `ya != yb` のとき：`(xa, ya), (xt, ya), (xt, yb), (xb, yb)`。
   - `ya == yb` のとき：`(xa, ya), (xb, yb)`。
   - `xa = x[left] + width[left]`（left の右辺）、`xb = x[right]`（right の左辺）、`xt` は track の x。
   - Step が `forward: false` なら、この点列を逆順にする。
2. Step の点列を順につなぐ。前の点列の最後の点と次の点列の最初の点は、両方残す。この二点は必ず軸に沿って並ぶ。
   - U ターンの item では、x が同じで y が異なる。間の線は縦の線になる。
   - 幅のある Label の item では、y が同じで x が異なる。間の線は横の線になる。
   - Dummy では同じ点になる。
3. **簡約。** 同じ点が続いたら一つにする。同じ直線の上の三点（x がすべて同じ、または y がすべて同じ）は中の点を消す。
4. **ラベル。** path の中の Label の item の矩形をそのまま使う（x は 8.2、y は `y[item]`、大きさは item の大きさ）。

### 8.4 段の矩形と出力

**Root：**
1. 全 item の矩形（入力の item とダミー）と全点の外接を求め、左上が (0, 0) になるようずらす。
2. `size` はその外接の大きさ。

**Region：**
1. `[top, bottom]` を、chains のすべての item（Entry と Exit を含む）の `[y, y + height]` と、すべての経路の点の y を含む最小の範囲とする。item も点も無いとき（中身の無い region）は `top = bottom = 0`。
2. 全体を `dy = header.height + REGION_PAD - top` だけ下にずらす。
3. `size = (X(layer_count - 1), header.height + REGION_PAD + (bottom - top) + REGION_PAD)`。高さの最小は `header.height + 2 * REGION_PAD`。
4. Entry の矩形は `(0, y, 0, 0)`、Exit の矩形は `(size.width, y, 0, 0)`。

`rects` は入力の item（先頭の `input.items.len()` 個）だけを返す。`pieces` は segment の順に返す。

### 8.5 試験

- **`two_segments_need_two_tracks`**：8.1 節の手順の例。s1 が y 0 → 100、s2 が y 50 → 150 のとき、s2 の track が 0、s1 の track が 1 になる。
- **`straight_link_has_no_track`**：ya == yb の link だけのとき `T == 0` になる。
- **`route_is_orthogonal`**：小さな二層の段で、すべての隣り合う点で x か y が等しい。
- **`region_has_min_width`**：見出しの幅 300、中身が小さい Region で、`size.width >= 340` になる。

---

## 9. 階層（`src/hierarchy.rs`、L7）と `run_level`（L7）

### 9.1 `run_level`

```rust
pub(crate) fn run_level(input: &LevelInput) -> Result<LevelOutput, LayoutError> {
    let layers = crate::layering::assign(input)?;
    let chains = crate::chains::build(input, &layers)?;
    let order = crate::order::arrange(&chains)?;
    let y = crate::position::vertical(&chains, &order)?;
    crate::route::finish(input, &chains, &order, &y)
}
```

L1 の骨組みでは本体は `todo!("L7")`。L7 がこのとおりに書く。

### 9.2 段の木

- 段の番号は、0 が最上段、`r + 1` が region r。
- `children_regions(段)` と `nodes(段)` を index の昇順で作る。
- 下からの順序は、region の木の後順にする。子は index の昇順にたどり、最後に段 0 を置く。

### 9.3 辺の分解

辺 e = (u, v) ごとに、次を行う。

1. `up(u)` を作る。u の段から段 0 までの段の列（u の段を含む）。`up(v)` も同じ。
2. LCA は `up(u)` と `up(v)` に共通する最初の段。
3. u 側の段の列 `U = up(u)` の LCA より前の部分（u の段から LCA の一つ手前まで）について、段 S ごとに区間を作る。
   - 始点：S が u の段なら node u の item、そうでなければ S の子 region（`U` の中で S の一つ手前の段）の Box。
   - 終点：S の Exit(e)。
4. LCA の段の区間を作る。始点は u 側の要素（u か、LCA の子の Box）、終点は v 側の要素（v か Box）。ラベルはこの区間だけに付ける。
5. v 側も同じように作る。始点は S の Entry(e)、終点は v か子の Box。
6. 辺の区間の順序は、u 側（内から外）、LCA、v 側（外から内）。

### 9.4 段の入力を作る

段 S の item は次の順に並べる。

1. S の node（index の昇順）。
2. S の子 region の Box（index の昇順）。
3. Entry(e)（e の昇順）。
4. Exit(e)（e の昇順）。

- Box の `size` と `entries`・`exits` は、子の段の `LevelOutput` から取る。Entry(e) の rect の y が entry の y、Exit(e) の rect の y が exit の y になる。
- segment は辺の index の昇順に並べる。
- frame は、段 0 なら `Root`（`Layering` から作る。Fixed の列は node の item だけで、段 0 の item は node だけ）、それ以外は `Region { header }`。

### 9.5 接続と出力

1. 段ごとの原点を求める。段 0 は `(DIAGRAM_PAD, DIAGRAM_PAD)`。region r は、親の段の原点に、親の段での Box の矩形の x, y を足したもの。
2. node の矩形は、その段の原点に、段の中の矩形を足したもの。
3. region の矩形は、親の段の原点に、Box の矩形を足したもの。
4. 辺の点列は、区間の順に各段の原点を足してつなぎ、8.3 節の簡約をかける。
5. ラベルは LCA の段の区間のもの。
6. 図の大きさは、段 0 の大きさ + `2 * DIAGRAM_PAD`。

### 9.6 `layout`

`lib.rs` の `layout` は L1 が書いたもの（1.4 節）。L7 は `hierarchy::layout` の本体と `level::run_level` の本体を書く。

- **空の要求。** `graph.nodes` も `graph.regions` も空なら、`run_level` を呼ばずに `Layout { size: Size { width: 2 * DIAGRAM_PAD, height: 2 * DIAGRAM_PAD }, nodes: vec![], regions: vec![], edges: vec![] }` を返す（edge は node が無いので必ず空。`check::input` が保証する）。
- region があれば段 0 には Box の item があるので、段 0 が空になることは無い。中身の無い region は、8.4 節の規則で見出しと余白だけの箱になる。

---

## 10. 検証と指標（`src/check.rs`、`src/metrics.rs`、L6）

### 10.1 入力の検証 `check::input(graph) -> Result<(), LayoutError>`

最初に数を調べる：`nodes.len() > MAX_NODES`、`regions.len() > MAX_REGIONS`、`edges.len() > MAX_EDGES` のどれかなら `TooMany`。その後、`LayoutError` の残りの variant の条件を、variant の並びの順に調べる。大きさは、node の size、region の header、ラベルの各値が 0 以上 `MAX_SIZE` 以下であること。`Layering::Fixed` の列番号の値には上限を設けない（3.2 節で詰める）。

### 10.2 出力の不変条件 `check::output(graph, layout) -> Result<(), String>`

公開版の `verify`（L1 が `lib.rs` に書いたもの）は、これを呼ぶだけ。違反の最初の一つを文で返す。

1. 配列の長さが要求と同じ。
2. 矩形はすべて幅・高さ 0 以上で、図の範囲 `[0, size]` の中にある。
3. **大きさ。** node の矩形は要求の `size` 以上（幅は等しいか大きい、高さは等しいか大きい）。ラベルの矩形の幅と高さは要求のラベルとちょうど等しい。region の矩形は、幅が `header.width + 2 * REGION_PAD` 以上、高さが `header.height + 2 * REGION_PAD` 以上。
4. node は、その region の矩形の内側にある。region は、親の region の矩形の内側にある。どちらも、親の region の見出しの帯（`[region.y, region.y + header.height + REGION_PAD)`）にかからない（子の上辺の y が `region.y + header.height + REGION_PAD` 以上）。
5. 祖先と子孫の関係に無い二つの要素（node と node、node と region、region と region）の矩形が重ならない。共有する辺は重なりとみなさない。
6. 経路は次をすべて満たす。
   - 点が 2 個以上ある。
   - 隣り合う点は x か y の一方だけが異なる。
   - 長さ 0 の線分が無い。
7. 経路の始点は始点の node の右辺上（`x == rect.x + width`、y が範囲内）にあり、最初の線分は右へ向かう。終点は終点の node の左辺上にあり、最後の線分は右へ向かう。
8. 経路の線分は、端以外の node の矩形の内部を通らない。端の node も、最初と最後の線分以外は通らない。内部とは境界を除く部分。
9. ラベルは、要求にラベルがあるときだけある。ラベルは node の矩形と重ならない。

計算量：5 と 8 は、全組を単純に比べる（5 は要素の組、8 は線分と node の組）。基準の最大（node 199、辺 679）で百万回程度の比較であり、K4 で監視する。x で並べた二分探索で絞る方式は、矩形の右端が x の順に並ばないため正しくないので使わない。

### 10.3 指標 `metrics::compute(width, height, routes) -> Metrics`（公開版は `lib.rs` の `metrics`）

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    /// 異なる辺の、水平な線分と垂直な線分の内部どうしの交差の数。
    pub crossings: usize,
    /// 折れの数の和（簡約した点の数 - 2）。
    pub bends: usize,
    pub width: f64,
    pub height: f64,
    /// 線分の長さの和。
    pub length: f64,
}
```

ELK の基準の折れ線にも同じ関数を使うため、引数は f64 にする。簡約は 8.3 節と同じ規則で行う（差の絶対値 1e-6 以下を同じとみなす）。

### 10.4 試験

L6 の試験は、すべて手で作った `Graph` と `Layout` で行い、`layout` を呼ばない（1.3 節）。基準にする正しい Layout は次のもの：node a（80×40）、b（80×40）、辺 a→b。`size = 248×88`、a の矩形 `(24, 24, 80, 40)`、b の矩形 `(144, 24, 80, 40)`、辺の点 `[(104, 44), (144, 44)]`。

- **`verify_accepts_hand_made_layout`**：上の Layout を verify が受け入れる。
- **`verify_rejects_overlap`**：b の矩形を `(90, 24, 80, 40)` にすると拒む。
- **`verify_rejects_shrunken_node`**：a の矩形の幅を 70 にすると拒む（辺の始点も `(94, 44)` に合わせる）。
- **`verify_rejects_header_overlap`**：region r（header 60×16）に a を入れ、a の上辺を `r.y + 10` に置いた Layout を拒む。
- **`verify_rejects_label_size`**：ラベル（30×10）付きの辺で、ラベルの矩形を 20×10 にすると拒む。
- **`input_rejects_too_many`**：`MAX_NODES + 1` 個の node（大きさ 0）の Graph で、`check::input` が `TooMany`。
- **`metrics_counts_one_crossing`**：十字の二本の線で crossings が 1 になる。

---

## 11. 基準との比較（L8）

### 11.1 変換（`tests/baseline.rs` と `examples/baseline_report.rs` で共有する `tests/common/baseline.rs`）

- 共有の方法：試験では `mod common;`（`tests/common/mod.rs` が `pub mod baseline;` を持つ）、example では `#[path = "../tests/common/baseline.rs"] mod baseline;` で読む。
- 基準の directory は `Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/layout-baseline")`。その下の `kivact/` と `xtask-core/` の `*.json` を、ファイル名の昇順に読む。
- JSON は `serde_json::Value` として読む（dev-dependency）。

`dev/layout-baseline/<group>/*.json` の `input` を、次のとおり `Graph` に変換する。

- **node**：`input.nodes` の順に変換し、id から index への表を作る。
  - size は `width.ceil() as i64`、`height.ceil() as i64`。
  - region は、下の region の変換で埋める。`Graph` を組み立てる前に表で決める。
- **region**：`input.regions` を木の前順でたどり、index を振る。
  - parent は木の親。
  - header は `(label_width.ceil(), label_height.ceil())`。
  - `nodes` に挙がる node の region を、この region にする。
- **edge**：`input.edges` の順に変換する。
  - label は、`input.label_metrics[edge.id]` があれば、その width・height の ceil。
- **layering**：`input.ranks` が空でなければ、node ごとにその node を含む rank の番号を入れて `Fixed` にする。空なら `Optimize`。
- **ELK の折れ線**：`scene.edges[].points` の (x, y)。大きさは `scene.width`、`scene.height`。

### 11.2 試験 `tests/baseline.rs`

```rust
#[test]
fn every_baseline_input_lays_out() // 全 146 件で layout が Ok、verify が Ok
#[test]
fn layout_is_deterministic()       // 全件で 2 回の結果が等しい
```

この試験は時間がかかりうるので、unit の確認では実行しない。Orchestrator が W3 の tier b と CP で `--release` で実行する。

### 11.3 比較表 `examples/baseline_report.rs`

`cargo run -p xtask-layout --release --example baseline_report` は次を出力する。

1. markdown の表を標準出力に出す。
   - 行は図ごと。列は group、name、nodes、edges、crossings（ELK / 本実装）、bends（ELK / 本実装）、area（ELK / 本実装）、時間（ms）。
   - 最後に次の行を付ける（設計書 2 章の測り方）。
     - 全 146 件の合計の行と、比（本実装 ÷ ELK）の行（K1〜K3）。ELK の和が 0 の列は、本実装も 0 なら比を `1.00`、そうでなければ `inf` と書く。
     - `kivact` の 121 件の時間の和（K4）。
2. 同じ内容を `target/layout-baseline-report.md` に書く。

時間は `std::time::Instant` で `layout` だけを測る。1 thread で、まず全件を一度配置し（暖機、測らない）、その後に各件を一度ずつ配置して測る。

---

## 12. 配置の時間の改善（L9、W5）

W3 の tier b の KPI の差戻し 17（K4 が 19.6 秒）への対応。設計者が測定し、時間の 99% が `position` の network simplex であることを確かめた。

- **所有**：`crates/xtask-layout/src/simplex.rs`、`src/route.rs` の `push_simplified` と `collinear`、`src/hierarchy.rs` の同名の関数、`rework/reference/simplex.rs.txt`
- **手順**：
  1. `crates/xtask-layout/src/simplex.rs` を消し、`git mv rework/reference/simplex.rs.txt crates/xtask-layout/src/simplex.rs` で置き換える。中身は一字一句変えない（6.1 節の改訂のとおり。`rework/reference/` は空になるので消える）。
  2. `hierarchy.rs` の `push_simplified` と `collinear` は `route.rs` の同名の関数の複製なので消す。`route.rs` の二つを `pub(crate)` にし、`hierarchy.rs` はそれを `use` する（W3 の L7 の報告）。
- **試験**：新しい試験は足さない。`simplex::tests` の 7 件はそのまま通る（置き換えるファイルに含まれる）。
- **確認**：
  ```bash
  cargo check -p xtask-layout --all-targets
  cargo test -p xtask-layout --lib
  cargo clippy -p xtask-layout --all-targets -- -D warnings
  ```
  基準の全件の試験と比較表（K1〜K4）は、W5 の tier b で Orchestrator が実行する。

