//! Role: crate依存のrankとstyle、モジュールの階層変換を検証する。
use super::*;
use xtask_modules::{Module, Reference};
use xtask_workspace::{Dependency, Package, Target};

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
    let roles = BTreeMap::from([
        ("app".into(), "アプリケーションを起動する".into()),
        ("lib".into(), "共通機能を提供する".into()),
    ]);
    let (graph, layout) = dependency_graph(&workspace, &roles).unwrap();
    assert_eq!(graph.nodes[0].label, "app\nアプリケーションを起動する");
    assert_eq!(graph.edges.len(), 3);
    assert_eq!(graph.edges[0].label, None);
    assert_eq!(graph.edges[1].label.as_deref(), Some("dev"));
    assert_eq!(layout.edge_styles[&1], EdgeStyle::Dev);
    assert_eq!(layout.edge_styles[&2], EdgeStyle::Unconstrained);
    let request = graph.layout_request(&layout).unwrap();
    assert_eq!(request.edges.len(), 3);
    assert!(
        request
            .edges
            .iter()
            .any(|edge| edge.from == "app" && edge.to == "lib" && edge.label.is_none())
    );
    assert!(
        request
            .edges
            .iter()
            .any(|edge| edge.label.as_deref() == Some("dev"))
    );
    assert!(request.edges.iter().any(|edge| edge.from == "lib"
        && edge.to == "app"
        && edge.label.as_deref() == Some("build")));
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
                role: Some(format!("{id} の役割")),
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
    assert_eq!(clusters[0].label, "crate\ncrate の役割");
    assert_eq!(
        clusters[0].children[0].children[0].label,
        "crate::child::leaf\ncrate::child::leaf の役割"
    );
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
    let request = graph
        .layout_request(&Layout {
            clusters,
            ..Layout::default()
        })
        .unwrap();
    assert_eq!(
        request.regions[0].children[0].children[0].id,
        "crate::child::leaf"
    );
    assert_eq!(request.edges.len(), 2);
    assert!(
        request
            .edges
            .iter()
            .any(|edge| edge.from == "crate" && edge.to == "crate::child::leaf")
    );
    assert!(
        request
            .edges
            .iter()
            .any(|edge| edge.from == "crate::child::leaf" && edge.to == "crate")
    );
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

fn target(name: &str, kind: &str, path: &Path) -> Target {
    Target {
        name: name.into(),
        kind: vec![kind.into()],
        src_path: path.into(),
        required_features: vec![],
        test: true,
        edition: "2024".into(),
    }
}

#[test]
fn primary_target_prefers_library_then_package_binary_and_lexical_names() {
    let mut workspace = workspace(&["app"], &[]);
    let package = &mut workspace.packages[0];
    package.targets = vec![
        target("z", "bin", Path::new("z.rs")),
        target("app", "bin", Path::new("main.rs")),
        target("a", "bin", Path::new("a.rs")),
    ];
    assert_eq!(primary_target(package).unwrap().name, "app");
    package
        .targets
        .push(target("other", "lib", Path::new("lib.rs")));
    assert_eq!(primary_target(package).unwrap().name, "other");
    package.targets.reverse();
    assert_eq!(primary_target(package).unwrap().name, "other");
    package
        .targets
        .retain(|target| target.name != "other" && target.name != "app");
    assert_eq!(primary_target(package).unwrap().name, "a");
    package.targets = vec![target("bench", "bench", Path::new("bench.rs"))];
    let error = primary_target(package).unwrap_err();
    assert!(error.contains("app") && error.contains("Cargo.toml") && error.contains("no primary"));
}

#[test]
fn diagrams_keep_partial_modules_and_reject_missing_crate_roles() {
    let mut graph = modules(&[("crate", None), ("crate::child", Some("crate"))]);
    graph.modules[1].role = None;
    let clusters = module_clusters(&graph).unwrap();
    assert_eq!(clusters[0].children[0].label, "crate::child\n[役割未記載]");
    let workspace = workspace(&["app"], &[]);
    assert!(
        dependency_graph(&workspace, &BTreeMap::new())
            .unwrap_err()
            .contains("app")
    );
}

#[test]
fn crate_roles_read_only_primary_root_and_report_package_file_context() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("lib.rs");
    let binary = directory.path().join("main.rs");
    std::fs::write(
        &library,
        "//! Role: 共通の公開機能を提供する\nmod missing;\ninclude!(\"unknown.rs\");\n",
    )
    .unwrap();
    std::fs::write(&binary, "fn main() {}\n").unwrap();
    let mut workspace = workspace(&["app"], &[]);
    workspace.packages[0].targets = vec![
        target("app", "bin", &binary),
        target("library", "rlib", &library),
    ];
    let mut report = Report::default();
    let roles = crate_roles(
        &workspace,
        xtask_modules::DEFAULT_MAX_ROLE_CHARS,
        &mut report,
    );
    assert!(report.is_success());
    assert_eq!(roles["app"], "共通の公開機能を提供する");
    let (graph, _) = dependency_graph(&workspace, &roles).unwrap();
    assert_eq!(graph.nodes[0].label, "app\n共通の公開機能を提供する");
    for source in [
        "pub fn missing_role() {}\n",
        "//! Role: \npub fn invalid_role() {}\n",
    ] {
        std::fs::write(&library, source).unwrap();
        let mut report = Report::default();
        let roles = crate_roles(
            &workspace,
            xtask_modules::DEFAULT_MAX_ROLE_CHARS,
            &mut report,
        );
        assert_eq!(roles["app"], "[役割未記載]");
        assert_eq!(report.errors.len(), 1);
        assert!(!report.is_success());
        let (graph, _) = dependency_graph(&workspace, &roles).unwrap();
        assert_eq!(graph.nodes[0].label, "app\n[役割未記載]");
        let error = &report.errors[0];
        assert!(
            error.contains("package `app`") && error.contains("lib.rs"),
            "{error}"
        );
    }
}

#[test]
fn crate_roles_keep_every_package_when_primary_targets_are_missing() {
    let workspace = workspace(&["first", "later"], &[("first", "later", "normal")]);
    let mut report = Report::default();
    let roles = crate_roles(
        &workspace,
        xtask_modules::DEFAULT_MAX_ROLE_CHARS,
        &mut report,
    );
    assert_eq!(roles.len(), 2);
    assert!(roles.values().all(|role| role == "[役割未記載]"));
    assert_eq!(report.errors.len(), 2);
    assert!(report.errors[0].contains("first") && report.errors[0].contains("Cargo.toml"));
    assert!(report.errors[1].contains("later") && report.errors[1].contains("no primary"));
    let (graph, layout) = dependency_graph(&workspace, &roles).unwrap();
    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.edges.len(), 1);
    assert_eq!(layout.ranks, vec![vec!["first"], vec!["later"]]);
}

#[test]
fn module_diagrams_keep_errors_and_continue_to_later_targets() {
    let binary = std::env::var_os("XTASK_NODE").unwrap_or_else(|| "node".into());
    if !std::process::Command::new(&binary)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        eprintln!("Node unavailable; optional module-diagram continuation test skipped");
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let config = Config::new(directory.path());
    let mut malformed = modules(&[("crate::bad", Some("missing"))]);
    malformed.modules[0].file = directory.path().join("bad.rs");
    let mut bad_edge = modules(&[]);
    bad_edge.edges.push(Reference {
        from: "absent".into(),
        to: "missing".into(),
    });
    let targets = vec![
        TargetGraph {
            package: "app".into(),
            target: "bad".into(),
            kind: "lib".into(),
            test: false,
            graph: malformed,
        },
        TargetGraph {
            package: "app".into(),
            target: "bad-edge".into(),
            kind: "lib".into(),
            test: true,
            graph: bad_edge,
        },
        TargetGraph {
            package: "app".into(),
            target: "good".into(),
            kind: "bin".into(),
            test: false,
            graph: modules(&[]),
        },
    ];
    let mut report = Report::default();
    super::modules(&config, &targets, DiagramTargets::All, &mut report).unwrap();
    assert_eq!(report.errors.len(), 2);
    assert!(
        report.errors[0].contains("app::bad")
            && report.errors[0].contains("bad.rs")
            && report.errors[0].contains("missing parent")
    );
    assert!(
        report.errors[1].contains("app::bad-edge")
            && report.errors[1].contains("test")
            && report.errors[1].contains("missing node")
    );
    let expected = config.output_dir().join("modules/app/good-bin-normal.svg");
    assert_eq!(report.artifacts.as_slice(), std::slice::from_ref(&expected));
    assert!(std::fs::read_to_string(&expected).unwrap().contains("<svg"));
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("good-bin-normal.svg"))
    );
    assert!(
        !config
            .output_dir()
            .join("modules/app/bad-lib-normal.svg")
            .exists()
    );
    assert!(
        !config
            .output_dir()
            .join("modules/app/bad-edge-lib-test.svg")
            .exists()
    );
}

#[test]
fn no_write_validates_graphs_without_creating_outputs_or_artifacts() {
    let directory = tempfile::tempdir().unwrap();
    let blocker = directory.path().join("not-a-directory");
    std::fs::write(&blocker, "existing data").unwrap();
    let stem = blocker.join("graph");
    let mut report = Report::default();
    write(
        &Graph::default(),
        &Layout::default(),
        &stem,
        &mut report,
        true,
    )
    .unwrap();
    assert!(report.artifacts.is_empty());
    assert_eq!(report.notes.len(), 1);
    assert!(report.notes[0].contains("validated") && report.notes[0].contains("(no-write)"));
    assert_eq!(std::fs::read_to_string(&blocker).unwrap(), "existing data");
    let invalid = Graph {
        edges: vec![Edge {
            from: "missing".into(),
            to: "absent".into(),
            label: None,
        }],
        ..Graph::default()
    };
    let untouched = directory.path().join("uncreated").join("bad");
    assert!(
        write(&invalid, &Layout::default(), &untouched, &mut report, true)
            .unwrap_err()
            .contains("missing node")
    );
    assert!(!untouched.parent().unwrap().exists());
    assert!(report.artifacts.is_empty());
    assert_eq!(report.notes.len(), 1);
}

#[test]
fn no_write_modules_preserve_errors_and_validate_later_targets() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = Config::new(directory.path());
    config.no_write = true;
    let targets = vec![
        TargetGraph {
            package: "app".into(),
            target: "bad".into(),
            kind: "lib".into(),
            test: false,
            graph: modules(&[("crate", Some("missing"))]),
        },
        TargetGraph {
            package: "app".into(),
            target: "good".into(),
            kind: "lib".into(),
            test: false,
            graph: modules(&[]),
        },
    ];
    let mut report = Report::default();
    super::modules(&config, &targets, DiagramTargets::All, &mut report).unwrap();
    assert_eq!(report.errors.len(), 1);
    assert!(report.errors[0].contains("missing parent"));
    assert!(report.artifacts.is_empty());
    assert_eq!(report.notes.len(), 1);
    assert!(
        report.notes[0].contains("good-lib-normal.svg") && report.notes[0].contains("(no-write)")
    );
    assert!(!config.output_dir().exists());
}
