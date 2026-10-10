use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Edge, Graph};

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
        Self::LeftToRight
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

pub(crate) fn sorted_edges<'a>(
    graph: &'a Graph,
    layout: &Layout,
) -> Vec<(usize, &'a Edge, EdgeStyle)> {
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

pub(crate) fn validate(graph: &Graph, layout: &Layout) -> Result<(), String> {
    let ids: BTreeSet<_> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    for index in layout.edge_styles.keys() {
        if *index >= graph.edges.len() {
            return Err(format!("edge style index {index} is out of bounds"));
        }
    }
    let mut ranked = BTreeMap::new();
    if !layout.ranks.is_empty() {
        if layout.direction != Direction::LeftToRight {
            return Err("normal ranks require left-to-right layout with fixed side ports".into());
        }
        if !layout.clusters.is_empty() {
            return Err("normal ranks are supported for flat crate graphs only".into());
        }
    }
    for (rank_index, rank) in layout.ranks.iter().enumerate() {
        if rank.is_empty() {
            return Err("rank groups must not be empty".into());
        }
        for id in rank {
            if !ids.contains(id.as_str()) {
                return Err(format!("rank references missing node: {id:?}"));
            }
            if ranked.insert(id.as_str(), rank_index).is_some() {
                return Err(format!("node occurs in more than one rank: {id:?}"));
            }
        }
    }
    if !layout.ranks.is_empty() {
        if ranked.len() != ids.len() {
            return Err("every crate node must have a normal rank".into());
        }
        for (_, edge, style) in sorted_edges(graph, layout) {
            if style == EdgeStyle::Normal && ranked[edge.from.as_str()] >= ranked[edge.to.as_str()]
            {
                return Err("normal edge violates normal rank order".into());
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
