//! Role: 入力の検証と、配置結果の不変条件を確かめる（付属書 L の 10 章）。
use crate::model::{
    Edge, Graph, Layering, Layout, LayoutError, MAX_EDGES, MAX_NODES, MAX_REGIONS, MAX_SIZE, Point,
    REGION_PAD, Rect, Region, Size,
};

/// 要求を検証する（付属書 L 10.1 節）。LayoutError の variant の並びの順に調べる。
pub(crate) fn input(graph: &Graph) -> Result<(), LayoutError> {
    check_counts(graph)?;
    check_node_regions(graph)?;
    check_region_parents(graph)?;
    check_region_cycles(graph)?;
    check_edge_ends(graph)?;
    check_sizes(graph)?;
    check_layering(graph)
}

fn check_counts(graph: &Graph) -> Result<(), LayoutError> {
    let too_many = graph.nodes.len() > MAX_NODES
        || graph.regions.len() > MAX_REGIONS
        || graph.edges.len() > MAX_EDGES;
    if too_many {
        Err(LayoutError::TooMany)
    } else {
        Ok(())
    }
}

fn check_node_regions(graph: &Graph) -> Result<(), LayoutError> {
    let count = graph.regions.len();
    match graph
        .nodes
        .iter()
        .position(|node| node.region.is_some_and(|region| region >= count))
    {
        Some(node) => Err(LayoutError::NodeRegion { node }),
        None => Ok(()),
    }
}

fn check_region_parents(graph: &Graph) -> Result<(), LayoutError> {
    let count = graph.regions.len();
    let invalid = |region: usize| {
        graph.regions[region]
            .parent
            .is_some_and(|parent| parent >= count || parent == region)
    };
    match (0..count).find(|&region| invalid(region)) {
        Some(region) => Err(LayoutError::RegionParent { region }),
        None => Ok(()),
    }
}

fn check_region_cycles(graph: &Graph) -> Result<(), LayoutError> {
    match (0..graph.regions.len()).find(|&region| !reaches_top(&graph.regions, region)) {
        Some(region) => Err(LayoutError::RegionCycle { region }),
        None => Ok(()),
    }
}

/// 親をたどると、閉路に入らず最上段に着くか。
fn reaches_top(regions: &[Region], start: usize) -> bool {
    let mut current = Some(start);
    for _ in 0..regions.len() {
        let Some(index) = current else {
            return true;
        };
        current = regions[index].parent;
    }
    current.is_none()
}

fn check_edge_ends(graph: &Graph) -> Result<(), LayoutError> {
    let count = graph.nodes.len();
    if let Some(edge) = graph
        .edges
        .iter()
        .position(|edge| edge.from >= count || edge.to >= count)
    {
        return Err(LayoutError::EdgeEndpoint { edge });
    }
    match graph.edges.iter().position(|edge| edge.from == edge.to) {
        Some(edge) => Err(LayoutError::SelfLoop { edge }),
        None => Ok(()),
    }
}

fn check_sizes(graph: &Graph) -> Result<(), LayoutError> {
    let nodes = graph.nodes.iter().all(|node| valid_size(node.size));
    let regions = graph.regions.iter().all(|region| valid_size(region.header));
    let labels = graph
        .edges
        .iter()
        .filter_map(|edge| edge.label)
        .all(valid_size);
    if nodes && regions && labels {
        Ok(())
    } else {
        Err(LayoutError::InvalidSize)
    }
}

fn valid_size(size: Size) -> bool {
    (0..=MAX_SIZE).contains(&size.width) && (0..=MAX_SIZE).contains(&size.height)
}

fn check_layering(graph: &Graph) -> Result<(), LayoutError> {
    match &graph.layering {
        Layering::Optimize => Ok(()),
        Layering::Fixed(columns) if columns.len() != graph.nodes.len() => {
            Err(LayoutError::FixedLength)
        }
        Layering::Fixed(_) if !graph.regions.is_empty() => Err(LayoutError::FixedWithRegions),
        Layering::Fixed(_) => Ok(()),
    }
}

/// 配置の結果が不変条件を満たすかを確かめる（付属書 L 10.2 節）。
pub(crate) fn output(graph: &Graph, layout: &Layout) -> Result<(), String> {
    input(graph).map_err(|error| error.to_string())?;
    check_lengths(graph, layout)?;
    check_bounds(layout)?;
    check_minimum_sizes(graph, layout)?;
    check_nesting(graph, layout)?;
    check_overlaps(graph, layout)?;
    check_routes(graph, layout)?;
    check_labels(graph, layout)
}

fn check_lengths(graph: &Graph, layout: &Layout) -> Result<(), String> {
    let same = layout.nodes.len() == graph.nodes.len()
        && layout.regions.len() == graph.regions.len()
        && layout.edges.len() == graph.edges.len();
    if same {
        Ok(())
    } else {
        Err("the layout differs from the graph in the number of nodes, regions or edges".into())
    }
}

/// すべての矩形が、図の範囲の中にあること。
fn check_bounds(layout: &Layout) -> Result<(), String> {
    let labels: Vec<Rect> = layout
        .edges
        .iter()
        .filter_map(|route| route.label)
        .collect();
    let mut rects = layout.nodes.iter().chain(&layout.regions).chain(&labels);
    match rects.find(|rect| !inside_diagram(layout.size, **rect)) {
        Some(rect) => Err(format!("a rectangle {rect:?} is outside the diagram")),
        None => Ok(()),
    }
}

fn inside_diagram(size: Size, rect: Rect) -> bool {
    rect.width >= 0
        && rect.height >= 0
        && rect.x >= 0
        && rect.y >= 0
        && rect.x + rect.width <= size.width
        && rect.y + rect.height <= size.height
}

/// node と region の大きさが、要求以上であること。
fn check_minimum_sizes(graph: &Graph, layout: &Layout) -> Result<(), String> {
    for (node, (spec, rect)) in graph.nodes.iter().zip(&layout.nodes).enumerate() {
        if rect.width < spec.size.width || rect.height < spec.size.height {
            return Err(format!("node {node} is smaller than its size"));
        }
    }
    for (region, (spec, rect)) in graph.regions.iter().zip(&layout.regions).enumerate() {
        let width = spec.header.width + 2 * REGION_PAD;
        let height = spec.header.height + 2 * REGION_PAD;
        if rect.width < width || rect.height < height {
            return Err(format!("region {region} is smaller than its header"));
        }
    }
    Ok(())
}

/// 入れ子が保たれ、子が親の見出しの帯にかからないこと。
fn check_nesting(graph: &Graph, layout: &Layout) -> Result<(), String> {
    for (node, spec) in graph.nodes.iter().enumerate() {
        let Some(region) = spec.region else {
            continue;
        };
        if !within_region(graph, layout, region, layout.nodes[node]) {
            return Err(format!("node {node} is not inside its region {region}"));
        }
    }
    for (region, spec) in graph.regions.iter().enumerate() {
        let Some(parent) = spec.parent else {
            continue;
        };
        if !within_region(graph, layout, parent, layout.regions[region]) {
            return Err(format!("region {region} is not inside its parent {parent}"));
        }
    }
    Ok(())
}

fn within_region(graph: &Graph, layout: &Layout, region: usize, inner: Rect) -> bool {
    let header = graph.regions[region].header;
    let outer = layout.regions[region];
    inside_rect(outer, inner) && inner.y >= outer.y + header.height + REGION_PAD
}

fn inside_rect(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

/// node か region を、重なりの検査で数える要素。
#[derive(Clone, Copy)]
enum Element {
    Node(usize),
    Region(usize),
}

/// 祖先と子孫の関係に無い二つの要素の矩形が重ならないこと。
fn check_overlaps(graph: &Graph, layout: &Layout) -> Result<(), String> {
    let elements: Vec<Element> = (0..graph.nodes.len())
        .map(Element::Node)
        .chain((0..graph.regions.len()).map(Element::Region))
        .collect();
    for (index, &first) in elements.iter().enumerate() {
        for &second in &elements[index + 1..] {
            let overlap = overlaps(rect_of(layout, first), rect_of(layout, second));
            if overlap && !related(graph, first, second) {
                return Err(format!(
                    "{} and {} overlap",
                    describe(first),
                    describe(second)
                ));
            }
        }
    }
    Ok(())
}

fn rect_of(layout: &Layout, element: Element) -> Rect {
    match element {
        Element::Node(node) => layout.nodes[node],
        Element::Region(region) => layout.regions[region],
    }
}

fn describe(element: Element) -> String {
    match element {
        Element::Node(node) => format!("node {node}"),
        Element::Region(region) => format!("region {region}"),
    }
}

/// 内部が重なるか。共有する辺は重なりとみなさない。
fn overlaps(first: Rect, second: Rect) -> bool {
    first.x < second.x + second.width
        && second.x < first.x + first.width
        && first.y < second.y + second.height
        && second.y < first.y + first.height
}

/// 祖先と子孫の関係にあるか。
fn related(graph: &Graph, first: Element, second: Element) -> bool {
    contains(graph, first, second) || contains(graph, second, first)
}

/// `outer` が region で、`inner` の祖先であるか。
fn contains(graph: &Graph, outer: Element, inner: Element) -> bool {
    match outer {
        Element::Region(region) => containers(graph, inner).contains(&region),
        Element::Node(_) => false,
    }
}

/// 要素を直接含む region から、最上段まで順にたどった region の一覧。
fn containers(graph: &Graph, element: Element) -> Vec<usize> {
    let mut found = Vec::new();
    let mut current = match element {
        Element::Node(node) => graph.nodes[node].region,
        Element::Region(region) => graph.regions[region].parent,
    };
    while let Some(region) = current {
        found.push(region);
        current = graph.regions[region].parent;
    }
    found
}

/// 辺の経路の形、端、node を通らないことを確かめる。
fn check_routes(graph: &Graph, layout: &Layout) -> Result<(), String> {
    for (index, (edge, route)) in graph.edges.iter().zip(&layout.edges).enumerate() {
        let points = &route.points;
        check_polyline(index, points)?;
        let from = layout.nodes[edge.from];
        let to = layout.nodes[edge.to];
        check_ends(index, (from, to), points)?;
        check_avoids_nodes(layout, (index, edge), points)?;
    }
    Ok(())
}

/// 点が二つ以上あり、隣り合う点は x か y の一方だけが異なること。
fn check_polyline(index: usize, points: &[Point]) -> Result<(), String> {
    if points.len() < 2 {
        return Err(format!("edge {index} has fewer than two points"));
    }
    for pair in points.windows(2) {
        let horizontal = pair[0].y == pair[1].y;
        let vertical = pair[0].x == pair[1].x;
        if horizontal == vertical {
            return Err(format!("edge {index} has a diagonal or empty segment"));
        }
    }
    Ok(())
}

/// 始点は始点の node の右辺から右へ、終点は終点の node の左辺へ右から入ること。
fn check_ends(index: usize, (from, to): (Rect, Rect), points: &[Point]) -> Result<(), String> {
    let (first, second) = (points[0], points[1]);
    let count = points.len();
    let (before, last) = (points[count - 2], points[count - 1]);
    let starts = first.x == from.x + from.width
        && (from.y..=from.y + from.height).contains(&first.y)
        && second.x > first.x;
    let ends = last.x == to.x && (to.y..=to.y + to.height).contains(&last.y) && before.x < last.x;
    if starts && ends {
        Ok(())
    } else {
        Err(format!(
            "edge {index} does not start on the right of its source or end on the left of its target"
        ))
    }
}

/// 端以外の node の内部を通らないこと。端の node は最初と最後の線分だけ例外。
fn check_avoids_nodes(
    layout: &Layout,
    (index, edge): (usize, &Edge),
    points: &[Point],
) -> Result<(), String> {
    let last_segment = points.len() - 2;
    for (segment, pair) in points.windows(2).enumerate() {
        for (node, rect) in layout.nodes.iter().enumerate() {
            let source = node == edge.from && segment == 0;
            let target = node == edge.to && segment == last_segment;
            if !(source || target) && crosses_interior(pair[0], pair[1], *rect) {
                return Err(format!("edge {index} passes through node {node}"));
            }
        }
    }
    Ok(())
}

/// 軸に沿った線分の内部が、矩形の内部と交わるか（境界の上だけは交わらない）。
fn crosses_interior(first: Point, second: Point, rect: Rect) -> bool {
    let (min_x, max_x) = (first.x.min(second.x), first.x.max(second.x));
    let (min_y, max_y) = (first.y.min(second.y), first.y.max(second.y));
    min_x < rect.x + rect.width && max_x > rect.x && min_y < rect.y + rect.height && max_y > rect.y
}

/// ラベルは、要求があるときだけあり、大きさが一致し、node と重ならないこと。
fn check_labels(graph: &Graph, layout: &Layout) -> Result<(), String> {
    for (index, (edge, route)) in graph.edges.iter().zip(&layout.edges).enumerate() {
        match (edge.label, route.label) {
            (None, None) => {}
            (Some(size), Some(rect)) => {
                if rect.width != size.width || rect.height != size.height {
                    return Err(format!("the label of edge {index} has the wrong size"));
                }
                if layout.nodes.iter().any(|node| overlaps(*node, rect)) {
                    return Err(format!("the label of edge {index} overlaps a node"));
                }
            }
            _ => {
                return Err(format!(
                    "edge {index} has a label in only one of the request and the layout"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Edge, Layering, MAX_NODES, Node, Point, Rect, Region, Route, Size};

    fn size(width: i64, height: i64) -> Size {
        Size { width, height }
    }

    fn rect(x: i64, y: i64, width: i64, height: i64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// 手で作った配置。node a（80×40）、b（80×40）、辺 a→b。
    fn two_nodes() -> (Graph, Layout) {
        let graph = Graph {
            nodes: vec![
                Node {
                    size: size(80, 40),
                    region: None,
                },
                Node {
                    size: size(80, 40),
                    region: None,
                },
            ],
            regions: vec![],
            edges: vec![Edge {
                from: 0,
                to: 1,
                label: None,
            }],
            layering: Layering::Optimize,
        };
        let layout = Layout {
            size: size(248, 88),
            nodes: vec![rect(24, 24, 80, 40), rect(144, 24, 80, 40)],
            regions: vec![],
            edges: vec![Route {
                points: vec![Point { x: 104, y: 44 }, Point { x: 144, y: 44 }],
                label: None,
            }],
        };
        (graph, layout)
    }

    #[test]
    fn verify_accepts_hand_made_layout() {
        let (graph, layout) = two_nodes();
        assert_eq!(output(&graph, &layout), Ok(()));
    }

    #[test]
    fn verify_rejects_overlap() {
        let (graph, mut layout) = two_nodes();
        layout.nodes[1] = rect(90, 24, 80, 40);
        assert!(output(&graph, &layout).is_err());
    }

    #[test]
    fn verify_rejects_shrunken_node() {
        let (graph, mut layout) = two_nodes();
        layout.nodes[0] = rect(24, 24, 70, 40);
        layout.edges[0].points[0] = Point { x: 94, y: 44 };
        assert!(output(&graph, &layout).is_err());
    }

    #[test]
    fn verify_rejects_header_overlap() {
        let graph = Graph {
            nodes: vec![Node {
                size: size(80, 40),
                region: Some(0),
            }],
            regions: vec![Region {
                parent: None,
                header: size(60, 16),
            }],
            edges: vec![],
            layering: Layering::Optimize,
        };
        let layout = Layout {
            size: size(188, 104),
            nodes: vec![rect(54, 34, 80, 40)],
            regions: vec![rect(24, 24, 140, 56)],
            edges: vec![],
        };
        assert!(output(&graph, &layout).is_err());
    }

    #[test]
    fn verify_rejects_label_size() {
        let (mut graph, mut layout) = two_nodes();
        graph.edges[0].label = Some(size(30, 10));
        layout.edges[0].label = Some(rect(110, 39, 20, 10));
        assert!(output(&graph, &layout).is_err());
    }

    #[test]
    fn input_rejects_too_many() {
        let graph = Graph {
            nodes: vec![
                Node {
                    size: Size::default(),
                    region: None,
                };
                MAX_NODES + 1
            ],
            regions: vec![],
            edges: vec![],
            layering: Layering::Optimize,
        };
        assert_eq!(input(&graph), Err(LayoutError::TooMany));
    }
}
