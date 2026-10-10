//! Role: グラフと配置設定を決定的な型付きELK要求へ変換する。
use crate::layout::{Cluster, Direction, EdgeStyle, Layout, sorted_edges, validate};
use crate::model::Graph;
use xtask_layout::{EdgeRequest, LayoutRequest, NodeRequest, RegionRequest};

impl Graph {
    /// Build a deterministic, validated request without starting the runtime or writing files.
    pub fn layout_request(&self, layout: &Layout) -> Result<LayoutRequest, String> {
        self.validate()?;
        validate(self, layout)?;
        Ok(request(self, layout))
    }
}

fn request(graph: &Graph, layout: &Layout) -> LayoutRequest {
    let mut nodes: Vec<_> = graph.nodes.iter().collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    let mut regions: Vec<_> = layout.clusters.iter().collect();
    regions.sort_by(|a, b| a.id.cmp(&b.id));
    let mut ranks = layout.ranks.clone();
    for rank in &mut ranks {
        rank.sort();
    }
    LayoutRequest {
        title: graph.title.clone(),
        direction: match layout.direction {
            Direction::LeftToRight => xtask_layout::Direction::LeftToRight,
            Direction::TopToBottom => xtask_layout::Direction::TopToBottom,
        },
        nodes: nodes
            .into_iter()
            .map(|node| NodeRequest {
                id: node.id.clone(),
                label: node.label.clone(),
            })
            .collect(),
        regions: regions.into_iter().map(region).collect(),
        edges: sorted_edges(graph, layout)
            .into_iter()
            .enumerate()
            .map(|(position, (_, edge, style))| EdgeRequest {
                id: format!(
                    "xtask-{}edge-{position}",
                    if style == EdgeStyle::Dev { "dev-" } else { "" }
                ),
                from: edge.from.clone(),
                to: edge.to.clone(),
                label: edge
                    .label
                    .clone()
                    .or_else(|| (style == EdgeStyle::Dev).then(|| "dev".to_owned())),
                style: match style {
                    EdgeStyle::Normal => xtask_layout::EdgeStyle::Normal,
                    EdgeStyle::Dev => xtask_layout::EdgeStyle::Dev,
                    EdgeStyle::Unconstrained => xtask_layout::EdgeStyle::Unconstrained,
                },
            })
            .collect(),
        ranks,
    }
}

fn region(cluster: &Cluster) -> RegionRequest {
    let mut nodes = cluster.nodes.clone();
    nodes.sort();
    let mut children: Vec<_> = cluster.children.iter().collect();
    children.sort_by(|a, b| a.id.cmp(&b.id));
    RegionRequest {
        id: cluster.id.clone(),
        label: cluster.label.clone(),
        nodes,
        children: children.into_iter().map(region).collect(),
    }
}
