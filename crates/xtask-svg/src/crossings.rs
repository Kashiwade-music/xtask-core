//! Role: 直線と曲線の交差を解析し、障害物を避ける半円ジャンプを選ぶ。
use super::route::{
    Path, Point, Segment, coefficients, evaluate, horizontal, point, tangent, vertical,
};
const RADIUS: f64 = 4.0;
const EPSILON: f64 = 0.000001;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Jump {
    pub(crate) segment: usize,
    pub(crate) center: Point,
    pub(crate) radius: f64,
    pub(crate) parameter: Option<f64>,
}

#[derive(Clone, Copy)]
pub(crate) struct Rectangle {
    pub(crate) left: f64,
    pub(crate) right: f64,
    pub(crate) top: f64,
    pub(crate) bottom: f64,
}

impl Rectangle {
    fn segment(segment: &Segment) -> Self {
        let mut bounds = Self {
            left: segment.start.x.min(segment.end.x),
            right: segment.start.x.max(segment.end.x),
            top: segment.start.y.min(segment.end.y),
            bottom: segment.start.y.max(segment.end.y),
        };
        if let Some(control) = segment.control {
            bounds.left = bounds.left.min(control.x);
            bounds.right = bounds.right.max(control.x);
            bounds.top = bounds.top.min(control.y);
            bounds.bottom = bounds.bottom.max(control.y);
        }
        // Quadratic intersections accept parameters slightly outside [0, 1].
        // On [-EPSILON, 1 + EPSILON], each coordinate's derivative is bounded
        // by 2 * hull span * (1 + 2 * EPSILON). Conservatively cover that
        // extrapolation, the curve-pair distance tolerance, and rounding.
        let padding = |min: f64, max: f64| {
            4.0 * EPSILON * (1.0 + EPSILON) * (max - min)
                + 10.0 * EPSILON
                + min.abs().max(max.abs()) * (32.0 * f64::EPSILON)
        };
        let x = padding(bounds.left, bounds.right);
        let y = padding(bounds.top, bounds.bottom);
        bounds.left -= x;
        bounds.right += x;
        bounds.top -= y;
        bounds.bottom += y;
        bounds
    }

    fn overlaps(&self, other: &Self) -> bool {
        // Reject only definitely separated bounds, retaining touching pairs.
        !(self.right < other.left
            || other.right < self.left
            || self.bottom < other.top
            || other.bottom < self.top)
    }

    fn include(&mut self, other: &Self) {
        self.left = self.left.min(other.left);
        self.right = self.right.max(other.right);
        self.top = self.top.min(other.top);
        self.bottom = self.bottom.max(other.bottom);
    }
}

fn intersection(a: &Segment, b: &Segment) -> Option<Point> {
    let (h, v) = if horizontal(a) && vertical(b) {
        (a, b)
    } else if vertical(a) && horizontal(b) {
        (b, a)
    } else {
        return None;
    };
    let point = Point {
        x: v.start.x,
        y: h.start.y,
    };
    // Segment endpoints can be internal polyline/curve joins. Whole edge
    // endpoints are excluded separately once path ownership is available.
    if point.x < h.start.x.min(h.end.x) - EPSILON
        || point.x > h.start.x.max(h.end.x) + EPSILON
        || point.y < v.start.y.min(v.end.y) - EPSILON
        || point.y > v.start.y.max(v.end.y) + EPSILON
    {
        return None;
    }
    Some(point)
}

fn quadratic_intersections(curve: &Segment, line: &Segment) -> Vec<Point> {
    let Some(control) = curve.control else {
        return Vec::new();
    };
    let (start, middle, end, target) = if horizontal(line) {
        (curve.start.y, control.y, curve.end.y, line.start.y)
    } else if vertical(line) {
        (curve.start.x, control.x, curve.end.x, line.start.x)
    } else {
        return Vec::new();
    };
    let a = start - 2.0 * middle + end;
    let b = 2.0 * (middle - start);
    let c = start - target;
    let roots = if a.abs() < EPSILON {
        if b.abs() < EPSILON {
            Vec::new()
        } else {
            vec![-c / b]
        }
    } else {
        let discriminant = b * b - 4.0 * a * c;
        if discriminant <= EPSILON {
            // Tangential touching does not cross the carrier line.
            Vec::new()
        } else {
            let root = discriminant.sqrt();
            vec![(-b - root) / (2.0 * a), (-b + root) / (2.0 * a)]
        }
    };
    roots
        .into_iter()
        .filter(|t| *t >= -EPSILON && *t <= 1.0 + EPSILON)
        .map(|t| {
            let u = 1.0 - t;
            Point {
                x: u * u * curve.start.x + 2.0 * u * t * control.x + t * t * curve.end.x,
                y: u * u * curve.start.y + 2.0 * u * t * control.y + t * t * curve.end.y,
            }
        })
        .filter(|point| {
            if horizontal(line) {
                point.x >= line.start.x.min(line.end.x) - EPSILON
                    && point.x <= line.start.x.max(line.end.x) + EPSILON
            } else {
                point.y >= line.start.y.min(line.end.y) - EPSILON
                    && point.y <= line.start.y.max(line.end.y) + EPSILON
            }
        })
        .collect()
}

fn roots(coefficients: &[f64]) -> Vec<f64> {
    let mut degree = coefficients.len() - 1;
    while degree > 0 && coefficients[degree].abs() < EPSILON {
        degree -= 1;
    }
    if degree == 0 {
        return Vec::new();
    }
    let coefficients = &coefficients[..=degree];
    let derivative: Vec<_> = coefficients
        .iter()
        .enumerate()
        .skip(1)
        .map(|(index, coefficient)| *coefficient * index as f64)
        .collect();
    let mut boundaries = roots(&derivative);
    boundaries.extend([0.0, 1.0]);
    boundaries.sort_by(f64::total_cmp);
    boundaries.dedup_by(|a, b| (*a - *b).abs() < EPSILON);
    let tolerance = EPSILON
        * coefficients
            .iter()
            .map(|value| value.abs())
            .sum::<f64>()
            .max(1.0);
    let mut result = Vec::new();
    for &point in &boundaries {
        if evaluate(coefficients, point).abs() < tolerance {
            result.push(point);
        }
    }
    // Derivative roots partition a polynomial into monotonic intervals, so
    // every transverse root is bracketed even when crossings are very close.
    for interval in boundaries.windows(2) {
        let mut left = interval[0];
        let mut right = interval[1];
        let sign = evaluate(coefficients, left).is_sign_positive();
        if sign == evaluate(coefficients, right).is_sign_positive() {
            continue;
        }
        for _ in 0..60 {
            let middle = (left + right) / 2.0;
            if evaluate(coefficients, middle).is_sign_positive() == sign {
                left = middle;
            } else {
                right = middle;
            }
        }
        result.push((left + right) / 2.0);
    }
    result.sort_by(f64::total_cmp);
    result.dedup_by(|a, b| (*a - *b).abs() < EPSILON);
    result
}

fn quadratic_pair(a: &Segment, b: &Segment) -> Vec<(Point, f64, f64)> {
    let ac = a.control.unwrap();
    let bc = b.control.unwrap();
    let ax = coefficients(a.start.x, ac.x, a.end.x);
    let ay = coefficients(a.start.y, ac.y, a.end.y);
    let bx = coefficients(b.start.x, bc.x, b.end.x);
    let by = coefficients(b.start.y, bc.y, b.end.y);
    // Eliminate u² from the two curve equations, then substitute the resulting
    // quadratic u(t) back into either axis to obtain a quartic in t.
    let denominator = by[2] * bx[1] - bx[2] * by[1];
    if denominator.abs() < EPSILON {
        // A degenerate quadratic is collinear and is not an actual rounded
        // corner. Straight carriers handle its crossings instead.
        return Vec::new();
    }
    let u = [
        (by[2] * (ax[0] - bx[0]) - bx[2] * (ay[0] - by[0])) / denominator,
        (by[2] * ax[1] - bx[2] * ay[1]) / denominator,
        (by[2] * ax[2] - bx[2] * ay[2]) / denominator,
    ];
    let (axis_a, axis_b) = if bx[2].abs() >= by[2].abs() {
        (ax, bx)
    } else {
        (ay, by)
    };
    let mut polynomial = [0.0; 5];
    for left in 0..3 {
        for right in 0..3 {
            polynomial[left + right] += axis_b[2] * u[left] * u[right];
        }
        polynomial[left] += axis_b[1] * u[left] - axis_a[left];
    }
    polynomial[0] += axis_b[0];
    roots(&polynomial)
        .into_iter()
        .filter_map(|ta| {
            let tb = evaluate(&u, ta);
            if !(-EPSILON..=1.0 + EPSILON).contains(&tb) {
                return None;
            }
            let pa = point(a, ta);
            let pb = point(b, tb);
            let da = tangent(a, ta);
            let db = tangent(b, tb);
            if pa.distance(pb) > EPSILON * 10.0 || (da.x * db.y - da.y * db.x).abs() < EPSILON {
                return None;
            }
            Some((pa, ta, tb))
        })
        .collect()
}

fn intersections(a: &Segment, b: &Segment) -> Vec<(Point, bool, Option<f64>)> {
    if a.control.is_some() && b.control.is_some() {
        return quadratic_pair(a, b)
            .into_iter()
            .map(|(point, ta, tb)| {
                if ta > EPSILON && ta < 1.0 - EPSILON {
                    (point, true, Some(ta))
                } else if tb > EPSILON && tb < 1.0 - EPSILON {
                    (point, false, Some(tb))
                } else {
                    (point, true, Some(ta.clamp(0.0, 1.0)))
                }
            })
            .collect();
    }
    if let Some(point) = intersection(a, b) {
        let a_interior = point.distance(a.start) > EPSILON && point.distance(a.end) > EPSILON;
        let b_interior = point.distance(b.start) > EPSILON && point.distance(b.end) > EPSILON;
        if a_interior || b_interior {
            vec![(point, a_interior && (!b_interior || horizontal(a)), None)]
        } else {
            vec![(point, horizontal(a), None)]
        }
    } else if a.control.is_some() {
        quadratic_intersections(a, b)
            .into_iter()
            .map(|point| (point, false, None))
            .collect()
    } else if b.control.is_some() {
        quadratic_intersections(b, a)
            .into_iter()
            .map(|point| (point, true, None))
            .collect()
    } else {
        Vec::new()
    }
}

pub(crate) fn crossings(paths: &[Path], obstacles: &[Rectangle]) -> Vec<Vec<Jump>> {
    let bounds: Vec<Vec<_>> = paths
        .iter()
        .map(|path| path.segments.iter().map(Rectangle::segment).collect())
        .collect();
    let path_bounds: Vec<_> = bounds
        .iter()
        .map(|segments| {
            segments.first().copied().map(|mut bounds| {
                for segment in &segments[1..] {
                    bounds.include(segment);
                }
                bounds
            })
        })
        .collect();
    let mut result: Vec<Vec<Jump>> = vec![Vec::new(); paths.len()];
    for left in 0..paths.len() {
        for right in left + 1..paths.len() {
            if paths[left].group == paths[right].group {
                continue;
            }
            if let (Some(a), Some(b)) = (&path_bounds[left], &path_bounds[right]) {
                if !a.overlaps(b) {
                    continue;
                }
            }
            for (a_index, a) in paths[left].segments.iter().enumerate() {
                for (b_index, b) in paths[right].segments.iter().enumerate() {
                    if !bounds[left][a_index].overlaps(&bounds[right][b_index]) {
                        continue;
                    }
                    for (center, carrier_a, parameter) in intersections(a, b) {
                        if [left, right].iter().any(|path_index| {
                            let segments = &paths[*path_index].segments;
                            center.distance(segments[0].start) < EPSILON
                                || center.distance(segments.last().unwrap().end) < EPSILON
                        }) {
                            continue;
                        }
                        let (path_index, segment_index) = if carrier_a {
                            (left, a_index)
                        } else {
                            (right, b_index)
                        };
                        if result
                            .iter()
                            .flatten()
                            .any(|jump| jump.center.distance(center) < EPSILON)
                        {
                            continue;
                        }
                        let mut radius = RADIUS;
                        for endpoint in [a.start, a.end, b.start, b.end] {
                            let distance = center.distance(endpoint);
                            if distance > EPSILON {
                                radius = radius.min(distance * 0.45);
                            }
                        }
                        for obstacle in obstacles {
                            let dx = (obstacle.left - center.x)
                                .max(center.x - obstacle.right)
                                .max(0.0);
                            let dy = (obstacle.top - center.y)
                                .max(center.y - obstacle.bottom)
                                .max(0.0);
                            radius = radius.min(dx.max(dy) / 2.0);
                        }
                        if radius <= EPSILON {
                            continue;
                        }
                        result[path_index].push(Jump {
                            segment: segment_index,
                            center,
                            radius,
                            parameter,
                        });
                    }
                }
            }
        }
    }
    for (path_index, jumps) in result.iter_mut().enumerate() {
        let centers = jumps.clone();
        for jump in jumps.iter_mut() {
            for neighbor in &centers {
                let distance = jump.center.distance(neighbor.center);
                if distance > EPSILON {
                    jump.radius = jump.radius.min((distance - 1.0).max(distance * 0.5) / 2.0);
                    if let (true, Some(t), Some(other)) = (
                        jump.segment == neighbor.segment,
                        jump.parameter,
                        neighbor.parameter,
                    ) {
                        let derivative = tangent(&paths[path_index].segments[jump.segment], t);
                        jump.radius = jump
                            .radius
                            .min((t - other).abs() * derivative.x.hypot(derivative.y) * 0.4);
                    }
                }
            }
        }
        jumps.retain(|jump| jump.radius > EPSILON);
    }
    result
}

#[cfg(test)]
mod tests {
    //! Role: 交差判定の許容誤差を保った境界による候補棄却を検証する。
    use super::*;

    fn segment(start: (f64, f64), end: (f64, f64), control: Option<(f64, f64)>) -> Segment {
        Segment {
            start: Point {
                x: start.0,
                y: start.1,
            },
            end: Point { x: end.0, y: end.1 },
            control: control.map(|(x, y)| Point { x, y }),
            straight: control.is_none(),
        }
    }

    #[test]
    fn bounds_preserve_intersections_including_parameter_and_coordinate_tolerances() {
        let curve = segment((0.0, 0.0), (1e9, 1e9), Some((0.0, 1e9)));
        let before = point(&curve, -EPSILON * 0.5);
        let after = point(&curve, 1.0 + EPSILON * 0.5);
        let before_line = segment((-1e4, before.y), (1e4, before.y), None);
        let after_line = segment((after.x, 1e9 - 1e4), (after.x, 1e9 + 1e4), None);
        assert!(!intersections(&curve, &before_line).is_empty());
        assert!(!intersections(&curve, &after_line).is_empty());
        let horizontal = segment((0.0, 0.0), (10.0, 0.0), None);
        let just_beyond = segment(
            (10.0 + EPSILON * 0.5, -1.0),
            (10.0 + EPSILON * 0.5, 1.0),
            None,
        );
        assert!(!intersections(&horizontal, &just_beyond).is_empty());
        let segments = [
            curve,
            before_line,
            after_line,
            horizontal,
            just_beyond,
            segment((0.0, -10.0), (0.0, 10.0), None),
            segment((0.0, 0.0), (0.0, 0.0), None),
            segment((-2.0, -2.0), (2.0, 2.0), Some((0.0, 0.0))),
            segment((-2.0, 2.0), (2.0, 2.0), Some((0.0, -2.0))),
            segment((-2.0, -2.0), (2.0, -2.0), Some((0.0, 2.0))),
            segment(
                (before.x - 10.0, before.y - 10.0),
                (before.x + 10.0, before.y - 10.0),
                Some((before.x, before.y + 10.0)),
            ),
            segment((100.0, 100.0), (110.0, 100.0), None),
        ];
        let mut rejected = 0;
        for a in &segments {
            for b in &segments {
                let expected = intersections(a, b);
                let bounded = if Rectangle::segment(a).overlaps(&Rectangle::segment(b)) {
                    intersections(a, b)
                } else {
                    rejected += 1;
                    Vec::new()
                };
                assert_eq!(bounded, expected, "a={a:?}, b={b:?}");
            }
        }
        assert!(rejected > 0);
    }

    #[test]
    fn bounds_contain_quadratic_extrapolation_and_keep_nested_control_hulls() {
        for curve in [
            segment((0.0, 0.0), (1e9, 1e9), Some((0.0, 1e9))),
            segment((-1e12, 1e12), (1e12, -1e12), Some((1e12, 1e12))),
            segment((5.0, 5.0), (5.0, 5.0), Some((5.0, 5.0))),
        ] {
            let bounds = Rectangle::segment(&curve);
            for t in [
                -EPSILON,
                -EPSILON * 0.5,
                0.0,
                0.5,
                1.0,
                1.0 + EPSILON * 0.5,
                1.0 + EPSILON,
            ] {
                let p = point(&curve, t);
                assert!(p.x >= bounds.left && p.x <= bounds.right);
                assert!(p.y >= bounds.top && p.y <= bounds.bottom);
            }
        }
        let outer = Rectangle::segment(&segment(
            (-100.0, -100.0),
            (100.0, 100.0),
            Some((-100.0, 100.0)),
        ));
        let inner = Rectangle::segment(&segment((-1.0, -1.0), (1.0, 1.0), Some((0.0, 1.0))));
        assert!(outer.overlaps(&inner) && inner.overlaps(&outer));
        let far = Rectangle::segment(&segment((200.0, 200.0), (210.0, 210.0), None));
        assert!(!outer.overlaps(&far));
    }
}
