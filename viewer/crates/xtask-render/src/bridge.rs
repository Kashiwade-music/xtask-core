//! Role: 水平の線分が別の edge の垂直の線分を跨ぐ点（跳び）を求める。
use crate::model::{Bridge, P};

/// 跳びの半径。隣の交点との最小の間隔は 2 * この値。
const BRIDGE_RADIUS: f64 = 4.0;
const MARGIN: f64 = 2.0 * BRIDGE_RADIUS;

/// 垂直の線分（x、y の範囲、所属する edge）。
#[derive(Clone, Copy)]
struct Vertical {
    x: f64,
    low: f64,
    high: f64,
    edge: usize,
}

/// 各 edge の水平の線分が、別の edge の垂直の線分と内部で交わる点ごとに跳びを作る。
/// 並びは edge の index、線分の順、x の昇順。
pub(crate) fn find(edges: &[Vec<P>]) -> Vec<Bridge> {
    let verticals = verticals(edges);
    let mut bridges = Vec::new();
    for (edge, points) in edges.iter().enumerate() {
        for pair in points.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            if start.y != end.y || start.x == end.x {
                continue;
            }
            let (left, right) = (start.x.min(end.x), start.x.max(end.x));
            for x in kept_crossings(&verticals, edge, start.y, left, right) {
                bridges.push(Bridge {
                    edge: u32::try_from(edge).unwrap_or(u32::MAX),
                    center: P { x, y: start.y },
                    radius: BRIDGE_RADIUS,
                });
            }
        }
    }
    bridges
}

/// 全 edge の垂直の線分を x の昇順に並べる（二分探索で候補を絞るため）。
fn verticals(edges: &[Vec<P>]) -> Vec<Vertical> {
    let mut result = Vec::new();
    for (edge, points) in edges.iter().enumerate() {
        for pair in points.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            if start.x == end.x && start.y != end.y {
                result.push(Vertical {
                    x: start.x,
                    low: start.y.min(end.y),
                    high: start.y.max(end.y),
                    edge,
                });
            }
        }
    }
    result.sort_by(|a, b| a.x.total_cmp(&b.x));
    result
}

/// 水平の線分（y、left..right）の上で、残す交点の x の昇順。
/// 両端から MARGIN 未満のもの、直前に残した交点から MARGIN 未満のもの（同じ x を含む）は除く。
fn kept_crossings(verticals: &[Vertical], edge: usize, y: f64, left: f64, right: f64) -> Vec<f64> {
    let start = verticals.partition_point(|vertical| vertical.x <= left);
    let end = verticals.partition_point(|vertical| vertical.x < right);
    let candidates = verticals.get(start..end).unwrap_or(&[]);
    let crossings = candidates
        .iter()
        .filter(|vertical| vertical.edge != edge && vertical.low < y && y < vertical.high)
        .map(|vertical| vertical.x);
    let mut kept: Vec<f64> = Vec::new();
    for x in crossings {
        if x - left < MARGIN || right - x < MARGIN {
            continue;
        }
        if kept.last().is_some_and(|last| x - last < MARGIN) {
            continue;
        }
        kept.push(x);
    }
    kept
}
