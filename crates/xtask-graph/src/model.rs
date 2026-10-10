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
    /// Produce valid, stable DOT regardless of the input ordering.
    pub fn dot(&self) -> Result<String, String> {
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
        let mut nodes: Vec<_> = self.nodes.iter().collect();
        nodes.sort_by(|left, right| left.id.cmp(&right.id));
        let mut edges: Vec<_> = self.edges.iter().collect();
        edges.sort_by(|left, right| {
            (&left.from, &left.to, &left.label).cmp(&(&right.from, &right.to, &right.label))
        });
        let mut dot = format!(
            "digraph {{\n  graph [label={}, labelloc=\"t\", rankdir=\"TB\"];\n  node [shape=\"box\"];\n",
            quote(&self.title)
        );
        for node in nodes {
            dot.push_str(&format!(
                "  {} [label={}];\n",
                quote(&node.id),
                quote(&node.label)
            ));
        }
        for edge in edges {
            dot.push_str(&format!("  {} -> {}", quote(&edge.from), quote(&edge.to)));
            if let Some(label) = &edge.label {
                dot.push_str(&format!(" [label={}]", quote(label)));
            }
            dot.push_str(";\n");
        }
        dot.push_str("}\n");
        Ok(dot)
    }
}

pub(crate) fn quote(value: &str) -> String {
    let mut quoted = String::from("\"");
    for character in value.chars() {
        match character {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}
