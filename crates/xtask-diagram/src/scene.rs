//! Role: 配置の結果を図のファイルの Scene に写し、座標の範囲を確かめる。
use xtask_archive::{Edge, Label, Node, Point, Rect, Region, Scene};
use xtask_layout::Layout;

use crate::model::Drawing;

const RANGE_ERROR: &str = "diagram exceeds the 32-bit coordinate range";

/// 一つの図を配置し、Scene に写す。失敗の文を Err で返す。
pub(crate) fn draw_graph(drawing: &Drawing) -> Result<Scene, String> {
    let layout = xtask_layout::layout(&drawing.graph).map_err(|error| error.to_string())?;
    to_scene(drawing, &layout)
}

fn to_scene(drawing: &Drawing, layout: &Layout) -> Result<Scene, String> {
    Ok(Scene {
        width: px(layout.size.width)?,
        height: px(layout.size.height)?,
        items: drawing.items.clone(),
        nodes: nodes(drawing, layout)?,
        regions: regions(drawing, layout)?,
        edges: edges(drawing, layout)?,
    })
}

fn nodes(drawing: &Drawing, layout: &Layout) -> Result<Vec<Node>, String> {
    drawing
        .nodes
        .iter()
        .zip(&layout.nodes)
        .map(|((item, lines), rect)| {
            Ok(Node {
                item: *item,
                rect: rect_of(*rect)?,
                lines: lines.clone(),
            })
        })
        .collect()
}

fn regions(drawing: &Drawing, layout: &Layout) -> Result<Vec<Region>, String> {
    drawing
        .regions
        .iter()
        .zip(&layout.regions)
        .zip(&drawing.graph.regions)
        .map(|(((item, lines), rect), region)| {
            Ok(Region {
                item: *item,
                parent: region.parent.map(|parent| parent as u32),
                rect: rect_of(*rect)?,
                lines: lines.clone(),
            })
        })
        .collect()
}

fn edges(drawing: &Drawing, layout: &Layout) -> Result<Vec<Edge>, String> {
    drawing
        .graph
        .edges
        .iter()
        .zip(&layout.edges)
        .zip(&drawing.edges)
        .map(|((edge, route), (style, lines))| {
            Ok(Edge {
                from: edge.from as u32,
                to: edge.to as u32,
                style: *style,
                points: route
                    .points
                    .iter()
                    .map(|point| {
                        Ok(Point {
                            x: px(point.x)?,
                            y: px(point.y)?,
                        })
                    })
                    .collect::<Result<_, String>>()?,
                label: label(route.label, lines)?,
            })
        })
        .collect()
}

/// 辺のラベル。配置の結果と要求は同じ有無でなければならない。
fn label(
    rect: Option<xtask_layout::Rect>,
    lines: &Option<Vec<String>>,
) -> Result<Option<Label>, String> {
    match (rect, lines) {
        (Some(rect), Some(lines)) => Ok(Some(Label {
            rect: rect_of(rect)?,
            lines: lines.clone(),
        })),
        (None, None) => Ok(None),
        _ => Err("internal layout error: an edge label is missing from the layout".into()),
    }
}

fn rect_of(rect: xtask_layout::Rect) -> Result<Rect, String> {
    Ok(Rect {
        x: px(rect.x)?,
        y: px(rect.y)?,
        width: px(rect.width)?,
        height: px(rect.height)?,
    })
}

fn px(value: i64) -> Result<i32, String> {
    i32::try_from(value).map_err(|_| RANGE_ERROR.into())
}
