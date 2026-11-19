//! Role: item の近傍（依存先・依存元と辺）と、item の矩形を求める。
use xtask_archive::Scene;

use crate::model::{Highlight, Picture, R};

/// item を端に持つ edge と、その反対の端の item。item 自身も含む。
pub fn neighbors(scene: &Scene, item: u32) -> Highlight {
    let mut highlight = Highlight::default();
    highlight.items.insert(item);
    for (index, edge) in scene.edges.iter().enumerate() {
        let (Some(from), Some(to)) = (
            scene.nodes.get(edge.from as usize),
            scene.nodes.get(edge.to as usize),
        ) else {
            continue;
        };
        if from.item == item || to.item == item {
            highlight
                .edges
                .insert(u32::try_from(index).unwrap_or(u32::MAX));
            highlight.items.insert(from.item);
            highlight.items.insert(to.item);
        }
    }
    highlight
}

/// item の矩形（Picture の座標）。region があればその region、無ければ node。
pub fn item_rect(picture: &Picture, item: u32) -> Option<R> {
    picture
        .regions
        .iter()
        .find(|region| region.item == item)
        .map(|region| region.rect)
        .or_else(|| {
            picture
                .nodes
                .iter()
                .find(|node| node.item == item)
                .map(|node| node.rect)
        })
}
