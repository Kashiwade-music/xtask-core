//! Role: 配置の結果の指標（交差・折れ・面積・総延長）を計算する。

/// 配置の指標。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    /// 異なる辺の、水平な線分と垂直な線分の内部どうしの交差の数。
    pub crossings: usize,
    /// 折れの数の和（簡約した点の数 - 2）。
    pub bends: usize,
    pub width: f64,
    pub height: f64,
    /// 線分の長さの和。
    pub length: f64,
}

/// 同じ点・同じ直線とみなす差（付属書 L 10.3 節と 8.3 節）。
const TOLERANCE: f64 = 1e-6;

type Point = (f64, f64);

/// 軸に平行な線分。`span` は走る向きの範囲、`at` は直交する向きの座標。
struct Line {
    route: usize,
    span: (f64, f64),
    at: f64,
}

pub(crate) fn compute(width: f64, height: f64, routes: &[Vec<Point>]) -> Metrics {
    let simplified: Vec<Vec<Point>> = routes.iter().map(|route| simplify(route)).collect();
    let bends = simplified
        .iter()
        .map(|points| points.len().saturating_sub(2))
        .sum();
    let length = simplified
        .iter()
        .map(|points| polyline_length(points))
        .sum();
    let crossings = count_crossings(&simplified);
    Metrics {
        crossings,
        bends,
        width,
        height,
        length,
    }
}

/// 同じ点を一つにし、同じ直線の上の中の点を消す（付属書 L 8.3 節の 3）。
fn simplify(route: &[Point]) -> Vec<Point> {
    let mut kept: Vec<Point> = Vec::new();
    for &point in route {
        while kept.len() >= 2 && collinear(kept[kept.len() - 2], kept[kept.len() - 1], point) {
            kept.pop();
        }
        if kept.last().is_some_and(|&last| same_point(last, point)) {
            continue;
        }
        kept.push(point);
    }
    kept
}

fn collinear(first: Point, middle: Point, last: Point) -> bool {
    (same(first.0, middle.0) && same(middle.0, last.0))
        || (same(first.1, middle.1) && same(middle.1, last.1))
}

fn same(first: f64, second: f64) -> bool {
    (first - second).abs() <= TOLERANCE
}

fn same_point(first: Point, second: Point) -> bool {
    same(first.0, second.0) && same(first.1, second.1)
}

fn polyline_length(points: &[Point]) -> f64 {
    points
        .windows(2)
        .map(|pair| (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1))
        .sum()
}

/// 異なる辺の、水平な線分と垂直な線分の内部どうしの交差の数。
fn count_crossings(routes: &[Vec<Point>]) -> usize {
    let (horizontal, vertical) = axis_lines(routes);
    let mut count = 0;
    for across in &horizontal {
        for along in &vertical {
            if across.route != along.route && crosses(across, along) {
                count += 1;
            }
        }
    }
    count
}

/// 水平な線分と垂直な線分を、経路ごとに集める。
fn axis_lines(routes: &[Vec<Point>]) -> (Vec<Line>, Vec<Line>) {
    let mut horizontal = Vec::new();
    let mut vertical = Vec::new();
    for (route, points) in routes.iter().enumerate() {
        for pair in points.windows(2) {
            let (first, second) = (pair[0], pair[1]);
            if same(first.1, second.1) {
                let span = (first.0.min(second.0), first.0.max(second.0));
                horizontal.push(Line {
                    route,
                    span,
                    at: first.1,
                });
            } else if same(first.0, second.0) {
                let span = (first.1.min(second.1), first.1.max(second.1));
                vertical.push(Line {
                    route,
                    span,
                    at: first.0,
                });
            }
        }
    }
    (horizontal, vertical)
}

/// 水平な線分と垂直な線分の内部が、一点で交わるか（端の一致は数えない）。
fn crosses(horizontal: &Line, vertical: &Line) -> bool {
    horizontal.span.0 < vertical.at
        && vertical.at < horizontal.span.1
        && vertical.span.0 < horizontal.at
        && horizontal.at < vertical.span.1
}

#[cfg(test)]
mod tests {
    //! Role: 配置の品質指標（交差の数など）の計算の試験。
    use super::*;

    #[test]
    fn metrics_counts_one_crossing() {
        let routes = vec![
            vec![(0.0, 50.0), (100.0, 50.0)],
            vec![(50.0, 0.0), (50.0, 100.0)],
        ];
        let metrics = compute(100.0, 100.0, &routes);
        assert_eq!(metrics.crossings, 1);
    }
}
