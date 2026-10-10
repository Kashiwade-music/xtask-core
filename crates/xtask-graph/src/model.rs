//! Role: グラフのノード・参照を定義し、識別子と接続先を検証する。
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Graph {
    pub title: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Node {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
}

impl Graph {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        for node in &self.nodes {
            if !ids.insert(node.id.as_str()) {
                return Err(format!("duplicate graph node id: {:?}", node.id));
            }
        }
        for edge in &self.edges {
            for endpoint in [&edge.from, &edge.to] {
                if !ids.contains(endpoint.as_str()) {
                    return Err(format!("graph edge references missing node: {endpoint:?}"));
                }
            }
        }
        Ok(())
    }
}
