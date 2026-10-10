use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use xtask_layout::{
    EDGE_FONT_SIZE, EdgeStyle, FONT_FAMILY, LINE_HEIGHT, NODE_FONT_SIZE, REGION_FONT_SIZE,
    REGION_LINE_HEIGHT, Scene, SceneEdge, SceneRegion, TITLE_FONT_SIZE,
};

use super::crossings::{Rectangle, crossings};
use super::drawing;
use super::route::{self, Point};

const STROKE: &str = "#475569";
const EPSILON: f64 = 0.00001;

/// Present ELK's absolute geometry as a standalone SVG.
///
/// Layout points, nodes and label positions are retained. Decoration changes
/// only local corner and crossing geometry; it does not route edges again.
pub fn render(scene: &Scene) -> Result<String, String> {
    validate(scene)?;
    let width = scene.width + 32.0;
    let height = scene.height + 58.0;
    let mut output = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.3}\" height=\"{height:.3}\" viewBox=\"0 0 {width:.3} {height:.3}\" data-layout-engine=\"ELK\" role=\"img\"><title>{}</title><rect width=\"100%\" height=\"100%\" fill=\"white\"/>",
        xml(&scene.title)
    );
    text(
        &mut output,
        std::slice::from_ref(&scene.title),
        width / 2.0,
        21.0,
        TITLE_FONT_SIZE,
        22.0,
    );
    output.push_str("<g class=\"graph\" transform=\"translate(16 42)\">\n");
    let regions: BTreeMap<_, _> = scene
        .regions
        .iter()
        .map(|region| (region.id.as_str(), region))
        .collect();
    regions_layer(&mut output, &regions, None);
    let mut edges: Vec<_> = scene.edges.iter().collect();
    edges.sort_by(|a, b| a.id.cmp(&b.id));
    let paths = edges
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            let points: Vec<_> = edge
                .points
                .iter()
                .map(|point| Point {
                    x: point.x,
                    y: point.y,
                })
                .collect();
            route::build(&points, index, edge.style == EdgeStyle::Dev)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let obstacles = obstacles(scene);
    let jumps = crossings(&paths, &obstacles);
    for ((edge, path), jumps) in edges.iter().zip(&paths).zip(&jumps) {
        let first = edge.points.first().unwrap();
        let last = edge.points.last().unwrap();
        let style = match edge.style {
            EdgeStyle::Normal => "normal",
            EdgeStyle::Dev => "dev",
            EdgeStyle::Unconstrained => "build",
        };
        let points = edge
            .points
            .iter()
            .map(|point| format!("{:.3},{:.3}", point.x, point.y))
            .collect::<Vec<_>>()
            .join(" ");
        write!(output, "<g class=\"edge\" data-points=\"{points}\" data-edge-id=\"{}\" data-from=\"{}\" data-to=\"{}\" data-style=\"{style}\" data-start-x=\"{:.3}\" data-start-y=\"{:.3}\" data-end-x=\"{:.3}\" data-end-y=\"{:.3}\"><title>{} → {}</title><path class=\"route\" fill=\"none\" stroke=\"{STROKE}\" stroke-width=\"1.3\" d=\"{}\"/>", xml(&edge.id),xml(&edge.from),xml(&edge.to),first.x,first.y,last.x,last.y,xml(&edge.from),xml(&edge.to),drawing::draw(path,jumps)?).unwrap();
        arrow(&mut output, edge);
        output.push_str("</g>\n");
    }
    // The background halo must paint after every edge, otherwise a later
    // underpass would cross the semicircle again.
    output.push_str("<g class=\"bridges\">\n");
    for (path, jumps) in paths.iter().zip(&jumps) {
        for jump in jumps {
            let data = drawing::arc(path, *jump);
            let halo = 1.3 + (drawing::radius(path, *jump) * 1.5).min(3.0);
            writeln!(output, "<path class=\"bridge-halo\" d=\"{data}\" fill=\"none\" stroke=\"white\" stroke-width=\"{halo:.3}\" stroke-linecap=\"round\"/><path class=\"bridge\" d=\"{data}\" fill=\"none\" stroke=\"{STROKE}\" stroke-width=\"1.3\"/>").unwrap();
        }
    }
    output.push_str("</g>\n");
    let mut nodes: Vec<_> = scene.nodes.iter().collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    for node in nodes {
        write!(output, "<g class=\"node\" data-node-id=\"{}\"><title>{}</title><rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" rx=\"2\" fill=\"white\" stroke=\"#334155\" stroke-width=\"1.2\"/>",xml(&node.id),xml(&node.label),node.x,node.y,node.width,node.height).unwrap();
        text(
            &mut output,
            &node.lines,
            node.x + node.width / 2.0,
            node.y + node.height / 2.0,
            NODE_FONT_SIZE,
            LINE_HEIGHT,
        );
        output.push_str("</g>\n");
    }
    for region in regions.values() {
        let title_height = region.lines.len() as f64 * REGION_LINE_HEIGHT;
        write!(output,"<g class=\"region-label\" data-region-id=\"{}\"><rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"white\"/>",xml(&region.id),region.x+3.0,region.y+3.0,region.width-6.0,title_height+4.0).unwrap();
        text(
            &mut output,
            &region.lines,
            region.x + region.width / 2.0,
            region.y + 5.0 + title_height / 2.0,
            REGION_FONT_SIZE,
            REGION_LINE_HEIGHT,
        );
        output.push_str("</g>\n");
    }
    for edge in &edges {
        if let Some(label) = &edge.label {
            write!(output,"<g class=\"edge-label\" data-edge-id=\"{}\"><rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" rx=\"2\" fill=\"white\"/>",xml(&edge.id),label.x-label.width/2.0-2.0,label.y-label.height/2.0-1.0,label.width+4.0,label.height+2.0).unwrap();
            text(
                &mut output,
                &label.lines,
                label.x,
                label.y,
                EDGE_FONT_SIZE,
                LINE_HEIGHT,
            );
            output.push_str("</g>\n");
        }
    }
    output.push_str("</g></svg>\n");
    Ok(output)
}

fn regions_layer(
    output: &mut String,
    regions: &BTreeMap<&str, &SceneRegion>,
    parent: Option<&str>,
) {
    for region in regions
        .values()
        .filter(|region| region.parent.as_deref() == parent)
    {
        write!(output,"<g class=\"cluster\" data-region-id=\"{}\" data-parent=\"{}\"><title>{}</title><rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" rx=\"4\" fill=\"none\" stroke=\"#94a3b8\" stroke-width=\"1\"/>",xml(&region.id),xml(region.parent.as_deref().unwrap_or("")),xml(&region.label),region.x,region.y,region.width,region.height).unwrap();
        regions_layer(output, regions, Some(&region.id));
        output.push_str("</g>\n");
    }
}

fn text(output: &mut String, lines: &[String], x: f64, y: f64, size: f64, line_height: f64) {
    let start = y - (lines.len().saturating_sub(1) as f64 * line_height) / 2.0 + size * 0.35;
    for (index, line) in lines.iter().enumerate() {
        write!(output,"<text x=\"{x:.3}\" y=\"{:.3}\" text-anchor=\"middle\" font-family=\"{}\" font-size=\"{size:.3}\" fill=\"#0f172a\">{}</text>",start+index as f64*line_height,xml(FONT_FAMILY),xml(line)).unwrap();
    }
}

fn arrow(output: &mut String, edge: &SceneEdge) {
    let tip = edge.points.last().unwrap();
    let previous = edge
        .points
        .iter()
        .rev()
        .find(|point| (point.x - tip.x).abs() > EPSILON || (point.y - tip.y).abs() > EPSILON)
        .unwrap();
    let dx = (tip.x - previous.x).signum();
    let length = (tip.x - previous.x).abs().min(8.0);
    let half_width = length * 0.4;
    write!(output,"<polygon class=\"arrowhead\" fill=\"{STROKE}\" points=\"{:.3},{:.3} {:.3},{:.3} {:.3},{:.3}\"/>",tip.x,tip.y,tip.x-dx*length,tip.y-half_width,tip.x-dx*length,tip.y+half_width).unwrap();
}

fn obstacles(scene: &Scene) -> Vec<Rectangle> {
    let mut result: Vec<_> = scene
        .nodes
        .iter()
        .map(|node| Rectangle {
            left: node.x,
            right: node.x + node.width,
            top: node.y,
            bottom: node.y + node.height,
        })
        .collect();
    for region in &scene.regions {
        result.push(Rectangle {
            left: region.x,
            right: region.x + region.width,
            top: region.y,
            bottom: region.y + 8.0 + region.lines.len() as f64 * REGION_LINE_HEIGHT,
        });
    }
    for edge in &scene.edges {
        if let Some(label) = &edge.label {
            result.push(Rectangle {
                left: label.x - label.width / 2.0 - 2.0,
                right: label.x + label.width / 2.0 + 2.0,
                top: label.y - label.height / 2.0 - 1.0,
                bottom: label.y + label.height / 2.0 + 1.0,
            });
        }
    }
    result
}

fn validate(scene: &Scene) -> Result<(), String> {
    if !scene.width.is_finite()
        || !scene.height.is_finite()
        || scene.width < 0.0
        || scene.height < 0.0
    {
        return Err("SVG scene dimensions must be finite and nonnegative".into());
    }
    let mut ids = BTreeSet::new();
    for (kind, id, x, y, width, height) in scene
        .nodes
        .iter()
        .map(|node| ("node", &node.id, node.x, node.y, node.width, node.height))
        .chain(scene.regions.iter().map(|region| {
            (
                "region",
                &region.id,
                region.x,
                region.y,
                region.width,
                region.height,
            )
        }))
    {
        if !ids.insert((kind, id)) {
            return Err(format!("duplicate SVG {kind} id: {id}"));
        }
        if [x, y, width, height].iter().any(|value| !value.is_finite())
            || width <= 0.0
            || height <= 0.0
        {
            return Err(format!("invalid SVG shape geometry: {id}"));
        }
    }
    let regions: BTreeMap<_, _> = scene
        .regions
        .iter()
        .map(|region| (&region.id, region))
        .collect();
    for region in &scene.regions {
        let mut visited = BTreeSet::new();
        let mut current = region;
        while let Some(parent) = &current.parent {
            if !visited.insert(parent) {
                return Err("SVG region parents contain a cycle".into());
            }
            current = regions
                .get(parent)
                .ok_or_else(|| format!("missing SVG parent region: {parent}"))?;
        }
    }
    let nodes: BTreeMap<_, _> = scene.nodes.iter().map(|node| (&node.id, node)).collect();
    let mut edges = BTreeSet::new();
    for edge in &scene.edges {
        if !edges.insert(&edge.id) {
            return Err(format!("duplicate SVG edge id: {}", edge.id));
        }
        if edge.points.len() < 2 {
            return Err("SVG edge has fewer than two route points".into());
        }
        for (id, point, neighbor) in [
            (
                &edge.from,
                edge.points.first().unwrap(),
                edge.points.get(1).unwrap(),
            ),
            (
                &edge.to,
                edge.points.last().unwrap(),
                edge.points.get(edge.points.len() - 2).unwrap(),
            ),
        ] {
            let node = nodes
                .get(id)
                .ok_or_else(|| format!("SVG edge references missing node: {id}"))?;
            if ((point.x - node.x).abs() > EPSILON
                && (point.x - node.x - node.width).abs() > EPSILON)
                || point.y < node.y - EPSILON
                || point.y > node.y + node.height + EPSILON
                || (neighbor.y - point.y).abs() > EPSILON
            {
                return Err(format!(
                    "SVG edge {} is not attached horizontally to a node side",
                    edge.id
                ));
            }
        }
        if let Some(label) = &edge.label {
            if [label.x, label.y, label.width, label.height]
                .iter()
                .any(|value| !value.is_finite())
                || label.width < 0.0
                || label.height < 0.0
            {
                return Err("SVG edge label geometry is invalid".into());
            }
        }
    }
    Ok(())
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
