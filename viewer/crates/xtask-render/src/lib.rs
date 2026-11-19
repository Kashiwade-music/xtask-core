//! Role: 図の Scene から、描く物の一覧（Picture）と近傍・矩形を作る公開の入口を定義する。
mod bridge;
mod build;
mod highlight;
mod model;
mod route;
#[cfg(test)]
mod tests;

pub mod palette;

pub use model::*;

use xtask_archive::Scene;

/// Scene から Picture を作る。座標は Picture の座標（Scene + OFFSET）。
pub fn picture(title: &str, scene: &Scene) -> Picture {
    build::picture(title, scene)
}

/// item を端に持つ edge と、その反対の端の item。item 自身も含む。
pub fn neighbors(scene: &Scene, item: u32) -> Highlight {
    highlight::neighbors(scene, item)
}

/// item の矩形（Picture の座標）。region があればその region、無ければ node。
pub fn item_rect(picture: &Picture, item: u32) -> Option<R> {
    highlight::item_rect(picture, item)
}
