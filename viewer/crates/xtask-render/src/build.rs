//! Role: Scene から Picture を組み立てる（題・余白・region・node・edge の座標の規則）。
use xtask_archive::style::{
    EDGE_FONT_SIZE, LINE_HEIGHT, NODE_FONT_SIZE, REGION_FONT_SIZE, REGION_LINE_HEIGHT,
    TITLE_FONT_SIZE, advance,
};
use xtask_archive::{EdgeStyle, Label, Region, Scene};

use crate::{EdgeShape, NodeShape, P, Picture, R, RegionShape, Rgb, Text, bridge, palette, route};

/// 図の中身をずらす量（題の下、左の余白）。
const OFFSET: P = P { x: 16.0, y: 42.0 };

/// Scene から Picture を作る。座標は Picture の座標（Scene + OFFSET）。
pub fn picture(title: &str, scene: &Scene) -> Picture {
    let title_width: f64 = title
        .chars()
        .map(|character| advance(character, TITLE_FONT_SIZE))
        .sum();
    let width = f64::from(scene.width).max(title_width) + 32.0;
    let height = f64::from(scene.height) + 58.0;
    let edge_points: Vec<Vec<P>> = scene
        .edges
        .iter()
        .map(|edge| {
            edge.points
                .iter()
                .map(|point| shift(f64::from(point.x), f64::from(point.y)))
                .collect()
        })
        .collect();
    Picture {
        width,
        height,
        title: Text {
            center_x: width / 2.0,
            baseline: 21.0 + TITLE_FONT_SIZE * 0.35,
            size: TITLE_FONT_SIZE,
            text: title.into(),
        },
        regions: regions(scene),
        edges: edges(scene, &edge_points),
        bridges: bridge::find(&edge_points),
        nodes: nodes(scene),
    }
}

fn shift(x: f64, y: f64) -> P {
    P {
        x: x + OFFSET.x,
        y: y + OFFSET.y,
    }
}

fn shift_rect(rect: xtask_archive::Rect) -> R {
    let origin = shift(f64::from(rect.x), f64::from(rect.y));
    R {
        x: origin.x,
        y: origin.y,
        width: f64::from(rect.width),
        height: f64::from(rect.height),
    }
}

/// 親から子の順（深さの昇順、同じ深さは index の順）に region を並べる。
fn regions(scene: &Scene) -> Vec<RegionShape> {
    let mut order: Vec<(usize, usize)> = (0..scene.regions.len())
        .map(|index| (depth(scene, index), index))
        .collect();
    order.sort_unstable();
    order
        .into_iter()
        .map(|(depth, index)| region_shape(&scene.regions[index], depth))
        .collect()
}

/// 最上位を 0 とする深さ。循環は validate で除かれているが、念のため長さで止める。
fn depth(scene: &Scene, index: usize) -> usize {
    let mut depth = 0;
    let mut current = scene.regions[index].parent;
    while let Some(parent) = current {
        depth += 1;
        if depth > scene.regions.len() {
            break;
        }
        current = scene
            .regions
            .get(parent as usize)
            .and_then(|region| region.parent);
    }
    depth
}

fn region_shape(region: &Region, depth: usize) -> RegionShape {
    let rect = shift_rect(region.rect);
    let fill = (depth > 0).then(|| region_background(depth));
    let band = R {
        x: rect.x + 3.0,
        y: rect.y + 3.0,
        width: rect.width - 6.0,
        height: region.lines.len() as f64 * REGION_LINE_HEIGHT + 4.0,
    };
    let texts = lines_at(
        &region.lines,
        rect.x + rect.width / 2.0,
        band.y + band.height / 2.0,
        REGION_FONT_SIZE,
        REGION_LINE_HEIGHT,
    );
    RegionShape {
        item: region.item,
        rect,
        fill,
        band,
        band_fill: fill.unwrap_or(palette::BACKGROUND),
        texts,
    }
}

/// 深さ d の背景色。明るさは 0.8^d（slate-300 へ少しずつ近づける）。
fn region_background(depth: usize) -> Rgb {
    let lightness = 0.8_f64.powi(i32::try_from(depth).unwrap_or(i32::MAX));
    let channel = |base: f64| (base + (255.0 - base) * lightness).round() as u8;
    Rgb(channel(203.0), channel(213.0), channel(225.0))
}

/// 行ごとの文字。`center_y` を行の中央として、旧い描画と同じ式で baseline を決める。
fn lines_at(
    lines: &[String],
    center_x: f64,
    center_y: f64,
    size: f64,
    line_height: f64,
) -> Vec<Text> {
    let first = center_y - (lines.len().saturating_sub(1) as f64 * line_height) / 2.0 + size * 0.35;
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| Text {
            center_x,
            baseline: first + index as f64 * line_height,
            size,
            text: line.clone(),
        })
        .collect()
}

fn nodes(scene: &Scene) -> Vec<NodeShape> {
    scene
        .nodes
        .iter()
        .map(|node| {
            let rect = shift_rect(node.rect);
            let texts = lines_at(
                &node.lines,
                rect.x + rect.width / 2.0,
                rect.y + rect.height / 2.0,
                NODE_FONT_SIZE,
                LINE_HEIGHT,
            );
            NodeShape {
                item: node.item,
                rect,
                texts,
            }
        })
        .collect()
}

fn edges(scene: &Scene, points: &[Vec<P>]) -> Vec<EdgeShape> {
    scene
        .edges
        .iter()
        .zip(points)
        .map(|(edge, points)| EdgeShape {
            from_item: scene.nodes[edge.from as usize].item,
            to_item: scene.nodes[edge.to as usize].item,
            dotted: edge.style == EdgeStyle::Dev,
            path: route::path(points),
            arrow: route::arrow(points),
            label: edge.label.as_ref().map(label_shape),
        })
        .collect()
}

/// ラベルの白い矩形は、枠を (-2, -1) 広げたもの。文字は枠の中央。
fn label_shape(label: &Label) -> (R, Vec<Text>) {
    let rect = shift_rect(label.rect);
    let backdrop = R {
        x: rect.x - 2.0,
        y: rect.y - 1.0,
        width: rect.width + 4.0,
        height: rect.height + 2.0,
    };
    let texts = lines_at(
        &label.lines,
        rect.x + rect.width / 2.0,
        rect.y + rect.height / 2.0,
        EDGE_FONT_SIZE,
        LINE_HEIGHT,
    );
    (backdrop, texts)
}
