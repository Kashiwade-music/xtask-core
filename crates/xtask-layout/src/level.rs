//! Role: 平らな段の要素・区間・中間結果の型を定義する。
use crate::model::{LayoutError, Point, Rect, Size};

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

/// chains の参照（link の両端と、道のりの link の index）が範囲内にあることを確かめる。
pub(crate) fn check_chains(chains: &Chains) -> Result<(), LayoutError> {
    let item_count = chains.items.len();
    let link_count = chains.links.len();
    if chains
        .links
        .iter()
        .any(|link| link.left >= item_count || link.right >= item_count)
    {
        return Err(LayoutError::Internal(
            "link refers to a missing item".to_string(),
        ));
    }
    if chains
        .paths
        .iter()
        .flatten()
        .any(|step| step.link >= link_count)
    {
        return Err(LayoutError::Internal(
            "path refers to a missing link".to_string(),
        ));
    }
    Ok(())
}

/// order が chains と合うことを確かめる。port は link ごと、層は各 item を一度ずつ含む。
pub(crate) fn check_order(chains: &Chains, order: &Order) -> Result<(), LayoutError> {
    let link_count = chains.links.len();
    if order.east.len() != link_count || order.west.len() != link_count {
        return Err(LayoutError::Internal(
            "port count differs from the link count".to_string(),
        ));
    }
    let mut seen = vec![0usize; chains.items.len()];
    for &item in order.layers.iter().flatten() {
        let count = seen
            .get_mut(item)
            .ok_or_else(|| LayoutError::Internal("layer refers to a missing item".to_string()))?;
        *count += 1;
    }
    if seen.iter().any(|&count| count != 1) {
        return Err(LayoutError::Internal(
            "layers do not list each item once".to_string(),
        ));
    }
    Ok(())
}
