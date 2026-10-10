use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Edge, Graph, quote};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Direction {
    TopToBottom,
    LeftToRight,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EdgeStyle {
    Normal,
    /// An actual sinusoidal SVG stroke, with no effect on node ranks.
    Dev,
    /// A solid dependency that does not affect node ranks (e.g. build).
    Unconstrained,
}

// Keep defaults explicit so source analysis need not trust derive helper attributes.
#[allow(clippy::derivable_impls)]
impl Default for Direction {
    fn default() -> Self {
        Self::TopToBottom
    }
}

#[allow(clippy::derivable_impls)]
impl Default for EdgeStyle {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Cluster {
    pub id: String,
    pub label: String,
    /// Nodes immediately contained in this region.
    pub nodes: Vec<String>,
    pub children: Vec<Cluster>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Layout {
    pub direction: Direction,
    /// Ordered normal-dependency levels; each inner group shares a rank.
    pub ranks: Vec<Vec<String>>,
    pub clusters: Vec<Cluster>,
    /// Keys are indices into the original `Graph.edges`; absent entries are normal.
    pub edge_styles: BTreeMap<usize, EdgeStyle>,
}

impl Graph {
    pub fn dot_with_layout(&self, layout: &Layout) -> Result<String, String> {
        let original = self.dot()?;
        if layout == &Layout::default() {
            return Ok(original);
        }
        validate(self, layout)?;
        let direction = match layout.direction {
            Direction::TopToBottom => "TB",
            Direction::LeftToRight => "LR",
        };
        let mut dot = format!(
            "digraph {{\n  graph [label={}, labelloc=\"t\", rankdir={}, newrank=true];\n  node [shape=\"box\"];\n",
            quote(&self.title),
            quote(direction),
        );
        let mut nodes: Vec<_> = self.nodes.iter().collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        for node in nodes {
            dot.push_str(&format!(
                "  {} [label={}];\n",
                quote(&node.id),
                quote(&node.label)
            ));
        }
        let mut clusters: Vec<_> = layout.clusters.iter().collect();
        clusters.sort_by(|a, b| a.id.cmp(&b.id));
        for cluster in clusters {
            emit_cluster(cluster, 1, &mut dot);
        }
        for rank in &layout.ranks {
            let mut rank = rank.clone();
            rank.sort();
            dot.push_str("  { rank=same;");
            for id in rank {
                dot.push_str(&format!(" {};", quote(&id)));
            }
            dot.push_str(" }\n");
        }
        for (position, (_, edge, style)) in sorted_edges(self, layout).into_iter().enumerate() {
            dot.push_str(&format!("  {} -> {}", quote(&edge.from), quote(&edge.to)));
            let mut attributes = Vec::new();
            let label = match style {
                EdgeStyle::Dev => Some(edge.label.as_deref().unwrap_or("dev")),
                _ => edge.label.as_deref(),
            };
            if let Some(label) = label {
                attributes.push(format!("label={}", quote(label)));
            }
            if style != EdgeStyle::Normal {
                attributes.push("constraint=false".into());
            }
            if style == EdgeStyle::Dev {
                attributes.push(format!(
                    "id={}",
                    quote(&format!("xtask-dev-edge-{position}"))
                ));
            }
            if !attributes.is_empty() {
                dot.push_str(&format!(" [{}]", attributes.join(", ")));
            }
            dot.push_str(";\n");
        }
        dot.push_str("}\n");
        Ok(dot)
    }

    pub(crate) fn dev_edge_ids(&self, layout: &Layout) -> Vec<String> {
        sorted_edges(self, layout)
            .into_iter()
            .enumerate()
            .filter(|(_, (_, _, style))| *style == EdgeStyle::Dev)
            .map(|(position, _)| format!("xtask-dev-edge-{position}"))
            .collect()
    }
}

fn sorted_edges<'a>(graph: &'a Graph, layout: &Layout) -> Vec<(usize, &'a Edge, EdgeStyle)> {
    let mut edges: Vec<_> = graph
        .edges
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            (
                index,
                edge,
                layout.edge_styles.get(&index).copied().unwrap_or_default(),
            )
        })
        .collect();
    edges.sort_by(|(_, a, sa), (_, b, sb)| {
        (&a.from, &a.to, &a.label, sa).cmp(&(&b.from, &b.to, &b.label, sb))
    });
    edges
}

fn validate(graph: &Graph, layout: &Layout) -> Result<(), String> {
    let ids: BTreeSet<_> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    for index in layout.edge_styles.keys() {
        if *index >= graph.edges.len() {
            return Err(format!("edge style index {index} is out of bounds"));
        }
    }
    let mut ranked = BTreeSet::new();
    for rank in &layout.ranks {
        if rank.is_empty() {
            return Err("rank groups must not be empty".into());
        }
        for id in rank {
            if !ids.contains(id.as_str()) {
                return Err(format!("rank references missing node: {id:?}"));
            }
            if !ranked.insert(id) {
                return Err(format!("node occurs in more than one rank: {id:?}"));
            }
        }
    }
    let mut cluster_ids = BTreeSet::new();
    let mut members = BTreeSet::new();
    for cluster in &layout.clusters {
        validate_cluster(cluster, &ids, &mut cluster_ids, &mut members)?;
    }
    Ok(())
}

fn validate_cluster<'a>(
    cluster: &'a Cluster,
    nodes: &BTreeSet<&str>,
    ids: &mut BTreeSet<&'a str>,
    members: &mut BTreeSet<&'a str>,
) -> Result<(), String> {
    if !ids.insert(&cluster.id) {
        return Err(format!("duplicate cluster id: {:?}", cluster.id));
    }
    for node in &cluster.nodes {
        if !nodes.contains(node.as_str()) {
            return Err(format!("cluster references missing node: {node:?}"));
        }
        if !members.insert(node) {
            return Err(format!(
                "node belongs to multiple cluster regions: {node:?}"
            ));
        }
    }
    for child in &cluster.children {
        validate_cluster(child, nodes, ids, members)?;
    }
    Ok(())
}

fn emit_cluster(cluster: &Cluster, depth: usize, dot: &mut String) {
    let indent = "  ".repeat(depth);
    dot.push_str(&format!(
        "{indent}subgraph {} {{\n{indent}  label={};\n",
        quote(&format!("cluster_{}", cluster.id)),
        quote(&cluster.label)
    ));
    let mut nodes: Vec<_> = cluster.nodes.iter().collect();
    nodes.sort();
    for node in nodes {
        dot.push_str(&format!("{indent}  {};\n", quote(node)));
    }
    let mut children: Vec<_> = cluster.children.iter().collect();
    children.sort_by(|a, b| a.id.cmp(&b.id));
    for child in children {
        emit_cluster(child, depth + 1, dot);
    }
    dot.push_str(&format!("{indent}}}\n"));
}
