//! Role: 線分と曲線から通常線・波線・半円ジャンプのSVGパスを生成する。
use super::crossings::Jump;
use super::route::{Path, Point, Segment, point, tangent};
use std::fmt::Write;
const EPSILON: f64 = 0.000001;

pub(crate) fn radius(path: &Path, jump: Jump) -> f64 {
    geometry(path, jump).radius
}

pub(crate) fn normal(segments: &[Segment]) -> String {
    let first = segments[0].start;
    let mut output = format!("M{:.3},{:.3}", first.x, first.y);
    for segment in segments {
        if let Some(control) = segment.control {
            write!(
                output,
                " Q{:.3},{:.3} {:.3},{:.3}",
                control.x, control.y, segment.end.x, segment.end.y
            )
            .unwrap();
        } else {
            write!(output, " L{:.3},{:.3}", segment.end.x, segment.end.y).unwrap();
        }
    }
    output
}

fn append_chunk(output: &mut String, segments: &[Segment], dev: bool) -> Result<(), String> {
    let geometry = if dev {
        super::wave::draw(segments)?
    } else {
        normal(segments)
    };
    if output.is_empty() {
        output.push_str(&geometry);
    } else {
        let (_, continuation) = geometry
            .split_once(' ')
            .ok_or("SVG jump chunk has no drawing command")?;
        output.push(' ');
        output.push_str(continuation);
    }
    Ok(())
}

pub(crate) fn draw(path: &Path, jumps: &[Jump]) -> Result<String, String> {
    if jumps.is_empty() {
        return if path.dev {
            super::wave::draw(&path.segments)
        } else {
            Ok(normal(&path.segments))
        };
    }
    let mut output = String::new();
    let mut bridges: Vec<_> = jumps.iter().map(|jump| geometry(path, *jump)).collect();
    bridges.sort_by(|a, b| {
        a.from
            .segment
            .cmp(&b.from.segment)
            .then(a.from.t.total_cmp(&b.from.t))
    });
    let mut cursor = Location { segment: 0, t: 0.0 };
    for bridge in bridges {
        append_chunk(&mut output, &range(path, cursor, bridge.from), path.dev)?;
        let sweep = bridge.sweep;
        write!(
            output,
            " A{:.3},{:.3} 0 0 {sweep} {:.3},{:.3}",
            bridge.radius, bridge.radius, bridge.after.x, bridge.after.y
        )
        .unwrap();
        cursor = bridge.to;
    }
    append_chunk(
        &mut output,
        &range(
            path,
            cursor,
            Location {
                segment: path.segments.len() - 1,
                t: 1.0,
            },
        ),
        path.dev,
    )?;
    Ok(output)
}

pub(crate) fn arc(path: &Path, jump: Jump) -> String {
    let bridge = geometry(path, jump);
    let sweep = bridge.sweep;
    format!(
        "M{:.3},{:.3} A{:.3},{:.3} 0 0 {sweep} {:.3},{:.3}",
        bridge.before.x,
        bridge.before.y,
        bridge.radius,
        bridge.radius,
        bridge.after.x,
        bridge.after.y
    )
}

struct Bridge {
    before: Point,
    after: Point,
    radius: f64,
    sweep: u8,
    from: Location,
    to: Location,
}

#[derive(Clone, Copy)]
struct Location {
    segment: usize,
    t: f64,
}

fn position(path: &Path, location: Location) -> Point {
    let segment = &path.segments[location.segment];
    if segment.control.is_some() {
        point(segment, location.t)
    } else {
        Point {
            x: segment.start.x + (segment.end.x - segment.start.x) * location.t,
            y: segment.start.y + (segment.end.y - segment.start.y) * location.t,
        }
    }
}

fn speed(segment: &Segment, t: f64) -> f64 {
    if segment.control.is_some() {
        let derivative = tangent(segment, t);
        derivative.x.hypot(derivative.y)
    } else {
        segment.start.distance(segment.end)
    }
}

fn geometry(path: &Path, jump: Jump) -> Bridge {
    let segment = &path.segments[jump.segment];
    let t = jump.parameter.unwrap_or_else(|| {
        jump.center.distance(segment.start) / segment.start.distance(segment.end)
    });
    let mut from = Location {
        segment: jump.segment,
        t,
    };
    let mut to = from;
    // Internal joins are part of one continuous edge. When a crossing lies
    // exactly on a join, take the bridge ends from its neighboring segments.
    while from.t <= EPSILON && from.segment > 0 {
        from.segment -= 1;
        from.t = 1.0;
        if speed(&path.segments[from.segment], from.t) > EPSILON {
            break;
        }
        from.t = 0.0;
    }
    while to.t >= 1.0 - EPSILON && to.segment + 1 < path.segments.len() {
        to.segment += 1;
        to.t = 0.0;
        if speed(&path.segments[to.segment], to.t) > EPSILON {
            break;
        }
        to.t = 1.0;
    }
    let left_speed = speed(&path.segments[from.segment], from.t);
    let right_speed = speed(&path.segments[to.segment], to.t);
    let limit = if from.segment == to.segment && segment.straight {
        1.0
    } else {
        0.45
    };
    from.t -= (jump.radius / left_speed).min(from.t * limit);
    to.t += (jump.radius / right_speed).min((1.0 - to.t) * limit);
    let before = position(path, from);
    let after = position(path, to);
    let direction = Point {
        x: after.x - before.x,
        y: after.y - before.y,
    };
    Bridge {
        before,
        after,
        radius: before.distance(after) / 2.0,
        sweep: u8::from(if direction.x.abs() >= direction.y.abs() {
            direction.x > 0.0
        } else {
            direction.y > 0.0
        }),
        from,
        to,
    }
}

fn range(path: &Path, from: Location, to: Location) -> Vec<Segment> {
    let mut output = Vec::new();
    for index in from.segment..=to.segment {
        let segment = &path.segments[index];
        let left = if index == from.segment { from.t } else { 0.0 };
        let right = if index == to.segment { to.t } else { 1.0 };
        let start = position(
            path,
            Location {
                segment: index,
                t: left,
            },
        );
        let end = position(
            path,
            Location {
                segment: index,
                t: right,
            },
        );
        let control = segment.control.map(|_| {
            let derivative = tangent(segment, left);
            Point {
                x: start.x + derivative.x * (right - left) / 2.0,
                y: start.y + derivative.y * (right - left) / 2.0,
            }
        });
        output.push(Segment {
            start,
            end,
            control,
            straight: segment.straight,
        });
    }
    output
}
