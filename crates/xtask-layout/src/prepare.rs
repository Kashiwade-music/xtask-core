//! Role: 文字を折り返して寸法を計算し、ELKへ渡す配置要求を準備する。
use crate::model::{
    Direction, EdgeRequest, LINE_HEIGHT, LayoutRequest, NODE_FONT_SIZE, REGION_FONT_SIZE,
    REGION_LINE_HEIGHT, TITLE_FONT_SIZE,
};
use serde::Serialize;
use std::collections::BTreeMap;

const LABEL_MAX_WIDTH: f64 = 320.0;
const REGION_LABEL_MAX_WIDTH: f64 = 200.0;

#[derive(Serialize)]
pub(crate) struct Prepared {
    title: String,
    title_width: f64,
    direction: Direction,
    nodes: Vec<PreparedNode>,
    regions: Vec<PreparedRegion>,
    edges: Vec<EdgeRequest>,
    ranks: Vec<Vec<String>>,
    label_metrics: BTreeMap<String, Metrics>,
}

#[derive(Serialize)]
struct PreparedNode {
    id: String,
    label: String,
    lines: Vec<String>,
    width: f64,
    height: f64,
}

#[derive(Serialize)]
struct PreparedRegion {
    id: String,
    label: String,
    lines: Vec<String>,
    label_width: f64,
    label_height: f64,
    nodes: Vec<String>,
    children: Vec<PreparedRegion>,
}

#[derive(Serialize)]
struct Metrics {
    lines: Vec<String>,
    width: f64,
    height: f64,
}

pub(crate) fn request(request: &LayoutRequest) -> Prepared {
    let mut incoming = BTreeMap::new();
    let mut outgoing = BTreeMap::new();
    for edge in &request.edges {
        *incoming.entry(edge.to.as_str()).or_insert(0usize) += 1;
        *outgoing.entry(edge.from.as_str()).or_insert(0usize) += 1;
    }
    let mut nodes: Vec<_> = request
        .nodes
        .iter()
        .map(|node| {
            let metrics = measure(&node.label, NODE_FONT_SIZE, LINE_HEIGHT, LABEL_MAX_WIDTH);
            let degree = incoming
                .get(node.id.as_str())
                .copied()
                .unwrap_or(0)
                .max(outgoing.get(node.id.as_str()).copied().unwrap_or(0));
            PreparedNode {
                id: node.id.clone(),
                label: node.label.clone(),
                lines: metrics.lines,
                width: (metrics.width + 28.0).max(96.0),
                height: (metrics.height + 24.0)
                    .max(42.0)
                    .max((degree + 1) as f64 * 16.0 + 24.0),
            }
        })
        .collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    // Equal-width nodes in a normal rank make column membership explicit.
    for rank in &request.ranks {
        let max_width = nodes
            .iter()
            .filter(|node| rank.contains(&node.id))
            .map(|node| node.width)
            .fold(0.0, f64::max);
        let max_height = nodes
            .iter()
            .filter(|node| rank.contains(&node.id))
            .map(|node| node.height)
            .fold(0.0, f64::max);
        for node in nodes.iter_mut().filter(|node| rank.contains(&node.id)) {
            match request.direction {
                Direction::LeftToRight => node.width = max_width,
                Direction::TopToBottom => node.height = max_height,
            }
        }
    }
    let mut regions: Vec<_> = request.regions.iter().map(region).collect();
    regions.sort_by(|a, b| a.id.cmp(&b.id));
    let mut edges = request.edges.clone();
    edges.sort_by(|a, b| a.id.cmp(&b.id));
    let label_metrics = edges
        .iter()
        .filter_map(|edge| {
            edge.label.as_ref().map(|label| {
                (
                    edge.id.clone(),
                    measure(label, NODE_FONT_SIZE, LINE_HEIGHT, LABEL_MAX_WIDTH),
                )
            })
        })
        .collect();
    Prepared {
        title: request.title.clone(),
        title_width: request
            .title
            .chars()
            .map(|character| advance(character, TITLE_FONT_SIZE))
            .sum(),
        direction: request.direction,
        nodes,
        regions,
        edges,
        ranks: request.ranks.clone(),
        label_metrics,
    }
}

fn region(input: &crate::model::RegionRequest) -> PreparedRegion {
    let metrics = measure(
        &input.label,
        REGION_FONT_SIZE,
        REGION_LINE_HEIGHT,
        REGION_LABEL_MAX_WIDTH,
    );
    let mut children: Vec<_> = input.children.iter().map(region).collect();
    children.sort_by(|a, b| a.id.cmp(&b.id));
    let mut nodes = input.nodes.clone();
    nodes.sort();
    PreparedRegion {
        id: input.id.clone(),
        label: input.label.clone(),
        lines: metrics.lines,
        label_width: metrics.width,
        label_height: metrics.height,
        nodes,
        children,
    }
}

fn measure(text: &str, font: f64, line_height: f64, max_width: f64) -> Metrics {
    let mut lines = Vec::new();
    for input in text.split('\n') {
        let mut line = String::new();
        let mut width = 0.0;
        for character in input.chars() {
            let next = advance(character, font);
            if width + next > max_width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                width = 0.0;
            }
            line.push(character);
            width += next;
        }
        lines.push(line);
    }
    let width = lines
        .iter()
        .map(|line| line.chars().map(|character| advance(character, font)).sum())
        .fold(0.0, f64::max);
    Metrics {
        height: lines.len() as f64 * line_height,
        lines,
        width,
    }
}

fn advance(character: char, font: f64) -> f64 {
    if character.is_ascii() {
        font * 0.62
    } else {
        font * 1.05
    }
}
