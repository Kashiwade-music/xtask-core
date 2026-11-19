//! Role: crate 依存図の配置要求（node・列・辺）を作る。
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use xtask_archive::{DiagramKind, EdgeStyle, Item};
use xtask_layout::{Graph, Layering, Size};
use xtask_workspace::{Dependency, DependencyKind, Package, Workspace};

use crate::text::{NO_ROLE, Text, node_box, node_text};
use crate::{Body, Drawing, Input, OpenIndices, Plan, analyzed_root_role, key};

const TITLE: &str = "Workspace crate dependencies (consumer -> provider)";

pub(crate) fn plan(input: &Input<'_>, opens: &OpenIndices) -> Plan {
    let body = match body(input, opens) {
        Ok(drawing) => Body::Graph(drawing),
        Err(message) => Body::Failed(message),
    };
    Plan {
        kind: DiagramKind::Crates,
        title: TITLE.into(),
        body,
    }
}

fn body(input: &Input<'_>, opens: &OpenIndices) -> Result<Drawing, String> {
    let packages = sorted_packages(input.workspace);
    let index: BTreeMap<&str, usize> = packages
        .iter()
        .enumerate()
        .map(|(position, package)| (package.name.as_str(), position))
        .collect();
    let outgoing = normal_outgoing(input.workspace, &index, packages.len())?;
    let members = cycle_members(&outgoing);
    if !members.is_empty() {
        let names: Vec<&str> = members
            .iter()
            .map(|&position| packages[position].name.as_str())
            .collect();
        return Err(format!(
            "cannot rank cyclic normal crate dependencies: {}",
            names.join(", ")
        ));
    }
    let columns = longest_depths(&outgoing);
    let (items, texts): (Vec<Item>, Vec<Text>) = packages
        .iter()
        .map(|package| entry(input, opens, package))
        .unzip();
    let mut sizes: Vec<Size> = texts.iter().map(node_box).collect();
    equalize_widths(&mut sizes, &columns);
    let (layout_edges, drawn_edges) = edges(&input.workspace.edges, &index)?;
    let layout_nodes = sizes
        .into_iter()
        .map(|size| xtask_layout::Node { size, region: None })
        .collect();
    let drawn_nodes = texts
        .into_iter()
        .enumerate()
        .map(|(position, text)| (position as u32, text.lines))
        .collect();
    Ok(Drawing {
        graph: Graph {
            nodes: layout_nodes,
            regions: Vec::new(),
            edges: layout_edges,
            layering: Layering::Fixed(columns),
        },
        items,
        nodes: drawn_nodes,
        regions: Vec::new(),
        edges: drawn_edges,
    })
}

fn sorted_packages(workspace: &Workspace) -> Vec<&Package> {
    let mut packages: Vec<&Package> = workspace.packages.iter().collect();
    packages.sort_by(|left, right| left.name.cmp(&right.name));
    packages
}

/// node の一つ（package）の item と、その文字。
fn entry(input: &Input<'_>, opens: &OpenIndices, package: &Package) -> (Item, Text) {
    let primary = crate::primary_target(package);
    let role = primary.and_then(|target| analyzed_root_role(input, package, target));
    let open = primary.and_then(|target| {
        opens
            .get(&key(&package.name, &target.name, &target.kinds))
            .copied()
    });
    let text = node_text(&format!(
        "{}\n{}",
        package.name,
        role.as_deref().unwrap_or(NO_ROLE)
    ));
    (
        Item {
            name: package.name.clone(),
            role,
            open,
        },
        text,
    )
}

/// 同じ列の node の幅を、その列の最大にそろえる。
fn equalize_widths(sizes: &mut [Size], columns: &[u32]) {
    let mut widest: BTreeMap<u32, i64> = BTreeMap::new();
    for (size, column) in sizes.iter().zip(columns) {
        let entry = widest.entry(*column).or_insert(0);
        *entry = (*entry).max(size.width);
    }
    for (size, column) in sizes.iter_mut().zip(columns) {
        size.width = widest[column];
    }
}

type Edges = (
    Vec<xtask_layout::Edge>,
    Vec<(EdgeStyle, Option<Vec<String>>)>,
);

/// 依存の辺。順は workspace.edges の順。
fn edges(dependencies: &[Dependency], index: &BTreeMap<&str, usize>) -> Result<Edges, String> {
    let mut layout_edges = Vec::new();
    let mut drawn_edges = Vec::new();
    for dependency in dependencies {
        let (style, label) = edge_label(dependency);
        let label = label.map(|value| node_text(&value));
        layout_edges.push(xtask_layout::Edge {
            from: endpoint(index, &dependency.from)?,
            to: endpoint(index, &dependency.to)?,
            label: label.as_ref().map(|text| text.size),
        });
        drawn_edges.push((style, label.map(|text| text.lines)));
    }
    Ok((layout_edges, drawn_edges))
}

/// 依存の種類ごとの線の型と、ラベル（target 条件の文を含む）。
fn edge_label(dependency: &Dependency) -> (EdgeStyle, Option<String>) {
    let (style, name) = match dependency.kind {
        DependencyKind::Normal => (EdgeStyle::Normal, None),
        DependencyKind::Dev => (EdgeStyle::Dev, Some("dev")),
        DependencyKind::Build => (EdgeStyle::Build, Some("build")),
    };
    let label = match (name, &dependency.target) {
        (None, None) => None,
        (None, Some(target)) => Some(target.clone()),
        (Some(name), None) => Some(name.to_string()),
        (Some(name), Some(target)) => Some(format!("{name}: {target}")),
    };
    (style, label)
}

fn endpoint(index: &BTreeMap<&str, usize>, name: &str) -> Result<usize, String> {
    index
        .get(name)
        .copied()
        .ok_or_else(|| format!("unknown dependency endpoint `{name}`"))
}

/// Normal の依存だけの、package ごとの依存先の集合。
fn normal_outgoing(
    workspace: &Workspace,
    index: &BTreeMap<&str, usize>,
    count: usize,
) -> Result<Vec<BTreeSet<usize>>, String> {
    let mut outgoing = vec![BTreeSet::new(); count];
    for dependency in workspace
        .edges
        .iter()
        .filter(|dependency| dependency.kind == DependencyKind::Normal)
    {
        let from = endpoint(index, &dependency.from)?;
        let to = endpoint(index, &dependency.to)?;
        outgoing[from].insert(to);
    }
    Ok(outgoing)
}

/// 閉路に属する package（一つ以上の辺をたどって自分に戻るもの）。
fn cycle_members(outgoing: &[BTreeSet<usize>]) -> Vec<usize> {
    (0..outgoing.len())
        .filter(|&position| reaches(outgoing, position, position))
        .collect()
}

/// `start` から一つ以上の辺をたどって `target` に届くか。
fn reaches(outgoing: &[BTreeSet<usize>], start: usize, target: usize) -> bool {
    let mut seen = vec![false; outgoing.len()];
    let mut queue: VecDeque<usize> = outgoing[start].iter().copied().collect();
    while let Some(node) = queue.pop_front() {
        if node == target {
            return true;
        }
        if std::mem::replace(&mut seen[node], true) {
            continue;
        }
        queue.extend(outgoing[node].iter().copied());
    }
    false
}

/// 消費側の root から最長の距離（列）。閉路が無いことを前提にする。
fn longest_depths(outgoing: &[BTreeSet<usize>]) -> Vec<u32> {
    let mut indegree = vec![0usize; outgoing.len()];
    for providers in outgoing {
        for &provider in providers {
            indegree[provider] += 1;
        }
    }
    let mut depth = vec![0u32; outgoing.len()];
    let mut ready: VecDeque<usize> = (0..outgoing.len())
        .filter(|&position| indegree[position] == 0)
        .collect();
    while let Some(consumer) = ready.pop_front() {
        for &provider in &outgoing[consumer] {
            depth[provider] = depth[provider].max(depth[consumer] + 1);
            indegree[provider] -= 1;
            if indegree[provider] == 0 {
                ready.push_back(provider);
            }
        }
    }
    depth
}
