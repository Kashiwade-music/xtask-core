use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Direction {
    LeftToRight,
    TopToBottom,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EdgeStyle {
    Normal,
    Dev,
    Unconstrained,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutRequest {
    pub title: String,
    pub direction: Direction,
    pub nodes: Vec<NodeRequest>,
    pub regions: Vec<RegionRequest>,
    pub edges: Vec<EdgeRequest>,
    /// Normal dependency levels for flat left-to-right crate graphs.
    /// Empty for module graphs. Ranked top-to-bottom layouts are rejected because
    /// their row ports cannot all remain on the node's left/right sides.
    pub ranks: Vec<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeRequest {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RegionRequest {
    pub id: String,
    pub label: String,
    pub nodes: Vec<String>,
    pub children: Vec<RegionRequest>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EdgeRequest {
    pub id: String,
    pub from: String,
    pub to: String,
    pub label: Option<String>,
    pub style: EdgeStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub title: String,
    pub width: f64,
    pub height: f64,
    pub nodes: Vec<SceneNode>,
    pub regions: Vec<SceneRegion>,
    pub edges: Vec<SceneEdge>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneNode {
    pub id: String,
    pub label: String,
    pub lines: Vec<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneRegion {
    pub id: String,
    pub parent: Option<String>,
    pub label: String,
    pub lines: Vec<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// An edge label's x/y represent its center in root coordinates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneLabel {
    pub text: String,
    pub lines: Vec<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub points: Vec<Point>,
    pub label: Option<SceneLabel>,
    pub style: EdgeStyle,
}

pub type Request = LayoutRequest;
pub const NODE_FONT_SIZE: f64 = 14.0;
pub const EDGE_FONT_SIZE: f64 = 14.0;
pub const REGION_FONT_SIZE: f64 = 12.0;
pub const TITLE_FONT_SIZE: f64 = 18.0;
pub const LINE_HEIGHT: f64 = 18.0;
pub const REGION_LINE_HEIGHT: f64 = 16.0;
pub const FONT_FAMILY: &str = "Consolas, Noto Sans Mono, monospace";
