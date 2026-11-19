//! Role: 図の Scene から、描く物の一覧（Picture）と近傍・矩形を作る公開の入口を定義する。
mod bridge;
mod build;
mod highlight;
mod route;
#[cfg(test)]
mod tests;

pub mod palette;

pub use build::picture;
pub use highlight::{item_rect, neighbors};

use std::collections::BTreeSet;

/// 点（Picture の座標、px）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct P {
    pub x: f64,
    pub y: f64,
}

/// 矩形（Picture の座標、px）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct R {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// 色（RGB）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// 経路の命令。
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Move(P),
    Line(P),
    Quad { control: P, to: P },
}

/// 一行の文字。`center_x` を中心に、`baseline` の高さに描く。
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub center_x: f64,
    pub baseline: f64,
    pub size: f64,
    pub text: String,
}

/// node の描画。
#[derive(Clone, Debug, PartialEq)]
pub struct NodeShape {
    pub item: u32,
    pub rect: R,
    pub texts: Vec<Text>,
}

/// region の描画。`band` は見出しの帯。
#[derive(Clone, Debug, PartialEq)]
pub struct RegionShape {
    pub item: u32,
    pub rect: R,
    pub fill: Option<Rgb>,
    pub band: R,
    pub band_fill: Rgb,
    pub texts: Vec<Text>,
}

/// edge の描画。`label` は白い矩形と文字。
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeShape {
    pub from_item: u32,
    pub to_item: u32,
    pub dotted: bool,
    pub path: Vec<Cmd>,
    pub arrow: [P; 3],
    pub label: Option<(R, Vec<Text>)>,
}

/// 交差の跳び。edge の path の上に、全 edge の後に描く。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bridge {
    pub edge: u32,
    pub center: P,
    pub radius: f64,
}

/// 座標はすべて Picture の座標（Scene の座標 + (16, 42)）。
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub width: f64,
    pub height: f64,
    pub title: Text,
    pub regions: Vec<RegionShape>,
    pub edges: Vec<EdgeShape>,
    pub bridges: Vec<Bridge>,
    pub nodes: Vec<NodeShape>,
}

/// 注目する item と、その依存の辺・item。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Highlight {
    pub items: BTreeSet<u32>,
    pub edges: BTreeSet<u32>,
}
