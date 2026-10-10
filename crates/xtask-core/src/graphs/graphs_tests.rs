use super::*;
use xtask_modules::{Module, Reference};
use xtask_workspace::{Dependency, Package};

fn workspace(names: &[&str], edges: &[(&str, &str, &str)]) -> Workspace {
    Workspace {
        root: ".".into(),
        packages: names
            .iter()
            .map(|name| Package {
                id: (*name).into(),
                name: (*name).into(),
                manifest_path: "Cargo.toml".into(),
                targets: Vec::new(),
                features: BTreeMap::new(),
            })
            .collect(),
        edges: edges
            .iter()
            .map(|(from, to, kind)| Dependency {
                from: (*from).into(),
                to: (*to).into(),
                kind: (*kind).into(),
                target: None,
            })
            .collect(),
    }
}

#[test]
fn normal_layers_follow_longest_path_and_ignore_dev_build_and_input_order() {
    let mut workspace = workspace(
        &["leaf", "app", "right", "left", "isolated"],
        &[
            ("app", "left", "normal"),
            ("app", "right", "normal"),
            ("left", "right", "normal"),
            ("right", "leaf", "normal"),
            ("app", "leaf", "normal"),
            ("leaf", "app", "dev"),
            ("isolated", "app", "build"),
        ],
    );
    let expected = vec![
        vec!["app", "isolated"],
        vec!["left"],
        vec!["right"],
        vec!["leaf"],
    ];
    assert_eq!(normal_ranks(&workspace).unwrap(), expected);
    let mut duplicate = workspace.edges[0].clone();
    duplicate.target = Some("cfg(unix)".into());
    workspace.edges.push(duplicate);
    workspace.edges.reverse();
    workspace.packages.reverse();
    assert_eq!(normal_ranks(&workspace).unwrap(), expected);
}

#[test]
fn dependency_kinds_keep_parallel_edges_and_only_dev_is_wavy() {
    let workspace = workspace(
        &["app", "lib"],
        &[
            ("app", "lib", "normal"),
            ("app", "lib", "dev"),
            ("lib", "app", "build"),
        ],
    );
    let (graph, layout) = dependency_graph(&workspace).unwrap();
    assert_eq!(graph.edges.len(), 3);
    assert_eq!(graph.edges[0].label, None);
    assert_eq!(graph.edges[1].label.as_deref(), Some("dev"));
    assert_eq!(layout.edge_styles[&1], EdgeStyle::Dev);
    assert_eq!(layout.edge_styles[&2], EdgeStyle::Unconstrained);
    let dot = graph.dot_with_layout(&layout).unwrap();
    assert!(!dot.contains("label=\"normal\""));
    assert!(dot.contains("label=\"dev\""));
    assert!(dot.contains("constraint=false"));
}

#[test]
fn cyclic_normal_dependencies_return_a_diagnostic() {
    let workspace = workspace(&["a", "b"], &[("a", "b", "normal"), ("b", "a", "normal")]);
    assert!(
        normal_ranks(&workspace)
            .unwrap_err()
            .contains("cyclic normal")
    );
}

fn modules(entries: &[(&str, Option<&str>)]) -> ModuleGraph {
    ModuleGraph {
        modules: entries
            .iter()
            .map(|(id, parent)| Module {
                id: (*id).into(),
                parent: parent.map(str::to_owned),
                file: "lib.rs".into(),
            })
            .collect(),
        edges: Vec::new(),
        cycles: Vec::new(),
        diagnostics: Vec::new(),
    }
}

#[test]
fn three_module_levels_and_parent_child_references_share_one_diagram() {
    let mut modules = modules(&[
        ("crate::child::leaf", Some("crate::child")),
        ("crate", None),
        ("crate::child", Some("crate")),
    ]);
    modules.edges = vec![
        Reference {
            from: "crate".into(),
            to: "crate::child::leaf".into(),
        },
        Reference {
            from: "crate::child::leaf".into(),
            to: "crate".into(),
        },
    ];
    let clusters = module_clusters(&modules).unwrap();
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].id, "crate");
    assert_eq!(
        clusters[0].children[0].children[0].nodes,
        ["crate::child::leaf"]
    );
    let graph = Graph {
        title: "modules".into(),
        nodes: modules
            .modules
            .iter()
            .map(|module| Node {
                id: module.id.clone(),
                label: "(self)".into(),
            })
            .collect(),
        edges: modules
            .edges
            .iter()
            .map(|edge| Edge {
                from: edge.from.clone(),
                to: edge.to.clone(),
                label: None,
            })
            .collect(),
    };
    let dot = graph
        .dot_with_layout(&Layout {
            clusters,
            ..Layout::default()
        })
        .unwrap();
    assert_eq!(dot.matches("subgraph").count(), 3);
    assert!(dot.contains("\"crate\" -> \"crate::child::leaf\""));
    assert!(dot.contains("\"crate::child::leaf\" -> \"crate\""));
    assert!(!dot.contains("\"crate\" -> \"crate::child\""));
}

#[test]
fn malformed_module_hierarchies_fail_instead_of_omitting_regions() {
    for entries in [
        vec![("a", Some("missing"))],
        vec![("a", None), ("a", None)],
        vec![("a", Some("b")), ("b", Some("a"))],
    ] {
        assert!(module_clusters(&modules(&entries)).is_err());
    }
    assert!(module_clusters(&modules(&[])).unwrap().is_empty());
}
