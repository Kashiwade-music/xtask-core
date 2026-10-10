use crate::Reference;
use std::collections::{BTreeMap, BTreeSet};
pub(crate) fn components(nodes: Vec<String>, edges: &[Reference]) -> Vec<Vec<String>> {
    fn visit(
        node: &str,
        adjacency: &BTreeMap<String, Vec<String>>,
        seen: &mut BTreeSet<String>,
        order: &mut Vec<String>,
    ) {
        if !seen.insert(node.into()) {
            return;
        }
        if let Some(next) = adjacency.get(node) {
            for target in next {
                visit(target, adjacency, seen, order);
            }
        }
        order.push(node.into());
    }
    let mut forward = BTreeMap::<String, Vec<String>>::new();
    let mut reverse = BTreeMap::<String, Vec<String>>::new();
    for edge in edges {
        forward
            .entry(edge.from.clone())
            .or_default()
            .push(edge.to.clone());
        reverse
            .entry(edge.to.clone())
            .or_default()
            .push(edge.from.clone());
    }
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    for node in nodes {
        visit(&node, &forward, &mut seen, &mut order);
    }
    seen.clear();
    let mut cycles = Vec::new();
    for node in order.into_iter().rev() {
        if seen.contains(&node) {
            continue;
        }
        let mut component = Vec::new();
        visit(&node, &reverse, &mut seen, &mut component);
        if component.len() > 1 {
            component.sort();
            cycles.push(component);
        }
    }
    cycles.sort();
    cycles
}
