//! Replace only explicitly identified dev edge paths with sinusoidal geometry.

use std::f64::consts::TAU;
use std::fmt::Write;

use svgtypes::{SimplePathSegment, SimplifyingPathParser};

pub(crate) fn render(svg: &str, dev_ids: &[String]) -> Result<String, String> {
    let document = roxmltree::Document::parse_with_options(
        svg,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(|error| format!("cannot parse Graphviz SVG for dev waves: {error}"))?;
    let mut replacements = Vec::new();
    for id in dev_ids {
        let groups: Vec<_> = document
            .descendants()
            .filter(|node| node.is_element() && node.attribute("id") == Some(id))
            .collect();
        if groups.len() != 1 {
            return Err(format!(
                "expected one SVG dev edge group {id:?}, found {}",
                groups.len()
            ));
        }
        let paths: Vec<_> = groups[0]
            .descendants()
            .filter(|node| node.has_tag_name("path"))
            .collect();
        if paths.is_empty() {
            return Err(format!("SVG dev edge {id:?} has no path"));
        }
        for path in paths {
            let attribute = path
                .attributes()
                .find(|attribute| attribute.name() == "d")
                .ok_or_else(|| format!("SVG dev edge {id:?} has a path without d"))?;
            replacements.push((attribute.range_value(), wave_path(attribute.value())?));
        }
    }
    replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut output = svg.to_owned();
    for (range, data) in replacements {
        output.replace_range(range, &data);
    }
    Ok(output)
}

#[derive(Clone, Copy, Debug)]
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn distance(self, other: Self) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
    fn interpolate(self, other: Self, t: f64) -> Self {
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
        }
    }
}

fn wave_path(path: &str) -> Result<String, String> {
    let points = flatten(path)?;
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

fn flatten(path: &str) -> Result<Vec<Point>, String> {
    let mut points: Vec<Point> = Vec::new();
    for segment in SimplifyingPathParser::from(path) {
        match segment.map_err(|error| format!("cannot parse SVG dev path: {error}"))? {
            SimplePathSegment::MoveTo { x, y } => {
                if !points.is_empty() {
                    return Err("SVG dev path has multiple subpaths".into());
                }
                points.push(Point { x, y });
            }
            SimplePathSegment::LineTo { x, y } => {
                if points.is_empty() {
                    return Err("SVG dev path lacks an initial move".into());
                }
                points.push(Point { x, y });
            }
            SimplePathSegment::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let start = *points.last().ok_or("SVG dev path lacks an initial move")?;
                let controls = [
                    start,
                    Point { x: x1, y: y1 },
                    Point { x: x2, y: y2 },
                    Point { x, y },
                ];
                sample_curve(&controls, &mut points)?;
            }
            SimplePathSegment::Quadratic { x1, y1, x, y } => {
                let start = *points.last().ok_or("SVG dev path lacks an initial move")?;
                sample_curve(
                    &[start, Point { x: x1, y: y1 }, Point { x, y }],
                    &mut points,
                )?;
            }
            SimplePathSegment::ClosePath => return Err("SVG dev edge path must be open".into()),
        }
    }
    if points.len() < 2
        || points
            .iter()
            .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return Err("SVG dev path requires finite points and a segment".into());
    }
    Ok(points)
}

fn sample_curve(controls: &[Point], points: &mut Vec<Point>) -> Result<(), String> {
    let length: f64 = controls
        .windows(2)
        .map(|pair| pair[0].distance(pair[1]))
        .sum();
    if !length.is_finite() {
        return Err("SVG dev curve has non-finite coordinates".into());
    }
    let steps = length.ceil().clamp(8.0, 1_000_000.0) as usize;
    for step in 1..=steps {
        let mut interpolated = controls.to_vec();
        let t = step as f64 / steps as f64;
        for remaining in (1..controls.len()).rev() {
            for index in 0..remaining {
                interpolated[index] = interpolated[index].interpolate(interpolated[index + 1], t);
            }
        }
        points.push(interpolated[0]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_path_has_true_oscillation_and_fixed_endpoints() {
        let output = wave_path("M0,0 L100,0").unwrap();
        let points = flatten(&output).unwrap();
        assert_eq!(
            points.first().unwrap().distance(Point { x: 0.0, y: 0.0 }),
            0.0
        );
        assert_eq!(
            points.last().unwrap().distance(Point { x: 100.0, y: 0.0 }),
            0.0
        );
        assert!(points.iter().any(|point| point.y > 2.0));
        assert!(points.iter().any(|point| point.y < -2.0));
    }

    #[test]
    fn short_and_multisegment_paths_are_valid() {
        for path in [
            "M0,0L1,1",
            "M0,0C1,2 3,4 5,6 C7,8 9,10 11,12",
            "M0,0c1,2 3,4 5,6",
            "M0,0Q5,8 10,0",
        ] {
            let output = wave_path(path).unwrap();
            assert!(flatten(&output).unwrap().len() >= 9);
        }
        for path in [
            "M0,0",
            "M0,0L0,0",
            "invalid",
            "M0,0L1,1Z",
            "M0,0L1,1M2,2L3,3",
        ] {
            assert!(wave_path(path).is_err(), "{path}");
        }
    }

    #[test]
    fn only_requested_geometry_is_changed() {
        let svg = "<svg><g id='normal'><path d='M0,0L40,0'/></g><g id='dev'><path d='M0,0L40,0'/><polygon points='40,0 35,3 35,-3'/><text>dev</text></g></svg>";
        let output = render(svg, &["dev".into()]).unwrap();
        assert!(output.contains("<g id='normal'><path d='M0,0L40,0'/></g>"));
        assert!(output.contains("<polygon points='40,0 35,3 35,-3'/><text>dev</text>"));
        assert!(output.contains(" L"));
        assert!(render(svg, &["missing".into()]).is_err());
        assert!(render("invalid XML", &["dev".into()]).is_err());
    }
}
