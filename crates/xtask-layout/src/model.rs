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
            Self::NodeRegion { node } => {
                write!(formatter, "node {node} refers to a missing region")
            }
            Self::RegionParent { region } => {
                write!(formatter, "region {region} has an invalid parent")
            }
            Self::RegionCycle { region } => {
                write!(formatter, "region {region} is in a parent cycle")
            }
            Self::EdgeEndpoint { edge } => {
                write!(formatter, "edge {edge} refers to a missing node")
            }
            Self::SelfLoop { edge } => {
                write!(formatter, "edge {edge} starts and ends at the same node")
            }
            Self::InvalidSize => write!(formatter, "a size is negative or larger than 100000"),
            Self::FixedLength => write!(formatter, "fixed layering must give one column per node"),
            Self::FixedWithRegions => {
                write!(formatter, "fixed layering cannot be used with regions")
            }
            Self::TooMany => write!(
                formatter,
                "the graph has more than 100000 nodes or regions, or more than 1000000 edges"
            ),
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
