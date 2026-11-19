//! Role: 手で作った解析の結果から、図の配置要求（plan）と組立て（draw）の結果を確かめる試験。
use std::collections::BTreeMap;
use std::path::PathBuf;

use xtask_archive::{Archive, Content, Diagram, DiagramKind, EdgeStyle, Mode};
use xtask_layout::Layering;
use xtask_modules::{Module, ModuleGraph, ModuleKind, Reference};
use xtask_workspace::{
    Dependency, DependencyKind, Edition, Package, Target, TargetKind, Workspace,
};

use crate::model::{Body, Drawing};
use crate::{Input, Outcome, Plan, TargetInput, draw, plan};

fn package(name: &str) -> Package {
    Package {
        id: format!("path+file:///w/{name}#{name}@0.1.0"),
        name: name.into(),
        manifest_path: PathBuf::from(format!("/w/{name}/Cargo.toml")),
        targets: vec![Target {
            name: name.into(),
            kinds: vec![TargetKind::Lib],
            src_path: PathBuf::from(format!("/w/{name}/src/lib.rs")),
            required_features: Vec::new(),
            test: true,
            edition: Edition::E2021,
        }],
    }
}

fn workspace(packages: Vec<Package>, edges: Vec<Dependency>) -> Workspace {
    Workspace {
        root: PathBuf::from("/w"),
        packages,
        edges,
        all_features: BTreeMap::new(),
    }
}

fn dependency(from: &str, to: &str, kind: DependencyKind, target: Option<&str>) -> Dependency {
    Dependency {
        from: from.into(),
        to: to.into(),
        kind,
        target: target.map(Into::into),
    }
}

fn normal(from: &str, to: &str) -> Dependency {
    dependency(from, to, DependencyKind::Normal, None)
}

fn module(id: &str, parent: Option<usize>, role: &str) -> Module {
    Module {
        id: id.into(),
        file: PathBuf::from(format!("/w/{id}.rs")),
        parent,
        kind: ModuleKind::File,
        role: Ok(role.into()),
    }
}

/// `crate`（root）、`crate::x`、`crate::y` の三つの module を持つ lib。
fn lib_graph() -> ModuleGraph {
    ModuleGraph {
        modules: vec![
            module("crate", None, "a root"),
            module("crate::x", Some(0), "x"),
            module("crate::y", Some(0), "y"),
        ],
        edges: vec![Reference {
            from: "crate::x".into(),
            to: "crate::y".into(),
        }],
        cycles: Vec::new(),
        diagnostics: Vec::new(),
    }
}

fn lib_input<'a>(
    name: &'a str,
    kinds: &'a [TargetKind],
    graph: &'a ModuleGraph,
    drawn: bool,
) -> TargetInput<'a> {
    TargetInput {
        package: name,
        target: name,
        kinds,
        mode: Mode::Normal,
        outcome: Outcome::Analyzed(graph),
        drawn,
    }
}

fn drawing(plan: &Plan) -> &Drawing {
    let Body::Graph(drawing) = &plan.body else {
        panic!("expected a drawn plan");
    };
    drawing
}

fn failure(plan: &Plan) -> &str {
    let Body::Failed(text) = &plan.body else {
        panic!("expected a failed plan");
    };
    text
}

#[test]
fn crate_plan_orders_and_links() {
    let kinds = [TargetKind::Lib];
    let graph = lib_graph();
    let workspace = workspace(vec![package("a"), package("b")], vec![normal("a", "b")]);
    let input = Input {
        workspace: &workspace,
        targets: vec![lib_input("a", &kinds, &graph, true)],
    };
    let plans = plan(&input);
    assert_eq!(plans.len(), 2);
    assert_eq!(plans[0].kind, DiagramKind::Crates);
    assert_eq!(
        plans[1].kind,
        DiagramKind::Modules {
            package: "a".into(),
            target: "a".into(),
            kind: "lib".into(),
            mode: Mode::Normal,
        }
    );
    let crates = drawing(&plans[0]);
    assert_eq!(crates.items[0].open, Some(1));
    assert_eq!(crates.graph.layering, Layering::Fixed(vec![0, 1]));
}

#[test]
fn undrawn_targets_still_give_roles() {
    let kinds = [TargetKind::Lib];
    let graph = lib_graph();
    let workspace = workspace(vec![package("a"), package("b")], vec![normal("a", "b")]);
    let input = Input {
        workspace: &workspace,
        targets: vec![lib_input("a", &kinds, &graph, false)],
    };
    let plans = plan(&input);
    assert_eq!(plans.len(), 1);
    let crates = drawing(&plans[0]);
    assert_eq!(crates.items[0].role.as_deref(), Some("a root"));
    assert_eq!(crates.items[0].open, None);
}

#[test]
fn module_plan_has_region_per_module() {
    let kinds = [TargetKind::Lib];
    let graph = lib_graph();
    let workspace = workspace(vec![package("a")], Vec::new());
    let input = Input {
        workspace: &workspace,
        targets: vec![lib_input("a", &kinds, &graph, true)],
    };
    let plans = plan(&input);
    let modules = drawing(&plans[1]);
    assert_eq!(modules.graph.regions.len(), 3);
    assert_eq!(modules.graph.nodes.len(), 3);
    assert_eq!(modules.graph.regions[1].parent, Some(0));
}

#[test]
fn cyclic_ranks_report_only_cycle_members() {
    let workspace = workspace(
        vec![package("a"), package("b"), package("c")],
        vec![normal("a", "b"), normal("b", "a"), normal("b", "c")],
    );
    let plans = plan(&Input {
        workspace: &workspace,
        targets: Vec::new(),
    });
    assert_eq!(
        failure(&plans[0]),
        "cannot rank cyclic normal crate dependencies: a, b"
    );
}

#[test]
fn cycle_bridge_is_not_a_member() {
    let workspace = workspace(
        vec![
            package("a"),
            package("b"),
            package("c"),
            package("d"),
            package("x"),
        ],
        vec![
            normal("a", "b"),
            normal("b", "a"),
            normal("b", "x"),
            normal("x", "c"),
            normal("c", "d"),
            normal("d", "c"),
        ],
    );
    let plans = plan(&Input {
        workspace: &workspace,
        targets: Vec::new(),
    });
    assert_eq!(
        failure(&plans[0]),
        "cannot rank cyclic normal crate dependencies: a, b, c, d"
    );
}

#[test]
fn dependency_kinds_set_style_and_label() {
    let workspace = workspace(
        vec![package("a"), package("b")],
        vec![
            dependency("a", "b", DependencyKind::Dev, None),
            dependency("a", "b", DependencyKind::Build, Some("cfg(windows)")),
        ],
    );
    let plans = plan(&Input {
        workspace: &workspace,
        targets: Vec::new(),
    });
    let crates = drawing(&plans[0]);
    assert_eq!(
        crates.edges[0],
        (EdgeStyle::Dev, Some(vec!["dev".to_string()]))
    );
    assert_eq!(
        crates.edges[1],
        (
            EdgeStyle::Build,
            Some(vec!["build: cfg(windows)".to_string()])
        )
    );
}

#[test]
fn draw_produces_valid_archive() {
    let kinds = [TargetKind::Lib];
    let graph = lib_graph();
    let workspace = workspace(vec![package("a"), package("b")], vec![normal("a", "b")]);
    let input = Input {
        workspace: &workspace,
        targets: vec![lib_input("a", &kinds, &graph, true)],
    };
    let plans = plan(&input);
    let diagrams: Vec<Diagram> = plans.iter().map(draw).collect();
    let archive = Archive {
        root_name: "r".into(),
        diagrams,
        notices: Vec::new(),
    };
    assert_eq!(xtask_archive::validate(&archive), Ok(()));
    assert!(matches!(archive.diagrams[0].content, Content::Drawn(_)));
    let Content::Drawn(scene) = &archive.diagrams[1].content else {
        panic!("the module diagram should be drawn");
    };
    assert_eq!(scene.nodes.len(), 3);
}
