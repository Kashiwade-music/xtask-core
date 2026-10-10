//! Role: 経路に沿う波線を生成し、矢印の接続位置と曲線の連続性を保つ。
use super::route::{Point, Segment};
use std::f64::consts::TAU;
use std::fmt::Write;

pub(crate) fn draw(segments: &[Segment]) -> Result<String, String> {
    let points = flatten(segments)?;
    let mut lengths = vec![0.0];
    for pair in points.windows(2) {
        lengths.push(lengths.last().unwrap() + pair[0].distance(pair[1]));
    }
    let total = *lengths.last().unwrap();
    if !total.is_finite() || total <= 0.0 {
        return Err("SVG dev path has zero or non-finite length".into());
    }
    let cycles = (total / 14.0).round().max(1.0);
    let amplitude = 2.2_f64.min(total / 8.0);
    let steps = (total / 0.9).ceil().clamp(8.0, 1_000_000.0) as usize;
    let mut segment = 0;
    let mut output = String::new();
    for step in 0..=steps {
        let distance = total * step as f64 / steps as f64;
        while segment + 2 < lengths.len() && lengths[segment + 1] < distance {
            segment += 1;
        }
        let start = points[segment];
        let end = points[segment + 1];
        let span = lengths[segment + 1] - lengths[segment];
        let base = start.interpolate(
            end,
            if span > 0.0 {
                (distance - lengths[segment]) / span
            } else {
                0.0
            },
        );
        let taper = (distance / 8.0)
            .min((total - distance) / 8.0)
            .clamp(0.0, 1.0);
        let offset = amplitude * taper * (TAU * cycles * distance / total).sin();
        let point = if span > 0.0 {
            Point {
                x: base.x - (end.y - start.y) / span * offset,
                y: base.y + (end.x - start.x) / span * offset,
            }
        } else {
            base
        };
        write!(
            output,
            "{}{:0.3},{:0.3}",
            if step == 0 { "M" } else { " L" },
            point.x,
            point.y
        )
        .unwrap();
    }
    Ok(output)
}

fn flatten(segments: &[Segment]) -> Result<Vec<Point>, String> {
    let mut points = vec![segments[0].start];
    for segment in segments {
        if let Some(control) = segment.control {
            let length = segment.start.distance(control) + control.distance(segment.end);
            let steps = length.ceil().clamp(8.0, 1_000_000.0) as usize;
            for step in 1..=steps {
                let t = step as f64 / steps as f64;
                points.push(
                    segment
                        .start
                        .interpolate(control, t)
                        .interpolate(control.interpolate(segment.end, t), t),
                );
            }
        } else {
            points.push(segment.end);
        }
    }
    if points.len() < 2 {
        return Err("SVG dev edge has no segment".into());
    }
    Ok(points)
}
