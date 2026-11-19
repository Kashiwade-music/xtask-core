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
