//! Role: region の木を下から配置し、region をまたぐ辺をつなぐ（付属書 L の 9 章）。
use std::collections::BTreeMap;

use crate::level::{
    BoxItem, Frame, Item, ItemId, ItemKind, LevelInput, LevelOutput, RootLayering, Segment,
    run_level,
};
use crate::model::{
    DIAGRAM_PAD, Edge, Graph, Layering, Layout, LayoutError, Point, Rect, Route, Size,
};

/// 段の中で区間の端になるもの（Graph の index で書く）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Anchor {
    /// 原子の node。
    Node(usize),
    /// 子 region の Box（region の index）。
    Child(usize),
    /// 辺の入口（左辺）と出口（右辺）。辺の index。
    Entry(usize),
    Exit(usize),
}

/// 段に置く区間の下書き。辺 `edge` が `from` の右辺から `to` の左辺へ進む。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    edge: usize,
    from: Anchor,
    to: Anchor,
    label: Option<Size>,
}

/// 段ごとの下書き。段の番号は 0 が最上段、r + 1 が region r。
struct Tree {
    /// 段ごとの node（index の昇順）。
    nodes: Vec<Vec<usize>>,
    /// 段ごとの子 region（index の昇順）。
    regions: Vec<Vec<usize>>,
    /// 段ごとの区間（辺の index の昇順）。
    spans: Vec<Vec<Span>>,
    /// 辺ごとの区間の並び（段の番号と、その段の spans の index）。
    pieces: Vec<Vec<(usize, usize)>>,
}

/// 配置の済んだ段。`output.rects` は `input.items` と同じ index。
struct Built {
    input: LevelInput,
    output: LevelOutput,
}

pub(crate) fn layout(graph: &Graph) -> Result<Layout, LayoutError> {
    if graph.nodes.is_empty() && graph.regions.is_empty() {
        let pad = 2 * DIAGRAM_PAD;
        return Ok(Layout {
            size: Size {
                width: pad,
                height: pad,
            },
            nodes: Vec::new(),
            regions: Vec::new(),
            edges: Vec::new(),
        });
    }
    let tree = plan(graph);
    let built = solve(graph, &tree)?;
    assemble(graph, &tree, &built)
}

/// 段の番号。region r の段は r + 1。
fn region_level(region: usize) -> usize {
    region + 1
}

/// node の region から見た段。region が無ければ最上段。
fn node_level(region: Option<usize>) -> usize {
    region.map_or(0, region_level)
}

/// 段の親の段。最上段には親が無い。
fn parent_level(graph: &Graph, level: usize) -> Option<usize> {
    if level == 0 {
        None
    } else {
        Some(node_level(graph.regions[level - 1].parent))
    }
}

/// 段から最上段までの段の列（段自身を先頭にして、上へ並べる）。
fn ancestors(graph: &Graph, level: usize) -> Vec<usize> {
    let mut chain = vec![level];
    let mut current = level;
    while let Some(parent) = parent_level(graph, current) {
        chain.push(parent);
        current = parent;
    }
    chain
}

/// 段ごとの node・子 region・区間を作り、辺を区間に分ける。
fn plan(graph: &Graph) -> Tree {
    let levels = graph.regions.len() + 1;
    let mut tree = Tree {
        nodes: vec![Vec::new(); levels],
        regions: vec![Vec::new(); levels],
        spans: vec![Vec::new(); levels],
        pieces: vec![Vec::new(); graph.edges.len()],
    };
    for (node, found) in graph.nodes.iter().enumerate() {
        tree.nodes[node_level(found.region)].push(node);
    }
    for (region, found) in graph.regions.iter().enumerate() {
        tree.regions[node_level(found.parent)].push(region);
    }
    for (index, edge) in graph.edges.iter().enumerate() {
        decompose(graph, index, *edge, &mut tree);
    }
    tree
}

/// 段 `chain[index]` の中で、辺の `node` 側の端。段の一番内側は node 自身、そうでなければ一つ下の段の Box。
fn below(chain: &[usize], index: usize, node: usize) -> Anchor {
    if index == 0 {
        Anchor::Node(node)
    } else {
        Anchor::Child(chain[index - 1] - 1)
    }
}

fn push_span(tree: &mut Tree, level: usize, span: Span) {
    tree.spans[level].push(span);
    tree.pieces[span.edge].push((level, tree.spans[level].len() - 1));
}

/// 辺を、u 側（内から外）、LCA、v 側（外から内）の区間に分ける（9.3 節）。
fn decompose(graph: &Graph, index: usize, edge: Edge, tree: &mut Tree) {
    let up_from = ancestors(graph, node_level(graph.nodes[edge.from].region));
    let up_to = ancestors(graph, node_level(graph.nodes[edge.to].region));
    // どちらの chain も最上段で終わるので、必ず共通の段がある。
    let lca_from = up_from
        .iter()
        .position(|level| up_to.contains(level))
        .expect("both chains end at the top level");
    let lca_to = up_to
        .iter()
        .position(|&level| level == up_from[lca_from])
        .expect("the LCA is in both chains");
    for i in 0..lca_from {
        let span = Span {
            edge: index,
            from: below(&up_from, i, edge.from),
            to: Anchor::Exit(index),
            label: None,
        };
        push_span(tree, up_from[i], span);
    }
    let lca = Span {
        edge: index,
        from: below(&up_from, lca_from, edge.from),
        to: below(&up_to, lca_to, edge.to),
        label: edge.label,
    };
    push_span(tree, up_from[lca_from], lca);
    for i in (0..lca_to).rev() {
        let span = Span {
            edge: index,
            from: Anchor::Entry(index),
            to: below(&up_to, i, edge.to),
            label: None,
        };
        push_span(tree, up_to[i], span);
    }
}

/// 段の後順。子の段を index の昇順にたどり、最後に親を置く。段 0 は最後に来る。
fn post_order(tree: &Tree) -> Vec<usize> {
    let mut order = Vec::with_capacity(tree.spans.len());
    let mut stack: Vec<(usize, usize)> = vec![(0, 0)];
    while let Some(&(level, next)) = stack.last() {
        if let Some(&region) = tree.regions[level].get(next) {
            if let Some(top) = stack.last_mut() {
                top.1 += 1;
            }
            stack.push((region_level(region), 0));
        } else {
            stack.pop();
            order.push(level);
        }
    }
    order
}

/// 段を下から順に配置する。
fn solve(graph: &Graph, tree: &Tree) -> Result<BTreeMap<usize, Built>, LayoutError> {
    let mut built = BTreeMap::new();
    for level in post_order(tree) {
        let input = level_input(graph, tree, level, &built)?;
        let output = run_level(&input)?;
        built.insert(level, Built { input, output });
    }
    Ok(built)
}

fn get_built(built: &BTreeMap<usize, Built>, level: usize) -> Result<&Built, LayoutError> {
    built
        .get(&level)
        .ok_or_else(|| LayoutError::Internal(format!("level {level} is not placed")))
}

fn level_input(
    graph: &Graph,
    tree: &Tree,
    level: usize,
    built: &BTreeMap<usize, Built>,
) -> Result<LevelInput, LayoutError> {
    let (items, slots) = level_items(graph, tree, level, built)?;
    let segments = tree.spans[level]
        .iter()
        .map(|span| -> Result<Segment, LayoutError> {
            Ok(Segment {
                edge: span.edge,
                from: slot(&slots, span.from)?,
                to: slot(&slots, span.to)?,
                label: span.label,
            })
        })
        .collect::<Result<Vec<Segment>, LayoutError>>()?;
    Ok(LevelInput {
        items,
        segments,
        frame: frame(graph, level),
    })
}

fn slot(slots: &BTreeMap<Anchor, ItemId>, anchor: Anchor) -> Result<ItemId, LayoutError> {
    slots
        .get(&anchor)
        .copied()
        .ok_or_else(|| LayoutError::Internal(format!("{anchor:?} is not in its level")))
}

/// 段の item（9.4 節の順：node、子の Box、Entry、Exit）と、区間の端から item への表。
fn level_items(
    graph: &Graph,
    tree: &Tree,
    level: usize,
    built: &BTreeMap<usize, Built>,
) -> Result<(Vec<Item>, BTreeMap<Anchor, ItemId>), LayoutError> {
    let mut items = Vec::new();
    let mut slots = BTreeMap::new();
    for &node in &tree.nodes[level] {
        slots.insert(Anchor::Node(node), items.len());
        items.push(Item {
            kind: ItemKind::Node(node),
            size: graph.nodes[node].size,
        });
    }
    for &region in &tree.regions[level] {
        slots.insert(Anchor::Child(region), items.len());
        items.push(box_item(region, get_built(built, region_level(region))?));
    }
    let entries = tree.spans[level].iter().filter_map(|span| match span.from {
        Anchor::Entry(edge) => Some(edge),
        _ => None,
    });
    for edge in entries {
        slots.insert(Anchor::Entry(edge), items.len());
        items.push(port(ItemKind::Entry(edge)));
    }
    let exits = tree.spans[level].iter().filter_map(|span| match span.to {
        Anchor::Exit(edge) => Some(edge),
        _ => None,
    });
    for edge in exits {
        slots.insert(Anchor::Exit(edge), items.len());
        items.push(port(ItemKind::Exit(edge)));
    }
    Ok((items, slots))
}

fn port(kind: ItemKind) -> Item {
    Item {
        kind,
        size: Size::default(),
    }
}

/// 配置の済んだ子 region を、親の段の Box にする。port の y は、子の段の port の rect の y。
fn box_item(region: usize, child: &Built) -> Item {
    let mut entries = Vec::new();
    let mut exits = Vec::new();
    for (item, rect) in child.input.items.iter().zip(&child.output.rects) {
        match item.kind {
            ItemKind::Entry(edge) => entries.push((edge, rect.y)),
            ItemKind::Exit(edge) => exits.push((edge, rect.y)),
            _ => {}
        }
    }
    Item {
        kind: ItemKind::Box(BoxItem {
            region,
            entries,
            exits,
        }),
        size: child.output.size,
    }
}

fn frame(graph: &Graph, level: usize) -> Frame {
    if level == 0 {
        Frame::Root(match &graph.layering {
            Layering::Optimize => RootLayering::Optimize,
            Layering::Fixed(columns) => RootLayering::Fixed(columns.clone()),
        })
    } else {
        Frame::Region {
            header: graph.regions[level - 1].header,
        }
    }
}

/// 段の原点を、親の段から順に決める。段 0 は図の余白の位置。
fn assemble(
    graph: &Graph,
    tree: &Tree,
    built: &BTreeMap<usize, Built>,
) -> Result<Layout, LayoutError> {
    let mut origins = BTreeMap::new();
    origins.insert(
        0,
        Point {
            x: DIAGRAM_PAD,
            y: DIAGRAM_PAD,
        },
    );
    let mut nodes = vec![Rect::default(); graph.nodes.len()];
    let mut regions = vec![Rect::default(); graph.regions.len()];
    // 後順の逆は、親の段が子の段より先になる。
    for level in post_order(tree).into_iter().rev() {
        let origin = origin_of(&origins, level)?;
        let solved = get_built(built, level)?;
        for (item, rect) in solved.input.items.iter().zip(&solved.output.rects) {
            let placed = Rect {
                x: origin.x + rect.x,
                y: origin.y + rect.y,
                ..*rect
            };
            match item.kind {
                ItemKind::Node(node) => nodes[node] = placed,
                ItemKind::Box(BoxItem { region, .. }) => {
                    regions[region] = placed;
                    origins.insert(
                        region_level(region),
                        Point {
                            x: placed.x,
                            y: placed.y,
                        },
                    );
                }
                _ => {}
            }
        }
    }
    let edges = (0..graph.edges.len())
        .map(|edge| edge_route(edge, tree, built, &origins))
        .collect::<Result<Vec<Route>, LayoutError>>()?;
    let root = get_built(built, 0)?;
    let size = Size {
        width: root.output.size.width + 2 * DIAGRAM_PAD,
        height: root.output.size.height + 2 * DIAGRAM_PAD,
    };
    Ok(Layout {
        size,
        nodes,
        regions,
        edges,
    })
}

fn origin_of(origins: &BTreeMap<usize, Point>, level: usize) -> Result<Point, LayoutError> {
    origins
        .get(&level)
        .copied()
        .ok_or_else(|| LayoutError::Internal(format!("level {level} has no origin")))
}

/// 辺の区間を段の原点で図の座標に移し、順につなぐ。ラベルは LCA の区間のもの。
fn edge_route(
    edge: usize,
    tree: &Tree,
    built: &BTreeMap<usize, Built>,
    origins: &BTreeMap<usize, Point>,
) -> Result<Route, LayoutError> {
    let mut points = Vec::new();
    let mut label = None;
    for &(level, index) in &tree.pieces[edge] {
        let origin = origin_of(origins, level)?;
        let piece = get_built(built, level)?
            .output
            .pieces
            .get(index)
            .ok_or_else(|| LayoutError::Internal(format!("edge {edge} lost a piece")))?;
        for point in &piece.points {
            push_simplified(
                &mut points,
                Point {
                    x: origin.x + point.x,
                    y: origin.y + point.y,
                },
            );
        }
        if let Some(rect) = piece.label {
            label = Some(Rect {
                x: origin.x + rect.x,
                y: origin.y + rect.y,
                ..rect
            });
        }
    }
    Ok(Route { points, label })
}

/// 同じ点を一つにし、同じ直線の上の三点では中の点を消す（8.3 節の 3）。
fn push_simplified(points: &mut Vec<Point>, next: Point) {
    if points.last() == Some(&next) {
        return;
    }
    let count = points.len();
    if count >= 2 && collinear(points[count - 2], points[count - 1], next) {
        points.pop();
        if points.last() == Some(&next) {
            return;
        }
    }
    points.push(next);
}

fn collinear(a: Point, b: Point, c: Point) -> bool {
    (a.x == b.x && b.x == c.x) || (a.y == b.y && b.y == c.y)
}

#[cfg(test)]
mod tests {
    use super::layout;
    use crate::model::{Edge, Graph, Layering, Node, Region, Size};
    use crate::verify;

    fn sized(width: i64, height: i64) -> Size {
        Size { width, height }
    }

    fn node(region: Option<usize>) -> Node {
        Node {
            size: sized(80, 40),
            region,
        }
    }

    fn region(parent: Option<usize>) -> Region {
        Region {
            parent,
            header: sized(100, 16),
        }
    }

    fn edge(from: usize, to: usize) -> Edge {
        Edge {
            from,
            to,
            label: None,
        }
    }

    fn graph(nodes: Vec<Node>, regions: Vec<Region>, edges: Vec<Edge>) -> Graph {
        Graph {
            nodes,
            regions,
            edges,
            layering: Layering::Optimize,
        }
    }

    #[test]
    fn nested_regions() {
        // r0 contains r1 and node b; r1 contains node a; the edge is a -> b.
        let graph = graph(
            vec![node(Some(1)), node(Some(0))],
            vec![region(None), region(Some(0))],
            vec![edge(0, 1)],
        );
        let result = layout(&graph).expect("nested regions lay out");
        verify(&graph, &result).expect("nested regions verify");
    }

    #[test]
    fn cross_region_edge() {
        // a is in r0, b is in r1, and both regions are at the top level.
        let graph = graph(
            vec![node(Some(0)), node(Some(1))],
            vec![region(None), region(None)],
            vec![edge(0, 1)],
        );
        let result = layout(&graph).expect("cross-region edge lays out");
        verify(&graph, &result).expect("cross-region edge verifies");
        let a = result.nodes[0];
        let b = result.nodes[1];
        let route = &result.edges[0].points;
        let first = route.first().copied().expect("route has a start");
        let last = route.last().copied().expect("route has an end");
        assert_eq!(first.x, a.x + a.width, "the route starts on a's right side");
        assert!(first.y >= a.y && first.y <= a.y + a.height);
        assert_eq!(last.x, b.x, "the route ends on b's left side");
        assert!(last.y >= b.y && last.y <= b.y + b.height);
    }

    #[test]
    fn region_without_boundary_edges() {
        let graph = graph(vec![node(Some(0))], vec![region(None)], vec![]);
        let result = layout(&graph).expect("region without edges lays out");
        verify(&graph, &result).expect("region without edges verifies");
        let inner = result.nodes[0];
        let outer = result.regions[0];
        assert!(outer.x <= inner.x && outer.y <= inner.y);
        assert!(outer.x + outer.width >= inner.x + inner.width);
        assert!(outer.y + outer.height >= inner.y + inner.height);
    }

    #[test]
    fn layout_two_nodes_verifies() {
        let graph = graph(vec![node(None), node(None)], vec![], vec![edge(0, 1)]);
        let result = layout(&graph).expect("two nodes lay out");
        verify(&graph, &result).expect("two nodes verify");
    }

    #[test]
    fn empty_graph_is_padding_only() {
        let graph = graph(vec![], vec![], vec![]);
        let result = layout(&graph).expect("empty graph lays out");
        assert_eq!(result.size, sized(48, 48));
        assert!(result.nodes.is_empty());
        assert!(result.regions.is_empty());
        assert!(result.edges.is_empty());
    }

    #[test]
    fn empty_region_has_header_space() {
        let graph = graph(vec![], vec![region(None)], vec![]);
        let result = layout(&graph).expect("empty region lays out");
        verify(&graph, &result).expect("empty region verifies");
        let outer = result.regions[0];
        assert!(outer.width >= 140, "width was {}", outer.width);
        assert!(outer.height >= 56, "height was {}", outer.height);
    }
}
