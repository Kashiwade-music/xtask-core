//! Role: module 図の配置要求（module ごとの node・region・辺）を解析結果から作る。
use std::collections::BTreeMap;

use xtask_archive::{EdgeStyle, Item};
use xtask_layout::{Edge, Graph, Layering, Node, Region};
use xtask_modules::ModuleGraph;

use crate::Drawing;
use crate::text::{NO_ROLE, node_box, node_text, region_text};

type DrawnEdges = Vec<(EdgeStyle, Option<Vec<String>>)>;

/// 解析結果の module 図。module の index が、item・node・region の index になる。
pub(crate) fn drawing(graph: &ModuleGraph) -> Result<Drawing, String> {
    let index: BTreeMap<&str, usize> = graph
        .modules
        .iter()
        .enumerate()
        .map(|(position, module)| (module.id.as_str(), position))
        .collect();
    let mut drawing = Drawing {
        graph: Graph {
            nodes: Vec::new(),
            regions: Vec::new(),
            edges: Vec::new(),
            layering: Layering::Optimize,
        },
        items: Vec::new(),
        nodes: Vec::new(),
        regions: Vec::new(),
        edges: Vec::new(),
    };
    for (position, module) in graph.modules.iter().enumerate() {
        let role = module.role.as_ref().map_or(NO_ROLE, String::as_str);
        let header = region_text(&format!("{}\n{role}", module.id));
        let this = node_text("(self)");
        drawing.items.push(Item {
            name: module.id.clone(),
            role: module.role.clone().ok(),
            open: None,
        });
        drawing.graph.regions.push(Region {
            parent: module.parent,
            header: header.size,
        });
        drawing.regions.push((position as u32, header.lines));
        drawing.graph.nodes.push(Node {
            size: node_box(&this),
            region: Some(position),
        });
        drawing.nodes.push((position as u32, this.lines));
    }
    let (edges, drawn_edges) = edges(graph, &index)?;
    drawing.graph.edges = edges;
    drawing.edges = drawn_edges;
    Ok(drawing)
}

/// 辺の順は graph.edges の順。自分への辺は除く。
fn edges(
    graph: &ModuleGraph,
    index: &BTreeMap<&str, usize>,
) -> Result<(Vec<Edge>, DrawnEdges), String> {
    let mut layout_edges = Vec::new();
    let mut drawn_edges = Vec::new();
    for reference in graph
        .edges
        .iter()
        .filter(|reference| reference.from != reference.to)
    {
        layout_edges.push(Edge {
            from: lookup(index, &reference.from)?,
            to: lookup(index, &reference.to)?,
            label: None,
        });
        drawn_edges.push((EdgeStyle::Normal, None));
    }
    Ok((layout_edges, drawn_edges))
}

fn lookup(index: &BTreeMap<&str, usize>, id: &str) -> Result<usize, String> {
    index
        .get(id)
        .copied()
        .ok_or_else(|| format!("module reference refers to an unknown module `{id}`"))
}
