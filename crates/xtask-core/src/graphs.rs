use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use xtask_graph::{Cluster, Direction, Edge, EdgeStyle, Graph, Layout, Node};
use xtask_modules::ModuleGraph;
use xtask_workspace::Workspace;

use crate::{Config, Report, TargetGraph};

pub(crate) fn dependencies(
    config: &Config,
    workspace: &Workspace,
    report: &mut Report,
) -> Result<(), String> {
    let (graph, layout) = dependency_graph(workspace)?;
    write(&graph, &layout, &config.output.join("crates"), report)
}

fn dependency_graph(workspace: &Workspace) -> Result<(Graph, Layout), String> {
    let graph = Graph {
        title: "Workspace crate dependencies (consumer -> provider)".into(),
        nodes: workspace
            .packages
            .iter()
            .map(|package| Node {
                id: package.name.clone(),
                label: package.name.clone(),
            })
            .collect(),
        edges: workspace
            .edges
            .iter()
            .map(|edge| Edge {
                from: edge.from.clone(),
                to: edge.to.clone(),
                label: match (edge.kind.as_str(), &edge.target) {
                    ("normal", None) => None,
                    ("normal", Some(target)) => Some(target.clone()),
                    (_, None) => Some(edge.kind.clone()),
                    (_, Some(target)) => Some(format!("{}: {target}", edge.kind)),
                },
            })
            .collect(),
    };
    let layout = Layout {
        direction: Direction::LeftToRight,
        ranks: normal_ranks(workspace)?,
        edge_styles: workspace
            .edges
            .iter()
            .enumerate()
            .map(|(index, edge)| {
                (
                    index,
                    match edge.kind.as_str() {
                        "normal" => EdgeStyle::Normal,
                        "dev" => EdgeStyle::Dev,
                        _ => EdgeStyle::Unconstrained,
                    },
                )
            })
            .collect(),
        ..Layout::default()
    };
    Ok((graph, layout))
}

/// Longest distance from a consumer root in the normal dependency DAG.
/// Deduplicate target-specific declarations before computing indegrees.
fn normal_ranks(workspace: &Workspace) -> Result<Vec<Vec<String>>, String> {
    let mut outgoing: BTreeMap<&str, BTreeSet<&str>> = workspace
        .packages
        .iter()
        .map(|package| (package.name.as_str(), BTreeSet::new()))
        .collect();
    let mut indegree: BTreeMap<&str, usize> = outgoing.keys().map(|&id| (id, 0)).collect();
    for edge in workspace.edges.iter().filter(|edge| edge.kind == "normal") {
        if !indegree.contains_key(edge.to.as_str()) {
            return Err(format!("unknown dependency endpoint `{}`", edge.to));
        }
        let providers = outgoing
            .get_mut(edge.from.as_str())
            .ok_or_else(|| format!("unknown dependency endpoint `{}`", edge.from))?;
        if providers.insert(&edge.to) {
            *indegree
                .get_mut(edge.to.as_str())
                .expect("validated provider") += 1;
        }
    }
    let mut ready: BTreeSet<_> = indegree
        .iter()
        .filter_map(|(&id, &degree)| (degree == 0).then_some(id))
        .collect();
    let mut depth: BTreeMap<&str, usize> = outgoing.keys().map(|&id| (id, 0)).collect();
    let mut visited = 0;
    while let Some(consumer) = ready.pop_first() {
        visited += 1;
        for &provider in &outgoing[consumer] {
            let level = depth[consumer] + 1;
            let provider_depth = depth.get_mut(provider).expect("validated provider");
            *provider_depth = (*provider_depth).max(level);
            let degree = indegree.get_mut(provider).expect("validated provider");
            *degree -= 1;
            if *degree == 0 {
                ready.insert(provider);
            }
        }
    }
    if visited != outgoing.len() {
        let blocked: Vec<_> = indegree
            .iter()
            .filter_map(|(&id, &degree)| (degree > 0).then_some(id))
            .collect();
        return Err(format!(
            "cannot rank cyclic normal crate dependencies: {}",
            blocked.join(", ")
        ));
    }
    let mut ranks = vec![Vec::new(); depth.values().max().map_or(0, |level| level + 1)];
    for (id, level) in depth {
        ranks[level].push(id.to_owned());
    }
    Ok(ranks)
}

pub(crate) fn modules(
    config: &Config,
    targets: &[TargetGraph],
    report: &mut Report,
) -> Result<(), String> {
    for target in targets {
        let graph = Graph {
            title: format!(
                "{}::{} ({}, {}): module dependencies",
                target.package,
                target.target,
                target.kind,
                if target.test { "test" } else { "normal" }
            ),
            nodes: target
                .graph
                .modules
                .iter()
                .map(|module| Node {
                    id: module.id.clone(),
                    label: "(self)".into(),
                })
                .collect(),
            edges: target
                .graph
                .edges
                .iter()
                .map(|edge| Edge {
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    label: None,
                })
                .collect(),
        };
        let layout = Layout {
            direction: Direction::LeftToRight,
            clusters: module_clusters(&target.graph)?,
            ..Layout::default()
        };
        let directory = config
            .output
            .join("modules")
            .join(safe_name(&target.package));
        let name = format!(
            "{}-{}-{}",
            safe_name(&target.target),
            safe_name(&target.kind),
            if target.test { "test" } else { "normal" }
        );
        write(&graph, &layout, &directory.join(name), report)?;
    }
    Ok(())
}

fn module_clusters(graph: &ModuleGraph) -> Result<Vec<Cluster>, String> {
    let mut children: BTreeMap<Option<&str>, Vec<&str>> = BTreeMap::new();
    let ids: BTreeSet<_> = graph
        .modules
        .iter()
        .map(|module| module.id.as_str())
        .collect();
    if ids.len() != graph.modules.len() {
        return Err("duplicate module IDs in hierarchy".into());
    }
    for module in &graph.modules {
        let parent = module.parent.as_deref();
        if let Some(parent) = parent {
            if !ids.contains(parent) {
                return Err(format!(
                    "module `{}` has missing parent `{parent}`",
                    module.id
                ));
            }
        }
        children.entry(parent).or_default().push(&module.id);
    }
    for siblings in children.values_mut() {
        siblings.sort_unstable();
    }
    fn build(
        id: &str,
        children: &BTreeMap<Option<&str>, Vec<&str>>,
        visited: &mut usize,
    ) -> Cluster {
        *visited += 1;
        Cluster {
            id: id.into(),
            label: id.into(),
            nodes: vec![id.into()],
            children: children
                .get(&Some(id))
                .into_iter()
                .flatten()
                .map(|child| build(child, children, visited))
                .collect(),
        }
    }
    let mut visited = 0;
    let clusters = children
        .get(&None)
        .into_iter()
        .flatten()
        .map(|root| build(root, &children, &mut visited))
        .collect();
    // Each module has exactly one parent: a parent cycle cannot be reached from a root.
    if visited != ids.len() {
        return Err("cycle in module hierarchy".into());
    }
    Ok(clusters)
}

fn safe_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_string()
            } else {
                format!("~{:x}~", c as u32)
            }
        })
        .collect()
}

fn write(graph: &Graph, layout: &Layout, stem: &Path, report: &mut Report) -> Result<(), String> {
    let dot = stem.with_extension("dot");
    let svg = stem.with_extension("svg");
    xtask_graph::write_with_layout(graph, layout, &dot, &svg)?;
    report.notes.push(format!("wrote {}", svg.display()));
    report.artifacts.extend([dot, svg]);
    Ok(())
}

#[cfg(test)]
mod graphs_tests;
