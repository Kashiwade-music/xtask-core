const EPSILON: f64 = 0.000001;

/// Round only existing orthogonal turns; never reroute an ELK edge.
pub(crate) fn build(points: &[Point], group: usize, dev: bool) -> Result<Path, String> {
    if points.len() < 2
        || points
            .iter()
            .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return Err("SVG edge requires at least two finite points".into());
    }
    let mut simplified: Vec<Point> = Vec::new();
    for &point in points {
        if simplified.last() == Some(&point) {
            continue;
        }
        while simplified.len() >= 2 {
            let a = simplified[simplified.len() - 2];
            let b = simplified[simplified.len() - 1];
            let cross = (b.x - a.x) * (point.y - b.y) - (b.y - a.y) * (point.x - b.x);
            let dot = (b.x - a.x) * (point.x - b.x) + (b.y - a.y) * (point.y - b.y);
            if cross.abs() > EPSILON || dot < 0.0 {
                break;
            }
            simplified.pop();
        }
        simplified.push(point);
    }
    if simplified.len() < 2 {
        return Err("SVG edge has zero length".into());
    }
    if simplified.windows(2).any(|pair| {
        (pair[0].x - pair[1].x).abs() > EPSILON && (pair[0].y - pair[1].y).abs() > EPSILON
    }) {
        return Err("ELK edge contains a diagonal segment".into());
    }
    let mut segments = Vec::new();
    let mut current = simplified[0];
    for index in 1..simplified.len() - 1 {
        let previous = simplified[index - 1];
        let corner = simplified[index];
        let next = simplified[index + 1];
        let dot = (corner.x - previous.x) * (next.x - corner.x)
            + (corner.y - previous.y) * (next.y - corner.y);
        if dot.abs() < EPSILON {
            let radius = 4.0_f64
                .min(previous.distance(corner) / 2.0)
                .min(corner.distance(next) / 2.0);
            let before = corner.towards(previous, radius);
            let after = corner.towards(next, radius);
            segments.push(Segment {
                start: current,
                end: before,
                control: None,
                straight: true,
            });
            segments.push(Segment {
                start: before,
                end: after,
                control: Some(corner),
                straight: false,
            });
            current = after;
        } else {
            segments.push(Segment {
                start: current,
                end: corner,
                control: None,
                straight: true,
            });
            current = corner;
        }
    }
    segments.push(Segment {
        start: current,
        end: *simplified.last().unwrap(),
        control: None,
        straight: true,
    });
    Ok(Path {
        segments,
        group,
        dev,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Point {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

impl Point {
    pub(crate) fn distance(self, other: Self) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }

    pub(crate) fn towards(self, other: Self, distance: f64) -> Self {
        let t = distance / self.distance(other);
        self.interpolate(other, t)
    }

    pub(crate) fn interpolate(self, other: Self, t: f64) -> Self {
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
        }
    }
}

pub(crate) struct Path {
    pub(crate) segments: Vec<Segment>,
    pub(crate) group: usize,
    pub(crate) dev: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Segment {
    pub(crate) start: Point,
    pub(crate) end: Point,
    pub(crate) straight: bool,
    pub(crate) control: Option<Point>,
}

pub(crate) fn horizontal(segment: &Segment) -> bool {
    segment.straight
        && (segment.start.y - segment.end.y).abs() < EPSILON
        && segment.start.distance(segment.end) > EPSILON
}

pub(crate) fn vertical(segment: &Segment) -> bool {
    segment.straight
        && (segment.start.x - segment.end.x).abs() < EPSILON
        && segment.start.distance(segment.end) > EPSILON
}

pub(crate) fn coefficients(start: f64, control: f64, end: f64) -> [f64; 3] {
    [start, 2.0 * (control - start), start - 2.0 * control + end]
}

pub(crate) fn evaluate(coefficients: &[f64], parameter: f64) -> f64 {
    coefficients
        .iter()
        .rev()
        .fold(0.0, |result, coefficient| result * parameter + coefficient)
}

pub(crate) fn point(segment: &Segment, t: f64) -> Point {
    let control = segment.control.unwrap();
    Point {
        x: evaluate(&coefficients(segment.start.x, control.x, segment.end.x), t),
        y: evaluate(&coefficients(segment.start.y, control.y, segment.end.y), t),
    }
}

pub(crate) fn tangent(segment: &Segment, t: f64) -> Point {
    let control = segment.control.unwrap();
    Point {
        x: 2.0 * ((1.0 - t) * (control.x - segment.start.x) + t * (segment.end.x - control.x)),
        y: 2.0 * ((1.0 - t) * (control.y - segment.start.y) + t * (segment.end.y - control.y)),
    }
}
