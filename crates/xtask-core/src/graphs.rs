//! Role: crate依存とモジュール階層・参照をSVG出力用のグラフへ変換する。
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use xtask_graph::{Cluster, Direction, Edge, EdgeStyle, Graph, Layout, Node};
use xtask_modules::ModuleGraph;
use xtask_workspace::{Package, Target, Workspace};

use crate::{Config, Report, TargetGraph};

pub(crate) fn dependencies(
    config: &Config,
    workspace: &Workspace,
    report: &mut Report,
    max_role_chars: usize,
) -> Result<(), String> {
    let roles = crate_roles(workspace, max_role_chars, report);
    let (graph, layout) = dependency_graph(workspace, &roles)?;
    write(
        &graph,
        &layout,
        &config.output_dir().join("crates"),
        report,
        config.no_write,
    )
}

fn primary_target(package: &Package) -> Result<&Target, String> {
    let library = |target: &&Target| {
        target.kind.iter().any(|kind| {
            matches!(
                kind.as_str(),
                "lib" | "rlib" | "staticlib" | "cdylib" | "dylib" | "proc-macro"
            )
        })
    };
    package
        .targets
        .iter()
        .filter(library)
        .min_by(|left, right| {
            (&left.name, &left.src_path, &left.kind).cmp(&(
                &right.name,
                &right.src_path,
                &right.kind,
            ))
        })
        .or_else(|| {
            package
                .targets
                .iter()
                .filter(|target| target.kind.iter().any(|kind| kind == "bin"))
                .min_by_key(|target| (target.name != package.name, &target.name, &target.src_path))
        })
        .ok_or_else(|| {
            format!(
                "package `{}` ({}): no primary library or binary target for crate role",
                package.name,
                package.manifest_path.display()
            )
        })
}

fn crate_roles(
    workspace: &Workspace,
    max_role_chars: usize,
    report: &mut Report,
) -> BTreeMap<String, String> {
    workspace
        .packages
        .iter()
        .map(|package| {
            let role = primary_target(package)
                .and_then(|target| {
                    xtask_modules::crate_role_with_limit(&target.src_path, max_role_chars).map_err(
                        |error| {
                            format!(
                                "package `{}` ({}): {error}",
                                package.name,
                                target.src_path.display()
                            )
                        },
                    )
                })
                .unwrap_or_else(|error| {
                    report.errors.push(error);
                    "[役割未記載]".into()
                });
            (package.name.clone(), role)
        })
        .collect()
}

fn dependency_graph(
    workspace: &Workspace,
    roles: &BTreeMap<String, String>,
) -> Result<(Graph, Layout), String> {
    let graph = Graph {
        title: "Workspace crate dependencies (consumer -> provider)".into(),
        nodes: workspace
            .packages
            .iter()
            .map(|package| {
                Ok(Node {
                    id: package.name.clone(),
                    label: format!(
                        "{}\n{}",
                        package.name,
                        roles.get(&package.name).ok_or_else(|| format!(
                            "package `{}` has no crate role",
                            package.name
                        ))?
                    ),
                })
            })
            .collect::<Result<_, String>>()?,
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
        if let Err(error) = module_diagram(config, target, report) {
            let source = target
                .graph
                .modules
                .iter()
                .filter(|module| module.parent.is_none())
                .min_by(|left, right| left.file.cmp(&right.file))
                .or_else(|| target.graph.modules.first())
                .map_or_else(
                    || "<no module source>".into(),
                    |module| module.file.display().to_string(),
                );
            report.errors.push(format!(
                "module diagram {}::{} ({}, {}, {source}): {error}",
                target.package,
                target.target,
                target.kind,
                if target.test { "test" } else { "normal" }
            ));
        }
    }
    Ok(())
}

fn module_diagram(
    config: &Config,
    target: &TargetGraph,
    report: &mut Report,
) -> Result<(), String> {
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
        .output_dir()
        .join("modules")
        .join(safe_name(&target.package));
    let name = format!(
        "{}-{}-{}",
        safe_name(&target.target),
        safe_name(&target.kind),
        if target.test { "test" } else { "normal" }
    );
    write(
        &graph,
        &layout,
        &directory.join(name),
        report,
        config.no_write,
    )
}

fn module_clusters(graph: &ModuleGraph) -> Result<Vec<Cluster>, String> {
    let mut children: BTreeMap<Option<&str>, Vec<&str>> = BTreeMap::new();
    let ids: BTreeSet<_> = graph
        .modules
        .iter()
        .map(|module| module.id.as_str())
        .collect();
    let roles: BTreeMap<_, _> = graph
        .modules
        .iter()
        .map(|module| {
            (
                module.id.as_str(),
                module.role.as_deref().unwrap_or("[役割未記載]"),
            )
        })
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
        roles: &BTreeMap<&str, &str>,
        visited: &mut usize,
    ) -> Cluster {
        *visited += 1;
        Cluster {
            id: id.into(),
            label: format!("{id}\n{}", roles[id]),
            nodes: vec![id.into()],
            children: children
                .get(&Some(id))
                .into_iter()
                .flatten()
                .map(|child| build(child, children, roles, visited))
                .collect(),
        }
    }
    let mut visited = 0;
    let clusters = children
        .get(&None)
        .into_iter()
        .flatten()
        .map(|root| build(root, &children, &roles, &mut visited))
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

fn write(
    graph: &Graph,
    layout: &Layout,
    stem: &Path,
    report: &mut Report,
    no_write: bool,
) -> Result<(), String> {
    let svg = stem.with_extension("svg");
    if no_write {
        graph.layout_request(layout)?;
        report
            .notes
            .push(format!("validated {} (no-write)", svg.display()));
        return Ok(());
    }
    xtask_graph::write_with_layout(graph, layout, &svg)?;
    report.notes.push(format!("wrote {}", svg.display()));
    report.artifacts.push(svg);
    Ok(())
}

#[cfg(test)]
mod graphs_tests;
